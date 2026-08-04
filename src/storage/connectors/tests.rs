use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use aster_drive_model::types::{
    MicrosoftGraphCloud, ObjectStorageDownloadStrategy, ObjectStorageUploadStrategy,
    ProviderDownloadFilenameMode, ProviderDownloadStrategy, ProviderResumableUploadStrategy,
    RemoteDownloadStrategy, RemoteUploadStrategy,
};
use aster_drive_storage::connector_descriptor::{
    StorageConnectorBadgeRgb, StorageConnectorCredentialMode, StorageConnectorDeploymentScope,
    StorageConnectorFieldScope, StorageConnectorObjectNamingMode, StorageConnectorSelectDataSource,
    StorageConnectorSelectValueKind,
};
use aster_drive_storage::{ConnectorConfigEnvelope, ConnectorId, StoragePolicyBehaviorConfig};

use super::azure_blob::AzureBlobConnectorConfigV1;
use super::local::LocalConnectorConfigV1;
use super::onedrive::{OneDriveAccountMode, OneDriveConnectorConfigV1};
use super::remote::RemoteConnectorConfigV1;
use super::s3::S3ConnectorConfigV1;
use super::sftp::SftpConnectorConfigV1;
use super::tencent_cos::TencentCosConnectorConfigV1;
use super::*;

struct LocalizationContractConnector {
    descriptor: StorageConnectorDescriptor,
    localization: aster_drive_storage::StorageConnectorLocalization,
}

#[async_trait::async_trait]
impl StorageConnector for LocalizationContractConnector {
    fn descriptor(&self) -> StorageConnectorDescriptor {
        self.descriptor.clone()
    }

    fn localization(&self) -> Result<aster_drive_storage::StorageConnectorLocalization> {
        Ok(self.localization.clone())
    }

    async fn build_draft_driver(
        &self,
        _context: &StorageConnectorContext<'_>,
        _policy: &storage_policy::Model,
        _credential: &StorageConnectorCredentialInput,
    ) -> Result<Box<dyn StorageDriver>> {
        panic!("localization contract tests do not construct drivers")
    }

    fn build_runtime_driver(
        &self,
        _registry: &crate::storage::DriverRegistry,
        _policy: &storage_policy::Model,
    ) -> Result<StorageConnectorDriver> {
        panic!("localization contract tests do not construct drivers")
    }

    fn upload_transport(
        &self,
        _policy: &storage_policy::Model,
    ) -> Result<StorageConnectorUploadTransport> {
        panic!("localization contract tests do not resolve upload transport")
    }
}

fn registry() -> &'static StorageConnectorRegistry {
    static REGISTRY: std::sync::LazyLock<StorageConnectorRegistry> =
        std::sync::LazyLock::new(|| {
            builtin_storage_connector_registry().expect("built-in connector registry")
        });
    &REGISTRY
}

fn connector(id: &'static str) -> &'static dyn StorageConnector {
    registry()
        .require_connector(&ConnectorId::declared(id))
        .expect("registered connector")
}

fn descriptor(id: &'static str) -> StorageConnectorDescriptor {
    connector(id).descriptor()
}

fn local_config(base_path: &str) -> LocalConnectorConfigV1 {
    LocalConnectorConfigV1 {
        base_path: base_path.to_string(),
        content_dedup: false,
    }
}

fn s3_config(upload: ObjectStorageUploadStrategy) -> S3ConnectorConfigV1 {
    S3ConnectorConfigV1 {
        endpoint: "https://s3.example.test".to_string(),
        bucket: "archive".to_string(),
        base_path: "tenant-a".to_string(),
        object_storage_upload_strategy: upload,
        object_storage_download_strategy: ObjectStorageDownloadStrategy::RelayStream,
        s3_path_style: true,
        s3_region: "auto".to_string(),
        s3_connect_timeout_secs: 5,
        s3_read_timeout_secs: 30,
        s3_operation_timeout_secs: 3_600,
    }
}

fn sftp_config() -> SftpConnectorConfigV1 {
    SftpConnectorConfigV1 {
        endpoint: "sftp://storage.example.test:22".to_string(),
        base_path: "tenant-a".to_string(),
        sftp_host_key_fingerprint: Some("SHA256:abc123".to_string()),
    }
}

fn azure_config(upload: ObjectStorageUploadStrategy) -> AzureBlobConnectorConfigV1 {
    AzureBlobConnectorConfigV1 {
        endpoint: "https://account.blob.core.windows.net".to_string(),
        bucket: "archive".to_string(),
        base_path: "tenant-a".to_string(),
        object_storage_upload_strategy: upload,
        object_storage_download_strategy: ObjectStorageDownloadStrategy::RelayStream,
    }
}

fn cos_config(upload: ObjectStorageUploadStrategy) -> TencentCosConnectorConfigV1 {
    TencentCosConnectorConfigV1 {
        endpoint: "https://cos.ap-beijing.myqcloud.com".to_string(),
        bucket: "archive-1250000000".to_string(),
        base_path: "tenant-a".to_string(),
        object_storage_upload_strategy: upload,
        object_storage_download_strategy: ObjectStorageDownloadStrategy::RelayStream,
        storage_native_processing_enabled: false,
        storage_native_media_metadata_enabled: false,
    }
}

fn remote_config(upload: RemoteUploadStrategy) -> RemoteConnectorConfigV1 {
    RemoteConnectorConfigV1 {
        base_path: "tenant-a".to_string(),
        remote_node_id: Some(7),
        remote_storage_target_key: Some("rst_test".to_string()),
        remote_download_strategy: RemoteDownloadStrategy::RelayStream,
        remote_upload_strategy: upload,
    }
}

fn onedrive_config(
    strategy: ProviderResumableUploadStrategy,
    account_mode: OneDriveAccountMode,
) -> OneDriveConnectorConfigV1 {
    OneDriveConnectorConfigV1 {
        base_path: "tenant-a".to_string(),
        provider_resumable_upload_strategy: strategy,
        provider_download_strategy: ProviderDownloadStrategy::ServerRelay,
        provider_download_filename_mode: ProviderDownloadFilenameMode::ProviderNative,
        cloud: MicrosoftGraphCloud::Global,
        account_mode,
        tenant: None,
        drive_id: None,
        root_item_id: None,
        site_id: None,
        group_id: None,
    }
}

fn policy<T: serde::Serialize>(connector_id: &'static str, values: T) -> storage_policy::Model {
    super::test_support::policy(
        connector_id,
        1,
        values,
        StoragePolicyBehaviorConfig::default(),
    )
}

#[test]
fn registry_exposes_each_builtin_connector_once_in_stable_order() {
    let descriptors = registry().descriptors();
    let actual = descriptors
        .iter()
        .map(|descriptor| descriptor.connector_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        vec![
            LocalConnector::ID,
            S3Connector::ID,
            SftpConnector::ID,
            AzureBlobConnector::ID,
            TencentCosConnector::ID,
            RemoteConnector::ID,
            OneDriveConnector::ID,
        ]
    );
    assert_eq!(actual.iter().copied().collect::<HashSet<_>>().len(), 7);
}

#[test]
fn builtin_bundles_keep_connector_owned_management_messages() {
    let locale = aster_drive_model::types::LocaleTag::parse("en").expect("English locale");
    let onedrive = connector(OneDriveConnector::ID)
        .localization()
        .expect("OneDrive localization")
        .bundle(&locale);
    assert_eq!(
        onedrive.messages.get("onedrive_credential_title"),
        Some(&"Microsoft Graph credential".to_string())
    );
    assert_eq!(
        onedrive
            .messages
            .get("policy_connector_created_authorize_next"),
        Some(&"OneDrive policy created. Authorize Microsoft Graph next.".to_string())
    );

    let onedrive_descriptor = descriptor(OneDriveConnector::ID);
    let credential_management = onedrive_descriptor
        .credential_management
        .as_ref()
        .expect("OneDrive credential management descriptor");
    assert_eq!(
        credential_management
            .status_keys
            .get("authorized")
            .map(String::as_str),
        Some("onedrive_credential_status_authorized")
    );
    assert_eq!(
        credential_management.created_authorize_next_key.as_deref(),
        Some("policy_connector_created_authorize_next")
    );

    let remote = connector(RemoteConnector::ID)
        .localization()
        .expect("remote localization")
        .bundle(&locale);
    assert_eq!(
        remote.messages.get("policy_wizard_remote_node_required"),
        Some(&"Choose a remote node before continuing.".to_string())
    );

    let remote_descriptor = descriptor(RemoteConnector::ID);
    assert_eq!(
        remote_descriptor
            .fields
            .iter()
            .find(|field| field.name == "remote_node_id")
            .and_then(|field| field.required_message_key.as_deref()),
        Some("policy_wizard_remote_node_required")
    );
    assert_eq!(
        remote_descriptor
            .fields
            .iter()
            .find(|field| field.name == "remote_storage_target_key")
            .and_then(|field| field.required_message_key.as_deref()),
        Some("policy_wizard_remote_storage_target_required")
    );
}

#[test]
fn registry_rejects_duplicate_and_unknown_connector_ids() {
    let error = match StorageConnectorRegistry::new(vec![
        Arc::new(LocalConnector),
        Arc::new(LocalConnector),
    ]) {
        Ok(_) => panic!("duplicate connector id must fail registration"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("registered more than once"));

    let error = match registry().require_connector(&ConnectorId::declared("com.example.missing")) {
        Ok(_) => panic!("unknown connector id must fail lookup"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("is not registered"));
}

#[test]
fn registry_rejects_localization_for_another_connector() {
    let descriptor = descriptor(LocalConnector::ID);
    let source = connector(LocalConnector::ID)
        .localization()
        .expect("local localization");
    let bundle = source.bundle(&aster_drive_model::types::LocaleTag::parse("en").unwrap());
    let localization = aster_drive_storage::StorageConnectorLocalization::new(
        ConnectorId::declared("com.example.other"),
        bundle.resolved_locale,
        "test",
        BTreeMap::from([(bundle.requested_locale, bundle.messages)]),
    )
    .expect("valid localization with the wrong connector id");

    let error = match StorageConnectorRegistry::new(vec![Arc::new(LocalizationContractConnector {
        descriptor,
        localization,
    })]) {
        Ok(_) => panic!("localization connector id mismatch must fail registration"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("returned localization for"));
}

#[test]
fn registry_rejects_localization_missing_a_descriptor_message() {
    let descriptor = descriptor(LocalConnector::ID);
    let locale = aster_drive_model::types::LocaleTag::parse("en").unwrap();
    let localization = aster_drive_storage::StorageConnectorLocalization::new(
        descriptor.connector_id.clone(),
        locale.clone(),
        "test",
        BTreeMap::from([(
            locale,
            BTreeMap::from([("driver_type_local".to_string(), "Local".to_string())]),
        )]),
    )
    .expect("partial localization is structurally valid");

    let error = match StorageConnectorRegistry::new(vec![Arc::new(LocalizationContractConnector {
        descriptor,
        localization,
    })]) {
        Ok(_) => panic!("missing descriptor message id must fail registration"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("missing descriptor message id"));
}

#[test]
fn policy_lookup_rejects_invalid_and_unregistered_persisted_ids() {
    let mut invalid = policy(LocalConnector::ID, local_config("./data/uploads"));
    invalid.connector_id = "INVALID ID".to_string();
    let error = match registry().require_policy(&invalid) {
        Ok(_) => panic!("invalid persisted connector id must fail"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("invalid connector id"));

    let mut missing = invalid;
    missing.connector_id = "com.example.missing".to_string();
    let error = match registry().require_policy(&missing) {
        Ok(_) => panic!("unregistered persisted connector id must fail"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("is not registered"));
}

#[test]
fn descriptors_are_complete_and_keep_config_credentials_separate() {
    for descriptor in registry().descriptors() {
        assert!(!descriptor.ui.label_key.trim().is_empty());
        assert!(!descriptor.ui.description_key.trim().is_empty());
        assert!(descriptor.ui.icon_src.is_some() || descriptor.ui.icon_name.is_some());
        assert!(descriptor.config_schema_version > 0);

        let mut names = Vec::new();
        for field in &descriptor.fields {
            assert!(
                !names.contains(&(field.scope, field.name.as_str())),
                "duplicate descriptor field '{}' in scope {:?}",
                field.name,
                field.scope
            );
            names.push((field.scope, field.name.as_str()));
            if field.secret {
                assert_ne!(field.scope, StorageConnectorFieldScope::ConnectorConfig);
            }
            if matches!(
                field.name.as_str(),
                "object_storage_upload_strategy"
                    | "object_storage_download_strategy"
                    | "remote_upload_strategy"
                    | "remote_download_strategy"
                    | "provider_resumable_upload_strategy"
                    | "provider_download_strategy"
                    | "provider_download_filename_mode"
            ) {
                assert!(
                    field.select.as_ref().is_some_and(|select| select
                        .options
                        .iter()
                        .all(|option| option.description_key.is_some())),
                    "strategy field '{}' must explain every option",
                    field.name
                );
            }
        }
    }

    for (id, rgb) in [
        (
            LocalConnector::ID,
            StorageConnectorBadgeRgb::new(16, 185, 129),
        ),
        (S3Connector::ID, StorageConnectorBadgeRgb::new(59, 130, 246)),
        (
            OneDriveConnector::ID,
            StorageConnectorBadgeRgb::new(59, 130, 246),
        ),
        (
            RemoteConnector::ID,
            StorageConnectorBadgeRgb::new(245, 158, 11),
        ),
        (
            SftpConnector::ID,
            StorageConnectorBadgeRgb::new(139, 92, 246),
        ),
        (
            TencentCosConnector::ID,
            StorageConnectorBadgeRgb::new(6, 182, 212),
        ),
        (
            AzureBlobConnector::ID,
            StorageConnectorBadgeRgb::new(14, 165, 233),
        ),
    ] {
        assert_eq!(descriptor(id).ui.badge_rgb, rgb);
    }

    assert_eq!(
        descriptor(LocalConnector::ID).deployment_scope,
        StorageConnectorDeploymentScope::InstanceLocal
    );
    for id in [
        S3Connector::ID,
        SftpConnector::ID,
        AzureBlobConnector::ID,
        TencentCosConnector::ID,
        RemoteConnector::ID,
        OneDriveConnector::ID,
    ] {
        assert_eq!(
            descriptor(id).deployment_scope,
            StorageConnectorDeploymentScope::SharedAcrossPrimaryInstances
        );
    }
}

#[test]
fn transfer_strategy_descriptors_keep_upload_and_download_copy_distinct() {
    for connector_id in [
        S3Connector::ID,
        AzureBlobConnector::ID,
        TencentCosConnector::ID,
    ] {
        assert_transfer_strategy_copy(
            &descriptor(connector_id),
            "object_storage_upload_strategy",
            "upload_strategy_relay_stream",
            "upload_strategy_relay_stream_desc",
            "upload_strategy_presigned",
            "upload_strategy_presigned_desc",
        );
        assert_transfer_strategy_copy(
            &descriptor(connector_id),
            "object_storage_download_strategy",
            "download_strategy_relay_stream",
            "download_strategy_relay_stream_desc",
            "download_strategy_presigned",
            "download_strategy_presigned_desc",
        );
    }

    let remote = descriptor(RemoteConnector::ID);
    assert_transfer_strategy_copy(
        &remote,
        "remote_upload_strategy",
        "upload_strategy_relay_stream",
        "upload_strategy_relay_stream_desc",
        "upload_strategy_presigned",
        "upload_strategy_presigned_desc",
    );
    assert_transfer_strategy_copy(
        &remote,
        "remote_download_strategy",
        "download_strategy_relay_stream",
        "download_strategy_relay_stream_desc",
        "download_strategy_presigned",
        "download_strategy_presigned_desc",
    );
}

fn assert_transfer_strategy_copy(
    descriptor: &StorageConnectorDescriptor,
    field_name: &str,
    relay_label: &str,
    relay_description: &str,
    presigned_label: &str,
    presigned_description: &str,
) {
    let field = descriptor
        .fields
        .iter()
        .find(|field| field.name == field_name)
        .unwrap_or_else(|| panic!("missing transfer strategy field '{field_name}'"));
    let options = &field.select.as_ref().expect("select descriptor").options;
    assert_eq!(options.len(), 2);
    assert_eq!(options[0].label_key, relay_label);
    assert_eq!(
        options[0].description_key.as_deref(),
        Some(relay_description)
    );
    assert_eq!(options[1].label_key, presigned_label);
    assert_eq!(
        options[1].description_key.as_deref(),
        Some(presigned_description)
    );
}

#[test]
fn object_naming_and_local_default_path_are_connector_owned() {
    assert_eq!(
        descriptor(OneDriveConnector::ID).capabilities.object_naming,
        StorageConnectorObjectNamingMode::OriginalFilename
    );
    for id in [
        LocalConnector::ID,
        S3Connector::ID,
        SftpConnector::ID,
        AzureBlobConnector::ID,
        TencentCosConnector::ID,
        RemoteConnector::ID,
    ] {
        assert_eq!(
            descriptor(id).capabilities.object_naming,
            StorageConnectorObjectNamingMode::OpaqueUuid
        );
    }
    let local = descriptor(LocalConnector::ID);
    assert_eq!(local.ui.base_path_empty_display, "./data/uploads");
    assert_eq!(local.ui.base_path_placeholder, "./data/uploads");
}

#[test]
fn remote_connector_declares_dynamic_select_sources_and_dependency_types() {
    let remote = descriptor(RemoteConnector::ID);
    remote
        .validate()
        .expect("built-in remote descriptor must be registration-safe");

    let node = remote
        .fields
        .iter()
        .find(|field| field.name == "remote_node_id")
        .unwrap();
    let node_select = node.select.as_ref().unwrap();
    assert_eq!(
        node_select.data_source,
        Some(StorageConnectorSelectDataSource::RemoteNodes)
    );
    assert_eq!(
        node_select.value_kind,
        StorageConnectorSelectValueKind::Integer
    );
    assert_eq!(node_select.depends_on, None);
    assert!(node_select.options.is_empty());

    let target = remote
        .fields
        .iter()
        .find(|field| field.name == "remote_storage_target_key")
        .unwrap();
    let target_select = target.select.as_ref().unwrap();
    assert_eq!(
        target_select.data_source,
        Some(StorageConnectorSelectDataSource::RemoteStorageTargets)
    );
    assert_eq!(
        target_select.value_kind,
        StorageConnectorSelectValueKind::String
    );
    assert_eq!(target_select.depends_on.as_deref(), Some("remote_node_id"));
    assert!(target_select.options.is_empty());
}

#[test]
fn credential_channels_are_mutually_exclusive_by_connector_contract() {
    let none = StorageConnectorCredentialInput::None;
    let wrong_static = StorageConnectorCredentialInput::Static(serde_json::json!({}));
    let wrong_application =
        StorageConnectorCredentialInput::AuthorizationApplication(serde_json::json!({}));

    let credential_values = |connector: &dyn StorageConnector, scope| {
        serde_json::Value::Object(
            connector
                .descriptor()
                .fields
                .into_iter()
                .filter(|field| field.scope == scope)
                .map(|field| {
                    (
                        field.name,
                        serde_json::Value::String("test-value".to_string()),
                    )
                })
                .collect(),
        )
    };

    for id in [LocalConnector::ID, RemoteConnector::ID] {
        let connector = connector(id);
        assert_eq!(
            connector.descriptor().credential_mode,
            StorageConnectorCredentialMode::None
        );
        assert!(connector.validate_credential_input(&none).is_ok());
        assert!(connector.validate_credential_input(&wrong_static).is_err());
        assert!(
            connector
                .validate_credential_input(&wrong_application)
                .is_err()
        );
    }
    for id in [
        S3Connector::ID,
        SftpConnector::ID,
        AzureBlobConnector::ID,
        TencentCosConnector::ID,
    ] {
        let connector = connector(id);
        assert_eq!(
            connector.descriptor().credential_mode,
            StorageConnectorCredentialMode::StaticSecret
        );
        let static_secret = StorageConnectorCredentialInput::Static(credential_values(
            connector,
            StorageConnectorFieldScope::StaticCredential,
        ));
        assert!(connector.validate_credential_input(&static_secret).is_ok());
        assert!(connector.validate_credential_input(&none).is_err());
        assert!(
            connector
                .validate_credential_input(&wrong_application)
                .is_err()
        );
    }
    let onedrive = connector(OneDriveConnector::ID);
    assert_eq!(
        onedrive.descriptor().credential_mode,
        StorageConnectorCredentialMode::OauthDelegated
    );
    let application = StorageConnectorCredentialInput::AuthorizationApplication(credential_values(
        onedrive,
        StorageConnectorFieldScope::AuthorizationApplication,
    ));
    assert!(onedrive.validate_credential_input(&application).is_ok());
    assert!(onedrive.validate_credential_input(&none).is_err());
    assert!(onedrive.validate_credential_input(&wrong_static).is_err());
}

#[test]
fn typed_connector_config_round_trips_and_rejects_unknown_fields() {
    let connector = connector(S3Connector::ID);
    let input = super::test_support::connection_config(
        S3Connector::ID,
        1,
        s3_config(ObjectStorageUploadStrategy::RelayStream),
    );
    let normalized = connector
        .validate_connector_config(&input)
        .expect("typed S3 config should normalize");
    let values: S3ConnectorConfigV1 =
        serde_json::from_value(serde_json::to_value(normalized.values).unwrap()).unwrap();
    assert_eq!(values.bucket, "archive");
    assert_eq!(values.s3_region, "auto");

    let mut values = BTreeMap::new();
    values.insert(
        "endpoint".to_string(),
        serde_json::json!("https://s3.example.test"),
    );
    values.insert("bucket".to_string(), serde_json::json!("archive"));
    values.insert("unknown".to_string(), serde_json::json!(true));
    let unknown = ConnectorConfigEnvelope::new(ConnectorId::declared(S3Connector::ID), 1, values);
    let error = connector
        .validate_connector_config(&unknown)
        .expect_err("unknown connector field must fail");
    assert!(error.to_string().contains("unknown"));
}

fn assert_empty_base_path_normalizes<T>(
    connector_id: &'static str,
    config: T,
    expected_base_path: &str,
) where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let input = super::test_support::connection_config(connector_id, 1, config);
    let normalized = connector(connector_id)
        .validate_connector_config(&input)
        .expect("an empty optional base_path should normalize");
    let typed: T = serde_json::from_value(
        serde_json::to_value(normalized.values).expect("normalized field map should serialize"),
    )
    .expect("normalized connector config should decode into its declared type");
    let typed = serde_json::to_value(typed).expect("typed connector config should serialize");
    assert_eq!(typed["base_path"], expected_base_path, "{connector_id}");
}

#[test]
fn optional_empty_base_paths_decode_without_weakening_required_fields() {
    assert_empty_base_path_normalizes(LocalConnector::ID, local_config(""), "./data/uploads");

    let mut s3 = s3_config(ObjectStorageUploadStrategy::RelayStream);
    s3.base_path.clear();
    assert_empty_base_path_normalizes(S3Connector::ID, s3.clone(), "");

    let mut sftp = sftp_config();
    sftp.base_path.clear();
    assert_empty_base_path_normalizes(SftpConnector::ID, sftp, "");

    let mut azure = azure_config(ObjectStorageUploadStrategy::RelayStream);
    azure.base_path.clear();
    assert_empty_base_path_normalizes(AzureBlobConnector::ID, azure, "");

    let mut cos = cos_config(ObjectStorageUploadStrategy::RelayStream);
    cos.base_path.clear();
    assert_empty_base_path_normalizes(TencentCosConnector::ID, cos, "");

    let mut remote = remote_config(RemoteUploadStrategy::RelayStream);
    remote.base_path.clear();
    assert_empty_base_path_normalizes(RemoteConnector::ID, remote, "");

    let mut onedrive = onedrive_config(
        ProviderResumableUploadStrategy::ServerRelay,
        OneDriveAccountMode::Personal,
    );
    onedrive.base_path.clear();
    assert_empty_base_path_normalizes(OneDriveConnector::ID, onedrive, "");

    let mut missing_required = serde_json::to_value(s3)
        .and_then(serde_json::from_value::<BTreeMap<String, serde_json::Value>>)
        .expect("typed S3 config should become a field map");
    missing_required.remove("endpoint");
    let error = connector(S3Connector::ID)
        .validate_connector_config(&ConnectorConfigEnvelope::new(
            ConnectorId::declared(S3Connector::ID),
            1,
            missing_required,
        ))
        .expect_err("missing required endpoint must still fail");
    assert!(error.to_string().contains("endpoint"));
}

#[test]
fn policy_envelope_rejects_connector_id_and_schema_mismatches() {
    let mut wrong_id = policy(LocalConnector::ID, local_config("./data/uploads"));
    wrong_id.connector_id = S3Connector::ID.to_string();
    let error = connector(S3Connector::ID)
        .policy_behavior(&wrong_id)
        .expect_err("envelope connector id mismatch must fail");
    assert!(error.to_string().contains("connector config id mismatch"));

    let wrong_schema = super::test_support::policy(
        LocalConnector::ID,
        2,
        local_config("./data/uploads"),
        StoragePolicyBehaviorConfig::default(),
    );
    let error = connector(LocalConnector::ID)
        .policy_behavior(&wrong_schema)
        .expect_err("connector schema mismatch must fail");
    assert!(error.to_string().contains("schema version mismatch"));
}

#[test]
fn onedrive_semantics_are_validated_inside_the_connector() {
    let connector = connector(OneDriveConnector::ID);
    let missing_site = super::test_support::connection_config(
        OneDriveConnector::ID,
        1,
        onedrive_config(
            ProviderResumableUploadStrategy::ServerRelay,
            OneDriveAccountMode::SharepointSite,
        ),
    );
    let error = connector
        .validate_connector_config(&missing_site)
        .expect_err("SharePoint mode without site id must fail");
    assert!(error.to_string().contains("requires site_id"));

    let mut group = onedrive_config(
        ProviderResumableUploadStrategy::ServerRelay,
        OneDriveAccountMode::GroupDrive,
    );
    group.group_id = Some("group-id".to_string());
    assert!(
        connector
            .validate_connector_config(&super::test_support::connection_config(
                OneDriveConnector::ID,
                1,
                group,
            ))
            .is_ok()
    );
}

#[test]
fn upload_transport_is_resolved_by_connector_owned_typed_config() {
    let cases = [
        (
            LocalConnector::ID,
            policy(LocalConnector::ID, local_config("./data/uploads")),
            StorageConnectorUploadTransport::Local,
        ),
        (
            S3Connector::ID,
            policy(
                S3Connector::ID,
                s3_config(ObjectStorageUploadStrategy::Presigned),
            ),
            StorageConnectorUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::Presigned),
        ),
        (
            SftpConnector::ID,
            policy(SftpConnector::ID, sftp_config()),
            StorageConnectorUploadTransport::Sftp,
        ),
        (
            AzureBlobConnector::ID,
            policy(
                AzureBlobConnector::ID,
                azure_config(ObjectStorageUploadStrategy::RelayStream),
            ),
            StorageConnectorUploadTransport::ObjectStorage(
                ObjectStorageUploadStrategy::RelayStream,
            ),
        ),
        (
            TencentCosConnector::ID,
            policy(
                TencentCosConnector::ID,
                cos_config(ObjectStorageUploadStrategy::Presigned),
            ),
            StorageConnectorUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::Presigned),
        ),
        (
            RemoteConnector::ID,
            policy(
                RemoteConnector::ID,
                remote_config(RemoteUploadStrategy::Presigned),
            ),
            StorageConnectorUploadTransport::Remote(RemoteUploadStrategy::Presigned),
        ),
        (
            OneDriveConnector::ID,
            policy(
                OneDriveConnector::ID,
                onedrive_config(
                    ProviderResumableUploadStrategy::FrontendDirect,
                    OneDriveAccountMode::Personal,
                ),
            ),
            StorageConnectorUploadTransport::ProviderResumable(
                ProviderResumableUploadStrategy::FrontendDirect,
            ),
        ),
    ];

    for (id, policy, expected) in cases {
        assert_eq!(connector(id).upload_transport(&policy).unwrap(), expected);
    }
}

#[test]
fn upload_transport_boundaries_preserve_chunk_and_direct_semantics() {
    let mut policy = policy(
        S3Connector::ID,
        s3_config(ObjectStorageUploadStrategy::Presigned),
    );
    policy.chunk_size = 5_242_880;
    let transport = connector(S3Connector::ID)
        .upload_transport(&policy)
        .unwrap();
    assert_eq!(
        transport.resolve_init_mode(&policy, 5_242_880),
        aster_drive_model::types::UploadMode::Presigned
    );
    assert_eq!(
        transport.resolve_init_mode(&policy, 5_242_881),
        aster_drive_model::types::UploadMode::PresignedMultipart
    );
    assert!(!transport.supports_streaming_direct_upload(&policy, 1));

    let remote = StorageConnectorUploadTransport::Remote(RemoteUploadStrategy::Presigned);
    assert!(!remote.supports_streaming_direct_upload(&policy, 0));
    assert!(remote.supports_streaming_direct_upload(&policy, 1));
}
