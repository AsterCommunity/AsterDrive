use std::sync::{Arc, LazyLock};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use futures::{StreamExt, TryStreamExt};
use http::{Method as HttpMethod, StatusCode};
use reqwest::Method;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio_util::io::{ReaderStream, StreamReader};

use crate::config::OUTBOUND_HTTP_USER_AGENT;
use crate::entities::managed_follower;
use crate::errors::{AsterError, Result};
use crate::storage::error::{StorageErrorKind, storage_driver_error};

use super::auth::{normalize_remote_base_url, sign_internal_request};
use super::errors::{build_remote_status_error_from_parts, map_reqwest_error};
use super::tunnel::server::{
    RemoteTunnelBroker, RemoteTunnelHttpResponse, RemoteTunnelStreamHttpResponse,
};
use super::{
    INTERNAL_AUTH_ACCESS_KEY_HEADER, INTERNAL_AUTH_NONCE_HEADER, INTERNAL_AUTH_SIGNATURE_HEADER,
    INTERNAL_AUTH_TIMESTAMP_HEADER, REMOTE_CONTROL_PLANE_BODY_LIMIT,
};

const DEFAULT_REMOTE_CONNECT_TIMEOUT_SECS: u64 = 5;
const DEFAULT_REMOTE_READ_TIMEOUT_SECS: u64 = 30;
const DEFAULT_REMOTE_OPERATION_TIMEOUT_SECS: u64 = 60 * 60;

static REMOTE_HTTP_CLIENT: LazyLock<std::result::Result<reqwest::Client, String>> =
    LazyLock::new(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(DEFAULT_REMOTE_CONNECT_TIMEOUT_SECS))
            .read_timeout(Duration::from_secs(DEFAULT_REMOTE_READ_TIMEOUT_SECS))
            .timeout(Duration::from_secs(DEFAULT_REMOTE_OPERATION_TIMEOUT_SECS))
            .user_agent(OUTBOUND_HTTP_USER_AGENT)
            .build()
            .map_err(|e| format!("build remote HTTP client: {e}"))
    });

pub enum RemoteRequestBody {
    Empty,
    Bytes(Bytes),
    Reader {
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: u64,
    },
}

impl RemoteRequestBody {
    pub fn content_length(&self) -> Result<Option<u64>> {
        match self {
            Self::Empty => Ok(None),
            Self::Bytes(body) => u64::try_from(body.len()).map(Some).map_err(|_| {
                storage_driver_error(
                    StorageErrorKind::Precondition,
                    "remote request body length exceeds u64 range",
                )
            }),
            Self::Reader { size, .. } => Ok(Some(*size)),
        }
    }

    fn into_reader(self) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        match self {
            Self::Empty => Ok(Box::new(std::io::Cursor::new(Bytes::new()))),
            Self::Bytes(body) => Ok(Box::new(std::io::Cursor::new(body))),
            Self::Reader { reader, .. } => Ok(reader),
        }
    }

    async fn into_buffered_bytes(self) -> Result<Bytes> {
        match self {
            Self::Empty => Ok(Bytes::new()),
            Self::Bytes(body) => Ok(body),
            Self::Reader { mut reader, size } => {
                if size
                    > u64::try_from(super::tunnel::server::REMOTE_TUNNEL_BODY_LIMIT)
                        .unwrap_or(u64::MAX)
                {
                    return Err(storage_driver_error(
                        StorageErrorKind::Unsupported,
                        format!(
                            "reverse tunnel streaming upload exceeds {} bytes; use direct transport or a streaming tunnel",
                            super::tunnel::server::REMOTE_TUNNEL_BODY_LIMIT
                        ),
                    ));
                }
                let capacity = aster_forge_utils::numbers::u64_to_usize(
                    size,
                    "reverse tunnel buffered upload size",
                )?;
                let mut data = Vec::with_capacity(capacity);
                reader.read_to_end(&mut data).await.map_err(|error| {
                    storage_driver_error(
                        StorageErrorKind::Transient,
                        format!("read reverse tunnel buffered upload: {error}"),
                    )
                })?;
                let actual_len = u64::try_from(data.len()).map_err(|_| {
                    storage_driver_error(
                        StorageErrorKind::Precondition,
                        "reverse tunnel buffered upload length overflow",
                    )
                })?;
                if actual_len != size {
                    return Err(storage_driver_error(
                        StorageErrorKind::Precondition,
                        format!(
                            "reverse tunnel buffered upload length mismatch: expected {size}, got {actual_len}"
                        ),
                    ));
                }
                Ok(Bytes::from(data))
            }
        }
    }

    fn clone_for_stream_attempt(&self) -> Option<Self> {
        match self {
            Self::Empty => Some(Self::Empty),
            Self::Bytes(body) => Some(Self::Bytes(body.clone())),
            Self::Reader { .. } => None,
        }
    }
}

pub struct RemoteTransportRequest {
    pub method: Method,
    pub path_and_query: String,
    pub content_type: Option<&'static str>,
    pub body: RemoteRequestBody,
}

pub enum RemoteTransportResponse {
    Direct(reqwest::Response),
    Tunnel(RemoteTunnelHttpResponse),
    TunnelStream(RemoteTunnelStreamHttpResponse),
}

impl RemoteTransportResponse {
    pub fn status(&self) -> StatusCode {
        match self {
            Self::Direct(response) => response.status(),
            Self::Tunnel(response) => response.status,
            Self::TunnelStream(response) => response.status,
        }
    }
}

#[async_trait]
pub trait RemoteTransport: Send + Sync {
    async fn send(&self, request: RemoteTransportRequest) -> Result<RemoteTransportResponse>;

    fn presigned_url(
        &self,
        _method: Method,
        _path_and_query: &str,
        _expires: Duration,
    ) -> Result<String> {
        Err(storage_driver_error(
            StorageErrorKind::Unsupported,
            "remote transport does not support presigned URLs",
        ))
    }
}

pub struct DirectHttpTransport {
    base_url: String,
    access_key: String,
    secret_key: String,
    client: reqwest::Client,
}

impl DirectHttpTransport {
    pub fn new(base_url: &str, access_key: &str, secret_key: &str) -> Result<Self> {
        let base_url = normalize_remote_base_url(base_url)?;
        if base_url.is_empty() {
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
        Ok(Self {
            base_url,
            access_key: access_key.trim().to_string(),
            secret_key: secret_key.to_string(),
            client: remote_http_client()?,
        })
    }

    fn url_for_request(&self, path_and_query: &str) -> Result<reqwest::Url> {
        if !path_and_query.starts_with('/') {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "remote protocol request path must start with '/'",
            ));
        }
        reqwest::Url::parse(&format!("{}{}", self.base_url, path_and_query)).map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("build remote storage url: {error}"),
            )
        })
    }
}

#[async_trait]
impl RemoteTransport for DirectHttpTransport {
    async fn send(&self, request: RemoteTransportRequest) -> Result<RemoteTransportResponse> {
        let content_length = request.body.content_length()?;
        let url = self.url_for_request(&request.path_and_query)?;
        let mut builder = signed_request(
            &self.client,
            &self.access_key,
            &self.secret_key,
            request.method,
            url,
            content_length,
        );
        if let Some(content_type) = request.content_type {
            builder = builder.header(reqwest::header::CONTENT_TYPE, content_type);
        }
        builder = match request.body {
            RemoteRequestBody::Empty => builder,
            RemoteRequestBody::Bytes(body) => builder.body(body),
            RemoteRequestBody::Reader { reader, .. } => {
                let stream = ReaderStream::new(reader).map_err(std::io::Error::other);
                builder.body(reqwest::Body::wrap_stream(stream))
            }
        };

        builder
            .send()
            .await
            .map(RemoteTransportResponse::Direct)
            .map_err(map_reqwest_error)
    }

    fn presigned_url(
        &self,
        method: Method,
        path_and_query: &str,
        expires: Duration,
    ) -> Result<String> {
        let mut url = self.url_for_request(path_and_query)?;
        let request_target = path_and_query_for_url(&url);
        let expires_at = presigned_expires_at(expires)?;
        let signature = super::auth::sign_presigned_request(
            &self.secret_key,
            method.as_str(),
            &request_target,
            &self.access_key,
            expires_at,
        );
        url.query_pairs_mut()
            .append_pair(super::PRESIGNED_AUTH_ACCESS_KEY_QUERY, &self.access_key)
            .append_pair(super::PRESIGNED_AUTH_EXPIRES_QUERY, &expires_at.to_string())
            .append_pair(super::PRESIGNED_AUTH_SIGNATURE_QUERY, &signature);

        Ok(url.to_string())
    }
}

pub struct ReverseTunnelTransport {
    remote_node: managed_follower::Model,
    broker: Arc<dyn RemoteTunnelBroker>,
}

impl ReverseTunnelTransport {
    pub fn new(remote_node: &managed_follower::Model, broker: Arc<dyn RemoteTunnelBroker>) -> Self {
        Self {
            remote_node: remote_node.clone(),
            broker,
        }
    }
}

#[async_trait]
impl RemoteTransport for ReverseTunnelTransport {
    async fn send(&self, request: RemoteTransportRequest) -> Result<RemoteTransportResponse> {
        let content_length = request.body.content_length()?;
        let method =
            HttpMethod::from_bytes(request.method.as_str().as_bytes()).map_err(|error| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    format!("convert reverse tunnel method: {error}"),
                )
            })?;
        let path_and_query = request.path_and_query;
        let extra_headers = request
            .content_type
            .map(|value| (reqwest::header::CONTENT_TYPE.to_string(), value.to_string()))
            .into_iter()
            .collect::<Vec<_>>();

        if self.broker.has_tunnel_stream_lane(&self.remote_node)
            && let Some(stream_body) = request.body.clone_for_stream_attempt()
        {
            match self
                .broker
                .clone()
                .send_tunnel_stream(
                    &self.remote_node,
                    method.clone(),
                    path_and_query.clone(),
                    content_length,
                    extra_headers.clone(),
                    stream_body.into_reader()?,
                )
                .await
            {
                Ok(response) => return Ok(RemoteTransportResponse::TunnelStream(response)),
                Err(error) if should_fallback_stream_error_to_poll(error.message()) => {
                    tracing::warn!(
                        remote_node_id = self.remote_node.id,
                        error = %error,
                        "reverse tunnel stream transport failed; falling back to poll mode"
                    );
                }
                Err(error) => return Err(error),
            }
        }

        let body = request.body.into_buffered_bytes().await?;
        self.broker
            .clone()
            .send_tunnel_request(
                &self.remote_node,
                method,
                path_and_query,
                content_length,
                extra_headers,
                body,
            )
            .await
            .map(RemoteTransportResponse::Tunnel)
    }
}

fn should_fallback_stream_error_to_poll(message: &str) -> bool {
    message.contains("reverse tunnel is offline")
        || message.contains("reverse tunnel streaming lane closed")
        || message.contains("reverse tunnel streaming response channel closed")
        || message
            .contains("reverse tunnel streaming request timed out waiting for follower response")
}

pub async fn ensure_success(response: RemoteTransportResponse, context: &str) -> Result<Vec<u8>> {
    let response = ensure_success_response(response, context).await?;
    match response {
        RemoteTransportResponse::Direct(response) => response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(map_reqwest_error),
        RemoteTransportResponse::Tunnel(response) => Ok(response.body.to_vec()),
        RemoteTransportResponse::TunnelStream(mut response) => {
            let mut body = Vec::new();
            response
                .body
                .read_to_end(&mut body)
                .await
                .map_err(|error| {
                    storage_driver_error(
                        StorageErrorKind::Transient,
                        format!("read reverse tunnel streaming response body: {error}"),
                    )
                })?;
            Ok(body)
        }
    }
}

pub async fn ensure_success_with_body_limit(
    response: RemoteTransportResponse,
    context: &str,
    max_body_bytes: usize,
) -> Result<Vec<u8>> {
    let response = ensure_success_response(response, context).await?;
    match response {
        RemoteTransportResponse::Direct(response) => {
            read_reqwest_response_body_limited(response, context, max_body_bytes).await
        }
        RemoteTransportResponse::Tunnel(response) => {
            ensure_body_within_limit(&response.body, context, max_body_bytes)?;
            Ok(response.body.to_vec())
        }
        RemoteTransportResponse::TunnelStream(response) => {
            read_async_body_limited(response.body, context, max_body_bytes).await
        }
    }
}

pub async fn ensure_success_without_body(
    response: RemoteTransportResponse,
    context: &str,
) -> Result<()> {
    ensure_success_response(response, context).await?;
    Ok(())
}

pub async fn ensure_success_response(
    response: RemoteTransportResponse,
    context: &str,
) -> Result<RemoteTransportResponse> {
    if response.status().is_success() {
        Ok(response)
    } else {
        Err(build_remote_status_error(response, context, false).await)
    }
}

pub async fn build_remote_status_error(
    response: RemoteTransportResponse,
    context: &str,
    not_found_as_record: bool,
) -> AsterError {
    match response {
        RemoteTransportResponse::Direct(response) => {
            let status = response.status();
            let body = match read_reqwest_response_body_limited(
                response,
                context,
                REMOTE_CONTROL_PLANE_BODY_LIMIT,
            )
            .await
            {
                Ok(body) => String::from_utf8_lossy(&body).into_owned(),
                Err(error) => return error,
            };
            build_remote_status_error_from_parts(status, &body, context, not_found_as_record)
        }
        RemoteTransportResponse::Tunnel(response) => {
            if let Err(error) =
                ensure_body_within_limit(&response.body, context, REMOTE_CONTROL_PLANE_BODY_LIMIT)
            {
                return error;
            }
            let body = String::from_utf8_lossy(&response.body);
            build_remote_status_error_from_parts(
                response.status,
                &body,
                context,
                not_found_as_record,
            )
        }
        RemoteTransportResponse::TunnelStream(response) => {
            let body = match read_async_body_limited(
                response.body,
                context,
                REMOTE_CONTROL_PLANE_BODY_LIMIT,
            )
            .await
            {
                Ok(body) => body,
                Err(error) => return error,
            };
            let body = String::from_utf8_lossy(&body);
            build_remote_status_error_from_parts(
                response.status,
                &body,
                context,
                not_found_as_record,
            )
        }
    }
}

pub(super) async fn read_reqwest_response_body_limited(
    response: reqwest::Response,
    context: &str,
    max_body_bytes: usize,
) -> Result<Vec<u8>> {
    let mut body = Vec::with_capacity(max_body_bytes.min(4096));
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(map_reqwest_error)?;
        extend_limited_body(&mut body, &chunk, context, max_body_bytes)?;
    }
    Ok(body)
}

async fn read_async_body_limited(
    mut body: Box<dyn AsyncRead + Unpin + Send>,
    context: &str,
    max_body_bytes: usize,
) -> Result<Vec<u8>> {
    let mut data = Vec::with_capacity(max_body_bytes.min(4096));
    let mut chunk = [0u8; 8192];
    loop {
        let read = body.read(&mut chunk).await.map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Transient,
                format!("read {context} response body: {error}"),
            )
        })?;
        if read == 0 {
            break;
        }
        extend_limited_body(&mut data, &chunk[..read], context, max_body_bytes)?;
    }
    Ok(data)
}

fn extend_limited_body(
    body: &mut Vec<u8>,
    chunk: &[u8],
    context: &str,
    max_body_bytes: usize,
) -> Result<()> {
    let next_len = body.len().checked_add(chunk.len()).ok_or_else(|| {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("{context} response body size overflow"),
        )
    })?;
    if next_len > max_body_bytes {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("{context} response body exceeds {max_body_bytes} bytes limit"),
        ));
    }
    body.extend_from_slice(chunk);
    Ok(())
}

fn ensure_body_within_limit(body: &[u8], context: &str, max_body_bytes: usize) -> Result<()> {
    if body.len() > max_body_bytes {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("{context} response body exceeds {max_body_bytes} bytes limit"),
        ));
    }
    Ok(())
}

pub fn response_stream(response: RemoteTransportResponse) -> Box<dyn AsyncRead + Unpin + Send> {
    match response {
        RemoteTransportResponse::Direct(response) => {
            let stream = response
                .bytes_stream()
                .map_err(|error| std::io::Error::other(error.to_string()));
            Box::new(StreamReader::new(stream))
        }
        RemoteTransportResponse::Tunnel(response) => Box::new(std::io::Cursor::new(response.body)),
        RemoteTransportResponse::TunnelStream(response) => response.body,
    }
}

pub fn signed_headers(
    access_key: &str,
    secret_key: &str,
    method: &Method,
    url: &reqwest::Url,
    content_length: Option<u64>,
) -> Vec<(String, String)> {
    let timestamp = chrono::Utc::now().timestamp();
    let nonce = uuid::Uuid::new_v4().to_string();
    let path_and_query = path_and_query_for_url(url);
    let signature = sign_internal_request(
        secret_key,
        method.as_str(),
        &path_and_query,
        timestamp,
        &nonce,
        content_length,
    );
    vec![
        (
            INTERNAL_AUTH_ACCESS_KEY_HEADER.to_string(),
            access_key.to_string(),
        ),
        (
            INTERNAL_AUTH_TIMESTAMP_HEADER.to_string(),
            timestamp.to_string(),
        ),
        (INTERNAL_AUTH_NONCE_HEADER.to_string(), nonce),
        (INTERNAL_AUTH_SIGNATURE_HEADER.to_string(), signature),
    ]
}

fn signed_request(
    client: &reqwest::Client,
    access_key: &str,
    secret_key: &str,
    method: Method,
    url: reqwest::Url,
    content_length: Option<u64>,
) -> reqwest::RequestBuilder {
    let mut builder = client.request(method.clone(), url.clone());
    for (name, value) in signed_headers(access_key, secret_key, &method, &url, content_length) {
        builder = builder.header(name, value);
    }
    if let Some(content_length) = content_length {
        builder = builder.header(reqwest::header::CONTENT_LENGTH, content_length);
    }
    builder
}

pub fn path_and_query_for_url(url: &reqwest::Url) -> String {
    if let Some(query) = url.query() {
        format!("{}?{query}", url.path())
    } else {
        url.path().to_string()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::remote_protocol::tunnel::server::{
        REMOTE_TUNNEL_BODY_LIMIT, RemoteTunnelHttpResponse, RemoteTunnelStreamHttpResponse,
    };
    use crate::types::RemoteNodeTransportMode;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::io::AsyncReadExt;

    struct TestTunnelBroker {
        stream_available: AtomicBool,
        fail_stream_with_lane_closed: AtomicBool,
        request_calls: AtomicUsize,
        stream_calls: AtomicUsize,
        request_body: Mutex<Vec<u8>>,
        stream_body: Mutex<Vec<u8>>,
        request_headers: Mutex<Vec<(String, String)>>,
        stream_headers: Mutex<Vec<(String, String)>>,
    }

    impl TestTunnelBroker {
        fn new(stream_available: bool) -> Self {
            Self {
                stream_available: AtomicBool::new(stream_available),
                fail_stream_with_lane_closed: AtomicBool::new(false),
                request_calls: AtomicUsize::new(0),
                stream_calls: AtomicUsize::new(0),
                request_body: Mutex::new(Vec::new()),
                stream_body: Mutex::new(Vec::new()),
                request_headers: Mutex::new(Vec::new()),
                stream_headers: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl RemoteTunnelBroker for TestTunnelBroker {
        async fn send_tunnel_request(
            self: Arc<Self>,
            _remote_node: &managed_follower::Model,
            method: HttpMethod,
            path_and_query: String,
            content_length: Option<u64>,
            extra_headers: Vec<(String, String)>,
            body: Bytes,
        ) -> Result<RemoteTunnelHttpResponse> {
            self.request_calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(method, HttpMethod::PUT);
            assert_eq!(path_and_query, "/api/v1/internal/storage/objects/poll.bin");
            assert_eq!(content_length, Some(9));
            *self
                .request_body
                .lock()
                .expect("request body lock should not be poisoned") = body.to_vec();
            *self
                .request_headers
                .lock()
                .expect("request header lock should not be poisoned") = extra_headers;
            Ok(RemoteTunnelHttpResponse {
                status: StatusCode::OK,
                headers: http::HeaderMap::new(),
                body: Bytes::from_static(b"poll-response"),
            })
        }

        async fn send_tunnel_stream(
            self: Arc<Self>,
            _remote_node: &managed_follower::Model,
            method: HttpMethod,
            path_and_query: String,
            content_length: Option<u64>,
            extra_headers: Vec<(String, String)>,
            mut body: Box<dyn AsyncRead + Unpin + Send>,
        ) -> Result<RemoteTunnelStreamHttpResponse> {
            self.stream_calls.fetch_add(1, Ordering::SeqCst);
            if self.fail_stream_with_lane_closed.load(Ordering::SeqCst) {
                return Err(storage_driver_error(
                    StorageErrorKind::Transient,
                    "reverse tunnel streaming lane closed",
                ));
            }
            assert_eq!(method, HttpMethod::PUT);
            assert_eq!(
                path_and_query,
                "/api/v1/internal/storage/objects/stream.bin"
            );
            assert_eq!(content_length, Some(11));
            let mut data = Vec::new();
            body.read_to_end(&mut data)
                .await
                .expect("stream request body should read");
            *self
                .stream_body
                .lock()
                .expect("stream body lock should not be poisoned") = data;
            *self
                .stream_headers
                .lock()
                .expect("stream header lock should not be poisoned") = extra_headers;
            Ok(RemoteTunnelStreamHttpResponse {
                status: StatusCode::OK,
                headers: http::HeaderMap::new(),
                body: Box::new(std::io::Cursor::new(Bytes::from_static(b"stream-response"))),
            })
        }

        fn has_tunnel_stream_lane(&self, _remote_node: &managed_follower::Model) -> bool {
            self.stream_available.load(Ordering::SeqCst)
        }
    }

    fn build_remote_node() -> managed_follower::Model {
        let now = chrono::Utc::now();
        managed_follower::Model {
            id: 9,
            name: "reverse-node".to_string(),
            base_url: String::new(),
            access_key: "access-key".to_string(),
            secret_key: "secret-key".to_string(),
            is_enabled: true,
            transport_mode: RemoteNodeTransportMode::ReverseTunnel,
            last_capabilities: "{}".to_string(),
            last_error: String::new(),
            last_checked_at: None,
            tunnel_last_error: String::new(),
            tunnel_last_seen_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[tokio::test]
    async fn reverse_tunnel_transport_uses_poll_fallback_without_stream_lane() {
        let broker = Arc::new(TestTunnelBroker::new(false));
        let transport = ReverseTunnelTransport::new(&build_remote_node(), broker.clone());

        let response = transport
            .send(RemoteTransportRequest {
                method: Method::PUT,
                path_and_query: "/api/v1/internal/storage/objects/poll.bin".to_string(),
                content_type: Some("application/octet-stream"),
                body: RemoteRequestBody::Reader {
                    reader: Box::new(std::io::Cursor::new(Bytes::from_static(b"poll-body"))),
                    size: 9,
                },
            })
            .await
            .expect("poll fallback request should succeed");

        match response {
            RemoteTransportResponse::Tunnel(response) => {
                assert_eq!(response.status, StatusCode::OK);
                assert_eq!(response.body, Bytes::from_static(b"poll-response"));
            }
            _ => panic!("reverse tunnel without stream lane should use poll fallback"),
        }
        assert_eq!(broker.request_calls.load(Ordering::SeqCst), 1);
        assert_eq!(broker.stream_calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            *broker
                .request_body
                .lock()
                .expect("request body lock should not be poisoned"),
            b"poll-body".to_vec()
        );
        assert!(
            broker
                .request_headers
                .lock()
                .expect("request header lock should not be poisoned")
                .iter()
                .any(|(name, value)| name == "content-type" && value == "application/octet-stream")
        );
    }

    #[tokio::test]
    async fn reverse_tunnel_transport_prefers_stream_lane_when_available() {
        let broker = Arc::new(TestTunnelBroker::new(true));
        let transport = ReverseTunnelTransport::new(&build_remote_node(), broker.clone());

        let response = transport
            .send(RemoteTransportRequest {
                method: Method::PUT,
                path_and_query: "/api/v1/internal/storage/objects/stream.bin".to_string(),
                content_type: Some("application/octet-stream"),
                body: RemoteRequestBody::Bytes(Bytes::from_static(b"stream-body")),
            })
            .await
            .expect("stream lane request should succeed");

        match response {
            RemoteTransportResponse::TunnelStream(mut response) => {
                let mut body = Vec::new();
                response
                    .body
                    .read_to_end(&mut body)
                    .await
                    .expect("stream response body should read");
                assert_eq!(response.status, StatusCode::OK);
                assert_eq!(body, b"stream-response");
            }
            _ => panic!("reverse tunnel with stream lane should use stream transport"),
        }
        assert_eq!(broker.request_calls.load(Ordering::SeqCst), 0);
        assert_eq!(broker.stream_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            *broker
                .stream_body
                .lock()
                .expect("stream body lock should not be poisoned"),
            b"stream-body".to_vec()
        );
        assert!(
            broker
                .stream_headers
                .lock()
                .expect("stream header lock should not be poisoned")
                .iter()
                .any(|(name, value)| name == "content-type" && value == "application/octet-stream")
        );
    }

    #[tokio::test]
    async fn reverse_tunnel_transport_falls_back_to_poll_when_stream_lane_closes() {
        let broker = Arc::new(TestTunnelBroker::new(true));
        broker
            .fail_stream_with_lane_closed
            .store(true, Ordering::SeqCst);
        let transport = ReverseTunnelTransport::new(&build_remote_node(), broker.clone());

        let response = transport
            .send(RemoteTransportRequest {
                method: Method::PUT,
                path_and_query: "/api/v1/internal/storage/objects/poll.bin".to_string(),
                content_type: Some("application/octet-stream"),
                body: RemoteRequestBody::Bytes(Bytes::from_static(b"poll-body")),
            })
            .await
            .expect("closed stream lane should fall back to poll request");

        match response {
            RemoteTransportResponse::Tunnel(response) => {
                assert_eq!(response.status, StatusCode::OK);
                assert_eq!(response.body, Bytes::from_static(b"poll-response"));
            }
            _ => panic!("closed stream lane should use poll fallback"),
        }
        assert_eq!(broker.stream_calls.load(Ordering::SeqCst), 1);
        assert_eq!(broker.request_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            *broker
                .request_body
                .lock()
                .expect("request body lock should not be poisoned"),
            b"poll-body".to_vec()
        );
    }

    #[tokio::test]
    async fn reverse_tunnel_poll_fallback_rejects_oversized_reader_before_dispatch() {
        let broker = Arc::new(TestTunnelBroker::new(false));
        let transport = ReverseTunnelTransport::new(&build_remote_node(), broker.clone());
        let oversized =
            u64::try_from(REMOTE_TUNNEL_BODY_LIMIT).expect("tunnel body limit should fit u64") + 1;

        let result = transport
            .send(RemoteTransportRequest {
                method: Method::PUT,
                path_and_query: "/api/v1/internal/storage/objects/poll.bin".to_string(),
                content_type: Some("application/octet-stream"),
                body: RemoteRequestBody::Reader {
                    reader: Box::new(std::io::Cursor::new(Bytes::new())),
                    size: oversized,
                },
            })
            .await;
        let error = match result {
            Ok(_) => panic!("oversized poll fallback reader should be rejected locally"),
            Err(error) => error,
        };

        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Unsupported)
        );
        assert!(error.message().contains("streaming upload exceeds"));
        assert_eq!(broker.request_calls.load(Ordering::SeqCst), 0);
        assert_eq!(broker.stream_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn limited_body_accumulation_accepts_exact_limit_and_rejects_one_byte_over() {
        let mut body = Vec::new();
        extend_limited_body(&mut body, b"123", "test control plane", 4)
            .expect("body below the limit should be accepted");
        extend_limited_body(&mut body, b"4", "test control plane", 4)
            .expect("body exactly at the limit should be accepted");
        assert_eq!(body, b"1234");

        let error = extend_limited_body(&mut body, b"5", "test control plane", 4)
            .expect_err("body one byte over the limit should be rejected");
        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Misconfigured)
        );
        assert!(error.message().contains("exceeds 4 bytes limit"));
        assert_eq!(body, b"1234", "rejected chunk must not be appended");

        ensure_body_within_limit(&body, "test buffered body", 4)
            .expect("buffered body exactly at the limit should be accepted");
        let error = ensure_body_within_limit(b"12345", "test buffered body", 4)
            .expect_err("buffered body one byte over the limit should be rejected");
        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Misconfigured)
        );
    }

    #[tokio::test]
    async fn limited_async_body_reader_enforces_exact_boundary() {
        let exact = read_async_body_limited(
            Box::new(std::io::Cursor::new(Bytes::from_static(b"1234"))),
            "test stream",
            4,
        )
        .await
        .expect("stream exactly at the limit should be accepted");
        assert_eq!(exact, b"1234");

        let error = read_async_body_limited(
            Box::new(std::io::Cursor::new(Bytes::from_static(b"12345"))),
            "test stream",
            4,
        )
        .await
        .expect_err("stream one byte over the limit should be rejected");
        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Misconfigured)
        );
        assert!(error.message().contains("exceeds 4 bytes limit"));
    }
}
