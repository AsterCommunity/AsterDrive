use crate::config::definitions::{
    ALL_CONFIGS, AUDIT_LOG_RECORDED_ACTIONS_KEY, FRONTEND_IMAGE_PREVIEW_PREFERENCE_KEY,
    OFFLINE_DOWNLOAD_ENGINE_KEY,
};
use crate::config::operations::OfflineDownloadEngine;
use crate::types::{AuditAction, SystemConfigValueType};
use serde::Serialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ConfigSchemaItem {
    pub key: String,
    pub label_i18n_key: String,
    pub description_i18n_key: String,
    pub value_type: SystemConfigValueType,
    pub category: String,
    pub description: String,
    pub requires_restart: bool,
    pub is_sensitive: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<ConfigSchemaOption>,
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
pub struct TemplateVariableItem {
    pub token: String,
    pub label_i18n_key: String,
    pub description_i18n_key: String,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TemplateVariableGroup {
    pub category: String,
    pub template_code: String,
    pub label_i18n_key: String,
    pub variables: Vec<TemplateVariableItem>,
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

pub fn list_template_variable_groups() -> Vec<TemplateVariableGroup> {
    crate::services::mail_template::list_template_variable_groups()
        .into_iter()
        .map(|group| TemplateVariableGroup {
            category: group.category,
            template_code: group.template_code,
            label_i18n_key: group.label_i18n_key,
            variables: group
                .variables
                .into_iter()
                .map(|variable| TemplateVariableItem {
                    token: variable.token,
                    label_i18n_key: variable.label_i18n_key,
                    description_i18n_key: variable.description_i18n_key,
                })
                .collect(),
        })
        .collect()
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

        assert_eq!(item.value_type, SystemConfigValueType::StringEnumSet);
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
}
