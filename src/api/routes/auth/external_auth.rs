//! 认证 API 路由：`external-auth`。

use super::cookies::{build_access_cookie, build_csrf_cookie, build_refresh_cookie};
use crate::api::error_code::ErrorCode;
use crate::api::middleware::csrf::{self, RequestSourceMode};
use crate::api::response::ApiResponse;
use crate::config::auth_runtime::RuntimeAuthPolicy;
use crate::config::site_url;
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::audit_service::{self, AuditContext, AuditRequestInfo};
use crate::services::auth_service::{Claims, LoginResult};
use crate::services::external_auth_service::{
    self as external_auth_service, ExternalAuthCallbackOutcome, ExternalAuthCallbackQuery,
    ExternalAuthEmailVerificationConfirmQuery, ExternalAuthEmailVerificationStartRequest,
    ExternalAuthLoginAuditDetails, ExternalAuthPasswordLinkRequest, ExternalAuthStartLoginRequest,
};
use crate::types::ExternalAuthProviderKind;
use crate::utils::numbers::u64_to_i64;
use actix_web::http::header;
use actix_web::{HttpRequest, HttpResponse, web};

fn parse_provider_kind(value: &str) -> Result<ExternalAuthProviderKind> {
    ExternalAuthProviderKind::parse(value).ok_or_else(|| {
        AsterError::record_not_found(format!("external auth provider kind '{value}'"))
    })
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/auth/external-auth/providers",
    tag = "auth",
    operation_id = "list_external_auth_providers",
    responses(
        (status = 200, description = "Enabled external auth providers", body = inline(ApiResponse<Vec<external_auth_service::ExternalAuthPublicProvider>>)),
    ),
)]
pub async fn list_providers(state: web::Data<PrimaryAppState>) -> Result<HttpResponse> {
    let providers = external_auth_service::list_public_providers(&state).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(providers)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/auth/external-auth/{kind}/{provider}/start",
    tag = "auth",
    operation_id = "start_external_auth_login",
    params(
        ("kind" = String, Path, description = "External auth provider kind"),
        ("provider" = String, Path, description = "External auth provider key"),
    ),
    request_body = ExternalAuthStartLoginRequest,
    responses(
        (status = 200, description = "External auth authorization URL", body = inline(ApiResponse<external_auth_service::ExternalAuthStartLoginResponse>)),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Provider not found"),
    ),
)]
pub async fn start_login(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    path: web::Path<(String, String)>,
    body: web::Json<ExternalAuthStartLoginRequest>,
) -> Result<HttpResponse> {
    csrf::ensure_request_source_allowed(&req, &state.runtime_config, RequestSourceMode::Required)?;
    let (kind, provider) = path.into_inner();
    let provider_kind = parse_provider_kind(&kind)?;
    let response = external_auth_service::start_login(
        &state,
        &req,
        provider_kind,
        &provider,
        body.return_path.as_deref(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(response)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/auth/external-auth/{kind}/{provider}/callback",
    tag = "auth",
    operation_id = "finish_external_auth_login",
    params(
        ("kind" = String, Path, description = "External auth provider kind"),
        ("provider" = String, Path, description = "External auth provider key"),
        ExternalAuthCallbackQuery,
    ),
    responses(
        (status = 302, description = "External auth callback completed and redirected"),
        (status = 302, description = "Invalid external auth callback redirected to login"),
    ),
)]
pub async fn finish_login(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    path: web::Path<(String, String)>,
    query: web::Query<ExternalAuthCallbackQuery>,
) -> Result<HttpResponse> {
    let audit_info = AuditRequestInfo::from_request(&req);
    let (kind, provider) = path.into_inner();
    let provider_kind = match parse_provider_kind(&kind) {
        Ok(provider_kind) => provider_kind,
        Err(error) => return Ok(external_auth_error_redirect_response(&state, &error)),
    };
    let result = match external_auth_service::finish_callback(
        &state,
        provider_kind,
        &provider,
        &query,
        audit_info.ip_address.as_deref(),
        audit_info.user_agent.as_deref(),
    )
    .await
    {
        Ok(ExternalAuthCallbackOutcome::Login(result)) => result,
        Ok(ExternalAuthCallbackOutcome::EmailVerificationRequired(pending)) => {
            return Ok(external_auth_email_required_redirect_response(
                &state,
                &pending.flow_token,
                &pending.return_path,
            ));
        }
        Err(error) => return Ok(external_auth_error_redirect_response(&state, &error)),
    };

    external_auth_login_redirect_response(&state, &audit_info, result).await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/auth/external-auth/email-verification/start",
    tag = "auth",
    operation_id = "start_external_auth_email_verification",
    request_body = ExternalAuthEmailVerificationStartRequest,
    responses(
        (status = 200, description = "External auth email verification email queued", body = inline(ApiResponse<external_auth_service::ExternalAuthEmailVerificationStartResponse>)),
        (status = 400, description = "Invalid flow or email"),
        (status = 403, description = "External auth linking or registration is not allowed"),
    ),
)]
pub async fn start_email_verification(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    body: web::Json<ExternalAuthEmailVerificationStartRequest>,
) -> Result<HttpResponse> {
    csrf::ensure_request_source_allowed(&req, &state.runtime_config, RequestSourceMode::Required)?;
    let response =
        external_auth_service::start_email_verification(&state, body.into_inner()).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(response)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/auth/external-auth/password-link",
    tag = "auth",
    operation_id = "link_external_auth_with_password",
    request_body = ExternalAuthPasswordLinkRequest,
    responses(
        (status = 200, description = "External auth identity linked and user logged in", body = inline(ApiResponse<super::AuthTokenResp>)),
        (status = 400, description = "Invalid flow or request"),
        (status = 401, description = "Invalid credentials"),
    ),
)]
pub async fn link_with_password(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    body: web::Json<ExternalAuthPasswordLinkRequest>,
) -> Result<HttpResponse> {
    csrf::ensure_request_source_allowed(&req, &state.runtime_config, RequestSourceMode::Required)?;
    let audit_info = AuditRequestInfo::from_request(&req);
    let result = external_auth_service::link_with_password(
        &state,
        body.into_inner(),
        audit_info.ip_address.as_deref(),
        audit_info.user_agent.as_deref(),
    )
    .await?;
    external_auth_login_json_response(&state, &audit_info, result).await
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/auth/external-auth/email-verification/confirm",
    tag = "auth",
    operation_id = "confirm_external_auth_email_verification",
    params(ExternalAuthEmailVerificationConfirmQuery),
    responses(
        (status = 302, description = "External auth email verification completed and redirected"),
    ),
)]
pub async fn confirm_email_verification(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    query: web::Query<ExternalAuthEmailVerificationConfirmQuery>,
) -> Result<HttpResponse> {
    let Some(token) = query
        .token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
    else {
        return Ok(external_auth_status_redirect_response(
            &state,
            "email_verification_missing",
        ));
    };

    let audit_info = AuditRequestInfo::from_request(&req);
    let result = match external_auth_service::confirm_email_verification(
        &state,
        token,
        audit_info.ip_address.as_deref(),
        audit_info.user_agent.as_deref(),
    )
    .await
    {
        Ok(result) => result,
        Err(AsterError::ContactVerificationExpired(_)) => {
            return Ok(external_auth_status_redirect_response(
                &state,
                "email_verification_expired",
            ));
        }
        Err(AsterError::ContactVerificationInvalid(_)) => {
            return Ok(external_auth_status_redirect_response(
                &state,
                "email_verification_invalid",
            ));
        }
        Err(error) => return Ok(external_auth_error_redirect_response(&state, &error)),
    };

    external_auth_login_redirect_response(&state, &audit_info, result).await
}

async fn external_auth_login_json_response(
    state: &PrimaryAppState,
    audit_info: &AuditRequestInfo,
    result: impl Into<ExternalAuthLoginRedirectResult>,
) -> Result<HttpResponse> {
    let result = result.into();
    let audit_ctx = audit_info.to_context(result.login.user_id);
    audit_service::log(
        state,
        &audit_ctx,
        audit_service::AuditAction::UserExternalAuthLogin,
        crate::services::audit_service::AuditEntityType::ExternalAuthIdentity,
        None,
        Some(&result.provider_key),
        audit_service::details(ExternalAuthLoginAuditDetails {
            provider_key: &result.provider_key,
            issuer: &result.issuer,
            subject: &result.subject,
            linked: result.linked,
            auto_provisioned: result.auto_provisioned,
        }),
    )
    .await;

    let auth_policy = RuntimeAuthPolicy::from_runtime_config(&state.runtime_config);
    let secure = auth_policy.cookie_secure;
    let csrf_token = csrf::build_csrf_token();
    let access_ttl = u64_to_i64(auth_policy.access_token_ttl_secs, "access token ttl")?;
    let refresh_ttl = u64_to_i64(auth_policy.refresh_token_ttl_secs, "refresh token ttl")?;

    Ok(HttpResponse::Ok()
        .cookie(build_access_cookie(
            &result.login.access_token,
            access_ttl,
            secure,
        ))
        .cookie(build_refresh_cookie(
            &result.login.refresh_token,
            refresh_ttl,
            secure,
        ))
        .cookie(build_csrf_cookie(&csrf_token, refresh_ttl, secure))
        .json(ApiResponse::ok(super::AuthTokenResp {
            expires_in: auth_policy.access_token_ttl_secs,
        })))
}

async fn external_auth_login_redirect_response(
    state: &PrimaryAppState,
    audit_info: &AuditRequestInfo,
    result: impl Into<ExternalAuthLoginRedirectResult>,
) -> Result<HttpResponse> {
    let result = result.into();
    let audit_ctx = audit_info.to_context(result.login.user_id);
    audit_service::log(
        state,
        &audit_ctx,
        audit_service::AuditAction::UserExternalAuthLogin,
        crate::services::audit_service::AuditEntityType::ExternalAuthIdentity,
        None,
        Some(&result.provider_key),
        audit_service::details(ExternalAuthLoginAuditDetails {
            provider_key: &result.provider_key,
            issuer: &result.issuer,
            subject: &result.subject,
            linked: result.linked,
            auto_provisioned: result.auto_provisioned,
        }),
    )
    .await;

    let auth_policy = RuntimeAuthPolicy::from_runtime_config(&state.runtime_config);
    let secure = auth_policy.cookie_secure;
    let csrf_token = csrf::build_csrf_token();
    let access_ttl = u64_to_i64(auth_policy.access_token_ttl_secs, "access token ttl")?;
    let refresh_ttl = u64_to_i64(auth_policy.refresh_token_ttl_secs, "refresh token ttl")?;
    let redirect_url = site_url::public_app_url_or_path(&state.runtime_config, &result.return_path);

    Ok(HttpResponse::Found()
        .append_header((header::LOCATION, redirect_url))
        .cookie(build_access_cookie(
            &result.login.access_token,
            access_ttl,
            secure,
        ))
        .cookie(build_refresh_cookie(
            &result.login.refresh_token,
            refresh_ttl,
            secure,
        ))
        .cookie(build_csrf_cookie(&csrf_token, refresh_ttl, secure))
        .finish())
}

struct ExternalAuthLoginRedirectResult {
    login: LoginResult,
    return_path: String,
    provider_key: String,
    issuer: String,
    subject: String,
    linked: bool,
    auto_provisioned: bool,
}

impl From<external_auth_service::ExternalAuthCallbackResult> for ExternalAuthLoginRedirectResult {
    fn from(value: external_auth_service::ExternalAuthCallbackResult) -> Self {
        Self {
            login: value.login,
            return_path: value.return_path,
            provider_key: value.provider_key,
            issuer: value.issuer,
            subject: value.subject,
            linked: value.linked,
            auto_provisioned: value.auto_provisioned,
        }
    }
}

impl From<external_auth_service::ExternalAuthEmailVerificationConfirmResult>
    for ExternalAuthLoginRedirectResult
{
    fn from(value: external_auth_service::ExternalAuthEmailVerificationConfirmResult) -> Self {
        Self {
            login: value.login,
            return_path: value.return_path,
            provider_key: value.provider_key,
            issuer: value.issuer,
            subject: value.subject,
            linked: value.linked,
            auto_provisioned: value.auto_provisioned,
        }
    }
}

impl From<external_auth_service::ExternalAuthPasswordLinkResult>
    for ExternalAuthLoginRedirectResult
{
    fn from(value: external_auth_service::ExternalAuthPasswordLinkResult) -> Self {
        Self {
            login: value.login,
            return_path: value.return_path,
            provider_key: value.provider_key,
            issuer: value.issuer,
            subject: value.subject,
            linked: value.linked,
            auto_provisioned: value.auto_provisioned,
        }
    }
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/auth/external-auth/links",
    tag = "auth",
    operation_id = "list_external_auth_links",
    responses(
        (status = 200, description = "Linked external auth identities", body = inline(ApiResponse<Vec<external_auth_service::ExternalAuthLinkInfo>>)),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_links(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
) -> Result<HttpResponse> {
    let links = external_auth_service::list_links(&state, claims.user_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(links)))
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/auth/external-auth/links/{id}",
    tag = "auth",
    operation_id = "delete_external_auth_link",
    params(("id" = i64, Path, description = "External auth identity link ID")),
    responses(
        (status = 200, description = "External auth identity unlinked"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "External auth identity link not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_link(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let id = path.into_inner();
    if !external_auth_service::delete_link(&state, claims.user_id, id).await? {
        return Err(AsterError::record_not_found(format!(
            "external auth identity link #{id}"
        )));
    }
    let ctx = AuditContext::from_request(&req, &claims);
    audit_service::log(
        &state,
        &ctx,
        audit_service::AuditAction::UserExternalAuthUnlink,
        crate::services::audit_service::AuditEntityType::ExternalAuthIdentity,
        Some(id),
        None,
        None,
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

fn external_auth_error_redirect_response(
    state: &PrimaryAppState,
    error: &AsterError,
) -> HttpResponse {
    tracing::warn!(error = %error, "external auth callback failed");
    let path = if error.http_status().is_server_error() {
        "/login?external_auth=error".to_string()
    } else {
        format!(
            "/login?external_auth=error&code={}",
            ErrorCode::from(error) as u32
        )
    };
    let redirect_url = site_url::public_app_url_or_path(&state.runtime_config, &path);
    HttpResponse::Found()
        .append_header((header::LOCATION, redirect_url))
        .finish()
}

fn external_auth_email_required_redirect_response(
    state: &PrimaryAppState,
    flow_token: &str,
    return_path: &str,
) -> HttpResponse {
    let path = format!(
        "/login?external_auth=email_required&flow={}&return_path={}",
        urlencoding::encode(flow_token),
        urlencoding::encode(return_path)
    );
    let redirect_url = site_url::public_app_url_or_path(&state.runtime_config, &path);
    HttpResponse::Found()
        .append_header((header::LOCATION, redirect_url))
        .finish()
}

fn external_auth_status_redirect_response(state: &PrimaryAppState, status: &str) -> HttpResponse {
    let path = format!("/login?external_auth={}", urlencoding::encode(status));
    let redirect_url = site_url::public_app_url_or_path(&state.runtime_config, &path);
    HttpResponse::Found()
        .append_header((header::LOCATION, redirect_url))
        .finish()
}
