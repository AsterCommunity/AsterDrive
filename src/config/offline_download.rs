//! Offline download runtime engine registry.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::config::operations::OfflineDownloadEngine;
use crate::errors::{AsterError, MapAsterErr, Result};

pub const OFFLINE_DOWNLOAD_ENGINE_REGISTRY_VERSION: i32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfflineDownloadEngineRegistryConfig {
    #[serde(default = "default_offline_download_engine_registry_version")]
    pub version: i32,
    #[serde(default)]
    pub engines: Vec<OfflineDownloadEngineConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfflineDownloadEngineConfig {
    pub kind: OfflineDownloadEngine,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

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

pub fn default_offline_download_engine_registry_json() -> String {
    serde_json::to_string_pretty(&default_offline_download_engine_registry())
        .expect("serialize default offline download engine registry")
}

pub fn normalize_offline_download_engine_registry_config_value(value: &str) -> Result<String> {
    let registry = parse_offline_download_engine_registry_config_value(value)?;
    serde_json::to_string_pretty(&registry).map_aster_err_ctx(
        "serialize offline download engine registry",
        AsterError::internal_error,
    )
}

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
