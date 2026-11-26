//! Offline download runtime engine registry.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::config::operations::OfflineDownloadEngine;
use crate::errors::{AsterError, MapAsterErr, Result};

/// Schema version of the stored engine registry JSON; entries with a different
/// version are rejected.
pub const OFFLINE_DOWNLOAD_ENGINE_REGISTRY_VERSION: i32 = 1;

/// Stored registry of offline download engines (system_config JSON value).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfflineDownloadEngineRegistryConfig {
    /// Registry schema version; must equal
    /// [`OFFLINE_DOWNLOAD_ENGINE_REGISTRY_VERSION`].
    #[serde(default = "default_offline_download_engine_registry_version")]
    pub version: i32,
    /// One entry per known engine; duplicates of the same kind are rejected.
    #[serde(default)]
    pub engines: Vec<OfflineDownloadEngineConfig>,
}

/// A single engine entry: its kind plus the enabled switch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfflineDownloadEngineConfig {
    /// Which engine this entry configures.
    pub kind: OfflineDownloadEngine,
    /// Whether downloads may use this engine; defaults to true.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Registry containing every known engine with its built-in default enabled
/// state.
pub fn default_offline_download_engine_registry() -> OfflineDownloadEngineRegistryConfig {
    OfflineDownloadEngineRegistryConfig {
        version: OFFLINE_DOWNLOAD_ENGINE_REGISTRY_VERSION,
        engines: OfflineDownloadEngine::ALL
            .into_iter()
            .map(|kind| OfflineDownloadEngineConfig {
                kind,
                enabled: kind.default_enabled(),
            })
            .collect(),
    }
}

/// Pretty-printed JSON of [`default_offline_download_engine_registry`], used
/// as the stored default; degrades to compact JSON or `{}` if serialization
/// fails.
pub fn default_offline_download_engine_registry_json() -> String {
    serde_json::to_string_pretty(&default_offline_download_engine_registry()).unwrap_or_else(
        |error| {
            tracing::warn!(%error, "failed to serialize default offline download engine registry");
            serde_json::to_string(&OfflineDownloadEngineRegistryConfig {
                version: OFFLINE_DOWNLOAD_ENGINE_REGISTRY_VERSION,
                engines: Vec::new(),
            })
            .unwrap_or_else(|fallback_error| {
                tracing::error!(%fallback_error, "failed to serialize fallback offline download engine registry");
                "{}".to_string()
            })
        },
    )
}

/// Validates a stored registry value and returns its canonical pretty-printed
/// JSON form.
pub fn normalize_offline_download_engine_registry_config_value(value: &str) -> Result<String> {
    let registry = parse_offline_download_engine_registry_config_value(value)?;
    serde_json::to_string_pretty(&registry).map_aster_err_ctx(
        "serialize offline download engine registry",
        AsterError::internal_error,
    )
}

/// Parses and validates a stored registry value: enforces the schema version
/// and rejects duplicate engine kinds.
pub fn parse_offline_download_engine_registry_config_value(
    value: &str,
) -> Result<OfflineDownloadEngineRegistryConfig> {
    let registry: OfflineDownloadEngineRegistryConfig = serde_json::from_str(value)
        .map_aster_err_ctx(
            "parse offline_download_engine_registry_json",
            AsterError::validation_error,
        )?;
    validate_registry(&registry)?;
    Ok(registry)
}

/// Engine kinds enabled for new offline downloads under the stored registry.
pub fn enabled_offline_download_engines_from_registry_value(
    value: &str,
) -> Result<Vec<OfflineDownloadEngine>> {
    Ok(parse_offline_download_engine_registry_config_value(value)?
        .engines
        .into_iter()
        .filter(|engine| engine.enabled)
        .map(|engine| engine.kind)
        .collect())
}

fn validate_registry(registry: &OfflineDownloadEngineRegistryConfig) -> Result<()> {
    if registry.version != OFFLINE_DOWNLOAD_ENGINE_REGISTRY_VERSION {
        return Err(AsterError::validation_error(format!(
            "offline_download_engine_registry_json version must be {OFFLINE_DOWNLOAD_ENGINE_REGISTRY_VERSION}"
        )));
    }

    let mut seen = HashSet::new();
    for engine in &registry.engines {
        if !seen.insert(engine.kind) {
            return Err(AsterError::validation_error(format!(
                "offline_download_engine_registry_json contains duplicate {} engine",
                engine.kind.as_str()
            )));
        }
    }

    Ok(())
}

const fn default_offline_download_engine_registry_version() -> i32 {
    OFFLINE_DOWNLOAD_ENGINE_REGISTRY_VERSION
}

const fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_normalization_preserves_disabled_all_engines() {
        let normalized = normalize_offline_download_engine_registry_config_value(
            r#"{"version":1,"engines":[{"kind":"builtin","enabled":false},{"kind":"aria2","enabled":false}]}"#,
        )
        .unwrap();

        assert!(
            enabled_offline_download_engines_from_registry_value(&normalized)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn registry_rejects_duplicate_engines() {
        assert!(
            normalize_offline_download_engine_registry_config_value(
                r#"{"version":1,"engines":[{"kind":"builtin"},{"kind":"builtin"}]}"#
            )
            .is_err()
        );
    }
}
