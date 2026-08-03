use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::managed_follower_repo;
use crate::errors::{
    AsterError, Result, precondition_failed_with_code, validation_error_with_code,
};
use crate::services::remote::capability::RemoteCapabilityResolver;
use crate::services::storage_policy::credential::crypto;
use crate::storage::drivers::remote::RemoteDriver;
use aster_drive_model::entities::{managed_follower, storage_policy};
use aster_drive_model::types::{
    RemoteDownloadStrategy, RemoteNodeTransportMode, RemoteUploadStrategy,
    parse_storage_policy_options,
};
use aster_drive_storage::connector_descriptor::{
    ObjectMultipartUploadCapabilitiesInput, StorageConnectorCapabilities,
    StorageConnectorDeploymentScope, StorageConnectorDescriptor, StorageConnectorFieldKind,
    StorageConnectorFieldScope, StorageConnectorObjectNamingMode,
    StorageConnectorUiDescriptorInput, StorageConnectorUploadWorkflows,
    draft_connection_test_action_descriptor, object_multipart_upload_capabilities,
    saved_connection_test_action_descriptor, server_relay_simple_upload_capabilities,
    storage_connector_field, storage_connector_field_with_options, storage_connector_ui_descriptor,
};
use aster_drive_storage::{StorageConnectorConfigSchema, StorageConnectorFieldDefaultValue};
use aster_drive_storage::{StorageDriver, StorageErrorKind, storage_driver_error};

use super::common::{ensure_onedrive_options_absent, ensure_storage_native_processing_supported};
use super::{
    StorageConnector, StorageConnectorConnectionInput, StorageConnectorUploadTransport,
    StoragePolicyCleanupDriverSnapshot, StoragePolicyCleanupRemoteNodeSnapshot,
    StoragePolicyCleanupSnapshots,
};

pub struct RemoteConnector;

aster_drive_storage::storage_connector_schema! {
    pub struct RemoteConnectorConfigV1 {
        config {
        pub base_path: String => storage_connector_field(
            "base_path", StorageConnectorFieldScope::ConnectorConfig,
            StorageConnectorFieldKind::Text, false, false,
        ),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub remote_node_id: Option<i64> => storage_connector_field(
            "remote_node_id", StorageConnectorFieldScope::ConnectorConfig,
            StorageConnectorFieldKind::Select, true, false,
        ),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub remote_storage_target_key: Option<String> => storage_connector_field(
            "remote_storage_target_key", StorageConnectorFieldScope::ConnectorConfig,
            StorageConnectorFieldKind::Select, true, false,
        ),
        pub remote_download_strategy: RemoteDownloadStrategy => remote_transfer_field(
            "remote_download_strategy",
        ),
        pub remote_upload_strategy: RemoteUploadStrategy => remote_transfer_field(
            "remote_upload_strategy",
        ),
        }
        credentials none
    }
}

fn remote_transfer_field(name: &str) -> aster_drive_storage::StorageConnectorFieldDescriptor {
    let mut field = storage_connector_field_with_options(
        name,
        StorageConnectorFieldScope::ConnectorConfig,
        StorageConnectorFieldKind::Select,
        true,
        false,
        vec!["relay_stream", "presigned"],
    );
    field.default_value = Some(StorageConnectorFieldDefaultValue::String(
        "relay_stream".to_string(),
    ));
    field
}

impl RemoteConnector {
    pub const ID: &'static str = "asterdrive.storage.remote";
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
            actions: vec![
                draft_connection_test_action_descriptor(),
                saved_connection_test_action_descriptor(false),
            ],
            driver_recommendations: Vec::new(),
            related_issues: vec![328, 329],
        }
    }
}

#[async_trait]
impl StorageConnector for RemoteConnector {
    fn descriptor(&self) -> StorageConnectorDescriptor {
        Self::descriptor_definition()
    }

    fn encode_config(
        &self,
        input: &StorageConnectorConnectionInput,
    ) -> Result<aster_drive_model::types::StoredConnectorConfig> {
        super::common::encode_typed_connector_config(
            Self::ID,
            1,
            RemoteConnectorConfigV1 {
                base_path: input.base_path.clone(),
                remote_node_id: input.remote_node_id,
                remote_storage_target_key: input.remote_storage_target_key.clone(),
                remote_download_strategy: input.options.effective_remote_download_strategy(),
                remote_upload_strategy: input.options.effective_remote_upload_strategy(),
            },
        )
    }

    fn normalize_connection_fields(
        &self,
        endpoint: &str,
        bucket: &str,
    ) -> Result<(String, String)> {
        let _ = (endpoint, bucket);
        Ok((String::new(), String::new()))
    }

    fn validate_connection_credentials(
        &self,
        input: &StorageConnectorConnectionInput,
    ) -> Result<()> {
        let _ = input;
        Ok(())
    }

    async fn validate_connection_binding(
        &self,
        db: &sea_orm::DatabaseConnection,
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

    async fn validate_policy_options(
        &self,
        db: &sea_orm::DatabaseConnection,
        remote_node_id: Option<i64>,
        options: &aster_drive_model::types::StoragePolicyOptions,
    ) -> Result<()> {
        ensure_storage_native_processing_supported(self.descriptor(), options)?;
        ensure_onedrive_options_absent(options)?;
        let Some(remote_node_id) = remote_node_id else {
            return Ok(());
        };
        let remote_node = managed_follower_repo::find_by_id(db, remote_node_id).await?;
        if remote_node
            .transport_mode
            .resolves_to_reverse_tunnel(&remote_node.base_url)
            && (options.effective_remote_download_strategy()
                == aster_drive_model::types::RemoteDownloadStrategy::Presigned
                || options.effective_remote_upload_strategy()
                    == aster_drive_model::types::RemoteUploadStrategy::Presigned)
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
    ) -> Result<Box<dyn StorageDriver>> {
        let remote_node_id = policy.remote_node_id.ok_or_else(|| {
            validation_error_with_code(
                ApiErrorCode::PolicyRemoteNodeRequired,
                "remote storage policy requires remote_node_id",
            )
        })?;
        let remote_node =
            managed_follower_repo::find_by_id(context.writer_db(), remote_node_id).await?;
        Ok(Box::new(
            context
                .remote_protocol()?
                .driver_for_policy(policy, &remote_node)?,
        ))
    }

    fn build_runtime_driver(
        &self,
        registry: &crate::storage::DriverRegistry,
        policy: &storage_policy::Model,
    ) -> Result<super::StorageConnectorDriver> {
        let remote_node_id = policy.remote_node_id.ok_or_else(|| {
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
        let options = parse_storage_policy_options(policy.options.as_ref());
        if let Err(error) = capabilities.ensure_remote_policy_options_supported(policy.id, &options)
        {
            tracing::warn!(
                remote_node_id,
                policy_id = policy.id,
                protocol_version = %capabilities.capabilities().protocol_version,
                min_supported_protocol_version = %capabilities.capabilities().min_supported_protocol_version,
                "remote storage policy protocol compatibility check failed: {error}"
            );
            return Err(error);
        }
        let driver = if let Some(remote_protocol) = registry.remote_protocol() {
            Arc::new(remote_protocol.driver_for_policy(policy, &remote_node)?)
        } else {
            Arc::new(RemoteDriver::new(policy, &remote_node)?)
        };
        Ok(super::StorageConnectorDriver::multipart(driver))
    }

    fn upload_transport(&self, policy: &storage_policy::Model) -> StorageConnectorUploadTransport {
        let options = parse_storage_policy_options(policy.options.as_ref());
        StorageConnectorUploadTransport::Remote(options.effective_remote_upload_strategy())
    }

    fn presigned_download_enabled(&self, policy: &storage_policy::Model) -> bool {
        let options = parse_storage_policy_options(policy.options.as_ref());
        options.effective_remote_download_strategy()
            == aster_drive_model::types::RemoteDownloadStrategy::Presigned
    }
    async fn cleanup_snapshot_for_policy(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
    ) -> Result<Option<StoragePolicyCleanupDriverSnapshot>> {
        let remote_node_id = policy.remote_node_id.ok_or_else(|| {
            AsterError::validation_error("remote storage policy requires remote_node_id")
        })?;
        let remote = managed_follower_repo::find_by_id(context.writer_db(), remote_node_id).await?;
        let encryption_key = &context.config().auth.storage_credential_secret_key;
        Ok(Some(StoragePolicyCleanupDriverSnapshot::RemoteNode(
            StoragePolicyCleanupRemoteNodeSnapshot {
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
        )))
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
                remote,
            )
            .await?,
            last_error: String::new(),
            last_checked_at: None,
            tunnel_last_error: String::new(),
            tunnel_last_seen_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        Ok(Arc::new(
            context
                .remote_protocol()?
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
    remote: &StoragePolicyCleanupRemoteNodeSnapshot,
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
