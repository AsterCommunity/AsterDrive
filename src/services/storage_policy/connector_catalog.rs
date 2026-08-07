//! Deployment-aware storage connector catalog for policy creation flows.

use crate::config::Config;
use crate::errors::{AsterError, Result};
use aster_drive_model::types::LocaleTag;
use aster_drive_storage::{StorageConnectorDescriptor, StorageConnectorLocalizationCatalog};
use sea_orm::ConnectionTrait;
use sha2::{Digest, Sha256};

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

pub(crate) fn list_storage_connector_catalog(
    registry: &crate::storage::connectors::StorageConnectorRegistry,
    config: &Config,
    context: StorageConnectorCatalogContext,
) -> Vec<StorageConnectorDescriptor> {
    crate::storage::connectors::list_storage_driver_descriptors(registry)
        .into_iter()
        .filter(|descriptor| connector_visible_in_context(config, descriptor, context))
        .collect()
}

pub(crate) fn list_storage_connector_localizations(
    registry: &crate::storage::connectors::StorageConnectorRegistry,
    config: &Config,
    context: StorageConnectorCatalogContext,
    requested_locale: &LocaleTag,
) -> Result<StorageConnectorLocalizationCatalog> {
    let resources = list_storage_connector_catalog(registry, config, context)
        .into_iter()
        .map(|descriptor| {
            registry
                .require_localization(&descriptor.connector_id)
                .map(|localization| localization.bundle(requested_locale))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(StorageConnectorLocalizationCatalog {
        requested_locale: requested_locale.clone(),
        resources,
    })
}

/// Build a strong ETag from the catalog selection and connector revisions.
/// Message bodies are already covered by each plugin's deterministic revision.
pub(crate) fn storage_connector_localization_etag(
    context: StorageConnectorCatalogContext,
    catalog: &StorageConnectorLocalizationCatalog,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(match context {
        StorageConnectorCatalogContext::Manage => b"manage".as_slice(),
        StorageConnectorCatalogContext::Create => b"create".as_slice(),
        StorageConnectorCatalogContext::InitialSetup => b"setup".as_slice(),
    });
    hasher.update([0]);
    hasher.update(catalog.requested_locale.as_str().as_bytes());
    hasher.update([0]);
    for resource in &catalog.resources {
        hasher.update(resource.connector_id.as_str().as_bytes());
        hasher.update([0]);
        hasher.update(resource.resolved_locale.as_str().as_bytes());
        hasher.update([0]);
        hasher.update(resource.revision.as_bytes());
        hasher.update([0]);
    }
    format!("\"{}\"", hex::encode(hasher.finalize()))
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
            descriptor.connector_id.as_str()
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
            connector_compatible_with_deployment(config, descriptor)
        }
        StorageConnectorCatalogContext::InitialSetup => {
            connector_compatible_with_deployment(config, descriptor)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        StorageConnectorCatalogContext, list_storage_connector_catalog,
        list_storage_connector_localizations, storage_connector_localization_etag,
    };
    use crate::config::{Config, DeploymentProfile};
    use aster_drive_model::types::LocaleTag;
    use aster_drive_storage::ConnectorId;

    fn connector_ids(config: &Config, context: StorageConnectorCatalogContext) -> Vec<ConnectorId> {
        let registry = crate::storage::connectors::builtin_storage_connector_registry()
            .expect("built-in connector registry");
        list_storage_connector_catalog(&registry, config, context)
            .into_iter()
            .map(|descriptor| descriptor.connector_id)
            .collect()
    }

    #[test]
    fn single_profile_creation_catalog_includes_instance_local_connectors() {
        let config = Config::default();

        let drivers = connector_ids(&config, StorageConnectorCatalogContext::Create);

        assert!(drivers.contains(&ConnectorId::declared("asterdrive.storage.local")));
        assert!(drivers.contains(&ConnectorId::declared("asterdrive.storage.alibaba_oss")));
        assert!(drivers.contains(&ConnectorId::declared("asterdrive.storage.huawei_obs")));
        assert!(drivers.contains(&ConnectorId::declared("asterdrive.storage.onedrive")));
        assert_eq!(drivers.len(), 9);
    }

    #[test]
    fn cluster_profile_creation_catalog_excludes_instance_local_connectors() {
        let mut config = Config::default();
        config.deployment.profile = DeploymentProfile::Cluster;

        let drivers = connector_ids(&config, StorageConnectorCatalogContext::Create);

        assert!(!drivers.contains(&ConnectorId::declared("asterdrive.storage.local")));
        assert!(drivers.contains(&ConnectorId::declared("asterdrive.storage.alibaba_oss")));
        assert!(drivers.contains(&ConnectorId::declared("asterdrive.storage.huawei_obs")));
        assert!(drivers.contains(&ConnectorId::declared("asterdrive.storage.onedrive")));
        assert_eq!(drivers.len(), 8);
    }

    #[test]
    fn initial_setup_catalog_exposes_unavailable_connectors_for_presentation() {
        let single = Config::default();
        let single_drivers = connector_ids(&single, StorageConnectorCatalogContext::InitialSetup);
        assert!(single_drivers.contains(&ConnectorId::declared("asterdrive.storage.local")));
        assert!(single_drivers.contains(&ConnectorId::declared("asterdrive.storage.alibaba_oss")));
        assert!(single_drivers.contains(&ConnectorId::declared("asterdrive.storage.huawei_obs")));
        assert!(single_drivers.contains(&ConnectorId::declared("asterdrive.storage.onedrive")));
        assert_eq!(single_drivers.len(), 9);

        let mut cluster = Config::default();
        cluster.deployment.profile = DeploymentProfile::Cluster;
        let cluster_drivers = connector_ids(&cluster, StorageConnectorCatalogContext::InitialSetup);
        assert!(!cluster_drivers.contains(&ConnectorId::declared("asterdrive.storage.local")));
        assert!(cluster_drivers.contains(&ConnectorId::declared("asterdrive.storage.alibaba_oss")));
        assert!(cluster_drivers.contains(&ConnectorId::declared("asterdrive.storage.huawei_obs")));
        assert!(cluster_drivers.contains(&ConnectorId::declared("asterdrive.storage.onedrive")));
        assert_eq!(cluster_drivers.len(), 8);
    }

    #[test]
    fn manage_catalog_preserves_incompatible_descriptors_for_existing_policies() {
        let mut config = Config::default();
        config.deployment.profile = DeploymentProfile::Cluster;

        let drivers = connector_ids(&config, StorageConnectorCatalogContext::Manage);

        assert!(drivers.contains(&ConnectorId::declared("asterdrive.storage.local")));
        assert!(drivers.contains(&ConnectorId::declared("asterdrive.storage.alibaba_oss")));
        assert!(drivers.contains(&ConnectorId::declared("asterdrive.storage.huawei_obs")));
        assert!(drivers.contains(&ConnectorId::declared("asterdrive.storage.onedrive")));
        assert_eq!(drivers.len(), 9);
    }

    #[test]
    fn localization_catalog_uses_the_same_context_filter_and_locale_fallback() {
        let registry = crate::storage::connectors::builtin_storage_connector_registry()
            .expect("built-in connector registry");
        let mut config = Config::default();
        config.deployment.profile = DeploymentProfile::Cluster;
        let requested_locale = LocaleTag::parse("zh-CN").unwrap();

        let catalog = list_storage_connector_localizations(
            &registry,
            &config,
            StorageConnectorCatalogContext::Create,
            &requested_locale,
        )
        .unwrap();

        assert_eq!(catalog.requested_locale, requested_locale);
        assert_eq!(catalog.resources.len(), 8);
        assert!(catalog.resources.iter().all(|resource| {
            resource.connector_id.as_str() != "asterdrive.storage.local"
                && resource.resolved_locale.as_str() == "zh"
                && resource.namespace == resource.connector_id.as_str()
                && !resource.messages.is_empty()
        }));
        let onedrive = catalog
            .resources
            .iter()
            .find(|resource| resource.connector_id.as_str() == "asterdrive.storage.onedrive")
            .expect("OneDrive localization resource");
        assert_eq!(
            onedrive.messages.get("onedrive_credential_title"),
            Some(&"Microsoft Graph 凭据".to_string())
        );
    }

    #[test]
    fn localization_etag_is_stable_and_varies_by_context_and_locale() {
        let registry = crate::storage::connectors::builtin_storage_connector_registry()
            .expect("built-in connector registry");
        let config = Config::default();
        let en = list_storage_connector_localizations(
            &registry,
            &config,
            StorageConnectorCatalogContext::Manage,
            &LocaleTag::parse("en").unwrap(),
        )
        .unwrap();
        let en_again = list_storage_connector_localizations(
            &registry,
            &config,
            StorageConnectorCatalogContext::Manage,
            &LocaleTag::parse("en").unwrap(),
        )
        .unwrap();
        let zh = list_storage_connector_localizations(
            &registry,
            &config,
            StorageConnectorCatalogContext::Manage,
            &LocaleTag::parse("zh").unwrap(),
        )
        .unwrap();

        let etag = storage_connector_localization_etag(StorageConnectorCatalogContext::Manage, &en);
        assert_eq!(
            etag,
            storage_connector_localization_etag(StorageConnectorCatalogContext::Manage, &en_again,)
        );
        assert_ne!(
            etag,
            storage_connector_localization_etag(StorageConnectorCatalogContext::Create, &en)
        );
        assert_ne!(
            etag,
            storage_connector_localization_etag(StorageConnectorCatalogContext::Manage, &zh)
        );
        assert!(etag.starts_with('"') && etag.ends_with('"'));
    }
}
