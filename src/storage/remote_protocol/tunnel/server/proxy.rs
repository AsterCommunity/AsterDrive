use std::sync::Arc;
use std::{collections::HashSet, str};

use async_trait::async_trait;
use base64::Engine as _;
use bytes::Bytes;
use futures::{StreamExt, TryStreamExt};
use hmac::Mac as _;
use http::{HeaderMap, Method};
use serde::Deserialize;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio_util::io::{ReaderStream, StreamReader};

use super::{
    REMOTE_TUNNEL_BODY_LIMIT, RemoteTunnelBroker, RemoteTunnelHttpResponse,
    RemoteTunnelOwnerDirectory, RemoteTunnelRegistry, RemoteTunnelStreamHttpResponse,
};
use crate::db::repository::managed_follower_repo;
use crate::entities::managed_follower;
use crate::errors::{AsterError, Result};
use crate::runtime::{RemoteProtocolRuntimeState, SharedRuntimeState};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::remote_protocol::tunnel::is_allowed_tunnel_target;
use crate::storage::remote_protocol::{
    INTERNAL_AUTH_ACCESS_KEY_HEADER, INTERNAL_AUTH_NONCE_HEADER, INTERNAL_AUTH_NONCE_TTL_SECS,
    INTERNAL_AUTH_SIGNATURE_HEADER, INTERNAL_AUTH_SKEW_SECS, INTERNAL_AUTH_TIMESTAMP_HEADER,
    internal_request_mac, sign_internal_request,
};

pub const REMOTE_TUNNEL_PROXY_PATH_PREFIX: &str = "/api/v1/internal/remote-tunnel/proxy";
const REMOTE_TUNNEL_PROXY_BODY_BRIDGE_CAPACITY: usize = 128 * 1024;
const REMOTE_TUNNEL_PROXY_HEADERS_LIMIT: usize = 16 * 1024;
const REMOTE_TUNNEL_PROXY_HEADER_COUNT_LIMIT: usize = 32;
const HMAC_SHA256_SIGNATURE_LEN: usize = 32;

#[derive(Debug, Deserialize)]
pub struct RemoteTunnelProxyQuery {
    method: String,
    path_and_query: String,
    fencing_token: String,
    #[serde(default)]
    headers: String,
}

pub struct ClusterRemoteTunnelBroker {
    local: Arc<RemoteTunnelRegistry>,
    directory: Arc<RemoteTunnelOwnerDirectory>,
    client: reqwest::Client,
}

impl ClusterRemoteTunnelBroker {
    pub fn new(
        local: Arc<RemoteTunnelRegistry>,
        directory: Arc<RemoteTunnelOwnerDirectory>,
    ) -> Self {
        Self {
            local,
            directory,
            client: reqwest::Client::new(),
        }
    }

    async fn current_owner(
        &self,
        remote_node_id: i64,
    ) -> Result<Option<super::RemoteTunnelOwnerLease>> {
        self.directory.current_owner(remote_node_id).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn send_proxy_stream(
        &self,
        owner: &super::RemoteTunnelOwnerLease,
        remote_node: &managed_follower::Model,
        method: Method,
        path_and_query: String,
        content_length: Option<u64>,
        extra_headers: Vec<(String, String)>,
        body: Box<dyn AsyncRead + Unpin + Send>,
    ) -> Result<RemoteTunnelStreamHttpResponse> {
        let proxy_path = format!("{REMOTE_TUNNEL_PROXY_PATH_PREFIX}/{}", remote_node.id);
        let mut url = reqwest::Url::parse(&format!("{}{}", owner.internal_endpoint, proxy_path))
            .map_err(|error| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    format!("build reverse tunnel owner proxy URL: {error}"),
                )
            })?;
        let encoded_headers = encode_proxy_headers(&extra_headers)?;
        url.query_pairs_mut()
            .append_pair("method", method.as_str())
            .append_pair("path_and_query", &path_and_query)
            .append_pair("fencing_token", &owner.fencing_token)
            .append_pair("headers", &encoded_headers);

        let request_target = match url.query() {
            Some(query) => format!("{}?{query}", url.path()),
            None => url.path().to_string(),
        };
        let timestamp = chrono::Utc::now().timestamp();
        let nonce = aster_forge_utils::id::new_uuid();
        let signature = sign_internal_request(
            self.directory.proxy_secret(),
            reqwest::Method::POST.as_str(),
            &request_target,
            timestamp,
            &nonce,
            content_length,
        );
        let body_stream = ReaderStream::new(body);
        let mut request = self
            .client
            .post(url)
            .header(INTERNAL_AUTH_ACCESS_KEY_HEADER, self.directory.runtime_id())
            .header(INTERNAL_AUTH_TIMESTAMP_HEADER, timestamp.to_string())
            .header(INTERNAL_AUTH_NONCE_HEADER, nonce)
            .header(INTERNAL_AUTH_SIGNATURE_HEADER, signature)
            .body(reqwest::Body::wrap_stream(body_stream));
        if let Some(content_length) = content_length {
            request = request.header(reqwest::header::CONTENT_LENGTH, content_length);
        }
        let response = request.send().await.map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Transient,
                format!("send reverse tunnel owner proxy request: {error}"),
            )
        })?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(storage_driver_error(
                StorageErrorKind::Transient,
                format!(
                    "reverse tunnel owner proxy returned HTTP {status}: {}",
                    body.trim()
                ),
            ));
        }
        let headers = response.headers().clone();
        let stream = response
            .bytes_stream()
            .map_err(|error| std::io::Error::other(error.to_string()));
        Ok(RemoteTunnelStreamHttpResponse {
            status,
            headers,
            body: Box::new(StreamReader::new(stream)),
        })
    }
}

#[async_trait]
impl RemoteTunnelBroker for ClusterRemoteTunnelBroker {
    async fn send_tunnel_request(
        self: Arc<Self>,
        remote_node: &managed_follower::Model,
        method: Method,
        path_and_query: String,
        content_length: Option<u64>,
        extra_headers: Vec<(String, String)>,
        body: Bytes,
    ) -> Result<RemoteTunnelHttpResponse> {
        let owner = self.current_owner(remote_node.id).await?;
        if owner
            .as_ref()
            .is_some_and(|owner| self.directory.is_local_owner(owner))
        {
            return self
                .directory
                .run_while_owned(
                    remote_node.id,
                    self.local.clone().send_tunnel_request(
                        remote_node,
                        method,
                        path_and_query,
                        content_length,
                        extra_headers,
                        body,
                    ),
                )
                .await;
        }
        let Some(owner) = owner else {
            return self
                .local
                .clone()
                .send_tunnel_request(
                    remote_node,
                    method,
                    path_and_query,
                    content_length,
                    extra_headers,
                    body,
                )
                .await;
        };
        let response = self
            .send_proxy_stream(
                &owner,
                remote_node,
                method,
                path_and_query,
                content_length,
                extra_headers,
                Box::new(std::io::Cursor::new(body)),
            )
            .await?;
        let RemoteTunnelStreamHttpResponse {
            status,
            headers,
            body,
        } = response;
        let mut reader = body.take(
            u64::try_from(REMOTE_TUNNEL_BODY_LIMIT)
                .unwrap_or(u64::MAX)
                .saturating_add(1),
        );
        let mut body = Vec::new();
        reader.read_to_end(&mut body).await.map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Transient,
                format!("read buffered tunnel owner proxy response: {error}"),
            )
        })?;
        if body.len() > REMOTE_TUNNEL_BODY_LIMIT {
            return Err(storage_driver_error(
                StorageErrorKind::Unsupported,
                "reverse tunnel owner proxy response exceeds buffered body limit",
            ));
        }
        Ok(RemoteTunnelHttpResponse {
            status,
            headers,
            body: Bytes::from(body),
        })
    }

    async fn send_tunnel_stream(
        self: Arc<Self>,
        remote_node: &managed_follower::Model,
        method: Method,
        path_and_query: String,
        content_length: Option<u64>,
        extra_headers: Vec<(String, String)>,
        body: Box<dyn AsyncRead + Unpin + Send>,
    ) -> Result<RemoteTunnelStreamHttpResponse> {
        let Some(owner) = self.current_owner(remote_node.id).await? else {
            return self
                .local
                .clone()
                .send_tunnel_stream(
                    remote_node,
                    method,
                    path_and_query,
                    content_length,
                    extra_headers,
                    body,
                )
                .await;
        };
        if self.directory.is_local_owner(&owner) {
            return self
                .local
                .clone()
                .send_tunnel_stream(
                    remote_node,
                    method,
                    path_and_query,
                    content_length,
                    extra_headers,
                    body,
                )
                .await;
        }
        self.send_proxy_stream(
            &owner,
            remote_node,
            method,
            path_and_query,
            content_length,
            extra_headers,
            body,
        )
        .await
    }

    fn has_tunnel_stream_lane(&self, _remote_node: &managed_follower::Model) -> bool {
        true
    }
}

pub async fn proxy_tunnel_request<S: RemoteProtocolRuntimeState>(
    state: &S,
    req: &actix_web::HttpRequest,
    remote_node_id: i64,
    query: RemoteTunnelProxyQuery,
    body: actix_web::web::Payload,
) -> Result<actix_web::HttpResponse> {
    let directory = state
        .remote_protocol()
        .tunnel_owner_directory()
        .ok_or_else(|| AsterError::auth_invalid_credentials("tunnel owner proxy is disabled"))?;
    let content_length = req
        .headers()
        .get(actix_web::http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    authorize_proxy_request(
        state,
        &directory,
        req,
        remote_node_id,
        &query.fencing_token,
        content_length,
    )
    .await?;

    let remote_node = managed_follower_repo::find_by_id(state.writer_db(), remote_node_id).await?;
    if !remote_node.is_enabled {
        return Err(AsterError::validation_error("remote node is disabled"));
    }
    let method = Method::from_bytes(query.method.as_bytes()).map_err(|error| {
        AsterError::validation_error(format!("invalid tunnel proxy method: {error}"))
    })?;
    if !is_allowed_tunnel_target(&query.path_and_query) {
        return Err(AsterError::validation_error(
            "tunnel proxy can only target internal storage paths",
        ));
    }
    let extra_headers = decode_proxy_headers(&query.headers)?;
    let body_reader = bridge_payload(body);
    let registry = state.remote_protocol().tunnel_registry().clone();

    directory
        .run_while_owned(remote_node_id, async move {
            if registry.has_tunnel_stream_lane(&remote_node) {
                let response = registry
                    .send_stream(
                        &remote_node,
                        method,
                        query.path_and_query,
                        content_length,
                        extra_headers,
                        body_reader,
                    )
                    .await?;
                return Ok(streaming_proxy_response(response));
            }

            let mut limited = body_reader.take(
                u64::try_from(REMOTE_TUNNEL_BODY_LIMIT)
                    .unwrap_or(u64::MAX)
                    .saturating_add(1),
            );
            let mut buffered = Vec::new();
            limited.read_to_end(&mut buffered).await.map_err(|error| {
                storage_driver_error(
                    StorageErrorKind::Transient,
                    format!("buffer tunnel owner proxy request: {error}"),
                )
            })?;
            if buffered.len() > REMOTE_TUNNEL_BODY_LIMIT {
                return Err(storage_driver_error(
                    StorageErrorKind::Unsupported,
                    "tunnel owner proxy request exceeds buffered body limit",
                ));
            }
            let response = registry
                .send(
                    &remote_node,
                    method,
                    query.path_and_query,
                    content_length,
                    extra_headers,
                    Bytes::from(buffered),
                )
                .await?;
            Ok(buffered_proxy_response(response))
        })
        .await
}

fn bridge_payload(mut body: actix_web::web::Payload) -> Box<dyn AsyncRead + Unpin + Send> {
    let (mut writer, reader) = tokio::io::duplex(REMOTE_TUNNEL_PROXY_BODY_BRIDGE_CAPACITY);
    actix_web::rt::spawn(async move {
        while let Some(chunk) = body.next().await {
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(error) => {
                    tracing::warn!("failed to read reverse tunnel proxy request body: {error}");
                    return;
                }
            };
            if writer.write_all(&chunk).await.is_err() {
                return;
            }
        }
        if let Err(error) = writer.shutdown().await {
            tracing::debug!("failed to finish reverse tunnel proxy request body: {error}");
        }
    });
    Box::new(reader)
}

async fn authorize_proxy_request<S: SharedRuntimeState>(
    state: &S,
    directory: &RemoteTunnelOwnerDirectory,
    req: &actix_web::HttpRequest,
    remote_node_id: i64,
    fencing_token: &str,
    content_length: Option<u64>,
) -> Result<()> {
    let runtime_id = proxy_header(req.headers(), INTERNAL_AUTH_ACCESS_KEY_HEADER)?;
    let timestamp = proxy_header(req.headers(), INTERNAL_AUTH_TIMESTAMP_HEADER)?
        .parse::<i64>()
        .map_err(|_| AsterError::auth_token_invalid("invalid tunnel proxy timestamp"))?;
    let nonce = proxy_header(req.headers(), INTERNAL_AUTH_NONCE_HEADER)?;
    let signature = proxy_header(req.headers(), INTERNAL_AUTH_SIGNATURE_HEADER)?;
    if (chrono::Utc::now().timestamp() - timestamp).abs() > INTERNAL_AUTH_SKEW_SECS {
        return Err(AsterError::auth_token_invalid(
            "tunnel proxy timestamp is outside allowed skew",
        ));
    }
    let request_target = req
        .uri()
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or_else(|| req.path());
    let signature = hex::decode(signature)
        .map_err(|_| AsterError::auth_invalid_credentials("tunnel proxy signature mismatch"))?;
    if signature.len() != HMAC_SHA256_SIGNATURE_LEN {
        return Err(AsterError::auth_invalid_credentials(
            "tunnel proxy signature mismatch",
        ));
    }
    let valid_signature = internal_request_mac(
        directory.proxy_secret(),
        req.method().as_str(),
        request_target,
        timestamp,
        &nonce,
        content_length,
    )
    .verify_slice(&signature)
    .is_ok();
    if !valid_signature {
        return Err(AsterError::auth_invalid_credentials(
            "tunnel proxy signature mismatch",
        ));
    }
    if !directory
        .verify_local_fencing(remote_node_id, fencing_token)
        .await?
    {
        return Err(AsterError::auth_invalid_credentials(
            "tunnel proxy fencing token is stale",
        ));
    }
    let nonce_key = format!("remote_tunnel_proxy_nonce:{runtime_id}:{nonce}");
    if !state
        .cache()
        .set_bytes_if_absent(&nonce_key, Vec::new(), Some(INTERNAL_AUTH_NONCE_TTL_SECS))
        .await
    {
        return Err(AsterError::auth_token_invalid(
            "tunnel proxy nonce has already been used",
        ));
    }
    Ok(())
}

fn decode_proxy_headers(encoded: &str) -> Result<Vec<(String, String)>> {
    if encoded.is_empty() {
        return Ok(Vec::new());
    }
    if encoded.len() > REMOTE_TUNNEL_PROXY_HEADERS_LIMIT.saturating_mul(2) {
        return Err(AsterError::validation_error(
            "tunnel proxy headers exceed encoded size limit",
        ));
    }
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|error| AsterError::validation_error(format!("decode proxy headers: {error}")))?;
    if bytes.len() > REMOTE_TUNNEL_PROXY_HEADERS_LIMIT {
        return Err(AsterError::validation_error(
            "tunnel proxy headers exceed decoded size limit",
        ));
    }
    let headers: Vec<(String, String)> = serde_json::from_slice(&bytes)
        .map_err(|error| AsterError::validation_error(format!("parse proxy headers: {error}")))?;
    validate_proxy_headers(&headers)?;
    Ok(headers)
}

fn encode_proxy_headers(headers: &[(String, String)]) -> Result<String> {
    validate_proxy_headers(headers)?;
    let bytes = serde_json::to_vec(headers).map_err(|error| {
        AsterError::internal_error(format!("encode tunnel proxy headers: {error}"))
    })?;
    if bytes.len() > REMOTE_TUNNEL_PROXY_HEADERS_LIMIT {
        return Err(AsterError::validation_error(
            "tunnel proxy headers exceed decoded size limit",
        ));
    }
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

fn validate_proxy_headers(headers: &[(String, String)]) -> Result<()> {
    if headers.len() > REMOTE_TUNNEL_PROXY_HEADER_COUNT_LIMIT {
        return Err(AsterError::validation_error(
            "tunnel proxy header count exceeds limit",
        ));
    }
    for (name, value) in headers {
        let name = http::HeaderName::from_bytes(name.as_bytes()).map_err(|error| {
            AsterError::validation_error(format!("invalid tunnel proxy header name: {error}"))
        })?;
        http::HeaderValue::from_str(value).map_err(|error| {
            AsterError::validation_error(format!("invalid tunnel proxy header value: {error}"))
        })?;
        if is_hop_by_hop_header(name.as_str()) {
            return Err(AsterError::validation_error(format!(
                "tunnel proxy header {} is hop-by-hop",
                name.as_str()
            )));
        }
    }
    Ok(())
}

fn proxy_header(headers: &actix_web::http::header::HeaderMap, name: &str) -> Result<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| AsterError::auth_token_invalid(format!("missing header {name}")))
}

fn streaming_proxy_response(response: RemoteTunnelStreamHttpResponse) -> actix_web::HttpResponse {
    let mut builder = actix_web::HttpResponse::build(
        actix_web::http::StatusCode::from_u16(response.status.as_u16())
            .unwrap_or(actix_web::http::StatusCode::BAD_GATEWAY),
    );
    copy_proxy_headers(&mut builder, &response.headers);
    builder.streaming(ReaderStream::new(response.body))
}

fn buffered_proxy_response(response: RemoteTunnelHttpResponse) -> actix_web::HttpResponse {
    let mut builder = actix_web::HttpResponse::build(
        actix_web::http::StatusCode::from_u16(response.status.as_u16())
            .unwrap_or(actix_web::http::StatusCode::BAD_GATEWAY),
    );
    copy_proxy_headers(&mut builder, &response.headers);
    builder.body(response.body)
}

fn copy_proxy_headers(builder: &mut actix_web::HttpResponseBuilder, headers: &HeaderMap) {
    let connection_headers = headers
        .get_all(http::header::CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<HashSet<_>>();
    for (name, value) in headers {
        if is_hop_by_hop_header(name.as_str())
            || connection_headers.contains(&name.as_str().to_ascii_lowercase())
        {
            continue;
        }
        let Ok(name) = actix_web::http::header::HeaderName::from_bytes(name.as_str().as_bytes())
        else {
            continue;
        };
        let Ok(value) = actix_web::http::header::HeaderValue::from_bytes(value.as_bytes()) else {
            continue;
        };
        builder.append_header((name, value));
    }
}

fn is_hop_by_hop_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, DeploymentProfile};
    use crate::entities::managed_follower;
    use crate::runtime::test_support::CacheOnlyState;
    use migration::Migrator;
    use sea_orm::{ActiveModelTrait, Set};

    #[test]
    fn proxy_headers_round_trip_and_reject_malformed_payloads() {
        let headers = vec![("content-type".to_string(), "application/json".to_string())];
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&headers).unwrap());
        assert_eq!(decode_proxy_headers(&encoded).unwrap(), headers);
        assert!(decode_proxy_headers("%%%").is_err());
        let malformed_json = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"not-json");
        assert!(decode_proxy_headers(&malformed_json).is_err());
    }

    #[test]
    fn proxy_headers_reject_invalid_hop_by_hop_and_oversized_values() {
        assert!(encode_proxy_headers(&[("connection".into(), "close".into())]).is_err());
        assert!(encode_proxy_headers(&[("x-test".into(), "bad\r\nvalue".into())]).is_err());
        assert!(
            encode_proxy_headers(&[(
                "x-test".into(),
                "x".repeat(REMOTE_TUNNEL_PROXY_HEADERS_LIMIT)
            )])
            .is_err()
        );
        assert!(
            encode_proxy_headers(
                &(0..=REMOTE_TUNNEL_PROXY_HEADER_COUNT_LIMIT)
                    .map(|index| (format!("x-test-{index}"), "value".into()))
                    .collect::<Vec<_>>()
            )
            .is_err()
        );
    }

    #[test]
    fn hop_by_hop_headers_are_filtered_case_insensitively() {
        assert!(is_hop_by_hop_header("Connection"));
        assert!(is_hop_by_hop_header("transfer-encoding"));
        assert!(!is_hop_by_hop_header("content-type"));
    }

    #[test]
    fn response_connection_tokens_are_filtered() {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONNECTION,
            "x-private, keep-alive".parse().unwrap(),
        );
        headers.insert("x-private", "secret".parse().unwrap());
        headers.insert(http::header::CONTENT_TYPE, "text/plain".parse().unwrap());

        let connection_headers = headers
            .get_all(http::header::CONNECTION)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(','))
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .collect::<HashSet<_>>();
        assert!(connection_headers.contains("x-private"));
        assert!(!connection_headers.contains("content-type"));
    }

    async fn claimed_directory() -> RemoteTunnelOwnerDirectory {
        let db = crate::db::connect_with_metrics(
            &crate::config::DatabaseConfig {
                url: "sqlite::memory:".to_string(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics::NoopMetrics::arc(),
        )
        .await
        .unwrap();
        Migrator::up(&db, None).await.unwrap();
        let now = chrono::Utc::now();
        managed_follower::ActiveModel {
            id: Set(7),
            name: Set("proxy follower".to_string()),
            base_url: Set(String::new()),
            access_key: Set("access".to_string()),
            secret_key: Set("secret".to_string()),
            is_enabled: Set(true),
            transport_mode: Set(crate::types::RemoteNodeTransportMode::ReverseTunnel),
            last_capabilities: Set("{}".to_string()),
            last_error: Set(String::new()),
            last_checked_at: Set(None),
            tunnel_last_error: Set(String::new()),
            tunnel_last_seen_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&db)
        .await
        .unwrap();
        let mut config = Config::default();
        config.deployment.profile = DeploymentProfile::Cluster;
        config.deployment.internal_endpoint = "http://primary-a:3000".to_string();
        config.deployment.internal_proxy_secret =
            "proxy-secret-for-tests-at-least-32-bytes".to_string();
        let directory =
            RemoteTunnelOwnerDirectory::from_deployment(db, &config.deployment, "runtime-a")
                .unwrap()
                .unwrap();
        directory.try_claim(7).await.unwrap();
        directory
    }

    fn proxy_auth_request(
        secret: &str,
        timestamp: i64,
        nonce: &str,
        fencing_token: &str,
    ) -> actix_web::HttpRequest {
        let mut url = reqwest::Url::parse(&format!(
            "http://primary-a:3000{REMOTE_TUNNEL_PROXY_PATH_PREFIX}/7"
        ))
        .unwrap();
        url.query_pairs_mut()
            .append_pair("method", "GET")
            .append_pair(
                "path_and_query",
                &format!(
                    "{}/capabilities",
                    crate::storage::remote_protocol::INTERNAL_STORAGE_BASE_PATH
                ),
            )
            .append_pair("fencing_token", fencing_token)
            .append_pair("headers", "W10");
        let uri = format!("{}?{}", url.path(), url.query().unwrap());
        let signature = sign_internal_request(secret, "POST", &uri, timestamp, nonce, None);
        actix_web::test::TestRequest::post()
            .uri(&uri)
            .insert_header((INTERNAL_AUTH_ACCESS_KEY_HEADER, "runtime-b"))
            .insert_header((INTERNAL_AUTH_TIMESTAMP_HEADER, timestamp.to_string()))
            .insert_header((INTERNAL_AUTH_NONCE_HEADER, nonce))
            .insert_header((INTERNAL_AUTH_SIGNATURE_HEADER, signature))
            .to_http_request()
    }

    #[actix_web::test]
    async fn proxy_auth_accepts_once_and_rejects_nonce_replay() {
        let state = CacheOnlyState::new().await;
        let directory = claimed_directory().await;
        let timestamp = chrono::Utc::now().timestamp();
        let request = proxy_auth_request(
            directory.proxy_secret(),
            timestamp,
            "replay-nonce",
            directory.fencing_token(),
        );
        authorize_proxy_request(
            &state,
            &directory,
            &request,
            7,
            directory.fencing_token(),
            None,
        )
        .await
        .unwrap();

        let replay = proxy_auth_request(
            directory.proxy_secret(),
            timestamp,
            "replay-nonce",
            directory.fencing_token(),
        );
        let error = authorize_proxy_request(
            &state,
            &directory,
            &replay,
            7,
            directory.fencing_token(),
            None,
        )
        .await
        .expect_err("reused proxy nonce must be rejected");
        assert!(error.message().contains("already been used"));
    }

    #[actix_web::test]
    async fn proxy_auth_rejects_missing_expired_wrong_and_short_signatures() {
        let state = CacheOnlyState::new().await;
        let directory = claimed_directory().await;
        let now = chrono::Utc::now().timestamp();

        let missing = actix_web::test::TestRequest::post()
            .uri(&format!("{REMOTE_TUNNEL_PROXY_PATH_PREFIX}/7"))
            .to_http_request();
        assert!(
            authorize_proxy_request(
                &state,
                &directory,
                &missing,
                7,
                directory.fencing_token(),
                None,
            )
            .await
            .unwrap_err()
            .message()
            .contains("missing header")
        );

        let expired_timestamp = now - INTERNAL_AUTH_SKEW_SECS - 1;
        let expired = proxy_auth_request(
            directory.proxy_secret(),
            expired_timestamp,
            "expired-nonce",
            directory.fencing_token(),
        );
        assert!(
            authorize_proxy_request(
                &state,
                &directory,
                &expired,
                7,
                directory.fencing_token(),
                None,
            )
            .await
            .unwrap_err()
            .message()
            .contains("outside allowed skew")
        );

        let wrong = proxy_auth_request(
            "wrong-secret",
            now,
            "wrong-secret-nonce",
            directory.fencing_token(),
        );
        assert!(
            authorize_proxy_request(
                &state,
                &directory,
                &wrong,
                7,
                directory.fencing_token(),
                None,
            )
            .await
            .unwrap_err()
            .message()
            .contains("signature mismatch")
        );

        let short = proxy_auth_request(
            directory.proxy_secret(),
            now,
            "short-signature-nonce",
            directory.fencing_token(),
        );
        let short = actix_web::test::TestRequest::post()
            .uri(short.uri().path_and_query().unwrap().as_str())
            .insert_header((INTERNAL_AUTH_ACCESS_KEY_HEADER, "runtime-b"))
            .insert_header((INTERNAL_AUTH_TIMESTAMP_HEADER, now.to_string()))
            .insert_header((INTERNAL_AUTH_NONCE_HEADER, "short-signature-nonce"))
            .insert_header((INTERNAL_AUTH_SIGNATURE_HEADER, "00"))
            .to_http_request();
        let error = authorize_proxy_request(
            &state,
            &directory,
            &short,
            7,
            directory.fencing_token(),
            None,
        )
        .await
        .expect_err("short HMAC signature must be rejected");
        assert!(error.message().contains("signature mismatch"));
    }
}
