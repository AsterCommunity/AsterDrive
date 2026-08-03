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

/// Declare every connector field once while keeping persistence channels
/// structurally separate.
///
/// Only `config` fields become members of the serde config struct. Static and
/// application credentials contribute descriptor fields but can never be
/// serialized into the connector config envelope.
#[macro_export]
macro_rules! storage_connector_schema {
    (
        $(#[$struct_meta:meta])*
        $visibility:vis struct $name:ident {
            config {
                $(
                    $(#[$field_meta:meta])*
                    $field_visibility:vis $field:ident: $field_type:ty => $descriptor:expr
                ),* $(,)?
            }
            credentials none
        }
    ) => {
        $crate::storage_connector_schema_impl! {
            $(#[$struct_meta])*
            $visibility struct $name {
                config {
                    $(
                        $(#[$field_meta])*
                        $field_visibility $field: $field_type => $descriptor
                    ),*
                }
                credential_mode = $crate::StorageConnectorCredentialMode::None;
                credentials {}
            }
        }
    };
    (
        $(#[$struct_meta:meta])*
        $visibility:vis struct $name:ident {
            config {
                $(
                    $(#[$field_meta:meta])*
                    $field_visibility:vis $field:ident: $field_type:ty => $descriptor:expr
                ),* $(,)?
            }
            credentials static {
                $(
                    $credential_field:ident => $credential_descriptor:expr
                ),* $(,)?
            }
        }
    ) => {
        $crate::storage_connector_schema_impl! {
            $(#[$struct_meta])*
            $visibility struct $name {
                config {
                    $(
                        $(#[$field_meta])*
                        $field_visibility $field: $field_type => $descriptor
                    ),*
                }
                credential_mode = $crate::StorageConnectorCredentialMode::StaticSecret;
                credentials {
                    $(
                        $credential_field => $credential_descriptor
                    ),*
                }
            }
        }
    };
    (
        $(#[$struct_meta:meta])*
        $visibility:vis struct $name:ident {
            config {
                $(
                    $(#[$field_meta:meta])*
                    $field_visibility:vis $field:ident: $field_type:ty => $descriptor:expr
                ),* $(,)?
            }
            credentials authorization_application {
                $(
                    $credential_field:ident => $credential_descriptor:expr
                ),* $(,)?
            }
        }
    ) => {
        $crate::storage_connector_schema_impl! {
            $(#[$struct_meta])*
            $visibility struct $name {
                config {
                    $(
                        $(#[$field_meta])*
                        $field_visibility $field: $field_type => $descriptor
                    ),*
                }
                credential_mode = $crate::StorageConnectorCredentialMode::OauthDelegated;
                credentials {
                    $(
                        $credential_field => $credential_descriptor
                    ),*
                }
            }
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! storage_connector_schema_impl {
    (
        $(#[$struct_meta:meta])*
        $visibility:vis struct $name:ident {
            config {
                $(
                    $(#[$field_meta:meta])*
                    $field_visibility:vis $field:ident: $field_type:ty => $descriptor:expr
                ),* $(,)?
            }
            credential_mode = $credential_mode:expr;
            credentials {
                $(
                    $credential_field:ident => $credential_descriptor:expr
                ),* $(,)?
            }
        }
    ) => {
        $(#[$struct_meta])*
        #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        $visibility struct $name {
            $(
                $(#[$field_meta])*
                $field_visibility $field: $field_type,
            )*
        }

        impl $crate::StorageConnectorConfigSchema for $name {
            fn connector_config_fields() -> Vec<$crate::StorageConnectorFieldDescriptor> {
                vec![
                    $(
                        {
                            let descriptor: $crate::StorageConnectorFieldDescriptor = $descriptor;
                            assert_eq!(
                                descriptor.name,
                                stringify!($field),
                                "connector config descriptor field name must match its serde field"
                            );
                            descriptor
                        }
                    ),*
                ]
            }

            fn credential_mode() -> $crate::StorageConnectorCredentialMode {
                $credential_mode
            }

            fn credential_fields() -> Vec<$crate::StorageConnectorFieldDescriptor> {
                vec![
                    $(
                        {
                            let descriptor: $crate::StorageConnectorFieldDescriptor = $credential_descriptor;
                            assert_eq!(
                                descriptor.name,
                                stringify!($credential_field),
                                "credential descriptor name must match its declared field"
                            );
                            descriptor
                        }
                    ),*
                ]
            }
        }
    };
}

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
pub struct ConnectorConfigEnvelope<T = BTreeMap<String, serde_json::Value>> {
    pub format_version: u32,
    pub connector_id: ConnectorId,
    pub schema_version: u32,
    pub values: T,
}

impl<T> ConnectorConfigEnvelope<T> {
    pub fn new(connector_id: ConnectorId, schema_version: u32, values: T) -> Self {
        Self {
            format_version: CONNECTOR_CONFIG_FORMAT_VERSION,
            connector_id,
            schema_version,
            values,
        }
    }
}

#[derive(Debug)]
pub enum ConnectorConfigCodecError {
    InvalidJson(serde_json::Error),
    FormatVersionMismatch { expected: u32, actual: u32 },
    ConnectorIdMismatch { expected: String, actual: String },
    SchemaVersionMismatch { expected: u32, actual: u32 },
}

impl fmt::Display for ConnectorConfigCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(error) => write!(formatter, "invalid connector config JSON: {error}"),
            Self::FormatVersionMismatch { expected, actual } => write!(
                formatter,
                "connector config format version mismatch: expected {expected}, got {actual}"
            ),
            Self::ConnectorIdMismatch { expected, actual } => write!(
                formatter,
                "connector config id mismatch: expected '{expected}', got '{actual}'"
            ),
            Self::SchemaVersionMismatch { expected, actual } => write!(
                formatter,
                "connector config schema version mismatch: expected {expected}, got {actual}"
            ),
        }
    }
}

impl std::error::Error for ConnectorConfigCodecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidJson(error) => Some(error),
            _ => None,
        }
    }
}

pub fn encode_connector_config<T: Serialize>(
    connector_id: ConnectorId,
    schema_version: u32,
    values: T,
) -> Result<String, ConnectorConfigCodecError> {
    serde_json::to_string(&ConnectorConfigEnvelope::new(
        connector_id,
        schema_version,
        values,
    ))
    .map_err(ConnectorConfigCodecError::InvalidJson)
}

pub fn decode_connector_config<'a, T: Deserialize<'a>>(
    raw: &'a str,
    expected_connector_id: &ConnectorId,
    expected_schema_version: u32,
) -> Result<T, ConnectorConfigCodecError> {
    let envelope: ConnectorConfigEnvelope<T> =
        serde_json::from_str(raw).map_err(ConnectorConfigCodecError::InvalidJson)?;
    if envelope.format_version != CONNECTOR_CONFIG_FORMAT_VERSION {
        return Err(ConnectorConfigCodecError::FormatVersionMismatch {
            expected: CONNECTOR_CONFIG_FORMAT_VERSION,
            actual: envelope.format_version,
        });
    }
    if &envelope.connector_id != expected_connector_id {
        return Err(ConnectorConfigCodecError::ConnectorIdMismatch {
            expected: expected_connector_id.as_str().to_string(),
            actual: envelope.connector_id.as_str().to_string(),
        });
    }
    if envelope.schema_version != expected_schema_version {
        return Err(ConnectorConfigCodecError::SchemaVersionMismatch {
            expected: expected_schema_version,
            actual: envelope.schema_version,
        });
    }
    Ok(envelope.values)
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
        let envelope = ConnectorConfigEnvelope::new(
            ConnectorId::declared("com.example.storage"),
            17,
            BTreeMap::from([(
                "opaque".to_string(),
                serde_json::json!({"nested": [1, true, "value"]}),
            )]),
        );

        let json = serde_json::to_string(&envelope).unwrap();
        let parsed: ConnectorConfigEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, envelope);
    }

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct TypedConfig {
        base_path: String,
    }

    #[test]
    fn typed_codec_round_trips_and_rejects_contract_mismatches() {
        let connector_id = ConnectorId::declared("com.example.storage");
        let raw = encode_connector_config(
            connector_id.clone(),
            4,
            TypedConfig {
                base_path: "archive".to_string(),
            },
        )
        .unwrap();
        assert_eq!(
            decode_connector_config::<TypedConfig>(&raw, &connector_id, 4).unwrap(),
            TypedConfig {
                base_path: "archive".to_string()
            }
        );
        assert!(matches!(
            decode_connector_config::<TypedConfig>(
                &raw,
                &ConnectorId::declared("com.example.other"),
                4
            ),
            Err(ConnectorConfigCodecError::ConnectorIdMismatch { .. })
        ));
        assert!(matches!(
            decode_connector_config::<TypedConfig>(&raw, &connector_id, 5),
            Err(ConnectorConfigCodecError::SchemaVersionMismatch { .. })
        ));
    }
}
