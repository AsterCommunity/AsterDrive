use async_trait::async_trait;
use std::time::Duration;

use crate::errors::{AsterError, Result};
use crate::storage::drivers::qiniu::{
    QiniuDriver, QiniuDriverConfig, QiniuRegionEndpoints, QiniuStaticCredentials,
};
use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::{ObjectStorageDownloadStrategy, ObjectStorageUploadStrategy};
use aster_drive_storage::connector_descriptor::{
    ObjectStorageConnectorDescriptorInput, StorageConnectorBadgeRgb,
    StorageConnectorDeploymentScope, StorageConnectorDescriptor, StorageConnectorFieldDisplayInput,
    StorageConnectorFieldKind, StorageConnectorFieldScope, StorageConnectorSelectOptionInput,
    StorageConnectorUiDescriptorInput, object_storage_connector_descriptor,
    storage_connector_field, storage_connector_field_with_display, storage_connector_select_field,
};
use aster_drive_storage::{
    StorageConnectorConfigSchema, StorageConnectorFieldDefaultValue, StorageDriver,
};

use super::common::{StorageTransferDirection, transfer_strategy_field};
use super::{StorageConnector, StorageConnectorCredentialInput, StorageConnectorUploadTransport};

mod localization;

pub struct QiniuConnector;

aster_drive_storage::storage_connector_schema! {
    pub struct QiniuConnectorConfigV1 {
        config {
            pub bucket: String => {
                let mut field = storage_connector_field(
                    "bucket", StorageConnectorFieldScope::ConnectorConfig,
                    StorageConnectorFieldKind::Text, true, false,
                );
                field.required_message_key = Some("qiniu_bucket_required".to_string());
                field
            },
            pub region: String => qiniu_region_field(),
            pub download_domain: String => storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                name: "download_domain", scope: StorageConnectorFieldScope::ConnectorConfig,
                kind: StorageConnectorFieldKind::Text, required: true, secret: false,
                label_key: "qiniu_download_domain", placeholder: Some("https://download.example.com"),
                help_key: Some("qiniu_download_domain_desc"), required_message_key: None,
                invalid_protocol_message_key: Some("qiniu_download_domain_protocol_error"),
                allowed_endpoint_protocols: vec!["http:", "https:"],
                allow_endpoint_without_protocol: false, trim_on_blur: true,
            }),
            pub object_prefix: String => {
                let mut field = storage_connector_field(
                    "object_prefix", StorageConnectorFieldScope::ConnectorConfig,
                    StorageConnectorFieldKind::Text, false, false,
                );
                field.default_value = Some(StorageConnectorFieldDefaultValue::String(String::new()));
                field.default_mode = aster_drive_storage::StorageConnectorFieldDefaultMode::MissingOrEmptyText;
                field
            },
            pub object_storage_upload_strategy: ObjectStorageUploadStrategy => transfer_strategy_field(
                "object_storage_upload_strategy", StorageTransferDirection::Upload,
            ),
            pub object_storage_download_strategy: ObjectStorageDownloadStrategy => transfer_strategy_field(
                "object_storage_download_strategy", StorageTransferDirection::Download,
            ),
        }
        credentials static QiniuStaticCredentialsV1 {
            pub qiniu_access_key: String => storage_connector_field(
                "qiniu_access_key", StorageConnectorFieldScope::StaticCredential,
                StorageConnectorFieldKind::Text, true, false,
            ),
            pub qiniu_secret_key: String => storage_connector_field(
                "qiniu_secret_key", StorageConnectorFieldScope::StaticCredential,
                StorageConnectorFieldKind::Secret, true, true,
            ),
        }
    }
}

fn qiniu_region_field() -> aster_drive_storage::StorageConnectorFieldDescriptor {
    let mut field = storage_connector_select_field(
        "region",
        StorageConnectorFieldScope::ConnectorConfig,
        true,
        vec![
            StorageConnectorSelectOptionInput {
                value: "z0",
                label_key: "qiniu_region_z0",
                description_key: Some("qiniu_region_z0_desc"),
            },
            StorageConnectorSelectOptionInput {
                value: "z1",
                label_key: "qiniu_region_z1",
                description_key: Some("qiniu_region_z1_desc"),
            },
            StorageConnectorSelectOptionInput {
                value: "z2",
                label_key: "qiniu_region_z2",
                description_key: Some("qiniu_region_z2_desc"),
            },
        ],
    );
    field.default_value = Some(StorageConnectorFieldDefaultValue::String("z0".to_string()));
    field
}

impl QiniuConnector {
    pub const ID: &'static str = "asterdrive.storage.qiniu";

    fn decode_config(policy: &storage_policy::Model) -> Result<QiniuConnectorConfigV1> {
        super::common::decode_typed_policy_config(policy, Self::ID, 1).map(|(config, _)| config)
    }

    fn endpoints(region: &str) -> Result<QiniuRegionEndpoints> {
        match region {
            "z0" => Ok(QiniuRegionEndpoints {
                upload: "https://up-z0.qiniup.com".to_string(),
                manage: "https://rs-z0.qiniuapi.com".to_string(),
                list: "https://rsf-z0.qiniuapi.com".to_string(),
            }),
            "z1" => Ok(QiniuRegionEndpoints {
                upload: "https://up-z1.qiniup.com".to_string(),
                manage: "https://rs-z1.qiniuapi.com".to_string(),
                list: "https://rsf-z1.qiniuapi.com".to_string(),
            }),
            "z2" => Ok(QiniuRegionEndpoints {
                upload: "https://up-z2.qiniup.com".to_string(),
                manage: "https://rs-z2.qiniuapi.com".to_string(),
                list: "https://rsf-z2.qiniuapi.com".to_string(),
            }),
            _ => Err(AsterError::validation_error("unsupported Qiniu region")),
        }
    }

    fn driver_config(config: QiniuConnectorConfigV1) -> Result<QiniuDriverConfig> {
        Ok(QiniuDriverConfig {
            endpoints: Self::endpoints(&config.region)?,
            bucket: config.bucket,
            region: config.region,
            download_domain: config.download_domain,
            object_prefix: config.object_prefix.trim_matches('/').to_string(),
            connect_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_secs(30),
            operation_timeout: Duration::from_secs(3_600),
        })
    }

    fn driver_credentials(credentials: QiniuStaticCredentialsV1) -> QiniuStaticCredentials {
        QiniuStaticCredentials {
            access_key: credentials.qiniu_access_key,
            secret_key: credentials.qiniu_secret_key,
        }
    }

    fn descriptor_definition() -> StorageConnectorDescriptor {
        object_storage_connector_descriptor(ObjectStorageConnectorDescriptorInput {
            connector_id: aster_drive_storage::ConnectorId::declared(Self::ID),
            label: "Qiniu Kodo",
            description: "Qiniu Cloud Kodo native object storage policy",
            ui: StorageConnectorUiDescriptorInput {
                label_key: "driver_type_qiniu",
                description_key: "policy_wizard_qiniu_storage_desc",
                icon_src: None,
                icon_name: Some("cloud"),
                badge_rgb: StorageConnectorBadgeRgb::new(0, 148, 255),
                helper_key: "policy_wizard_qiniu_helper",
                config_step_title_key: "policy_wizard_step_connection_title",
                config_step_description_key: "policy_wizard_step_qiniu_connection_desc",
                edit_context_key: "policy_edit_context_qiniu_desc",
                base_path_empty_display: "core:root",
                base_path_placeholder: "tenant/prefix",
            },
            deployment_scope: StorageConnectorDeploymentScope::SharedAcrossPrimaryInstances,
            supports_initial_setup: true,
            credential_mode: QiniuConnectorConfigV1::credential_mode(),
            fields: QiniuConnectorConfigV1::descriptor_fields(),
            presigned_part_etag_required: true,
            storage_native_processing: false,
            config_schema_version: 1,
            credential_schema_version: Some(1),
            related_issues: Vec::new(),
        })
    }

    fn build_driver(
        config: QiniuConnectorConfigV1,
        credentials: QiniuStaticCredentialsV1,
    ) -> Result<QiniuDriver> {
        QiniuDriver::new(
            Self::driver_config(config)?,
            Self::driver_credentials(credentials),
        )
        .map_err(Into::into)
    }
}

#[async_trait]
impl StorageConnector for QiniuConnector {
    fn descriptor(&self) -> StorageConnectorDescriptor {
        Self::descriptor_definition()
    }

    fn localization(&self) -> Result<aster_drive_storage::StorageConnectorLocalization> {
        let descriptor = Self::descriptor_definition();
        super::localization::builtin_connector_localization(
            Self::ID,
            &descriptor,
            localization::MESSAGES,
        )
    }

    fn validate_connector_config(
        &self,
        input: &aster_drive_storage::ConnectorConfigEnvelope,
    ) -> Result<aster_drive_storage::ConnectorConfigEnvelope> {
        let normalized =
            aster_drive_storage::connector_descriptor::normalize_storage_connector_config(
                &self.descriptor(),
                input,
            )
            .map_err(|error| AsterError::validation_error(error.to_string()))?;
        let config: QiniuConnectorConfigV1 =
            super::common::decode_normalized_connector_config(&normalized)?;
        let driver_config = Self::driver_config(config.clone())?;
        QiniuDriver::validate_config(
            &driver_config,
            &QiniuStaticCredentials {
                access_key: "placeholder".to_string(),
                secret_key: "placeholder".to_string(),
            },
        )
        .map_err(|error| AsterError::validation_error(error.message().to_string()))?;
        super::common::encode_normalized_connector_config(
            normalized.connector_id,
            normalized.schema_version,
            config,
        )
    }

    fn validate_credential_input(&self, input: &StorageConnectorCredentialInput) -> Result<()> {
        let credential: QiniuStaticCredentialsV1 =
            super::common::decode_static_credential(input, Self::ID)?;
        super::common::validate_required_credential_field(
            &credential.qiniu_access_key,
            "qiniu_access_key",
            Self::ID,
        )?;
        super::common::validate_required_credential_field(
            &credential.qiniu_secret_key,
            "qiniu_secret_key",
            Self::ID,
        )
    }

    fn import_legacy_credential(
        &self,
        _encryption_key: &str,
        _policy: &storage_policy::Model,
        input: super::LegacyStorageConnectorCredentialInput,
    ) -> Result<Option<serde_json::Value>> {
        super::common::import_legacy_static_credential(Self::ID, input, |legacy| {
            QiniuStaticCredentialsV1 {
                qiniu_access_key: legacy.access_key,
                qiniu_secret_key: legacy.secret_key,
            }
        })
    }

    async fn build_draft_driver(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        credential: &StorageConnectorCredentialInput,
    ) -> Result<Box<dyn StorageDriver>> {
        let _ = context;
        let config = Self::decode_config(policy)?;
        let credentials: QiniuStaticCredentialsV1 =
            super::common::decode_static_credential(credential, Self::ID)?;
        Ok(Box::new(Self::build_driver(config, credentials)?))
    }

    fn build_runtime_driver(
        &self,
        registry: &crate::storage::DriverRegistry,
        policy: &storage_policy::Model,
    ) -> Result<super::StorageConnectorDriver> {
        let config = Self::decode_config(policy)?;
        let credentials: QiniuStaticCredentialsV1 =
            super::common::runtime_static_credential(registry, policy, Self::ID)?;
        Ok(super::StorageConnectorDriver::multipart(
            std::sync::Arc::new(Self::build_driver(config, credentials)?),
        ))
    }

    async fn build_cleanup_driver(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        snapshots: super::StoragePolicyCleanupSnapshots<'_>,
    ) -> Result<std::sync::Arc<dyn StorageDriver>> {
        let config = Self::decode_config(policy)?;
        let credentials: QiniuStaticCredentialsV1 =
            super::common::static_credential_from_cleanup_snapshot(
                context,
                policy,
                snapshots,
                Self::ID,
                1,
            )?;
        Ok(std::sync::Arc::new(Self::build_driver(
            config,
            credentials,
        )?))
    }

    fn upload_transport(
        &self,
        policy: &storage_policy::Model,
    ) -> Result<StorageConnectorUploadTransport> {
        let config = Self::decode_config(policy)?;
        Ok(StorageConnectorUploadTransport::ObjectStorage(
            config.object_storage_upload_strategy,
        ))
    }

    fn presigned_download_enabled(&self, policy: &storage_policy::Model) -> Result<bool> {
        let config = Self::decode_config(policy)?;
        Ok(config.object_storage_download_strategy == ObjectStorageDownloadStrategy::Presigned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_drive_storage::traits::extensions::PresignedStorageDriver;

    #[test]
    fn region_endpoint_mapping_is_backend_owned() {
        let z0 = QiniuConnector::endpoints("z0").expect("z0 endpoint mapping");
        assert_eq!(z0.upload, "https://up-z0.qiniup.com");
        assert_eq!(z0.manage, "https://rs-z0.qiniuapi.com");
        assert_eq!(z0.list, "https://rsf-z0.qiniuapi.com");
        assert!(QiniuConnector::endpoints("custom").is_err());
    }

    #[test]
    fn descriptor_declares_form_presigned_and_multipart_capabilities() {
        let descriptor = QiniuConnector::descriptor_definition();
        assert_eq!(descriptor.connector_id.as_str(), QiniuConnector::ID);
        assert!(descriptor.capabilities.presigned_download);
        assert!(descriptor.upload_workflows.presigned_upload);
        assert!(descriptor.upload_workflows.object_multipart_upload);
        assert!(
            descriptor
                .upload_workflows
                .object_multipart_upload_capabilities
                .is_some()
        );
        assert!(!descriptor.capabilities.storage_native_thumbnail);
        assert!(!descriptor.capabilities.storage_native_media_metadata);
    }

    #[tokio::test]
    async fn form_request_contains_only_provider_fields_and_no_secret_key() {
        let driver = QiniuDriver::new(
            QiniuDriverConfig {
                bucket: "bucket".to_string(),
                region: "z0".to_string(),
                download_domain: "https://download.example.test".to_string(),
                object_prefix: "tenant".to_string(),
                endpoints: QiniuConnector::endpoints("z0").expect("z0 endpoints"),
                connect_timeout: Duration::from_secs(1),
                read_timeout: Duration::from_secs(1),
                operation_timeout: Duration::from_secs(1),
            },
            QiniuStaticCredentials {
                access_key: "ak".to_string(),
                secret_key: "secret-value".to_string(),
            },
        )
        .expect("driver config");
        let request = driver
            .presigned_form_upload_request("files/object", Duration::from_secs(60))
            .await
            .expect("form request")
            .expect("Qiniu supports form uploads");
        assert_eq!(request.url, "https://up-z0.qiniup.com");
        assert_eq!(
            request.fields.get("key"),
            Some(&"tenant/files/object".to_string())
        );
        assert!(request.fields.contains_key("token"));
        assert!(!request.fields.values().any(|value| value == "secret-value"));
    }
}
