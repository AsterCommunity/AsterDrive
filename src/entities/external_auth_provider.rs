//! SeaORM 实体定义：`external_auth_providers`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::{ExternalAuthProtocol, ExternalAuthProviderKind};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "external_auth_providers")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub key: String,
    pub display_name: String,
    pub icon_url: Option<String>,
    pub provider_kind: ExternalAuthProviderKind,
    pub protocol: ExternalAuthProtocol,
    pub issuer_url: Option<String>,
    pub authorization_url: Option<String>,
    pub token_url: Option<String>,
    pub userinfo_url: Option<String>,
    pub client_id: String,
    #[serde(skip_serializing)]
    pub client_secret: Option<String>,
    pub scopes: String,
    pub enabled: bool,
    pub auto_provision_enabled: bool,
    pub auto_link_verified_email_enabled: bool,
    pub require_email_verified: bool,
    pub subject_claim: Option<String>,
    pub username_claim: Option<String>,
    pub display_name_claim: Option<String>,
    pub email_claim: Option<String>,
    pub email_verified_claim: Option<String>,
    pub groups_claim: Option<String>,
    pub avatar_url_claim: Option<String>,
    pub allowed_domains: Option<String>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::external_auth_email_verification_flow::Entity")]
    ExternalAuthEmailVerificationFlows,
    #[sea_orm(has_many = "super::external_auth_identity::Entity")]
    ExternalAuthIdentities,
    #[sea_orm(has_many = "super::external_auth_login_flow::Entity")]
    ExternalAuthLoginFlows,
}

impl Related<super::external_auth_email_verification_flow::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ExternalAuthEmailVerificationFlows.def()
    }
}

impl Related<super::external_auth_identity::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ExternalAuthIdentities.def()
    }
}

impl Related<super::external_auth_login_flow::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ExternalAuthLoginFlows.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
