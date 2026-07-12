//! 配置子模块：`wopi`。

use crate::config::RuntimeConfig;
use crate::errors::{AsterError, Result};

pub use crate::config::definitions::{
    WOPI_ACCESS_TOKEN_TTL_SECS_KEY, WOPI_DISCOVERY_CACHE_TTL_SECS_KEY, WOPI_LOCK_TTL_SECS_KEY,
};

pub const DEFAULT_WOPI_ACCESS_TOKEN_TTL_SECS: u64 = 15 * 60;
pub const DEFAULT_WOPI_LOCK_TTL_SECS: u64 = 30 * 60;
pub const DEFAULT_WOPI_DISCOVERY_CACHE_TTL_SECS: u64 = 5 * 60;

pub fn normalize_ttl_config_value(key: &str, value: &str) -> Result<String> {
    let parsed = parse_positive_u64(value)
        .ok_or_else(|| AsterError::validation_error(format!("{key} must be a positive integer")))?;
    Ok(parsed.to_string())
}

pub fn access_token_ttl_secs(runtime_config: &RuntimeConfig) -> i64 {
    read_positive_i64(
        runtime_config,
        WOPI_ACCESS_TOKEN_TTL_SECS_KEY,
        DEFAULT_WOPI_ACCESS_TOKEN_TTL_SECS,
    )
}

pub fn lock_ttl_secs(runtime_config: &RuntimeConfig) -> i64 {
    read_positive_i64(
        runtime_config,
        WOPI_LOCK_TTL_SECS_KEY,
        DEFAULT_WOPI_LOCK_TTL_SECS,
    )
}

pub fn discovery_cache_ttl_secs(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        WOPI_DISCOVERY_CACHE_TTL_SECS_KEY,
        DEFAULT_WOPI_DISCOVERY_CACHE_TTL_SECS,
    )
}

fn parse_positive_u64(value: &str) -> Option<u64> {
    let parsed = value.trim().parse::<u64>().ok()?;
    (parsed > 0).then_some(parsed)
}

fn read_positive_u64(runtime_config: &RuntimeConfig, key: &str, default: u64) -> u64 {
    match runtime_config.get(key) {
        Some(raw) => match parse_positive_u64(&raw) {
            Some(value) => value,
            None => {
                tracing::warn!(key, value = %raw, "invalid runtime WOPI config; using default");
                default
            }
        },
        None => default,
    }
}

fn read_positive_i64(runtime_config: &RuntimeConfig, key: &str, default: u64) -> i64 {
    let value = read_positive_u64(runtime_config, key, default);
    i64::try_from(value).unwrap_or_else(|_| {
        tracing::warn!(key, value, "runtime WOPI config exceeds i64; using default");
        i64::try_from(default).unwrap_or(i64::MAX)
    })
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_WOPI_ACCESS_TOKEN_TTL_SECS, DEFAULT_WOPI_DISCOVERY_CACHE_TTL_SECS,
        DEFAULT_WOPI_LOCK_TTL_SECS, WOPI_ACCESS_TOKEN_TTL_SECS_KEY,
        WOPI_DISCOVERY_CACHE_TTL_SECS_KEY, WOPI_LOCK_TTL_SECS_KEY, access_token_ttl_secs,
        discovery_cache_ttl_secs, lock_ttl_secs, normalize_ttl_config_value,
    };
    use crate::config::RuntimeConfig;
    use aster_forge_db::system_config;
    use chrono::Utc;

    fn config_model(key: &str, value: &str) -> system_config::Model {
        system_config::Model {
            id: 1,
            key: key.to_string(),
            value: value.to_string(),
            value_type: aster_forge_config::ConfigValueType::Number,
            requires_restart: false,
            is_sensitive: false,
            source: aster_forge_config::ConfigSource::System,
            visibility: aster_forge_config::ConfigVisibility::Private,
            namespace: String::new(),
            category: crate::config::definitions::CONFIG_CATEGORY_SITE_PREVIEW.to_string(),
            description: "test".to_string(),
            updated_at: Utc::now(),
            updated_by: None,
        }
    }

    #[test]
    fn ttl_readers_use_default_for_missing_and_invalid_values() {
        let runtime_config = RuntimeConfig::new();
        assert_eq!(
            access_token_ttl_secs(&runtime_config),
            crate::utils::numbers::u64_to_i64(
                DEFAULT_WOPI_ACCESS_TOKEN_TTL_SECS,
                WOPI_ACCESS_TOKEN_TTL_SECS_KEY,
            )
            .unwrap()
        );
        assert_eq!(
            lock_ttl_secs(&runtime_config),
            crate::utils::numbers::u64_to_i64(DEFAULT_WOPI_LOCK_TTL_SECS, WOPI_LOCK_TTL_SECS_KEY,)
                .unwrap()
        );
        assert_eq!(
            discovery_cache_ttl_secs(&runtime_config),
            DEFAULT_WOPI_DISCOVERY_CACHE_TTL_SECS
        );

        runtime_config.apply(config_model(WOPI_ACCESS_TOKEN_TTL_SECS_KEY, "0"));
        runtime_config.apply(config_model(WOPI_LOCK_TTL_SECS_KEY, "-1"));
        runtime_config.apply(config_model(WOPI_DISCOVERY_CACHE_TTL_SECS_KEY, "abc"));

        assert_eq!(
            access_token_ttl_secs(&runtime_config),
            crate::utils::numbers::u64_to_i64(
                DEFAULT_WOPI_ACCESS_TOKEN_TTL_SECS,
                WOPI_ACCESS_TOKEN_TTL_SECS_KEY,
            )
            .unwrap()
        );
        assert_eq!(
            lock_ttl_secs(&runtime_config),
            crate::utils::numbers::u64_to_i64(DEFAULT_WOPI_LOCK_TTL_SECS, WOPI_LOCK_TTL_SECS_KEY,)
                .unwrap()
        );
        assert_eq!(
            discovery_cache_ttl_secs(&runtime_config),
            DEFAULT_WOPI_DISCOVERY_CACHE_TTL_SECS
        );
    }

    #[test]
    fn ttl_readers_use_positive_values() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(WOPI_ACCESS_TOKEN_TTL_SECS_KEY, "7200"));
        runtime_config.apply(config_model(WOPI_LOCK_TTL_SECS_KEY, "120"));
        runtime_config.apply(config_model(WOPI_DISCOVERY_CACHE_TTL_SECS_KEY, "30"));

        assert_eq!(access_token_ttl_secs(&runtime_config), 7200);
        assert_eq!(lock_ttl_secs(&runtime_config), 120);
        assert_eq!(discovery_cache_ttl_secs(&runtime_config), 30);
    }

    #[test]
    fn normalize_ttl_requires_positive_integer() {
        assert_eq!(
            normalize_ttl_config_value(WOPI_ACCESS_TOKEN_TTL_SECS_KEY, "3600").unwrap(),
            "3600"
        );
        assert!(normalize_ttl_config_value(WOPI_ACCESS_TOKEN_TTL_SECS_KEY, "0").is_err());
        assert!(normalize_ttl_config_value(WOPI_ACCESS_TOKEN_TTL_SECS_KEY, "abc").is_err());
    }
}
