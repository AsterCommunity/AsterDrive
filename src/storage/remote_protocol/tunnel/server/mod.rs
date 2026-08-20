//! Reverse tunnel transport for remote followers.

use crate::db::repository::managed_follower_repo;
use crate::errors::{AsterError, Result};
use crate::runtime::RemoteProtocolRuntimeState;
use aster_drive_model::entities::managed_follower;
use aster_drive_storage::StorageErrorKind;
use chrono::Utc;
use futures::StreamExt as _;
use serde::Serialize;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

mod auth;
mod frame;
mod owner;
mod payload;
mod proxy;
mod registry;
mod response;
#[cfg(test)]
mod tests;

use owner::{RemoteTunnelOwnerReleaseGuard, RemoteTunnelStreamOwnerClaim};

pub use auth::authorize_tunnel_request;
pub use frame::{
    RemoteTunnelStreamFrame, RemoteTunnelStreamFrameKind, decode_stream_frame, encode_stream_frame,
};
pub use owner::{
    REMOTE_TUNNEL_OWNER_RENEW_INTERVAL, RemoteTunnelOwnerClaim, RemoteTunnelOwnerDirectory,
    RemoteTunnelOwnerLease,
};
pub use payload::{
    RemoteTunnelPollRequest, RemoteTunnelPollResponse, RemoteTunnelRequest, RemoteTunnelResponse,
};
pub use proxy::{
    ClusterRemoteTunnelBroker, REMOTE_TUNNEL_PROXY_PATH_PREFIX, RemoteTunnelProxyQuery,
    proxy_tunnel_request,
};
pub(crate) use registry::TunnelDisconnectReason;
pub use registry::{
    RemoteTunnelBroker, RemoteTunnelHttpResponse, RemoteTunnelRegistry,
    RemoteTunnelStreamHttpResponse, reverse_tunnel_offline_error,
};
pub use response::{
    empty_envelope_response, envelope_response, response_headers_for_tunnel,
    tunnel_response_from_reqwest,
};

pub const REMOTE_TUNNEL_BASE_PATH: &str = "/api/v1/internal/remote-tunnel";
pub const REMOTE_TUNNEL_POLL_PATH: &str = "/api/v1/internal/remote-tunnel/poll";
pub const REMOTE_TUNNEL_COMPLETE_PATH: &str = "/api/v1/internal/remote-tunnel/complete";
pub const REMOTE_TUNNEL_CONNECT_PATH: &str = "/api/v1/internal/remote-tunnel/connect";

const REMOTE_TUNNEL_POLL_TIMEOUT: Duration = Duration::from_secs(25);
const REMOTE_TUNNEL_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
const REMOTE_TUNNEL_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(45);
const REMOTE_TUNNEL_CLOSE_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const REMOTE_TUNNEL_SHUTDOWN_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
pub const REMOTE_TUNNEL_BODY_LIMIT: usize = 64 * 1024 * 1024;
pub const REMOTE_TUNNEL_JSON_LIMIT: usize = REMOTE_TUNNEL_BODY_LIMIT * 2 + 1024 * 1024;
pub const REMOTE_TUNNEL_POLL_METADATA_BUDGET: usize = 64 * 1024;
pub const REMOTE_TUNNEL_POLL_BODY_LIMIT: usize =
    ((crate::storage::remote_protocol::REMOTE_CONTROL_PLANE_BODY_LIMIT
        - REMOTE_TUNNEL_POLL_METADATA_BUDGET)
        / 4)
        * 3;
pub const REMOTE_TUNNEL_STREAM_CHUNK_SIZE: usize = 64 * 1024;
pub const REMOTE_TUNNEL_STREAM_FRAME_LIMIT: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RemoteTunnelOnlineStatus {
    /// A recent poll or stream handshake is within the online TTL.
    Online,
    /// No successful handshake has been observed within the online TTL.
    Offline,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct RemoteTunnelInfo {
    /// Online status derived from recent successful tunnel handshakes and the online TTL.
    pub status: RemoteTunnelOnlineStatus,
    /// Transient runtime error from the tunnel control/data path. The next successful poll or
    /// stream handshake clears it; it is not a historical error log.
    pub runtime_error: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    /// Last successful poll or stream handshake persisted by the primary.
    pub last_handshake_at: Option<chrono::DateTime<Utc>>,
}

pub async fn poll<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node: &managed_follower::Model,
) -> Result<RemoteTunnelPollResponse> {
    if !remote_node.is_enabled {
        return Err(AsterError::validation_error("remote node is disabled"));
    }
    ensure_reverse_tunnel_transport(remote_node)?;

    claim_tunnel_ownership(state, remote_node).await?;

    let registry = state.remote_protocol().tunnel_registry();
    let (request_rx, _registration) = registry.register_poll(remote_node);
    registry.record_handshake(remote_node, None);
    managed_follower_repo::touch_tunnel_success(state.writer_db(), remote_node.id, Utc::now())
        .await?;
    // A successful control-plane handshake means the runtime path recovered. This clears only
    // transient tunnel telemetry and leaves the separate probe `last_probe_error` untouched.
    registry.clear_error(remote_node.id);

    let request = tokio::time::timeout(REMOTE_TUNNEL_POLL_TIMEOUT, request_rx)
        .await
        .ok()
        .and_then(std::result::Result::ok)
        .map(|queued| queued.request);

    Ok(RemoteTunnelPollResponse { request })
}

pub async fn complete<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node: &managed_follower::Model,
    response: RemoteTunnelResponse,
) -> Result<()> {
    ensure_reverse_tunnel_transport(remote_node)?;
    if response.body.len() > REMOTE_TUNNEL_BODY_LIMIT {
        return Err(crate::errors::storage_driver_error(
            StorageErrorKind::Unsupported,
            format!(
                "reverse tunnel response body exceeds {} bytes; use direct transport or a streaming tunnel",
                REMOTE_TUNNEL_BODY_LIMIT
            ),
        ));
    }
    let reported_error = reported_tunnel_error(&response);
    match state
        .remote_protocol()
        .tunnel_registry()
        .complete(remote_node, response)
    {
        Ok(()) => Ok(()),
        Err(error) => {
            let Some(reported_error) =
                reported_error.filter(|_| is_missing_pending_tunnel_error(error.message()))
            else {
                return Err(error);
            };
            mark_tunnel_error(state, &remote_node.access_key, reported_error).await?;
            Ok(())
        }
    }
}

pub async fn connect_stream<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node: managed_follower::Model,
    session: actix_ws::Session,
    stream: actix_ws::MessageStream,
    shutdown_token: CancellationToken,
) -> Result<()> {
    if !remote_node.is_enabled {
        return Err(AsterError::validation_error("remote node is disabled"));
    }
    ensure_reverse_tunnel_transport(&remote_node)?;

    let owner_release_guard = claim_stream_tunnel_ownership(state, &remote_node).await?;
    let owner_directory = owner_release_guard.directory();

    owner_release_guard
        .run_and_release(run_connected_stream(
            state,
            remote_node,
            session,
            stream,
            owner_directory,
            shutdown_token,
        ))
        .await
}

async fn claim_stream_tunnel_ownership<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node: &managed_follower::Model,
) -> Result<RemoteTunnelOwnerReleaseGuard> {
    let Some(owner_directory) = state.remote_protocol().tunnel_owner_directory() else {
        return Ok(RemoteTunnelOwnerReleaseGuard::unmanaged(remote_node.id));
    };

    match owner_directory.try_claim_stream(remote_node.id).await? {
        RemoteTunnelStreamOwnerClaim::Owned(guard) => Ok(guard),
        RemoteTunnelStreamOwnerClaim::Standby(owner) => {
            Err(tunnel_owned_by_another_primary_error(remote_node.id, owner))
        }
    }
}

async fn run_connected_stream<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node: managed_follower::Model,
    mut session: actix_ws::Session,
    mut stream: actix_ws::MessageStream,
    owner_directory: Option<std::sync::Arc<RemoteTunnelOwnerDirectory>>,
    shutdown_token: CancellationToken,
) -> Result<()> {
    let registry = state.remote_protocol().tunnel_registry().clone();
    let (lane_id, mut request_rx, registration) = registry.register_stream_lane(&remote_node);
    registry.record_handshake(&remote_node, Some(&lane_id));
    tracing::info!(
        remote_node_id = remote_node.id,
        lane_id = %lane_id,
        "reverse tunnel streaming lane connected"
    );
    managed_follower_repo::touch_tunnel_success(state.writer_db(), remote_node.id, Utc::now())
        .await?;
    // Stream registration is the same successful tunnel handshake as poll registration, so it
    // clears only transient tunnel telemetry and leaves capability-probe state untouched.
    registry.clear_error(remote_node.id);

    let mut owner_renewal = tokio::time::interval(REMOTE_TUNNEL_OWNER_RENEW_INTERVAL);
    owner_renewal.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    owner_renewal.tick().await;
    let mut heartbeat = tokio::time::interval(REMOTE_TUNNEL_HEARTBEAT_INTERVAL);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await;
    let mut liveness = TunnelHeartbeat::new(Instant::now());
    let mut draining = false;
    let disconnect_reason: Option<TunnelDisconnectReason>;
    let mut drain_deadline = Box::pin(tokio::time::sleep(Duration::from_secs(24 * 60 * 60)));

    loop {
        tokio::select! {
            biased;
            _ = shutdown_token.cancelled(), if !draining => {
                draining = true;
                if registry.stream_lane_is_busy(&remote_node, &lane_id) {
                    tracing::info!(
                        remote_node_id = remote_node.id,
                        lane_id = %lane_id,
                        timeout_secs = REMOTE_TUNNEL_SHUTDOWN_DRAIN_TIMEOUT.as_secs(),
                        "reverse tunnel primary shutdown draining in-flight streaming request"
                    );
                    drain_deadline.as_mut().reset(
                        tokio::time::Instant::now() + REMOTE_TUNNEL_SHUTDOWN_DRAIN_TIMEOUT,
                    );
                } else {
                    tracing::info!(
                        remote_node_id = remote_node.id,
                        lane_id = %lane_id,
                        "reverse tunnel streaming lane closing for primary shutdown"
                    );
                    disconnect_reason = Some(TunnelDisconnectReason::GracefulShutdown);
                    break;
                }
            }
            _ = &mut drain_deadline, if draining => {
                tracing::warn!(
                    remote_node_id = remote_node.id,
                    lane_id = %lane_id,
                    timeout_secs = REMOTE_TUNNEL_SHUTDOWN_DRAIN_TIMEOUT.as_secs(),
                    "reverse tunnel shutdown drain timed out; closing streaming lane with in-flight request"
                );
                disconnect_reason = Some(TunnelDisconnectReason::GracefulShutdown);
                break;
            }
            _ = owner_renewal.tick(), if owner_directory.is_some() => {
                let Some(directory) = owner_directory.as_ref() else {
                    continue;
                };
                match directory.renew(remote_node.id).await {
                    Ok(true) => {
                    }
                    Ok(false) => {
                        tracing::warn!(
                            remote_node_id = remote_node.id,
                            runtime_id = %directory.runtime_id(),
                            "reverse tunnel owner lease was fenced by another primary"
                        );
                        disconnect_reason = Some(TunnelDisconnectReason::OwnerFenced);
                        break;
                    }
                    Err(error) => {
                        tracing::warn!(
                            remote_node_id = remote_node.id,
                            runtime_id = %directory.runtime_id(),
                            "reverse tunnel owner lease renewal failed: {error}"
                        );
                        disconnect_reason = Some(TunnelDisconnectReason::OwnerRenewalFailed);
                        break;
                    }
                }
            }
            _ = heartbeat.tick() => {
                let now = Instant::now();
                if liveness.is_timed_out(now) {
                    tracing::warn!(
                        remote_node_id = remote_node.id,
                        lane_id = %lane_id,
                        timeout_secs = REMOTE_TUNNEL_HEARTBEAT_TIMEOUT.as_secs(),
                        "reverse tunnel streaming lane heartbeat timed out waiting for follower activity"
                    );
                    disconnect_reason = Some(TunnelDisconnectReason::HeartbeatTimeout);
                    break;
                }
                if let Err(error) = session.ping(b"aster-tunnel-heartbeat").await {
                    tracing::warn!(
                        remote_node_id = remote_node.id,
                        lane_id = %lane_id,
                        "failed to send reverse tunnel heartbeat ping: {error}"
                    );
                    disconnect_reason = Some(TunnelDisconnectReason::HeartbeatSendFailed);
                    break;
                }
            }
            message = stream.next() => {
                let Some(message) = message else {
                    disconnect_reason = Some(TunnelDisconnectReason::Eof);
                    break;
                };
                let message = match message {
                    Ok(message) => message,
                    Err(error) => {
                        tracing::warn!(
                            remote_node_id = remote_node.id,
                            lane_id = %lane_id,
                            "reverse tunnel streaming lane read failed: {error}"
                        );
                        disconnect_reason = Some(if error.to_string().to_ascii_lowercase().contains("reset") {
                            TunnelDisconnectReason::ConnectionReset
                        } else {
                            TunnelDisconnectReason::ProtocolReadError
                        });
                        break;
                    }
                };
                match message {
                    actix_ws::Message::Binary(bytes) => {
                        liveness.record_activity(Instant::now());
                        match decode_stream_frame(bytes) {
                            Ok(frame) => {
                                registry.update_last_handshake(remote_node.id);
                                if let Err(error) = registry
                                    .complete_stream_frame(&remote_node, &lane_id, frame)
                                    .await
                                {
                                    tracing::warn!(
                                        remote_node_id = remote_node.id,
                                        lane_id = %lane_id,
                                        "failed to handle reverse tunnel streaming frame: {error}"
                                    );
                                }
                            }
                            Err(error) => {
                                tracing::warn!(
                                    remote_node_id = remote_node.id,
                                    lane_id = %lane_id,
                                    "failed to decode reverse tunnel streaming frame: {error}"
                                );
                                disconnect_reason = Some(TunnelDisconnectReason::ProtocolDecodeError);
                                break;
                            }
                        }
                    }
                    actix_ws::Message::Ping(bytes) => {
                        if session.pong(&bytes).await.is_err() {
                            disconnect_reason = Some(TunnelDisconnectReason::ProtocolReadError);
                            break;
                        }
                        liveness.record_activity(Instant::now());
                        registry.update_last_handshake(remote_node.id);
                    }
                    actix_ws::Message::Pong(_) => {
                        liveness.record_activity(Instant::now());
                        registry.update_last_handshake(remote_node.id);
                    }
                    actix_ws::Message::Close(reason) => {
                        tracing::info!(
                            remote_node_id = remote_node.id,
                            lane_id = %lane_id,
                            close_reason = ?reason,
                            "reverse tunnel streaming lane closed by follower"
                        );
                        disconnect_reason = Some(TunnelDisconnectReason::PeerClose);
                        break;
                    }
                    _ => {}
                }
            }
            frame = request_rx.recv() => {
                let Some(frame) = frame else {
                    disconnect_reason = Some(TunnelDisconnectReason::Eof);
                    break;
                };
                let bytes = encode_stream_frame(&frame)?;
                if session.binary(bytes).await.is_err() {
                    disconnect_reason = Some(TunnelDisconnectReason::ConnectionReset);
                    break;
                }
            }
        }
        if draining && !registry.stream_lane_is_busy(&remote_node, &lane_id) {
            tracing::info!(
                remote_node_id = remote_node.id,
                lane_id = %lane_id,
                "reverse tunnel streaming lane drained before primary shutdown"
            );
            disconnect_reason = Some(TunnelDisconnectReason::GracefulShutdown);
            break;
        }
    }

    let close_handshake_ok = close_connected_stream(
        session,
        stream,
        remote_node.id,
        &lane_id,
        if shutdown_token.is_cancelled() {
            Some(actix_ws::CloseReason {
                code: actix_ws::CloseCode::Away,
                description: Some("primary shutdown".to_string()),
            })
        } else {
            None
        },
    )
    .await;
    let final_disconnect_reason = finalize_disconnect_reason(disconnect_reason, close_handshake_ok);
    registration.set_disconnect_reason(final_disconnect_reason);
    Ok(())
}

fn finalize_disconnect_reason(
    reason: Option<TunnelDisconnectReason>,
    close_handshake_ok: bool,
) -> TunnelDisconnectReason {
    match (reason, close_handshake_ok) {
        (Some(reason), _) => reason,
        (None, true) => TunnelDisconnectReason::Eof,
        (None, false) => TunnelDisconnectReason::CloseHandshakeFailed,
    }
}

#[derive(Debug, Clone, Copy)]
struct TunnelHeartbeat {
    last_activity_at: Instant,
}

impl TunnelHeartbeat {
    fn new(now: Instant) -> Self {
        Self {
            last_activity_at: now,
        }
    }

    fn record_activity(&mut self, now: Instant) {
        self.last_activity_at = now;
    }

    fn is_timed_out(&self, now: Instant) -> bool {
        now.duration_since(self.last_activity_at) >= REMOTE_TUNNEL_HEARTBEAT_TIMEOUT
    }
}

async fn close_connected_stream(
    session: actix_ws::Session,
    mut stream: actix_ws::MessageStream,
    remote_node_id: i64,
    lane_id: &str,
    reason: Option<actix_ws::CloseReason>,
) -> bool {
    if let Err(error) = session.close(reason).await {
        tracing::warn!(
            remote_node_id,
            lane_id,
            "failed to send reverse tunnel streaming lane close frame: {error}"
        );
        return false;
    }
    let handshake = tokio::time::timeout(REMOTE_TUNNEL_CLOSE_HANDSHAKE_TIMEOUT, async {
        while let Some(message) = stream.next().await {
            match message {
                Ok(actix_ws::Message::Close(_)) => return Ok(()),
                Err(error) => return Err(error),
                Ok(_) => {}
            }
        }
        Ok(())
    })
    .await;
    match handshake {
        Ok(Ok(())) => true,
        Ok(Err(error)) => {
            tracing::warn!(
                remote_node_id,
                lane_id,
                "reverse tunnel streaming lane close handshake read failed: {error}"
            );
            false
        }
        Err(_) => {
            tracing::warn!(
                remote_node_id,
                lane_id,
                timeout_secs = REMOTE_TUNNEL_CLOSE_HANDSHAKE_TIMEOUT.as_secs(),
                "reverse tunnel streaming lane close handshake timed out"
            );
            false
        }
    }
}

async fn claim_tunnel_ownership<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node: &managed_follower::Model,
) -> Result<Option<std::sync::Arc<RemoteTunnelOwnerDirectory>>> {
    let Some(owner_directory) = state.remote_protocol().tunnel_owner_directory() else {
        return Ok(None);
    };

    match owner_directory.try_claim(remote_node.id).await? {
        RemoteTunnelOwnerClaim::Owned(_) => Ok(Some(owner_directory)),
        RemoteTunnelOwnerClaim::Standby(owner) => {
            Err(tunnel_owned_by_another_primary_error(remote_node.id, owner))
        }
    }
}

fn tunnel_owned_by_another_primary_error(
    remote_node_id: i64,
    owner: Option<RemoteTunnelOwnerLease>,
) -> AsterError {
    let owner = owner
        .map(|owner| {
            format!(
                "runtime {} at {}",
                owner.runtime_id, owner.internal_endpoint
            )
        })
        .unwrap_or_else(|| "another primary".to_string());
    crate::errors::storage_driver_error(
        StorageErrorKind::Transient,
        format!("reverse tunnel remote node #{remote_node_id} is owned by {owner}"),
    )
}

pub fn tunnel_info_for_node<S: RemoteProtocolRuntimeState>(
    state: &S,
    node: &managed_follower::Model,
) -> RemoteTunnelInfo {
    RemoteTunnelInfo {
        status: if node
            .transport_mode
            .resolves_to_reverse_tunnel(&node.base_url)
            && state.remote_protocol().tunnel_registry().is_online(node)
        {
            RemoteTunnelOnlineStatus::Online
        } else {
            RemoteTunnelOnlineStatus::Offline
        },
        // Prefer the in-memory value so a newly observed runtime failure is visible before the
        // asynchronous persistence task commits it; the database value survives process restart.
        runtime_error: state
            .remote_protocol()
            .tunnel_registry()
            .runtime_error(node.id)
            .unwrap_or_else(|| node.tunnel_runtime_error.clone()),
        last_handshake_at: node.tunnel_last_handshake_at,
    }
}

pub(crate) fn ensure_reverse_tunnel_transport(remote_node: &managed_follower::Model) -> Result<()> {
    if remote_node
        .transport_mode
        .resolves_to_reverse_tunnel(&remote_node.base_url)
    {
        Ok(())
    } else {
        Err(AsterError::validation_error(
            "remote node transport does not resolve to reverse tunnel",
        ))
    }
}

pub async fn mark_tunnel_error<S: RemoteProtocolRuntimeState>(
    state: &S,
    access_key: &str,
    error: impl std::fmt::Display,
) -> Result<()> {
    let Some(remote_node) =
        managed_follower_repo::find_by_access_key(state.writer_db(), access_key).await?
    else {
        return Ok(());
    };
    state
        .remote_protocol()
        .tunnel_registry()
        .persist_runtime_error(state.writer_db(), remote_node.id, error.to_string())
        .await?;
    Ok(())
}

fn reported_tunnel_error(response: &RemoteTunnelResponse) -> Option<String> {
    if !(500..600).contains(&response.status) {
        return None;
    }
    let message = String::from_utf8_lossy(&response.body).trim().to_string();
    if message.is_empty() {
        Some(format!(
            "reverse tunnel follower reported HTTP {}",
            response.status
        ))
    } else {
        Some(message)
    }
}

fn is_missing_pending_tunnel_error(message: &str) -> bool {
    message.contains("reverse tunnel request is no longer pending")
        || message.contains("reverse tunnel request receiver closed")
}
