//! 工作空间存储服务子模块：`store`。

mod empty;
pub(crate) mod from_temp;
mod preuploaded_contract;

use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, DbBackend, Set};

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{file_create_idempotency_repo, file_repo};
use crate::errors::{AsterError, Result, precondition_failed_with_code};
use crate::runtime::PrimaryAppState;
use crate::services::events::storage_change;
use aster_drive_model::entities::file;

use super::{
    NewFileMode, ParsedUploadPath, PreparedNonDedupBlobUpload, WorkspaceStorageScope, check_quota,
    cleanup_preuploaded_blob_upload, create_new_file_from_blob,
    create_new_file_from_blob_with_actor_username, ensure_upload_parent_path_on,
    lock_folder_access_on, lock_storage_usage, persist_preuploaded_blob,
    resolve_policy_for_size_with_verified_folder_on, resolve_verified_folder_policy_hint_on,
    update_storage_used, verify_file_access,
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
pub(crate) use empty::{EmptyFileNameMode, PreparedEmptyFile, publish_empty_file_created};
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
}

#[derive(Clone, Default)]
pub(crate) struct StoreFromTempHints<'a> {
    pub resolved_policy: Option<aster_drive_model::entities::storage_policy::Model>,
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
    name_mode: EmptyFileNameMode,
) -> Result<file::Model> {
    Ok(
        create_empty_with_idempotency(state, scope, folder_id, filename, name_mode, None)
            .await?
            .file,
    )
}

#[derive(Debug)]
pub(crate) struct EmptyFileCreateResult {
    pub file: file::Model,
    pub replayed: bool,
}

pub(crate) async fn create_empty_with_idempotency(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_id: Option<i64>,
    filename: &str,
    name_mode: EmptyFileNameMode,
    idempotency: Option<(&str, &str)>,
) -> Result<EmptyFileCreateResult> {
    create_empty_transactional(
        state,
        scope,
        folder_id,
        filename,
        name_mode,
        idempotency,
        None,
    )
    .await
}

pub(crate) async fn create_empty_from_relative_path_with_idempotency(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    parsed: ParsedUploadPath,
    actor_username: Option<String>,
    idempotency: Option<(&str, &str)>,
) -> Result<EmptyFileCreateResult> {
    let base_folder_id = parsed.base_folder_id;
    let filename = parsed.filename.clone();
    create_empty_transactional(
        state,
        scope,
        base_folder_id,
        &filename,
        EmptyFileNameMode::Exact,
        idempotency,
        Some((parsed, actor_username)),
    )
    .await
}

async fn create_empty_transactional(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_id: Option<i64>,
    filename: &str,
    name_mode: EmptyFileNameMode,
    idempotency: Option<(&str, &str)>,
    relative_parent: Option<(ParsedUploadPath, Option<String>)>,
) -> Result<EmptyFileCreateResult> {
    tracing::debug!(
        scope = ?scope,
        folder_id,
        filename = %filename,
        "creating empty file"
    );

    let filename = aster_forge_validation::filename::normalize_validate_name(filename)?;
    let workspace = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            crate::services::files::lock::LockWorkspace::Personal { user_id }
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            crate::services::files::lock::LockWorkspace::Team { team_id }
        }
    };
    let transaction_filename = filename.clone();
    let transaction_idempotency =
        idempotency.map(|(key_hash, fingerprint)| (key_hash.to_owned(), fingerprint.to_owned()));
    let result =
        aster_forge_db::transaction::with_transaction(state.writer_db(), async move |txn| {
            crate::services::files::lock::lock_workspace_for_mutation_on(txn, workspace).await?;
            lock_storage_usage(txn, scope).await?;
            let idempotency_scope = match scope {
                WorkspaceStorageScope::Personal { user_id } => {
                    file_create_idempotency_repo::FileCreateIdempotencyScope {
                        actor_user_id: user_id,
                        workspace_kind: "personal",
                        workspace_id: user_id,
                    }
                }
                WorkspaceStorageScope::Team {
                    team_id,
                    actor_user_id,
                } => file_create_idempotency_repo::FileCreateIdempotencyScope {
                    actor_user_id,
                    workspace_kind: "team",
                    workspace_id: team_id,
                },
            };
            let mut idempotency_claim = None;
            if let Some((key_hash, fingerprint)) = &transaction_idempotency {
                if let Some(existing) = file_create_idempotency_repo::find(
                    txn,
                    idempotency_scope,
                    key_hash,
                )
                .await?
                {
                    if existing.expires_at <= Utc::now() {
                        file_create_idempotency_repo::delete(txn, existing.id).await?;
                    } else {
                        if existing.request_fingerprint != *fingerprint {
                            return Err(AsterError::conflict(
                                "idempotency key was already used with a different create-empty request",
                            ));
                        }
                        let result_file_id = existing.result_file_id.ok_or_else(|| {
                            AsterError::conflict(
                                "idempotency result file was purged during its retention window",
                            )
                        })?;
                        let file = file_repo::find_by_id(txn, result_file_id)
                            .await
                            .map_err(|_| {
                                AsterError::conflict(
                                    "idempotency result file was purged during its retention window",
                                )
                            })?;
                        return Ok(EmptyFileCreateResult {
                            file,
                            replayed: true,
                        });
                    }
                }
                let now = Utc::now();
                idempotency_claim = Some(
                    file_create_idempotency_repo::create_claim(
                        txn,
                        idempotency_scope,
                        key_hash,
                        fingerprint,
                        now,
                        now + Duration::hours(24),
                    )
                    .await?,
                );
            }
            let base_folder = match folder_id {
                Some(folder_id) => {
                    let folder = lock_folder_access_on(txn, state, scope, folder_id).await?;
                    Some(resolve_verified_folder_policy_hint_on(txn, scope, folder).await?)
                }
                None => None,
            };
            crate::services::files::lock::enforce_collection_membership_mutation_on(
                txn,
                workspace,
                folder_id,
                &crate::services::files::lock::SubmittedLockCredentials::none(),
            )
            .await?;
            let (resolved_folder_id, resolved_folder) =
                if let Some((parsed, actor_username)) = &relative_parent {
                    let transaction_parsed = ParsedUploadPath {
                        base_folder_id: parsed.base_folder_id,
                        base_folder,
                        parent_segments: parsed.parent_segments.clone(),
                        filename: parsed.filename.clone(),
                    };
                    let parent = ensure_upload_parent_path_on(
                        state,
                        txn,
                        scope,
                        &transaction_parsed,
                        actor_username.as_deref(),
                    )
                    .await?;
                    if parent.folder_id != folder_id {
                        crate::services::files::lock::enforce_collection_membership_mutation_on(
                            txn,
                            workspace,
                            parent.folder_id,
                            &crate::services::files::lock::SubmittedLockCredentials::none(),
                        )
                        .await?;
                    }
                    (parent.folder_id, parent.folder)
                } else {
                    (folder_id, base_folder)
                };
            let policy = resolve_policy_for_size_with_verified_folder_on(
                state,
                txn,
                scope,
                resolved_folder,
                0,
            )
            .await?;
            let transaction_prepared = PreparedEmptyFile::with_resolved_policy(
                scope,
                resolved_folder_id,
                &transaction_filename,
                name_mode,
                policy.id,
            )?;
            let blob = transaction_prepared.persist_blob_on(txn).await?;
            let file = transaction_prepared.create_file_on(txn, &blob).await?;
            if let Some(claim) = idempotency_claim {
                file_create_idempotency_repo::complete(txn, claim, file.id).await?;
            }
            Ok(EmptyFileCreateResult { file, replayed: false })
        })
        .await;
    let created = match result {
        Ok(created) => created,
        Err(error) => return Err(error),
    };
    if !created.replayed {
        publish_empty_file_created(state, scope, &created.file);
    }
    tracing::debug!(
        scope = ?scope,
        file_id = created.file.id,
        blob_id = created.file.blob_id,
        folder_id = created.file.folder_id,
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
                debug_assert_eq!(
                    blob.storage_path.as_deref(),
                    Some(verified_blob.storage_path())
                );

                let result = if let Some((old_file, old_blob)) = overwrite_ctx {
                    let current_file = revalidate_preuploaded_overwrite_target(
                        txn,
                        scope,
                        &old_file,
                        &lock_credentials,
                    )
                    .await?;
                    let existing_id = current_file.id;
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

                    let next_ver =
                        crate::db::repository::version_repo::next_version(txn, existing_id).await?;
                    crate::db::repository::version_repo::create(
                        txn,
                        aster_drive_model::entities::file_version::ActiveModel {
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
