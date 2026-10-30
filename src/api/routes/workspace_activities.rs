//! Authorized resource activity adapters; public shares deliberately have no activity endpoint.
#[cfg(all(debug_assertions, feature = "openapi"))]
use crate::{api::response::ApiResponse, services::workspace::activity::WorkspaceActivityPage};
use crate::{
    errors::Result,
    runtime::PrimaryAppState,
    services::{
        auth::local::Claims,
        workspace::{
            activity::{self, ActivityListQuery},
            scope::WorkspaceStorageScope,
        },
    },
};
use actix_web::{HttpResponse, web};

async fn response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    entity: Option<(&str, i64)>,
    query: &ActivityListQuery,
) -> Result<HttpResponse> {
    let page = activity::list_activities(state, scope, entity, query).await?;
    Ok(HttpResponse::Ok().json(crate::api::response::success(page)))
}

#[aster_forge_api_docs_macros::path(get,path="/api/v1/files/{id}/activities",tag="files",operation_id="list_file_activities",params(("id"=i64,Path),ActivityListQuery),responses((status=200,body=inline(ApiResponse<WorkspaceActivityPage>))),security(("bearer"=[])))]
pub async fn file(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    id: web::Path<i64>,
    query: web::Query<ActivityListQuery>,
) -> Result<HttpResponse> {
    response(
        &state,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        Some(("file", *id)),
        &query,
    )
    .await
}

#[aster_forge_api_docs_macros::path(get,path="/api/v1/folders/{id}/activities",tag="folders",operation_id="list_folder_activities",params(("id"=i64,Path),ActivityListQuery),responses((status=200,body=inline(ApiResponse<WorkspaceActivityPage>))),security(("bearer"=[])))]
pub async fn folder(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    id: web::Path<i64>,
    query: web::Query<ActivityListQuery>,
) -> Result<HttpResponse> {
    response(
        &state,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        Some(("folder", *id)),
        &query,
    )
    .await
}

#[aster_forge_api_docs_macros::path(get,path="/api/v1/teams/{team_id}/files/{id}/activities",tag="teams",operation_id="list_team_file_activities",params(("team_id"=i64,Path),("id"=i64,Path),ActivityListQuery),responses((status=200,body=inline(ApiResponse<WorkspaceActivityPage>))),security(("bearer"=[])))]
pub async fn team_file(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<(i64, i64)>,
    query: web::Query<ActivityListQuery>,
) -> Result<HttpResponse> {
    let (team_id, id) = *path;
    response(
        &state,
        super::team_scope(team_id, claims.user_id),
        Some(("file", id)),
        &query,
    )
    .await
}

#[aster_forge_api_docs_macros::path(get,path="/api/v1/teams/{team_id}/folders/{id}/activities",tag="teams",operation_id="list_team_folder_activities",params(("team_id"=i64,Path),("id"=i64,Path),ActivityListQuery),responses((status=200,body=inline(ApiResponse<WorkspaceActivityPage>))),security(("bearer"=[])))]
pub async fn team_folder(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<(i64, i64)>,
    query: web::Query<ActivityListQuery>,
) -> Result<HttpResponse> {
    let (team_id, id) = *path;
    response(
        &state,
        super::team_scope(team_id, claims.user_id),
        Some(("folder", id)),
        &query,
    )
    .await
}

#[aster_forge_api_docs_macros::path(get,path="/api/v1/teams/{id}/activities",tag="teams",operation_id="list_team_activities",params(("id"=i64,Path),ActivityListQuery),responses((status=200,body=inline(ApiResponse<WorkspaceActivityPage>))),security(("bearer"=[])))]
pub async fn team(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    id: web::Path<i64>,
    query: web::Query<ActivityListQuery>,
) -> Result<HttpResponse> {
    response(&state, super::team_scope(*id, claims.user_id), None, &query).await
}

#[aster_forge_api_docs_macros::path(get,path="/api/v1/teams/{id}/activities/export",tag="teams",operation_id="export_team_activities",params(("id"=i64,Path)),responses((status=200,description="Workspace activity CSV")),security(("bearer"=[])))]
pub async fn export_team(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    id: web::Path<i64>,
) -> Result<HttpResponse> {
    let export = crate::services::workspace::activity_export::prepare(
        &state,
        super::team_scope(*id, claims.user_id),
    )
    .await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Content-Type", "text/csv; charset=utf-8"))
        .insert_header((
            "Content-Disposition",
            format!(
                "attachment; filename=\"asterdrive-team-{}-activities.csv\"",
                *id
            ),
        ))
        .insert_header(("X-Activity-Export-Rows", export.total.to_string()))
        .streaming(export.stream))
}

#[aster_forge_api_docs_macros::path(get,path="/api/v1/admin/teams/{id}/activities",tag="admin",operation_id="admin_list_team_activities",params(("id"=i64,Path),ActivityListQuery),responses((status=200,body=inline(ApiResponse<WorkspaceActivityPage>))),security(("bearer"=[])))]
pub async fn admin_team(
    state: web::Data<PrimaryAppState>,
    id: web::Path<i64>,
    query: web::Query<ActivityListQuery>,
) -> Result<HttpResponse> {
    let page = activity::list_admin_team(state.get_ref(), *id, &query).await?;
    Ok(HttpResponse::Ok().json(crate::api::response::success(page)))
}
#[aster_forge_api_docs_macros::path(get,path="/api/v1/admin/teams/{id}/activities/export",tag="admin",operation_id="admin_export_team_activities",params(("id"=i64,Path)),responses((status=200,description="Workspace activity CSV")),security(("bearer"=[])))]
pub async fn admin_export_team(
    state: web::Data<PrimaryAppState>,
    id: web::Path<i64>,
) -> Result<HttpResponse> {
    let export = crate::services::workspace::activity_export::prepare_admin(&state, *id).await?;
    Ok(HttpResponse::Ok()
        .insert_header(("Content-Type", "text/csv; charset=utf-8"))
        .insert_header((
            "Content-Disposition",
            format!(
                "attachment; filename=\"asterdrive-team-{}-activities.csv\"",
                *id
            ),
        ))
        .insert_header(("X-Activity-Export-Rows", export.total.to_string()))
        .streaming(export.stream))
}
