//! 用户偏好、UI 配置与 locale 标签类型。

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
    /// Follow the operating system setting.
    #[default]
    System,
    /// Always use the light theme.
    Light,
    /// Always use the dark theme.
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
    /// Parses a `#rrggbb` hex color, accepting legacy preset names as aliases.
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

    /// Returns the normalized `#rrggbb` hex color string.
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
    /// Render items as rows in a list.
    #[default]
    List,
    /// Render items as thumbnails in a grid.
    Grid,
}

/// Preferred gesture for opening items in the browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum BrowserOpenMode {
    /// A single click opens the item.
    #[default]
    SingleClick,
    /// A double click opens the item.
    DoubleClick,
}

/// Normalized BCP 47 locale tag used by product preferences and plugin-facing
/// localization contracts.
///
/// The backend validates language tags without fixing the protocol to the
/// frontend's currently bundled locale set. Product UI choices remain limited
/// by the resources actually shipped by that frontend build.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    schema(value_type = String, example = "zh-CN")
)]
pub struct LocaleTag(String);

impl LocaleTag {
    /// Parses and normalizes a BCP 47 language tag.
    pub fn parse(value: impl AsRef<str>) -> std::result::Result<Self, String> {
        let value = value.as_ref().trim();
        if value.is_empty() {
            return Err("locale tag cannot be empty".to_string());
        }
        value
            .parse::<language_tags::LanguageTag>()
            .map(|tag| Self(tag.into_string()))
            .map_err(|error| format!("invalid BCP 47 locale tag '{value}': {error}"))
    }

    /// Returns the normalized tag string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the primary language subtag, for example `zh` for `zh-Hans-CN`.
    pub fn primary_language(&self) -> &str {
        self.0.split('-').next().unwrap_or(self.as_str())
    }
}

impl Default for LocaleTag {
    fn default() -> Self {
        Self("en".to_string())
    }
}

impl Serialize for LocaleTag {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for LocaleTag {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(de::Error::custom)
    }
}

/// Stored user preferences (serialized as JSON in `users.config`).
/// Empty struct (all fields None) is treated as null by `get_preferences`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct UserPreferences {
    /// Selected UI theme mode.
    pub theme_mode: Option<ThemeMode>,
    /// Selected UI accent color.
    pub color_preset: Option<ColorPreset>,
    /// File browser view mode.
    pub view_mode: Option<PrefViewMode>,
    /// Gesture that opens items in the file browser.
    pub browser_open_mode: Option<BrowserOpenMode>,
    /// Default sort key for file and folder listings.
    pub sort_by: Option<super::sort::SortBy>,
    /// Default sort direction for file and folder listings.
    pub sort_order: Option<aster_forge_api::SortOrder>,
    /// Preferred UI locale.
    pub language: Option<LocaleTag>,
    /// Time zone used to render timestamps: an IANA identifier or the literal
    /// `browser` to follow the client's zone.
    pub display_time_zone: Option<String>,
    /// Whether the client receives real-time storage change events.
    pub storage_event_stream_enabled: Option<bool>,
}

impl UserPreferences {
    /// Returns `true` when every preference is unset.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Open-ended `users.config` payload:
/// structured built-in preferences + arbitrary custom frontend keys.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UserConfig {
    /// Structured built-in preferences.
    #[serde(flatten, default)]
    pub preferences: UserPreferences,
    /// Arbitrary extra keys written by frontend versions; preserved verbatim.
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl UserConfig {
    /// Returns `true` when neither preferences nor extra keys are present.
    pub fn is_empty(&self) -> bool {
        self.preferences.is_empty() && self.extra.is_empty()
    }
}

/// Raw JSON string wrapper stored in `users.config`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, DeriveValueType)]
pub struct StoredUserConfig(pub String);

impl StoredUserConfig {
    /// Deserializes the stored JSON text into a `UserConfig`.
    pub fn parse(&self) -> serde_json::Result<UserConfig> {
        serde_json::from_str(&self.0)
    }

    /// Serializes a `UserConfig` into the stored JSON representation.
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

#[cfg(test)]
mod tests {
    use super::LocaleTag;

    #[test]
    fn locale_tag_normalizes_bcp47_case_and_preserves_existing_short_tags() {
        assert_eq!(LocaleTag::parse(" zh-cn ").unwrap().as_str(), "zh-CN");
        assert_eq!(LocaleTag::parse("EN-us").unwrap().as_str(), "en-US");
        assert_eq!(LocaleTag::parse("en").unwrap().as_str(), "en");
        assert_eq!(LocaleTag::parse("zh").unwrap().as_str(), "zh");
    }

    #[test]
    fn locale_tag_serde_rejects_empty_and_malformed_values() {
        let locale: LocaleTag = serde_json::from_str(r#""zh-Hans-CN""#).unwrap();
        assert_eq!(locale.primary_language(), "zh");
        assert_eq!(serde_json::to_string(&locale).unwrap(), r#""zh-Hans-CN""#);
        assert!(serde_json::from_str::<LocaleTag>(r#""""#).is_err());
        assert!(serde_json::from_str::<LocaleTag>(r#""not_a_locale""#).is_err());
    }
}
