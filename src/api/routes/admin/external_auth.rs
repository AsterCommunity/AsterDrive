//! 管理员 API 路由：`external-auth`。

use crate::api::pagination::LimitOffsetQuery;
#[cfg(all(debug_assertions, feature = "openapi"))]
use crate::api::pagination::OffsetPage;
use crate::api::response::ApiResponse;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::auth::external::{
    self as external, AdminExternalAuthProviderInfo, CreateExternalAuthProviderInput,
    ExternalAuthProviderAuditDetails, ExternalAuthProviderTestParamsInput,
    UpdateExternalAuthProviderInput,
};
use crate::services::auth::local::Claims;
use crate::services::ops::audit;
use actix_web::{HttpRequest, HttpResponse, web};
use serde::Serialize;

fn external_auth_provider_audit_details(
    provider: &AdminExternalAuthProviderInfo,
) -> Option<serde_json::Value> {
    audit::details(ExternalAuthProviderAuditDetails {
        key: &provider.key,
        icon_url: provider.icon_url.as_deref(),
        issuer_url: provider.issuer_url.as_deref(),
        enabled: provider.enabled,
        auto_provision_enabled: provider.auto_provision_enabled,
        auto_link_verified_email_enabled: provider.auto_link_verified_email_enabled,
        require_email_verified: provider.require_email_verified,
    })
}

#[derive(Serialize)]
struct ExternalAuthProviderTestParamsAuditDetails<'a> {
    provider_kind: &'a str,
    key: &'a str,
    success: bool,
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/admin/external-auth/providers",
    tag = "admin",
    operation_id = "admin_list_external_auth_providers",
    params(LimitOffsetQuery),
    responses(
        (status = 200, description = "External auth providers", body = inline(ApiResponse<OffsetPage<external::AdminExternalAuthProviderInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_external_auth_providers(
    state: web::Data<PrimaryAppState>,
    page: web::Query<LimitOffsetQuery>,
) -> Result<HttpResponse> {
    let providers =
        external::list_admin_providers(state.get_ref(), page.limit_or(50, 100), page.offset())
            .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(providers)))
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/admin/external-auth/provider-kinds",
    tag = "admin",
    operation_id = "admin_list_external_auth_provider_kinds",
    responses(
        (status = 200, description = "Supported external auth provider kinds", body = inline(ApiResponse<Vec<external::ExternalAuthProviderKindInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_external_auth_provider_kinds() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(external::list_provider_kinds())))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/admin/external-auth/providers",
    tag = "admin",
    operation_id = "admin_create_external_auth_provider",
    request_body = CreateExternalAuthProviderInput,
    responses(
        (status = 201, description = "External auth provider created", body = inline(ApiResponse<external::AdminExternalAuthProviderInfo>)),
        (status = 400, description = "Invalid provider configuration"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn create_external_auth_provider(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<CreateExternalAuthProviderInput>,
) -> Result<HttpResponse> {
    let provider = external::create_provider(state.get_ref(), body.into_inner()).await?;
    let ctx = audit::AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::AdminCreateExternalAuthProvider,
        crate::services::ops::audit::AuditEntityType::ExternalAuthProvider,
        Some(provider.id),
        Some(&provider.key),
        || external_auth_provider_audit_details(&provider),
    )
    .await;
    Ok(HttpResponse::Created().json(ApiResponse::ok(provider)))
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/admin/external-auth/providers/{id}",
    tag = "admin",
    operation_id = "admin_get_external_auth_provider",
    params(("id" = i64, Path, description = "External auth provider ID")),
    responses(
        (status = 200, description = "External auth provider", body = inline(ApiResponse<external::AdminExternalAuthProviderInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "External auth provider not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn get_external_auth_provider(
    state: web::Data<PrimaryAppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let provider = external::get_admin_provider(state.get_ref(), *path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(provider)))
}

#[aster_forge_api_docs_macros::path(
    patch,
    path = "/api/v1/admin/external-auth/providers/{id}",
    tag = "admin",
    operation_id = "admin_update_external_auth_provider",
    params(("id" = i64, Path, description = "External auth provider ID")),
    request_body = UpdateExternalAuthProviderInput,
    responses(
        (status = 200, description = "External auth provider updated", body = inline(ApiResponse<external::AdminExternalAuthProviderInfo>)),
        (status = 400, description = "Invalid provider configuration"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "External auth provider not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn update_external_auth_provider(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<UpdateExternalAuthProviderInput>,
) -> Result<HttpResponse> {
    let provider = external::update_provider(state.get_ref(), *path, body.into_inner()).await?;
    let ctx = audit::AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::AdminUpdateExternalAuthProvider,
        crate::services::ops::audit::AuditEntityType::ExternalAuthProvider,
        Some(provider.id),
        Some(&provider.key),
        || external_auth_provider_audit_details(&provider),
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(provider)))
}

#[aster_forge_api_docs_macros::path(
    delete,
    path = "/api/v1/admin/external-auth/providers/{id}",
    tag = "admin",
    operation_id = "admin_delete_external_auth_provider",
    params(("id" = i64, Path, description = "External auth provider ID")),
    responses(
        (status = 200, description = "External auth provider deleted"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "External auth provider not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_external_auth_provider(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let provider = external::get_admin_provider(state.get_ref(), *path).await?;
    external::delete_provider(state.get_ref(), *path).await?;
    let ctx = audit::AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::AdminDeleteExternalAuthProvider,
        crate::services::ops::audit::AuditEntityType::ExternalAuthProvider,
        Some(provider.id),
        Some(&provider.key),
        || external_auth_provider_audit_details(&provider),
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/admin/external-auth/providers/test",
    tag = "admin",
    operation_id = "admin_test_external_auth_provider_params",
    request_body = ExternalAuthProviderTestParamsInput,
    responses(
        (status = 200, description = "External auth provider parameters tested", body = inline(ApiResponse<external::ExternalAuthProviderTestResult>)),
        (status = 400, description = "Discovery failed"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn test_external_auth_provider_params(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<ExternalAuthProviderTestParamsInput>,
) -> Result<HttpResponse> {
    let input = body.into_inner();
    let provider_kind = input.provider_kind.as_str();
    let result = external::test_provider_params(state.get_ref(), input).await;
    let success = result.is_ok();
    let ctx = audit::AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::AdminTestExternalAuthProvider,
        crate::services::ops::audit::AuditEntityType::ExternalAuthProvider,
        None,
        Some("draft"),
        || {
            audit::details(ExternalAuthProviderTestParamsAuditDetails {
                provider_kind,
                key: "draft",
                success,
            })
        },
    )
    .await;
    let result = result?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/admin/external-auth/providers/{id}/test",
    tag = "admin",
    operation_id = "admin_test_external_auth_provider",
    params(("id" = i64, Path, description = "External auth provider ID")),
    responses(
        (status = 200, description = "External auth provider tested", body = inline(ApiResponse<external::ExternalAuthProviderTestResult>)),
        (status = 400, description = "Discovery failed"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "External auth provider not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn test_external_auth_provider(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let provider = external::get_admin_provider(state.get_ref(), *path).await?;
    let result = external::test_provider(state.get_ref(), *path).await?;
    let ctx = audit::AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::AdminTestExternalAuthProvider,
        crate::services::ops::audit::AuditEntityType::ExternalAuthProvider,
        Some(provider.id),
        Some(&provider.key),
        || external_auth_provider_audit_details(&provider),
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}
