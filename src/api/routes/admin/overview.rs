//! 管理员 API 路由：`overview`。

use crate::api::response::ApiResponse;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::ops::admin;
use actix_web::{HttpResponse, web};

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/admin/overview",
    tag = "admin",
    operation_id = "get_admin_overview",
    params(admin::AdminOverviewQuery),
    responses(
        (status = 200, description = "Admin overview", body = inline(ApiResponse<crate::services::ops::admin::AdminOverview>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn get_overview(
    state: web::Data<PrimaryAppState>,
    query: web::Query<admin::AdminOverviewQuery>,
) -> Result<HttpResponse> {
    let overview = admin::get_overview(
        state.get_ref(),
        query.days_or_default(),
        query.timezone_name(),
        query.event_limit_or_default(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(overview)))
}
