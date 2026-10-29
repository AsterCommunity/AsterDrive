//! Declarative management UI, capability and credential-status presentation.
use super::*;

impl GoogleDriveConnector {
    pub(super) fn descriptor_definition() -> StorageConnectorDescriptor {
        StorageConnectorDescriptor {
            connector_id: aster_drive_storage::ConnectorId::declared(Self::ID),
            label: "Google Drive".into(),
            description: "Administrator-managed Google Drive storage backend".into(),
            ui: aster_drive_storage::connector_descriptor::storage_connector_ui_descriptor(
                StorageConnectorUiDescriptorInput {
                    label_key: "driver_type_google_drive",
                    description_key: "policy_wizard_google_drive_storage_desc",
                    badge_rgb: StorageConnectorBadgeRgb::new(66, 133, 244),
                    helper_key: "policy_wizard_google_drive_helper",
                    config_step_title_key: "policy_wizard_step_google_drive_title",
                    config_step_description_key: "policy_wizard_step_google_drive_desc",
                    edit_context_key: "policy_edit_context_google_drive_desc",
                    base_path_empty_display: "core:root",
                    base_path_placeholder: "AsterDrive/blobs",
                },
            ),
            credential_mode: GoogleDriveConnectorConfigV1::credential_mode(),
            deployment_scope: StorageConnectorDeploymentScope::SharedAcrossPrimaryInstances,
            supports_initial_setup: false,
            requires_authorization: true,
            authorization_provider: Some("google_drive".into()),
            credential_management: Some(StorageConnectorCredentialManagementDescriptor {
                title_key: "google_drive_credential_title".into(),
                loading_key: "google_drive_credential_loading".into(),
                status_presentations: credential_status_presentations(),
                reauthorize_action_key: Some("google_drive_reauthorize_action".into()),
                authorized_at_key: Some("google_drive_authorized_at".into()),
                refreshed_at_key: Some("google_drive_refreshed_at".into()),
                validated_at_key: Some("google_drive_validated_at".into()),
                redirect_uri_key: Some("google_drive_redirect_uri".into()),
                redirect_uri_help_key: Some("google_drive_redirect_uri_help".into()),
                redirect_uri_copy_key: Some("google_drive_copy_redirect_uri".into()),
                save_before_authorize_key: Some("google_drive_save_before_authorize".into()),
                authorization_started_key: Some("google_drive_authorization_started".into()),
                save_before_validate_key: Some("google_drive_save_before_validate".into()),
                validation_success_key: Some("google_drive_validation_success".into()),
                validation_success_detail_key: None,
                created_authorize_next_key: None,
            }),
            capabilities: StorageConnectorCapabilities {
                efficient_range: true,
                capacity: true,
                list: false,
                presigned_download: false,
                custom_download_base_url: false,
                storage_native_thumbnail: false,
                storage_native_media_metadata: false,
                remote_node_binding: false,
                object_storage_transfer_strategy: false,
                object_naming: StorageConnectorObjectNamingMode::OpaqueUuid,
            },
            upload_workflows: StorageConnectorUploadWorkflows {
                simple_upload: true,
                simple_upload_capabilities: server_relay_simple_upload_capabilities(Some(
                    5 * 1024 * 1024,
                )),
                stream_upload: true,
                object_multipart_upload: false,
                object_multipart_upload_capabilities: None,
                provider_resumable_upload: true,
                presigned_upload: false,
                frontend_direct_provider_resumable_upload: true,
                provider_resumable_upload_capabilities: Some(
                    aster_drive_storage::connector_descriptor::StorageConnectorProviderResumableUploadCapabilities {
                        provider: "google_drive".into(),
                        session_label: "Google Drive resumable upload".into(),
                        min_fragment_size: 256 * 1024,
                        default_fragment_size: 8 * 1024 * 1024,
                        max_fragment_size: 8 * 1024 * 1024,
                        fragment_alignment: 256 * 1024,
                        max_simple_upload_size: Some(5 * 1024 * 1024),
                        frontend_direct_upload: true,
                        implicit_completion: true,
                        abort_supported: true,
                        status_query_supported: true,
                    },
                ),
            },
            fields: GoogleDriveConnectorConfigV1::descriptor_fields(),
            config_schema_version: 1,
            credential_schema_version: Some(1),
            actions: vec![
                start_authorization_action_descriptor(),
                validate_credential_action_descriptor(),
                saved_connection_test_action_descriptor(true),
            ],
            promotions: Vec::new(),
            related_issues: vec![128],
        }
    }
}

fn credential_status_presentations()
-> std::collections::BTreeMap<String, StorageConnectorCredentialStatusPresentation> {
    [
        (
            "missing",
            "google_drive_credential_status_missing",
            StorageConnectorCredentialStatusTone::Neutral,
            None,
        ),
        (
            "authorized",
            "google_drive_credential_status_authorized",
            StorageConnectorCredentialStatusTone::Success,
            None,
        ),
        (
            "reauth_required",
            "google_drive_credential_status_reauth_required",
            StorageConnectorCredentialStatusTone::Warning,
            Some("google_drive_reauth_help"),
        ),
        (
            "permission_denied",
            "google_drive_credential_status_permission_denied",
            StorageConnectorCredentialStatusTone::Danger,
            Some("google_drive_permission_help"),
        ),
        (
            "revoked",
            "google_drive_credential_status_revoked",
            StorageConnectorCredentialStatusTone::Danger,
            Some("google_drive_reauth_help"),
        ),
        (
            "invalid",
            "google_drive_credential_status_invalid",
            StorageConnectorCredentialStatusTone::Danger,
            Some("google_drive_invalid_help"),
        ),
    ]
    .into_iter()
    .map(|(status, label, tone, description)| {
        (
            status.into(),
            StorageConnectorCredentialStatusPresentation {
                label_key: label.into(),
                tone,
                description_key: description.map(Into::into),
                attention_title_key: None,
                attention_guidance_key: None,
                reason_rules: Vec::new(),
                reason_fallback_key: description.map(Into::into),
            },
        )
    })
    .collect()
}
