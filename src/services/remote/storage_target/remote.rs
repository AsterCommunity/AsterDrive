use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{policy_repo, system_initialization_repo};
use crate::errors::{Result, precondition_failed_with_code, validation_error_with_code};
use crate::runtime::RemoteProtocolRuntimeState;
use crate::services::remote::capability::RemoteCapabilityResolver;
use crate::services::remote::remote_node;
use crate::storage::remote_protocol::{
    RemoteCreateStorageTargetRequest, RemoteStorageTargetConnectorCatalog, RemoteStorageTargetInfo,
    RemoteUpdateStorageTargetRequest,
};
use aster_drive_model::entities::managed_follower;
use aster_drive_model::types::LocaleTag;
use sea_orm::ConnectionTrait;

async fn referencing_policy_ids<C: ConnectionTrait>(
    state: &impl RemoteProtocolRuntimeState,
    db: &C,
    remote_node_id: i64,
    target_key: &str,
) -> Result<Vec<i64>> {
    let mut ids = Vec::new();
    for policy in policy_repo::find_all(db).await? {
        let Some(binding) = crate::storage::connectors::resolve_remote_policy_binding(
            state.driver_registry().connectors(),
            &policy,
        )?
        else {
            continue;
        };
        if binding.remote_node_id == Some(remote_node_id)
            && binding.remote_storage_target_key.as_deref() == Some(target_key)
        {
            ids.push(policy.id);
        }
    }
    Ok(ids)
}

fn referenced_target_error(target_key: &str, policy_ids: &[i64]) -> crate::errors::AsterError {
    precondition_failed_with_code(
        ApiErrorCode::RemoteStorageTargetReferenced,
        format!(
            "remote storage target '{target_key}' is referenced by storage policies {policy_ids:?}; create a new policy and migrate existing data before changing or deleting this target"
        ),
    )
}

/// Lists storage targets registered on the remote follower, fetched live
/// from the follower's storage-target API rather than cached locally.
pub async fn list_remote<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
) -> Result<Vec<RemoteStorageTargetInfo>> {
    remote_client_for_node(state, remote_node_id)
        .await?
        .list_storage_targets()
        .await
}

/// Fetches the follower's connector catalog (localized) for building remote
/// storage targets on that follower.
pub async fn list_remote_connector_catalog<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
    locale: &LocaleTag,
) -> Result<RemoteStorageTargetConnectorCatalog> {
    remote_client_for_node(state, remote_node_id)
        .await?
        .list_storage_target_connector_catalog(locale)
        .await
}

/// Creates a storage target on the remote follower. The node must have
/// completed enrollment, and the requested connector must be declared in
/// the follower's cached remote storage target capabilities.
pub async fn create_remote<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
    input: RemoteCreateStorageTargetRequest,
) -> Result<RemoteStorageTargetInfo> {
    let node = remote_node_for_storage_target_write(state, remote_node_id).await?;
    RemoteCapabilityResolver::from_remote_node(&node)
        .ensure_remote_storage_target_connector_supported(
            state.driver_registry().connectors(),
            input.connector_id(),
        )?;
    remote_node::remote_storage_client_for_node(state, &node)?
        .create_storage_target(&input)
        .await
}

/// Updates a storage target on the remote follower. When the request carries
/// a connection, the target key must be canonical, the connector immutable,
/// and targets still referenced by storage policies cannot change their
/// connector configuration; these checks run under the storage topology
/// lock against the follower's live target state.
pub async fn update_remote<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
    target_key: &str,
    input: RemoteUpdateStorageTargetRequest,
) -> Result<RemoteStorageTargetInfo> {
    require_canonical_target_key(target_key)?;
    let node = remote_node_for_storage_target_write(state, remote_node_id).await?;
    if let Some(connection) = input.connection.as_ref() {
        RemoteCapabilityResolver::from_remote_node(&node)
            .ensure_remote_storage_target_connector_supported(
                state.driver_registry().connectors(),
                &connection.connector_config.connector_id,
            )?;
        let txn = aster_forge_db::transaction::begin(state.writer_db()).await?;
        system_initialization_repo::acquire_storage_topology_lock(&txn).await?;
        let policy_ids = referencing_policy_ids(state, &txn, remote_node_id, target_key).await?;
        let existing = remote_node::remote_storage_client_for_node(state, &node)?
            .list_storage_targets()
            .await?
            .into_iter()
            .find(|target| target.target_key == target_key)
            .ok_or_else(|| {
                validation_error_with_code(
                    ApiErrorCode::RemoteStorageTargetNotFound,
                    format!("remote storage target '{target_key}' is not configured"),
                )
            })?;
        if existing.connector_id != connection.connector_config.connector_id.as_str() {
            return Err(validation_error_with_code(
                ApiErrorCode::RemoteStorageTargetConnectorUnsupported,
                format!(
                    "remote storage target '{target_key}' connector is immutable; create a new target instead"
                ),
            ));
        }
        if !policy_ids.is_empty()
            && (existing.connector_id != connection.connector_config.connector_id.as_str()
                || existing.connector_config != connection.connector_config)
        {
            return Err(referenced_target_error(target_key, &policy_ids));
        }
        let updated = remote_node::remote_storage_client_for_node(state, &node)?
            .update_storage_target(target_key, &input)
            .await?;
        aster_forge_db::transaction::commit(txn).await?;
        return Ok(updated);
    }
    remote_node::remote_storage_client_for_node(state, &node)?
        .update_storage_target(target_key, &input)
        .await
}

/// Deletes a storage target on the remote follower, rejected while any
/// storage policy still references it. The reference check and the delete
/// call are separated by the storage topology lock, and the transaction
/// commits only after the follower confirms the deletion.
pub async fn delete_remote<S: RemoteProtocolRuntimeState>(
    state: &S,
    remote_node_id: i64,
    target_key: &str,
) -> Result<()> {
    require_canonical_target_key(target_key)?;
    tracing::debug!(
        remote_node_id,
        target_key,
        "deleting remote storage target on remote node"
    );
    let client = remote_client_for_node(state, remote_node_id).await?;
    let txn = aster_forge_db::transaction::begin(state.writer_db()).await?;
    system_initialization_repo::acquire_storage_topology_lock(&txn).await?;
    let policy_ids = referencing_policy_ids(state, &txn, remote_node_id, target_key).await?;
    if !policy_ids.is_empty() {
        return Err(referenced_target_error(target_key, &policy_ids));
    }
    client.delete_storage_target(target_key).await?;
    aster_forge_db::transaction::commit(txn).await?;
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

fn require_canonical_target_key(key: &str) -> Result<()> {
    if key.is_empty() || key != key.trim() || key.chars().any(char::is_control) {
        return Err(crate::errors::AsterError::validation_error(
            "remote storage target key must be nonempty and canonical without surrounding whitespace",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::require_canonical_target_key;
    #[test]
    fn target_keys_are_checked_before_reference_lookup_and_network_selection() {
        assert!(require_canonical_target_key("rst-target").is_ok());
        for key in [
            "",
            " rst-target",
            "rst-target ",
            "\trst-target",
            "rst-\ntarget",
        ] {
            assert!(require_canonical_target_key(key).is_err());
        }
    }
}
