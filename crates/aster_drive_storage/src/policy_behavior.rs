//! Versioned core-owned behavior for storage policies.
//!
//! Connector configuration and product behavior have different owners. A
//! connector controls provider/runtime configuration, while AsterDrive core
//! controls cross-connector behavior such as thumbnail and media metadata
//! processing. Keeping this envelope typed prevents connector plugins from
//! accidentally redefining product semantics.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

pub const STORAGE_POLICY_BEHAVIOR_FORMAT_VERSION: u32 = 1;
pub const STORAGE_POLICY_BEHAVIOR_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
/// Core-owned, per-policy storage-native processing preferences.
///
/// These switches only add provider-native candidates ahead of the global
/// media-processing registry. Disabling them does not disable thumbnail or
/// metadata processing for files on the policy; it makes those files continue
/// through the ordinary global processor chain. Extension vectors are retained
/// as dormant configuration while their corresponding switch is off.
pub struct StoragePolicyBehaviorConfig {
    /// Prefer the storage provider's native thumbnail implementation for
    /// matching files. `false` leaves the global thumbnail processors enabled.
    #[serde(default)]
    pub storage_native_thumbnail_enabled: bool,
    /// File extensions eligible for provider-native thumbnails. An empty list
    /// matches no files even when enabled. Retained as dormant configuration
    /// while `storage_native_thumbnail_enabled` is false.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub storage_native_thumbnail_extensions: Vec<String>,
    /// Prefer the storage provider's native media-metadata implementation for
    /// matching files. `false` leaves the global metadata processors enabled.
    #[serde(default)]
    pub storage_native_media_metadata_enabled: bool,
    /// File extensions eligible for provider-native media metadata. An empty
    /// list matches no files even when enabled. Retained as dormant
    /// configuration while the corresponding switch is false.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub storage_native_media_metadata_extensions: Vec<String>,
}

impl StoragePolicyBehaviorConfig {
    /// Canonicalize core-owned extension lists before they are persisted or
    /// exposed through capability aggregation.
    ///
    /// File extensions are case-insensitive identifiers, not display text.
    /// Storing one canonical representation prevents duplicate behavior and
    /// keeps API output deterministic across connectors and plugins.
    pub fn normalized(mut self) -> Self {
        self.storage_native_thumbnail_extensions =
            normalize_extensions(self.storage_native_thumbnail_extensions);
        self.storage_native_media_metadata_extensions =
            normalize_extensions(self.storage_native_media_metadata_extensions);
        self
    }

    pub fn uses_storage_native_thumbnail(&self) -> bool {
        self.storage_native_thumbnail_enabled
    }

    pub fn storage_native_thumbnail_matches_file_name(&self, file_name: &str) -> bool {
        self.uses_storage_native_thumbnail()
            && extension_matches(file_name, &self.storage_native_thumbnail_extensions)
    }

    pub fn uses_storage_native_media_metadata(&self) -> bool {
        self.storage_native_media_metadata_enabled
    }

    pub fn storage_native_media_metadata_matches_file_name(&self, file_name: &str) -> bool {
        self.uses_storage_native_media_metadata()
            && extension_matches(file_name, &self.storage_native_media_metadata_extensions)
    }
}

fn normalize_extensions(extensions: Vec<String>) -> Vec<String> {
    extensions
        .into_iter()
        .filter_map(|extension| {
            let extension = extension
                .trim()
                .trim_start_matches('.')
                .to_ascii_lowercase();
            (!extension.is_empty()).then_some(extension)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
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
            values: values.normalized(),
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
            storage_native_thumbnail_enabled: true,
            storage_native_thumbnail_extensions: vec!["jpg".to_string(), "webp".to_string()],
            storage_native_media_metadata_enabled: true,
            storage_native_media_metadata_extensions: vec!["mp4".to_string()],
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
    fn empty_behavior_envelope_serializes_explicit_disabled_switches() {
        let value = serde_json::to_value(StoragePolicyBehaviorConfigEnvelope::empty()).unwrap();

        assert_eq!(
            value["format_version"],
            STORAGE_POLICY_BEHAVIOR_FORMAT_VERSION
        );
        assert_eq!(
            value["schema_version"],
            STORAGE_POLICY_BEHAVIOR_SCHEMA_VERSION
        );
        assert_eq!(
            value["values"],
            serde_json::json!({
                "storage_native_thumbnail_enabled": false,
                "storage_native_media_metadata_enabled": false
            })
        );
    }

    #[test]
    fn behavior_codec_rejects_unknown_fields_and_version_mismatches() {
        let unknown = r#"{"format_version":1,"schema_version":2,"values":{"unknown":true}}"#;
        assert!(matches!(
            decode_storage_policy_behavior_config(unknown),
            Err(StoragePolicyBehaviorConfigCodecError::InvalidJson(_))
        ));

        let wrong_version = r#"{"format_version":2,"schema_version":2,"values":{}}"#;
        assert!(matches!(
            decode_storage_policy_behavior_config(wrong_version),
            Err(StoragePolicyBehaviorConfigCodecError::FormatVersionMismatch { .. })
        ));
    }

    #[test]
    fn storage_native_extension_matching_is_case_insensitive_and_requires_a_suffix() {
        let behavior = StoragePolicyBehaviorConfig {
            storage_native_thumbnail_enabled: true,
            storage_native_thumbnail_extensions: vec!["jpg".to_string()],
            storage_native_media_metadata_enabled: true,
            storage_native_media_metadata_extensions: vec!["mp4".to_string()],
        };

        assert!(behavior.storage_native_thumbnail_matches_file_name("cover.JPG"));
        assert!(!behavior.storage_native_thumbnail_matches_file_name("coverjpg"));
        assert!(!behavior.storage_native_thumbnail_matches_file_name("cover."));
        assert!(behavior.storage_native_media_metadata_matches_file_name("clip.MP4"));
        assert!(!behavior.storage_native_media_metadata_matches_file_name("clip.mp3"));

        let disabled_metadata = StoragePolicyBehaviorConfig {
            storage_native_media_metadata_enabled: false,
            storage_native_media_metadata_extensions: vec!["mp4".to_string()],
            ..Default::default()
        };
        assert!(!disabled_metadata.storage_native_media_metadata_matches_file_name("clip.mp4"));
    }

    #[test]
    fn enabled_storage_native_behaviors_with_empty_extensions_match_no_files() {
        let behavior = StoragePolicyBehaviorConfig {
            storage_native_thumbnail_enabled: true,
            storage_native_media_metadata_enabled: true,
            ..Default::default()
        };

        assert!(behavior.uses_storage_native_thumbnail());
        assert!(behavior.uses_storage_native_media_metadata());
        assert!(!behavior.storage_native_thumbnail_matches_file_name("cover.jpg"));
        assert!(!behavior.storage_native_media_metadata_matches_file_name("clip.mp4"));
    }

    #[test]
    fn behavior_normalization_trims_dots_case_blanks_and_duplicates() {
        let behavior = StoragePolicyBehaviorConfig {
            storage_native_thumbnail_enabled: true,
            storage_native_thumbnail_extensions: vec![
                " .JPG ".to_string(),
                "jpg".to_string(),
                "...WEBP".to_string(),
                " . ".to_string(),
                String::new(),
            ],
            storage_native_media_metadata_enabled: true,
            storage_native_media_metadata_extensions: vec![
                "MP4".to_string(),
                ".m4a".to_string(),
                " mp4 ".to_string(),
            ],
        }
        .normalized();

        assert_eq!(
            behavior.storage_native_thumbnail_extensions,
            ["jpg", "webp"]
        );
        assert_eq!(
            behavior.storage_native_media_metadata_extensions,
            ["m4a", "mp4"]
        );
        assert!(behavior.storage_native_thumbnail_enabled);
    }

    #[test]
    fn behavior_envelope_persists_only_normalized_extensions() {
        let envelope = StoragePolicyBehaviorConfigEnvelope::new(StoragePolicyBehaviorConfig {
            storage_native_thumbnail_enabled: false,
            storage_native_thumbnail_extensions: vec![" .PNG ".to_string(), "png".to_string()],
            storage_native_media_metadata_enabled: false,
            storage_native_media_metadata_extensions: vec![" .MP4 ".to_string()],
        });

        assert_eq!(envelope.values.storage_native_thumbnail_extensions, ["png"]);
        assert_eq!(
            envelope.values.storage_native_media_metadata_extensions,
            ["mp4"]
        );
        assert!(!envelope.values.uses_storage_native_thumbnail());
        assert!(!envelope.values.uses_storage_native_media_metadata());
    }
}
