//! 管理员 API 路由：`locks`。

use crate::api::dto::admin::AdminLockListQuery;
use crate::api::response::{ApiResponse, RemovedCountResponse};
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{auth::local::Claims, files::lock, ops::audit};
use actix_web::{HttpRequest, HttpResponse, web};
use aster_forge_api::LimitOffsetQuery;
#[cfg(all(debug_assertions, feature = "openapi"))]
use aster_forge_api::OffsetPage;

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/admin/locks",
    tag = "admin",
    operation_id = "list_locks",
    params(LimitOffsetQuery, AdminLockListQuery),
    responses(
        (status = 200, description = "All WebDAV locks", body = inline(ApiResponse<OffsetPage<lock::ResourceLock>>)),
        (status = 403, description = "Admin required"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_locks(
    state: web::Data<PrimaryAppState>,
    page: web::Query<LimitOffsetQuery>,
    query: web::Query<AdminLockListQuery>,
) -> Result<HttpResponse> {
    let locks = lock::list_paginated(
        state.get_ref(),
        page.limit_or(50, 100),
        page.offset(),
        query.sort_by(),
        query.sort_order(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(locks)))
}

#[aster_forge_api_docs_macros::path(
    delete,
    path = "/api/v1/admin/locks/{id}",
    tag = "admin",
    operation_id = "force_unlock",
    params(("id" = i64, Path, description = "Lock ID")),
    responses(
        (status = 200, description = "Lock released"),
        (status = 403, description = "Admin required"),
        (status = 404, description = "Lock not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn force_unlock(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let ctx = audit::AuditContext::from_request(&req, &claims);
    lock::force_unlock_with_audit(state.get_ref(), *path, &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[aster_forge_api_docs_macros::path(
    delete,
    path = "/api/v1/admin/locks/expired",
    tag = "admin",
    operation_id = "cleanup_expired_locks",
    responses(
        (status = 200, description = "Expired locks cleaned up", body = inline(ApiResponse<crate::api::response::RemovedCountResponse>)),
        (status = 403, description = "Admin required"),
    ),
    security(("bearer" = [])),
)]
pub async fn cleanup_expired_locks(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
) -> Result<HttpResponse> {
    let ctx = audit::AuditContext::from_request(&req, &claims);
    let count = lock::cleanup_expired_with_audit(state.get_ref(), &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(RemovedCountResponse { removed: count })))
}
