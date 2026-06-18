use async_trait::async_trait;
use sea_orm::ConnectionTrait;
use std::sync::Arc;

use crate::db::repository::storage_policy_credential_repo;
use crate::entities::storage_policy;
use crate::errors::{AsterError, Result};
use crate::runtime::{RemoteProtocolRuntimeState, SharedRuntimeState};
use crate::storage::StorageDriver;
use crate::storage::connector_descriptor::{
    StorageConnectorCapabilities, StorageConnectorCredentialMode, StorageConnectorDescriptor,
    StorageConnectorDescriptorProvider, StorageConnectorFieldKind, StorageConnectorFieldScope,
    StorageConnectorUploadWorkflows, saved_connection_test_action_descriptor,
    start_authorization_action_descriptor, storage_connector_field,
    storage_connector_field_with_legacy, storage_connector_field_with_options,
    validate_credential_action_descriptor,
};
use crate::storage::drivers::onedrive::{
    MicrosoftGraphClient, MicrosoftGraphClientConfig, OneDriveDriver,
};
use crate::types::{
    DriverType, StorageCredentialKind, StorageCredentialProvider, StorageCredentialStatus,
};

use super::common::{
    ensure_storage_native_processing_supported, unsupported_draft_connection_test_error,
    validate_onedrive_options,
};
use super::{
    StorageConnector, StorageConnectorConnectionInput, StorageConnectorUploadTransport,
    StoragePolicyCleanupDriverSnapshot, StoragePolicyCleanupOneDriveCredentialSnapshot,
    StoragePolicyCleanupSnapshots,
};

pub struct OneDriveConnector;

impl StorageConnectorDescriptorProvider for OneDriveConnector {
    fn storage_connector_descriptor() -> StorageConnectorDescriptor {
        StorageConnectorDescriptor {
            driver_type: DriverType::OneDrive,
            enabled: true,
            label: "OneDrive / SharePoint".to_string(),
            description: "Microsoft Graph-backed OneDrive or SharePoint storage policy".to_string(),
            credential_mode: StorageConnectorCredentialMode::OauthDelegated,
            requires_authorization: true,
            authorization_provider: Some("microsoft_graph".to_string()),
            capabilities: StorageConnectorCapabilities {
                efficient_range: true,
                capacity: true,
                list: false,
                presigned_download: false,
                storage_native_thumbnail: false,
                storage_native_media_metadata: false,
                remote_node_binding: false,
                s3_transfer_strategy: false,
            },
            upload_workflows: StorageConnectorUploadWorkflows {
                simple_upload: true,
                stream_upload: true,
                object_multipart_upload: false,
                provider_resumable_upload: true,
                presigned_upload: false,
                frontend_direct_provider_resumable_upload: false,
            },
            fields: vec![
                storage_connector_field_with_legacy(
                    "client_id",
                    StorageConnectorFieldScope::ApplicationCredential,
                    StorageConnectorFieldKind::Text,
                    true,
                    false,
                    "access_key",
                ),
                storage_connector_field_with_legacy(
                    "client_secret",
                    StorageConnectorFieldScope::ApplicationCredential,
                    StorageConnectorFieldKind::Secret,
                    true,
                    true,
                    "secret_key",
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
            related_issues: vec![328, 329, 330],
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

    async fn validate_policy_options<C: ConnectionTrait + Sync>(
        db: &C,
        remote_node_id: Option<i64>,
        options: &crate::types::StoragePolicyOptions,
    ) -> Result<()> {
        let _ = (db, remote_node_id);
        ensure_storage_native_processing_supported(Self::storage_connector_descriptor(), options)?;
        validate_onedrive_options(options)
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
        let _ = policy;
        StorageConnectorUploadTransport::OneDrive
    }
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
        let token_provider = crate::services::storage_credential_service::build_microsoft_graph_cleanup_token_provider(
            state.config().auth.storage_credential_secret_key.clone(),
            policy,
            crate::services::storage_credential_service::MicrosoftGraphCleanupTokenSnapshot {
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
        tenant_id: credential.tenant_id,
        client_id: metadata_string(&metadata, "client_id"),
        client_secret_ciphertext: metadata_string(&metadata, "client_secret_ciphertext"),
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
    fn non_empty_string_trims_and_filters_blank_values() {
        assert_eq!(
            non_empty_string(" root ".to_string()),
            Some("root".to_string())
        );
        assert_eq!(non_empty_string(" \n\t ".to_string()), None);
    }
}
