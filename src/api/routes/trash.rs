//! API 路由：`trash`。

use crate::api::dto::TrashItemPath;
use crate::api::middleware::auth::JwtAuth;
use crate::api::middleware::rate_limit;
use crate::api::pagination::TrashListQuery;
use crate::api::response::ApiResponse;
use crate::config::{NetworkTrustConfig, RateLimitConfig};
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{
    audit_service, auth_service::Claims, task_service, trash_service,
    workspace_storage_service::WorkspaceStorageScope,
};
use crate::types::EntityType;
use actix_governor::Governor;
use actix_web::middleware::Condition;
use actix_web::{HttpRequest, HttpResponse, web};

pub fn routes(
    rl: &RateLimitConfig,
    network_trust: &NetworkTrustConfig,
) -> impl actix_web::dev::HttpServiceFactory + use<> {
    let limiter = rate_limit::build_governor(&rl.api, &network_trust.trusted_proxies);

    web::scope("/trash")
        .wrap(JwtAuth)
        .wrap(Condition::new(rl.enabled, Governor::new(&limiter)))
        .route("", web::get().to(list_trash))
        .route("", web::delete().to(purge_all))
        .route("/{entity_type}/{id}/restore", web::post().to(restore))
        .route("/{entity_type}/{id}", web::delete().to(purge_one))
}

pub fn team_routes() -> actix_web::Scope {
    web::scope("/{team_id}/trash")
        .route("", web::get().to(team_list_trash))
        .route("", web::delete().to(team_purge_all))
        .route("/{entity_type}/{id}/restore", web::post().to(team_restore))
        .route("/{entity_type}/{id}", web::delete().to(team_purge_one))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/trash",
    tag = "trash",
    operation_id = "list_trash",
    params(TrashListQuery),
    responses(
        (status = 200, description = "Trash contents", body = inline(ApiResponse<trash_service::TrashContents>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn list_trash(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    query: web::Query<TrashListQuery>,
) -> Result<HttpResponse> {
    let file_cursor = query.file_cursor().map(|(expires_at, id)| {
        trash_service::expires_cursor_to_deleted_cursor(&state, expires_at, id)
    });
    let contents = trash_service::list_trash(
        &state,
        claims.user_id,
        query.folder_limit(),
        query.folder_offset(),
        query.file_limit(),
        file_cursor,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(contents)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/trash/{entity_type}/{id}/restore",
    tag = "trash",
    operation_id = "restore_from_trash",
    params(
        ("entity_type" = EntityType, Path, description = "file or folder"),
        ("id" = i64, Path, description = "Entity ID"),
    ),
    responses(
        (status = 200, description = "Restored"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = crate::api::constants::OPENAPI_NOT_FOUND),
    ),
    security(("bearer" = [])),
)]
pub async fn restore(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<TrashItemPath>,
) -> Result<HttpResponse> {
    match path.entity_type {
        EntityType::File => trash_service::restore_file(&state, path.id, claims.user_id).await?,
        EntityType::Folder => {
            trash_service::restore_folder(&state, path.id, claims.user_id).await?
        }
    }
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    audit_service::log(
        &state,
        &ctx,
        match path.entity_type {
            EntityType::File => audit_service::AuditAction::FileRestore,
            EntityType::Folder => audit_service::AuditAction::FolderRestore,
        },
        audit_service::AuditEntityType::from_entity_type(path.entity_type),
        Some(path.id),
        None,
        None,
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/trash/{entity_type}/{id}",
    tag = "trash",
    operation_id = "purge_from_trash",
    params(
        ("entity_type" = EntityType, Path, description = "file or folder"),
        ("id" = i64, Path, description = "Entity ID"),
    ),
    responses(
        (status = 200, description = "Permanently deleted"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = crate::api::constants::OPENAPI_NOT_FOUND),
    ),
    security(("bearer" = [])),
)]
pub async fn purge_one(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<TrashItemPath>,
) -> Result<HttpResponse> {
    match path.entity_type {
        EntityType::File => trash_service::purge_file(&state, path.id, claims.user_id).await?,
        EntityType::Folder => trash_service::purge_folder(&state, path.id, claims.user_id).await?,
    }
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    audit_service::log(
        &state,
        &ctx,
        match path.entity_type {
            EntityType::File => audit_service::AuditAction::FilePurge,
            EntityType::Folder => audit_service::AuditAction::FolderPurge,
        },
        audit_service::AuditEntityType::from_entity_type(path.entity_type),
        Some(path.id),
        None,
        None,
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/trash",
    tag = "trash",
    operation_id = "purge_all_trash",
    responses(
        (status = 200, description = "Trash purge task created", body = inline(ApiResponse<task_service::TaskInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn purge_all(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
) -> Result<HttpResponse> {
    let scope = WorkspaceStorageScope::Personal {
        user_id: claims.user_id,
    };
    let task = task_service::create_trash_purge_all_task_in_scope(&state, scope).await?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    audit_service::log(
        &state,
        &ctx,
        audit_service::AuditAction::TrashPurgeAll,
        crate::services::audit_service::AuditEntityType::Trash,
        Some(task.id),
        Some(&task.display_name),
        None,
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(task)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/trash",
    tag = "teams",
    operation_id = "list_team_trash",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        TrashListQuery
    ),
    responses(
        (status = 200, description = "Team trash contents", body = inline(ApiResponse<trash_service::TrashContents>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_list_trash(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
    query: web::Query<TrashListQuery>,
) -> Result<HttpResponse> {
    let team_id = *path;
    let file_cursor = query.file_cursor().map(|(expires_at, id)| {
        trash_service::expires_cursor_to_deleted_cursor(&state, expires_at, id)
    });
    let contents = trash_service::list_team_trash(
        &state,
        team_id,
        claims.user_id,
        query.folder_limit(),
        query.folder_offset(),
        query.file_limit(),
        file_cursor,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(contents)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/trash/{entity_type}/{id}/restore",
    tag = "teams",
    operation_id = "restore_team_trash_item",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("entity_type" = EntityType, Path, description = "file or folder"),
        ("id" = i64, Path, description = "Entity ID"),
    ),
    responses(
        (status = 200, description = "Restored"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = crate::api::constants::OPENAPI_NOT_FOUND),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_restore(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, EntityType, i64)>,
) -> Result<HttpResponse> {
    let (team_id, entity_type, id) = path.into_inner();
    match entity_type {
        EntityType::File => {
            trash_service::restore_team_file(&state, team_id, id, claims.user_id).await?
        }
        EntityType::Folder => {
            trash_service::restore_team_folder(&state, team_id, id, claims.user_id).await?
        }
    }
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    audit_service::log(
        &state,
        &ctx,
        match entity_type {
            EntityType::File => audit_service::AuditAction::FileRestore,
            EntityType::Folder => audit_service::AuditAction::FolderRestore,
        },
        audit_service::AuditEntityType::from_entity_type(entity_type),
        Some(id),
        None,
        None,
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/teams/{team_id}/trash/{entity_type}/{id}",
    tag = "teams",
    operation_id = "purge_team_trash_item",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("entity_type" = EntityType, Path, description = "file or folder"),
        ("id" = i64, Path, description = "Entity ID"),
    ),
    responses(
        (status = 200, description = "Permanently deleted"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = crate::api::constants::OPENAPI_NOT_FOUND),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_purge_one(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, EntityType, i64)>,
) -> Result<HttpResponse> {
    let (team_id, entity_type, id) = path.into_inner();
    match entity_type {
        EntityType::File => {
            trash_service::purge_team_file(&state, team_id, id, claims.user_id).await?
        }
        EntityType::Folder => {
            trash_service::purge_team_folder(&state, team_id, id, claims.user_id).await?
        }
    }
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    audit_service::log(
        &state,
        &ctx,
        match entity_type {
            EntityType::File => audit_service::AuditAction::FilePurge,
            EntityType::Folder => audit_service::AuditAction::FolderPurge,
        },
        audit_service::AuditEntityType::from_entity_type(entity_type),
        Some(id),
        None,
        None,
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/teams/{team_id}/trash",
    tag = "teams",
    operation_id = "purge_all_team_trash",
    params(("team_id" = i64, Path, description = "Team ID")),
    responses(
        (status = 200, description = "Team trash purge task created", body = inline(ApiResponse<task_service::TaskInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_purge_all(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let team_id = *path;
    let scope = WorkspaceStorageScope::Team {
        team_id,
        actor_user_id: claims.user_id,
    };
    let task = task_service::create_trash_purge_all_task_in_scope(&state, scope).await?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    audit_service::log(
        &state,
        &ctx,
        audit_service::AuditAction::TrashPurgeAll,
        crate::services::audit_service::AuditEntityType::Trash,
        Some(task.id),
        Some(&task.display_name),
        None,
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(task)))
}
