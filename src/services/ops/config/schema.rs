use crate::config::definitions::{
    ALL_CONFIGS, AUDIT_LOG_RECORDED_ACTIONS_KEY, AUTH_ALLOW_USER_REGISTRATION_KEY,
    BRANDING_DESCRIPTION_KEY, BRANDING_FAVICON_URL_KEY, BRANDING_TITLE_KEY,
    BRANDING_WORDMARK_DARK_URL_KEY, BRANDING_WORDMARK_LIGHT_URL_KEY, MAIL_SMTP_HOST_KEY,
    MEDIA_METADATA_ENABLED_KEY, MEDIA_METADATA_MAX_SOURCE_BYTES_KEY, PUBLIC_SITE_URL_KEY,
};
use crate::config::media_processing::MEDIA_PROCESSING_REGISTRY_JSON_KEY;
use crate::config::operations::{
    FRONTEND_IMAGE_PREVIEW_PREFERENCE_KEY, OFFLINE_DOWNLOAD_ENGINE_KEY,
    OFFLINE_DOWNLOAD_ENGINE_REGISTRY_JSON_KEY, OfflineDownloadEngine,
};
use crate::services::preview::apps::PREVIEW_APPS_CONFIG_KEY;
use crate::types::AuditAction;
use aster_forge_config::ConfigValueType;
use serde::Serialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use super::actions::{ConfigActionType, MAIL_CONFIG_ACTION_KEY};

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ConfigSchemaItem {
    pub key: String,
    pub label_i18n_key: String,
    pub description_i18n_key: String,
    pub value_type: ConfigValueType,
    pub category: String,
    pub description: String,
    pub requires_restart: bool,
    pub is_sensitive: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<ConfigSchemaOption>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ConfigActionDescriptor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalidates: Vec<ConfigInvalidationTarget>,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ConfigSchemaOption {
    pub value: String,
    pub label_i18n_key: String,
    pub group: String,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ConfigActionDescriptor {
    pub action: ConfigActionType,
    pub target_key: String,
    pub label_i18n_key: String,
    pub presentation: ConfigActionPresentation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub draft_value_keys: Vec<String>,
    pub value_source_key: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ConfigActionPresentation {
    pub category: String,
    pub subcategory: Option<String>,
    pub group: String,
    pub order: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum ConfigInvalidationTarget {
    FrontendConfig,
    PreviewApps,
    ThumbnailSupport,
    MediaDataSupport,
}

pub fn get_schema() -> Vec<ConfigSchemaItem> {
    ALL_CONFIGS
        .iter()
        .map(|def| ConfigSchemaItem {
            key: def.key.to_string(),
            label_i18n_key: def.label_i18n_key.to_string(),
            description_i18n_key: def.description_i18n_key.to_string(),
            value_type: def.value_type,
            category: def.category.to_string(),
            description: def.description.to_string(),
            requires_restart: def.requires_restart,
            is_sensitive: def.is_sensitive,
            options: config_schema_options(def.key),
            actions: config_schema_actions(def.key),
            invalidates: config_schema_invalidates(def.key),
        })
        .collect()
}

fn config_schema_options(key: &str) -> Vec<ConfigSchemaOption> {
    match key {
        FRONTEND_IMAGE_PREVIEW_PREFERENCE_KEY => ["original_first", "preview_first"]
            .into_iter()
            .map(|value| ConfigSchemaOption {
                value: value.to_string(),
                label_i18n_key: format!("settings_image_preview_preference_option_{value}"),
                group: "image_preview_preference".to_string(),
            })
            .collect(),
        OFFLINE_DOWNLOAD_ENGINE_KEY => OfflineDownloadEngine::ALL
            .iter()
            .map(|engine| ConfigSchemaOption {
                value: engine.as_str().to_string(),
                label_i18n_key: format!(
                    "settings_offline_download_engine_option_{}",
                    engine.as_str()
                ),
                group: "offline_download_engine".to_string(),
            })
            .collect(),
        // Keep enum-set options backend-authored so the UI cannot drift from AuditAction.
        AUDIT_LOG_RECORDED_ACTIONS_KEY => AuditAction::ALL
            .iter()
            .map(|action| ConfigSchemaOption {
                value: action.as_str().to_string(),
                label_i18n_key: format!("audit_action_{}", action.as_str()),
                group: action.group().to_string(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn media_processing_cli_action(
    action: ConfigActionType,
    label_i18n_key: &str,
    order: i32,
) -> ConfigActionDescriptor {
    ConfigActionDescriptor {
        action,
        target_key: MEDIA_PROCESSING_REGISTRY_JSON_KEY.to_string(),
        label_i18n_key: label_i18n_key.to_string(),
        presentation: ConfigActionPresentation {
            category: "file_processing".to_string(),
            subcategory: Some("media".to_string()),
            group: "editor".to_string(),
            order,
        },
        draft_value_keys: vec![MEDIA_PROCESSING_REGISTRY_JSON_KEY.to_string()],
        value_source_key: Some(MEDIA_PROCESSING_REGISTRY_JSON_KEY.to_string()),
    }
}

fn config_schema_actions(key: &str) -> Vec<ConfigActionDescriptor> {
    match key {
        MAIL_SMTP_HOST_KEY => vec![ConfigActionDescriptor {
            action: ConfigActionType::SendTestEmail,
            target_key: MAIL_CONFIG_ACTION_KEY.to_string(),
            label_i18n_key: "mail_send_test_email".to_string(),
            presentation: ConfigActionPresentation {
                category: "mail".to_string(),
                subcategory: Some("config".to_string()),
                group: "test".to_string(),
                order: 10,
            },
            draft_value_keys: Vec::new(),
            value_source_key: None,
        }],
        PREVIEW_APPS_CONFIG_KEY => vec![ConfigActionDescriptor {
            action: ConfigActionType::BuildWopiDiscoveryPreviewConfig,
            target_key: PREVIEW_APPS_CONFIG_KEY.to_string(),
            label_i18n_key: "preview_apps_wopi_discovery_action".to_string(),
            presentation: ConfigActionPresentation {
                category: "site".to_string(),
                subcategory: Some("preview".to_string()),
                group: "editor".to_string(),
                order: 10,
            },
            draft_value_keys: vec![PREVIEW_APPS_CONFIG_KEY.to_string()],
            value_source_key: Some(PREVIEW_APPS_CONFIG_KEY.to_string()),
        }],
        MEDIA_PROCESSING_REGISTRY_JSON_KEY => vec![
            media_processing_cli_action(
                ConfigActionType::TestVipsCli,
                "media_processing_test_vips_cli",
                10,
            ),
            media_processing_cli_action(
                ConfigActionType::TestFfmpegCli,
                "media_processing_test_ffmpeg_cli",
                20,
            ),
            media_processing_cli_action(
                ConfigActionType::TestFfprobeCli,
                "media_processing_test_ffprobe_cli",
                30,
            ),
        ],
        OFFLINE_DOWNLOAD_ENGINE_REGISTRY_JSON_KEY => vec![ConfigActionDescriptor {
            action: ConfigActionType::TestAria2Rpc,
            target_key: OFFLINE_DOWNLOAD_ENGINE_REGISTRY_JSON_KEY.to_string(),
            label_i18n_key: "offline_download_test_aria2_rpc".to_string(),
            presentation: ConfigActionPresentation {
                category: "file_processing".to_string(),
                subcategory: Some("offline_download".to_string()),
                group: "editor".to_string(),
                order: 10,
            },
            draft_value_keys: vec![
                OFFLINE_DOWNLOAD_ENGINE_REGISTRY_JSON_KEY.to_string(),
                crate::config::operations::OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY.to_string(),
                crate::config::operations::OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY.to_string(),
                crate::config::operations::OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS_KEY
                    .to_string(),
            ],
            value_source_key: Some(OFFLINE_DOWNLOAD_ENGINE_REGISTRY_JSON_KEY.to_string()),
        }],
        _ => Vec::new(),
    }
}

fn config_schema_invalidates(key: &str) -> Vec<ConfigInvalidationTarget> {
    match key {
        PUBLIC_SITE_URL_KEY
        | AUTH_ALLOW_USER_REGISTRATION_KEY
        | BRANDING_TITLE_KEY
        | BRANDING_DESCRIPTION_KEY
        | BRANDING_FAVICON_URL_KEY
        | BRANDING_WORDMARK_DARK_URL_KEY
        | BRANDING_WORDMARK_LIGHT_URL_KEY
        | FRONTEND_IMAGE_PREVIEW_PREFERENCE_KEY => {
            vec![ConfigInvalidationTarget::FrontendConfig]
        }
        PREVIEW_APPS_CONFIG_KEY => vec![ConfigInvalidationTarget::PreviewApps],
        MEDIA_PROCESSING_REGISTRY_JSON_KEY => {
            vec![
                ConfigInvalidationTarget::ThumbnailSupport,
                ConfigInvalidationTarget::MediaDataSupport,
            ]
        }
        MEDIA_METADATA_ENABLED_KEY | MEDIA_METADATA_MAX_SOURCE_BYTES_KEY => {
            vec![ConfigInvalidationTarget::MediaDataSupport]
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::operations::OfflineDownloadEngine;

    #[test]
    fn audit_recorded_actions_schema_options_cover_all_actions() {
        let item = get_schema()
            .into_iter()
            .find(|item| item.key == AUDIT_LOG_RECORDED_ACTIONS_KEY)
            .expect("audit action scope config should be in schema");

        assert_eq!(item.value_type, ConfigValueType::StringEnumSet);
        assert_eq!(item.options.len(), AuditAction::COUNT);

        for (option, action) in item.options.iter().zip(AuditAction::ALL) {
            assert_eq!(option.value, action.as_str());
            assert_eq!(
                option.label_i18n_key,
                format!("audit_action_{}", action.as_str())
            );
            assert_eq!(option.group, action.group());
        }
    }

    #[test]
    fn offline_download_engine_schema_options_follow_engine_registry() {
        let item = get_schema()
            .into_iter()
            .find(|item| item.key == OFFLINE_DOWNLOAD_ENGINE_KEY)
            .expect("legacy offline download engine config should be in schema");

        let expected = OfflineDownloadEngine::ALL
            .into_iter()
            .map(|engine| engine.as_str())
            .collect::<Vec<_>>();
        let actual = item
            .options
            .iter()
            .map(|option| option.value.as_str())
            .collect::<Vec<_>>();

        assert_eq!(actual, expected);
    }

    #[test]
    fn image_preview_preference_schema_options_cover_supported_values() {
        let item = get_schema()
            .into_iter()
            .find(|item| item.key == FRONTEND_IMAGE_PREVIEW_PREFERENCE_KEY)
            .expect("image preview preference config should be in schema");

        let actual = item
            .options
            .iter()
            .map(|option| option.value.as_str())
            .collect::<Vec<_>>();

        assert_eq!(actual, ["original_first", "preview_first"]);
    }

    #[test]
    fn schema_exposes_builtin_config_actions() {
        let schema = get_schema();

        let mail_action = schema
            .iter()
            .find(|item| item.key == MAIL_SMTP_HOST_KEY)
            .and_then(|item| item.actions.first())
            .expect("mail config should expose test email action");
        assert_eq!(mail_action.action, ConfigActionType::SendTestEmail);
        assert_eq!(mail_action.target_key, MAIL_CONFIG_ACTION_KEY);
        assert_eq!(mail_action.presentation.category, "mail");
        assert_eq!(
            mail_action.presentation.subcategory.as_deref(),
            Some("config")
        );

        let preview_actions = schema
            .iter()
            .find(|item| item.key == PREVIEW_APPS_CONFIG_KEY)
            .expect("preview apps config should be in schema")
            .actions
            .iter()
            .map(|action| action.action)
            .collect::<Vec<_>>();
        assert_eq!(
            preview_actions,
            [ConfigActionType::BuildWopiDiscoveryPreviewConfig]
        );

        let media_actions = schema
            .iter()
            .find(|item| item.key == MEDIA_PROCESSING_REGISTRY_JSON_KEY)
            .expect("media processing config should be in schema")
            .actions
            .iter()
            .map(|action| action.action)
            .collect::<Vec<_>>();
        assert_eq!(
            media_actions,
            [
                ConfigActionType::TestVipsCli,
                ConfigActionType::TestFfmpegCli,
                ConfigActionType::TestFfprobeCli,
            ]
        );

        let offline_action = schema
            .iter()
            .find(|item| item.key == OFFLINE_DOWNLOAD_ENGINE_REGISTRY_JSON_KEY)
            .and_then(|item| item.actions.first())
            .expect("offline download registry should expose aria2 test action");
        assert_eq!(offline_action.action, ConfigActionType::TestAria2Rpc);
        assert_eq!(
            offline_action.draft_value_keys,
            [
                OFFLINE_DOWNLOAD_ENGINE_REGISTRY_JSON_KEY,
                crate::config::operations::OFFLINE_DOWNLOAD_ARIA2_RPC_URL_KEY,
                crate::config::operations::OFFLINE_DOWNLOAD_ARIA2_RPC_SECRET_KEY,
                crate::config::operations::OFFLINE_DOWNLOAD_ARIA2_REQUEST_TIMEOUT_SECS_KEY,
            ]
        );
    }

    #[test]
    fn schema_exposes_public_config_invalidation_targets() {
        let schema = get_schema();
        let invalidates_for = |key: &str| {
            schema
                .iter()
                .find(|item| item.key == key)
                .unwrap_or_else(|| panic!("{key} should be in schema"))
                .invalidates
                .clone()
        };

        assert_eq!(
            invalidates_for(PUBLIC_SITE_URL_KEY),
            [ConfigInvalidationTarget::FrontendConfig]
        );
        assert_eq!(
            invalidates_for(AUTH_ALLOW_USER_REGISTRATION_KEY),
            [ConfigInvalidationTarget::FrontendConfig]
        );
        assert_eq!(
            invalidates_for(PREVIEW_APPS_CONFIG_KEY),
            [ConfigInvalidationTarget::PreviewApps]
        );
        assert_eq!(
            invalidates_for(MEDIA_PROCESSING_REGISTRY_JSON_KEY),
            [
                ConfigInvalidationTarget::ThumbnailSupport,
                ConfigInvalidationTarget::MediaDataSupport,
            ]
        );
        assert_eq!(
            invalidates_for(MEDIA_METADATA_ENABLED_KEY),
            [ConfigInvalidationTarget::MediaDataSupport]
        );
    }
}
