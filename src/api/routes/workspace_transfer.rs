//! API 路由：跨工作空间资源复制。

use crate::api::dto::batch::{WorkspaceRef, WorkspaceTransferCopyReq};
use crate::api::dto::validate_request;
use crate::api::middleware::auth::JwtAuth;
use crate::api::middleware::rate_limit;
use crate::api::response::ApiResponse;
use crate::config::{NetworkTrustConfig, RateLimitConfig};
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{
    audit_service::AuditContext, auth::local::Claims, files::batch,
    workspace::storage::WorkspaceStorageScope,
};
use actix_governor::Governor;
use actix_web::middleware::Condition;
use actix_web::{HttpRequest, HttpResponse, web};

pub fn routes(
    rl: &RateLimitConfig,
    network_trust: &NetworkTrustConfig,
) -> impl actix_web::dev::HttpServiceFactory + use<> {
    let limiter = rate_limit::build_governor(&rl.write, &network_trust.trusted_proxies);

    web::scope("/workspace-transfer")
        .wrap(JwtAuth)
        .wrap(Condition::new(rl.enabled, Governor::new(&limiter)))
        .route("/copy", web::post().to(copy_to_workspace))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/workspace-transfer/copy",
    tag = "batch",
    operation_id = "workspace_transfer_copy",
    request_body = WorkspaceTransferCopyReq,
    responses(
        (status = 200, description = "Workspace transfer copy result", body = inline(ApiResponse<batch::BatchResult>)),
        (status = 400, description = "Invalid request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn copy_to_workspace(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<WorkspaceTransferCopyReq>,
) -> Result<HttpResponse> {
    let body = body.into_inner();
    validate_request(&body)?;

    let source_scope = workspace_ref_to_scope(&body.source_workspace, claims.user_id);
    let dest_scope = workspace_ref_to_scope(&body.destination_workspace, claims.user_id);
    let ctx = AuditContext::from_request(&req, &claims);
    let result = batch::batch_copy_between_scopes_with_audit(
        state.get_ref(),
        source_scope,
        dest_scope,
        &body.file_ids,
        &body.folder_ids,
        body.target_folder_id,
        &ctx,
    )
    .await?;

    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

fn workspace_ref_to_scope(value: &WorkspaceRef, actor_user_id: i64) -> WorkspaceStorageScope {
    match value {
        WorkspaceRef::Personal => WorkspaceStorageScope::Personal {
            user_id: actor_user_id,
        },
        WorkspaceRef::Team { team_id } => WorkspaceStorageScope::Team {
            team_id: *team_id,
            actor_user_id,
        },
    }
}
