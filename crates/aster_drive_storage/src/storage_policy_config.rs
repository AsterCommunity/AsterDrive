//! Versioned persisted configuration for one storage policy.
//!
//! The database stores one atomic envelope, while ownership remains explicit:
//! the selected connector owns `connector`, and AsterDrive core owns
//! `behavior`. Plugins never deserialize or redefine core behavior fields.

use std::fmt;

use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::connector_config::{
    CONNECTOR_CONFIG_FORMAT_VERSION, ConnectorConfigEnvelope, ConnectorId,
};
use crate::policy_behavior::{
    STORAGE_POLICY_BEHAVIOR_FORMAT_VERSION, STORAGE_POLICY_BEHAVIOR_SCHEMA_VERSION,
    StoragePolicyBehaviorConfig, StoragePolicyBehaviorConfigEnvelope,
};

/// Envelope layout version for the combined storage policy configuration.
pub const STORAGE_POLICY_CONFIG_FORMAT_VERSION: u32 = 1;

/// Atomic persisted configuration for one storage policy.
///
/// Ownership is split explicitly: the selected connector owns `connector`,
/// AsterDrive core owns `behavior`. The whole envelope is stored and decoded
/// as one unit so a partial upgrade can never mix versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyConfigEnvelope<T = serde_json::Value> {
    /// Envelope layout version; decoders expect
    /// [`STORAGE_POLICY_CONFIG_FORMAT_VERSION`].
    pub format_version: u32,
    /// Connector-owned configuration section.
    pub connector: ConnectorConfigEnvelope<T>,
    /// Core-owned behavior section.
    pub behavior: StoragePolicyBehaviorConfigEnvelope,
}

impl<T> StoragePolicyConfigEnvelope<T> {
    /// Builds the envelope with the current format version and wraps
    /// `behavior` in its own versioned envelope.
    pub fn new(
        connector: ConnectorConfigEnvelope<T>,
        behavior: StoragePolicyBehaviorConfig,
    ) -> Self {
        Self {
            format_version: STORAGE_POLICY_CONFIG_FORMAT_VERSION,
            connector,
            behavior: StoragePolicyBehaviorConfigEnvelope::new(behavior),
        }
    }
}

/// Failure modes of the combined storage policy config codec.
#[derive(Debug)]
pub enum StoragePolicyConfigCodecError {
    /// The payload is not valid JSON or contains unknown fields.
    InvalidJson(serde_json::Error),
    /// Top-level envelope layout version mismatch.
    FormatVersionMismatch {
        /// Envelope layout version this build was compiled with.
        expected: u32,
        /// Envelope layout version stored in the payload.
        actual: u32,
    },
    /// Connector section layout version mismatch.
    ConnectorFormatVersionMismatch {
        /// Connector envelope layout version this build understands.
        expected: u32,
        /// Connector envelope layout version stored in the payload.
        actual: u32,
    },
    /// The connector section was persisted by a different connector.
    ConnectorIdMismatch {
        /// Connector identifier the caller expected.
        expected: String,
        /// Connector identifier stored in the payload.
        actual: String,
    },
    /// Connector schema version differs from the caller's expected value.
    ConnectorSchemaVersionMismatch {
        /// Connector schema version the caller expects.
        expected: u32,
        /// Connector schema version stored in the payload.
        actual: u32,
    },
    /// Behavior section layout version mismatch.
    BehaviorFormatVersionMismatch {
        /// Behavior envelope layout version this build understands.
        expected: u32,
        /// Behavior envelope layout version stored in the payload.
        actual: u32,
    },
    /// Behavior schema version mismatch.
    BehaviorSchemaVersionMismatch {
        /// Behavior schema version this build understands.
        expected: u32,
        /// Behavior schema version stored in the payload.
        actual: u32,
    },
}

impl fmt::Display for StoragePolicyConfigCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(error) => {
                write!(formatter, "invalid storage policy config JSON: {error}")
            }
            Self::FormatVersionMismatch { expected, actual } => write!(
                formatter,
                "storage policy config format version mismatch: expected {expected}, got {actual}"
            ),
            Self::ConnectorFormatVersionMismatch { expected, actual } => write!(
                formatter,
                "connector config format version mismatch: expected {expected}, got {actual}"
            ),
            Self::ConnectorIdMismatch { expected, actual } => write!(
                formatter,
                "connector config id mismatch: expected '{expected}', got '{actual}'"
            ),
            Self::ConnectorSchemaVersionMismatch { expected, actual } => write!(
                formatter,
                "connector config schema version mismatch: expected {expected}, got {actual}"
            ),
            Self::BehaviorFormatVersionMismatch { expected, actual } => write!(
                formatter,
                "policy behavior format version mismatch: expected {expected}, got {actual}"
            ),
            Self::BehaviorSchemaVersionMismatch { expected, actual } => write!(
                formatter,
                "policy behavior schema version mismatch: expected {expected}, got {actual}"
            ),
        }
    }
}

impl std::error::Error for StoragePolicyConfigCodecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidJson(error) => Some(error),
            _ => None,
        }
    }
}

/// Serializes the combined connector + behavior configuration into one
/// versioned envelope string for persistence.
///
/// Serialization failure maps to
/// [`StoragePolicyConfigCodecError::InvalidJson`]; ownership validation is
/// deferred to [`decode_storage_policy_config`].
pub fn encode_storage_policy_config(
    connector: ConnectorConfigEnvelope<serde_json::Value>,
    behavior: StoragePolicyBehaviorConfig,
) -> Result<String, StoragePolicyConfigCodecError> {
    serde_json::to_string(&StoragePolicyConfigEnvelope::new(connector, behavior))
        .map_err(StoragePolicyConfigCodecError::InvalidJson)
}

/// Decodes the combined envelope and returns both owned sections as typed values.
///
/// Every ownership marker must match: the top-level format version, the
/// connector section's format version, connector id, and schema version
/// (`expected_connector_id` / `expected_connector_schema_version`), plus the
/// behavior section's format and schema versions. Any mismatch is reported as
/// the corresponding [`StoragePolicyConfigCodecError`] variant so a payload
/// written by another connector or older schema is never silently reinterpreted.
pub fn decode_storage_policy_config<'a, T: Deserialize<'a>>(
    raw: &'a str,
    expected_connector_id: &ConnectorId,
    expected_connector_schema_version: u32,
) -> Result<(T, StoragePolicyBehaviorConfig), StoragePolicyConfigCodecError> {
    let envelope: StoragePolicyConfigEnvelope<T> =
        serde_json::from_str(raw).map_err(StoragePolicyConfigCodecError::InvalidJson)?;

    validate_envelope(
        &envelope,
        expected_connector_id,
        expected_connector_schema_version,
    )?;
    Ok((envelope.connector.values, envelope.behavior.values))
}

fn validate_envelope<T>(
    envelope: &StoragePolicyConfigEnvelope<T>,
    expected_connector_id: &ConnectorId,
    expected_connector_schema_version: u32,
) -> Result<(), StoragePolicyConfigCodecError> {
    if envelope.format_version != STORAGE_POLICY_CONFIG_FORMAT_VERSION {
        return Err(StoragePolicyConfigCodecError::FormatVersionMismatch {
            expected: STORAGE_POLICY_CONFIG_FORMAT_VERSION,
            actual: envelope.format_version,
        });
    }
    if envelope.connector.format_version != CONNECTOR_CONFIG_FORMAT_VERSION {
        return Err(
            StoragePolicyConfigCodecError::ConnectorFormatVersionMismatch {
                expected: CONNECTOR_CONFIG_FORMAT_VERSION,
                actual: envelope.connector.format_version,
            },
        );
    }
    if &envelope.connector.connector_id != expected_connector_id {
        return Err(StoragePolicyConfigCodecError::ConnectorIdMismatch {
            expected: expected_connector_id.as_str().to_string(),
            actual: envelope.connector.connector_id.as_str().to_string(),
        });
    }
    if envelope.connector.schema_version != expected_connector_schema_version {
        return Err(
            StoragePolicyConfigCodecError::ConnectorSchemaVersionMismatch {
                expected: expected_connector_schema_version,
                actual: envelope.connector.schema_version,
            },
        );
    }
    if envelope.behavior.format_version != STORAGE_POLICY_BEHAVIOR_FORMAT_VERSION {
        return Err(
            StoragePolicyConfigCodecError::BehaviorFormatVersionMismatch {
                expected: STORAGE_POLICY_BEHAVIOR_FORMAT_VERSION,
                actual: envelope.behavior.format_version,
            },
        );
    }
    if envelope.behavior.schema_version != STORAGE_POLICY_BEHAVIOR_SCHEMA_VERSION {
        return Err(
            StoragePolicyConfigCodecError::BehaviorSchemaVersionMismatch {
                expected: STORAGE_POLICY_BEHAVIOR_SCHEMA_VERSION,
                actual: envelope.behavior.schema_version,
            },
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TestConnectorConfig {
        base_path: String,
    }

    fn encoded() -> String {
        encode_storage_policy_config(
            ConnectorConfigEnvelope::new(
                ConnectorId::declared("com.example.storage"),
                3,
                serde_json::to_value(TestConnectorConfig {
                    base_path: "archive".to_string(),
                })
                .unwrap(),
            ),
            StoragePolicyBehaviorConfig::default(),
        )
        .unwrap()
    }

    #[test]
    fn combined_envelope_round_trips_owned_sections() {
        let (connector, behavior) = decode_storage_policy_config::<TestConnectorConfig>(
            &encoded(),
            &ConnectorId::declared("com.example.storage"),
            3,
        )
        .unwrap();

        assert_eq!(
            connector,
            TestConnectorConfig {
                base_path: "archive".to_string()
            }
        );
        assert_eq!(behavior, StoragePolicyBehaviorConfig::default());
    }

    #[test]
    fn combined_envelope_rejects_unknown_and_mismatched_sections() {
        let unknown = r#"{"format_version":1,"connector":{"format_version":1,"connector_id":"com.example.storage","schema_version":3,"values":{"base_path":"archive"}},"behavior":{"format_version":1,"schema_version":1,"values":{}},"unknown":true}"#;
        assert!(matches!(
            decode_storage_policy_config::<TestConnectorConfig>(
                unknown,
                &ConnectorId::declared("com.example.storage"),
                3
            ),
            Err(StoragePolicyConfigCodecError::InvalidJson(_))
        ));

        assert!(matches!(
            decode_storage_policy_config::<TestConnectorConfig>(
                &encoded(),
                &ConnectorId::declared("com.example.other"),
                3
            ),
            Err(StoragePolicyConfigCodecError::ConnectorIdMismatch { .. })
        ));
    }
}
