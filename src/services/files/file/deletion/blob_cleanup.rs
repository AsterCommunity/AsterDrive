use crate::db::repository::file_repo;
use crate::errors::AsterError;
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::media::processing;
use aster_drive_model::entities::{file_blob, storage_policy};
use aster_drive_storage::StorageDriver;
use std::sync::Arc;

pub(crate) async fn ensure_blob_cleanup_if_unreferenced(
    state: &PrimaryAppState,
    blob_id: i64,
) -> bool {
    let current_blob = match file_repo::find_blob_by_id(state.writer_db(), blob_id).await {
        Ok(current_blob) => current_blob,
        Err(AsterError::RecordNotFound(_)) => return true,
        Err(error) => {
            tracing::warn!(
                blob_id,
                "failed to reload blob before deciding whether cleanup is needed: {error}"
            );
            return false;
        }
    };

    if current_blob.ref_count == file_repo::BLOB_CLEANUP_CLAIMED_REF_COUNT {
        tracing::debug!(
            blob_id = current_blob.id,
            "skipping blob cleanup because cleanup is already claimed"
        );
        return true;
    }

    if current_blob.ref_count != 0 {
        return true;
    }

    match file_repo::claim_blob_cleanup(state.writer_db(), current_blob.id).await {
        Ok(true) => {
            cleanup_claimed_blob(state, &current_blob, &mut |policy| {
                state.driver_registry().get_driver(policy)
            })
            .await
        }
        Ok(false) => true,
        Err(error) => {
            tracing::warn!(
                blob_id = current_blob.id,
                "failed to claim blob cleanup: {error}"
            );
            false
        }
    }
}

pub(crate) async fn cleanup_unreferenced_blob(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
) -> bool {
    cleanup_unreferenced_blob_with_driver(state, blob, &mut |policy| {
        state.driver_registry().get_driver(policy)
    })
    .await
}

pub(crate) async fn cleanup_unreferenced_blob_with_driver<F>(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    driver_for_policy: &mut F,
) -> bool
where
    F: FnMut(&storage_policy::Model) -> crate::errors::Result<Arc<dyn StorageDriver>>,
{
    let current_blob = match file_repo::find_blob_by_id(state.writer_db(), blob.id).await {
        Ok(current_blob) => current_blob,
        Err(AsterError::RecordNotFound(_)) => return true,
        Err(error) => {
            tracing::warn!(
                blob_id = blob.id,
                "failed to reload blob before cleanup: {error}"
            );
            return false;
        }
    };

    if current_blob.ref_count == file_repo::BLOB_CLEANUP_CLAIMED_REF_COUNT {
        let eligible = crate::services::ops::integrity::reconcile_blob_reference_count(
            state.writer_db(),
            current_blob.id,
            crate::services::ops::integrity::BlobReconcileIntent::Cleanup,
        )
        .await;
        return match eligible {
            Ok(Some(reconciled)) => {
                cleanup_claimed_blob(state, &reconciled.blob, driver_for_policy).await
            }
            Ok(None) => false,
            Err(error) => {
                tracing::warn!(
                    blob_id = current_blob.id,
                    "cannot verify claimed cleanup retry: {error}"
                );
                false
            }
        };
    }

    if current_blob.ref_count != 0 {
        tracing::warn!(
            blob_id = current_blob.id,
            ref_count = current_blob.ref_count,
            "skipping blob cleanup because blob is referenced again"
        );
        return false;
    }

    match file_repo::claim_blob_cleanup(state.writer_db(), current_blob.id).await {
        Ok(true) => {}
        Ok(false) => {
            tracing::warn!(
                blob_id = current_blob.id,
                "skipping blob cleanup because another worker already claimed it or it was revived"
            );
            return false;
        }
        Err(error) => {
            tracing::warn!(
                blob_id = current_blob.id,
                "failed to claim blob cleanup: {error}"
            );
            return false;
        }
    }

    cleanup_claimed_blob(state, &current_blob, driver_for_policy).await
}

enum CompletedBlobCleanup {
    Virtual,
    RetainedShared,
    Deleted,
}

async fn cleanup_claimed_blob<F>(
    state: &PrimaryAppState,
    current_blob: &file_blob::Model,
    driver_for_policy: &mut F,
) -> bool
where
    F: FnMut(&storage_policy::Model) -> crate::errors::Result<Arc<dyn StorageDriver>>,
{
    let mut attempt = aster_drive_metrics::BlobCleanupGuard::new(state.metrics().clone());
    let completed = cleanup_claimed_blob_impl(state, current_blob, driver_for_policy).await;
    let (outcome, bytes) = match &completed {
        None => (aster_drive_metrics::CleanupOutcome::Failure, 0),
        Some(CompletedBlobCleanup::Virtual) => (aster_drive_metrics::CleanupOutcome::Virtual, 0),
        Some(CompletedBlobCleanup::RetainedShared) => {
            (aster_drive_metrics::CleanupOutcome::RetainedShared, 0)
        }
        Some(CompletedBlobCleanup::Deleted) => (
            aster_drive_metrics::CleanupOutcome::Deleted,
            u64::try_from(current_blob.size).unwrap_or(0),
        ),
    };
    attempt.finish(outcome, bytes);
    completed.is_some()
}

async fn cleanup_claimed_blob_impl<F>(
    state: &PrimaryAppState,
    current_blob: &file_blob::Model,
    driver_for_policy: &mut F,
) -> Option<CompletedBlobCleanup>
where
    F: FnMut(&storage_policy::Model) -> crate::errors::Result<Arc<dyn StorageDriver>>,
{
    if current_blob.is_virtual_empty() {
        return match file_repo::delete_claimed_blob_generation(state.writer_db(), current_blob)
            .await
        {
            Ok(true) => Some(CompletedBlobCleanup::Virtual),
            Ok(false) => None,
            Err(error) => {
                tracing::warn!(
                    blob_id = current_blob.id,
                    "failed to delete virtual-empty blob metadata: {error}"
                );
                None
            }
        };
    }

    let Some(storage_path) = current_blob.storage_path_for_connector() else {
        tracing::warn!(blob_id = current_blob.id, backing = ?current_blob.backing, "stored blob has no connector storage path");
        return None;
    };

    let Some(policy) = state.policy_snapshot().get_policy(current_blob.policy_id) else {
        tracing::warn!(
            blob_id = current_blob.id,
            policy_id = current_blob.policy_id,
            "failed to load storage policy during blob cleanup: policy missing from snapshot"
        );
        return None;
    };

    let driver = match driver_for_policy(&policy) {
        Ok(driver) => driver,
        Err(error) => {
            tracing::warn!(
                blob_id = current_blob.id,
                policy_id = current_blob.policy_id,
                "failed to resolve storage driver during blob cleanup: {error}"
            );
            return None;
        }
    };

    // New publications own unique physical keys. Historical rows can still alias
    // one object across policies: another metadata row, even a zero-ref row that
    // may be adopted, owns the bytes until it is independently retired. Concurrent
    // alias cleanups may conservatively leave an orphan, but never destroy a live
    // owner's content. Do not narrow this to positive committed reference counts.
    match other_blob_owns_same_object(state, current_blob, driver.as_ref(), driver_for_policy).await
    {
        Ok(true) => {
            return match file_repo::delete_claimed_blob_generation(state.writer_db(), current_blob)
                .await
            {
                Ok(true) => Some(CompletedBlobCleanup::RetainedShared),
                Ok(false) => None,
                Err(error) => {
                    tracing::warn!(
                        blob_id = current_blob.id,
                        "failed to retire shared blob metadata: {error}"
                    );
                    None
                }
            };
        }
        Ok(false) => {}
        Err(error) => {
            tracing::warn!(
                blob_id = current_blob.id,
                "cannot establish shared object ownership; retaining bytes: {error}"
            );
            return None;
        }
    }

    if let Err(error) =
        processing::delete_thumbnail_with_driver(state, current_blob, driver.clone()).await
    {
        tracing::warn!(
            blob_id = current_blob.id,
            "failed to delete thumbnail during blob cleanup: {error}"
        );
        // Keep the blob row until every derived object is gone. If we deleted
        // the primary object and row after a thumbnail delete failure, that
        // thumbnail would become an untracked storage object with no normal
        // maintenance path left to retry it.
        return None;
    }

    let object_deleted = match driver.delete(storage_path).await {
        Ok(()) => true,
        Err(error) => match driver.exists(storage_path).await {
            Ok(false) => {
                tracing::warn!(
                    blob_id = current_blob.id,
                    path = %storage_path,
                    "blob delete returned error but object is already absent: {error}"
                );
                true
            }
            Ok(true) => {
                tracing::warn!(
                    blob_id = current_blob.id,
                    path = %storage_path,
                    "failed to delete blob object, keeping blob row for retry: {error}"
                );
                false
            }
            Err(exists_error) => {
                tracing::warn!(
                    blob_id = current_blob.id,
                    path = %storage_path,
                    "failed to delete blob object and verify existence, keeping blob row for retry: delete_error={error}, exists_error={exists_error}"
                );
                false
            }
        },
    };

    if !object_deleted {
        return None;
    }

    match file_repo::delete_claimed_blob_generation(state.writer_db(), current_blob).await {
        Ok(true) => Some(CompletedBlobCleanup::Deleted),
        Ok(false) => {
            tracing::warn!(
                blob_id = current_blob.id,
                "blob object is gone but cleanup claim was lost before deleting blob row"
            );
            None
        }
        Err(error) => {
            tracing::warn!(
                blob_id = current_blob.id,
                "blob object is gone but failed to delete blob row: {error}"
            );
            None
        }
    }
}

async fn other_blob_owns_same_object<F>(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    driver: &dyn StorageDriver,
    driver_for_policy: &mut F,
) -> crate::errors::Result<bool>
where
    F: FnMut(&storage_policy::Model) -> crate::errors::Result<Arc<dyn StorageDriver>>,
{
    let Some(path) = blob.storage_path_for_connector() else {
        return Ok(false);
    };
    let others =
        file_repo::find_other_blobs_at_storage_path(state.writer_db(), path, blob.id).await?;
    // Normal UUID keys have no aliases: avoid any path/provider probing then.
    // For exceptionally shared historical keys, retain bytes rather than load
    // unbounded policy metadata just to authorize a destructive operation.
    match others.len() {
        0 => return Ok(false),
        129.. => return Ok(true),
        _ => {}
    }
    let local_path = driver
        .extensions()
        .local_path
        .map(|local| local.resolve_local_path(path))
        .transpose()?;
    for other in others {
        let policy =
            crate::db::repository::policy_repo::find_by_id(state.writer_db(), other.policy_id)
                .await?;
        let other_driver = driver_for_policy(&policy)?;
        match (&local_path, other_driver.extensions().local_path) {
            (Some(path), Some(local)) => {
                if *path
                    == local.resolve_local_path(
                        other.storage_path_for_connector().ok_or_else(|| {
                            AsterError::internal_error("stored alias has no path")
                        })?,
                    )?
                {
                    return Ok(true);
                }
            }
            // A relative key without provider-specific physical identity is
            // conservatively shared. It must not authorize destructive cleanup.
            _ => return Ok(true),
        }
    }
    Ok(false)
}
