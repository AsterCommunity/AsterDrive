//! Azure Blob Storage driver.

mod error;
mod multipart;
mod presigned;
mod storage_driver;

use std::time::Duration;

use azure_core::http::Url;
use azure_storage_blob::{
    BlobClient, BlobClientOptions, BlobContainerClient, BlobContainerClientOptions,
    BlockBlobClient, BlockBlobClientOptions,
};

use crate::entities::storage_policy;
use crate::errors::{AsterError, Result};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::object_key;
use crate::types::effective_s3_multipart_chunk_size;

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
    InvalidEndpoint(String),
    MissingContainer,
}

impl AzureBlobConfigError {
    pub fn into_aster_error(self) -> AsterError {
        match self {
            Self::MissingEndpoint => storage_driver_error(
                StorageErrorKind::Misconfigured,
                "endpoint is required for Azure Blob storage",
            ),
            Self::InvalidEndpoint(endpoint) => storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("invalid Azure Blob endpoint URL: '{endpoint}'"),
            ),
            Self::MissingContainer => storage_driver_error(
                StorageErrorKind::Misconfigured,
                "container is required for Azure Blob storage",
            ),
        }
    }
}

#[derive(Clone)]
pub struct AzureBlobDriver {
    endpoint: String,
    account_name: String,
    account_key: String,
    container: String,
    base_path: String,
    chunk_size: i64,
}

impl AzureBlobDriver {
    pub fn validate_policy(policy: &storage_policy::Model) -> Result<()> {
        Self::normalize_endpoint_and_container(&policy.endpoint, &policy.bucket)?;
        if policy.access_key.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "access_key cannot be empty for Azure Blob storage policies",
            ));
        }
        if policy.secret_key.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "secret_key cannot be empty for Azure Blob storage policies",
            ));
        }
        Ok(())
    }

    pub fn new(policy: &storage_policy::Model) -> Result<Self> {
        Self::validate_policy(policy)?;
        let normalized = Self::normalize_endpoint_and_container(&policy.endpoint, &policy.bucket)?;
        Ok(Self {
            endpoint: normalized.endpoint,
            account_name: policy.access_key.trim().to_string(),
            account_key: policy.secret_key.trim().to_string(),
            container: normalized.container,
            base_path: policy.base_path.clone(),
            chunk_size: effective_s3_multipart_chunk_size(policy.chunk_size),
        })
    }

    pub fn normalize_endpoint_and_container(
        endpoint: &str,
        container: &str,
    ) -> Result<NormalizedAzureBlobConfig> {
        Self::try_normalize_endpoint_and_container(endpoint, container)
            .map_err(AzureBlobConfigError::into_aster_error)
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

        let parsed: http::Uri = endpoint
            .parse()
            .map_err(|_| AzureBlobConfigError::InvalidEndpoint(endpoint.to_string()))?;
        let scheme = parsed
            .scheme_str()
            .ok_or_else(|| AzureBlobConfigError::InvalidEndpoint(endpoint.to_string()))?;
        if scheme != "http" && scheme != "https" {
            return Err(AzureBlobConfigError::InvalidEndpoint(endpoint.to_string()));
        }
        parsed
            .authority()
            .ok_or_else(|| AzureBlobConfigError::InvalidEndpoint(endpoint.to_string()))?;

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
            self.service_sas_query(None, permissions, expires)?
        ))
        .map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("build Azure Blob container URL: {error}"),
            )
        })
    }

    fn blob_url(&self, path: &str, permissions: &str, expires: Duration) -> Result<Url> {
        let key = self.full_key(path);
        Url::parse(&format!(
            "{}/{}/{}?{}",
            self.endpoint,
            percent_encode_path_segment(&self.container),
            percent_encode_blob_path(&key),
            self.service_sas_query(Some(&key), permissions, expires)?
        ))
        .map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("build Azure Blob URL: {error}"),
            )
        })
    }

    fn block_blob_url(&self, path: &str, permissions: &str, expires: Duration) -> Result<Url> {
        self.blob_url(path, permissions, expires)
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

    fn container_client_options() -> BlobContainerClientOptions {
        BlobContainerClientOptions {
            version: AZURE_STORAGE_VERSION.to_string(),
            ..Default::default()
        }
    }

    fn blob_client(&self, path: &str, permissions: &str) -> Result<BlobClient> {
        BlobClient::new(
            self.blob_url(path, permissions, DEFAULT_OPERATION_SAS_TTL)?,
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
            .is_some_and(|host| {
                host.eq_ignore_ascii_case("localhost")
                    || host == "127.0.0.1"
                    || host == "::1"
                    || host == "[::1]"
            })
    }

    fn sas_protocol(&self) -> &'static str {
        if self.endpoint_uses_loopback_host() {
            "https,http"
        } else {
            "https"
        }
    }

    fn effective_chunk_size(&self) -> i64 {
        self.chunk_size
    }

    fn chunk_size_for_content(&self, content_length: u64) -> Result<usize> {
        let configured = crate::utils::numbers::i64_to_u64(
            self.effective_chunk_size(),
            "Azure Blob configured chunk size",
        )?;
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
        crate::utils::numbers::u64_to_usize(chunk_size, "Azure Blob effective chunk size")
    }

    fn block_id(part_number: i32) -> Result<Vec<u8>> {
        if part_number <= 0 {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("Azure Blob part_number must be positive: {part_number}"),
            ));
        }
        Ok(format!("aster-part-{part_number:010}").into_bytes())
    }

    fn block_id_marker(part_number: i32) -> Result<String> {
        use base64::Engine as _;
        Ok(base64::engine::general_purpose::STANDARD.encode(Self::block_id(part_number)?))
    }

    fn service_sas_query(
        &self,
        blob_name: Option<&str>,
        permissions: &str,
        expires: Duration,
    ) -> Result<String> {
        use azure_core::credentials::Secret;
        use azure_core::hmac::hmac_sha256;
        use chrono::{SecondsFormat, Utc};

        let starts_on = Utc::now() - chrono::Duration::minutes(5);
        let expires_on = Utc::now()
            + chrono::Duration::from_std(expires).map_err(|error| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    format!("invalid Azure Blob SAS expiry: {error}"),
                )
            })?;
        let signed_start = starts_on.to_rfc3339_opts(SecondsFormat::Secs, true);
        let signed_expiry = expires_on.to_rfc3339_opts(SecondsFormat::Secs, true);
        let canonicalized_resource = match blob_name {
            Some(blob_name) => format!(
                "/blob/{}/{}/{}",
                self.account_name, self.container, blob_name
            ),
            None => format!("/blob/{}/{}", self.account_name, self.container),
        };
        let signed_resource = if blob_name.is_some() { "b" } else { "c" };
        let signed_protocol = self.sas_protocol();

        // Service SAS string-to-sign for version 2020-12-06+.
        // Empty fields are significant and must remain as blank lines.
        let string_to_sign = [
            permissions,             // signed permissions
            &signed_start,           // signed start
            &signed_expiry,          // signed expiry
            &canonicalized_resource, // canonicalized resource
            "",                      // signed identifier
            "",                      // signed IP
            signed_protocol,         // signed protocol
            AZURE_STORAGE_VERSION,   // signed version
            signed_resource,         // signed resource
            "",                      // signed snapshot time
            "",                      // signed encryption scope
            "",                      // rscc
            "",                      // rscd
            "",                      // rsce
            "",                      // rscl
            "",                      // rsct
        ]
        .join("\n");
        let signature = hmac_sha256(&string_to_sign, &Secret::new(self.account_key.clone()))
            .map_err(|error| {
                storage_driver_error(
                    StorageErrorKind::Auth,
                    format!("sign Azure Blob SAS failed: {error}"),
                )
            })?;

        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        serializer
            .append_pair("sv", AZURE_STORAGE_VERSION)
            .append_pair("spr", signed_protocol)
            .append_pair("st", &signed_start)
            .append_pair("se", &signed_expiry)
            .append_pair("sr", signed_resource)
            .append_pair("sp", permissions)
            .append_pair("sig", &signature);
        Ok(serializer.finish())
    }

    fn map_azure_error(ctx: &str, error: azure_core::Error) -> AsterError {
        let kind = Self::classify_azure_error(&error);
        storage_driver_error(kind, format!("{ctx}: {}", Self::format_azure_error(error)))
    }

    fn rewrap_azure_error(ctx: &str, error: azure_core::Error) -> AsterError {
        storage_driver_error(StorageErrorKind::Misconfigured, format!("{ctx}: {error}"))
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

    use super::AzureBlobDriver;
    use crate::entities::storage_policy;
    use crate::types::{
        DriverType, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions,
        effective_s3_multipart_chunk_size,
    };

    fn sample_policy() -> storage_policy::Model {
        storage_policy::Model {
            id: 1,
            name: "Azure Blob".to_string(),
            driver_type: DriverType::AzureBlob,
            endpoint: " https://acct.blob.core.windows.net/ ".to_string(),
            bucket: " photos ".to_string(),
            access_key: " account-name ".to_string(),
            secret_key: "c2VjcmV0".to_string(),
            base_path: "base path".to_string(),
            remote_node_id: None,
            max_file_size: 0,
            allowed_types: StoredStoragePolicyAllowedTypes::empty(),
            options: StoredStoragePolicyOptions::empty(),
            is_default: false,
            chunk_size: 1,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
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
            ("https:///missing-host", "photos"),
            ("https://acct.blob.core.windows.net", ""),
        ] {
            assert!(
                AzureBlobDriver::normalize_endpoint_and_container(endpoint, container).is_err(),
                "expected invalid Azure config for endpoint={endpoint:?} container={container:?}",
            );
        }

        let mut policy = sample_policy();
        policy.access_key.clear();
        assert!(AzureBlobDriver::validate_policy(&policy).is_err());

        let mut policy = sample_policy();
        policy.secret_key = "   ".to_string();
        assert!(AzureBlobDriver::validate_policy(&policy).is_err());
    }

    #[test]
    fn new_trims_credentials_and_applies_minimum_chunk_size() {
        let driver = AzureBlobDriver::new(&sample_policy()).expect("valid Azure driver");

        assert_eq!(driver.endpoint, "https://acct.blob.core.windows.net");
        assert_eq!(driver.container, "photos");
        assert_eq!(driver.account_name, "account-name");
        assert_eq!(driver.chunk_size, effective_s3_multipart_chunk_size(1));
    }

    #[test]
    fn block_ids_are_stable_and_orderable() {
        assert_eq!(
            AzureBlobDriver::block_id_marker(12).expect("block id"),
            "YXN0ZXItcGFydC0wMDAwMDAwMDEy"
        );
    }

    #[test]
    fn rejects_non_positive_block_ids() {
        assert!(AzureBlobDriver::block_id_marker(0).is_err());
        assert!(AzureBlobDriver::block_id_marker(-1).is_err());
    }

    #[test]
    fn builds_encoded_blob_and_container_sas_urls() {
        let driver = AzureBlobDriver::new(&sample_policy()).expect("valid Azure driver");
        let blob_url = driver
            .blob_url(
                "folder with space/中文+plus.txt",
                "rw",
                Duration::from_secs(300),
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
        let mut policy = sample_policy();
        policy.endpoint = "http://127.0.0.1:10000/devstoreaccount1".to_string();
        policy.access_key = "devstoreaccount1".to_string();
        let driver = AzureBlobDriver::new(&policy).expect("valid Azurite driver");
        let blob_url = driver
            .blob_url("local.bin", "cw", Duration::from_secs(300))
            .expect("blob URL");
        let query: HashMap<_, _> = blob_url.query_pairs().into_owned().collect();

        assert_eq!(query.get("spr").map(String::as_str), Some("https,http"));
    }

    #[test]
    fn chunk_size_respects_configured_minimum_and_azure_block_limits() {
        let driver = AzureBlobDriver::new(&sample_policy()).expect("valid Azure driver");
        let configured = effective_s3_multipart_chunk_size(1);
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

        let too_large = super::AZURE_BLOCK_BLOB_MAX_BLOCKS
            .checked_mul(super::AZURE_BLOCK_BLOB_MAX_BLOCK_SIZE)
            .and_then(|value| value.checked_add(1))
            .expect("test size within u64");
        assert!(driver.chunk_size_for_content(too_large).is_err());
    }
}
