use super::*;
use crate::api::api_error_code::ApiErrorCode;
use crate::config::DatabaseConfig;
use crate::storage::connector_descriptor::{
    StorageConnectorActionKind, StorageConnectorAffordanceAction,
    StorageConnectorDescriptorProvider, StorageConnectorFieldScope, StoragePolicyExecutableAction,
};
use chrono::Utc;
use migration::Migrator;
use sea_orm::ActiveValue::Set;

use crate::entities::storage_policy;
use crate::types::{
    MicrosoftGraphCloud, OneDriveAccountMode, RemoteUploadStrategy, S3UploadStrategy,
    StoragePolicyOptions, StoredStoragePolicyAllowedTypes, UploadMode,
    parse_storage_policy_options,
};

const OBJECT_STORAGE_LARGE_UPLOAD_SIZE: i64 = 5_242_881;
const ONEDRIVE_MAX_SIMPLE_UPLOAD_SIZE: u64 = 250_000_000;

async fn setup_connector_test_db() -> sea_orm::DatabaseConnection {
    let db = crate::db::connect_with_metrics(
        &DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            pool_size: 1,
            retry_count: 0,
        },
        crate::metrics_core::NoopMetrics::arc(),
    )
    .await
    .expect("connector test DB should connect");
    Migrator::up(&db, None)
        .await
        .expect("connector test migrations should succeed");
    db
}

async fn create_saved_connector_policy(
    db: &sea_orm::DatabaseConnection,
    driver_type: DriverType,
) -> storage_policy::Model {
    let now = Utc::now();
    crate::db::repository::policy_repo::create(
        db,
        storage_policy::ActiveModel {
            name: Set(format!("Saved {}", driver_type.as_str())),
            driver_type: Set(driver_type),
            endpoint: Set(match driver_type {
                DriverType::AzureBlob => "https://acct.blob.core.windows.net".to_string(),
                DriverType::S3 | DriverType::TencentCos => "https://s3.example.test".to_string(),
                _ => String::new(),
            }),
            bucket: Set("archive".to_string()),
            access_key: Set("saved-access".to_string()),
            secret_key: Set("saved-secret".to_string()),
            base_path: Set("tenant-a".to_string()),
            remote_node_id: Set(None),
            max_file_size: Set(0),
            allowed_types: Set(StoredStoragePolicyAllowedTypes::empty()),
            options: Set(crate::types::StoredStoragePolicyOptions::empty()),
            is_default: Set(false),
            chunk_size: Set(5_242_880),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .expect("saved connector policy should insert")
}

fn draft_connection(driver_type: DriverType) -> StorageConnectorConnectionInput {
    StorageConnectorConnectionInput {
        driver_type,
        endpoint: match driver_type {
            DriverType::AzureBlob => "https://acct.blob.core.windows.net".to_string(),
            DriverType::S3 | DriverType::TencentCos => "https://s3.example.test".to_string(),
            _ => String::new(),
        },
        bucket: "archive".to_string(),
        access_key: String::new(),
        secret_key: String::new(),
        base_path: "tenant-b".to_string(),
        remote_node_id: None,
        options: StoragePolicyOptions::default(),
    }
}

async fn assert_saved_credentials_merge_for_driver(driver_type: DriverType) {
    let db = setup_connector_test_db().await;
    let saved = create_saved_connector_policy(&db, driver_type).await;

    let merged = common::merge_saved_static_credentials_for_draft(
        &db,
        Some(saved.id),
        draft_connection(driver_type),
        "draft storage policy connection test",
    )
    .await
    .expect("blank draft credentials should merge from saved policy");

    assert_eq!(merged.access_key, "saved-access");
    assert_eq!(merged.secret_key, "saved-secret");
}

async fn assert_saved_credentials_driver_mismatch_for_driver(driver_type: DriverType) {
    let db = setup_connector_test_db().await;
    let saved = create_saved_connector_policy(&db, DriverType::Local).await;

    let error = common::merge_saved_static_credentials_for_draft(
        &db,
        Some(saved.id),
        draft_connection(driver_type),
        "draft storage policy connection test",
    )
    .await
    .expect_err("saved policy with a different driver should be rejected");

    assert_eq!(
        error.api_error_code(),
        ApiErrorCode::PolicyActionParameterInvalid
    );
}

#[tokio::test]
async fn s3_draft_connection_can_merge_saved_credentials() {
    assert!(S3Connector::supports_saved_draft_credentials());
    assert_saved_credentials_merge_for_driver(DriverType::S3).await;
}

#[tokio::test]
async fn s3_draft_connection_rejects_saved_credential_driver_mismatch() {
    assert!(S3Connector::supports_saved_draft_credentials());
    assert_saved_credentials_driver_mismatch_for_driver(DriverType::S3).await;
}

#[tokio::test]
async fn azure_blob_draft_connection_can_merge_saved_credentials() {
    assert!(AzureBlobConnector::supports_saved_draft_credentials());
    assert_saved_credentials_merge_for_driver(DriverType::AzureBlob).await;
}

#[tokio::test]
async fn azure_blob_draft_connection_rejects_saved_credential_driver_mismatch() {
    assert!(AzureBlobConnector::supports_saved_draft_credentials());
    assert_saved_credentials_driver_mismatch_for_driver(DriverType::AzureBlob).await;
}

#[test]
fn descriptors_cover_every_storage_driver() {
    let descriptors = list_storage_driver_descriptors();

    assert_eq!(descriptors.len(), 6);
    for driver_type in [
        DriverType::Local,
        DriverType::S3,
        DriverType::AzureBlob,
        DriverType::TencentCos,
        DriverType::Remote,
        DriverType::OneDrive,
    ] {
        assert!(
            descriptors
                .iter()
                .any(|descriptor| descriptor.driver_type == driver_type),
            "missing descriptor for {driver_type:?}"
        );
    }
}

#[test]
fn descriptors_expose_connector_owned_ui_metadata() {
    let descriptors = list_storage_driver_descriptors();

    for descriptor in descriptors {
        assert!(
            !descriptor.ui.label_key.trim().is_empty(),
            "{:?} missing label key",
            descriptor.driver_type
        );
        assert!(
            !descriptor.ui.description_key.trim().is_empty(),
            "{:?} missing description key",
            descriptor.driver_type
        );
        assert!(
            !descriptor.ui.helper_key.trim().is_empty(),
            "{:?} missing create helper key",
            descriptor.driver_type
        );
        assert!(
            !descriptor.ui.edit_context_key.trim().is_empty(),
            "{:?} missing edit context key",
            descriptor.driver_type
        );
        assert!(
            descriptor.ui.icon_src.is_some() || descriptor.ui.icon_name.is_some(),
            "{:?} should declare a visual affordance",
            descriptor.driver_type
        );
    }

    let local = storage_driver_descriptor(DriverType::Local);
    assert_eq!(local.ui.label_key, "driver_type_local");
    assert_eq!(
        local.ui.config_step_title_key,
        "policy_wizard_step_local_title"
    );
    assert_eq!(
        local.ui.config_step_description_key,
        "policy_wizard_step_local_desc"
    );
    assert_eq!(local.ui.base_path_empty_display, "./data");
    assert_eq!(local.ui.base_path_placeholder, "./data");

    let azure = storage_driver_descriptor(DriverType::AzureBlob);
    assert_eq!(azure.ui.label_key, "driver_type_azure_blob");
    assert_eq!(azure.ui.helper_key, "policy_wizard_azure_blob_helper");
    assert_eq!(
        azure.ui.config_step_description_key,
        "policy_wizard_step_azure_blob_connection_desc"
    );
    assert_eq!(
        azure.ui.edit_context_key,
        "policy_edit_context_azure_blob_desc"
    );
    assert_eq!(azure.ui.base_path_empty_display, "core:root");

    let onedrive = storage_driver_descriptor(DriverType::OneDrive);
    assert_eq!(onedrive.ui.label_key, "driver_type_onedrive");
    assert_eq!(onedrive.ui.helper_key, "policy_wizard_onedrive_helper");
    assert_eq!(
        onedrive.ui.config_step_title_key,
        "policy_wizard_step_onedrive_title"
    );
}

#[test]
fn connector_registry_covers_every_builtin_storage_driver() {
    for driver_type in [
        DriverType::Local,
        DriverType::S3,
        DriverType::AzureBlob,
        DriverType::TencentCos,
        DriverType::Remote,
        DriverType::OneDrive,
    ] {
        let connector = connector_for(driver_type).expect("registered connector");

        assert_eq!(connector.driver_type, driver_type);
        assert_eq!(connector.connector.descriptor().driver_type, driver_type);
    }
}

#[test]
fn local_descriptor_declares_content_dedup_policy_option() {
    let descriptor = storage_driver_descriptor(DriverType::Local);

    assert!(descriptor.fields.iter().any(|field| {
        field.name == "content_dedup"
            && field.scope == StorageConnectorFieldScope::PolicyOptions
            && field.kind
                == crate::storage::connector_descriptor::StorageConnectorFieldKind::Boolean
    }));
}

#[test]
fn transfer_strategy_policy_options_are_declared_by_descriptors() {
    let s3 = storage_driver_descriptor(DriverType::S3);
    assert!(has_policy_option(&s3, "s3_upload_strategy"));
    assert!(has_policy_option(&s3, "s3_download_strategy"));
    assert!(has_policy_option(&s3, "s3_path_style"));

    let azure_blob = storage_driver_descriptor(DriverType::AzureBlob);
    assert!(has_policy_option(&azure_blob, "s3_upload_strategy"));
    assert!(has_policy_option(&azure_blob, "s3_download_strategy"));
    assert!(!has_policy_option(&azure_blob, "s3_path_style"));

    let remote = storage_driver_descriptor(DriverType::Remote);
    assert!(has_policy_option(&remote, "remote_download_strategy"));
    assert!(has_policy_option(&remote, "remote_upload_strategy"));
}

#[test]
fn object_storage_connection_field_display_metadata_is_connector_owned() {
    let s3 = storage_driver_descriptor(DriverType::S3);
    let s3_endpoint = field(&s3, "endpoint");
    assert_eq!(s3_endpoint.label_key, "endpoint");
    assert_eq!(
        s3_endpoint.placeholder.as_deref(),
        Some("https://s3.amazonaws.com")
    );
    assert_eq!(s3_endpoint.help_key.as_deref(), Some("s3_endpoint_hint"));
    let s3_path_style = field(&s3, "s3_path_style");
    assert_eq!(s3_path_style.label_key, "s3_path_style");
    assert_eq!(
        s3_path_style.help_key.as_deref(),
        Some("s3_path_style_desc")
    );
    assert_eq!(
        s3_path_style.visible_when_driver_types,
        vec![DriverType::S3]
    );

    let azure_blob = storage_driver_descriptor(DriverType::AzureBlob);
    assert_eq!(
        field(&azure_blob, "endpoint").placeholder.as_deref(),
        Some("https://<account>.blob.core.windows.net")
    );
    assert_eq!(
        field(&azure_blob, "endpoint").help_key.as_deref(),
        Some("azure_blob_endpoint_hint")
    );
    assert_eq!(
        field(&azure_blob, "endpoint")
            .invalid_protocol_message_key
            .as_deref(),
        Some("azure_blob_endpoint_protocol_required_error")
    );
    assert_eq!(
        field(&azure_blob, "access_key").label_key,
        "azure_blob_account_name"
    );
    assert!(field(&azure_blob, "access_key").trim_on_blur);
    assert_eq!(
        field(&azure_blob, "secret_key").label_key,
        "azure_blob_account_key"
    );
    assert_eq!(
        field(&azure_blob, "bucket").required_message_key.as_deref(),
        Some("policy_wizard_container_required")
    );

    let tencent_cos = storage_driver_descriptor(DriverType::TencentCos);
    assert_eq!(
        field(&tencent_cos, "endpoint").placeholder.as_deref(),
        Some("https://<bucket-appid>.cos.<region>.myqcloud.com")
    );
    assert_eq!(
        field(&tencent_cos, "endpoint").help_key.as_deref(),
        Some("cos_endpoint_hint")
    );
    assert!(!has_policy_option(&tencent_cos, "s3_path_style"));
}

#[test]
fn s3_descriptor_declares_connector_owned_endpoint_driver_recommendation() {
    let s3 = storage_driver_descriptor(DriverType::S3);
    let recommendation = s3
        .driver_recommendations
        .iter()
        .find(|recommendation| recommendation.target_driver_type == DriverType::TencentCos)
        .expect("S3 connector should recommend the specialized Tencent COS driver");

    assert!(
        recommendation
            .endpoint_host_rules
            .iter()
            .any(|rule| rule.equals.as_deref() == Some("myqcloud.com"))
    );
    assert!(
        recommendation
            .endpoint_host_rules
            .iter()
            .any(|rule| rule.ends_with.as_deref() == Some(".myqcloud.com"))
    );

    let tencent_cos = storage_driver_descriptor(DriverType::TencentCos);
    assert!(tencent_cos.driver_recommendations.is_empty());
}

#[test]
fn onedrive_descriptor_requires_saved_authorized_connection_test() {
    let descriptor = storage_driver_descriptor(DriverType::OneDrive);

    assert_eq!(
        descriptor.authorization_provider.as_deref(),
        Some("microsoft_graph")
    );
    assert!(!descriptor.actions.iter().any(|action| {
        action.affordance_action == Some(StorageConnectorAffordanceAction::TestDraftConnection)
            && action.kind == StorageConnectorActionKind::ConnectionTest
    }));
    let saved_connection_test = descriptor
        .actions
        .iter()
        .find(|action| {
            action.affordance_action == Some(StorageConnectorAffordanceAction::TestSavedConnection)
                && action.kind == StorageConnectorActionKind::ConnectionTest
        })
        .expect("saved connection test action");
    assert!(saved_connection_test.requires_saved_policy);
    assert!(saved_connection_test.requires_authorization);
    assert!(descriptor.requires_authorization);
    assert!(descriptor.upload_workflows.stream_upload);
    assert!(!descriptor.upload_workflows.object_multipart_upload);
    assert!(descriptor.upload_workflows.provider_resumable_upload);
    assert!(
        !descriptor
            .upload_workflows
            .frontend_direct_provider_resumable_upload
    );
    let upload_capabilities = descriptor
        .upload_workflows
        .provider_resumable_upload_capabilities
        .as_ref()
        .expect("OneDrive should describe provider-native upload session semantics");
    assert_eq!(upload_capabilities.provider, "microsoft_graph");
    assert_eq!(
        upload_capabilities.session_label,
        "Microsoft Graph upload session"
    );
    assert_eq!(upload_capabilities.min_fragment_size, 320 * 1024);
    assert_eq!(upload_capabilities.fragment_alignment, 320 * 1024);
    assert_eq!(upload_capabilities.default_fragment_size, 10 * 1024 * 1024);
    assert_eq!(upload_capabilities.max_fragment_size, 50 * 1024 * 1024);
    assert_eq!(
        upload_capabilities.max_simple_upload_size,
        Some(ONEDRIVE_MAX_SIMPLE_UPLOAD_SIZE)
    );
    assert!(!upload_capabilities.frontend_direct_upload);
    assert!(upload_capabilities.implicit_completion);
    assert!(!upload_capabilities.abort_supported);
    assert!(!upload_capabilities.status_query_supported);
}

#[test]
fn onedrive_prepare_connection_for_storage_clears_legacy_policy_key_fields() {
    let prepared = prepare_connection_for_storage(
        StorageConnectorConnectionInput {
            driver_type: DriverType::OneDrive,
            endpoint: String::new(),
            bucket: String::new(),
            access_key: "legacy-client-id".to_string(),
            secret_key: "legacy-client-secret".to_string(),
            base_path: "docs".to_string(),
            remote_node_id: None,
            options: StoragePolicyOptions::default(),
        },
        &StorageConnectorApplicationConfigInput {
            microsoft_graph: Some(crate::storage::MicrosoftGraphApplicationConfigInput {
                client_id: Some("metadata-client-id".to_string()),
                client_secret: Some("metadata-client-secret".to_string()),
                ..Default::default()
            }),
        },
    )
    .expect("OneDrive app config should be accepted by connector");

    assert_eq!(prepared.access_key, "");
    assert_eq!(prepared.secret_key, "");
    assert_eq!(prepared.base_path, "docs");
}

#[test]
fn non_onedrive_connectors_reject_microsoft_graph_application_config() {
    let error = prepare_connection_for_storage(
        StorageConnectorConnectionInput {
            driver_type: DriverType::S3,
            endpoint: "https://s3.example.test".to_string(),
            bucket: "bucket".to_string(),
            access_key: "access".to_string(),
            secret_key: "secret".to_string(),
            base_path: String::new(),
            remote_node_id: None,
            options: StoragePolicyOptions::default(),
        },
        &StorageConnectorApplicationConfigInput {
            microsoft_graph: Some(crate::storage::MicrosoftGraphApplicationConfigInput {
                client_id: Some("client-id".to_string()),
                ..Default::default()
            }),
        },
    )
    .expect_err("S3 connector should reject Microsoft Graph app config");

    assert!(
        error
            .to_string()
            .contains("application credential config is not valid for s3")
    );
}

#[test]
fn credential_validation_support_is_declared_by_connector_action() {
    assert_eq!(
        ensure_storage_credential_validation_supported(
            DriverType::OneDrive,
            StorageCredentialProvider::MicrosoftGraph,
        )
        .unwrap(),
        StorageCredentialKind::OauthDelegated
    );

    let s3_error = ensure_storage_credential_validation_supported(
        DriverType::S3,
        StorageCredentialProvider::MicrosoftGraph,
    )
    .unwrap_err();
    assert!(
        s3_error
            .to_string()
            .contains("is not supported for s3 storage policies")
    );

    let provider_error = ensure_storage_credential_validation_supported(
        DriverType::OneDrive,
        StorageCredentialProvider::GoogleDrive,
    )
    .unwrap_err();
    assert!(
        provider_error
            .to_string()
            .contains("validation provider 'google_drive' is not supported")
    );
}

#[test]
fn runtime_credential_requirement_is_connector_owned() {
    for driver_type in [
        DriverType::Local,
        DriverType::S3,
        DriverType::AzureBlob,
        DriverType::TencentCos,
        DriverType::Remote,
    ] {
        assert_eq!(
            runtime_credential_requirement(driver_type),
            None,
            "{driver_type:?} should not require delegated runtime credential loading"
        );
    }

    let onedrive = runtime_credential_requirement(DriverType::OneDrive)
        .expect("OneDrive should declare Microsoft Graph runtime credentials");
    assert_eq!(onedrive.provider, StorageCredentialProvider::MicrosoftGraph);
    assert_eq!(
        onedrive.credential_kind,
        StorageCredentialKind::OauthDelegated
    );
    assert!(onedrive.requires_application_config);
    assert!(onedrive.requires_authorization);
}

#[test]
fn tencent_cos_descriptor_exposes_cors_action() {
    let descriptor = storage_driver_descriptor(DriverType::TencentCos);

    assert!(descriptor.actions.iter().any(|action| action.policy_action
        == Some(StoragePolicyExecutableAction::ConfigureTencentCosCors)
        && action.kind == StorageConnectorActionKind::PolicyAction
        && action.mutates_remote_state));
    assert!(descriptor.capabilities.s3_transfer_strategy);
}

#[test]
fn storage_native_support_is_declared_by_connector_capabilities() {
    assert!(!storage_connector_supports_native_thumbnail(DriverType::S3));
    assert!(!storage_connector_supports_native_media_metadata(
        DriverType::S3
    ));
    assert!(storage_connector_supports_native_thumbnail(
        DriverType::TencentCos
    ));
    assert!(storage_connector_supports_native_media_metadata(
        DriverType::TencentCos
    ));
}

#[test]
fn unsupported_storage_native_media_metadata_is_rejected() {
    let options = StoragePolicyOptions {
        storage_native_processing_enabled: Some(true),
        storage_native_media_metadata_enabled: Some(true),
        media_metadata_extensions: vec!["mp4".to_string()],
        ..Default::default()
    };

    let error = common::ensure_storage_native_processing_supported(
        storage_driver_descriptor(DriverType::S3),
        &options,
    )
    .unwrap_err();

    assert_eq!(
        error.api_error_code(),
        ApiErrorCode::PolicyNativeMediaMetadataUnsupported
    );
    assert!(
        error
            .to_string()
            .contains("storage-native media metadata processing")
    );
}

#[test]
fn local_connector_normalizes_connection_paths() {
    let (endpoint, bucket) =
        LocalConnector::normalize_connection_fields("  /data/uploads  ", "  ").unwrap();

    assert_eq!(endpoint, "/data/uploads");
    assert_eq!(bucket, "");
}

#[test]
fn azure_blob_connector_maps_endpoint_and_container_errors() {
    let endpoint_error = AzureBlobConnector::normalize_connection_fields("", "photos").unwrap_err();
    assert_eq!(
        endpoint_error.api_error_code(),
        ApiErrorCode::PolicyStorageEndpointInvalid
    );

    let container_error =
        AzureBlobConnector::normalize_connection_fields("https://acct.blob.core.windows.net", "")
            .unwrap_err();
    assert_eq!(
        container_error.api_error_code(),
        ApiErrorCode::PolicyStorageBucketRequired
    );

    let invalid_endpoint_error =
        AzureBlobConnector::normalize_connection_fields("acct.blob.core.windows.net", "photos")
            .unwrap_err();
    assert_eq!(
        invalid_endpoint_error.api_error_code(),
        ApiErrorCode::PolicyStorageEndpointInvalid
    );
}

#[test]
fn onedrive_options_are_rejected_for_non_onedrive_connector() {
    let options = StoragePolicyOptions {
        onedrive_account_mode: Some(OneDriveAccountMode::WorkOrSchool),
        onedrive_drive_id: Some("drive".to_string()),
        onedrive_root_item_id: Some("root".to_string()),
        ..Default::default()
    };

    let error = common::ensure_onedrive_options_absent(&options).unwrap_err();

    assert_eq!(
        error.api_error_code(),
        ApiErrorCode::PolicyOneDriveOptionsUnsupported
    );
    assert!(
        error
            .to_string()
            .contains("OneDrive options are only valid for OneDrive")
    );
}

#[test]
fn onedrive_connector_accepts_automatic_default_drive() {
    let options = StoragePolicyOptions {
        onedrive_account_mode: Some(OneDriveAccountMode::WorkOrSchool),
        ..Default::default()
    };

    common::validate_onedrive_options(&options)
        .expect("work or school OneDrive resolves the default drive during authorization");
}

#[test]
fn onedrive_connector_requires_account_mode() {
    let options = StoragePolicyOptions {
        onedrive_root_item_id: Some("root".to_string()),
        ..Default::default()
    };

    let error = common::validate_onedrive_options(&options).unwrap_err();

    assert_eq!(
        error.api_error_code(),
        ApiErrorCode::PolicyOneDriveAccountModeRequired
    );
    assert!(
        error
            .to_string()
            .contains("OneDrive storage policies require onedrive_account_mode")
    );
}

#[test]
fn onedrive_connector_rejects_personal_china_cloud() {
    let options = StoragePolicyOptions {
        onedrive_cloud: Some(MicrosoftGraphCloud::China),
        onedrive_account_mode: Some(OneDriveAccountMode::Personal),
        ..Default::default()
    };

    let error = common::validate_onedrive_options(&options).unwrap_err();

    assert_eq!(
        error.api_error_code(),
        ApiErrorCode::PolicyOneDrivePersonalChinaCloudUnsupported
    );
    assert!(error.to_string().contains("global Microsoft Graph cloud"));
}

#[test]
fn onedrive_connector_sharepoint_site_requires_site_id_without_drive_id() {
    let options = StoragePolicyOptions {
        onedrive_account_mode: Some(OneDriveAccountMode::SharepointSite),
        ..Default::default()
    };

    let error = common::validate_onedrive_options(&options).unwrap_err();

    assert_eq!(
        error.api_error_code(),
        ApiErrorCode::PolicyOneDriveSharePointSiteRequired
    );
    assert!(error.to_string().contains("onedrive_site_id"));
}

#[test]
fn onedrive_connector_group_drive_requires_group_id_without_drive_id() {
    let options = StoragePolicyOptions {
        onedrive_account_mode: Some(OneDriveAccountMode::GroupDrive),
        ..Default::default()
    };

    let error = common::validate_onedrive_options(&options).unwrap_err();

    assert_eq!(
        error.api_error_code(),
        ApiErrorCode::PolicyOneDriveGroupRequired
    );
    assert!(error.to_string().contains("onedrive_group_id"));
}

#[test]
fn onedrive_connector_modes_reject_other_mode_target_ids() {
    let options = StoragePolicyOptions {
        onedrive_account_mode: Some(OneDriveAccountMode::SharepointSite),
        onedrive_site_id: Some("site".to_string()),
        onedrive_group_id: Some("group".to_string()),
        ..Default::default()
    };

    let error = common::validate_onedrive_options(&options).unwrap_err();

    assert_eq!(
        error.api_error_code(),
        ApiErrorCode::PolicyOneDriveOptionsUnsupported
    );
    assert!(error.to_string().contains("onedrive_group_id"));

    let options = StoragePolicyOptions {
        onedrive_account_mode: Some(OneDriveAccountMode::GroupDrive),
        onedrive_site_id: Some("site".to_string()),
        onedrive_group_id: Some("group".to_string()),
        ..Default::default()
    };

    let error = common::validate_onedrive_options(&options).unwrap_err();

    assert_eq!(
        error.api_error_code(),
        ApiErrorCode::PolicyOneDriveOptionsUnsupported
    );
    assert!(error.to_string().contains("onedrive_site_id"));
}

#[test]
fn onedrive_connector_personal_and_work_modes_reject_site_and_group_ids() {
    for mode in [
        OneDriveAccountMode::Personal,
        OneDriveAccountMode::WorkOrSchool,
    ] {
        let options = StoragePolicyOptions {
            onedrive_account_mode: Some(mode),
            onedrive_site_id: Some("site".to_string()),
            ..Default::default()
        };
        let error = common::validate_onedrive_options(&options).unwrap_err();
        assert_eq!(
            error.api_error_code(),
            ApiErrorCode::PolicyOneDriveOptionsUnsupported
        );
        assert!(error.to_string().contains("onedrive_site_id"));

        let options = StoragePolicyOptions {
            onedrive_account_mode: Some(mode),
            onedrive_group_id: Some("group".to_string()),
            ..Default::default()
        };
        let error = common::validate_onedrive_options(&options).unwrap_err();
        assert_eq!(
            error.api_error_code(),
            ApiErrorCode::PolicyOneDriveOptionsUnsupported
        );
        assert!(error.to_string().contains("onedrive_group_id"));
    }
}

#[test]
fn connector_action_endpoint_gate_rejects_non_endpoint_actions() {
    let onedrive = OneDriveConnector::storage_connector_descriptor();

    assert!(onedrive.actions.iter().any(|action| {
        action.affordance_action == Some(StorageConnectorAffordanceAction::StartAuthorization)
            && action.kind == StorageConnectorActionKind::Authorization
    }));
    assert!(
        common::unsupported_policy_action_error(
            onedrive,
            StoragePolicyExecutableAction::ConfigureTencentCosCors
        )
        .to_string()
        .contains("not supported")
    );
    assert!(
        TencentCosConnector::storage_connector_supports_policy_action(
            StoragePolicyExecutableAction::ConfigureTencentCosCors
        )
    );
    assert!(
        !OneDriveConnector::storage_connector_supports_policy_action(
            StoragePolicyExecutableAction::ConfigureTencentCosCors
        )
    );
}

fn mock_policy(driver_type: DriverType, chunk_size: i64, options: &str) -> storage_policy::Model {
    storage_policy::Model {
        id: 1,
        name: "test".to_string(),
        driver_type,
        endpoint: String::new(),
        bucket: String::new(),
        access_key: String::new(),
        secret_key: String::new(),
        base_path: String::new(),
        remote_node_id: None,
        max_file_size: 0,
        allowed_types: StoredStoragePolicyAllowedTypes::empty(),
        options: options.to_string().into(),
        is_default: false,
        chunk_size,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn has_policy_option(descriptor: &crate::storage::StorageConnectorDescriptor, name: &str) -> bool {
    descriptor
        .fields
        .iter()
        .any(|field| field.scope == StorageConnectorFieldScope::PolicyOptions && field.name == name)
}

fn field<'a>(
    descriptor: &'a crate::storage::StorageConnectorDescriptor,
    name: &str,
) -> &'a crate::storage::StorageConnectorFieldDescriptor {
    descriptor
        .fields
        .iter()
        .find(|field| field.name == name)
        .unwrap_or_else(|| panic!("descriptor field '{name}' should exist"))
}

#[test]
fn local_policy_resolves_direct_and_chunked_modes() {
    let policy = mock_policy(DriverType::Local, 1024, "{}");
    let transport = resolve_policy_upload_transport(&policy);

    assert_eq!(transport, StorageConnectorUploadTransport::Local);
    assert_eq!(
        transport.resolve_init_mode(&policy, 100),
        UploadMode::Direct
    );
    assert_eq!(
        transport.resolve_init_mode(&policy, 2048),
        UploadMode::Chunked
    );
    assert!(!transport.supports_streaming_direct_upload(&policy, 100));
    assert!(!transport.uses_relay_multipart_tracking());
    assert_eq!(transport.opaque_blob_hash_prefix(), None);
    assert_eq!(
        transport.chunked_completion(),
        StorageConnectorChunkedCompletion::AssembleLocalChunks
    );
    assert!(!presigned_download_enabled(&policy));
}

#[test]
fn non_local_upload_transports_expose_opaque_blob_hash_prefix() {
    for transport in [
        StorageConnectorUploadTransport::ObjectStorage(S3UploadStrategy::RelayStream),
        StorageConnectorUploadTransport::ObjectStorage(S3UploadStrategy::Presigned),
        StorageConnectorUploadTransport::Remote(RemoteUploadStrategy::RelayStream),
        StorageConnectorUploadTransport::Remote(RemoteUploadStrategy::Presigned),
        StorageConnectorUploadTransport::StreamUpload,
    ] {
        assert!(
            transport.opaque_blob_hash_prefix().is_some(),
            "{transport:?} must declare an opaque blob hash prefix"
        );
    }
}

#[test]
fn presigned_download_policy_is_connector_owned() {
    let s3 = mock_policy(
        DriverType::S3,
        1024,
        r#"{"s3_download_strategy":"presigned"}"#,
    );
    let remote = mock_policy(
        DriverType::Remote,
        1024,
        r#"{"remote_download_strategy":"presigned"}"#,
    );
    let relay_s3 = mock_policy(
        DriverType::S3,
        1024,
        r#"{"s3_download_strategy":"relay_stream"}"#,
    );

    assert!(presigned_download_enabled(&s3));
    assert!(presigned_download_enabled(&remote));
    assert!(!presigned_download_enabled(&relay_s3));
}

#[test]
fn s3_relay_stream_uses_effective_chunk_size_and_relay_tracking() {
    let policy = mock_policy(
        DriverType::S3,
        1_048_576,
        r#"{"s3_upload_strategy":"relay_stream"}"#,
    );
    let transport = resolve_policy_upload_transport(&policy);

    assert_eq!(
        transport,
        StorageConnectorUploadTransport::ObjectStorage(S3UploadStrategy::RelayStream)
    );
    assert_eq!(transport.effective_chunk_size(&policy), 5_242_880);
    assert_eq!(
        transport.resolve_init_mode(&policy, 5_242_880),
        UploadMode::Direct
    );
    assert_eq!(
        transport.resolve_init_mode(&policy, 5_242_881),
        UploadMode::Chunked
    );
    assert!(transport.supports_streaming_direct_upload(&policy, 1024));
    assert!(!transport.supports_streaming_direct_upload(&policy, 5_242_881));
    assert!(transport.uses_relay_multipart_tracking());
    assert_eq!(transport.opaque_blob_hash_prefix(), Some("s3"));
    assert_eq!(
        transport.chunked_completion(),
        StorageConnectorChunkedCompletion::AssembleLocalChunks
    );
}

#[test]
fn s3_presigned_uses_presigned_modes() {
    let policy = mock_policy(
        DriverType::S3,
        1024,
        r#"{"s3_upload_strategy":"presigned"}"#,
    );
    let transport = resolve_policy_upload_transport(&policy);

    assert_eq!(
        transport,
        StorageConnectorUploadTransport::ObjectStorage(S3UploadStrategy::Presigned)
    );
    assert_eq!(
        transport.resolve_init_mode(&policy, 5_242_880),
        UploadMode::Presigned
    );
    assert_eq!(
        transport.resolve_init_mode(&policy, 5_242_881),
        UploadMode::PresignedMultipart
    );
    assert!(!transport.supports_streaming_direct_upload(&policy, 1024));
    assert!(!transport.uses_relay_multipart_tracking());
    assert_eq!(transport.opaque_blob_hash_prefix(), Some("s3"));
    assert_eq!(
        transport.chunked_completion(),
        StorageConnectorChunkedCompletion::AssembleLocalChunks
    );
}

#[test]
fn azure_blob_relay_stream_uses_object_storage_transport_modes() {
    let policy = mock_policy(
        DriverType::AzureBlob,
        1_048_576,
        r#"{"s3_upload_strategy":"relay_stream"}"#,
    );
    let transport = resolve_policy_upload_transport(&policy);

    assert_eq!(
        transport,
        StorageConnectorUploadTransport::ObjectStorage(S3UploadStrategy::RelayStream)
    );
    assert_eq!(transport.effective_chunk_size(&policy), 5_242_880);
    assert_eq!(
        transport.resolve_init_mode(&policy, 5_242_880),
        UploadMode::Direct
    );
    assert_eq!(
        transport.resolve_init_mode(&policy, 5_242_881),
        UploadMode::Chunked
    );
    assert!(transport.supports_streaming_direct_upload(&policy, 1024));
    assert!(!transport.supports_streaming_direct_upload(&policy, 5_242_881));
    assert!(transport.uses_relay_multipart_tracking());
    assert_eq!(
        transport.chunked_completion(),
        StorageConnectorChunkedCompletion::AssembleLocalChunks
    );
}

#[test]
fn azure_blob_presigned_uses_object_storage_presigned_modes() {
    let policy = mock_policy(
        DriverType::AzureBlob,
        1024,
        r#"{"s3_upload_strategy":"presigned"}"#,
    );
    let transport = resolve_policy_upload_transport(&policy);

    assert_eq!(
        transport,
        StorageConnectorUploadTransport::ObjectStorage(S3UploadStrategy::Presigned)
    );
    assert_eq!(
        transport.resolve_init_mode(&policy, 5_242_880),
        UploadMode::Presigned
    );
    assert_eq!(
        transport.resolve_init_mode(&policy, 5_242_881),
        UploadMode::PresignedMultipart
    );
    assert!(!transport.supports_streaming_direct_upload(&policy, 1024));
    assert!(!transport.uses_relay_multipart_tracking());
}

#[test]
fn tencent_cos_presigned_uses_object_storage_presigned_modes() {
    let options = parse_storage_policy_options(r#"{"s3_upload_strategy":"presigned"}"#);
    assert_eq!(
        options.effective_s3_upload_strategy(),
        S3UploadStrategy::Presigned
    );
    let policy = mock_policy(
        DriverType::TencentCos,
        1024,
        r#"{"s3_upload_strategy":"presigned"}"#,
    );
    let transport = resolve_policy_upload_transport(&policy);

    assert_eq!(
        transport,
        StorageConnectorUploadTransport::ObjectStorage(S3UploadStrategy::Presigned)
    );
    assert_eq!(
        transport.resolve_init_mode(&policy, 5_242_880),
        UploadMode::Presigned
    );
    assert_eq!(
        transport.resolve_init_mode(&policy, 5_242_881),
        UploadMode::PresignedMultipart
    );
    assert!(!transport.supports_streaming_direct_upload(&policy, 1024));
    assert!(!transport.uses_relay_multipart_tracking());
}

#[test]
fn remote_relay_stream_uses_direct_and_chunked_modes() {
    let policy = mock_policy(
        DriverType::Remote,
        1024,
        r#"{"remote_upload_strategy":"relay_stream"}"#,
    );
    let transport = resolve_policy_upload_transport(&policy);

    assert_eq!(
        transport,
        StorageConnectorUploadTransport::Remote(RemoteUploadStrategy::RelayStream)
    );
    assert_eq!(
        transport.resolve_init_mode(&policy, 100),
        UploadMode::Direct
    );
    assert_eq!(
        transport.resolve_init_mode(&policy, 2048),
        UploadMode::Chunked
    );
    assert!(transport.supports_streaming_direct_upload(&policy, 100));
    assert!(transport.uses_relay_multipart_tracking());
    assert_eq!(transport.opaque_blob_hash_prefix(), Some("remote"));
    assert_eq!(
        transport.chunked_completion(),
        StorageConnectorChunkedCompletion::RelayLocalChunksToStreamUpload
    );
}

#[test]
fn remote_presigned_keeps_presigned_init_but_allows_server_streaming_fast_path() {
    let policy = mock_policy(
        DriverType::Remote,
        1024,
        r#"{"remote_upload_strategy":"presigned"}"#,
    );
    let transport = resolve_policy_upload_transport(&policy);

    assert_eq!(
        transport,
        StorageConnectorUploadTransport::Remote(RemoteUploadStrategy::Presigned)
    );
    assert_eq!(
        transport.resolve_init_mode(&policy, 100),
        UploadMode::Presigned
    );
    assert_eq!(
        transport.resolve_init_mode(&policy, 2048),
        UploadMode::PresignedMultipart
    );
    assert!(transport.supports_streaming_direct_upload(&policy, 100));
    assert!(!transport.uses_relay_multipart_tracking());
    assert_eq!(transport.opaque_blob_hash_prefix(), Some("remote"));
    assert_eq!(
        transport.chunked_completion(),
        StorageConnectorChunkedCompletion::RelayLocalChunksToStreamUpload
    );
}

#[test]
fn onedrive_uses_server_relay_without_presigned_or_multipart_tracking() {
    let policy = mock_policy(DriverType::OneDrive, 1024, "{}");
    let transport = resolve_policy_upload_transport(&policy);

    assert_eq!(transport, StorageConnectorUploadTransport::StreamUpload);
    assert_eq!(
        transport.resolve_init_mode(&policy, 1024),
        UploadMode::Direct
    );
    assert_eq!(
        transport.resolve_init_mode(&policy, 1025),
        UploadMode::Chunked
    );
    assert!(transport.supports_streaming_direct_upload(&policy, 1024));
    assert!(!transport.supports_streaming_direct_upload(&policy, 0));
    assert!(!transport.uses_relay_multipart_tracking());
    assert_eq!(transport.opaque_blob_hash_prefix(), Some("provider"));
    assert_eq!(
        transport.chunked_completion(),
        StorageConnectorChunkedCompletion::RelayLocalChunksToStreamUpload
    );
}

#[test]
fn upload_workflow_descriptors_match_default_connector_transports() {
    assert_upload_workflow_alignment(
        DriverType::Local,
        "{}",
        ExpectedUploadWorkflow {
            transport: StorageConnectorUploadTransport::Local,
            object_multipart: false,
            provider_resumable: false,
            presigned: false,
            frontend_direct_provider_resumable: false,
            small_mode: UploadMode::Direct,
            large_mode: UploadMode::Chunked,
            chunked_completion: StorageConnectorChunkedCompletion::AssembleLocalChunks,
        },
    );
    for driver_type in [
        DriverType::S3,
        DriverType::AzureBlob,
        DriverType::TencentCos,
    ] {
        assert_upload_workflow_alignment(
            driver_type,
            r#"{"s3_upload_strategy":"relay_stream"}"#,
            ExpectedUploadWorkflow {
                transport: StorageConnectorUploadTransport::ObjectStorage(
                    S3UploadStrategy::RelayStream,
                ),
                object_multipart: true,
                provider_resumable: false,
                presigned: true,
                frontend_direct_provider_resumable: false,
                small_mode: UploadMode::Direct,
                large_mode: UploadMode::Chunked,
                chunked_completion: StorageConnectorChunkedCompletion::AssembleLocalChunks,
            },
        );
    }
    assert_upload_workflow_alignment(
        DriverType::Remote,
        r#"{"remote_upload_strategy":"relay_stream"}"#,
        ExpectedUploadWorkflow {
            transport: StorageConnectorUploadTransport::Remote(RemoteUploadStrategy::RelayStream),
            object_multipart: true,
            provider_resumable: false,
            presigned: true,
            frontend_direct_provider_resumable: false,
            small_mode: UploadMode::Direct,
            large_mode: UploadMode::Chunked,
            chunked_completion: StorageConnectorChunkedCompletion::RelayLocalChunksToStreamUpload,
        },
    );
    assert_upload_workflow_alignment(
        DriverType::OneDrive,
        "{}",
        ExpectedUploadWorkflow {
            transport: StorageConnectorUploadTransport::StreamUpload,
            object_multipart: false,
            provider_resumable: true,
            presigned: false,
            frontend_direct_provider_resumable: false,
            small_mode: UploadMode::Direct,
            large_mode: UploadMode::Chunked,
            chunked_completion: StorageConnectorChunkedCompletion::RelayLocalChunksToStreamUpload,
        },
    );
}

#[test]
fn upload_workflow_descriptors_match_presigned_connector_transports() {
    for driver_type in [
        DriverType::S3,
        DriverType::AzureBlob,
        DriverType::TencentCos,
    ] {
        assert_upload_workflow_alignment(
            driver_type,
            r#"{"s3_upload_strategy":"presigned"}"#,
            ExpectedUploadWorkflow {
                transport: StorageConnectorUploadTransport::ObjectStorage(
                    S3UploadStrategy::Presigned,
                ),
                object_multipart: true,
                provider_resumable: false,
                presigned: true,
                frontend_direct_provider_resumable: false,
                small_mode: UploadMode::Presigned,
                large_mode: UploadMode::PresignedMultipart,
                chunked_completion: StorageConnectorChunkedCompletion::AssembleLocalChunks,
            },
        );
    }
    assert_upload_workflow_alignment(
        DriverType::Remote,
        r#"{"remote_upload_strategy":"presigned"}"#,
        ExpectedUploadWorkflow {
            transport: StorageConnectorUploadTransport::Remote(RemoteUploadStrategy::Presigned),
            object_multipart: true,
            provider_resumable: false,
            presigned: true,
            frontend_direct_provider_resumable: false,
            small_mode: UploadMode::Presigned,
            large_mode: UploadMode::PresignedMultipart,
            chunked_completion: StorageConnectorChunkedCompletion::RelayLocalChunksToStreamUpload,
        },
    );
}

#[derive(Debug, Clone, Copy)]
struct ExpectedUploadWorkflow {
    transport: StorageConnectorUploadTransport,
    object_multipart: bool,
    provider_resumable: bool,
    presigned: bool,
    frontend_direct_provider_resumable: bool,
    small_mode: UploadMode,
    large_mode: UploadMode,
    chunked_completion: StorageConnectorChunkedCompletion,
}

fn assert_upload_workflow_alignment(
    driver_type: DriverType,
    options: &str,
    expected: ExpectedUploadWorkflow,
) {
    let large_upload_size = match expected.transport {
        StorageConnectorUploadTransport::ObjectStorage(_) => OBJECT_STORAGE_LARGE_UPLOAD_SIZE,
        StorageConnectorUploadTransport::Local
        | StorageConnectorUploadTransport::Remote(_)
        | StorageConnectorUploadTransport::StreamUpload => 2048,
    };
    let descriptor = storage_driver_descriptor(driver_type);
    let workflows = descriptor.upload_workflows;
    assert!(
        workflows.simple_upload,
        "{driver_type:?} should expose simple upload because every built-in connector accepts small direct uploads"
    );
    assert!(workflows.simple_upload_capabilities.server_side_relay);
    assert!(workflows.simple_upload_capabilities.policy_limited);
    assert_eq!(
        workflows
            .simple_upload_capabilities
            .max_provider_single_request_size,
        if driver_type == DriverType::OneDrive {
            Some(ONEDRIVE_MAX_SIMPLE_UPLOAD_SIZE)
        } else {
            None
        },
        "{driver_type:?} descriptor simple upload provider limit drifted"
    );
    assert!(
        workflows.stream_upload,
        "{driver_type:?} should expose server-mediated stream upload"
    );
    assert_eq!(
        workflows.object_multipart_upload, expected.object_multipart,
        "{driver_type:?} descriptor object multipart workflow drifted from upload transport"
    );
    assert_eq!(
        workflows.object_multipart_upload_capabilities.is_some(),
        expected.object_multipart,
        "{driver_type:?} descriptor object multipart detail drifted from workflow flag"
    );
    if let Some(capabilities) = workflows.object_multipart_upload_capabilities.as_ref() {
        assert_eq!(
            capabilities.min_part_size,
            crate::types::OBJECT_MULTIPART_MIN_PART_SIZE
        );
        assert!(capabilities.policy_limited_part_size);
        assert!(capabilities.relay_part_upload);
        assert!(capabilities.presigned_part_upload);
        assert!(capabilities.explicit_complete_required);
        assert!(capabilities.abort_supported);
        assert!(capabilities.list_parts_supported);
        assert_eq!(
            capabilities.presigned_part_etag_required,
            driver_type != DriverType::AzureBlob,
            "{driver_type:?} descriptor presigned part ETag requirement drifted"
        );
    }
    assert_eq!(
        workflows.provider_resumable_upload, expected.provider_resumable,
        "{driver_type:?} descriptor provider resumable workflow drifted from upload transport"
    );
    assert_eq!(
        workflows.provider_resumable_upload_capabilities.is_some(),
        expected.provider_resumable,
        "{driver_type:?} descriptor provider resumable detail drifted from provider resumable workflow"
    );
    assert_eq!(
        workflows.presigned_upload, expected.presigned,
        "{driver_type:?} descriptor presigned workflow drifted from upload transport"
    );
    assert_eq!(
        workflows.frontend_direct_provider_resumable_upload,
        expected.frontend_direct_provider_resumable,
        "{driver_type:?} descriptor frontend provider resumable workflow drifted from upload transport"
    );

    let policy = mock_policy(driver_type, 1024, options);
    let transport = resolve_policy_upload_transport(&policy);
    assert_eq!(
        transport, expected.transport,
        "{driver_type:?} runtime upload transport drifted from descriptor expectation"
    );
    assert_eq!(
        transport.resolve_init_mode(&policy, 100),
        expected.small_mode,
        "{driver_type:?} small upload mode is inconsistent with workflow descriptor"
    );
    assert_eq!(
        transport.resolve_init_mode(&policy, large_upload_size),
        expected.large_mode,
        "{driver_type:?} large upload mode is inconsistent with workflow descriptor"
    );
    assert_eq!(
        transport.chunked_completion(),
        expected.chunked_completion,
        "{driver_type:?} chunked completion strategy is inconsistent with upload transport"
    );
    assert_eq!(
        matches!(
            transport,
            StorageConnectorUploadTransport::ObjectStorage(_)
                | StorageConnectorUploadTransport::Remote(_)
        ),
        expected.object_multipart || expected.presigned,
        "{driver_type:?} descriptor should only claim object/presigned workflows for transports with upload sessions"
    );
}
