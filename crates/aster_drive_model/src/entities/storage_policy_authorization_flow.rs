//! SeaORM entity definition for `storage_policy_authorization_flows`.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::{StorageAuthorizationFlowStatus, StorageCredentialProvider};

/// One-time OAuth authorization flow row for a storage policy credential.
/// Core persists the connector-generated state without interpreting it.
#[derive(Clone, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "storage_policy_authorization_flows")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// OAuth provider this flow belongs to (Microsoft Graph / Google Drive).
    pub provider: StorageCredentialProvider,
    /// Policy being authorized; `None` only for historical rows created
    /// before flows were strictly bound to a saved policy.
    pub policy_id: Option<i64>,
    /// User who started the flow; owns it for auth purposes.
    pub created_by_user_id: i64,
    /// Hash of the connector-generated OAuth `state`; lookup key when the
    /// provider calls back. The raw state is never stored.
    pub state_hash: String,
    /// PKCE verifier issued by the connector, if its flow uses PKCE;
    /// never serialized to API responses.
    #[serde(skip_serializing)]
    pub pkce_verifier: Option<String>,
    /// Callback URI the provider must redirect to after authorization.
    pub redirect_uri: String,
    /// OAuth scopes requested for the credential, serialized as a JSON array.
    pub scopes: String,
    /// Opaque connector-owned flow context (JSON); core stores it verbatim
    /// and hands it back to the connector at callback time.
    pub context: String,
    /// Current lifecycle state of the flow.
    pub status: StorageAuthorizationFlowStatus,
    /// Row creation timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    /// Time after which the pending flow expires (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: DateTimeUtc,
    /// When the flow was consumed by a successful callback; `None` while
    /// still pending, expired or cancelled.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub consumed_at: Option<DateTimeUtc>,
}

impl fmt::Debug for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Model")
            .field("id", &self.id)
            .field("provider", &self.provider)
            .field("policy_id", &self.policy_id)
            .field("created_by_user_id", &self.created_by_user_id)
            .field("state_hash", &self.state_hash)
            .field(
                "pkce_verifier",
                &self.pkce_verifier.as_ref().map(|_| "***REDACTED***"),
            )
            .field("redirect_uri", &self.redirect_uri)
            .field("scopes", &self.scopes)
            .field("context", &"***REDACTED***")
            .field("status", &self.status)
            .field("created_at", &self.created_at)
            .field("expires_at", &self.expires_at)
            .field("consumed_at", &self.consumed_at)
            .finish()
    }
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// The storage policy being authorized (FK `policy_id`).
    #[sea_orm(
        belongs_to = "super::storage_policy::Entity",
        from = "Column::PolicyId",
        to = "super::storage_policy::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    StoragePolicy,
    /// The user who started the flow (FK `created_by_user_id`).
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::CreatedByUserId",
        to = "super::user::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    CreatedByUser,
}

impl Related<super::storage_policy::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::StoragePolicy.def()
    }
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::CreatedByUser.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_authorization_flow_secrets() {
        let now = chrono::Utc::now();
        let model = Model {
            id: 1,
            provider: StorageCredentialProvider::MicrosoftGraph,
            policy_id: Some(2),
            created_by_user_id: 3,
            state_hash: "state-hash".to_string(),
            pkce_verifier: Some("pkce-secret".to_string()),
            redirect_uri: "https://drive.example.com/callback".to_string(),
            scopes: r#"["offline_access"]"#.to_string(),
            context: r#"{"draft":"secret"}"#.to_string(),
            status: StorageAuthorizationFlowStatus::Pending,
            created_at: now,
            expires_at: now,
            consumed_at: None,
        };

        let debug = format!("{model:?}");
        assert!(debug.contains(r#"pkce_verifier: Some("***REDACTED***")"#));
        assert!(debug.contains(r#"context: "***REDACTED***""#));
        assert!(!debug.contains("pkce-secret"));
        assert!(!debug.contains("draft"));
    }
}
