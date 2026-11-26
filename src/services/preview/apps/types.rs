//! 预览应用服务子模块：`types`。

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Runtime config key that stores the serialized preview apps JSON.
pub const PREVIEW_APPS_CONFIG_KEY: &str = "frontend_preview_apps_json";

/// Public preview apps configuration persisted as JSON in the runtime config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PublicPreviewAppsConfig {
    /// Config schema version; validation rejects the whole document unless it
    /// matches the currently supported version.
    #[serde(default = "super::default_preview_apps_version")]
    pub version: i32,
    /// Preview app definitions offered to the frontend.
    #[serde(default)]
    pub apps: Vec<PublicPreviewAppDefinition>,
}

/// A single preview app entry exposed to the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PublicPreviewAppDefinition {
    /// Unique app identifier; `builtin.*` keys are reserved for the built-in
    /// apps and must use [`PreviewAppProvider::Builtin`].
    pub key: String,
    /// Rendering backend that decides which config fields apply.
    pub provider: PreviewAppProvider,
    /// Icon URL path served by this instance, e.g. `/static/preview-apps/pdf.svg`.
    pub icon: String,
    /// Disabled apps are filtered out before the config is served.
    #[serde(default = "super::default_true")]
    pub enabled: bool,
    /// Display names keyed by lowercase locale (`en`, `zh-CN`); never empty
    /// after validation.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
    /// File extensions (lowercase, no leading dot) the app matches; empty
    /// means it is never matched by extension.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    /// Provider-specific settings consumed by the matching provider.
    #[serde(default, skip_serializing_if = "PublicPreviewAppConfig::is_empty")]
    pub config: PublicPreviewAppConfig,
}

/// Rendering backend of a preview app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum PreviewAppProvider {
    /// Frontend built-in renderer; restricted to the required `builtin.*` keys.
    Builtin,
    /// External viewer reached through a configured URL template.
    UrlTemplate,
    /// External editor/viewer integrated via the WOPI protocol.
    Wopi,
}

impl PreviewAppProvider {
    /// Canonical snake_case name used in serialized config and logs.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::UrlTemplate => "url_template",
            Self::Wopi => "wopi",
        }
    }
}

impl<'de> Deserialize<'de> for PreviewAppProvider {
    /// Accepts a case-insensitive string; unknown values are rejected.
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

/// How the frontend presents an external preview app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum PreviewOpenMode {
    /// Embed the app inside the preview panel via an iframe.
    Iframe,
    /// Open the app in a new browser tab.
    NewTab,
}

impl<'de> Deserialize<'de> for PreviewOpenMode {
    /// Accepts a case-insensitive string; unknown values are rejected.
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

/// Provider-specific settings for one preview app.
///
/// Only the fields relevant to the app's [`PreviewAppProvider`] are
/// interpreted; the rest are ignored at runtime.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PublicPreviewAppConfig {
    /// Separator for the built-in table app: one of `auto`, `,`, tab, `;` or `|`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<String>,
    /// Required for the `url_template` and WOPI providers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<PreviewOpenMode>,
    /// URL template for the `url_template` provider; `{{file_preview_url}}`
    /// is replaced with the file's inline preview URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url_template: Option<String>,
    /// Extra origins trusted for source checks; the action/discovery URL
    /// origins are always included implicitly.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_origins: Vec<String>,
    /// WOPI discovery action name (e.g. `edit`, `view`); defaults to `edit`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// Fully expanded WOPI action URL that bypasses discovery; the
    /// `{{wopi_src}}`/`{{WOPISrc}}` placeholder is replaced with the file's
    /// WOPISrc endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_url: Option<String>,
    /// Fallback alias for `action_url` kept for older configs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_url_template: Option<String>,
    /// WOPI host discovery XML URL; used both to resolve action URLs and to
    /// obtain proof keys for request validation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery_url: Option<String>,
    /// Extra form fields appended to the WOPI post target.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub form_fields: BTreeMap<String, String>,
}

impl PublicPreviewAppConfig {
    /// True when no provider-specific setting is present.
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
