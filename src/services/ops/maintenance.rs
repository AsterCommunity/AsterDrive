//! 服务模块：`ops::maintenance`。

use std::collections::HashSet;

use chrono::Utc;

use crate::db::repository::{file_create_idempotency_repo, file_repo, upload_session_repo};
use crate::errors::Result;
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use aster_drive_model::entities::upload_session;

const COMPLETED_SESSION_BATCH_SIZE: u64 = 1_000;
const BLOB_RECONCILE_BATCH_SIZE: u64 = 1_000;
const MULTIPART_ABORT_MAX_ATTEMPTS: u32 = 3;
const MULTIPART_ABORT_INITIAL_BACKOFF_MS: u64 = 200;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct UploadSessionMaintenanceStats {
    pub completed_sessions_deleted: u64,
    pub broken_completed_sessions_deleted: u64,
    pub file_create_idempotencies_deleted: u64,
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
    let mut stats = UploadSessionMaintenanceStats {
        file_create_idempotencies_deleted: file_create_idempotency_repo::delete_expired(
            state.writer_db(),
            now,
        )
        .await?,
        ..Default::default()
    };

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

            let temp_dir = aster_forge_utils::paths::upload_temp_dir(
                &state.config().server.upload_temp_dir,
                &session.id,
            );
            aster_forge_utils::fs::cleanup_temp_dir(&temp_dir).await;

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
        let actual_ref_counts =
            super::integrity::actual_blob_ref_counts_for_blobs(state.writer_db(), &blob_ids)
                .await?;

        for blob in blobs {
            if blob.ref_count == file_repo::BLOB_CLEANUP_CLAIMED_REF_COUNT {
                if crate::services::files::file::cleanup_unreferenced_blob(state, &blob).await {
                    stats.orphan_blobs_deleted += 1;
                }
                continue;
            }
            let actual_refs = actual_ref_counts.get(&blob.id).copied().unwrap_or(0);

            if i64::from(blob.ref_count) == actual_refs && actual_refs > 0 {
                continue;
            }

            let Some(reconciled) = super::integrity::reconcile_blob_reference_count(
                state.writer_db(),
                blob.id,
                super::integrity::BlobReconcileIntent::Repair,
            )
            .await?
            else {
                continue;
            };
            if reconciled.ref_count_fixed {
                stats.ref_count_fixed += 1;
            }
            if reconciled.actual_refs == 0
                && crate::services::files::file::cleanup_unreferenced_blob(state, &reconciled.blob)
                    .await
            {
                stats.orphan_blobs_deleted += 1;
            }
        }
    }

    Ok(stats)
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
                Err(err) if err.kind() == aster_drive_storage::StorageErrorKind::NotFound => {
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
    driver: &dyn aster_drive_storage::StorageDriver,
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
