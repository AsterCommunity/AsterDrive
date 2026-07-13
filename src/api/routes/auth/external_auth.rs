//! 认证 API 路由：`external-auth`。

use super::cookies::{build_access_cookie, build_csrf_cookie, build_refresh_cookie};
use crate::api::response::ApiResponse;
use crate::config::auth_runtime::RuntimeAuthPolicy;
use crate::config::site_url;
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::auth::external::{
    self as external, ExternalAuthCallbackOutcome, ExternalAuthCallbackQuery,
    ExternalAuthEmailVerificationConfirmQuery, ExternalAuthEmailVerificationStartRequest,
    ExternalAuthLoginAuditDetails, ExternalAuthPasswordLinkRequest, ExternalAuthStartLoginRequest,
};
use crate::services::auth::local::Claims;
use crate::services::auth::mfa::{self, PrimaryLoginCompletion};
use crate::services::ops::audit::{self, AuditContext, AuditRequestInfo};
use crate::types::ExternalAuthProviderKind;
use actix_web::http::header;
use actix_web::{HttpRequest, HttpResponse, web};
use aster_forge_actix_middleware::csrf::{self, RequestSourceMode};
use aster_forge_utils::numbers::u64_to_i64;

fn parse_provider_kind(value: &str) -> Result<ExternalAuthProviderKind> {
    value.parse().map_err(|()| {
        AsterError::record_not_found(format!("external auth provider kind '{value}'"))
    })
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/auth/external-auth/providers",
    tag = "auth",
    operation_id = "list_external_auth_providers",
    responses(
        (status = 200, description = "Enabled external auth providers", body = inline(ApiResponse<Vec<external::ExternalAuthPublicProvider>>)),
    ),
)]
pub async fn list_providers(state: web::Data<PrimaryAppState>) -> Result<HttpResponse> {
    let providers = external::list_public_providers(state.get_ref()).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(providers)))
}

#[aster_forge_api_docs_macros::path(
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
        (status = 200, description = "External auth authorization URL", body = inline(ApiResponse<external::ExternalAuthStartLoginResponse>)),
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
    csrf::ensure_request_source_allowed(
        &req,
        &site_url::public_site_urls(state.get_ref().runtime_config()),
        RequestSourceMode::Required,
    )?;
    let (kind, provider) = path.into_inner();
    let provider_kind = parse_provider_kind(&kind)?;
    let response = external::start_login(
        state.get_ref(),
        &req,
        provider_kind,
        &provider,
        body.return_path.as_deref(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(response)))
}

#[aster_forge_api_docs_macros::path(
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
    let audit_info = AuditRequestInfo::from_request_with_trusted_proxies(
        &req,
        &state.get_ref().config().network_trust.trusted_proxies,
    );
    let (kind, provider) = path.into_inner();
    let provider_kind = match parse_provider_kind(&kind) {
        Ok(provider_kind) => provider_kind,
        Err(error) => {
            return Ok(external_auth_error_redirect_response(
                state.get_ref(),
                &error,
            ));
        }
    };
    let result = match external::finish_callback(
        state.get_ref(),
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
                state.get_ref(),
                &pending.flow_token,
                &pending.return_path,
            ));
        }
        Err(error) => {
            return Ok(external_auth_error_redirect_response(
                state.get_ref(),
                &error,
            ));
        }
    };

    external_auth_login_redirect_response(state.get_ref(), &audit_info, result).await
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/auth/external-auth/email-verification/start",
    tag = "auth",
    operation_id = "start_external_auth_email_verification",
    request_body = ExternalAuthEmailVerificationStartRequest,
    responses(
        (status = 200, description = "External auth email verification email queued", body = inline(ApiResponse<external::ExternalAuthEmailVerificationStartResponse>)),
        (status = 400, description = "Invalid flow or email"),
        (status = 403, description = "External auth linking or registration is not allowed"),
    ),
)]
pub async fn start_email_verification(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    body: web::Json<ExternalAuthEmailVerificationStartRequest>,
) -> Result<HttpResponse> {
    csrf::ensure_request_source_allowed(
        &req,
        &site_url::public_site_urls(state.get_ref().runtime_config()),
        RequestSourceMode::Required,
    )?;
    let response = external::start_email_verification(state.get_ref(), body.into_inner()).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(response)))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/auth/external-auth/password-link",
    tag = "auth",
    operation_id = "link_external_auth_with_password",
    request_body = ExternalAuthPasswordLinkRequest,
    responses(
        (status = 200, description = "External auth identity linked; login completed or MFA challenge required", body = inline(ApiResponse<super::LoginResponse>)),
        (status = 400, description = "Invalid flow or request"),
        (status = 401, description = "Invalid credentials"),
    ),
)]
pub async fn link_with_password(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    body: web::Json<ExternalAuthPasswordLinkRequest>,
) -> Result<HttpResponse> {
    csrf::ensure_request_source_allowed(
        &req,
        &site_url::public_site_urls(state.get_ref().runtime_config()),
        RequestSourceMode::Required,
    )?;
    let audit_info = AuditRequestInfo::from_request_with_trusted_proxies(
        &req,
        &state.get_ref().config().network_trust.trusted_proxies,
    );
    let result = external::link_with_password(
        state.get_ref(),
        body.into_inner(),
        audit_info.ip_address.as_deref(),
        audit_info.user_agent.as_deref(),
    )
    .await?;
    external_auth_login_json_response(state.get_ref(), &audit_info, result).await
}

#[aster_forge_api_docs_macros::path(
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
            state.get_ref(),
            "email_verification_missing",
        ));
    };

    let audit_info = AuditRequestInfo::from_request_with_trusted_proxies(
        &req,
        &state.get_ref().config().network_trust.trusted_proxies,
    );
    let result = match external::confirm_email_verification(
        state.get_ref(),
        token,
        audit_info.ip_address.as_deref(),
        audit_info.user_agent.as_deref(),
    )
    .await
    {
        Ok(result) => result,
        Err(AsterError::ContactVerificationExpired(_)) => {
            return Ok(external_auth_status_redirect_response(
                state.get_ref(),
                "email_verification_expired",
            ));
        }
        Err(AsterError::ContactVerificationInvalid(_)) => {
            return Ok(external_auth_status_redirect_response(
                state.get_ref(),
                "email_verification_invalid",
            ));
        }
        Err(error) => {
            return Ok(external_auth_error_redirect_response(
                state.get_ref(),
                &error,
            ));
        }
    };

    external_auth_login_redirect_response(state.get_ref(), &audit_info, result).await
}

async fn external_auth_login_json_response(
    state: &PrimaryAppState,
    audit_info: &AuditRequestInfo,
    result: impl Into<ExternalAuthLoginRedirectResult>,
) -> Result<HttpResponse> {
    let result = result.into();
    let audit_ctx = audit_info.to_context(result.primary_login.user.id);
    audit::log_with_details(
        state,
        &audit_ctx,
        audit::AuditAction::UserExternalAuthLogin,
        crate::services::ops::audit::AuditEntityType::ExternalAuthIdentity,
        None,
        Some(&result.primary_login.provider_key),
        || {
            audit::details(ExternalAuthLoginAuditDetails {
                provider_key: &result.primary_login.provider_key,
                issuer: &result.primary_login.issuer,
                subject: &result.primary_login.subject,
                linked: result.primary_login.linked,
                auto_provisioned: result.primary_login.auto_provisioned,
            })
        },
    )
    .await;
    log_external_auth_link_if_needed(state, &audit_ctx, &result).await;

    let completion = complete_external_primary_login(state, audit_info, &result).await?;
    super::session::login_completion_response(state, completion)
}

async fn external_auth_login_redirect_response(
    state: &PrimaryAppState,
    audit_info: &AuditRequestInfo,
    result: impl Into<ExternalAuthLoginRedirectResult>,
) -> Result<HttpResponse> {
    let result = result.into();
    let audit_ctx = audit_info.to_context(result.primary_login.user.id);
    audit::log_with_details(
        state,
        &audit_ctx,
        audit::AuditAction::UserExternalAuthLogin,
        crate::services::ops::audit::AuditEntityType::ExternalAuthIdentity,
        None,
        Some(&result.primary_login.provider_key),
        || {
            audit::details(ExternalAuthLoginAuditDetails {
                provider_key: &result.primary_login.provider_key,
                issuer: &result.primary_login.issuer,
                subject: &result.primary_login.subject,
                linked: result.primary_login.linked,
                auto_provisioned: result.primary_login.auto_provisioned,
            })
        },
    )
    .await;
    log_external_auth_link_if_needed(state, &audit_ctx, &result).await;

    let return_path = result.primary_login.return_path.clone();
    let completion = complete_external_primary_login(state, audit_info, &result).await?;
    external_auth_redirect_completion_response(state, completion, &return_path)
}

async fn log_external_auth_link_if_needed(
    state: &PrimaryAppState,
    audit_ctx: &AuditContext,
    result: &ExternalAuthLoginRedirectResult,
) {
    if !result.primary_login.linked {
        return;
    }

    audit::log_with_details(
        state,
        audit_ctx,
        audit::AuditAction::UserExternalAuthLink,
        crate::services::ops::audit::AuditEntityType::ExternalAuthIdentity,
        None,
        Some(&result.primary_login.provider_key),
        || {
            audit::details(ExternalAuthLoginAuditDetails {
                provider_key: &result.primary_login.provider_key,
                issuer: &result.primary_login.issuer,
                subject: &result.primary_login.subject,
                linked: result.primary_login.linked,
                auto_provisioned: result.primary_login.auto_provisioned,
            })
        },
    )
    .await;
}

struct ExternalAuthLoginRedirectResult {
    primary_login: external::ExternalAuthPrimaryLogin,
}

async fn complete_external_primary_login(
    state: &PrimaryAppState,
    audit_info: &AuditRequestInfo,
    result: &ExternalAuthLoginRedirectResult,
) -> Result<PrimaryLoginCompletion> {
    mfa::complete_primary_login_or_start_mfa(
        state,
        &result.primary_login.user,
        crate::types::MfaFirstFactor::ExternalAuth,
        Some(&result.primary_login.return_path),
        audit_info.ip_address.as_deref(),
        audit_info.user_agent.as_deref(),
    )
    .await
}

fn external_auth_redirect_completion_response(
    state: &PrimaryAppState,
    completion: PrimaryLoginCompletion,
    return_path: &str,
) -> Result<HttpResponse> {
    match completion {
        PrimaryLoginCompletion::Authenticated(result) => {
            let auth_policy = RuntimeAuthPolicy::from_runtime_config(state.runtime_config());
            let secure = auth_policy.cookie_secure;
            let csrf_token = csrf::build_csrf_token();
            let access_ttl = u64_to_i64(auth_policy.access_token_ttl_secs, "access token ttl")?;
            let refresh_ttl = u64_to_i64(auth_policy.refresh_token_ttl_secs, "refresh token ttl")?;
            let redirect_url = site_url::public_app_url_or_path(
                state.runtime_config(),
                if result.password_change_required {
                    "/force-password-change"
                } else {
                    return_path
                },
            );

            Ok(HttpResponse::Found()
                .append_header((header::LOCATION, redirect_url))
                .cookie(build_access_cookie(
                    &result.access_token,
                    access_ttl,
                    secure,
                ))
                .cookie(build_refresh_cookie(
                    &result.refresh_token,
                    refresh_ttl,
                    secure,
                ))
                .cookie(build_csrf_cookie(&csrf_token, refresh_ttl, secure))
                .finish())
        }
        PrimaryLoginCompletion::MfaRequired(challenge) => {
            let methods = challenge
                .methods
                .iter()
                .map(|method| method.as_str())
                .collect::<Vec<_>>()
                .join(",");
            let path = format!(
                "/login?mfa=required&flow={}&expires_in={}&methods={}&return_path={}",
                urlencoding::encode(&challenge.flow_token),
                challenge.expires_in,
                urlencoding::encode(&methods),
                urlencoding::encode(return_path)
            );
            let redirect_url = site_url::public_app_url_or_path(state.runtime_config(), &path);
            Ok(HttpResponse::Found()
                .append_header((header::LOCATION, redirect_url))
                .finish())
        }
    }
}

impl From<external::ExternalAuthCallbackResult> for ExternalAuthLoginRedirectResult {
    fn from(value: external::ExternalAuthCallbackResult) -> Self {
        Self {
            primary_login: value.primary_login,
        }
    }
}

impl From<external::ExternalAuthEmailVerificationConfirmResult>
    for ExternalAuthLoginRedirectResult
{
    fn from(value: external::ExternalAuthEmailVerificationConfirmResult) -> Self {
        Self {
            primary_login: value.primary_login,
        }
    }
}

impl From<external::ExternalAuthPasswordLinkResult> for ExternalAuthLoginRedirectResult {
    fn from(value: external::ExternalAuthPasswordLinkResult) -> Self {
        Self {
            primary_login: value.primary_login,
        }
    }
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/auth/external-auth/links",
    tag = "auth",
    operation_id = "list_external_auth_links",
    responses(
        (status = 200, description = "Linked external auth identities", body = inline(ApiResponse<Vec<external::ExternalAuthLinkInfo>>)),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_links(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
) -> Result<HttpResponse> {
    let links = external::list_links(state.get_ref(), claims.user_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(links)))
}

#[aster_forge_api_docs_macros::path(
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
    let link = external::list_links(state.get_ref(), claims.user_id)
        .await?
        .into_iter()
        .find(|link| link.id == id);
    if !external::delete_link(state.get_ref(), claims.user_id, id).await? {
        return Err(AsterError::record_not_found(format!(
            "external auth identity link #{id}"
        )));
    }
    let ctx = AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::UserExternalAuthUnlink,
        crate::services::ops::audit::AuditEntityType::ExternalAuthIdentity,
        Some(id),
        link.as_ref().map(|link| link.provider_key.as_str()),
        || {
            link.as_ref().and_then(|link| {
                audit::details(audit::ExternalAuthUnlinkAuditDetails {
                    provider_key: &link.provider_key,
                    issuer: &link.issuer,
                    subject: &link.subject,
                })
            })
        },
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
            error.api_error_code().as_str()
        )
    };
    let redirect_url = site_url::public_app_url_or_path(state.runtime_config(), &path);
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
    let redirect_url = site_url::public_app_url_or_path(state.runtime_config(), &path);
    HttpResponse::Found()
        .append_header((header::LOCATION, redirect_url))
        .finish()
}

fn external_auth_status_redirect_response(state: &PrimaryAppState, status: &str) -> HttpResponse {
    let path = format!("/login?external_auth={}", urlencoding::encode(status));
    let redirect_url = site_url::public_app_url_or_path(state.runtime_config(), &path);
    HttpResponse::Found()
        .append_header((header::LOCATION, redirect_url))
        .finish()
}
