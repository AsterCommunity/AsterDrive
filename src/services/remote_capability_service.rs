use crate::api::api_error_code::ApiErrorCode;
use crate::entities::managed_follower;
use crate::errors::{Result, validation_error_with_code};
use crate::services::remote_storage_target_service::{
    RemoteStorageTargetDriverDescriptor, registered_remote_storage_target_driver_types,
    remote_storage_target_driver_descriptor,
};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::remote_protocol::{RemoteStorageCapabilities, RemoteStorageTargetCapabilities};
use crate::types::{
    DriverType, RemoteDownloadStrategy, RemoteUploadStrategy, StoragePolicyOptions,
};

const LEGACY_MANAGED_INGRESS_IMPLICIT_PROTOCOL_VERSION: u16 = 4;

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

    pub fn ensure_remote_policy_options_supported(
        &self,
        policy_id: i64,
        options: &StoragePolicyOptions,
    ) -> Result<()> {
        let context = format!(
            "remote storage policy #{policy_id} on remote node #{}",
            self.remote_node_id
        );
        self.ensure_protocol_compatible(&context)?;
        self.ensure_features(&context, &self.base_policy_required_features())?;
        self.ensure_presigned_cors_for_options(options, &context, &context)?;

        Ok(())
    }

    pub fn ensure_binding_policy_options_supported(
        &self,
        remote_node_name: &str,
        policy_requirements: &[(i64, &StoragePolicyOptions)],
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
        for (policy_id, options) in policy_requirements {
            let download_context =
                format!("{context}; policy #{policy_id} requires remote presigned download");
            let upload_context =
                format!("{context}; policy #{policy_id} requires remote presigned upload");
            self.ensure_presigned_cors_for_options(options, &download_context, &upload_context)?;
        }

        Ok(())
    }

    pub fn managed_ingress_driver_descriptors(&self) -> Vec<RemoteStorageTargetDriverDescriptor> {
        self.supported_registered_managed_ingress_driver_types()
            .into_iter()
            .filter_map(|driver_type| remote_storage_target_driver_descriptor(driver_type).ok())
            .collect()
    }

    pub fn ensure_managed_ingress_driver_supported(&self, driver_type: DriverType) -> Result<()> {
        if self.supports_managed_ingress_driver(driver_type) {
            return Ok(());
        }

        Err(validation_error_with_code(
            ApiErrorCode::ManagedIngressDriverUnsupported,
            format!(
                "remote node #{} does not declare remote storage target support for the {} driver",
                self.remote_node_id,
                driver_type.as_str()
            ),
        ))
    }

    pub fn supports_managed_ingress_driver(&self, driver_type: DriverType) -> bool {
        self.effective_managed_ingress()
            .supports_known_driver(driver_type)
            && remote_storage_target_driver_descriptor(driver_type).is_ok()
    }

    pub fn requires_direct_transport_for_presigned(options: &StoragePolicyOptions) -> bool {
        options.effective_remote_download_strategy() == RemoteDownloadStrategy::Presigned
            || options.effective_remote_upload_strategy() == RemoteUploadStrategy::Presigned
    }

    fn effective_managed_ingress(&self) -> RemoteStorageTargetCapabilities {
        if let Some(capabilities) = &self.capabilities.managed_ingress {
            return capabilities.clone();
        }

        if parse_protocol_version(&self.capabilities.protocol_version)
            == Some(LEGACY_MANAGED_INGRESS_IMPLICIT_PROTOCOL_VERSION)
        {
            return RemoteStorageTargetCapabilities::from_known_driver_types(vec![
                DriverType::Local,
                DriverType::S3,
            ]);
        }

        RemoteStorageTargetCapabilities::default()
    }

    fn supported_registered_managed_ingress_driver_types(&self) -> Vec<DriverType> {
        let managed_ingress = self.effective_managed_ingress();
        if !managed_ingress.enabled {
            return Vec::new();
        }

        registered_remote_storage_target_driver_types()
            .into_iter()
            .filter(|driver_type| managed_ingress.supports_known_driver(*driver_type))
            .filter(|driver_type| remote_storage_target_driver_descriptor(*driver_type).is_ok())
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

        Err(storage_driver_error(
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

        Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!(
                "{context}: remote internal storage browser CORS contract is incomplete: {}; allowed_headers={:?}; exposed_headers={:?}",
                details.join("; "),
                self.capabilities.browser_cors.allowed_headers,
                self.capabilities.browser_cors.exposed_headers
            ),
        ))
    }

    fn ensure_presigned_cors_for_options(
        &self,
        options: &StoragePolicyOptions,
        download_context: &str,
        upload_context: &str,
    ) -> Result<()> {
        if options.effective_remote_download_strategy() == RemoteDownloadStrategy::Presigned {
            self.ensure_browser_presigned_cors(
                download_context,
                &["range"],
                &["Accept-Ranges", "Content-Range", "Content-Length"],
            )?;
        }

        if options.effective_remote_upload_strategy() == RemoteUploadStrategy::Presigned {
            self.ensure_browser_presigned_cors(upload_context, &["content-type"], &["ETag"])?;
        }

        Ok(())
    }
}

fn parse_protocol_version(value: &str) -> Option<u16> {
    value
        .trim()
        .strip_prefix('v')
        .or_else(|| value.trim().strip_prefix('V'))
        .unwrap_or_else(|| value.trim())
        .parse::<u16>()
        .ok()
}

fn contains_header(headers: &[String], expected: &str) -> bool {
    headers
        .iter()
        .any(|header| header.eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolver_treats_empty_cached_capabilities_conservatively() {
        let resolver = RemoteCapabilityResolver::from_last_capabilities(42, "");

        assert!(resolver.managed_ingress_driver_descriptors().is_empty());
        let error = resolver
            .ensure_managed_ingress_driver_supported(DriverType::Local)
            .unwrap_err();
        assert_eq!(
            error.api_error_code_override(),
            Some(ApiErrorCode::ManagedIngressDriverUnsupported)
        );
    }

    #[test]
    fn resolver_treats_unknown_cached_capabilities_conservatively() {
        let resolver = RemoteCapabilityResolver::from_last_capabilities(42, "{}");

        assert!(resolver.managed_ingress_driver_descriptors().is_empty());
        let error = resolver
            .ensure_managed_ingress_driver_supported(DriverType::Local)
            .unwrap_err();
        assert_eq!(
            error.api_error_code_override(),
            Some(ApiErrorCode::ManagedIngressDriverUnsupported)
        );
    }

    #[test]
    fn resolver_filters_unknown_future_managed_ingress_driver_ids() {
        let last_capabilities = serde_json::json!({
            "protocol_version": "v5",
            "min_supported_protocol_version": "v4",
            "managed_ingress": {
                "enabled": true,
                "driver_types": ["local", "plugin.example.archive", "s3"]
            }
        })
        .to_string();

        let descriptors = RemoteCapabilityResolver::from_last_capabilities(42, &last_capabilities)
            .managed_ingress_driver_descriptors();

        assert_eq!(
            descriptors
                .iter()
                .map(|descriptor| descriptor.driver_type)
                .collect::<Vec<_>>(),
            vec![DriverType::Local, DriverType::S3]
        );
    }

    #[test]
    fn resolver_uses_registered_order_and_deduplicates_descriptors() {
        let last_capabilities = serde_json::json!({
            "protocol_version": "v5",
            "min_supported_protocol_version": "v4",
            "managed_ingress": {
                "enabled": true,
                "driver_types": ["s3", "plugin.example.archive", "local", "s3"]
            }
        })
        .to_string();

        let descriptors = RemoteCapabilityResolver::from_last_capabilities(42, &last_capabilities)
            .managed_ingress_driver_descriptors();

        assert_eq!(
            descriptors
                .iter()
                .map(|descriptor| descriptor.driver_type)
                .collect::<Vec<_>>(),
            vec![DriverType::Local, DriverType::S3]
        );
    }

    #[test]
    fn resolver_rejects_managed_ingress_driver_missing_from_cached_capabilities() {
        let last_capabilities = serde_json::json!({
            "protocol_version": "v5",
            "min_supported_protocol_version": "v4",
            "managed_ingress": {
                "enabled": true,
                "driver_types": ["local"]
            }
        })
        .to_string();

        let error = RemoteCapabilityResolver::from_last_capabilities(42, &last_capabilities)
            .ensure_managed_ingress_driver_supported(DriverType::S3)
            .unwrap_err();

        assert_eq!(
            error.api_error_code_override(),
            Some(ApiErrorCode::ManagedIngressDriverUnsupported)
        );
        assert!(error.message().contains(
            "remote node #42 does not declare remote storage target support for the s3 driver"
        ));
    }

    #[test]
    fn resolver_keeps_v4_fallback_for_missing_managed_ingress() {
        let last_capabilities = serde_json::json!({
            "protocol_version": "v4",
            "min_supported_protocol_version": "v4"
        })
        .to_string();

        let descriptors = RemoteCapabilityResolver::from_last_capabilities(42, &last_capabilities)
            .managed_ingress_driver_descriptors();

        assert_eq!(
            descriptors
                .iter()
                .map(|descriptor| descriptor.driver_type)
                .collect::<Vec<_>>(),
            vec![DriverType::Local, DriverType::S3]
        );
    }

    #[test]
    fn resolver_does_not_apply_v4_fallback_to_v5_missing_managed_ingress() {
        let last_capabilities = serde_json::json!({
            "protocol_version": "v5",
            "min_supported_protocol_version": "v4"
        })
        .to_string();

        let resolver = RemoteCapabilityResolver::from_last_capabilities(42, &last_capabilities);

        assert!(resolver.managed_ingress_driver_descriptors().is_empty());
        assert!(!resolver.supports_managed_ingress_driver(DriverType::Local));
        assert!(!resolver.supports_managed_ingress_driver(DriverType::S3));
    }

    #[test]
    fn resolver_honors_explicit_disabled_managed_ingress_on_v4() {
        let last_capabilities = serde_json::json!({
            "protocol_version": "v4",
            "min_supported_protocol_version": "v4",
            "managed_ingress": {
                "enabled": false,
                "driver_types": ["local", "s3"]
            }
        })
        .to_string();

        let resolver = RemoteCapabilityResolver::from_last_capabilities(42, &last_capabilities);

        assert!(resolver.managed_ingress_driver_descriptors().is_empty());
        assert!(!resolver.supports_managed_ingress_driver(DriverType::Local));
    }

    #[test]
    fn resolver_honors_unknown_only_managed_ingress_driver_ids() {
        let last_capabilities = serde_json::json!({
            "protocol_version": "v5",
            "min_supported_protocol_version": "v4",
            "managed_ingress": {
                "enabled": true,
                "driver_types": ["plugin.example.archive"]
            }
        })
        .to_string();

        let resolver = RemoteCapabilityResolver::from_last_capabilities(42, &last_capabilities);

        assert!(resolver.managed_ingress_driver_descriptors().is_empty());
        assert!(!resolver.supports_managed_ingress_driver(DriverType::Local));
    }

    #[test]
    fn resolver_accepts_current_v5_protocol_and_default_policy_options() {
        let capabilities = RemoteStorageCapabilities::current();
        let resolver = RemoteCapabilityResolver::from_capabilities(42, capabilities);

        resolver
            .ensure_protocol_compatible("current v5 remote node")
            .expect("current capabilities should be protocol-compatible");
        resolver
            .ensure_remote_policy_options_supported(7, &StoragePolicyOptions::default())
            .expect("current capabilities should support default remote policy options");
    }

    #[test]
    fn resolver_exposes_current_v5_managed_ingress_driver_descriptors() {
        let capabilities = RemoteStorageCapabilities::current()
            .with_remote_storage_target_driver_types(vec![DriverType::Local, DriverType::S3]);
        let resolver = RemoteCapabilityResolver::from_capabilities(42, capabilities);

        assert_eq!(
            resolver
                .managed_ingress_driver_descriptors()
                .iter()
                .map(|descriptor| descriptor.driver_type)
                .collect::<Vec<_>>(),
            vec![DriverType::Local, DriverType::S3]
        );
    }

    #[test]
    fn resolver_blocks_remote_presigned_download_without_browser_range_cors() {
        let mut capabilities = RemoteStorageCapabilities::current();
        capabilities.browser_cors.allowed_headers = vec!["content-type".to_string()];
        capabilities.browser_cors.exposed_headers =
            vec!["Accept-Ranges".to_string(), "Content-Length".to_string()];
        let resolver = RemoteCapabilityResolver::from_capabilities(7, capabilities);
        let options = StoragePolicyOptions {
            remote_download_strategy: Some(RemoteDownloadStrategy::Presigned),
            ..Default::default()
        };

        let error = resolver
            .ensure_remote_policy_options_supported(42, &options)
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
        let options = StoragePolicyOptions {
            remote_upload_strategy: Some(RemoteUploadStrategy::Presigned),
            ..Default::default()
        };

        let error = resolver
            .ensure_remote_policy_options_supported(42, &options)
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
            .ensure_remote_policy_options_supported(42, &StoragePolicyOptions::default())
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
            .ensure_remote_policy_options_supported(42, &StoragePolicyOptions::default())
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
        let relay = StoragePolicyOptions::default();
        let presigned_download = StoragePolicyOptions {
            remote_download_strategy: Some(RemoteDownloadStrategy::Presigned),
            ..Default::default()
        };
        let requirements = [(41, &relay), (42, &presigned_download)];

        let error = resolver
            .ensure_binding_policy_options_supported("edge-a", &requirements)
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
                &StoragePolicyOptions::default()
            )
        );
        assert!(
            RemoteCapabilityResolver::requires_direct_transport_for_presigned(
                &StoragePolicyOptions {
                    remote_download_strategy: Some(RemoteDownloadStrategy::Presigned),
                    ..Default::default()
                }
            )
        );
        assert!(
            RemoteCapabilityResolver::requires_direct_transport_for_presigned(
                &StoragePolicyOptions {
                    remote_upload_strategy: Some(RemoteUploadStrategy::Presigned),
                    ..Default::default()
                }
            )
        );
    }
}
