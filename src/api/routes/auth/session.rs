//! 认证 API 路由：`session`。

use std::collections::HashSet;

use super::{AuthTokenResp, ChangePasswordReq, storage_event_frame};
use crate::api::middleware::csrf::{self, RequestSourceMode};
use crate::api::request_auth::{access_cookie_token, bearer_token};
use crate::api::response::{ApiResponse, RemovedCountResponse};
use crate::config::auth_runtime::RuntimeAuthPolicy;
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::audit_service::{self, AuditContext, AuditRequestInfo};
use crate::services::auth_service::Claims;
use crate::services::storage_change_service::StorageChangeWorkspace;
use crate::services::{auth_service, team_service, user_service};
use crate::types::TokenType;
use crate::utils::numbers::{u64_to_i64, usize_to_i64};
use actix_web::{HttpRequest, HttpResponse, web};
use bytes::Bytes;
use tokio_util::sync::CancellationToken;

use super::cookies::{
    REFRESH_COOKIE, build_access_cookie, build_csrf_cookie, build_refresh_cookie,
    clear_access_cookie, clear_csrf_cookie, clear_refresh_cookie,
};

fn refresh_cookie_jti(state: &PrimaryAppState, req: &HttpRequest) -> Option<String> {
    let refresh_token = req.cookie(REFRESH_COOKIE)?.value().to_string();
    let claims = auth_service::verify_token(&refresh_token, &state.config.auth.jwt_secret).ok()?;
    if claims.token_type != TokenType::Refresh {
        return None;
    }
    claims.jti
}

async fn revalidate_storage_event_stream(
    state: &PrimaryAppState,
    user_id: i64,
    session_version: i64,
    refresh_visible_teams: bool,
) -> Result<Option<HashSet<i64>>> {
    let snapshot = auth_service::get_auth_snapshot(state, user_id).await?;
    if !snapshot.status.is_active() {
        return Err(AsterError::auth_forbidden("account is disabled"));
    }
    if snapshot.session_version != session_version {
        return Err(AsterError::auth_token_invalid("session revoked"));
    }
    if !refresh_visible_teams {
        return Ok(None);
    }

    team_service::list_user_team_ids(state, user_id, false)
        .await
        .map(Some)
}

async fn wait_for_shutdown_signal(shutdown_token: Option<CancellationToken>) {
    match shutdown_token {
        Some(token) => token.cancelled().await,
        None => std::future::pending::<()>().await,
    }
}

pub async fn get_storage_events(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    shutdown_token: Option<web::Data<CancellationToken>>,
) -> Result<HttpResponse> {
    let user_id = claims.user_id;
    let session_version = claims.session_version;
    let visible_team_ids = revalidate_storage_event_stream(&state, user_id, session_version, true)
        .await?
        .expect("visible teams should be loaded on initial SSE auth check");
    let mut rx = state.storage_change_tx.subscribe();
    let shutdown_token = shutdown_token.map(|token| token.get_ref().clone());

    let stream = async_stream::stream! {
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(15));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut visible_team_ids = visible_team_ids;

        loop {
            tokio::select! {
                biased;
                _ = wait_for_shutdown_signal(shutdown_token.clone()) => {
                    tracing::info!(
                        user_id,
                        "closing storage change event stream during server shutdown"
                    );
                    break;
                }
                _ = heartbeat.tick() => {
                    match revalidate_storage_event_stream(&state, user_id, session_version, true).await {
                        Ok(Some(updated_team_ids)) => {
                            visible_team_ids = updated_team_ids;
                        }
                        Ok(None) => {}
                        Err(error) => {
                            tracing::info!(
                                user_id,
                                error_code = error.code(),
                                error = error.message(),
                                "closing storage change event stream after periodic auth revalidation failed"
                            );
                            break;
                        }
                    }
                    yield Ok::<Bytes, actix_web::Error>(Bytes::from_static(b": keep-alive\n\n"));
                }
                recv = rx.recv() => {
                    match recv {
                        Ok(event) => {
                            let refresh_visible_teams =
                                matches!(event.workspace, Some(StorageChangeWorkspace::Team { .. }));
                            match revalidate_storage_event_stream(
                                &state,
                                user_id,
                                session_version,
                                refresh_visible_teams,
                            )
                            .await
                            {
                                Ok(Some(updated_team_ids)) => {
                                    visible_team_ids = updated_team_ids;
                                }
                                Ok(None) => {}
                                Err(error) => {
                                    tracing::info!(
                                        user_id,
                                        error_code = error.code(),
                                        error = error.message(),
                                        "closing storage change event stream after event auth revalidation failed"
                                    );
                                    break;
                                }
                            }
                            if !event.is_visible_to(user_id, &visible_team_ids) {
                                continue;
                            }
                            if let Some(frame) = storage_event_frame(&event) {
                                yield Ok(frame);
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(user_id, skipped, "storage change event stream lagged");
                            if let Err(error) =
                                revalidate_storage_event_stream(&state, user_id, session_version, false).await
                            {
                                tracing::info!(
                                    user_id,
                                    error_code = error.code(),
                                    error = error.message(),
                                    "closing storage change event stream after lagged auth revalidation failed"
                                );
                                break;
                            }
                            if let Some(frame) = storage_event_frame(
                                &crate::services::storage_change_service::StorageChangeEvent::sync_required(),
                            ) {
                                yield Ok(frame);
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    };

    Ok(HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        .insert_header(("Content-Encoding", "identity"))
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(stream))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/auth/login",
    tag = "auth",
    operation_id = "login",
    request_body = super::LoginReq,
    responses(
        (status = 200, description = "Login successful, tokens set in HttpOnly cookies", body = inline(ApiResponse<AuthTokenResp>)),
        (status = 401, description = "Invalid credentials"),
    ),
)]
pub async fn login(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    body: web::Json<super::LoginReq>,
) -> Result<HttpResponse> {
    csrf::ensure_request_source_allowed(
        &req,
        &state.runtime_config,
        RequestSourceMode::OptionalWhenPresent,
    )?;
    let audit_info = AuditRequestInfo::from_request(&req);
    let result =
        auth_service::login_with_audit(&state, &body.identifier, &body.password, &audit_info)
            .await?;
    let auth_policy = RuntimeAuthPolicy::from_runtime_config(&state.runtime_config);

    let secure = auth_policy.cookie_secure;
    let csrf_token = csrf::build_csrf_token();
    let access_ttl = u64_to_i64(auth_policy.access_token_ttl_secs, "access token ttl")?;
    let refresh_ttl = u64_to_i64(auth_policy.refresh_token_ttl_secs, "refresh token ttl")?;
    Ok(HttpResponse::Ok()
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
        .json(ApiResponse::ok(AuthTokenResp {
            expires_in: auth_policy.access_token_ttl_secs,
        })))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/auth/refresh",
    tag = "auth",
    operation_id = "refresh",
    responses(
        (status = 200, description = "Tokens refreshed, new access/refresh tokens set in HttpOnly cookies", body = inline(ApiResponse<AuthTokenResp>)),
        (status = 401, description = "Invalid refresh token"),
    ),
)]
pub async fn refresh(state: web::Data<PrimaryAppState>, req: HttpRequest) -> Result<HttpResponse> {
    csrf::ensure_request_source_allowed(
        &req,
        &state.runtime_config,
        RequestSourceMode::OptionalWhenPresent,
    )?;
    csrf::ensure_double_submit_token(&req)?;
    let audit_info = AuditRequestInfo::from_request(&req);
    let auth_policy = RuntimeAuthPolicy::from_runtime_config(&state.runtime_config);
    let refresh_tok = req
        .cookie(REFRESH_COOKIE)
        .map(|c| c.value().to_string())
        .ok_or_else(|| AsterError::auth_token_invalid("missing refresh cookie"))?;

    let (access, refresh_token) = auth_service::refresh_tokens(
        &state,
        &refresh_tok,
        audit_info.ip_address.as_deref(),
        audit_info.user_agent.as_deref(),
    )
    .await?;

    let secure = auth_policy.cookie_secure;
    let csrf_token = csrf::build_csrf_token();
    let access_ttl = u64_to_i64(auth_policy.access_token_ttl_secs, "access token ttl")?;
    let refresh_ttl = u64_to_i64(auth_policy.refresh_token_ttl_secs, "refresh token ttl")?;
    Ok(HttpResponse::Ok()
        .cookie(build_access_cookie(&access, access_ttl, secure))
        .cookie(build_refresh_cookie(&refresh_token, refresh_ttl, secure))
        .cookie(build_csrf_cookie(&csrf_token, refresh_ttl, secure))
        .json(ApiResponse::ok(AuthTokenResp {
            expires_in: auth_policy.access_token_ttl_secs,
        })))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/auth/logout",
    tag = "auth",
    operation_id = "logout",
    responses(
        (status = 200, description = "Logged out, cookies cleared"),
    ),
)]
pub async fn logout(state: web::Data<PrimaryAppState>, req: HttpRequest) -> HttpResponse {
    if access_cookie_token(&req).is_some() || req.cookie(REFRESH_COOKIE).is_some() {
        if let Err(error) = csrf::ensure_request_source_allowed(
            &req,
            &state.runtime_config,
            RequestSourceMode::OptionalWhenPresent,
        ) {
            return actix_web::ResponseError::error_response(&error);
        }
        if let Err(error) = csrf::ensure_double_submit_token(&req) {
            return actix_web::ResponseError::error_response(&error);
        }
    }

    let audit_info = AuditRequestInfo::from_request(&req);
    if let Some(refresh_token) = req
        .cookie(REFRESH_COOKIE)
        .map(|cookie| cookie.value().to_string())
        && let Err(error) = auth_service::revoke_refresh_token(&state, &refresh_token).await
    {
        tracing::warn!("failed to revoke refresh token on logout: {error}");
    }
    for token in [
        req.cookie(REFRESH_COOKIE)
            .map(|cookie| cookie.value().to_string()),
        access_cookie_token(&req),
        bearer_token(&req),
    ]
    .into_iter()
    .flatten()
    {
        if auth_service::log_logout_for_token(&state, &token, &audit_info).await {
            break;
        }
    }

    let secure = RuntimeAuthPolicy::from_runtime_config(&state.runtime_config).cookie_secure;
    HttpResponse::Ok()
        .cookie(clear_access_cookie(secure))
        .cookie(clear_refresh_cookie(secure))
        .cookie(clear_csrf_cookie(secure))
        .json(ApiResponse::<()>::ok_empty())
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/auth/me",
    tag = "auth",
    operation_id = "me",
    responses(
        (status = 200, description = "Current user info", body = inline(ApiResponse<crate::api::routes::auth::MeResponse>)),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer" = [])),
)]
pub async fn me(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
) -> Result<HttpResponse> {
    let resp =
        user_service::get_me(&state, claims.user_id, usize_to_i64(claims.exp, "jwt exp")?).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(resp)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/auth/sessions",
    tag = "auth",
    operation_id = "list_auth_sessions",
    responses(
        (status = 200, description = "Active login devices", body = inline(ApiResponse<Vec<crate::services::auth_service::AuthSessionInfo>>)),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_sessions(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    claims: web::ReqData<Claims>,
) -> Result<HttpResponse> {
    let sessions = auth_service::list_auth_sessions(
        &state,
        claims.user_id,
        refresh_cookie_jti(&state, &req).as_deref(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(sessions)))
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/auth/sessions/others",
    tag = "auth",
    operation_id = "revoke_other_auth_sessions",
    responses(
        (status = 200, description = "Other login devices revoked", body = inline(ApiResponse<crate::api::response::RemovedCountResponse>)),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_other_sessions(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    claims: web::ReqData<Claims>,
) -> Result<HttpResponse> {
    let current_refresh_jti = refresh_cookie_jti(&state, &req)
        .ok_or_else(|| AsterError::auth_token_invalid("missing current refresh session"))?;
    let removed =
        auth_service::revoke_other_auth_sessions(&state, claims.user_id, &current_refresh_jti)
            .await?;
    let ctx = AuditContext::from_request(&req, &claims);
    audit_service::log(
        &state,
        &ctx,
        audit_service::AuditAction::UserRevokeOtherSessions,
        Some("auth_session"),
        None,
        None,
        audit_service::details(audit_service::AuthSessionAuditDetails {
            session_id: None,
            removed: Some(removed),
            revoked_current: false,
        }),
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(RemovedCountResponse { removed })))
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/auth/sessions/{id}",
    tag = "auth",
    operation_id = "revoke_auth_session",
    params(("id" = String, Path, description = "Session ID")),
    responses(
        (status = 200, description = "Login device revoked"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Session not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_session(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    claims: web::ReqData<Claims>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let current_refresh_jti = refresh_cookie_jti(&state, &req);
    let revoked_current = auth_service::revoke_auth_session(
        &state,
        claims.user_id,
        path.as_str(),
        current_refresh_jti.as_deref(),
    )
    .await?;
    let ctx = AuditContext::from_request(&req, &claims);
    audit_service::log(
        &state,
        &ctx,
        audit_service::AuditAction::UserRevokeSession,
        Some("auth_session"),
        None,
        Some(path.as_str()),
        audit_service::details(audit_service::AuthSessionAuditDetails {
            session_id: Some(path.as_str()),
            removed: None,
            revoked_current,
        }),
    )
    .await;

    let secure = RuntimeAuthPolicy::from_runtime_config(&state.runtime_config).cookie_secure;
    let mut response = HttpResponse::Ok();
    if revoked_current {
        response
            .cookie(clear_access_cookie(secure))
            .cookie(clear_refresh_cookie(secure))
            .cookie(clear_csrf_cookie(secure));
    }
    Ok(response.json(ApiResponse::<()>::ok_empty()))
}

#[api_docs_macros::path(
    put,
    path = "/api/v1/auth/password",
    tag = "auth",
    operation_id = "change_password",
    request_body = ChangePasswordReq,
    responses(
        (status = 200, description = "Password updated", body = inline(ApiResponse<AuthTokenResp>)),
        (status = 400, description = "Invalid new password"),
        (status = 401, description = "Current password is invalid"),
    ),
    security(("bearer" = [])),
)]
pub async fn put_password(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    claims: web::ReqData<Claims>,
    body: web::Json<ChangePasswordReq>,
) -> Result<HttpResponse> {
    let ctx = AuditContext::from_request(&req, &claims);
    let user = auth_service::change_password_with_audit(
        &state,
        claims.user_id,
        &body.current_password,
        &body.new_password,
        &ctx,
    )
    .await?;
    let auth_policy = RuntimeAuthPolicy::from_runtime_config(&state.runtime_config);
    let (access_token, refresh_token) = auth_service::issue_tokens_for_session(
        &state,
        user.id,
        user.session_version,
        ctx.ip_address.as_deref(),
        ctx.user_agent.as_deref(),
    )
    .await?;

    let secure = auth_policy.cookie_secure;
    let csrf_token = csrf::build_csrf_token();
    let access_ttl = u64_to_i64(auth_policy.access_token_ttl_secs, "access token ttl")?;
    let refresh_ttl = u64_to_i64(auth_policy.refresh_token_ttl_secs, "refresh token ttl")?;
    Ok(HttpResponse::Ok()
        .cookie(build_access_cookie(&access_token, access_ttl, secure))
        .cookie(build_refresh_cookie(&refresh_token, refresh_ttl, secure))
        .cookie(build_csrf_cookie(&csrf_token, refresh_ttl, secure))
        .json(ApiResponse::ok(AuthTokenResp {
            expires_in: auth_policy.access_token_ttl_secs,
        })))
}
