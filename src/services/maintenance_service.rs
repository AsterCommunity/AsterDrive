//! 服务模块：`maintenance_service`。

use std::collections::{HashMap, HashSet};

use chrono::Utc;

use crate::db::repository::{file_repo, upload_session_repo, version_repo};
use crate::entities::{file_blob, upload_session};
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};

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
            state.writer_db(),
            now,
            last_id.as_deref(),
            COMPLETED_SESSION_BATCH_SIZE,
        )
        .await?;

        if sessions.is_empty() {
            break;
        }
        last_id = sessions.last().map(|session| session.id.clone());

        let completed_temp_keys: Vec<String> = sessions
            .iter()
            .filter_map(|session| session.object_temp_key.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let tracked_blob_paths = file_repo::find_blob_storage_paths_by_storage_paths(
            state.writer_db(),
            &completed_temp_keys,
        )
        .await?;

        for session in sessions {
            let broken_completed = session.file_id.is_none();

            if session_stale_temp_key(&session, &tracked_blob_paths).is_some() {
                let cleanup_complete =
                    cleanup_completed_session_stale_temp_object(state, &session).await;
                if !cleanup_complete {
                    tracing::warn!(
                        session_id = %session.id,
                        "keeping completed upload session because stale temp object cleanup is incomplete"
                    );
                    continue;
                }
            }

            let temp_dir = crate::utils::paths::upload_temp_dir(
                &state.config().server.upload_temp_dir,
                &session.id,
            );
            crate::utils::cleanup_temp_dir(&temp_dir).await;

            match upload_session_repo::delete(state.writer_db(), &session.id).await {
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

fn session_stale_temp_key<'a>(
    session: &'a upload_session::Model,
    tracked_blob_paths: &HashSet<String>,
) -> Option<&'a str> {
    let temp_key = session.object_temp_key.as_deref()?;
    // Completed presigned uploads can have both a real file_id and the original
    // PUT temp key. Only skip cleanup when that key is still the tracked blob.
    if tracked_blob_paths.contains(temp_key) {
        return None;
    }
    Some(temp_key)
}

pub async fn reconcile_blob_state(state: &PrimaryAppState) -> Result<BlobMaintenanceStats> {
    let mut last_blob_id: Option<i64> = None;
    let mut stats = BlobMaintenanceStats::default();

    loop {
        let blobs = file_repo::find_blobs_paginated(
            state.writer_db(),
            last_blob_id,
            None,
            BLOB_RECONCILE_BATCH_SIZE,
        )
        .await?;

        if blobs.is_empty() {
            break;
        }
        last_blob_id = blobs.last().map(|blob| blob.id);
        let blob_ids: Vec<i64> = blobs.iter().map(|blob| blob.id).collect();
        // Keep reconcile memory bounded to the current blob page. Loading every
        // file/version reference count up front can spike memory on large
        // installs even though blob rows themselves are paginated.
        let actual_ref_counts = current_blob_ref_counts(state, &blob_ids).await?;

        for blob in blobs {
            let actual_refs = actual_ref_counts.get(&blob.id).copied().unwrap_or(0);

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
    let txn = crate::db::transaction::begin(state.writer_db()).await?;
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

async fn current_blob_ref_counts(
    state: &PrimaryAppState,
    blob_ids: &[i64],
) -> Result<HashMap<i64, i32>> {
    let file_refs =
        file_repo::count_blob_refs_from_files_for_blobs(state.writer_db(), blob_ids).await?;
    let version_refs =
        version_repo::count_blob_refs_from_versions_for_blobs(state.writer_db(), blob_ids).await?;
    let mut actual = HashMap::with_capacity(blob_ids.len());

    for blob_id in blob_ids {
        let file_count = file_refs.get(blob_id).copied().unwrap_or(0);
        let version_count = version_refs.get(blob_id).copied().unwrap_or(0);
        let total_refs = file_count.checked_add(version_count).ok_or_else(|| {
            AsterError::internal_error("blob ref count overflow during batch reconcile")
        })?;
        actual.insert(
            *blob_id,
            crate::utils::numbers::i64_to_i32(total_refs, "blob actual reference count")?,
        );
    }

    Ok(actual)
}

fn blob_cleanup_claim_is_stale(blob: &file_blob::Model) -> bool {
    Utc::now()
        .signed_duration_since(blob.updated_at)
        .num_seconds()
        >= BLOB_CLEANUP_CLAIM_TIMEOUT_SECS
}

async fn cleanup_completed_session_stale_temp_object(
    state: &PrimaryAppState,
    session: &upload_session::Model,
) -> bool {
    let Some(temp_key) = session.object_temp_key.as_deref() else {
        return true;
    };

    let Some(policy) = state.policy_snapshot().get_policy(session.policy_id) else {
        tracing::warn!(
            session_id = %session.id,
            policy_id = session.policy_id,
            "failed to load storage policy for completed upload session cleanup"
        );
        return false;
    };

    let Ok(driver) = state.driver_registry().get_driver(&policy) else {
        tracing::warn!(
            session_id = %session.id,
            policy_id = session.policy_id,
            "failed to resolve storage driver for completed upload session cleanup"
        );
        return false;
    };

    if let Some(multipart_id) = session.object_multipart_id.as_deref() {
        let Ok(multipart) = state.driver_registry().get_multipart_driver(&policy) else {
            // 策略不支持 multipart（如已切换为 Local），跳过 abort 直接删 key
            return delete_completed_stale_temp_object(&*driver, session, temp_key).await;
        };

        for attempt in 1..=MULTIPART_ABORT_MAX_ATTEMPTS {
            match multipart
                .abort_multipart_upload(temp_key, multipart_id)
                .await
            {
                Ok(()) => {
                    return delete_completed_stale_temp_object(&*driver, session, temp_key).await;
                }
                Err(err) => {
                    if attempt == MULTIPART_ABORT_MAX_ATTEMPTS {
                        tracing::warn!(
                            session_id = %session.id,
                            temp_key = %temp_key,
                            max_attempts = MULTIPART_ABORT_MAX_ATTEMPTS,
                            "failed to abort stale multipart upload for completed session after retries: {err}"
                        );
                        // Keep the session as the retry handle. Deleting an object key is not
                        // enough to guarantee incomplete multipart parts were released.
                        return false;
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
    }

    delete_completed_stale_temp_object(&*driver, session, temp_key).await
}

async fn delete_completed_stale_temp_object(
    driver: &dyn crate::storage::StorageDriver,
    session: &upload_session::Model,
    temp_key: &str,
) -> bool {
    match driver.delete(temp_key).await {
        Ok(()) => true,
        Err(e) => match driver.exists(temp_key).await {
            Ok(false) => true,
            Ok(true) => {
                tracing::warn!(
                    session_id = %session.id,
                    temp_key = %temp_key,
                    "failed to delete stale temp object for completed session: {e}"
                );
                false
            }
            Err(exists_error) => {
                tracing::warn!(
                    session_id = %session.id,
                    temp_key = %temp_key,
                    "failed to delete stale temp object and verify existence for completed session: delete_error={e}, exists_error={exists_error}"
                );
                false
            }
        },
    }
}
