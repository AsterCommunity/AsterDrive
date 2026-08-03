//! Versioned core-owned behavior for storage policies.
//!
//! Connector configuration and product behavior have different owners. A
//! connector controls provider/runtime configuration, while AsterDrive core
//! controls cross-connector behavior such as thumbnail and media metadata
//! processing. Keeping this envelope typed prevents connector plugins from
//! accidentally redefining product semantics.

use aster_drive_model::types::MediaProcessorKind;
use serde::{Deserialize, Serialize};
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

pub const STORAGE_POLICY_BEHAVIOR_FORMAT_VERSION: u32 = 1;
pub const STORAGE_POLICY_BEHAVIOR_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyBehaviorConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_processor: Option<MediaProcessorKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub thumbnail_extensions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media_metadata_extensions: Vec<String>,
}

impl StoragePolicyBehaviorConfig {
    pub fn uses_storage_native_thumbnail(&self) -> bool {
        self.thumbnail_processor == Some(MediaProcessorKind::StorageNative)
    }

    pub fn storage_native_thumbnail_matches_file_name(&self, file_name: &str) -> bool {
        self.uses_storage_native_thumbnail()
            && extension_matches(file_name, &self.thumbnail_extensions)
    }

    pub fn uses_storage_native_media_metadata(&self) -> bool {
        !self.media_metadata_extensions.is_empty()
    }

    pub fn storage_native_media_metadata_matches_file_name(&self, file_name: &str) -> bool {
        extension_matches(file_name, &self.media_metadata_extensions)
    }
}

fn extension_matches(file_name: &str, extensions: &[String]) -> bool {
    let Some((_, extension)) = file_name.rsplit_once('.') else {
        return false;
    };
    !extension.is_empty()
        && extensions
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(extension))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StoragePolicyBehaviorConfigEnvelope {
    pub format_version: u32,
    pub schema_version: u32,
    pub values: StoragePolicyBehaviorConfig,
}

impl StoragePolicyBehaviorConfigEnvelope {
    pub fn new(values: StoragePolicyBehaviorConfig) -> Self {
        Self {
            format_version: STORAGE_POLICY_BEHAVIOR_FORMAT_VERSION,
            schema_version: STORAGE_POLICY_BEHAVIOR_SCHEMA_VERSION,
            values,
        }
    }

    pub fn empty() -> Self {
        Self::new(StoragePolicyBehaviorConfig::default())
    }
}

#[derive(Debug)]
pub enum StoragePolicyBehaviorConfigCodecError {
    InvalidJson(serde_json::Error),
    FormatVersionMismatch { expected: u32, actual: u32 },
    SchemaVersionMismatch { expected: u32, actual: u32 },
}

impl fmt::Display for StoragePolicyBehaviorConfigCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(error) => write!(formatter, "invalid policy behavior JSON: {error}"),
            Self::FormatVersionMismatch { expected, actual } => write!(
                formatter,
                "policy behavior format version mismatch: expected {expected}, got {actual}"
            ),
            Self::SchemaVersionMismatch { expected, actual } => write!(
                formatter,
                "policy behavior schema version mismatch: expected {expected}, got {actual}"
            ),
        }
    }
}

impl std::error::Error for StoragePolicyBehaviorConfigCodecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidJson(error) => Some(error),
            _ => None,
        }
    }
}

pub fn encode_storage_policy_behavior_config(
    values: StoragePolicyBehaviorConfig,
) -> Result<String, StoragePolicyBehaviorConfigCodecError> {
    serde_json::to_string(&StoragePolicyBehaviorConfigEnvelope::new(values))
        .map_err(StoragePolicyBehaviorConfigCodecError::InvalidJson)
}

pub fn decode_storage_policy_behavior_config(
    raw: &str,
) -> Result<StoragePolicyBehaviorConfig, StoragePolicyBehaviorConfigCodecError> {
    let envelope: StoragePolicyBehaviorConfigEnvelope =
        serde_json::from_str(raw).map_err(StoragePolicyBehaviorConfigCodecError::InvalidJson)?;
    if envelope.format_version != STORAGE_POLICY_BEHAVIOR_FORMAT_VERSION {
        return Err(
            StoragePolicyBehaviorConfigCodecError::FormatVersionMismatch {
                expected: STORAGE_POLICY_BEHAVIOR_FORMAT_VERSION,
                actual: envelope.format_version,
            },
        );
    }
    if envelope.schema_version != STORAGE_POLICY_BEHAVIOR_SCHEMA_VERSION {
        return Err(
            StoragePolicyBehaviorConfigCodecError::SchemaVersionMismatch {
                expected: STORAGE_POLICY_BEHAVIOR_SCHEMA_VERSION,
                actual: envelope.schema_version,
            },
        );
    }
    Ok(envelope.values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn behavior_envelope_round_trips_versioned_core_fields() {
        let envelope = StoragePolicyBehaviorConfigEnvelope::new(StoragePolicyBehaviorConfig {
            thumbnail_processor: Some(MediaProcessorKind::StorageNative),
            thumbnail_extensions: vec!["jpg".to_string(), "webp".to_string()],
            media_metadata_extensions: vec!["mp4".to_string()],
        });

        let json = serde_json::to_string(&envelope).unwrap();
        let parsed: StoragePolicyBehaviorConfigEnvelope = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed, envelope);
        assert_eq!(
            parsed.format_version,
            STORAGE_POLICY_BEHAVIOR_FORMAT_VERSION
        );
        assert_eq!(
            parsed.schema_version,
            STORAGE_POLICY_BEHAVIOR_SCHEMA_VERSION
        );
    }

    #[test]
    fn empty_behavior_envelope_omits_empty_values() {
        let value = serde_json::to_value(StoragePolicyBehaviorConfigEnvelope::empty()).unwrap();

        assert_eq!(
            value["format_version"],
            STORAGE_POLICY_BEHAVIOR_FORMAT_VERSION
        );
        assert_eq!(
            value["schema_version"],
            STORAGE_POLICY_BEHAVIOR_SCHEMA_VERSION
        );
        assert_eq!(value["values"], serde_json::json!({}));
    }

    #[test]
    fn behavior_codec_rejects_unknown_fields_and_version_mismatches() {
        let unknown = r#"{"format_version":1,"schema_version":1,"values":{"unknown":true}}"#;
        assert!(matches!(
            decode_storage_policy_behavior_config(unknown),
            Err(StoragePolicyBehaviorConfigCodecError::InvalidJson(_))
        ));

        let wrong_version = r#"{"format_version":2,"schema_version":1,"values":{}}"#;
        assert!(matches!(
            decode_storage_policy_behavior_config(wrong_version),
            Err(StoragePolicyBehaviorConfigCodecError::FormatVersionMismatch { .. })
        ));
    }

    #[test]
    fn storage_native_extension_matching_is_case_insensitive_and_requires_a_suffix() {
        let behavior = StoragePolicyBehaviorConfig {
            thumbnail_processor: Some(MediaProcessorKind::StorageNative),
            thumbnail_extensions: vec!["jpg".to_string()],
            media_metadata_extensions: vec!["mp4".to_string()],
        };

        assert!(behavior.storage_native_thumbnail_matches_file_name("cover.JPG"));
        assert!(!behavior.storage_native_thumbnail_matches_file_name("coverjpg"));
        assert!(!behavior.storage_native_thumbnail_matches_file_name("cover."));
        assert!(behavior.storage_native_media_metadata_matches_file_name("clip.MP4"));
        assert!(!behavior.storage_native_media_metadata_matches_file_name("clip.mp3"));
    }
}
