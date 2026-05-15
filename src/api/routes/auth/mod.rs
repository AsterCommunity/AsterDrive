//! 认证 API 路由聚合入口。

pub use crate::api::dto::auth::*;
use crate::api::middleware::auth::JwtAuth;
use crate::api::middleware::rate_limit;
use crate::api::request_auth::access_token;
use crate::config::RateLimitConfig;
use crate::config::site_url;
use crate::runtime::PrimaryAppState;
use crate::services::{auth_service, storage_change_service};
use actix_governor::Governor;
use actix_web::http::header;
use actix_web::middleware::Condition;
use actix_web::{HttpRequest, HttpResponse, web};
use bytes::Bytes;
use rand::RngExt;

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
    check, confirm_contact_verification, confirm_password_reset, register, request_password_reset,
    resend_register_activation, setup,
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
pub mod passkeys;
pub mod profile;
pub mod public;
pub mod session;

pub fn routes(rl: &RateLimitConfig) -> impl actix_web::dev::HttpServiceFactory + use<> {
    let auth_limiter = rate_limit::build_governor(&rl.auth, &rl.trusted_proxies);
    let api_limiter = rate_limit::build_governor(&rl.api, &rl.trusted_proxies);

    web::scope("/auth")
        .service(
            web::resource("/check")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route(web::post().to(check)),
        )
        .service(
            web::resource("/register")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route(web::post().to(register)),
        )
        .service(
            web::resource("/register/resend")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route(web::post().to(resend_register_activation)),
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
            web::resource("/password/reset/request")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route(web::post().to(request_password_reset)),
        )
        .service(
            web::resource("/password/reset/confirm")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route(web::post().to(confirm_password_reset)),
        )
        .service(
            web::resource("/login")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route(web::post().to(login)),
        )
        .service(
            web::resource("/passkeys/login/start")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route(web::post().to(start_passkey_login)),
        )
        .service(
            web::resource("/passkeys/login/finish")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route(web::post().to(finish_passkey_login)),
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
            web::resource("/sessions")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route(web::get().to(list_sessions)),
        )
        .service(
            web::resource("/sessions/others")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route(web::delete().to(delete_other_sessions)),
        )
        .service(
            web::resource("/sessions/{id}")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route(web::delete().to(delete_session)),
        )
        .service(
            web::resource("/password")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route(web::put().to(put_password)),
        )
        .service(
            web::resource("/passkeys")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route(web::get().to(list_passkeys)),
        )
        .service(
            web::resource("/passkeys/register/start")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route(web::post().to(start_passkey_registration)),
        )
        .service(
            web::resource("/passkeys/register/finish")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route(web::post().to(finish_passkey_registration)),
        )
        .service(
            web::resource("/passkeys/{id}")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route(web::patch().to(rename_passkey))
                .route(web::delete().to(delete_passkey)),
        )
        .service(
            web::resource("/email/change")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route(web::post().to(request_email_change)),
        )
        .service(
            web::resource("/email/change/resend")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route(web::post().to(resend_email_change)),
        )
        .service(
            web::resource("/preferences")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route(web::patch().to(patch_preferences)),
        )
        .service(
            web::resource("/profile")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route(web::patch().to(patch_profile)),
        )
        .service(
            web::resource("/profile/avatar/upload")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route(web::post().to(upload_avatar)),
        )
        .service(
            web::resource("/profile/avatar/source")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route(web::put().to(put_avatar_source)),
        )
        .service(
            web::resource("/events/storage")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route(web::get().to(get_storage_events)),
        )
        .service(
            web::resource("/profile/avatar/{size}")
                .wrap(JwtAuth)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route(web::get().to(get_self_avatar)),
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

    auth_service::authenticate_access_token(state, &token)
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

    site_url::public_app_url_or_path(&state.runtime_config, &redirect_path)
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
