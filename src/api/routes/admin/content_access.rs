//! Administrator-only transport for governed workspace reads and download initialization.
#[cfg(all(debug_assertions, feature = "openapi"))]
use crate::api::response::ApiResponse;
use crate::{
    errors::Result,
    runtime::PrimaryAppState,
    services::{
        auth::local::Claims,
        files::file,
        ops::{
            audit,
            content_access::{self, *},
        },
    },
};
use actix_web::{HttpRequest, HttpResponse, web};

#[aster_forge_api_docs_macros::path(get, path = "/api/v1/admin/content-access/status", tag = "admin", operation_id = "admin_content_access_status",
 responses((status = 200, body = inline(ApiResponse<AdminContentAccessStatus>))), security(("bearer" = [])))]
pub async fn status(state: web::Data<PrimaryAppState>) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(crate::api::response::success(AdminContentAccessStatus {
            enabled: content_access::is_enabled(state.get_ref()).await?,
        })))
}
#[aster_forge_api_docs_macros::path(post, path = "/api/v1/admin/content-access", tag = "admin", operation_id = "admin_create_content_access",
 request_body = CreateAdminContentAccess, responses((status = 200, body = inline(ApiResponse<AdminContentAccessInfo>))), security(("bearer" = [])))]
pub async fn create(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<CreateAdminContentAccess>,
) -> Result<HttpResponse> {
    let ctx = audit::AuditContext::from_request(&req, &claims);
    let info = content_access::create(state.get_ref(), &claims, &ctx, &body).await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(crate::api::response::success(info)))
}
#[derive(serde::Deserialize)]
pub struct WorkspaceAccessQuery {
    pub access_id: String,
    pub folder_id: Option<i64>,
    #[serde(default)]
    pub deleted: bool,
}
#[aster_forge_api_docs_macros::path(get, path = "/api/v1/admin/content-access/workspace", tag = "admin", operation_id = "admin_workspace_contents",
 params(("access_id" = String, Query), ("folder_id" = Option<i64>, Query), ("deleted" = Option<bool>, Query), ("folder_limit" = Option<u64>, Query), ("folder_offset" = Option<u64>, Query),
 ("file_limit" = Option<u64>, Query), ("file_after_value" = Option<String>, Query), ("file_after_id" = Option<i64>, Query),
 ("sort_by" = Option<crate::api::pagination::SortBy>, Query), ("sort_order" = Option<aster_forge_api::SortOrder>, Query)),
 responses((status = 200, body = inline(ApiResponse<AdminWorkspaceContents>))), security(("bearer" = [])))]
pub async fn workspace(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    query: web::Query<WorkspaceAccessQuery>,
    pagination: web::Query<crate::api::pagination::FolderListQuery>,
) -> Result<HttpResponse> {
    let info = content_access::list_workspace(
        state.get_ref(),
        &claims,
        &AdminWorkspaceQuery {
            access_id: query.access_id.clone(),
            folder_id: query.folder_id,
            deleted: query.deleted,
            pagination: pagination.into_inner(),
        },
    )
    .await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(crate::api::response::success(info)))
}
#[aster_forge_api_docs_macros::path(post, path = "/api/v1/admin/files/{id}/download-init", tag = "admin", operation_id = "admin_file_download_init",
 params(("id" = i64, Path)), request_body = AdminDownloadInitRequest,
 responses((status = 200, body = inline(ApiResponse<AdminDownloadInfo>))), security(("bearer" = [])))]
pub async fn init_download(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<AdminDownloadInitRequest>,
) -> Result<HttpResponse> {
    let ctx = audit::AuditContext::from_request(&req, &claims);
    let info = content_access::init_download(state.get_ref(), &claims, &ctx, *path, &body).await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(crate::api::response::success(info)))
}
#[aster_forge_api_docs_macros::path(get, path = "/api/v1/admin/content-access/{id}/download", tag = "admin", operation_id = "admin_download_content",
 params(("id" = String, Path), ("Range" = Option<String>, Header), ("If-None-Match" = Option<String>, Header)),
 responses((status = 200, description = "Attachment stream"), (status = 206, description = "Partial content"), (status = 304, description = "Not modified"), (status = 400, description = "Invalid or unsatisfiable range")), security(("bearer" = [])))]
pub async fn download(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let outcome = content_access::download(
        state.get_ref(),
        &claims,
        &path,
        req.headers()
            .get("If-None-Match")
            .and_then(|h| h.to_str().ok()),
        req.headers().get("Range"),
    )
    .await?;
    Ok(file::outcome_to_response(outcome))
}

#[derive(serde::Deserialize)]
pub struct ContentFileQuery {
    pub access_id: String,
}
#[aster_forge_api_docs_macros::path(get, path = "/api/v1/admin/content-access/files/{id}", tag = "admin", operation_id = "admin_workspace_file",
 params(("id" = i64, Path), ("access_id" = String, Query)),
 responses((status = 200, body = inline(ApiResponse<crate::services::workspace::models::FileInfo>))), security(("bearer" = [])))]
pub async fn get_file(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
    query: web::Query<ContentFileQuery>,
) -> Result<HttpResponse> {
    let info = content_access::get_file(state.get_ref(), &claims, &query.access_id, *path).await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(crate::api::response::success(info)))
}

#[aster_forge_api_docs_macros::path(get, path = "/api/v1/admin/content-access/files/{id}/versions", tag = "admin", operation_id = "admin_workspace_versions",
 params(("id" = i64, Path), ("access_id" = String, Query), ("after_sequence" = Option<i64>, Query), ("limit" = Option<u64>, Query)),
 responses((status = 200, body = inline(ApiResponse<Vec<crate::services::workspace::models::FileVersion>>))), security(("bearer" = [])))]
pub async fn list_versions(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
    access: web::Query<ContentFileQuery>,
    query: web::Query<crate::services::workspace::models::FileVersionListQuery>,
) -> Result<HttpResponse> {
    let versions = content_access::list_versions(
        state.get_ref(),
        &claims,
        &access.access_id,
        *path,
        query.into_inner(),
    )
    .await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(crate::api::response::success(versions)))
}

#[aster_forge_api_docs_macros::path(get, path = "/api/v1/admin/content-access/trash", tag = "admin", operation_id = "admin_workspace_trash",
 params(("access_id" = String, Query), ("folder_limit" = Option<u64>, Query), ("folder_offset" = Option<u64>, Query), ("file_limit" = Option<u64>, Query), ("file_after_expires_at" = Option<String>, Query), ("file_after_id" = Option<i64>, Query)),
 responses((status = 200, body = inline(ApiResponse<crate::services::files::trash::TrashContents>))), security(("bearer" = [])))]
pub async fn list_trash(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    access: web::Query<ContentFileQuery>,
    query: web::Query<crate::api::pagination::TrashListQuery>,
) -> Result<HttpResponse> {
    let contents = content_access::list_trash(
        state.get_ref(),
        &claims,
        &access.access_id,
        query.into_inner(),
    )
    .await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(crate::api::response::success(contents)))
}

#[aster_forge_api_docs_macros::path(get, path = "/api/v1/admin/content-access/files/{id}/thumbnail", tag = "admin", operation_id = "admin_workspace_thumbnail",
 params(("id" = i64, Path), ("access_id" = String, Query), ("If-None-Match" = Option<String>, Header)),
 responses((status = 200, description = "WebP thumbnail"), (status = 202, description = "Thumbnail generation pending"), (status = 304, description = "Not modified")), security(("bearer" = [])))]
pub async fn get_thumbnail(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    access: web::Query<ContentFileQuery>,
) -> Result<HttpResponse> {
    let result =
        content_access::get_thumbnail(state.get_ref(), &claims, &access.access_id, *path).await?;
    match result {
        Some(result) => Ok(crate::api::routes::files::access::thumbnail_response(
            result,
            req.headers()
                .get("If-None-Match")
                .and_then(|value| value.to_str().ok()),
            "no-store".to_string(),
        )),
        None => Ok(HttpResponse::Accepted()
            .insert_header(("Cache-Control", "no-store"))
            .insert_header(("Retry-After", "2"))
            .json(crate::api::response::empty_success())),
    }
}

#[aster_forge_api_docs_macros::path(get, path = "/api/v1/admin/content-access/folders/{id}", tag = "admin", operation_id = "admin_workspace_folder",
 params(("id" = i64, Path), ("access_id" = String, Query)),
 responses((status = 200, body = inline(ApiResponse<AdminFolderInfo>))), security(("bearer" = [])))]
pub async fn get_folder(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
    access: web::Query<ContentFileQuery>,
) -> Result<HttpResponse> {
    let info =
        content_access::get_folder(state.get_ref(), &claims, &access.access_id, *path).await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(crate::api::response::success(info)))
}
