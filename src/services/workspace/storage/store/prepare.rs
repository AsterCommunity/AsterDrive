use std::sync::Arc;

use chrono::Utc;
use sha2::{Digest, Sha256};

use super::{DedupTarget, TempBlobPlan};
use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::file_repo;
use crate::entities::{file, file_blob, storage_policy};
use crate::errors::{AsterError, MapAsterErr, Result, file_upload_error_with_code};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::workspace::storage::HASH_BUF_SIZE;
use crate::services::workspace::storage::{
    StorageOperationContext, StoreFromTempHints, StoreFromTempParams, WorkspaceStorageScope,
    check_quota, local_content_dedup_enabled, prepare_non_dedup_blob_upload,
    resolve_policy_for_size, upload_temp_file_to_prepared_blob,
    upload_temp_file_to_prepared_blob_cancellable, verify_file_access,
};
use crate::storage::StorageDriver;

pub(super) struct PreparedStoreFromTemp {
    pub scope: WorkspaceStorageScope,
    pub folder_id: Option<i64>,
    pub filename: String,
    pub temp_path: String,
    pub size: i64,
    pub existing_file_id: Option<i64>,
    pub policy: storage_policy::Model,
    pub driver: Arc<dyn StorageDriver>,
    pub blob_plan: TempBlobPlan,
    pub overwrite_ctx: Option<OverwriteContext>,
    pub operation_context: StorageOperationContext,
    pub storage_delta: i64,
    pub quota_prechecked: bool,
    pub mime: String,
    pub now: chrono::DateTime<Utc>,
    pub actor_username: Option<String>,
}

#[derive(Clone)]
pub(super) struct OverwriteContext {
    pub old_file: file::Model,
    pub old_blob: file_blob::Model,
    pub skip_lock_check: bool,
}

fn upload_hash_temp_open_failed(message: String) -> AsterError {
    file_upload_error_with_code(ApiErrorCode::UploadHashTempOpenFailed, message)
}

fn upload_hash_temp_read_failed(message: String) -> AsterError {
    file_upload_error_with_code(ApiErrorCode::UploadHashTempReadFailed, message)
}

pub(super) async fn prepare_store_from_temp(
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
        operation_context,
    } = hints;
    operation_context.checkpoint()?;

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

    let filename = aster_forge_validation::filename::normalize_validate_name(filename)?;

    let policy = match resolved_policy {
        Some(policy) => policy,
        None => resolve_policy_for_size(state, scope, folder_id, size).await?,
    };
    operation_context.checkpoint()?;
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

    let driver = state.driver_registry().get_driver(&policy)?;
    let blob_plan = build_temp_blob_plan(
        temp_path,
        size,
        &filename,
        precomputed_hash,
        should_dedup,
        &policy,
        &operation_context,
    )
    .await?;
    operation_context.checkpoint()?;
    let overwrite_ctx =
        load_overwrite_context(state, scope, existing_file_id, skip_lock_check).await?;
    operation_context.checkpoint()?;
    let storage_delta = overwrite_ctx.as_ref().map_or(size, |_| size);

    let quota_prechecked = storage_delta > 0 && matches!(blob_plan, TempBlobPlan::Preuploaded(_));
    if quota_prechecked {
        check_quota(state.writer_db(), scope, storage_delta).await?;
    }
    operation_context.checkpoint()?;

    if let TempBlobPlan::Preuploaded(preuploaded_blob) = &blob_plan {
        if operation_context.is_cancellable() {
            upload_temp_file_to_prepared_blob_cancellable(
                driver.as_ref(),
                preuploaded_blob,
                temp_path,
                &operation_context,
            )
            .await?;
        } else {
            upload_temp_file_to_prepared_blob(driver.as_ref(), preuploaded_blob, temp_path).await?;
        }
        if let Err(error) = operation_context.checkpoint() {
            crate::services::workspace::storage::cleanup_preuploaded_blob_upload(
                driver.as_ref(),
                preuploaded_blob,
                "cancellation after temp preupload",
            )
            .await;
            return Err(error);
        }
    } else {
        operation_context.checkpoint()?;
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
        operation_context,
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
    filename: &str,
    precomputed_hash: Option<&str>,
    should_dedup: bool,
    policy: &storage_policy::Model,
    operation_context: &StorageOperationContext,
) -> Result<TempBlobPlan> {
    if should_dedup {
        return Ok(TempBlobPlan::Dedup(
            compute_dedup_target(temp_path, precomputed_hash, operation_context).await?,
        ));
    }

    Ok(TempBlobPlan::Preuploaded(prepare_non_dedup_blob_upload(
        policy,
        size,
        Some(filename),
    )?))
}

async fn compute_dedup_target(
    temp_path: &str,
    precomputed_hash: Option<&str>,
    operation_context: &StorageOperationContext,
) -> Result<DedupTarget> {
    use tokio::io::AsyncReadExt;

    operation_context.checkpoint()?;
    let file_hash = match precomputed_hash {
        Some(file_hash) => file_hash.to_string(),
        None => {
            let mut hasher = Sha256::new();
            let mut reader = tokio::fs::File::open(temp_path)
                .await
                .map_aster_err_ctx("open temp", upload_hash_temp_open_failed)?;
            let mut buf = vec![0u8; HASH_BUF_SIZE];
            loop {
                operation_context.checkpoint()?;
                let n = reader
                    .read(&mut buf)
                    .await
                    .map_aster_err_ctx("read temp", upload_hash_temp_read_failed)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
            }
            operation_context.checkpoint()?;
            aster_forge_crypto::sha256_digest_to_hex(&hasher.finalize())
        }
    };

    Ok(DedupTarget {
        storage_path: aster_forge_validation::filename::storage_path_from_blob_key(&file_hash)?,
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

    let old_blob = file_repo::find_blob_by_id(state.writer_db(), old_file.blob_id).await?;
    if let Err(err) = crate::services::media::processing::delete_thumbnail(state, &old_blob).await {
        tracing::warn!("failed to delete thumbnail for blob {}: {err}", old_blob.id);
    }

    Ok(Some(OverwriteContext {
        old_file,
        old_blob,
        skip_lock_check,
    }))
}
