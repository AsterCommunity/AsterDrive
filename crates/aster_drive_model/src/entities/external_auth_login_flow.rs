//! SeaORM 实体定义：`external_auth_login_flows`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Single OAuth/OIDC sign-in flow from authorization redirect to consumed callback.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "external_auth_login_flows")]
pub struct Model {
    /// Auto-incrementing surrogate key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Provider the flow authenticates against; references `external_auth_providers.id`.
    pub provider_id: i64,
    /// Hex hash of the OAuth `state` parameter; the raw state is never stored and the
    /// hash keys the callback lookup.
    pub state_hash: String,
    /// Hex hash of the browser-binding secret tied to this flow; the callback must present
    /// the matching secret. `None` for flows created before browser binding existed.
    pub browser_binding_hash: Option<String>,
    /// OIDC `nonce` sent to the provider, matched against the returned ID token; `None`
    /// for protocols that do not use one.
    pub nonce: Option<String>,
    /// PKCE verifier for the flow, paired with the authorization request's challenge;
    /// `None` when the provider runs without PKCE.
    pub pkce_verifier: Option<String>,
    /// Redirect URI the authorization code must come back to; must match the registered
    /// provider callback route.
    pub redirect_uri: String,
    /// Frontend path the browser returns to after sign-in completes; `None` falls back to
    /// the default landing route.
    pub return_path: Option<String>,
    /// UTC instant the flow was created.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    /// Absolute expiry instant; expired flows can never be consumed.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: DateTimeUtc,
    /// UTC instant the flow completed; `None` while the callback has not arrived.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub consumed_at: Option<DateTimeUtc>,
}

/// Declared database relations for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// The provider the flow authenticates against, matched on `provider_id` to
    /// `external_auth_providers.id`.
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
