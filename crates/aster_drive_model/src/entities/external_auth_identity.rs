//! SeaORM 实体定义：`external_auth_identities`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Persistent link between a local account and one external-auth provider identity.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "external_auth_identities")]
pub struct Model {
    /// Auto-incrementing surrogate key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Linked local account; references `users.id`.
    pub user_id: i64,
    /// External-auth provider the identity comes from; references `external_auth_providers.id`.
    pub provider_id: i64,
    /// Issuer-scoped namespace qualifying `subject`, so one provider can serve several
    /// issuers or account modes without collisions.
    pub identity_namespace: String,
    /// Stable external subject (`sub`) issued by the provider; unique per provider and
    /// namespace.
    pub subject: String,
    /// Email reported by the provider at the latest login; informational only and may
    /// drift from the local account's email. `None` when the provider sent no email.
    pub email_snapshot: Option<String>,
    /// Display name reported by the provider at the latest login; `None` when absent.
    pub display_name_snapshot: Option<String>,
    /// UTC instant the link was created.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    /// UTC instant of the latest link update.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
    /// UTC instant this identity last authenticated successfully; `None` until first login.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub last_login_at: Option<DateTimeUtc>,
}

/// Declared database relations for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// The identity's provider, matched on `provider_id` to `external_auth_providers.id`.
    #[sea_orm(
        belongs_to = "super::external_auth_provider::Entity",
        from = "Column::ProviderId",
        to = "super::external_auth_provider::Column::Id"
    )]
    ExternalAuthProvider,
    /// The linked local account, matched on `user_id` to `users.id`.
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
}

impl Related<super::external_auth_provider::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ExternalAuthProvider.def()
    }
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
