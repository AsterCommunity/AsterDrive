use std::time::Duration;

use async_trait::async_trait;

use crate::errors::{AsterError, Result};
use crate::storage::drivers::qiniu::{
    QiniuDownloadDelivery, QiniuDriver, QiniuDriverConfig, QiniuStaticCredentials,
};
use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::{ObjectStorageDownloadStrategy, ProviderResumableUploadStrategy};
use aster_drive_storage::connector_descriptor::{
    ObjectStorageConnectorDescriptorInput, StorageConnectorBadgeRgb,
    StorageConnectorDeploymentScope, StorageConnectorDescriptor, StorageConnectorFieldDisplayInput,
    StorageConnectorFieldKind, StorageConnectorFieldScope, StorageConnectorUiDescriptorInput,
    object_storage_connector_descriptor, storage_connector_field,
    storage_connector_field_with_display,
};
use aster_drive_storage::{
    StorageConnectorConfigSchema, StorageConnectorFieldDefaultMode,
    StorageConnectorFieldDefaultValue, StorageDriver,
};

use super::common::{StorageTransferDirection, transfer_strategy_field};
use super::{
    StorageConnector, StorageConnectorCredentialInput, StorageConnectorIcon,
    StorageConnectorUploadTransport,
};

mod localization;
mod upgrade;
pub(crate) use upgrade::convert as convert_legacy_configs;

pub struct QiniuConnector;

aster_drive_storage::storage_connector_schema! {
    pub struct QiniuConnectorConfigV2 {
        config {
            pub bucket: String => {
                let mut field = storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                    name: "bucket", scope: StorageConnectorFieldScope::ConnectorConfig,
                    kind: StorageConnectorFieldKind::Text, required: true, secret: false,
                    label_key: "qiniu_native_bucket", placeholder: Some("native-bucket-name"),
                    help_key: Some("qiniu_native_bucket_desc"), required_message_key: None,
                    invalid_protocol_message_key: None, allowed_endpoint_protocols: Vec::new(),
                    allow_endpoint_without_protocol: false, trim_on_blur: true,
                });
                field.required_message_key = Some("qiniu_bucket_required".to_string());
                field
            },
            pub base_path: String => {
                let mut field = storage_connector_field(
                    "base_path", StorageConnectorFieldScope::ConnectorConfig,
                    StorageConnectorFieldKind::Text, false, false,
                );
                field.default_value = Some(StorageConnectorFieldDefaultValue::String(String::new()));
                field.default_mode = StorageConnectorFieldDefaultMode::MissingOrEmptyText;
                field
            },
            pub download_origin: String => storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                name: "download_origin", scope: StorageConnectorFieldScope::ConnectorConfig,
                kind: StorageConnectorFieldKind::Text, required: true, secret: false,
                label_key: "qiniu_download_origin", placeholder: Some("https://origin.example.com"),
                help_key: Some("qiniu_download_origin_desc"), required_message_key: None,
                invalid_protocol_message_key: Some("qiniu_https_required"), allowed_endpoint_protocols: vec!["https:"],
                allow_endpoint_without_protocol: false, trim_on_blur: true,
            }),
            #[serde(default = "crate::storage::drivers::qiniu::default_download_auth_validity_seconds")]
            pub download_auth_validity_seconds: u32 => {
                let mut field = storage_connector_field("download_auth_validity_seconds", StorageConnectorFieldScope::ConnectorConfig, StorageConnectorFieldKind::Number, false, false);
                field.label_key = "qiniu_download_auth_validity_seconds".into();
                field.help_key = Some("qiniu_download_auth_validity_seconds_desc".into());
                field.default_value = Some(StorageConnectorFieldDefaultValue::Integer(180));
                field.validation.min_integer = Some(1);
                field.validation.max_integer = Some(3600);
                field
            },
            #[serde(default)]
            pub download_delivery: QiniuDownloadDelivery => {
                let mut field = aster_drive_storage::storage_connector_select_field(
                    "download_delivery", StorageConnectorFieldScope::ConnectorConfig, true,
                    vec![
                        aster_drive_storage::StorageConnectorSelectOptionInput { value: "origin", label_key: "qiniu_delivery_origin", description_key: None },
                        aster_drive_storage::StorageConnectorSelectOptionInput { value: "qiniu_cdn", label_key: "qiniu_delivery_cdn", description_key: None },
                    ],
                );
                field.label_key = "qiniu_download_delivery".into();
                field.default_value = Some(StorageConnectorFieldDefaultValue::String("origin".into()));
                field
            },
            pub download_base_url: Option<String> => {
                let mut field = storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                    name: "download_base_url", scope: StorageConnectorFieldScope::ConnectorConfig,
                    kind: StorageConnectorFieldKind::Text, required: false, secret: false,
                    label_key: "qiniu_cdn_domain", placeholder: Some("https://cdn.example.com"),
                    help_key: Some("qiniu_cdn_domain_desc"), required_message_key: None,
                    invalid_protocol_message_key: Some("qiniu_https_required"), allowed_endpoint_protocols: vec!["https:"],
                    allow_endpoint_without_protocol: false, trim_on_blur: true,
                });
                field.visible_when = vec![aster_drive_storage::StorageConnectorFieldCondition {
                    scope: None, field: "download_delivery".into(), value: StorageConnectorFieldDefaultValue::String("qiniu_cdn".into()), negate: false,
                }];
                field.required_when = field.visible_when.clone();
                field
            },
            pub provider_resumable_upload_strategy: ProviderResumableUploadStrategy => {
                let mut field = aster_drive_storage::storage_connector_select_field(
                    "provider_resumable_upload_strategy", StorageConnectorFieldScope::ConnectorConfig, true,
                    vec![
                        aster_drive_storage::StorageConnectorSelectOptionInput { value: "server_relay", label_key: "provider_resumable_upload_strategy_server_relay", description_key: Some("qiniu_upload_relay_desc") },
                        aster_drive_storage::StorageConnectorSelectOptionInput { value: "frontend_direct", label_key: "provider_resumable_upload_strategy_frontend_direct", description_key: Some("qiniu_upload_direct_desc") },
                    ],
                );
                field.default_value = Some(StorageConnectorFieldDefaultValue::String("server_relay".into()));
                field
            },
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

impl QiniuConnector {
    pub const ID: &'static str = "asterdrive.storage.qiniu";

    async fn probe(driver: &QiniuDriver) -> Result<()> {
        let path = format!("_aster_connection_test-{}", uuid::Uuid::new_v4());
        let result = async {
            driver.put(&path, b"ok").await?;
            if driver.metadata(&path).await?.size != 2
                || driver.head_metadata(&path).await?.size != 2
                || driver.get(&path).await? != b"ok"
            {
                return Err(AsterError::storage_driver_error(
                    "Kodo probe returned inconsistent content/metadata",
                ));
            }
            let mut reader = driver.get_range(&path, 1, Some(1)).await?;
            let mut data = Vec::new();
            tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut data)
                .await
                .map_err(|_| AsterError::storage_driver_error("Kodo probe Range read failed"))?;
            if data != b"k" {
                return Err(AsterError::storage_driver_error(
                    "Kodo probe returned an inconsistent byte range",
                ));
            }
            Ok(())
        }
        .await;
        let cleanup = driver.delete(&path).await;
        if let Err(error) = &cleanup {
            tracing::warn!(kind = error.kind().as_str(), "Kodo probe cleanup failed");
        }
        result?;
        cleanup.map_err(Into::into)
    }

    fn decode_config(policy: &storage_policy::Model) -> Result<QiniuConnectorConfigV2> {
        super::common::decode_typed_policy_config(policy, Self::ID, 2).map(|(config, _)| config)
    }

    fn driver_config(config: QiniuConnectorConfigV2) -> QiniuDriverConfig {
        QiniuDriverConfig {
            bucket: config.bucket,
            base_path: config.base_path,
            download_origin: config.download_origin,
            download_delivery: config.download_delivery,
            download_base_url: config.download_base_url,
            download_auth_validity_seconds: config.download_auth_validity_seconds,
            connect_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_secs(30),
            operation_timeout: Duration::from_secs(3_600),
        }
    }

    fn driver_credentials(credentials: QiniuStaticCredentialsV1) -> QiniuStaticCredentials {
        QiniuStaticCredentials {
            access_key: credentials.qiniu_access_key,
            secret_key: credentials.qiniu_secret_key,
        }
    }

    pub(super) fn descriptor_definition() -> StorageConnectorDescriptor {
        let mut descriptor =
            object_storage_connector_descriptor(ObjectStorageConnectorDescriptorInput {
                connector_id: aster_drive_storage::ConnectorId::declared(Self::ID),
                label: "Qiniu Kodo",
                description: "Qiniu Cloud Kodo native object storage policy",
                ui: StorageConnectorUiDescriptorInput {
                    label_key: "driver_type_qiniu",
                    description_key: "policy_wizard_qiniu_storage_desc",
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
                credential_mode: QiniuConnectorConfigV2::credential_mode(),
                fields: QiniuConnectorConfigV2::descriptor_fields(),
                presigned_part_etag_required: false,
                storage_native_processing: false,
                config_schema_version: 2,
                credential_schema_version: Some(1),
                related_issues: vec![702],
            });
        descriptor.capabilities.custom_download_base_url = true;
        descriptor.capabilities.object_storage_transfer_strategy = false;
        descriptor.upload_workflows.presigned_upload = false;
        descriptor.upload_workflows.provider_resumable_upload = true;
        descriptor
            .upload_workflows
            .frontend_direct_provider_resumable_upload = true;
        descriptor
            .upload_workflows
            .provider_resumable_upload_capabilities =
            Some(crate::storage::drivers::qiniu::resumable::upload_capabilities().into());
        if let Some(capabilities) = descriptor
            .upload_workflows
            .object_multipart_upload_capabilities
            .as_mut()
        {
            capabilities.min_part_size = 1024 * 1024;
        }
        descriptor
    }

    fn build_driver(
        config: QiniuConnectorConfigV2,
        credentials: QiniuStaticCredentialsV1,
    ) -> Result<QiniuDriver> {
        QiniuDriver::new(
            Self::driver_config(config),
            Self::driver_credentials(credentials),
        )
        .map_err(Into::into)
    }
}

#[async_trait]
impl StorageConnector for QiniuConnector {
    async fn test_draft_connection(
        &self,
        context: &super::StorageConnectorContext<'_>,
        input: super::TestDraftStorageConnectorConnectionInput,
    ) -> Result<()> {
        let connection = input.connection;
        let credentials = self
            .prepare_draft_credential(context, input.policy_id, connection.storage.credential)
            .await?;
        self.validate_credential_input(&credentials)?;
        let config = self.validate_connector_config(&connection.storage.connector_config)?;
        self.validate_policy_behavior(&connection.behavior.normalized())?;
        let driver = Self::build_driver(
            super::common::decode_normalized_connector_config(&config)?,
            super::common::decode_static_credential(&credentials, Self::ID)?,
        )?;
        Self::probe(&driver).await
    }

    async fn test_saved_connection(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
    ) -> Result<()> {
        let driver = Self::build_driver(
            Self::decode_config(policy)?,
            super::common::runtime_static_credential(context.driver_registry(), policy, Self::ID)?,
        )?;
        Self::probe(&driver).await
    }
    fn icon(&self) -> Option<StorageConnectorIcon> {
        Some(StorageConnectorIcon {
            bytes: include_bytes!("assets/qiniuyun-kodo.svg"),
            content_type: "image/svg+xml",
            revision: "1",
        })
    }

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
        let mut config: QiniuConnectorConfigV2 =
            super::common::decode_normalized_connector_config(&normalized)?;
        config.bucket = config.bucket.trim().to_string();
        config.base_path = config.base_path.trim_matches('/').to_string();
        config.download_origin = config
            .download_origin
            .trim()
            .trim_end_matches('/')
            .to_string();
        config.download_base_url = config
            .download_base_url
            .map(|url| url.trim().trim_end_matches('/').to_string());
        QiniuDriver::validate_config(
            &Self::driver_config(config.clone()),
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

    async fn build_driver_from_connection(
        &self,
        context: &super::StorageConnectorContext<'_>,
        connector_config: &aster_drive_storage::ConnectorConfigEnvelope,
        credential: &StorageConnectorCredentialInput,
    ) -> Result<Box<dyn StorageDriver>> {
        let _ = context;
        let config = super::common::decode_normalized_connector_config(connector_config)?;
        let credentials = super::common::decode_static_credential(credential, Self::ID)?;
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
        Ok(StorageConnectorUploadTransport::ProviderResumable(
            config.provider_resumable_upload_strategy,
        ))
    }

    fn direct_download_enabled(&self, policy: &storage_policy::Model) -> Result<bool> {
        let config = Self::decode_config(policy)?;
        Ok(config.object_storage_download_strategy == ObjectStorageDownloadStrategy::Presigned)
    }
}
