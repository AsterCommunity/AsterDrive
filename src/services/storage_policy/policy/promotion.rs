//! Descriptor-driven in-place storage connector promotion.

use std::collections::BTreeMap;

use aster_drive_model::entities::storage_policy;
use aster_drive_storage::{
    ConnectorConfigEnvelope, ConnectorId, StorageConnectorCredentialMode,
    StorageConnectorPromotionDescriptor, StorageConnectorPromotionFieldMapping,
    StorageConnectorPromotionId, StorageConnectorPromotionValueMatcher,
    StoragePolicyConfigEnvelope,
};
use aster_forge_db::transaction;

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{
    file_repo, policy_repo, storage_policy_connector_credential_repo, upload_session_repo,
};
use crate::errors::{AsterError, Result, validation_error_with_code};
use crate::runtime::RemoteProtocolRuntimeState;
use crate::storage::StorageConnectorCredentialInput;

use super::models::{PromoteStoragePolicyConnectorInput, StoragePolicy};

const PROMOTION_SAMPLE_SIZE: u64 = 10;

pub(super) struct StoragePolicyPromotionExecution {
    pub policy: StoragePolicy,
    pub source_connector_id: ConnectorId,
    pub target_connector_id: ConnectorId,
    pub promotion_id: StorageConnectorPromotionId,
    pub verified_blob_count: usize,
}

struct PromotionCommit {
    existing: storage_policy::Model,
    target_connector_id: ConnectorId,
    target_descriptor: aster_drive_storage::StorageConnectorDescriptor,
    encoded_storage_config: aster_drive_model::types::StoredStoragePolicyConfig,
    credential: Option<CredentialPromotionCommit>,
}

struct CredentialPromotionCommit {
    source: aster_drive_model::entities::storage_policy_connector_credential::Model,
    target_payload: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PromotionCredentialPath {
    Static,
    None,
}

pub(super) async fn execute_connector_promotion(
    state: &(impl RemoteProtocolRuntimeState + Sync),
    id: i64,
    input: PromoteStoragePolicyConnectorInput,
) -> Result<StoragePolicyPromotionExecution> {
    let existing = policy_repo::find_by_id(state.writer_db(), id).await?;
    let source_storage_config = decode_policy_storage_config(&existing)?;
    let source_connector_id = source_storage_config.connector.connector_id.clone();
    let source_config_values =
        decode_source_config_values(id, &source_storage_config.connector.values)?;
    let target_connector_id = input.target_connector_id.clone();
    let promotion_id = input.promotion_id.clone();
    let connectors = state.driver_registry().connectors();
    let target_connector = connectors.require_input_connector(&target_connector_id)?;
    let target_descriptor = target_connector.descriptor();
    let promotion = connectors
        .promotion_descriptor(&target_connector_id, &promotion_id)?
        .ok_or_else(|| {
            validation_error_with_code(
                ApiErrorCode::PolicyPromotionTargetUnsupported,
                format!(
                    "storage connector '{}' does not declare promotion '{}'",
                    target_connector_id,
                    promotion_id.as_str()
                ),
            )
        })?;
    validate_promotion_source(id, &promotion, &source_connector_id, &source_config_values)?;
    crate::services::ops::deployment::validate_storage_policy_driver(
        connectors,
        state.config(),
        &target_connector_id,
    )?;
    ensure_no_active_uploads(state.writer_db(), id).await?;

    let target_values =
        map_promotion_config_values(&promotion.config_mappings, &source_config_values)?;
    let target_config = ConnectorConfigEnvelope::new(
        target_connector_id.clone(),
        target_descriptor.config_schema_version,
        target_values,
    );
    let target_config = crate::storage::connectors::normalize_connector_config(
        connectors,
        state.writer_db(),
        target_config,
    )
    .await?;
    ensure_preserved_promotion_values(
        &promotion.config_mappings,
        &source_config_values,
        &target_config.values,
    )?;

    let source_descriptor = connectors.require_policy(&existing)?.descriptor();
    let source_credential =
        storage_policy_connector_credential_repo::find_by_policy(state.writer_db(), id).await?;
    let (target_credential, credential_commit) =
        match promotion_credential_path(target_descriptor.credential_mode)? {
            PromotionCredentialPath::Static => {
                let source_credential = source_credential.as_ref().ok_or_else(|| {
                    AsterError::database_operation(format!(
                        "storage policy #{id} has no connector credential to promote"
                    ))
                })?;
                let source_schema_version =
                    crate::storage::connectors::credential_schema_version(&source_descriptor)?;
                let source_values = crate::storage::connectors::decode_connector_credential(
                    &state.config().auth.storage_credential_secret_key,
                    source_credential,
                    &source_connector_id,
                    source_schema_version,
                )?;
                let target_values = map_promotion_credential_values(
                    &promotion.credential_mappings,
                    &source_values,
                )?;
                let credential = StorageConnectorCredentialInput::Static(target_values.clone());
                crate::storage::connectors::validate_credential_input(
                    connectors,
                    &target_connector_id,
                    &credential,
                )?;
                (
                    credential,
                    Some(CredentialPromotionCommit {
                        source: source_credential.clone(),
                        target_payload: target_values,
                    }),
                )
            }
            PromotionCredentialPath::None => (StorageConnectorCredentialInput::None, None),
        };

    let behavior = source_storage_config.behavior.values;
    target_connector.validate_policy_behavior(&behavior)?;
    let persisted_target_config = ConnectorConfigEnvelope::new(
        target_config.connector_id.clone(),
        target_config.schema_version,
        serde_json::Value::Object(
            target_config
                .values
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        ),
    );
    let encoded_storage_config =
        aster_drive_storage::encode_storage_policy_config(persisted_target_config, behavior)
            .map(aster_drive_model::types::StoredStoragePolicyConfig)
            .map_err(|error| {
                AsterError::internal_error(format!(
                    "serialize promoted storage policy config: {error}"
                ))
            })?;
    let mut candidate = existing.clone();
    candidate.connector_id = target_connector_id.as_str().to_string();
    candidate.storage_config = encoded_storage_config.clone();
    let candidate_driver = target_connector
        .build_draft_driver(
            &crate::storage::connectors::remote_connector_context(state),
            &candidate,
            &target_credential,
        )
        .await?;
    let blobs = file_repo::find_stored_blobs_by_policy_paginated(
        state.writer_db(),
        id,
        0,
        PROMOTION_SAMPLE_SIZE,
    )
    .await?;
    verify_promotion_blob_sample(candidate_driver.as_ref(), &blobs).await?;
    let verified_blob_count = blobs.len();

    let result = commit_connector_promotion(
        state,
        id,
        PromotionCommit {
            existing,
            target_connector_id: target_connector_id.clone(),
            target_descriptor,
            encoded_storage_config,
            credential: credential_commit,
        },
    )
    .await?;
    let policy = reload_promoted_policy(state, result.id).await?;
    Ok(StoragePolicyPromotionExecution {
        policy,
        source_connector_id,
        target_connector_id,
        promotion_id,
        verified_blob_count,
    })
}

async fn commit_connector_promotion(
    state: &(impl RemoteProtocolRuntimeState + Sync),
    policy_id: i64,
    commit: PromotionCommit,
) -> Result<storage_policy::Model> {
    let txn = transaction::begin(state.writer_db()).await?;
    let locked = policy_repo::lock_by_id(&txn, policy_id).await?;
    ensure_promotion_policy_unchanged(&commit.existing, &locked)?;
    ensure_no_active_uploads(&txn, policy_id).await?;
    let result = policy_repo::promote_connector(
        &txn,
        locked,
        commit.target_connector_id.as_str().to_string(),
        commit.encoded_storage_config,
    )
    .await?;
    if let Some(credential) = commit.credential {
        let target_schema_version =
            crate::storage::connectors::credential_schema_version(&commit.target_descriptor)?;
        let target_schema_version_i32 = database_schema_version(target_schema_version)?;
        let plaintext = credential.target_payload.to_string();
        let ciphertext =
            crate::services::storage_policy::credential::crypto::encrypt_connector_credential(
                &state.config().auth.storage_credential_secret_key,
                policy_id,
                commit.target_connector_id.as_str(),
                target_schema_version,
                &plaintext,
            )?;
        let promoted = storage_policy_connector_credential_repo::promote_if_revision(
            &txn,
            storage_policy_connector_credential_repo::ConnectorCredentialPromotion {
                policy_id,
                source_connector_id: &credential.source.connector_id,
                source_schema_version: credential.source.schema_version,
                expected_revision: credential.source.revision,
                target_connector_id: commit.target_connector_id.as_str().to_string(),
                target_schema_version: target_schema_version_i32,
                ciphertext,
            },
        )
        .await?;
        require_credential_promotion(promoted)?;
    }
    transaction::commit(txn).await?;
    Ok(result)
}

fn require_credential_promotion(promoted: bool) -> Result<()> {
    if promoted {
        return Ok(());
    }
    Err(AsterError::validation_error(
        "storage connector credential changed while promotion was being validated; retry the operation",
    ))
}

fn database_schema_version(schema_version: u32) -> Result<i32> {
    i32::try_from(schema_version).map_err(|_| {
        AsterError::validation_error("connector credential schema version exceeds database range")
    })
}

fn promotion_credential_path(
    mode: StorageConnectorCredentialMode,
) -> Result<PromotionCredentialPath> {
    match mode {
        StorageConnectorCredentialMode::StaticSecret => Ok(PromotionCredentialPath::Static),
        StorageConnectorCredentialMode::None => Ok(PromotionCredentialPath::None),
        _ => Err(validation_error_with_code(
            ApiErrorCode::PolicyPromotionTargetUnsupported,
            "connector promotions currently require compatible static credentials or credential-free connectors",
        )),
    }
}

async fn reload_promoted_policy(
    state: &(impl RemoteProtocolRuntimeState + Sync),
    policy_id: i64,
) -> Result<StoragePolicy> {
    state
        .driver_registry()
        .reload_storage_policy_credentials(state.writer_db(), state.config())
        .await?;
    state.driver_registry().invalidate(policy_id);
    state
        .driver_registry()
        .reload_policy_snapshot(state.policy_snapshot(), state.writer_db())
        .await?;
    crate::services::ops::config::invalidate_public_thumbnail_support_cache();
    crate::services::ops::config::invalidate_public_media_data_support_cache();
    crate::services::ops::config::runtime::publish_storage_topology_reload_after_commit(
        state,
        "promote_connector",
        "storage_policy",
        policy_id,
    )
    .await;
    policy_repo::find_by_id(state.writer_db(), policy_id)
        .await
        .and_then(StoragePolicy::try_from)
}

fn validate_promotion_source(
    policy_id: i64,
    promotion: &StorageConnectorPromotionDescriptor,
    source_connector_id: &ConnectorId,
    source_config_values: &BTreeMap<String, serde_json::Value>,
) -> Result<()> {
    if &promotion.source_connector_id != source_connector_id {
        return Err(validation_error_with_code(
            ApiErrorCode::PolicyPromotionSourceUnsupported,
            format!(
                "storage policy #{policy_id} uses connector '{}', but promotion '{}' requires '{}'",
                source_connector_id,
                promotion.promotion_id.as_str(),
                promotion.source_connector_id
            ),
        ));
    }
    if promotion_requirements_match(promotion, source_config_values) {
        return Ok(());
    }
    Err(validation_error_with_code(
        ApiErrorCode::PolicyPromotionTargetUnsupported,
        format!(
            "storage policy #{policy_id} does not satisfy promotion '{}' requirements",
            promotion.promotion_id.as_str()
        ),
    ))
}

async fn ensure_no_active_uploads<C: sea_orm::ConnectionTrait>(
    db: &C,
    policy_id: i64,
) -> Result<()> {
    let active_upload_sessions = upload_session_repo::count_active_by_policy(db, policy_id).await?;
    if active_upload_sessions == 0 {
        return Ok(());
    }
    Err(validation_error_with_code(
        ApiErrorCode::PolicyUploadSessionsExist,
        format!(
            "cannot promote policy: {active_upload_sessions} active upload session(s) still reference it"
        ),
    ))
}

async fn verify_promotion_blob_sample(
    driver: &dyn aster_drive_storage::StorageDriver,
    blobs: &[aster_drive_model::entities::file_blob::Model],
) -> Result<()> {
    for blob in blobs {
        let storage_path = blob.storage_path.as_deref().ok_or_else(|| {
            AsterError::database_operation(format!("stored blob {} has no storage path", blob.id))
        })?;
        let metadata = driver.metadata(storage_path).await.map_err(|error| {
            AsterError::storage_driver_error(format!(
                "verify existing object '{storage_path}' (blob id {}) before connector promotion: {error}",
                blob.id
            ))
        })?;
        let actual_size =
            aster_forge_utils::numbers::u64_to_i64(metadata.size, "blob metadata size")?;
        if actual_size != blob.size {
            return Err(AsterError::storage_driver_error(format!(
                "object '{storage_path}' (blob id {}) size mismatch before connector promotion: expected {}, got {actual_size}",
                blob.id, blob.size
            )));
        }
    }
    Ok(())
}

fn decode_policy_storage_config(
    policy: &storage_policy::Model,
) -> Result<StoragePolicyConfigEnvelope> {
    let config: StoragePolicyConfigEnvelope = serde_json::from_str(policy.storage_config.as_ref())
        .map_err(|error| {
            AsterError::database_operation(format!(
                "storage policy {} has invalid storage_config: {error}",
                policy.id
            ))
        })?;
    if config.connector.connector_id.as_str() != policy.connector_id {
        return Err(AsterError::database_operation(format!(
            "storage policy {} connector id does not match storage_config",
            policy.id
        )));
    }
    Ok(config)
}

fn decode_source_config_values(
    policy_id: i64,
    values: &serde_json::Value,
) -> Result<BTreeMap<String, serde_json::Value>> {
    serde_json::from_value(values.clone()).map_err(|error| {
        AsterError::database_operation(format!(
            "storage policy {policy_id} connector config must be an object: {error}"
        ))
    })
}

fn promotion_requirements_match(
    promotion: &StorageConnectorPromotionDescriptor,
    values: &BTreeMap<String, serde_json::Value>,
) -> bool {
    promotion.requirements.iter().all(|requirement| {
        let Some(value) = values
            .get(&requirement.source_field)
            .and_then(serde_json::Value::as_str)
        else {
            return false;
        };
        let matches = match &requirement.matcher {
            StorageConnectorPromotionValueMatcher::StringEquals {
                value: expected,
                case_sensitive,
            } => compare_promotion_text(value, expected, *case_sensitive, |left, right| {
                left == right
            }),
            StorageConnectorPromotionValueMatcher::StringSuffix {
                suffix,
                case_sensitive,
            } => compare_promotion_text(value, suffix, *case_sensitive, |left, right| {
                left.ends_with(right)
            }),
            StorageConnectorPromotionValueMatcher::StringPrefix {
                prefix,
                case_sensitive,
            } => compare_promotion_text(value, prefix, *case_sensitive, |left, right| {
                left.starts_with(right)
            }),
            StorageConnectorPromotionValueMatcher::UrlHostSuffix { suffix } => {
                url::Url::parse(value)
                    .ok()
                    .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
                    .is_some_and(|host| host.ends_with(&suffix.to_ascii_lowercase()))
            }
        };
        matches != requirement.negate
    })
}

fn compare_promotion_text(
    value: &str,
    expected: &str,
    case_sensitive: bool,
    compare: impl FnOnce(&str, &str) -> bool,
) -> bool {
    if case_sensitive {
        compare(value, expected)
    } else {
        compare(&value.to_ascii_lowercase(), &expected.to_ascii_lowercase())
    }
}

fn map_promotion_config_values(
    mappings: &[StorageConnectorPromotionFieldMapping],
    source: &BTreeMap<String, serde_json::Value>,
) -> Result<BTreeMap<String, serde_json::Value>> {
    mappings
        .iter()
        .map(|mapping| {
            source
                .get(&mapping.source_field)
                .cloned()
                .map(|value| (mapping.target_field.clone(), value))
                .ok_or_else(|| {
                    AsterError::validation_error(format!(
                        "promotion source config field '{}' is missing",
                        mapping.source_field
                    ))
                })
        })
        .collect()
}

fn map_promotion_credential_values(
    mappings: &[StorageConnectorPromotionFieldMapping],
    source: &serde_json::Value,
) -> Result<serde_json::Value> {
    let source = source.as_object().ok_or_else(|| {
        AsterError::database_operation("stored connector credential payload must be an object")
    })?;
    let target = mappings
        .iter()
        .map(|mapping| {
            source
                .get(&mapping.source_field)
                .cloned()
                .map(|value| (mapping.target_field.clone(), value))
                .ok_or_else(|| {
                    AsterError::database_operation(format!(
                        "stored connector credential is missing promotion field '{}'",
                        mapping.source_field
                    ))
                })
        })
        .collect::<Result<serde_json::Map<String, serde_json::Value>>>()?;
    Ok(serde_json::Value::Object(target))
}

fn ensure_preserved_promotion_values(
    mappings: &[StorageConnectorPromotionFieldMapping],
    source: &BTreeMap<String, serde_json::Value>,
    target: &BTreeMap<String, serde_json::Value>,
) -> Result<()> {
    for mapping in mappings.iter().filter(|mapping| mapping.preserve_value) {
        if source.get(&mapping.source_field) != target.get(&mapping.target_field) {
            return Err(AsterError::validation_error(format!(
                "connector promotion cannot change preserved field '{}'",
                mapping.source_field
            )));
        }
    }
    Ok(())
}

fn ensure_promotion_policy_unchanged(
    expected: &storage_policy::Model,
    locked: &storage_policy::Model,
) -> Result<()> {
    if locked.connector_id == expected.connector_id
        && locked.storage_config == expected.storage_config
        && locked.updated_at == expected.updated_at
    {
        return Ok(());
    }
    Err(AsterError::validation_error(
        "storage policy changed while connector promotion was being validated; retry the operation",
    ))
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use tokio::io::AsyncRead;

    use super::*;
    use aster_drive_storage::{
        BlobMetadata, StorageConnectorPromotionRequirement, StorageDriver, StorageErrorKind,
        storage_driver_error,
    };

    struct PromotionMetadataDriver {
        size: Option<u64>,
    }

    #[async_trait]
    impl StorageDriver for PromotionMetadataDriver {
        async fn put(&self, _path: &str, _data: &[u8]) -> aster_drive_storage::Result<String> {
            Err(unexpected_driver_call("put"))
        }

        async fn get(&self, _path: &str) -> aster_drive_storage::Result<Vec<u8>> {
            Err(unexpected_driver_call("get"))
        }

        async fn get_stream(
            &self,
            _path: &str,
        ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
            Err(unexpected_driver_call("get_stream"))
        }

        async fn delete(&self, _path: &str) -> aster_drive_storage::Result<()> {
            Err(unexpected_driver_call("delete"))
        }

        async fn exists(&self, _path: &str) -> aster_drive_storage::Result<bool> {
            Err(unexpected_driver_call("exists"))
        }

        async fn metadata(&self, _path: &str) -> aster_drive_storage::Result<BlobMetadata> {
            self.size
                .map(|size| BlobMetadata {
                    size,
                    content_type: None,
                })
                .ok_or_else(|| {
                    storage_driver_error(StorageErrorKind::Transient, "metadata unavailable")
                })
        }
    }

    fn unexpected_driver_call(operation: &str) -> aster_drive_storage::StorageError {
        storage_driver_error(
            StorageErrorKind::Unsupported,
            format!("unexpected {operation}"),
        )
    }

    fn promotion_blob(size: i64) -> aster_drive_model::entities::file_blob::Model {
        let now = chrono::Utc::now();
        aster_drive_model::entities::file_blob::Model {
            id: 7,
            hash: "hash".to_string(),
            size,
            policy_id: 3,
            storage_path: Some("objects/sample".to_string()),
            backing: aster_drive_model::types::file_blob::FileBlobBacking::Stored,
            thumbnail_path: None,
            thumbnail_processor: None,
            thumbnail_version: None,
            ref_count: 1,
            created_at: now,
            updated_at: now,
        }
    }

    fn promotion_with_requirements(
        requirements: Vec<StorageConnectorPromotionRequirement>,
    ) -> StorageConnectorPromotionDescriptor {
        StorageConnectorPromotionDescriptor {
            promotion_id: StorageConnectorPromotionId::declared("test_promotion"),
            source_connector_id: ConnectorId::declared("com.example.source"),
            description_key: "description".to_string(),
            confirmation_key: "confirmation".to_string(),
            requirements,
            config_mappings: Vec::new(),
            credential_mappings: Vec::new(),
        }
    }

    #[tokio::test]
    async fn sample_verifies_metadata_and_size() {
        let blob = promotion_blob(12);
        verify_promotion_blob_sample(
            &PromotionMetadataDriver { size: Some(12) },
            std::slice::from_ref(&blob),
        )
        .await
        .expect("matching metadata should pass");

        let error = verify_promotion_blob_sample(
            &PromotionMetadataDriver { size: Some(11) },
            std::slice::from_ref(&blob),
        )
        .await
        .expect_err("size mismatch must block promotion");
        assert!(error.message().contains("size mismatch"));

        let error = verify_promotion_blob_sample(&PromotionMetadataDriver { size: None }, &[blob])
            .await
            .expect_err("metadata failure must block promotion");
        assert!(error.message().contains("verify existing object"));
        assert!(error.message().contains("blob id 7"));

        let mut missing_path = promotion_blob(12);
        missing_path.storage_path = None;
        let error = verify_promotion_blob_sample(
            &PromotionMetadataDriver { size: Some(12) },
            &[missing_path],
        )
        .await
        .expect_err("stored blob without path must fail");
        assert!(
            error
                .message()
                .contains("stored blob 7 has no storage path")
        );
    }

    #[tokio::test]
    async fn metadata_test_driver_rejects_unexpected_operations() {
        let driver = PromotionMetadataDriver { size: Some(1) };
        assert!(driver.put("path", b"data").await.is_err());
        assert!(driver.get("path").await.is_err());
        assert!(driver.get_stream("path").await.is_err());
        assert!(driver.delete("path").await.is_err());
        assert!(driver.exists("path").await.is_err());
        assert!(unexpected_driver_call("probe").message().contains("probe"));
    }

    #[test]
    fn matchers_cover_prefix_suffix_url_case_negation_and_all_requirements() {
        let values = BTreeMap::from([
            ("provider".to_string(), serde_json::json!("Alibaba-OSS")),
            ("bucket".to_string(), serde_json::json!("archive-prod")),
            (
                "endpoint".to_string(),
                serde_json::json!("HTTPS://archive.oss-cn-hangzhou.aliyuncs.com"),
            ),
            ("region".to_string(), serde_json::json!("cn-hangzhou")),
        ]);
        let promotion = promotion_with_requirements(vec![
            requirement(
                "provider",
                StorageConnectorPromotionValueMatcher::StringEquals {
                    value: "alibaba-oss".to_string(),
                    case_sensitive: false,
                },
                false,
            ),
            requirement(
                "bucket",
                StorageConnectorPromotionValueMatcher::StringSuffix {
                    suffix: "-PROD".to_string(),
                    case_sensitive: false,
                },
                false,
            ),
            requirement(
                "endpoint",
                StorageConnectorPromotionValueMatcher::StringPrefix {
                    prefix: "https://".to_string(),
                    case_sensitive: false,
                },
                false,
            ),
            requirement(
                "endpoint",
                StorageConnectorPromotionValueMatcher::UrlHostSuffix {
                    suffix: ".aliyuncs.com".to_string(),
                },
                false,
            ),
            requirement(
                "region",
                StorageConnectorPromotionValueMatcher::StringEquals {
                    value: "auto".to_string(),
                    case_sensitive: false,
                },
                true,
            ),
        ]);
        assert!(promotion_requirements_match(&promotion, &values));

        let case_sensitive = promotion_with_requirements(vec![requirement(
            "provider",
            StorageConnectorPromotionValueMatcher::StringEquals {
                value: "alibaba-oss".to_string(),
                case_sensitive: true,
            },
            false,
        )]);
        assert!(!promotion_requirements_match(&case_sensitive, &values));

        let missing = promotion_with_requirements(vec![requirement(
            "missing",
            StorageConnectorPromotionValueMatcher::StringEquals {
                value: "value".to_string(),
                case_sensitive: false,
            },
            false,
        )]);
        assert!(!promotion_requirements_match(&missing, &values));

        let mut automatic_region = values.clone();
        automatic_region.insert("region".to_string(), serde_json::json!("AUTO"));
        assert!(!promotion_requirements_match(&promotion, &automatic_region));

        for endpoint in [
            "not a url",
            "https://evilaliyuncs.com",
            "http://archive.oss-cn-hangzhou.aliyuncs.com",
        ] {
            let mut invalid = values.clone();
            invalid.insert("endpoint".to_string(), serde_json::json!(endpoint));
            assert!(!promotion_requirements_match(&promotion, &invalid));
        }
    }

    fn requirement(
        source_field: &str,
        matcher: StorageConnectorPromotionValueMatcher,
        negate: bool,
    ) -> StorageConnectorPromotionRequirement {
        StorageConnectorPromotionRequirement {
            source_field: source_field.to_string(),
            matcher,
            negate,
        }
    }

    #[test]
    fn mapping_rejects_missing_and_changed_values() {
        let mappings = vec![StorageConnectorPromotionFieldMapping {
            source_field: "bucket".to_string(),
            target_field: "container".to_string(),
            preserve_value: true,
        }];
        let source = BTreeMap::from([("bucket".to_string(), serde_json::json!("archive"))]);
        let mapped = map_promotion_config_values(&mappings, &source).unwrap();
        assert_eq!(mapped["container"], "archive");
        ensure_preserved_promotion_values(&mappings, &source, &mapped).unwrap();

        let changed = BTreeMap::from([("container".to_string(), serde_json::json!("other"))]);
        assert!(ensure_preserved_promotion_values(&mappings, &source, &changed).is_err());
        assert!(map_promotion_config_values(&mappings, &BTreeMap::new()).is_err());

        let credential_mappings = vec![StorageConnectorPromotionFieldMapping {
            source_field: "access_key".to_string(),
            target_field: "secret_id".to_string(),
            preserve_value: false,
        }];
        assert!(
            map_promotion_credential_values(&credential_mappings, &serde_json::json!("bad"))
                .is_err()
        );
        assert!(
            map_promotion_credential_values(&credential_mappings, &serde_json::json!({})).is_err()
        );
    }

    #[test]
    fn policy_guard_detects_connector_config_and_revision_changes() {
        let expected = crate::storage::connectors::test_support::local_policy("data/uploads");
        ensure_promotion_policy_unchanged(&expected, &expected).unwrap();

        let mut changed_connector = expected.clone();
        changed_connector.connector_id = "com.example.changed".to_string();
        assert!(ensure_promotion_policy_unchanged(&expected, &changed_connector).is_err());

        let mut changed_config = expected.clone();
        changed_config.storage_config =
            aster_drive_model::types::StoredStoragePolicyConfig("{\"changed\":true}".to_string());
        assert!(ensure_promotion_policy_unchanged(&expected, &changed_config).is_err());

        let mut changed_revision = expected.clone();
        changed_revision.updated_at += chrono::Duration::seconds(1);
        assert!(ensure_promotion_policy_unchanged(&expected, &changed_revision).is_err());
        require_credential_promotion(true).unwrap();
        assert!(require_credential_promotion(false).is_err());
        assert_eq!(database_schema_version(1).unwrap(), 1);
        assert!(database_schema_version(u32::MAX).is_err());
        assert_eq!(
            promotion_credential_path(StorageConnectorCredentialMode::StaticSecret).unwrap(),
            PromotionCredentialPath::Static
        );
        assert_eq!(
            promotion_credential_path(StorageConnectorCredentialMode::None).unwrap(),
            PromotionCredentialPath::None
        );
        assert!(promotion_credential_path(StorageConnectorCredentialMode::OauthDelegated).is_err());
        assert!(promotion_credential_path(StorageConnectorCredentialMode::RemoteNode).is_err());
    }

    #[test]
    fn policy_config_decoder_rejects_invalid_json_and_connector_mismatch() {
        let mut invalid_json =
            crate::storage::connectors::test_support::local_policy("data/uploads");
        invalid_json.storage_config =
            aster_drive_model::types::StoredStoragePolicyConfig("not-json".to_string());
        assert!(decode_policy_storage_config(&invalid_json).is_err());

        let mut mismatch = crate::storage::connectors::test_support::local_policy("data/uploads");
        mismatch.connector_id = "com.example.mismatch".to_string();
        let error =
            decode_policy_storage_config(&mismatch).expect_err("connector mismatch must fail");
        assert!(error.message().contains("does not match storage_config"));

        let error = decode_source_config_values(7, &serde_json::json!("not-an-object"))
            .expect_err("non-object connector values must fail");
        assert!(
            error
                .message()
                .contains("connector config must be an object")
        );
    }
}
