use super::TempBlobPlan;
use crate::errors::{AsterError, Result};
use crate::services::workspace::storage::PreparedNonDedupBlobUpload;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TempStoreBlobCleanupPlan {
    RollbackStagedDedupIfUnreferenced,
    RetainExistingDedupObject,
    CleanupPreuploadedBlobOnDbFailure,
}

#[derive(Debug, Clone)]
pub(super) enum VerifiedTempStoreBlobSource {
    ContentAddressed {
        file_hash: String,
    },
    PreuploadedNonDedup {
        prepared: PreparedNonDedupBlobUpload,
    },
}

#[derive(Debug, Clone)]
pub(super) struct VerifiedTempStoreBlob {
    size: i64,
    policy_id: i64,
    storage_path: String,
    source: VerifiedTempStoreBlobSource,
    cleanup: TempStoreBlobCleanupPlan,
}

impl VerifiedTempStoreBlob {
    pub(super) fn from_staged_plan(
        blob_plan: &TempBlobPlan,
        size: i64,
        policy_id: i64,
        staged_dedup_object_created: bool,
    ) -> Result<Self> {
        if size < 0 {
            return Err(AsterError::validation_error(format!(
                "verified temp store blob size must be non-negative, got {size}",
            )));
        }

        match blob_plan {
            TempBlobPlan::Dedup(target) => Ok(Self {
                size,
                policy_id,
                storage_path: target.storage_path.clone(),
                source: VerifiedTempStoreBlobSource::ContentAddressed {
                    file_hash: target.file_hash.clone(),
                },
                cleanup: if staged_dedup_object_created {
                    TempStoreBlobCleanupPlan::RollbackStagedDedupIfUnreferenced
                } else {
                    TempStoreBlobCleanupPlan::RetainExistingDedupObject
                },
            }),
            TempBlobPlan::Preuploaded(prepared) => {
                if prepared.size() != size {
                    return Err(AsterError::validation_error(format!(
                        "preuploaded blob size {} does not match verified temp store size {size}",
                        prepared.size(),
                    )));
                }
                if prepared.policy_id() != policy_id {
                    return Err(AsterError::validation_error(format!(
                        "preuploaded blob policy {} does not match verified temp store policy {policy_id}",
                        prepared.policy_id(),
                    )));
                }
                Ok(Self {
                    size,
                    policy_id,
                    storage_path: prepared.storage_path().to_string(),
                    source: VerifiedTempStoreBlobSource::PreuploadedNonDedup {
                        prepared: prepared.clone(),
                    },
                    cleanup: TempStoreBlobCleanupPlan::CleanupPreuploadedBlobOnDbFailure,
                })
            }
        }
    }

    pub(super) fn size(&self) -> i64 {
        self.size
    }

    pub(super) fn policy_id(&self) -> i64 {
        self.policy_id
    }

    pub(super) fn storage_path(&self) -> &str {
        &self.storage_path
    }

    pub(super) fn source(&self) -> &VerifiedTempStoreBlobSource {
        &self.source
    }

    pub(super) fn cleanup(&self) -> &TempStoreBlobCleanupPlan {
        &self.cleanup
    }
}

#[cfg(test)]
mod tests {
    use super::{TempStoreBlobCleanupPlan, VerifiedTempStoreBlob, VerifiedTempStoreBlobSource};
    use crate::services::workspace::storage::PreparedNonDedupBlobUpload;
    use crate::services::workspace::storage::store::from_temp::{DedupTarget, TempBlobPlan};

    #[test]
    fn staged_dedup_blob_carries_rollback_cleanup_plan() {
        let plan = TempBlobPlan::Dedup(DedupTarget {
            file_hash: "hash".to_string(),
            storage_path: "blobs/hash".to_string(),
        });

        let verified = VerifiedTempStoreBlob::from_staged_plan(&plan, 12, 7, true)
            .expect("verified dedup blob should be accepted");

        assert_eq!(verified.size(), 12);
        assert_eq!(verified.policy_id(), 7);
        assert_eq!(verified.storage_path(), "blobs/hash");
        assert!(matches!(
            verified.source(),
            VerifiedTempStoreBlobSource::ContentAddressed { file_hash }
                if file_hash == "hash"
        ));
        assert_eq!(
            verified.cleanup(),
            &TempStoreBlobCleanupPlan::RollbackStagedDedupIfUnreferenced,
        );
    }

    #[test]
    fn existing_dedup_blob_carries_retain_cleanup_plan() {
        let plan = TempBlobPlan::Dedup(DedupTarget {
            file_hash: "hash".to_string(),
            storage_path: "blobs/hash".to_string(),
        });

        let verified = VerifiedTempStoreBlob::from_staged_plan(&plan, 12, 7, false)
            .expect("verified dedup blob should be accepted");

        assert_eq!(
            verified.cleanup(),
            &TempStoreBlobCleanupPlan::RetainExistingDedupObject,
        );
    }

    #[test]
    fn preuploaded_blob_carries_cleanup_plan_and_validates_size_policy() {
        let plan = TempBlobPlan::Preuploaded(PreparedNonDedupBlobUpload::Opaque {
            upload_id: "opaque-id".to_string(),
            hash_prefix: "s3",
            storage_path: "files/opaque-id".to_string(),
            size: 33,
            policy_id: 9,
        });

        let verified = VerifiedTempStoreBlob::from_staged_plan(&plan, 33, 9, false)
            .expect("verified preuploaded blob should be accepted");

        assert_eq!(verified.size(), 33);
        assert_eq!(verified.policy_id(), 9);
        assert_eq!(verified.storage_path(), "files/opaque-id");
        assert!(matches!(
            verified.source(),
            VerifiedTempStoreBlobSource::PreuploadedNonDedup { .. }
        ));
        assert_eq!(
            verified.cleanup(),
            &TempStoreBlobCleanupPlan::CleanupPreuploadedBlobOnDbFailure,
        );
    }

    #[test]
    fn preuploaded_blob_rejects_mismatched_size_or_policy() {
        let plan = TempBlobPlan::Preuploaded(PreparedNonDedupBlobUpload::Opaque {
            upload_id: "opaque-id".to_string(),
            hash_prefix: "s3",
            storage_path: "files/opaque-id".to_string(),
            size: 33,
            policy_id: 9,
        });

        let size_error = VerifiedTempStoreBlob::from_staged_plan(&plan, 34, 9, false)
            .expect_err("size mismatch should be rejected");
        assert!(size_error.to_string().contains("size"));

        let policy_error = VerifiedTempStoreBlob::from_staged_plan(&plan, 33, 10, false)
            .expect_err("policy mismatch should be rejected");
        assert!(policy_error.to_string().contains("policy"));
    }
}
