//! Reverse tunnel transport for remote followers.

use crate::db::repository::managed_follower_repo;
use crate::entities::managed_follower;
use crate::errors::{AsterError, Result};
use crate::runtime::{RemoteProtocolRuntimeState, SharedRuntimeState};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use chrono::Utc;
use futures::StreamExt as _;
use serde::Serialize;
use std::time::Duration;

mod auth;
mod frame;
mod owner;
mod payload;
mod proxy;
mod registry;
mod response;
#[cfg(test)]
mod tests;

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
const REMOTE_TUNNEL_STREAM_READ_TIMEOUT: Duration = Duration::from_secs(60);
pub const REMOTE_TUNNEL_BODY_LIMIT: usize = 64 * 1024 * 1024;
pub const REMOTE_TUNNEL_JSON_LIMIT: usize = REMOTE_TUNNEL_BODY_LIMIT * 2 + 1024 * 1024;
pub const REMOTE_TUNNEL_STREAM_CHUNK_SIZE: usize = 64 * 1024;
pub const REMOTE_TUNNEL_STREAM_FRAME_LIMIT: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RemoteTunnelOnlineStatus {
    Online,
    Offline,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct RemoteTunnelInfo {
    pub status: RemoteTunnelOnlineStatus,
    pub last_error: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub last_seen_at: Option<chrono::DateTime<Utc>>,
}

pub async fn poll<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node: &managed_follower::Model,
) -> Result<RemoteTunnelPollResponse> {
    if !remote_node.is_enabled {
        return Err(AsterError::validation_error("remote node is disabled"));
    }

    claim_tunnel_ownership(state, remote_node).await?;

    let registry = state.remote_protocol().tunnel_registry();
    let (request_rx, _registration) = registry.register_poll(remote_node);
    managed_follower_repo::touch_tunnel_result(
        state.writer_db(),
        remote_node.id,
        String::new(),
        Some(Utc::now()),
    )
    .await?;
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
    if response.body.len() > REMOTE_TUNNEL_BODY_LIMIT {
        return Err(storage_driver_error(
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
    mut session: actix_ws::Session,
    mut stream: actix_ws::MessageStream,
) -> Result<()> {
    if !remote_node.is_enabled {
        return Err(AsterError::validation_error("remote node is disabled"));
    }

    let owner_directory = claim_tunnel_ownership(state, &remote_node).await?;

    let registry = state.remote_protocol().tunnel_registry().clone();
    let (lane_id, mut request_rx, _registration) = registry.register_stream_lane(&remote_node);
    managed_follower_repo::touch_tunnel_result(
        state.writer_db(),
        remote_node.id,
        String::new(),
        Some(Utc::now()),
    )
    .await?;
    registry.clear_error(remote_node.id);

    let mut owner_renewal = tokio::time::interval(REMOTE_TUNNEL_OWNER_RENEW_INTERVAL);
    owner_renewal.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    owner_renewal.tick().await;

    loop {
        tokio::select! {
            biased;
            _ = owner_renewal.tick(), if owner_directory.is_some() => {
                let Some(directory) = owner_directory.as_ref() else {
                    continue;
                };
                match directory.renew(remote_node.id).await {
                    Ok(true) => {
                        if session.ping(b"owner-renew".as_slice()).await.is_err() {
                            break;
                        }
                    }
                    Ok(false) => {
                        tracing::warn!(
                            remote_node_id = remote_node.id,
                            runtime_id = %directory.runtime_id(),
                            "reverse tunnel owner lease was fenced by another primary"
                        );
                        break;
                    }
                    Err(error) => {
                        tracing::warn!(
                            remote_node_id = remote_node.id,
                            runtime_id = %directory.runtime_id(),
                            "reverse tunnel owner lease renewal failed: {error}"
                        );
                        break;
                    }
                }
            }
            message = tokio::time::timeout(REMOTE_TUNNEL_STREAM_READ_TIMEOUT, stream.next()) => {
                let Some(message) = (match message {
                    Ok(message) => message,
                    Err(_) => {
                        tracing::warn!(
                            remote_node_id = remote_node.id,
                            lane_id = %lane_id,
                            timeout_secs = REMOTE_TUNNEL_STREAM_READ_TIMEOUT.as_secs(),
                            "reverse tunnel streaming lane timed out waiting for follower frames"
                        );
                        break;
                    }
                }) else {
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
                        break;
                    }
                };
                match message {
                    actix_ws::Message::Binary(bytes) => {
                        match decode_stream_frame(bytes) {
                            Ok(frame) => {
                                registry.update_last_seen(remote_node.id);
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
                                break;
                            }
                        }
                    }
                    actix_ws::Message::Ping(bytes) => {
                        if session.pong(&bytes).await.is_err() {
                            break;
                        }
                        registry.update_last_seen(remote_node.id);
                    }
                    actix_ws::Message::Pong(_) => {
                        registry.update_last_seen(remote_node.id);
                    }
                    actix_ws::Message::Close(_) => break,
                    _ => {}
                }
            }
            frame = request_rx.recv() => {
                let Some(frame) = frame else {
                    break;
                };
                let bytes = encode_stream_frame(&frame)?;
                if session.binary(bytes).await.is_err() {
                    break;
                }
            }
        }
    }

    Ok(())
}

async fn claim_tunnel_ownership<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node: &managed_follower::Model,
) -> Result<Option<std::sync::Arc<RemoteTunnelOwnerDirectory>>> {
    let owner_directory = state.remote_protocol().tunnel_owner_directory();
    if !state.config().deployment.profile.is_cluster() {
        return Ok(owner_directory);
    }
    let Some(owner_directory) = owner_directory else {
        return Err(AsterError::config_error(
            "cluster reverse tunnel requires deployment.internal_endpoint and deployment.internal_proxy_secret",
        ));
    };

    match owner_directory.try_claim(remote_node.id).await? {
        RemoteTunnelOwnerClaim::Owned(_) => Ok(Some(owner_directory)),
        RemoteTunnelOwnerClaim::Standby(owner) => {
            let owner = owner
                .map(|owner| {
                    format!(
                        "runtime {} at {}",
                        owner.runtime_id, owner.internal_endpoint
                    )
                })
                .unwrap_or_else(|| "another primary".to_string());
            Err(storage_driver_error(
                StorageErrorKind::Transient,
                format!(
                    "reverse tunnel remote node #{} is owned by {owner}",
                    remote_node.id
                ),
            ))
        }
    }
}

pub fn tunnel_info_for_node<S: RemoteProtocolRuntimeState>(
    state: &S,
    node: &managed_follower::Model,
) -> RemoteTunnelInfo {
    RemoteTunnelInfo {
        status: if state.remote_protocol().tunnel_registry().is_online(node) {
            RemoteTunnelOnlineStatus::Online
        } else {
            RemoteTunnelOnlineStatus::Offline
        },
        last_error: state
            .remote_protocol()
            .tunnel_registry()
            .last_error(node.id)
            .unwrap_or_else(|| node.tunnel_last_error.clone()),
        last_seen_at: node.tunnel_last_seen_at,
    }
}

pub async fn mark_tunnel_error<S: SharedRuntimeState>(
    state: &S,
    access_key: &str,
    error: impl std::fmt::Display,
) -> Result<()> {
    let Some(remote_node) =
        managed_follower_repo::find_by_access_key(state.writer_db(), access_key).await?
    else {
        return Ok(());
    };
    managed_follower_repo::touch_tunnel_result(
        state.writer_db(),
        remote_node.id,
        error.to_string(),
        remote_node.tunnel_last_seen_at,
    )
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
