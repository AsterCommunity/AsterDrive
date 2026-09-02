use crate::errors::Result;
use crate::runtime::RemoteProtocolRuntimeState;
use crate::services::remote::capability::RemoteCapabilityResolver;
use crate::services::remote::remote_node;
use crate::storage::remote_protocol::{
    RemoteCreateStorageTargetRequest, RemoteStorageTargetInfo, RemoteUpdateStorageTargetRequest,
};
use aster_drive_model::entities::managed_follower;

use super::driver::{
    RemoteStorageTargetDriverDescriptor, remote_storage_target_descriptor_from_connector,
};

pub async fn list_remote<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
) -> Result<Vec<RemoteStorageTargetInfo>> {
    remote_client_for_node(state, remote_node_id)
        .await?
        .list_storage_targets()
        .await
}

pub async fn list_remote_driver_descriptors<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
) -> Result<Vec<RemoteStorageTargetDriverDescriptor>> {
    let _node = remote_node_for_storage_target_write(state, remote_node_id).await?;
    state
        .driver_registry()
        .connectors()
        .remote_target_connectors()
        .into_iter()
        .map(remote_storage_target_descriptor_from_connector)
        .collect()
}

pub async fn create_remote<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
    input: RemoteCreateStorageTargetRequest,
) -> Result<RemoteStorageTargetInfo> {
    let node = remote_node_for_storage_target_write(state, remote_node_id).await?;
    ensure_remote_storage_target_connector_supported(&node, input.connector_id())?;
    remote_node::remote_storage_client_for_node(state, &node)?
        .create_storage_target(&input)
        .await
}

pub async fn update_remote<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
    target_key: &str,
    input: RemoteUpdateStorageTargetRequest,
) -> Result<RemoteStorageTargetInfo> {
    let node = remote_node_for_storage_target_write(state, remote_node_id).await?;
    if let Some(config) = input.connector_config.as_ref() {
        ensure_remote_storage_target_connector_supported(&node, &config.connector_id)?;
    } else if let Some(driver) = input.driver_type {
        let connector_id = super::driver::remote_storage_target_connector_id(driver)?;
        ensure_remote_storage_target_connector_supported(&node, &connector_id)?;
    }
    remote_node::remote_storage_client_for_node(state, &node)?
        .update_storage_target(target_key, &input)
        .await
}

pub async fn delete_remote<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
    target_key: &str,
) -> Result<()> {
    tracing::debug!(
        remote_node_id,
        target_key,
        "deleting remote storage target on remote node"
    );
    remote_client_for_node(state, remote_node_id)
        .await?
        .delete_storage_target(target_key)
        .await?;
    tracing::info!(
        remote_node_id,
        target_key,
        "deleted remote storage target on remote node"
    );
    Ok(())
}

async fn remote_client_for_node<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
) -> Result<crate::storage::remote_protocol::RemoteStorageClient> {
    let node = remote_node_for_storage_target_write(state, remote_node_id).await?;
    remote_node::remote_storage_client_for_node(state, &node)
}

async fn remote_node_for_storage_target_write<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
) -> Result<managed_follower::Model> {
    remote_node::require_completed_enrollment(state, remote_node_id).await
}

fn ensure_remote_storage_target_connector_supported(
    node: &managed_follower::Model,
    connector_id: &aster_drive_storage::ConnectorId,
) -> Result<()> {
    let resolver = remote_capability_resolver(node);
    if resolver.supports_remote_storage_target_connector(connector_id) {
        return Ok(());
    }
    resolver.ensure_remote_storage_target_driver_supported(
        connector_id
            .as_str()
            .rsplit_once('.')
            .and_then(|(_, suffix)| suffix.parse().ok())
            .unwrap_or(aster_drive_model::types::RemoteStorageTargetDriverKind::S3),
    )
}

fn remote_capability_resolver(node: &managed_follower::Model) -> RemoteCapabilityResolver {
    RemoteCapabilityResolver::from_remote_node(node)
}
