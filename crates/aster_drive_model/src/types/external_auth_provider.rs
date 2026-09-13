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
    #[sea_orm(string_value = "legacy")]
    #[default]
    Legacy,
    #[sea_orm(string_value = "unified")]
    Unified,
}

impl ExternalAuthCallbackMode {
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
    pub const EMPTY_JSON: &str = "{}";

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
