//! SeaORM entity definition for `storage_policy_credentials`.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::{StorageCredentialKind, StorageCredentialProvider, StorageCredentialStatus};

#[derive(Clone, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "storage_policy_credentials")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub policy_id: i64,
    pub provider: StorageCredentialProvider,
    pub credential_kind: StorageCredentialKind,
    pub account_label: Option<String>,
    pub subject: Option<String>,
    pub tenant_id: Option<String>,
    pub scopes: String,
    #[serde(skip_serializing)]
    pub access_token_ciphertext: Option<String>,
    #[serde(skip_serializing)]
    pub refresh_token_ciphertext: Option<String>,
    pub metadata: String,
    pub status: StorageCredentialStatus,
    pub status_reason: Option<String>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub expires_at: Option<DateTimeUtc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub authorized_at: Option<DateTimeUtc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub last_refreshed_at: Option<DateTimeUtc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub last_validated_at: Option<DateTimeUtc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
}

impl fmt::Debug for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Model")
            .field("id", &self.id)
            .field("policy_id", &self.policy_id)
            .field("provider", &self.provider)
            .field("credential_kind", &self.credential_kind)
            .field("account_label", &self.account_label)
            .field("subject", &self.subject)
            .field("tenant_id", &self.tenant_id)
            .field("scopes", &self.scopes)
            .field(
                "access_token_ciphertext",
                &self
                    .access_token_ciphertext
                    .as_ref()
                    .map(|_| "***REDACTED***"),
            )
            .field(
                "refresh_token_ciphertext",
                &self
                    .refresh_token_ciphertext
                    .as_ref()
                    .map(|_| "***REDACTED***"),
            )
            .field("metadata", &"***REDACTED***")
            .field("status", &self.status)
            .field("status_reason", &self.status_reason)
            .field("expires_at", &self.expires_at)
            .field("authorized_at", &self.authorized_at)
            .field("last_refreshed_at", &self.last_refreshed_at)
            .field("last_validated_at", &self.last_validated_at)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::storage_policy::Entity",
        from = "Column::PolicyId",
        to = "super::storage_policy::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    StoragePolicy,
}

impl Related<super::storage_policy::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::StoragePolicy.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_storage_credential_secrets() {
        let now = chrono::Utc::now();
        let model = Model {
            id: 1,
            policy_id: 2,
            provider: StorageCredentialProvider::MicrosoftGraph,
            credential_kind: StorageCredentialKind::OauthDelegated,
            account_label: Some("admin@example.com".to_string()),
            subject: Some("subject".to_string()),
            tenant_id: Some("tenant".to_string()),
            scopes: r#"["offline_access","Files.ReadWrite.All"]"#.to_string(),
            access_token_ciphertext: Some("access-secret".to_string()),
            refresh_token_ciphertext: Some("refresh-secret".to_string()),
            metadata: r#"{"drive_id":"secret-drive"}"#.to_string(),
            status: StorageCredentialStatus::Authorized,
            status_reason: None,
            expires_at: Some(now),
            authorized_at: Some(now),
            last_refreshed_at: None,
            last_validated_at: None,
            created_at: now,
            updated_at: now,
        };

        let debug = format!("{model:?}");
        assert!(debug.contains(r#"access_token_ciphertext: Some("***REDACTED***")"#));
        assert!(debug.contains(r#"refresh_token_ciphertext: Some("***REDACTED***")"#));
        assert!(debug.contains(r#"metadata: "***REDACTED***""#));
        assert!(!debug.contains("access-secret"));
        assert!(!debug.contains("refresh-secret"));
        assert!(!debug.contains("secret-drive"));
    }
}
