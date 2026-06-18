use super::*;
use crate::api::api_error_code::ApiErrorCode;
use crate::storage::connector_descriptor::{
    StorageConnectorAction, StorageConnectorActionKind, StorageConnectorDescriptorProvider,
    StoragePolicyExecutableAction,
};
use chrono::Utc;

use crate::entities::storage_policy;
use crate::types::{
    MicrosoftGraphCloud, OneDriveAccountMode, RemoteUploadStrategy, S3UploadStrategy,
    StoragePolicyOptions, StoredStoragePolicyAllowedTypes, UploadMode,
    parse_storage_policy_options,
};

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
            && field.scope
                == crate::storage::connector_descriptor::StorageConnectorFieldScope::PolicyOptions
            && field.kind
                == crate::storage::connector_descriptor::StorageConnectorFieldKind::Boolean
    }));
}

#[test]
fn onedrive_descriptor_requires_saved_authorized_connection_test() {
    let descriptor = storage_driver_descriptor(DriverType::OneDrive);

    assert_eq!(
        descriptor.authorization_provider.as_deref(),
        Some("microsoft_graph")
    );
    assert!(!descriptor.actions.iter().any(|action| {
        action.action == StorageConnectorAction::TestDraftConnection
            && action.kind == StorageConnectorActionKind::ConnectionTest
    }));
    let saved_connection_test = descriptor
        .actions
        .iter()
        .find(|action| {
            action.action == StorageConnectorAction::TestSavedConnection
                && action.kind == StorageConnectorActionKind::ConnectionTest
        })
        .expect("saved connection test action");
    assert!(saved_connection_test.requires_saved_policy);
    assert!(saved_connection_test.requires_authorization);
    assert!(descriptor.requires_authorization);
    assert!(descriptor.upload_workflows.stream_upload);
    assert!(!descriptor.upload_workflows.object_multipart_upload);
    assert!(descriptor.upload_workflows.provider_resumable_upload);
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
fn tencent_cos_descriptor_exposes_cors_action() {
    let descriptor = storage_driver_descriptor(DriverType::TencentCos);

    assert!(descriptor.actions.iter().any(|action| action.action
        == StorageConnectorAction::ConfigureTencentCosCors
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
fn connector_action_endpoint_gate_rejects_non_endpoint_actions() {
    let onedrive = OneDriveConnector::storage_connector_descriptor();

    assert!(onedrive.actions.iter().any(|action| {
        action.action == StorageConnectorAction::StartAuthorization
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
    assert!(!presigned_download_enabled(&policy));
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
}

#[test]
fn onedrive_uses_server_relay_without_presigned_or_multipart_tracking() {
    let policy = mock_policy(DriverType::OneDrive, 1024, "{}");
    let transport = resolve_policy_upload_transport(&policy);

    assert_eq!(transport, StorageConnectorUploadTransport::OneDrive);
    assert_eq!(
        transport.resolve_init_mode(&policy, 1024),
        UploadMode::Direct
    );
    assert_eq!(
        transport.resolve_init_mode(&policy, 1025),
        UploadMode::Chunked
    );
    assert!(transport.supports_streaming_direct_upload(&policy, 1024));
    assert!(!transport.uses_relay_multipart_tracking());
}
