//! 预览应用服务子模块：`types`。

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

pub const PREVIEW_APPS_CONFIG_KEY: &str = "frontend_preview_apps_json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PublicPreviewAppsConfig {
    #[serde(default = "super::default_preview_apps_version")]
    pub version: i32,
    #[serde(default)]
    pub apps: Vec<PublicPreviewAppDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PublicPreviewAppDefinition {
    pub key: String,
    pub provider: PreviewAppProvider,
    pub icon: String,
    #[serde(default = "super::default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    #[serde(default, skip_serializing_if = "PublicPreviewAppConfig::is_empty")]
    pub config: PublicPreviewAppConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum PreviewAppProvider {
    Builtin,
    UrlTemplate,
    Wopi,
}

impl PreviewAppProvider {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::UrlTemplate => "url_template",
            Self::Wopi => "wopi",
        }
    }
}

impl<'de> Deserialize<'de> for PreviewAppProvider {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        match raw.trim().to_ascii_lowercase().as_str() {
            "builtin" => Ok(Self::Builtin),
            "url_template" => Ok(Self::UrlTemplate),
            "wopi" => Ok(Self::Wopi),
            other => Err(serde::de::Error::custom(format!(
                "unsupported preview app provider '{other}'",
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum PreviewOpenMode {
    Iframe,
    NewTab,
}

impl<'de> Deserialize<'de> for PreviewOpenMode {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        match raw.trim().to_ascii_lowercase().as_str() {
            "iframe" => Ok(Self::Iframe),
            "new_tab" => Ok(Self::NewTab),
            other => Err(serde::de::Error::custom(format!(
                "unsupported preview open mode '{other}'",
            ))),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PublicPreviewAppConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<PreviewOpenMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url_template: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_origins: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_url_template: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery_url: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub form_fields: BTreeMap<String, String>,
}

impl PublicPreviewAppConfig {
    pub(crate) fn is_empty(&self) -> bool {
        self.delimiter.is_none()
            && self.mode.is_none()
            && self.url_template.is_none()
            && self.allowed_origins.is_empty()
            && self.action.is_none()
            && self.action_url.is_none()
            && self.action_url_template.is_none()
            && self.discovery_url.is_none()
            && self.form_fields.is_empty()
    }
}
