//! 认证 API 路由：`mfa`。

use crate::api::middleware::csrf::{self, RequestSourceMode};
use crate::api::response::ApiResponse;
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::auth_service::Claims;
use crate::services::{audit_service, mfa_service};
use actix_web::{HttpRequest, HttpResponse, web};
use mfa_service::{MfaChallengeVerifyRequest, MfaEmailCodeSendRequest};

#[api_docs_macros::path(
    post,
    path = "/api/v1/auth/mfa/challenge/verify",
    tag = "auth",
    operation_id = "verify_mfa_challenge",
    request_body = MfaChallengeVerifyRequest,
    responses(
        (status = 200, description = "MFA challenge verified and tokens set in HttpOnly cookies", body = inline(ApiResponse<super::LoginResponse>)),
        (status = 401, description = "Invalid MFA flow or code"),
    ),
)]
pub async fn verify_challenge(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    body: web::Json<MfaChallengeVerifyRequest>,
) -> Result<HttpResponse> {
    csrf::ensure_request_source_allowed(
        &req,
        state.get_ref().runtime_config(),
        RequestSourceMode::OptionalWhenPresent,
    )?;
    let audit_info = audit_service::AuditRequestInfo::from_request_with_trusted_proxies(
        &req,
        &state.get_ref().config().network_trust.trusted_proxies,
    );
    let result = mfa_service::verify_challenge(
        state.get_ref(),
        &body.flow_token,
        body.method.clone().into(),
        &body.code,
        &audit_info,
    )
    .await?;
    super::session::authenticated_login_response(
        state.get_ref(),
        &result.access_token,
        &result.refresh_token,
        result.password_change_required,
    )
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/auth/mfa/challenge/email-code/send",
    tag = "auth",
    operation_id = "send_mfa_email_code",
    request_body = MfaEmailCodeSendRequest,
    responses(
        (status = 200, description = "MFA email code sent", body = inline(ApiResponse<mfa_service::MfaEmailCodeSendResponse>)),
        (status = 401, description = "Invalid MFA flow"),
        (status = 429, description = "Email code resend cooldown is still active"),
    ),
)]
pub async fn send_email_code(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    body: web::Json<MfaEmailCodeSendRequest>,
) -> Result<HttpResponse> {
    csrf::ensure_request_source_allowed(
        &req,
        state.get_ref().runtime_config(),
        RequestSourceMode::OptionalWhenPresent,
    )?;
    let audit_info = audit_service::AuditRequestInfo::from_request_with_trusted_proxies(
        &req,
        &state.get_ref().config().network_trust.trusted_proxies,
    );
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        mfa_service::send_email_code(state.get_ref(), &body.flow_token, &audit_info).await?,
    )))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/auth/mfa",
    tag = "auth",
    operation_id = "get_mfa_status",
    responses(
        (status = 200, description = "Current user's MFA status", body = inline(ApiResponse<mfa_service::MfaStatus>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn status(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        mfa_service::get_status(state.get_ref(), claims.user_id).await?,
    )))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/auth/mfa/totp/setup/start",
    tag = "auth",
    operation_id = "start_totp_setup",
    responses(
        (status = 200, description = "TOTP setup flow", body = inline(ApiResponse<mfa_service::TotpSetupStartResponse>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn start_totp_setup(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        mfa_service::start_totp_setup(state.get_ref(), claims.user_id).await?,
    )))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/auth/mfa/totp/setup/finish",
    tag = "auth",
    operation_id = "finish_totp_setup",
    request_body = mfa_service::TotpSetupFinishRequest,
    responses(
        (status = 200, description = "TOTP MFA enabled and recovery codes returned", body = inline(ApiResponse<mfa_service::TotpSetupFinishResponse>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn finish_totp_setup(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    claims: web::ReqData<Claims>,
    body: web::Json<mfa_service::TotpSetupFinishRequest>,
) -> Result<HttpResponse> {
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        mfa_service::verify_totp_setup(state.get_ref(), claims.user_id, body.into_inner(), &ctx)
            .await?,
    )))
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/auth/mfa/factors/{id}",
    tag = "auth",
    operation_id = "delete_mfa_factor",
    params(("id" = i64, Path, description = "MFA factor ID")),
    request_body = mfa_service::MfaSensitiveActionRequest,
    responses(
        (status = 200, description = "MFA factor deleted"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "MFA factor not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_factor(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
    body: web::Json<mfa_service::MfaSensitiveActionRequest>,
) -> Result<HttpResponse> {
    let factor_id = *path;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    if !mfa_service::delete_factor(
        state.get_ref(),
        claims.user_id,
        factor_id,
        body.into_inner(),
        &ctx,
    )
    .await?
    {
        return Err(AsterError::record_not_found(format!(
            "mfa factor #{factor_id}"
        )));
    }
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/auth/mfa/recovery-codes/regenerate",
    tag = "auth",
    operation_id = "regenerate_mfa_recovery_codes",
    request_body = mfa_service::MfaSensitiveActionRequest,
    responses(
        (status = 200, description = "New recovery codes", body = inline(ApiResponse<Vec<String>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn regenerate_recovery_codes(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    claims: web::ReqData<Claims>,
    body: web::Json<mfa_service::MfaSensitiveActionRequest>,
) -> Result<HttpResponse> {
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        mfa_service::regenerate_recovery_codes(
            state.get_ref(),
            claims.user_id,
            body.into_inner(),
            &ctx,
        )
        .await?,
    )))
}
