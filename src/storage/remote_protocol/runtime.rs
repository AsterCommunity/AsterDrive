use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::entities::{managed_follower, storage_policy};
use crate::errors::Result;
use crate::storage::drivers::remote::RemoteDriver;
use crate::types::RemoteNodeTransportMode;

use super::RemoteStorageClient;
use super::tunnel::server::{RemoteTunnelBroker, RemoteTunnelRegistry};

#[derive(Clone)]
pub struct RemoteProtocolRuntime {
    tunnel_registry: Arc<RemoteTunnelRegistry>,
}

impl RemoteProtocolRuntime {
    pub fn new() -> Self {
        Self {
            tunnel_registry: Arc::new(RemoteTunnelRegistry::new()),
        }
    }

    pub fn set_persistence_db(&self, db: DatabaseConnection) {
        self.tunnel_registry.set_persistence_db(db);
    }

    pub fn tunnel_registry(&self) -> &Arc<RemoteTunnelRegistry> {
        &self.tunnel_registry
    }

    pub(crate) fn tunnel_broker(&self) -> Arc<dyn RemoteTunnelBroker> {
        self.tunnel_registry.clone()
    }

    pub fn client_for_node(&self, node: &managed_follower::Model) -> Result<RemoteStorageClient> {
        match node.transport_mode {
            RemoteNodeTransportMode::Direct => {
                RemoteStorageClient::new(&node.base_url, &node.access_key, &node.secret_key)
            }
            RemoteNodeTransportMode::ReverseTunnel => {
                RemoteStorageClient::new_reverse_tunnel(node, self.tunnel_broker())
            }
            RemoteNodeTransportMode::Auto => {
                if node.base_url.trim().is_empty() {
                    RemoteStorageClient::new_reverse_tunnel(node, self.tunnel_broker())
                } else {
                    RemoteStorageClient::new(&node.base_url, &node.access_key, &node.secret_key)
                }
            }
        }
    }

    pub(crate) fn driver_for_policy(
        &self,
        policy: &storage_policy::Model,
        follower: &managed_follower::Model,
    ) -> Result<RemoteDriver> {
        RemoteDriver::new_with_client(policy, follower, self.client_for_node(follower)?)
    }
}

impl Default for RemoteProtocolRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::error::StorageErrorKind;
    use crate::storage::traits::driver::PresignedDownloadOptions;
    use std::time::Duration;

    fn build_node(
        transport_mode: RemoteNodeTransportMode,
        base_url: &str,
    ) -> managed_follower::Model {
        let now = chrono::Utc::now();
        managed_follower::Model {
            id: 11,
            name: "runtime-node".to_string(),
            base_url: base_url.to_string(),
            access_key: "runtime-access".to_string(),
            secret_key: "runtime-secret".to_string(),
            is_enabled: true,
            transport_mode,
            last_capabilities: "{}".to_string(),
            last_error: String::new(),
            last_checked_at: None,
            tunnel_last_error: String::new(),
            tunnel_last_seen_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn direct_mode_requires_base_url() {
        let runtime = RemoteProtocolRuntime::new();
        let node = build_node(RemoteNodeTransportMode::Direct, "");

        let error = match runtime.client_for_node(&node) {
            Ok(_) => panic!("direct remote transport should require base_url"),
            Err(error) => error,
        };

        assert!(error.message().contains("base_url is required"));
    }

    #[test]
    fn reverse_tunnel_mode_accepts_empty_base_url_and_disables_presigned_urls() {
        let runtime = RemoteProtocolRuntime::new();
        let node = build_node(RemoteNodeTransportMode::ReverseTunnel, "");
        let client = runtime
            .client_for_node(&node)
            .expect("reverse tunnel client should not require base_url");

        let error = client
            .presigned_put_url("object.bin", Duration::from_secs(60))
            .expect_err("reverse tunnel transport should not support presigned PUT URLs");

        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Unsupported)
        );
        assert!(error.message().contains("does not support presigned"));
    }

    #[test]
    fn auto_mode_without_base_url_resolves_to_reverse_tunnel() {
        let runtime = RemoteProtocolRuntime::new();
        let node = build_node(RemoteNodeTransportMode::Auto, "   ");
        let client = runtime
            .client_for_node(&node)
            .expect("auto client without base_url should use reverse tunnel");

        let error = client
            .presigned_url(
                "object.bin",
                Duration::from_secs(60),
                PresignedDownloadOptions::default(),
            )
            .expect_err("auto empty base_url should not support direct presigned URLs");

        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Unsupported)
        );
        assert!(error.message().contains("does not support presigned"));
    }

    #[test]
    fn auto_mode_with_base_url_resolves_to_direct_transport() {
        let runtime = RemoteProtocolRuntime::new();
        let node = build_node(
            RemoteNodeTransportMode::Auto,
            "http://storage.example.com/root/",
        );
        let client = runtime
            .client_for_node(&node)
            .expect("auto client with base_url should use direct transport");

        let url = client
            .presigned_put_url("object.bin", Duration::from_secs(60))
            .expect("direct transport should build presigned PUT URL");
        let parsed = reqwest::Url::parse(&url).expect("presigned URL should parse");

        assert_eq!(
            parsed.path(),
            "/root/api/v1/internal/storage/objects/object.bin"
        );
        assert!(parsed.query_pairs().any(|(key, value)| key
            == crate::storage::remote_protocol::PRESIGNED_AUTH_ACCESS_KEY_QUERY
            && value == "runtime-access"));
    }
}
