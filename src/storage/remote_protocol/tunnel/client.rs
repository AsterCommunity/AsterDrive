//! Follower-side reverse tunnel client.

use super::{
    is_allowed_tunnel_target,
    server::{
        REMOTE_TUNNEL_BODY_LIMIT, REMOTE_TUNNEL_COMPLETE_PATH, REMOTE_TUNNEL_CONNECT_PATH,
        REMOTE_TUNNEL_POLL_PATH, REMOTE_TUNNEL_STREAM_CHUNK_SIZE, RemoteTunnelPollResponse,
        RemoteTunnelRequest, RemoteTunnelResponse, RemoteTunnelStreamFrame,
        RemoteTunnelStreamFrameKind, decode_stream_frame, encode_stream_frame,
        tunnel_response_from_reqwest,
    },
};
use crate::api::api_error_code::ApiErrorCode;
use crate::config::OUTBOUND_HTTP_USER_AGENT;
use crate::db::repository::master_binding_repo;
use crate::entities::master_binding;
use crate::errors::{AsterError, Result};
use crate::runtime::{FollowerAppState, SharedRuntimeState};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::remote_protocol::transport::read_reqwest_response_body_limited;
use crate::storage::remote_protocol::{
    INTERNAL_AUTH_ACCESS_KEY_HEADER, INTERNAL_AUTH_NONCE_HEADER, INTERNAL_AUTH_SIGNATURE_HEADER,
    INTERNAL_AUTH_TIMESTAMP_HEADER, REMOTE_CONTROL_PLANE_BODY_LIMIT, sign_internal_request,
};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use reqwest::Method;
use std::collections::HashMap;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};
use tokio_util::sync::CancellationToken;

const FOLLOWER_TUNNEL_STREAM_LANES: usize = 4;
const FOLLOWER_TUNNEL_BASE_BACKOFF: Duration = Duration::from_secs(1);
const FOLLOWER_TUNNEL_MAX_BACKOFF: Duration = Duration::from_secs(30);
const FOLLOWER_TUNNEL_RECONCILE_INTERVAL: Duration = Duration::from_secs(30);
const FOLLOWER_TUNNEL_WORKER_JOIN_TIMEOUT: Duration = Duration::from_secs(5);
const FOLLOWER_TUNNEL_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const FOLLOWER_TUNNEL_READ_TIMEOUT: Duration = Duration::from_secs(30);
const FOLLOWER_TUNNEL_OPERATION_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const FORBIDDEN_TUNNEL_TARGET_BODY: &[u8] = b"reverse tunnel can only proxy internal storage paths";
const REVERSE_TUNNEL_STREAM_REQUEST_BODY_CHANNEL_CAPACITY: usize = 16;

#[derive(Debug, serde::Deserialize)]
struct ApiEnvelope<T> {
    code: ApiErrorCode,
    msg: String,
    data: Option<T>,
}

struct BindingTunnelWorker {
    fingerprint: String,
    shutdown_token: CancellationToken,
    handle: JoinHandle<()>,
}

pub async fn run_follower_tunnel_worker(
    state: actix_web::web::Data<FollowerAppState>,
    shutdown_token: CancellationToken,
) {
    let client = match tunnel_http_client() {
        Ok(client) => client,
        Err(error) => {
            tracing::warn!("failed to build reverse tunnel client: {error}");
            return;
        }
    };
    let local_base_url = format!("http://127.0.0.1:{}", state.config().server.port);
    let mut workers = HashMap::new();

    loop {
        if shutdown_token.is_cancelled() {
            break;
        }
        reconcile_binding_workers(
            state.get_ref(),
            &client,
            &local_base_url,
            &shutdown_token,
            &mut workers,
        )
        .await;

        tokio::select! {
            biased;
            _ = shutdown_token.cancelled() => break,
            _ = tokio::time::sleep(FOLLOWER_TUNNEL_RECONCILE_INTERVAL) => {}
        }
    }

    stop_all_binding_workers(workers).await;
}

async fn reconcile_binding_workers(
    state: &FollowerAppState,
    client: &reqwest::Client,
    local_base_url: &str,
    parent_shutdown: &CancellationToken,
    workers: &mut HashMap<i64, BindingTunnelWorker>,
) {
    let bindings = match master_binding_repo::find_all(state.writer_db()).await {
        Ok(bindings) => bindings,
        Err(error) => {
            tracing::warn!("failed to load master bindings for reverse tunnel polling: {error}");
            return;
        }
    };

    let enabled = bindings
        .into_iter()
        .filter(|binding| binding.is_enabled)
        .collect::<Vec<_>>();

    let enabled_ids = enabled.iter().map(|binding| binding.id).collect::<Vec<_>>();
    let stale_ids = workers
        .keys()
        .copied()
        .filter(|id| !enabled_ids.contains(id))
        .collect::<Vec<_>>();
    for id in stale_ids {
        if let Some(worker) = workers.remove(&id) {
            stop_binding_worker(worker).await;
        }
    }

    for binding in enabled {
        let binding_id = binding.id;
        let fingerprint = binding_worker_fingerprint(&binding);
        if workers
            .get(&binding_id)
            .map(|worker| worker.fingerprint == fingerprint)
            .unwrap_or(false)
        {
            continue;
        }

        if let Some(worker) = workers.remove(&binding_id) {
            stop_binding_worker(worker).await;
        }

        let client = client.clone();
        let local_base_url = local_base_url.to_string();
        let worker_shutdown = parent_shutdown.child_token();
        let loop_shutdown = worker_shutdown.clone();
        let handle = tokio::spawn(async move {
            run_binding_tunnel_loop(client, binding, local_base_url, loop_shutdown).await;
        });
        workers.insert(
            binding_id,
            BindingTunnelWorker {
                fingerprint,
                shutdown_token: worker_shutdown,
                handle,
            },
        );
    }
}

async fn stop_all_binding_workers(workers: HashMap<i64, BindingTunnelWorker>) {
    for worker in workers.into_values() {
        stop_binding_worker(worker).await;
    }
}

async fn stop_binding_worker(mut worker: BindingTunnelWorker) {
    worker.shutdown_token.cancel();
    tokio::select! {
        result = &mut worker.handle => {
            if let Err(error) = result {
                tracing::warn!("reverse tunnel binding worker failed to join: {error}");
            }
        }
        _ = tokio::time::sleep(FOLLOWER_TUNNEL_WORKER_JOIN_TIMEOUT) => {
            worker.handle.abort();
            if let Err(error) = worker.handle.await {
                tracing::warn!("reverse tunnel binding worker aborted: {error}");
            }
        }
    }
}

async fn run_binding_poll_loop(
    client: reqwest::Client,
    binding: master_binding::Model,
    local_base_url: String,
    shutdown_token: CancellationToken,
) {
    let mut backoff = FOLLOWER_TUNNEL_BASE_BACKOFF;
    loop {
        let result = tokio::select! {
            biased;
            _ = shutdown_token.cancelled() => break,
            result = poll_once(&client, &binding, &local_base_url) => result,
        };

        match result {
            Ok(()) => {
                backoff = FOLLOWER_TUNNEL_BASE_BACKOFF;
            }
            Err(error) => {
                if let Err(mark_error) = mark_tunnel_error(&client, &binding, error.message()).await
                {
                    tracing::warn!(
                        access_key = %binding.access_key,
                        master_url = %binding.master_url,
                        "failed to report reverse tunnel error to primary: {mark_error}"
                    );
                }
                tracing::warn!(
                    access_key = %binding.access_key,
                    master_url = %binding.master_url,
                    "reverse tunnel poll failed: {error}"
                );
                tokio::select! {
                    biased;
                    _ = shutdown_token.cancelled() => break,
                    _ = tokio::time::sleep(backoff) => {}
                }
                backoff = backoff.saturating_mul(2).min(FOLLOWER_TUNNEL_MAX_BACKOFF);
            }
        }
    }
}

async fn run_binding_tunnel_loop(
    client: reqwest::Client,
    binding: master_binding::Model,
    local_base_url: String,
    shutdown_token: CancellationToken,
) {
    let stream_shutdown = shutdown_token.child_token();
    let mut stream_handles = Vec::with_capacity(FOLLOWER_TUNNEL_STREAM_LANES);
    for lane_index in 0..FOLLOWER_TUNNEL_STREAM_LANES {
        let lane_client = client.clone();
        let lane_binding = binding.clone();
        let lane_base_url = local_base_url.clone();
        let lane_shutdown = stream_shutdown.child_token();
        stream_handles.push(tokio::spawn(async move {
            run_binding_stream_lane_loop(
                lane_client,
                lane_binding,
                lane_base_url,
                lane_index,
                lane_shutdown,
            )
            .await;
        }));
    }

    let poll_shutdown = shutdown_token.child_token();
    let poll_client = client.clone();
    let poll_binding = binding;
    let poll_base_url = local_base_url;
    let mut poll_handle = tokio::spawn(async move {
        run_binding_poll_loop(poll_client, poll_binding, poll_base_url, poll_shutdown).await;
    });

    tokio::select! {
        biased;
        _ = shutdown_token.cancelled() => {}
        result = &mut poll_handle => {
            if let Err(error) = result {
                tracing::warn!("reverse tunnel poll fallback worker failed to join: {error}");
            }
        }
    }

    stream_shutdown.cancel();
    for handle in stream_handles {
        if let Err(error) = handle.await {
            tracing::warn!("reverse tunnel streaming lane worker failed to join: {error}");
        }
    }
    poll_handle.abort();
    let _ = poll_handle.await;
}

async fn run_binding_stream_lane_loop(
    client: reqwest::Client,
    binding: master_binding::Model,
    local_base_url: String,
    lane_index: usize,
    shutdown_token: CancellationToken,
) {
    let mut backoff = FOLLOWER_TUNNEL_BASE_BACKOFF;
    loop {
        let result = tokio::select! {
            biased;
            _ = shutdown_token.cancelled() => break,
            result = connect_stream_lane_once(&client, &binding, &local_base_url, lane_index, &shutdown_token) => result,
        };

        match result {
            Ok(()) => {
                backoff = FOLLOWER_TUNNEL_BASE_BACKOFF;
            }
            Err(error) => {
                tracing::warn!(
                    access_key = %binding.access_key,
                    master_url = %binding.master_url,
                    lane_index,
                    "reverse tunnel streaming lane failed: {error}"
                );
                tokio::select! {
                    biased;
                    _ = shutdown_token.cancelled() => break,
                    _ = tokio::time::sleep(backoff) => {}
                }
                backoff = backoff.saturating_mul(2).min(FOLLOWER_TUNNEL_MAX_BACKOFF);
            }
        }
    }
}

async fn poll_once(
    client: &reqwest::Client,
    binding: &master_binding::Model,
    local_base_url: &str,
) -> Result<()> {
    let poll_url = format!("{}{}", binding.master_url, REMOTE_TUNNEL_POLL_PATH);
    let body = serde_json::to_vec(&serde_json::json!({ "access_key": binding.access_key }))
        .map_err(|error| AsterError::internal_error(format!("encode tunnel poll: {error}")))?;
    let response = signed_master_request(
        client,
        binding,
        Method::POST,
        &poll_url,
        REMOTE_TUNNEL_POLL_PATH,
        Some(body),
    )
    .await?;
    let poll =
        parse_api_response::<RemoteTunnelPollResponse>(response, "reverse tunnel poll").await?;
    let Some(request) = poll.request else {
        return Ok(());
    };
    let response = execute_tunnel_request(client, local_base_url, &request).await?;
    complete_request(client, binding, response).await
}

async fn connect_stream_lane_once(
    client: &reqwest::Client,
    binding: &master_binding::Model,
    local_base_url: &str,
    lane_index: usize,
    shutdown_token: &CancellationToken,
) -> Result<()> {
    let connect_url = stream_connect_url(&binding.master_url)?;
    let request = signed_master_ws_request(binding, &connect_url)?;
    let (ws, _) = connect_async(request).await.map_err(|error| {
        storage_driver_error(
            StorageErrorKind::Transient,
            format!("connect reverse tunnel streaming lane: {error}"),
        )
    })?;
    tracing::debug!(
        access_key = %binding.access_key,
        master_url = %binding.master_url,
        lane_index,
        "reverse tunnel streaming lane connected"
    );

    let (mut write, mut read) = ws.split();
    while !shutdown_token.is_cancelled() {
        let message = tokio::select! {
            biased;
            _ = shutdown_token.cancelled() => break,
            message = read.next() => message,
        };
        let Some(message) = message else {
            break;
        };
        let message = message.map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Transient,
                format!("read reverse tunnel streaming lane: {error}"),
            )
        })?;
        let frame = match message {
            WsMessage::Binary(bytes) => decode_stream_frame(bytes)?,
            WsMessage::Ping(bytes) => {
                write.send(WsMessage::Pong(bytes)).await.map_err(|error| {
                    storage_driver_error(
                        StorageErrorKind::Transient,
                        format!("pong reverse tunnel streaming lane: {error}"),
                    )
                })?;
                continue;
            }
            WsMessage::Pong(_) => continue,
            WsMessage::Close(_) => break,
            _ => continue,
        };
        if frame.kind != RemoteTunnelStreamFrameKind::RequestStart {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "reverse tunnel streaming lane expected request_start",
            ));
        }
        execute_stream_tunnel_request(client, local_base_url, frame, &mut read, &mut write).await?;
    }

    Ok(())
}

async fn execute_stream_tunnel_request<R, W>(
    client: &reqwest::Client,
    local_base_url: &str,
    start: RemoteTunnelStreamFrame,
    read: &mut R,
    write: &mut W,
) -> Result<()>
where
    R: futures::Stream<
            Item = std::result::Result<WsMessage, tokio_tungstenite::tungstenite::Error>,
        > + Unpin,
    W: futures::Sink<WsMessage, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let request_id = start.request_id.clone();
    let method = start
        .method
        .as_deref()
        .ok_or_else(|| AsterError::validation_error("stream request_start missing method"))?;
    let path_and_query = start.path_and_query.as_deref().ok_or_else(|| {
        AsterError::validation_error("stream request_start missing path_and_query")
    })?;

    if !is_allowed_tunnel_target(path_and_query) {
        send_stream_error_response(
            write,
            &request_id,
            http::StatusCode::FORBIDDEN,
            FORBIDDEN_TUNNEL_TARGET_BODY,
        )
        .await?;
        drain_stream_request_body(&request_id, read).await?;
        return Ok(());
    }

    let method = Method::from_bytes(method.as_bytes()).map_err(|error| {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("invalid reverse tunnel stream request method: {error}"),
        )
    })?;
    let url = format!("{local_base_url}{path_and_query}");
    let (body_tx, body_rx) = tokio::sync::mpsc::channel::<std::io::Result<Bytes>>(
        REVERSE_TUNNEL_STREAM_REQUEST_BODY_CHANNEL_CAPACITY,
    );
    let mut body_tx = Some(body_tx);
    let request_body = reqwest::Body::wrap_stream(async_stream::stream! {
        let mut body_rx = body_rx;
        while let Some(chunk) = body_rx.recv().await {
            yield chunk;
        }
    });
    let mut builder = client.request(method, url);
    for (name, value) in &start.headers {
        builder = builder.header(name, value);
    }

    let response_task = tokio::spawn(async move { builder.body(request_body).send().await });
    let mut body_finished = false;
    while !body_finished {
        let message = read.next().await.ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Transient,
                "reverse tunnel streaming lane closed before request body end",
            )
        })?;
        let message = message.map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Transient,
                format!("read reverse tunnel streaming request body: {error}"),
            )
        })?;
        match message {
            WsMessage::Binary(bytes) => {
                let frame = decode_stream_frame(bytes)?;
                if frame.request_id != request_id {
                    return Err(AsterError::validation_error(
                        "reverse tunnel streaming lane received interleaved request",
                    ));
                }
                match frame.kind {
                    RemoteTunnelStreamFrameKind::RequestBody => {
                        if let Some(tx) = body_tx.as_ref()
                            && tx.send(Ok(frame.body)).await.is_err()
                        {
                            body_tx = None;
                        }
                    }
                    RemoteTunnelStreamFrameKind::RequestEnd => {
                        body_tx = None;
                        body_finished = true;
                    }
                    RemoteTunnelStreamFrameKind::Error => {
                        response_task.abort();
                        return Ok(());
                    }
                    _ => {
                        return Err(AsterError::validation_error(
                            "unexpected reverse tunnel streaming request body frame",
                        ));
                    }
                }
            }
            WsMessage::Ping(bytes) => {
                write.send(WsMessage::Pong(bytes)).await.map_err(|error| {
                    storage_driver_error(
                        StorageErrorKind::Transient,
                        format!("pong reverse tunnel streaming request body: {error}"),
                    )
                })?;
            }
            WsMessage::Pong(_) => {}
            WsMessage::Close(_) => {
                return Err(storage_driver_error(
                    StorageErrorKind::Transient,
                    "reverse tunnel streaming lane closed during request",
                ));
            }
            _ => {}
        }
    }
    drop(body_tx);

    let response = match response_task.await {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => {
            let _ = send_stream_frame(
                write,
                RemoteTunnelStreamFrame::error(
                    request_id,
                    format!("execute reverse tunnel streaming local request: {error}"),
                ),
            )
            .await;
            return Ok(());
        }
        Err(error) => {
            let _ = send_stream_frame(
                write,
                RemoteTunnelStreamFrame::error(
                    request_id,
                    format!("reverse tunnel local request task failed: {error}"),
                ),
            )
            .await;
            return Ok(());
        }
    };
    send_stream_response(write, &request_id, response).await
}

async fn drain_stream_request_body<R>(request_id: &str, read: &mut R) -> Result<()>
where
    R: futures::Stream<
            Item = std::result::Result<WsMessage, tokio_tungstenite::tungstenite::Error>,
        > + Unpin,
{
    loop {
        let Some(message) = read.next().await else {
            return Ok(());
        };
        let message = message.map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Transient,
                format!("drain reverse tunnel streaming request body: {error}"),
            )
        })?;
        let WsMessage::Binary(bytes) = message else {
            continue;
        };
        let frame = decode_stream_frame(bytes)?;
        if frame.request_id != request_id {
            return Err(AsterError::validation_error(
                "reverse tunnel streaming lane received interleaved request while draining",
            ));
        }
        match frame.kind {
            RemoteTunnelStreamFrameKind::RequestEnd | RemoteTunnelStreamFrameKind::Error => {
                return Ok(());
            }
            RemoteTunnelStreamFrameKind::RequestBody => {}
            _ => {
                return Err(AsterError::validation_error(
                    "unexpected reverse tunnel streaming frame while draining request",
                ));
            }
        }
    }
}

async fn send_stream_error_response<W>(
    write: &mut W,
    request_id: &str,
    status: http::StatusCode,
    body: &[u8],
) -> Result<()>
where
    W: futures::Sink<WsMessage, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    send_stream_frame(
        write,
        RemoteTunnelStreamFrame {
            kind: RemoteTunnelStreamFrameKind::ResponseStart,
            request_id: request_id.to_string(),
            method: None,
            path_and_query: None,
            headers: Vec::new(),
            content_length: Some(u64::try_from(body.len()).map_err(|_| {
                storage_driver_error(
                    StorageErrorKind::Precondition,
                    "reverse tunnel streaming error body length overflow",
                )
            })?),
            status: Some(status.as_u16()),
            message: None,
            body: Bytes::new(),
        },
    )
    .await?;
    if !body.is_empty() {
        send_stream_body_chunks(write, request_id, Bytes::copy_from_slice(body)).await?;
    }
    send_stream_frame(
        write,
        RemoteTunnelStreamFrame {
            kind: RemoteTunnelStreamFrameKind::ResponseEnd,
            request_id: request_id.to_string(),
            method: None,
            path_and_query: None,
            headers: Vec::new(),
            content_length: None,
            status: None,
            message: None,
            body: Bytes::new(),
        },
    )
    .await
}

async fn send_stream_response<W>(
    write: &mut W,
    request_id: &str,
    response: reqwest::Response,
) -> Result<()>
where
    W: futures::Sink<WsMessage, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let status = response.status();
    let headers = response_headers_for_reqwest(response.headers());
    send_stream_frame(
        write,
        RemoteTunnelStreamFrame {
            kind: RemoteTunnelStreamFrameKind::ResponseStart,
            request_id: request_id.to_string(),
            method: None,
            path_and_query: None,
            headers,
            content_length: response.content_length(),
            status: Some(status.as_u16()),
            message: None,
            body: Bytes::new(),
        },
    )
    .await?;

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Transient,
                format!("read reverse tunnel streaming local response body: {error}"),
            )
        })?;
        if chunk.is_empty() {
            continue;
        }
        send_stream_body_chunks(write, request_id, chunk).await?;
    }

    send_stream_frame(
        write,
        RemoteTunnelStreamFrame {
            kind: RemoteTunnelStreamFrameKind::ResponseEnd,
            request_id: request_id.to_string(),
            method: None,
            path_and_query: None,
            headers: Vec::new(),
            content_length: None,
            status: None,
            message: None,
            body: Bytes::new(),
        },
    )
    .await
}

async fn send_stream_frame<W>(write: &mut W, frame: RemoteTunnelStreamFrame) -> Result<()>
where
    W: futures::Sink<WsMessage, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let bytes = encode_stream_frame(&frame)?;
    write.send(WsMessage::Binary(bytes)).await.map_err(|error| {
        storage_driver_error(
            StorageErrorKind::Transient,
            format!("send reverse tunnel streaming frame: {error}"),
        )
    })
}

async fn send_stream_body_chunks<W>(write: &mut W, request_id: &str, body: Bytes) -> Result<()>
where
    W: futures::Sink<WsMessage, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    for chunk in body.chunks(REMOTE_TUNNEL_STREAM_CHUNK_SIZE) {
        send_stream_frame(
            write,
            RemoteTunnelStreamFrame {
                kind: RemoteTunnelStreamFrameKind::ResponseBody,
                request_id: request_id.to_string(),
                method: None,
                path_and_query: None,
                headers: Vec::new(),
                content_length: None,
                status: None,
                message: None,
                body: Bytes::copy_from_slice(chunk),
            },
        )
        .await?;
    }
    Ok(())
}

async fn execute_tunnel_request(
    client: &reqwest::Client,
    local_base_url: &str,
    request: &RemoteTunnelRequest,
) -> Result<RemoteTunnelResponse> {
    if !is_allowed_tunnel_target(&request.path_and_query) {
        return Ok(RemoteTunnelResponse {
            request_id: request.request_id.clone(),
            status: 403,
            headers: Vec::new(),
            body: FORBIDDEN_TUNNEL_TARGET_BODY.to_vec(),
        });
    }

    if request.body.len() > REMOTE_TUNNEL_BODY_LIMIT {
        return Ok(RemoteTunnelResponse {
            request_id: request.request_id.clone(),
            status: 413,
            headers: Vec::new(),
            body: b"reverse tunnel request body too large".to_vec(),
        });
    }

    let url = format!("{}{}", local_base_url, request.path_and_query);
    let method = Method::from_bytes(request.method.as_bytes()).map_err(|error| {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("invalid reverse tunnel request method: {error}"),
        )
    })?;
    let mut builder = client.request(method, url);
    for (name, value) in &request.headers {
        builder = builder.header(name, value);
    }
    if !request.body.is_empty() {
        builder = builder.body(request.body.clone());
    }

    let response = builder.send().await.map_err(|error| {
        storage_driver_error(
            StorageErrorKind::Transient,
            format!("execute reverse tunnel local request: {error}"),
        )
    })?;
    tunnel_response_from_reqwest(request.request_id.clone(), response).await
}

fn response_headers_for_reqwest(headers: &reqwest::header::HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_string(), value.to_string()))
        })
        .collect()
}

async fn complete_request(
    client: &reqwest::Client,
    binding: &master_binding::Model,
    response: RemoteTunnelResponse,
) -> Result<()> {
    let complete_url = format!("{}{}", binding.master_url, REMOTE_TUNNEL_COMPLETE_PATH);
    let body = serde_json::to_vec(&response).map_err(|error| {
        AsterError::internal_error(format!("encode tunnel completion: {error}"))
    })?;
    let response = signed_master_request(
        client,
        binding,
        Method::POST,
        &complete_url,
        REMOTE_TUNNEL_COMPLETE_PATH,
        Some(body),
    )
    .await?;
    parse_empty_api_response(response, "reverse tunnel completion").await
}

async fn mark_tunnel_error(
    client: &reqwest::Client,
    binding: &master_binding::Model,
    error: &str,
) -> Result<()> {
    let request_id = aster_forge_utils::id::new_uuid();
    let response = RemoteTunnelResponse {
        request_id,
        status: 502,
        headers: Vec::new(),
        body: error.as_bytes().to_vec(),
    };
    complete_request(client, binding, response).await
}

async fn signed_master_request(
    client: &reqwest::Client,
    binding: &master_binding::Model,
    method: Method,
    url: &str,
    path_and_query: &str,
    body: Option<Vec<u8>>,
) -> Result<reqwest::Response> {
    let content_length = body
        .as_ref()
        .map(|body| {
            u64::try_from(body.len()).map_err(|_| {
                storage_driver_error(
                    StorageErrorKind::Precondition,
                    "reverse tunnel request body length overflow",
                )
            })
        })
        .transpose()?;
    let timestamp = chrono::Utc::now().timestamp();
    let nonce = aster_forge_utils::id::new_uuid();
    let signature = sign_internal_request(
        &binding.secret_key,
        method.as_str(),
        path_and_query,
        timestamp,
        &nonce,
        content_length,
    );

    let mut builder = client
        .request(method, url)
        .header(INTERNAL_AUTH_ACCESS_KEY_HEADER, &binding.access_key)
        .header(INTERNAL_AUTH_TIMESTAMP_HEADER, timestamp.to_string())
        .header(INTERNAL_AUTH_NONCE_HEADER, nonce)
        .header(INTERNAL_AUTH_SIGNATURE_HEADER, signature)
        .header(reqwest::header::CONTENT_TYPE, "application/json");
    if let Some(content_length) = content_length {
        builder = builder.header(reqwest::header::CONTENT_LENGTH, content_length);
    }
    if let Some(body) = body {
        builder = builder.body(body);
    }

    builder.send().await.map_err(|error| {
        storage_driver_error(
            StorageErrorKind::Transient,
            format!("send reverse tunnel master request: {error}"),
        )
    })
}

fn signed_master_ws_request(
    binding: &master_binding::Model,
    url: &str,
) -> Result<tokio_tungstenite::tungstenite::handshake::client::Request> {
    let timestamp = chrono::Utc::now().timestamp();
    let nonce = aster_forge_utils::id::new_uuid();
    let signature = sign_internal_request(
        &binding.secret_key,
        Method::GET.as_str(),
        REMOTE_TUNNEL_CONNECT_PATH,
        timestamp,
        &nonce,
        None,
    );

    let mut request = url.into_client_request().map_err(|error| {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("build reverse tunnel streaming websocket request: {error}"),
        )
    })?;
    let headers = request.headers_mut();
    headers.insert(
        INTERNAL_AUTH_ACCESS_KEY_HEADER,
        header_value(&binding.access_key, INTERNAL_AUTH_ACCESS_KEY_HEADER)?,
    );
    headers.insert(
        INTERNAL_AUTH_TIMESTAMP_HEADER,
        header_value(&timestamp.to_string(), INTERNAL_AUTH_TIMESTAMP_HEADER)?,
    );
    headers.insert(
        INTERNAL_AUTH_NONCE_HEADER,
        header_value(&nonce, INTERNAL_AUTH_NONCE_HEADER)?,
    );
    headers.insert(
        INTERNAL_AUTH_SIGNATURE_HEADER,
        header_value(&signature, INTERNAL_AUTH_SIGNATURE_HEADER)?,
    );
    Ok(request)
}

fn stream_connect_url(master_url: &str) -> Result<String> {
    let mut url = reqwest::Url::parse(master_url).map_err(|error| {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("invalid reverse tunnel master url: {error}"),
        )
    })?;
    let scheme = match url.scheme() {
        "http" => "ws",
        "https" => "wss",
        other => {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("reverse tunnel master url must use http/https, got '{other}'"),
            ));
        }
    };
    url.set_scheme(scheme).map_err(|_| {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            "failed to build reverse tunnel websocket url",
        )
    })?;
    url.set_path(REMOTE_TUNNEL_CONNECT_PATH);
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.to_string())
}

fn header_value(
    value: &str,
    name: &'static str,
) -> Result<tokio_tungstenite::tungstenite::http::HeaderValue> {
    tokio_tungstenite::tungstenite::http::HeaderValue::from_str(value).map_err(|error| {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("invalid reverse tunnel websocket header {name}: {error}"),
        )
    })
}

async fn parse_api_response<T: for<'de> serde::Deserialize<'de>>(
    response: reqwest::Response,
    action: &str,
) -> Result<T> {
    let status = response.status();
    let body =
        read_reqwest_response_body_limited(response, action, REMOTE_CONTROL_PLANE_BODY_LIMIT)
            .await?;
    let envelope: ApiEnvelope<T> = serde_json::from_slice(&body).map_err(|error| {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("failed to parse {action} response: {error}"),
        )
    })?;
    if !status.is_success() || envelope.code != ApiErrorCode::Success {
        let message = if envelope.msg.trim().is_empty() {
            format!("{action} failed with HTTP {status}")
        } else {
            envelope.msg
        };
        return Err(storage_driver_error(StorageErrorKind::Transient, message));
    }
    envelope.data.ok_or_else(|| {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("{action} response missing data"),
        )
    })
}

async fn parse_empty_api_response(response: reqwest::Response, action: &str) -> Result<()> {
    let status = response.status();
    let body =
        read_reqwest_response_body_limited(response, action, REMOTE_CONTROL_PLANE_BODY_LIMIT)
            .await?;
    let envelope: ApiEnvelope<serde_json::Value> =
        serde_json::from_slice(&body).map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("failed to parse {action} response: {error}"),
            )
        })?;
    if !status.is_success() || envelope.code != ApiErrorCode::Success {
        let message = if envelope.msg.trim().is_empty() {
            format!("{action} failed with HTTP {status}")
        } else {
            envelope.msg
        };
        return Err(storage_driver_error(StorageErrorKind::Transient, message));
    }
    Ok(())
}

fn tunnel_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(FOLLOWER_TUNNEL_CONNECT_TIMEOUT)
        .read_timeout(FOLLOWER_TUNNEL_READ_TIMEOUT)
        .timeout(FOLLOWER_TUNNEL_OPERATION_TIMEOUT)
        .user_agent(OUTBOUND_HTTP_USER_AGENT)
        .build()
        .map_err(|error| {
            AsterError::internal_error(format!("tunnel_http_client build tunnel client: {error}"))
        })
}

fn binding_worker_fingerprint(binding: &master_binding::Model) -> String {
    format!(
        "{}\n{}\n{}\n{}",
        binding.master_url, binding.access_key, binding.secret_key, binding.is_enabled
    )
}

#[cfg(test)]
mod tests {
    use super::{
        execute_stream_tunnel_request, is_allowed_tunnel_target, signed_master_ws_request,
        stream_connect_url,
    };
    use crate::entities::master_binding;
    use crate::storage::remote_protocol::tunnel::server::{
        RemoteTunnelStreamFrame, RemoteTunnelStreamFrameKind, decode_stream_frame,
        encode_stream_frame,
    };
    use crate::storage::remote_protocol::{
        INTERNAL_AUTH_ACCESS_KEY_HEADER, INTERNAL_AUTH_NONCE_HEADER,
        INTERNAL_AUTH_SIGNATURE_HEADER, INTERNAL_AUTH_TIMESTAMP_HEADER, INTERNAL_STORAGE_BASE_PATH,
        sign_internal_request,
    };
    use actix_web::{App, HttpResponse, HttpServer, web};
    use bytes::Bytes;
    use futures::Sink;
    use std::pin::Pin;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::task::{Context, Poll};
    use tokio::sync::mpsc;
    use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;

    struct TestServer {
        base_url: String,
        handle: actix_web::dev::ServerHandle,
        task: tokio::task::JoinHandle<std::io::Result<()>>,
    }

    impl TestServer {
        async fn stop(self) {
            self.handle.stop(true).await;
            let _ = self.task.await;
        }
    }

    struct ChannelSink {
        tx: mpsc::UnboundedSender<WsMessage>,
    }

    impl Sink<WsMessage> for ChannelSink {
        type Error = tokio_tungstenite::tungstenite::Error;

        fn poll_ready(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<std::result::Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn start_send(
            self: Pin<&mut Self>,
            item: WsMessage,
        ) -> std::result::Result<(), Self::Error> {
            self.tx
                .send(item)
                .map_err(|_| tokio_tungstenite::tungstenite::Error::ConnectionClosed)
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<std::result::Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_close(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<std::result::Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    #[test]
    fn tunnel_target_guard_only_allows_internal_storage_paths() {
        assert!(is_allowed_tunnel_target(INTERNAL_STORAGE_BASE_PATH));
        assert!(is_allowed_tunnel_target(
            "/api/v1/internal/storage/objects/file.txt"
        ));
        assert!(is_allowed_tunnel_target(
            "/api/v1/internal/storage/objects?prefix=files"
        ));

        assert!(!is_allowed_tunnel_target("/api/v1/admin/users"));
        assert!(!is_allowed_tunnel_target("/api/v1/internal/storage-admin"));
        assert!(!is_allowed_tunnel_target(
            "/api/v1/internal/storage/../admin/users"
        ));
        assert!(!is_allowed_tunnel_target(
            "/api/v1/internal/storage/%2e%2e/admin/users"
        ));
        assert!(!is_allowed_tunnel_target(
            "http://127.0.0.1/api/v1/internal/storage/objects"
        ));
    }

    #[test]
    fn tunnel_target_guard_rejects_fragment_backslash_and_encoded_traversal_edges() {
        assert!(!is_allowed_tunnel_target(
            "/api/v1/internal/storage/objects/file.txt#fragment"
        ));
        assert!(!is_allowed_tunnel_target(
            "/api/v1/internal/storage/objects\\file.txt"
        ));
        assert!(!is_allowed_tunnel_target(
            "/api/v1/internal/storage/objects/%2e/file.txt"
        ));
        assert!(!is_allowed_tunnel_target(
            "/api/v1/internal/storage/objects/%2E%2E/file.txt"
        ));
        assert!(!is_allowed_tunnel_target(
            "/api/v1/internal/storage%2dadmin/objects"
        ));
    }

    #[test]
    fn stream_connect_url_maps_http_schemes_and_clears_query_fragment() {
        let ws = stream_connect_url("http://master.example.com?token=old#frag")
            .expect("http master URL should map to ws");
        assert_eq!(
            ws,
            "ws://master.example.com/api/v1/internal/remote-tunnel/connect"
        );

        let wss = stream_connect_url("https://master.example.com/root?token=old#frag")
            .expect("https master URL should map to wss");
        assert_eq!(
            wss,
            "wss://master.example.com/api/v1/internal/remote-tunnel/connect"
        );

        let error = stream_connect_url("ftp://master.example.com")
            .expect_err("unsupported master URL scheme should fail");
        assert!(error.message().contains("must use http/https"));
    }

    #[test]
    fn signed_master_ws_request_includes_verifiable_internal_auth_headers() {
        let binding = build_binding();
        let request = signed_master_ws_request(
            &binding,
            "ws://master.example.com/api/v1/internal/remote-tunnel/connect",
        )
        .expect("websocket request should build");

        assert_eq!(request.method(), "GET");
        assert_eq!(
            request.uri().path(),
            "/api/v1/internal/remote-tunnel/connect"
        );
        let headers = request.headers();
        assert_eq!(
            headers
                .get(INTERNAL_AUTH_ACCESS_KEY_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some("binding-access")
        );
        let timestamp = headers
            .get(INTERNAL_AUTH_TIMESTAMP_HEADER)
            .and_then(|value| value.to_str().ok())
            .expect("timestamp header should be present");
        let nonce = headers
            .get(INTERNAL_AUTH_NONCE_HEADER)
            .and_then(|value| value.to_str().ok())
            .expect("nonce header should be present");
        let expected = sign_internal_request(
            "binding-secret",
            "GET",
            "/api/v1/internal/remote-tunnel/connect",
            timestamp
                .parse()
                .expect("timestamp header should parse as i64"),
            nonce,
            None,
        );
        assert_eq!(
            headers
                .get(INTERNAL_AUTH_SIGNATURE_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some(expected.as_str())
        );
    }

    #[tokio::test]
    async fn streaming_request_body_reader_close_keeps_lane_and_returns_local_response() {
        let calls = Arc::new(AtomicUsize::new(0));
        let server = spawn_stream_early_response_server(calls.clone()).await;

        let request_id = "stream-local-early-response";
        let frames = vec![
            request_body_frame(request_id, b"chunk-a"),
            request_body_frame(request_id, b"chunk-b"),
            request_end_frame(request_id),
        ];
        let mut read = futures::stream::iter(frames.into_iter().map(|frame| {
            encode_stream_frame(&frame)
                .map(WsMessage::Binary)
                .map_err(|_| tokio_tungstenite::tungstenite::Error::ConnectionClosed)
        }));
        let (mut write, mut written_rx) = channel_sink();

        execute_stream_tunnel_request(
            &reqwest::Client::new(),
            &server.base_url,
            request_start_frame(
                request_id,
                "PUT",
                "/api/v1/internal/storage/objects/too-large.bin",
            ),
            &mut read,
            &mut write,
        )
        .await
        .expect("local early response should not close the streaming lane");

        let start = next_written_frame(&mut written_rx).await;
        assert_eq!(start.kind, RemoteTunnelStreamFrameKind::ResponseStart);
        assert_eq!(start.status, Some(413));
        let body = next_written_frame(&mut written_rx).await;
        assert_eq!(body.kind, RemoteTunnelStreamFrameKind::ResponseBody);
        assert_eq!(body.body, Bytes::from_static(b"too large"));
        let end = next_written_frame(&mut written_rx).await;
        assert_eq!(end.kind, RemoteTunnelStreamFrameKind::ResponseEnd);
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        server.stop().await;
    }

    #[tokio::test]
    async fn streaming_local_request_failure_sends_error_frame_without_lane_error() {
        let listener =
            std::net::TcpListener::bind(("127.0.0.1", 0)).expect("ephemeral listener should bind");
        let port = listener
            .local_addr()
            .expect("listener should have local addr")
            .port();
        drop(listener);

        let request_id = "stream-local-connect-failure";
        let frames = vec![request_end_frame(request_id)];
        let mut read = futures::stream::iter(frames.into_iter().map(|frame| {
            encode_stream_frame(&frame)
                .map(WsMessage::Binary)
                .map_err(|_| tokio_tungstenite::tungstenite::Error::ConnectionClosed)
        }));
        let (mut write, mut written_rx) = channel_sink();
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("test client should build without proxy");

        execute_stream_tunnel_request(
            &client,
            &format!("http://127.0.0.1:{port}"),
            request_start_frame(
                request_id,
                "GET",
                "/api/v1/internal/storage/objects/missing-server.bin",
            ),
            &mut read,
            &mut write,
        )
        .await
        .expect("local request failure should be reported as a stream error frame");

        let error = next_written_frame(&mut written_rx).await;
        assert_eq!(error.kind, RemoteTunnelStreamFrameKind::Error);
        assert_eq!(error.request_id, request_id);
        assert!(
            error
                .message
                .as_deref()
                .unwrap_or_default()
                .contains("execute reverse tunnel streaming local request")
        );
    }

    fn channel_sink() -> (ChannelSink, mpsc::UnboundedReceiver<WsMessage>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (ChannelSink { tx }, rx)
    }

    async fn next_written_frame(
        written_rx: &mut mpsc::UnboundedReceiver<WsMessage>,
    ) -> RemoteTunnelStreamFrame {
        let message = written_rx
            .recv()
            .await
            .expect("stream should write a websocket message");
        let WsMessage::Binary(bytes) = message else {
            panic!("stream should write binary frames");
        };
        decode_stream_frame(bytes).expect("written stream frame should decode")
    }

    async fn spawn_stream_early_response_server(calls: Arc<AtomicUsize>) -> TestServer {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
            .expect("test server listener should bind");
        let port = listener
            .local_addr()
            .expect("test server listener should have address")
            .port();
        let server = HttpServer::new(move || {
            App::new().app_data(web::Data::new(calls.clone())).route(
                "/api/v1/internal/storage/objects/too-large.bin",
                web::put().to(
                    |calls: web::Data<Arc<AtomicUsize>>, _body: web::Payload| async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        HttpResponse::PayloadTooLarge().body("too large")
                    },
                ),
            )
        })
        .listen(listener)
        .expect("test server should listen")
        .run();
        let handle = server.handle();
        let task = tokio::spawn(server);
        TestServer {
            base_url: format!("http://127.0.0.1:{port}"),
            handle,
            task,
        }
    }

    fn request_start_frame(
        request_id: &str,
        method: &str,
        path_and_query: &str,
    ) -> RemoteTunnelStreamFrame {
        RemoteTunnelStreamFrame {
            kind: RemoteTunnelStreamFrameKind::RequestStart,
            request_id: request_id.to_string(),
            method: Some(method.to_string()),
            path_and_query: Some(path_and_query.to_string()),
            headers: Vec::new(),
            content_length: None,
            status: None,
            message: None,
            body: Bytes::new(),
        }
    }

    fn request_body_frame(request_id: &str, body: &'static [u8]) -> RemoteTunnelStreamFrame {
        RemoteTunnelStreamFrame {
            kind: RemoteTunnelStreamFrameKind::RequestBody,
            request_id: request_id.to_string(),
            method: None,
            path_and_query: None,
            headers: Vec::new(),
            content_length: None,
            status: None,
            message: None,
            body: Bytes::from_static(body),
        }
    }

    fn request_end_frame(request_id: &str) -> RemoteTunnelStreamFrame {
        RemoteTunnelStreamFrame {
            kind: RemoteTunnelStreamFrameKind::RequestEnd,
            request_id: request_id.to_string(),
            method: None,
            path_and_query: None,
            headers: Vec::new(),
            content_length: None,
            status: None,
            message: None,
            body: Bytes::new(),
        }
    }

    fn build_binding() -> master_binding::Model {
        let now = chrono::Utc::now();
        master_binding::Model {
            id: 1,
            name: "binding".to_string(),
            master_url: "http://master.example.com".to_string(),
            access_key: "binding-access".to_string(),
            secret_key: "binding-secret".to_string(),
            is_enabled: true,
            storage_namespace: "namespace".to_string(),
            created_at: now,
            updated_at: now,
        }
    }
}
