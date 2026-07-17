use crate::db::repository::file_repo;
use crate::entities::{file, file_blob};
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::workspace::storage::{
    StorageOperationContext, check_quota, cleanup_preuploaded_blob_upload, persist_preuploaded_blob,
};
use aster_forge_db::transaction;
use sea_orm::ConnectionTrait;

use super::TempBlobPlan;
use super::contract::{
    TempStoreBlobCleanupPlan, VerifiedTempStoreBlob, VerifiedTempStoreBlobSource,
};
use super::prepare::PreparedStoreFromTemp;
use super::write_record::WriteFileRecordFromTempParams;

pub(super) async fn persist_temp_store(
    state: &PrimaryAppState,
    prepared: PreparedStoreFromTemp,
    new_file_mode: super::NewFileMode,
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
        operation_context,
        storage_delta,
        quota_prechecked,
        mime,
        now,
        actor_username,
    } = prepared;

    operation_context.checkpoint()?;
    if storage_delta > 0 && !quota_prechecked {
        check_quota(state.writer_db(), scope, storage_delta).await?;
    }
    operation_context.checkpoint()?;
    let staged_dedup_target = stage_temp_blob_before_transaction(
        &blob_plan,
        driver.as_ref(),
        size,
        &temp_path,
        &operation_context,
    )
    .await?;
    let verified_blob = match VerifiedTempStoreBlob::from_staged_plan(
        &blob_plan,
        size,
        policy.id,
        staged_dedup_target,
    ) {
        Ok(verified_blob) => verified_blob,
        Err(error) => {
            cleanup_staged_blob_plan_after_contract_failure(
                state,
                &blob_plan,
                staged_dedup_target,
                driver.as_ref(),
                policy.id,
                "verified temp store contract failure",
            )
            .await;
            return Err(error);
        }
    };
    if let Err(error) = operation_context.checkpoint() {
        cleanup_verified_temp_blob_after_db_failure(
            state,
            &verified_blob,
            driver.as_ref(),
            "cancellation after temp file upload",
        )
        .await;
        return Err(error);
    }

    let create_result = async {
        let txn = transaction::begin(state.writer_db()).await?;
        crate::services::workspace::storage::lock_storage_usage(&txn, scope).await?;
        operation_context.checkpoint()?;
        if storage_delta > 0 {
            check_quota(&txn, scope, storage_delta).await?;
        }
        operation_context.checkpoint()?;

        let blob = persist_temp_blob(&txn, &verified_blob).await?;
        operation_context.checkpoint()?;
        let result = super::write_record::write_file_record_from_temp(
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
        operation_context.checkpoint()?;

        transaction::commit(txn).await?;
        Ok::<file::Model, AsterError>(result)
    }
    .await;

    match create_result {
        Ok(result) => Ok(result),
        Err(error) => {
            cleanup_verified_temp_blob_after_db_failure(
                state,
                &verified_blob,
                driver.as_ref(),
                "DB error after temp file upload",
            )
            .await;
            Err(error)
        }
    }
}

async fn stage_temp_blob_before_transaction(
    blob_plan: &TempBlobPlan,
    driver: &dyn crate::storage::StorageDriver,
    size: i64,
    temp_path: &str,
    operation_context: &StorageOperationContext,
) -> Result<bool> {
    match blob_plan {
        TempBlobPlan::Dedup(target) => Ok(
            crate::storage::drivers::local::promote_local_file_if_absent_with_check(
                driver,
                &target.storage_path,
                temp_path,
                size,
                || operation_context.checkpoint(),
            )
            .await?
            .created(),
        ),
        TempBlobPlan::Preuploaded(_) => Ok(false),
    }
}

async fn cleanup_staged_blob_plan_after_contract_failure(
    state: &PrimaryAppState,
    blob_plan: &TempBlobPlan,
    staged_dedup_target: bool,
    driver: &dyn crate::storage::StorageDriver,
    policy_id: i64,
    reason: &str,
) {
    match blob_plan {
        TempBlobPlan::Dedup(target) if staged_dedup_target => {
            rollback_staged_dedup_blob(
                state,
                &target.file_hash,
                &target.storage_path,
                driver,
                policy_id,
            )
            .await;
        }
        TempBlobPlan::Dedup(_) => {}
        TempBlobPlan::Preuploaded(preuploaded_blob) => {
            cleanup_preuploaded_blob_upload(driver, preuploaded_blob, reason).await;
        }
    }
}

async fn cleanup_verified_temp_blob_after_db_failure(
    state: &PrimaryAppState,
    verified_blob: &VerifiedTempStoreBlob,
    driver: &dyn crate::storage::StorageDriver,
    reason: &str,
) {
    match verified_blob.cleanup() {
        TempStoreBlobCleanupPlan::RollbackStagedDedupIfUnreferenced => {
            if let VerifiedTempStoreBlobSource::ContentAddressed { file_hash } =
                verified_blob.source()
            {
                rollback_staged_dedup_blob(
                    state,
                    file_hash,
                    verified_blob.storage_path(),
                    driver,
                    verified_blob.policy_id(),
                )
                .await;
            }
        }
        TempStoreBlobCleanupPlan::CleanupPreuploadedBlobOnDbFailure => {
            if let VerifiedTempStoreBlobSource::PreuploadedNonDedup { prepared } =
                verified_blob.source()
            {
                cleanup_preuploaded_blob_upload(driver, prepared, reason).await;
            }
        }
        TempStoreBlobCleanupPlan::RetainExistingDedupObject => {}
    }
}

async fn rollback_staged_dedup_blob(
    state: &PrimaryAppState,
    file_hash: &str,
    storage_path: &str,
    driver: &dyn crate::storage::StorageDriver,
    policy_id: i64,
) {
    match file_repo::find_blob_by_hash(state.writer_db(), file_hash, policy_id).await {
        Ok(Some(blob)) => {
            tracing::debug!(
                blob_id = blob.id,
                storage_path,
                "skipping staged dedup blob rollback because a blob row now references it"
            );
            return;
        }
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(
                storage_path,
                "failed to verify staged dedup blob before rollback; keeping object: {error}"
            );
            return;
        }
    }

    if let Err(error) = driver.delete(storage_path).await {
        tracing::warn!(
            storage_path,
            "failed to rollback staged dedup blob after DB error: {error}"
        );
    }
}

async fn persist_temp_blob<C: ConnectionTrait>(
    txn: &C,
    verified_blob: &VerifiedTempStoreBlob,
) -> Result<file_blob::Model> {
    match verified_blob.source() {
        VerifiedTempStoreBlobSource::ContentAddressed { file_hash } => {
            let blob = file_repo::find_or_create_blob(
                txn,
                file_hash,
                verified_blob.size(),
                verified_blob.policy_id(),
                verified_blob.storage_path(),
            )
            .await?;
            Ok(blob.model)
        }
        VerifiedTempStoreBlobSource::PreuploadedNonDedup { prepared } => {
            persist_preuploaded_blob(txn, prepared).await
        }
    }
}
