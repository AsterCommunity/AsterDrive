//! SeaORM 实体定义：`external_auth_providers`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::external_auth_provider::{
    ExternalAuthCallbackMode, StoredExternalAuthProviderOptions,
};
use aster_forge_external_auth::{ExternalAuthProtocol, ExternalAuthProviderKind};

/// Configured external authentication provider (OIDC/OAuth2/...) and its claim mapping.
#[derive(Clone, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "external_auth_providers")]
pub struct Model {
    /// Auto-incrementing surrogate key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Stable URL-safe provider identifier used in routes and auth callbacks; unique per
    /// provider kind.
    pub key: String,
    /// Human-facing provider name shown on sign-in UI.
    pub display_name: String,
    /// Optional logo URL rendered next to the provider on sign-in UI.
    pub icon_url: Option<String>,
    /// Built-in provider preset that decides driver behavior and defaults.
    pub provider_kind: ExternalAuthProviderKind,
    /// Wire protocol spoken with the provider.
    pub protocol: ExternalAuthProtocol,
    /// Callback route family the provider registration expects.
    pub callback_mode: ExternalAuthCallbackMode,
    /// Raw JSON object of provider-kind-specific options (e.g. tenant IDs); secrets must
    /// not be placed here — they belong in `client_secret`.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub options: StoredExternalAuthProviderOptions,
    /// OIDC issuer URL used for discovery; required for discovery-based protocols, `None`
    /// for manually configured endpoints.
    pub issuer_url: Option<String>,
    /// Manually configured authorization endpoint; `None` when discovery supplies it.
    pub authorization_url: Option<String>,
    /// Manually configured token endpoint; `None` when discovery supplies it.
    pub token_url: Option<String>,
    /// Manually configured userinfo endpoint; `None` when discovery supplies it.
    pub userinfo_url: Option<String>,
    /// OAuth client identifier registered at the provider.
    pub client_id: String,
    /// OAuth client secret registered at the provider; sensitive, never serialized.
    #[serde(skip_serializing)]
    pub client_secret: Option<String>,
    /// Space-separated OAuth scope list requested during authorization.
    pub scopes: String,
    /// Whether the provider accepts new sign-ins; disabled providers are hidden and rejected.
    pub enabled: bool,
    /// Whether sign-in with an unlinked external identity creates a local account.
    pub auto_provision_enabled: bool,
    /// Whether a verified email that matches an existing local account is linked
    /// automatically without explicit confirmation.
    pub auto_link_verified_email_enabled: bool,
    /// Whether sign-in requires the provider to assert the email as verified.
    pub require_email_verified: bool,
    /// Claim name override for the stable subject; `None` uses the protocol default (`sub`).
    pub subject_claim: Option<String>,
    /// Claim name override for the preferred username; `None` uses the protocol default.
    pub username_claim: Option<String>,
    /// Claim name override for the display name; `None` uses the protocol default.
    pub display_name_claim: Option<String>,
    /// Claim name override for the email; `None` uses the protocol default (`email`).
    pub email_claim: Option<String>,
    /// Claim name override for the email-verified flag; `None` uses the protocol default.
    pub email_verified_claim: Option<String>,
    /// Claim name override for group memberships; `None` uses the protocol default.
    pub groups_claim: Option<String>,
    /// Claim name override for the avatar URL; `None` uses the protocol default.
    pub avatar_url_claim: Option<String>,
    /// JSON array of email domains allowed to sign in; `None` or empty allows every domain.
    pub allowed_domains: Option<String>,
    /// UTC instant the provider was created.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    /// UTC instant of the latest configuration update.
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
            .field("callback_mode", &self.callback_mode)
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

/// Declared database relations for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// Email-verification flows opened under this provider.
    #[sea_orm(has_many = "super::external_auth_email_verification_flow::Entity")]
    ExternalAuthEmailVerificationFlows,
    /// Local accounts linked through this provider.
    #[sea_orm(has_many = "super::external_auth_identity::Entity")]
    ExternalAuthIdentities,
    /// Sign-in flows started against this provider.
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
            callback_mode: ExternalAuthCallbackMode::Legacy,
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
