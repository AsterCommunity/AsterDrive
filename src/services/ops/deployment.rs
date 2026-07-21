//! Deployment topology checks shared by startup, readiness, and doctor.

use sea_orm::DatabaseConnection;

use crate::config::{Config, DeploymentProfile};
use crate::db::repository::{managed_follower_repo, policy_repo};
use crate::errors::{AsterError, Result};
use crate::types::{DriverType, RemoteNodeTransportMode};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClusterTopologyReport {
    pub reverse_tunnel_nodes: Vec<(i64, String)>,
    pub local_storage_policies: Vec<(i64, String)>,
}

impl ClusterTopologyReport {
    pub fn has_issues(&self) -> bool {
        !self.reverse_tunnel_nodes.is_empty() || !self.local_storage_policies.is_empty()
    }

    pub fn issue_messages(&self) -> Vec<String> {
        let mut messages = Vec::new();
        if !self.reverse_tunnel_nodes.is_empty() {
            let nodes = self
                .reverse_tunnel_nodes
                .iter()
                .map(|(id, name)| format!("#{id} ({name})"))
                .collect::<Vec<_>>()
                .join(", ");
            messages.push(format!(
                "cluster profile has reverse tunnel remote nodes: {nodes}; use direct transport until cross-primary tunnel routing is available"
            ));
        }
        if !self.local_storage_policies.is_empty() {
            let policies = self
                .local_storage_policies
                .iter()
                .map(|(id, name)| format!("#{id} ({name})"))
                .collect::<Vec<_>>()
                .join(", ");
            messages.push(format!(
                "cluster profile has local storage policies: {policies}; use shared object storage"
            ));
        }
        messages
    }
}

pub fn validate_storage_policy_driver(config: &Config, driver_type: DriverType) -> Result<()> {
    if config.deployment.profile.is_cluster() && driver_type == DriverType::Local {
        return Err(AsterError::validation_error(
            "cluster deployment profile requires storage shared by every primary; local storage policies belong to the single profile",
        ));
    }
    Ok(())
}

pub fn validate_remote_node_transport(
    config: &Config,
    transport_mode: RemoteNodeTransportMode,
    base_url: &str,
    is_enabled: bool,
) -> Result<()> {
    if config.deployment.profile.is_cluster()
        && is_enabled
        && transport_mode.resolves_to_reverse_tunnel(base_url)
        && !config.deployment.internal_proxy_enabled()
    {
        return Err(AsterError::validation_error(
            "cluster reverse tunnel requires deployment.internal_endpoint and deployment.internal_proxy_secret on every primary",
        ));
    }
    Ok(())
}

pub async fn inspect_primary_topology(
    db: &DatabaseConnection,
    config: &Config,
) -> Result<ClusterTopologyReport> {
    if !matches!(config.deployment.profile, DeploymentProfile::Cluster) {
        return Ok(ClusterTopologyReport::default());
    }

    let reverse_tunnel_nodes = if config.deployment.internal_proxy_enabled() {
        Vec::new()
    } else {
        managed_follower_repo::find_all(db)
            .await?
            .into_iter()
            .filter(|node| {
                node.is_enabled
                    && node
                        .transport_mode
                        .resolves_to_reverse_tunnel(&node.base_url)
            })
            .map(|node| (node.id, node.name))
            .collect()
    };

    let local_storage_policies = policy_repo::find_all(db)
        .await?
        .into_iter()
        .filter(|policy| policy.driver_type == DriverType::Local)
        .map(|policy| (policy.id, policy.name))
        .collect();

    Ok(ClusterTopologyReport {
        reverse_tunnel_nodes,
        local_storage_policies,
    })
}

pub async fn validate_primary_topology(db: &DatabaseConnection, config: &Config) -> Result<()> {
    let report = inspect_primary_topology(db, config).await?;
    if !report.has_issues() {
        return Ok(());
    }

    Err(AsterError::config_error(format!(
        "deployment profile '{}' is not compatible with the current primary topology: {}",
        config.deployment.profile.as_str(),
        report.issue_messages().join("; ")
    )))
}

#[cfg(test)]
mod tests {
    use super::{
        ClusterTopologyReport, inspect_primary_topology, validate_primary_topology,
        validate_remote_node_transport, validate_storage_policy_driver,
    };
    use crate::config::{Config, DeploymentProfile};
    use crate::entities::managed_follower;
    use crate::types::RemoteNodeTransportMode;
    use migration::Migrator;
    use sea_orm::{ActiveModelTrait, Set};

    async fn setup_db() -> sea_orm::DatabaseConnection {
        let db = crate::db::connect_with_metrics(
            &crate::config::DatabaseConfig {
                url: "sqlite::memory:".to_string(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics::NoopMetrics::arc(),
        )
        .await
        .expect("deployment topology test database should connect");
        Migrator::up(&db, None)
            .await
            .expect("deployment topology test migrations should run");
        db
    }

    #[test]
    fn topology_report_describes_reverse_tunnel_and_local_storage_issues() {
        let report = ClusterTopologyReport {
            reverse_tunnel_nodes: vec![(7, "follower-a".to_string())],
            local_storage_policies: vec![(3, "local-default".to_string())],
        };

        let messages = report.issue_messages();
        assert_eq!(messages.len(), 2);
        assert!(messages[0].contains("#7 (follower-a)"));
        assert!(messages[1].contains("#3 (local-default)"));
    }

    #[test]
    fn cluster_write_guards_reject_local_storage_and_enabled_reverse_tunnel() {
        let mut config = Config::default();
        config.deployment.profile = DeploymentProfile::Cluster;

        assert!(validate_storage_policy_driver(&config, crate::types::DriverType::Local).is_err());
        assert!(
            validate_remote_node_transport(
                &config,
                RemoteNodeTransportMode::ReverseTunnel,
                "",
                true,
            )
            .is_err()
        );
        validate_remote_node_transport(&config, RemoteNodeTransportMode::ReverseTunnel, "", false)
            .expect("disabled reverse tunnel nodes may remain configured");

        config.deployment.internal_endpoint = "http://primary-a:3000".to_string();
        config.deployment.internal_proxy_secret =
            "cluster-secret-for-tests-at-least-32-bytes".to_string();
        validate_remote_node_transport(&config, RemoteNodeTransportMode::ReverseTunnel, "", true)
            .expect("configured cluster proxy should accept reverse tunnel nodes");
    }

    #[tokio::test]
    async fn cluster_topology_detects_enabled_reverse_tunnel_nodes() {
        let db = setup_db().await;
        let now = chrono::Utc::now();
        managed_follower::ActiveModel {
            name: Set("follower-a".to_string()),
            base_url: Set(String::new()),
            access_key: Set("access".to_string()),
            secret_key: Set("secret".to_string()),
            is_enabled: Set(true),
            transport_mode: Set(RemoteNodeTransportMode::ReverseTunnel),
            last_capabilities: Set("{}".to_string()),
            last_error: Set(String::new()),
            last_checked_at: Set(None),
            tunnel_last_error: Set(String::new()),
            tunnel_last_seen_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("reverse tunnel node should insert");

        let mut config = Config::default();
        config.deployment.profile = DeploymentProfile::Cluster;

        let report = inspect_primary_topology(&db, &config)
            .await
            .expect("cluster topology should be inspectable");
        assert_eq!(
            report.reverse_tunnel_nodes,
            vec![(1, "follower-a".to_string())]
        );
        assert!(validate_primary_topology(&db, &config).await.is_err());
    }

    #[tokio::test]
    async fn single_profile_skips_cluster_topology_restrictions() {
        let db = setup_db().await;
        let config = Config::default();

        let report = inspect_primary_topology(&db, &config)
            .await
            .expect("single profile topology should be inspectable");
        assert!(!report.has_issues());
        validate_primary_topology(&db, &config)
            .await
            .expect("single profile should retain current topology support");
    }
}
