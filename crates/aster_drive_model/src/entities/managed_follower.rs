//! SeaORM 实体定义：`managed_follower`。

use sea_orm::Set;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::RemoteNodeTransportMode;

#[derive(Clone, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "managed_followers")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub name: String,
    pub base_url: String,
    pub access_key: String,
    #[serde(skip_serializing)]
    pub secret_key: String,
    pub is_enabled: bool,
    pub transport_mode: RemoteNodeTransportMode,
    pub last_capabilities: String,
    pub last_error: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub last_checked_at: Option<DateTimeUtc>,
    pub tunnel_last_error: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub tunnel_last_seen_at: Option<DateTimeUtc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
}

impl fmt::Debug for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Model")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("base_url", &self.base_url)
            .field("access_key", &"***REDACTED***")
            .field("secret_key", &"***REDACTED***")
            .field("is_enabled", &self.is_enabled)
            .field("transport_mode", &self.transport_mode)
            .field("last_capabilities", &self.last_capabilities)
            .field("last_error", &self.last_error)
            .field("last_checked_at", &self.last_checked_at)
            .field("tunnel_last_error", &self.tunnel_last_error)
            .field("tunnel_last_seen_at", &self.tunnel_last_seen_at)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::follower_enrollment_session::Entity")]
    FollowerEnrollmentSessions,
    #[sea_orm(has_many = "super::storage_policy::Entity")]
    StoragePolicies,
}

impl Related<super::follower_enrollment_session::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::FollowerEnrollmentSessions.def()
    }
}

impl Related<super::storage_policy::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::StoragePolicies.def()
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        if insert {
            if !self.transport_mode.is_set() {
                self.transport_mode = Set(RemoteNodeTransportMode::Direct);
            }
            if !self.tunnel_last_error.is_set() {
                self.tunnel_last_error = Set(String::new());
            }
            if !self.tunnel_last_seen_at.is_set() {
                self.tunnel_last_seen_at = Set(None);
            }
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_managed_follower_credentials() {
        let now = chrono::Utc::now();
        let model = Model {
            id: 1,
            name: "follower".to_string(),
            base_url: "https://follower.example.test".to_string(),
            access_key: "plain-access-key".to_string(),
            secret_key: "plain-secret-key".to_string(),
            is_enabled: true,
            transport_mode: RemoteNodeTransportMode::Direct,
            last_capabilities: "{}".to_string(),
            last_error: String::new(),
            last_checked_at: None,
            tunnel_last_error: String::new(),
            tunnel_last_seen_at: None,
            created_at: now,
            updated_at: now,
        };

        let debug = format!("{model:?}");
        assert!(debug.contains(r#"access_key: "***REDACTED***""#));
        assert!(debug.contains(r#"secret_key: "***REDACTED***""#));
        assert!(!debug.contains("plain-access-key"));
        assert!(!debug.contains("plain-secret-key"));
    }
}
