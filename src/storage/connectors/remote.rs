use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::managed_follower_repo;
use crate::errors::{
    AsterError, Result, precondition_failed_with_code, validation_error_with_code,
};
use crate::services::remote::capability::RemoteCapabilityResolver;
use crate::services::storage_policy::credential::crypto;
use crate::storage::drivers::remote::{RemoteDriver, RemoteDriverConfig};
use aster_drive_model::entities::{managed_follower, storage_policy};
use aster_drive_model::types::{
    RemoteDownloadStrategy, RemoteNodeTransportMode, RemoteUploadStrategy,
};
use aster_drive_storage::connector_descriptor::{
    ObjectMultipartUploadCapabilitiesInput, StorageConnectorBadgeRgb, StorageConnectorCapabilities,
    StorageConnectorDeploymentScope, StorageConnectorDescriptor, StorageConnectorFieldKind,
    StorageConnectorFieldScope, StorageConnectorObjectNamingMode,
    StorageConnectorUiDescriptorInput, StorageConnectorUploadWorkflows,
    draft_connection_test_action_descriptor, object_multipart_upload_capabilities,
    saved_connection_test_action_descriptor, server_relay_simple_upload_capabilities,
    storage_connector_dynamic_select_field, storage_connector_field,
    storage_connector_ui_descriptor,
};
use aster_drive_storage::{
    StorageConnectorConfigSchema, StorageConnectorFieldDefaultValue,
    StorageConnectorSelectDataSource, StorageConnectorSelectValueKind,
};
use aster_drive_storage::{StorageDriver, StorageErrorKind, storage_driver_error};

use super::common::{StorageTransferDirection, transfer_strategy_field};
use super::{
    RemotePolicyBindingProjection, StorageConnector, StorageConnectorCredentialInput,
    StorageConnectorUploadTransport, StoragePolicyCleanupDriverSnapshot,
    StoragePolicyCleanupSnapshots,
};

mod localization;

pub struct RemoteConnector;

const CLEANUP_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoteCleanupSnapshotV1 {
    id: i64,
    name: String,
    base_url: String,
    #[serde(default)]
    transport_mode: RemoteNodeTransportMode,
    access_key_ciphertext: String,
    secret_key_ciphertext: String,
    #[serde(default)]
    last_capabilities: String,
}

aster_drive_storage::storage_connector_schema! {
    pub struct RemoteConnectorConfigV1 {
        config {
        pub base_path: String => {
            let mut field = storage_connector_field(
                "base_path", StorageConnectorFieldScope::ConnectorConfig,
                StorageConnectorFieldKind::Text, false, false,
            );
            field.default_value = Some(StorageConnectorFieldDefaultValue::String(String::new()));
            field.default_mode = aster_drive_storage::StorageConnectorFieldDefaultMode::MissingOrEmptyText;
            field
        },
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub remote_node_id: Option<i64> => {
            let mut field = storage_connector_dynamic_select_field(
                "remote_node_id", StorageConnectorFieldScope::ConnectorConfig, true,
                StorageConnectorSelectValueKind::Integer,
                StorageConnectorSelectDataSource::RemoteNodes,
                None,
            );
            field.required_message_key = Some("policy_wizard_remote_node_required".to_string());
            field
        },
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub remote_storage_target_key: Option<String> => {
            let mut field = storage_connector_dynamic_select_field(
                "remote_storage_target_key", StorageConnectorFieldScope::ConnectorConfig, true,
                StorageConnectorSelectValueKind::String,
                StorageConnectorSelectDataSource::RemoteStorageTargets,
                Some("remote_node_id"),
            );
            field.required_message_key =
                Some("policy_wizard_remote_storage_target_required".to_string());
            field
        },
        pub remote_download_strategy: RemoteDownloadStrategy => transfer_strategy_field(
            "remote_download_strategy", StorageTransferDirection::Download,
        ),
        pub remote_upload_strategy: RemoteUploadStrategy => transfer_strategy_field(
            "remote_upload_strategy", StorageTransferDirection::Upload,
        ),
        }
        credentials none
    }
}

impl RemoteConnector {
    pub const ID: &'static str = "asterdrive.storage.remote";

    fn decode_config(policy: &storage_policy::Model) -> Result<RemoteConnectorConfigV1> {
        super::common::decode_typed_policy_config(policy, Self::ID, 1)
            .map(|(config, _behavior)| config)
    }

    fn driver_config(
        policy: &storage_policy::Model,
        config: &RemoteConnectorConfigV1,
    ) -> RemoteDriverConfig {
        RemoteDriverConfig {
            base_path: config.base_path.clone(),
            remote_storage_target_key: config.remote_storage_target_key.clone(),
            max_file_size: policy.max_file_size,
        }
    }
}

impl RemoteConnector {
    fn descriptor_definition() -> StorageConnectorDescriptor {
        StorageConnectorDescriptor {
            connector_id: aster_drive_storage::ConnectorId::declared(Self::ID),
            label: "Remote node".to_string(),
            description: "Remote follower node storage policy".to_string(),
            ui: storage_connector_ui_descriptor(StorageConnectorUiDescriptorInput {
                label_key: "driver_type_remote",
                description_key: "policy_wizard_remote_storage_desc",
                icon_src: Some("/static/storage/asterdrive-node.svg"),
                icon_name: None,
                badge_rgb: StorageConnectorBadgeRgb::new(245, 158, 11),
                helper_key: "policy_wizard_remote_helper",
                config_step_title_key: "policy_wizard_step_remote_title",
                config_step_description_key: "policy_wizard_step_remote_desc",
                edit_context_key: "policy_edit_context_remote_desc",
                base_path_empty_display: "core:root",
                base_path_placeholder: "tenant/prefix",
            }),
            credential_mode: RemoteConnectorConfigV1::credential_mode(),
            deployment_scope: StorageConnectorDeploymentScope::SharedAcrossPrimaryInstances,
            supports_initial_setup: true,
            requires_authorization: false,
            authorization_provider: None,
            credential_management: None,
            capabilities: StorageConnectorCapabilities {
                efficient_range: true,
                capacity: true,
                list: true,
                presigned_download: true,
                storage_native_thumbnail: false,
                storage_native_media_metadata: false,
                remote_node_binding: true,
                object_storage_transfer_strategy: false,
                object_naming: StorageConnectorObjectNamingMode::OpaqueUuid,
            },
            upload_workflows: StorageConnectorUploadWorkflows {
                simple_upload: true,
                simple_upload_capabilities: server_relay_simple_upload_capabilities(None),
                stream_upload: true,
                object_multipart_upload: true,
                object_multipart_upload_capabilities: Some(object_multipart_upload_capabilities(
                    ObjectMultipartUploadCapabilitiesInput {
                        presigned_part_etag_required: true,
                    },
                )),
                provider_resumable_upload: false,
                presigned_upload: true,
                frontend_direct_provider_resumable_upload: false,
                provider_resumable_upload_capabilities: None,
            },
            fields: RemoteConnectorConfigV1::descriptor_fields(),
            config_schema_version: 1,
            credential_schema_version: None,
            actions: vec![
                draft_connection_test_action_descriptor(),
                saved_connection_test_action_descriptor(false),
            ],
            promotions: Vec::new(),
            related_issues: vec![328, 329],
        }
    }
}

#[async_trait]
impl StorageConnector for RemoteConnector {
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

    async fn validate_config_binding(
        &self,
        db: &sea_orm::DatabaseConnection,
        connector_config: &aster_drive_storage::ConnectorConfigEnvelope,
    ) -> Result<()> {
        let config: RemoteConnectorConfigV1 =
            super::common::decode_normalized_connector_config(connector_config)?;
        let remote_node_id = config.remote_node_id.ok_or_else(|| {
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
        if remote_node
            .transport_mode
            .resolves_to_reverse_tunnel(&remote_node.base_url)
            && (config.remote_download_strategy == RemoteDownloadStrategy::Presigned
                || config.remote_upload_strategy == RemoteUploadStrategy::Presigned)
        {
            return Err(validation_error_with_code(
                ApiErrorCode::PolicyRemoteNodeTransferStrategyUnsupported,
                "reverse tunnel remote nodes do not support presigned browser transfer strategies",
            ));
        }
        Ok(())
    }

    async fn build_draft_driver(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        credential: &StorageConnectorCredentialInput,
    ) -> Result<Box<dyn StorageDriver>> {
        let _ = credential;
        let config = Self::decode_config(policy)?;
        let remote_node_id = config.remote_node_id.ok_or_else(|| {
            validation_error_with_code(
                ApiErrorCode::PolicyRemoteNodeRequired,
                "remote storage policy requires remote_node_id",
            )
        })?;
        let remote_node =
            managed_follower_repo::find_by_id(context.writer_db(), remote_node_id).await?;
        Ok(Box::new(context.remote_protocol()?.driver_for_config(
            &Self::driver_config(policy, &config),
            &remote_node,
        )?))
    }

    fn build_runtime_driver(
        &self,
        registry: &crate::storage::DriverRegistry,
        policy: &storage_policy::Model,
    ) -> Result<super::StorageConnectorDriver> {
        let config = Self::decode_config(policy)?;
        let remote_node_id = config.remote_node_id.ok_or_else(|| {
            AsterError::from(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "remote storage policy missing remote_node_id",
            ))
        })?;
        let remote_node = registry
            .get_managed_follower(remote_node_id)
            .ok_or_else(|| {
                AsterError::from(storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    format!("remote node #{remote_node_id} not loaded in registry"),
                ))
            })?;
        if !remote_node.is_enabled {
            return Err(precondition_failed_with_code(
                ApiErrorCode::RemoteNodeDisabled,
                format!("remote node #{remote_node_id} is disabled"),
            ));
        }
        let capabilities = RemoteCapabilityResolver::from_remote_node(&remote_node);
        if let Err(error) = capabilities.ensure_remote_policy_config_supported(
            policy.id,
            config.remote_download_strategy,
            config.remote_upload_strategy,
        ) {
            tracing::warn!(
                remote_node_id,
                policy_id = policy.id,
                protocol_version = %capabilities.capabilities().protocol_version,
                min_supported_protocol_version = %capabilities.capabilities().min_supported_protocol_version,
                "remote storage policy protocol compatibility check failed: {error}"
            );
            return Err(error);
        }
        let driver_config = Self::driver_config(policy, &config);
        let driver = if let Some(remote_protocol) = registry.remote_protocol() {
            Arc::new(remote_protocol.driver_for_config(&driver_config, &remote_node)?)
        } else {
            Arc::new(RemoteDriver::new(&driver_config, &remote_node)?)
        };
        Ok(super::StorageConnectorDriver::multipart(driver))
    }

    fn upload_transport(
        &self,
        policy: &storage_policy::Model,
    ) -> Result<StorageConnectorUploadTransport> {
        Self::decode_config(policy)
            .map(|config| StorageConnectorUploadTransport::Remote(config.remote_upload_strategy))
    }

    fn remote_binding_projection(
        &self,
        policy: &storage_policy::Model,
    ) -> Result<Option<RemotePolicyBindingProjection>> {
        let config = Self::decode_config(policy)?;
        Ok(Some(RemotePolicyBindingProjection {
            remote_node_id: config.remote_node_id,
            download_strategy: config.remote_download_strategy,
            upload_strategy: config.remote_upload_strategy,
        }))
    }

    fn presigned_download_enabled(&self, policy: &storage_policy::Model) -> Result<bool> {
        Self::decode_config(policy)
            .map(|config| config.remote_download_strategy == RemoteDownloadStrategy::Presigned)
    }
    async fn cleanup_snapshot_for_policy(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
    ) -> Result<Option<StoragePolicyCleanupDriverSnapshot>> {
        let config = Self::decode_config(policy)?;
        let remote_node_id = config.remote_node_id.ok_or_else(|| {
            AsterError::validation_error("remote storage policy requires remote_node_id")
        })?;
        let remote = managed_follower_repo::find_by_id(context.writer_db(), remote_node_id).await?;
        let encryption_key = &context.config().auth.storage_credential_secret_key;
        StoragePolicyCleanupDriverSnapshot::encode(
            aster_drive_storage::ConnectorId::declared(Self::ID),
            CLEANUP_SNAPSHOT_SCHEMA_VERSION,
            &RemoteCleanupSnapshotV1 {
                id: remote.id,
                name: remote.name,
                base_url: remote.base_url,
                transport_mode: remote.transport_mode,
                access_key_ciphertext: encrypt_remote_snapshot_secret(
                    encryption_key,
                    policy.id,
                    remote.id,
                    "access_key",
                    &remote.access_key,
                )?,
                secret_key_ciphertext: encrypt_remote_snapshot_secret(
                    encryption_key,
                    policy.id,
                    remote.id,
                    "secret_key",
                    &remote.secret_key,
                )?,
                last_capabilities: remote.last_capabilities,
            },
        )
        .map(Some)
    }

    fn cleanup_snapshot_required(&self) -> bool {
        true
    }

    async fn build_cleanup_driver(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        snapshots: StoragePolicyCleanupSnapshots<'_>,
    ) -> Result<Arc<dyn StorageDriver>> {
        let remote = remote_snapshot_from_cleanup_input(snapshots)?;
        let encryption_key = &context.config().auth.storage_credential_secret_key;
        let follower = managed_follower::Model {
            id: remote.id,
            name: remote.name.clone(),
            base_url: remote.base_url.clone(),
            access_key: decrypt_remote_snapshot_secret(
                encryption_key,
                policy.id,
                remote.id,
                "access_key",
                &remote.access_key_ciphertext,
            )?,
            secret_key: decrypt_remote_snapshot_secret(
                encryption_key,
                policy.id,
                remote.id,
                "secret_key",
                &remote.secret_key_ciphertext,
            )?,
            is_enabled: true,
            transport_mode: remote.transport_mode,
            last_capabilities: remote_capabilities_from_snapshot_or_current(
                context.writer_db(),
                &remote,
            )
            .await?,
            last_error: String::new(),
            last_checked_at: None,
            tunnel_last_error: String::new(),
            tunnel_last_seen_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let config = Self::decode_config(policy)?;
        Ok(Arc::new(context.remote_protocol()?.driver_for_config(
            &Self::driver_config(policy, &config),
            &follower,
        )?))
    }
}

fn remote_snapshot_from_cleanup_input(
    snapshots: StoragePolicyCleanupSnapshots<'_>,
) -> Result<RemoteCleanupSnapshotV1> {
    snapshots
        .driver_snapshot
        .ok_or_else(|| {
            AsterError::validation_error("remote storage policy cleanup missing remote snapshot")
        })?
        .decode(RemoteConnector::ID, CLEANUP_SNAPSHOT_SCHEMA_VERSION)
}

fn remote_snapshot_secret_aad(policy_id: i64, remote_node_id: i64, field: &str) -> String {
    // Cleanup tasks are durable background payloads. Bind encrypted remote-node
    // credentials to the deleted policy and node so copied payloads cannot be
    // replayed under another cleanup task.
    format!("storage_policy_cleanup:{policy_id}:remote_node:{remote_node_id}:{field}")
}

fn encrypt_remote_snapshot_secret(
    encryption_key: &str,
    policy_id: i64,
    remote_node_id: i64,
    field: &str,
    plaintext: &str,
) -> Result<String> {
    crypto::encrypt_token(
        encryption_key,
        remote_snapshot_secret_aad(policy_id, remote_node_id, field).as_bytes(),
        plaintext,
    )
}

fn decrypt_remote_snapshot_secret(
    encryption_key: &str,
    policy_id: i64,
    remote_node_id: i64,
    field: &str,
    ciphertext: &str,
) -> Result<String> {
    crypto::decrypt_token(
        encryption_key,
        remote_snapshot_secret_aad(policy_id, remote_node_id, field).as_bytes(),
        ciphertext,
    )
}

async fn remote_capabilities_from_snapshot_or_current(
    db: &sea_orm::DatabaseConnection,
    remote: &RemoteCleanupSnapshotV1,
) -> Result<String> {
    if !remote.last_capabilities.trim().is_empty() {
        return Ok(remote.last_capabilities.clone());
    }

    // Pre-0.3.0 cleanup payloads did not store remote capabilities. Use the
    // current node row only as a fallback so newly created cleanup tasks remain
    // self-contained snapshots.
    managed_follower_repo::find_by_id(db, remote.id)
        .await
        .map(|node| node.last_capabilities)
}
