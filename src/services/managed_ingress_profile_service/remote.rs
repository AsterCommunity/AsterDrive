use crate::errors::Result;
use crate::runtime::RemoteProtocolRuntimeState;
use crate::services::managed_follower_service;
use crate::storage::remote_protocol::{
    RemoteCreateIngressProfileRequest, RemoteIngressProfileInfo, RemoteUpdateIngressProfileRequest,
};

pub async fn list_remote<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
) -> Result<Vec<RemoteIngressProfileInfo>> {
    remote_client_for_node(state, remote_node_id)
        .await?
        .list_ingress_profiles()
        .await
}

pub async fn create_remote<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
    input: RemoteCreateIngressProfileRequest,
) -> Result<RemoteIngressProfileInfo> {
    remote_client_for_node(state, remote_node_id)
        .await?
        .create_ingress_profile(&input)
        .await
}

pub async fn update_remote<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
    profile_key: &str,
    input: RemoteUpdateIngressProfileRequest,
) -> Result<RemoteIngressProfileInfo> {
    remote_client_for_node(state, remote_node_id)
        .await?
        .update_ingress_profile(profile_key, &input)
        .await
}

pub async fn delete_remote<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
    profile_key: &str,
) -> Result<()> {
    tracing::debug!(
        remote_node_id,
        profile_key,
        "deleting remote managed ingress profile"
    );
    remote_client_for_node(state, remote_node_id)
        .await?
        .delete_ingress_profile(profile_key)
        .await?;
    tracing::info!(
        remote_node_id,
        profile_key,
        "deleted remote managed ingress profile"
    );
    Ok(())
}

async fn remote_client_for_node<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
) -> Result<crate::storage::remote_protocol::RemoteStorageClient> {
    let node =
        managed_follower_service::require_completed_enrollment(state, remote_node_id).await?;
    managed_follower_service::remote_storage_client_for_node(state, &node)
}
