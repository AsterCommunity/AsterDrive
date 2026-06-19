use async_trait::async_trait;
use chrono::Utc;
use sea_orm::ConnectionTrait;
use std::sync::Arc;

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::managed_follower_repo;
use crate::entities::{managed_follower, storage_policy};
use crate::errors::{AsterError, Result, validation_error_with_code};
use crate::runtime::{RemoteProtocolRuntimeState, SharedRuntimeState};
use crate::storage::StorageDriver;
use crate::storage::connector_descriptor::{
    StorageConnectorCapabilities, StorageConnectorCredentialMode, StorageConnectorDescriptor,
    StorageConnectorDescriptorProvider, StorageConnectorFieldKind, StorageConnectorFieldScope,
    StorageConnectorUploadWorkflows, draft_connection_test_action_descriptor,
    saved_connection_test_action_descriptor, storage_connector_field,
    storage_connector_field_with_options, storage_connector_ui_descriptor,
};
use crate::types::{DriverType, RemoteNodeTransportMode, parse_storage_policy_options};

use super::common::{ensure_onedrive_options_absent, ensure_storage_native_processing_supported};
use super::{
    StorageConnector, StorageConnectorConnectionInput, StorageConnectorUploadTransport,
    StoragePolicyCleanupDriverSnapshot, StoragePolicyCleanupRemoteNodeSnapshot,
    StoragePolicyCleanupSnapshots,
};

pub struct RemoteConnector;

impl StorageConnectorDescriptorProvider for RemoteConnector {
    fn storage_connector_descriptor() -> StorageConnectorDescriptor {
        StorageConnectorDescriptor {
            driver_type: DriverType::Remote,
            enabled: true,
            label: "Remote node".to_string(),
            description: "Remote follower node storage policy".to_string(),
            ui: storage_connector_ui_descriptor(
                "driver_type_remote",
                "policy_wizard_remote_storage_desc",
                Some("/static/storage/asterdrive-node.svg"),
                None,
                "policy_wizard_remote_helper",
                "policy_wizard_step_remote_title",
                "policy_wizard_step_remote_desc",
                "policy_edit_context_remote_desc",
                "core:root",
                "tenant/prefix",
            ),
            credential_mode: StorageConnectorCredentialMode::RemoteNode,
            requires_authorization: false,
            authorization_provider: None,
            capabilities: StorageConnectorCapabilities {
                efficient_range: true,
                capacity: true,
                list: true,
                presigned_download: true,
                storage_native_thumbnail: false,
                storage_native_media_metadata: false,
                remote_node_binding: true,
                s3_transfer_strategy: false,
            },
            upload_workflows: StorageConnectorUploadWorkflows {
                simple_upload: true,
                stream_upload: true,
                object_multipart_upload: true,
                provider_resumable_upload: false,
                presigned_upload: true,
                frontend_direct_provider_resumable_upload: false,
                provider_resumable_upload_capabilities: None,
            },
            fields: vec![
                storage_connector_field(
                    "remote_node_id",
                    StorageConnectorFieldScope::RemoteNodeBinding,
                    StorageConnectorFieldKind::Select,
                    true,
                    false,
                ),
                storage_connector_field(
                    "base_path",
                    StorageConnectorFieldScope::Connection,
                    StorageConnectorFieldKind::Text,
                    false,
                    false,
                ),
                storage_connector_field_with_options(
                    "remote_download_strategy",
                    StorageConnectorFieldScope::PolicyOptions,
                    StorageConnectorFieldKind::Select,
                    true,
                    false,
                    vec!["relay_stream", "presigned"],
                ),
                storage_connector_field_with_options(
                    "remote_upload_strategy",
                    StorageConnectorFieldScope::PolicyOptions,
                    StorageConnectorFieldKind::Select,
                    true,
                    false,
                    vec!["relay_stream", "presigned"],
                ),
            ],
            actions: vec![
                draft_connection_test_action_descriptor(),
                saved_connection_test_action_descriptor(false),
            ],
            driver_recommendations: Vec::new(),
            related_issues: vec![328, 329],
        }
    }
}

#[async_trait(?Send)]
impl StorageConnector for RemoteConnector {
    fn driver_type() -> DriverType {
        DriverType::Remote
    }

    fn normalize_connection_fields(endpoint: &str, bucket: &str) -> Result<(String, String)> {
        let _ = (endpoint, bucket);
        Ok((String::new(), String::new()))
    }

    fn validate_connection_credentials(input: &StorageConnectorConnectionInput) -> Result<()> {
        let _ = input;
        Ok(())
    }

    async fn validate_connection_binding<C: ConnectionTrait + Sync>(
        db: &C,
        input: &StorageConnectorConnectionInput,
    ) -> Result<Option<i64>> {
        let remote_node_id = input.remote_node_id.ok_or_else(|| {
            validation_error_with_code(
                ApiErrorCode::PolicyRemoteNodeRequired,
                "remote storage policy requires remote_node_id",
            )
        })?;
        let remote_node = managed_follower_repo::find_by_id(db, remote_node_id).await?;
        if !remote_node.is_enabled {
            return Err(validation_error_with_code(
                ApiErrorCode::PolicyRemoteNodeDisabled,
                format!("remote node #{remote_node_id} is disabled"),
            ));
        }
        if remote_node.transport_mode == RemoteNodeTransportMode::Direct
            && remote_node.base_url.trim().is_empty()
        {
            return Err(validation_error_with_code(
                ApiErrorCode::PolicyRemoteNodeBaseUrlRequired,
                "remote node base_url is required for remote storage policies",
            ));
        }
        Ok(Some(remote_node_id))
    }

    async fn validate_policy_options<C: ConnectionTrait + Sync>(
        db: &C,
        remote_node_id: Option<i64>,
        options: &crate::types::StoragePolicyOptions,
    ) -> Result<()> {
        ensure_storage_native_processing_supported(Self::storage_connector_descriptor(), options)?;
        ensure_onedrive_options_absent(options)?;
        let Some(remote_node_id) = remote_node_id else {
            return Ok(());
        };
        let remote_node = managed_follower_repo::find_by_id(db, remote_node_id).await?;
        if remote_node
            .transport_mode
            .resolves_to_reverse_tunnel(&remote_node.base_url)
            && (options.effective_remote_download_strategy()
                == crate::types::RemoteDownloadStrategy::Presigned
                || options.effective_remote_upload_strategy()
                    == crate::types::RemoteUploadStrategy::Presigned)
        {
            return Err(validation_error_with_code(
                ApiErrorCode::PolicyRemoteNodeTransferStrategyUnsupported,
                "reverse tunnel remote nodes do not support presigned browser transfer strategies",
            ));
        }
        Ok(())
    }

    async fn build_draft_driver<S: RemoteProtocolRuntimeState + Sync + ?Sized>(
        state: &S,
        policy: &storage_policy::Model,
    ) -> Result<Box<dyn StorageDriver>> {
        let remote_node_id = policy
            .remote_node_id
            .expect("remote connector validation must require remote_node_id");
        let remote_node =
            managed_follower_repo::find_by_id(state.writer_db(), remote_node_id).await?;
        Ok(Box::new(
            state
                .remote_protocol()
                .driver_for_policy(policy, &remote_node)?,
        ))
    }

    fn upload_transport(policy: &storage_policy::Model) -> StorageConnectorUploadTransport {
        let options = parse_storage_policy_options(policy.options.as_ref());
        StorageConnectorUploadTransport::Remote(options.effective_remote_upload_strategy())
    }

    fn presigned_download_enabled(policy: &storage_policy::Model) -> bool {
        let options = parse_storage_policy_options(policy.options.as_ref());
        options.effective_remote_download_strategy()
            == crate::types::RemoteDownloadStrategy::Presigned
    }
}

impl RemoteConnector {
    pub(super) async fn cleanup_snapshot_for_policy<S: SharedRuntimeState + Sync + ?Sized>(
        state: &S,
        policy: &storage_policy::Model,
    ) -> Result<Option<StoragePolicyCleanupDriverSnapshot>> {
        let remote_node_id = policy.remote_node_id.ok_or_else(|| {
            AsterError::validation_error("remote storage policy requires remote_node_id")
        })?;
        let remote = managed_follower_repo::find_by_id(state.writer_db(), remote_node_id).await?;
        Ok(Some(StoragePolicyCleanupDriverSnapshot::RemoteNode(
            StoragePolicyCleanupRemoteNodeSnapshot {
                id: remote.id,
                name: remote.name,
                base_url: remote.base_url,
                transport_mode: remote.transport_mode,
                access_key: remote.access_key,
                secret_key: remote.secret_key,
                last_capabilities: remote.last_capabilities,
            },
        )))
    }

    pub(super) async fn build_cleanup_driver<S: RemoteProtocolRuntimeState + Sync + ?Sized>(
        state: &S,
        policy: &storage_policy::Model,
        snapshots: StoragePolicyCleanupSnapshots<'_>,
    ) -> Result<Arc<dyn StorageDriver>> {
        let remote = remote_snapshot_from_cleanup_input(snapshots)?;
        let follower = managed_follower::Model {
            id: remote.id,
            name: remote.name.clone(),
            base_url: remote.base_url.clone(),
            access_key: remote.access_key.clone(),
            secret_key: remote.secret_key.clone(),
            is_enabled: true,
            transport_mode: remote.transport_mode,
            last_capabilities: remote_capabilities_from_snapshot_or_current(state, remote).await?,
            last_error: String::new(),
            last_checked_at: None,
            tunnel_last_error: String::new(),
            tunnel_last_seen_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        Ok(Arc::new(
            state
                .remote_protocol()
                .driver_for_policy(policy, &follower)?,
        ))
    }
}

fn remote_snapshot_from_cleanup_input(
    snapshots: StoragePolicyCleanupSnapshots<'_>,
) -> Result<&StoragePolicyCleanupRemoteNodeSnapshot> {
    match snapshots.driver_snapshot {
        Some(StoragePolicyCleanupDriverSnapshot::RemoteNode(snapshot)) => Ok(snapshot),
        Some(_) => Err(AsterError::validation_error(
            "remote storage policy cleanup received incompatible driver snapshot",
        )),
        None => snapshots.legacy_remote_node.ok_or_else(|| {
            AsterError::validation_error("remote storage policy cleanup missing remote snapshot")
        }),
    }
}

async fn remote_capabilities_from_snapshot_or_current(
    state: &(impl RemoteProtocolRuntimeState + ?Sized),
    remote: &StoragePolicyCleanupRemoteNodeSnapshot,
) -> Result<String> {
    if !remote.last_capabilities.trim().is_empty() {
        return Ok(remote.last_capabilities.clone());
    }

    // Pre-0.3.0 cleanup payloads did not store remote capabilities. Use the
    // current node row only as a fallback so newly created cleanup tasks remain
    // self-contained snapshots.
    managed_follower_repo::find_by_id(state.writer_db(), remote.id)
        .await
        .map(|node| node.last_capabilities)
}
