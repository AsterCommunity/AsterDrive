//! 管理员 API 路由：`files` / `file-blobs` observability and maintenance.

use crate::api::dto::admin::{
    AdminFileBlobListQuery, AdminFileListQuery, CreateBlobMaintenanceTaskReq,
};
use crate::api::dto::validate_request;
use crate::api::pagination::LimitOffsetQuery;
#[cfg(all(debug_assertions, feature = "openapi"))]
use crate::api::pagination::OffsetPage;
use crate::api::response::ApiResponse;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{admin_file_service, audit_service, auth_service::Claims, task_service};
use actix_web::{HttpRequest, HttpResponse, web};

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/files",
    tag = "admin",
    operation_id = "admin_list_files",
    params(LimitOffsetQuery, AdminFileListQuery),
    responses(
        (status = 200, description = "List files with current blob summary", body = inline(ApiResponse<OffsetPage<crate::api::dto::admin::AdminFileInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_files(
    state: web::Data<PrimaryAppState>,
    page: web::Query<LimitOffsetQuery>,
    query: web::Query<AdminFileListQuery>,
) -> Result<HttpResponse> {
    let page =
        admin_file_service::list_files(&state, page.limit_or(50, 100), page.offset(), &query)
            .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(page)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/files/{id}",
    tag = "admin",
    operation_id = "admin_get_file",
    params(("id" = i64, Path, description = "File ID")),
    responses(
        (status = 200, description = "File details with current blob and version summaries", body = inline(ApiResponse<crate::api::dto::admin::AdminFileDetail>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = crate::api::constants::OPENAPI_NOT_FOUND),
    ),
    security(("bearer" = [])),
)]
pub async fn get_file(
    state: web::Data<PrimaryAppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let file = admin_file_service::get_file(&state, *path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(file)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/file-blobs",
    tag = "admin",
    operation_id = "admin_list_file_blobs",
    params(LimitOffsetQuery, AdminFileBlobListQuery),
    responses(
        (status = 200, description = "List file blobs", body = inline(ApiResponse<OffsetPage<crate::api::dto::admin::AdminFileBlobInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_file_blobs(
    state: web::Data<PrimaryAppState>,
    page: web::Query<LimitOffsetQuery>,
    query: web::Query<AdminFileBlobListQuery>,
) -> Result<HttpResponse> {
    let page =
        admin_file_service::list_blobs(&state, page.limit_or(50, 100), page.offset(), &query)
            .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(page)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/file-blobs/{id}",
    tag = "admin",
    operation_id = "admin_get_file_blob",
    params(("id" = i64, Path, description = "File blob ID")),
    responses(
        (status = 200, description = "File blob details with file and version references", body = inline(ApiResponse<crate::api::dto::admin::AdminFileBlobDetail>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = crate::api::constants::OPENAPI_NOT_FOUND),
    ),
    security(("bearer" = [])),
)]
pub async fn get_file_blob(
    state: web::Data<PrimaryAppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let blob = admin_file_service::get_blob(&state, *path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(blob)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/file-blobs/maintenance",
    tag = "admin",
    operation_id = "admin_create_blob_maintenance_task",
    request_body = CreateBlobMaintenanceTaskReq,
    responses(
        (status = 200, description = "Blob maintenance task created", body = inline(ApiResponse<task_service::TaskInfo>)),
        (status = 400, description = "Validation error"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = crate::api::constants::OPENAPI_NOT_FOUND),
    ),
    security(("bearer" = [])),
)]
pub async fn create_blob_maintenance_task(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<CreateBlobMaintenanceTaskReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let task = task_service::create_blob_maintenance_task_for_admin(
        &state,
        claims.user_id,
        body.action,
        body.blob_ids.clone(),
    )
    .await?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    audit_service::log(
        &state,
        &ctx,
        audit_service::AuditAction::AdminCreateBlobMaintenanceTask,
        audit_service::AuditEntityType::Task,
        Some(task.id),
        Some(&task.display_name),
        audit_service::details(audit_service::AdminBlobMaintenanceAuditDetails {
            action: body.action,
            blob_ids: body.blob_ids.as_deref(),
        }),
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(task)))
}
