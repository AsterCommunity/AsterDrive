//! Deployment-aware storage connector catalog for policy creation flows.

use crate::config::Config;
use crate::errors::{AsterError, Result};
use crate::storage::StorageConnectorDescriptor;
use sea_orm::ConnectionTrait;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageConnectorCatalogContext {
    /// Descriptor lookup for existing policies, diagnostics, and editing.
    Manage,
    /// Candidate connectors for creating a new policy.
    Create,
    /// Connectors shown while creating the first default policy.
    ///
    /// Entries that cannot complete setup remain visible and advertise that
    /// restriction through `supports_initial_setup`.
    InitialSetup,
}

pub fn list_storage_connector_catalog(
    config: &Config,
    context: StorageConnectorCatalogContext,
) -> Vec<StorageConnectorDescriptor> {
    crate::storage::connectors::list_storage_driver_descriptors()
        .into_iter()
        .filter(|descriptor| connector_visible_in_context(config, descriptor, context))
        .collect()
}

pub fn connector_compatible_with_deployment(
    config: &Config,
    descriptor: &StorageConnectorDescriptor,
) -> bool {
    config.deployment.allows_instance_local_state()
        || descriptor.deployment_scope.supports_multi_primary()
}

pub async fn validate_connector_for_current_setup_state<C: ConnectionTrait>(
    db: &C,
    descriptor: &StorageConnectorDescriptor,
) -> Result<crate::services::system_setup::SystemSetupState> {
    let setup_state = crate::services::system_setup::state(db).await?;
    if setup_state == crate::services::system_setup::SystemSetupState::NeedsStorage
        && !descriptor.supports_initial_setup
    {
        return Err(AsterError::validation_error(format!(
            "storage connector '{}' requires post-setup configuration and cannot create the initial storage policy",
            descriptor.driver_type.as_str()
        )));
    }
    Ok(setup_state)
}

fn connector_visible_in_context(
    config: &Config,
    descriptor: &StorageConnectorDescriptor,
    context: StorageConnectorCatalogContext,
) -> bool {
    match context {
        StorageConnectorCatalogContext::Manage => true,
        StorageConnectorCatalogContext::Create => {
            descriptor.enabled && connector_compatible_with_deployment(config, descriptor)
        }
        StorageConnectorCatalogContext::InitialSetup => {
            descriptor.enabled && connector_compatible_with_deployment(config, descriptor)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{StorageConnectorCatalogContext, list_storage_connector_catalog};
    use crate::config::{Config, DeploymentProfile};
    use aster_drive_model::types::DriverType;

    fn driver_types(config: &Config, context: StorageConnectorCatalogContext) -> Vec<DriverType> {
        list_storage_connector_catalog(config, context)
            .into_iter()
            .map(|descriptor| descriptor.driver_type)
            .collect()
    }

    #[test]
    fn single_profile_creation_catalog_includes_instance_local_connectors() {
        let config = Config::default();

        let drivers = driver_types(&config, StorageConnectorCatalogContext::Create);

        assert!(drivers.contains(&DriverType::Local));
        assert!(drivers.contains(&DriverType::OneDrive));
        assert_eq!(drivers.len(), 7);
    }

    #[test]
    fn cluster_profile_creation_catalog_excludes_instance_local_connectors() {
        let mut config = Config::default();
        config.deployment.profile = DeploymentProfile::Cluster;

        let drivers = driver_types(&config, StorageConnectorCatalogContext::Create);

        assert!(!drivers.contains(&DriverType::Local));
        assert!(drivers.contains(&DriverType::OneDrive));
        assert_eq!(drivers.len(), 6);
    }

    #[test]
    fn initial_setup_catalog_exposes_unavailable_connectors_for_presentation() {
        let single = Config::default();
        let single_drivers = driver_types(&single, StorageConnectorCatalogContext::InitialSetup);
        assert!(single_drivers.contains(&DriverType::Local));
        assert!(single_drivers.contains(&DriverType::OneDrive));
        assert_eq!(single_drivers.len(), 7);

        let mut cluster = Config::default();
        cluster.deployment.profile = DeploymentProfile::Cluster;
        let cluster_drivers = driver_types(&cluster, StorageConnectorCatalogContext::InitialSetup);
        assert!(!cluster_drivers.contains(&DriverType::Local));
        assert!(cluster_drivers.contains(&DriverType::OneDrive));
        assert_eq!(cluster_drivers.len(), 6);
    }

    #[test]
    fn manage_catalog_preserves_incompatible_descriptors_for_existing_policies() {
        let mut config = Config::default();
        config.deployment.profile = DeploymentProfile::Cluster;

        let drivers = driver_types(&config, StorageConnectorCatalogContext::Manage);

        assert!(drivers.contains(&DriverType::Local));
        assert!(drivers.contains(&DriverType::OneDrive));
        assert_eq!(drivers.len(), 7);
    }
}
