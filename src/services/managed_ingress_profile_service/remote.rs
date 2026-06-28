use crate::api::api_error_code::ApiErrorCode;
use crate::entities::managed_follower;
use crate::errors::{Result, validation_error_with_code};
use crate::runtime::RemoteProtocolRuntimeState;
use crate::services::managed_follower_service;
use crate::storage::remote_protocol::{
    RemoteCreateIngressProfileRequest, RemoteIngressProfileInfo, RemoteUpdateIngressProfileRequest,
};
use crate::types::DriverType;

use super::driver::{ManagedIngressDriverDescriptor, managed_ingress_driver_descriptor};

pub async fn list_remote<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
) -> Result<Vec<RemoteIngressProfileInfo>> {
    remote_client_for_node(state, remote_node_id)
        .await?
        .list_ingress_profiles()
        .await
}

pub async fn list_remote_driver_descriptors<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
) -> Result<Vec<ManagedIngressDriverDescriptor>> {
    let node = remote_node_for_ingress_write(state, remote_node_id).await?;
    let capabilities = managed_follower_service::parse_capabilities(&node.last_capabilities);
    let managed_ingress = capabilities.effective_managed_ingress();
    let descriptors = managed_ingress
        .driver_types
        .iter()
        .filter_map(|driver_type| driver_type.as_known_driver_type())
        .filter(|driver_type| managed_ingress.supports_known_driver(*driver_type))
        .filter_map(|driver_type| managed_ingress_driver_descriptor(driver_type).ok())
        .collect();

    Ok(descriptors)
}

pub async fn create_remote<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
    input: RemoteCreateIngressProfileRequest,
) -> Result<RemoteIngressProfileInfo> {
    let node = remote_node_for_ingress_write(state, remote_node_id).await?;
    ensure_remote_ingress_driver_supported(&node, input.driver_type())?;
    managed_follower_service::remote_storage_client_for_node(state, &node)?
        .create_ingress_profile(&input)
        .await
}

pub async fn update_remote<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
    profile_key: &str,
    input: RemoteUpdateIngressProfileRequest,
) -> Result<RemoteIngressProfileInfo> {
    let node = remote_node_for_ingress_write(state, remote_node_id).await?;
    if let Some(driver_type) = input.driver_type {
        ensure_remote_ingress_driver_supported(&node, driver_type)?;
    }
    managed_follower_service::remote_storage_client_for_node(state, &node)?
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
    let node = remote_node_for_ingress_write(state, remote_node_id).await?;
    managed_follower_service::remote_storage_client_for_node(state, &node)
}

async fn remote_node_for_ingress_write<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
) -> Result<managed_follower::Model> {
    managed_follower_service::require_completed_enrollment(state, remote_node_id).await
}

fn ensure_remote_ingress_driver_supported(
    node: &managed_follower::Model,
    driver_type: DriverType,
) -> Result<()> {
    let capabilities = managed_follower_service::parse_capabilities(&node.last_capabilities);
    if capabilities
        .effective_managed_ingress()
        .supports_known_driver(driver_type)
    {
        return Ok(());
    }

    Err(validation_error_with_code(
        ApiErrorCode::ManagedIngressDriverUnsupported,
        format!(
            "remote node #{} does not declare managed ingress support for the {} driver",
            node.id,
            driver_type.as_str()
        ),
    ))
}
