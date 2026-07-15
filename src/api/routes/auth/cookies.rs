//! 认证 API 路由：`cookies`。

use crate::api::request_auth::ACCESS_COOKIE;
use actix_web::cookie::time::Duration as CookieDuration;
use actix_web::cookie::{Cookie, SameSite};
use aster_forge_actix_middleware::csrf::CSRF_COOKIE;
use aster_forge_crypto as hash;

pub(super) const REFRESH_COOKIE: &str = "aster_refresh";
const ACCESS_COOKIE_PATH: &str = "/";
const REFRESH_COOKIE_PATH: &str = "/api/v1/auth";
const EXTERNAL_AUTH_COOKIE_PATH: &str = "/api/v1/auth/external-auth";
const EXTERNAL_AUTH_COOKIE_PREFIX: &str = "aster_external_auth_";

fn build_cookie(
    name: &str,
    path: &str,
    value: &str,
    max_age_secs: i64,
    secure: bool,
) -> Cookie<'static> {
    Cookie::build(name.to_string(), value.to_string())
        .path(path.to_string())
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .max_age(CookieDuration::seconds(max_age_secs))
        .finish()
}

fn clear_cookie(name: &str, path: &str, secure: bool) -> Cookie<'static> {
    Cookie::build(name.to_string(), "")
        .path(path.to_string())
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .max_age(CookieDuration::ZERO)
        .finish()
}

pub(super) fn build_access_cookie(value: &str, max_age_secs: i64, secure: bool) -> Cookie<'static> {
    build_cookie(
        ACCESS_COOKIE,
        ACCESS_COOKIE_PATH,
        value,
        max_age_secs,
        secure,
    )
}

pub(super) fn build_refresh_cookie(
    value: &str,
    max_age_secs: i64,
    secure: bool,
) -> Cookie<'static> {
    build_cookie(
        REFRESH_COOKIE,
        REFRESH_COOKIE_PATH,
        value,
        max_age_secs,
        secure,
    )
}

pub(super) fn clear_access_cookie(secure: bool) -> Cookie<'static> {
    clear_cookie(ACCESS_COOKIE, ACCESS_COOKIE_PATH, secure)
}

pub(super) fn clear_refresh_cookie(secure: bool) -> Cookie<'static> {
    clear_cookie(REFRESH_COOKIE, REFRESH_COOKIE_PATH, secure)
}

pub(super) fn build_external_auth_binding_cookie(
    state: &str,
    value: &str,
    max_age_secs: i64,
    secure: bool,
) -> Cookie<'static> {
    build_cookie(
        &external_auth_binding_cookie_name(state),
        EXTERNAL_AUTH_COOKIE_PATH,
        value,
        max_age_secs,
        secure,
    )
}

pub(super) fn external_auth_binding_cookie_name(state: &str) -> String {
    format!(
        "{EXTERNAL_AUTH_COOKIE_PREFIX}{}",
        hash::sha256_hex(state.as_bytes())
    )
}

pub(super) fn clear_external_auth_binding_cookie(name: &str, secure: bool) -> Cookie<'static> {
    clear_cookie(name, EXTERNAL_AUTH_COOKIE_PATH, secure)
}

pub(super) fn build_csrf_cookie(value: &str, max_age_secs: i64, secure: bool) -> Cookie<'static> {
    Cookie::build(CSRF_COOKIE.to_string(), value.to_string())
        .path("/".to_string())
        .http_only(false)
        .same_site(SameSite::Lax)
        .secure(secure)
        .max_age(CookieDuration::seconds(max_age_secs))
        .finish()
}

pub(super) fn clear_csrf_cookie(secure: bool) -> Cookie<'static> {
    Cookie::build(CSRF_COOKIE.to_string(), "")
        .path("/".to_string())
        .http_only(false)
        .same_site(SameSite::Lax)
        .secure(secure)
        .max_age(CookieDuration::ZERO)
        .finish()
}
