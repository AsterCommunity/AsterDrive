//! 管理员 API 路由：`storage_migrations`。

use crate::api::dto::admin::{CreateStoragePolicyMigrationReq, DryRunStoragePolicyMigrationReq};
use crate::api::dto::validate_request;
use crate::api::response::ApiResponse;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{ops::audit::AuditContext, auth::local::Claims, task};
use actix_web::HttpRequest;
use actix_web::{HttpResponse, web};

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/storage-migrations",
    tag = "admin",
    operation_id = "create_storage_policy_migration",
    request_body = CreateStoragePolicyMigrationReq,
    responses(
        (status = 200, description = "Storage policy migration task created", body = inline(ApiResponse<task::types::TaskInfo>)),
        (status = 400, description = "Validation error"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn create_storage_policy_migration(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    body: web::Json<CreateStoragePolicyMigrationReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let task = task::storage_migration::create_storage_policy_migration_task(
        state.get_ref(),
        task::storage_migration::CreateStoragePolicyMigrationInput {
            source_policy_id: body.source_policy_id,
            target_policy_id: body.target_policy_id,
            delete_source_after_success: body.delete_source_after_success,
            creator_user_id: claims.user_id,
        },
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(task)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/storage-migrations/dry-run",
    tag = "admin",
    operation_id = "dry_run_storage_policy_migration",
    request_body = DryRunStoragePolicyMigrationReq,
    responses(
        (status = 200, description = "Storage policy migration preflight", body = inline(ApiResponse<task::types::StoragePolicyMigrationDryRun>)),
        (status = 400, description = "Validation error"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn dry_run_storage_policy_migration(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    body: web::Json<DryRunStoragePolicyMigrationReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let dry_run = task::storage_migration::dry_run_storage_policy_migration(
        state.get_ref(),
        task::storage_migration::CreateStoragePolicyMigrationInput {
            source_policy_id: body.source_policy_id,
            target_policy_id: body.target_policy_id,
            delete_source_after_success: body.delete_source_after_success,
            creator_user_id: claims.user_id,
        },
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(dry_run)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/storage-migrations/{task_id}/resume",
    tag = "admin",
    operation_id = "resume_storage_policy_migration",
    params(("task_id" = i64, Path, description = "Storage policy migration task ID")),
    responses(
        (status = 200, description = "Storage policy migration reset for checkpoint resume", body = inline(ApiResponse<task::types::TaskInfo>)),
        (status = 400, description = "Task is not retryable"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Task not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn resume_storage_policy_migration(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let ctx = AuditContext::from_request(&req, &claims);
    let task = task::storage_migration::resume_storage_policy_migration_for_admin(
        state.get_ref(),
        *path,
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(task)))
}
