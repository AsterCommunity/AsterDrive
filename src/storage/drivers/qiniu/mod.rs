//! Provider evidence: developer-docs/zh-CN/reference/storage-provider-evidence.md#qiniu
//! Kodo native wire contracts: qiniu/go-sdk f278ca5c9cc5d4a85f304e4b73f2b0b36437c66e.
//! See auth/credentials.go and storagev2/internal/api-specs/ at that revision.
mod auth;
pub(crate) mod cors;
mod download;
mod http;
mod multipart;
mod objects;
pub(crate) mod resumable;
#[cfg(test)]
pub(crate) mod tests;
pub(crate) mod upgrade;
mod upload;

use aster_drive_storage::{Result, StorageErrorKind, storage_driver_error};
pub use download::{QiniuDownloadDelivery, default_download_auth_validity_seconds};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QiniuDriverConfig {
    pub bucket: String,
    pub base_path: String,
    pub download_origin: String,
    pub download_delivery: QiniuDownloadDelivery,
    pub download_base_url: Option<String>,
    pub download_auth_validity_seconds: u32,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub operation_timeout: Duration,
}

#[derive(Clone, PartialEq, Eq)]
pub struct QiniuStaticCredentials {
    pub access_key: String,
    pub secret_key: String,
}

impl std::fmt::Debug for QiniuStaticCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QiniuStaticCredentials")
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct QiniuDriver {
    config: QiniuDriverConfig,
    credentials: QiniuStaticCredentials,
    /// Reused for bounded control exchanges: discovery, metadata, management,
    /// multipart lifecycle operations and the small migration identity probe.
    /// operation_timeout covers connection, headers and the entire response body.
    control_client: aster_forge_http::HttpClient,
    /// Reused for GET/Range bodies without a whole-transfer deadline. reqwest's
    /// read timeout bounds waiting for headers and idle reads of the response.
    download_client: aster_forge_http::HttpClient,
    /// Reused for form/part uploads. reqwest's read_timeout would also limit time
    /// spent uploading before response headers arrive, so send_upload implements
    /// progress-aware idle detection and bounds the small acknowledgement instead.
    upload_client: aster_forge_http::HttpClient,
    region_query: Url,
    /// Bucket configuration uses UC, independently of upload/RS/download hosts.
    bucket_api: Url,
    endpoints: std::sync::Arc<Mutex<Option<Endpoints>>>,
}

struct Endpoints {
    upload: Url,
    management: Url,
    list: Url,
    expires_at: Instant,
}

#[derive(Serialize, Deserialize)]
struct MultipartSession {
    upload_id: String,
    expires_at: i64,
    key: String,
    bucket: String,
}

impl QiniuDriver {
    pub fn validate_config(
        config: &QiniuDriverConfig,
        credentials: &QiniuStaticCredentials,
    ) -> Result<()> {
        if config.bucket.trim().is_empty()
            || config.bucket.contains([':', '/', '\\'])
            || config.bucket.chars().any(char::is_control)
        {
            return Err(invalid(
                "confirm the native Kodo bucket name; a legacy S3 space name is not a native bucket mapping",
            ));
        }
        validate_key(config.base_path.trim_matches('/'), true)?;
        download::validate_domain(&config.download_origin)?;
        if !(1..=3600).contains(&config.download_auth_validity_seconds) {
            return Err(invalid(
                "Kodo download authentication validity must be between 1 and 3600 seconds",
            ));
        }
        match config.download_delivery {
            QiniuDownloadDelivery::Origin => {}
            QiniuDownloadDelivery::QiniuCdn => {
                download::validate_domain(config.download_base_url.as_deref().unwrap_or_default())?;
            }
        }
        if credentials.access_key.trim().is_empty()
            || credentials.secret_key.trim().is_empty()
            || credentials.access_key.contains([':', '\r', '\n'])
        {
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "Qiniu AccessKey and SecretKey are required",
            ));
        }
        Ok(())
    }

    pub fn new(mut config: QiniuDriverConfig, credentials: QiniuStaticCredentials) -> Result<Self> {
        Self::validate_config(&config, &credentials)?;
        config.base_path = config.base_path.trim_matches('/').to_string();
        // DriverRegistry caches this driver by policy; reqwest clones share each
        // client's connection pool. Build these clients once, never per request.
        // All also reject redirects so signed URLs/UpTokens cannot follow them.
        let control_client =
            aster_drive_outbound::ClientConfig::new(aster_drive_outbound::Profile::ProviderControl)
                .connect_timeout(config.connect_timeout)
                .read_timeout(config.read_timeout)
                .timeout(config.operation_timeout)
                .build()
                .map_err(http::transport_error)?;
        let download_client = aster_drive_outbound::ClientConfig::new(
            aster_drive_outbound::Profile::ProviderDownload,
        )
        .connect_timeout(config.connect_timeout)
        .read_timeout(config.read_timeout)
        .build()
        .map_err(http::transport_error)?;
        // Upload progress/acknowledgement watchdogs remain in send_upload.
        let upload_client =
            aster_drive_outbound::ClientConfig::new(aster_drive_outbound::Profile::ProviderUpload)
                .connect_timeout(config.connect_timeout)
                .build()
                .map_err(http::transport_error)?;
        Ok(Self {
            config,
            credentials,
            control_client,
            download_client,
            upload_client,
            region_query: Url::parse("https://uc.qbox.me/v4/query")
                .map_err(|_| invalid("invalid region query URL"))?,
            bucket_api: Url::parse("https://uc.qiniuapi.com")
                .map_err(|_| invalid("invalid bucket configuration URL"))?,
            endpoints: std::sync::Arc::new(Mutex::new(None)),
        })
    }

    fn key(&self, path: &str) -> Result<String> {
        validate_key(path, false)?;
        Ok(aster_drive_storage::object_key::join_key_prefix(
            &self.config.base_path,
            path,
        ))
    }

    fn entry(&self, path: &str) -> Result<String> {
        Ok(auth::encode(format!(
            "{}:{}",
            self.config.bucket,
            self.key(path)?
        )))
    }
}

fn invalid(message: &'static str) -> aster_drive_storage::StorageError {
    storage_driver_error(StorageErrorKind::Misconfigured, message)
}

fn validate_key(key: &str, allow_empty: bool) -> Result<()> {
    if (!allow_empty && key.is_empty())
        || key.starts_with('/')
        || key.contains('\\')
        || key.chars().any(char::is_control)
        || key.split('/').any(|s| matches!(s, "." | ".."))
    {
        return Err(invalid(
            "Kodo keys must be relative with no dot segments, backslashes or control characters",
        ));
    }
    Ok(())
}
