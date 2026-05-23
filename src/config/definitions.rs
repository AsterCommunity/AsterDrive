//! 运行时配置定义 — 所有 system_config 键的单一数据源
//!
//! 启动时默认配置初始化流程遍历此数组，
//! 对每项执行 INSERT ... ON CONFLICT DO NOTHING。
//!
//! 所有 `system_config` 键字符串在此处以 `pub const` 形式声明，
//! 子模块通过 `pub use crate::config::definitions::*_KEY` 重导出，
//! 不再各自定义本地 `const`，确保单一数据源。

use crate::types::SystemConfigValueType;

// ── Auth keys ────────────────────────────────────────────────────────────────
pub const AUTH_COOKIE_SECURE_KEY: &str = "auth_cookie_secure";
pub const AUTH_ACCESS_TOKEN_TTL_SECS_KEY: &str = "auth_access_token_ttl_secs";
pub const AUTH_REFRESH_TOKEN_TTL_SECS_KEY: &str = "auth_refresh_token_ttl_secs";
pub const AUTH_REGISTER_ACTIVATION_TTL_SECS_KEY: &str = "auth_register_activation_ttl_secs";
pub const AUTH_CONTACT_CHANGE_TTL_SECS_KEY: &str = "auth_contact_change_ttl_secs";
pub const AUTH_PASSWORD_RESET_TTL_SECS_KEY: &str = "auth_password_reset_ttl_secs";
pub const AUTH_CONTACT_VERIFICATION_RESEND_COOLDOWN_SECS_KEY: &str =
    "auth_contact_verification_resend_cooldown_secs";
pub const AUTH_PASSWORD_RESET_REQUEST_COOLDOWN_SECS_KEY: &str =
    "auth_password_reset_request_cooldown_secs";
pub const AUTH_ALLOW_USER_REGISTRATION_KEY: &str = "auth_allow_user_registration";
pub const AUTH_REGISTER_ACTIVATION_ENABLED_KEY: &str = "auth_register_activation_enabled";

// ── CORS keys ────────────────────────────────────────────────────────────────
pub const CORS_ENABLED_KEY: &str = "cors_enabled";
pub const CORS_ALLOWED_ORIGINS_KEY: &str = "cors_allowed_origins";
pub const CORS_ALLOW_CREDENTIALS_KEY: &str = "cors_allow_credentials";
pub const CORS_MAX_AGE_SECS_KEY: &str = "cors_max_age_secs";

// ── Operations keys ──────────────────────────────────────────────────────────
pub const MAIL_OUTBOX_DISPATCH_INTERVAL_SECS_KEY: &str = "mail_outbox_dispatch_interval_secs";
pub const BACKGROUND_TASK_DISPATCH_INTERVAL_SECS_KEY: &str =
    "background_task_dispatch_interval_secs";
pub const BACKGROUND_TASK_DISPATCH_IDLE_MAX_INTERVAL_SECS_KEY: &str =
    "background_task_dispatch_idle_max_interval_secs";
pub const BACKGROUND_TASK_MAX_CONCURRENCY_KEY: &str = "background_task_max_concurrency";
pub const BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY_KEY: &str =
    "background_task_archive_max_concurrency";
pub const BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY_KEY: &str =
    "background_task_thumbnail_max_concurrency";
pub const BACKGROUND_TASK_MAX_ATTEMPTS_KEY: &str = "background_task_max_attempts";
pub const SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY_KEY: &str =
    "share_download_rollback_queue_capacity";
pub const SHARE_STREAM_SESSION_TTL_SECS_KEY: &str = "share_stream_session_ttl_secs";
pub const MAINTENANCE_CLEANUP_INTERVAL_SECS_KEY: &str = "maintenance_cleanup_interval_secs";
pub const BLOB_RECONCILE_INTERVAL_SECS_KEY: &str = "blob_reconcile_interval_secs";
pub const REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS_KEY: &str = "remote_node_health_test_interval_secs";
pub const TEAM_MEMBER_LIST_MAX_LIMIT_KEY: &str = "team_member_list_max_limit";
pub const TASK_LIST_MAX_LIMIT_KEY: &str = "task_list_max_limit";
pub const AVATAR_MAX_UPLOAD_SIZE_BYTES_KEY: &str = "avatar_max_upload_size_bytes";
pub const THUMBNAIL_MAX_SOURCE_BYTES_KEY: &str = "thumbnail_max_source_bytes";
pub const MEDIA_METADATA_ENABLED_KEY: &str = "media_metadata_enabled";
pub const MEDIA_METADATA_MAX_SOURCE_BYTES_KEY: &str = "media_metadata_max_source_bytes";
pub const MEDIA_PROCESSING_REGISTRY_JSON_KEY: &str = "media_processing_registry_json";
pub const THUMBNAIL_DEFAULT_PROCESSOR_KEY: &str = "thumbnail_default_processor";
pub const THUMBNAIL_VIPS_CLI_ENABLED_KEY: &str = "thumbnail_vips_cli_enabled";
pub const THUMBNAIL_VIPS_COMMAND_KEY: &str = "thumbnail_vips_command";

// ── Storage keys ─────────────────────────────────────────────────────────────
pub const MAX_VERSIONS_PER_FILE_KEY: &str = "max_versions_per_file";
pub const TRASH_RETENTION_DAYS_KEY: &str = "trash_retention_days";
pub const TEAM_ARCHIVE_RETENTION_DAYS_KEY: &str = "team_archive_retention_days";
pub const TASK_RETENTION_HOURS_KEY: &str = "task_retention_hours";
pub const DEFAULT_STORAGE_QUOTA_KEY: &str = "default_storage_quota";
pub const ARCHIVE_EXTRACT_MAX_SOURCE_BYTES_KEY: &str = "archive_extract_max_source_bytes";
pub const ARCHIVE_EXTRACT_MAX_STAGING_BYTES_KEY: &str = "archive_extract_max_staging_bytes";
pub const ARCHIVE_EXTRACT_MAX_UNCOMPRESSED_BYTES_KEY: &str =
    "archive_extract_max_uncompressed_bytes";
pub const ARCHIVE_EXTRACT_MAX_ENTRIES_KEY: &str = "archive_extract_max_entries";
pub const ARCHIVE_EXTRACT_MAX_FILES_KEY: &str = "archive_extract_max_files";
pub const ARCHIVE_EXTRACT_MAX_DIRECTORIES_KEY: &str = "archive_extract_max_directories";
pub const ARCHIVE_EXTRACT_MAX_DEPTH_KEY: &str = "archive_extract_max_depth";
pub const ARCHIVE_EXTRACT_MAX_PATH_BYTES_KEY: &str = "archive_extract_max_path_bytes";
pub const ARCHIVE_EXTRACT_MAX_COMPRESSION_RATIO_KEY: &str = "archive_extract_max_compression_ratio";
pub const ARCHIVE_EXTRACT_MAX_ENTRY_COMPRESSION_RATIO_KEY: &str =
    "archive_extract_max_entry_compression_ratio";
pub const ARCHIVE_EXTRACT_MAX_DURATION_SECS_KEY: &str = "archive_extract_max_duration_secs";
pub const ARCHIVE_PREVIEW_ENABLED_KEY: &str = "archive_preview_enabled";
pub const ARCHIVE_PREVIEW_USER_ENABLED_KEY: &str = "archive_preview_user_enabled";
pub const ARCHIVE_PREVIEW_SHARE_ENABLED_KEY: &str = "archive_preview_share_enabled";
pub const ARCHIVE_PREVIEW_MAX_SOURCE_BYTES_KEY: &str = "archive_preview_max_source_bytes";
pub const ARCHIVE_PREVIEW_MAX_ENTRIES_KEY: &str = "archive_preview_max_entries";
pub const ARCHIVE_PREVIEW_MAX_MANIFEST_BYTES_KEY: &str = "archive_preview_max_manifest_bytes";
pub const ARCHIVE_PREVIEW_MAX_DURATION_SECS_KEY: &str = "archive_preview_max_duration_secs";
pub const ARCHIVE_BUILD_MAX_ENTRIES_KEY: &str = "archive_build_max_entries";
pub const ARCHIVE_BUILD_MAX_TOTAL_SOURCE_BYTES_KEY: &str = "archive_build_max_total_source_bytes";
pub const ARCHIVE_BUILD_MAX_TEMP_BYTES_KEY: &str = "archive_build_max_temp_bytes";

// ── Mail keys ────────────────────────────────────────────────────────────────
pub const MAIL_SMTP_HOST_KEY: &str = "mail_smtp_host";
pub const MAIL_SMTP_PORT_KEY: &str = "mail_smtp_port";
pub const MAIL_SMTP_USERNAME_KEY: &str = "mail_smtp_username";
pub const MAIL_SMTP_PASSWORD_KEY: &str = "mail_smtp_password";
pub const MAIL_FROM_ADDRESS_KEY: &str = "mail_from_address";
pub const MAIL_FROM_NAME_KEY: &str = "mail_from_name";
pub const MAIL_SECURITY_KEY: &str = "mail_security";
pub const MAIL_TEMPLATE_REGISTER_ACTIVATION_SUBJECT_KEY: &str =
    "mail_template_register_activation_subject";
pub const MAIL_TEMPLATE_REGISTER_ACTIVATION_HTML_KEY: &str =
    "mail_template_register_activation_html";
pub const MAIL_TEMPLATE_CONTACT_CHANGE_CONFIRMATION_SUBJECT_KEY: &str =
    "mail_template_contact_change_confirmation_subject";
pub const MAIL_TEMPLATE_CONTACT_CHANGE_CONFIRMATION_HTML_KEY: &str =
    "mail_template_contact_change_confirmation_html";
pub const MAIL_TEMPLATE_PASSWORD_RESET_SUBJECT_KEY: &str = "mail_template_password_reset_subject";
pub const MAIL_TEMPLATE_PASSWORD_RESET_HTML_KEY: &str = "mail_template_password_reset_html";
pub const MAIL_TEMPLATE_PASSWORD_RESET_NOTICE_SUBJECT_KEY: &str =
    "mail_template_password_reset_notice_subject";
pub const MAIL_TEMPLATE_PASSWORD_RESET_NOTICE_HTML_KEY: &str =
    "mail_template_password_reset_notice_html";
pub const MAIL_TEMPLATE_CONTACT_CHANGE_NOTICE_SUBJECT_KEY: &str =
    "mail_template_contact_change_notice_subject";
pub const MAIL_TEMPLATE_CONTACT_CHANGE_NOTICE_HTML_KEY: &str =
    "mail_template_contact_change_notice_html";
pub const MAIL_TEMPLATE_EXTERNAL_AUTH_EMAIL_VERIFICATION_SUBJECT_KEY: &str =
    "mail_template_external_auth_email_verification_subject";
pub const MAIL_TEMPLATE_EXTERNAL_AUTH_EMAIL_VERIFICATION_HTML_KEY: &str =
    "mail_template_external_auth_email_verification_html";

// ── General / branding keys ──────────────────────────────────────────────────
pub const PUBLIC_SITE_URL_KEY: &str = "public_site_url";
pub const BRANDING_TITLE_KEY: &str = "branding_title";
pub const BRANDING_DESCRIPTION_KEY: &str = "branding_description";
pub const BRANDING_FAVICON_URL_KEY: &str = "branding_favicon_url";
pub const BRANDING_WORDMARK_DARK_URL_KEY: &str = "branding_wordmark_dark_url";
pub const BRANDING_WORDMARK_LIGHT_URL_KEY: &str = "branding_wordmark_light_url";

// ── WOPI keys ────────────────────────────────────────────────────────────────
pub const WOPI_ACCESS_TOKEN_TTL_SECS_KEY: &str = "wopi_access_token_ttl_secs";
pub const WOPI_LOCK_TTL_SECS_KEY: &str = "wopi_lock_ttl_secs";
pub const WOPI_DISCOVERY_CACHE_TTL_SECS_KEY: &str = "wopi_discovery_cache_ttl_secs";

// ── Avatar keys ──────────────────────────────────────────────────────────────
pub const AVATAR_DIR_KEY: &str = "avatar_dir";

// ── Audit keys ───────────────────────────────────────────────────────────────
pub const AUDIT_LOG_ENABLED_KEY: &str = "audit_log_enabled";
pub const AUDIT_LOG_RETENTION_DAYS_KEY: &str = "audit_log_retention_days";

// ── WebDAV keys ──────────────────────────────────────────────────────────────
pub const WEBDAV_ENABLED_KEY: &str = "webdav_enabled";
pub const WEBDAV_BLOCK_SYSTEM_FILES_ENABLED_KEY: &str = "webdav_block_system_files_enabled";
pub const WEBDAV_BLOCK_SYSTEM_FILE_PATTERNS_KEY: &str = "webdav_block_system_file_patterns";
pub const DEFAULT_WEBDAV_SYSTEM_FILE_PATTERNS: &[&str] = &[
    ".DS_Store",
    "._*",
    ".Spotlight-V100",
    ".Trashes",
    ".fseventsd",
    "Thumbs.db",
    "desktop.ini",
    "$RECYCLE.BIN",
    "System Volume Information",
];

fn empty_string_array_default() -> String {
    "[]".to_string()
}

fn default_webdav_system_file_patterns() -> String {
    match serde_json::to_string(DEFAULT_WEBDAV_SYSTEM_FILE_PATTERNS) {
        Ok(value) => value,
        Err(_) => "[]".to_string(),
    }
}

/// 单条配置定义
pub struct ConfigDef {
    /// 配置键（数据库 unique key）
    pub key: &'static str,
    /// 前端显示名称的 i18n key
    pub label_i18n_key: &'static str,
    /// 前端描述文案的 i18n key
    pub description_i18n_key: &'static str,
    /// 值类型：前端渲染用
    pub value_type: SystemConfigValueType,
    /// 默认值生成函数
    pub default_fn: fn() -> String,
    /// 修改后是否需要重启
    pub requires_restart: bool,
    /// 是否敏感值
    pub is_sensitive: bool,
    /// 分类（前端分组用）
    pub category: &'static str,
    /// 描述
    pub description: &'static str,
}

/// 所有运行时配置项
pub static ALL_CONFIGS: &[ConfigDef] = &[
    // ── Auth ────────────────────────────────────────────────
    ConfigDef {
        key: AUTH_COOKIE_SECURE_KEY,
        label_i18n_key: "settings_item_auth_cookie_secure_label",
        description_i18n_key: "settings_item_auth_cookie_secure_desc",
        value_type: SystemConfigValueType::Boolean,
        default_fn: || "true".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "auth",
        description: "Whether auth and share verification cookies require HTTPS",
    },
    ConfigDef {
        key: AUTH_ACCESS_TOKEN_TTL_SECS_KEY,
        label_i18n_key: "settings_item_auth_access_token_ttl_secs_label",
        description_i18n_key: "settings_item_auth_access_token_ttl_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || "900".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "auth",
        description: "Access token lifetime in seconds",
    },
    ConfigDef {
        key: AUTH_REFRESH_TOKEN_TTL_SECS_KEY,
        label_i18n_key: "settings_item_auth_refresh_token_ttl_secs_label",
        description_i18n_key: "settings_item_auth_refresh_token_ttl_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || "604800".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "auth",
        description: "Refresh token lifetime in seconds",
    },
    ConfigDef {
        key: AUTH_REGISTER_ACTIVATION_TTL_SECS_KEY,
        label_i18n_key: "settings_item_auth_register_activation_ttl_secs_label",
        description_i18n_key: "settings_item_auth_register_activation_ttl_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || "86400".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "auth",
        description: "Registration activation link lifetime in seconds",
    },
    ConfigDef {
        key: AUTH_CONTACT_CHANGE_TTL_SECS_KEY,
        label_i18n_key: "settings_item_auth_contact_change_ttl_secs_label",
        description_i18n_key: "settings_item_auth_contact_change_ttl_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || "86400".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "auth",
        description: "Contact change confirmation link lifetime in seconds",
    },
    ConfigDef {
        key: AUTH_PASSWORD_RESET_TTL_SECS_KEY,
        label_i18n_key: "settings_item_auth_password_reset_ttl_secs_label",
        description_i18n_key: "settings_item_auth_password_reset_ttl_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || "3600".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "auth",
        description: "Password reset link lifetime in seconds",
    },
    ConfigDef {
        key: AUTH_CONTACT_VERIFICATION_RESEND_COOLDOWN_SECS_KEY,
        label_i18n_key: "settings_item_auth_contact_verification_resend_cooldown_secs_label",
        description_i18n_key: "settings_item_auth_contact_verification_resend_cooldown_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || "60".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "auth",
        description: "Minimum cooldown between verification email resends in seconds",
    },
    ConfigDef {
        key: AUTH_PASSWORD_RESET_REQUEST_COOLDOWN_SECS_KEY,
        label_i18n_key: "settings_item_auth_password_reset_request_cooldown_secs_label",
        description_i18n_key: "settings_item_auth_password_reset_request_cooldown_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || "60".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "auth",
        description: "Minimum cooldown between password reset email requests for the same user in seconds",
    },
    // ── WebDAV ──────────────────────────────────────────────
    ConfigDef {
        key: WEBDAV_ENABLED_KEY,
        label_i18n_key: "settings_item_webdav_enabled_label",
        description_i18n_key: "settings_item_webdav_enabled_desc",
        value_type: SystemConfigValueType::Boolean,
        default_fn: || "true".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "webdav",
        description: "Enable or disable WebDAV access",
    },
    ConfigDef {
        key: WEBDAV_BLOCK_SYSTEM_FILES_ENABLED_KEY,
        label_i18n_key: "settings_item_webdav_block_system_files_enabled_label",
        description_i18n_key: "settings_item_webdav_block_system_files_enabled_desc",
        value_type: SystemConfigValueType::Boolean,
        default_fn: || "true".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "webdav",
        description: "Block WebDAV clients from creating common operating-system metadata files and folders",
    },
    ConfigDef {
        key: WEBDAV_BLOCK_SYSTEM_FILE_PATTERNS_KEY,
        label_i18n_key: "settings_item_webdav_block_system_file_patterns_label",
        description_i18n_key: "settings_item_webdav_block_system_file_patterns_desc",
        value_type: SystemConfigValueType::StringArray,
        default_fn: default_webdav_system_file_patterns,
        requires_restart: false,
        is_sensitive: false,
        category: "webdav",
        description: "WebDAV basename patterns blocked when system-file protection is enabled",
    },
    // ── Network ─────────────────────────────────────────────
    ConfigDef {
        key: CORS_ENABLED_KEY,
        label_i18n_key: "settings_item_cors_enabled_label",
        description_i18n_key: "settings_item_cors_enabled_desc",
        value_type: SystemConfigValueType::Boolean,
        default_fn: || "false".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "network",
        description: "Enable CORS handling for cross-origin browser requests. When disabled, the server skips all CORS headers and enforcement",
    },
    ConfigDef {
        key: CORS_ALLOWED_ORIGINS_KEY,
        label_i18n_key: "settings_item_cors_allowed_origins_label",
        description_i18n_key: "settings_item_cors_allowed_origins_desc",
        value_type: SystemConfigValueType::String,
        default_fn: || String::new(),
        requires_restart: false,
        is_sensitive: false,
        category: "network",
        description: "Comma-separated CORS origin whitelist. Empty = skip CORS headers and let the browser block cross-origin access, '*' = allow any origin",
    },
    ConfigDef {
        key: CORS_ALLOW_CREDENTIALS_KEY,
        label_i18n_key: "settings_item_cors_allow_credentials_label",
        description_i18n_key: "settings_item_cors_allow_credentials_desc",
        value_type: SystemConfigValueType::Boolean,
        default_fn: || "false".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "network",
        description: "Whether CORS responses include Access-Control-Allow-Credentials",
    },
    ConfigDef {
        key: CORS_MAX_AGE_SECS_KEY,
        label_i18n_key: "settings_item_cors_max_age_secs_label",
        description_i18n_key: "settings_item_cors_max_age_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || "3600".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "network",
        description: "CORS preflight cache duration in seconds",
    },
    // ── Operations ──────────────────────────────────────────
    ConfigDef {
        key: MAIL_OUTBOX_DISPATCH_INTERVAL_SECS_KEY,
        label_i18n_key: "settings_item_mail_outbox_dispatch_interval_secs_label",
        description_i18n_key: "settings_item_mail_outbox_dispatch_interval_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_MAIL_OUTBOX_DISPATCH_INTERVAL_SECS.to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "operations",
        description: "Seconds between mail outbox dispatch polls",
    },
    ConfigDef {
        key: BACKGROUND_TASK_DISPATCH_INTERVAL_SECS_KEY,
        label_i18n_key: "settings_item_background_task_dispatch_interval_secs_label",
        description_i18n_key: "settings_item_background_task_dispatch_interval_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_BACKGROUND_TASK_DISPATCH_INTERVAL_SECS.to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "operations",
        description: "Seconds between background task dispatch polls",
    },
    ConfigDef {
        key: BACKGROUND_TASK_DISPATCH_IDLE_MAX_INTERVAL_SECS_KEY,
        label_i18n_key: "settings_item_background_task_dispatch_idle_max_interval_secs_label",
        description_i18n_key: "settings_item_background_task_dispatch_idle_max_interval_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_BACKGROUND_TASK_DISPATCH_IDLE_MAX_INTERVAL_SECS
                .to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "operations",
        description: "Maximum seconds between background task dispatch polls after idle backoff",
    },
    ConfigDef {
        key: BACKGROUND_TASK_MAX_CONCURRENCY_KEY,
        label_i18n_key: "settings_item_background_task_max_concurrency_label",
        description_i18n_key: "settings_item_background_task_max_concurrency_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_BACKGROUND_TASK_MAX_CONCURRENCY.to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "operations.background_task",
        description: "Reserved fallback concurrency cap; currently unused until future task kinds are assigned to the fallback lane",
    },
    ConfigDef {
        key: BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY_KEY,
        label_i18n_key: "settings_item_background_task_archive_max_concurrency_label",
        description_i18n_key: "settings_item_background_task_archive_max_concurrency_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY.to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "operations.background_task",
        description: "Maximum number of archive background tasks the server may execute at the same time",
    },
    ConfigDef {
        key: BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY_KEY,
        label_i18n_key: "settings_item_background_task_thumbnail_max_concurrency_label",
        description_i18n_key: "settings_item_background_task_thumbnail_max_concurrency_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY.to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "operations.background_task",
        description: "Maximum number of thumbnail background tasks the server may execute at the same time",
    },
    ConfigDef {
        key: BACKGROUND_TASK_MAX_ATTEMPTS_KEY,
        label_i18n_key: "settings_item_background_task_max_attempts_label",
        description_i18n_key: "settings_item_background_task_max_attempts_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || crate::config::operations::DEFAULT_BACKGROUND_TASK_MAX_ATTEMPTS.to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "operations",
        description: "Maximum number of attempts for workspace background tasks before they permanently fail",
    },
    ConfigDef {
        key: SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY_KEY,
        label_i18n_key: "settings_item_share_download_rollback_queue_capacity_label",
        description_i18n_key: "settings_item_share_download_rollback_queue_capacity_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY.to_string()
        },
        requires_restart: true,
        is_sensitive: false,
        category: "operations",
        description: "Maximum buffered shared download rollback jobs before overflow aggregation is used",
    },
    ConfigDef {
        key: SHARE_STREAM_SESSION_TTL_SECS_KEY,
        label_i18n_key: "settings_item_share_stream_session_ttl_secs_label",
        description_i18n_key: "settings_item_share_stream_session_ttl_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || crate::config::operations::DEFAULT_SHARE_STREAM_SESSION_TTL_SECS.to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "storage",
        description: "Lifetime in seconds for shared file stream sessions",
    },
    ConfigDef {
        key: MAINTENANCE_CLEANUP_INTERVAL_SECS_KEY,
        label_i18n_key: "settings_item_maintenance_cleanup_interval_secs_label",
        description_i18n_key: "settings_item_maintenance_cleanup_interval_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_MAINTENANCE_CLEANUP_INTERVAL_SECS.to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "operations",
        description: "Seconds between periodic maintenance cleanup runs",
    },
    ConfigDef {
        key: BLOB_RECONCILE_INTERVAL_SECS_KEY,
        label_i18n_key: "settings_item_blob_reconcile_interval_secs_label",
        description_i18n_key: "settings_item_blob_reconcile_interval_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || crate::config::operations::DEFAULT_BLOB_RECONCILE_INTERVAL_SECS.to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "operations",
        description: "Seconds between full blob reconciliation runs",
    },
    ConfigDef {
        key: REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS_KEY,
        label_i18n_key: "settings_item_remote_node_health_test_interval_secs_label",
        description_i18n_key: "settings_item_remote_node_health_test_interval_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS.to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "operations",
        description: "Seconds between periodic system health checks for database, cache, and remote nodes",
    },
    ConfigDef {
        key: TEAM_MEMBER_LIST_MAX_LIMIT_KEY,
        label_i18n_key: "settings_item_team_member_list_max_limit_label",
        description_i18n_key: "settings_item_team_member_list_max_limit_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || crate::config::operations::DEFAULT_TEAM_MEMBER_LIST_MAX_LIMIT.to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "operations",
        description: "Maximum page size accepted by team member listing endpoints",
    },
    ConfigDef {
        key: TASK_LIST_MAX_LIMIT_KEY,
        label_i18n_key: "settings_item_task_list_max_limit_label",
        description_i18n_key: "settings_item_task_list_max_limit_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || crate::config::operations::DEFAULT_TASK_LIST_MAX_LIMIT.to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "operations",
        description: "Maximum page size accepted by background task listing endpoints",
    },
    // ── Storage ─────────────────────────────────────────────
    ConfigDef {
        key: MAX_VERSIONS_PER_FILE_KEY,
        label_i18n_key: "settings_item_max_versions_per_file_label",
        description_i18n_key: "settings_item_max_versions_per_file_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || "10".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "storage",
        description: "Maximum number of historical versions kept per file",
    },
    ConfigDef {
        key: TRASH_RETENTION_DAYS_KEY,
        label_i18n_key: "settings_item_trash_retention_days_label",
        description_i18n_key: "settings_item_trash_retention_days_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || "7".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "storage",
        description: "Days before soft-deleted items are permanently purged",
    },
    ConfigDef {
        key: TEAM_ARCHIVE_RETENTION_DAYS_KEY,
        label_i18n_key: "settings_item_team_archive_retention_days_label",
        description_i18n_key: "settings_item_team_archive_retention_days_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || "7".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "storage",
        description: "Days before archived teams are permanently deleted",
    },
    ConfigDef {
        key: TASK_RETENTION_HOURS_KEY,
        label_i18n_key: "settings_item_task_retention_hours_label",
        description_i18n_key: "settings_item_task_retention_hours_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || "24".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "storage",
        description: "Hours before temporary background task artifacts expire; task records remain as history",
    },
    ConfigDef {
        key: DEFAULT_STORAGE_QUOTA_KEY,
        label_i18n_key: "settings_item_default_storage_quota_label",
        description_i18n_key: "settings_item_default_storage_quota_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || "0".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "storage",
        description: "Default storage quota for new users in bytes (0 = unlimited)",
    },
    ConfigDef {
        key: ARCHIVE_EXTRACT_MAX_SOURCE_BYTES_KEY,
        label_i18n_key: "settings_item_archive_extract_max_source_bytes_label",
        description_i18n_key: "settings_item_archive_extract_max_source_bytes_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_ARCHIVE_EXTRACT_MAX_SOURCE_BYTES.to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "storage.archive_extract",
        description: "Maximum source ZIP file bytes accepted for online archive extraction",
    },
    ConfigDef {
        key: ARCHIVE_EXTRACT_MAX_STAGING_BYTES_KEY,
        label_i18n_key: "settings_item_archive_extract_max_staging_bytes_label",
        description_i18n_key: "settings_item_archive_extract_max_staging_bytes_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_ARCHIVE_EXTRACT_MAX_STAGING_BYTES.to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "storage.archive_extract",
        description: "Maximum total temporary bytes allowed for archive extract staging, including the downloaded source archive and extracted files",
    },
    ConfigDef {
        key: ARCHIVE_EXTRACT_MAX_UNCOMPRESSED_BYTES_KEY,
        label_i18n_key: "settings_item_archive_extract_max_uncompressed_bytes_label",
        description_i18n_key: "settings_item_archive_extract_max_uncompressed_bytes_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_ARCHIVE_EXTRACT_MAX_UNCOMPRESSED_BYTES.to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "storage.archive_extract",
        description: "Maximum total uncompressed file bytes accepted inside a ZIP archive before import",
    },
    ConfigDef {
        key: ARCHIVE_EXTRACT_MAX_ENTRIES_KEY,
        label_i18n_key: "settings_item_archive_extract_max_entries_label",
        description_i18n_key: "settings_item_archive_extract_max_entries_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || crate::config::operations::DEFAULT_ARCHIVE_EXTRACT_MAX_ENTRIES.to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "storage.archive_extract",
        description: "Maximum number of central-directory entries accepted in a ZIP archive",
    },
    ConfigDef {
        key: ARCHIVE_EXTRACT_MAX_FILES_KEY,
        label_i18n_key: "settings_item_archive_extract_max_files_label",
        description_i18n_key: "settings_item_archive_extract_max_files_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || crate::config::operations::DEFAULT_ARCHIVE_EXTRACT_MAX_FILES.to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "storage.archive_extract",
        description: "Maximum number of file entries accepted in a ZIP archive",
    },
    ConfigDef {
        key: ARCHIVE_EXTRACT_MAX_DIRECTORIES_KEY,
        label_i18n_key: "settings_item_archive_extract_max_directories_label",
        description_i18n_key: "settings_item_archive_extract_max_directories_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_ARCHIVE_EXTRACT_MAX_DIRECTORIES.to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "storage.archive_extract",
        description: "Maximum number of directory paths accepted in a ZIP archive",
    },
    ConfigDef {
        key: ARCHIVE_EXTRACT_MAX_DEPTH_KEY,
        label_i18n_key: "settings_item_archive_extract_max_depth_label",
        description_i18n_key: "settings_item_archive_extract_max_depth_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || crate::config::operations::DEFAULT_ARCHIVE_EXTRACT_MAX_DEPTH.to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "storage.archive_extract",
        description: "Maximum normalized path depth accepted for ZIP archive entries",
    },
    ConfigDef {
        key: ARCHIVE_EXTRACT_MAX_PATH_BYTES_KEY,
        label_i18n_key: "settings_item_archive_extract_max_path_bytes_label",
        description_i18n_key: "settings_item_archive_extract_max_path_bytes_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_ARCHIVE_EXTRACT_MAX_PATH_BYTES.to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "storage.archive_extract",
        description: "Maximum UTF-8 byte length accepted for a normalized ZIP archive entry path",
    },
    ConfigDef {
        key: ARCHIVE_EXTRACT_MAX_COMPRESSION_RATIO_KEY,
        label_i18n_key: "settings_item_archive_extract_max_compression_ratio_label",
        description_i18n_key: "settings_item_archive_extract_max_compression_ratio_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_ARCHIVE_EXTRACT_MAX_COMPRESSION_RATIO.to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "storage.archive_extract",
        description: "Maximum total uncompressed-to-compressed byte ratio accepted for a ZIP archive",
    },
    ConfigDef {
        key: ARCHIVE_EXTRACT_MAX_ENTRY_COMPRESSION_RATIO_KEY,
        label_i18n_key: "settings_item_archive_extract_max_entry_compression_ratio_label",
        description_i18n_key: "settings_item_archive_extract_max_entry_compression_ratio_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_ARCHIVE_EXTRACT_MAX_ENTRY_COMPRESSION_RATIO
                .to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "storage.archive_extract",
        description: "Maximum per-file uncompressed-to-compressed byte ratio accepted for ZIP archive entries",
    },
    ConfigDef {
        key: ARCHIVE_EXTRACT_MAX_DURATION_SECS_KEY,
        label_i18n_key: "settings_item_archive_extract_max_duration_secs_label",
        description_i18n_key: "settings_item_archive_extract_max_duration_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_ARCHIVE_EXTRACT_MAX_DURATION_SECS.to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "storage.archive_extract",
        description: "Maximum wall-clock seconds allowed for one online archive extraction task",
    },
    ConfigDef {
        key: ARCHIVE_PREVIEW_ENABLED_KEY,
        label_i18n_key: "settings_item_archive_preview_enabled_label",
        description_i18n_key: "settings_item_archive_preview_enabled_desc",
        value_type: SystemConfigValueType::Boolean,
        default_fn: || "false".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "storage.archive_preview",
        description: "Master switch for read-only ZIP archive preview",
    },
    ConfigDef {
        key: ARCHIVE_PREVIEW_USER_ENABLED_KEY,
        label_i18n_key: "settings_item_archive_preview_user_enabled_label",
        description_i18n_key: "settings_item_archive_preview_user_enabled_desc",
        value_type: SystemConfigValueType::Boolean,
        default_fn: || "false".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "storage.archive_preview",
        description: "Allow signed-in users to preview ZIP manifests for personal and team files",
    },
    ConfigDef {
        key: ARCHIVE_PREVIEW_SHARE_ENABLED_KEY,
        label_i18n_key: "settings_item_archive_preview_share_enabled_label",
        description_i18n_key: "settings_item_archive_preview_share_enabled_desc",
        value_type: SystemConfigValueType::Boolean,
        default_fn: || "false".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "storage.archive_preview",
        description: "Allow public share pages to preview ZIP manifests after share access checks",
    },
    ConfigDef {
        key: ARCHIVE_PREVIEW_MAX_SOURCE_BYTES_KEY,
        label_i18n_key: "settings_item_archive_preview_max_source_bytes_label",
        description_i18n_key: "settings_item_archive_preview_max_source_bytes_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_ARCHIVE_PREVIEW_MAX_SOURCE_BYTES.to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "storage.archive_preview",
        description: "Maximum source ZIP file bytes accepted for read-only archive preview",
    },
    ConfigDef {
        key: ARCHIVE_PREVIEW_MAX_ENTRIES_KEY,
        label_i18n_key: "settings_item_archive_preview_max_entries_label",
        description_i18n_key: "settings_item_archive_preview_max_entries_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || crate::config::operations::DEFAULT_ARCHIVE_PREVIEW_MAX_ENTRIES.to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "storage.archive_preview",
        description: "Maximum number of ZIP central-directory entries accepted for archive preview",
    },
    ConfigDef {
        key: ARCHIVE_PREVIEW_MAX_MANIFEST_BYTES_KEY,
        label_i18n_key: "settings_item_archive_preview_max_manifest_bytes_label",
        description_i18n_key: "settings_item_archive_preview_max_manifest_bytes_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_ARCHIVE_PREVIEW_MAX_MANIFEST_BYTES.to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "storage.archive_preview",
        description: "Maximum serialized ZIP preview manifest bytes returned to clients",
    },
    ConfigDef {
        key: ARCHIVE_PREVIEW_MAX_DURATION_SECS_KEY,
        label_i18n_key: "settings_item_archive_preview_max_duration_secs_label",
        description_i18n_key: "settings_item_archive_preview_max_duration_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_ARCHIVE_PREVIEW_MAX_DURATION_SECS.to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "storage.archive_preview",
        description: "Maximum wall-clock seconds allowed for one ZIP preview scan",
    },
    ConfigDef {
        key: ARCHIVE_BUILD_MAX_ENTRIES_KEY,
        label_i18n_key: "settings_item_archive_build_max_entries_label",
        description_i18n_key: "settings_item_archive_build_max_entries_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || crate::config::operations::DEFAULT_ARCHIVE_BUILD_MAX_ENTRIES.to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "storage.archive_build",
        description: "Maximum expanded file and directory entries accepted for archive compression or download",
    },
    ConfigDef {
        key: ARCHIVE_BUILD_MAX_TOTAL_SOURCE_BYTES_KEY,
        label_i18n_key: "settings_item_archive_build_max_total_source_bytes_label",
        description_i18n_key: "settings_item_archive_build_max_total_source_bytes_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_ARCHIVE_BUILD_MAX_TOTAL_SOURCE_BYTES.to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "storage.archive_build",
        description: "Maximum total source bytes accepted for archive compression or download",
    },
    ConfigDef {
        key: ARCHIVE_BUILD_MAX_TEMP_BYTES_KEY,
        label_i18n_key: "settings_item_archive_build_max_temp_bytes_label",
        description_i18n_key: "settings_item_archive_build_max_temp_bytes_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || crate::config::operations::DEFAULT_ARCHIVE_BUILD_MAX_TEMP_BYTES.to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "storage.archive_build",
        description: "Maximum estimated or actual ZIP output bytes accepted for archive compression or download",
    },
    ConfigDef {
        key: THUMBNAIL_MAX_SOURCE_BYTES_KEY,
        label_i18n_key: "settings_item_thumbnail_max_source_bytes_label",
        description_i18n_key: "settings_item_thumbnail_max_source_bytes_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || crate::config::operations::DEFAULT_THUMBNAIL_MAX_SOURCE_BYTES.to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "storage.media_processing",
        description: "Maximum original file size eligible for thumbnail generation in bytes",
    },
    ConfigDef {
        key: MEDIA_METADATA_ENABLED_KEY,
        label_i18n_key: "settings_item_media_metadata_enabled_label",
        description_i18n_key: "settings_item_media_metadata_enabled_desc",
        value_type: SystemConfigValueType::Boolean,
        default_fn: || crate::config::operations::DEFAULT_MEDIA_METADATA_ENABLED.to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "storage.media_processing",
        description: "Enable backend blob-level media metadata extraction and cache",
    },
    ConfigDef {
        key: MEDIA_METADATA_MAX_SOURCE_BYTES_KEY,
        label_i18n_key: "settings_item_media_metadata_max_source_bytes_label",
        description_i18n_key: "settings_item_media_metadata_max_source_bytes_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_MEDIA_METADATA_MAX_SOURCE_BYTES.to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "storage.media_processing",
        description: "Maximum original file size eligible for backend media metadata extraction in bytes",
    },
    ConfigDef {
        key: MEDIA_PROCESSING_REGISTRY_JSON_KEY,
        label_i18n_key: "settings_item_media_processing_registry_json_label",
        description_i18n_key: "settings_item_media_processing_registry_json_desc",
        value_type: SystemConfigValueType::Multiline,
        default_fn: crate::config::media_processing::default_media_processing_registry_json,
        requires_restart: false,
        is_sensitive: false,
        category: "storage.media_processing",
        description: "Unified media processing registry for thumbnail and metadata processors",
    },
    // ── User ───────────────────────────────────────────────
    ConfigDef {
        key: AUTH_ALLOW_USER_REGISTRATION_KEY,
        label_i18n_key: "settings_item_auth_allow_user_registration_label",
        description_i18n_key: "settings_item_auth_allow_user_registration_desc",
        value_type: SystemConfigValueType::Boolean,
        default_fn: || "true".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "user.registration_and_login",
        description: "Whether new users can self-register from the public auth flow",
    },
    ConfigDef {
        key: AUTH_REGISTER_ACTIVATION_ENABLED_KEY,
        label_i18n_key: "settings_item_auth_register_activation_enabled_label",
        description_i18n_key: "settings_item_auth_register_activation_enabled_desc",
        value_type: SystemConfigValueType::Boolean,
        default_fn: || "true".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "user.registration_and_login",
        description: "Whether newly registered users must activate their account by email before signing in",
    },
    ConfigDef {
        key: AVATAR_DIR_KEY,
        label_i18n_key: "settings_item_avatar_dir_label",
        description_i18n_key: "settings_item_avatar_dir_desc",
        value_type: SystemConfigValueType::String,
        default_fn: || crate::config::avatar::DEFAULT_AVATAR_DIR.to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "user.avatar",
        description: "Local directory used for uploaded avatar files (relative paths resolve under ./data)",
    },
    ConfigDef {
        key: AVATAR_MAX_UPLOAD_SIZE_BYTES_KEY,
        label_i18n_key: "settings_item_avatar_max_upload_size_bytes_label",
        description_i18n_key: "settings_item_avatar_max_upload_size_bytes_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || crate::config::operations::DEFAULT_AVATAR_MAX_UPLOAD_SIZE_BYTES.to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "user.avatar",
        description: "Maximum avatar upload size in bytes before the request is rejected",
    },
    ConfigDef {
        key: "gravatar_base_url",
        label_i18n_key: "settings_item_gravatar_base_url_label",
        description_i18n_key: "settings_item_gravatar_base_url_desc",
        value_type: SystemConfigValueType::String,
        default_fn: || "https://www.gravatar.com/avatar".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "user.avatar",
        description: "Gravatar avatar base URL (change to proxy/mirror if needed)",
    },
    // ── Audit ─────────────────────────────────────────────
    ConfigDef {
        key: AUDIT_LOG_ENABLED_KEY,
        label_i18n_key: "settings_item_audit_log_enabled_label",
        description_i18n_key: "settings_item_audit_log_enabled_desc",
        value_type: SystemConfigValueType::Boolean,
        default_fn: || "true".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "audit",
        description: "Enable or disable audit logging",
    },
    ConfigDef {
        key: AUDIT_LOG_RETENTION_DAYS_KEY,
        label_i18n_key: "settings_item_audit_log_retention_days_label",
        description_i18n_key: "settings_item_audit_log_retention_days_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || "90".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "audit",
        description: "Days before audit log entries are permanently deleted",
    },
    // ── Mail ──────────────────────────────────────────────
    ConfigDef {
        key: MAIL_SMTP_HOST_KEY,
        label_i18n_key: "settings_item_mail_smtp_host_label",
        description_i18n_key: "settings_item_mail_smtp_host_desc",
        value_type: SystemConfigValueType::String,
        default_fn: String::new,
        requires_restart: false,
        is_sensitive: false,
        category: "mail.config",
        description: "SMTP server hostname used for transactional email delivery",
    },
    ConfigDef {
        key: MAIL_SMTP_PORT_KEY,
        label_i18n_key: "settings_item_mail_smtp_port_label",
        description_i18n_key: "settings_item_mail_smtp_port_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || "587".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "mail.config",
        description: "SMTP server port used for transactional email delivery",
    },
    ConfigDef {
        key: MAIL_SMTP_USERNAME_KEY,
        label_i18n_key: "settings_item_mail_smtp_username_label",
        description_i18n_key: "settings_item_mail_smtp_username_desc",
        value_type: SystemConfigValueType::String,
        default_fn: String::new,
        requires_restart: false,
        is_sensitive: false,
        category: "mail.config",
        description: "SMTP username for authenticated mail delivery",
    },
    ConfigDef {
        key: MAIL_SMTP_PASSWORD_KEY,
        label_i18n_key: "settings_item_mail_smtp_password_label",
        description_i18n_key: "settings_item_mail_smtp_password_desc",
        value_type: SystemConfigValueType::String,
        default_fn: String::new,
        requires_restart: false,
        is_sensitive: true,
        category: "mail.config",
        description: "SMTP password for authenticated mail delivery",
    },
    ConfigDef {
        key: MAIL_FROM_ADDRESS_KEY,
        label_i18n_key: "settings_item_mail_from_address_label",
        description_i18n_key: "settings_item_mail_from_address_desc",
        value_type: SystemConfigValueType::String,
        default_fn: String::new,
        requires_restart: false,
        is_sensitive: false,
        category: "mail.config",
        description: "From address used for account activation and contact verification email",
    },
    ConfigDef {
        key: MAIL_FROM_NAME_KEY,
        label_i18n_key: "settings_item_mail_from_name_label",
        description_i18n_key: "settings_item_mail_from_name_desc",
        value_type: SystemConfigValueType::String,
        default_fn: || "AsterDrive".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "mail.config",
        description: "Display name used for account activation and contact verification email",
    },
    ConfigDef {
        key: MAIL_SECURITY_KEY,
        label_i18n_key: "settings_item_mail_security_label",
        description_i18n_key: "settings_item_mail_security_desc",
        value_type: SystemConfigValueType::Boolean,
        default_fn: || "true".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "mail.config",
        description: "Whether SMTP uses encryption. Port 465 uses implicit SSL/TLS; other ports use STARTTLS when enabled",
    },
    ConfigDef {
        key: MAIL_TEMPLATE_REGISTER_ACTIVATION_SUBJECT_KEY,
        label_i18n_key: "settings_item_mail_template_register_activation_subject_label",
        description_i18n_key: "settings_item_mail_template_register_activation_subject_desc",
        value_type: SystemConfigValueType::String,
        default_fn: || {
            crate::config::mail::default_template_subject(
                crate::types::MailTemplateCode::RegisterActivation,
            )
            .to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "mail.template",
        description: "Subject template for registration activation emails",
    },
    ConfigDef {
        key: MAIL_TEMPLATE_REGISTER_ACTIVATION_HTML_KEY,
        label_i18n_key: "settings_item_mail_template_register_activation_html_label",
        description_i18n_key: "settings_item_mail_template_register_activation_html_desc",
        value_type: SystemConfigValueType::Multiline,
        default_fn: || {
            crate::config::mail::default_template_html(
                crate::types::MailTemplateCode::RegisterActivation,
            )
            .to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "mail.template",
        description: "HTML template for registration activation emails. Prefer a complete HTML document for best client compatibility",
    },
    ConfigDef {
        key: MAIL_TEMPLATE_CONTACT_CHANGE_CONFIRMATION_SUBJECT_KEY,
        label_i18n_key: "settings_item_mail_template_contact_change_confirmation_subject_label",
        description_i18n_key: "settings_item_mail_template_contact_change_confirmation_subject_desc",
        value_type: SystemConfigValueType::String,
        default_fn: || {
            crate::config::mail::default_template_subject(
                crate::types::MailTemplateCode::ContactChangeConfirmation,
            )
            .to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "mail.template",
        description: "Subject template for email change confirmation emails",
    },
    ConfigDef {
        key: MAIL_TEMPLATE_CONTACT_CHANGE_CONFIRMATION_HTML_KEY,
        label_i18n_key: "settings_item_mail_template_contact_change_confirmation_html_label",
        description_i18n_key: "settings_item_mail_template_contact_change_confirmation_html_desc",
        value_type: SystemConfigValueType::Multiline,
        default_fn: || {
            crate::config::mail::default_template_html(
                crate::types::MailTemplateCode::ContactChangeConfirmation,
            )
            .to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "mail.template",
        description: "HTML template for email change confirmation emails. Prefer a complete HTML document for best client compatibility",
    },
    ConfigDef {
        key: MAIL_TEMPLATE_PASSWORD_RESET_SUBJECT_KEY,
        label_i18n_key: "settings_item_mail_template_password_reset_subject_label",
        description_i18n_key: "settings_item_mail_template_password_reset_subject_desc",
        value_type: SystemConfigValueType::String,
        default_fn: || {
            crate::config::mail::default_template_subject(
                crate::types::MailTemplateCode::PasswordReset,
            )
            .to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "mail.template",
        description: "Subject template for password reset emails",
    },
    ConfigDef {
        key: MAIL_TEMPLATE_PASSWORD_RESET_HTML_KEY,
        label_i18n_key: "settings_item_mail_template_password_reset_html_label",
        description_i18n_key: "settings_item_mail_template_password_reset_html_desc",
        value_type: SystemConfigValueType::Multiline,
        default_fn: || {
            crate::config::mail::default_template_html(
                crate::types::MailTemplateCode::PasswordReset,
            )
            .to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "mail.template",
        description: "HTML template for password reset emails. Prefer a complete HTML document for best client compatibility",
    },
    ConfigDef {
        key: MAIL_TEMPLATE_PASSWORD_RESET_NOTICE_SUBJECT_KEY,
        label_i18n_key: "settings_item_mail_template_password_reset_notice_subject_label",
        description_i18n_key: "settings_item_mail_template_password_reset_notice_subject_desc",
        value_type: SystemConfigValueType::String,
        default_fn: || {
            crate::config::mail::default_template_subject(
                crate::types::MailTemplateCode::PasswordResetNotice,
            )
            .to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "mail.template",
        description: "Subject template for password reset confirmation emails",
    },
    ConfigDef {
        key: MAIL_TEMPLATE_PASSWORD_RESET_NOTICE_HTML_KEY,
        label_i18n_key: "settings_item_mail_template_password_reset_notice_html_label",
        description_i18n_key: "settings_item_mail_template_password_reset_notice_html_desc",
        value_type: SystemConfigValueType::Multiline,
        default_fn: || {
            crate::config::mail::default_template_html(
                crate::types::MailTemplateCode::PasswordResetNotice,
            )
            .to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "mail.template",
        description: "HTML template for password reset confirmation emails. Prefer a complete HTML document for best client compatibility",
    },
    ConfigDef {
        key: MAIL_TEMPLATE_CONTACT_CHANGE_NOTICE_SUBJECT_KEY,
        label_i18n_key: "settings_item_mail_template_contact_change_notice_subject_label",
        description_i18n_key: "settings_item_mail_template_contact_change_notice_subject_desc",
        value_type: SystemConfigValueType::String,
        default_fn: || {
            crate::config::mail::default_template_subject(
                crate::types::MailTemplateCode::ContactChangeNotice,
            )
            .to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "mail.template",
        description: "Subject template for previous-address email change notices",
    },
    ConfigDef {
        key: MAIL_TEMPLATE_CONTACT_CHANGE_NOTICE_HTML_KEY,
        label_i18n_key: "settings_item_mail_template_contact_change_notice_html_label",
        description_i18n_key: "settings_item_mail_template_contact_change_notice_html_desc",
        value_type: SystemConfigValueType::Multiline,
        default_fn: || {
            crate::config::mail::default_template_html(
                crate::types::MailTemplateCode::ContactChangeNotice,
            )
            .to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "mail.template",
        description: "HTML template for previous-address email change notices. Prefer a complete HTML document for best client compatibility",
    },
    ConfigDef {
        key: MAIL_TEMPLATE_EXTERNAL_AUTH_EMAIL_VERIFICATION_SUBJECT_KEY,
        label_i18n_key: "settings_item_mail_template_external_auth_email_verification_subject_label",
        description_i18n_key: "settings_item_mail_template_external_auth_email_verification_subject_desc",
        value_type: SystemConfigValueType::String,
        default_fn: || {
            crate::config::mail::default_template_subject(
                crate::types::MailTemplateCode::ExternalAuthEmailVerification,
            )
            .to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "mail.template",
        description: "Subject template for external auth email verification emails",
    },
    ConfigDef {
        key: MAIL_TEMPLATE_EXTERNAL_AUTH_EMAIL_VERIFICATION_HTML_KEY,
        label_i18n_key: "settings_item_mail_template_external_auth_email_verification_html_label",
        description_i18n_key: "settings_item_mail_template_external_auth_email_verification_html_desc",
        value_type: SystemConfigValueType::Multiline,
        default_fn: || {
            crate::config::mail::default_template_html(
                crate::types::MailTemplateCode::ExternalAuthEmailVerification,
            )
            .to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: "mail.template",
        description: "HTML template for external auth email verification emails. Prefer a complete HTML document for best client compatibility",
    },
    // ── General ─────────────────────────────────────────────
    ConfigDef {
        key: PUBLIC_SITE_URL_KEY,
        label_i18n_key: "settings_item_public_site_url_label",
        description_i18n_key: "settings_item_public_site_url_desc",
        value_type: SystemConfigValueType::StringArray,
        default_fn: empty_string_array_default,
        requires_restart: false,
        is_sensitive: false,
        category: "general",
        description: "Trusted public HTTP(S) frontend origins as a JSON string array. They are used to generate share, preview, WebDAV, WOPI, and callback URLs, and they also extend exact-match trusted frontend origins for cookie-authenticated same-site CSRF checks. This is separate from CORS and mainly affects same-site subdomain deployments; do not add domains you do not control. The first origin is the fallback",
    },
    ConfigDef {
        key: BRANDING_TITLE_KEY,
        label_i18n_key: "settings_item_branding_title_label",
        description_i18n_key: "settings_item_branding_title_desc",
        value_type: SystemConfigValueType::String,
        default_fn: || "AsterDrive".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "general",
        description: "Public browser title used by anonymous and authenticated pages",
    },
    ConfigDef {
        key: BRANDING_DESCRIPTION_KEY,
        label_i18n_key: "settings_item_branding_description_label",
        description_i18n_key: "settings_item_branding_description_desc",
        value_type: SystemConfigValueType::String,
        default_fn: || "Self-hosted cloud storage".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "general",
        description: "Public HTML description metadata exposed to anonymous pages",
    },
    ConfigDef {
        key: BRANDING_FAVICON_URL_KEY,
        label_i18n_key: "settings_item_branding_favicon_url_label",
        description_i18n_key: "settings_item_branding_favicon_url_desc",
        value_type: SystemConfigValueType::String,
        default_fn: || "/favicon.svg".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "general",
        description: "Public favicon URL applied at runtime for anonymous and authenticated pages",
    },
    ConfigDef {
        key: BRANDING_WORDMARK_DARK_URL_KEY,
        label_i18n_key: "settings_item_branding_wordmark_dark_url_label",
        description_i18n_key: "settings_item_branding_wordmark_dark_url_desc",
        value_type: SystemConfigValueType::String,
        default_fn: || "/static/asterdrive/asterdrive-dark.svg".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "general",
        description: "Public site logo URL used on light surfaces such as headers and forms",
    },
    ConfigDef {
        key: BRANDING_WORDMARK_LIGHT_URL_KEY,
        label_i18n_key: "settings_item_branding_wordmark_light_url_label",
        description_i18n_key: "settings_item_branding_wordmark_light_url_desc",
        value_type: SystemConfigValueType::String,
        default_fn: || "/static/asterdrive/asterdrive-light.svg".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "general",
        description: "Public site logo URL used on dark surfaces such as the login hero panel",
    },
    ConfigDef {
        key: WOPI_ACCESS_TOKEN_TTL_SECS_KEY,
        label_i18n_key: "settings_item_wopi_access_token_ttl_secs_label",
        description_i18n_key: "settings_item_wopi_access_token_ttl_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || crate::config::wopi::DEFAULT_WOPI_ACCESS_TOKEN_TTL_SECS.to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "general.preview",
        description: "Lifetime of WOPI access tokens in seconds",
    },
    ConfigDef {
        key: WOPI_LOCK_TTL_SECS_KEY,
        label_i18n_key: "settings_item_wopi_lock_ttl_secs_label",
        description_i18n_key: "settings_item_wopi_lock_ttl_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || crate::config::wopi::DEFAULT_WOPI_LOCK_TTL_SECS.to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "general.preview",
        description: "Lifetime of active WOPI locks in seconds before they expire automatically",
    },
    ConfigDef {
        key: WOPI_DISCOVERY_CACHE_TTL_SECS_KEY,
        label_i18n_key: "settings_item_wopi_discovery_cache_ttl_secs_label",
        description_i18n_key: "settings_item_wopi_discovery_cache_ttl_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || crate::config::wopi::DEFAULT_WOPI_DISCOVERY_CACHE_TTL_SECS.to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: "general.preview",
        description: "How long fetched WOPI discovery metadata stays cached in seconds",
    },
    ConfigDef {
        key: crate::services::preview_app_service::PREVIEW_APPS_CONFIG_KEY,
        label_i18n_key: "settings_item_frontend_preview_apps_json_label",
        description_i18n_key: "settings_item_frontend_preview_apps_json_desc",
        value_type: SystemConfigValueType::Multiline,
        default_fn: crate::services::preview_app_service::default_public_preview_apps_json,
        requires_restart: false,
        is_sensitive: false,
        category: "general.preview",
        description: "Public preview app registry used by the web frontend, including extension bindings",
    },
];
