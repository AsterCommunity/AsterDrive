use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{Result, validation_error_with_code};
use crate::services::remote::storage_target::{
    RemoteStorageTargetConnectorDescriptor, registered_remote_storage_target_connector_ids,
    remote_storage_target_connector_descriptor,
};
use crate::storage::remote_protocol::RemoteStorageCapabilities;
use aster_drive_model::entities::managed_follower;
use aster_drive_model::types::{RemoteDownloadStrategy, RemoteUploadStrategy};
use aster_drive_storage::StorageErrorKind;

#[derive(Debug, Clone)]
pub struct RemoteCapabilityResolver {
    remote_node_id: i64,
    capabilities: RemoteStorageCapabilities,
}

impl RemoteCapabilityResolver {
    pub fn from_remote_node(node: &managed_follower::Model) -> Self {
        Self::from_last_capabilities(node.id, &node.last_capabilities)
    }

    pub fn from_last_capabilities(remote_node_id: i64, last_capabilities: &str) -> Self {
        Self::from_capabilities(
            remote_node_id,
            RemoteStorageCapabilities::from_stored_json(last_capabilities),
        )
    }

    pub fn from_capabilities(remote_node_id: i64, capabilities: RemoteStorageCapabilities) -> Self {
        Self {
            remote_node_id,
            capabilities,
        }
    }

    pub fn capabilities(&self) -> &RemoteStorageCapabilities {
        &self.capabilities
    }

    pub fn ensure_protocol_compatible(&self, context: &str) -> Result<()> {
        self.capabilities.validate_protocol(context)
    }

    pub fn ensure_remote_policy_config_supported(
        &self,
        policy_id: i64,
        download_strategy: RemoteDownloadStrategy,
        upload_strategy: RemoteUploadStrategy,
    ) -> Result<()> {
        let context = format!(
            "remote storage policy #{policy_id} on remote node #{}",
            self.remote_node_id
        );
        self.ensure_protocol_compatible(&context)?;
        self.ensure_features(&context, &self.base_policy_required_features())?;
        self.ensure_presigned_cors_for_strategies(
            download_strategy,
            upload_strategy,
            &context,
            &context,
        )?;

        Ok(())
    }

    pub fn ensure_binding_policy_configs_supported(
        &self,
        remote_node_name: &str,
        policy_requirements: &[(i64, RemoteDownloadStrategy, RemoteUploadStrategy)],
    ) -> Result<()> {
        let context = format!(
            "remote node #{} ('{remote_node_name}') binding reload",
            self.remote_node_id
        );
        self.ensure_protocol_compatible(&context)?;
        if policy_requirements.is_empty() {
            return Ok(());
        }

        self.ensure_features(&context, &self.base_policy_required_features())?;
        for (policy_id, download_strategy, upload_strategy) in policy_requirements {
            let download_context =
                format!("{context}; policy #{policy_id} requires remote presigned download");
            let upload_context =
                format!("{context}; policy #{policy_id} requires remote presigned upload");
            self.ensure_presigned_cors_for_strategies(
                *download_strategy,
                *upload_strategy,
                &download_context,
                &upload_context,
            )?;
        }

        Ok(())
    }

    pub fn remote_storage_target_connector_descriptors(
        &self,
    ) -> Vec<RemoteStorageTargetConnectorDescriptor> {
        self.supported_registered_remote_storage_target_connector_ids()
            .into_iter()
            .filter_map(|connector_id| {
                remote_storage_target_connector_descriptor(&connector_id).ok()
            })
            .collect()
    }

    pub fn ensure_remote_storage_target_connector_supported(
        &self,
        connector_id: &str,
    ) -> Result<()> {
        let context = format!(
            "remote storage target write on remote node #{}",
            self.remote_node_id
        );
        if !self.supports_remote_storage_target_connector(connector_id) {
            return Err(validation_error_with_code(
                ApiErrorCode::ManagedIngressDriverUnsupported,
                format!(
                    "remote node #{} does not declare remote storage target connector '{}'",
                    self.remote_node_id, connector_id
                ),
            ));
        }

        self.ensure_protocol_compatible(&context)
    }

    pub fn supports_remote_storage_target_connector(&self, connector_id: &str) -> bool {
        self.capabilities
            .remote_storage_target
            .as_ref()
            .cloned()
            .unwrap_or_default()
            .supports_connector(connector_id)
            && remote_storage_target_connector_descriptor(connector_id).is_ok()
    }

    pub fn requires_direct_transport_for_presigned(
        download_strategy: RemoteDownloadStrategy,
        upload_strategy: RemoteUploadStrategy,
    ) -> bool {
        download_strategy == RemoteDownloadStrategy::Presigned
            || upload_strategy == RemoteUploadStrategy::Presigned
    }

    fn supported_registered_remote_storage_target_connector_ids(&self) -> Vec<String> {
        let remote_storage_target = self
            .capabilities
            .remote_storage_target
            .clone()
            .unwrap_or_default();
        if !remote_storage_target.enabled {
            return Vec::new();
        }

        registered_remote_storage_target_connector_ids()
            .into_iter()
            .filter(|connector_id| remote_storage_target.supports_connector(connector_id))
            .filter(|connector_id| remote_storage_target_connector_descriptor(connector_id).is_ok())
            .collect()
    }

    fn ensure_features(&self, context: &str, required: &[(&'static str, bool)]) -> Result<()> {
        let missing = required
            .iter()
            .filter_map(|(name, supported)| (!*supported).then_some(*name))
            .collect::<Vec<_>>();
        if missing.is_empty() {
            return Ok(());
        }

        Err(crate::errors::storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!(
                "{context}: remote internal storage protocol is missing required feature(s): {}; remote declared features: {:?}",
                missing.join(", "),
                self.capabilities.features
            ),
        ))
    }

    fn base_policy_required_features(&self) -> Vec<(&'static str, bool)> {
        vec![
            ("object_get", self.capabilities.features.object_get),
            ("object_head", self.capabilities.features.object_head),
            ("object_put", self.capabilities.features.object_put),
            ("object_delete", self.capabilities.features.object_delete),
            ("metadata", self.capabilities.features.metadata),
            ("range_get", self.capabilities.features.range_get),
            (
                "accept_ranges_header",
                self.capabilities.features.accept_ranges_header,
            ),
            ("list", self.capabilities.features.list),
            ("compose", self.capabilities.features.compose),
        ]
    }

    fn ensure_browser_presigned_cors(
        &self,
        context: &str,
        required_allowed_headers: &[&str],
        required_exposed_headers: &[&str],
    ) -> Result<()> {
        self.ensure_features(
            context,
            &[(
                "browser_presigned_cors",
                self.capabilities.features.browser_presigned_cors,
            )],
        )?;

        let missing_allowed = required_allowed_headers
            .iter()
            .filter(|header| {
                !contains_header(&self.capabilities.browser_cors.allowed_headers, header)
            })
            .copied()
            .collect::<Vec<_>>();
        let missing_exposed = required_exposed_headers
            .iter()
            .filter(|header| {
                !contains_header(&self.capabilities.browser_cors.exposed_headers, header)
            })
            .copied()
            .collect::<Vec<_>>();

        if missing_allowed.is_empty() && missing_exposed.is_empty() {
            return Ok(());
        }

        let mut details = Vec::new();
        if !missing_allowed.is_empty() {
            details.push(format!(
                "allowed_headers missing {}",
                missing_allowed.join(", ")
            ));
        }
        if !missing_exposed.is_empty() {
            details.push(format!(
                "exposed_headers missing {}",
                missing_exposed.join(", ")
            ));
        }

        Err(crate::errors::storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!(
                "{context}: remote internal storage browser CORS contract is incomplete: {}; allowed_headers={:?}; exposed_headers={:?}",
                details.join("; "),
                self.capabilities.browser_cors.allowed_headers,
                self.capabilities.browser_cors.exposed_headers
            ),
        ))
    }

    fn ensure_presigned_cors_for_strategies(
        &self,
        download_strategy: RemoteDownloadStrategy,
        upload_strategy: RemoteUploadStrategy,
        download_context: &str,
        upload_context: &str,
    ) -> Result<()> {
        if download_strategy == RemoteDownloadStrategy::Presigned {
            self.ensure_browser_presigned_cors(
                download_context,
                &["range"],
                &["Accept-Ranges", "Content-Range", "Content-Length"],
            )?;
        }

        if upload_strategy == RemoteUploadStrategy::Presigned {
            self.ensure_browser_presigned_cors(upload_context, &["content-type"], &["ETag"])?;
        }

        Ok(())
    }
}

fn contains_header(headers: &[String], expected: &str) -> bool {
    headers
        .iter()
        .any(|header| header.eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCAL_CONNECTOR_ID: &str = "asterdrive.remote-target.local";
    const S3_CONNECTOR_ID: &str = "asterdrive.remote-target.s3";

    #[test]
    fn resolver_treats_missing_capabilities_conservatively() {
        for raw in ["", "{}"] {
            let resolver = RemoteCapabilityResolver::from_last_capabilities(42, raw);
            assert!(
                resolver
                    .remote_storage_target_connector_descriptors()
                    .is_empty()
            );
            let error = resolver
                .ensure_remote_storage_target_connector_supported(LOCAL_CONNECTOR_ID)
                .unwrap_err();
            assert_eq!(
                error.api_error_code_override(),
                Some(ApiErrorCode::ManagedIngressDriverUnsupported)
            );
        }
    }

    #[test]
    fn resolver_rejects_v5_protocol_without_translating_legacy_driver_names() {
        let raw = serde_json::json!({
            "protocol_version": "v5",
            "min_supported_protocol_version": "v5",
            "managed_ingress": {
                "enabled": true,
                "driver_types": ["s3", "plugin.example.archive", "local", "s3"]
            }
        })
        .to_string();
        let resolver = RemoteCapabilityResolver::from_last_capabilities(42, &raw);
        assert!(
            resolver
                .remote_storage_target_connector_descriptors()
                .is_empty()
        );
        let error = resolver
            .ensure_protocol_compatible("remote target connector discovery")
            .unwrap_err();
        assert!(error.message().contains("local supports v6-v6"));
        assert!(!resolver.supports_remote_storage_target_connector(LOCAL_CONNECTOR_ID));
        assert!(!resolver.supports_remote_storage_target_connector("plugin.example.archive"));
    }

    #[test]
    fn resolver_preserves_v6_connector_ids_and_filters_only_by_local_registry() {
        let capabilities = RemoteStorageCapabilities::current()
            .with_remote_storage_target_connector_ids(vec![
                "plugin.example.archive".to_string(),
                S3_CONNECTOR_ID.to_string(),
            ]);
        let resolver = RemoteCapabilityResolver::from_capabilities(42, capabilities);
        assert_eq!(
            resolver
                .remote_storage_target_connector_descriptors()
                .iter()
                .map(|descriptor| descriptor.connector_id.as_str())
                .collect::<Vec<_>>(),
            vec![S3_CONNECTOR_ID]
        );
        assert!(!resolver.supports_remote_storage_target_connector("plugin.example.archive"));
        assert!(resolver.supports_remote_storage_target_connector(S3_CONNECTOR_ID));
    }

    #[test]
    fn resolver_rejects_undeclared_connector() {
        let capabilities = RemoteStorageCapabilities::current()
            .with_remote_storage_target_connector_ids(vec![LOCAL_CONNECTOR_ID.to_string()]);
        let error = RemoteCapabilityResolver::from_capabilities(42, capabilities)
            .ensure_remote_storage_target_connector_supported(S3_CONNECTOR_ID)
            .unwrap_err();
        assert_eq!(
            error.api_error_code_override(),
            Some(ApiErrorCode::ManagedIngressDriverUnsupported)
        );
        assert!(error.message().contains(S3_CONNECTOR_ID));
    }

    #[test]
    fn resolver_blocks_presigned_download_without_browser_range_cors() {
        let mut capabilities = RemoteStorageCapabilities::current();
        capabilities.browser_cors.allowed_headers = vec!["content-type".to_string()];
        capabilities.browser_cors.exposed_headers = vec!["Accept-Ranges".to_string()];
        let error = RemoteCapabilityResolver::from_capabilities(7, capabilities)
            .ensure_remote_policy_config_supported(
                42,
                RemoteDownloadStrategy::Presigned,
                RemoteUploadStrategy::RelayStream,
            )
            .unwrap_err();
        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Misconfigured)
        );
        assert!(error.message().contains("range"));
    }

    #[test]
    fn resolver_accepts_current_relay_policy_contract() {
        RemoteCapabilityResolver::from_capabilities(42, RemoteStorageCapabilities::current())
            .ensure_remote_policy_config_supported(
                7,
                RemoteDownloadStrategy::RelayStream,
                RemoteUploadStrategy::RelayStream,
            )
            .unwrap();
    }
}
