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

use aster_drive_model::entities::user;
use aster_drive_model::types::external_auth_provider::ExternalAuthCallbackMode;
use aster_forge_api::NullablePatch;
use aster_forge_external_auth::{
    ExternalAuthProtocol, ExternalAuthProviderKind, ExternalAuthProviderOptions,
};

pub use links::{cleanup_expired_flows, delete_link, list_links};
pub use login::{finish_callback, start_login};
pub use normalize::callback_redirect_uri;
pub use password_link::link_with_password;
pub use providers::{
    create_provider, delete_provider, get_admin_provider, list_admin_providers,
    list_provider_kinds, list_provider_kinds_with_origin, list_public_providers,
    list_public_providers_by_kind, test_provider, test_provider_params, update_provider,
};
pub(crate) use verification::complete_pending_link;
pub use verification::{confirm_email_verification, start_email_verification};

const FLOW_TTL_SECS: u64 = 300;
const BROWSER_BINDING_SECRET_BYTES: usize = 32;
const EMAIL_VERIFICATION_FLOW_TTL_SECS: u64 = 1_800;
const REDACTED_SECRET: &str = "***REDACTED***";
const EXTERNAL_AUTH_USER_PASSWORD_BYTES: usize = 48;
const EXTERNAL_AUTH_IDENTITY_NAMESPACE_MAX_LEN: usize = 512;
const EXTERNAL_AUTH_URL_MAX_LEN: usize = 2048;
const USERNAME_MAX_LEN: usize = 16;
const USERNAME_MIN_LEN: usize = 4;

/// Anonymous-facing projection of one enabled external auth provider for login screens.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthPublicProvider {
    /// Stable unique key of the provider instance, used in start-login and legacy callback URLs.
    pub key: String,
    /// Provider family that selects the protocol driver.
    pub kind: ExternalAuthProviderKind,
    /// Administrator-configured label shown on login and link-management pages.
    pub display_name: String,
    /// Optional icon URL rendered next to the provider button.
    pub icon_url: Option<String>,
}

/// Static capability descriptor of one supported provider kind, driving the admin creation
/// form and the public description of available login methods.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthProviderKindInfo {
    /// Provider family the descriptor describes.
    pub kind: ExternalAuthProviderKind,
    /// Wire protocol spoken by this kind (OAuth 2.0 or OIDC).
    pub protocol: ExternalAuthProtocol,
    /// Human-readable name of the provider kind.
    pub display_name: String,
    /// Short explanation of what this kind connects to.
    pub description: String,
    /// Space-separated scopes requested when the administrator leaves scopes unset.
    pub default_scopes: String,
    /// Suggested initial settings for creating a provider of this kind.
    pub create_defaults: ExternalAuthProviderCreateDefaults,
    /// Whether `issuer_url` may be configured at all.
    pub issuer_url_supported: bool,
    /// Whether creation fails without an `issuer_url` (discovery-only kinds).
    pub issuer_url_required: bool,
    /// Whether authorization/token/userinfo endpoints can be entered manually.
    pub manual_endpoint_configuration_supported: bool,
    /// Whether `authorization_url` is mandatory when manual endpoint configuration is used.
    pub authorization_url_required: bool,
    /// Whether `token_url` is mandatory when manual endpoint configuration is used.
    pub token_url_required: bool,
    /// Whether `userinfo_url` is mandatory when manual endpoint configuration is used.
    pub userinfo_url_required: bool,
    /// Whether the kind discovers endpoints from the issuer metadata document.
    pub supports_discovery: bool,
    /// Whether the driver issues PKCE with its authorization requests.
    pub supports_pkce: bool,
    /// Whether the kind exposes an `email_verified`-style claim that login policy can rely on.
    pub supports_email_verified_claim: bool,
    /// Absolute unified callback URI for the request origin; empty when no origin is available.
    pub unified_callback_uri: String,
}

/// Scheme and host of the incoming request, used to build absolute provider callback URIs.
#[derive(Clone, Debug)]
pub struct ExternalAuthRequestOrigin {
    /// Request URL scheme (`http` or `https`).
    pub scheme: String,
    /// Request host, including a non-default port.
    pub host: String,
}

/// Initial settings suggested in the admin UI when creating a provider of a given kind.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthProviderCreateDefaults {
    /// Pre-filled display name; empty for kinds that require custom endpoints or tenants.
    pub display_name: String,
    /// Kind-specific options (e.g. the Microsoft tenant) suggested at creation.
    pub options: ExternalAuthProviderOptions,
    /// Suggested space-separated scopes taken from the kind descriptor.
    pub scopes: String,
    /// Whether the provider is enabled right after creation.
    pub enabled: bool,
    /// Default for creating local accounts automatically on first external login.
    pub auto_provision_enabled: bool,
    /// Default for linking identities automatically to accounts with a verified matching email.
    pub auto_link_verified_email_enabled: bool,
    /// Default for rejecting logins whose email is not verified at the provider.
    pub require_email_verified: bool,
}

/// Body of the request that starts an external login.
#[derive(Debug, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthStartLoginRequest {
    /// In-app path to return to after login; normalized and rejected if it points outside the app.
    pub return_path: Option<String>,
}

/// Response of a successfully started external login.
#[derive(Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthStartLoginResponse {
    /// Provider authorization URL the browser must be redirected to.
    pub authorization_url: String,
}

/// Data the route layer turns into a short-lived browser binding cookie for one login flow.
/// The cookie is keyed by the OAuth `state` and must be presented again on the callback, so
/// an authorization response cannot be replayed from a different browser.
pub struct ExternalAuthBrowserBinding {
    /// OAuth `state` value of this flow; also derives the binding cookie name.
    pub state: String,
    /// Random secret persisted only as a hash in the login flow and carried in the cookie.
    pub secret: String,
    /// Cookie lifetime in seconds; equals the login flow TTL.
    pub max_age_secs: i64,
}

/// Service result of starting an external login: the redirect target plus the cookie payload.
pub struct ExternalAuthStartLoginResult {
    /// Authorization URL returned to the client.
    pub response: ExternalAuthStartLoginResponse,
    /// Binding data the route layer persists as a cookie alongside the response.
    pub browser_binding: ExternalAuthBrowserBinding,
}

/// Callback URL shape the browser used, checked against the flow's stored provider
/// so a legacy callback cannot be swapped onto another provider's flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExternalAuthCallbackRoute {
    /// Unified callback shared by all providers; the provider is taken from the flow.
    Unified,
    // TODO(v1.0.0): Remove the legacy callback route after the documented migration window.
    /// Per-provider legacy callback; its path segments must match the flow's provider.
    Legacy {
        /// Provider kind encoded in the legacy callback path.
        provider_kind: ExternalAuthProviderKind,
        /// Provider key encoded in the legacy callback path.
        provider_key: String,
    },
}

/// Body of the request that sends the verification email for a pending external login.
#[derive(Debug, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthEmailVerificationStartRequest {
    /// Flow token issued when the callback deferred the login to email verification.
    pub flow_token: String,
    /// Email address the verification link should be sent to.
    pub email: String,
}

/// Body of the request that links an external identity using password proof as the first factor.
#[derive(Debug, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthPasswordLinkRequest {
    /// Flow token issued when the callback deferred the login to email verification.
    pub flow_token: String,
    /// Local account identifier (username or email) of the account to link.
    pub identifier: String,
    /// Account password verified as the first factor before any identity is written.
    pub password: String,
}

/// Response acknowledging that the verification email was queued.
#[derive(Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthEmailVerificationStartResponse {
    /// Generic confirmation message safe to show to the user.
    pub message: String,
}

/// Query parameters carried by the emailed verification link.
#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(utoipa::IntoParams, utoipa::ToSchema)
)]
pub struct ExternalAuthEmailVerificationConfirmQuery {
    /// One-time verification token from the email link; required to complete the flow.
    pub token: Option<String>,
}

/// Query parameters the OAuth provider appends to the callback redirect (RFC 6749).
#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(utoipa::IntoParams, utoipa::ToSchema)
)]
pub struct ExternalAuthCallbackQuery {
    /// Authorization code to exchange at the provider's token endpoint.
    pub code: Option<String>,
    /// Opaque `state` value used to look up the login flow and its binding cookie.
    pub state: Option<String>,
    /// Error code sent by the provider instead of a code when authorization failed.
    pub error: Option<String>,
    /// Human-readable provider error detail, only surfaced to the login error redirect.
    pub error_description: Option<String>,
}

/// One external identity linked to a user, as shown on the account security page.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthLinkInfo {
    /// Identity row id, used by the delete-link endpoint.
    pub id: i64,
    /// Provider row the identity was created under.
    pub provider_id: i64,
    /// Stable key of the provider instance.
    pub provider_key: String,
    /// Provider family of the provider instance.
    pub provider_kind: ExternalAuthProviderKind,
    /// Provider display name at read time.
    pub provider_display_name: String,
    /// Provider icon URL at read time.
    pub provider_icon_url: Option<String>,
    /// Identity namespace of the provider (issuer URL, or fixed namespace for dedicated kinds).
    pub issuer: String,
    /// Subject claim value that identifies the account at the provider.
    pub subject: String,
    /// Email claimed when the identity was linked; may diverge from the provider's current email.
    pub email_snapshot: Option<String>,
    /// Display name claimed when the identity was linked.
    pub display_name_snapshot: Option<String>,
    /// When the identity link was created.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<Utc>,
    /// When the identity row was last updated.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<Utc>,
    /// When this identity last completed a login; `None` until then.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub last_login_at: Option<chrono::DateTime<Utc>>,
}

/// No credential is included in Debug or public JSON. Construction follows browser proof and
/// first-factor verification; the final transaction independently checks the persisted flow.
#[derive(Clone)]
pub struct PendingExternalLink {
    /// Email-verification flow row that will carry the identity insert once committed.
    pub flow_id: i64,
    /// Raw browser binding proof re-verified against the flow's stored hash at completion.
    pub browser_pfoxum: String,
}
impl std::fmt::Debug for PendingExternalLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingExternalLink")
            .field("flow_id", &self.flow_id)
            .finish_non_exhaustive()
    }
}

/// Fully authenticated primary login produced by an external auth flow, consumed by the
/// route layer to create the session inside one transaction together with any pending link.
#[derive(Debug, Clone)]
pub struct ExternalAuthPrimaryLogin {
    /// Local account the external identity resolved, linked or auto-provisioned to.
    pub user: user::Model,
    /// In-app path to redirect to after the session is established; defaults to `/`.
    pub return_path: String,
    /// Key of the provider instance that performed the login.
    pub provider_key: String,
    /// Identity namespace of the provider (issuer URL, or fixed namespace for dedicated kinds).
    pub issuer: String,
    /// Subject claim value identifying the account at the provider.
    pub subject: String,
    /// Whether the identity link was committed within this login transaction; deferred
    /// links (still awaiting the account's MFA) stay false until that completion runs.
    pub linked: bool,
    /// Whether this login created the local account as a new auto-provisioned user.
    pub auto_provisioned: bool,
    /// Pending association is committed by the authenticated completion transaction only.
    pub pending_link: Option<PendingExternalLink>,
}

/// Full administrator-facing projection of one configured external auth provider.
/// Secrets are redacted; callback URIs are resolved against the current request origin.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct AdminExternalAuthProviderInfo {
    /// Provider row id used by the update, delete and test endpoints.
    pub id: i64,
    /// Immutable unique key generated at creation; names the legacy callback path.
    pub key: String,
    /// Provider family that selects the protocol driver.
    pub provider_kind: ExternalAuthProviderKind,
    /// Wire protocol spoken by this provider.
    pub protocol: ExternalAuthProtocol,
    /// Whether logins redirect to the unified or the legacy per-provider callback path.
    pub callback_mode: ExternalAuthCallbackMode,
    /// Absolute callback URI in effect; administrators register it at the provider.
    pub callback_uri: String,
    /// Absolute legacy per-provider callback URI, kept for existing provider registrations.
    pub legacy_callback_uri: String,
    /// Absolute unified callback URI shared by all providers.
    pub unified_callback_uri: String,
    /// Administrator-configured label shown on login screens.
    pub display_name: String,
    /// Optional icon URL rendered next to the provider button.
    pub icon_url: Option<String>,
    /// Kind-specific driver options (e.g. the Microsoft tenant).
    pub options: ExternalAuthProviderOptions,
    /// Issuer URL used for discovery; `None` for kinds with fixed endpoints or tenant options.
    pub issuer_url: Option<String>,
    /// Manually configured authorization endpoint; only present for manual-endpoint kinds.
    pub authorization_url: Option<String>,
    /// Manually configured token endpoint; only present for manual-endpoint kinds.
    pub token_url: Option<String>,
    /// Manually configured userinfo endpoint; only present for manual-endpoint kinds.
    pub userinfo_url: Option<String>,
    /// OAuth client id issued by the provider.
    pub client_id: String,
    /// Redacted placeholder whenever a secret is stored; the value itself never leaves the server.
    pub client_secret: Option<String>,
    /// Whether a non-empty client secret is stored, without exposing it.
    pub client_secret_configured: bool,
    /// Space-separated OAuth scopes requested during authorization.
    pub scopes: String,
    /// Whether the provider currently accepts logins.
    pub enabled: bool,
    /// Whether first-time external logins create local accounts automatically.
    pub auto_provision_enabled: bool,
    /// Whether identities are linked automatically to accounts with a verified matching email.
    pub auto_link_verified_email_enabled: bool,
    /// Whether logins are rejected unless the provider asserts a verified email.
    pub require_email_verified: bool,
    /// Claim that identifies the account at the provider; `None` uses the protocol default.
    pub subject_claim: Option<String>,
    /// Claim mapped to the local preferred username; `None` uses the protocol default.
    pub username_claim: Option<String>,
    /// Claim mapped to the display name; `None` uses the protocol default.
    pub display_name_claim: Option<String>,
    /// Claim mapped to the email address; `None` uses the protocol default.
    pub email_claim: Option<String>,
    /// Claim asserting the provider verified the email; `None` uses the protocol default.
    pub email_verified_claim: Option<String>,
    /// Claim read as the list of group memberships; `None` disables group sync.
    pub groups_claim: Option<String>,
    /// Claim read as the avatar image URL; `None` disables avatar sync.
    pub avatar_url_claim: Option<String>,
    /// Email domains allowed to log in or link through this provider; empty allows all.
    pub allowed_domains: Vec<String>,
    /// When the provider row was created.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<Utc>,
    /// When the provider row was last updated, including successful connection tests.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<Utc>,
}

/// Administrator input for creating a provider. Endpoint, scope, option and claim fields
/// are validated against the kind's descriptor; omitted policy flags take the kind defaults.
#[derive(Debug, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct CreateExternalAuthProviderInput {
    /// Provider family to create; selects the driver and validation rules.
    pub provider_kind: ExternalAuthProviderKind,
    /// Callback route style; defaults to the unified callback when omitted.
    pub callback_mode: Option<ExternalAuthCallbackMode>,
    /// Label shown on login screens.
    pub display_name: String,
    /// Optional icon URL rendered next to the provider button.
    pub icon_url: Option<String>,
    /// Kind-specific options (e.g. the Microsoft tenant); defaults apply when omitted.
    pub options: Option<ExternalAuthProviderOptions>,
    /// Issuer URL for discovery-based kinds; rejected for kinds with fixed endpoints.
    pub issuer_url: Option<String>,
    /// Manual authorization endpoint; requires a kind that supports manual configuration.
    pub authorization_url: Option<String>,
    /// Manual token endpoint; requires a kind that supports manual configuration.
    pub token_url: Option<String>,
    /// Manual userinfo endpoint; requires a kind that supports manual configuration.
    pub userinfo_url: Option<String>,
    /// OAuth client id issued by the provider.
    pub client_id: String,
    /// OAuth client secret; stored server-side and never returned by any endpoint.
    pub client_secret: Option<String>,
    /// Space-separated scopes; the kind's defaults apply when omitted.
    pub scopes: Option<String>,
    /// Whether the provider accepts logins immediately after creation.
    pub enabled: Option<bool>,
    /// Whether first-time external logins create local accounts automatically.
    pub auto_provision_enabled: Option<bool>,
    /// Whether identities are linked automatically to accounts with a verified matching email.
    pub auto_link_verified_email_enabled: Option<bool>,
    /// Whether logins are rejected unless the provider asserts a verified email.
    pub require_email_verified: Option<bool>,
    /// Claim that identifies the account at the provider; `None` uses the protocol default.
    pub subject_claim: Option<String>,
    /// Claim mapped to the local preferred username; `None` uses the protocol default.
    pub username_claim: Option<String>,
    /// Claim mapped to the display name; `None` uses the protocol default.
    pub display_name_claim: Option<String>,
    /// Claim mapped to the email address; `None` uses the protocol default.
    pub email_claim: Option<String>,
    /// Claim asserting the provider verified the email; `None` uses the protocol default.
    pub email_verified_claim: Option<String>,
    /// Claim read as the list of group memberships; `None` disables group sync.
    pub groups_claim: Option<String>,
    /// Claim read as the avatar image URL; `None` disables avatar sync.
    pub avatar_url_claim: Option<String>,
    /// Email domains allowed to log in or link through this provider; empty allows all.
    pub allowed_domains: Option<Vec<String>>,
}

/// Administrator input for patching a provider. Fields left absent keep their stored value;
/// nullable fields distinguish "clear" (`null`) from "absent" via `NullablePatch`, and the
/// redacted secret placeholder is treated as "keep the stored secret".
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct UpdateExternalAuthProviderInput {
    /// Callback route style to switch to; absent keeps the current mode.
    pub callback_mode: Option<ExternalAuthCallbackMode>,
    /// New label shown on login screens; absent keeps the current one.
    pub display_name: Option<String>,
    /// New icon URL; `null` clears it, absent keeps the current one.
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub icon_url: Option<NullablePatch<String>>,
    /// New issuer URL for discovery-based kinds; `null` clears it, absent keeps the current one.
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub issuer_url: Option<NullablePatch<String>>,
    /// New manual authorization endpoint; `null` clears it, absent keeps the current one.
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub authorization_url: Option<NullablePatch<String>>,
    /// New manual token endpoint; `null` clears it, absent keeps the current one.
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub token_url: Option<NullablePatch<String>>,
    /// New manual userinfo endpoint; `null` clears it, absent keeps the current one.
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub userinfo_url: Option<NullablePatch<String>>,
    /// Kind-specific options to replace wholesale; absent keeps the current ones.
    pub options: Option<ExternalAuthProviderOptions>,
    /// New OAuth client id; absent keeps the current one.
    pub client_id: Option<String>,
    /// New client secret; `null` clears it, absent keeps the current one, and the
    /// redacted placeholder is treated as "keep the stored secret".
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub client_secret: Option<NullablePatch<String>>,
    /// New space-separated scopes; absent keeps the current ones.
    pub scopes: Option<String>,
    /// Toggle whether the provider accepts logins; guarded so local login stays available.
    pub enabled: Option<bool>,
    /// Toggle automatic local account creation on first external login.
    pub auto_provision_enabled: Option<bool>,
    /// Toggle automatic linking to accounts with a verified matching email.
    pub auto_link_verified_email_enabled: Option<bool>,
    /// Toggle rejection of logins without a provider-verified email.
    pub require_email_verified: Option<bool>,
    /// Claim that identifies the account at the provider; `null` resets to the protocol default.
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub subject_claim: Option<NullablePatch<String>>,
    /// Claim mapped to the local preferred username; `null` resets to the protocol default.
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub username_claim: Option<NullablePatch<String>>,
    /// Claim mapped to the display name; `null` resets to the protocol default.
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub display_name_claim: Option<NullablePatch<String>>,
    /// Claim mapped to the email address; `null` resets to the protocol default.
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub email_claim: Option<NullablePatch<String>>,
    /// Claim asserting the provider verified the email; `null` resets to the protocol default.
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub email_verified_claim: Option<NullablePatch<String>>,
    /// Claim read as the list of group memberships; `null` disables group sync.
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub groups_claim: Option<NullablePatch<String>>,
    /// Claim read as the avatar image URL; `null` disables avatar sync.
    #[serde(
        default,
        deserialize_with = "aster_forge_api::deserialize_nullable_patch_option"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub avatar_url_claim: Option<NullablePatch<String>>,
    /// Email domains allowed to log in or link; `null` resets to allow all.
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

/// Connection settings the admin UI can test before a provider row exists. The settings are
/// validated like a create request and exercised against the real provider endpoints.
#[derive(Debug, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct ExternalAuthProviderTestParamsInput {
    /// Provider family whose driver runs the connection test.
    pub provider_kind: ExternalAuthProviderKind,
    /// Kind-specific options (e.g. the Microsoft tenant) under test.
    pub options: Option<ExternalAuthProviderOptions>,
    /// Issuer URL under test for discovery-based kinds.
    pub issuer_url: Option<String>,
    /// Manual authorization endpoint under test.
    pub authorization_url: Option<String>,
    /// Manual token endpoint under test.
    pub token_url: Option<String>,
    /// Manual userinfo endpoint under test.
    pub userinfo_url: Option<String>,
    /// OAuth client id to authenticate with.
    pub client_id: String,
    /// OAuth client secret to authenticate with; not persisted by this endpoint.
    pub client_secret: Option<String>,
    /// Space-separated scopes to request; the kind's defaults apply when omitted.
    pub scopes: Option<String>,
}

/// Provider identity context serialized into audit entries for external logins.
#[derive(Clone, Debug, Serialize)]
pub struct ExternalAuthLoginAuditDetails<'a> {
    /// Key of the provider instance that authenticated the user.
    pub provider_key: &'a str,
    /// Identity namespace of the provider (issuer URL or fixed namespace).
    pub issuer: &'a str,
    /// Subject claim value identifying the account at the provider.
    pub subject: &'a str,
    /// Whether this login committed a new identity link.
    pub linked: bool,
    /// Whether this login created the local account.
    pub auto_provisioned: bool,
}

/// Configuration snapshot serialized into audit entries for provider admin changes.
#[derive(Clone, Debug, Serialize)]
pub struct ExternalAuthProviderAuditDetails<'a> {
    /// Key of the provider instance that was changed.
    pub key: &'a str,
    /// Callback route style at audit time.
    pub callback_mode: ExternalAuthCallbackMode,
    /// Icon URL at audit time.
    pub icon_url: Option<&'a str>,
    /// Issuer URL at audit time.
    pub issuer_url: Option<&'a str>,
    /// Whether the provider accepted logins at audit time.
    pub enabled: bool,
    /// Auto-provisioning policy at audit time.
    pub auto_provision_enabled: bool,
    /// Verified-email auto-link policy at audit time.
    pub auto_link_verified_email_enabled: bool,
    /// Verified-email login requirement at audit time.
    pub require_email_verified: bool,
}

/// Pending email-verification flow handed to the route layer when an external login cannot
/// finish without proving the email. Carries no credential; only opaque tokens.
pub struct PendingExternalAuthEmailVerification {
    /// Email-verification flow row that the follow-up requests operate on.
    pub flow_id: i64,
    /// Raw browser binding proof to re-verify against the flow's stored hash.
    pub browser_pfoxum: String,
    /// Opaque flow token the browser presents to start the email verification.
    pub flow_token: String,
    /// In-app path to return to after the flow completes; defaults to `/`.
    pub return_path: String,
}

/// Result of confirming the emailed verification token: a fully authenticated login whose
/// pending link, if any, is committed by the route layer's completion transaction.
pub struct ExternalAuthEmailVerificationConfirmResult {
    /// Primary login established by the confirmed flow.
    pub primary_login: ExternalAuthPrimaryLogin,
}

/// Result of the password first factor: a fully authenticated login whose pending link is
/// committed by the route layer only after the account's required MFA succeeds.
pub struct ExternalAuthPasswordLinkResult {
    /// Primary login established by the password-verified flow.
    pub primary_login: ExternalAuthPrimaryLogin,
}

/// Outcome of processing an OAuth callback: a completed login, or a pending flow that
/// must first complete the email verification step.
#[expect(
    clippy::large_enum_variant,
    reason = "one-shot service-to-route result; boxing would add a heap allocation without shrinking retained state"
)]
pub enum ExternalAuthCallbackOutcome {
    /// Callback resolved into a fully authenticated primary login.
    Login(ExternalAuthCallbackResult),
    /// Callback deferred the login to email verification; the pending flow is handed back.
    EmailVerificationRequired(PendingExternalAuthEmailVerification),
}

/// Successful callback wrapper handed back to the route layer.
pub struct ExternalAuthCallbackResult {
    /// Primary login established by the callback.
    pub primary_login: ExternalAuthPrimaryLogin,
}
