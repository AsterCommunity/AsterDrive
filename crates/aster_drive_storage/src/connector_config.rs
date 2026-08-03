//! Plugin-safe connector identity and persisted configuration envelope.
//!
//! Core services treat `values` as opaque after generic schema validation. A
//! connector owns deserialization into its private typed config, semantic
//! validation, schema upgrades, and runtime-driver construction.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

pub const CONNECTOR_CONFIG_FORMAT_VERSION: u32 = 1;

/// Stable connector/plugin identifier.
///
/// Built-in connectors use reverse-DNS-style identifiers such as
/// `asterdrive.storage.local`. Dynamically loaded plugins use the same type and
/// registry path, so core code never needs a built-in connector enum.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ConnectorId(String);

impl ConnectorId {
    pub fn declared(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn validate(&self) -> Result<(), ConnectorIdError> {
        validate_connector_id(self.as_str())
    }
}

impl fmt::Display for ConnectorId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorIdError;

impl fmt::Display for ConnectorIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "connector id must be 3-128 lowercase ASCII letters, digits, '.', '-' or '_' and contain no empty segments",
        )
    }
}

impl std::error::Error for ConnectorIdError {}

fn validate_connector_id(value: &str) -> Result<(), ConnectorIdError> {
    if !(3..=128).contains(&value.len())
        || value.starts_with('.')
        || value.ends_with('.')
        || value.contains("..")
        || value.bytes().any(|byte| {
            !byte.is_ascii_lowercase()
                && !byte.is_ascii_digit()
                && !matches!(byte, b'.' | b'-' | b'_')
        })
    {
        return Err(ConnectorIdError);
    }
    Ok(())
}

/// Persisted configuration for exactly one connector.
///
/// A storage policy currently has one active connector, so a map of historical
/// namespaces would only create ambiguous ownership. If the connector is
/// temporarily unavailable, this entire envelope is preserved byte-for-byte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ConnectorConfigEnvelope {
    pub format_version: u32,
    pub connector_id: ConnectorId,
    pub schema_version: u32,
    #[serde(default)]
    pub values: BTreeMap<String, serde_json::Value>,
}

impl ConnectorConfigEnvelope {
    pub fn empty(connector_id: ConnectorId, schema_version: u32) -> Self {
        Self {
            format_version: CONNECTOR_CONFIG_FORMAT_VERSION,
            connector_id,
            schema_version,
            values: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connector_id_accepts_builtin_and_plugin_namespaces() {
        for value in [
            "asterdrive.storage.local",
            "asterdrive.storage.s3",
            "com.example.archive_storage",
            "io.example.storage-v2",
        ] {
            ConnectorId::declared(value).validate().unwrap();
        }
    }

    #[test]
    fn connector_id_rejects_ambiguous_or_nonportable_values() {
        for value in [
            "S3",
            ".example.storage",
            "example.storage.",
            "example..storage",
            "example/storage",
            "存储.plugin",
        ] {
            assert!(ConnectorId::declared(value).validate().is_err(), "{value}");
        }
    }

    #[test]
    fn connector_config_envelope_round_trips_unknown_values() {
        let envelope = ConnectorConfigEnvelope {
            format_version: CONNECTOR_CONFIG_FORMAT_VERSION,
            connector_id: ConnectorId::declared("com.example.storage"),
            schema_version: 17,
            values: BTreeMap::from([(
                "opaque".to_string(),
                serde_json::json!({"nested": [1, true, "value"]}),
            )]),
        };

        let json = serde_json::to_string(&envelope).unwrap();
        let parsed: ConnectorConfigEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, envelope);
    }
}
