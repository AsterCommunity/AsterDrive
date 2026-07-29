//! SeaORM entity definition for `storage_policy_authorization_flows`.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::{StorageAuthorizationFlowStatus, StorageCredentialProvider};

#[derive(Clone, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "storage_policy_authorization_flows")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub provider: StorageCredentialProvider,
    pub policy_id: Option<i64>,
    pub created_by_user_id: i64,
    pub state_hash: String,
    #[serde(skip_serializing)]
    pub pkce_verifier: Option<String>,
    pub redirect_uri: String,
    pub scopes: String,
    pub context: String,
    pub status: StorageAuthorizationFlowStatus,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: DateTimeUtc,
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
