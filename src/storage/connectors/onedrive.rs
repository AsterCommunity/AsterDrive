use async_trait::async_trait;
use sea_orm::ConnectionTrait;
use serde::Deserialize;
use std::sync::Arc;

use crate::db::repository::{
    storage_connector_application_config_repo, storage_policy_credential_repo,
};
use crate::entities::storage_policy;
use crate::errors::{AsterError, Result};
use crate::runtime::{RemoteProtocolRuntimeState, SharedRuntimeState};
use crate::storage::StorageDriver;
use crate::storage::connector_descriptor::{
    StorageConnectorCapabilities, StorageConnectorCredentialMode, StorageConnectorDescriptor,
    StorageConnectorDescriptorProvider, StorageConnectorFieldKind, StorageConnectorFieldScope,
    StorageConnectorObjectNamingMode, StorageConnectorProviderResumableUploadCapabilities,
    StorageConnectorUiDescriptorInput, StorageConnectorUploadWorkflows,
    saved_connection_test_action_descriptor, server_relay_simple_upload_capabilities,
    start_authorization_action_descriptor, storage_connector_field,
    storage_connector_field_with_options, storage_connector_ui_descriptor,
    validate_credential_action_descriptor,
};
use crate::storage::drivers::onedrive::{
    MicrosoftGraphClient, MicrosoftGraphClientConfig, OneDriveDriver,
    microsoft_graph_upload_capabilities,
};
use crate::types::{
    DriverType, ProviderDownloadFilenameMode, ProviderDownloadStrategy, StorageCredentialKind,
    StorageCredentialProvider, StorageCredentialStatus, parse_storage_policy_options,
};

use super::common::{
    ensure_storage_native_processing_supported, unsupported_draft_connection_test_error,
    validate_onedrive_options,
};
use super::{
    StorageConnector, StorageConnectorApplicationConfigInput, StorageConnectorConnectionInput,
    StorageConnectorCredentialRequirement, StorageConnectorRuntimeCredential,
    StorageConnectorUploadTransport, StorageCredentialValidationOutcome,
    StoragePolicyCleanupDriverSnapshot, StoragePolicyCleanupOneDriveCredentialSnapshot,
    StoragePolicyCleanupSnapshots,
};

pub struct OneDriveConnector;

#[derive(Debug, Deserialize)]
struct OneDriveCredentialMetadata {
    #[serde(default)]
    drive_id: Option<String>,
    #[serde(default)]
    root_item_id: Option<String>,
}

impl StorageConnectorDescriptorProvider for OneDriveConnector {
    fn storage_connector_descriptor() -> StorageConnectorDescriptor {
        let upload_capabilities = microsoft_graph_upload_capabilities();
        StorageConnectorDescriptor {
            driver_type: DriverType::OneDrive,
            enabled: true,
            label: "OneDrive / SharePoint".to_string(),
            description: "Microsoft Graph-backed OneDrive or SharePoint storage policy".to_string(),
            ui: storage_connector_ui_descriptor(StorageConnectorUiDescriptorInput {
                label_key: "driver_type_onedrive",
                description_key: "policy_wizard_onedrive_storage_desc",
                icon_src: Some("/static/storage/onedrive.svg"),
                icon_name: None,
                helper_key: "policy_wizard_onedrive_helper",
                config_step_title_key: "policy_wizard_step_onedrive_title",
                config_step_description_key: "policy_wizard_step_onedrive_desc",
                edit_context_key: "policy_edit_context_onedrive_desc",
                base_path_empty_display: "core:root",
                base_path_placeholder: "tenant/prefix",
            }),
            credential_mode: StorageConnectorCredentialMode::OauthDelegated,
            requires_authorization: true,
            authorization_provider: Some("microsoft_graph".to_string()),
            capabilities: StorageConnectorCapabilities {
                efficient_range: true,
                capacity: true,
                list: false,
                presigned_download: true,
                storage_native_thumbnail: false,
                storage_native_media_metadata: false,
                remote_node_binding: false,
                object_storage_transfer_strategy: false,
                object_naming: StorageConnectorObjectNamingMode::OriginalFilename,
            },
            upload_workflows: StorageConnectorUploadWorkflows {
                simple_upload: true,
                simple_upload_capabilities: server_relay_simple_upload_capabilities(
                    upload_capabilities.max_simple_upload_size,
                ),
                stream_upload: true,
                object_multipart_upload: false,
                object_multipart_upload_capabilities: None,
                provider_resumable_upload: true,
                presigned_upload: false,
                frontend_direct_provider_resumable_upload: true,
                provider_resumable_upload_capabilities: Some(
                    StorageConnectorProviderResumableUploadCapabilities {
                        provider: upload_capabilities.provider.to_string(),
                        session_label: upload_capabilities.session_label.to_string(),
                        min_fragment_size: upload_capabilities.min_fragment_size,
                        default_fragment_size: upload_capabilities.default_fragment_size,
                        max_fragment_size: upload_capabilities.max_fragment_size,
                        fragment_alignment: upload_capabilities.fragment_alignment,
                        max_simple_upload_size: upload_capabilities.max_simple_upload_size,
                        frontend_direct_upload: upload_capabilities.frontend_direct_upload,
                        implicit_completion: upload_capabilities.implicit_completion,
                        abort_supported: upload_capabilities.abort_supported,
                        status_query_supported: upload_capabilities.status_query_supported,
                    },
                ),
            },
            fields: vec![
                storage_connector_field(
                    "client_id",
                    StorageConnectorFieldScope::ApplicationCredential,
                    StorageConnectorFieldKind::Text,
                    true,
                    false,
                ),
                storage_connector_field(
                    "client_secret",
                    StorageConnectorFieldScope::ApplicationCredential,
                    StorageConnectorFieldKind::Secret,
                    true,
                    true,
                ),
                storage_connector_field_with_options(
                    "provider_resumable_upload_strategy",
                    StorageConnectorFieldScope::PolicyOptions,
                    StorageConnectorFieldKind::Select,
                    true,
                    false,
                    vec!["server_relay", "frontend_direct"],
                ),
                storage_connector_field_with_options(
                    "provider_download_strategy",
                    StorageConnectorFieldScope::PolicyOptions,
                    StorageConnectorFieldKind::Select,
                    true,
                    false,
                    vec!["server_relay", "frontend_direct"],
                ),
                storage_connector_field_with_options(
                    "provider_download_filename_mode",
                    StorageConnectorFieldScope::PolicyOptions,
                    StorageConnectorFieldKind::Select,
                    true,
                    false,
                    vec!["provider_native", "strict_current"],
                ),
                storage_connector_field_with_options(
                    "cloud",
                    StorageConnectorFieldScope::PolicyOptions,
                    StorageConnectorFieldKind::Select,
                    true,
                    false,
                    vec!["global", "china"],
                ),
                storage_connector_field_with_options(
                    "account_mode",
                    StorageConnectorFieldScope::PolicyOptions,
                    StorageConnectorFieldKind::Select,
                    true,
                    false,
                    vec![
                        "personal",
                        "work_or_school",
                        "sharepoint_site",
                        "group_drive",
                    ],
                ),
                storage_connector_field(
                    "tenant",
                    StorageConnectorFieldScope::PolicyOptions,
                    StorageConnectorFieldKind::Text,
                    false,
                    false,
                ),
                storage_connector_field(
                    "drive_id",
                    StorageConnectorFieldScope::PolicyOptions,
                    StorageConnectorFieldKind::Text,
                    false,
                    false,
                ),
                storage_connector_field(
                    "root_item_id",
                    StorageConnectorFieldScope::PolicyOptions,
                    StorageConnectorFieldKind::Text,
                    false,
                    false,
                ),
                storage_connector_field(
                    "site_id",
                    StorageConnectorFieldScope::PolicyOptions,
                    StorageConnectorFieldKind::Text,
                    false,
                    false,
                ),
                storage_connector_field(
                    "group_id",
                    StorageConnectorFieldScope::PolicyOptions,
                    StorageConnectorFieldKind::Text,
                    false,
                    false,
                ),
            ],
            actions: vec![
                start_authorization_action_descriptor(),
                validate_credential_action_descriptor(),
                saved_connection_test_action_descriptor(true),
            ],
            driver_recommendations: Vec::new(),
            related_issues: vec![328, 329, 330, 349],
        }
    }
}

#[async_trait(?Send)]
impl StorageConnector for OneDriveConnector {
    fn driver_type() -> DriverType {
        DriverType::OneDrive
    }

    fn normalize_connection_fields(endpoint: &str, bucket: &str) -> Result<(String, String)> {
        let _ = (endpoint, bucket);
        Ok((String::new(), String::new()))
    }

    fn validate_connection_credentials(input: &StorageConnectorConnectionInput) -> Result<()> {
        let _ = input;
        Ok(())
    }

    fn prepare_connection_for_storage(
        mut input: StorageConnectorConnectionInput,
        application_config: &StorageConnectorApplicationConfigInput,
    ) -> Result<StorageConnectorConnectionInput> {
        let _ = application_config;
        // Microsoft Graph application credentials are connector-owned config,
        // not S3-style policy access keys. Clear the legacy columns at the
        // storage boundary so storage_policy::policy never has to know this rule.
        input.access_key.clear();
        input.secret_key.clear();
        Ok(input)
    }

    async fn validate_policy_options<C: ConnectionTrait + Sync>(
        db: &C,
        remote_node_id: Option<i64>,
        options: &crate::types::StoragePolicyOptions,
    ) -> Result<()> {
        let _ = (db, remote_node_id);
        ensure_storage_native_processing_supported(Self::storage_connector_descriptor(), options)?;
        validate_onedrive_options(options)
    }

    async fn persist_application_config<C: ConnectionTrait + Sync>(
        db: &C,
        encryption_key: &str,
        policy_id: i64,
        options: &crate::types::StoragePolicyOptions,
        application_config: StorageConnectorApplicationConfigInput,
    ) -> Result<()> {
        let Some(microsoft_graph) = application_config.microsoft_graph else {
            return Ok(());
        };
        crate::services::storage_policy::credential::upsert_microsoft_graph_application_config(
            db,
            encryption_key,
            policy_id,
            microsoft_graph_config_with_policy_options(microsoft_graph, options),
        )
        .await?;
        Ok(())
    }

    async fn build_draft_driver<S: RemoteProtocolRuntimeState + Sync + ?Sized>(
        state: &S,
        policy: &storage_policy::Model,
    ) -> Result<Box<dyn StorageDriver>> {
        let _ = (state, policy);
        Err(unsupported_draft_connection_test_error(
            Self::storage_connector_descriptor(),
        ))
    }

    fn upload_transport(policy: &storage_policy::Model) -> StorageConnectorUploadTransport {
        let options = parse_storage_policy_options(policy.options.as_ref());
        StorageConnectorUploadTransport::ProviderResumable(
            options.effective_provider_resumable_upload_strategy(),
        )
    }

    fn presigned_download_enabled(policy: &storage_policy::Model) -> bool {
        let options = parse_storage_policy_options(policy.options.as_ref());
        options.effective_provider_download_strategy() == ProviderDownloadStrategy::FrontendDirect
    }

    fn presigned_download_requires_filename_match(policy: &storage_policy::Model) -> bool {
        let options = parse_storage_policy_options(policy.options.as_ref());
        options.effective_provider_download_filename_mode()
            == ProviderDownloadFilenameMode::StrictCurrent
    }

    fn runtime_credential_requirement() -> Option<StorageConnectorCredentialRequirement> {
        Some(StorageConnectorCredentialRequirement {
            provider: StorageCredentialProvider::MicrosoftGraph,
            credential_kind: StorageCredentialKind::OauthDelegated,
            requires_application_config: true,
            requires_authorization: true,
        })
    }

    async fn load_runtime_credential(
        db: &sea_orm::DatabaseConnection,
        config: &crate::config::Config,
        policy: &storage_policy::Model,
        credential: &crate::entities::storage_policy_credential::Model,
    ) -> Result<Option<StorageConnectorRuntimeCredential>> {
        let metadata = parse_onedrive_credential_metadata(&credential.metadata);
        let (drive_id, root_item_id) = metadata
            .map(|value| (value.drive_id, value.root_item_id))
            .unwrap_or((None, None));
        let options = parse_storage_policy_options(policy.options.as_ref());
        let application_config =
            match storage_connector_application_config_repo::find_by_policy_provider(
                db,
                credential.policy_id,
                StorageCredentialProvider::MicrosoftGraph,
            )
            .await
            {
                Ok(Some(config)) => config,
                Ok(None) => {
                    tracing::warn!(
                        policy_id = credential.policy_id,
                        credential_id = credential.id,
                        "skipping OneDrive credential reload because Microsoft Graph application config is missing"
                    );
                    return Ok(None);
                }
                Err(error) => {
                    tracing::warn!(
                        policy_id = credential.policy_id,
                        credential_id = credential.id,
                        error = %error,
                        "skipping OneDrive credential reload because application config lookup failed"
                    );
                    return Ok(None);
                }
            };
        let token_provider =
            match crate::services::storage_policy::credential::build_microsoft_graph_credential_token_provider(
                db.clone(),
                config.auth.storage_credential_secret_key.clone(),
                policy,
                credential,
                &application_config,
                options.effective_onedrive_cloud(),
            ) {
                Ok(token_provider) => token_provider,
                Err(error) => {
                    tracing::warn!(
                        policy_id = credential.policy_id,
                        credential_id = credential.id,
                        error = %error,
                        "skipping OneDrive credential reload because token provider initialization failed"
                    );
                    return Ok(None);
                }
            };
        Ok(Some(StorageConnectorRuntimeCredential::MicrosoftGraph(
            super::models::OneDriveCredentialRuntime {
                token_provider,
                drive_id,
                root_item_id,
            },
        )))
    }

    fn build_authorized_driver(
        policy: &storage_policy::Model,
        credential: StorageConnectorRuntimeCredential,
    ) -> Result<Arc<dyn StorageDriver>> {
        let StorageConnectorRuntimeCredential::MicrosoftGraph(credential) = credential;
        let options = parse_storage_policy_options(policy.options.as_ref());
        let drive_id = options
            .onedrive_drive_id
            .clone()
            .and_then(non_empty_string)
            .or_else(|| credential.drive_id.and_then(non_empty_string))
            .ok_or_else(|| {
                crate::storage::error::storage_driver_error(
                    crate::storage::StorageErrorKind::Misconfigured,
                    "OneDrive storage policy missing resolved drive_id; reauthorize Microsoft Graph",
                )
            })?;
        let configured_root_item_id = options
            .onedrive_root_item_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let root_item_id = configured_root_item_id
            .filter(|value| !value.eq_ignore_ascii_case("root"))
            .map(ToOwned::to_owned)
            .or_else(|| credential.root_item_id.and_then(non_empty_string))
            .or_else(|| configured_root_item_id.map(ToOwned::to_owned))
            .ok_or_else(|| {
                crate::storage::error::storage_driver_error(
                    crate::storage::StorageErrorKind::Misconfigured,
                    "OneDrive storage policy missing resolved root_item_id; reauthorize Microsoft Graph",
                )
            })?;
        if root_item_id.trim().is_empty() {
            return Err(crate::storage::error::storage_driver_error(
                crate::storage::StorageErrorKind::Misconfigured,
                "OneDrive storage policy missing resolved root_item_id; reauthorize Microsoft Graph",
            ));
        }
        if drive_id.trim().is_empty() {
            return Err(crate::storage::error::storage_driver_error(
                crate::storage::StorageErrorKind::Misconfigured,
                "OneDrive storage policy missing resolved drive_id; reauthorize Microsoft Graph",
            ));
        }
        let client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::with_token_provider(
            options.effective_onedrive_cloud().graph_base_url(),
            credential.token_provider,
        ))?;
        Ok(Arc::new(OneDriveDriver::new(
            client,
            drive_id,
            root_item_id,
            policy.base_path.clone(),
            policy.chunk_size,
        )))
    }

    async fn validate_credential(
        db: &sea_orm::DatabaseConnection,
        config: &crate::config::Config,
        policy: &storage_policy::Model,
        credential: &crate::entities::storage_policy_credential::Model,
    ) -> Result<StorageCredentialValidationOutcome> {
        let options = parse_storage_policy_options(policy.options.as_ref());
        let application_config =
            storage_connector_application_config_repo::find_by_policy_provider(
                db,
                policy.id,
                StorageCredentialProvider::MicrosoftGraph,
            )
            .await?
            .ok_or_else(|| {
                AsterError::validation_error(
                    "storage connector application config is required before validating credential",
                )
            })?;
        let token_provider =
            crate::services::storage_policy::credential::build_microsoft_graph_credential_token_provider(
                db.clone(),
                config.auth.storage_credential_secret_key.clone(),
                policy,
                credential,
                &application_config,
                options.effective_onedrive_cloud(),
            )?;
        let client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::with_token_provider(
            options.effective_onedrive_cloud().graph_base_url(),
            token_provider,
        ))?;
        let location = crate::services::storage_policy::credential::resolve_onedrive_location(
            &client, &options,
        )
        .await?;
        let root_item = location.root_item;
        let metadata = crate::services::storage_policy::credential::storage_credential_metadata(
            crate::services::storage_policy::credential::StorageCredentialMetadataInput {
                cloud: options.effective_onedrive_cloud(),
                drive_id: &location.drive_id,
                root_item_id: &root_item.id,
                root_item_name: root_item.name.as_deref(),
                id_token: None,
            },
        )?;
        Ok(StorageCredentialValidationOutcome {
            account_label: root_item.name.clone(),
            subject: Some(root_item.id.clone()),
            metadata,
            root_item_id: root_item.id,
            root_item_name: root_item.name,
        })
    }
}

fn microsoft_graph_config_with_policy_options(
    mut input: crate::storage::MicrosoftGraphApplicationConfigInput,
    options: &crate::types::StoragePolicyOptions,
) -> crate::storage::MicrosoftGraphApplicationConfigInput {
    // OneDrive keeps cloud/tenant in policy options for driver behavior; the
    // app-config row mirrors them so OAuth start can be driven by saved provider
    // metadata without reading legacy policy key fields.
    input.cloud = input.cloud.or(options.onedrive_cloud);
    if input
        .tenant
        .as_ref()
        .is_none_or(|tenant| tenant.trim().is_empty())
    {
        input.tenant = options.onedrive_tenant.clone();
    }
    input
}

impl OneDriveConnector {
    pub(super) async fn cleanup_snapshot_for_policy<S: SharedRuntimeState + Sync + ?Sized>(
        state: &S,
        policy: &storage_policy::Model,
    ) -> Result<Option<StoragePolicyCleanupDriverSnapshot>> {
        onedrive_credential_snapshot_for_policy(state, policy)
            .await
            .map(|snapshot| snapshot.map(StoragePolicyCleanupDriverSnapshot::MicrosoftGraph))
    }

    pub(super) async fn build_cleanup_driver<S: RemoteProtocolRuntimeState + Sync + ?Sized>(
        state: &S,
        policy: &storage_policy::Model,
        snapshots: StoragePolicyCleanupSnapshots<'_>,
    ) -> Result<Arc<dyn StorageDriver>> {
        let credential = onedrive_snapshot_from_cleanup_input(snapshots)?;
        let token_provider = crate::services::storage_policy::credential::build_microsoft_graph_cleanup_token_provider(
            state.config().auth.storage_credential_secret_key.clone(),
            policy,
            crate::services::storage_policy::credential::MicrosoftGraphCleanupTokenSnapshot {
                cloud: credential.cloud,
                tenant_id: credential.tenant_id.clone(),
                client_id: credential.client_id.clone(),
                client_secret_ciphertext: credential.client_secret_ciphertext.clone(),
                access_token_ciphertext: credential.access_token_ciphertext.clone(),
                refresh_token_ciphertext: credential.refresh_token_ciphertext.clone(),
                expires_at: credential.expires_at,
            },
        )?;
        let client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::with_token_provider(
            credential.cloud.graph_base_url(),
            token_provider,
        ))?;
        Ok(Arc::new(OneDriveDriver::new(
            client,
            credential.drive_id.clone(),
            credential.root_item_id.clone(),
            policy.base_path.clone(),
            policy.chunk_size,
        )))
    }
}

async fn onedrive_credential_snapshot_for_policy(
    state: &(impl SharedRuntimeState + ?Sized),
    policy: &storage_policy::Model,
) -> Result<Option<StoragePolicyCleanupOneDriveCredentialSnapshot>> {
    let Some(credential) = storage_policy_credential_repo::find_by_policy_provider_kind(
        state.writer_db(),
        policy.id,
        StorageCredentialProvider::MicrosoftGraph,
        StorageCredentialKind::OauthDelegated,
    )
    .await?
    else {
        tracing::warn!(
            policy_id = policy.id,
            "OneDrive storage policy cleanup missing credential snapshot; skipping deferred cleanup"
        );
        return Ok(None);
    };
    if credential.status != StorageCredentialStatus::Authorized {
        tracing::warn!(
            policy_id = policy.id,
            status = ?credential.status,
            "OneDrive storage policy credential is not authorized; skipping deferred cleanup"
        );
        return Ok(None);
    }
    let Some(access_token_ciphertext) = credential.access_token_ciphertext else {
        tracing::warn!(
            policy_id = policy.id,
            "OneDrive storage policy cleanup missing access token snapshot; skipping deferred cleanup"
        );
        return Ok(None);
    };
    let Some(refresh_token_ciphertext) = credential.refresh_token_ciphertext else {
        tracing::warn!(
            policy_id = policy.id,
            "OneDrive storage policy cleanup missing refresh token snapshot; skipping deferred cleanup"
        );
        return Ok(None);
    };
    let Some(application_config) =
        storage_connector_application_config_repo::find_by_policy_provider(
            state.writer_db(),
            policy.id,
            StorageCredentialProvider::MicrosoftGraph,
        )
        .await?
    else {
        tracing::warn!(
            policy_id = policy.id,
            "OneDrive storage policy cleanup missing application config snapshot; skipping deferred cleanup"
        );
        return Ok(None);
    };
    let metadata = serde_json::from_str::<serde_json::Value>(&credential.metadata)
        .ok()
        .unwrap_or_default();
    let options = crate::types::parse_storage_policy_options(policy.options.as_ref());
    let cloud = metadata
        .get("cloud")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_else(|| options.effective_onedrive_cloud());
    let Some(drive_id) = options
        .onedrive_drive_id
        .clone()
        .and_then(non_empty_string)
        .or_else(|| metadata_string(&metadata, "drive_id"))
    else {
        tracing::warn!(
            policy_id = policy.id,
            "OneDrive storage policy cleanup missing drive_id snapshot; skipping deferred cleanup"
        );
        return Ok(None);
    };
    let configured_root_item_id = options
        .onedrive_root_item_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(root_item_id) = configured_root_item_id
        .filter(|value| !value.eq_ignore_ascii_case("root"))
        .map(ToOwned::to_owned)
        .or_else(|| metadata_string(&metadata, "root_item_id"))
        .or_else(|| configured_root_item_id.map(ToOwned::to_owned))
    else {
        tracing::warn!(
            policy_id = policy.id,
            "OneDrive storage policy cleanup missing root_item_id snapshot; skipping deferred cleanup"
        );
        return Ok(None);
    };

    Ok(Some(StoragePolicyCleanupOneDriveCredentialSnapshot {
        cloud,
        tenant_id: application_config.tenant_id.or(credential.tenant_id),
        client_id: application_config.client_id,
        client_secret_ciphertext: application_config.client_secret_ciphertext,
        drive_id,
        root_item_id,
        access_token_ciphertext,
        refresh_token_ciphertext: Some(refresh_token_ciphertext),
        expires_at: credential.expires_at,
    }))
}

fn onedrive_snapshot_from_cleanup_input(
    snapshots: StoragePolicyCleanupSnapshots<'_>,
) -> Result<&StoragePolicyCleanupOneDriveCredentialSnapshot> {
    match snapshots.driver_snapshot {
        Some(StoragePolicyCleanupDriverSnapshot::MicrosoftGraph(snapshot)) => Ok(snapshot),
        Some(_) => Err(AsterError::validation_error(
            "OneDrive storage policy cleanup received incompatible driver snapshot",
        )),
        None => snapshots.legacy_onedrive_credential.ok_or_else(|| {
            AsterError::validation_error(
                "OneDrive storage policy cleanup missing credential snapshot",
            )
        }),
    }
}

fn metadata_string(metadata: &serde_json::Value, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_onedrive_credential_metadata(raw: &str) -> Option<OneDriveCredentialMetadata> {
    serde_json::from_str(raw).ok()
}

fn non_empty_string(value: String) -> Option<String> {
    let value = value.trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_string_trims_and_filters_blank_values() {
        let metadata = serde_json::json!({
            "drive_id": " drive ",
            "blank": "   "
        });

        assert_eq!(
            metadata_string(&metadata, "drive_id"),
            Some("drive".to_string())
        );
        assert_eq!(metadata_string(&metadata, "blank"), None);
        assert_eq!(metadata_string(&metadata, "missing"), None);
    }

    #[test]
    fn onedrive_metadata_parse_preserves_optional_ids_for_runtime_fallback() {
        let metadata = parse_onedrive_credential_metadata(
            r#"{"drive_id":"resolved-drive","root_item_id":"resolved-root"}"#,
        )
        .expect("metadata should parse");

        assert_eq!(metadata.drive_id, Some("resolved-drive".to_string()));
        assert_eq!(metadata.root_item_id, Some("resolved-root".to_string()));
    }

    #[test]
    fn non_empty_string_trims_and_filters_blank_values() {
        assert_eq!(
            non_empty_string(" root ".to_string()),
            Some("root".to_string())
        );
        assert_eq!(non_empty_string(" \n\t ".to_string()), None);
    }
}
