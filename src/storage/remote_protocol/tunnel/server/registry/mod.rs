use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use sea_orm::DatabaseConnection;
use tokio::sync::Notify;

use crate::entities::managed_follower;
use crate::storage::error::{StorageErrorKind, storage_driver_error};

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

#[derive(Default)]
pub struct RemoteTunnelRegistry {
    connections: DashMap<String, RemoteTunnelConnection>,
    stream_lanes: DashMap<String, Vec<Arc<StreamingTunnelLane>>>,
    pending: DashMap<String, PendingTunnelResponse>,
    stream_pending: DashMap<String, PendingStreamResponse>,
    last_errors: DashMap<i64, String>,
    last_seen_at: DashMap<i64, chrono::DateTime<chrono::Utc>>,
    persistence_db: parking_lot::RwLock<Option<DatabaseConnection>>,
    connection_notify: Notify,
}

impl RemoteTunnelRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_persistence_db(&self, db: DatabaseConnection) {
        *self.persistence_db.write() = Some(db);
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

    pub fn last_error(&self, remote_node_id: i64) -> Option<String> {
        self.last_errors
            .get(&remote_node_id)
            .map(|entry| entry.value().clone())
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
    storage_driver_error(
        StorageErrorKind::Transient,
        format!("remote node #{remote_node_id} reverse tunnel is offline"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_tunnel_seen_time_obeys_online_ttl_boundary() {
        assert!(is_recent_tunnel_seen_at(chrono::Utc::now()));
        let expired = chrono::Utc::now()
            - chrono::Duration::from_std(REMOTE_TUNNEL_ONLINE_TTL).unwrap()
            - chrono::Duration::milliseconds(1);
        assert!(!is_recent_tunnel_seen_at(expired));
    }
}
