//! SeaORM 实体定义：`external_auth_providers`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::external_auth_provider::StoredExternalAuthProviderOptions;
use aster_forge_external_auth::{ExternalAuthProtocol, ExternalAuthProviderKind};

#[derive(Clone, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
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
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub options: StoredExternalAuthProviderOptions,
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

impl fmt::Debug for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Model")
            .field("id", &self.id)
            .field("key", &self.key)
            .field("display_name", &self.display_name)
            .field("icon_url", &self.icon_url)
            .field("provider_kind", &self.provider_kind)
            .field("protocol", &self.protocol)
            .field("options", &"***REDACTED***")
            .field("issuer_url", &self.issuer_url)
            .field("authorization_url", &self.authorization_url)
            .field("token_url", &self.token_url)
            .field("userinfo_url", &self.userinfo_url)
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "***REDACTED***"),
            )
            .field("scopes", &self.scopes)
            .field("enabled", &self.enabled)
            .field("auto_provision_enabled", &self.auto_provision_enabled)
            .field(
                "auto_link_verified_email_enabled",
                &self.auto_link_verified_email_enabled,
            )
            .field("require_email_verified", &self.require_email_verified)
            .field("subject_claim", &self.subject_claim)
            .field("username_claim", &self.username_claim)
            .field("display_name_claim", &self.display_name_claim)
            .field("email_claim", &self.email_claim)
            .field("email_verified_claim", &self.email_verified_claim)
            .field("groups_claim", &self.groups_claim)
            .field("avatar_url_claim", &self.avatar_url_claim)
            .field("allowed_domains", &self.allowed_domains)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_sensitive_options() {
        let now = chrono::Utc::now();
        let model = Model {
            id: 1,
            key: "microsoft".to_string(),
            display_name: "Microsoft".to_string(),
            icon_url: None,
            provider_kind: ExternalAuthProviderKind::Microsoft,
            protocol: ExternalAuthProtocol::Oidc,
            options: StoredExternalAuthProviderOptions(
                r#"{"microsoft":{"tenant":"secret-tenant"}}"#.to_string(),
            ),
            issuer_url: None,
            authorization_url: None,
            token_url: None,
            userinfo_url: None,
            client_id: "client-id".to_string(),
            client_secret: Some("client-secret".to_string()),
            scopes: "openid profile email".to_string(),
            enabled: true,
            auto_provision_enabled: false,
            auto_link_verified_email_enabled: false,
            require_email_verified: true,
            subject_claim: None,
            username_claim: None,
            display_name_claim: None,
            email_claim: None,
            email_verified_claim: None,
            groups_claim: None,
            avatar_url_claim: None,
            allowed_domains: None,
            created_at: now,
            updated_at: now,
        };

        let debug = format!("{model:?}");
        assert!(debug.contains(r#"options: "***REDACTED***""#));
        assert!(debug.contains(r#"client_secret: Some("***REDACTED***")"#));
        assert!(!debug.contains("secret-tenant"));
        assert!(!debug.contains("client-secret"));
    }
}
