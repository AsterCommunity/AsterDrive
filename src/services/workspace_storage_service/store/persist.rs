use crate::db::repository::file_repo;
use crate::entities::{file, file_blob};
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::workspace_storage_service::{
    check_quota, cleanup_preuploaded_blob_upload, persist_preuploaded_blob,
};
use sea_orm::ConnectionTrait;

use super::TempBlobPlan;
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
        storage_delta,
        quota_prechecked,
        mime,
        now,
        actor_username,
    } = prepared;
    let cleanup_blob_plan = blob_plan.clone();

    if storage_delta > 0 && !quota_prechecked {
        check_quota(&state.db, scope, storage_delta).await?;
    }
    stage_temp_blob_before_transaction(&blob_plan, driver.as_ref(), size, &temp_path).await?;

    let create_result = async {
        let txn = crate::db::transaction::begin(&state.db).await?;
        if storage_delta > 0 {
            check_quota(&txn, scope, storage_delta).await?;
        }

        let blob = persist_temp_blob(&txn, &blob_plan, size, policy.id).await?;
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

async fn stage_temp_blob_before_transaction(
    blob_plan: &TempBlobPlan,
    driver: &dyn crate::storage::driver::StorageDriver,
    size: i64,
    temp_path: &str,
) -> Result<()> {
    match blob_plan {
        TempBlobPlan::Dedup(target) => {
            crate::storage::drivers::local::promote_local_file_if_absent(
                driver,
                &target.storage_path,
                temp_path,
                size,
            )
            .await
        }
        TempBlobPlan::Preuploaded(_) => Ok(()),
    }
}

async fn persist_temp_blob<C: ConnectionTrait>(
    txn: &C,
    blob_plan: &TempBlobPlan,
    size: i64,
    policy_id: i64,
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
            Ok(blob.model)
        }
        TempBlobPlan::Preuploaded(preuploaded_blob) => {
            persist_preuploaded_blob(txn, preuploaded_blob).await
        }
    }
}
