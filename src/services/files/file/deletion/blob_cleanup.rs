use crate::db::repository::file_repo;
use crate::entities::{file_blob, storage_policy};
use crate::errors::AsterError;
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::media::processing;
use crate::storage::StorageDriver;
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
        tracing::debug!(
            blob_id = current_blob.id,
            "skipping blob cleanup because cleanup is already claimed"
        );
        return false;
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

async fn cleanup_claimed_blob<F>(
    state: &PrimaryAppState,
    current_blob: &file_blob::Model,
    driver_for_policy: &mut F,
) -> bool
where
    F: FnMut(&storage_policy::Model) -> crate::errors::Result<Arc<dyn StorageDriver>>,
{
    async fn restore_cleanup_claim(state: &PrimaryAppState, blob_id: i64, reason: &str) {
        match file_repo::restore_blob_cleanup_claim(state.writer_db(), blob_id).await {
            Ok(true) => {}
            Ok(false) => {
                tracing::warn!(
                    blob_id,
                    "blob cleanup claim was already released while handling {reason}"
                );
            }
            Err(error) => {
                tracing::warn!(
                    blob_id,
                    "failed to restore blob cleanup claim after {reason}: {error}"
                );
            }
        }
    }

    let Some(policy) = state.policy_snapshot().get_policy(current_blob.policy_id) else {
        tracing::warn!(
            blob_id = current_blob.id,
            policy_id = current_blob.policy_id,
            "failed to load storage policy during blob cleanup: policy missing from snapshot"
        );
        restore_cleanup_claim(state, current_blob.id, "policy lookup failure").await;
        return false;
    };

    let driver = match driver_for_policy(&policy) {
        Ok(driver) => driver,
        Err(error) => {
            tracing::warn!(
                blob_id = current_blob.id,
                policy_id = current_blob.policy_id,
                "failed to resolve storage driver during blob cleanup: {error}"
            );
            restore_cleanup_claim(state, current_blob.id, "driver resolution failure").await;
            return false;
        }
    };

    if let Err(error) =
        processing::delete_thumbnail_with_driver(state, current_blob, driver.clone())
            .await
    {
        tracing::warn!(
            blob_id = current_blob.id,
            "failed to delete thumbnail during blob cleanup: {error}"
        );
        // Keep the blob row until every derived object is gone. If we deleted
        // the primary object and row after a thumbnail delete failure, that
        // thumbnail would become an untracked storage object with no normal
        // maintenance path left to retry it.
        restore_cleanup_claim(state, current_blob.id, "thumbnail delete error").await;
        return false;
    }

    let object_deleted = match driver.delete(&current_blob.storage_path).await {
        Ok(()) => true,
        Err(error) => match driver.exists(&current_blob.storage_path).await {
            Ok(false) => {
                tracing::warn!(
                    blob_id = current_blob.id,
                    path = %current_blob.storage_path,
                    "blob delete returned error but object is already absent: {error}"
                );
                true
            }
            Ok(true) => {
                tracing::warn!(
                    blob_id = current_blob.id,
                    path = %current_blob.storage_path,
                    "failed to delete blob object, keeping blob row for retry: {error}"
                );
                restore_cleanup_claim(state, current_blob.id, "delete error").await;
                false
            }
            Err(exists_error) => {
                tracing::warn!(
                    blob_id = current_blob.id,
                    path = %current_blob.storage_path,
                    "failed to delete blob object and verify existence, keeping blob row for retry: delete_error={error}, exists_error={exists_error}"
                );
                restore_cleanup_claim(state, current_blob.id, "delete verification error").await;
                false
            }
        },
    };

    if !object_deleted {
        return false;
    }

    match file_repo::delete_blob_if_cleanup_claimed(state.writer_db(), current_blob.id).await {
        Ok(true) => true,
        Ok(false) => {
            tracing::warn!(
                blob_id = current_blob.id,
                "blob object is gone but cleanup claim was lost before deleting blob row"
            );
            restore_cleanup_claim(
                state,
                current_blob.id,
                "lost cleanup claim before row delete",
            )
            .await;
            false
        }
        Err(error) => {
            tracing::warn!(
                blob_id = current_blob.id,
                "blob object is gone but failed to delete blob row: {error}"
            );
            restore_cleanup_claim(state, current_blob.id, "row delete failure").await;
            false
        }
    }
}
