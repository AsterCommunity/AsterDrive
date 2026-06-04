//! 配置子模块：`system_config`。

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use crate::config::RuntimeConfig;
use crate::config::audit;
use crate::config::auth_runtime;
use crate::config::avatar;
use crate::config::bool_like::parse_bool_like;
use crate::config::branding;
use crate::config::cors;
use crate::config::definitions::{ALL_CONFIGS, ConfigDef};
use crate::config::local_email_policy;
use crate::config::mail;
use crate::config::media_processing;
use crate::config::offline_download;
use crate::config::operations;
use crate::config::site_url;
use crate::config::wopi;
use crate::entities::system_config;
use crate::errors::{AsterError, Result};
use crate::services::preview_app_service;
use crate::types::{SystemConfigSource, SystemConfigValueType};

pub trait SystemConfigValueLookup {
    fn get_config_value(&self, key: &str) -> Option<String>;
}

impl SystemConfigValueLookup for RuntimeConfig {
    fn get_config_value(&self, key: &str) -> Option<String> {
        self.get(key)
    }
}

impl<T> SystemConfigValueLookup for Arc<T>
where
    T: SystemConfigValueLookup + ?Sized,
{
    fn get_config_value(&self, key: &str) -> Option<String> {
        self.as_ref().get_config_value(key)
    }
}

impl SystemConfigValueLookup for HashMap<String, String> {
    fn get_config_value(&self, key: &str) -> Option<String> {
        self.get(key).cloned()
    }
}

impl SystemConfigValueLookup for BTreeMap<String, String> {
    fn get_config_value(&self, key: &str) -> Option<String> {
        self.get(key).cloned()
    }
}

pub fn get_definition(key: &str) -> Option<&'static ConfigDef> {
    ALL_CONFIGS.iter().find(|def| def.key == key)
}

pub fn validate_value_type(value_type: SystemConfigValueType, value: &str) -> Result<()> {
    let trimmed = value.trim();
    match value_type {
        SystemConfigValueType::Boolean => {
            if trimmed != "true" && trimmed != "false" {
                return Err(AsterError::validation_error(
                    "boolean config must be 'true' or 'false'",
                ));
            }
        }
        SystemConfigValueType::Number => {
            if trimmed.parse::<f64>().is_err() {
                return Err(AsterError::validation_error(
                    "number config must be a valid number",
                ));
            }
        }
        SystemConfigValueType::StringArray | SystemConfigValueType::StringEnumSet => {
            serde_json::from_str::<Vec<String>>(trimmed).map_err(|err| {
                AsterError::validation_error(format!(
                    "{} config must be a JSON array of strings: {err}",
                    value_type.as_str()
                ))
            })?;
        }
        SystemConfigValueType::String | SystemConfigValueType::Multiline => {}
    }
    Ok(())
}

pub fn normalize_system_value<L>(lookup: &L, key: &str, value: &str) -> Result<String>
where
    L: SystemConfigValueLookup + ?Sized,
{
    match key {
        avatar::AVATAR_DIR_KEY => avatar::normalize_avatar_dir_config_value(value),
        audit::AUDIT_LOG_RECORDED_ACTIONS_KEY => {
            audit::normalize_recorded_actions_config_value(value)
        }
        auth_runtime::AUTH_COOKIE_SECURE_KEY => {
            auth_runtime::normalize_cookie_secure_config_value(value)
        }
        auth_runtime::AUTH_ALLOW_USER_REGISTRATION_KEY => {
            auth_runtime::normalize_allow_user_registration_config_value(value)
        }
        auth_runtime::AUTH_REGISTER_ACTIVATION_ENABLED_KEY => {
            auth_runtime::normalize_register_activation_enabled_config_value(value)
        }
        auth_runtime::AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY
        | auth_runtime::AUTH_EMAIL_CODE_LOGIN_ALLOW_TOTP_FALLBACK_KEY
        | auth_runtime::AUTH_PASSKEY_LOGIN_ENABLED_KEY => {
            auth_runtime::normalize_email_code_login_bool_config_value(key, value)
        }
        auth_runtime::AUTH_ACCESS_TOKEN_TTL_SECS_KEY
        | auth_runtime::AUTH_REFRESH_TOKEN_TTL_SECS_KEY
        | auth_runtime::AUTH_REGISTER_ACTIVATION_TTL_SECS_KEY
        | auth_runtime::AUTH_CONTACT_CHANGE_TTL_SECS_KEY
        | auth_runtime::AUTH_PASSWORD_RESET_TTL_SECS_KEY
        | auth_runtime::AUTH_EMAIL_CODE_LOGIN_TTL_SECS_KEY
        | auth_runtime::AUTH_EMAIL_CODE_LOGIN_RESEND_COOLDOWN_SECS_KEY
        | auth_runtime::AUTH_PASSWORD_RESET_REQUEST_COOLDOWN_SECS_KEY
        | auth_runtime::AUTH_CONTACT_VERIFICATION_RESEND_COOLDOWN_SECS_KEY => {
            auth_runtime::normalize_token_ttl_config_value(key, value)
        }
        local_email_policy::AUTH_LOCAL_EMAIL_ALLOWLIST_KEY
        | local_email_policy::AUTH_LOCAL_EMAIL_BLOCKLIST_KEY => {
            local_email_policy::normalize_local_email_policy_config_value(key, value)
        }
        cors::CORS_ENABLED_KEY => cors::normalize_enabled_config_value(value),
        cors::CORS_ALLOWED_ORIGINS_KEY => {
            let normalized = cors::normalize_allowed_origins_config_value(value)?;
            let parsed = cors::parse_allowed_origins_value(&normalized)?;
            let allow_credentials = lookup
                .get_config_value(cors::CORS_ALLOW_CREDENTIALS_KEY)
                .and_then(|raw| parse_bool_like(&raw))
                .unwrap_or(cors::DEFAULT_CORS_ALLOW_CREDENTIALS);
            cors::validate_runtime_cors_combination(&parsed, allow_credentials)?;
            Ok(normalized)
        }
        cors::CORS_ALLOW_CREDENTIALS_KEY => {
            let normalized = cors::normalize_allow_credentials_config_value(value)?;
            let allow_credentials = normalized == "true";
            let current_origins = lookup
                .get_config_value(cors::CORS_ALLOWED_ORIGINS_KEY)
                .unwrap_or_default();
            let parsed = cors::parse_allowed_origins_value(&current_origins)?;
            cors::validate_runtime_cors_combination(&parsed, allow_credentials)?;
            Ok(normalized)
        }
        cors::CORS_MAX_AGE_SECS_KEY => cors::normalize_max_age_config_value(value),
        operations::MAIL_OUTBOX_DISPATCH_INTERVAL_SECS_KEY
        | operations::BACKGROUND_TASK_DISPATCH_INTERVAL_SECS_KEY
        | operations::BACKGROUND_TASK_DISPATCH_IDLE_MAX_INTERVAL_SECS_KEY
        | operations::MAINTENANCE_CLEANUP_INTERVAL_SECS_KEY
        | operations::BLOB_RECONCILE_INTERVAL_SECS_KEY
        | operations::REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS_KEY
        | operations::ARCHIVE_EXTRACT_MAX_DURATION_SECS_KEY
        | operations::OFFLINE_DOWNLOAD_REQUEST_TIMEOUT_SECS_KEY
        | operations::OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS_KEY => {
            operations::normalize_interval_config_value(key, value)
        }
        operations::OFFLINE_DOWNLOAD_ENGINE_KEY => {
            operations::normalize_offline_download_engine_config_value(value)
        }
        operations::OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY => {
            operations::normalize_offline_download_aria2_rpc_url_config_value(value)
        }
        operations::OFFLINE_DOWNLOAD_TEMP_DIR_KEY => {
            operations::normalize_offline_download_temp_dir_config_value(value)
        }
        operations::OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY => Ok(value.trim().to_string()),
        operations::SHARE_STREAM_SESSION_TTL_SECS_KEY => {
            operations::normalize_share_stream_session_ttl_config_value(key, value)
        }
        operations::BACKGROUND_TASK_MAX_CONCURRENCY_KEY
        | operations::BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY_KEY
        | operations::BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY_KEY
        | operations::BACKGROUND_TASK_STORAGE_MIGRATION_MAX_CONCURRENCY_KEY
        | operations::OFFLINE_DOWNLOAD_MAX_CONCURRENCY_KEY => {
            operations::normalize_concurrency_config_value(key, value)
        }
        operations::SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY_KEY => {
            operations::normalize_queue_capacity_config_value(key, value)
        }
        operations::BACKGROUND_TASK_MAX_ATTEMPTS_KEY => {
            operations::normalize_attempts_config_value(key, value)
        }
        operations::TEAM_MEMBER_LIST_MAX_LIMIT_KEY | operations::TASK_LIST_MAX_LIMIT_KEY => {
            operations::normalize_list_max_limit_config_value(key, value)
        }
        operations::MEDIA_METADATA_ENABLED_KEY => {
            operations::normalize_bool_config_value(key, value)
        }
        operations::AVATAR_MAX_UPLOAD_SIZE_BYTES_KEY
        | operations::MEDIA_METADATA_MAX_SOURCE_BYTES_KEY
        | operations::ARCHIVE_EXTRACT_MAX_SOURCE_BYTES_KEY
        | operations::ARCHIVE_EXTRACT_MAX_STAGING_BYTES_KEY
        | operations::ARCHIVE_EXTRACT_MAX_UNCOMPRESSED_BYTES_KEY
        | operations::ARCHIVE_EXTRACT_MAX_ENTRIES_KEY
        | operations::ARCHIVE_EXTRACT_MAX_FILES_KEY
        | operations::ARCHIVE_EXTRACT_MAX_DIRECTORIES_KEY
        | operations::ARCHIVE_EXTRACT_MAX_DEPTH_KEY
        | operations::ARCHIVE_EXTRACT_MAX_PATH_BYTES_KEY
        | operations::ARCHIVE_EXTRACT_MAX_COMPRESSION_RATIO_KEY
        | operations::ARCHIVE_EXTRACT_MAX_ENTRY_COMPRESSION_RATIO_KEY
        | operations::ARCHIVE_BUILD_MAX_ENTRIES_KEY
        | operations::ARCHIVE_BUILD_MAX_TOTAL_SOURCE_BYTES_KEY
        | operations::ARCHIVE_BUILD_MAX_TEMP_BYTES_KEY
        | operations::OFFLINE_DOWNLOAD_ARIA2_SPLIT_KEY
        | operations::OFFLINE_DOWNLOAD_ARIA2_MAX_CONNECTION_PER_SERVER_KEY
        | operations::OFFLINE_DOWNLOAD_MAX_FILE_SIZE_BYTES_KEY
        | operations::THUMBNAIL_MAX_SOURCE_BYTES_KEY => {
            operations::normalize_bytes_config_value(key, value)
        }
        operations::OFFLINE_DOWNLOAD_MAX_MB_PER_SEC_KEY
        | operations::OFFLINE_DOWNLOAD_ARIA2_LOWEST_SPEED_LIMIT_BYTES_PER_SEC_KEY => {
            operations::normalize_non_negative_u64_config_value(key, value)
        }
        media_processing::MEDIA_PROCESSING_REGISTRY_JSON_KEY => {
            media_processing::normalize_media_processing_registry_config_value(value)
        }
        operations::OFFLINE_DOWNLOAD_ENGINE_REGISTRY_JSON_KEY => {
            offline_download::normalize_offline_download_engine_registry_config_value(value)
        }
        mail::MAIL_SMTP_HOST_KEY => mail::normalize_smtp_host_config_value(value),
        mail::MAIL_SMTP_PORT_KEY => mail::normalize_smtp_port_config_value(value),
        mail::MAIL_FROM_ADDRESS_KEY => mail::normalize_mail_address_config_value(value),
        mail::MAIL_FROM_NAME_KEY => mail::normalize_mail_name_config_value(value),
        mail::MAIL_SECURITY_KEY => mail::normalize_mail_security_config_value(value),
        mail::MAIL_TEMPLATE_REGISTER_ACTIVATION_SUBJECT_KEY
        | mail::MAIL_TEMPLATE_CONTACT_CHANGE_CONFIRMATION_SUBJECT_KEY
        | mail::MAIL_TEMPLATE_PASSWORD_RESET_SUBJECT_KEY
        | mail::MAIL_TEMPLATE_PASSWORD_RESET_NOTICE_SUBJECT_KEY
        | mail::MAIL_TEMPLATE_CONTACT_CHANGE_NOTICE_SUBJECT_KEY
        | mail::MAIL_TEMPLATE_EXTERNAL_AUTH_EMAIL_VERIFICATION_SUBJECT_KEY
        | mail::MAIL_TEMPLATE_LOGIN_EMAIL_CODE_SUBJECT_KEY => {
            mail::normalize_mail_template_subject_config_value(key, value)
        }
        mail::MAIL_TEMPLATE_REGISTER_ACTIVATION_HTML_KEY
        | mail::MAIL_TEMPLATE_CONTACT_CHANGE_CONFIRMATION_HTML_KEY
        | mail::MAIL_TEMPLATE_PASSWORD_RESET_HTML_KEY
        | mail::MAIL_TEMPLATE_PASSWORD_RESET_NOTICE_HTML_KEY
        | mail::MAIL_TEMPLATE_CONTACT_CHANGE_NOTICE_HTML_KEY
        | mail::MAIL_TEMPLATE_EXTERNAL_AUTH_EMAIL_VERIFICATION_HTML_KEY
        | mail::MAIL_TEMPLATE_LOGIN_EMAIL_CODE_HTML_KEY => {
            mail::normalize_mail_template_body_config_value(key, value)
        }
        site_url::PUBLIC_SITE_URL_KEY => site_url::normalize_public_site_url_config_value(value),
        branding::BRANDING_TITLE_KEY => branding::normalize_title_config_value(value),
        branding::BRANDING_DESCRIPTION_KEY => branding::normalize_description_config_value(value),
        branding::BRANDING_FAVICON_URL_KEY => branding::normalize_favicon_url_config_value(value),
        branding::BRANDING_WORDMARK_DARK_URL_KEY => {
            branding::normalize_wordmark_dark_url_config_value(value)
        }
        branding::BRANDING_WORDMARK_LIGHT_URL_KEY => {
            branding::normalize_wordmark_light_url_config_value(value)
        }
        preview_app_service::PREVIEW_APPS_CONFIG_KEY => {
            preview_app_service::normalize_public_preview_apps_config_value(value)
        }
        wopi::WOPI_ACCESS_TOKEN_TTL_SECS_KEY
        | wopi::WOPI_LOCK_TTL_SECS_KEY
        | wopi::WOPI_DISCOVERY_CACHE_TTL_SECS_KEY => wopi::normalize_ttl_config_value(key, value),
        _ => Ok(value.to_string()),
    }
}

pub fn apply_definition(mut config: system_config::Model) -> system_config::Model {
    if config.source != SystemConfigSource::System {
        return config;
    }

    let Some(def) = get_definition(&config.key) else {
        return config;
    };

    config.value_type = def.value_type;
    config.requires_restart = def.requires_restart;
    config.is_sensitive = def.is_sensitive;
    config.category = def.category.to_string();
    config.description = def.description.to_string();
    config
}

#[cfg(test)]
mod tests {
    use super::{apply_definition, normalize_system_value, validate_value_type};
    use crate::config::operations::{
        BACKGROUND_TASK_MAX_CONCURRENCY_KEY, DEFAULT_SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY,
        MAX_SHARE_STREAM_SESSION_TTL_SECS, MIN_SHARE_STREAM_SESSION_TTL_SECS,
        SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY_KEY, SHARE_STREAM_SESSION_TTL_SECS_KEY,
    };
    use crate::entities::system_config;
    use crate::types::{SystemConfigSource, SystemConfigValueType};
    use chrono::Utc;
    use std::collections::HashMap;

    fn model(key: &str, value: &str, source: SystemConfigSource) -> system_config::Model {
        system_config::Model {
            id: 1,
            key: key.to_string(),
            value: value.to_string(),
            value_type: SystemConfigValueType::String,
            requires_restart: false,
            is_sensitive: false,
            source,
            visibility: crate::types::SystemConfigVisibility::Private,
            namespace: String::new(),
            category: String::new(),
            description: String::new(),
            updated_at: Utc::now(),
            updated_by: None,
        }
    }

    #[test]
    fn validate_value_type_enforces_declared_types() {
        assert!(validate_value_type(SystemConfigValueType::Boolean, "true").is_ok());
        assert!(validate_value_type(SystemConfigValueType::Boolean, " yes ").is_err());
        assert!(validate_value_type(SystemConfigValueType::Number, "42").is_ok());
        assert!(validate_value_type(SystemConfigValueType::Number, "nope").is_err());
        assert!(validate_value_type(SystemConfigValueType::StringArray, r#"["a"]"#).is_ok());
        assert!(validate_value_type(SystemConfigValueType::StringArray, r#""a""#).is_err());
        assert!(validate_value_type(SystemConfigValueType::StringEnumSet, r#"["a"]"#).is_ok());
        assert!(validate_value_type(SystemConfigValueType::StringEnumSet, r#""a""#).is_err());
    }

    #[test]
    fn normalize_system_value_validates_audit_action_scope() {
        let lookup = HashMap::new();
        assert_eq!(
            normalize_system_value(
                &lookup,
                crate::config::audit::AUDIT_LOG_RECORDED_ACTIONS_KEY,
                r#"["file_upload","user_login"]"#
            )
            .unwrap(),
            r#"["file_upload","user_login"]"#
        );
        assert!(
            normalize_system_value(
                &lookup,
                crate::config::audit::AUDIT_LOG_RECORDED_ACTIONS_KEY,
                r#"["unknown_action"]"#
            )
            .is_err()
        );
        assert!(
            normalize_system_value(
                &lookup,
                crate::config::audit::AUDIT_LOG_RECORDED_ACTIONS_KEY,
                r#"["user_login","user_login"]"#
            )
            .is_err()
        );
        assert_eq!(
            normalize_system_value(
                &lookup,
                crate::config::audit::AUDIT_LOG_RECORDED_ACTIONS_KEY,
                "[]"
            )
            .unwrap(),
            "[]"
        );
    }

    #[test]
    fn normalize_system_value_uses_capacity_limit_for_share_download_rollback_queue() {
        let lookup = HashMap::new();
        assert_eq!(
            normalize_system_value(
                &lookup,
                SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY_KEY,
                &DEFAULT_SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY.to_string(),
            )
            .unwrap(),
            DEFAULT_SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY.to_string()
        );
        assert!(
            normalize_system_value(&lookup, BACKGROUND_TASK_MAX_CONCURRENCY_KEY, "1024").is_err()
        );
    }

    #[test]
    fn normalize_system_value_uses_lookup_for_cross_field_validation() {
        let lookup = HashMap::from([("cors_allow_credentials".to_string(), "true".to_string())]);

        let err = normalize_system_value(&lookup, "cors_allowed_origins", "*").unwrap_err();
        assert!(
            err.message()
                .contains("cors_allow_credentials cannot be true when cors_allowed_origins is '*'")
        );
    }

    #[test]
    fn normalize_system_value_enforces_share_stream_session_ttl_bounds() {
        let lookup = HashMap::new();

        assert_eq!(
            normalize_system_value(
                &lookup,
                SHARE_STREAM_SESSION_TTL_SECS_KEY,
                &MIN_SHARE_STREAM_SESSION_TTL_SECS.to_string(),
            )
            .unwrap(),
            MIN_SHARE_STREAM_SESSION_TTL_SECS.to_string()
        );
        assert_eq!(
            normalize_system_value(
                &lookup,
                SHARE_STREAM_SESSION_TTL_SECS_KEY,
                &MAX_SHARE_STREAM_SESSION_TTL_SECS.to_string(),
            )
            .unwrap(),
            MAX_SHARE_STREAM_SESSION_TTL_SECS.to_string()
        );
        assert!(
            normalize_system_value(
                &lookup,
                SHARE_STREAM_SESSION_TTL_SECS_KEY,
                &(MIN_SHARE_STREAM_SESSION_TTL_SECS - 1).to_string(),
            )
            .is_err()
        );
        assert!(
            normalize_system_value(
                &lookup,
                SHARE_STREAM_SESSION_TTL_SECS_KEY,
                &(MAX_SHARE_STREAM_SESSION_TTL_SECS + 1).to_string(),
            )
            .is_err()
        );
    }

    #[test]
    fn apply_definition_overlays_schema_metadata_for_system_rows() {
        let config = apply_definition(model(
            "public_site_url",
            r#"["https://drive.example.com"]"#,
            SystemConfigSource::System,
        ));
        assert_eq!(config.value_type, SystemConfigValueType::StringArray);
        assert_eq!(
            config.category,
            crate::config::definitions::CONFIG_CATEGORY_SITE
        );
        assert!(
            config
                .description
                .contains("share, preview, WebDAV, WOPI, and callback URLs")
        );

        let custom = apply_definition(model("custom.demo", "value", SystemConfigSource::Custom));
        assert_eq!(custom.category, "");
    }
}
