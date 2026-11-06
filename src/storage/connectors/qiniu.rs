//! Provider evidence: developer-docs/zh-CN/reference/storage-provider-evidence.md#qiniu
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

mod cors;
mod localization;
pub(crate) mod upgrade;

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
        descriptor
            .actions
            .push(cors::configure_cors_action_descriptor());
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
    async fn execute_draft_action(
        &self,
        context: &super::StorageConnectorContext<'_>,
        input: super::ExecuteDraftStorageConnectorActionInput,
    ) -> Result<super::StorageConnectorActionResult> {
        cors::validate_action(&input.action_id, &input.values)?;
        let origins = cors::allowed_origins(context.runtime_config())?;
        let connection = input.connection;
        let credential = self
            .prepare_draft_credential(context, input.policy_id, connection.storage.credential)
            .await?;
        self.validate_credential_input(&credential)?;
        let config = self.validate_connector_config(&connection.storage.connector_config)?;
        self.validate_policy_behavior(&connection.behavior.normalized())?;
        let driver = Self::build_driver(
            super::common::decode_normalized_connector_config(&config)?,
            super::common::decode_static_credential(&credential, Self::ID)?,
        )?;
        super::StorageConnectorActionResult::with_output(
            input.action_id,
            driver.configure_asterdrive_cors(&origins).await?,
        )
    }

    async fn execute_saved_action(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        input: super::ExecuteSavedStorageConnectorActionInput,
    ) -> Result<super::StorageConnectorActionResult> {
        cors::validate_action(&input.action_id, &input.values)?;
        let origins = cors::allowed_origins(context.runtime_config())?;
        let driver = Self::build_driver(
            Self::decode_config(policy)?,
            super::common::runtime_static_credential(context.driver_registry(), policy, Self::ID)?,
        )?;
        super::StorageConnectorActionResult::with_output(
            input.action_id,
            driver.configure_asterdrive_cors(&origins).await?,
        )
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::drivers::qiniu::tests::http_fixture_driver;

    #[tokio::test]
    async fn connection_probe_verifies_content_and_always_attempts_cleanup() {
        // Probe keys are random; the fixture echoes the form key so this checks
        // the real upload/stat/HEAD/GET/Range/delete protocol, not a fake driver.
        use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
        use std::sync::{Arc, Mutex};
        for failure in ["none", "stat", "head", "get", "range", "delete", "upload"] {
            let requests = Arc::new(Mutex::new(Vec::<String>::new()));
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let captured = requests.clone();
            let server=HttpServer::new(move||{
                let captured=captured.clone();
                App::new().default_service(web::to(move|req:HttpRequest,body:web::Bytes|{
                    let captured=captured.clone();async move {
                        let method=req.method().as_str();let path=req.path();captured.lock().unwrap().push(format!("{method} {path}"));
                        match (method,path) {
                            ("POST","/")=> {
                                if failure=="upload" {return HttpResponse::ServiceUnavailable().finish();}
                                let raw=String::from_utf8_lossy(&body);let start=raw.find("name=\"key\"").unwrap();let tail=&raw[start..];let value=&tail[tail.find("\r\n\r\n").unwrap()+4..];let key=value.split("\r\n").next().unwrap();
                                HttpResponse::Ok().json(serde_json::json!({"key":key,"hash":"hash"}))
                            }
                            ("GET",path) if path.starts_with("/stat/")=>HttpResponse::Ok().json(serde_json::json!({"fsize":if failure=="stat" {3}else{2},"mimeType":"application/octet-stream"})),
                            ("POST",path) if path.starts_with("/delete/")=>match failure {"delete"=>HttpResponse::Forbidden().finish(),_=>HttpResponse::Ok().finish()},
                            ("HEAD",_)=>HttpResponse::Ok().body(if failure=="head" {"bad"}else{"ok"}),
                            ("GET",_) if req.headers().contains_key("range")=>HttpResponse::PartialContent().insert_header(("content-range","bytes 1-1/2")).body(if failure=="range" {"x"}else{"k"}),
                            ("GET",_)=>HttpResponse::Ok().body(if failure=="get" {"no"}else{"ok"}),
                            _=>HttpResponse::NotFound().finish(),
                        }
                    }
                }))
            }).workers(1).listen(listener).unwrap().run();
            let handle = server.handle();
            tokio::spawn(server);
            let result = QiniuConnector::probe(&http_fixture_driver(&base).await).await;
            assert_eq!(
                result.is_ok(),
                failure == "none",
                "{failure}: {result:?}, requests={:?}",
                requests.lock().unwrap()
            );
            {
                let recorded = requests.lock().unwrap();
                assert!(recorded.last().unwrap().starts_with("POST /delete/"));
                if failure == "none" {
                    assert_eq!(recorded.len(), 6);
                }
            }
            handle.stop(true).await;
        }
    }

    #[tokio::test]
    async fn runtime_and_cleanup_factories_use_encrypted_credentials_and_survive_row_deletion() {
        use crate::storage::connectors::{
            StorageConnectorContext, StoragePolicyCleanupSnapshots, test_support,
        };
        use aster_drive_storage::ConnectorId;
        use sea_orm::ActiveModelTrait;
        let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
        test_support::migrate_current_storage_test_schema(&db).await;
        let policy=test_support::insertable_policy(test_support::policy(QiniuConnector::ID,2,serde_json::json!({"bucket":"archive","base_path":"tenant","download_origin":"https://origin.example.test","download_delivery":"origin","provider_resumable_upload_strategy":"server_relay","object_storage_download_strategy":"presigned"}),Default::default())).insert(&db).await.unwrap();
        let mut config = crate::config::Config::default();
        config.auth.storage_credential_secret_key = "native-qiniu-cleanup-test-master-key".into();
        let registry = crate::storage::DriverRegistry::noop().unwrap();
        let runtime = crate::config::RuntimeConfig::new();
        assert!(
            QiniuConnector
                .build_runtime_driver(&registry, &policy)
                .is_err()
        );
        super::super::persist_connector_credential_payload(
            &db,
            &config.auth.storage_credential_secret_key,
            policy.id,
            &ConnectorId::declared(QiniuConnector::ID),
            1,
            &serde_json::json!({"qiniu_access_key":"AK","qiniu_secret_key":"SK"}),
        )
        .await
        .unwrap();
        registry
            .reload_storage_policy_credentials(&db, &config)
            .await
            .unwrap();
        let runtime_driver = registry.get_driver(&policy).unwrap();
        assert!(runtime_driver.extensions().multipart.is_some());
        assert!(runtime_driver.extensions().provider_resumable.is_some());
        let context = StorageConnectorContext::new(&db, &config, &runtime, &registry, None);
        let snapshot = QiniuConnector
            .cleanup_snapshot_for_policy(&context, &policy)
            .await
            .unwrap()
            .unwrap();
        crate::db::repository::storage_policy_connector_credential_repo::delete_by_policy(
            &db, policy.id,
        )
        .await
        .unwrap();
        let driver = QiniuConnector
            .build_cleanup_driver(
                &context,
                &policy,
                StoragePolicyCleanupSnapshots {
                    driver_snapshot: Some(&snapshot),
                },
            )
            .await
            .unwrap();
        let signed = driver
            .extensions()
            .direct_download
            .unwrap()
            .resolve_download_url("owned", Duration::from_secs(60), Default::default())
            .await
            .unwrap()
            .unwrap();
        assert!(
            signed
                .url
                .starts_with("https://origin.example.test/tenant/owned?")
        );
        assert!(
            QiniuConnector
                .build_cleanup_driver(
                    &context,
                    &policy,
                    StoragePolicyCleanupSnapshots {
                        driver_snapshot: None
                    }
                )
                .await
                .is_err()
        );
        let mut wrong = policy.clone();
        wrong.id += 1;
        assert!(
            QiniuConnector
                .build_cleanup_driver(
                    &context,
                    &wrong,
                    StoragePolicyCleanupSnapshots {
                        driver_snapshot: Some(&snapshot)
                    }
                )
                .await
                .is_err()
        );
        for credential in [
            serde_json::json!({"qiniu_access_key":"","qiniu_secret_key":"SK"}),
            serde_json::json!({"qiniu_access_key":"AK","qiniu_secret_key":""}),
        ] {
            assert!(
                QiniuConnector
                    .validate_credential_input(&StorageConnectorCredentialInput::Static(credential))
                    .is_err()
            );
        }
    }
}
