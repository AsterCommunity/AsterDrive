//! 配置子模块：`system_config`。

use crate::config::RuntimeConfig;
use crate::config::definitions::CONFIG_REGISTRY;
use aster_forge_config::ConfigSource;
use aster_forge_config::{ConfigValueLookup, StoredConfig};
use aster_forge_db::system_config;

impl ConfigValueLookup for RuntimeConfig {
    fn get_config_value(&self, key: &str) -> Option<String> {
        self.get(key)
    }
}

pub fn apply_definition(mut config: system_config::Model) -> system_config::Model {
    if config.source != ConfigSource::System {
        return config;
    }

    let stored = CONFIG_REGISTRY.apply_definition(model_to_stored_config(&config));
    config.value_type = stored.value_type;
    config.requires_restart = stored.requires_restart;
    config.is_sensitive = stored.is_sensitive;
    config.visibility = stored.visibility;
    config.category = stored.category;
    config.description = stored.description;
    config
}

fn model_to_stored_config(config: &system_config::Model) -> StoredConfig {
    StoredConfig {
        id: config.id,
        key: config.key.clone(),
        value: config.value.clone(),
        value_type: config.value_type,
        requires_restart: config.requires_restart,
        is_sensitive: config.is_sensitive,
        source: config.source,
        visibility: config.visibility,
        category: config.category.clone(),
        description: config.description.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::apply_definition;
    use crate::config::auth_runtime::AUTH_USER_INVITATION_TTL_SECS_KEY;
    use crate::config::definitions::CONFIG_REGISTRY;
    use crate::config::mail::{
        MAIL_TEMPLATE_USER_INVITATION_HTML_KEY, MAIL_TEMPLATE_USER_INVITATION_SUBJECT_KEY,
    };
    use crate::config::operations::{
        ARCHIVE_COMPRESS_ENABLED_KEY, BACKGROUND_TASK_MAX_CONCURRENCY_KEY,
        DEFAULT_SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY, IMAGE_PREVIEW_MAX_DIMENSION_KEY,
        MAX_DERIVATIVE_MAX_DIMENSION, MAX_SHARE_STREAM_SESSION_TTL_SECS,
        MIN_SHARE_STREAM_SESSION_TTL_SECS, SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY_KEY,
        SHARE_STREAM_SESSION_TTL_SECS_KEY, THUMBNAIL_MAX_DIMENSION_KEY,
    };
    use aster_forge_config::{ConfigSource, ConfigValueType};
    use aster_forge_db::system_config;
    use chrono::Utc;
    use std::collections::HashMap;

    fn model(key: &str, value: &str, source: ConfigSource) -> system_config::Model {
        system_config::Model {
            id: 1,
            key: key.to_string(),
            value: value.to_string(),
            value_type: ConfigValueType::String,
            requires_restart: false,
            is_sensitive: false,
            source,
            visibility: aster_forge_config::ConfigVisibility::Private,
            namespace: String::new(),
            category: String::new(),
            description: String::new(),
            updated_at: Utc::now(),
            updated_by: None,
        }
    }

    #[test]
    fn validate_value_type_enforces_declared_types() {
        assert!(
            aster_forge_config::validate_storage_value(ConfigValueType::Boolean, "true").is_ok()
        );
        assert!(
            aster_forge_config::validate_storage_value(ConfigValueType::Boolean, " yes ").is_err()
        );
        assert!(aster_forge_config::validate_storage_value(ConfigValueType::Number, "42").is_ok());
        assert!(
            aster_forge_config::validate_storage_value(ConfigValueType::Number, "nope").is_err()
        );
        assert!(
            aster_forge_config::validate_storage_value(ConfigValueType::StringArray, r#"["a"]"#)
                .is_ok()
        );
        assert!(
            aster_forge_config::validate_storage_value(ConfigValueType::StringArray, r#""a""#)
                .is_err()
        );
        assert!(
            aster_forge_config::validate_storage_value(ConfigValueType::StringEnumSet, r#"["a"]"#)
                .is_ok()
        );
        assert!(
            aster_forge_config::validate_storage_value(ConfigValueType::StringEnumSet, r#""a""#)
                .is_err()
        );
    }

    #[test]
    fn normalize_system_value_validates_audit_action_scope() {
        let lookup = HashMap::new();
        assert_eq!(
            CONFIG_REGISTRY
                .normalize_value(
                    &lookup,
                    crate::config::audit::AUDIT_LOG_RECORDED_ACTIONS_KEY,
                    r#"["file_upload","user_login"]"#
                )
                .unwrap(),
            r#"["file_upload","user_login"]"#
        );
        assert!(
            CONFIG_REGISTRY
                .normalize_value(
                    &lookup,
                    crate::config::audit::AUDIT_LOG_RECORDED_ACTIONS_KEY,
                    r#"["unknown_action"]"#
                )
                .is_err()
        );
        assert!(
            CONFIG_REGISTRY
                .normalize_value(
                    &lookup,
                    crate::config::audit::AUDIT_LOG_RECORDED_ACTIONS_KEY,
                    r#"["user_login","user_login"]"#
                )
                .is_err()
        );
        assert_eq!(
            CONFIG_REGISTRY
                .normalize_value(
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
            CONFIG_REGISTRY
                .normalize_value(
                    &lookup,
                    SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY_KEY,
                    &DEFAULT_SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY.to_string(),
                )
                .unwrap(),
            DEFAULT_SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY.to_string()
        );
        assert!(
            CONFIG_REGISTRY
                .normalize_value(&lookup, BACKGROUND_TASK_MAX_CONCURRENCY_KEY, "1024")
                .is_err()
        );
    }

    #[test]
    fn normalize_system_value_enforces_boolean_storage_contract() {
        let lookup = HashMap::new();
        assert_eq!(
            CONFIG_REGISTRY
                .normalize_value(&lookup, ARCHIVE_COMPRESS_ENABLED_KEY, "true")
                .unwrap(),
            "true"
        );
        assert_eq!(
            CONFIG_REGISTRY
                .normalize_value(&lookup, ARCHIVE_COMPRESS_ENABLED_KEY, "false")
                .unwrap(),
            "false"
        );
        for invalid in [" yes ", "off", "sometimes", "1"] {
            assert!(
                CONFIG_REGISTRY
                    .normalize_value(&lookup, ARCHIVE_COMPRESS_ENABLED_KEY, invalid)
                    .is_err(),
                "{invalid:?} must not bypass Forge boolean storage validation"
            );
        }
    }

    #[test]
    fn normalize_system_value_validates_derivative_dimensions() {
        let lookup = HashMap::new();

        assert_eq!(
            CONFIG_REGISTRY
                .normalize_value(&lookup, THUMBNAIL_MAX_DIMENSION_KEY, "1")
                .unwrap(),
            "1"
        );
        assert_eq!(
            CONFIG_REGISTRY
                .normalize_value(&lookup, THUMBNAIL_MAX_DIMENSION_KEY, " 320 ")
                .unwrap(),
            "320"
        );
        assert_eq!(
            CONFIG_REGISTRY
                .normalize_value(
                    &lookup,
                    IMAGE_PREVIEW_MAX_DIMENSION_KEY,
                    &MAX_DERIVATIVE_MAX_DIMENSION.to_string(),
                )
                .unwrap(),
            MAX_DERIVATIVE_MAX_DIMENSION.to_string()
        );
        for invalid in ["0", "-1", "12.5", "abc", ""] {
            assert!(
                CONFIG_REGISTRY
                    .normalize_value(&lookup, THUMBNAIL_MAX_DIMENSION_KEY, invalid)
                    .is_err(),
                "{invalid:?} should be rejected"
            );
        }
        assert!(
            CONFIG_REGISTRY
                .normalize_value(
                    &lookup,
                    IMAGE_PREVIEW_MAX_DIMENSION_KEY,
                    &(u64::from(MAX_DERIVATIVE_MAX_DIMENSION) + 1).to_string(),
                )
                .is_err()
        );
    }

    #[test]
    fn normalize_system_value_enforces_user_invitation_ttl_is_positive() {
        let lookup = HashMap::new();

        assert_eq!(
            CONFIG_REGISTRY
                .normalize_value(&lookup, AUTH_USER_INVITATION_TTL_SECS_KEY, " 3600 ")
                .unwrap(),
            "3600"
        );
        assert!(
            CONFIG_REGISTRY
                .normalize_value(&lookup, AUTH_USER_INVITATION_TTL_SECS_KEY, "0")
                .is_err()
        );
        assert!(
            CONFIG_REGISTRY
                .normalize_value(&lookup, AUTH_USER_INVITATION_TTL_SECS_KEY, "-1")
                .is_err()
        );
        assert!(
            CONFIG_REGISTRY
                .normalize_value(&lookup, AUTH_USER_INVITATION_TTL_SECS_KEY, "forever")
                .is_err()
        );
    }

    #[test]
    fn normalize_system_value_validates_user_invitation_mail_templates() {
        let lookup = HashMap::new();

        assert!(
            CONFIG_REGISTRY
                .normalize_value(
                    &lookup,
                    MAIL_TEMPLATE_USER_INVITATION_SUBJECT_KEY,
                    "Invite\n{{email}}"
                )
                .is_err()
        );
        assert_eq!(
            CONFIG_REGISTRY
                .normalize_value(
                    &lookup,
                    MAIL_TEMPLATE_USER_INVITATION_HTML_KEY,
                    "<p>line1\r\nline2</p>"
                )
                .unwrap(),
            "<p>line1\nline2</p>"
        );
    }

    #[test]
    fn normalize_system_value_uses_lookup_for_cross_field_validation() {
        let lookup = HashMap::from([("cors_allow_credentials".to_string(), "true".to_string())]);

        let err = CONFIG_REGISTRY
            .normalize_value(&lookup, "cors_allowed_origins", "*")
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("cors_allow_credentials cannot be true when cors_allowed_origins is '*'")
        );
    }

    #[test]
    fn normalize_system_value_enforces_share_stream_session_ttl_bounds() {
        let lookup = HashMap::new();

        assert_eq!(
            CONFIG_REGISTRY
                .normalize_value(
                    &lookup,
                    SHARE_STREAM_SESSION_TTL_SECS_KEY,
                    &MIN_SHARE_STREAM_SESSION_TTL_SECS.to_string(),
                )
                .unwrap(),
            MIN_SHARE_STREAM_SESSION_TTL_SECS.to_string()
        );
        assert_eq!(
            CONFIG_REGISTRY
                .normalize_value(
                    &lookup,
                    SHARE_STREAM_SESSION_TTL_SECS_KEY,
                    &MAX_SHARE_STREAM_SESSION_TTL_SECS.to_string(),
                )
                .unwrap(),
            MAX_SHARE_STREAM_SESSION_TTL_SECS.to_string()
        );
        assert!(
            CONFIG_REGISTRY
                .normalize_value(
                    &lookup,
                    SHARE_STREAM_SESSION_TTL_SECS_KEY,
                    &(MIN_SHARE_STREAM_SESSION_TTL_SECS - 1).to_string(),
                )
                .is_err()
        );
        assert!(
            CONFIG_REGISTRY
                .normalize_value(
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
            ConfigSource::System,
        ));
        assert_eq!(config.value_type, ConfigValueType::StringArray);
        assert_eq!(
            config.category,
            crate::config::definitions::CONFIG_CATEGORY_SITE
        );
        assert!(
            config
                .description
                .contains("share, preview, WebDAV, WOPI, and callback URLs")
        );

        let custom = apply_definition(model("custom.demo", "value", ConfigSource::Custom));
        assert_eq!(custom.category, "");
    }
}
