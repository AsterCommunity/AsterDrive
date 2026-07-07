//! 管理员 API 路由：`tasks`。

use crate::api::dto::admin::{AdminTaskCleanupReq, AdminTaskListQuery};
use crate::api::dto::validate_request;
use crate::api::pagination::LimitOffsetQuery;
#[cfg(all(debug_assertions, feature = "openapi"))]
use crate::api::pagination::OffsetPage;
use crate::api::response::{ApiResponse, RemovedCountResponse};
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{auth::local::Claims, ops::audit, task_service};
use actix_web::{HttpRequest, HttpResponse, web};

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/tasks",
    tag = "admin",
    operation_id = "admin_list_tasks",
    params(LimitOffsetQuery, AdminTaskListQuery),
    responses(
        (status = 200, description = "All background tasks", body = inline(ApiResponse<OffsetPage<task_service::types::TaskInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_tasks(
    state: web::Data<PrimaryAppState>,
    page: web::Query<LimitOffsetQuery>,
    query: web::Query<AdminTaskListQuery>,
) -> Result<HttpResponse> {
    let page = task_service::list_tasks_paginated_for_admin(
        state.get_ref(),
        page.limit_or(20, 100),
        page.offset(),
        task_service::AdminTaskListFilters {
            kind: query.kind,
            status: query.status,
        },
        query.sort_by(),
        query.sort_order(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(page)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/tasks/cleanup",
    tag = "admin",
    operation_id = "admin_cleanup_tasks",
    request_body = AdminTaskCleanupReq,
    responses(
        (status = 200, description = "Completed tasks cleaned up", body = inline(ApiResponse<RemovedCountResponse>)),
        (status = 400, description = "Validation error"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn cleanup_tasks(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<AdminTaskCleanupReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let removed = task_service::cleanup_tasks_for_admin(
        state.get_ref(),
        task_service::AdminTaskCleanupFilters {
            finished_before: body.finished_before,
            kind: body.kind,
            status: body.status,
        },
    )
    .await?;
    let ctx = audit::AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::AdminCleanupTasks,
        crate::services::ops::audit::AuditEntityType::Task,
        None,
        None,
        || {
            audit::details(audit::AdminTaskCleanupAuditDetails {
                removed,
                finished_before: body.finished_before,
                kind: body.kind,
                status: body.status,
            })
        },
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(RemovedCountResponse { removed })))
}
