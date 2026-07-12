use sea_orm::entity::prelude::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::collections::BTreeMap;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Theme mode for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

const DEFAULT_COLOR_PRESET: &str = "#2563eb";

/// User-selected UI accent color.
///
/// Stored and returned as a normalized `#rrggbb` hex color. The legacy preset
/// names are accepted while parsing old user config, then normalized on output.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    schema(value_type = String, example = "#2563eb")
)]
pub struct ColorPreset(String);

impl ColorPreset {
    pub fn parse(value: impl AsRef<str>) -> std::result::Result<Self, String> {
        let value = value.as_ref().trim();
        if let Some(hex) = legacy_color_preset(value) {
            return Ok(Self(hex.to_string()));
        }
        if is_hex_color(value) {
            return Ok(Self(value.to_ascii_lowercase()));
        }
        Err("color_preset must be a #rrggbb hex color".to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ColorPreset {
    fn default() -> Self {
        Self(DEFAULT_COLOR_PRESET.to_string())
    }
}

impl Serialize for ColorPreset {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ColorPreset {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(de::Error::custom)
    }
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..]
            .iter()
            .all(|byte| byte.is_ascii_hexdigit())
}

fn legacy_color_preset(value: &str) -> Option<&'static str> {
    match value {
        "blue" => Some("#2563eb"),
        "green" => Some("#16a34a"),
        "purple" => Some("#9333ea"),
        "orange" => Some("#f97316"),
        _ => None,
    }
}

/// File browser view mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PrefViewMode {
    #[default]
    List,
    Grid,
}

/// Preferred gesture for opening items in the browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum BrowserOpenMode {
    #[default]
    SingleClick,
    DoubleClick,
}

/// Interface display language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    En,
    Zh,
}

/// Stored user preferences (serialized as JSON in `users.config`).
/// Empty struct (all fields None) is treated as null by `get_preferences`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct UserPreferences {
    pub theme_mode: Option<ThemeMode>,
    pub color_preset: Option<ColorPreset>,
    pub view_mode: Option<PrefViewMode>,
    pub browser_open_mode: Option<BrowserOpenMode>,
    pub sort_by: Option<crate::api::pagination::SortBy>,
    pub sort_order: Option<aster_forge_api::SortOrder>,
    pub language: Option<Language>,
    pub display_time_zone: Option<String>,
    pub storage_event_stream_enabled: Option<bool>,
}

impl UserPreferences {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Open-ended `users.config` payload:
/// structured built-in preferences + arbitrary custom frontend keys.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UserConfig {
    #[serde(flatten, default)]
    pub preferences: UserPreferences,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl UserConfig {
    pub fn is_empty(&self) -> bool {
        self.preferences.is_empty() && self.extra.is_empty()
    }
}

/// Raw JSON string wrapper stored in `users.config`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, DeriveValueType)]
pub struct StoredUserConfig(pub String);

impl StoredUserConfig {
    pub fn parse(&self) -> serde_json::Result<UserConfig> {
        serde_json::from_str(&self.0)
    }

    pub fn from_config(config: &UserConfig) -> serde_json::Result<Self> {
        serde_json::to_string(config).map(Self)
    }
}

impl AsRef<str> for StoredUserConfig {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for StoredUserConfig {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<StoredUserConfig> for String {
    fn from(value: StoredUserConfig) -> Self {
        value.0
    }
}
