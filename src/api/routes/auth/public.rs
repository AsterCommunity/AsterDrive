//! 认证 API 路由：`public`。

use super::{
    ActionMessageResp, CheckResp, ContactVerificationConfirmQuery,
    ContactVerificationRedirectStatus, PasswordResetConfirmReq, PasswordResetRequestReq,
    RegisterReq, ResendRegisterActivationReq, SetupReq, apply_auth_mail_response_floor,
    contact_verification_redirect_response, request_has_active_access_session,
};
use crate::api::response::ApiResponse;
use crate::config::{auth_runtime::RuntimeAuthPolicy, cors, site_url};
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::audit_service::AuditRequestInfo;
use crate::services::{auth_service, config_service, user_service};
use crate::types::VerificationPurpose;
use actix_web::{HttpRequest, HttpResponse, http::header, web};

fn setup_request_public_origin(req: &HttpRequest) -> Option<String> {
    if let Some(origin) = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .and_then(|origin| cors::normalize_origin(origin, false).ok())
    {
        return Some(origin);
    }

    let conn = req.connection_info();
    cors::normalize_origin(&format!("{}://{}", conn.scheme(), conn.host()), false).ok()
}

async fn bootstrap_public_site_url_from_setup(
    state: &PrimaryAppState,
    req: &HttpRequest,
    user_id: i64,
) {
    if !site_url::public_site_urls(&state.runtime_config).is_empty() {
        return;
    }

    let Some(origin) = setup_request_public_origin(req) else {
        return;
    };

    match config_service::set(
        state,
        site_url::PUBLIC_SITE_URL_KEY,
        vec![origin.clone()],
        user_id,
    )
    .await
    {
        Ok(_) => tracing::info!(origin, "bootstrapped public_site_url from setup request"),
        Err(error) => tracing::warn!(
            origin,
            error = %error,
            "failed to bootstrap public_site_url from setup request"
        ),
    }
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/auth/check",
    tag = "auth",
    operation_id = "check_auth_state",
    responses(
        (status = 200, description = "Check result", body = inline(ApiResponse<CheckResp>)),
    ),
)]
pub async fn check(state: web::Data<PrimaryAppState>) -> Result<HttpResponse> {
    let has_users = auth_service::check_auth_state(&state).await?;
    let auth_policy = RuntimeAuthPolicy::from_runtime_config(&state.runtime_config);
    Ok(HttpResponse::Ok().json(ApiResponse::ok(CheckResp {
        has_users,
        allow_user_registration: auth_policy.allow_user_registration,
        passkey_login_enabled: auth_policy.passkey_login_enabled,
    })))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/auth/setup",
    tag = "auth",
    operation_id = "setup",
    request_body = SetupReq,
    responses(
        (status = 201, description = "Admin account created", body = inline(ApiResponse<crate::api::routes::auth::UserInfo>)),
        (status = 400, description = "System already initialized"),
    ),
)]
pub async fn setup(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    body: web::Json<SetupReq>,
) -> Result<HttpResponse> {
    let audit_info = AuditRequestInfo::from_request(&req);
    let user = auth_service::setup_with_audit(
        &state,
        &body.username,
        &body.email,
        &body.password,
        &audit_info,
    )
    .await?;
    bootstrap_public_site_url_from_setup(&state, &req, user.id).await;
    let user_info = user_service::get_self_info(&state, user.id).await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(user_info)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/auth/register",
    tag = "auth",
    operation_id = "register",
    request_body = RegisterReq,
    responses(
        (status = 201, description = "Registration successful", body = inline(ApiResponse<crate::api::routes::auth::UserInfo>)),
        (status = 400, description = "Validation error"),
    ),
)]
pub async fn register(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    body: web::Json<RegisterReq>,
) -> Result<HttpResponse> {
    let audit_info = AuditRequestInfo::from_request(&req);
    let user = auth_service::register_with_audit(
        &state,
        &body.username,
        &body.email,
        &body.password,
        &audit_info,
    )
    .await?;
    let user_info = user_service::get_self_info(&state, user.id).await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(user_info)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/auth/register/resend",
    tag = "auth",
    operation_id = "resend_register_activation",
    request_body = ResendRegisterActivationReq,
    responses(
        (status = 200, description = "Activation resend request accepted", body = inline(ApiResponse<ActionMessageResp>)),
    ),
)]
pub async fn resend_register_activation(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    body: web::Json<ResendRegisterActivationReq>,
) -> Result<HttpResponse> {
    let started_at = tokio::time::Instant::now();
    let audit_info = AuditRequestInfo::from_request(&req);
    let result =
        auth_service::resend_register_activation_with_audit(&state, &body.identifier, &audit_info)
            .await;
    match result {
        Ok(user) => user,
        Err(error) => {
            apply_auth_mail_response_floor(started_at).await;
            return Err(error);
        }
    };
    apply_auth_mail_response_floor(started_at).await;

    Ok(HttpResponse::Ok().json(ApiResponse::ok(ActionMessageResp {
        message: "If the account can be reactivated, an activation email will be sent".to_string(),
    })))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/auth/contact-verification/confirm",
    tag = "auth",
    operation_id = "confirm_contact_verification",
    params(ContactVerificationConfirmQuery),
    responses(
        (status = 302, description = "Verification consumed and browser redirected to the frontend"),
    ),
)]
pub async fn confirm_contact_verification(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    query: web::Query<ContactVerificationConfirmQuery>,
) -> Result<HttpResponse> {
    let has_active_session = request_has_active_access_session(&state, &req).await;
    let fallback_path = if has_active_session {
        "/settings/security"
    } else {
        "/login"
    };
    let Some(token) = query
        .token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
    else {
        return Ok(contact_verification_redirect_response(
            &state,
            fallback_path,
            ContactVerificationRedirectStatus::Missing,
            None,
        ));
    };

    let audit_info = AuditRequestInfo::from_request(&req);
    let result =
        match auth_service::confirm_contact_verification_with_audit(&state, token, &audit_info)
            .await
        {
            Ok(result) => result,
            Err(AsterError::ContactVerificationInvalid(_)) => {
                return Ok(contact_verification_redirect_response(
                    &state,
                    fallback_path,
                    ContactVerificationRedirectStatus::Invalid,
                    None,
                ));
            }
            Err(AsterError::ContactVerificationExpired(_)) => {
                return Ok(contact_verification_redirect_response(
                    &state,
                    fallback_path,
                    ContactVerificationRedirectStatus::Expired,
                    None,
                ));
            }
            Err(error) => return Err(error),
        };

    if result.purpose == VerificationPurpose::PasswordReset {
        return Ok(contact_verification_redirect_response(
            &state,
            fallback_path,
            ContactVerificationRedirectStatus::Invalid,
            None,
        ));
    }

    let (redirect_path, redirect_status, email) = match result.purpose {
        VerificationPurpose::RegisterActivation if has_active_session => (
            "/settings/security",
            ContactVerificationRedirectStatus::RegisterActivated,
            None,
        ),
        VerificationPurpose::RegisterActivation => (
            "/login",
            ContactVerificationRedirectStatus::RegisterActivated,
            None,
        ),
        VerificationPurpose::ContactChange if has_active_session => (
            "/settings/security",
            ContactVerificationRedirectStatus::EmailChanged,
            Some(result.target.as_str()),
        ),
        VerificationPurpose::ContactChange => (
            "/login",
            ContactVerificationRedirectStatus::EmailChanged,
            Some(result.target.as_str()),
        ),
        VerificationPurpose::PasswordReset => (
            fallback_path,
            ContactVerificationRedirectStatus::Invalid,
            None,
        ),
    };

    Ok(contact_verification_redirect_response(
        &state,
        redirect_path,
        redirect_status,
        email,
    ))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/auth/password/reset/request",
    tag = "auth",
    operation_id = "request_password_reset",
    request_body = PasswordResetRequestReq,
    responses(
        (status = 200, description = "Password reset request accepted", body = inline(ApiResponse<ActionMessageResp>)),
        (status = 400, description = "Invalid email input"),
    ),
)]
pub async fn request_password_reset(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    body: web::Json<PasswordResetRequestReq>,
) -> Result<HttpResponse> {
    let started_at = tokio::time::Instant::now();
    let audit_info = AuditRequestInfo::from_request(&req);
    match auth_service::request_password_reset_with_audit(&state, &body.email, &audit_info).await {
        Ok(_) => {}
        Err(error) => {
            apply_auth_mail_response_floor(started_at).await;
            return Err(error);
        }
    }
    apply_auth_mail_response_floor(started_at).await;

    Ok(HttpResponse::Ok().json(ApiResponse::ok(ActionMessageResp {
        message: "If the account is eligible, a password reset email will be sent".to_string(),
    })))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/auth/password/reset/confirm",
    tag = "auth",
    operation_id = "confirm_password_reset",
    request_body = PasswordResetConfirmReq,
    responses(
        (status = 200, description = "Password reset successful", body = inline(ApiResponse<ActionMessageResp>)),
        (status = 400, description = "Invalid token or password"),
        (status = 410, description = "Reset token expired"),
    ),
)]
pub async fn confirm_password_reset(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    body: web::Json<PasswordResetConfirmReq>,
) -> Result<HttpResponse> {
    let audit_info = AuditRequestInfo::from_request(&req);
    auth_service::confirm_password_reset_with_audit(
        &state,
        &body.token,
        &body.new_password,
        &audit_info,
    )
    .await?;

    Ok(HttpResponse::Ok().json(ApiResponse::ok(ActionMessageResp {
        message: "Password reset successful".to_string(),
    })))
}
