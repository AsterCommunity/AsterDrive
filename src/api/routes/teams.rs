//! API 路由：`teams`。

pub use crate::api::dto::teams::*;
use crate::api::dto::validate_request;
use crate::api::middleware::auth::JwtAuth;
use crate::api::middleware::rate_limit;
use crate::api::pagination::LimitOffsetQuery;
use crate::api::response::ApiResponse;
use crate::api::routes::{batch, folders, search, shares, tasks, trash};
use crate::config::RateLimitConfig;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{audit_service, auth_service::Claims, team_service};
use crate::types::TeamMemberRole;
use actix_governor::Governor;
use actix_web::middleware::Condition;
use actix_web::{HttpRequest, HttpResponse, web};

pub fn routes(rl: &RateLimitConfig) -> impl actix_web::dev::HttpServiceFactory + use<> {
    let limiter = rate_limit::build_governor(&rl.api, &rl.trusted_proxies);

    web::scope("/teams")
        .wrap(JwtAuth)
        .wrap(Condition::new(rl.enabled, Governor::new(&limiter)))
        .route("", web::get().to(list_teams))
        .route("", web::post().to(create_team))
        .route("/{id}", web::get().to(get_team))
        .route("/{id}", web::patch().to(patch_team))
        .route("/{id}", web::delete().to(delete_team))
        .route("/{id}/restore", web::post().to(restore_team))
        .route("/{id}/audit-logs", web::get().to(list_audit_logs))
        .route("/{id}/members", web::get().to(list_members))
        .route("/{id}/members", web::post().to(add_member))
        .route(
            "/{id}/members/{member_user_id}",
            web::patch().to(patch_member),
        )
        .route(
            "/{id}/members/{member_user_id}",
            web::delete().to(delete_member),
        )
        .service(batch::team_routes())
        .service(search::team_routes())
        .service(shares::team_routes())
        .service(trash::team_routes())
        .service(tasks::team_routes())
        .service(folders::team_routes())
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/teams",
    tag = "teams",
    operation_id = "list_teams",
    params(ListTeamsQuery),
    responses(
        (status = 200, description = "Teams visible to the signed-in user", body = inline(ApiResponse<Vec<team_service::TeamInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn list_teams(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    query: web::Query<ListTeamsQuery>,
) -> Result<HttpResponse> {
    let teams =
        team_service::list_teams(&state, claims.user_id, query.archived.unwrap_or(false)).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(teams)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/teams",
    tag = "teams",
    operation_id = "create_team",
    request_body = CreateTeamReq,
    responses(
        (status = 201, description = "Team created", body = inline(ApiResponse<team_service::TeamInfo>)),
        (status = 400, description = "Validation error"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "System admin required"),
    ),
    security(("bearer" = [])),
)]
pub async fn create_team(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<CreateTeamReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let team = team_service::create_team_with_audit(
        &state,
        claims.user_id,
        team_service::CreateTeamInput {
            name: body.name.clone(),
            description: body.description.clone(),
        },
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(team)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/teams/{id}",
    tag = "teams",
    operation_id = "get_team",
    params(("id" = i64, Path, description = "Team ID")),
    responses(
        (status = 200, description = "Team details", body = inline(ApiResponse<team_service::TeamInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = crate::api::constants::OPENAPI_NOT_FOUND),
    ),
    security(("bearer" = [])),
)]
pub async fn get_team(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let team = team_service::get_team(&state, *path, claims.user_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(team)))
}

#[api_docs_macros::path(
    patch,
    path = "/api/v1/teams/{id}",
    tag = "teams",
    operation_id = "patch_team",
    params(("id" = i64, Path, description = "Team ID")),
    request_body = PatchTeamReq,
    responses(
        (status = 200, description = "Team updated", body = inline(ApiResponse<team_service::TeamInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = crate::api::constants::OPENAPI_NOT_FOUND),
    ),
    security(("bearer" = [])),
)]
pub async fn patch_team(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<PatchTeamReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let team = team_service::update_team_with_audit(
        &state,
        *path,
        claims.user_id,
        team_service::UpdateTeamInput {
            name: body.name.clone(),
            description: body.description.clone(),
        },
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(team)))
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/teams/{id}",
    tag = "teams",
    operation_id = "delete_team",
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
    team_service::archive_team_with_audit(&state, *path, claims.user_id, &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/teams/{id}/restore",
    tag = "teams",
    operation_id = "restore_team",
    params(("id" = i64, Path, description = "Team ID")),
    responses(
        (status = 200, description = "Team restored", body = inline(ApiResponse<team_service::TeamInfo>)),
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
    let team = team_service::restore_team_with_audit(&state, *path, claims.user_id, &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(team)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/teams/{id}/audit-logs",
    tag = "teams",
    operation_id = "list_team_audit_logs",
    params(
        ("id" = i64, Path, description = "Team ID"),
        LimitOffsetQuery,
        audit_service::AuditLogFilterQuery
    ),
    responses(
        (status = 200, description = "Team audit log entries", body = inline(ApiResponse<crate::api::pagination::OffsetPage<audit_service::TeamAuditEntryInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = crate::api::constants::OPENAPI_NOT_FOUND),
    ),
    security(("bearer" = [])),
)]
pub async fn list_audit_logs(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
    page: web::Query<LimitOffsetQuery>,
    query: web::Query<audit_service::AuditLogFilterQuery>,
) -> Result<HttpResponse> {
    let page = team_service::list_team_audit_entries(
        &state,
        *path,
        claims.user_id,
        audit_service::AuditLogFilters::from_query(&query),
        page.limit_or(20, 200),
        page.offset(),
    )
    .await?;

    Ok(HttpResponse::Ok().json(ApiResponse::ok(page)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/teams/{id}/members",
    tag = "teams",
    operation_id = "list_team_members",
    params(
        ("id" = i64, Path, description = "Team ID"),
        LimitOffsetQuery,
        ListTeamMembersQuery
    ),
    responses(
        (status = 200, description = "Team members", body = inline(ApiResponse<team_service::TeamMemberPage>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = crate::api::constants::OPENAPI_NOT_FOUND),
    ),
    security(("bearer" = [])),
)]
pub async fn list_members(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
    page: web::Query<LimitOffsetQuery>,
    query: web::Query<ListTeamMembersQuery>,
) -> Result<HttpResponse> {
    let members = team_service::list_members(
        &state,
        *path,
        claims.user_id,
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
    path = "/api/v1/teams/{id}/members",
    tag = "teams",
    operation_id = "add_team_member",
    params(("id" = i64, Path, description = "Team ID")),
    request_body = AddTeamMemberReq,
    responses(
        (status = 201, description = "Member added", body = inline(ApiResponse<team_service::TeamMemberInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = crate::api::constants::OPENAPI_NOT_FOUND),
    ),
    security(("bearer" = [])),
)]
pub async fn add_member(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<AddTeamMemberReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let member = team_service::add_member_with_audit(
        &state,
        *path,
        claims.user_id,
        team_service::AddTeamMemberInput {
            user_id: body.user_id,
            identifier: body.identifier.clone(),
            role: body.role.unwrap_or(TeamMemberRole::Member),
        },
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(member)))
}

#[api_docs_macros::path(
    patch,
    path = "/api/v1/teams/{id}/members/{member_user_id}",
    tag = "teams",
    operation_id = "patch_team_member",
    params(
        ("id" = i64, Path, description = "Team ID"),
        ("member_user_id" = i64, Path, description = "Member user ID")
    ),
    request_body = PatchTeamMemberReq,
    responses(
        (status = 200, description = "Member updated", body = inline(ApiResponse<team_service::TeamMemberInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = crate::api::constants::OPENAPI_NOT_FOUND),
    ),
    security(("bearer" = [])),
)]
pub async fn patch_member(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
    body: web::Json<PatchTeamMemberReq>,
) -> Result<HttpResponse> {
    let (team_id, member_user_id) = path.into_inner();
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let member = team_service::update_member_role_with_audit(
        &state,
        team_id,
        claims.user_id,
        member_user_id,
        body.role,
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(member)))
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/teams/{id}/members/{member_user_id}",
    tag = "teams",
    operation_id = "delete_team_member",
    params(
        ("id" = i64, Path, description = "Team ID"),
        ("member_user_id" = i64, Path, description = "Member user ID")
    ),
    responses(
        (status = 200, description = "Member removed"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = crate::api::constants::OPENAPI_NOT_FOUND),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_member(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
) -> Result<HttpResponse> {
    let (team_id, member_user_id) = path.into_inner();
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    team_service::remove_member_with_audit(&state, team_id, claims.user_id, member_user_id, &ctx)
        .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}
