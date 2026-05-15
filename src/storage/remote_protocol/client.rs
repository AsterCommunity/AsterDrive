use std::sync::LazyLock;
use std::time::Duration;

use futures::TryStreamExt;
use percent_encoding::{AsciiSet, CONTROLS, percent_encode};
use reqwest::Method;
use tokio::io::AsyncRead;
use tokio_util::io::{ReaderStream, StreamReader};

use crate::errors::{AsterError, Result};
use crate::storage::driver::{BlobMetadata, PresignedDownloadOptions};
use crate::storage::error::{StorageErrorKind, storage_driver_error};

use super::auth::{normalize_remote_base_url, sign_internal_request, sign_presigned_request};
use super::errors::{
    build_remote_status_error, ensure_success, ensure_success_response,
    ensure_success_without_body, map_reqwest_error, parse_ingress_profile_response,
    remote_api_error_kind,
};
use super::models::{
    ApiEnvelope, RemoteBindingSyncRequest, RemoteCreateIngressProfileRequest,
    RemoteIngressProfileInfo, RemoteStorageCapabilities, RemoteStorageComposeRequest,
    RemoteStorageComposeResponse, RemoteStorageListResponse, RemoteStorageObjectMetadata,
    RemoteUpdateIngressProfileRequest,
};
use super::{
    INTERNAL_AUTH_ACCESS_KEY_HEADER, INTERNAL_AUTH_NONCE_HEADER, INTERNAL_AUTH_SIGNATURE_HEADER,
    INTERNAL_AUTH_TIMESTAMP_HEADER, INTERNAL_STORAGE_BASE_PATH, PRESIGNED_AUTH_ACCESS_KEY_QUERY,
    PRESIGNED_AUTH_EXPIRES_QUERY, PRESIGNED_AUTH_SIGNATURE_QUERY,
    PRESIGNED_RESPONSE_CACHE_CONTROL_QUERY, PRESIGNED_RESPONSE_CONTENT_DISPOSITION_QUERY,
    PRESIGNED_RESPONSE_CONTENT_TYPE_QUERY,
};

const STORAGE_KEY_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'?')
    .add(b'[')
    .add(b']')
    .add(b'{')
    .add(b'}');
const INGRESS_PROFILE_KEY_ENCODE_SET: &AsciiSet = &STORAGE_KEY_ENCODE_SET.add(b'/');

const DEFAULT_REMOTE_CONNECT_TIMEOUT_SECS: u64 = 5;
const DEFAULT_REMOTE_READ_TIMEOUT_SECS: u64 = 30;
const DEFAULT_REMOTE_OPERATION_TIMEOUT_SECS: u64 = 60 * 60;

static REMOTE_HTTP_CLIENT: LazyLock<std::result::Result<reqwest::Client, String>> =
    LazyLock::new(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(DEFAULT_REMOTE_CONNECT_TIMEOUT_SECS))
            .read_timeout(Duration::from_secs(DEFAULT_REMOTE_READ_TIMEOUT_SECS))
            .timeout(Duration::from_secs(DEFAULT_REMOTE_OPERATION_TIMEOUT_SECS))
            .build()
            .map_err(|e| format!("build remote HTTP client: {e}"))
    });
#[derive(Clone)]
pub struct RemoteStorageClient {
    base_url: String,
    access_key: String,
    secret_key: String,
    client: reqwest::Client,
}

impl RemoteStorageClient {
    pub fn new(base_url: &str, access_key: &str, secret_key: &str) -> Result<Self> {
        let normalized_base_url = normalize_remote_base_url(base_url)?;
        if normalized_base_url.is_empty() {
            return Err(AsterError::validation_error(
                "remote node base_url is required for outbound access",
            ));
        }
        if access_key.trim().is_empty() {
            return Err(AsterError::validation_error(
                "remote node access_key cannot be empty",
            ));
        }
        if secret_key.trim().is_empty() {
            return Err(AsterError::validation_error(
                "remote node secret_key cannot be empty",
            ));
        }

        let client = remote_http_client()?;

        Ok(Self {
            base_url: normalized_base_url,
            access_key: access_key.trim().to_string(),
            secret_key: secret_key.to_string(),
            client,
        })
    }

    pub async fn probe_capabilities(&self) -> Result<RemoteStorageCapabilities> {
        let url = self.url_for_path(&format!("{INTERNAL_STORAGE_BASE_PATH}/capabilities"))?;
        let response = self
            .signed_request(Method::GET, url, None)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let body = ensure_success(response, "probe remote storage capabilities").await?;
        let envelope: ApiEnvelope<RemoteStorageCapabilities> = serde_json::from_slice(&body)
            .map_err(|e| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    format!("decode remote storage capabilities response: {e}"),
                )
            })?;
        if envelope.code != 0 {
            return Err(storage_driver_error(
                remote_api_error_kind(envelope.code).unwrap_or(StorageErrorKind::Unknown),
                format!("remote storage capabilities failed: {}", envelope.msg),
            ));
        }
        let capabilities = envelope.data.ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "remote storage capabilities response missing data",
            )
        })?;
        capabilities.validate_protocol("remote storage capabilities probe")?;
        Ok(capabilities)
    }

    pub async fn put_bytes(&self, key: &str, data: &[u8]) -> Result<()> {
        let url = self.object_url(key)?;
        let content_length = u64::try_from(data.len()).map_err(|_| {
            storage_driver_error(
                StorageErrorKind::Precondition,
                "remote upload body length overflow",
            )
        })?;
        let response = self
            .signed_request(Method::PUT, url, Some(content_length))
            .body(data.to_vec())
            .send()
            .await
            .map_err(map_reqwest_error)?;
        ensure_success_without_body(response, "put remote storage object").await
    }

    pub async fn put_reader(
        &self,
        key: &str,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: u64,
    ) -> Result<()> {
        let url = self.object_url(key)?;
        let stream = ReaderStream::new(reader).map_err(std::io::Error::other);
        let response = self
            .signed_request(Method::PUT, url, Some(size))
            .body(reqwest::Body::wrap_stream(stream))
            .send()
            .await
            .map_err(map_reqwest_error)?;
        ensure_success_without_body(response, "stream put remote storage object").await
    }

    pub async fn get_bytes(&self, key: &str) -> Result<Vec<u8>> {
        let url = self.object_url(key)?;
        let response = self
            .signed_request(Method::GET, url, None)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        ensure_success(response, "get remote storage object").await
    }

    pub async fn get_stream(
        &self,
        key: &str,
        offset: Option<u64>,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let mut url = self.object_url(key)?;
        {
            let mut query = url.query_pairs_mut();
            if let Some(offset) = offset {
                query.append_pair("offset", &offset.to_string());
            }
            if let Some(length) = length {
                query.append_pair("length", &length.to_string());
            }
        }

        let response = self
            .signed_request(Method::GET, url, None)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let response = ensure_success_response(response, "stream remote storage object").await?;
        let stream = response
            .bytes_stream()
            .map_err(|error| std::io::Error::other(error.to_string()));
        Ok(Box::new(StreamReader::new(stream)))
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        let url = self.object_url(key)?;
        let response = self
            .signed_request(Method::DELETE, url, None)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        ensure_success_without_body(response, "delete remote storage object").await
    }

    pub async fn exists(&self, key: &str) -> Result<bool> {
        let url = self.object_url(key)?;
        let response = self
            .signed_request(Method::HEAD, url, None)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        match response.status() {
            reqwest::StatusCode::OK => Ok(true),
            reqwest::StatusCode::NOT_FOUND => Ok(false),
            _ => {
                let error =
                    build_remote_status_error(response, "head remote storage object", true).await;
                Err(error)
            }
        }
    }

    pub async fn metadata(&self, key: &str) -> Result<BlobMetadata> {
        let url = self.object_metadata_url(key)?;
        let response = self
            .signed_request(Method::GET, url, None)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let body = ensure_success(response, "get remote storage metadata").await?;
        let envelope: ApiEnvelope<RemoteStorageObjectMetadata> = serde_json::from_slice(&body)
            .map_err(|e| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    format!("decode remote storage metadata response: {e}"),
                )
            })?;
        if envelope.code != 0 {
            return Err(storage_driver_error(
                remote_api_error_kind(envelope.code).unwrap_or(StorageErrorKind::Unknown),
                format!("remote storage metadata failed: {}", envelope.msg),
            ));
        }
        let metadata = envelope.data.ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "remote storage metadata response missing data",
            )
        })?;
        Ok(BlobMetadata {
            size: metadata.size,
            content_type: metadata.content_type,
        })
    }

    pub async fn list_paths(&self, prefix: Option<&str>) -> Result<Vec<String>> {
        let mut url = self.url_for_path(&format!("{INTERNAL_STORAGE_BASE_PATH}/objects"))?;
        if let Some(prefix) = prefix.filter(|value| !value.is_empty()) {
            url.query_pairs_mut().append_pair("prefix", prefix);
        }
        let response = self
            .signed_request(Method::GET, url, None)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let body = ensure_success(response, "list remote storage objects").await?;
        let envelope: ApiEnvelope<RemoteStorageListResponse> = serde_json::from_slice(&body)
            .map_err(|e| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    format!("decode remote storage list response: {e}"),
                )
            })?;
        if envelope.code != 0 {
            return Err(storage_driver_error(
                remote_api_error_kind(envelope.code).unwrap_or(StorageErrorKind::Unknown),
                format!("remote storage list failed: {}", envelope.msg),
            ));
        }
        Ok(envelope.data.unwrap_or_default().items)
    }

    pub async fn sync_binding(&self, binding: &RemoteBindingSyncRequest) -> Result<()> {
        let url = self.url_for_path(&format!("{INTERNAL_STORAGE_BASE_PATH}/binding"))?;
        let body = serde_json::to_vec(binding).map_err(|e| {
            storage_driver_error(
                StorageErrorKind::Unknown,
                format!("encode remote binding sync request: {e}"),
            )
        })?;
        let content_length = u64::try_from(body.len()).map_err(|_| {
            storage_driver_error(
                StorageErrorKind::Precondition,
                "remote binding sync body length overflow",
            )
        })?;
        let response = self
            .signed_request(Method::PUT, url, Some(content_length))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        ensure_success_without_body(response, "sync remote binding state").await
    }

    pub async fn list_ingress_profiles(&self) -> Result<Vec<RemoteIngressProfileInfo>> {
        let url = self.url_for_path(&format!("{INTERNAL_STORAGE_BASE_PATH}/ingress-profiles"))?;
        let response = self
            .signed_request(Method::GET, url, None)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let body = ensure_success(response, "list remote ingress profiles").await?;
        let envelope: ApiEnvelope<Vec<RemoteIngressProfileInfo>> = serde_json::from_slice(&body)
            .map_err(|e| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    format!("decode remote ingress profile list response: {e}"),
                )
            })?;
        if envelope.code != 0 {
            return Err(storage_driver_error(
                remote_api_error_kind(envelope.code).unwrap_or(StorageErrorKind::Unknown),
                format!("remote ingress profile list failed: {}", envelope.msg),
            ));
        }
        envelope.data.ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "list remote ingress profiles response missing data",
            )
        })
    }

    pub async fn create_ingress_profile(
        &self,
        profile: &RemoteCreateIngressProfileRequest,
    ) -> Result<RemoteIngressProfileInfo> {
        let url = self.url_for_path(&format!("{INTERNAL_STORAGE_BASE_PATH}/ingress-profiles"))?;
        let body = serde_json::to_vec(profile).map_err(|e| {
            storage_driver_error(
                StorageErrorKind::Unknown,
                format!("encode remote ingress profile create request: {e}"),
            )
        })?;
        let content_length = u64::try_from(body.len()).map_err(|_| {
            storage_driver_error(
                StorageErrorKind::Precondition,
                "remote ingress profile create body length overflow",
            )
        })?;
        let response = self
            .signed_request(Method::POST, url, Some(content_length))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        parse_ingress_profile_response(response, "create remote ingress profile").await
    }

    pub async fn update_ingress_profile(
        &self,
        profile_key: &str,
        profile: &RemoteUpdateIngressProfileRequest,
    ) -> Result<RemoteIngressProfileInfo> {
        let url = self.ingress_profile_url(profile_key)?;
        let body = serde_json::to_vec(profile).map_err(|e| {
            storage_driver_error(
                StorageErrorKind::Unknown,
                format!("encode remote ingress profile update request: {e}"),
            )
        })?;
        let content_length = u64::try_from(body.len()).map_err(|_| {
            storage_driver_error(
                StorageErrorKind::Precondition,
                "remote ingress profile update body length overflow",
            )
        })?;
        let response = self
            .signed_request(Method::PATCH, url, Some(content_length))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        parse_ingress_profile_response(response, "update remote ingress profile").await
    }

    pub async fn delete_ingress_profile(&self, profile_key: &str) -> Result<()> {
        let url = self.ingress_profile_url(profile_key)?;
        let response = self
            .signed_request(Method::DELETE, url, None)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        ensure_success_without_body(response, "delete remote ingress profile").await
    }

    pub fn presigned_put_url(&self, key: &str, expires: Duration) -> Result<String> {
        let mut url = self.object_url(key)?;
        let request_target = presigned_request_target(&url);
        let expires_at = presigned_expires_at(expires)?;
        let signature = sign_presigned_request(
            &self.secret_key,
            Method::PUT.as_str(),
            &request_target,
            &self.access_key,
            expires_at,
        );
        url.query_pairs_mut()
            .append_pair(PRESIGNED_AUTH_ACCESS_KEY_QUERY, &self.access_key)
            .append_pair(PRESIGNED_AUTH_EXPIRES_QUERY, &expires_at.to_string())
            .append_pair(PRESIGNED_AUTH_SIGNATURE_QUERY, &signature);

        Ok(url.to_string())
    }

    pub fn presigned_url(
        &self,
        key: &str,
        expires: Duration,
        options: PresignedDownloadOptions,
    ) -> Result<String> {
        let mut url = self.object_url(key)?;
        {
            let mut query = url.query_pairs_mut();
            if let Some(cache_control) = options.response_cache_control.as_deref() {
                query.append_pair(PRESIGNED_RESPONSE_CACHE_CONTROL_QUERY, cache_control);
            }
            if let Some(content_disposition) = options.response_content_disposition.as_deref() {
                query.append_pair(
                    PRESIGNED_RESPONSE_CONTENT_DISPOSITION_QUERY,
                    content_disposition,
                );
            }
            if let Some(content_type) = options.response_content_type.as_deref() {
                query.append_pair(PRESIGNED_RESPONSE_CONTENT_TYPE_QUERY, content_type);
            }
        }

        let request_target = presigned_request_target(&url);
        let expires_at = presigned_expires_at(expires)?;
        let signature = sign_presigned_request(
            &self.secret_key,
            Method::GET.as_str(),
            &request_target,
            &self.access_key,
            expires_at,
        );
        url.query_pairs_mut()
            .append_pair(PRESIGNED_AUTH_ACCESS_KEY_QUERY, &self.access_key)
            .append_pair(PRESIGNED_AUTH_EXPIRES_QUERY, &expires_at.to_string())
            .append_pair(PRESIGNED_AUTH_SIGNATURE_QUERY, &signature);

        Ok(url.to_string())
    }

    pub async fn compose_objects(
        &self,
        target_key: &str,
        part_keys: Vec<String>,
        expected_size: i64,
    ) -> Result<RemoteStorageComposeResponse> {
        let url = self.url_for_path(&format!("{INTERNAL_STORAGE_BASE_PATH}/compose"))?;
        let body = serde_json::to_vec(&RemoteStorageComposeRequest {
            target_key: target_key.to_string(),
            part_keys,
            expected_size,
        })
        .map_err(|e| {
            storage_driver_error(
                StorageErrorKind::Unknown,
                format!("encode remote compose request: {e}"),
            )
        })?;
        let content_length = u64::try_from(body.len()).map_err(|_| {
            storage_driver_error(
                StorageErrorKind::Precondition,
                "remote compose body length overflow",
            )
        })?;
        let response = self
            .signed_request(Method::POST, url, Some(content_length))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let body = ensure_success(response, "compose remote storage objects").await?;
        let envelope: ApiEnvelope<RemoteStorageComposeResponse> = serde_json::from_slice(&body)
            .map_err(|e| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    format!("decode remote storage compose response: {e}"),
                )
            })?;
        if envelope.code != 0 {
            return Err(storage_driver_error(
                remote_api_error_kind(envelope.code).unwrap_or(StorageErrorKind::Unknown),
                format!("remote storage compose failed: {}", envelope.msg),
            ));
        }
        envelope.data.ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "remote storage compose response missing data",
            )
        })
    }

    fn signed_request(
        &self,
        method: Method,
        url: reqwest::Url,
        content_length: Option<u64>,
    ) -> reqwest::RequestBuilder {
        let timestamp = chrono::Utc::now().timestamp();
        let nonce = uuid::Uuid::new_v4().to_string();
        let path_and_query = if let Some(query) = url.query() {
            format!("{}?{query}", url.path())
        } else {
            url.path().to_string()
        };
        let signature = sign_internal_request(
            &self.secret_key,
            method.as_str(),
            &path_and_query,
            timestamp,
            &nonce,
            content_length,
        );

        let mut request = self
            .client
            .request(method, url)
            .header(INTERNAL_AUTH_ACCESS_KEY_HEADER, &self.access_key)
            .header(INTERNAL_AUTH_TIMESTAMP_HEADER, timestamp.to_string())
            .header(INTERNAL_AUTH_NONCE_HEADER, nonce)
            .header(INTERNAL_AUTH_SIGNATURE_HEADER, signature);
        if let Some(content_length) = content_length {
            request = request.header(reqwest::header::CONTENT_LENGTH, content_length);
        }
        request
    }

    fn url_for_path(&self, path: &str) -> Result<reqwest::Url> {
        let joined = format!("{}{}", self.base_url, path);
        reqwest::Url::parse(&joined).map_err(|e| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("build remote storage url: {e}"),
            )
        })
    }

    fn object_url(&self, key: &str) -> Result<reqwest::Url> {
        let key = key.trim_start_matches('/');
        let encoded_key = percent_encode(key.as_bytes(), STORAGE_KEY_ENCODE_SET).to_string();
        self.url_for_path(&format!(
            "{INTERNAL_STORAGE_BASE_PATH}/objects/{encoded_key}"
        ))
    }

    fn object_metadata_url(&self, key: &str) -> Result<reqwest::Url> {
        let key = key.trim_start_matches('/');
        let encoded_key = percent_encode(key.as_bytes(), STORAGE_KEY_ENCODE_SET).to_string();
        self.url_for_path(&format!(
            "{INTERNAL_STORAGE_BASE_PATH}/objects/{encoded_key}/metadata"
        ))
    }

    pub(super) fn ingress_profile_url(&self, profile_key: &str) -> Result<reqwest::Url> {
        let encoded_key = percent_encode(
            profile_key.trim().as_bytes(),
            INGRESS_PROFILE_KEY_ENCODE_SET,
        )
        .to_string();
        self.url_for_path(&format!(
            "{INTERNAL_STORAGE_BASE_PATH}/ingress-profiles/{encoded_key}"
        ))
    }
}

fn remote_http_client() -> Result<reqwest::Client> {
    REMOTE_HTTP_CLIENT
        .as_ref()
        .cloned()
        .map_err(|message| storage_driver_error(StorageErrorKind::Misconfigured, message.clone()))
}

fn presigned_expires_at(expires: Duration) -> Result<i64> {
    let expires_secs = i64::try_from(expires.as_secs()).map_err(|_| {
        storage_driver_error(
            StorageErrorKind::Precondition,
            "remote presigned URL expiry exceeds i64 range",
        )
    })?;
    if expires_secs <= 0 {
        return Err(storage_driver_error(
            StorageErrorKind::Precondition,
            "remote presigned URL expiry must be positive",
        ));
    }

    chrono::Utc::now()
        .timestamp()
        .checked_add(expires_secs)
        .ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Precondition,
                "remote presigned URL expiry overflow",
            )
        })
}

fn presigned_request_target(url: &reqwest::Url) -> String {
    if let Some(query) = url.query() {
        format!("{}?{query}", url.path())
    } else {
        url.path().to_string()
    }
}
