//! 认证 API 路由聚合入口。

pub use crate::api::dto::auth::{
    AcceptUserInvitationReq, ActionMessageResp, AuthTokenResp, ChangePasswordReq, CheckResp,
    ContactVerificationConfirmQuery, LoginReq, LoginResponse, MeQuery, PasskeyLoginFinishReq,
    PasskeyLoginStartReq, PasskeyRegisterFinishReq, PasskeyRegisterStartReq,
    PasswordResetConfirmReq, PasswordResetRequestReq, PatchPasskeyReq, RegisterReq,
    RequestEmailChangeReq, ResendRegisterActivationReq, SetupReq, UpdateAvatarSourceReq,
    UpdateProfileReq,
};
use crate::api::middleware::auth::JwtAuth;
use crate::api::middleware::rate_limit;
use crate::api::request_auth::access_token;
use crate::config::site_url;
use crate::config::{NetworkTrustConfig, RateLimitConfig};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{auth::local, storage_change_service};
use actix_governor::Governor;
use actix_web::http::header;
use actix_web::middleware::Condition;
use actix_web::{HttpRequest, HttpResponse, web};
use bytes::Bytes;
use rand::RngExt;

pub use self::external_auth::{
    confirm_email_verification as confirm_external_auth_email_verification,
    delete_link as delete_external_auth_link, finish_login as finish_external_auth_login,
    link_with_password as link_external_auth_with_password, list_links as list_external_auth_links,
    list_providers as list_external_auth_providers,
    start_email_verification as start_external_auth_email_verification,
    start_login as start_external_auth_login,
};
pub use self::mfa::{
    delete_factor as delete_mfa_factor, finish_totp_setup, regenerate_recovery_codes,
    send_email_code as send_mfa_email_code, start_totp_setup, status as mfa_status,
    verify_challenge as verify_mfa_challenge,
};
pub use self::passkeys::{
    delete_passkey, finish_login as finish_passkey_login,
    finish_registration as finish_passkey_registration, list_passkeys, rename_passkey,
    start_login as start_passkey_login, start_registration as start_passkey_registration,
};
pub use self::profile::{
    get_self_avatar, patch_preferences, patch_profile, put_avatar_source, request_email_change,
    resend_email_change, upload_avatar,
};
pub use self::public::{
    accept_user_invitation, check, confirm_contact_verification, confirm_password_reset, register,
    request_password_reset, resend_register_activation, setup, verify_user_invitation,
};
pub use self::session::{
    delete_other_sessions, delete_session, get_storage_events, list_sessions, login, logout, me,
    put_password, refresh,
};
pub use crate::services::profile_service::{AvatarInfo, UserProfileInfo};
pub use crate::services::user_service::{
    MePartialResponse, MeResponse, UpdatePreferencesReq, UserInfo, UserPreferences,
};
pub use crate::types::{
    AvatarSource, BrowserOpenMode, ColorPreset, Language, PrefViewMode, ThemeMode,
};

const AUTH_MAIL_RESPONSE_FLOOR_MS: u64 = 350;
const AUTH_MAIL_RESPONSE_JITTER_MS: u64 = 125;

pub mod cookies;
pub mod external_auth;
pub mod mfa;
pub mod passkeys;
pub mod profile;
pub mod public;
pub mod session;

pub fn routes(
    rl: &RateLimitConfig,
    network_trust: &NetworkTrustConfig,
) -> impl actix_web::dev::HttpServiceFactory + use<> {
    let auth_limiter = rate_limit::build_governor(&rl.auth, &network_trust.trusted_proxies);
    let api_limiter = rate_limit::build_governor(&rl.api, &network_trust.trusted_proxies);

    web::scope("/auth")
        .service(
            web::resource("/check")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route(web::post().to(check)),
        )
        .service(
            web::scope("/register")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route("", web::post().to(register))
                .route("/resend", web::post().to(resend_register_activation)),
        )
        .service(
            web::scope("/invitations")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route("/{token}", web::get().to(verify_user_invitation))
                .route("/{token}/accept", web::post().to(accept_user_invitation)),
        )
        .service(
            web::resource("/setup")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route(web::post().to(setup)),
        )
        .service(
            web::resource("/contact-verification/confirm")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route(web::get().to(confirm_contact_verification)),
        )
        .service(
            web::scope("/password/reset")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route("/request", web::post().to(request_password_reset))
                .route("/confirm", web::post().to(confirm_password_reset)),
        )
        .service(
            web::resource("/login")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route(web::post().to(login)),
        )
        .service(
            web::scope("/mfa/challenge")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route("/verify", web::post().to(verify_mfa_challenge))
                .route("/email-code/send", web::post().to(send_mfa_email_code)),
        )
        .service(
            web::scope("/passkeys/login")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route("/start", web::post().to(start_passkey_login))
                .route("/finish", web::post().to(finish_passkey_login)),
        )
        .service(
            web::resource("/external-auth/providers")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route(web::get().to(list_external_auth_providers)),
        )
        .service(
            web::scope("/external-auth/email-verification")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route(
                    "/start",
                    web::post().to(start_external_auth_email_verification),
                )
                .route(
                    "/confirm",
                    web::get().to(confirm_external_auth_email_verification),
                ),
        )
        .service(
            web::resource("/external-auth/password-link")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route(web::post().to(link_external_auth_with_password)),
        )
        .service(
            web::scope("/external-auth/links")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route("", web::get().to(list_external_auth_links))
                .route("/{id}", web::delete().to(delete_external_auth_link)),
        )
        .service(
            web::scope("/external-auth/{kind}/{provider}")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route("/start", web::post().to(start_external_auth_login))
                .route("/callback", web::get().to(finish_external_auth_login)),
        )
        .service(
            web::resource("/refresh")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route(web::post().to(refresh)),
        )
        .service(
            web::resource("/logout")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route(web::post().to(logout)),
        )
        .service(
            web::resource("/me")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route(web::get().to(me)),
        )
        .service(
            web::scope("/sessions")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route("", web::get().to(list_sessions))
                .route("/others", web::delete().to(delete_other_sessions))
                .route("/{id}", web::delete().to(delete_session)),
        )
        .service(
            web::resource("/password")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route(web::put().to(put_password)),
        )
        .service(
            web::scope("/mfa")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route("", web::get().to(mfa_status))
                .route("/totp/setup/start", web::post().to(start_totp_setup))
                .route("/totp/setup/finish", web::post().to(finish_totp_setup))
                .route("/factors/{id}", web::delete().to(delete_mfa_factor))
                .route(
                    "/recovery-codes/regenerate",
                    web::post().to(regenerate_recovery_codes),
                ),
        )
        .service(
            web::scope("/passkeys")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route("", web::get().to(list_passkeys))
                .route(
                    "/register/start",
                    web::post().to(start_passkey_registration),
                )
                .route(
                    "/register/finish",
                    web::post().to(finish_passkey_registration),
                )
                .route("/{id}", web::patch().to(rename_passkey))
                .route("/{id}", web::delete().to(delete_passkey)),
        )
        .service(
            web::scope("/email/change")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route("", web::post().to(request_email_change))
                .route("/resend", web::post().to(resend_email_change)),
        )
        .service(
            web::resource("/preferences")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route(web::patch().to(patch_preferences)),
        )
        .service(
            web::scope("/profile")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route("", web::patch().to(patch_profile))
                .route("/avatar/upload", web::post().to(upload_avatar))
                .route("/avatar/source", web::put().to(put_avatar_source))
                .route("/avatar/{size}", web::get().to(get_self_avatar)),
        )
        .service(
            web::resource("/events/storage")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route(web::get().to(get_storage_events)),
        )
}

async fn apply_auth_mail_response_floor(started_at: tokio::time::Instant) {
    let mut rng = rand::rng();
    let jitter_ms = rng.random_range(0..=AUTH_MAIL_RESPONSE_JITTER_MS);
    let target = std::time::Duration::from_millis(AUTH_MAIL_RESPONSE_FLOOR_MS + jitter_ms);
    let elapsed = started_at.elapsed();
    if elapsed < target {
        tokio::time::sleep(target - elapsed).await;
    }
}

#[derive(Clone, Copy)]
enum ContactVerificationRedirectStatus {
    EmailChanged,
    Expired,
    Invalid,
    Missing,
    RegisterActivated,
}

impl ContactVerificationRedirectStatus {
    fn as_query_value(self) -> &'static str {
        match self {
            Self::EmailChanged => "email-changed",
            Self::Expired => "expired",
            Self::Invalid => "invalid",
            Self::Missing => "missing",
            Self::RegisterActivated => "register-activated",
        }
    }
}

async fn request_has_active_access_session(state: &PrimaryAppState, req: &HttpRequest) -> bool {
    let Some(token) = access_token(req) else {
        return false;
    };

    local::authenticate_access_token(state, &token)
        .await
        .is_ok()
}

fn contact_verification_redirect_url(
    state: &PrimaryAppState,
    path: &str,
    status: ContactVerificationRedirectStatus,
    email: Option<&str>,
) -> String {
    let mut redirect_path = format!("{path}?contact_verification={}", status.as_query_value());

    if let Some(email) = email {
        redirect_path.push_str("&email=");
        redirect_path.push_str(&urlencoding::encode(email));
    }

    site_url::public_app_url_or_path(state.runtime_config(), &redirect_path)
}

fn contact_verification_redirect_response(
    state: &PrimaryAppState,
    path: &str,
    status: ContactVerificationRedirectStatus,
    email: Option<&str>,
) -> HttpResponse {
    HttpResponse::Found()
        .append_header((
            header::LOCATION,
            contact_verification_redirect_url(state, path, status, email),
        ))
        .finish()
}

fn storage_event_frame(event: &storage_change_service::StorageChangeEvent) -> Option<Bytes> {
    serde_json::to_string(event)
        .map(|json| Bytes::from(format!("data: {json}\n\n")))
        .map_err(|e| tracing::warn!("failed to serialize storage change event: {e}"))
        .ok()
}
