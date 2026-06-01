//! 运行时配置定义 — 所有 system_config 键的单一数据源
//!
//! 启动时默认配置初始化流程遍历此数组，
//! 对每项执行 INSERT ... ON CONFLICT DO NOTHING。
//!
//! 所有 `system_config` 键字符串在此处以 `pub const` 形式声明，
//! 子模块通过 `pub use crate::config::definitions::*_KEY` 重导出，
//! 不再各自定义本地 `const`，确保单一数据源。

use crate::types::SystemConfigValueType;

// ── Category keys ───────────────────────────────────────────────────────────
pub const CONFIG_CATEGORY_SITE: &str = "site";
pub const CONFIG_CATEGORY_SITE_PREVIEW: &str = "site.preview";
pub const CONFIG_CATEGORY_USER_REGISTRATION: &str = "user.registration_and_login";
pub const CONFIG_CATEGORY_USER_AVATAR: &str = "user.avatar";
pub const CONFIG_CATEGORY_AUTH: &str = "auth";
pub const CONFIG_CATEGORY_MAIL_CONFIG: &str = "mail.config";
pub const CONFIG_CATEGORY_MAIL_TEMPLATE: &str = "mail.template";
pub const CONFIG_CATEGORY_NETWORK: &str = "network";
pub const CONFIG_CATEGORY_RUNTIME_MAIL: &str = "runtime.mail";
pub const CONFIG_CATEGORY_RUNTIME_BACKGROUND_TASK: &str = "runtime.background_task";
pub const CONFIG_CATEGORY_RUNTIME_MAINTENANCE: &str = "runtime.maintenance";
pub const CONFIG_CATEGORY_RUNTIME_LIMITS: &str = "runtime.limits";
pub const CONFIG_CATEGORY_RUNTIME_SHARE_STREAM: &str = "runtime.share_stream";
pub const CONFIG_CATEGORY_STORAGE: &str = "storage";
pub const CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_EXTRACT: &str = "file_processing.archive_extract";
pub const CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_PREVIEW: &str = "file_processing.archive_preview";
pub const CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_BUILD: &str = "file_processing.archive_build";
pub const CONFIG_CATEGORY_FILE_PROCESSING_OFFLINE_DOWNLOAD: &str =
    "file_processing.offline_download";
pub const CONFIG_CATEGORY_FILE_PROCESSING_MEDIA: &str = "file_processing.media";
pub const CONFIG_CATEGORY_WEBDAV: &str = "webdav";
pub const CONFIG_CATEGORY_AUDIT: &str = "audit";

pub const SYSTEM_CONFIG_ALLOWED_CATEGORIES: &[&str] = &[
    CONFIG_CATEGORY_SITE,
    CONFIG_CATEGORY_SITE_PREVIEW,
    CONFIG_CATEGORY_USER_REGISTRATION,
    CONFIG_CATEGORY_USER_AVATAR,
    CONFIG_CATEGORY_AUTH,
    CONFIG_CATEGORY_MAIL_CONFIG,
    CONFIG_CATEGORY_MAIL_TEMPLATE,
    CONFIG_CATEGORY_NETWORK,
    CONFIG_CATEGORY_RUNTIME_MAIL,
    CONFIG_CATEGORY_RUNTIME_BACKGROUND_TASK,
    CONFIG_CATEGORY_RUNTIME_MAINTENANCE,
    CONFIG_CATEGORY_RUNTIME_LIMITS,
    CONFIG_CATEGORY_RUNTIME_SHARE_STREAM,
    CONFIG_CATEGORY_STORAGE,
    CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_EXTRACT,
    CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_PREVIEW,
    CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_BUILD,
    CONFIG_CATEGORY_FILE_PROCESSING_OFFLINE_DOWNLOAD,
    CONFIG_CATEGORY_FILE_PROCESSING_MEDIA,
    CONFIG_CATEGORY_WEBDAV,
    CONFIG_CATEGORY_AUDIT,
];

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
pub const AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY: &str = "auth_email_code_login_enabled";
pub const AUTH_EMAIL_CODE_LOGIN_ALLOW_TOTP_FALLBACK_KEY: &str =
    "auth_email_code_login_allow_totp_fallback";
pub const AUTH_EMAIL_CODE_LOGIN_TTL_SECS_KEY: &str = "auth_email_code_login_ttl_secs";
pub const AUTH_EMAIL_CODE_LOGIN_RESEND_COOLDOWN_SECS_KEY: &str =
    "auth_email_code_login_resend_cooldown_secs";
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
pub const BACKGROUND_TASK_STORAGE_MIGRATION_MAX_CONCURRENCY_KEY: &str =
    "background_task_storage_migration_max_concurrency";
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
pub const OFFLINE_DOWNLOAD_MAX_FILE_SIZE_BYTES_KEY: &str = "offline_download_max_file_size_bytes";
pub const OFFLINE_DOWNLOAD_MAX_MB_PER_SEC_KEY: &str = "offline_download_max_mb_per_sec";
pub const OFFLINE_DOWNLOAD_MAX_CONCURRENCY_KEY: &str = "offline_download_max_concurrency";
pub const OFFLINE_DOWNLOAD_REQUEST_TIMEOUT_SECS_KEY: &str = "offline_download_request_timeout_secs";
pub const OFFLINE_DOWNLOAD_ENGINE_KEY: &str = "offline_download_engine";
pub const OFFLINE_DOWNLOAD_ENGINE_REGISTRY_JSON_KEY: &str = "offline_download_engine_registry_json";
pub const OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY: &str = "offline_download_aria2_rpc_url";
pub const OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY: &str = "offline_download_aria2_rpc_secret";
pub const OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS_KEY: &str =
    "offline_download_aria2_request_timeout_secs";
pub const OFFLINE_DOWNLOAD_ARIA2_SPLIT_KEY: &str = "offline_download_aria2_split";
pub const OFFLINE_DOWNLOAD_ARIA2_MAX_CONNECTION_PER_SERVER_KEY: &str =
    "offline_download_aria2_max_connection_per_server";
pub const OFFLINE_DOWNLOAD_ARIA2_LOWEST_SPEED_LIMIT_BYTES_PER_SEC_KEY: &str =
    "offline_download_aria2_lowest_speed_limit_bytes_per_sec";

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
pub const MAIL_TEMPLATE_LOGIN_EMAIL_CODE_SUBJECT_KEY: &str =
    "mail_template_login_email_code_subject";
pub const MAIL_TEMPLATE_LOGIN_EMAIL_CODE_HTML_KEY: &str = "mail_template_login_email_code_html";

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
pub const AUDIT_LOG_RECORDED_ACTIONS_KEY: &str = "audit_log_recorded_actions";

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
        category: CONFIG_CATEGORY_AUTH,
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
        category: CONFIG_CATEGORY_AUTH,
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
        category: CONFIG_CATEGORY_AUTH,
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
        category: CONFIG_CATEGORY_AUTH,
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
        category: CONFIG_CATEGORY_AUTH,
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
        category: CONFIG_CATEGORY_AUTH,
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
        category: CONFIG_CATEGORY_AUTH,
        description: "Minimum cooldown between verification email resends in seconds",
    },
    ConfigDef {
        key: AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY,
        label_i18n_key: "settings_item_auth_email_code_login_enabled_label",
        description_i18n_key: "settings_item_auth_email_code_login_enabled_desc",
        value_type: SystemConfigValueType::Boolean,
        default_fn: || "false".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_AUTH,
        description: "Allow verified-email users to complete MFA with a one-time email code when mail is configured",
    },
    ConfigDef {
        key: AUTH_EMAIL_CODE_LOGIN_ALLOW_TOTP_FALLBACK_KEY,
        label_i18n_key: "settings_item_auth_email_code_login_allow_totp_fallback_label",
        description_i18n_key: "settings_item_auth_email_code_login_allow_totp_fallback_desc",
        value_type: SystemConfigValueType::Boolean,
        default_fn: || "false".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_AUTH,
        description: "Allow users with TOTP MFA to use email code MFA as a fallback method",
    },
    ConfigDef {
        key: AUTH_EMAIL_CODE_LOGIN_TTL_SECS_KEY,
        label_i18n_key: "settings_item_auth_email_code_login_ttl_secs_label",
        description_i18n_key: "settings_item_auth_email_code_login_ttl_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || "600".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_AUTH,
        description: "Maximum email MFA login code lifetime in seconds; actual lifetime is capped by the remaining MFA challenge time",
    },
    ConfigDef {
        key: AUTH_EMAIL_CODE_LOGIN_RESEND_COOLDOWN_SECS_KEY,
        label_i18n_key: "settings_item_auth_email_code_login_resend_cooldown_secs_label",
        description_i18n_key: "settings_item_auth_email_code_login_resend_cooldown_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || "60".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_AUTH,
        description: "Minimum cooldown between email MFA login code sends in seconds",
    },
    ConfigDef {
        key: AUTH_PASSWORD_RESET_REQUEST_COOLDOWN_SECS_KEY,
        label_i18n_key: "settings_item_auth_password_reset_request_cooldown_secs_label",
        description_i18n_key: "settings_item_auth_password_reset_request_cooldown_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || "60".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_AUTH,
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
        category: CONFIG_CATEGORY_WEBDAV,
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
        category: CONFIG_CATEGORY_WEBDAV,
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
        category: CONFIG_CATEGORY_WEBDAV,
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
        category: CONFIG_CATEGORY_NETWORK,
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
        category: CONFIG_CATEGORY_NETWORK,
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
        category: CONFIG_CATEGORY_NETWORK,
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
        category: CONFIG_CATEGORY_NETWORK,
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
        category: CONFIG_CATEGORY_RUNTIME_MAIL,
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
        category: CONFIG_CATEGORY_RUNTIME_BACKGROUND_TASK,
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
        category: CONFIG_CATEGORY_RUNTIME_BACKGROUND_TASK,
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
        category: CONFIG_CATEGORY_RUNTIME_BACKGROUND_TASK,
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
        category: CONFIG_CATEGORY_RUNTIME_BACKGROUND_TASK,
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
        category: CONFIG_CATEGORY_RUNTIME_BACKGROUND_TASK,
        description: "Maximum number of thumbnail background tasks the server may execute at the same time",
    },
    ConfigDef {
        key: BACKGROUND_TASK_STORAGE_MIGRATION_MAX_CONCURRENCY_KEY,
        label_i18n_key: "settings_item_background_task_storage_migration_max_concurrency_label",
        description_i18n_key: "settings_item_background_task_storage_migration_max_concurrency_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_BACKGROUND_TASK_STORAGE_MIGRATION_MAX_CONCURRENCY
                .to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_RUNTIME_BACKGROUND_TASK,
        description: "Maximum number of storage policy migration tasks the server may execute at the same time",
    },
    ConfigDef {
        key: BACKGROUND_TASK_MAX_ATTEMPTS_KEY,
        label_i18n_key: "settings_item_background_task_max_attempts_label",
        description_i18n_key: "settings_item_background_task_max_attempts_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || crate::config::operations::DEFAULT_BACKGROUND_TASK_MAX_ATTEMPTS.to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_RUNTIME_BACKGROUND_TASK,
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
        category: CONFIG_CATEGORY_RUNTIME_LIMITS,
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
        category: CONFIG_CATEGORY_RUNTIME_SHARE_STREAM,
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
        category: CONFIG_CATEGORY_RUNTIME_MAINTENANCE,
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
        category: CONFIG_CATEGORY_RUNTIME_MAINTENANCE,
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
        category: CONFIG_CATEGORY_RUNTIME_MAINTENANCE,
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
        category: CONFIG_CATEGORY_RUNTIME_LIMITS,
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
        category: CONFIG_CATEGORY_RUNTIME_LIMITS,
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
        category: CONFIG_CATEGORY_STORAGE,
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
        category: CONFIG_CATEGORY_STORAGE,
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
        category: CONFIG_CATEGORY_STORAGE,
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
        category: CONFIG_CATEGORY_RUNTIME_BACKGROUND_TASK,
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
        category: CONFIG_CATEGORY_STORAGE,
        description: "Default storage quota for new users and teams in bytes (0 = unlimited)",
    },
    ConfigDef {
        key: OFFLINE_DOWNLOAD_ENGINE_KEY,
        label_i18n_key: "settings_item_offline_download_engine_label",
        description_i18n_key: "settings_item_offline_download_engine_desc",
        value_type: SystemConfigValueType::String,
        default_fn: || crate::config::operations::DEFAULT_OFFLINE_DOWNLOAD_ENGINE.to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_FILE_PROCESSING_OFFLINE_DOWNLOAD,
        description: "Offline download engine: builtin or aria2. builtin remains the default self-contained engine.",
    },
    ConfigDef {
        key: OFFLINE_DOWNLOAD_ENGINE_REGISTRY_JSON_KEY,
        label_i18n_key: "settings_item_offline_download_engine_registry_json_label",
        description_i18n_key: "settings_item_offline_download_engine_registry_json_desc",
        value_type: SystemConfigValueType::Multiline,
        default_fn: crate::config::offline_download::default_offline_download_engine_registry_json,
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_FILE_PROCESSING_OFFLINE_DOWNLOAD,
        description: "Ordered offline download engine registry. Enabled engines are tried in order; an empty enabled set disables link import.",
    },
    ConfigDef {
        key: OFFLINE_DOWNLOAD_MAX_FILE_SIZE_BYTES_KEY,
        label_i18n_key: "settings_item_offline_download_max_file_size_bytes_label",
        description_i18n_key: "settings_item_offline_download_max_file_size_bytes_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_OFFLINE_DOWNLOAD_MAX_FILE_SIZE_BYTES.to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_FILE_PROCESSING_OFFLINE_DOWNLOAD,
        description: "Maximum file size allowed for offline HTTP/HTTPS downloads in bytes. Tune this together with offline_download_request_timeout_secs; the 1 GiB / 600s defaults require roughly 1.7 MiB/s sustained throughput.",
    },
    ConfigDef {
        key: OFFLINE_DOWNLOAD_MAX_MB_PER_SEC_KEY,
        label_i18n_key: "settings_item_offline_download_max_mb_per_sec_label",
        description_i18n_key: "settings_item_offline_download_max_mb_per_sec_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_OFFLINE_DOWNLOAD_MAX_MB_PER_SEC.to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_FILE_PROCESSING_OFFLINE_DOWNLOAD,
        description: "Maximum offline HTTP/HTTPS download speed in MB/s (0 = unlimited). If set below the throughput needed by offline_download_max_file_size_bytes and offline_download_request_timeout_secs, large downloads will time out.",
    },
    ConfigDef {
        key: OFFLINE_DOWNLOAD_MAX_CONCURRENCY_KEY,
        label_i18n_key: "settings_item_offline_download_max_concurrency_label",
        description_i18n_key: "settings_item_offline_download_max_concurrency_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_OFFLINE_DOWNLOAD_MAX_CONCURRENCY.to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_FILE_PROCESSING_OFFLINE_DOWNLOAD,
        description: "Maximum number of offline download tasks executed concurrently",
    },
    ConfigDef {
        key: OFFLINE_DOWNLOAD_REQUEST_TIMEOUT_SECS_KEY,
        label_i18n_key: "settings_item_offline_download_request_timeout_secs_label",
        description_i18n_key: "settings_item_offline_download_request_timeout_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_OFFLINE_DOWNLOAD_REQUEST_TIMEOUT_SECS.to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_FILE_PROCESSING_OFFLINE_DOWNLOAD,
        description: "Timeout in seconds for offline download HTTP requests. Tune this with offline_download_max_file_size_bytes and offline_download_max_mb_per_sec; the 1 GiB / 600s defaults require roughly 1.7 MiB/s sustained throughput.",
    },
    ConfigDef {
        key: OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY,
        label_i18n_key: "settings_item_offline_download_aria2_rpc_url_label",
        description_i18n_key: "settings_item_offline_download_aria2_rpc_url_desc",
        value_type: SystemConfigValueType::String,
        default_fn: || String::new(),
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_FILE_PROCESSING_OFFLINE_DOWNLOAD,
        description: "aria2 JSON-RPC endpoint used when offline_download_engine is aria2, for example http://127.0.0.1:6800/jsonrpc",
    },
    ConfigDef {
        key: OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY,
        label_i18n_key: "settings_item_offline_download_aria2_rpc_secret_label",
        description_i18n_key: "settings_item_offline_download_aria2_rpc_secret_desc",
        value_type: SystemConfigValueType::String,
        default_fn: || String::new(),
        requires_restart: false,
        is_sensitive: true,
        category: CONFIG_CATEGORY_FILE_PROCESSING_OFFLINE_DOWNLOAD,
        description: "aria2 JSON-RPC secret. Stored separately from task payloads and sent as token:<secret>.",
    },
    ConfigDef {
        key: OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS_KEY,
        label_i18n_key: "settings_item_offline_download_aria2_request_timeout_secs_label",
        description_i18n_key: "settings_item_offline_download_aria2_request_timeout_secs_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS
                .to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_FILE_PROCESSING_OFFLINE_DOWNLOAD,
        description: "Timeout in seconds for individual aria2 JSON-RPC requests. The full download duration is still controlled by offline_download_request_timeout_secs.",
    },
    ConfigDef {
        key: OFFLINE_DOWNLOAD_ARIA2_SPLIT_KEY,
        label_i18n_key: "settings_item_offline_download_aria2_split_label",
        description_i18n_key: "settings_item_offline_download_aria2_split_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || crate::config::operations::DEFAULT_OFFLINE_DOWNLOAD_ARIA2_SPLIT.to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_FILE_PROCESSING_OFFLINE_DOWNLOAD,
        description: "aria2 split option for offline downloads. This is an administrator-controlled safe subset, not arbitrary aria2 option passthrough.",
    },
    ConfigDef {
        key: OFFLINE_DOWNLOAD_ARIA2_MAX_CONNECTION_PER_SERVER_KEY,
        label_i18n_key: "settings_item_offline_download_aria2_max_connection_per_server_label",
        description_i18n_key: "settings_item_offline_download_aria2_max_connection_per_server_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_OFFLINE_DOWNLOAD_ARIA2_MAX_CONNECTION_PER_SERVER
                .to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_FILE_PROCESSING_OFFLINE_DOWNLOAD,
        description: "aria2 max-connection-per-server option for offline downloads.",
    },
    ConfigDef {
        key: OFFLINE_DOWNLOAD_ARIA2_LOWEST_SPEED_LIMIT_BYTES_PER_SEC_KEY,
        label_i18n_key: "settings_item_offline_download_aria2_lowest_speed_limit_bytes_per_sec_label",
        description_i18n_key: "settings_item_offline_download_aria2_lowest_speed_limit_bytes_per_sec_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || {
            crate::config::operations::DEFAULT_OFFLINE_DOWNLOAD_ARIA2_LOWEST_SPEED_LIMIT_BYTES_PER_SEC
                .to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_FILE_PROCESSING_OFFLINE_DOWNLOAD,
        description: "aria2 lowest-speed-limit option in bytes per second. Use 0 to disable this aria2-side abort threshold.",
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
        category: CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_EXTRACT,
        description: "Maximum source archive file bytes accepted for online archive extraction",
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
        category: CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_EXTRACT,
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
        category: CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_EXTRACT,
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
        category: CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_EXTRACT,
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
        category: CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_EXTRACT,
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
        category: CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_EXTRACT,
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
        category: CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_EXTRACT,
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
        category: CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_EXTRACT,
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
        category: CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_EXTRACT,
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
        category: CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_EXTRACT,
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
        category: CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_EXTRACT,
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
        category: CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_PREVIEW,
        description: "Master switch for read-only archive preview",
    },
    ConfigDef {
        key: ARCHIVE_PREVIEW_USER_ENABLED_KEY,
        label_i18n_key: "settings_item_archive_preview_user_enabled_label",
        description_i18n_key: "settings_item_archive_preview_user_enabled_desc",
        value_type: SystemConfigValueType::Boolean,
        default_fn: || "false".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_PREVIEW,
        description: "Allow signed-in users to preview archive manifests for personal and team files",
    },
    ConfigDef {
        key: ARCHIVE_PREVIEW_SHARE_ENABLED_KEY,
        label_i18n_key: "settings_item_archive_preview_share_enabled_label",
        description_i18n_key: "settings_item_archive_preview_share_enabled_desc",
        value_type: SystemConfigValueType::Boolean,
        default_fn: || "false".to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_PREVIEW,
        description: "Allow public share pages to preview archive manifests after share access checks",
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
        category: CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_PREVIEW,
        description: "Maximum source archive bytes accepted for read-only archive preview",
    },
    ConfigDef {
        key: ARCHIVE_PREVIEW_MAX_ENTRIES_KEY,
        label_i18n_key: "settings_item_archive_preview_max_entries_label",
        description_i18n_key: "settings_item_archive_preview_max_entries_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || crate::config::operations::DEFAULT_ARCHIVE_PREVIEW_MAX_ENTRIES.to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_PREVIEW,
        description: "Maximum number of archive entries accepted for archive preview",
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
        category: CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_PREVIEW,
        description: "Maximum serialized archive preview manifest bytes returned to clients",
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
        category: CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_PREVIEW,
        description: "Maximum wall-clock seconds allowed for one archive preview scan",
    },
    ConfigDef {
        key: ARCHIVE_BUILD_MAX_ENTRIES_KEY,
        label_i18n_key: "settings_item_archive_build_max_entries_label",
        description_i18n_key: "settings_item_archive_build_max_entries_desc",
        value_type: SystemConfigValueType::Number,
        default_fn: || crate::config::operations::DEFAULT_ARCHIVE_BUILD_MAX_ENTRIES.to_string(),
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_BUILD,
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
        category: CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_BUILD,
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
        category: CONFIG_CATEGORY_FILE_PROCESSING_ARCHIVE_BUILD,
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
        category: CONFIG_CATEGORY_FILE_PROCESSING_MEDIA,
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
        category: CONFIG_CATEGORY_FILE_PROCESSING_MEDIA,
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
        category: CONFIG_CATEGORY_FILE_PROCESSING_MEDIA,
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
        category: CONFIG_CATEGORY_FILE_PROCESSING_MEDIA,
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
        category: CONFIG_CATEGORY_USER_REGISTRATION,
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
        category: CONFIG_CATEGORY_USER_REGISTRATION,
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
        category: CONFIG_CATEGORY_USER_AVATAR,
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
        category: CONFIG_CATEGORY_USER_AVATAR,
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
        category: CONFIG_CATEGORY_USER_AVATAR,
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
        category: CONFIG_CATEGORY_AUDIT,
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
        category: CONFIG_CATEGORY_AUDIT,
        description: "Days before audit log entries are permanently deleted",
    },
    ConfigDef {
        key: AUDIT_LOG_RECORDED_ACTIONS_KEY,
        label_i18n_key: "settings_item_audit_log_recorded_actions_label",
        description_i18n_key: "settings_item_audit_log_recorded_actions_desc",
        value_type: SystemConfigValueType::StringEnumSet,
        default_fn: crate::config::audit::default_recorded_actions_value,
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_AUDIT,
        description: "Audit actions that should be recorded",
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
        category: CONFIG_CATEGORY_MAIL_CONFIG,
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
        category: CONFIG_CATEGORY_MAIL_CONFIG,
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
        category: CONFIG_CATEGORY_MAIL_CONFIG,
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
        category: CONFIG_CATEGORY_MAIL_CONFIG,
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
        category: CONFIG_CATEGORY_MAIL_CONFIG,
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
        category: CONFIG_CATEGORY_MAIL_CONFIG,
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
        category: CONFIG_CATEGORY_MAIL_CONFIG,
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
        category: CONFIG_CATEGORY_MAIL_TEMPLATE,
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
        category: CONFIG_CATEGORY_MAIL_TEMPLATE,
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
        category: CONFIG_CATEGORY_MAIL_TEMPLATE,
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
        category: CONFIG_CATEGORY_MAIL_TEMPLATE,
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
        category: CONFIG_CATEGORY_MAIL_TEMPLATE,
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
        category: CONFIG_CATEGORY_MAIL_TEMPLATE,
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
        category: CONFIG_CATEGORY_MAIL_TEMPLATE,
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
        category: CONFIG_CATEGORY_MAIL_TEMPLATE,
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
        category: CONFIG_CATEGORY_MAIL_TEMPLATE,
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
        category: CONFIG_CATEGORY_MAIL_TEMPLATE,
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
        category: CONFIG_CATEGORY_MAIL_TEMPLATE,
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
        category: CONFIG_CATEGORY_MAIL_TEMPLATE,
        description: "HTML template for external auth email verification emails. Prefer a complete HTML document for best client compatibility",
    },
    ConfigDef {
        key: MAIL_TEMPLATE_LOGIN_EMAIL_CODE_SUBJECT_KEY,
        label_i18n_key: "settings_item_mail_template_login_email_code_subject_label",
        description_i18n_key: "settings_item_mail_template_login_email_code_subject_desc",
        value_type: SystemConfigValueType::String,
        default_fn: || {
            crate::config::mail::default_template_subject(
                crate::types::MailTemplateCode::LoginEmailCode,
            )
            .to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_MAIL_TEMPLATE,
        description: "Subject template for login email code messages",
    },
    ConfigDef {
        key: MAIL_TEMPLATE_LOGIN_EMAIL_CODE_HTML_KEY,
        label_i18n_key: "settings_item_mail_template_login_email_code_html_label",
        description_i18n_key: "settings_item_mail_template_login_email_code_html_desc",
        value_type: SystemConfigValueType::Multiline,
        default_fn: || {
            crate::config::mail::default_template_html(
                crate::types::MailTemplateCode::LoginEmailCode,
            )
            .to_string()
        },
        requires_restart: false,
        is_sensitive: false,
        category: CONFIG_CATEGORY_MAIL_TEMPLATE,
        description: "HTML template for login email code messages. Prefer a complete HTML document for best client compatibility",
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
        category: CONFIG_CATEGORY_SITE,
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
        category: CONFIG_CATEGORY_SITE,
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
        category: CONFIG_CATEGORY_SITE,
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
        category: CONFIG_CATEGORY_SITE,
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
        category: CONFIG_CATEGORY_SITE,
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
        category: CONFIG_CATEGORY_SITE,
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
        category: CONFIG_CATEGORY_SITE_PREVIEW,
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
        category: CONFIG_CATEGORY_SITE_PREVIEW,
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
        category: CONFIG_CATEGORY_SITE_PREVIEW,
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
        category: CONFIG_CATEGORY_SITE_PREVIEW,
        description: "Public preview app registry used by the web frontend, including extension bindings",
    },
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::Value;

    use super::{ALL_CONFIGS, SYSTEM_CONFIG_ALLOWED_CATEGORIES};

    #[test]
    fn all_config_categories_are_registered() {
        let allowed_categories = SYSTEM_CONFIG_ALLOWED_CATEGORIES
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let duplicate_allowed_categories = duplicate_values(SYSTEM_CONFIG_ALLOWED_CATEGORIES);
        let unknown_categories = ALL_CONFIGS
            .iter()
            .map(|def| def.category)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|category| !allowed_categories.contains(category))
            .collect::<Vec<_>>();

        assert!(
            duplicate_allowed_categories.is_empty(),
            "duplicate system_config allowed categories: {duplicate_allowed_categories:?}"
        );
        assert!(
            unknown_categories.is_empty(),
            "unknown system_config categories: {unknown_categories:?}"
        );
    }

    #[test]
    fn second_level_config_categories_have_frontend_i18n_text() {
        let zh_admin_common = parse_admin_common_locale(
            "zh",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/frontend-panel/src/i18n/locales/zh/admin/settings-common.json"
            )),
        );
        let en_admin_common = parse_admin_common_locale(
            "en",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/frontend-panel/src/i18n/locales/en/admin/settings-common.json"
            )),
        );
        let second_level_categories = ALL_CONFIGS
            .iter()
            .filter_map(|def| subcategory_i18n_key(def.category))
            .collect::<BTreeSet<_>>();

        let missing_keys = second_level_categories
            .iter()
            .flat_map(|base_key| [base_key.clone(), format!("{base_key}_desc")])
            .flat_map(|key| {
                let mut missing = Vec::new();
                if !zh_admin_common.contains(&key) {
                    missing.push(format!("zh:{key}"));
                }
                if !en_admin_common.contains(&key) {
                    missing.push(format!("en:{key}"));
                }
                missing
            })
            .collect::<Vec<_>>();

        assert!(
            missing_keys.is_empty(),
            "missing frontend i18n for second-level system_config categories: {missing_keys:?}"
        );
    }

    fn duplicate_values(values: &[&'static str]) -> Vec<&'static str> {
        let mut seen = BTreeSet::new();
        values
            .iter()
            .copied()
            .filter(|value| !seen.insert(*value))
            .collect()
    }

    fn subcategory_i18n_key(category: &str) -> Option<String> {
        let (root, subcategory) = category.split_once('.')?;
        Some(format!(
            "settings_subcategory_{root}_{}",
            subcategory.replace('.', "_")
        ))
    }

    fn parse_admin_common_locale(lang: &str, json: &str) -> BTreeSet<String> {
        let value = serde_json::from_str::<Value>(json)
            .unwrap_or_else(|err| panic!("{lang} settings-common.json must be valid JSON: {err}"));
        value
            .as_object()
            .unwrap_or_else(|| panic!("{lang} settings-common.json must be a JSON object"))
            .keys()
            .cloned()
            .collect()
    }
}
