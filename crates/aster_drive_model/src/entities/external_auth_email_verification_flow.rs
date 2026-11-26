//! SeaORM 实体定义：`external_auth_email_verification_flows`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Email-verification flow behind an external-auth identity (recovery binding or claim
/// confirmation). Progresses start -> email sent -> consumed.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "external_auth_email_verification_flows")]
pub struct Model {
    /// Auto-incrementing surrogate key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// External-auth provider the identity belongs to; references `external_auth_providers.id`.
    pub provider_id: i64,
    /// Issuer-scoped namespace qualifying `subject`, so the same provider can serve
    /// multiple issuers or account modes.
    pub identity_namespace: String,
    /// Stable external subject (`sub`) of the identity going through verification.
    pub subject: String,
    /// Email the verification message is addressed to; `None` until an email is requested.
    pub target_email: Option<String>,
    /// Display name claimed by the provider, snapshotted at flow creation for use after
    /// upstream claims rotate.
    pub display_name_snapshot: Option<String>,
    /// Preferred username claimed by the provider, snapshotted at flow creation.
    pub preferred_username_snapshot: Option<String>,
    /// Frontend path the browser returns to after the flow completes; `None` falls back
    /// to the default landing route.
    pub return_path: Option<String>,
    /// Hex hash of the flow token identifying this row in browser callbacks; plaintext
    /// tokens are never stored.
    pub flow_token_hash: String,
    /// Hex hash of the browser-binding secret that must accompany the flow token; `None`
    /// for flows created before browser binding existed.
    #[serde(skip_serializing)]
    pub browser_binding_hash: Option<String>,
    /// Hex hash of the emailed verification code; `None` until the email is sent.
    pub verification_token_hash: Option<String>,
    /// UTC instant the verification email was requested; `None` while not yet sent.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub email_requested_at: Option<DateTimeUtc>,
    /// UTC instant the flow was created.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    /// Absolute expiry instant; expired flows are never accepted or consumable.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: DateTimeUtc,
    /// UTC instant the flow completed; `None` while still pending.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub consumed_at: Option<DateTimeUtc>,
}

/// Declared database relations for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// The provider owning the flow, matched on `provider_id` to `external_auth_providers.id`.
    #[sea_orm(
        belongs_to = "super::external_auth_provider::Entity",
        from = "Column::ProviderId",
        to = "super::external_auth_provider::Column::Id"
    )]
    ExternalAuthProvider,
}

impl Related<super::external_auth_provider::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ExternalAuthProvider.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
