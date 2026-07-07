//! 管理员 API 路由：`audit_logs`。

use crate::api::dto::admin::AdminAuditLogSortQuery;
use crate::api::pagination::LimitOffsetQuery;
#[cfg(all(debug_assertions, feature = "openapi"))]
use crate::api::pagination::OffsetPage;
use crate::api::response::ApiResponse;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::ops::audit;
use actix_web::{HttpResponse, web};

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/audit-logs",
    tag = "admin",
    operation_id = "list_audit_logs",
    params(LimitOffsetQuery, audit::AuditLogFilterQuery, AdminAuditLogSortQuery),
    responses(
        (status = 200, description = "Audit log entries", body = inline(ApiResponse<OffsetPage<audit::AuditLogEntry>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_audit_logs(
    state: web::Data<PrimaryAppState>,
    page: web::Query<LimitOffsetQuery>,
    query: web::Query<audit::AuditLogFilterQuery>,
    sort: web::Query<AdminAuditLogSortQuery>,
) -> Result<HttpResponse> {
    let filters = audit::AuditLogFilters::from_query(&query);
    let page = audit::query(
        state.get_ref(),
        filters,
        page.limit_or(50, 200),
        page.offset(),
        sort.sort_by(),
        sort.sort_order(),
    )
    .await?;

    Ok(HttpResponse::Ok().json(ApiResponse::ok(page)))
}
