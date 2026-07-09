//! 外部认证服务聚合入口。
//!
//! 这组模块负责外部认证 provider 管理、登录回调、邮箱补验、账号绑定和清理任务。
//! 对外仍通过 `crate::services::auth::external::*` 暴露，避免 route 层感知拆分。

mod links;
mod login;
mod normalize;
mod password_link;
mod providers;
mod resolution;
mod verification;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::entities::user;
use crate::types::{ExternalAuthProtocol, ExternalAuthProviderKind, ExternalAuthProviderOptions};
use aster_forge_api::NullablePatch;

pub use links::{cleanup_expired_flows, delete_link, list_links};
pub use login::{finish_callback, start_login};
pub use normalize::callback_redirect_uri;
pub use password_link::link_with_password;
pub use providers::{
    create_provider, delete_provider, get_admin_provider, list_admin_providers,
    list_provider_kinds, list_public_providers, list_public_providers_by_kind, test_provider,
    test_provider_params, update_provider,
};
pub use verification::{confirm_email_verification, start_email_verification};

const DEFAULT_SCOPES: &str = "openid email profile";
const FLOW_TTL_SECS: u64 = 300;
const EMAIL_VERIFICATION_FLOW_TTL_SECS: u64 = 1_800;
const REDACTED_SECRET: &str = "***REDACTED***";
const EXTERNAL_AUTH_USER_PASSWORD_BYTES: usize = 48;
const EXTERNAL_AUTH_IDENTITY_NAMESPACE_MAX_LEN: usize = 512;
const EXTERNAL_AUTH_URL_MAX_LEN: usize = 2048;
const USERNAME_MAX_LEN: usize = 16;
const USERNAME_MIN_LEN: usize = 4;

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthPublicProvider {
    pub key: String,
    pub kind: ExternalAuthProviderKind,
    pub display_name: String,
    pub icon_url: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthProviderKindInfo {
    pub kind: ExternalAuthProviderKind,
    pub protocol: ExternalAuthProtocol,
    pub display_name: String,
    pub description: String,
    pub default_scopes: String,
    pub issuer_url_required: bool,
    pub manual_endpoint_configuration_supported: bool,
    pub authorization_url_required: bool,
    pub token_url_required: bool,
    pub userinfo_url_required: bool,
    pub supports_discovery: bool,
    pub supports_pkce: bool,
    pub supports_email_verified_claim: bool,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthStartLoginRequest {
    pub return_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthStartLoginResponse {
    pub authorization_url: String,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthEmailVerificationStartRequest {
    pub flow_token: String,
    pub email: String,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthPasswordLinkRequest {
    pub flow_token: String,
    pub identifier: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthEmailVerificationStartResponse {
    pub message: String,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(utoipa::IntoParams, utoipa::ToSchema)
)]
pub struct ExternalAuthEmailVerificationConfirmQuery {
    pub token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(utoipa::IntoParams, utoipa::ToSchema)
)]
pub struct ExternalAuthCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthLinkInfo {
    pub id: i64,
    pub provider_id: i64,
    pub provider_key: String,
    pub provider_kind: ExternalAuthProviderKind,
    pub provider_display_name: String,
    pub provider_icon_url: Option<String>,
    pub issuer: String,
    pub subject: String,
    pub email_snapshot: Option<String>,
    pub display_name_snapshot: Option<String>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub last_login_at: Option<chrono::DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct ExternalAuthPrimaryLogin {
    pub user: user::Model,
    pub return_path: String,
    pub provider_key: String,
    pub issuer: String,
    pub subject: String,
    pub linked: bool,
    pub auto_provisioned: bool,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct AdminExternalAuthProviderInfo {
    pub id: i64,
    pub key: String,
    pub provider_kind: ExternalAuthProviderKind,
    pub protocol: ExternalAuthProtocol,
    pub display_name: String,
    pub icon_url: Option<String>,
    pub options: ExternalAuthProviderOptions,
    pub issuer_url: Option<String>,
    pub authorization_url: Option<String>,
    pub token_url: Option<String>,
    pub userinfo_url: Option<String>,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub client_secret_configured: bool,
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
    pub allowed_domains: Vec<String>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct CreateExternalAuthProviderInput {
    pub provider_kind: ExternalAuthProviderKind,
    pub display_name: String,
    pub icon_url: Option<String>,
    pub options: Option<ExternalAuthProviderOptions>,
    pub issuer_url: Option<String>,
    pub authorization_url: Option<String>,
    pub token_url: Option<String>,
    pub userinfo_url: Option<String>,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub scopes: Option<String>,
    pub enabled: Option<bool>,
    pub auto_provision_enabled: Option<bool>,
    pub auto_link_verified_email_enabled: Option<bool>,
    pub require_email_verified: Option<bool>,
    pub subject_claim: Option<String>,
    pub username_claim: Option<String>,
    pub display_name_claim: Option<String>,
    pub email_claim: Option<String>,
    pub email_verified_claim: Option<String>,
    pub groups_claim: Option<String>,
    pub avatar_url_claim: Option<String>,
    pub allowed_domains: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct UpdateExternalAuthProviderInput {
    pub display_name: Option<String>,
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub icon_url: Option<NullablePatch<String>>,
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub issuer_url: Option<NullablePatch<String>>,
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub authorization_url: Option<NullablePatch<String>>,
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub token_url: Option<NullablePatch<String>>,
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub userinfo_url: Option<NullablePatch<String>>,
    pub options: Option<ExternalAuthProviderOptions>,
    pub client_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub client_secret: Option<NullablePatch<String>>,
    pub scopes: Option<String>,
    pub enabled: Option<bool>,
    pub auto_provision_enabled: Option<bool>,
    pub auto_link_verified_email_enabled: Option<bool>,
    pub require_email_verified: Option<bool>,
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub subject_claim: Option<NullablePatch<String>>,
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub username_claim: Option<NullablePatch<String>>,
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub display_name_claim: Option<NullablePatch<String>>,
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub email_claim: Option<NullablePatch<String>>,
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub email_verified_claim: Option<NullablePatch<String>>,
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub groups_claim: Option<NullablePatch<String>>,
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub avatar_url_claim: Option<NullablePatch<String>>,
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<Vec<String>>)
    )]
    pub allowed_domains: Option<NullablePatch<Vec<String>>>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthProviderTestParamsInput {
    pub provider_kind: ExternalAuthProviderKind,
    pub options: Option<ExternalAuthProviderOptions>,
    pub issuer_url: Option<String>,
    pub authorization_url: Option<String>,
    pub token_url: Option<String>,
    pub userinfo_url: Option<String>,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub scopes: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthProviderTestCheck {
    pub name: String,
    pub success: bool,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthProviderTestResult {
    pub provider: String,
    pub issuer: Option<String>,
    pub authorization_endpoint: Option<String>,
    pub token_endpoint: Option<String>,
    pub userinfo_endpoint: Option<String>,
    pub jwks_key_count: Option<usize>,
    pub checks: Vec<ExternalAuthProviderTestCheck>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExternalAuthLoginAuditDetails<'a> {
    pub provider_key: &'a str,
    pub issuer: &'a str,
    pub subject: &'a str,
    pub linked: bool,
    pub auto_provisioned: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExternalAuthProviderAuditDetails<'a> {
    pub key: &'a str,
    pub icon_url: Option<&'a str>,
    pub issuer_url: Option<&'a str>,
    pub enabled: bool,
    pub auto_provision_enabled: bool,
    pub auto_link_verified_email_enabled: bool,
    pub require_email_verified: bool,
}

pub struct PendingExternalAuthEmailVerification {
    pub flow_token: String,
    pub return_path: String,
}

pub struct ExternalAuthEmailVerificationConfirmResult {
    pub primary_login: ExternalAuthPrimaryLogin,
}

pub struct ExternalAuthPasswordLinkResult {
    pub primary_login: ExternalAuthPrimaryLogin,
}

#[expect(
    clippy::large_enum_variant,
    reason = "one-shot service-to-route result; boxing would add a heap allocation without shrinking retained state"
)]
pub enum ExternalAuthCallbackOutcome {
    Login(ExternalAuthCallbackResult),
    EmailVerificationRequired(PendingExternalAuthEmailVerification),
}

pub struct ExternalAuthCallbackResult {
    pub primary_login: ExternalAuthPrimaryLogin,
}
