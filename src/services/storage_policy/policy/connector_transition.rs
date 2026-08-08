use aster_forge_db::transaction;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, Set};

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{
    file_repo, policy_repo, storage_policy_connector_credential_repo, upload_session_repo,
};
use crate::errors::{
    AsterError, MapAsterErr, Result, precondition_failed_with_code, validation_error_with_code,
};
use crate::runtime::RemoteProtocolRuntimeState;
use crate::storage::connectors::{
    PreparedStorageConnectorTransition, StorageConnectorTransitionSavedState,
    StoredStorageConnectorCredentialPayload,
};
use crate::storage::{
    ExecuteStorageConnectorTransitionInput, ResolveStorageConnectorTransitionsInput,
    StorageConnectorTransitionPreviewList,
};
use aster_drive_model::entities::{storage_policy, storage_policy_connector_credential};
use aster_drive_storage::{ConnectorConfigEnvelope, ConnectorId};

use super::StoragePolicy;

const TRANSITION_OBJECT_SAMPLE_SIZE: u64 = 10;

pub(super) struct ExecutedStorageConnectorTransition {
    pub(super) policy: StoragePolicy,
    pub(super) source_connector_id: ConnectorId,
}

pub async fn resolve_connector_transitions(
    state: &(impl RemoteProtocolRuntimeState + Sync),
    input: ResolveStorageConnectorTransitionsInput,
) -> Result<StorageConnectorTransitionPreviewList> {
    crate::storage::connectors::resolve_transition_previews(
        state.driver_registry().connectors(),
        state.writer_db(),
        input,
    )
    .await
}

pub(super) async fn execute_connector_transition(
    state: &(impl RemoteProtocolRuntimeState + Sync),
    policy_id: i64,
    input: ExecuteStorageConnectorTransitionInput,
) -> Result<ExecutedStorageConnectorTransition> {
    let source_policy = policy_repo::find_by_id(state.writer_db(), policy_id).await?;
    let source = StoragePolicy::try_from(source_policy.clone())?;
    let source_connector_id = ConnectorId::declared(source_policy.connector_id.clone());
    let registry = state.driver_registry().connectors();
    let (target_connector, _) = registry
        .require_saved_transition(
            &input.target_connector_id,
            &input.transition_id,
            &source_connector_id,
        )
        .map_err(|error| {
            validation_error_with_code(
                ApiErrorCode::PolicyConnectorTransitionUnsupported,
                error.to_string(),
            )
        })?;

    ensure_no_active_upload_sessions(state.writer_db(), policy_id).await?;
    let source_credential =
        storage_policy_connector_credential_repo::find_by_policy(state.writer_db(), policy_id)
            .await?;
    let source_credential_payload = source_credential
        .as_ref()
        .map(|credential| decode_stored_credential(state, credential))
        .transpose()?;
    let prepared = target_connector.prepare_inbound_transition(
        &input.transition_id,
        StorageConnectorTransitionSavedState {
            policy: &source_policy,
            connector_config: &source.connector_config,
            behavior: &source.behavior,
            credential: source_credential_payload.as_ref(),
        },
    )?;
    let candidate_policy = candidate_policy(&source_policy, &prepared)?;
    let candidate_driver = target_connector
        .build_draft_driver(
            &crate::storage::connectors::remote_connector_context(state),
            &candidate_policy,
            &prepared.credential,
        )
        .await?;
    verify_existing_objects(state, policy_id, candidate_driver.as_ref()).await?;

    let txn = transaction::begin(state.writer_db()).await?;
    let locked_policy = policy_repo::lock_by_id(&txn, policy_id).await?;
    ensure_policy_unchanged(&source_policy, &locked_policy)?;
    let locked_credential =
        storage_policy_connector_credential_repo::lock_by_policy(&txn, policy_id).await?;
    ensure_credential_unchanged(source_credential.as_ref(), locked_credential.as_ref())?;
    ensure_no_active_upload_sessions(&txn, policy_id).await?;

    let locked_source = StoragePolicy::try_from(locked_policy.clone())?;
    let locked_credential_payload = locked_credential
        .as_ref()
        .map(|credential| decode_stored_credential(state, credential))
        .transpose()?;
    let prepared = target_connector.prepare_inbound_transition(
        &input.transition_id,
        StorageConnectorTransitionSavedState {
            policy: &locked_policy,
            connector_config: &locked_source.connector_config,
            behavior: &locked_source.behavior,
            credential: locked_credential_payload.as_ref(),
        },
    )?;
    let storage_config = encode_storage_config(&prepared.connector_config, &prepared.behavior)?;

    let mut active: storage_policy::ActiveModel = locked_policy.into();
    active.connector_id = Set(prepared.connector_config.connector_id.as_str().to_string());
    active.storage_config = Set(storage_config);
    active.updated_at = Set(Utc::now());
    let updated = active
        .update(&txn)
        .await
        .map_aster_err(AsterError::database_operation)?;

    match prepared.credential {
        crate::storage::StorageConnectorCredentialInput::None => {
            storage_policy_connector_credential_repo::delete_by_policy(&txn, policy_id).await?;
        }
        credential => {
            target_connector
                .persist_credential(
                    &txn,
                    &state.config().auth.storage_credential_secret_key,
                    policy_id,
                    &prepared.connector_config,
                    credential,
                )
                .await?;
        }
    }
    transaction::commit(txn).await?;

    state.driver_registry().invalidate(policy_id);
    state
        .driver_registry()
        .reload_storage_policy_credentials(state.writer_db(), state.config())
        .await?;
    state
        .driver_registry()
        .reload_policy_snapshot(state.policy_snapshot(), state.writer_db())
        .await?;
    crate::services::ops::config::invalidate_public_thumbnail_support_cache();
    crate::services::ops::config::invalidate_public_media_data_support_cache();
    crate::services::ops::config::runtime::publish_storage_topology_reload_after_commit(
        state,
        "connector_transition",
        "storage_policy",
        policy_id,
    )
    .await;

    let policy = policy_repo::find_by_id(state.writer_db(), updated.id)
        .await
        .and_then(StoragePolicy::try_from)?;
    Ok(ExecutedStorageConnectorTransition {
        policy,
        source_connector_id,
    })
}

fn decode_stored_credential(
    state: &impl RemoteProtocolRuntimeState,
    credential: &storage_policy_connector_credential::Model,
) -> Result<StoredStorageConnectorCredentialPayload> {
    let connector_id = ConnectorId::declared(credential.connector_id.clone());
    connector_id
        .validate()
        .map_err(|error| AsterError::database_operation(error.to_string()))?;
    let schema_version = u32::try_from(credential.schema_version).map_err(|_| {
        AsterError::database_operation("stored connector credential schema version is negative")
    })?;
    let values = crate::storage::connectors::decode_connector_credential(
        &state.config().auth.storage_credential_secret_key,
        credential,
        &connector_id,
        schema_version,
    )?;
    Ok(StoredStorageConnectorCredentialPayload {
        connector_id,
        schema_version,
        values,
    })
}

fn candidate_policy(
    source: &storage_policy::Model,
    prepared: &PreparedStorageConnectorTransition,
) -> Result<storage_policy::Model> {
    let mut candidate = source.clone();
    candidate.connector_id = prepared.connector_config.connector_id.as_str().to_string();
    candidate.storage_config =
        encode_storage_config(&prepared.connector_config, &prepared.behavior)?;
    Ok(candidate)
}

fn encode_storage_config(
    connector_config: &ConnectorConfigEnvelope,
    behavior: &aster_drive_storage::StoragePolicyBehaviorConfig,
) -> Result<aster_drive_model::types::StoredStoragePolicyConfig> {
    let connector_config = ConnectorConfigEnvelope::new(
        connector_config.connector_id.clone(),
        connector_config.schema_version,
        serde_json::to_value(&connector_config.values).map_err(|error| {
            AsterError::internal_error(format!("serialize connector transition config: {error}"))
        })?,
    );
    aster_drive_storage::encode_storage_policy_config(connector_config, behavior.clone())
        .map(aster_drive_model::types::StoredStoragePolicyConfig)
        .map_err(|error| {
            AsterError::internal_error(format!("encode connector transition config: {error}"))
        })
}

async fn ensure_no_active_upload_sessions<C: sea_orm::ConnectionTrait>(
    db: &C,
    policy_id: i64,
) -> Result<()> {
    let active = upload_session_repo::count_active_by_policy(db, policy_id).await?;
    if active == 0 {
        return Ok(());
    }
    Err(precondition_failed_with_code(
        ApiErrorCode::PolicyUploadSessionsExist,
        format!(
            "storage policy connector transition is blocked by {active} active upload session(s)"
        ),
    ))
}

fn ensure_policy_unchanged(
    expected: &storage_policy::Model,
    actual: &storage_policy::Model,
) -> Result<()> {
    if expected.connector_id == actual.connector_id
        && expected.storage_config == actual.storage_config
        && expected.updated_at == actual.updated_at
    {
        return Ok(());
    }
    Err(precondition_failed_with_code(
        ApiErrorCode::PolicyConnectorTransitionConflict,
        "storage policy changed while its connector transition was being verified",
    ))
}

fn ensure_credential_unchanged(
    expected: Option<&storage_policy_connector_credential::Model>,
    actual: Option<&storage_policy_connector_credential::Model>,
) -> Result<()> {
    let unchanged = match (expected, actual) {
        (None, None) => true,
        (Some(expected), Some(actual)) => {
            expected.id == actual.id
                && expected.connector_id == actual.connector_id
                && expected.schema_version == actual.schema_version
                && expected.revision == actual.revision
                && expected.ciphertext == actual.ciphertext
        }
        _ => false,
    };
    if unchanged {
        return Ok(());
    }
    Err(precondition_failed_with_code(
        ApiErrorCode::PolicyConnectorTransitionConflict,
        "storage policy credential changed while its connector transition was being verified",
    ))
}

async fn verify_existing_objects(
    state: &impl RemoteProtocolRuntimeState,
    policy_id: i64,
    driver: &dyn aster_drive_storage::StorageDriver,
) -> Result<()> {
    let blobs = file_repo::find_blobs_by_policy_paginated(
        state.writer_db(),
        policy_id,
        0,
        TRANSITION_OBJECT_SAMPLE_SIZE,
    )
    .await?;
    verify_object_sample(&blobs, driver).await
}

async fn verify_object_sample(
    blobs: &[aster_drive_model::entities::file_blob::Model],
    driver: &dyn aster_drive_storage::StorageDriver,
) -> Result<()> {
    for blob in blobs {
        let metadata = driver.metadata(&blob.storage_path).await.map_err(|error| {
            AsterError::storage_driver_error(format!(
                "verify object '{}' (blob id {}) before connector transition: {error}",
                blob.storage_path, blob.id
            ))
        })?;
        let actual_size =
            aster_forge_utils::numbers::u64_to_i64(metadata.size, "blob metadata size")?;
        if actual_size != blob.size {
            return Err(AsterError::storage_driver_error(format!(
                "object '{}' (blob id {}) size mismatch before connector transition: expected {}, got {}",
                blob.storage_path, blob.id, blob.size, actual_size
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use aster_drive_storage::{BlobMetadata, StorageDriver};
    use async_trait::async_trait;

    use super::{ensure_credential_unchanged, ensure_policy_unchanged, verify_object_sample};
    use crate::api::api_error_code::ApiErrorCode;

    struct MetadataDriver {
        size: Option<u64>,
    }

    #[async_trait]
    impl StorageDriver for MetadataDriver {
        async fn put(&self, _path: &str, _data: &[u8]) -> aster_drive_storage::Result<String> {
            unreachable!("object verification only reads metadata")
        }

        async fn get(&self, _path: &str) -> aster_drive_storage::Result<Vec<u8>> {
            unreachable!("object verification only reads metadata")
        }

        async fn get_stream(
            &self,
            _path: &str,
        ) -> aster_drive_storage::Result<Box<dyn tokio::io::AsyncRead + Unpin + Send>> {
            unreachable!("object verification only reads metadata")
        }

        async fn delete(&self, _path: &str) -> aster_drive_storage::Result<()> {
            unreachable!("object verification only reads metadata")
        }

        async fn exists(&self, _path: &str) -> aster_drive_storage::Result<bool> {
            unreachable!("object verification only reads metadata")
        }

        async fn metadata(&self, _path: &str) -> aster_drive_storage::Result<BlobMetadata> {
            self.size
                .map(|size| BlobMetadata {
                    size,
                    content_type: None,
                })
                .ok_or_else(|| {
                    aster_drive_storage::storage_driver_error(
                        aster_drive_storage::StorageErrorKind::Transient,
                        "metadata probe failed",
                    )
                })
        }
    }

    fn blob(size: i64) -> aster_drive_model::entities::file_blob::Model {
        let now = chrono::Utc::now();
        aster_drive_model::entities::file_blob::Model {
            id: 7,
            hash: "sha256:test".to_string(),
            size,
            policy_id: 3,
            storage_path: "tenant-a/file.bin".to_string(),
            thumbnail_path: None,
            thumbnail_processor: None,
            thumbnail_version: None,
            ref_count: 1,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn source_snapshot_guards_detect_policy_and_credential_changes() {
        use aster_drive_model::entities::storage_policy_connector_credential;
        use aster_drive_model::types::{
            ObjectStorageDownloadStrategy, ObjectStorageUploadStrategy,
        };
        use chrono::Utc;

        let policy = crate::storage::connectors::test_support::s3_policy(
            "https://bucket.cos.ap-hongkong.myqcloud.com",
            "bucket",
            "tenant-a",
            ObjectStorageUploadStrategy::RelayStream,
            ObjectStorageDownloadStrategy::RelayStream,
        );
        assert!(ensure_policy_unchanged(&policy, &policy).is_ok());
        let mut changed = policy.clone();
        changed.connector_id = "plugin.changed".to_string();
        let policy_error =
            ensure_policy_unchanged(&policy, &changed).expect_err("changed policy snapshot");
        assert_eq!(
            policy_error.http_status(),
            actix_web::http::StatusCode::PRECONDITION_FAILED
        );
        assert_eq!(
            policy_error.api_error_code(),
            ApiErrorCode::PolicyConnectorTransitionConflict
        );

        let now = Utc::now();
        let credential = storage_policy_connector_credential::Model {
            id: 1,
            policy_id: policy.id,
            connector_id: policy.connector_id.clone(),
            schema_version: 1,
            revision: 2,
            ciphertext: "encrypted".to_string(),
            created_at: now,
            updated_at: now,
        };
        assert!(ensure_credential_unchanged(None, None).is_ok());
        assert!(ensure_credential_unchanged(Some(&credential), Some(&credential)).is_ok());
        let missing_error = ensure_credential_unchanged(Some(&credential), None)
            .expect_err("missing credential snapshot");
        assert_eq!(
            missing_error.http_status(),
            actix_web::http::StatusCode::PRECONDITION_FAILED
        );
        assert_eq!(
            missing_error.api_error_code(),
            ApiErrorCode::PolicyConnectorTransitionConflict
        );
        let mut changed_credential = credential.clone();
        changed_credential.revision += 1;
        let credential_error =
            ensure_credential_unchanged(Some(&credential), Some(&changed_credential))
                .expect_err("changed credential snapshot");
        assert_eq!(
            credential_error.http_status(),
            actix_web::http::StatusCode::PRECONDITION_FAILED
        );
        assert_eq!(
            credential_error.api_error_code(),
            ApiErrorCode::PolicyConnectorTransitionConflict
        );
    }

    #[tokio::test]
    async fn object_sample_requires_readable_metadata_with_matching_size() {
        let blobs = vec![blob(42)];
        verify_object_sample(&blobs, &MetadataDriver { size: Some(42) })
            .await
            .expect("matching object metadata");

        let mismatch = verify_object_sample(&blobs, &MetadataDriver { size: Some(41) })
            .await
            .expect_err("size mismatch must reject transition");
        assert!(mismatch.to_string().contains("size mismatch"));

        let failure = verify_object_sample(&blobs, &MetadataDriver { size: None })
            .await
            .expect_err("metadata failure must reject transition");
        assert!(failure.to_string().contains("metadata probe failed"));
        assert!(failure.to_string().contains("tenant-a/file.bin"));
    }
}
