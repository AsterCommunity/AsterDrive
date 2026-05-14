//! 服务模块：`maintenance_service`。

use std::collections::HashSet;

use chrono::Utc;

use crate::db::repository::{file_repo, upload_session_repo, version_repo};
use crate::entities::{file_blob, upload_session};
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;

const COMPLETED_SESSION_BATCH_SIZE: u64 = 1_000;
const BLOB_RECONCILE_BATCH_SIZE: u64 = 1_000;
const BLOB_CLEANUP_CLAIM_TIMEOUT_SECS: i64 = 10 * 60;
const MULTIPART_ABORT_MAX_ATTEMPTS: u32 = 3;
const MULTIPART_ABORT_INITIAL_BACKOFF_MS: u64 = 200;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct UploadSessionMaintenanceStats {
    pub completed_sessions_deleted: u64,
    pub broken_completed_sessions_deleted: u64,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BlobMaintenanceStats {
    pub ref_count_fixed: u64,
    pub orphan_blobs_deleted: u64,
}

pub async fn cleanup_expired_completed_upload_sessions(
    state: &PrimaryAppState,
) -> Result<UploadSessionMaintenanceStats> {
    let now = Utc::now();
    let mut last_id: Option<String> = None;
    let mut stats = UploadSessionMaintenanceStats::default();

    loop {
        let sessions = upload_session_repo::find_expired_completed_paginated(
            &state.db,
            now,
            last_id.as_deref(),
            COMPLETED_SESSION_BATCH_SIZE,
        )
        .await?;

        if sessions.is_empty() {
            break;
        }
        last_id = sessions.last().map(|session| session.id.clone());

        let broken_temp_keys: Vec<String> = sessions
            .iter()
            .filter(|session| session.file_id.is_none())
            .filter_map(|session| session.s3_temp_key.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let tracked_blob_paths =
            file_repo::find_blob_storage_paths_by_storage_paths(&state.db, &broken_temp_keys)
                .await?;

        for session in sessions {
            let broken_completed = session.file_id.is_none();

            if broken_completed {
                cleanup_broken_completed_session_object(state, &session, &tracked_blob_paths).await;
            }

            let temp_dir = crate::utils::paths::upload_temp_dir(
                &state.config.server.upload_temp_dir,
                &session.id,
            );
            crate::utils::cleanup_temp_dir(&temp_dir).await;

            match upload_session_repo::delete(&state.db, &session.id).await {
                Ok(()) => {
                    stats.completed_sessions_deleted += 1;
                    if broken_completed {
                        stats.broken_completed_sessions_deleted += 1;
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        session_id = %session.id,
                        "failed to delete expired completed upload session: {e}"
                    );
                }
            }
        }
    }

    Ok(stats)
}

pub async fn reconcile_blob_state(state: &PrimaryAppState) -> Result<BlobMaintenanceStats> {
    let mut actual_ref_counts = load_actual_blob_ref_counts(state).await?;
    let mut last_blob_id: Option<i64> = None;
    let mut stats = BlobMaintenanceStats::default();

    loop {
        let blobs =
            file_repo::find_blobs_paginated(&state.db, last_blob_id, BLOB_RECONCILE_BATCH_SIZE)
                .await?;

        if blobs.is_empty() {
            break;
        }
        last_blob_id = blobs.last().map(|blob| blob.id);

        for blob in blobs {
            let actual_refs = match actual_ref_counts.remove(&blob.id) {
                Some(count) => i32::try_from(count).map_err(|_| {
                    AsterError::internal_error(format!(
                        "actual ref count overflow for blob {}",
                        blob.id
                    ))
                })?,
                None => 0,
            };

            if blob.ref_count == actual_refs && actual_refs > 0 {
                continue;
            }

            let Some(reconciled) = reconcile_single_blob_ref_count(state, blob.id).await? else {
                continue;
            };
            if reconciled.ref_count_fixed {
                stats.ref_count_fixed += 1;
            }
            if reconciled.actual_refs == 0
                && crate::services::file_service::cleanup_unreferenced_blob(state, &reconciled.blob)
                    .await
            {
                stats.orphan_blobs_deleted += 1;
            }
        }
    }

    Ok(stats)
}

struct ReconciledBlob {
    blob: file_blob::Model,
    actual_refs: i32,
    ref_count_fixed: bool,
}

async fn reconcile_single_blob_ref_count(
    state: &PrimaryAppState,
    blob_id: i64,
) -> Result<Option<ReconciledBlob>> {
    let txn = crate::db::transaction::begin(&state.db).await?;
    let result = async {
        let mut blob = match file_repo::lock_blob_by_id(&txn, blob_id).await {
            Ok(blob) => blob,
            Err(AsterError::RecordNotFound(_)) => return Ok(None),
            Err(error) => return Err(error),
        };
        if blob.ref_count == file_repo::BLOB_CLEANUP_CLAIMED_REF_COUNT
            && !blob_cleanup_claim_is_stale(&blob)
        {
            tracing::debug!(
                blob_id,
                "skipping blob reconcile because cleanup is already claimed"
            );
            return Ok(None);
        }
        let actual_refs = current_blob_ref_count(&txn, blob_id).await?;
        let ref_count_fixed = blob.ref_count != actual_refs;
        if ref_count_fixed {
            file_repo::set_blob_ref_count(&txn, blob_id, actual_refs).await?;
            blob = file_repo::find_blob_by_id(&txn, blob_id).await?;
        }
        Ok(Some(ReconciledBlob {
            blob,
            actual_refs,
            ref_count_fixed,
        }))
    }
    .await;

    match result {
        Ok(reconciled) => {
            crate::db::transaction::commit(txn).await?;
            Ok(reconciled)
        }
        Err(error) => {
            if let Err(rollback_error) = crate::db::transaction::rollback(txn).await {
                tracing::error!(
                    blob_id,
                    original_error = %error,
                    rollback_error = %rollback_error,
                    "failed to rollback blob reconcile transaction"
                );
            }
            Err(error)
        }
    }
}

async fn current_blob_ref_count<C: sea_orm::ConnectionTrait>(db: &C, blob_id: i64) -> Result<i32> {
    let file_refs = file_repo::count_blob_refs_from_files_for_blob(db, blob_id).await?;
    let version_refs = version_repo::count_blob_refs_from_versions_for_blob(db, blob_id).await?;
    let total_refs = file_refs
        .checked_add(version_refs)
        .ok_or_else(|| AsterError::internal_error("blob ref count overflow during reconcile"))?;
    i32::try_from(total_refs).map_err(|_| {
        AsterError::internal_error(format!("actual ref count overflow for blob {blob_id}"))
    })
}

async fn load_actual_blob_ref_counts(
    state: &PrimaryAppState,
) -> Result<std::collections::HashMap<i64, i64>> {
    let mut actual = file_repo::count_blob_refs_from_files(&state.db).await?;

    let version_refs = version_repo::count_blob_refs_from_versions(&state.db).await?;
    for (blob_id, ref_count) in version_refs {
        *actual.entry(blob_id).or_insert(0) += ref_count;
    }

    Ok(actual)
}

fn blob_cleanup_claim_is_stale(blob: &file_blob::Model) -> bool {
    Utc::now()
        .signed_duration_since(blob.updated_at)
        .num_seconds()
        >= BLOB_CLEANUP_CLAIM_TIMEOUT_SECS
}

async fn cleanup_broken_completed_session_object(
    state: &PrimaryAppState,
    session: &upload_session::Model,
    tracked_blob_paths: &HashSet<String>,
) {
    let Some(temp_key) = session.s3_temp_key.as_deref() else {
        return;
    };

    if tracked_blob_paths.contains(temp_key) {
        return;
    }

    let Some(policy) = state.policy_snapshot.get_policy(session.policy_id) else {
        tracing::warn!(
            session_id = %session.id,
            policy_id = session.policy_id,
            "failed to load storage policy for completed upload session cleanup"
        );
        return;
    };

    let Ok(driver) = state.driver_registry.get_driver(&policy) else {
        tracing::warn!(
            session_id = %session.id,
            policy_id = session.policy_id,
            "failed to resolve storage driver for completed upload session cleanup"
        );
        return;
    };

    if let Some(multipart_id) = session.s3_multipart_id.as_deref() {
        let Ok(multipart) = state.driver_registry.get_multipart_driver(&policy) else {
            // 策略不支持 multipart（如已切换为 Local），跳过 abort 直接删 key
            if let Err(e) = driver.delete(temp_key).await {
                tracing::warn!(
                    session_id = %session.id,
                    temp_key = %temp_key,
                    "failed to delete stale temp object for completed session: {e}"
                );
            }
            return;
        };

        let mut abort_error = None;
        for attempt in 1..=MULTIPART_ABORT_MAX_ATTEMPTS {
            match multipart
                .abort_multipart_upload(temp_key, multipart_id)
                .await
            {
                Ok(()) => {
                    abort_error = None;
                    break;
                }
                Err(err) => {
                    if attempt == MULTIPART_ABORT_MAX_ATTEMPTS {
                        abort_error = Some(err);
                        break;
                    }
                    let backoff_ms = MULTIPART_ABORT_INITIAL_BACKOFF_MS * (1_u64 << (attempt - 1));
                    tracing::warn!(
                        session_id = %session.id,
                        temp_key = %temp_key,
                        attempt,
                        max_attempts = MULTIPART_ABORT_MAX_ATTEMPTS,
                        backoff_ms,
                        "failed to abort stale multipart upload for completed session, retrying: {err}"
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                }
            }
        }

        if let Some(e) = abort_error {
            tracing::warn!(
                session_id = %session.id,
                temp_key = %temp_key,
                max_attempts = MULTIPART_ABORT_MAX_ATTEMPTS,
                "failed to abort stale multipart upload for completed session after retries: {e}"
            );

            // 删除对象 key 不能回收仍在进行中的 multipart parts；生产环境仍应配置
            // S3/MinIO 生命周期规则来清理 incomplete multipart uploads。
            if let Err(delete_err) = driver.delete(temp_key).await {
                tracing::warn!(
                    session_id = %session.id,
                    temp_key = %temp_key,
                    "failed to delete stale completed multipart object after abort retries exhausted: {delete_err}"
                );
            }
        }
    } else if let Err(e) = driver.delete(temp_key).await {
        tracing::warn!(
            session_id = %session.id,
            temp_key = %temp_key,
            "failed to delete stale temp object for completed session: {e}"
        );
    }
}
