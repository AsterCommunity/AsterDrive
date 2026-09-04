use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{Result, validation_error_with_code};
use crate::services::remote::storage_target::remote_storage_target_descriptor_from_connector;
use crate::storage::remote_protocol::{RemoteStorageCapabilities, RemoteStorageTargetCapabilities};
use aster_drive_model::entities::managed_follower;
use aster_drive_model::types::{RemoteDownloadStrategy, RemoteUploadStrategy};
use aster_drive_storage::StorageErrorKind;
use aster_drive_storage::{ConnectorId, StorageConnectorDescriptor};

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
        registry: &crate::storage::connectors::StorageConnectorRegistry,
    ) -> Vec<StorageConnectorDescriptor> {
        let capabilities = self.effective_remote_storage_target_capabilities();
        registry
            .remote_target_connectors()
            .into_iter()
            .filter_map(|connector| remote_storage_target_descriptor_from_connector(connector).ok())
            .filter(|descriptor| {
                capabilities.supports_connector_id(descriptor.connector_id.as_str())
            })
            .collect()
    }

    pub fn supports_remote_storage_target_connector(
        &self,
        registry: &crate::storage::connectors::StorageConnectorRegistry,
        connector_id: &ConnectorId,
    ) -> bool {
        self.effective_remote_storage_target_capabilities()
            .supports_connector_id(connector_id.as_str())
            && registry
                .require_remote_target_connector(connector_id)
                .is_ok()
    }

    pub fn ensure_remote_storage_target_connector_supported(
        &self,
        registry: &crate::storage::connectors::StorageConnectorRegistry,
        connector_id: &ConnectorId,
    ) -> Result<()> {
        if self.supports_remote_storage_target_connector(registry, connector_id) {
            return Ok(());
        }

        Err(validation_error_with_code(
            ApiErrorCode::RemoteStorageTargetConnectorUnsupported,
            format!(
                "remote node #{} does not declare remote storage target support for connector '{}'",
                self.remote_node_id, connector_id
            ),
        ))
    }

    pub fn requires_direct_transport_for_presigned(
        download_strategy: RemoteDownloadStrategy,
        upload_strategy: RemoteUploadStrategy,
    ) -> bool {
        download_strategy == RemoteDownloadStrategy::Presigned
            || upload_strategy == RemoteUploadStrategy::Presigned
    }

    fn effective_remote_storage_target_capabilities(&self) -> RemoteStorageTargetCapabilities {
        if let Some(capabilities) = &self.capabilities.remote_storage_target {
            return capabilities.clone();
        }

        RemoteStorageTargetCapabilities::default()
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

    fn registry() -> crate::storage::connectors::StorageConnectorRegistry {
        crate::storage::connectors::builtin_storage_connector_registry().unwrap()
    }

    fn connector_id(value: &str) -> ConnectorId {
        ConnectorId::declared(value)
    }

    #[test]
    fn resolver_treats_missing_target_capabilities_conservatively() {
        for raw in ["", "{}"] {
            let resolver = RemoteCapabilityResolver::from_last_capabilities(42, raw);
            assert!(
                resolver
                    .remote_storage_target_connector_descriptors(&registry())
                    .is_empty()
            );
            let error = resolver
                .ensure_remote_storage_target_connector_supported(
                    &registry(),
                    &connector_id("asterdrive.storage.local"),
                )
                .unwrap_err();
            assert_eq!(
                error.api_error_code_override(),
                Some(ApiErrorCode::RemoteStorageTargetConnectorUnsupported)
            );
        }
    }

    #[test]
    fn resolver_filters_unknown_connectors_and_preserves_registry_order() {
        let last_capabilities = serde_json::json!({
            "protocol_version": "v6",
            "min_supported_protocol_version": "v6",
            "remote_storage_target": {
                "enabled": true,
                "connector_ids": [
                    "asterdrive.storage.s3",
                    "plugin.example.archive",
                    "asterdrive.storage.local",
                    "asterdrive.storage.s3"
                ]
            }
        })
        .to_string();
        let descriptors = RemoteCapabilityResolver::from_last_capabilities(42, &last_capabilities)
            .remote_storage_target_connector_descriptors(&registry());
        assert_eq!(
            descriptors
                .iter()
                .map(|descriptor| descriptor.connector_id.as_str())
                .collect::<Vec<_>>(),
            vec!["asterdrive.storage.local", "asterdrive.storage.s3"]
        );
    }

    #[test]
    fn resolver_rejects_connector_missing_from_cached_capabilities() {
        let last_capabilities = serde_json::json!({
            "protocol_version": "v6",
            "min_supported_protocol_version": "v6",
            "remote_storage_target": {
                "enabled": true,
                "connector_ids": ["asterdrive.storage.local"]
            }
        })
        .to_string();
        let error = RemoteCapabilityResolver::from_last_capabilities(42, &last_capabilities)
            .ensure_remote_storage_target_connector_supported(
                &registry(),
                &connector_id("asterdrive.storage.s3"),
            )
            .unwrap_err();
        assert_eq!(
            error.api_error_code_override(),
            Some(ApiErrorCode::RemoteStorageTargetConnectorUnsupported)
        );
        assert!(error.message().contains("asterdrive.storage.s3"));
    }

    #[test]
    fn resolver_honors_disabled_and_unknown_only_target_capabilities() {
        for target in [
            serde_json::json!({
                "enabled": false,
                "connector_ids": ["asterdrive.storage.local"]
            }),
            serde_json::json!({
                "enabled": true,
                "connector_ids": ["plugin.example.archive"]
            }),
        ] {
            let raw = serde_json::json!({
                "protocol_version": "v6",
                "min_supported_protocol_version": "v6",
                "remote_storage_target": target
            })
            .to_string();
            let resolver = RemoteCapabilityResolver::from_last_capabilities(42, &raw);
            assert!(
                resolver
                    .remote_storage_target_connector_descriptors(&registry())
                    .is_empty()
            );
            assert!(!resolver.supports_remote_storage_target_connector(
                &registry(),
                &connector_id("asterdrive.storage.local",)
            ));
        }
    }

    #[test]
    fn resolver_accepts_current_v6_protocol_and_exposes_connector_descriptors() {
        let capabilities = RemoteStorageCapabilities::current()
            .with_remote_storage_target_connector_ids(vec![
                "asterdrive.storage.local".to_string(),
                "asterdrive.storage.s3".to_string(),
            ]);
        let resolver = RemoteCapabilityResolver::from_capabilities(42, capabilities);
        resolver
            .ensure_protocol_compatible("current v6 remote node")
            .expect("current capabilities should be protocol-compatible");
        resolver
            .ensure_remote_policy_config_supported(
                7,
                RemoteDownloadStrategy::RelayStream,
                RemoteUploadStrategy::RelayStream,
            )
            .expect("current capabilities should support relay remote policy config");
        assert_eq!(
            resolver
                .remote_storage_target_connector_descriptors(&registry())
                .iter()
                .map(|descriptor| descriptor.connector_id.as_str())
                .collect::<Vec<_>>(),
            vec!["asterdrive.storage.local", "asterdrive.storage.s3"]
        );
    }

    #[test]
    fn resolver_blocks_remote_presigned_download_without_browser_range_cors() {
        let mut capabilities = RemoteStorageCapabilities::current();
        capabilities.browser_cors.allowed_headers = vec!["content-type".to_string()];
        capabilities.browser_cors.exposed_headers =
            vec!["Accept-Ranges".to_string(), "Content-Length".to_string()];
        let resolver = RemoteCapabilityResolver::from_capabilities(7, capabilities);
        let error = resolver
            .ensure_remote_policy_config_supported(
                42,
                RemoteDownloadStrategy::Presigned,
                RemoteUploadStrategy::RelayStream,
            )
            .expect_err("missing Range/CORS headers should block presigned remote download");

        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Misconfigured)
        );
        assert!(
            error
                .message()
                .contains("browser CORS contract is incomplete"),
            "unexpected error message: {}",
            error.message()
        );
        assert!(error.message().contains("allowed_headers missing range"));
        assert!(
            error
                .message()
                .contains("exposed_headers missing Content-Range")
        );
    }

    #[test]
    fn resolver_blocks_remote_presigned_upload_without_browser_content_type_cors() {
        let mut capabilities = RemoteStorageCapabilities::current();
        capabilities.browser_cors.allowed_headers = vec!["range".to_string()];
        capabilities.browser_cors.exposed_headers = vec!["Accept-Ranges".to_string()];
        let resolver = RemoteCapabilityResolver::from_capabilities(7, capabilities);
        let error = resolver
            .ensure_remote_policy_config_supported(
                42,
                RemoteDownloadStrategy::RelayStream,
                RemoteUploadStrategy::Presigned,
            )
            .expect_err("missing content-type/ETag CORS headers should block presigned upload");

        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Misconfigured)
        );
        assert!(
            error
                .message()
                .contains("allowed_headers missing content-type")
        );
        assert!(error.message().contains("exposed_headers missing ETag"));
    }

    #[test]
    fn resolver_blocks_remote_policy_when_required_base_feature_is_missing() {
        let mut capabilities = RemoteStorageCapabilities::current();
        capabilities.features.metadata = false;
        let resolver = RemoteCapabilityResolver::from_capabilities(7, capabilities);

        let error = resolver
            .ensure_remote_policy_config_supported(
                42,
                RemoteDownloadStrategy::RelayStream,
                RemoteUploadStrategy::RelayStream,
            )
            .expect_err("missing metadata feature should block remote policy use");

        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Misconfigured)
        );
        assert!(error.message().contains("metadata"));
        assert!(
            error
                .message()
                .contains("remote storage policy #42 on remote node #7")
        );
    }

    #[test]
    fn resolver_blocks_incompatible_protocol_for_policy_options() {
        let capabilities = RemoteStorageCapabilities {
            protocol_version: "v1".to_string(),
            min_supported_protocol_version: "v1".to_string(),
            ..RemoteStorageCapabilities::current()
        };
        let resolver = RemoteCapabilityResolver::from_capabilities(7, capabilities);

        let error = resolver
            .ensure_remote_policy_config_supported(
                42,
                RemoteDownloadStrategy::RelayStream,
                RemoteUploadStrategy::RelayStream,
            )
            .expect_err("incompatible protocol should block remote policy use");

        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Misconfigured)
        );
        assert!(error.message().contains("protocol incompatible"));
        assert!(error.message().contains("remote node #7"));
    }

    #[test]
    fn resolver_binding_validation_reports_presigned_policy_context() {
        let mut capabilities = RemoteStorageCapabilities::current();
        capabilities.browser_cors.allowed_headers = vec!["content-type".to_string()];
        capabilities.browser_cors.exposed_headers =
            vec!["Accept-Ranges".to_string(), "Content-Length".to_string()];
        let resolver = RemoteCapabilityResolver::from_capabilities(7, capabilities);
        let requirements = [
            (
                41,
                RemoteDownloadStrategy::RelayStream,
                RemoteUploadStrategy::RelayStream,
            ),
            (
                42,
                RemoteDownloadStrategy::Presigned,
                RemoteUploadStrategy::RelayStream,
            ),
        ];

        let error = resolver
            .ensure_binding_policy_configs_supported("edge-a", &requirements)
            .expect_err("binding validation should include the failing policy context");

        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Misconfigured)
        );
        assert!(
            error
                .message()
                .contains("remote node #7 ('edge-a') binding reload; policy #42")
        );
        assert!(error.message().contains("presigned download"));
    }

    #[test]
    fn resolver_requires_direct_transport_for_any_presigned_strategy() {
        assert!(
            !RemoteCapabilityResolver::requires_direct_transport_for_presigned(
                RemoteDownloadStrategy::RelayStream,
                RemoteUploadStrategy::RelayStream,
            )
        );
        assert!(
            RemoteCapabilityResolver::requires_direct_transport_for_presigned(
                RemoteDownloadStrategy::Presigned,
                RemoteUploadStrategy::RelayStream,
            )
        );
        assert!(
            RemoteCapabilityResolver::requires_direct_transport_for_presigned(
                RemoteDownloadStrategy::RelayStream,
                RemoteUploadStrategy::Presigned,
            )
        );
    }
}
