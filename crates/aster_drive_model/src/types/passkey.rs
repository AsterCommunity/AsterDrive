//! WebAuthn passkey 凭据存储包装。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Raw JSON payload stored in `passkeys.credential`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, DeriveValueType)]
pub struct StoredPasskeyCredential(pub String);

impl StoredPasskeyCredential {
    /// Deserializes the stored JSON; returns an error when the payload is
    /// malformed.
    pub fn parse(&self) -> serde_json::Result<serde_json::Value> {
        serde_json::from_str(&self.0)
    }

    /// Serializes a JSON value into the stored form.
    pub fn from_json(value: &serde_json::Value) -> serde_json::Result<Self> {
        serde_json::to_string(value).map(Self)
    }
}

impl AsRef<str> for StoredPasskeyCredential {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for StoredPasskeyCredential {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<StoredPasskeyCredential> for String {
    fn from(value: StoredPasskeyCredential) -> Self {
        value.0
    }
}
