use bytes::Bytes;
use http::Method;
use tokio::sync::oneshot;

use super::broker::RemoteTunnelHttpResponse;
use super::headers::request_headers;
use super::{
    REMOTE_TUNNEL_CONNECT_WAIT_TIMEOUT, REMOTE_TUNNEL_REQUEST_TIMEOUT, RemoteTunnelRegistry,
    reverse_tunnel_offline_error,
};
use crate::errors::{AsterError, Result};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::remote_protocol::tunnel::server::response::tunnel_http_response;
use crate::storage::remote_protocol::tunnel::server::{
    REMOTE_TUNNEL_BODY_LIMIT, RemoteTunnelRequest, RemoteTunnelResponse,
};
use aster_drive_model::entities::managed_follower;

#[derive(Debug)]
pub(crate) struct QueuedTunnelRequest {
    pub(crate) request: RemoteTunnelRequest,
}

#[derive(Debug)]
pub(super) struct RemoteTunnelConnection {
    pub(super) connection_id: String,
    pub(super) remote_node_id: i64,
    pub(super) request_tx: oneshot::Sender<QueuedTunnelRequest>,
}

#[derive(Debug)]
pub(super) struct PendingTunnelResponse {
    pub(super) remote_node_id: i64,
    response_tx: oneshot::Sender<RemoteTunnelResponse>,
}

struct PendingTunnelGuard<'a> {
    registry: &'a RemoteTunnelRegistry,
    request_id: String,
}

impl Drop for PendingTunnelGuard<'_> {
    fn drop(&mut self) {
        self.registry.pending.remove(&self.request_id);
    }
}

pub(crate) struct RemoteTunnelRegistrationGuard<'a> {
    registry: &'a RemoteTunnelRegistry,
    access_key: String,
    remote_node_id: i64,
    connection_id: String,
}

impl Drop for RemoteTunnelRegistrationGuard<'_> {
    fn drop(&mut self) {
        self.registry.unregister_if_same(
            &self.access_key,
            self.remote_node_id,
            &self.connection_id,
        );
    }
}

impl RemoteTunnelRegistry {
    pub(crate) fn register_poll(
        &self,
        remote_node: &managed_follower::Model,
    ) -> (
        oneshot::Receiver<QueuedTunnelRequest>,
        RemoteTunnelRegistrationGuard<'_>,
    ) {
        // Poll registrations are single-dispatch: request_sender consumes the
        // oneshot request_tx, so concurrent senders wait for later polls.
        let (request_tx, request_rx) = oneshot::channel();
        let connection_id = aster_forge_utils::id::new_uuid();
        self.connections.insert(
            remote_node.access_key.clone(),
            RemoteTunnelConnection {
                connection_id: connection_id.clone(),
                remote_node_id: remote_node.id,
                request_tx,
            },
        );
        self.update_last_seen(remote_node.id);
        self.connection_notify.notify_waiters();
        let guard = RemoteTunnelRegistrationGuard {
            registry: self,
            access_key: remote_node.access_key.clone(),
            remote_node_id: remote_node.id,
            connection_id,
        };
        (request_rx, guard)
    }

    fn unregister_if_same(&self, access_key: &str, remote_node_id: i64, connection_id: &str) {
        let should_remove = self
            .connections
            .get(access_key)
            .map(|entry| {
                entry.remote_node_id == remote_node_id && entry.connection_id == connection_id
            })
            .unwrap_or(false);
        if should_remove {
            self.connections.remove(access_key);
        }
    }

    pub async fn send(
        &self,
        remote_node: &managed_follower::Model,
        method: Method,
        path_and_query: String,
        content_length: Option<u64>,
        extra_headers: Vec<(String, String)>,
        body: Bytes,
    ) -> Result<RemoteTunnelHttpResponse> {
        if body.len() > REMOTE_TUNNEL_BODY_LIMIT {
            let error = storage_driver_error(
                StorageErrorKind::Unsupported,
                format!(
                    "reverse tunnel request body exceeds {} bytes; use direct transport or a streaming tunnel",
                    REMOTE_TUNNEL_BODY_LIMIT
                ),
            );
            self.record_error(remote_node.id, error.message());
            return Err(error);
        }

        let request_tx = match self.request_sender(remote_node).await {
            Ok(request_tx) => request_tx,
            Err(error) => {
                self.record_error(remote_node.id, error.message());
                return Err(error);
            }
        };

        let request_id = aster_forge_utils::id::new_uuid();
        let (response_tx, response_rx) = oneshot::channel();
        self.pending.insert(
            request_id.clone(),
            PendingTunnelResponse {
                remote_node_id: remote_node.id,
                response_tx,
            },
        );
        let _pending_guard = PendingTunnelGuard {
            registry: self,
            request_id: request_id.clone(),
        };
        let request = RemoteTunnelRequest {
            request_id: request_id.clone(),
            method: method.as_str().to_string(),
            headers: request_headers(remote_node, &method, &path_and_query, content_length)
                .chain(extra_headers)
                .collect(),
            path_and_query,
            body: body.to_vec(),
        };

        if request_tx.send(QueuedTunnelRequest { request }).is_err() {
            let error = storage_driver_error(
                StorageErrorKind::Transient,
                "reverse tunnel poll closed before request dispatch",
            );
            self.record_error(remote_node.id, error.message());
            return Err(error);
        }

        let response = match tokio::time::timeout(REMOTE_TUNNEL_REQUEST_TIMEOUT, response_rx).await
        {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => {
                let error = storage_driver_error(
                    StorageErrorKind::Transient,
                    format!("reverse tunnel response channel closed: {error}"),
                );
                self.record_error(remote_node.id, error.message());
                return Err(error);
            }
            Err(_) => {
                let error = storage_driver_error(
                    StorageErrorKind::Transient,
                    "reverse tunnel request timed out waiting for follower response",
                );
                self.record_error(remote_node.id, error.message());
                return Err(error);
            }
        };
        match tunnel_http_response(response) {
            Ok(response) => {
                if response.status.is_success() {
                    self.clear_error(remote_node.id);
                } else {
                    self.record_error(
                        remote_node.id,
                        format!(
                            "reverse tunnel returned HTTP {}: {}",
                            response.status,
                            String::from_utf8_lossy(&response.body)
                        ),
                    );
                }
                Ok(response)
            }
            Err(error) => {
                self.record_error(remote_node.id, error.message());
                Err(error)
            }
        }
    }

    async fn request_sender(
        &self,
        remote_node: &managed_follower::Model,
    ) -> Result<oneshot::Sender<QueuedTunnelRequest>> {
        tokio::time::timeout(REMOTE_TUNNEL_CONNECT_WAIT_TIMEOUT, async {
            loop {
                let notified = self.connection_notify.notified();
                if let Some((_, connection)) = self
                    .connections
                    .remove_if(&remote_node.access_key, |_, connection| {
                        connection.remote_node_id == remote_node.id
                    })
                {
                    return connection.request_tx;
                }
                notified.await;
            }
        })
        .await
        .map_err(|_| reverse_tunnel_offline_error(remote_node.id))
    }

    pub(crate) fn complete(
        &self,
        remote_node: &managed_follower::Model,
        response: RemoteTunnelResponse,
    ) -> Result<()> {
        let Some(pending) = self.pending.get(&response.request_id) else {
            return Err(AsterError::validation_error(
                "reverse tunnel request is no longer pending",
            ));
        };
        if pending.remote_node_id != remote_node.id {
            return Err(AsterError::auth_invalid_credentials(
                "reverse tunnel completion does not belong to this remote node",
            ));
        }
        drop(pending);

        let Some((_, pending)) = self.pending.remove(&response.request_id) else {
            return Err(AsterError::validation_error(
                "reverse tunnel request is no longer pending",
            ));
        };
        pending
            .response_tx
            .send(response)
            .map_err(|_| AsterError::validation_error("reverse tunnel request receiver closed"))
    }
}
