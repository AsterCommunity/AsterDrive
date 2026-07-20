//! 工作空间存储服务子模块：`store`。

pub(crate) mod from_temp;
mod preuploaded_contract;

use chrono::Utc;
use sea_orm::{ActiveModelTrait, DbBackend, Set};

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::file_repo;
use crate::entities::file;
use crate::errors::{AsterError, Result, precondition_failed_with_code};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::events::storage_change;

use super::{
    NewFileMode, PreparedNonDedupBlobUpload, WorkspaceStorageScope, check_quota,
    cleanup_preuploaded_blob_upload, create_new_file_from_blob,
    create_new_file_from_blob_with_actor_username, local_content_dedup_enabled, lock_storage_usage,
    persist_preuploaded_blob, prepare_non_dedup_blob_upload, resolve_policy_for_size,
    update_storage_used, verify_file_access, verify_folder_access,
};
use preuploaded_contract::{
    VerifiedPreuploadedNondedupStoreBlob, cleanup_verified_preuploaded_nondedup_store_blob,
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
    let filename = aster_forge_validation::filename::normalize_validate_name(filename)?;

    const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const EMPTY_SIZE: i64 = 0;

    let policy = resolve_policy_for_size(state, scope, folder_id, EMPTY_SIZE).await?;
    let driver = state.driver_registry().get_driver(&policy)?;
    let should_dedup = local_content_dedup_enabled(&policy);
    let now = Utc::now();

    let created = if should_dedup {
        let storage_path =
            aster_forge_validation::filename::storage_path_from_blob_key(EMPTY_SHA256)?;
        if !driver.exists(&storage_path).await? {
            driver.put(&storage_path, &[]).await?;
        }
        let retry_on_mysql_deadlock = state.writer_db().get_database_backend() == DbBackend::MySql;
        let transaction_storage_path = storage_path.clone();
        let transaction_filename = filename.clone();
        let transaction_now = now;
        let policy_id = policy.id;
        aster_forge_db::transaction::with_transaction_retry(
            state.writer_db(),
            &aster_forge_db::retry::RetryConfig::deadlock(),
            move |txn| {
                let storage_path = transaction_storage_path.clone();
                let filename = transaction_filename.clone();
                let now = transaction_now;
                Box::pin(async move {
                    lock_storage_usage(txn, scope).await?;
                    let blob = file_repo::find_or_create_blob(
                        txn,
                        EMPTY_SHA256,
                        EMPTY_SIZE,
                        policy_id,
                        &storage_path,
                    )
                    .await?;
                    create_new_file_from_blob(txn, scope, folder_id, &filename, &blob.model, now)
                        .await
                })
            },
            move |error: &AsterError| {
                retry_on_mysql_deadlock
                    && error.database_error_kind()
                        == Some(aster_forge_db::DatabaseErrorKind::Deadlock)
            },
        )
        .await?
    } else {
        let prepared = prepare_non_dedup_blob_upload(&policy, EMPTY_SIZE, Some(&filename))?;
        driver.put(prepared.storage_path(), &[]).await?;
        let retry_on_mysql_deadlock = state.writer_db().get_database_backend() == DbBackend::MySql;
        let transaction_prepared = prepared.clone();
        let transaction_filename = filename.clone();
        let transaction_now = now;
        let result = aster_forge_db::transaction::with_transaction_retry(
            state.writer_db(),
            &aster_forge_db::retry::RetryConfig::deadlock(),
            move |txn| {
                let prepared = transaction_prepared.clone();
                let filename = transaction_filename.clone();
                let now = transaction_now;
                Box::pin(async move {
                    lock_storage_usage(txn, scope).await?;
                    let blob = persist_preuploaded_blob(txn, &prepared).await?;
                    create_new_file_from_blob(txn, scope, folder_id, &filename, &blob, now).await
                })
            },
            move |error: &AsterError| {
                retry_on_mysql_deadlock
                    && error.database_error_kind()
                        == Some(aster_forge_db::DatabaseErrorKind::Deadlock)
            },
        )
        .await;
        match result {
            Ok(file) => file,
            Err(error) => {
                if !error.database_commit_outcome_uncertain() {
                    cleanup_preuploaded_blob_upload(
                        driver.as_ref(),
                        &prepared,
                        "empty file DB error",
                    )
                    .await;
                }
                return Err(error);
            }
        }
    };
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

    let retry_on_mysql_deadlock = state.writer_db().get_database_backend() == DbBackend::MySql;
    let transaction_verified_blob = verified_blob.clone();
    let transaction_overwrite_ctx = overwrite_ctx.clone();
    let transaction_mime = mime.clone();
    let transaction_filename = filename.clone();
    let transaction_actor_username = actor_username.map(str::to_owned);
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
            let now = transaction_now;
            Box::pin(async move {
                lock_storage_usage(txn, scope).await?;
                if storage_delta > 0 {
                    check_quota(txn, scope, storage_delta).await?;
                }

                let blob = persist_preuploaded_blob(txn, verified_blob.prepared()).await?;
                debug_assert_eq!(blob.size, verified_blob.size());
                debug_assert_eq!(blob.policy_id, verified_blob.policy_id());
                debug_assert_eq!(blob.storage_path, verified_blob.storage_path());

                let result = if let Some((old_file, old_blob)) = overwrite_ctx {
                    let current_file = revalidate_preuploaded_overwrite_target(
                        txn,
                        scope,
                        &old_file,
                        skip_lock_check,
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
                        update_storage_used(txn, scope, storage_delta).await?;
                    }
                    updated
                } else {
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
