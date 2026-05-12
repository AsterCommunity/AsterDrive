//! 管理员 API 路由：`teams`。

use crate::api::dto::admin::{AdminCreateTeamReq, AdminPatchTeamReq, AdminTeamListQuery};
use crate::api::dto::teams::{AddTeamMemberReq, ListTeamMembersQuery, PatchTeamMemberReq};
use crate::api::dto::validate_request;
use crate::api::pagination::LimitOffsetQuery;
#[cfg(all(debug_assertions, feature = "openapi"))]
use crate::api::pagination::OffsetPage;
use crate::api::response::ApiResponse;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{audit_service, auth_service::Claims, team_service};
use actix_web::{HttpRequest, HttpResponse, web};

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/teams",
    tag = "admin",
    operation_id = "admin_list_teams",
    params(LimitOffsetQuery, AdminTeamListQuery),
    responses(
        (status = 200, description = "List active teams", body = inline(ApiResponse<OffsetPage<crate::services::team_service::AdminTeamInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_teams(
    state: web::Data<PrimaryAppState>,
    page: web::Query<LimitOffsetQuery>,
    query: web::Query<AdminTeamListQuery>,
) -> Result<HttpResponse> {
    let teams = team_service::list_admin_teams(
        &state,
        page.limit_or(50, 100),
        page.offset(),
        query.keyword.as_deref(),
        query.archived.unwrap_or(false),
        query.sort_by(),
        query.sort_order(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(teams)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/teams",
    tag = "admin",
    operation_id = "admin_create_team",
    request_body = AdminCreateTeamReq,
    responses(
        (status = 201, description = "Team created", body = inline(ApiResponse<crate::services::team_service::AdminTeamInfo>)),
        (status = 400, description = "Validation error"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn create_team(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<AdminCreateTeamReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let team = team_service::create_admin_team_with_audit(
        &state,
        claims.user_id,
        team_service::AdminCreateTeamInput {
            name: body.name.clone(),
            description: body.description.clone(),
            admin_user_id: body.admin_user_id,
            admin_identifier: body.admin_identifier.clone(),
            policy_group_id: body.policy_group_id,
        },
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(team)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/teams/{id}",
    tag = "admin",
    operation_id = "admin_get_team",
    params(("id" = i64, Path, description = "Team ID")),
    responses(
        (status = 200, description = "Team details", body = inline(ApiResponse<crate::services::team_service::AdminTeamInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = crate::api::constants::OPENAPI_NOT_FOUND),
    ),
    security(("bearer" = [])),
)]
pub async fn get_team(
    state: web::Data<PrimaryAppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let team = team_service::get_admin_team(&state, *path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(team)))
}

#[api_docs_macros::path(
    patch,
    path = "/api/v1/admin/teams/{id}",
    tag = "admin",
    operation_id = "admin_update_team",
    params(("id" = i64, Path, description = "Team ID")),
    request_body = AdminPatchTeamReq,
    responses(
        (status = 200, description = "Team updated", body = inline(ApiResponse<crate::services::team_service::AdminTeamInfo>)),
        (status = 400, description = "Validation error"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = crate::api::constants::OPENAPI_NOT_FOUND),
    ),
    security(("bearer" = [])),
)]
pub async fn update_team(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<AdminPatchTeamReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let team = team_service::update_admin_team_with_audit(
        &state,
        *path,
        team_service::AdminUpdateTeamInput {
            name: body.name.clone(),
            description: body.description.clone(),
            policy_group_id: body.policy_group_id,
        },
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(team)))
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/admin/teams/{id}",
    tag = "admin",
    operation_id = "admin_delete_team",
    params(("id" = i64, Path, description = "Team ID")),
    responses(
        (status = 200, description = "Team archived"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = crate::api::constants::OPENAPI_NOT_FOUND),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_team(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    team_service::archive_admin_team_with_audit(&state, *path, &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/teams/{id}/restore",
    tag = "admin",
    operation_id = "admin_restore_team",
    params(("id" = i64, Path, description = "Team ID")),
    responses(
        (status = 200, description = "Team restored", body = inline(ApiResponse<crate::services::team_service::AdminTeamInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = crate::api::constants::OPENAPI_NOT_FOUND),
    ),
    security(("bearer" = [])),
)]
pub async fn restore_team(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let team = team_service::restore_admin_team_with_audit(&state, *path, &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(team)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/teams/{id}/audit-logs",
    tag = "admin",
    operation_id = "admin_list_team_audit_logs",
    params(
        ("id" = i64, Path, description = "Team ID"),
        LimitOffsetQuery,
        audit_service::AuditLogFilterQuery
    ),
    responses(
        (status = 200, description = "Team audit log entries", body = inline(ApiResponse<OffsetPage<audit_service::TeamAuditEntryInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = crate::api::constants::OPENAPI_NOT_FOUND),
    ),
    security(("bearer" = [])),
)]
pub async fn list_team_audit_logs(
    state: web::Data<PrimaryAppState>,
    path: web::Path<i64>,
    page: web::Query<LimitOffsetQuery>,
    query: web::Query<audit_service::AuditLogFilterQuery>,
) -> Result<HttpResponse> {
    let page = team_service::list_admin_team_audit_entries(
        &state,
        *path,
        audit_service::AuditLogFilters::from_query(&query),
        page.limit_or(20, 200),
        page.offset(),
    )
    .await?;

    Ok(HttpResponse::Ok().json(ApiResponse::ok(page)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/teams/{id}/members",
    tag = "admin",
    operation_id = "admin_list_team_members",
    params(
        ("id" = i64, Path, description = "Team ID"),
        LimitOffsetQuery,
        ListTeamMembersQuery
    ),
    responses(
        (status = 200, description = "Team members", body = inline(ApiResponse<crate::services::team_service::TeamMemberPage>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Team not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_team_members(
    state: web::Data<PrimaryAppState>,
    path: web::Path<i64>,
    page: web::Query<LimitOffsetQuery>,
    query: web::Query<ListTeamMembersQuery>,
) -> Result<HttpResponse> {
    let members = team_service::list_admin_members(
        &state,
        *path,
        {
            let mut filters = team_service::TeamMemberListFilters::from_inputs(
                query.keyword.as_deref(),
                query.role,
                query.status,
            );
            filters.sort_by = query.sort_by();
            filters.sort_order = query.sort_order();
            filters
        },
        page.limit_or(20, 100),
        page.offset(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(members)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/teams/{id}/members",
    tag = "admin",
    operation_id = "admin_add_team_member",
    params(("id" = i64, Path, description = "Team ID")),
    request_body = AddTeamMemberReq,
    responses(
        (status = 201, description = "Member added", body = inline(ApiResponse<crate::services::team_service::TeamMemberInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Team not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn add_team_member(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<AddTeamMemberReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let member = team_service::add_admin_member_with_audit(
        &state,
        *path,
        team_service::AddTeamMemberInput {
            user_id: body.user_id,
            identifier: body.identifier.clone(),
            role: body.role.unwrap_or(crate::types::TeamMemberRole::Member),
        },
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(member)))
}

#[api_docs_macros::path(
    patch,
    path = "/api/v1/admin/teams/{id}/members/{member_user_id}",
    tag = "admin",
    operation_id = "admin_patch_team_member",
    params(
        ("id" = i64, Path, description = "Team ID"),
        ("member_user_id" = i64, Path, description = "Member user ID")
    ),
    request_body = PatchTeamMemberReq,
    responses(
        (status = 200, description = "Member updated", body = inline(ApiResponse<crate::services::team_service::TeamMemberInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Team or member not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn patch_team_member(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
    body: web::Json<PatchTeamMemberReq>,
) -> Result<HttpResponse> {
    let (team_id, member_user_id) = path.into_inner();
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let member = team_service::update_admin_member_role_with_audit(
        &state,
        team_id,
        member_user_id,
        body.role,
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(member)))
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/admin/teams/{id}/members/{member_user_id}",
    tag = "admin",
    operation_id = "admin_delete_team_member",
    params(
        ("id" = i64, Path, description = "Team ID"),
        ("member_user_id" = i64, Path, description = "Member user ID")
    ),
    responses(
        (status = 200, description = "Member removed"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Team or member not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_team_member(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
) -> Result<HttpResponse> {
    let (team_id, member_user_id) = path.into_inner();
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    team_service::remove_admin_member_with_audit(&state, team_id, member_user_id, &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}
