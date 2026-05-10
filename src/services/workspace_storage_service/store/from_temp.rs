use std::sync::Arc;

use chrono::Utc;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Set};
use sha2::{Digest, Sha256};

use crate::db::repository::file_repo;
use crate::entities::{file, file_blob, storage_policy};
use crate::errors::{
    AsterError, MapAsterErr, Result, file_upload_error_with_subcode,
    precondition_failed_with_subcode,
};
use crate::runtime::PrimaryAppState;
use crate::services::storage_change_service;
use crate::storage::driver::StorageDriver;

use super::{
    HASH_BUF_SIZE, NewFileMode, PreparedNonDedupBlobUpload, StoreFromTempHints,
    StoreFromTempParams, WorkspaceStorageScope, check_quota, cleanup_preuploaded_blob_upload,
    create_exact_file_from_blob, create_exact_file_from_blob_with_actor_username,
    create_new_file_from_blob, create_new_file_from_blob_with_actor_username,
    local_content_dedup_enabled, persist_preuploaded_blob, prepare_non_dedup_blob_upload,
    resolve_policy_for_size, update_storage_used, upload_temp_file_to_prepared_blob,
    verify_file_access,
};

#[derive(Clone)]
struct DedupTarget {
    file_hash: String,
    storage_path: String,
}

struct OverwriteContext {
    old_file: file::Model,
    old_blob: file_blob::Model,
    skip_lock_check: bool,
}

#[derive(Clone)]
enum TempBlobPlan {
    Dedup(DedupTarget),
    Preuploaded(PreparedNonDedupBlobUpload),
}

struct PreparedStoreFromTemp {
    scope: WorkspaceStorageScope,
    folder_id: Option<i64>,
    filename: String,
    temp_path: String,
    size: i64,
    existing_file_id: Option<i64>,
    policy: storage_policy::Model,
    driver: Arc<dyn StorageDriver>,
    blob_plan: TempBlobPlan,
    overwrite_ctx: Option<OverwriteContext>,
    storage_delta: i64,
    quota_prechecked: bool,
    mime: String,
    now: chrono::DateTime<Utc>,
    actor_username: Option<String>,
}

struct WriteFileRecordFromTempParams<'a> {
    scope: WorkspaceStorageScope,
    folder_id: Option<i64>,
    filename: &'a str,
    mime: &'a str,
    blob: &'a file_blob::Model,
    overwrite_ctx: Option<OverwriteContext>,
    now: chrono::DateTime<Utc>,
    storage_delta: i64,
    new_file_mode: NewFileMode,
    actor_username: Option<&'a str>,
}

fn upload_hash_temp_open_failed(message: String) -> AsterError {
    file_upload_error_with_subcode("upload.hash_temp_open_failed", message)
}

fn upload_hash_temp_read_failed(message: String) -> AsterError {
    file_upload_error_with_subcode("upload.hash_temp_read_failed", message)
}

pub(super) async fn store_from_temp_internal(
    state: &PrimaryAppState,
    params: StoreFromTempParams<'_>,
    hints: StoreFromTempHints<'_>,
    new_file_mode: NewFileMode,
) -> Result<file::Model> {
    let prepared = prepare_store_from_temp(state, params, hints).await?;
    let scope = prepared.scope;
    let existing_file_id = prepared.existing_file_id;
    let overwritten = existing_file_id.is_some();
    let result = persist_temp_store(state, prepared, new_file_mode).await?;

    let event_kind = if overwritten {
        storage_change_service::StorageChangeKind::FileUpdated
    } else {
        storage_change_service::StorageChangeKind::FileCreated
    };
    storage_change_service::publish(
        state,
        storage_change_service::StorageChangeEvent::new(
            event_kind,
            scope,
            vec![result.id],
            vec![],
            vec![result.folder_id],
        ),
    );

    if let Some(existing_id) = existing_file_id {
        crate::services::version_service::cleanup_excess(state, existing_id).await?;
    }

    tracing::debug!(
        scope = ?scope,
        file_id = result.id,
        blob_id = result.blob_id,
        folder_id = result.folder_id,
        overwritten,
        size = result.size,
        "stored file from temp"
    );

    Ok(result)
}

async fn prepare_store_from_temp(
    state: &PrimaryAppState,
    params: StoreFromTempParams<'_>,
    hints: StoreFromTempHints<'_>,
) -> Result<PreparedStoreFromTemp> {
    let StoreFromTempParams {
        scope,
        folder_id,
        filename,
        temp_path,
        size,
        existing_file_id,
        skip_lock_check,
    } = params;
    let StoreFromTempHints {
        resolved_policy,
        precomputed_hash,
        actor_username,
    } = hints;

    tracing::debug!(
        scope = ?scope,
        folder_id,
        filename = %filename,
        size,
        existing_file_id,
        skip_lock_check,
        policy_hint = resolved_policy.as_ref().map(|policy| policy.id),
        has_precomputed_hash = precomputed_hash.is_some(),
        "storing file from temp"
    );

    let filename = crate::utils::normalize_validate_name(filename)?;

    let policy = match resolved_policy {
        Some(policy) => policy,
        None => resolve_policy_for_size(state, scope, folder_id, size).await?,
    };
    let should_dedup = local_content_dedup_enabled(&policy);

    tracing::debug!(
        scope = ?scope,
        policy_id = policy.id,
        driver_type = ?policy.driver_type,
        should_dedup,
        "resolved storage policy for temp file"
    );

    if policy.max_file_size > 0 && size > policy.max_file_size {
        return Err(AsterError::file_too_large(format!(
            "file size {} exceeds limit {}",
            size, policy.max_file_size
        )));
    }

    let driver = state.driver_registry.get_driver(&policy)?;
    let blob_plan =
        build_temp_blob_plan(temp_path, size, precomputed_hash, should_dedup, &policy).await?;
    let overwrite_ctx =
        load_overwrite_context(state, scope, existing_file_id, skip_lock_check).await?;
    let storage_delta = overwrite_ctx.as_ref().map_or(size, |_| size);

    let quota_prechecked = storage_delta > 0 && matches!(blob_plan, TempBlobPlan::Preuploaded(_));
    if quota_prechecked {
        check_quota(&state.db, scope, storage_delta).await?;
    }

    if let TempBlobPlan::Preuploaded(preuploaded_blob) = &blob_plan {
        upload_temp_file_to_prepared_blob(driver.as_ref(), preuploaded_blob, temp_path).await?;
    }

    Ok(PreparedStoreFromTemp {
        scope,
        folder_id,
        filename: filename.clone(),
        temp_path: temp_path.to_string(),
        size,
        existing_file_id,
        policy,
        driver,
        blob_plan,
        overwrite_ctx,
        storage_delta,
        quota_prechecked,
        mime: mime_guess::from_path(&filename)
            .first_or_octet_stream()
            .to_string(),
        now: Utc::now(),
        actor_username: actor_username.map(ToOwned::to_owned),
    })
}

async fn build_temp_blob_plan(
    temp_path: &str,
    size: i64,
    precomputed_hash: Option<&str>,
    should_dedup: bool,
    policy: &storage_policy::Model,
) -> Result<TempBlobPlan> {
    if should_dedup {
        return Ok(TempBlobPlan::Dedup(
            compute_dedup_target(temp_path, precomputed_hash).await?,
        ));
    }

    Ok(TempBlobPlan::Preuploaded(prepare_non_dedup_blob_upload(
        policy, size,
    )))
}

async fn compute_dedup_target(
    temp_path: &str,
    precomputed_hash: Option<&str>,
) -> Result<DedupTarget> {
    use tokio::io::AsyncReadExt;

    let file_hash = match precomputed_hash {
        Some(file_hash) => file_hash.to_string(),
        None => {
            let mut hasher = Sha256::new();
            let mut reader = tokio::fs::File::open(temp_path)
                .await
                .map_aster_err_ctx("open temp", upload_hash_temp_open_failed)?;
            let mut buf = vec![0u8; HASH_BUF_SIZE];
            loop {
                let n = reader
                    .read(&mut buf)
                    .await
                    .map_aster_err_ctx("read temp", upload_hash_temp_read_failed)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
            }
            crate::utils::hash::sha256_digest_to_hex(&hasher.finalize())
        }
    };

    Ok(DedupTarget {
        storage_path: crate::utils::storage_path_from_blob_key(&file_hash),
        file_hash,
    })
}

async fn load_overwrite_context(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    existing_file_id: Option<i64>,
    skip_lock_check: bool,
) -> Result<Option<OverwriteContext>> {
    let Some(existing_id) = existing_file_id else {
        return Ok(None);
    };

    let old_file = verify_file_access(state, scope, existing_id).await?;
    if old_file.is_locked && !skip_lock_check {
        return Err(AsterError::resource_locked("file is locked"));
    }

    let old_blob = file_repo::find_blob_by_id(&state.db, old_file.blob_id).await?;
    if let Err(err) =
        crate::services::media_processing_service::delete_thumbnail(state, &old_blob).await
    {
        tracing::warn!("failed to delete thumbnail for blob {}: {err}", old_blob.id);
    }

    Ok(Some(OverwriteContext {
        old_file,
        old_blob,
        skip_lock_check,
    }))
}

async fn persist_temp_store(
    state: &PrimaryAppState,
    prepared: PreparedStoreFromTemp,
    new_file_mode: NewFileMode,
) -> Result<file::Model> {
    let PreparedStoreFromTemp {
        scope,
        folder_id,
        filename,
        temp_path,
        size,
        existing_file_id: _,
        policy,
        driver,
        blob_plan,
        overwrite_ctx,
        storage_delta,
        quota_prechecked,
        mime,
        now,
        actor_username,
    } = prepared;
    let cleanup_blob_plan = blob_plan.clone();

    let create_result = async {
        let txn = crate::db::transaction::begin(&state.db).await?;
        if storage_delta > 0 && !quota_prechecked {
            check_quota(&txn, scope, storage_delta).await?;
        }

        let blob = persist_temp_blob(
            &txn,
            &blob_plan,
            driver.as_ref(),
            size,
            policy.id,
            &temp_path,
        )
        .await?;
        let result = write_file_record_from_temp(
            &txn,
            WriteFileRecordFromTempParams {
                scope,
                folder_id,
                filename: &filename,
                mime: &mime,
                blob: &blob,
                overwrite_ctx,
                now,
                storage_delta,
                new_file_mode,
                actor_username: actor_username.as_deref(),
            },
        )
        .await?;

        crate::db::transaction::commit(txn).await?;
        Ok::<file::Model, AsterError>(result)
    }
    .await;

    match create_result {
        Ok(result) => Ok(result),
        Err(error) => {
            if let TempBlobPlan::Preuploaded(preuploaded_blob) = &cleanup_blob_plan {
                cleanup_preuploaded_blob_upload(
                    driver.as_ref(),
                    preuploaded_blob,
                    "DB error after temp file upload",
                )
                .await;
            }
            Err(error)
        }
    }
}

async fn persist_temp_blob<C: ConnectionTrait>(
    txn: &C,
    blob_plan: &TempBlobPlan,
    driver: &dyn StorageDriver,
    size: i64,
    policy_id: i64,
    temp_path: &str,
) -> Result<file_blob::Model> {
    match blob_plan {
        TempBlobPlan::Dedup(target) => {
            let blob = file_repo::find_or_create_blob(
                txn,
                &target.file_hash,
                size,
                policy_id,
                &target.storage_path,
            )
            .await?;
            if blob.inserted {
                let stream_driver = driver.as_stream_upload().ok_or_else(|| {
                    AsterError::storage_driver_error("stream upload not supported")
                })?;
                stream_driver
                    .put_file(&target.storage_path, temp_path)
                    .await?;
            }
            Ok(blob.model)
        }
        TempBlobPlan::Preuploaded(preuploaded_blob) => {
            persist_preuploaded_blob(txn, preuploaded_blob).await
        }
    }
}

async fn write_file_record_from_temp<C: ConnectionTrait>(
    txn: &C,
    params: WriteFileRecordFromTempParams<'_>,
) -> Result<file::Model> {
    let WriteFileRecordFromTempParams {
        scope,
        folder_id,
        filename,
        mime,
        blob,
        overwrite_ctx,
        now,
        storage_delta,
        new_file_mode,
        actor_username,
    } = params;
    let result = if let Some(OverwriteContext {
        old_file,
        old_blob,
        skip_lock_check,
    }) = overwrite_ctx
    {
        let current_file =
            revalidate_overwrite_target(txn, scope, &old_file, skip_lock_check).await?;
        let existing_id = current_file.id;
        let mut active: file::ActiveModel = current_file.into();
        active.blob_id = Set(blob.id);
        active.size = Set(blob.size);
        active.mime_type = Set(mime.to_string());
        active.updated_at = Set(now);
        let updated = active
            .update(txn)
            .await
            .map_aster_err(AsterError::database_operation)?;

        let next_ver = crate::db::repository::version_repo::next_version(txn, existing_id).await?;
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
        updated
    } else {
        match new_file_mode {
            NewFileMode::ResolveUnique => match actor_username {
                Some(username) => {
                    create_new_file_from_blob_with_actor_username(
                        txn, scope, folder_id, filename, blob, now, username,
                    )
                    .await?
                }
                None => {
                    create_new_file_from_blob(txn, scope, folder_id, filename, blob, now).await?
                }
            },
            NewFileMode::Exact => match actor_username {
                Some(username) => {
                    create_exact_file_from_blob_with_actor_username(
                        txn, scope, folder_id, filename, blob, now, username,
                    )
                    .await?
                }
                None => {
                    create_exact_file_from_blob(txn, scope, folder_id, filename, blob, now).await?
                }
            },
        }
    };

    if storage_delta != 0 {
        update_storage_used(txn, scope, storage_delta).await?;
    }

    Ok(result)
}

async fn revalidate_overwrite_target<C: ConnectionTrait>(
    txn: &C,
    scope: WorkspaceStorageScope,
    old_file: &file::Model,
    skip_lock_check: bool,
) -> Result<file::Model> {
    let current_file = file_repo::lock_by_id(txn, old_file.id).await?;
    crate::services::workspace_storage_service::ensure_active_file_scope(&current_file, scope)?;

    if current_file.blob_id != old_file.blob_id {
        return Err(precondition_failed_with_subcode(
            "file.modified_during_write",
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
