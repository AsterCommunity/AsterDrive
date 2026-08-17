use super::driver::RemoteStorageTargetConnectorDescriptor;
use crate::errors::Result;
use crate::runtime::RemoteProtocolRuntimeState;
use crate::services::remote::capability::RemoteCapabilityResolver;
use crate::services::remote::remote_node;
use crate::storage::remote_protocol::{
    RemoteCreateStorageTargetRequest, RemoteStorageTargetInfo, RemoteUpdateStorageTargetRequest,
};
use aster_drive_model::entities::managed_follower;

pub async fn list_remote<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
) -> Result<Vec<RemoteStorageTargetInfo>> {
    remote_client_for_node(state, remote_node_id)
        .await?
        .list_storage_targets()
        .await
}

pub async fn list_remote_connector_descriptors<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
) -> Result<Vec<RemoteStorageTargetConnectorDescriptor>> {
    let node = remote_node_for_storage_target_write(state, remote_node_id).await?;
    Ok(remote_capability_resolver(&node).remote_storage_target_connector_descriptors())
}

pub async fn create_remote<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
    input: RemoteCreateStorageTargetRequest,
) -> Result<RemoteStorageTargetInfo> {
    let node = remote_node_for_storage_target_write(state, remote_node_id).await?;
    ensure_remote_storage_target_connector_supported(
        &node,
        input.connector_config.connector_id.as_str(),
    )?;
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
    if let Some(config) = &input.connector_config {
        ensure_remote_storage_target_connector_supported(&node, config.connector_id.as_str())?;
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
    connector_id: &str,
) -> Result<()> {
    remote_capability_resolver(node).ensure_remote_storage_target_connector_supported(connector_id)
}

fn remote_capability_resolver(node: &managed_follower::Model) -> RemoteCapabilityResolver {
    RemoteCapabilityResolver::from_remote_node(node)
}
