//! 认证 API 路由：`passkeys`。

use super::{
    PasskeyLoginFinishReq, PasskeyLoginStartReq, PasskeyRegisterFinishReq, PasskeyRegisterStartReq,
    PatchPasskeyReq,
};
use crate::api::response::ApiResponse;
use crate::config::site_url;
use crate::db::repository::passkey_repo;
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::ops::audit::{self, AuditContext, AuditRequestInfo};
use crate::services::{auth::local::Claims, auth::passkey};
use actix_web::{HttpRequest, HttpResponse, web};
use aster_forge_actix_middleware::csrf::{self, RequestSourceMode};
use serde_json::json;

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/auth/passkeys",
    tag = "auth",
    operation_id = "list_passkeys",
    responses(
        (status = 200, description = "Registered passkeys for current user", body = inline(ApiResponse<Vec<passkey::PasskeyInfo>>)),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_passkeys(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
) -> Result<HttpResponse> {
    let items = passkey::list_passkeys(state.get_ref(), claims.user_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(items)))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/auth/passkeys/register/start",
    tag = "auth",
    operation_id = "start_passkey_registration",
    request_body = PasskeyRegisterStartReq,
    responses(
        (status = 200, description = "Passkey registration challenge", body = inline(ApiResponse<passkey::PasskeyRegisterStartResp>)),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer" = [])),
)]
pub async fn start_registration(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    body: web::Json<PasskeyRegisterStartReq>,
) -> Result<HttpResponse> {
    let resp =
        passkey::start_registration(state.get_ref(), claims.user_id, body.name.as_deref()).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(resp)))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/auth/passkeys/register/finish",
    tag = "auth",
    operation_id = "finish_passkey_registration",
    request_body = PasskeyRegisterFinishReq,
    responses(
        (status = 200, description = "Passkey registered", body = inline(ApiResponse<passkey::PasskeyInfo>)),
        (status = 400, description = "Invalid passkey registration"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer" = [])),
)]
pub async fn finish_registration(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    claims: web::ReqData<Claims>,
    body: web::Json<PasskeyRegisterFinishReq>,
) -> Result<HttpResponse> {
    let passkey = passkey::finish_registration(
        state.get_ref(),
        claims.user_id,
        &body.flow_id,
        body.credential.clone(),
        body.name.as_deref(),
    )
    .await?;
    let ctx = AuditContext::from_request(&req, &claims);
    let details = passkey_info_audit_details(&passkey);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::UserPasskeyRegister,
        crate::services::ops::audit::AuditEntityType::Passkey,
        Some(passkey.id),
        Some(&passkey.name),
        || Some(details.clone()),
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(passkey)))
}

#[aster_forge_api_docs_macros::path(
    patch,
    path = "/api/v1/auth/passkeys/{id}",
    tag = "auth",
    operation_id = "rename_passkey",
    params(("id" = i64, Path, description = "Passkey ID")),
    request_body = PatchPasskeyReq,
    responses(
        (status = 200, description = "Passkey renamed", body = inline(ApiResponse<passkey::PasskeyInfo>)),
        (status = 400, description = "Invalid passkey name"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Passkey not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn rename_passkey(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
    body: web::Json<PatchPasskeyReq>,
) -> Result<HttpResponse> {
    let id = path.into_inner();
    let previous = passkey_repo::find_by_id_for_user(state.writer_db(), id, claims.user_id)
        .await?
        .ok_or_else(|| AsterError::record_not_found(format!("passkey #{id}")))?;
    let passkey = passkey::rename_passkey(state.get_ref(), claims.user_id, id, &body.name).await?;
    let ctx = AuditContext::from_request(&req, &claims);
    let details = json!({
        "passkey_id": passkey.id,
        "previous_name": previous.name,
        "next_name": &passkey.name,
        "backup_eligible": passkey.backup_eligible,
        "backed_up": passkey.backed_up,
    });
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::UserPasskeyRename,
        crate::services::ops::audit::AuditEntityType::Passkey,
        Some(passkey.id),
        Some(&passkey.name),
        || Some(details.clone()),
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(passkey)))
}

#[aster_forge_api_docs_macros::path(
    delete,
    path = "/api/v1/auth/passkeys/{id}",
    tag = "auth",
    operation_id = "delete_passkey",
    params(("id" = i64, Path, description = "Passkey ID")),
    responses(
        (status = 200, description = "Passkey deleted"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Passkey not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_passkey(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let id = path.into_inner();
    let passkey = passkey_repo::find_by_id_for_user(state.writer_db(), id, claims.user_id)
        .await?
        .ok_or_else(|| AsterError::record_not_found(format!("passkey #{id}")))?;
    let passkey_name = passkey.name.clone();
    if !passkey::delete_passkey(state.get_ref(), claims.user_id, id).await? {
        return Err(AsterError::record_not_found(format!("passkey #{id}")));
    }
    let ctx = AuditContext::from_request(&req, &claims);
    let details = json!({
        "passkey_id": passkey.id,
        "name": &passkey.name,
        "backup_eligible": passkey.backup_eligible,
        "backed_up": passkey.backed_up,
        "last_used_at": passkey.last_used_at,
    });
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::UserPasskeyDelete,
        crate::services::ops::audit::AuditEntityType::Passkey,
        Some(id),
        Some(&passkey_name),
        || Some(details.clone()),
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/auth/passkeys/login/start",
    tag = "auth",
    operation_id = "start_passkey_login",
    request_body = PasskeyLoginStartReq,
    responses(
        (status = 200, description = "Passkey login challenge", body = inline(ApiResponse<passkey::PasskeyLoginStartResp>)),
        (status = 401, description = "Invalid credentials"),
    ),
)]
pub async fn start_login(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    body: web::Json<PasskeyLoginStartReq>,
) -> Result<HttpResponse> {
    csrf::ensure_request_source_allowed(
        &req,
        &site_url::public_site_urls(state.get_ref().runtime_config()),
        RequestSourceMode::Required,
    )?;
    let resp = passkey::start_login(
        state.get_ref(),
        body.identifier.as_deref(),
        body.conditional.unwrap_or(false),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(resp)))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/auth/passkeys/login/finish",
    tag = "auth",
    operation_id = "finish_passkey_login",
    request_body = PasskeyLoginFinishReq,
    responses(
        (status = 200, description = "Passkey login successful or password change required, tokens set in HttpOnly cookies", body = inline(ApiResponse<super::LoginResponse>)),
        (status = 401, description = "Invalid credentials"),
    ),
)]
pub async fn finish_login(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    body: web::Json<PasskeyLoginFinishReq>,
) -> Result<HttpResponse> {
    csrf::ensure_request_source_allowed(
        &req,
        &site_url::public_site_urls(state.get_ref().runtime_config()),
        RequestSourceMode::OptionalWhenPresent,
    )?;
    let audit_info = AuditRequestInfo::from_request_with_trusted_proxies(
        &req,
        &state.get_ref().config().network_trust.trusted_proxies,
    );
    let result = passkey::finish_login(
        state.get_ref(),
        &body.flow_id,
        body.credential.clone(),
        audit_info.ip_address.as_deref(),
        audit_info.user_agent.as_deref(),
    )
    .await?;
    let audit_ctx = audit_info.to_context(result.login.user_id);
    let details = json!({
        "passkey_id": result.passkey_id,
        "name": &result.passkey_name,
        "password_change_required": result.login.password_change_required,
    });
    audit::log_with_details(
        state.get_ref(),
        &audit_ctx,
        audit::AuditAction::UserPasskeyLogin,
        audit::AuditEntityType::Passkey,
        Some(result.passkey_id),
        Some(&result.passkey_name),
        || Some(details.clone()),
    )
    .await;

    super::session::authenticated_login_response(
        state.get_ref(),
        &result.login.access_token,
        &result.login.refresh_token,
        result.login.password_change_required,
    )
}

fn passkey_info_audit_details(passkey: &passkey::PasskeyInfo) -> serde_json::Value {
    json!({
        "passkey_id": passkey.id,
        "name": &passkey.name,
        "backup_eligible": passkey.backup_eligible,
        "backed_up": passkey.backed_up,
        "sign_count": passkey.sign_count,
        "last_used_at": passkey.last_used_at,
    })
}
