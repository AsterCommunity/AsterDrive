//! 工作空间存储服务子模块：`store`。

pub(crate) mod from_temp;

use chrono::Utc;
use sea_orm::{ActiveModelTrait, Set};

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::file_repo;
use crate::entities::file;
use crate::errors::{AsterError, MapAsterErr, Result, precondition_failed_with_code};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::events::storage_change;

use super::{
    NewFileMode, PreparedNonDedupBlobUpload, WorkspaceStorageScope, check_quota,
    cleanup_preuploaded_blob_upload, create_new_file_from_blob,
    create_new_file_from_blob_with_actor_username, local_content_dedup_enabled,
    persist_preuploaded_blob, prepare_non_dedup_blob_upload, resolve_policy_for_size,
    update_storage_used, verify_file_access, verify_folder_access,
};

#[derive(Clone, Copy)]
pub(crate) struct StoreFromTempParams<'a> {
    pub scope: WorkspaceStorageScope,
    pub folder_id: Option<i64>,
    pub filename: &'a str,
    pub temp_path: &'a str,
    pub size: i64,
    pub existing_file_id: Option<i64>,
    pub skip_lock_check: bool,
}

impl<'a> StoreFromTempParams<'a> {
    pub(crate) fn new(
        scope: WorkspaceStorageScope,
        folder_id: Option<i64>,
        filename: &'a str,
        temp_path: &'a str,
        size: i64,
    ) -> Self {
        Self {
            scope,
            folder_id,
            filename,
            temp_path,
            size,
            existing_file_id: None,
            skip_lock_check: false,
        }
    }

    pub(crate) fn overwrite(mut self, existing_file_id: i64) -> Self {
        self.existing_file_id = Some(existing_file_id);
        self
    }

    pub(crate) fn skip_lock_check(mut self) -> Self {
        self.skip_lock_check = true;
        self
    }
}

#[derive(Clone, Default)]
pub(crate) struct StoreFromTempHints<'a> {
    pub resolved_policy: Option<crate::entities::storage_policy::Model>,
    pub precomputed_hash: Option<&'a str>,
    pub actor_username: Option<&'a str>,
    pub operation_context: crate::services::workspace::storage::StorageOperationContext,
}

pub(crate) struct StorePreuploadedNondedupParams<'a> {
    pub scope: WorkspaceStorageScope,
    pub folder_id: Option<i64>,
    pub filename: &'a str,
    pub size: i64,
    pub existing_file_id: Option<i64>,
    pub skip_lock_check: bool,
    pub policy: &'a crate::entities::storage_policy::Model,
    pub preuploaded_blob: PreparedNonDedupBlobUpload,
    pub actor_username: Option<&'a str>,
}

#[derive(Debug)]
struct VerifiedPreuploadedNondedupStoreBlob {
    size: i64,
    policy_id: i64,
    storage_path: String,
    prepared: PreparedNonDedupBlobUpload,
}

impl VerifiedPreuploadedNondedupStoreBlob {
    fn new(size: i64, policy_id: i64, prepared: PreparedNonDedupBlobUpload) -> Result<Self> {
        if size < 0 {
            return Err(AsterError::validation_error(format!(
                "verified preuploaded store blob size must be non-negative, got {size}",
            )));
        }
        if prepared.size() != size {
            return Err(AsterError::validation_error(format!(
                "preuploaded blob size {} does not match verified store size {size}",
                prepared.size(),
            )));
        }
        if prepared.policy_id() != policy_id {
            return Err(AsterError::validation_error(format!(
                "preuploaded blob policy {} does not match verified store policy {policy_id}",
                prepared.policy_id(),
            )));
        }

        Ok(Self {
            size,
            policy_id,
            storage_path: prepared.storage_path().to_string(),
            prepared,
        })
    }

    fn size(&self) -> i64 {
        self.size
    }

    fn policy_id(&self) -> i64 {
        self.policy_id
    }

    fn storage_path(&self) -> &str {
        &self.storage_path
    }

    fn prepared(&self) -> &PreparedNonDedupBlobUpload {
        &self.prepared
    }
}

async fn cleanup_verified_preuploaded_nondedup_store_blob(
    driver: &dyn crate::storage::StorageDriver,
    verified_blob: &VerifiedPreuploadedNondedupStoreBlob,
    reason: &str,
) {
    cleanup_preuploaded_blob_upload(driver, verified_blob.prepared(), reason).await;
}

#[cfg(test)]
mod preuploaded_contract_tests {
    use super::{
        PreparedNonDedupBlobUpload, VerifiedPreuploadedNondedupStoreBlob,
        cleanup_verified_preuploaded_nondedup_store_blob,
    };
    use crate::errors::Result;
    use crate::storage::{BlobMetadata, StorageDriver};
    use async_trait::async_trait;
    use std::sync::Mutex;
    use tokio::io::AsyncRead;

    #[derive(Default)]
    struct RecordingDeleteDriver {
        deleted_paths: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl StorageDriver for RecordingDeleteDriver {
        async fn put(&self, _path: &str, _data: &[u8]) -> Result<String> {
            unreachable!()
        }

        async fn get(&self, _path: &str) -> Result<Vec<u8>> {
            unreachable!()
        }

        async fn get_stream(&self, _path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
            unreachable!()
        }

        async fn delete(&self, path: &str) -> Result<()> {
            self.deleted_paths
                .lock()
                .expect("deleted paths lock should not be poisoned")
                .push(path.to_string());
            Ok(())
        }

        async fn exists(&self, _path: &str) -> Result<bool> {
            unreachable!()
        }

        async fn metadata(&self, _path: &str) -> Result<BlobMetadata> {
            unreachable!()
        }

        async fn copy_object(&self, _src_path: &str, _dest_path: &str) -> Result<String> {
            unreachable!()
        }
    }

    fn opaque_preupload(size: i64, policy_id: i64) -> PreparedNonDedupBlobUpload {
        PreparedNonDedupBlobUpload::Opaque {
            upload_id: "opaque-id".to_string(),
            hash_prefix: "s3",
            storage_path: "files/opaque-id".to_string(),
            size,
            policy_id,
        }
    }

    #[test]
    fn verified_preuploaded_store_blob_carries_size_policy_and_storage_path() {
        let verified = VerifiedPreuploadedNondedupStoreBlob::new(33, 9, opaque_preupload(33, 9))
            .expect("verified preupload should be accepted");

        assert_eq!(verified.size(), 33);
        assert_eq!(verified.policy_id(), 9);
        assert_eq!(verified.storage_path(), "files/opaque-id");
        assert_eq!(verified.prepared().storage_path(), "files/opaque-id");
    }

    #[test]
    fn verified_preuploaded_store_blob_rejects_invalid_contract() {
        let negative = VerifiedPreuploadedNondedupStoreBlob::new(-1, 9, opaque_preupload(33, 9))
            .expect_err("negative size should be rejected");
        assert!(negative.to_string().contains("non-negative"));

        let size_error = VerifiedPreuploadedNondedupStoreBlob::new(34, 9, opaque_preupload(33, 9))
            .expect_err("size mismatch should be rejected");
        assert!(size_error.to_string().contains("size"));

        let policy_error =
            VerifiedPreuploadedNondedupStoreBlob::new(33, 10, opaque_preupload(33, 9))
                .expect_err("policy mismatch should be rejected");
        assert!(policy_error.to_string().contains("policy"));
    }

    #[tokio::test]
    async fn cleanup_verified_preuploaded_store_blob_deletes_opaque_object() {
        let verified = VerifiedPreuploadedNondedupStoreBlob::new(33, 9, opaque_preupload(33, 9))
            .expect("verified preupload should be accepted");
        let driver = RecordingDeleteDriver::default();

        cleanup_verified_preuploaded_nondedup_store_blob(&driver, &verified, "test cleanup").await;

        let deleted_paths = driver
            .deleted_paths
            .lock()
            .expect("deleted paths lock should not be poisoned")
            .clone();
        assert_eq!(deleted_paths, vec!["files/opaque-id"]);
    }
}

pub(crate) async fn store_from_temp_with_hints(
    state: &PrimaryAppState,
    params: StoreFromTempParams<'_>,
    hints: StoreFromTempHints<'_>,
) -> Result<file::Model> {
    from_temp::store_from_temp_internal(state, params, hints, NewFileMode::ResolveUnique, true)
        .await
}

pub(crate) async fn store_from_temp_exact_name_with_hints(
    state: &PrimaryAppState,
    params: StoreFromTempParams<'_>,
    hints: StoreFromTempHints<'_>,
) -> Result<file::Model> {
    from_temp::store_from_temp_internal(state, params, hints, NewFileMode::Exact, true).await
}

pub(crate) async fn store_from_temp_exact_name_silent_with_hints(
    state: &PrimaryAppState,
    params: StoreFromTempParams<'_>,
    hints: StoreFromTempHints<'_>,
) -> Result<file::Model> {
    from_temp::store_from_temp_internal(state, params, hints, NewFileMode::Exact, false).await
}

pub(crate) async fn create_empty(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_id: Option<i64>,
    filename: &str,
) -> Result<file::Model> {
    tracing::debug!(
        scope = ?scope,
        folder_id,
        filename = %filename,
        "creating empty file"
    );

    if let Some(folder_id) = folder_id {
        verify_folder_access(state, scope, folder_id).await?;
    }
    let filename = crate::utils::normalize_validate_name(filename)?;

    const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const EMPTY_SIZE: i64 = 0;

    let policy = resolve_policy_for_size(state, scope, folder_id, EMPTY_SIZE).await?;
    let driver = state.driver_registry().get_driver(&policy)?;
    let should_dedup = local_content_dedup_enabled(&policy);
    let now = Utc::now();

    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    let blob = if should_dedup {
        let storage_path = crate::utils::storage_path_from_blob_key(EMPTY_SHA256);
        let blob = file_repo::find_or_create_blob(
            &txn,
            EMPTY_SHA256,
            EMPTY_SIZE,
            policy.id,
            &storage_path,
        )
        .await?;
        if blob.inserted {
            driver.put(&storage_path, &[]).await?;
        }
        blob.model
    } else {
        let prepared = prepare_non_dedup_blob_upload(&policy, EMPTY_SIZE)?;
        let blob = persist_preuploaded_blob(&txn, &prepared).await?;
        driver.put(&blob.storage_path, &[]).await?;
        blob
    };

    let created = create_new_file_from_blob(&txn, scope, folder_id, &filename, &blob, now).await?;
    crate::db::transaction::commit(txn).await?;
    storage_change::publish(
        state,
        storage_change::StorageChangeEvent::new(
            storage_change::StorageChangeKind::FileCreated,
            scope,
            vec![created.id],
            vec![],
            vec![created.folder_id],
        )
        .with_storage_delta(EMPTY_SIZE),
    );
    tracing::debug!(
        scope = ?scope,
        file_id = created.id,
        blob_id = created.blob_id,
        folder_id = created.folder_id,
        "created empty file"
    );
    Ok(created)
}

pub(crate) async fn store_preuploaded_nondedup(
    state: &PrimaryAppState,
    params: StorePreuploadedNondedupParams<'_>,
) -> Result<file::Model> {
    let StorePreuploadedNondedupParams {
        scope,
        folder_id,
        filename,
        size,
        existing_file_id,
        skip_lock_check,
        policy,
        preuploaded_blob,
        actor_username,
    } = params;
    let db = state.writer_db();

    tracing::debug!(
        scope = ?scope,
        folder_id,
        filename = %filename,
        size,
        existing_file_id,
        skip_lock_check,
        policy_id = policy.id,
        "storing file from preuploaded blob"
    );

    let filename = crate::utils::normalize_validate_name(filename)?;

    let driver = state.driver_registry().get_driver(policy)?;
    let verified_blob = match VerifiedPreuploadedNondedupStoreBlob::new(
        size,
        policy.id,
        preuploaded_blob.clone(),
    ) {
        Ok(verified_blob) => verified_blob,
        Err(error) => {
            cleanup_preuploaded_blob_upload(
                driver.as_ref(),
                &preuploaded_blob,
                "preuploaded contract validation failure",
            )
            .await;
            return Err(error);
        }
    };

    if policy.max_file_size > 0 && verified_blob.size() > policy.max_file_size {
        cleanup_verified_preuploaded_nondedup_store_blob(
            driver.as_ref(),
            &verified_blob,
            "size validation failure",
        )
        .await;
        return Err(AsterError::file_too_large(format!(
            "file size {} exceeds limit {}",
            verified_blob.size(),
            policy.max_file_size
        )));
    }

    let now = Utc::now();

    let overwrite_ctx = if let Some(existing_id) = existing_file_id {
        let old_file = verify_file_access(state, scope, existing_id).await?;
        if old_file.is_locked && !skip_lock_check {
            cleanup_verified_preuploaded_nondedup_store_blob(
                driver.as_ref(),
                &verified_blob,
                "lock check failure",
            )
            .await;
            return Err(AsterError::resource_locked("file is locked"));
        }
        let old_blob = file_repo::find_blob_by_id(db, old_file.blob_id).await?;
        if let Err(err) =
            crate::services::media::processing::delete_thumbnail(state, &old_blob).await
        {
            tracing::warn!("failed to delete thumbnail for blob {}: {err}", old_blob.id);
        }
        Some((old_file, old_blob))
    } else {
        None
    };
    let storage_delta = overwrite_ctx
        .as_ref()
        .map_or(verified_blob.size(), |_| verified_blob.size());

    let mime = mime_guess::from_path(&filename)
        .first_or_octet_stream()
        .to_string();

    let create_result = async {
        let txn = crate::db::transaction::begin(state.writer_db()).await?;
        if storage_delta > 0 {
            check_quota(&txn, scope, storage_delta).await?;
        }

        let blob = persist_preuploaded_blob(&txn, verified_blob.prepared()).await?;
        debug_assert_eq!(blob.size, verified_blob.size());
        debug_assert_eq!(blob.policy_id, verified_blob.policy_id());
        debug_assert_eq!(blob.storage_path, verified_blob.storage_path());

        let result = if let Some((old_file, old_blob)) = overwrite_ctx {
            let current_file =
                revalidate_preuploaded_overwrite_target(&txn, scope, &old_file, skip_lock_check)
                    .await?;
            let existing_id = current_file.id;
            let current_name = current_file.name.clone();
            let mut active: file::ActiveModel = current_file.into();
            active.blob_id = Set(blob.id);
            active.size = Set(blob.size);
            let classification =
                crate::utils::file_classification::classify_file(&current_name, &mime);
            active.mime_type = Set(mime);
            active.extension = Set(classification.extension);
            active.compound_extension = Set(classification.compound_extension);
            active.file_category = Set(classification.category);
            active.updated_at = Set(now);
            let updated = active
                .update(&txn)
                .await
                .map_aster_err(AsterError::database_operation)?;

            let next_ver =
                crate::db::repository::version_repo::next_version(&txn, existing_id).await?;
            crate::db::repository::version_repo::create(
                &txn,
                crate::entities::file_version::ActiveModel {
                    file_id: Set(existing_id),
                    blob_id: Set(old_blob.id),
                    version: Set(next_ver),
                    size: Set(old_blob.size),
                    created_at: Set(now),
                    ..Default::default()
                },
            )
            .await?;

            if storage_delta != 0 {
                update_storage_used(&txn, scope, storage_delta).await?;
            }
            updated
        } else {
            let created = match actor_username {
                Some(username) => {
                    create_new_file_from_blob_with_actor_username(
                        &txn, scope, folder_id, &filename, &blob, now, username,
                    )
                    .await?
                }
                None => {
                    create_new_file_from_blob(&txn, scope, folder_id, &filename, &blob, now).await?
                }
            };
            if storage_delta != 0 {
                update_storage_used(&txn, scope, storage_delta).await?;
            }
            created
        };

        crate::db::transaction::commit(txn).await?;
        Ok::<file::Model, AsterError>(result)
    }
    .await;

    let result = match create_result {
        Ok(result) => result,
        Err(error) => {
            cleanup_verified_preuploaded_nondedup_store_blob(
                driver.as_ref(),
                &verified_blob,
                "DB error after direct upload",
            )
            .await;
            return Err(error);
        }
    };

    let event_kind = if existing_file_id.is_some() {
        storage_change::StorageChangeKind::FileUpdated
    } else {
        storage_change::StorageChangeKind::FileCreated
    };
    storage_change::publish(
        state,
        storage_change::StorageChangeEvent::new(
            event_kind,
            scope,
            vec![result.id],
            vec![],
            vec![result.folder_id],
        )
        .with_storage_delta(storage_delta),
    );

    if let Some(existing_id) = existing_file_id {
        crate::services::content::version::cleanup_excess(state, existing_id).await?;
    }

    tracing::debug!(
        scope = ?scope,
        file_id = result.id,
        blob_id = result.blob_id,
        folder_id = result.folder_id,
        overwritten = existing_file_id.is_some(),
        size = result.size,
        "stored file from preuploaded blob"
    );

    Ok(result)
}

async fn revalidate_preuploaded_overwrite_target<C: sea_orm::ConnectionTrait>(
    txn: &C,
    scope: WorkspaceStorageScope,
    old_file: &file::Model,
    skip_lock_check: bool,
) -> Result<file::Model> {
    let current_file = file_repo::lock_by_id(txn, old_file.id).await?;
    super::ensure_active_file_scope(&current_file, scope)?;

    if current_file.blob_id != old_file.blob_id {
        return Err(precondition_failed_with_code(
            ApiErrorCode::FileModifiedDuringWrite,
            "file changed while upload body was being received",
        ));
    }

    if current_file.is_locked {
        if !skip_lock_check {
            return Err(AsterError::resource_locked("file is locked"));
        }

        let lock = crate::db::repository::lock_repo::find_by_entity(
            txn,
            crate::types::EntityType::File,
            current_file.id,
        )
        .await?;
        if let Some(lock) = lock
            && lock.owner_id != Some(scope.actor_user_id())
        {
            return Err(AsterError::resource_locked(
                "file is locked by another user",
            ));
        }
    }

    Ok(current_file)
}
