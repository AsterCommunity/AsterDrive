//! 配置子模块：`avatar`。

use std::path::{Path, PathBuf};

use crate::config::RuntimeConfig;
use crate::errors::{AsterError, MapAsterErr, Result};

pub use crate::config::definitions::AVATAR_DIR_KEY;
pub const DEFAULT_AVATAR_DIR: &str = "avatar";
const DEFAULT_DATA_DIR: &str = "data";

const MAX_AVATAR_DIR_LEN: usize = 4096;

pub fn normalize_avatar_dir_config_value(value: &str) -> Result<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Ok(DEFAULT_AVATAR_DIR.to_string());
    }
    if normalized.len() > MAX_AVATAR_DIR_LEN {
        return Err(AsterError::validation_error(format!(
            "avatar_dir exceeds {MAX_AVATAR_DIR_LEN} characters",
        )));
    }
    if normalized.chars().any(char::is_control) {
        return Err(AsterError::validation_error(
            "avatar_dir cannot contain control characters",
        ));
    }
    Ok(normalized.to_string())
}

pub fn avatar_dir_or_default(runtime_config: &RuntimeConfig) -> String {
    runtime_config
        .get(AVATAR_DIR_KEY)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_AVATAR_DIR.to_string())
}

pub fn resolve_local_avatar_root_dir(runtime_config: &RuntimeConfig) -> Result<PathBuf> {
    let configured = avatar_dir_or_default(runtime_config);
    let configured_path = Path::new(&configured);
    if configured_path.is_absolute() {
        return Ok(configured_path.to_path_buf());
    }

    std::env::current_dir()
        .map(|cwd| cwd.join(DEFAULT_DATA_DIR).join(configured_path))
        .map_aster_err(|e| {
            AsterError::storage_driver_error(format!("resolve avatar_dir '{configured}': {e}"))
        })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        AVATAR_DIR_KEY, DEFAULT_AVATAR_DIR, avatar_dir_or_default,
        normalize_avatar_dir_config_value, resolve_local_avatar_root_dir,
    };
    use crate::config::RuntimeConfig;
    use crate::config::definitions::CONFIG_CATEGORY_USER_AVATAR;
    use aster_forge_db::system_config;
    use chrono::Utc;

    fn config_model(key: &str, value: &str) -> system_config::Model {
        system_config::Model {
            id: 1,
            key: key.to_string(),
            value: value.to_string(),
            value_type: aster_forge_config::ConfigValueType::String,
            requires_restart: false,
            is_sensitive: false,
            source: aster_forge_config::ConfigSource::System,
            visibility: aster_forge_config::ConfigVisibility::Private,
            namespace: String::new(),
            category: CONFIG_CATEGORY_USER_AVATAR.to_string(),
            description: "test".to_string(),
            updated_at: Utc::now(),
            updated_by: None,
        }
    }

    #[test]
    fn avatar_dir_normalization_trims_and_falls_back_to_default() {
        assert_eq!(
            normalize_avatar_dir_config_value("  ").unwrap(),
            DEFAULT_AVATAR_DIR
        );
        assert_eq!(
            normalize_avatar_dir_config_value("  /srv/avatars  ").unwrap(),
            "/srv/avatars"
        );
        assert!(normalize_avatar_dir_config_value("avatar\nnext").is_err());
    }

    #[test]
    fn avatar_dir_defaults_when_runtime_value_missing_or_blank() {
        let runtime_config = RuntimeConfig::new();
        assert_eq!(avatar_dir_or_default(&runtime_config), DEFAULT_AVATAR_DIR);

        runtime_config.apply(config_model(AVATAR_DIR_KEY, "   "));
        assert_eq!(avatar_dir_or_default(&runtime_config), DEFAULT_AVATAR_DIR);
    }

    #[test]
    fn resolve_local_avatar_root_dir_keeps_absolute_and_expands_relative_paths_under_data_dir() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(AVATAR_DIR_KEY, "/tmp/asterdrive-avatar-test"));
        assert_eq!(
            resolve_local_avatar_root_dir(&runtime_config).unwrap(),
            PathBuf::from("/tmp/asterdrive-avatar-test")
        );

        runtime_config.apply(config_model(AVATAR_DIR_KEY, "avatar"));
        let resolved = resolve_local_avatar_root_dir(&runtime_config).unwrap();
        assert!(resolved.is_absolute());
        assert!(resolved.ends_with("data/avatar"));
    }
}
