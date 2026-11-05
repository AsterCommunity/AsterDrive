//! Azure Blob Storage driver.

pub mod cdn;
pub(crate) mod cors;
mod error;
mod multipart;
mod presigned;
mod signing;
mod storage_driver;

use crate::errors::AsterError;
use std::time::Duration;

use azure_core::http::{ClientOptions, RetryOptions, Url};
use azure_storage_blob::{
    BlobClient, BlobClientOptions, BlobContainerClient, BlobContainerClientOptions,
    BlockBlobClient, BlockBlobClientOptions,
};

use aster_drive_model::types::effective_object_multipart_chunk_size;
use aster_drive_storage::object_key;
use aster_drive_storage::{
    MapStorageErr, Result, StorageError, StorageErrorKind, storage_driver_error,
};
use aster_forge_utils::net::is_loopback_host;
use sha2::{Digest, Sha256};

const AZURE_STORAGE_VERSION: &str = "2023-11-03";
const DEFAULT_OPERATION_SAS_TTL: Duration = Duration::from_secs(60 * 60);
const AZURE_BLOCK_BLOB_MAX_BLOCKS: u64 = 50_000;
const AZURE_BLOCK_BLOB_MAX_BLOCK_SIZE: u64 = 4_000 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedAzureBlobConfig {
    pub endpoint: String,
    pub container: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AzureBlobConfigError {
    MissingEndpoint,
    InvalidEndpoint,
    MissingContainer,
}

impl AzureBlobConfigError {
    pub fn into_aster_error(self) -> AsterError {
        self.into_storage_error().into()
    }

    fn into_storage_error(self) -> StorageError {
        match self {
            Self::MissingEndpoint => storage_driver_error(
                StorageErrorKind::Misconfigured,
                "endpoint is required for Azure Blob storage",
            ),
            Self::InvalidEndpoint => storage_driver_error(
                StorageErrorKind::Misconfigured,
                "invalid Azure Blob endpoint URL: expected an absolute HTTP(S) URL without credentials, query, fragment, control characters, or parent path segments; non-loopback endpoints must use HTTPS",
            ),
            Self::MissingContainer => storage_driver_error(
                StorageErrorKind::Misconfigured,
                "container is required for Azure Blob storage",
            ),
        }
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct AzureBlobResponseHeaders<'a> {
    pub cache_control: Option<&'a str>,
    pub content_disposition: Option<&'a str>,
    pub content_type: Option<&'a str>,
}

#[derive(Clone)]
pub struct AzureBlobDriver {
    endpoint: String,
    account_name: String,
    account_key: String,
    container: String,
    base_path: String,
    chunk_size: i64,
    front_door: Option<cdn::FrontDoorConfig>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AzureBlobDriverConfig {
    pub endpoint: String,
    pub container: String,
    pub base_path: String,
    pub chunk_size: i64,
    pub download_delivery: cdn::AzureBlobDownloadDelivery,
    pub download_base_url: String,
    pub azure_cdn_cache_max_age_seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AzureBlobStaticCredentials {
    pub account_name: String,
    pub account_key: String,
}

impl AzureBlobDriver {
    pub fn validate_config(
        config: &AzureBlobDriverConfig,
        credentials: &AzureBlobStaticCredentials,
    ) -> Result<()> {
        Self::normalize_endpoint_and_container(&config.endpoint, &config.container)?;
        cdn::FrontDoorConfig::from_config(config)?;
        if credentials.account_name.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "access_key cannot be empty for Azure Blob storage policies",
            ));
        }
        if credentials.account_key.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "secret_key cannot be empty for Azure Blob storage policies",
            ));
        }
        Ok(())
    }

    pub fn new(
        config: AzureBlobDriverConfig,
        credentials: AzureBlobStaticCredentials,
    ) -> Result<Self> {
        Self::validate_config(&config, &credentials)?;
        let normalized =
            Self::normalize_endpoint_and_container(&config.endpoint, &config.container)?;
        let front_door = cdn::FrontDoorConfig::from_config(&config)?;
        Ok(Self {
            endpoint: normalized.endpoint,
            account_name: credentials.account_name.trim().to_string(),
            account_key: credentials.account_key.trim().to_string(),
            container: normalized.container,
            base_path: config.base_path,
            chunk_size: effective_object_multipart_chunk_size(config.chunk_size),
            front_door,
        })
    }

    pub fn normalize_endpoint_and_container(
        endpoint: &str,
        container: &str,
    ) -> Result<NormalizedAzureBlobConfig> {
        Self::try_normalize_endpoint_and_container(endpoint, container)
            .map_err(AzureBlobConfigError::into_storage_error)
    }

    pub fn try_normalize_endpoint_and_container(
        endpoint: &str,
        container: &str,
    ) -> std::result::Result<NormalizedAzureBlobConfig, AzureBlobConfigError> {
        let endpoint = endpoint.trim().trim_end_matches('/');
        let container = container.trim();
        if endpoint.is_empty() {
            return Err(AzureBlobConfigError::MissingEndpoint);
        }
        if container.is_empty() {
            return Err(AzureBlobConfigError::MissingContainer);
        }

        // HTTP URI parsing preserves the configured path instead of resolving
        // dot segments or repairing browser-style URL spellings.
        let parsed: http::Uri = endpoint
            .parse()
            .map_err(|_| AzureBlobConfigError::InvalidEndpoint)?;
        let https_only =
            parsed.scheme_str() == Some("http") && !parsed.host().is_some_and(is_loopback_host);
        if !matches!(parsed.scheme_str(), Some("http" | "https"))
            || parsed
                .authority()
                .is_none_or(|authority| authority.as_str().contains('@'))
            || https_only
            || parsed.query().is_some()
            || endpoint.contains('#')
            || endpoint.chars().any(char::is_control)
        {
            return Err(AzureBlobConfigError::InvalidEndpoint);
        }
        let decoded_path = percent_encoding::percent_decode_str(parsed.path()).decode_utf8();
        if !matches!(decoded_path, Ok(ref path) if path.split('/').all(|segment| segment != "..")) {
            return Err(AzureBlobConfigError::InvalidEndpoint);
        }

        Ok(NormalizedAzureBlobConfig {
            endpoint: endpoint.to_string(),
            container: container.to_string(),
        })
    }

    fn full_key(&self, path: &str) -> String {
        object_key::join_key_prefix(&self.base_path, path)
    }

    fn relative_key<'a>(&self, key: &'a str) -> Option<&'a str> {
        object_key::strip_key_prefix(&self.base_path, key)
    }

    fn container_url(&self, permissions: &str, expires: Duration) -> Result<Url> {
        Url::parse(&format!(
            "{}/{}?{}",
            self.endpoint,
            percent_encode_path_segment(&self.container),
            self.service_sas_query(
                None,
                permissions,
                expires,
                AzureBlobResponseHeaders::default(),
                Self::sas_protocol(&self.endpoint),
            )?
        ))
        .map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("build Azure Blob container URL: {error}"),
            )
        })
    }

    fn blob_url(
        &self,
        endpoint: &str,
        path: &str,
        permissions: &str,
        expires: Duration,
        response_headers: AzureBlobResponseHeaders<'_>,
    ) -> Result<Url> {
        let key = self.full_key(path);
        Url::parse(&format!(
            "{}/{}/{}?{}",
            endpoint.trim_end_matches('/'),
            percent_encode_path_segment(&self.container),
            percent_encode_blob_path(&key),
            self.service_sas_query(
                Some(&key),
                permissions,
                expires,
                response_headers,
                Self::sas_protocol(endpoint)
            )?
        ))
        .map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("build Azure Blob URL: {error}"),
            )
        })
    }

    fn block_blob_url(&self, path: &str, permissions: &str, expires: Duration) -> Result<Url> {
        self.blob_url(
            &self.endpoint,
            path,
            permissions,
            expires,
            AzureBlobResponseHeaders::default(),
        )
    }

    fn client_options() -> BlobClientOptions {
        BlobClientOptions {
            version: AZURE_STORAGE_VERSION.to_string(),
            ..Default::default()
        }
    }

    fn block_blob_client_options() -> BlockBlobClientOptions {
        BlockBlobClientOptions {
            version: AZURE_STORAGE_VERSION.to_string(),
            ..Default::default()
        }
    }

    fn block_blob_client_options_without_retries() -> BlockBlobClientOptions {
        BlockBlobClientOptions {
            client_options: ClientOptions {
                retry: RetryOptions::none(),
                ..Default::default()
            },
            version: AZURE_STORAGE_VERSION.to_string(),
        }
    }

    fn container_client_options() -> BlobContainerClientOptions {
        BlobContainerClientOptions {
            version: AZURE_STORAGE_VERSION.to_string(),
            ..Default::default()
        }
    }

    fn blob_client(&self, path: &str, permissions: &str) -> Result<BlobClient> {
        BlobClient::new(
            self.blob_url(
                &self.endpoint,
                path,
                permissions,
                DEFAULT_OPERATION_SAS_TTL,
                AzureBlobResponseHeaders::default(),
            )?,
            None,
            Some(Self::client_options()),
        )
        .map_err(|error| Self::rewrap_azure_error("build Azure Blob client", error))
    }

    fn block_blob_client(&self, path: &str, permissions: &str) -> Result<BlockBlobClient> {
        BlockBlobClient::new(
            self.block_blob_url(path, permissions, DEFAULT_OPERATION_SAS_TTL)?,
            None,
            Some(Self::block_blob_client_options()),
        )
        .map_err(|error| Self::rewrap_azure_error("build Azure Blob block client", error))
    }

    fn block_blob_client_without_retries(
        &self,
        path: &str,
        permissions: &str,
    ) -> Result<BlockBlobClient> {
        BlockBlobClient::new(
            self.block_blob_url(path, permissions, DEFAULT_OPERATION_SAS_TTL)?,
            None,
            Some(Self::block_blob_client_options_without_retries()),
        )
        .map_err(|error| Self::rewrap_azure_error("build Azure Blob block client", error))
    }

    fn container_client(&self, permissions: &str) -> Result<BlobContainerClient> {
        BlobContainerClient::new(
            self.container_url(permissions, DEFAULT_OPERATION_SAS_TTL)?,
            None,
            Some(Self::container_client_options()),
        )
        .map_err(|error| Self::rewrap_azure_error("build Azure Blob container client", error))
    }

    fn endpoint_uses_loopback_host(&self) -> bool {
        Url::parse(&self.endpoint)
            .ok()
            .and_then(|url| url.host_str().map(str::to_string))
            .is_some_and(|host| is_loopback_host(&host))
    }

    fn sas_protocol(endpoint: &str) -> &'static str {
        if Url::parse(endpoint).ok().is_some_and(|url| {
            url.scheme() == "http" && url.host_str().is_some_and(is_loopback_host)
        }) {
            "https,http"
        } else {
            "https"
        }
    }

    fn effective_chunk_size(&self) -> i64 {
        self.chunk_size
    }

    fn chunk_size_for_content(&self, content_length: u64) -> Result<usize> {
        let configured = aster_forge_utils::numbers::i64_to_u64(
            self.effective_chunk_size(),
            "Azure Blob configured chunk size",
        )
        .map_storage_err(StorageErrorKind::Misconfigured)?;
        let required = if content_length == 0 {
            1
        } else {
            content_length
                .checked_add(AZURE_BLOCK_BLOB_MAX_BLOCKS - 1)
                .ok_or_else(|| {
                    storage_driver_error(
                        StorageErrorKind::Misconfigured,
                        "Azure Blob upload size is too large",
                    )
                })?
                / AZURE_BLOCK_BLOB_MAX_BLOCKS
        };
        let chunk_size = configured.max(required);
        if chunk_size > AZURE_BLOCK_BLOB_MAX_BLOCK_SIZE {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!(
                    "Azure Blob upload requires block size {chunk_size}, exceeding the 4000 MiB service limit"
                ),
            ));
        }
        aster_forge_utils::numbers::u64_to_usize(chunk_size, "Azure Blob effective chunk size")
            .map_storage_err(StorageErrorKind::Misconfigured)
    }

    fn block_namespace(upload_id: &str) -> String {
        let digest = Sha256::digest(upload_id.as_bytes());
        format!("aster-{}", &hex::encode(digest)[..16])
    }

    fn block_id(upload_id: &str, part_number: i32) -> Result<Vec<u8>> {
        if part_number <= 0 {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("Azure Blob part_number must be positive: {part_number}"),
            ));
        }
        Ok(format!(
            "{}-part-{part_number:010}",
            Self::block_namespace(upload_id)
        )
        .into_bytes())
    }

    fn block_id_marker(upload_id: &str, part_number: i32) -> Result<String> {
        use base64::Engine as _;
        Ok(base64::engine::general_purpose::STANDARD
            .encode(Self::block_id(upload_id, part_number)?))
    }

    fn map_azure_error(ctx: &str, error: azure_core::Error) -> StorageError {
        let marker = error.to_string();
        let kind = if marker.contains("reader exceeded declared size")
            || marker.contains("reader ended before declared size")
        {
            StorageErrorKind::Precondition
        } else {
            Self::classify_azure_error(&error)
        };
        let detail = Self::format_azure_error(error);
        storage_driver_error(kind, format!("{ctx}: {detail}"))
    }

    fn rewrap_azure_error(ctx: &str, error: azure_core::Error) -> StorageError {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!(
                "{ctx}: {}",
                crate::errors::sanitize_storage_driver_client_message(&error.to_string())
            ),
        )
    }
}

fn percent_encode_path_segment(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string()
}

fn percent_encode_blob_path(value: &str) -> String {
    value
        .split('/')
        .map(percent_encode_path_segment)
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::Duration;

    use azure_core::credentials::Secret;
    use azure_core::hmac::hmac_sha256;
    use chrono::{SecondsFormat, Utc};

    use super::{
        AzureBlobDriver, AzureBlobDriverConfig, AzureBlobResponseHeaders,
        AzureBlobStaticCredentials,
    };
    use aster_drive_model::types::effective_object_multipart_chunk_size;
    use aster_drive_storage::{
        DirectDownloadOptions, DirectDownloadStorageDriver, PresignedUploadStorageDriver,
        StorageDriver,
    };
    use base64::Engine as _;
    use hmac::{Hmac, KeyInit, Mac};

    fn sample_config() -> AzureBlobDriverConfig {
        AzureBlobDriverConfig {
            endpoint: " https://acct.blob.core.windows.net/ ".to_string(),
            container: " photos ".to_string(),
            base_path: "base path".to_string(),
            chunk_size: 1,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn numbered_browser_request_declares_block_identity_without_a_response_header_parser() {
        use aster_drive_storage::{BrowserUploadCompletion, MultipartStorageDriver};
        let driver = AzureBlobDriver::new(sample_config(), sample_credentials()).unwrap();
        let id = driver.create_multipart_upload("object").await.unwrap();
        let request = driver
            .browser_part_request("object", &id, 3, Duration::from_secs(60))
            .await
            .unwrap();
        let Some(BrowserUploadCompletion::Fixed { token }) = request.completion else {
            panic!("Azure must declare its fixed block identity");
        };
        let query = url::Url::parse(&request.url)
            .unwrap()
            .query_pairs()
            .into_owned()
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(query["blockid"], token);
        assert!(request.response_headers.is_empty());
        assert!(request.authorization.is_none());
    }

    fn sample_credentials() -> AzureBlobStaticCredentials {
        AzureBlobStaticCredentials {
            account_name: " account-name ".to_string(),
            account_key: "c2VjcmV0".to_string(),
        }
    }

    #[test]
    fn validates_endpoint_and_container() {
        let normalized = AzureBlobDriver::normalize_endpoint_and_container(
            " https://acct.blob.core.windows.net/ ",
            " photos ",
        )
        .expect("valid Azure Blob config");

        assert_eq!(normalized.endpoint, "https://acct.blob.core.windows.net");
        assert_eq!(normalized.container, "photos");
    }

    #[test]
    fn rejects_invalid_endpoint_container_and_credentials() {
        for (endpoint, container) in [
            ("", "photos"),
            ("acct.blob.core.windows.net", "photos"),
            ("ftp://acct.blob.core.windows.net", "photos"),
            ("https://?", "photos"),
            ("https:///missing-host", "photos"),
            ("https:/missing-authority", "photos"),
            ("https://acct.blob.core.windows.net", ""),
        ] {
            assert!(
                AzureBlobDriver::normalize_endpoint_and_container(endpoint, container).is_err(),
                "expected invalid Azure config for endpoint={endpoint:?} container={container:?}",
            );
        }

        let mut credentials = sample_credentials();
        credentials.account_name.clear();
        assert!(AzureBlobDriver::validate_config(&sample_config(), &credentials).is_err());

        let mut credentials = sample_credentials();
        credentials.account_key = "   ".to_string();
        assert!(AzureBlobDriver::validate_config(&sample_config(), &credentials).is_err());
    }

    #[test]
    fn new_trims_credentials_and_applies_minimum_chunk_size() {
        let driver = AzureBlobDriver::new(sample_config(), sample_credentials())
            .expect("valid Azure driver");

        assert_eq!(driver.endpoint, "https://acct.blob.core.windows.net");
        assert_eq!(driver.container, "photos");
        assert_eq!(driver.account_name, "account-name");
        assert_eq!(driver.chunk_size, effective_object_multipart_chunk_size(1));
    }

    #[test]
    fn block_ids_are_stable_and_orderable() {
        let first = AzureBlobDriver::block_id("upload-a", 12).expect("block id");
        let second = AzureBlobDriver::block_id("upload-a", 13).expect("block id");
        assert!(first < second);
        assert_ne!(
            first,
            AzureBlobDriver::block_id("upload-b", 12).expect("isolated block id")
        );
    }

    #[test]
    fn rejects_non_positive_block_ids() {
        assert!(AzureBlobDriver::block_id_marker("upload", 0).is_err());
        assert!(AzureBlobDriver::block_id_marker("upload", -1).is_err());
    }

    #[test]
    fn rejects_endpoint_userinfo_query_fragment_controls_and_parent_paths() {
        for endpoint in [
            "https://user@acct.blob.core.windows.net",
            "https://user:password@acct.blob.core.windows.net",
            "https://acct.blob.core.windows.net?api-version=2023-11-03",
            "https://acct.blob.core.windows.net/#fragment",
            "https://acct.blob.core.windows.net/../shared",
            "https://acct.blob.core.windows.net/%2e%2e/shared",
            "https://acct.blob.core.windows.net/base/%2E%2E/shared",
            "https://acct.blob.core.windows.net/base\tpath",
            // SAS for non-loopback hosts is always signed `spr=https`, so a plain
            // HTTP endpoint could only fail at request time with a misleading 403.
            "http://storage.internal:10000/devstoreaccount1",
            "http://files.contoso.example",
        ] {
            assert!(
                AzureBlobDriver::normalize_endpoint_and_container(endpoint, "photos").is_err(),
                "expected invalid Azure endpoint: {endpoint:?}",
            );
        }
    }

    #[test]
    fn accepts_official_custom_and_local_endpoint_paths() {
        for (endpoint, expected) in [
            (
                "https://acct.blob.core.windows.net/",
                "https://acct.blob.core.windows.net",
            ),
            (
                " https://files.contoso.example ",
                "https://files.contoso.example",
            ),
            (
                "https://files.contoso.example/base-path",
                "https://files.contoso.example/base-path",
            ),
            (
                "http://127.0.0.1:10000/devstoreaccount1",
                "http://127.0.0.1:10000/devstoreaccount1",
            ),
            (
                "http://localhost:10000/devstoreaccount1",
                "http://localhost:10000/devstoreaccount1",
            ),
        ] {
            let normalized = AzureBlobDriver::normalize_endpoint_and_container(endpoint, "photos")
                .unwrap_or_else(|error| {
                    panic!("expected valid Azure endpoint {endpoint:?}: {error:?}")
                });
            assert_eq!(normalized.endpoint, expected);
            assert_eq!(normalized.container, "photos");
        }
    }

    #[test]
    fn invalid_endpoint_error_does_not_replay_userinfo_or_raw_endpoint() {
        let endpoint = "https://user:password@acct.blob.core.windows.net?sig=leak";
        let error = AzureBlobDriver::normalize_endpoint_and_container(endpoint, "photos")
            .expect_err("endpoint with credentials must fail");
        let message = error.message();
        assert!(message.contains("invalid Azure Blob endpoint URL"));
        assert!(!message.contains("password"));
        assert!(!message.contains("sig=leak"));
        assert!(!message.contains(endpoint));
    }

    /// Recompute the service-SAS signature over the documented 2020-12-06+
    /// string-to-sign layout (signed version 2023-11-03, blob resource) and
    /// compare it with the `sig` query parameter.
    fn assert_signed_service_sas(
        query: &HashMap<String, String>,
        signed_permissions: &str,
        canonical_resource: &str,
        rscc: &str,
        rscd: &str,
        rsct: &str,
    ) {
        let string_to_sign = [
            signed_permissions, // signed permissions
            query.get("st").expect("st should exist").as_str(),
            query.get("se").expect("se should exist").as_str(),
            canonical_resource,
            "",           // signed identifier
            "",           // signed IP
            "https",      // signed protocol
            "2023-11-03", // signed version
            "b",          // signed resource
            "",           // signed snapshot time
            "",           // signed encryption scope
            rscc,         // rscc
            rscd,         // rscd
            "",           // rsce
            "",           // rscl
            rsct,         // rsct
        ]
        .join("\n");
        let expected = hmac_sha256(&string_to_sign, &Secret::new("c2VjcmV0".to_string()))
            .expect("recompute Azure Blob SAS signature");
        assert_eq!(
            query.get("sig").map(String::as_str),
            Some(expected.as_str()),
            "service SAS signature must match the recomputed string-to-sign"
        );
    }

    #[test]
    fn sas_response_headers_are_serialized_and_signed_together() {
        let driver = AzureBlobDriver::new(sample_config(), sample_credentials())
            .expect("valid Azure driver");
        let headers = AzureBlobResponseHeaders {
            cache_control: Some("private, max-age=0"),
            content_disposition: Some("attachment; filename=报告.txt"),
            content_type: Some("text/plain"),
        };
        let url = driver
            .blob_url(
                &driver.endpoint,
                "folder with space/中文+plus.txt",
                "r",
                Duration::from_secs(300),
                headers,
            )
            .expect("download SAS URL");
        let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
        let expiry = query
            .get("se")
            .expect("signed Azure Blob URL should include se");

        assert_eq!(
            query.get("rscc").map(String::as_str),
            Some("private, max-age=0")
        );
        assert_eq!(
            query.get("rscd").map(String::as_str),
            Some("attachment; filename=报告.txt")
        );
        assert_eq!(query.get("rsct").map(String::as_str), Some("text/plain"));
        assert_eq!(query.get("spr").map(String::as_str), Some("https"));
        assert_signed_service_sas(
            &query,
            "r",
            "/blob/account-name/photos/base path/folder with space/中文+plus.txt",
            "private, max-age=0",
            "attachment; filename=报告.txt",
            "text/plain",
        );
        assert_eq!(
            query.get("sp").map(String::as_str),
            Some("r"),
            "download SAS should remain read-only"
        );
        assert!(expiry > &Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true));
        let latest_expiry = (Utc::now() + chrono::Duration::seconds(301))
            .to_rfc3339_opts(SecondsFormat::Secs, true);
        assert!(expiry.as_str() <= latest_expiry.as_str());
    }

    #[test]
    fn sas_urls_without_response_headers_do_not_serialize_override_fields() {
        let driver = AzureBlobDriver::new(sample_config(), sample_credentials())
            .expect("valid Azure driver");
        let url = driver
            .blob_url(
                &driver.endpoint,
                "folder with space/中文+plus.txt",
                "r",
                Duration::from_secs(300),
                AzureBlobResponseHeaders::default(),
            )
            .expect("download SAS URL");
        let query: HashMap<_, _> = url.query_pairs().into_owned().collect();

        assert!(!query.contains_key("rscc"));
        assert!(!query.contains_key("rscd"));
        assert!(!query.contains_key("rsct"));
        assert_signed_service_sas(
            &query,
            "r",
            "/blob/account-name/photos/base path/folder with space/中文+plus.txt",
            "",
            "",
            "",
        );
    }

    #[test]
    fn custom_domain_host_changes_url_but_not_sas_canonical_resource() {
        let mut config = sample_config();
        config.endpoint = "https://files.contoso.example".to_string();
        let driver = AzureBlobDriver::new(config, sample_credentials())
            .expect("valid custom-domain Azure driver");
        let url = driver
            .blob_url(
                &driver.endpoint,
                "folder with space/中文+plus.txt",
                "r",
                Duration::from_secs(300),
                AzureBlobResponseHeaders::default(),
            )
            .expect("custom-domain blob URL");
        let query: HashMap<_, _> = url.query_pairs().into_owned().collect();

        assert_eq!(url.host_str(), Some("files.contoso.example"));
        assert_eq!(
            url.path(),
            "/photos/base%20path/folder%20with%20space/%E4%B8%AD%E6%96%87%2Bplus%2Etxt",
        );
        assert_signed_service_sas(
            &query,
            "r",
            "/blob/account-name/photos/base path/folder with space/中文+plus.txt",
            "",
            "",
            "",
        );
    }

    #[test]
    fn builds_encoded_blob_and_container_sas_urls() {
        let driver = AzureBlobDriver::new(sample_config(), sample_credentials())
            .expect("valid Azure driver");
        let blob_url = driver
            .blob_url(
                &driver.endpoint,
                "folder with space/中文+plus.txt",
                "rw",
                Duration::from_secs(300),
                AzureBlobResponseHeaders::default(),
            )
            .expect("blob URL");

        assert_eq!(blob_url.scheme(), "https");
        assert_eq!(blob_url.host_str(), Some("acct.blob.core.windows.net"));
        assert_eq!(
            blob_url.path(),
            "/photos/base%20path/folder%20with%20space/%E4%B8%AD%E6%96%87%2Bplus%2Etxt",
        );
        let query: HashMap<_, _> = blob_url.query_pairs().into_owned().collect();
        assert_eq!(query.get("sv").map(String::as_str), Some("2023-11-03"));
        assert_eq!(query.get("spr").map(String::as_str), Some("https"));
        assert_eq!(query.get("sr").map(String::as_str), Some("b"));
        assert_eq!(query.get("sp").map(String::as_str), Some("rw"));
        assert!(query.get("sig").is_some_and(|value| !value.is_empty()));

        let container_url = driver
            .container_url("rl", Duration::from_secs(300))
            .expect("container URL");
        let query: HashMap<_, _> = container_url.query_pairs().into_owned().collect();
        assert_eq!(container_url.path(), "/photos");
        assert_eq!(query.get("sr").map(String::as_str), Some("c"));
        assert_eq!(query.get("sp").map(String::as_str), Some("rl"));
    }

    #[test]
    fn local_azurite_sas_urls_allow_http() {
        let mut config = sample_config();
        config.endpoint = "http://127.0.0.1:10000/devstoreaccount1".to_string();
        let mut credentials = sample_credentials();
        credentials.account_name = "devstoreaccount1".to_string();
        let driver = AzureBlobDriver::new(config, credentials).expect("valid Azurite driver");
        let blob_url = driver
            .blob_url(
                &driver.endpoint,
                "local.bin",
                "cw",
                Duration::from_secs(300),
                AzureBlobResponseHeaders::default(),
            )
            .expect("blob URL");
        let query: HashMap<_, _> = blob_url.query_pairs().into_owned().collect();

        assert_eq!(query.get("spr").map(String::as_str), Some("https,http"));
    }

    #[tokio::test]
    async fn download_sas_signs_response_overrides_without_changing_the_object_path() {
        let driver = AzureBlobDriver::new(sample_config(), sample_credentials())
            .expect("valid Azure driver");
        let filename = "报告 100%+计划.pdf";
        let disposition =
            crate::services::files::file::DownloadDisposition::Attachment.header_value(filename);
        let cache_control = "private, max-age=0, must-revalidate";
        let content_type = "application/pdf";
        let download = driver
            .resolve_download_url(
                "files/opaque-id",
                Duration::from_secs(300),
                DirectDownloadOptions {
                    download_name: Some(filename.to_string()),
                    response_cache_control: Some(cache_control.to_string()),
                    response_content_disposition: Some(disposition.clone()),
                    response_content_type: Some(content_type.to_string()),
                    ..Default::default()
                },
            )
            .await
            .expect("download SAS")
            .expect("direct download");
        let url = url::Url::parse(&download.url).expect("download URL");
        assert_eq!(url.path(), "/photos/base%20path/files/opaque%2Did");
        let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(query.get("rscd"), Some(&disposition));
        assert_eq!(query.get("rsct").map(String::as_str), Some(content_type));
        assert_eq!(query.get("rscc").map(String::as_str), Some(cache_control));
        assert_eq!(query.get("sp").map(String::as_str), Some("r"));

        // Build the 2020-12-06+ protocol vector independently of the URL builder.
        // SAS signs decoded header values, including literal RFC 5987 percent escapes.
        let string_to_sign = format!(
            "r\n{}\n{}\n/blob/account-name/photos/base path/files/opaque-id\n\n\nhttps\n2023-11-03\nb\n\n\n{cache_control}\n{disposition}\n\n\n{content_type}",
            query["st"], query["se"],
        );
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(b"secret").expect("fixture key");
        mac.update(string_to_sign.as_bytes());
        let signature =
            base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());
        assert_eq!(query.get("sig"), Some(&signature));
    }

    #[tokio::test]
    async fn download_sas_defaults_and_other_operations_omit_response_overrides() {
        let driver = AzureBlobDriver::new(sample_config(), sample_credentials())
            .expect("valid Azure driver");
        let download = driver
            .resolve_download_url(
                "files/opaque-id",
                Duration::from_secs(300),
                DirectDownloadOptions::default(),
            )
            .await
            .expect("download SAS")
            .expect("direct download");
        let upload = driver
            .presigned_put_request("files/opaque-id", Duration::from_secs(300))
            .await
            .expect("upload SAS")
            .expect("presigned upload");
        let container = driver
            .container_url("rl", Duration::from_secs(300))
            .expect("container SAS");
        let block_blob = driver
            .block_blob_url("files/opaque-id", "cw", Duration::from_secs(300))
            .expect("block blob SAS");
        for url in [
            download.url,
            upload.url,
            container.to_string(),
            block_blob.to_string(),
        ] {
            let url = url::Url::parse(&url).expect("SAS URL");
            let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
            for field in ["rscc", "rscd", "rsce", "rscl", "rsct"] {
                assert!(
                    !query.contains_key(field),
                    "unexpected response override {field}"
                );
            }
        }
    }

    #[test]
    fn chunk_size_respects_configured_minimum_and_azure_block_limits() {
        let driver = AzureBlobDriver::new(sample_config(), sample_credentials())
            .expect("valid Azure driver");
        let configured = effective_object_multipart_chunk_size(1);
        let configured_u64 = u64::try_from(configured).expect("configured chunk size");

        assert_eq!(
            driver
                .chunk_size_for_content(0)
                .expect("zero-byte chunk size"),
            usize::try_from(configured).expect("configured chunk size usize"),
        );
        assert_eq!(
            driver
                .chunk_size_for_content(super::AZURE_BLOCK_BLOB_MAX_BLOCKS * configured_u64)
                .expect("exactly fits configured size"),
            usize::try_from(configured).expect("configured chunk size usize"),
        );
        assert_eq!(
            driver
                .chunk_size_for_content(super::AZURE_BLOCK_BLOB_MAX_BLOCKS * configured_u64 + 1)
                .expect("requires one more byte per block"),
            usize::try_from(configured + 1).expect("raised chunk size usize"),
        );

        let service_limit = super::AZURE_BLOCK_BLOB_MAX_BLOCKS
            .checked_mul(super::AZURE_BLOCK_BLOB_MAX_BLOCK_SIZE)
            .expect("test size within u64");
        assert_eq!(
            driver
                .chunk_size_for_content(service_limit)
                .expect("service limit should be accepted"),
            usize::try_from(super::AZURE_BLOCK_BLOB_MAX_BLOCK_SIZE)
                .expect("service block size usize"),
        );
        assert!(
            driver.chunk_size_for_content(service_limit + 1).is_err(),
            "one byte beyond 50,000 maximum-size blocks must be rejected"
        );
        assert!(
            driver.chunk_size_for_content(u64::MAX).is_err(),
            "content-length arithmetic overflow must be rejected"
        );
        let multipart = driver
            .extensions()
            .multipart
            .expect("Azure multipart capability");
        assert_eq!(
            multipart.capabilities().max_parts,
            super::AZURE_BLOCK_BLOB_MAX_BLOCKS
        );
        assert_eq!(
            multipart.capabilities().max_part_size,
            Some(super::AZURE_BLOCK_BLOB_MAX_BLOCK_SIZE)
        );

        let oversized_config = AzureBlobDriver::new(
            AzureBlobDriverConfig {
                chunk_size: i64::try_from(super::AZURE_BLOCK_BLOB_MAX_BLOCK_SIZE + 1)
                    .expect("oversized config fits i64"),
                ..sample_config()
            },
            sample_credentials(),
        )
        .expect("driver should retain oversized config for runtime validation");
        assert!(oversized_config.chunk_size_for_content(0).is_err());
    }
}
