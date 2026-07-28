//! Deployment topology checks shared by startup, readiness, and doctor.

use sea_orm::DatabaseConnection;

use crate::config::Config;
use crate::db::repository::{managed_follower_repo, policy_repo};
use crate::errors::{AsterError, Result};
use aster_drive_model::types::{DriverType, RemoteNodeTransportMode, UploadSessionKind};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeploymentTopologyReport {
    pub reverse_tunnel_nodes: Vec<(i64, String)>,
    pub instance_local_storage_policies: Vec<(i64, String)>,
}

impl DeploymentTopologyReport {
    pub fn has_issues(&self) -> bool {
        !self.reverse_tunnel_nodes.is_empty() || !self.instance_local_storage_policies.is_empty()
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
                "this deployment profile has reverse tunnel remote nodes without internal primary proxying: {nodes}; configure internal proxying or use direct transport"
            ));
        }
        if !self.instance_local_storage_policies.is_empty() {
            let policies = self
                .instance_local_storage_policies
                .iter()
                .map(|(id, name)| format!("#{id} ({name})"))
                .collect::<Vec<_>>()
                .join(", ");
            messages.push(format!(
                "this deployment profile has instance-local storage policies: {policies}; use storage shared by every primary"
            ));
        }
        messages
    }
}

pub fn validate_storage_policy_driver(config: &Config, driver_type: DriverType) -> Result<()> {
    let descriptor = crate::storage::connectors::storage_driver_descriptor(driver_type)?;
    if !crate::services::storage_policy::connector_catalog::connector_compatible_with_deployment(
        config,
        &descriptor,
    ) {
        return Err(AsterError::validation_error(format!(
            "this deployment profile requires storage shared by every primary; connector '{}' has deployment scope '{}'",
            driver_type.as_str(),
            descriptor.deployment_scope.as_str()
        )));
    }
    Ok(())
}

pub fn validate_upload_session_kind(config: &Config, kind: UploadSessionKind) -> Result<()> {
    if !config.deployment.allows_instance_local_state()
        && matches!(
            kind,
            UploadSessionKind::OffsetStaging | UploadSessionKind::StreamStaging
        )
    {
        return Err(AsterError::validation_error(format!(
            "this deployment profile cannot initialize upload session kind '{}': Pod-local staging is not shared across primaries; use a connector-native multipart, presigned, or frontend-direct resumable upload mode",
            kind.as_str()
        )));
    }
    Ok(())
}

pub fn validate_remote_node_transport(
    config: &Config,
    transport_mode: RemoteNodeTransportMode,
    base_url: &str,
    is_enabled: bool,
) -> Result<()> {
    if config.deployment.requires_shared_runtime()
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
) -> Result<DeploymentTopologyReport> {
    if !config.deployment.requires_shared_runtime() {
        return Ok(DeploymentTopologyReport::default());
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

    let mut instance_local_storage_policies = Vec::new();
    for policy in policy_repo::find_all(db).await? {
        let descriptor = crate::storage::connectors::storage_driver_descriptor(policy.driver_type)?;
        if !crate::services::storage_policy::connector_catalog::connector_compatible_with_deployment(
            config,
            &descriptor,
        ) {
            instance_local_storage_policies.push((policy.id, policy.name));
        }
    }

    Ok(DeploymentTopologyReport {
        reverse_tunnel_nodes,
        instance_local_storage_policies,
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
        DeploymentTopologyReport, inspect_primary_topology, validate_primary_topology,
        validate_remote_node_transport, validate_storage_policy_driver,
        validate_upload_session_kind,
    };
    use crate::config::{Config, DeploymentProfile};
    use aster_drive_migration::Migrator;
    use aster_drive_model::entities::managed_follower;
    use aster_drive_model::types::{RemoteNodeTransportMode, UploadSessionKind};
    use sea_orm::{ActiveModelTrait, Set};

    async fn setup_db() -> sea_orm::DatabaseConnection {
        let db = crate::db::connect_with_metrics(
            &crate::config::DatabaseConfig {
                url: "sqlite::memory:".into(),
                pool_size: 1,
                retry_count: 0,
            },
            aster_drive_metrics::NoopMetrics::arc(),
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
        let report = DeploymentTopologyReport {
            reverse_tunnel_nodes: vec![(7, "follower-a".to_string())],
            instance_local_storage_policies: vec![(3, "local-default".to_string())],
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

        let error =
            validate_storage_policy_driver(&config, aster_drive_model::types::DriverType::Local)
                .expect_err("cluster profile must reject instance-local connectors")
                .to_string();
        assert!(error.contains("local"));
        assert!(error.contains("instance_local"));
        for driver_type in [
            aster_drive_model::types::DriverType::S3,
            aster_drive_model::types::DriverType::Sftp,
            aster_drive_model::types::DriverType::AzureBlob,
            aster_drive_model::types::DriverType::TencentCos,
            aster_drive_model::types::DriverType::Remote,
            aster_drive_model::types::DriverType::OneDrive,
        ] {
            validate_storage_policy_driver(&config, driver_type).unwrap_or_else(|error| {
                panic!("cluster profile rejected shared connector {driver_type:?}: {error}")
            });
        }
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

    #[test]
    fn single_profile_accepts_every_upload_session_kind() {
        let config = Config::default();

        for kind in all_upload_session_kinds() {
            validate_upload_session_kind(&config, kind).unwrap_or_else(|error| {
                panic!("single profile rejected {}: {error}", kind.as_str())
            });
        }
    }

    #[test]
    fn cluster_profile_rejects_only_pod_local_staging_upload_sessions() {
        let mut config = Config::default();
        config.deployment.profile = DeploymentProfile::Cluster;

        for kind in all_upload_session_kinds() {
            let result = validate_upload_session_kind(&config, kind);
            if matches!(
                kind,
                UploadSessionKind::OffsetStaging | UploadSessionKind::StreamStaging
            ) {
                let error = result.unwrap_err().to_string();
                assert!(error.contains(kind.as_str()));
                assert!(error.contains("Pod-local staging"));
                assert!(error.contains("connector-native"));
            } else {
                result.unwrap_or_else(|error| {
                    panic!("cluster profile rejected {}: {error}", kind.as_str())
                });
            }
        }
    }

    fn all_upload_session_kinds() -> [UploadSessionKind; 9] {
        [
            UploadSessionKind::OffsetStaging,
            UploadSessionKind::StreamStaging,
            UploadSessionKind::ProviderRelayMultipart,
            UploadSessionKind::ProviderPresignedSingle,
            UploadSessionKind::ProviderPresignedMultipart,
            UploadSessionKind::RemoteRelayMultipart,
            UploadSessionKind::RemotePresignedSingle,
            UploadSessionKind::RemotePresignedMultipart,
            UploadSessionKind::ProviderDirectResumable,
        ]
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
