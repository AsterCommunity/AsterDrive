use crate::entities::storage_policy;
use crate::types::{
    DriverType, RemoteUploadStrategy, S3UploadStrategy, UploadMode,
    effective_s3_multipart_chunk_size, parse_storage_policy_options,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PolicyUploadTransport {
    Local,
    S3(S3UploadStrategy),
    Remote(RemoteUploadStrategy),
    GoogleDrive,
}

impl PolicyUploadTransport {
    pub(crate) fn effective_chunk_size(self, policy: &storage_policy::Model) -> i64 {
        match self {
            Self::S3(_) => effective_s3_multipart_chunk_size(policy.chunk_size),
            Self::Local | Self::Remote(_) | Self::GoogleDrive => policy.chunk_size,
        }
    }

    pub(crate) fn resolve_init_mode(
        self,
        policy: &storage_policy::Model,
        total_size: i64,
    ) -> UploadMode {
        let fits_single_request = self.fits_single_request(policy, total_size);
        match (self, fits_single_request) {
            (Self::S3(S3UploadStrategy::Presigned), true)
            | (Self::Remote(RemoteUploadStrategy::Presigned), true) => UploadMode::Presigned,
            (Self::S3(S3UploadStrategy::Presigned), false)
            | (Self::Remote(RemoteUploadStrategy::Presigned), false) => {
                UploadMode::PresignedMultipart
            }
            (_, true) => UploadMode::Direct,
            (_, false) => UploadMode::Chunked,
        }
    }

    pub(crate) fn supports_streaming_direct_upload(
        self,
        policy: &storage_policy::Model,
        declared_size: i64,
    ) -> bool {
        if declared_size <= 0 {
            return false;
        }

        match self {
            Self::Local => false,
            Self::S3(S3UploadStrategy::RelayStream) => {
                self.fits_single_request(policy, declared_size)
            }
            Self::S3(S3UploadStrategy::Presigned) => false,
            // `/files/upload` 和 WebDAV 这类服务端持有字节流的入口，
            // 允许继续直接 relay 到 remote provider；init 协商是否改走 presigned
            // 则由 `resolve_init_mode` 单独决定。
            Self::Remote(RemoteUploadStrategy::RelayStream)
            | Self::Remote(RemoteUploadStrategy::Presigned) => true,
            Self::GoogleDrive => true,
        }
    }

    pub(crate) fn uses_relay_multipart_tracking(self) -> bool {
        matches!(
            self,
            Self::S3(S3UploadStrategy::RelayStream)
                | Self::Remote(RemoteUploadStrategy::RelayStream)
                | Self::GoogleDrive
        )
    }

    fn fits_single_request(self, policy: &storage_policy::Model, total_size: i64) -> bool {
        let chunk_size = self.effective_chunk_size(policy);
        chunk_size == 0 || total_size <= chunk_size
    }
}

pub(crate) fn resolve_policy_upload_transport(
    policy: &storage_policy::Model,
) -> PolicyUploadTransport {
    let options = parse_storage_policy_options(policy.options.as_ref());
    match policy.driver_type {
        DriverType::Local => PolicyUploadTransport::Local,
        DriverType::S3 | DriverType::TencentCos => {
            PolicyUploadTransport::S3(options.effective_s3_upload_strategy())
        }
        DriverType::Remote => {
            PolicyUploadTransport::Remote(options.effective_remote_upload_strategy())
        }
        DriverType::GoogleDrive => PolicyUploadTransport::GoogleDrive,
    }
}

pub(crate) fn streaming_direct_upload_eligible(
    policy: &storage_policy::Model,
    declared_size: i64,
) -> bool {
    resolve_policy_upload_transport(policy).supports_streaming_direct_upload(policy, declared_size)
}

#[cfg(test)]
mod tests {
    use super::{PolicyUploadTransport, resolve_policy_upload_transport};
    use crate::entities::storage_policy;
    use crate::types::{
        DriverType, RemoteUploadStrategy, S3UploadStrategy, StoredStoragePolicyAllowedTypes,
        UploadMode, parse_storage_policy_options,
    };
    use chrono::Utc;

    fn mock_policy(
        driver_type: DriverType,
        chunk_size: i64,
        options: &str,
    ) -> storage_policy::Model {
        storage_policy::Model {
            id: 1,
            name: "test".to_string(),
            driver_type,
            endpoint: String::new(),
            bucket: String::new(),
            access_key: String::new(),
            secret_key: String::new(),
            base_path: String::new(),
            remote_node_id: None,
            max_file_size: 0,
            allowed_types: StoredStoragePolicyAllowedTypes::empty(),
            options: options.to_string().into(),
            is_default: false,
            chunk_size,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn local_policy_resolves_direct_and_chunked_modes() {
        let policy = mock_policy(DriverType::Local, 1024, "{}");
        let transport = resolve_policy_upload_transport(&policy);

        assert_eq!(transport, PolicyUploadTransport::Local);
        assert_eq!(
            transport.resolve_init_mode(&policy, 100),
            UploadMode::Direct
        );
        assert_eq!(
            transport.resolve_init_mode(&policy, 2048),
            UploadMode::Chunked
        );
        assert!(!transport.supports_streaming_direct_upload(&policy, 100));
        assert!(!transport.uses_relay_multipart_tracking());
    }

    #[test]
    fn s3_relay_stream_uses_effective_chunk_size_and_relay_tracking() {
        let policy = mock_policy(
            DriverType::S3,
            1_048_576,
            r#"{"s3_upload_strategy":"relay_stream"}"#,
        );
        let transport = resolve_policy_upload_transport(&policy);

        assert_eq!(
            transport,
            PolicyUploadTransport::S3(S3UploadStrategy::RelayStream)
        );
        assert_eq!(transport.effective_chunk_size(&policy), 5_242_880);
        assert_eq!(
            transport.resolve_init_mode(&policy, 5_242_880),
            UploadMode::Direct
        );
        assert_eq!(
            transport.resolve_init_mode(&policy, 5_242_881),
            UploadMode::Chunked
        );
        assert!(transport.supports_streaming_direct_upload(&policy, 1024));
        assert!(!transport.supports_streaming_direct_upload(&policy, 5_242_881));
        assert!(transport.uses_relay_multipart_tracking());
    }

    #[test]
    fn s3_presigned_uses_presigned_modes() {
        let policy = mock_policy(
            DriverType::S3,
            1024,
            r#"{"s3_upload_strategy":"presigned"}"#,
        );
        let transport = resolve_policy_upload_transport(&policy);

        assert_eq!(
            transport,
            PolicyUploadTransport::S3(S3UploadStrategy::Presigned)
        );
        assert_eq!(
            transport.resolve_init_mode(&policy, 5_242_880),
            UploadMode::Presigned
        );
        assert_eq!(
            transport.resolve_init_mode(&policy, 5_242_881),
            UploadMode::PresignedMultipart
        );
        assert!(!transport.supports_streaming_direct_upload(&policy, 1024));
        assert!(!transport.uses_relay_multipart_tracking());
    }

    #[test]
    fn tencent_cos_presigned_uses_s3_presigned_modes() {
        let options = parse_storage_policy_options(r#"{"s3_upload_strategy":"presigned"}"#);
        assert_eq!(
            options.effective_s3_upload_strategy(),
            S3UploadStrategy::Presigned
        );
        let policy = mock_policy(
            DriverType::TencentCos,
            1024,
            r#"{"s3_upload_strategy":"presigned"}"#,
        );
        let transport = resolve_policy_upload_transport(&policy);

        assert_eq!(
            transport,
            PolicyUploadTransport::S3(S3UploadStrategy::Presigned)
        );
        assert_eq!(
            transport.resolve_init_mode(&policy, 5_242_880),
            UploadMode::Presigned
        );
        assert_eq!(
            transport.resolve_init_mode(&policy, 5_242_881),
            UploadMode::PresignedMultipart
        );
        assert!(!transport.supports_streaming_direct_upload(&policy, 1024));
        assert!(!transport.uses_relay_multipart_tracking());
    }

    #[test]
    fn remote_relay_stream_uses_direct_and_chunked_modes() {
        let policy = mock_policy(
            DriverType::Remote,
            1024,
            r#"{"remote_upload_strategy":"relay_stream"}"#,
        );
        let transport = resolve_policy_upload_transport(&policy);

        assert_eq!(
            transport,
            PolicyUploadTransport::Remote(RemoteUploadStrategy::RelayStream)
        );
        assert_eq!(
            transport.resolve_init_mode(&policy, 100),
            UploadMode::Direct
        );
        assert_eq!(
            transport.resolve_init_mode(&policy, 2048),
            UploadMode::Chunked
        );
        assert!(transport.supports_streaming_direct_upload(&policy, 100));
        assert!(transport.uses_relay_multipart_tracking());
    }

    #[test]
    fn remote_presigned_keeps_presigned_init_but_allows_server_streaming_fast_path() {
        let policy = mock_policy(
            DriverType::Remote,
            1024,
            r#"{"remote_upload_strategy":"presigned"}"#,
        );
        let transport = resolve_policy_upload_transport(&policy);

        assert_eq!(
            transport,
            PolicyUploadTransport::Remote(RemoteUploadStrategy::Presigned)
        );
        assert_eq!(
            transport.resolve_init_mode(&policy, 100),
            UploadMode::Presigned
        );
        assert_eq!(
            transport.resolve_init_mode(&policy, 2048),
            UploadMode::PresignedMultipart
        );
        assert!(transport.supports_streaming_direct_upload(&policy, 100));
        assert!(!transport.uses_relay_multipart_tracking());
    }
}
