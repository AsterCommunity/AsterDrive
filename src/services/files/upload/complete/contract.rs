use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{Result, upload_assembly_error_with_code};
use crate::services::workspace::storage::{self, PreparedNonDedupBlobUpload};
use crate::storage::StorageDriver;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum UploadCleanupPlan {
    DeleteStorageObjectOnDbFailure,
    CleanupPreuploadedBlobOnDbFailure,
    RetainForOrphanBlobGc,
    RetainCompletedMultipartObject,
}

#[derive(Debug, Clone)]
pub(super) enum VerifiedUploadSource {
    ContentAddressed {
        file_hash: String,
    },
    OpaqueObject {
        file_hash: String,
    },
    PreuploadedNonDedup {
        prepared: PreparedNonDedupBlobUpload,
    },
}

#[derive(Debug, Clone)]
pub(super) struct VerifiedUploadedBlob {
    size: i64,
    policy_id: i64,
    storage_path: String,
    source: VerifiedUploadSource,
    cleanup: UploadCleanupPlan,
}

impl VerifiedUploadedBlob {
    pub(super) fn copied_opaque_object(
        size: i64,
        policy_id: i64,
        storage_path: String,
        file_hash: String,
    ) -> Result<Self> {
        Self::new(
            size,
            policy_id,
            storage_path,
            VerifiedUploadSource::OpaqueObject { file_hash },
            UploadCleanupPlan::DeleteStorageObjectOnDbFailure,
        )
    }

    pub(super) fn precommitted_provider_object(
        size: i64,
        policy_id: i64,
        storage_path: String,
        file_hash: String,
    ) -> Result<Self> {
        Self::new(
            size,
            policy_id,
            storage_path,
            VerifiedUploadSource::OpaqueObject { file_hash },
            UploadCleanupPlan::DeleteStorageObjectOnDbFailure,
        )
    }

    pub(super) fn completed_multipart_object(
        size: i64,
        policy_id: i64,
        storage_path: String,
        file_hash: String,
    ) -> Result<Self> {
        Self::new(
            size,
            policy_id,
            storage_path,
            VerifiedUploadSource::OpaqueObject { file_hash },
            UploadCleanupPlan::RetainCompletedMultipartObject,
        )
    }

    pub(super) fn deduplicated_content(
        size: i64,
        policy_id: i64,
        storage_path: String,
        file_hash: String,
    ) -> Result<Self> {
        Self::new(
            size,
            policy_id,
            storage_path,
            VerifiedUploadSource::ContentAddressed { file_hash },
            UploadCleanupPlan::RetainForOrphanBlobGc,
        )
    }

    pub(super) fn preuploaded_non_dedup(prepared: PreparedNonDedupBlobUpload) -> Result<Self> {
        Self::new(
            prepared.size(),
            prepared.policy_id(),
            prepared.storage_path().to_string(),
            VerifiedUploadSource::PreuploadedNonDedup { prepared },
            UploadCleanupPlan::CleanupPreuploadedBlobOnDbFailure,
        )
    }

    fn new(
        size: i64,
        policy_id: i64,
        storage_path: String,
        source: VerifiedUploadSource,
        cleanup: UploadCleanupPlan,
    ) -> Result<Self> {
        if size < 0 {
            return Err(upload_assembly_error_with_code(
                ApiErrorCode::UploadTempObjectSizeMismatch,
                format!("verified upload size must be non-negative, got {size}"),
            ));
        }

        Ok(Self {
            size,
            policy_id,
            storage_path,
            source,
            cleanup,
        })
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

    pub(super) fn source(&self) -> &VerifiedUploadSource {
        &self.source
    }

    pub(super) fn cleanup(&self) -> &UploadCleanupPlan {
        &self.cleanup
    }

    pub(super) fn file_hash(&self) -> Option<&str> {
        match &self.source {
            VerifiedUploadSource::ContentAddressed { file_hash }
            | VerifiedUploadSource::OpaqueObject { file_hash } => Some(file_hash),
            VerifiedUploadSource::PreuploadedNonDedup { .. } => None,
        }
    }
}

pub(super) async fn cleanup_verified_upload_after_db_failure(
    driver: &dyn StorageDriver,
    verified: &VerifiedUploadedBlob,
    reason: &str,
) {
    match verified.cleanup() {
        UploadCleanupPlan::DeleteStorageObjectOnDbFailure => {
            if let Err(error) = driver.delete(verified.storage_path()).await {
                tracing::warn!(
                    storage_path = %verified.storage_path(),
                    "failed to delete verified upload object after {reason}: {error}"
                );
            }
        }
        UploadCleanupPlan::CleanupPreuploadedBlobOnDbFailure => {
            if let VerifiedUploadSource::PreuploadedNonDedup { prepared } = verified.source() {
                storage::cleanup_preuploaded_blob_upload(driver, prepared, reason).await;
            }
        }
        UploadCleanupPlan::RetainForOrphanBlobGc
        | UploadCleanupPlan::RetainCompletedMultipartObject => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{
        UploadCleanupPlan, VerifiedUploadSource, VerifiedUploadedBlob,
        cleanup_verified_upload_after_db_failure,
    };
    use crate::errors::Result;
    use crate::services::workspace::storage::PreparedNonDedupBlobUpload;
    use crate::storage::{BlobMetadata, StorageDriver};
    use async_trait::async_trait;
    use std::sync::Mutex;
    use tokio::io::AsyncRead;

    #[derive(Default)]
    struct RecordingDeleteDriver {
        deleted_paths: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl StorageDriver for RecordingDeleteDriver {
        async fn put(&self, _path: &str, _data: &[u8]) -> Result<String> {
            unreachable!()
        }

        async fn get(&self, _path: &str) -> Result<Vec<u8>> {
            unreachable!()
        }

        async fn get_stream(&self, _path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
            unreachable!()
        }

        async fn delete(&self, path: &str) -> Result<()> {
            self.deleted_paths
                .lock()
                .expect("deleted paths lock should not be poisoned")
                .push(path.to_string());
            Ok(())
        }

        async fn exists(&self, _path: &str) -> Result<bool> {
            unreachable!()
        }

        async fn metadata(&self, _path: &str) -> Result<BlobMetadata> {
            unreachable!()
        }

        async fn copy_object(&self, _src_path: &str, _dest_path: &str) -> Result<String> {
            unreachable!()
        }
    }

    #[test]
    fn copied_opaque_object_carries_db_failure_delete_plan() {
        let verified = VerifiedUploadedBlob::copied_opaque_object(
            12,
            7,
            "files/final".to_string(),
            "s3-upload-1".to_string(),
        )
        .expect("verified object should be accepted");

        assert_eq!(verified.size(), 12);
        assert_eq!(verified.policy_id(), 7);
        assert_eq!(verified.storage_path(), "files/final");
        assert_eq!(verified.file_hash(), Some("s3-upload-1"));
        assert_eq!(
            verified.cleanup(),
            &UploadCleanupPlan::DeleteStorageObjectOnDbFailure
        );
    }

    #[test]
    fn completed_multipart_object_keeps_existing_cleanup_semantics_explicit() {
        let verified = VerifiedUploadedBlob::completed_multipart_object(
            12,
            7,
            "files/multipart".to_string(),
            "s3-upload-1".to_string(),
        )
        .expect("verified object should be accepted");

        assert_eq!(
            verified.cleanup(),
            &UploadCleanupPlan::RetainCompletedMultipartObject
        );
    }

    #[test]
    fn preuploaded_blob_carries_preuploaded_cleanup_plan() {
        let prepared = PreparedNonDedupBlobUpload::Opaque {
            upload_id: "opaque-id".to_string(),
            hash_prefix: "s3",
            storage_path: "files/opaque-id".to_string(),
            size: 33,
            policy_id: 9,
        };

        let verified = VerifiedUploadedBlob::preuploaded_non_dedup(prepared)
            .expect("preuploaded blob should be accepted");

        assert_eq!(verified.size(), 33);
        assert_eq!(verified.policy_id(), 9);
        assert_eq!(verified.storage_path(), "files/opaque-id");
        assert!(matches!(
            verified.source(),
            VerifiedUploadSource::PreuploadedNonDedup { .. }
        ));
        assert_eq!(
            verified.cleanup(),
            &UploadCleanupPlan::CleanupPreuploadedBlobOnDbFailure
        );
    }

    #[test]
    fn verified_blob_rejects_negative_size() {
        let error = VerifiedUploadedBlob::deduplicated_content(
            -1,
            1,
            "blobs/hash".to_string(),
            "hash".to_string(),
        )
        .expect_err("negative verified size should be rejected");

        assert!(error.to_string().contains("non-negative"));
    }

    #[tokio::test]
    async fn cleanup_after_db_failure_deletes_copied_opaque_object() {
        let verified = VerifiedUploadedBlob::copied_opaque_object(
            12,
            7,
            "files/final".to_string(),
            "s3-upload-1".to_string(),
        )
        .expect("verified object should be accepted");
        let driver = RecordingDeleteDriver::default();

        cleanup_verified_upload_after_db_failure(&driver, &verified, "test cleanup").await;

        let deleted_paths = driver
            .deleted_paths
            .lock()
            .expect("deleted paths lock should not be poisoned")
            .clone();
        assert_eq!(deleted_paths, vec!["files/final"]);
    }

    #[tokio::test]
    async fn cleanup_after_db_failure_retains_completed_multipart_object() {
        let verified = VerifiedUploadedBlob::completed_multipart_object(
            12,
            7,
            "files/multipart".to_string(),
            "s3-upload-1".to_string(),
        )
        .expect("verified object should be accepted");
        let driver = RecordingDeleteDriver::default();

        cleanup_verified_upload_after_db_failure(&driver, &verified, "test cleanup").await;

        let deleted_paths = driver
            .deleted_paths
            .lock()
            .expect("deleted paths lock should not be poisoned")
            .clone();
        assert!(deleted_paths.is_empty());
    }
}
