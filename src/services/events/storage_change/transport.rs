use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::{StorageChangeAudience, StorageChangeEvent};
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState, StorageChangeRuntimeState};

const STORAGE_CHANGE_TOPIC_SUFFIX: &str = "storage_events";

#[derive(Debug, Serialize, Deserialize)]
struct StorageChangeTransportMessage {
    origin_runtime_id: String,
    audience: StorageChangeAudience,
    event: StorageChangeEvent,
}

pub fn build_cross_instance_bus(
    config: &aster_forge_config::ConfigSyncConfig,
) -> Result<Option<Arc<aster_forge_events::RedisEventBus>>> {
    if config.backend.trim().eq_ignore_ascii_case("redis") {
        let topic = storage_change_topic(&config.topic);
        return aster_forge_events::RedisEventBus::from_url(&config.endpoint, topic)
            .map(Arc::new)
            .map(Some)
            .map_err(|error| {
                AsterError::internal_error(format!(
                    "failed to configure cross-instance storage events: {error}"
                ))
            });
    }
    Ok(None)
}

pub(super) fn publish_cross_instance<S: StorageChangeRuntimeState>(
    state: &S,
    event: &StorageChangeEvent,
) {
    let Some(bus) = state.storage_change_bus().cloned() else {
        return;
    };
    let payload = match encode_transport_message(state.config_sync().runtime_id(), event) {
        Ok(payload) => payload,
        Err(error) => {
            tracing::warn!(%error, "failed to encode cross-instance storage event");
            return;
        }
    };
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        tracing::warn!("skip cross-instance storage event publish without Tokio runtime");
        return;
    };
    drop(handle.spawn(async move {
        if let Err(error) = bus.publish(payload).await {
            tracing::warn!(%error, "failed to publish cross-instance storage event");
        }
    }));
}

pub async fn run_cross_instance_subscription(
    state: Arc<PrimaryAppState>,
    shutdown: CancellationToken,
) {
    let Some(bus) = state.storage_change_bus.clone() else {
        return;
    };
    let runtime_id = state.config_sync().runtime_id().to_string();
    let observer_state = state.clone();
    let observer = move |observation: aster_forge_events::EventConnectionObservation| {
        let state_label = match observation.state {
            aster_forge_events::EventConnectionState::Connected => "connected",
            aster_forge_events::EventConnectionState::Disconnected => "disconnected",
            aster_forge_events::EventConnectionState::Reconnecting => "reconnecting",
            aster_forge_events::EventConnectionState::Recovered => "recovered",
        };
        tracing::info!(
            state = state_label,
            reconnect_attempt = observation.reconnect_attempt,
            backoff_ms = observation.backoff.as_millis(),
            "cross-instance storage event subscription state changed"
        );
        if observation.state == aster_forge_events::EventConnectionState::Disconnected {
            super::publish_local(observer_state.as_ref(), StorageChangeEvent::sync_required());
        }
    };

    bus.run_subscription(shutdown, Some(&observer), move |payload| {
        let state = state.clone();
        let runtime_id = runtime_id.clone();
        async move {
            match decode_remote_event(&runtime_id, &payload) {
                Ok(Some(event)) => super::publish_local(state.as_ref(), event),
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(%error, "failed to parse cross-instance storage event");
                }
            }
        }
    })
    .await;
}

fn storage_change_topic(config_topic: &str) -> String {
    let topic = config_topic.trim();
    if let Some(prefix) = topic.strip_suffix(".config_reload") {
        format!("{prefix}.{STORAGE_CHANGE_TOPIC_SUFFIX}")
    } else {
        format!("{topic}.{STORAGE_CHANGE_TOPIC_SUFFIX}")
    }
}

fn encode_transport_message(
    origin_runtime_id: &str,
    event: &StorageChangeEvent,
) -> serde_json::Result<String> {
    serde_json::to_string(&StorageChangeTransportMessage {
        origin_runtime_id: origin_runtime_id.to_string(),
        audience: event.audience,
        event: event.clone(),
    })
}

fn decode_remote_event(
    current_runtime_id: &str,
    payload: &str,
) -> serde_json::Result<Option<StorageChangeEvent>> {
    let message: StorageChangeTransportMessage = serde_json::from_str(payload)?;
    if message.origin_runtime_id == current_runtime_id {
        return Ok(None);
    }
    let mut event = message.event;
    event.audience = message.audience;
    Ok(Some(event))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{
        build_cross_instance_bus, decode_remote_event, encode_transport_message,
        storage_change_topic,
    };
    use crate::services::events::storage_change::{StorageChangeEvent, StorageChangeKind};
    use crate::services::workspace::storage::WorkspaceStorageScope;
    use aster_forge_config::ConfigSyncConfig;

    #[test]
    fn derives_storage_topic_without_reusing_config_channel() {
        assert_eq!(
            storage_change_topic("aster_drive.config_reload"),
            "aster_drive.storage_events"
        );
        assert_eq!(
            storage_change_topic("custom.notifications"),
            "custom.notifications.storage_events"
        );
    }

    #[test]
    fn transport_round_trip_preserves_private_audience() {
        let event = StorageChangeEvent::new(
            StorageChangeKind::FolderCreated,
            WorkspaceStorageScope::Personal { user_id: 42 },
            Vec::new(),
            vec![7],
            vec![None],
        );
        let payload = encode_transport_message("runtime-a", &event).unwrap();
        let decoded = decode_remote_event("runtime-b", &payload)
            .unwrap()
            .expect("remote event should be accepted");

        assert!(decoded.is_visible_to(42, &HashSet::new()));
        assert!(!decoded.is_visible_to(41, &HashSet::new()));
        assert_eq!(decoded.kind, StorageChangeKind::FolderCreated);
        assert_eq!(decoded.folder_ids, vec![7]);
    }

    #[test]
    fn transport_round_trip_preserves_team_audience() {
        let event = StorageChangeEvent::new(
            StorageChangeKind::FolderCreated,
            WorkspaceStorageScope::Team {
                team_id: 19,
                actor_user_id: 42,
            },
            Vec::new(),
            vec![7],
            vec![None],
        );
        let payload = encode_transport_message("runtime-a", &event).unwrap();
        let decoded = decode_remote_event("runtime-b", &payload)
            .unwrap()
            .expect("remote event should be accepted");

        let mut memberships = HashSet::new();
        memberships.insert(19);
        assert!(decoded.is_visible_to(42, &memberships));
        assert!(!decoded.is_visible_to(41, &HashSet::new()));
        assert_eq!(decoded.workspace, event.workspace);
        assert_eq!(decoded.folder_ids, vec![7]);
    }

    #[test]
    fn cross_instance_bus_is_disabled_without_redis_backend() {
        let config = ConfigSyncConfig::default();
        assert!(
            build_cross_instance_bus(&config)
                .expect("disabled config should be accepted")
                .is_none()
        );

        let config = ConfigSyncConfig {
            backend: " NONE ".to_string(),
            ..ConfigSyncConfig::default()
        };
        assert!(
            build_cross_instance_bus(&config)
                .expect("none backend should be accepted")
                .is_none()
        );
    }

    #[test]
    fn cross_instance_bus_builds_for_valid_redis_endpoint() {
        let config = ConfigSyncConfig {
            backend: "redis".to_string(),
            endpoint: "redis://127.0.0.1:6379/0".to_string(),
            topic: "aster_drive.config_reload".to_string(),
        };
        let bus = build_cross_instance_bus(&config)
            .expect("valid Redis config should be accepted")
            .expect("Redis backend should create a bus");
        assert_eq!(bus.topic(), "aster_drive.storage_events");
    }

    #[test]
    fn cross_instance_bus_rejects_empty_or_invalid_redis_endpoint() {
        for endpoint in ["", "not a redis url"] {
            let config = ConfigSyncConfig {
                backend: "redis".to_string(),
                endpoint: endpoint.to_string(),
                topic: "aster_drive.config_reload".to_string(),
            };
            let error = match build_cross_instance_bus(&config) {
                Ok(_) => panic!("invalid Redis endpoint should fail configuration"),
                Err(error) => error,
            };
            assert!(error.to_string().contains("cross-instance storage events"));
        }
    }

    #[test]
    fn transport_suppresses_self_echo() {
        let event = StorageChangeEvent::sync_required();
        let payload = encode_transport_message("runtime-a", &event).unwrap();

        assert!(
            decode_remote_event("runtime-a", &payload)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn malformed_transport_payload_is_rejected() {
        assert!(decode_remote_event("runtime-a", "not-json").is_err());
    }
}
