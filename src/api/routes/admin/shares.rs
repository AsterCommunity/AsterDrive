//! 管理员 API 路由：`shares`。

use crate::api::dto::admin::AdminShareListQuery;
use crate::api::pagination::LimitOffsetQuery;
#[cfg(all(debug_assertions, feature = "openapi"))]
use crate::api::pagination::OffsetPage;
use crate::api::response::ApiResponse;
use crate::errors::Result;
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{audit_service, auth::local::Claims, share_service};
use actix_web::{HttpRequest, HttpResponse, web};

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/shares",
    tag = "admin",
    operation_id = "list_all_shares",
    params(LimitOffsetQuery, AdminShareListQuery),
    responses(
        (status = 200, description = "All shares", body = inline(ApiResponse<OffsetPage<share_service::ShareInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_all_shares(
    state: web::Data<PrimaryAppState>,
    page: web::Query<LimitOffsetQuery>,
    query: web::Query<AdminShareListQuery>,
) -> Result<HttpResponse> {
    let shares = share_service::list_paginated(
        state.get_ref(),
        page.limit_or(50, 100),
        page.offset(),
        query.sort_by(),
        query.sort_order(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(shares)))
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/admin/shares/{id}",
    tag = "admin",
    operation_id = "admin_delete_share",
    params(("id" = i64, Path, description = "Share ID")),
    responses(
        (status = 200, description = "Share deleted"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Share not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn admin_delete_share(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let share = crate::db::repository::share_repo::find_by_id(state.writer_db(), *path).await?;
    share_service::admin_delete_share(state.get_ref(), *path).await?;
    let target = match share_service::share_target_for_share(&share) {
        Ok(target) => Some(target),
        Err(error) => {
            tracing::warn!(
                share_id = share.id,
                "failed to resolve share delete audit target after admin delete: {error}"
            );
            None
        }
    };
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    audit_service::log_with_details(
        state.get_ref(),
        &ctx,
        audit_service::AuditAction::AdminDeleteShare,
        crate::services::audit_service::AuditEntityType::Share,
        Some(*path),
        Some(&share.token),
        || {
            audit_service::details(audit_service::ShareDeleteAuditDetails {
                token: &share.token,
                target_type: target.as_ref().map(|target| target.r#type),
                target_id: target.as_ref().map(|target| target.id),
                team_id: share.team_id,
                has_password: share.password.is_some(),
                expires_at: share.expires_at,
                max_downloads: share.max_downloads,
            })
        },
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}
