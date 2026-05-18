//! API 路由：`properties`。

use crate::api::dto::{
    properties::{EntityPath, PropPath, SetPropReq},
    validate_request,
};
use crate::api::middleware::auth::JwtAuth;
use crate::api::middleware::rate_limit;
use crate::api::response::ApiResponse;
use crate::config::RateLimitConfig;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{audit_service, auth_service::Claims, property_service};
use actix_governor::Governor;
use actix_web::middleware::Condition;
use actix_web::{HttpRequest, HttpResponse, web};

pub fn routes(rl: &RateLimitConfig) -> impl actix_web::dev::HttpServiceFactory + use<> {
    let limiter = rate_limit::build_governor(&rl.api, &rl.trusted_proxies);

    web::scope("/properties")
        .wrap(JwtAuth)
        .wrap(Condition::new(rl.enabled, Governor::new(&limiter)))
        .route("/{entity_type}/{entity_id}", web::get().to(list_props))
        .route("/{entity_type}/{entity_id}", web::put().to(set_prop))
        .route(
            "/{entity_type}/{entity_id}/{namespace}/{name}",
            web::delete().to(delete_prop),
        )
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/properties/{entity_type}/{entity_id}",
    tag = "properties",
    operation_id = "list_properties",
    params(
        ("entity_type" = crate::types::EntityType, Path, description = "Entity type: 'file' or 'folder'"),
        ("entity_id" = i64, Path, description = "Entity ID"),
    ),
    responses(
        (status = 200, description = "Properties list", body = inline(ApiResponse<Vec<property_service::EntityProperty>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "Entity not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_props(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<EntityPath>,
) -> Result<HttpResponse> {
    let path = path.into_inner();
    validate_request(&path)?;
    let props =
        property_service::list(&state, path.entity_type, path.entity_id, claims.user_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(props)))
}

#[api_docs_macros::path(
    put,
    path = "/api/v1/properties/{entity_type}/{entity_id}",
    tag = "properties",
    operation_id = "set_property",
    params(
        ("entity_type" = crate::types::EntityType, Path, description = "Entity type: 'file' or 'folder'"),
        ("entity_id" = i64, Path, description = "Entity ID"),
    ),
    request_body = SetPropReq,
    responses(
        (status = 200, description = "Property set", body = inline(ApiResponse<property_service::EntityProperty>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "DAV: namespace is read-only"),
        (status = 404, description = "Entity not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn set_prop(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<EntityPath>,
    body: web::Json<SetPropReq>,
) -> Result<HttpResponse> {
    let path = path.into_inner();
    let body = body.into_inner();
    validate_request(&path)?;
    validate_request(&body)?;
    let prop = property_service::set(
        &state,
        path.entity_type,
        path.entity_id,
        claims.user_id,
        &body.namespace,
        &body.name,
        body.value.as_deref(),
    )
    .await?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let property_name = format!("{}:{}", body.namespace, body.name);
    audit_service::log(
        &state,
        &ctx,
        audit_service::AuditAction::PropertySet,
        audit_service::AuditEntityType::from_entity_type(path.entity_type),
        Some(path.entity_id),
        Some(&property_name),
        audit_service::details(audit_service::PropertyAuditDetails {
            entity_type: path.entity_type.as_str(),
            namespace: &body.namespace,
            name: &body.name,
        }),
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(prop)))
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/properties/{entity_type}/{entity_id}/{namespace}/{name}",
    tag = "properties",
    operation_id = "delete_property",
    params(
        ("entity_type" = crate::types::EntityType, Path, description = "Entity type: 'file' or 'folder'"),
        ("entity_id" = i64, Path, description = "Entity ID"),
        ("namespace" = String, Path, description = "Property namespace"),
        ("name" = String, Path, description = "Property name"),
    ),
    responses(
        (status = 200, description = "Property deleted"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "DAV: namespace is read-only"),
        (status = 404, description = "Entity not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_prop(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<PropPath>,
) -> Result<HttpResponse> {
    let path = path.into_inner();
    validate_request(&path)?;
    property_service::delete(
        &state,
        path.entity_type,
        path.entity_id,
        claims.user_id,
        &path.namespace,
        &path.name,
    )
    .await?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let property_name = format!("{}:{}", path.namespace, path.name);
    audit_service::log(
        &state,
        &ctx,
        audit_service::AuditAction::PropertyDelete,
        audit_service::AuditEntityType::from_entity_type(path.entity_type),
        Some(path.entity_id),
        Some(&property_name),
        audit_service::details(audit_service::PropertyAuditDetails {
            entity_type: path.entity_type.as_str(),
            namespace: &path.namespace,
            name: &path.name,
        }),
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}
