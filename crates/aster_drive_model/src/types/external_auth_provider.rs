//! 外部认证 provider 的回调模式与 options 存储类型。

use sea_orm::{DeriveActiveEnum, DeriveValueType, EnumIter, sea_query::StringLen};
use serde::{Deserialize, Serialize};

/// Callback route selected for newly started external-auth login flows.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize,
)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum ExternalAuthCallbackMode {
    // TODO(v1.0.0): Remove after legacy provider callback registrations no longer need migration.
    /// Provider-specific callback route inherited from existing registrations.
    #[sea_orm(string_value = "legacy")]
    #[default]
    Legacy,
    /// Shared callback route that new provider registrations should use.
    #[sea_orm(string_value = "unified")]
    Unified,
}

impl ExternalAuthCallbackMode {
    /// Returns the snake_case route mode name shared by storage and API values.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Unified => "unified",
        }
    }
}

/// Raw JSON object stored in `external_auth_providers.options`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, DeriveValueType)]
pub struct StoredExternalAuthProviderOptions(pub String);

impl StoredExternalAuthProviderOptions {
    /// Canonical serialized form of an empty options object.
    pub const EMPTY_JSON: &str = "{}";

    /// Returns a wrapper holding the canonical empty JSON object.
    pub fn empty() -> Self {
        Self(Self::EMPTY_JSON.to_string())
    }
}

impl AsRef<str> for StoredExternalAuthProviderOptions {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for StoredExternalAuthProviderOptions {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<StoredExternalAuthProviderOptions> for String {
    fn from(value: StoredExternalAuthProviderOptions) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::ExternalAuthCallbackMode;

    #[test]
    fn callback_modes_have_stable_wire_values() {
        assert_eq!(ExternalAuthCallbackMode::Legacy.as_str(), "legacy");
        assert_eq!(ExternalAuthCallbackMode::Unified.as_str(), "unified");
        assert_eq!(
            serde_json::to_string(&ExternalAuthCallbackMode::Legacy).unwrap(),
            "\"legacy\""
        );
        assert_eq!(
            serde_json::to_string(&ExternalAuthCallbackMode::Unified).unwrap(),
            "\"unified\""
        );
    }
}
