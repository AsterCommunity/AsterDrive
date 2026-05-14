use crate::db::repository::file_repo;
use crate::entities::file_blob;
use crate::errors::AsterError;
use crate::runtime::PrimaryAppState;
use crate::services::media_processing_service;

pub(crate) async fn ensure_blob_cleanup_if_unreferenced(
    state: &PrimaryAppState,
    blob_id: i64,
) -> bool {
    let current_blob = match file_repo::find_blob_by_id(&state.db, blob_id).await {
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

    match file_repo::claim_blob_cleanup(&state.db, current_blob.id).await {
        Ok(true) => cleanup_claimed_blob(state, &current_blob).await,
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
    let current_blob = match file_repo::find_blob_by_id(&state.db, blob.id).await {
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

    match file_repo::claim_blob_cleanup(&state.db, current_blob.id).await {
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

    cleanup_claimed_blob(state, &current_blob).await
}

async fn cleanup_claimed_blob(state: &PrimaryAppState, current_blob: &file_blob::Model) -> bool {
    async fn restore_cleanup_claim(state: &PrimaryAppState, blob_id: i64, reason: &str) {
        match file_repo::restore_blob_cleanup_claim(&state.db, blob_id).await {
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

    if let Err(error) = media_processing_service::delete_thumbnail(state, current_blob).await {
        tracing::warn!(
            blob_id = current_blob.id,
            "failed to delete thumbnail during blob cleanup: {error}"
        );
    }

    let Some(policy) = state.policy_snapshot.get_policy(current_blob.policy_id) else {
        tracing::warn!(
            blob_id = current_blob.id,
            policy_id = current_blob.policy_id,
            "failed to load storage policy during blob cleanup: policy missing from snapshot"
        );
        restore_cleanup_claim(state, current_blob.id, "policy lookup failure").await;
        return false;
    };

    let driver = match state.driver_registry.get_driver(&policy) {
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

    match file_repo::delete_blob_if_cleanup_claimed(&state.db, current_blob.id).await {
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
