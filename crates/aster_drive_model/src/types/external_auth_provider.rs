use sea_orm::DeriveValueType;
use serde::{Deserialize, Serialize};

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
