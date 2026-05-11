//! 配置子模块：`operations`。

use crate::config::RuntimeConfig;
use crate::errors::{AsterError, Result};
use crate::utils::numbers::{u64_to_i64, u64_to_usize, usize_to_u64};

pub use crate::config::definitions::{
    ARCHIVE_BUILD_MAX_ENTRIES_KEY, ARCHIVE_BUILD_MAX_TEMP_BYTES_KEY,
    ARCHIVE_BUILD_MAX_TOTAL_SOURCE_BYTES_KEY, ARCHIVE_EXTRACT_MAX_COMPRESSION_RATIO_KEY,
    ARCHIVE_EXTRACT_MAX_DEPTH_KEY, ARCHIVE_EXTRACT_MAX_DIRECTORIES_KEY,
    ARCHIVE_EXTRACT_MAX_DURATION_SECS_KEY, ARCHIVE_EXTRACT_MAX_ENTRIES_KEY,
    ARCHIVE_EXTRACT_MAX_ENTRY_COMPRESSION_RATIO_KEY, ARCHIVE_EXTRACT_MAX_FILES_KEY,
    ARCHIVE_EXTRACT_MAX_PATH_BYTES_KEY, ARCHIVE_EXTRACT_MAX_SOURCE_BYTES_KEY,
    ARCHIVE_EXTRACT_MAX_STAGING_BYTES_KEY, ARCHIVE_EXTRACT_MAX_UNCOMPRESSED_BYTES_KEY,
    AVATAR_MAX_UPLOAD_SIZE_BYTES_KEY, BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY_KEY,
    BACKGROUND_TASK_DISPATCH_INTERVAL_SECS_KEY, BACKGROUND_TASK_MAX_ATTEMPTS_KEY,
    BACKGROUND_TASK_MAX_CONCURRENCY_KEY, BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY_KEY,
    BLOB_RECONCILE_INTERVAL_SECS_KEY, MAIL_OUTBOX_DISPATCH_INTERVAL_SECS_KEY,
    MAINTENANCE_CLEANUP_INTERVAL_SECS_KEY, REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS_KEY,
    SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY_KEY, TASK_LIST_MAX_LIMIT_KEY,
    TEAM_MEMBER_LIST_MAX_LIMIT_KEY, THUMBNAIL_MAX_SOURCE_BYTES_KEY,
};

pub const DEFAULT_MAIL_OUTBOX_DISPATCH_INTERVAL_SECS: u64 = 5;
pub const DEFAULT_BACKGROUND_TASK_DISPATCH_INTERVAL_SECS: u64 = 5;
pub const DEFAULT_BACKGROUND_TASK_MAX_CONCURRENCY: usize = 1;
pub const DEFAULT_BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY: usize = 2;
pub const DEFAULT_BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY: usize = 3;
pub const DEFAULT_BACKGROUND_TASK_MAX_ATTEMPTS: i32 = 3;
pub const DEFAULT_SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY: usize = 1024;
pub const DEFAULT_MAINTENANCE_CLEANUP_INTERVAL_SECS: u64 = 3600;
pub const DEFAULT_BLOB_RECONCILE_INTERVAL_SECS: u64 = 6 * 3600;
pub const DEFAULT_REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS: u64 = 300;
pub const DEFAULT_TEAM_MEMBER_LIST_MAX_LIMIT: u64 = 100;
pub const DEFAULT_TASK_LIST_MAX_LIMIT: u64 = 100;
pub const DEFAULT_AVATAR_MAX_UPLOAD_SIZE_BYTES: u64 = 10 * 1024 * 1024;
pub const DEFAULT_THUMBNAIL_MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
pub const DEFAULT_ARCHIVE_EXTRACT_MAX_SOURCE_BYTES: u64 = 512 * 1024 * 1024;
pub const DEFAULT_ARCHIVE_EXTRACT_MAX_STAGING_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const DEFAULT_ARCHIVE_EXTRACT_MAX_UNCOMPRESSED_BYTES: u64 = 1024 * 1024 * 1024;
pub const DEFAULT_ARCHIVE_EXTRACT_MAX_ENTRIES: u64 = 10_000;
pub const DEFAULT_ARCHIVE_EXTRACT_MAX_FILES: u64 = 10_000;
pub const DEFAULT_ARCHIVE_EXTRACT_MAX_DIRECTORIES: u64 = 2_000;
pub const DEFAULT_ARCHIVE_EXTRACT_MAX_DEPTH: u64 = 64;
pub const DEFAULT_ARCHIVE_EXTRACT_MAX_PATH_BYTES: u64 = 4096;
pub const DEFAULT_ARCHIVE_EXTRACT_MAX_COMPRESSION_RATIO: u64 = 200;
pub const DEFAULT_ARCHIVE_EXTRACT_MAX_ENTRY_COMPRESSION_RATIO: u64 = 500;
pub const DEFAULT_ARCHIVE_EXTRACT_MAX_DURATION_SECS: u64 = 300;
pub const DEFAULT_ARCHIVE_BUILD_MAX_ENTRIES: u64 = 10_000;
pub const DEFAULT_ARCHIVE_BUILD_MAX_TOTAL_SOURCE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const DEFAULT_ARCHIVE_BUILD_MAX_TEMP_BYTES: u64 = 2 * 1024 * 1024 * 1024;

pub const MAX_LIST_PAGE_LIMIT: u64 = 1000;

pub fn normalize_interval_config_value(key: &str, value: &str) -> Result<String> {
    normalize_positive_u64_config_value(key, value)
}

pub fn normalize_concurrency_config_value(key: &str, value: &str) -> Result<String> {
    normalize_positive_u64_config_value(key, value)
}

pub fn normalize_attempts_config_value(key: &str, value: &str) -> Result<String> {
    normalize_positive_i32_config_value(key, value)
}

pub fn normalize_bytes_config_value(key: &str, value: &str) -> Result<String> {
    normalize_positive_u64_config_value(key, value)
}

pub fn normalize_list_max_limit_config_value(key: &str, value: &str) -> Result<String> {
    let parsed = parse_positive_u64(value).ok_or_else(|| {
        AsterError::validation_error(format!(
            "{key} must be a positive integer between 1 and {MAX_LIST_PAGE_LIMIT}",
        ))
    })?;
    if parsed > MAX_LIST_PAGE_LIMIT {
        return Err(AsterError::validation_error(format!(
            "{key} must be at most {MAX_LIST_PAGE_LIMIT}",
        )));
    }
    Ok(parsed.to_string())
}

pub fn mail_outbox_dispatch_interval_secs(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        MAIL_OUTBOX_DISPATCH_INTERVAL_SECS_KEY,
        DEFAULT_MAIL_OUTBOX_DISPATCH_INTERVAL_SECS,
    )
}

pub fn background_task_dispatch_interval_secs(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        BACKGROUND_TASK_DISPATCH_INTERVAL_SECS_KEY,
        DEFAULT_BACKGROUND_TASK_DISPATCH_INTERVAL_SECS,
    )
}

pub fn background_task_max_concurrency(runtime_config: &RuntimeConfig) -> usize {
    read_concurrency(
        runtime_config,
        BACKGROUND_TASK_MAX_CONCURRENCY_KEY,
        DEFAULT_BACKGROUND_TASK_MAX_CONCURRENCY,
    )
}

pub fn background_task_archive_max_concurrency(runtime_config: &RuntimeConfig) -> usize {
    read_concurrency(
        runtime_config,
        BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY_KEY,
        DEFAULT_BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY,
    )
}

pub fn background_task_thumbnail_max_concurrency(runtime_config: &RuntimeConfig) -> usize {
    read_concurrency(
        runtime_config,
        BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY_KEY,
        DEFAULT_BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY,
    )
}

pub fn background_task_max_attempts(runtime_config: &RuntimeConfig) -> i32 {
    read_positive_i32(
        runtime_config,
        BACKGROUND_TASK_MAX_ATTEMPTS_KEY,
        DEFAULT_BACKGROUND_TASK_MAX_ATTEMPTS,
    )
}

pub fn share_download_rollback_queue_capacity(runtime_config: &RuntimeConfig) -> usize {
    let default_value = usize_to_u64(
        DEFAULT_SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY,
        SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY_KEY,
    )
    .unwrap_or(u64::MAX);
    usize::try_from(read_positive_u64(
        runtime_config,
        SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY_KEY,
        default_value,
    ))
    .unwrap_or_else(|_| {
        tracing::warn!(
            key = SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY_KEY,
            "share download rollback queue capacity exceeds usize; using default"
        );
        DEFAULT_SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY
    })
}

pub fn maintenance_cleanup_interval_secs(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        MAINTENANCE_CLEANUP_INTERVAL_SECS_KEY,
        DEFAULT_MAINTENANCE_CLEANUP_INTERVAL_SECS,
    )
}

pub fn blob_reconcile_interval_secs(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        BLOB_RECONCILE_INTERVAL_SECS_KEY,
        DEFAULT_BLOB_RECONCILE_INTERVAL_SECS,
    )
}

pub fn remote_node_health_test_interval_secs(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS_KEY,
        DEFAULT_REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS,
    )
}

pub fn team_member_list_max_limit(runtime_config: &RuntimeConfig) -> u64 {
    read_bounded_u64(
        runtime_config,
        TEAM_MEMBER_LIST_MAX_LIMIT_KEY,
        DEFAULT_TEAM_MEMBER_LIST_MAX_LIMIT,
        1,
        MAX_LIST_PAGE_LIMIT,
    )
}

pub fn task_list_max_limit(runtime_config: &RuntimeConfig) -> u64 {
    read_bounded_u64(
        runtime_config,
        TASK_LIST_MAX_LIMIT_KEY,
        DEFAULT_TASK_LIST_MAX_LIMIT,
        1,
        MAX_LIST_PAGE_LIMIT,
    )
}

pub fn avatar_max_upload_size_bytes(runtime_config: &RuntimeConfig) -> usize {
    let default_value = u64_to_usize(
        DEFAULT_AVATAR_MAX_UPLOAD_SIZE_BYTES,
        AVATAR_MAX_UPLOAD_SIZE_BYTES_KEY,
    )
    .unwrap_or(usize::MAX);
    usize::try_from(read_positive_u64(
        runtime_config,
        AVATAR_MAX_UPLOAD_SIZE_BYTES_KEY,
        DEFAULT_AVATAR_MAX_UPLOAD_SIZE_BYTES,
    ))
    .unwrap_or_else(|_| {
        tracing::warn!(
            key = AVATAR_MAX_UPLOAD_SIZE_BYTES_KEY,
            "avatar upload size config exceeds usize; using default"
        );
        default_value
    })
}

pub fn thumbnail_max_source_bytes(runtime_config: &RuntimeConfig) -> i64 {
    let default_value = u64_to_i64(
        DEFAULT_THUMBNAIL_MAX_SOURCE_BYTES,
        THUMBNAIL_MAX_SOURCE_BYTES_KEY,
    )
    .unwrap_or(i64::MAX);
    u64_to_i64(
        read_positive_u64(
            runtime_config,
            THUMBNAIL_MAX_SOURCE_BYTES_KEY,
            DEFAULT_THUMBNAIL_MAX_SOURCE_BYTES,
        ),
        THUMBNAIL_MAX_SOURCE_BYTES_KEY,
    )
    .unwrap_or_else(|_| {
        tracing::warn!(
            key = THUMBNAIL_MAX_SOURCE_BYTES_KEY,
            "thumbnail source size config exceeds i64; using default"
        );
        default_value
    })
}

pub fn archive_extract_max_staging_bytes(runtime_config: &RuntimeConfig) -> i64 {
    read_positive_i64_bytes(
        runtime_config,
        ARCHIVE_EXTRACT_MAX_STAGING_BYTES_KEY,
        DEFAULT_ARCHIVE_EXTRACT_MAX_STAGING_BYTES,
        "archive extract staging size config exceeds i64; using default",
    )
}

pub fn archive_extract_max_source_bytes(runtime_config: &RuntimeConfig) -> i64 {
    read_positive_i64_bytes(
        runtime_config,
        ARCHIVE_EXTRACT_MAX_SOURCE_BYTES_KEY,
        DEFAULT_ARCHIVE_EXTRACT_MAX_SOURCE_BYTES,
        "archive extract source size config exceeds i64; using default",
    )
}

pub fn archive_extract_max_uncompressed_bytes(runtime_config: &RuntimeConfig) -> i64 {
    read_positive_i64_bytes(
        runtime_config,
        ARCHIVE_EXTRACT_MAX_UNCOMPRESSED_BYTES_KEY,
        DEFAULT_ARCHIVE_EXTRACT_MAX_UNCOMPRESSED_BYTES,
        "archive extract uncompressed size config exceeds i64; using default",
    )
}

pub fn archive_extract_max_entries(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        ARCHIVE_EXTRACT_MAX_ENTRIES_KEY,
        DEFAULT_ARCHIVE_EXTRACT_MAX_ENTRIES,
    )
}

pub fn archive_extract_max_files(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        ARCHIVE_EXTRACT_MAX_FILES_KEY,
        DEFAULT_ARCHIVE_EXTRACT_MAX_FILES,
    )
}

pub fn archive_extract_max_directories(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        ARCHIVE_EXTRACT_MAX_DIRECTORIES_KEY,
        DEFAULT_ARCHIVE_EXTRACT_MAX_DIRECTORIES,
    )
}

pub fn archive_extract_max_depth(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        ARCHIVE_EXTRACT_MAX_DEPTH_KEY,
        DEFAULT_ARCHIVE_EXTRACT_MAX_DEPTH,
    )
}

pub fn archive_extract_max_path_bytes(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        ARCHIVE_EXTRACT_MAX_PATH_BYTES_KEY,
        DEFAULT_ARCHIVE_EXTRACT_MAX_PATH_BYTES,
    )
}

pub fn archive_extract_max_compression_ratio(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        ARCHIVE_EXTRACT_MAX_COMPRESSION_RATIO_KEY,
        DEFAULT_ARCHIVE_EXTRACT_MAX_COMPRESSION_RATIO,
    )
}

pub fn archive_extract_max_entry_compression_ratio(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        ARCHIVE_EXTRACT_MAX_ENTRY_COMPRESSION_RATIO_KEY,
        DEFAULT_ARCHIVE_EXTRACT_MAX_ENTRY_COMPRESSION_RATIO,
    )
}

pub fn archive_extract_max_duration_secs(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        ARCHIVE_EXTRACT_MAX_DURATION_SECS_KEY,
        DEFAULT_ARCHIVE_EXTRACT_MAX_DURATION_SECS,
    )
}

pub fn archive_build_max_entries(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        ARCHIVE_BUILD_MAX_ENTRIES_KEY,
        DEFAULT_ARCHIVE_BUILD_MAX_ENTRIES,
    )
}

pub fn archive_build_max_total_source_bytes(runtime_config: &RuntimeConfig) -> i64 {
    read_positive_i64_bytes(
        runtime_config,
        ARCHIVE_BUILD_MAX_TOTAL_SOURCE_BYTES_KEY,
        DEFAULT_ARCHIVE_BUILD_MAX_TOTAL_SOURCE_BYTES,
        "archive build total source size config exceeds i64; using default",
    )
}

pub fn archive_build_max_temp_bytes(runtime_config: &RuntimeConfig) -> i64 {
    read_positive_i64_bytes(
        runtime_config,
        ARCHIVE_BUILD_MAX_TEMP_BYTES_KEY,
        DEFAULT_ARCHIVE_BUILD_MAX_TEMP_BYTES,
        "archive build temp size config exceeds i64; using default",
    )
}

fn read_positive_i64_bytes(
    runtime_config: &RuntimeConfig,
    key: &str,
    default: u64,
    overflow_message: &str,
) -> i64 {
    let default_value = u64_to_i64(default, key).unwrap_or(i64::MAX);
    u64_to_i64(read_positive_u64(runtime_config, key, default), key).unwrap_or_else(|_| {
        tracing::warn!(key, overflow_message);
        default_value
    })
}

fn normalize_positive_u64_config_value(key: &str, value: &str) -> Result<String> {
    let parsed = parse_positive_u64(value)
        .ok_or_else(|| AsterError::validation_error(format!("{key} must be a positive integer")))?;
    Ok(parsed.to_string())
}

fn normalize_positive_i32_config_value(key: &str, value: &str) -> Result<String> {
    let parsed = parse_positive_i32(value)
        .ok_or_else(|| AsterError::validation_error(format!("{key} must be a positive integer")))?;
    Ok(parsed.to_string())
}

fn parse_positive_u64(value: &str) -> Option<u64> {
    let parsed = value.trim().parse::<u64>().ok()?;
    (parsed > 0).then_some(parsed)
}

fn parse_positive_i32(value: &str) -> Option<i32> {
    let parsed = value.trim().parse::<i32>().ok()?;
    (parsed > 0).then_some(parsed)
}

fn read_positive_u64(runtime_config: &RuntimeConfig, key: &str, default: u64) -> u64 {
    match runtime_config.get(key) {
        Some(raw) => match parse_positive_u64(&raw) {
            Some(value) => value,
            None => {
                tracing::warn!(key, value = %raw, "invalid runtime operations config; using default");
                default
            }
        },
        None => default,
    }
}

fn read_positive_i32(runtime_config: &RuntimeConfig, key: &str, default: i32) -> i32 {
    match runtime_config.get(key) {
        Some(raw) => match parse_positive_i32(&raw) {
            Some(value) => value,
            None => {
                tracing::warn!(key, value = %raw, "invalid runtime operations config; using default");
                default
            }
        },
        None => default,
    }
}

fn read_concurrency(runtime_config: &RuntimeConfig, key: &str, default: usize) -> usize {
    let default_value = usize_to_u64(default, key).unwrap_or(u64::MAX);
    usize::try_from(read_positive_u64(runtime_config, key, default_value)).unwrap_or_else(|_| {
        tracing::warn!(key, "{key} exceeds usize; using default");
        default
    })
}

fn read_bounded_u64(
    runtime_config: &RuntimeConfig,
    key: &str,
    default: u64,
    min: u64,
    max: u64,
) -> u64 {
    match runtime_config.get(key) {
        Some(raw) => match raw.trim().parse::<u64>() {
            Ok(value) if (min..=max).contains(&value) => value,
            _ => {
                tracing::warn!(
                    key,
                    value = %raw,
                    min,
                    max,
                    "invalid runtime operations config; using default"
                );
                default
            }
        },
        None => default,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ARCHIVE_EXTRACT_MAX_STAGING_BYTES_KEY, AVATAR_MAX_UPLOAD_SIZE_BYTES_KEY,
        BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY_KEY, BACKGROUND_TASK_MAX_ATTEMPTS_KEY,
        BACKGROUND_TASK_MAX_CONCURRENCY_KEY, BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY_KEY,
        BLOB_RECONCILE_INTERVAL_SECS_KEY, DEFAULT_ARCHIVE_EXTRACT_MAX_STAGING_BYTES,
        DEFAULT_AVATAR_MAX_UPLOAD_SIZE_BYTES, DEFAULT_BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY,
        DEFAULT_BACKGROUND_TASK_MAX_ATTEMPTS, DEFAULT_BACKGROUND_TASK_MAX_CONCURRENCY,
        DEFAULT_BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY, DEFAULT_BLOB_RECONCILE_INTERVAL_SECS,
        DEFAULT_REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS,
        DEFAULT_SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY, DEFAULT_TASK_LIST_MAX_LIMIT,
        DEFAULT_TEAM_MEMBER_LIST_MAX_LIMIT, REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS_KEY,
        SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY_KEY, TASK_LIST_MAX_LIMIT_KEY,
        TEAM_MEMBER_LIST_MAX_LIMIT_KEY, archive_extract_max_staging_bytes,
        avatar_max_upload_size_bytes, background_task_archive_max_concurrency,
        background_task_max_attempts, background_task_max_concurrency,
        background_task_thumbnail_max_concurrency, blob_reconcile_interval_secs,
        normalize_attempts_config_value, normalize_bytes_config_value,
        normalize_concurrency_config_value, normalize_interval_config_value,
        normalize_list_max_limit_config_value, remote_node_health_test_interval_secs,
        share_download_rollback_queue_capacity, task_list_max_limit, team_member_list_max_limit,
    };
    use crate::config::RuntimeConfig;
    use crate::entities::system_config;
    use chrono::Utc;

    fn config_model(key: &str, value: &str) -> system_config::Model {
        system_config::Model {
            id: 1,
            key: key.to_string(),
            value: value.to_string(),
            value_type: crate::types::SystemConfigValueType::Number,
            requires_restart: false,
            is_sensitive: false,
            source: crate::types::SystemConfigSource::System,
            namespace: String::new(),
            category: "operations".to_string(),
            description: "test".to_string(),
            updated_at: Utc::now(),
            updated_by: None,
        }
    }

    #[test]
    fn interval_reader_uses_default_for_missing_and_invalid_values() {
        let runtime_config = RuntimeConfig::new();
        assert_eq!(
            blob_reconcile_interval_secs(&runtime_config),
            DEFAULT_BLOB_RECONCILE_INTERVAL_SECS
        );
        assert_eq!(
            remote_node_health_test_interval_secs(&runtime_config),
            DEFAULT_REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS
        );

        runtime_config.apply(config_model(BLOB_RECONCILE_INTERVAL_SECS_KEY, "0"));
        assert_eq!(
            blob_reconcile_interval_secs(&runtime_config),
            DEFAULT_BLOB_RECONCILE_INTERVAL_SECS
        );

        runtime_config.apply(config_model(
            REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS_KEY,
            "120",
        ));
        assert_eq!(remote_node_health_test_interval_secs(&runtime_config), 120);

        runtime_config.apply(config_model(REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS_KEY, "0"));
        assert_eq!(
            remote_node_health_test_interval_secs(&runtime_config),
            DEFAULT_REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS
        );
    }

    #[test]
    fn list_limit_reader_accepts_bounded_values_only() {
        let runtime_config = RuntimeConfig::new();
        assert_eq!(
            team_member_list_max_limit(&runtime_config),
            DEFAULT_TEAM_MEMBER_LIST_MAX_LIMIT
        );

        runtime_config.apply(config_model(TEAM_MEMBER_LIST_MAX_LIMIT_KEY, "250"));
        runtime_config.apply(config_model(TASK_LIST_MAX_LIMIT_KEY, "0"));

        assert_eq!(team_member_list_max_limit(&runtime_config), 250);
        assert_eq!(
            task_list_max_limit(&runtime_config),
            DEFAULT_TASK_LIST_MAX_LIMIT
        );
    }

    #[test]
    fn background_task_concurrency_reader_uses_runtime_value_and_default() {
        let runtime_config = RuntimeConfig::new();
        assert_eq!(
            background_task_max_concurrency(&runtime_config),
            DEFAULT_BACKGROUND_TASK_MAX_CONCURRENCY
        );
        assert_eq!(
            background_task_archive_max_concurrency(&runtime_config),
            DEFAULT_BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY
        );
        assert_eq!(
            background_task_thumbnail_max_concurrency(&runtime_config),
            DEFAULT_BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY
        );

        runtime_config.apply(config_model(BACKGROUND_TASK_MAX_CONCURRENCY_KEY, "3"));
        runtime_config.apply(config_model(
            BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY_KEY,
            "2",
        ));
        runtime_config.apply(config_model(
            BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY_KEY,
            "4",
        ));
        assert_eq!(background_task_max_concurrency(&runtime_config), 3usize);
        assert_eq!(
            background_task_archive_max_concurrency(&runtime_config),
            2usize
        );
        assert_eq!(
            background_task_thumbnail_max_concurrency(&runtime_config),
            4usize
        );

        runtime_config.apply(config_model(BACKGROUND_TASK_MAX_CONCURRENCY_KEY, "0"));
        runtime_config.apply(config_model(
            BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY_KEY,
            "0",
        ));
        runtime_config.apply(config_model(
            BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY_KEY,
            "0",
        ));
        assert_eq!(
            background_task_max_concurrency(&runtime_config),
            DEFAULT_BACKGROUND_TASK_MAX_CONCURRENCY
        );
        assert_eq!(
            background_task_archive_max_concurrency(&runtime_config),
            DEFAULT_BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY
        );
        assert_eq!(
            background_task_thumbnail_max_concurrency(&runtime_config),
            DEFAULT_BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY
        );

        runtime_config.apply(config_model(BACKGROUND_TASK_MAX_CONCURRENCY_KEY, "abc"));
        runtime_config.apply(config_model(
            BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY_KEY,
            "abc",
        ));
        runtime_config.apply(config_model(
            BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY_KEY,
            "abc",
        ));
        assert_eq!(
            background_task_max_concurrency(&runtime_config),
            DEFAULT_BACKGROUND_TASK_MAX_CONCURRENCY
        );
        assert_eq!(
            background_task_archive_max_concurrency(&runtime_config),
            DEFAULT_BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY
        );
        assert_eq!(
            background_task_thumbnail_max_concurrency(&runtime_config),
            DEFAULT_BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY
        );
    }

    #[test]
    fn background_task_attempts_reader_uses_runtime_value_and_default() {
        let runtime_config = RuntimeConfig::new();
        assert_eq!(
            background_task_max_attempts(&runtime_config),
            DEFAULT_BACKGROUND_TASK_MAX_ATTEMPTS
        );

        runtime_config.apply(config_model(BACKGROUND_TASK_MAX_ATTEMPTS_KEY, "5"));
        assert_eq!(background_task_max_attempts(&runtime_config), 5);

        runtime_config.apply(config_model(BACKGROUND_TASK_MAX_ATTEMPTS_KEY, "0"));
        assert_eq!(
            background_task_max_attempts(&runtime_config),
            DEFAULT_BACKGROUND_TASK_MAX_ATTEMPTS
        );
    }

    #[test]
    fn share_download_rollback_queue_capacity_reader_uses_runtime_value_and_default() {
        let runtime_config = RuntimeConfig::new();
        assert_eq!(
            share_download_rollback_queue_capacity(&runtime_config),
            DEFAULT_SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY
        );

        runtime_config.apply(config_model(
            SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY_KEY,
            "2048",
        ));
        assert_eq!(
            share_download_rollback_queue_capacity(&runtime_config),
            2048
        );

        runtime_config.apply(config_model(
            SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY_KEY,
            "0",
        ));
        assert_eq!(
            share_download_rollback_queue_capacity(&runtime_config),
            DEFAULT_SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY
        );
    }

    #[test]
    fn avatar_upload_reader_uses_runtime_value() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(AVATAR_MAX_UPLOAD_SIZE_BYTES_KEY, "4096"));
        assert_eq!(avatar_max_upload_size_bytes(&runtime_config), 4096usize);
    }

    #[test]
    fn archive_extract_staging_reader_uses_runtime_value_and_default() {
        let runtime_config = RuntimeConfig::new();
        assert_eq!(
            archive_extract_max_staging_bytes(&runtime_config),
            crate::utils::numbers::u64_to_i64(
                DEFAULT_ARCHIVE_EXTRACT_MAX_STAGING_BYTES,
                ARCHIVE_EXTRACT_MAX_STAGING_BYTES_KEY,
            )
            .unwrap()
        );

        runtime_config.apply(config_model(
            ARCHIVE_EXTRACT_MAX_STAGING_BYTES_KEY,
            "1048576",
        ));
        assert_eq!(
            archive_extract_max_staging_bytes(&runtime_config),
            1_048_576
        );
    }

    #[test]
    fn normalize_helpers_reject_invalid_values() {
        assert_eq!(
            normalize_interval_config_value("test_interval", " 60 ").unwrap(),
            "60"
        );
        assert_eq!(
            normalize_concurrency_config_value("test_concurrency", "4").unwrap(),
            "4"
        );
        assert_eq!(
            normalize_attempts_config_value("test_attempts", "3").unwrap(),
            "3"
        );
        assert_eq!(
            normalize_bytes_config_value("test_bytes", "1024").unwrap(),
            "1024"
        );
        assert_eq!(
            normalize_list_max_limit_config_value("test_limit", "1000").unwrap(),
            "1000"
        );
        assert!(normalize_interval_config_value("test_interval", "0").is_err());
        assert!(normalize_concurrency_config_value("test_concurrency", "0").is_err());
        assert!(normalize_attempts_config_value("test_attempts", "0").is_err());
        assert!(normalize_bytes_config_value("test_bytes", "-1").is_err());
        assert!(normalize_list_max_limit_config_value("test_limit", "1001").is_err());
    }

    #[test]
    fn avatar_upload_reader_falls_back_for_invalid_values() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(AVATAR_MAX_UPLOAD_SIZE_BYTES_KEY, "invalid"));
        assert_eq!(
            avatar_max_upload_size_bytes(&runtime_config),
            crate::utils::numbers::u64_to_usize(
                DEFAULT_AVATAR_MAX_UPLOAD_SIZE_BYTES,
                AVATAR_MAX_UPLOAD_SIZE_BYTES_KEY,
            )
            .unwrap()
        );
    }

    #[test]
    fn archive_extract_staging_reader_falls_back_for_invalid_values() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(
            ARCHIVE_EXTRACT_MAX_STAGING_BYTES_KEY,
            "invalid",
        ));
        assert_eq!(
            archive_extract_max_staging_bytes(&runtime_config),
            crate::utils::numbers::u64_to_i64(
                DEFAULT_ARCHIVE_EXTRACT_MAX_STAGING_BYTES,
                ARCHIVE_EXTRACT_MAX_STAGING_BYTES_KEY,
            )
            .unwrap()
        );
    }
}
