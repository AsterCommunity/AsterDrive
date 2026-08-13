//! 工作空间存储服务子模块：`store`。

mod empty;
pub(crate) mod from_temp;
mod preuploaded_contract;

use chrono::Utc;
use sea_orm::{ActiveModelTrait, DbBackend, Set};

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::file_repo;
use crate::errors::{AsterError, Result, precondition_failed_with_code};
use crate::runtime::PrimaryAppState;
use crate::services::events::storage_change;
use aster_drive_model::entities::file;

use super::{
    NewFileMode, PreparedNonDedupBlobUpload, WorkspaceStorageScope, check_quota,
    cleanup_preuploaded_blob_upload, create_new_file_from_blob,
    create_new_file_from_blob_with_actor_username, lock_storage_usage, persist_preuploaded_blob,
    update_storage_used, verify_file_access, verify_folder_access,
};

#[derive(Clone, Copy)]
pub(crate) enum FileWritePrecondition {
    Missing,
    Existing(FileWriteSnapshot),
}

#[derive(Clone, Copy)]
pub(crate) struct FileWriteSnapshot {
    id: i64,
    blob_id: i64,
    size: i64,
    updated_at: chrono::DateTime<Utc>,
}

impl FileWritePrecondition {
    pub(crate) fn existing(file: &file::Model) -> Self {
        Self::Existing(FileWriteSnapshot {
            id: file.id,
            blob_id: file.blob_id,
            size: file.size,
            updated_at: file.updated_at,
        })
    }
}
pub(crate) use empty::{EmptyFileNameMode, PreparedEmptyFile};
use preuploaded_contract::{
    VerifiedPreuploadedNondedupStoreBlob, cleanup_verified_preuploaded_nondedup_store_blob,
};

#[derive(Clone)]
pub(crate) struct StoreFromTempParams<'a> {
    pub scope: WorkspaceStorageScope,
    pub folder_id: Option<i64>,
    pub filename: &'a str,
    pub temp_path: &'a str,
    pub size: i64,
    pub existing_file_id: Option<i64>,
    pub lock_credentials: crate::services::files::lock::LockMutationCredentials,
    pub file_precondition: Option<FileWritePrecondition>,
    pub expected_current_revision_id: Option<i64>,
    pub expected_current_revision_etag: Option<String>,
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
            lock_credentials: crate::services::files::lock::LockMutationCredentials::None,
            file_precondition: None,
            expected_current_revision_id: None,
            expected_current_revision_etag: None,
        }
    }

    pub(crate) fn overwrite(mut self, existing_file_id: i64) -> Self {
        self.existing_file_id = Some(existing_file_id);
        self
    }

    pub(crate) fn with_lock_credentials(
        mut self,
        credentials: crate::services::files::lock::LockMutationCredentials,
    ) -> Self {
        self.lock_credentials = credentials;
        self
    }

    pub(crate) fn with_expected_revision(mut self, revision_id: Option<i64>) -> Self {
        self.expected_current_revision_id = revision_id;
        self
    }

    pub(crate) fn with_expected_revision_etag(mut self, etag: Option<&str>) -> Self {
        self.expected_current_revision_etag = etag.map(ToOwned::to_owned);
        self
    }
}

#[derive(Clone, Default)]
pub(crate) struct StoreFromTempHints<'a> {
    pub resolved_policy: Option<aster_drive_model::entities::storage_policy::Model>,
    pub precomputed_hash: Option<&'a str>,
    pub actor_username: Option<&'a str>,
    pub operation_context: crate::services::workspace::storage::StorageOperationContext,
    pub revision_etag: Option<&'a str>,
}

pub(crate) struct StorePreuploadedNondedupParams<'a> {
    pub scope: WorkspaceStorageScope,
    pub folder_id: Option<i64>,
    pub filename: &'a str,
    pub size: i64,
    pub existing_file_id: Option<i64>,
    pub lock_credentials: crate::services::files::lock::LockMutationCredentials,
    pub policy: &'a aster_drive_model::entities::storage_policy::Model,
    pub preuploaded_blob: PreparedNonDedupBlobUpload,
    pub actor_username: Option<&'a str>,
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
    let prepared = PreparedEmptyFile::prepare(
        state,
        scope,
        folder_id,
        filename,
        EmptyFileNameMode::ResolveUnique,
    )
    .await?;
    let workspace = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            crate::services::files::lock::LockWorkspace::Personal { user_id }
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            crate::services::files::lock::LockWorkspace::Team { team_id }
        }
    };
    let transaction_prepared = prepared.clone();
    let result =
        aster_forge_db::transaction::with_transaction(state.writer_db(), async move |txn| {
            crate::services::files::lock::lock_workspace_for_mutation_on(txn, workspace).await?;
            lock_storage_usage(txn, scope).await?;
            crate::services::files::lock::enforce_collection_membership_mutation_on(
                txn,
                workspace,
                folder_id,
                &crate::services::files::lock::SubmittedLockCredentials::none(),
            )
            .await?;
            let blob = transaction_prepared.persist_blob_on(txn).await?;
            transaction_prepared.create_file_on(txn, &blob).await
        })
        .await;
    let created = match result {
        Ok(created) => created,
        Err(error) => {
            if !error.database_commit_outcome_uncertain() {
                prepared
                    .cleanup_after_db_failure("empty file DB error")
                    .await;
            }
            return Err(error);
        }
    };
    prepared.publish_created(state, &created);
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
        lock_credentials,
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
        lock_credentials = ?lock_credentials,
        policy_id = policy.id,
        "storing file from preuploaded blob"
    );

    let filename = aster_forge_validation::filename::normalize_validate_name(filename)?;

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
        let submitted = lock_credentials.submitted();
        if let Err(error) =
            crate::services::files::lock::enforce_file_mutation(db, &old_file, &submitted).await
        {
            cleanup_verified_preuploaded_nondedup_store_blob(
                driver.as_ref(),
                &verified_blob,
                "lock check failure",
            )
            .await;
            return Err(error);
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

    let retry_on_mysql_deadlock = state.writer_db().get_database_backend() == DbBackend::MySql;
    let transaction_verified_blob = verified_blob.clone();
    let transaction_overwrite_ctx = overwrite_ctx.clone();
    let transaction_mime = mime.clone();
    let transaction_filename = filename.clone();
    let transaction_actor_username = actor_username.map(str::to_owned);
    let transaction_lock_credentials = lock_credentials.clone();
    let transaction_now = now;
    let create_result = aster_forge_db::transaction::with_transaction_retry(
        state.writer_db(),
        &aster_forge_db::retry::RetryConfig::deadlock(),
        move |txn| {
            let verified_blob = transaction_verified_blob.clone();
            let overwrite_ctx = transaction_overwrite_ctx.clone();
            let mime = transaction_mime.clone();
            let filename = transaction_filename.clone();
            let actor_username = transaction_actor_username.clone();
            let lock_credentials = transaction_lock_credentials.clone();
            let now = transaction_now;
            Box::pin(async move {
                let workspace = match scope {
                    WorkspaceStorageScope::Personal { user_id } => {
                        crate::services::files::lock::LockWorkspace::Personal { user_id }
                    }
                    WorkspaceStorageScope::Team { team_id, .. } => {
                        crate::services::files::lock::LockWorkspace::Team { team_id }
                    }
                };
                crate::services::files::lock::lock_workspace_for_mutation_on(txn, workspace)
                    .await?;
                lock_storage_usage(txn, scope).await?;
                if storage_delta > 0 {
                    check_quota(txn, scope, storage_delta).await?;
                }

                let blob = persist_preuploaded_blob(txn, verified_blob.prepared()).await?;
                debug_assert_eq!(blob.size, verified_blob.size());
                debug_assert_eq!(blob.policy_id, verified_blob.policy_id());
                debug_assert_eq!(blob.storage_path, verified_blob.storage_path());

                let result = if let Some((old_file, _old_blob)) = overwrite_ctx {
                    let current_file = revalidate_preuploaded_overwrite_target(
                        txn,
                        scope,
                        &old_file,
                        &lock_credentials,
                    )
                    .await?;
                    let existing_id = current_file.id;
                    let expected_revision_id =
                        crate::db::repository::revision_repo::lock_history_by_file_id(
                            txn,
                            existing_id,
                        )
                        .await?
                        .current_revision_id;
                    let current_name = current_file.name.clone();
                    let mut active: file::ActiveModel = current_file.into();
                    active.blob_id = Set(blob.id);
                    active.size = Set(blob.size);
                    let classification =
                        aster_forge_file_classification::classify_file(&current_name, &mime);
                    active.mime_type = Set(mime.clone());
                    active.extension = Set(classification.extension);
                    active.compound_extension = Set(classification.compound_extension);
                    active.file_category = Set(classification.category);
                    active.updated_at = Set(now);
                    let updated = active.update(txn).await.map_err(AsterError::from)?;

                    let actor_username = match actor_username {
                        Some(username) => username,
                        None => {
                            crate::services::workspace::storage::load_scope_actor_username(
                                txn, scope,
                            )
                            .await?
                        }
                    };
                    crate::db::repository::revision_repo::append(
                        txn,
                        existing_id,
                        expected_revision_id,
                        crate::db::repository::revision_repo::NewRevision {
                            blob_id: blob.id,
                            logical_size: blob.size,
                            mime_type: &mime,
                            content_sha256: None,
                            creator_user_id: Some(scope.actor_user_id()),
                            creator_display_name: &actor_username,
                            comment: None,
                            reason: crate::db::repository::revision_repo::RevisionReason::Overwrite,
                            created_at: now,
                            etag: None,
                        },
                    )
                    .await
                    .map_err(|error| match error {
                        crate::db::repository::revision_repo::RevisionAppendError::HeadChanged => {
                            precondition_failed_with_code(
                                ApiErrorCode::FileModifiedDuringWrite,
                                "file revision head changed while content was being committed",
                            )
                        }
                        crate::db::repository::revision_repo::RevisionAppendError::EtagMismatch => {
                            precondition_failed_with_code(
                                ApiErrorCode::FileEtagMismatch,
                                "file has been modified (ETag mismatch)",
                            )
                        }
                        crate::db::repository::revision_repo::RevisionAppendError::Repository(
                            error,
                        ) => error,
                    })?;

                    if storage_delta != 0 {
                        update_storage_used(txn, scope, storage_delta).await?;
                    }
                    updated
                } else {
                    let submitted = lock_credentials.submitted();
                    crate::services::files::lock::enforce_collection_membership_mutation_on(
                        txn, workspace, folder_id, &submitted,
                    )
                    .await?;
                    let created = match actor_username.as_deref() {
                        Some(username) => {
                            create_new_file_from_blob_with_actor_username(
                                txn, scope, folder_id, &filename, &blob, now, username,
                            )
                            .await?
                        }
                        None => {
                            create_new_file_from_blob(txn, scope, folder_id, &filename, &blob, now)
                                .await?
                        }
                    };
                    if storage_delta != 0 {
                        update_storage_used(txn, scope, storage_delta).await?;
                    }
                    created
                };

                Ok::<file::Model, AsterError>(result)
            })
        },
        move |error: &AsterError| {
            retry_on_mysql_deadlock
                && error.database_error_kind() == Some(aster_forge_db::DatabaseErrorKind::Deadlock)
        },
    )
    .await;

    let result = match create_result {
        Ok(result) => result,
        Err(error) => {
            if !error.database_commit_outcome_uncertain() {
                cleanup_verified_preuploaded_nondedup_store_blob(
                    driver.as_ref(),
                    &verified_blob,
                    "DB error after direct upload",
                )
                .await;
            }
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
    lock_credentials: &crate::services::files::lock::LockMutationCredentials,
) -> Result<file::Model> {
    let credentials = lock_credentials.submitted();
    let current_file =
        crate::services::files::lock::enforce_file_mutation_on(txn, old_file, &credentials).await?;
    super::ensure_active_file_scope(&current_file, scope)?;

    if current_file.blob_id != old_file.blob_id {
        return Err(precondition_failed_with_code(
            ApiErrorCode::FileModifiedDuringWrite,
            "file changed while upload body was being received",
        ));
    }

    Ok(current_file)
}
