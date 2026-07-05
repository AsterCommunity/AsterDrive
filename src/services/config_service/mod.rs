//! 配置服务聚合入口。

mod actions;
mod public;
mod schema;
mod system;

pub use actions::{
    ConfigActionResult, ConfigActionType, ExecuteConfigActionInput, MAIL_CONFIG_ACTION_KEY,
    execute_action_with_audit,
};
pub use public::{
    PUBLIC_CONFIG_CACHE_CONTROL, PublicBranding, PublicCustomConfig, PublicFrontendConfig,
    PublicFrontendMediaConfig, get_public_branding, get_public_custom_config,
    get_public_frontend_config, get_public_media_data_support, get_public_preview_apps,
    get_public_thumbnail_support,
};
pub(crate) use public::{
    invalidate_public_media_data_support_cache, invalidate_public_thumbnail_support_cache,
};
pub use schema::{
    ConfigActionDescriptor, ConfigActionPresentation, ConfigSchemaItem, ConfigSchemaOption,
    TemplateVariableGroup, TemplateVariableItem, get_schema, list_template_variable_groups,
};
pub use system::{
    SystemConfig, SystemConfigValue, delete, delete_with_audit, get_by_key, list_paginated, set,
    set_with_audit, set_with_audit_and_visibility, set_with_visibility,
};
