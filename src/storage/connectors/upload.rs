use crate::entities::storage_policy;
use crate::types::{
    RemoteUploadStrategy, S3UploadStrategy, UploadMode, effective_s3_multipart_chunk_size,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageConnectorUploadTransport {
    Local,
    ObjectStorage(S3UploadStrategy),
    Remote(RemoteUploadStrategy),
    OneDrive,
}

impl StorageConnectorUploadTransport {
    pub fn effective_chunk_size(self, policy: &storage_policy::Model) -> i64 {
        match self {
            Self::ObjectStorage(_) => effective_s3_multipart_chunk_size(policy.chunk_size),
            Self::Local | Self::Remote(_) | Self::OneDrive => policy.chunk_size,
        }
    }

    pub fn resolve_init_mode(self, policy: &storage_policy::Model, total_size: i64) -> UploadMode {
        let fits_single_request = self.fits_single_request(policy, total_size);
        match (self, fits_single_request) {
            (Self::ObjectStorage(S3UploadStrategy::Presigned), true)
            | (Self::Remote(RemoteUploadStrategy::Presigned), true) => UploadMode::Presigned,
            (Self::ObjectStorage(S3UploadStrategy::Presigned), false)
            | (Self::Remote(RemoteUploadStrategy::Presigned), false) => {
                UploadMode::PresignedMultipart
            }
            (_, true) => UploadMode::Direct,
            (_, false) => UploadMode::Chunked,
        }
    }

    pub fn supports_streaming_direct_upload(
        self,
        policy: &storage_policy::Model,
        declared_size: i64,
    ) -> bool {
        if declared_size <= 0 {
            return false;
        }

        match self {
            Self::Local => false,
            Self::ObjectStorage(S3UploadStrategy::RelayStream) => {
                self.fits_single_request(policy, declared_size)
            }
            Self::ObjectStorage(S3UploadStrategy::Presigned) => false,
            Self::Remote(RemoteUploadStrategy::RelayStream)
            | Self::Remote(RemoteUploadStrategy::Presigned) => true,
            Self::OneDrive => true,
        }
    }

    pub fn uses_relay_multipart_tracking(self) -> bool {
        matches!(
            self,
            Self::ObjectStorage(S3UploadStrategy::RelayStream)
                | Self::Remote(RemoteUploadStrategy::RelayStream)
        )
    }

    fn fits_single_request(self, policy: &storage_policy::Model, total_size: i64) -> bool {
        let chunk_size = self.effective_chunk_size(policy);
        chunk_size == 0 || total_size <= chunk_size
    }
}
