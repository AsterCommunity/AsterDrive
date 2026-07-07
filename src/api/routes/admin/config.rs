//! 管理员 API 路由：`config`。

use crate::api::dto::admin::{ExecuteConfigActionReq, ExecuteConfigActionResp, SetConfigReq};
use crate::api::pagination::LimitOffsetQuery;
#[cfg(all(debug_assertions, feature = "openapi"))]
use crate::api::pagination::OffsetPage;
use crate::api::response::ApiResponse;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{audit_service, config_service};
use actix_web::{HttpRequest, HttpResponse, web};

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/config",
    tag = "admin",
    operation_id = "list_config",
    params(LimitOffsetQuery),
    responses(
        (status = 200, description = "List config entries", body = inline(ApiResponse<OffsetPage<config_service::SystemConfig>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_config(
    state: web::Data<PrimaryAppState>,
    query: web::Query<LimitOffsetQuery>,
) -> Result<HttpResponse> {
    let configs =
        config_service::list_paginated(state.get_ref(), query.limit_or(50, 100), query.offset())
            .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(configs)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/config/schema",
    tag = "admin",
    operation_id = "config_schema",
    responses(
        (status = 200, description = "Config schema", body = ApiResponse<Vec<config_service::ConfigSchemaItem>>),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn config_schema() -> Result<HttpResponse> {
    let schema = config_service::get_schema();
    Ok(HttpResponse::Ok().json(ApiResponse::ok(schema)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/config/template-variables",
    tag = "admin",
    operation_id = "config_template_variables",
    responses(
        (status = 200, description = "Template variables", body = ApiResponse<Vec<config_service::TemplateVariableGroup>>),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn config_template_variables() -> Result<HttpResponse> {
    let groups = config_service::list_template_variable_groups();
    Ok(HttpResponse::Ok().json(ApiResponse::ok(groups)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/config/{key}",
    tag = "admin",
    operation_id = "get_config",
    params(("key" = String, Path, description = "Config key")),
    responses(
        (status = 200, description = "Config entry", body = inline(ApiResponse<config_service::SystemConfig>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Config key not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn get_config(
    state: web::Data<PrimaryAppState>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let config = config_service::get_by_key(state.get_ref(), &path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(config)))
}

#[api_docs_macros::path(
    put,
    path = "/api/v1/admin/config/{key}",
    tag = "admin",
    operation_id = "set_config",
    params(("key" = String, Path, description = "Config key")),
    request_body = SetConfigReq,
    responses(
        (status = 200, description = "Config value set", body = inline(ApiResponse<config_service::SystemConfig>)),
        (status = 400, description = "Invalid request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn set_config(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<crate::services::auth::local::Claims>,
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<SetConfigReq>,
) -> Result<HttpResponse> {
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let config = config_service::set_with_audit_and_visibility(
        state.get_ref(),
        &path,
        &body.value,
        body.visibility,
        claims.user_id,
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(config)))
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/admin/config/{key}",
    tag = "admin",
    operation_id = "delete_config",
    params(("key" = String, Path, description = "Config key")),
    responses(
        (status = 200, description = "Config entry deleted"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Config key not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_config(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<crate::services::auth::local::Claims>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    config_service::delete_with_audit(state.get_ref(), &path, &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/config/{key}/action",
    tag = "admin",
    operation_id = "execute_config_action",
    params(("key" = String, Path, description = "Config action target key")),
    request_body = ExecuteConfigActionReq,
    responses(
        (status = 200, description = "Config action executed", body = inline(ApiResponse<ExecuteConfigActionResp>)),
        (status = 400, description = "Invalid request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Config action target not found"),
        (status = 503, description = "Mail service unavailable"),
    ),
    security(("bearer" = [])),
)]
pub async fn execute_config_action(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<crate::services::auth::local::Claims>,
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<ExecuteConfigActionReq>,
) -> Result<HttpResponse> {
    crate::api::dto::validate_request(&*body)?;
    let key = path.into_inner();
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let action_result = config_service::execute_action_with_audit(
        state.get_ref(),
        config_service::ExecuteConfigActionInput {
            key: &key,
            action: body.action,
            actor_user_id: claims.user_id,
            draft_values: body.draft_values.as_ref(),
            target_email: body.target_email.as_deref(),
            value: body.value.as_deref(),
            discovery_url: body.discovery_url.as_deref(),
        },
        &ctx,
    )
    .await?;

    Ok(
        HttpResponse::Ok().json(ApiResponse::ok(ExecuteConfigActionResp {
            message: action_result.message,
            value: action_result.value,
        })),
    )
}
