#![allow(dead_code, unused_imports)]

use actix_web::{body::MessageBody, dev::ServiceResponse};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

pub mod oauth2;
pub mod oidc;

const EXTERNAL_AUTH_COOKIE_PREFIX: &str = "aster_external_auth_";
static EXTERNAL_AUTH_BINDING_COOKIES: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn external_auth_binding_cookie_from_response<B: MessageBody>(
    resp: &ServiceResponse<B>,
) -> String {
    let cookie = external_auth_binding_set_cookie(resp);
    format!("{}={}", cookie.name(), cookie.value())
}

pub fn external_auth_binding_set_cookie<B: MessageBody>(
    resp: &ServiceResponse<B>,
) -> actix_web::cookie::Cookie<'static> {
    resp.response()
        .cookies()
        .find(|cookie| cookie.name().starts_with(EXTERNAL_AUTH_COOKIE_PREFIX))
        .expect("external auth response should set a browser binding cookie")
        .into_owned()
}

pub fn remember_external_auth_binding(state: &str, cookie_header: String) {
    EXTERNAL_AUTH_BINDING_COOKIES
        .lock()
        .expect("external auth binding cookie map should lock")
        .insert(state.to_string(), cookie_header);
}

pub fn external_auth_binding_cookie_header(state: &str) -> String {
    EXTERNAL_AUTH_BINDING_COOKIES
        .lock()
        .expect("external auth binding cookie map should lock")
        .get(state)
        .cloned()
        .expect("external auth binding cookie should be remembered")
}

pub fn external_auth_binding_cookie_name(cookie_header: &str) -> &str {
    cookie_header
        .split_once('=')
        .map(|(name, _)| name)
        .expect("external auth binding cookie should contain a name")
}

pub fn assert_external_auth_binding_cookie_cleared<B: MessageBody>(resp: &ServiceResponse<B>) {
    let cookie = external_auth_binding_set_cookie(resp);
    assert_eq!(cookie.value(), "");
    assert_eq!(cookie.path(), Some("/api/v1/auth/external-auth"));
    assert!(cookie.http_only().unwrap_or(false));
    assert_eq!(cookie.same_site(), Some(actix_web::cookie::SameSite::Lax));
    assert_eq!(cookie.max_age().map(|age| age.whole_seconds()), Some(0));
}
