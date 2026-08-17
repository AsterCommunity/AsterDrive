use std::sync::Arc;

use crate::storage::remote_protocol::RemoteStorageTargetInfo;
use aster_drive_model::entities::remote_storage_target;
use aster_drive_storage::{ConnectorConfigEnvelope, ConnectorId, StorageDriver};

use super::driver::connector_available;

#[derive(Clone)]
pub struct ResolvedRemoteStorageTarget {
    pub driver: Arc<dyn StorageDriver>,
}

pub(super) fn present_target(
    model: remote_storage_target::Model,
    credential_configured: bool,
) -> RemoteStorageTargetInfo {
    let (connector_config, parse_error) =
        match serde_json::from_str::<ConnectorConfigEnvelope>(&model.connector_config) {
            Ok(config) => (config, None),
            Err(_) => (
                ConnectorConfigEnvelope::new(
                    ConnectorId::declared(&model.connector_id),
                    0,
                    Default::default(),
                ),
                Some("stored connector configuration is invalid"),
            ),
        };
    let last_error = match (model.last_error.trim(), parse_error) {
        ("", Some(error)) => error.to_string(),
        (existing, Some(error)) => format!("{existing}; {error}"),
        (_, None) => model.last_error,
    };
    RemoteStorageTargetInfo {
        target_key: model.target_key,
        name: model.name,
        connector_id: model.connector_id.clone(),
        connector_config,
        credential_configured,
        connector_available: parse_error.is_none() && connector_available(&model.connector_id),
        is_default: model.is_default,
        desired_revision: model.desired_revision,
        applied_revision: model.applied_revision,
        last_error,
        created_at: model.created_at,
        updated_at: model.updated_at,
    }
}
