//! 管理员目录 API。

use crate::api::dto::admin::SetFolderPolicyReq;
use crate::api::dto::validate_request;
use crate::api::response::ApiResponse;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{audit_service, auth::local::Claims, files::folder};
use actix_web::{HttpRequest, HttpResponse, web};

#[api_docs_macros::path(
    put,
    path = "/api/v1/admin/folders/{id}/policy",
    tag = "admin",
    operation_id = "admin_set_folder_policy",
    params(("id" = i64, Path, description = "Folder ID")),
    request_body = SetFolderPolicyReq,
    responses(
        (status = 200, description = "Folder policy binding updated", body = inline(ApiResponse<crate::services::workspace_models::FolderInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Folder or policy not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn set_folder_policy(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<SetFolderPolicyReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let folder =
        folder::admin_set_policy_with_audit(state.get_ref(), *path, body.policy_id, &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(folder)))
}
