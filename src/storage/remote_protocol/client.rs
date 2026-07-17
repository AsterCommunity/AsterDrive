use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::StatusCode;
use percent_encoding::{AsciiSet, CONTROLS, percent_encode};
use reqwest::Method;
use tokio::io::AsyncRead;

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::Result;
use crate::storage::StorageCapacityInfo;
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::traits::driver::{BlobMetadata, PresignedDownloadOptions};

use super::errors::remote_api_error_kind;
use super::models::{
    ApiEnvelope, RemoteBindingSyncRequest, RemoteCreateStorageTargetRequest,
    RemoteStorageCapabilities, RemoteStorageCapacityResponse, RemoteStorageComposeRequest,
    RemoteStorageComposeResponse, RemoteStorageListResponse, RemoteStorageObjectMetadata,
    RemoteStorageTargetInfo, RemoteUpdateStorageTargetRequest,
};
use super::transport::{
    DirectHttpTransport, RemoteRequestBody, RemoteTransport, RemoteTransportRequest,
    RemoteTransportResponse, ReverseTunnelTransport, build_remote_status_error, ensure_success,
    ensure_success_response, ensure_success_with_body_limit, ensure_success_without_body,
    response_stream,
};
use super::tunnel::server::RemoteTunnelBroker;
use super::{
    INTERNAL_STORAGE_BASE_PATH, PRESIGNED_RESPONSE_CACHE_CONTROL_QUERY,
    PRESIGNED_RESPONSE_CONTENT_DISPOSITION_QUERY, PRESIGNED_RESPONSE_CONTENT_TYPE_QUERY,
    REMOTE_CONTROL_PLANE_BODY_LIMIT, REMOTE_LIST_PAGE_BODY_LIMIT, REMOTE_LIST_PAGE_SIZE,
    REMOTE_POLICY_MAX_FILE_SIZE_QUERY, REMOTE_STORAGE_TARGET_KEY_QUERY,
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
const STORAGE_TARGET_KEY_ENCODE_SET: &AsciiSet = &STORAGE_KEY_ENCODE_SET.add(b'/');

#[derive(Clone)]
pub struct RemoteStorageClient {
    transport: Arc<dyn RemoteTransport>,
    storage_target_key: Option<String>,
    policy_max_file_size: i64,
}

impl RemoteStorageClient {
    pub fn new(base_url: &str, access_key: &str, secret_key: &str) -> Result<Self> {
        Ok(Self {
            transport: Arc::new(DirectHttpTransport::new(base_url, access_key, secret_key)?),
            storage_target_key: None,
            policy_max_file_size: 0,
        })
    }

    pub(crate) fn new_reverse_tunnel(
        remote_node: &crate::entities::managed_follower::Model,
        broker: std::sync::Arc<dyn RemoteTunnelBroker>,
    ) -> Result<Self> {
        Ok(Self {
            transport: Arc::new(ReverseTunnelTransport::new(remote_node, broker)),
            storage_target_key: None,
            policy_max_file_size: 0,
        })
    }

    pub fn with_policy_context(&self, target_key: Option<&str>, max_file_size: i64) -> Self {
        Self {
            transport: self.transport.clone(),
            // None is preserved for legacy remote policies that rely on the
            // follower binding default. New policy flows should pass an
            // explicit target key so the signed request pins the target.
            storage_target_key: target_key
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            policy_max_file_size: max_file_size.max(0),
        }
    }

    pub async fn probe_capabilities(&self) -> Result<RemoteStorageCapabilities> {
        let path = format!("{INTERNAL_STORAGE_BASE_PATH}/capabilities");
        let response = self
            .send_signed(Method::GET, path, None, RemoteRequestBody::Empty)
            .await?;
        let body = ensure_success_with_body_limit(
            response,
            "probe remote storage capabilities",
            REMOTE_CONTROL_PLANE_BODY_LIMIT,
        )
        .await?;
        let envelope: ApiEnvelope<RemoteStorageCapabilities> = serde_json::from_slice(&body)
            .map_err(|e| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    format!("decode remote storage capabilities response: {e}"),
                )
            })?;
        if envelope.code != ApiErrorCode::Success {
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
        let mut path = self.object_path(key);
        self.append_policy_max_file_size(&mut path);
        let response = self
            .send_signed(
                Method::PUT,
                path,
                None,
                RemoteRequestBody::Bytes(Bytes::copy_from_slice(data)),
            )
            .await?;
        ensure_success_without_body(response, "put remote storage object").await
    }

    pub async fn put_reader(
        &self,
        key: &str,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: u64,
    ) -> Result<()> {
        let mut path = self.object_path(key);
        self.append_policy_max_file_size(&mut path);
        let response = self
            .send_signed(
                Method::PUT,
                path,
                None,
                RemoteRequestBody::Reader { reader, size },
            )
            .await?;
        ensure_success_without_body(response, "stream put remote storage object").await
    }

    pub async fn get_bytes(&self, key: &str) -> Result<Vec<u8>> {
        let path = self.object_path(key);
        let response = self
            .send_signed(Method::GET, path, None, RemoteRequestBody::Empty)
            .await?;
        ensure_success(response, "get remote storage object").await
    }

    pub async fn get_stream(
        &self,
        key: &str,
        offset: Option<u64>,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let mut path = self.object_path(key);
        let mut query = Vec::new();
        if let Some(offset) = offset {
            query.push(("offset", offset.to_string()));
        }
        if let Some(length) = length {
            query.push(("length", length.to_string()));
        }
        append_query_pairs(&mut path, query);

        let response = self
            .send_signed(Method::GET, path, None, RemoteRequestBody::Empty)
            .await?;
        let response = ensure_success_response(response, "stream remote storage object").await?;
        Ok(response_stream(response))
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        let path = self.object_path(key);
        let response = self
            .send_signed(Method::DELETE, path, None, RemoteRequestBody::Empty)
            .await?;
        ensure_success_without_body(response, "delete remote storage object").await
    }

    pub async fn exists(&self, key: &str) -> Result<bool> {
        let path = self.object_path(key);
        let response = self
            .send_signed(Method::HEAD, path, None, RemoteRequestBody::Empty)
            .await?;
        match response.status() {
            StatusCode::OK => Ok(true),
            StatusCode::NOT_FOUND => Ok(false),
            _ => {
                let error =
                    build_remote_status_error(response, "head remote storage object", true).await;
                Err(error)
            }
        }
    }

    pub async fn metadata(&self, key: &str) -> Result<BlobMetadata> {
        let path = self.object_metadata_path(key);
        let response = self
            .send_signed(Method::GET, path, None, RemoteRequestBody::Empty)
            .await?;
        let body = ensure_success_with_body_limit(
            response,
            "get remote storage metadata",
            REMOTE_CONTROL_PLANE_BODY_LIMIT,
        )
        .await?;
        let envelope: ApiEnvelope<RemoteStorageObjectMetadata> = serde_json::from_slice(&body)
            .map_err(|e| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    format!("decode remote storage metadata response: {e}"),
                )
            })?;
        if envelope.code != ApiErrorCode::Success {
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
        let prefix = prefix.filter(|value| !value.is_empty());
        let mut cursor: Option<u64> = None;
        let mut items = Vec::new();

        loop {
            let mut path = format!("{INTERNAL_STORAGE_BASE_PATH}/objects");
            self.append_storage_target_key(&mut path);
            let mut query = vec![("limit", REMOTE_LIST_PAGE_SIZE.to_string())];
            if let Some(prefix) = prefix {
                query.push(("prefix", prefix.to_string()));
            }
            if let Some(cursor) = cursor {
                query.push(("cursor", cursor.to_string()));
            }
            append_query_pairs(&mut path, query);

            let response = self
                .send_signed(Method::GET, path, None, RemoteRequestBody::Empty)
                .await?;
            let body = ensure_success_with_body_limit(
                response,
                "list remote storage objects page",
                REMOTE_LIST_PAGE_BODY_LIMIT,
            )
            .await?;
            let envelope: ApiEnvelope<RemoteStorageListResponse> = serde_json::from_slice(&body)
                .map_err(|e| {
                    storage_driver_error(
                        StorageErrorKind::Misconfigured,
                        format!("decode remote storage list response: {e}"),
                    )
                })?;
            if envelope.code != ApiErrorCode::Success {
                return Err(storage_driver_error(
                    remote_api_error_kind(envelope.code).unwrap_or(StorageErrorKind::Unknown),
                    format!("remote storage list failed: {}", envelope.msg),
                ));
            }

            let page = envelope.data.unwrap_or_default();
            items.extend(page.items);
            let Some(next_cursor) = page.next_cursor else {
                break;
            };
            if next_cursor <= cursor.unwrap_or(0) {
                return Err(storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "remote storage list cursor did not advance",
                ));
            }
            cursor = Some(next_cursor);
        }

        Ok(items)
    }

    pub async fn capacity_info(&self) -> Result<StorageCapacityInfo> {
        let mut path = format!("{INTERNAL_STORAGE_BASE_PATH}/capacity");
        self.append_storage_target_key(&mut path);
        let response = self
            .send_signed(Method::GET, path, None, RemoteRequestBody::Empty)
            .await?;
        let body = ensure_success_with_body_limit(
            response,
            "get remote storage capacity",
            REMOTE_CONTROL_PLANE_BODY_LIMIT,
        )
        .await?;
        let envelope: ApiEnvelope<RemoteStorageCapacityResponse> = serde_json::from_slice(&body)
            .map_err(|e| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    format!("decode remote storage capacity response: {e}"),
                )
            })?;
        if envelope.code != ApiErrorCode::Success {
            return Err(storage_driver_error(
                remote_api_error_kind(envelope.code).unwrap_or(StorageErrorKind::Unknown),
                format!("remote storage capacity failed: {}", envelope.msg),
            ));
        }
        envelope.data.map(|data| data.capacity).ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "remote storage capacity response missing data",
            )
        })
    }

    pub async fn sync_binding(&self, binding: &RemoteBindingSyncRequest) -> Result<()> {
        let path = format!("{INTERNAL_STORAGE_BASE_PATH}/binding");
        let body = serde_json::to_vec(binding).map_err(|e| {
            storage_driver_error(
                StorageErrorKind::Unknown,
                format!("encode remote binding sync request: {e}"),
            )
        })?;
        let response = self
            .send_signed(
                Method::PUT,
                path,
                Some("application/json"),
                RemoteRequestBody::Bytes(Bytes::from(body)),
            )
            .await?;
        ensure_success_without_body(response, "sync remote binding state").await
    }

    pub async fn list_storage_targets(&self) -> Result<Vec<RemoteStorageTargetInfo>> {
        let path = format!("{INTERNAL_STORAGE_BASE_PATH}/targets");
        let response = self
            .send_signed(Method::GET, path, None, RemoteRequestBody::Empty)
            .await?;
        let body = ensure_success_with_body_limit(
            response,
            "list remote storage targets",
            REMOTE_CONTROL_PLANE_BODY_LIMIT,
        )
        .await?;
        let envelope: ApiEnvelope<Vec<RemoteStorageTargetInfo>> = serde_json::from_slice(&body)
            .map_err(|e| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    format!("decode remote storage target list response: {e}"),
                )
            })?;
        if envelope.code != ApiErrorCode::Success {
            return Err(storage_driver_error(
                remote_api_error_kind(envelope.code).unwrap_or(StorageErrorKind::Unknown),
                format!("remote storage target list failed: {}", envelope.msg),
            ));
        }
        envelope.data.ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "list remote storage targets response missing data",
            )
        })
    }

    pub async fn create_storage_target(
        &self,
        target: &RemoteCreateStorageTargetRequest,
    ) -> Result<RemoteStorageTargetInfo> {
        let path = format!("{INTERNAL_STORAGE_BASE_PATH}/targets");
        let body = serde_json::to_vec(target).map_err(|e| {
            storage_driver_error(
                StorageErrorKind::Unknown,
                format!("encode remote storage target create request: {e}"),
            )
        })?;
        let response = self
            .send_signed(
                Method::POST,
                path,
                Some("application/json"),
                RemoteRequestBody::Bytes(Bytes::from(body)),
            )
            .await?;
        parse_storage_target_response(response, "create remote storage target").await
    }

    pub async fn update_storage_target(
        &self,
        target_key: &str,
        target: &RemoteUpdateStorageTargetRequest,
    ) -> Result<RemoteStorageTargetInfo> {
        let path = self.storage_target_path(target_key);
        let body = serde_json::to_vec(target).map_err(|e| {
            storage_driver_error(
                StorageErrorKind::Unknown,
                format!("encode remote storage target update request: {e}"),
            )
        })?;
        let response = self
            .send_signed(
                Method::PATCH,
                path,
                Some("application/json"),
                RemoteRequestBody::Bytes(Bytes::from(body)),
            )
            .await?;
        parse_storage_target_response(response, "update remote storage target").await
    }

    pub async fn delete_storage_target(&self, target_key: &str) -> Result<()> {
        let path = self.storage_target_path(target_key);
        let response = self
            .send_signed(Method::DELETE, path, None, RemoteRequestBody::Empty)
            .await?;
        ensure_success_without_body(response, "delete remote storage target").await
    }

    pub fn presigned_put_url(&self, key: &str, expires: Duration) -> Result<String> {
        let mut path = self.object_path(key);
        self.append_policy_max_file_size(&mut path);
        self.transport.presigned_url(Method::PUT, &path, expires)
    }

    pub fn presigned_url(
        &self,
        key: &str,
        expires: Duration,
        options: PresignedDownloadOptions,
    ) -> Result<String> {
        let mut path = self.object_path(key);
        let mut query = Vec::new();
        if let Some(cache_control) = options.response_cache_control {
            query.push((PRESIGNED_RESPONSE_CACHE_CONTROL_QUERY, cache_control));
        }
        if let Some(content_disposition) = options.response_content_disposition {
            query.push((
                PRESIGNED_RESPONSE_CONTENT_DISPOSITION_QUERY,
                content_disposition,
            ));
        }
        if let Some(content_type) = options.response_content_type {
            query.push((PRESIGNED_RESPONSE_CONTENT_TYPE_QUERY, content_type));
        }
        append_query_pairs(&mut path, query);
        self.transport.presigned_url(Method::GET, &path, expires)
    }

    pub async fn compose_objects(
        &self,
        target_key: &str,
        part_keys: Vec<String>,
        expected_size: i64,
    ) -> Result<RemoteStorageComposeResponse> {
        let mut path = format!("{INTERNAL_STORAGE_BASE_PATH}/compose");
        self.append_storage_target_key(&mut path);
        self.append_policy_max_file_size(&mut path);
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
        let response = self
            .send_signed(
                Method::POST,
                path,
                Some("application/json"),
                RemoteRequestBody::Bytes(Bytes::from(body)),
            )
            .await?;
        let body = ensure_success_with_body_limit(
            response,
            "compose remote storage objects",
            REMOTE_CONTROL_PLANE_BODY_LIMIT,
        )
        .await?;
        let envelope: ApiEnvelope<RemoteStorageComposeResponse> = serde_json::from_slice(&body)
            .map_err(|e| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    format!("decode remote storage compose response: {e}"),
                )
            })?;
        if envelope.code != ApiErrorCode::Success {
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

    async fn send_signed(
        &self,
        method: Method,
        path_and_query: String,
        content_type: Option<&'static str>,
        body: RemoteRequestBody,
    ) -> Result<RemoteTransportResponse> {
        self.transport
            .send(RemoteTransportRequest {
                method,
                path_and_query,
                content_type,
                body,
            })
            .await
    }

    fn object_path(&self, key: &str) -> String {
        let key = key.trim_start_matches('/');
        let encoded_key = percent_encode(key.as_bytes(), STORAGE_KEY_ENCODE_SET).to_string();
        let mut path = format!("{INTERNAL_STORAGE_BASE_PATH}/objects/{encoded_key}");
        self.append_storage_target_key(&mut path);
        path
    }

    fn object_metadata_path(&self, key: &str) -> String {
        let key = key.trim_start_matches('/');
        let encoded_key = percent_encode(key.as_bytes(), STORAGE_KEY_ENCODE_SET).to_string();
        let mut path = format!("{INTERNAL_STORAGE_BASE_PATH}/objects/{encoded_key}/metadata");
        self.append_storage_target_key(&mut path);
        path
    }

    pub(super) fn storage_target_path(&self, target_key: &str) -> String {
        let encoded_key =
            percent_encode(target_key.trim().as_bytes(), STORAGE_TARGET_KEY_ENCODE_SET).to_string();
        format!("{INTERNAL_STORAGE_BASE_PATH}/targets/{encoded_key}")
    }

    fn append_storage_target_key(&self, path: &mut String) {
        if let Some(target_key) = &self.storage_target_key {
            append_query_pairs(
                path,
                [(REMOTE_STORAGE_TARGET_KEY_QUERY, target_key.clone())],
            );
        }
    }

    fn append_policy_max_file_size(&self, path: &mut String) {
        append_query_pairs(
            path,
            [(
                REMOTE_POLICY_MAX_FILE_SIZE_QUERY,
                self.policy_max_file_size.to_string(),
            )],
        );
    }
}

async fn parse_storage_target_response(
    response: RemoteTransportResponse,
    context: &str,
) -> Result<RemoteStorageTargetInfo> {
    let body =
        ensure_success_with_body_limit(response, context, REMOTE_CONTROL_PLANE_BODY_LIMIT).await?;
    let envelope: ApiEnvelope<RemoteStorageTargetInfo> =
        serde_json::from_slice(&body).map_err(|e| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("decode remote storage target response: {e}"),
            )
        })?;
    if envelope.code != ApiErrorCode::Success {
        return Err(storage_driver_error(
            remote_api_error_kind(envelope.code).unwrap_or(StorageErrorKind::Unknown),
            format!("{context} failed: {}", envelope.msg),
        ));
    }
    envelope.data.ok_or_else(|| {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("{context} response missing data"),
        )
    })
}

fn append_query_pairs(
    path_and_query: &mut String,
    pairs: impl IntoIterator<Item = (&'static str, String)>,
) {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    let mut has_values = false;
    for (key, value) in pairs {
        serializer.append_pair(key, &value);
        has_values = true;
    }
    if !has_values {
        return;
    }
    let query = serializer.finish();
    if path_and_query.contains('?') {
        path_and_query.push('&');
    } else {
        path_and_query.push('?');
    }
    path_and_query.push_str(&query);
}
