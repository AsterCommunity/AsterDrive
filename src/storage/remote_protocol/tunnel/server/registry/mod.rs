use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use sea_orm::DatabaseConnection;
use tokio::sync::Notify;

use crate::config::RuntimeConfig;
use crate::services::ops::audit::{self, AuditContext, AuditLogInput};
use aster_drive_model::entities::managed_follower;
use aster_drive_model::types::{AuditAction, AuditEntityType};
use aster_drive_storage::StorageErrorKind;

mod broker;
mod headers;
mod persistence;
mod polling;
mod streaming;

pub use broker::{RemoteTunnelBroker, RemoteTunnelHttpResponse, RemoteTunnelStreamHttpResponse};

use persistence::persist_tunnel_error;
use polling::{PendingTunnelResponse, RemoteTunnelConnection};
use streaming::{PendingStreamResponse, StreamingTunnelLane};

const REMOTE_TUNNEL_CONNECT_WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const REMOTE_TUNNEL_REQUEST_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const REMOTE_TUNNEL_ONLINE_TTL: Duration = Duration::from_secs(75);
const REMOTE_TUNNEL_STREAM_CHANNEL_CAPACITY: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TunnelDisconnectReason {
    GracefulShutdown,
    PeerClose,
    Eof,
    ConnectionReset,
    ProtocolReadError,
    ProtocolDecodeError,
    HeartbeatTimeout,
    HeartbeatSendFailed,
    OwnerFenced,
    OwnerRenewalFailed,
    CloseHandshakeFailed,
}

impl TunnelDisconnectReason {
    fn action(self) -> AuditAction {
        match self {
            Self::GracefulShutdown | Self::PeerClose => AuditAction::RemoteNodeGracefulDisconnect,
            Self::HeartbeatTimeout => AuditAction::RemoteNodeHeartbeatTimeout,
            Self::Eof
            | Self::ConnectionReset
            | Self::ProtocolReadError
            | Self::ProtocolDecodeError
            | Self::HeartbeatSendFailed
            | Self::OwnerFenced
            | Self::OwnerRenewalFailed
            | Self::CloseHandshakeFailed => AuditAction::RemoteNodeUnexpectedDisconnect,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::GracefulShutdown => "primary_shutdown",
            Self::PeerClose => "peer_close",
            Self::Eof => "eof",
            Self::ConnectionReset => "connection_reset",
            Self::ProtocolReadError => "protocol_read_error",
            Self::ProtocolDecodeError => "protocol_decode_error",
            Self::HeartbeatTimeout => "heartbeat_timeout",
            Self::HeartbeatSendFailed => "heartbeat_send_failed",
            Self::OwnerFenced => "owner_fenced",
            Self::OwnerRenewalFailed => "owner_renewal_failed",
            Self::CloseHandshakeFailed => "close_handshake_failed",
        }
    }

    const fn priority(self) -> u8 {
        match self {
            Self::GracefulShutdown | Self::PeerClose => 0,
            Self::Eof
            | Self::ConnectionReset
            | Self::ProtocolReadError
            | Self::ProtocolDecodeError
            | Self::HeartbeatSendFailed
            | Self::OwnerFenced
            | Self::OwnerRenewalFailed
            | Self::CloseHandshakeFailed => 1,
            Self::HeartbeatTimeout => 2,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct ConnectionLifecycleState {
    online: bool,
    generation: u64,
    outage_generation: u64,
    active_lanes: usize,
    lane_count: usize,
    first_lane_id: Option<String>,
    pending_disconnect_reason: Option<TunnelDisconnectReason>,
    last_disconnect_reason: Option<TunnelDisconnectReason>,
    observation_revision: u64,
}

#[derive(Default)]
pub struct RemoteTunnelRegistry {
    connections: DashMap<String, RemoteTunnelConnection>,
    stream_lanes: DashMap<String, Vec<Arc<StreamingTunnelLane>>>,
    pending: DashMap<String, PendingTunnelResponse>,
    stream_pending: DashMap<String, PendingStreamResponse>,
    last_errors: DashMap<i64, String>,
    last_seen_at: DashMap<i64, chrono::DateTime<chrono::Utc>>,
    lifecycle: DashMap<i64, ConnectionLifecycleState>,
    persistence_db: parking_lot::RwLock<Option<DatabaseConnection>>,
    audit_runtime_config: parking_lot::RwLock<Option<Arc<RuntimeConfig>>>,
    connection_notify: Notify,
}

impl RemoteTunnelRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_persistence_db(&self, db: DatabaseConnection) {
        *self.persistence_db.write() = Some(db);
    }

    pub fn set_audit_runtime_config(&self, runtime_config: Arc<RuntimeConfig>) {
        *self.audit_runtime_config.write() = Some(runtime_config);
    }

    pub fn is_online(&self, remote_node: &managed_follower::Model) -> bool {
        let local_last_seen = self
            .last_seen_at
            .get(&remote_node.id)
            .map(|last_seen_at| *last_seen_at.value());
        local_last_seen
            .or(remote_node.tunnel_last_seen_at)
            .is_some_and(is_recent_tunnel_seen_at)
    }

    pub(crate) fn update_last_seen(&self, remote_node_id: i64) {
        self.last_seen_at.insert(remote_node_id, chrono::Utc::now());
    }

    pub(crate) fn record_handshake(
        self: &Arc<Self>,
        remote_node: &managed_follower::Model,
        lane_id: Option<&str>,
    ) {
        let (event, observation_revision) = {
            let mut state = self.lifecycle.entry(remote_node.id).or_default();
            state.observation_revision = state.observation_revision.saturating_add(1);
            state.pending_disconnect_reason = None;
            if let Some(lane_id) = lane_id {
                state.active_lanes = state.active_lanes.saturating_add(1);
                state.lane_count = state.lane_count.max(state.active_lanes);
                if state.first_lane_id.is_none() {
                    state.first_lane_id = Some(lane_id.to_string());
                }
            }
            let event = if state.online {
                None
            } else {
                state.online = true;
                state.generation = state.generation.saturating_add(1);
                Some((state.clone(), lane_id.map(ToOwned::to_owned)))
            };
            (event, state.observation_revision)
        };
        if let Some((state, lane_id)) = event {
            self.record_lifecycle_audit(
                remote_node,
                AuditAction::RemoteNodeConnected,
                "connected",
                &state,
                lane_id.as_deref().or(state.first_lane_id.as_deref()),
                chrono::Utc::now(),
            );
        }
        self.schedule_lifecycle_expiry(remote_node.clone(), observation_revision);
    }

    pub(crate) fn record_stream_disconnect(
        self: &Arc<Self>,
        remote_node: &managed_follower::Model,
        reason: TunnelDisconnectReason,
    ) {
        let graceful_event = {
            let mut state = self.lifecycle.entry(remote_node.id).or_default();
            if !state.online {
                return;
            }
            state.active_lanes = state.active_lanes.saturating_sub(1);
            state.pending_disconnect_reason = Some(
                state
                    .pending_disconnect_reason
                    .filter(|current| current.priority() >= reason.priority())
                    .unwrap_or(reason),
            );
            if state.active_lanes == 0 && reason.priority() == 0 {
                state.observation_revision = state.observation_revision.saturating_add(1);
                state.online = false;
                state.outage_generation = state.outage_generation.saturating_add(1);
                let reason = state.pending_disconnect_reason.take().unwrap_or(reason);
                state.last_disconnect_reason = Some(reason);
                let event = state.clone();
                state.first_lane_id = None;
                state.lane_count = 0;
                Some((event, reason))
            } else {
                None
            }
        };
        if let Some((state, reason)) = graceful_event {
            self.record_lifecycle_audit(
                remote_node,
                reason.action(),
                reason.as_str(),
                &state,
                state.first_lane_id.as_deref(),
                chrono::Utc::now(),
            );
        }
    }

    fn schedule_lifecycle_expiry(
        self: &Arc<Self>,
        remote_node: managed_follower::Model,
        observation_revision: u64,
    ) {
        let registry = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(REMOTE_TUNNEL_ONLINE_TTL).await;
            registry.expire_lifecycle_if_stale(&remote_node, observation_revision);
        });
    }

    fn expire_lifecycle_if_stale(
        &self,
        remote_node: &managed_follower::Model,
        observation_revision: u64,
    ) {
        let event = {
            let mut state = self.lifecycle.entry(remote_node.id).or_default();
            if !state.online || state.observation_revision != observation_revision {
                return;
            }
            state.online = false;
            state.outage_generation = state.outage_generation.saturating_add(1);
            let reason = state
                .pending_disconnect_reason
                .take()
                .unwrap_or(TunnelDisconnectReason::Eof);
            state.last_disconnect_reason = Some(reason);
            let event = state.clone();
            state.active_lanes = 0;
            state.first_lane_id = None;
            state.lane_count = 0;
            (event, reason)
        };
        let (state, reason) = event;
        self.record_lifecycle_audit(
            remote_node,
            reason.action(),
            reason.as_str(),
            &state,
            state.first_lane_id.as_deref(),
            chrono::Utc::now(),
        );
    }

    fn record_lifecycle_audit(
        &self,
        remote_node: &managed_follower::Model,
        action: AuditAction,
        reason: &'static str,
        state: &ConnectionLifecycleState,
        first_lane_id: Option<&str>,
        observed_at: chrono::DateTime<chrono::Utc>,
    ) {
        let Some(db) = self.persistence_db.read().clone() else {
            return;
        };
        let Some(runtime_config) = self.audit_runtime_config.read().clone() else {
            return;
        };
        let details = audit::details(audit::RemoteNodeConnectionAuditDetails {
            remote_node_id: remote_node.id,
            binding_id: remote_node.id,
            transport: remote_node
                .transport_mode
                .resolve(&remote_node.base_url)
                .as_str(),
            reason,
            generation: state.generation,
            outage_generation: state.outage_generation,
            active_lanes: state.active_lanes,
            lane_count: state.lane_count,
            observed_at,
            first_lane_id,
        });
        let entity_id = remote_node.id;
        let entity_name = remote_node.name.clone();
        tokio::spawn(async move {
            let ctx = AuditContext::system();
            audit::log_with_db_and_config(
                &db,
                &runtime_config,
                AuditLogInput {
                    ctx: &ctx,
                    action,
                    entity_type: AuditEntityType::RemoteNode,
                    entity_id: Some(entity_id),
                    entity_name: Some(&entity_name),
                },
                || details,
            )
            .await;
        });
    }

    pub fn last_error(&self, remote_node_id: i64) -> Option<String> {
        self.last_errors
            .get(&remote_node_id)
            .map(|entry| entry.value().clone())
    }

    #[cfg(test)]
    pub(crate) fn pending_poll_request_count(&self) -> usize {
        self.pending.len()
    }

    fn record_error(&self, remote_node_id: i64, error: impl Into<String>) {
        let error = error.into();
        if error.trim().is_empty() {
            self.clear_error(remote_node_id);
        } else {
            self.last_errors.insert(remote_node_id, error);
            self.persist_error(remote_node_id);
        }
    }

    pub(super) fn clear_error(&self, remote_node_id: i64) {
        if self.last_errors.remove(&remote_node_id).is_some() {
            self.persist_error(remote_node_id);
        }
    }

    fn persist_error(&self, remote_node_id: i64) {
        let Some(db) = self.persistence_db.read().clone() else {
            return;
        };
        let error = self.last_error(remote_node_id).unwrap_or_default();
        tokio::spawn(async move {
            if let Err(persist_error) = persist_tunnel_error(&db, remote_node_id, error).await {
                tracing::warn!(
                    remote_node_id,
                    "failed to persist reverse tunnel error state: {persist_error}"
                );
            }
        });
    }
}

fn is_recent_tunnel_seen_at(last_seen_at: chrono::DateTime<chrono::Utc>) -> bool {
    chrono::Duration::from_std(REMOTE_TUNNEL_ONLINE_TTL)
        .ok()
        .is_some_and(|ttl| last_seen_at + ttl > chrono::Utc::now())
}

pub fn reverse_tunnel_offline_error(remote_node_id: i64) -> crate::errors::AsterError {
    crate::errors::storage_driver_error(
        StorageErrorKind::Transient,
        format!("remote node #{remote_node_id} reverse tunnel is offline"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_drive_model::types::RemoteNodeTransportMode;

    fn test_remote_node() -> managed_follower::Model {
        let now = chrono::Utc::now();
        managed_follower::Model {
            id: 42,
            name: "edge-a".to_string(),
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
            binding_revision: 1,
            binding_applied_revision: 1,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn persisted_tunnel_seen_time_obeys_online_ttl_boundary() {
        assert!(is_recent_tunnel_seen_at(chrono::Utc::now()));
        let expired = chrono::Utc::now()
            - chrono::Duration::from_std(REMOTE_TUNNEL_ONLINE_TTL).unwrap()
            - chrono::Duration::milliseconds(1);
        assert!(!is_recent_tunnel_seen_at(expired));
    }

    #[tokio::test]
    async fn lifecycle_aggregates_four_lanes_into_one_outage_generation() {
        let registry = Arc::new(RemoteTunnelRegistry::new());
        let node = test_remote_node();

        for lane in ["lane-0", "lane-1", "lane-2", "lane-3"] {
            registry.record_handshake(&node, Some(lane));
        }
        registry.record_handshake(&node, None);
        registry.record_handshake(&node, None);
        let state = registry.lifecycle.get(&node.id).expect("lifecycle state");
        assert!(state.online);
        assert_eq!(state.generation, 1);
        assert_eq!(state.outage_generation, 0);
        assert_eq!(state.active_lanes, 4);
        assert_eq!(state.lane_count, 4);
        drop(state);

        for _ in 0..3 {
            registry.record_stream_disconnect(&node, TunnelDisconnectReason::Eof);
        }
        let state = registry.lifecycle.get(&node.id).expect("lifecycle state");
        assert!(state.online);
        assert_eq!(state.active_lanes, 1);
        drop(state);

        registry.record_stream_disconnect(&node, TunnelDisconnectReason::HeartbeatTimeout);
        let state = registry.lifecycle.get(&node.id).expect("lifecycle state");
        assert!(state.online);
        assert_eq!(state.active_lanes, 0);
        assert_eq!(state.generation, 1);
        assert_eq!(state.outage_generation, 0);
        assert_eq!(state.lane_count, 4);
        assert_eq!(state.last_disconnect_reason, None);
        drop(state);

        registry.record_stream_disconnect(&node, TunnelDisconnectReason::Eof);
        let state = registry.lifecycle.get(&node.id).expect("lifecycle state");
        assert_eq!(state.outage_generation, 0);
        let revision = state.observation_revision;
        drop(state);

        registry.expire_lifecycle_if_stale(&node, revision);
        let state = registry.lifecycle.get(&node.id).expect("lifecycle state");
        assert!(!state.online);
        assert_eq!(state.outage_generation, 1);
        assert_eq!(state.lane_count, 0);
        assert_eq!(
            state.last_disconnect_reason,
            Some(TunnelDisconnectReason::HeartbeatTimeout)
        );
        drop(state);

        registry.record_handshake(&node, Some("lane-recovered"));
        let state = registry.lifecycle.get(&node.id).expect("lifecycle state");
        assert!(state.online);
        assert_eq!(state.generation, 2);
        assert_eq!(state.outage_generation, 1);
        assert_eq!(state.active_lanes, 1);
    }

    #[test]
    fn lifecycle_reason_codes_keep_graceful_and_failure_actions_distinct() {
        assert_eq!(
            TunnelDisconnectReason::GracefulShutdown.action(),
            AuditAction::RemoteNodeGracefulDisconnect
        );
        assert_eq!(
            TunnelDisconnectReason::PeerClose.action(),
            AuditAction::RemoteNodeGracefulDisconnect
        );
        assert_eq!(
            TunnelDisconnectReason::HeartbeatTimeout.action(),
            AuditAction::RemoteNodeHeartbeatTimeout
        );
        assert_eq!(
            TunnelDisconnectReason::ConnectionReset.action(),
            AuditAction::RemoteNodeUnexpectedDisconnect
        );
        assert_eq!(
            TunnelDisconnectReason::CloseHandshakeFailed.action(),
            AuditAction::RemoteNodeUnexpectedDisconnect
        );
        assert_eq!(TunnelDisconnectReason::OwnerFenced.as_str(), "owner_fenced");
    }

    #[tokio::test]
    async fn lifecycle_keeps_strongest_lane_failure_until_aggregate_disconnect() {
        let registry = Arc::new(RemoteTunnelRegistry::new());
        let node = test_remote_node();
        registry.record_handshake(&node, Some("lane-0"));
        registry.record_handshake(&node, Some("lane-1"));

        registry.record_stream_disconnect(&node, TunnelDisconnectReason::HeartbeatTimeout);
        registry.record_stream_disconnect(&node, TunnelDisconnectReason::PeerClose);

        let state = registry.lifecycle.get(&node.id).expect("lifecycle state");
        assert_eq!(state.outage_generation, 1);
        assert_eq!(
            state.last_disconnect_reason,
            Some(TunnelDisconnectReason::HeartbeatTimeout)
        );
    }

    #[tokio::test]
    async fn stale_expiry_cannot_disconnect_after_a_new_poll_handshake() {
        let registry = Arc::new(RemoteTunnelRegistry::new());
        let node = test_remote_node();
        registry.record_handshake(&node, Some("lane-0"));
        let stale_revision = registry
            .lifecycle
            .get(&node.id)
            .expect("lifecycle state")
            .observation_revision;
        registry.record_stream_disconnect(&node, TunnelDisconnectReason::Eof);
        registry.record_handshake(&node, None);

        registry.expire_lifecycle_if_stale(&node, stale_revision);

        let state = registry.lifecycle.get(&node.id).expect("lifecycle state");
        assert!(state.online);
        assert_eq!(state.generation, 1);
        assert_eq!(state.outage_generation, 0);
        assert_eq!(state.pending_disconnect_reason, None);
    }

    #[tokio::test]
    async fn graceful_last_lane_disconnects_immediately() {
        let registry = Arc::new(RemoteTunnelRegistry::new());
        let node = test_remote_node();
        registry.record_handshake(&node, Some("lane-0"));

        registry.record_stream_disconnect(&node, TunnelDisconnectReason::GracefulShutdown);

        let state = registry.lifecycle.get(&node.id).expect("lifecycle state");
        assert!(!state.online);
        assert_eq!(state.generation, 1);
        assert_eq!(state.outage_generation, 1);
        assert_eq!(
            state.last_disconnect_reason,
            Some(TunnelDisconnectReason::GracefulShutdown)
        );
    }

    #[test]
    fn lifecycle_details_are_redacted_and_do_not_include_credentials() {
        let node = test_remote_node();
        let state = ConnectionLifecycleState {
            online: true,
            generation: 3,
            outage_generation: 2,
            active_lanes: 1,
            lane_count: 4,
            first_lane_id: Some("lane-0".to_string()),
            pending_disconnect_reason: None,
            last_disconnect_reason: Some(TunnelDisconnectReason::ConnectionReset),
            observation_revision: 3,
        };
        let details = audit::details(audit::RemoteNodeConnectionAuditDetails {
            remote_node_id: node.id,
            binding_id: node.id,
            transport: node.transport_mode.resolve(&node.base_url).as_str(),
            reason: "connected",
            generation: state.generation,
            outage_generation: state.outage_generation,
            active_lanes: state.active_lanes,
            lane_count: state.lane_count,
            observed_at: chrono::Utc::now(),
            first_lane_id: state.first_lane_id.as_deref(),
        })
        .expect("details should serialize");
        let encoded = details.to_string();
        assert!(encoded.contains("binding_id"));
        assert!(!encoded.contains(&node.access_key));
        assert!(!encoded.contains(&node.secret_key));
        assert!(!encoded.contains("signature"));
        assert!(!encoded.contains("token"));
    }
}
