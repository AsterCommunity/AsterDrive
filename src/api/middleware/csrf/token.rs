//! CSRF 中间件子模块：`token`。

use actix_web::{HttpRequest, dev::ServiceRequest, http::header};
use rand::RngExt;

use crate::api::subcode::ApiSubcode;
use crate::errors::{Result, auth_forbidden_with_subcode};

use super::constants::CSRF_COOKIE;

pub fn build_csrf_token() -> String {
    use base64::Engine;

    let mut bytes = [0_u8; 32];
    rand::rng().fill(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn ensure_double_submit_token(req: &HttpRequest) -> Result<()> {
    let cookie_token = req
        .cookie(CSRF_COOKIE)
        .map(|cookie| cookie.value().to_string())
        .ok_or_else(|| {
            auth_forbidden_with_subcode(ApiSubcode::AuthCsrfCookieMissing, "missing CSRF cookie")
        })?;
    let header_token = req
        .headers()
        .get(header::HeaderName::from_static("x-csrf-token"))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            auth_forbidden_with_subcode(
                ApiSubcode::AuthCsrfHeaderMissing,
                "missing X-CSRF-Token header",
            )
        })?;

    if header_token != cookie_token {
        return Err(auth_forbidden_with_subcode(
            ApiSubcode::AuthCsrfTokenInvalid,
            "invalid CSRF token",
        ));
    }

    Ok(())
}

pub fn ensure_service_double_submit_token(req: &ServiceRequest) -> Result<()> {
    ensure_double_submit_token(req.request())
}
