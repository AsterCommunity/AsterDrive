use crate::errors::Result;
use crate::services::workspace::storage::{
    PreparedNonDedupBlobUpload, cleanup_preuploaded_blob_upload,
};

#[derive(Debug)]
pub(super) struct VerifiedPreuploadedNondedupStoreBlob {
    size: i64,
    policy_id: i64,
    storage_path: String,
    prepared: PreparedNonDedupBlobUpload,
}

impl VerifiedPreuploadedNondedupStoreBlob {
    pub(super) fn new(
        size: i64,
        policy_id: i64,
        prepared: PreparedNonDedupBlobUpload,
    ) -> Result<Self> {
        prepared.ensure_matches(size, policy_id, "verified preuploaded store blob")?;

        Ok(Self {
            size,
            policy_id,
            storage_path: prepared.storage_path().to_string(),
            prepared,
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

    pub(super) fn prepared(&self) -> &PreparedNonDedupBlobUpload {
        &self.prepared
    }
}

pub(super) async fn cleanup_verified_preuploaded_nondedup_store_blob(
    driver: &dyn crate::storage::StorageDriver,
    verified_blob: &VerifiedPreuploadedNondedupStoreBlob,
    reason: &str,
) {
    cleanup_preuploaded_blob_upload(driver, verified_blob.prepared(), reason).await;
}

#[cfg(test)]
mod tests {
    use super::{
        VerifiedPreuploadedNondedupStoreBlob, cleanup_verified_preuploaded_nondedup_store_blob,
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

    fn opaque_preupload(size: i64, policy_id: i64) -> PreparedNonDedupBlobUpload {
        PreparedNonDedupBlobUpload::Opaque {
            upload_id: "opaque-id".to_string(),
            hash_prefix: "s3",
            storage_path: "files/opaque-id".to_string(),
            size,
            policy_id,
        }
    }

    #[test]
    fn verified_preuploaded_store_blob_carries_size_policy_and_storage_path() {
        let verified = VerifiedPreuploadedNondedupStoreBlob::new(33, 9, opaque_preupload(33, 9))
            .expect("verified preupload should be accepted");

        assert_eq!(verified.size(), 33);
        assert_eq!(verified.policy_id(), 9);
        assert_eq!(verified.storage_path(), "files/opaque-id");
        assert_eq!(verified.prepared().storage_path(), "files/opaque-id");
    }

    #[test]
    fn verified_preuploaded_store_blob_rejects_invalid_contract() {
        let negative = VerifiedPreuploadedNondedupStoreBlob::new(-1, 9, opaque_preupload(33, 9))
            .expect_err("negative size should be rejected");
        assert!(negative.to_string().contains("non-negative"));

        let size_error = VerifiedPreuploadedNondedupStoreBlob::new(34, 9, opaque_preupload(33, 9))
            .expect_err("size mismatch should be rejected");
        assert!(size_error.to_string().contains("size"));

        let policy_error =
            VerifiedPreuploadedNondedupStoreBlob::new(33, 10, opaque_preupload(33, 9))
                .expect_err("policy mismatch should be rejected");
        assert!(policy_error.to_string().contains("policy"));
    }

    #[tokio::test]
    async fn cleanup_verified_preuploaded_store_blob_deletes_opaque_object() {
        let verified = VerifiedPreuploadedNondedupStoreBlob::new(33, 9, opaque_preupload(33, 9))
            .expect("verified preupload should be accepted");
        let driver = RecordingDeleteDriver::default();

        cleanup_verified_preuploaded_nondedup_store_blob(&driver, &verified, "test cleanup").await;

        let deleted_paths = driver
            .deleted_paths
            .lock()
            .expect("deleted paths lock should not be poisoned")
            .clone();
        assert_eq!(deleted_paths, vec!["files/opaque-id"]);
    }
}
