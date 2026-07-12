//! Runtime WebDAV configuration helpers.

use crate::config::RuntimeConfig;
use crate::config::definitions::DEFAULT_WEBDAV_MAX_ACTIVE_LOCKS_PER_USER;
use crate::errors::{AsterError, Result};

pub use crate::config::definitions::{
    WEBDAV_BLOCK_SYSTEM_FILE_PATTERNS_KEY, WEBDAV_BLOCK_SYSTEM_FILES_ENABLED_KEY,
    WEBDAV_ENABLED_KEY, WEBDAV_MAX_ACTIVE_LOCKS_PER_USER_KEY,
};

pub fn max_active_locks_per_user(runtime_config: &RuntimeConfig) -> u64 {
    match runtime_config.get(WEBDAV_MAX_ACTIVE_LOCKS_PER_USER_KEY) {
        Some(raw) => match raw.trim().parse::<u64>() {
            Ok(value) if value > 0 => value,
            _ => {
                tracing::warn!(
                    key = WEBDAV_MAX_ACTIVE_LOCKS_PER_USER_KEY,
                    value = %raw,
                    "invalid WebDAV active lock limit; using default"
                );
                DEFAULT_WEBDAV_MAX_ACTIVE_LOCKS_PER_USER
            }
        },
        None => DEFAULT_WEBDAV_MAX_ACTIVE_LOCKS_PER_USER,
    }
}

pub fn normalize_max_active_locks_per_user_config_value(value: &str) -> Result<String> {
    let parsed = value.trim().parse::<u64>().ok().filter(|value| *value > 0);
    parsed.map(|value| value.to_string()).ok_or_else(|| {
        AsterError::validation_error(format!(
            "{WEBDAV_MAX_ACTIVE_LOCKS_PER_USER_KEY} must be a positive integer"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::{max_active_locks_per_user, normalize_max_active_locks_per_user_config_value};
    use crate::config::RuntimeConfig;
    use crate::config::definitions::{
        CONFIG_CATEGORY_WEBDAV, DEFAULT_WEBDAV_MAX_ACTIVE_LOCKS_PER_USER,
        WEBDAV_MAX_ACTIVE_LOCKS_PER_USER_KEY,
    };
    use aster_forge_config::{ConfigSource, ConfigValueType};
    use aster_forge_db::system_config;
    use chrono::Utc;

    fn config_model(value: &str) -> system_config::Model {
        system_config::Model {
            id: 1,
            key: WEBDAV_MAX_ACTIVE_LOCKS_PER_USER_KEY.to_string(),
            value: value.to_string(),
            value_type: ConfigValueType::Number,
            requires_restart: false,
            is_sensitive: false,
            source: ConfigSource::System,
            visibility: aster_forge_config::ConfigVisibility::Private,
            namespace: String::new(),
            category: CONFIG_CATEGORY_WEBDAV.to_string(),
            description: "test".to_string(),
            updated_at: Utc::now(),
            updated_by: None,
        }
    }

    #[test]
    fn max_active_locks_per_user_reads_runtime_value() {
        let runtime_config = RuntimeConfig::new();
        assert_eq!(
            max_active_locks_per_user(&runtime_config),
            DEFAULT_WEBDAV_MAX_ACTIVE_LOCKS_PER_USER
        );

        runtime_config.apply(config_model("3"));
        assert_eq!(max_active_locks_per_user(&runtime_config), 3);

        runtime_config.apply(config_model("0"));
        assert_eq!(
            max_active_locks_per_user(&runtime_config),
            DEFAULT_WEBDAV_MAX_ACTIVE_LOCKS_PER_USER
        );

        runtime_config.apply(config_model("nope"));
        assert_eq!(
            max_active_locks_per_user(&runtime_config),
            DEFAULT_WEBDAV_MAX_ACTIVE_LOCKS_PER_USER
        );
    }

    #[test]
    fn normalize_max_active_locks_per_user_requires_positive_integer() {
        assert_eq!(
            normalize_max_active_locks_per_user_config_value(" 12 ").unwrap(),
            "12"
        );
        assert!(normalize_max_active_locks_per_user_config_value("0").is_err());
        assert!(normalize_max_active_locks_per_user_config_value("-1").is_err());
        assert!(normalize_max_active_locks_per_user_config_value("1.5").is_err());
    }
}
