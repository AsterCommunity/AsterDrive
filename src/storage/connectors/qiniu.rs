use std::time::Duration;

use async_trait::async_trait;

use crate::errors::{AsterError, Result};
use crate::storage::drivers::qiniu::{QiniuDriver, QiniuDriverConfig, QiniuStaticCredentials};
use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::{ObjectStorageDownloadStrategy, ObjectStorageUploadStrategy};
use aster_drive_storage::connector_descriptor::{
    ObjectStorageConnectorDescriptorInput, StorageConnectorBadgeRgb,
    StorageConnectorDeploymentScope, StorageConnectorDescriptor, StorageConnectorFieldDisplayInput,
    StorageConnectorFieldKind, StorageConnectorFieldScope, StorageConnectorPromotionDescriptor,
    StorageConnectorPromotionRequirement, StorageConnectorPromotionValueMatcher,
    StorageConnectorUiDescriptorInput, object_storage_connector_descriptor,
    storage_connector_field, storage_connector_field_with_display,
};
use aster_drive_storage::{
    StorageConnectorConfigSchema, StorageConnectorFieldDefaultMode,
    StorageConnectorFieldDefaultValue, StorageDriver,
};

use super::common::{StorageTransferDirection, transfer_strategy_field};
use super::{StorageConnector, StorageConnectorCredentialInput, StorageConnectorUploadTransport};

mod localization;

pub struct QiniuConnector;

const PROMOTE_FROM_S3_ID: &str = "promote_from_s3";

fn promote_from_s3_descriptor() -> StorageConnectorPromotionDescriptor {
    super::s3::s3_compatible_promotion_descriptor(super::s3::S3CompatiblePromotionDescriptorInput {
        promotion_id: PROMOTE_FROM_S3_ID,
        description_key: "policy_qiniu_promote_from_s3_desc",
        confirmation_key: "policy_qiniu_promote_from_s3_confirm",
        requirements: vec![
            StorageConnectorPromotionRequirement {
                source_field: "endpoint".to_string(),
                matcher: StorageConnectorPromotionValueMatcher::StringPrefix {
                    prefix: "https://".to_string(),
                    case_sensitive: false,
                },
                negate: false,
            },
            StorageConnectorPromotionRequirement {
                source_field: "endpoint".to_string(),
                matcher: StorageConnectorPromotionValueMatcher::UrlHostSuffix {
                    suffix: ".qiniucs.com".to_string(),
                },
                negate: false,
            },
            StorageConnectorPromotionRequirement {
                source_field: "s3_region".to_string(),
                matcher: StorageConnectorPromotionValueMatcher::StringEquals {
                    value: "auto".to_string(),
                    case_sensitive: false,
                },
                negate: true,
            },
        ],
        target_region_field: Some("s3_region"),
        target_access_key_field: "qiniu_access_key",
        target_secret_key_field: "qiniu_secret_key",
    })
}

aster_drive_storage::storage_connector_schema! {
    pub struct QiniuConnectorConfigV1 {
        config {
            pub endpoint: String => storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                name: "endpoint", scope: StorageConnectorFieldScope::ConnectorConfig,
                kind: StorageConnectorFieldKind::Text, required: true, secret: false,
                label_key: "qiniu_s3_endpoint", placeholder: Some("https://s3.cn-east-1.qiniucs.com"),
                help_key: Some("qiniu_s3_endpoint_desc"), required_message_key: None,
                invalid_protocol_message_key: Some("qiniu_s3_endpoint_protocol_error"),
                allowed_endpoint_protocols: vec!["https:"],
                allow_endpoint_without_protocol: false, trim_on_blur: true,
            }),
            pub bucket: String => {
                let mut field = storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                    name: "bucket", scope: StorageConnectorFieldScope::ConnectorConfig,
                    kind: StorageConnectorFieldKind::Text, required: true, secret: false,
                    label_key: "qiniu_s3_bucket", placeholder: Some("globally-unique-s3-space-name"),
                    help_key: Some("qiniu_s3_bucket_desc"), required_message_key: None,
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
            pub s3_region: String => {
                let mut field = storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                    name: "s3_region", scope: StorageConnectorFieldScope::ConnectorConfig,
                    kind: StorageConnectorFieldKind::Text, required: true, secret: false,
                    label_key: "qiniu_s3_region", placeholder: Some("cn-east-1"),
                    help_key: Some("qiniu_s3_region_desc"), required_message_key: None,
                    invalid_protocol_message_key: None, allowed_endpoint_protocols: Vec::new(),
                    allow_endpoint_without_protocol: false, trim_on_blur: true,
                });
                field.validation.max_length = Some(128);
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

impl QiniuConnector {
    pub const ID: &'static str = "asterdrive.storage.qiniu";

    fn decode_config(policy: &storage_policy::Model) -> Result<QiniuConnectorConfigV1> {
        super::common::decode_typed_policy_config(policy, Self::ID, 1).map(|(config, _)| config)
    }

    fn driver_config(config: QiniuConnectorConfigV1) -> QiniuDriverConfig {
        QiniuDriverConfig {
            endpoint: config.endpoint,
            bucket: config.bucket,
            base_path: config.base_path,
            region: config.s3_region,
            // First-class provider connectors own addressing. `false` lets the
            // AWS endpoint resolver select virtual-hosted style when the S3
            // space name is DNS-compatible and fall back when required.
            path_style: false,
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
                description: "Qiniu Cloud Kodo S3-compatible object storage policy",
                ui: StorageConnectorUiDescriptorInput {
                    label_key: "driver_type_qiniu",
                    description_key: "policy_wizard_qiniu_storage_desc",
                    icon_src: Some("/static/storage/qiniuyun-kodo.svg"),
                    icon_name: None,
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
                related_issues: vec![519, 474],
            });
        descriptor.promotions.push(promote_from_s3_descriptor());
        descriptor
    }

    fn build_driver(
        config: QiniuConnectorConfigV1,
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
        let mut config: QiniuConnectorConfigV1 =
            super::common::decode_normalized_connector_config(&normalized)?;
        let connection = crate::storage::drivers::s3_config::normalize_s3_endpoint_and_bucket(
            &config.endpoint,
            &config.bucket,
        )
        .map_err(|error| error.into_aster_error())?;
        config.endpoint = connection.endpoint;
        config.bucket = connection.bucket;
        QiniuDriver::validate_config(
            &Self::driver_config(config.clone()),
            &QiniuStaticCredentials {
                access_key: "placeholder".to_string(),
                secret_key: "placeholder".to_string(),
            },
        )
        .map_err(|error| AsterError::validation_error(error.message().to_string()))?;
        config.endpoint =
            normalize_qiniu_s3_endpoint(&config.endpoint, &config.bucket, &config.s3_region)?;
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
        Ok(StorageConnectorUploadTransport::ObjectStorage(
            config.object_storage_upload_strategy,
        ))
    }

    fn presigned_download_enabled(&self, policy: &storage_policy::Model) -> Result<bool> {
        let config = Self::decode_config(policy)?;
        Ok(config.object_storage_download_strategy == ObjectStorageDownloadStrategy::Presigned)
    }
}

fn normalize_qiniu_s3_endpoint(endpoint: &str, bucket: &str, region: &str) -> Result<String> {
    let parsed = url::Url::parse(endpoint)
        .map_err(|_| AsterError::validation_error("invalid Qiniu Kodo S3 endpoint URL"))?;
    if parsed.scheme() != "https" {
        return Err(AsterError::validation_error(
            "Qiniu Kodo S3 endpoint must use HTTPS",
        ));
    }
    let host = parsed.host_str().ok_or_else(|| {
        AsterError::validation_error("Qiniu Kodo S3 endpoint requires a hostname")
    })?;
    let expected_host = format!("s3.{region}.qiniucs.com");
    let uses_root_path = parsed.path().is_empty() || parsed.path() == "/";
    if parsed.port().is_some()
        || !uses_root_path
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(AsterError::validation_error(format!(
            "Qiniu Kodo S3 endpoint must use 'https://{expected_host}' or 'https://<S3-space-name>.{expected_host}' with no port, path, query, or fragment"
        )));
    }

    if host == expected_host {
        return Ok(format!("https://{expected_host}"));
    }

    let bucket_suffix = format!(".{expected_host}");
    let Some(endpoint_bucket) = host.strip_suffix(&bucket_suffix) else {
        return Err(AsterError::validation_error(format!(
            "Qiniu Kodo S3 endpoint must use 'https://{expected_host}' or 'https://<S3-space-name>.{expected_host}'; custom CNAMEs are not accepted"
        )));
    };
    if endpoint_bucket != bucket {
        return Err(AsterError::validation_error(format!(
            "Qiniu Kodo S3 endpoint identifies S3 space '{endpoint_bucket}', but the configured S3 space name is '{bucket}'"
        )));
    }

    Ok(format!("https://{expected_host}"))
}
