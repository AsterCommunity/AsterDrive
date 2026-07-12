//! 配置子模块：`operations`。

use crate::config::bool_like::parse_bool_like;
use crate::config::{RuntimeConfig, media_processing};
use crate::errors::{AsterError, Result};
use crate::utils::numbers::{u64_to_i64, u64_to_usize, usize_to_u64};
use serde::{Deserialize, Serialize};

pub use crate::config::definitions::{
    ARCHIVE_BUILD_MAX_ENTRIES_KEY, ARCHIVE_BUILD_MAX_TEMP_BYTES_KEY,
    ARCHIVE_BUILD_MAX_TOTAL_SOURCE_BYTES_KEY, ARCHIVE_COMPRESS_ENABLED_KEY,
    ARCHIVE_DOWNLOAD_SHARE_ENABLED_KEY, ARCHIVE_DOWNLOAD_USER_ENABLED_KEY,
    ARCHIVE_EXTRACT_MAX_COMPRESSION_RATIO_KEY, ARCHIVE_EXTRACT_MAX_DEPTH_KEY,
    ARCHIVE_EXTRACT_MAX_DIRECTORIES_KEY, ARCHIVE_EXTRACT_MAX_DURATION_SECS_KEY,
    ARCHIVE_EXTRACT_MAX_ENTRIES_KEY, ARCHIVE_EXTRACT_MAX_ENTRY_COMPRESSION_RATIO_KEY,
    ARCHIVE_EXTRACT_MAX_FILES_KEY, ARCHIVE_EXTRACT_MAX_PATH_BYTES_KEY,
    ARCHIVE_EXTRACT_MAX_SOURCE_BYTES_KEY, ARCHIVE_EXTRACT_MAX_STAGING_BYTES_KEY,
    ARCHIVE_EXTRACT_MAX_UNCOMPRESSED_BYTES_KEY, ARCHIVE_PREVIEW_ENABLED_KEY,
    ARCHIVE_PREVIEW_MAX_DURATION_SECS_KEY, ARCHIVE_PREVIEW_MAX_ENTRIES_KEY,
    ARCHIVE_PREVIEW_MAX_MANIFEST_BYTES_KEY, ARCHIVE_PREVIEW_MAX_SOURCE_BYTES_KEY,
    ARCHIVE_PREVIEW_SHARE_ENABLED_KEY, ARCHIVE_PREVIEW_USER_ENABLED_KEY,
    AVATAR_MAX_UPLOAD_SIZE_BYTES_KEY, BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY_KEY,
    BACKGROUND_TASK_DISPATCH_IDLE_MAX_INTERVAL_SECS_KEY,
    BACKGROUND_TASK_DISPATCH_INTERVAL_SECS_KEY, BACKGROUND_TASK_MAX_ATTEMPTS_KEY,
    BACKGROUND_TASK_MAX_CONCURRENCY_KEY, BACKGROUND_TASK_STORAGE_MIGRATION_MAX_CONCURRENCY_KEY,
    BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY_KEY, BLOB_RECONCILE_INTERVAL_SECS_KEY,
    FRONTEND_IMAGE_PREVIEW_PREFERENCE_KEY, IMAGE_PREVIEW_MAX_DIMENSION_KEY,
    MAIL_OUTBOX_DISPATCH_INTERVAL_SECS_KEY, MAINTENANCE_CLEANUP_INTERVAL_SECS_KEY,
    MEDIA_METADATA_ENABLED_KEY, MEDIA_METADATA_MAX_SOURCE_BYTES_KEY,
    OFFLINE_DOWNLOAD_ARIA2_LOWEST_SPEED_LIMIT_BYTES_PER_SEC_KEY,
    OFFLINE_DOWNLOAD_ARIA2_MAX_CONNECTION_PER_SERVER_KEY,
    OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS_KEY, OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY,
    OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY, OFFLINE_DOWNLOAD_ARIA2_SPLIT_KEY,
    OFFLINE_DOWNLOAD_ENGINE_KEY, OFFLINE_DOWNLOAD_ENGINE_REGISTRY_JSON_KEY,
    OFFLINE_DOWNLOAD_MAX_CONCURRENCY_KEY, OFFLINE_DOWNLOAD_MAX_FILE_SIZE_BYTES_KEY,
    OFFLINE_DOWNLOAD_MAX_MB_PER_SEC_KEY, OFFLINE_DOWNLOAD_REQUEST_TIMEOUT_SECS_KEY,
    OFFLINE_DOWNLOAD_TEMP_DIR_KEY, REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS_KEY,
    SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY_KEY, SHARE_STREAM_SESSION_TTL_SECS_KEY,
    TASK_LIST_MAX_LIMIT_KEY, TEAM_MEMBER_LIST_MAX_LIMIT_KEY, THUMBNAIL_MAX_DIMENSION_KEY,
    THUMBNAIL_MAX_SOURCE_BYTES_KEY,
};

pub const DEFAULT_MAIL_OUTBOX_DISPATCH_INTERVAL_SECS: u64 = 5;
pub const DEFAULT_BACKGROUND_TASK_DISPATCH_INTERVAL_SECS: u64 = 5;
pub const DEFAULT_BACKGROUND_TASK_DISPATCH_IDLE_MAX_INTERVAL_SECS: u64 = 60;
pub const DEFAULT_BACKGROUND_TASK_MAX_CONCURRENCY: usize = 1;
pub const DEFAULT_BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY: usize = 2;
pub const DEFAULT_BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY: usize = 1;
pub const DEFAULT_BACKGROUND_TASK_STORAGE_MIGRATION_MAX_CONCURRENCY: usize = 1;
const MAX_BACKGROUND_TASK_CONCURRENCY: usize = 16;
pub const DEFAULT_BACKGROUND_TASK_MAX_ATTEMPTS: i32 = 3;
pub const DEFAULT_SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY: usize = 1024;
pub const DEFAULT_SHARE_STREAM_SESSION_TTL_SECS: u64 = 3 * 60 * 60;
pub const MIN_SHARE_STREAM_SESSION_TTL_SECS: u64 = 5 * 60;
pub const MAX_SHARE_STREAM_SESSION_TTL_SECS: u64 = 24 * 60 * 60;
pub const DEFAULT_MAINTENANCE_CLEANUP_INTERVAL_SECS: u64 = 3600;
pub const DEFAULT_BLOB_RECONCILE_INTERVAL_SECS: u64 = 6 * 3600;
pub const DEFAULT_REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS: u64 = 300;
pub const DEFAULT_TEAM_MEMBER_LIST_MAX_LIMIT: u64 = 100;
pub const DEFAULT_TASK_LIST_MAX_LIMIT: u64 = 100;
pub const DEFAULT_AVATAR_MAX_UPLOAD_SIZE_BYTES: u64 = 10 * 1024 * 1024;
pub const DEFAULT_THUMBNAIL_MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
pub const DEFAULT_THUMBNAIL_MAX_DIMENSION: u32 = 400;
pub const DEFAULT_IMAGE_PREVIEW_MAX_DIMENSION: u32 = 1600;
pub const MAX_DERIVATIVE_MAX_DIMENSION: u32 = 16_384;
pub const DEFAULT_MEDIA_METADATA_ENABLED: bool = true;
pub const DEFAULT_MEDIA_METADATA_MAX_SOURCE_BYTES: u64 = 256 * 1024 * 1024;
/// Default offline download limits are sized for slow-but-usable links.
/// With the default 1 GiB cap and 600s request timeout, sustained throughput
/// must stay above roughly 1.7 MiB/s to finish in time. The default 5 MiB/s
/// speed cap leaves room above that target; operators who lower the cap or run
/// on slower networks should raise `offline_download_request_timeout_secs`.
pub const DEFAULT_OFFLINE_DOWNLOAD_MAX_FILE_SIZE_BYTES: u64 = 1024 * 1024 * 1024;
pub const DEFAULT_OFFLINE_DOWNLOAD_MAX_MB_PER_SEC: u64 = 5;
pub const DEFAULT_OFFLINE_DOWNLOAD_MAX_CONCURRENCY: usize = 1;
/// See the note above `DEFAULT_OFFLINE_DOWNLOAD_MAX_FILE_SIZE_BYTES`.
pub const DEFAULT_OFFLINE_DOWNLOAD_REQUEST_TIMEOUT_SECS: u64 = 600;
pub const DEFAULT_OFFLINE_DOWNLOAD_ENGINE: &str = "builtin";
pub const DEFAULT_OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS: u64 = 10;
pub const DEFAULT_OFFLINE_DOWNLOAD_ARIA2_SPLIT: u64 = 5;
pub const DEFAULT_OFFLINE_DOWNLOAD_ARIA2_MAX_CONNECTION_PER_SERVER: u64 = 5;
pub const DEFAULT_OFFLINE_DOWNLOAD_ARIA2_LOWEST_SPEED_LIMIT_BYTES_PER_SEC: u64 = 0;
const MAX_OFFLINE_DOWNLOAD_TEMP_DIR_LEN: usize = 4096;
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
pub const DEFAULT_ARCHIVE_PREVIEW_ENABLED: bool = false;
pub const DEFAULT_ARCHIVE_PREVIEW_USER_ENABLED: bool = false;
pub const DEFAULT_ARCHIVE_PREVIEW_SHARE_ENABLED: bool = false;
pub const DEFAULT_ARCHIVE_PREVIEW_MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
pub const DEFAULT_ARCHIVE_PREVIEW_MAX_ENTRIES: u64 = 2_000;
pub const DEFAULT_ARCHIVE_PREVIEW_MAX_MANIFEST_BYTES: u64 = 64 * 1024;
pub const DEFAULT_ARCHIVE_PREVIEW_MAX_DURATION_SECS: u64 = 30;
pub const DEFAULT_ARCHIVE_COMPRESS_ENABLED: bool = true;
pub const DEFAULT_ARCHIVE_BUILD_MAX_ENTRIES: u64 = 10_000;
pub const DEFAULT_ARCHIVE_BUILD_MAX_TOTAL_SOURCE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const DEFAULT_ARCHIVE_BUILD_MAX_TEMP_BYTES: u64 = 2 * 1024 * 1024 * 1024;

pub const MAX_LIST_PAGE_LIMIT: u64 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfflineDownloadEngine {
    Builtin,
    Aria2,
}

impl OfflineDownloadEngine {
    pub const ALL: [Self; 2] = [Self::Builtin, Self::Aria2];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Aria2 => "aria2",
        }
    }

    pub const fn default_enabled(self) -> bool {
        match self {
            Self::Builtin => true,
            Self::Aria2 => false,
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Builtin => "AsterDrive built-in",
            Self::Aria2 => "aria2",
        }
    }
}

pub fn parse_offline_download_engine(value: &str) -> Option<OfflineDownloadEngine> {
    let value = value.trim().to_ascii_lowercase();
    OfflineDownloadEngine::ALL
        .into_iter()
        .find(|engine| engine.as_str() == value)
}

pub fn normalize_interval_config_value(key: &str, value: &str) -> Result<String> {
    normalize_positive_u64_config_value(key, value)
}

pub fn normalize_concurrency_config_value(key: &str, value: &str) -> Result<String> {
    let parsed = parse_positive_u64(value)
        .ok_or_else(|| AsterError::validation_error(format!("{key} must be a positive integer")))?;
    if parsed > usize_to_u64(MAX_BACKGROUND_TASK_CONCURRENCY, key).unwrap_or(u64::MAX) {
        return Err(AsterError::validation_error(format!(
            "{key} cannot exceed {MAX_BACKGROUND_TASK_CONCURRENCY}"
        )));
    }
    Ok(parsed.to_string())
}

pub fn normalize_queue_capacity_config_value(key: &str, value: &str) -> Result<String> {
    normalize_positive_u64_config_value(key, value)
}

pub fn normalize_attempts_config_value(key: &str, value: &str) -> Result<String> {
    normalize_positive_i32_config_value(key, value)
}

pub fn normalize_bytes_config_value(key: &str, value: &str) -> Result<String> {
    normalize_positive_u64_config_value(key, value)
}

pub fn normalize_derivative_dimension_config_value(key: &str, value: &str) -> Result<String> {
    let parsed = parse_positive_u64(value)
        .ok_or_else(|| AsterError::validation_error(format!("{key} must be a positive integer")))?;
    if parsed > u64::from(MAX_DERIVATIVE_MAX_DIMENSION) {
        return Err(AsterError::validation_error(format!(
            "{key} cannot exceed {MAX_DERIVATIVE_MAX_DIMENSION}"
        )));
    }
    Ok(parsed.to_string())
}

pub fn normalize_non_negative_u64_config_value(key: &str, value: &str) -> Result<String> {
    let parsed = parse_non_negative_u64(value).ok_or_else(|| {
        AsterError::validation_error(format!("{key} must be a non-negative integer"))
    })?;
    Ok(parsed.to_string())
}

pub fn normalize_offline_download_engine_config_value(value: &str) -> Result<String> {
    parse_offline_download_engine(value)
        .map(|engine| engine.as_str().to_string())
        .ok_or_else(|| {
            AsterError::validation_error("offline_download_engine must be 'builtin' or 'aria2'")
        })
}

pub fn normalize_offline_download_aria2_rpc_url_config_value(value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let url = url::Url::parse(trimmed).map_err(|error| {
        AsterError::validation_error(format!(
            "offline_download_aria2_rpc_url is invalid: {error}"
        ))
    })?;
    match url.scheme() {
        "http" | "https" => {}
        _ => {
            return Err(AsterError::validation_error(
                "offline_download_aria2_rpc_url must use http or https",
            ));
        }
    }
    if url.host_str().is_none() {
        return Err(AsterError::validation_error(
            "offline_download_aria2_rpc_url must include a host",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(AsterError::validation_error(
            "offline_download_aria2_rpc_url must not include credentials",
        ));
    }
    Ok(url.to_string())
}

pub fn normalize_offline_download_temp_dir_config_value(value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    if trimmed.len() > MAX_OFFLINE_DOWNLOAD_TEMP_DIR_LEN {
        return Err(AsterError::validation_error(format!(
            "offline_download_temp_dir exceeds {MAX_OFFLINE_DOWNLOAD_TEMP_DIR_LEN} characters"
        )));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(AsterError::validation_error(
            "offline_download_temp_dir cannot contain control characters",
        ));
    }
    if !std::path::Path::new(trimmed).is_absolute() {
        return Err(AsterError::validation_error(
            "offline_download_temp_dir must be an absolute path, or blank to use server.temp_dir",
        ));
    }
    Ok(trimmed.to_string())
}

pub fn normalize_bool_config_value(key: &str, value: &str) -> Result<String> {
    match parse_bool_like(value) {
        Some(value) => Ok(if value { "true" } else { "false" }.to_string()),
        None => Err(AsterError::validation_error(format!(
            "{key} must be 'true' or 'false'",
        ))),
    }
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

pub fn normalize_share_stream_session_ttl_config_value(key: &str, value: &str) -> Result<String> {
    let parsed = parse_positive_u64(value).ok_or_else(|| {
        AsterError::validation_error(format!(
            "{key} must be a positive integer between {MIN_SHARE_STREAM_SESSION_TTL_SECS} and {MAX_SHARE_STREAM_SESSION_TTL_SECS}"
        ))
    })?;
    if !(MIN_SHARE_STREAM_SESSION_TTL_SECS..=MAX_SHARE_STREAM_SESSION_TTL_SECS).contains(&parsed) {
        return Err(AsterError::validation_error(format!(
            "{key} must be between {MIN_SHARE_STREAM_SESSION_TTL_SECS} and {MAX_SHARE_STREAM_SESSION_TTL_SECS}"
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

pub fn background_task_dispatch_idle_max_interval_secs(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        BACKGROUND_TASK_DISPATCH_IDLE_MAX_INTERVAL_SECS_KEY,
        DEFAULT_BACKGROUND_TASK_DISPATCH_IDLE_MAX_INTERVAL_SECS,
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

pub fn background_task_storage_migration_max_concurrency(runtime_config: &RuntimeConfig) -> usize {
    read_concurrency(
        runtime_config,
        BACKGROUND_TASK_STORAGE_MIGRATION_MAX_CONCURRENCY_KEY,
        DEFAULT_BACKGROUND_TASK_STORAGE_MIGRATION_MAX_CONCURRENCY,
    )
}

pub fn offline_download_max_concurrency(runtime_config: &RuntimeConfig) -> usize {
    read_concurrency(
        runtime_config,
        OFFLINE_DOWNLOAD_MAX_CONCURRENCY_KEY,
        DEFAULT_OFFLINE_DOWNLOAD_MAX_CONCURRENCY,
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

pub fn share_stream_session_ttl_secs(runtime_config: &RuntimeConfig) -> u64 {
    read_bounded_u64(
        runtime_config,
        SHARE_STREAM_SESSION_TTL_SECS_KEY,
        DEFAULT_SHARE_STREAM_SESSION_TTL_SECS,
        MIN_SHARE_STREAM_SESSION_TTL_SECS,
        MAX_SHARE_STREAM_SESSION_TTL_SECS,
    )
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

pub fn thumbnail_max_dimension(runtime_config: &RuntimeConfig) -> u32 {
    read_bounded_u32(
        runtime_config,
        THUMBNAIL_MAX_DIMENSION_KEY,
        DEFAULT_THUMBNAIL_MAX_DIMENSION,
        1,
        MAX_DERIVATIVE_MAX_DIMENSION,
    )
}

pub fn image_preview_max_dimension(runtime_config: &RuntimeConfig) -> u32 {
    read_bounded_u32(
        runtime_config,
        IMAGE_PREVIEW_MAX_DIMENSION_KEY,
        DEFAULT_IMAGE_PREVIEW_MAX_DIMENSION,
        1,
        MAX_DERIVATIVE_MAX_DIMENSION,
    )
}

pub fn media_metadata_enabled(runtime_config: &RuntimeConfig) -> bool {
    read_bool(
        runtime_config,
        MEDIA_METADATA_ENABLED_KEY,
        DEFAULT_MEDIA_METADATA_ENABLED,
    )
}

pub fn media_metadata_max_source_bytes(runtime_config: &RuntimeConfig) -> i64 {
    read_positive_i64_bytes(
        runtime_config,
        MEDIA_METADATA_MAX_SOURCE_BYTES_KEY,
        DEFAULT_MEDIA_METADATA_MAX_SOURCE_BYTES,
        "media metadata source size config exceeds i64; using default",
    )
}

pub fn frontend_image_preview_preference(
    runtime_config: &RuntimeConfig,
) -> media_processing::PublicImagePreviewPreference {
    media_processing::PublicImagePreviewPreference::from_config_value(
        runtime_config
            .get(FRONTEND_IMAGE_PREVIEW_PREFERENCE_KEY)
            .as_deref()
            .unwrap_or("original_first"),
    )
}

pub fn normalize_frontend_image_preview_preference_config_value(value: &str) -> Result<String> {
    let value = value.trim();
    match value {
        "original_first" | "preview_first" => Ok(value.to_string()),
        _ => Err(AsterError::validation_error(
            "frontend image preview preference must be original_first or preview_first",
        )),
    }
}

pub fn offline_download_max_file_size_bytes(runtime_config: &RuntimeConfig) -> i64 {
    read_positive_i64_bytes(
        runtime_config,
        OFFLINE_DOWNLOAD_MAX_FILE_SIZE_BYTES_KEY,
        DEFAULT_OFFLINE_DOWNLOAD_MAX_FILE_SIZE_BYTES,
        "offline download max file size config exceeds i64; using default",
    )
}

pub fn offline_download_engine(runtime_config: &RuntimeConfig) -> OfflineDownloadEngine {
    runtime_config
        .get(OFFLINE_DOWNLOAD_ENGINE_KEY)
        .as_deref()
        .and_then(parse_offline_download_engine)
        .unwrap_or(OfflineDownloadEngine::Builtin)
}

pub fn offline_download_enabled_engines(
    runtime_config: &RuntimeConfig,
) -> Vec<OfflineDownloadEngine> {
    if let Some(registry) = runtime_config.get(OFFLINE_DOWNLOAD_ENGINE_REGISTRY_JSON_KEY) {
        match crate::config::offline_download::enabled_offline_download_engines_from_registry_value(
            &registry,
        ) {
            Ok(engines) => return engines,
            Err(error) => {
                tracing::warn!(
                    "invalid offline_download_engine_registry_json runtime config; falling back to offline_download_engine: {error}"
                );
            }
        }
    }

    vec![offline_download_engine(runtime_config)]
}

pub fn offline_download_max_bytes_per_sec(runtime_config: &RuntimeConfig) -> Option<u64> {
    let max_mb_per_sec = read_non_negative_u64(
        runtime_config,
        OFFLINE_DOWNLOAD_MAX_MB_PER_SEC_KEY,
        DEFAULT_OFFLINE_DOWNLOAD_MAX_MB_PER_SEC,
    );
    if max_mb_per_sec == 0 {
        return None;
    }

    max_mb_per_sec.checked_mul(1024 * 1024).or_else(|| {
        tracing::warn!(
            key = OFFLINE_DOWNLOAD_MAX_MB_PER_SEC_KEY,
            "offline download speed config exceeds u64 bytes/s; disabling speed limit"
        );
        None
    })
}

pub fn offline_download_request_timeout_secs(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        OFFLINE_DOWNLOAD_REQUEST_TIMEOUT_SECS_KEY,
        DEFAULT_OFFLINE_DOWNLOAD_REQUEST_TIMEOUT_SECS,
    )
}

pub fn offline_download_aria2_rpc_url(runtime_config: &RuntimeConfig) -> Option<String> {
    runtime_config
        .get(OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn offline_download_aria2_rpc_secret(runtime_config: &RuntimeConfig) -> Option<String> {
    runtime_config
        .get(OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn offline_download_temp_dir(runtime_config: &RuntimeConfig) -> Option<String> {
    runtime_config
        .get(OFFLINE_DOWNLOAD_TEMP_DIR_KEY)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn offline_download_aria2_request_timeout_secs(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS_KEY,
        DEFAULT_OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS,
    )
}

pub fn offline_download_aria2_split(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        OFFLINE_DOWNLOAD_ARIA2_SPLIT_KEY,
        DEFAULT_OFFLINE_DOWNLOAD_ARIA2_SPLIT,
    )
}

pub fn offline_download_aria2_max_connection_per_server(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        OFFLINE_DOWNLOAD_ARIA2_MAX_CONNECTION_PER_SERVER_KEY,
        DEFAULT_OFFLINE_DOWNLOAD_ARIA2_MAX_CONNECTION_PER_SERVER,
    )
}

pub fn offline_download_aria2_lowest_speed_limit_bytes_per_sec(
    runtime_config: &RuntimeConfig,
) -> Option<u64> {
    let value = read_non_negative_u64(
        runtime_config,
        OFFLINE_DOWNLOAD_ARIA2_LOWEST_SPEED_LIMIT_BYTES_PER_SEC_KEY,
        DEFAULT_OFFLINE_DOWNLOAD_ARIA2_LOWEST_SPEED_LIMIT_BYTES_PER_SEC,
    );
    (value > 0).then_some(value)
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

pub fn archive_preview_enabled(runtime_config: &RuntimeConfig) -> bool {
    read_bool(
        runtime_config,
        ARCHIVE_PREVIEW_ENABLED_KEY,
        DEFAULT_ARCHIVE_PREVIEW_ENABLED,
    )
}

pub fn archive_preview_user_enabled(runtime_config: &RuntimeConfig) -> bool {
    read_bool(
        runtime_config,
        ARCHIVE_PREVIEW_USER_ENABLED_KEY,
        DEFAULT_ARCHIVE_PREVIEW_USER_ENABLED,
    )
}

pub fn archive_preview_share_enabled(runtime_config: &RuntimeConfig) -> bool {
    read_bool(
        runtime_config,
        ARCHIVE_PREVIEW_SHARE_ENABLED_KEY,
        DEFAULT_ARCHIVE_PREVIEW_SHARE_ENABLED,
    )
}

pub fn archive_preview_max_source_bytes(runtime_config: &RuntimeConfig) -> i64 {
    read_positive_i64_bytes(
        runtime_config,
        ARCHIVE_PREVIEW_MAX_SOURCE_BYTES_KEY,
        DEFAULT_ARCHIVE_PREVIEW_MAX_SOURCE_BYTES,
        "archive preview source size config exceeds i64; using default",
    )
}

pub fn archive_preview_max_entries(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        ARCHIVE_PREVIEW_MAX_ENTRIES_KEY,
        DEFAULT_ARCHIVE_PREVIEW_MAX_ENTRIES,
    )
}

pub fn archive_preview_max_manifest_bytes(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        ARCHIVE_PREVIEW_MAX_MANIFEST_BYTES_KEY,
        DEFAULT_ARCHIVE_PREVIEW_MAX_MANIFEST_BYTES,
    )
}

pub fn archive_preview_max_duration_secs(runtime_config: &RuntimeConfig) -> u64 {
    read_positive_u64(
        runtime_config,
        ARCHIVE_PREVIEW_MAX_DURATION_SECS_KEY,
        DEFAULT_ARCHIVE_PREVIEW_MAX_DURATION_SECS,
    )
}

pub fn archive_compress_enabled(runtime_config: &RuntimeConfig) -> bool {
    read_bool(
        runtime_config,
        ARCHIVE_COMPRESS_ENABLED_KEY,
        DEFAULT_ARCHIVE_COMPRESS_ENABLED,
    )
}

pub fn archive_download_user_enabled(runtime_config: &RuntimeConfig) -> bool {
    read_bool(runtime_config, ARCHIVE_DOWNLOAD_USER_ENABLED_KEY, true)
}

pub fn archive_download_share_enabled(runtime_config: &RuntimeConfig) -> bool {
    read_bool(runtime_config, ARCHIVE_DOWNLOAD_SHARE_ENABLED_KEY, true)
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

fn parse_non_negative_u64(value: &str) -> Option<u64> {
    value.trim().parse::<u64>().ok()
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

fn read_non_negative_u64(runtime_config: &RuntimeConfig, key: &str, default: u64) -> u64 {
    match runtime_config.get(key) {
        Some(raw) => match parse_non_negative_u64(&raw) {
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

fn read_bool(runtime_config: &RuntimeConfig, key: &str, default: bool) -> bool {
    match runtime_config.get(key) {
        Some(raw) => match parse_bool_like(&raw) {
            Some(value) => value,
            None => {
                tracing::warn!(key, value = %raw, "invalid runtime operations boolean config; using default");
                default
            }
        },
        None => default,
    }
}

fn read_concurrency(runtime_config: &RuntimeConfig, key: &str, default: usize) -> usize {
    let default_value = usize_to_u64(default, key).unwrap_or(u64::MAX);
    let value = usize::try_from(read_positive_u64(runtime_config, key, default_value))
        .unwrap_or_else(|_| {
            tracing::warn!(key, "{key} exceeds usize; using default");
            default
        });
    if value > MAX_BACKGROUND_TASK_CONCURRENCY {
        tracing::warn!(
            key,
            value,
            max = MAX_BACKGROUND_TASK_CONCURRENCY,
            "runtime concurrency config exceeds maximum; clamping"
        );
    }
    value.min(MAX_BACKGROUND_TASK_CONCURRENCY)
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

fn read_bounded_u32(
    runtime_config: &RuntimeConfig,
    key: &str,
    default: u32,
    min: u32,
    max: u32,
) -> u32 {
    match runtime_config.get(key) {
        Some(raw) => match raw.trim().parse::<u32>() {
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
        ARCHIVE_COMPRESS_ENABLED_KEY, ARCHIVE_DOWNLOAD_SHARE_ENABLED_KEY,
        ARCHIVE_DOWNLOAD_USER_ENABLED_KEY, ARCHIVE_EXTRACT_MAX_STAGING_BYTES_KEY,
        AVATAR_MAX_UPLOAD_SIZE_BYTES_KEY, BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY_KEY,
        BACKGROUND_TASK_DISPATCH_IDLE_MAX_INTERVAL_SECS_KEY, BACKGROUND_TASK_MAX_ATTEMPTS_KEY,
        BACKGROUND_TASK_MAX_CONCURRENCY_KEY, BACKGROUND_TASK_STORAGE_MIGRATION_MAX_CONCURRENCY_KEY,
        BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY_KEY, BLOB_RECONCILE_INTERVAL_SECS_KEY,
        DEFAULT_ARCHIVE_COMPRESS_ENABLED, DEFAULT_ARCHIVE_EXTRACT_MAX_STAGING_BYTES,
        DEFAULT_AVATAR_MAX_UPLOAD_SIZE_BYTES, DEFAULT_BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY,
        DEFAULT_BACKGROUND_TASK_DISPATCH_IDLE_MAX_INTERVAL_SECS,
        DEFAULT_BACKGROUND_TASK_MAX_ATTEMPTS, DEFAULT_BACKGROUND_TASK_MAX_CONCURRENCY,
        DEFAULT_BACKGROUND_TASK_STORAGE_MIGRATION_MAX_CONCURRENCY,
        DEFAULT_BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY, DEFAULT_BLOB_RECONCILE_INTERVAL_SECS,
        DEFAULT_IMAGE_PREVIEW_MAX_DIMENSION, DEFAULT_OFFLINE_DOWNLOAD_MAX_CONCURRENCY,
        DEFAULT_OFFLINE_DOWNLOAD_MAX_MB_PER_SEC, DEFAULT_REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS,
        DEFAULT_SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY, DEFAULT_SHARE_STREAM_SESSION_TTL_SECS,
        DEFAULT_TASK_LIST_MAX_LIMIT, DEFAULT_TEAM_MEMBER_LIST_MAX_LIMIT,
        DEFAULT_THUMBNAIL_MAX_DIMENSION, FRONTEND_IMAGE_PREVIEW_PREFERENCE_KEY,
        IMAGE_PREVIEW_MAX_DIMENSION_KEY, MAX_BACKGROUND_TASK_CONCURRENCY,
        MAX_DERIVATIVE_MAX_DIMENSION, MAX_SHARE_STREAM_SESSION_TTL_SECS,
        MIN_SHARE_STREAM_SESSION_TTL_SECS, OFFLINE_DOWNLOAD_ENGINE_KEY,
        OFFLINE_DOWNLOAD_ENGINE_REGISTRY_JSON_KEY, OFFLINE_DOWNLOAD_MAX_CONCURRENCY_KEY,
        OFFLINE_DOWNLOAD_MAX_MB_PER_SEC_KEY, OFFLINE_DOWNLOAD_TEMP_DIR_KEY,
        REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS_KEY, SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY_KEY,
        SHARE_STREAM_SESSION_TTL_SECS_KEY, TASK_LIST_MAX_LIMIT_KEY, TEAM_MEMBER_LIST_MAX_LIMIT_KEY,
        THUMBNAIL_MAX_DIMENSION_KEY, archive_compress_enabled, archive_download_share_enabled,
        archive_download_user_enabled, archive_extract_max_staging_bytes,
        avatar_max_upload_size_bytes, background_task_archive_max_concurrency,
        background_task_dispatch_idle_max_interval_secs, background_task_max_attempts,
        background_task_max_concurrency, background_task_storage_migration_max_concurrency,
        background_task_thumbnail_max_concurrency, blob_reconcile_interval_secs,
        frontend_image_preview_preference, image_preview_max_dimension,
        normalize_attempts_config_value, normalize_bool_config_value, normalize_bytes_config_value,
        normalize_concurrency_config_value, normalize_derivative_dimension_config_value,
        normalize_frontend_image_preview_preference_config_value, normalize_interval_config_value,
        normalize_list_max_limit_config_value, normalize_non_negative_u64_config_value,
        normalize_offline_download_temp_dir_config_value, normalize_queue_capacity_config_value,
        normalize_share_stream_session_ttl_config_value, offline_download_enabled_engines,
        offline_download_max_bytes_per_sec, offline_download_max_concurrency,
        offline_download_temp_dir, read_concurrency, remote_node_health_test_interval_secs,
        share_download_rollback_queue_capacity, share_stream_session_ttl_secs, task_list_max_limit,
        team_member_list_max_limit, thumbnail_max_dimension,
    };
    use crate::config::RuntimeConfig;
    use crate::config::definitions::{ALL_CONFIGS, CONFIG_CATEGORY_RUNTIME_MAINTENANCE};
    use aster_forge_db::system_config;
    use chrono::Utc;

    fn config_model(key: &str, value: &str) -> system_config::Model {
        let category = ALL_CONFIGS
            .iter()
            .find(|def| def.key == key)
            .map(|def| def.category)
            .unwrap_or(CONFIG_CATEGORY_RUNTIME_MAINTENANCE);

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
            category: category.to_string(),
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
    fn frontend_image_preview_preference_reader_and_normalizer_are_strict() {
        let runtime_config = RuntimeConfig::new();
        assert_eq!(
            frontend_image_preview_preference(&runtime_config).as_str(),
            "original_first"
        );

        runtime_config.apply(config_model(
            FRONTEND_IMAGE_PREVIEW_PREFERENCE_KEY,
            "preview_first",
        ));
        assert_eq!(
            frontend_image_preview_preference(&runtime_config).as_str(),
            "preview_first"
        );

        runtime_config.apply(config_model(
            FRONTEND_IMAGE_PREVIEW_PREFERENCE_KEY,
            "sideways",
        ));
        assert_eq!(
            frontend_image_preview_preference(&runtime_config).as_str(),
            "original_first"
        );

        assert_eq!(
            normalize_frontend_image_preview_preference_config_value(" preview_first ").unwrap(),
            "preview_first"
        );
        assert!(normalize_frontend_image_preview_preference_config_value("sideways").is_err());
    }

    #[test]
    fn derivative_dimension_readers_use_runtime_value_and_default() {
        let runtime_config = RuntimeConfig::new();
        assert_eq!(
            thumbnail_max_dimension(&runtime_config),
            DEFAULT_THUMBNAIL_MAX_DIMENSION
        );
        assert_eq!(
            image_preview_max_dimension(&runtime_config),
            DEFAULT_IMAGE_PREVIEW_MAX_DIMENSION
        );

        runtime_config.apply(config_model(THUMBNAIL_MAX_DIMENSION_KEY, "320"));
        runtime_config.apply(config_model(IMAGE_PREVIEW_MAX_DIMENSION_KEY, "2048"));
        assert_eq!(thumbnail_max_dimension(&runtime_config), 320);
        assert_eq!(image_preview_max_dimension(&runtime_config), 2048);

        runtime_config.apply(config_model(THUMBNAIL_MAX_DIMENSION_KEY, "1"));
        runtime_config.apply(config_model(
            IMAGE_PREVIEW_MAX_DIMENSION_KEY,
            &MAX_DERIVATIVE_MAX_DIMENSION.to_string(),
        ));
        assert_eq!(thumbnail_max_dimension(&runtime_config), 1);
        assert_eq!(
            image_preview_max_dimension(&runtime_config),
            MAX_DERIVATIVE_MAX_DIMENSION
        );

        runtime_config.apply(config_model(THUMBNAIL_MAX_DIMENSION_KEY, "0"));
        runtime_config.apply(config_model(
            IMAGE_PREVIEW_MAX_DIMENSION_KEY,
            &(u64::from(MAX_DERIVATIVE_MAX_DIMENSION) + 1).to_string(),
        ));
        assert_eq!(
            thumbnail_max_dimension(&runtime_config),
            DEFAULT_THUMBNAIL_MAX_DIMENSION
        );
        assert_eq!(
            image_preview_max_dimension(&runtime_config),
            DEFAULT_IMAGE_PREVIEW_MAX_DIMENSION
        );

        runtime_config.apply(config_model(THUMBNAIL_MAX_DIMENSION_KEY, "abc"));
        runtime_config.apply(config_model(IMAGE_PREVIEW_MAX_DIMENSION_KEY, "-1"));
        assert_eq!(
            thumbnail_max_dimension(&runtime_config),
            DEFAULT_THUMBNAIL_MAX_DIMENSION
        );
        assert_eq!(
            image_preview_max_dimension(&runtime_config),
            DEFAULT_IMAGE_PREVIEW_MAX_DIMENSION
        );
    }

    #[test]
    fn derivative_dimension_normalizer_requires_positive_bounded_integer() {
        assert_eq!(
            normalize_derivative_dimension_config_value(THUMBNAIL_MAX_DIMENSION_KEY, "1").unwrap(),
            "1"
        );
        assert_eq!(
            normalize_derivative_dimension_config_value(THUMBNAIL_MAX_DIMENSION_KEY, " 320 ")
                .unwrap(),
            "320"
        );
        assert_eq!(
            normalize_derivative_dimension_config_value(
                IMAGE_PREVIEW_MAX_DIMENSION_KEY,
                &MAX_DERIVATIVE_MAX_DIMENSION.to_string(),
            )
            .unwrap(),
            MAX_DERIVATIVE_MAX_DIMENSION.to_string()
        );
        for invalid in ["0", "-1", "12.5", "abc", ""] {
            assert!(
                normalize_derivative_dimension_config_value(THUMBNAIL_MAX_DIMENSION_KEY, invalid)
                    .is_err(),
                "{invalid:?} should be rejected"
            );
        }
        assert!(
            normalize_derivative_dimension_config_value(
                IMAGE_PREVIEW_MAX_DIMENSION_KEY,
                &(u64::from(MAX_DERIVATIVE_MAX_DIMENSION) + 1).to_string(),
            )
            .is_err()
        );
    }

    #[test]
    fn background_task_dispatch_idle_max_interval_reader_uses_runtime_value_and_default() {
        let runtime_config = RuntimeConfig::new();
        assert_eq!(
            background_task_dispatch_idle_max_interval_secs(&runtime_config),
            DEFAULT_BACKGROUND_TASK_DISPATCH_IDLE_MAX_INTERVAL_SECS
        );

        runtime_config.apply(config_model(
            BACKGROUND_TASK_DISPATCH_IDLE_MAX_INTERVAL_SECS_KEY,
            "45",
        ));
        assert_eq!(
            background_task_dispatch_idle_max_interval_secs(&runtime_config),
            45
        );

        runtime_config.apply(config_model(
            BACKGROUND_TASK_DISPATCH_IDLE_MAX_INTERVAL_SECS_KEY,
            "0",
        ));
        assert_eq!(
            background_task_dispatch_idle_max_interval_secs(&runtime_config),
            DEFAULT_BACKGROUND_TASK_DISPATCH_IDLE_MAX_INTERVAL_SECS
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
        assert_eq!(
            offline_download_max_concurrency(&runtime_config),
            DEFAULT_OFFLINE_DOWNLOAD_MAX_CONCURRENCY
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
        runtime_config.apply(config_model(OFFLINE_DOWNLOAD_MAX_CONCURRENCY_KEY, "5"));
        assert_eq!(background_task_max_concurrency(&runtime_config), 3usize);
        assert_eq!(
            background_task_archive_max_concurrency(&runtime_config),
            2usize
        );
        assert_eq!(
            background_task_thumbnail_max_concurrency(&runtime_config),
            4usize
        );
        assert_eq!(offline_download_max_concurrency(&runtime_config), 5usize);

        runtime_config.apply(config_model(BACKGROUND_TASK_MAX_CONCURRENCY_KEY, "0"));
        runtime_config.apply(config_model(
            BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY_KEY,
            "0",
        ));
        runtime_config.apply(config_model(
            BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY_KEY,
            "0",
        ));
        runtime_config.apply(config_model(OFFLINE_DOWNLOAD_MAX_CONCURRENCY_KEY, "0"));
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
        assert_eq!(
            offline_download_max_concurrency(&runtime_config),
            DEFAULT_OFFLINE_DOWNLOAD_MAX_CONCURRENCY
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
        runtime_config.apply(config_model(OFFLINE_DOWNLOAD_MAX_CONCURRENCY_KEY, "abc"));
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
        assert_eq!(
            offline_download_max_concurrency(&runtime_config),
            DEFAULT_OFFLINE_DOWNLOAD_MAX_CONCURRENCY
        );

        runtime_config.apply(config_model(
            BACKGROUND_TASK_MAX_CONCURRENCY_KEY,
            &(MAX_BACKGROUND_TASK_CONCURRENCY + 1).to_string(),
        ));
        runtime_config.apply(config_model(
            BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY_KEY,
            "999",
        ));
        runtime_config.apply(config_model(
            BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY_KEY,
            "18446744073709551615",
        ));
        runtime_config.apply(config_model(OFFLINE_DOWNLOAD_MAX_CONCURRENCY_KEY, "100"));
        assert_eq!(
            background_task_max_concurrency(&runtime_config),
            MAX_BACKGROUND_TASK_CONCURRENCY
        );
        assert_eq!(
            background_task_archive_max_concurrency(&runtime_config),
            MAX_BACKGROUND_TASK_CONCURRENCY
        );
        assert_eq!(
            background_task_thumbnail_max_concurrency(&runtime_config),
            MAX_BACKGROUND_TASK_CONCURRENCY
        );
        assert_eq!(
            offline_download_max_concurrency(&runtime_config),
            MAX_BACKGROUND_TASK_CONCURRENCY
        );
    }

    #[test]
    fn read_concurrency_applies_min_max_and_fallback_boundaries() {
        let runtime_config = RuntimeConfig::new();
        assert_eq!(
            read_concurrency(&runtime_config, BACKGROUND_TASK_MAX_CONCURRENCY_KEY, 3),
            3
        );

        runtime_config.apply(config_model(BACKGROUND_TASK_MAX_CONCURRENCY_KEY, "1"));
        assert_eq!(
            read_concurrency(&runtime_config, BACKGROUND_TASK_MAX_CONCURRENCY_KEY, 3),
            1
        );

        runtime_config.apply(config_model(
            BACKGROUND_TASK_MAX_CONCURRENCY_KEY,
            &MAX_BACKGROUND_TASK_CONCURRENCY.to_string(),
        ));
        assert_eq!(
            read_concurrency(&runtime_config, BACKGROUND_TASK_MAX_CONCURRENCY_KEY, 3),
            MAX_BACKGROUND_TASK_CONCURRENCY
        );

        runtime_config.apply(config_model(
            BACKGROUND_TASK_MAX_CONCURRENCY_KEY,
            &(MAX_BACKGROUND_TASK_CONCURRENCY + 1).to_string(),
        ));
        assert_eq!(
            read_concurrency(&runtime_config, BACKGROUND_TASK_MAX_CONCURRENCY_KEY, 3),
            MAX_BACKGROUND_TASK_CONCURRENCY
        );

        runtime_config.apply(config_model(BACKGROUND_TASK_MAX_CONCURRENCY_KEY, "0"));
        assert_eq!(
            read_concurrency(&runtime_config, BACKGROUND_TASK_MAX_CONCURRENCY_KEY, 3),
            3
        );

        runtime_config.apply(config_model(BACKGROUND_TASK_MAX_CONCURRENCY_KEY, "abc"));
        assert_eq!(
            read_concurrency(&runtime_config, BACKGROUND_TASK_MAX_CONCURRENCY_KEY, 3),
            3
        );

        runtime_config.apply(config_model(
            BACKGROUND_TASK_MAX_CONCURRENCY_KEY,
            "18446744073709551615",
        ));
        let expected = if usize::try_from(u64::MAX).is_ok() {
            MAX_BACKGROUND_TASK_CONCURRENCY
        } else {
            3
        };
        assert_eq!(
            read_concurrency(&runtime_config, BACKGROUND_TASK_MAX_CONCURRENCY_KEY, 3),
            expected
        );
    }

    #[test]
    fn offline_download_speed_reader_converts_mb_per_sec_to_bytes_per_sec() {
        let runtime_config = RuntimeConfig::new();
        assert_eq!(DEFAULT_OFFLINE_DOWNLOAD_MAX_MB_PER_SEC, 5);
        assert_eq!(
            offline_download_max_bytes_per_sec(&runtime_config),
            if DEFAULT_OFFLINE_DOWNLOAD_MAX_MB_PER_SEC == 0 {
                None
            } else {
                DEFAULT_OFFLINE_DOWNLOAD_MAX_MB_PER_SEC.checked_mul(1024 * 1024)
            }
        );

        runtime_config.apply(config_model(OFFLINE_DOWNLOAD_MAX_MB_PER_SEC_KEY, "0"));
        assert_eq!(offline_download_max_bytes_per_sec(&runtime_config), None);

        runtime_config.apply(config_model(OFFLINE_DOWNLOAD_MAX_MB_PER_SEC_KEY, "12"));
        assert_eq!(
            offline_download_max_bytes_per_sec(&runtime_config),
            Some(12 * 1024 * 1024)
        );

        runtime_config.apply(config_model(OFFLINE_DOWNLOAD_MAX_MB_PER_SEC_KEY, "invalid"));
        assert_eq!(
            offline_download_max_bytes_per_sec(&runtime_config),
            if DEFAULT_OFFLINE_DOWNLOAD_MAX_MB_PER_SEC == 0 {
                None
            } else {
                DEFAULT_OFFLINE_DOWNLOAD_MAX_MB_PER_SEC.checked_mul(1024 * 1024)
            }
        );
    }

    #[test]
    fn storage_migration_concurrency_reader_uses_runtime_value_and_default() {
        let runtime_config = RuntimeConfig::new();
        assert_eq!(
            background_task_storage_migration_max_concurrency(&runtime_config),
            DEFAULT_BACKGROUND_TASK_STORAGE_MIGRATION_MAX_CONCURRENCY
        );

        runtime_config.apply(config_model(
            BACKGROUND_TASK_STORAGE_MIGRATION_MAX_CONCURRENCY_KEY,
            "3",
        ));
        assert_eq!(
            background_task_storage_migration_max_concurrency(&runtime_config),
            3usize
        );

        runtime_config.apply(config_model(
            BACKGROUND_TASK_STORAGE_MIGRATION_MAX_CONCURRENCY_KEY,
            "invalid",
        ));
        assert_eq!(
            background_task_storage_migration_max_concurrency(&runtime_config),
            DEFAULT_BACKGROUND_TASK_STORAGE_MIGRATION_MAX_CONCURRENCY
        );
    }

    #[test]
    fn non_negative_u64_normalizer_accepts_zero() {
        assert_eq!(
            normalize_non_negative_u64_config_value(OFFLINE_DOWNLOAD_MAX_MB_PER_SEC_KEY, "0")
                .unwrap(),
            "0"
        );
        assert_eq!(
            normalize_non_negative_u64_config_value(OFFLINE_DOWNLOAD_MAX_MB_PER_SEC_KEY, "25")
                .unwrap(),
            "25"
        );
        assert!(
            normalize_non_negative_u64_config_value(OFFLINE_DOWNLOAD_MAX_MB_PER_SEC_KEY, "-1")
                .is_err()
        );
        assert!(
            normalize_non_negative_u64_config_value(OFFLINE_DOWNLOAD_MAX_MB_PER_SEC_KEY, "1.5")
                .is_err()
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
    fn share_stream_session_ttl_reader_uses_bounded_runtime_value_and_default() {
        let runtime_config = RuntimeConfig::new();
        assert_eq!(
            share_stream_session_ttl_secs(&runtime_config),
            DEFAULT_SHARE_STREAM_SESSION_TTL_SECS
        );

        runtime_config.apply(config_model(
            SHARE_STREAM_SESSION_TTL_SECS_KEY,
            &MIN_SHARE_STREAM_SESSION_TTL_SECS.to_string(),
        ));
        assert_eq!(
            share_stream_session_ttl_secs(&runtime_config),
            MIN_SHARE_STREAM_SESSION_TTL_SECS
        );

        runtime_config.apply(config_model(
            SHARE_STREAM_SESSION_TTL_SECS_KEY,
            &MAX_SHARE_STREAM_SESSION_TTL_SECS.to_string(),
        ));
        assert_eq!(
            share_stream_session_ttl_secs(&runtime_config),
            MAX_SHARE_STREAM_SESSION_TTL_SECS
        );

        runtime_config.apply(config_model(SHARE_STREAM_SESSION_TTL_SECS_KEY, "299"));
        assert_eq!(
            share_stream_session_ttl_secs(&runtime_config),
            DEFAULT_SHARE_STREAM_SESSION_TTL_SECS
        );

        runtime_config.apply(config_model(SHARE_STREAM_SESSION_TTL_SECS_KEY, "86401"));
        assert_eq!(
            share_stream_session_ttl_secs(&runtime_config),
            DEFAULT_SHARE_STREAM_SESSION_TTL_SECS
        );

        runtime_config.apply(config_model(SHARE_STREAM_SESSION_TTL_SECS_KEY, "invalid"));
        assert_eq!(
            share_stream_session_ttl_secs(&runtime_config),
            DEFAULT_SHARE_STREAM_SESSION_TTL_SECS
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
            normalize_concurrency_config_value(
                "test_concurrency",
                &MAX_BACKGROUND_TASK_CONCURRENCY.to_string()
            )
            .unwrap(),
            MAX_BACKGROUND_TASK_CONCURRENCY.to_string()
        );
        assert!(
            normalize_concurrency_config_value(
                "test_concurrency",
                &(MAX_BACKGROUND_TASK_CONCURRENCY + 1).to_string()
            )
            .is_err()
        );
        assert_eq!(
            normalize_queue_capacity_config_value(
                SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY_KEY,
                &DEFAULT_SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY.to_string()
            )
            .unwrap(),
            DEFAULT_SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY.to_string()
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
        assert_eq!(
            normalize_share_stream_session_ttl_config_value(
                SHARE_STREAM_SESSION_TTL_SECS_KEY,
                &MIN_SHARE_STREAM_SESSION_TTL_SECS.to_string(),
            )
            .unwrap(),
            MIN_SHARE_STREAM_SESSION_TTL_SECS.to_string()
        );
        assert_eq!(
            normalize_share_stream_session_ttl_config_value(
                SHARE_STREAM_SESSION_TTL_SECS_KEY,
                &MAX_SHARE_STREAM_SESSION_TTL_SECS.to_string(),
            )
            .unwrap(),
            MAX_SHARE_STREAM_SESSION_TTL_SECS.to_string()
        );
        assert!(normalize_interval_config_value("test_interval", "0").is_err());
        assert!(normalize_concurrency_config_value("test_concurrency", "0").is_err());
        assert!(
            normalize_queue_capacity_config_value(SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY_KEY, "0")
                .is_err()
        );
        assert!(normalize_attempts_config_value("test_attempts", "0").is_err());
        assert!(normalize_bytes_config_value("test_bytes", "-1").is_err());
        assert!(normalize_list_max_limit_config_value("test_limit", "1001").is_err());
        assert_eq!(
            normalize_bool_config_value("test_bool", " yes ").unwrap(),
            "true"
        );
        assert_eq!(
            normalize_bool_config_value("test_bool", " off ").unwrap(),
            "false"
        );
        assert!(normalize_bool_config_value("test_bool", "sometimes").is_err());
        assert!(
            normalize_share_stream_session_ttl_config_value(SHARE_STREAM_SESSION_TTL_SECS_KEY, "0")
                .is_err()
        );
        assert!(
            normalize_share_stream_session_ttl_config_value(
                SHARE_STREAM_SESSION_TTL_SECS_KEY,
                &(MIN_SHARE_STREAM_SESSION_TTL_SECS - 1).to_string(),
            )
            .is_err()
        );
        assert!(
            normalize_share_stream_session_ttl_config_value(
                SHARE_STREAM_SESSION_TTL_SECS_KEY,
                &(MAX_SHARE_STREAM_SESSION_TTL_SECS + 1).to_string(),
            )
            .is_err()
        );
    }

    #[test]
    fn archive_compress_enabled_reader_defaults_enabled_and_handles_invalid_values() {
        let runtime_config = RuntimeConfig::new();
        assert_eq!(
            archive_compress_enabled(&runtime_config),
            DEFAULT_ARCHIVE_COMPRESS_ENABLED
        );

        runtime_config.apply(config_model(ARCHIVE_COMPRESS_ENABLED_KEY, "false"));
        assert!(!archive_compress_enabled(&runtime_config));

        runtime_config.apply(config_model(ARCHIVE_COMPRESS_ENABLED_KEY, "true"));
        assert!(archive_compress_enabled(&runtime_config));

        runtime_config.apply(config_model(ARCHIVE_COMPRESS_ENABLED_KEY, "invalid"));
        assert_eq!(
            archive_compress_enabled(&runtime_config),
            DEFAULT_ARCHIVE_COMPRESS_ENABLED
        );
    }

    #[test]
    fn archive_download_enabled_readers_default_enabled_and_handle_invalid_values() {
        let runtime_config = RuntimeConfig::new();
        assert!(archive_download_user_enabled(&runtime_config));
        assert!(archive_download_share_enabled(&runtime_config));

        runtime_config.apply(config_model(ARCHIVE_DOWNLOAD_USER_ENABLED_KEY, "false"));
        runtime_config.apply(config_model(ARCHIVE_DOWNLOAD_SHARE_ENABLED_KEY, "off"));
        assert!(!archive_download_user_enabled(&runtime_config));
        assert!(!archive_download_share_enabled(&runtime_config));

        runtime_config.apply(config_model(ARCHIVE_DOWNLOAD_USER_ENABLED_KEY, "true"));
        runtime_config.apply(config_model(ARCHIVE_DOWNLOAD_SHARE_ENABLED_KEY, "on"));
        assert!(archive_download_user_enabled(&runtime_config));
        assert!(archive_download_share_enabled(&runtime_config));

        runtime_config.apply(config_model(ARCHIVE_DOWNLOAD_USER_ENABLED_KEY, "invalid"));
        runtime_config.apply(config_model(ARCHIVE_DOWNLOAD_SHARE_ENABLED_KEY, "invalid"));
        assert!(archive_download_user_enabled(&runtime_config));
        assert!(archive_download_share_enabled(&runtime_config));
    }

    #[test]
    fn offline_download_temp_dir_normalizes_absolute_path_or_blank() {
        assert_eq!(
            normalize_offline_download_temp_dir_config_value("   ").unwrap(),
            ""
        );
        assert_eq!(
            normalize_offline_download_temp_dir_config_value(" /tmp/asterdrive-offline ").unwrap(),
            "/tmp/asterdrive-offline"
        );
        assert!(normalize_offline_download_temp_dir_config_value("data/offline").is_err());
        assert!(normalize_offline_download_temp_dir_config_value("/tmp/offline\nbad").is_err());
        assert!(
            normalize_offline_download_temp_dir_config_value(&format!("/{}", "a".repeat(4095)))
                .is_ok()
        );
        assert!(
            normalize_offline_download_temp_dir_config_value(&format!("/{}", "a".repeat(4096)))
                .is_err()
        );

        let runtime_config = RuntimeConfig::new();
        assert_eq!(offline_download_temp_dir(&runtime_config), None);
        runtime_config.apply(config_model(
            OFFLINE_DOWNLOAD_TEMP_DIR_KEY,
            " /tmp/asterdrive-offline ",
        ));
        assert_eq!(
            offline_download_temp_dir(&runtime_config),
            Some("/tmp/asterdrive-offline".to_string())
        );
    }

    #[test]
    fn offline_download_enabled_engines_reads_registry_order_and_all_disabled() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(
            OFFLINE_DOWNLOAD_ENGINE_REGISTRY_JSON_KEY,
            r#"{"version":1,"engines":[{"kind":"aria2","enabled":true},{"kind":"builtin","enabled":true}]}"#,
        ));
        assert_eq!(
            offline_download_enabled_engines(&runtime_config),
            vec![
                super::OfflineDownloadEngine::Aria2,
                super::OfflineDownloadEngine::Builtin
            ]
        );

        runtime_config.apply(config_model(
            OFFLINE_DOWNLOAD_ENGINE_REGISTRY_JSON_KEY,
            r#"{"version":1,"engines":[{"kind":"aria2","enabled":false},{"kind":"builtin","enabled":false}]}"#,
        ));
        assert!(offline_download_enabled_engines(&runtime_config).is_empty());
    }

    #[test]
    fn offline_download_enabled_engines_falls_back_to_legacy_engine_on_invalid_registry() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(OFFLINE_DOWNLOAD_ENGINE_KEY, "aria2"));
        runtime_config.apply(config_model(
            OFFLINE_DOWNLOAD_ENGINE_REGISTRY_JSON_KEY,
            r#"{"version":999,"engines":[]}"#,
        ));

        assert_eq!(
            offline_download_enabled_engines(&runtime_config),
            vec![super::OfflineDownloadEngine::Aria2]
        );
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
