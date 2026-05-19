//! API 路由：`search`。

use crate::api::middleware::auth::JwtAuth;
use crate::api::middleware::rate_limit;
use crate::api::response::ApiResponse;
use crate::api::routes::team_scope;
use crate::config::{NetworkTrustConfig, RateLimitConfig};
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
#[cfg(all(debug_assertions, feature = "openapi"))]
use crate::services::search_service::SearchResults;
use crate::services::{
    auth_service::Claims,
    search_service::{self, SearchParams},
    workspace_storage_service::WorkspaceStorageScope,
};
use actix_governor::Governor;
use actix_web::middleware::Condition;
use actix_web::{HttpResponse, web};

pub fn routes(
    rl: &RateLimitConfig,
    network_trust: &NetworkTrustConfig,
) -> impl actix_web::dev::HttpServiceFactory + use<> {
    let limiter = rate_limit::build_governor(&rl.api, &network_trust.trusted_proxies);

    web::scope("/search")
        .wrap(JwtAuth)
        .wrap(Condition::new(rl.enabled, Governor::new(&limiter)))
        .route("", web::get().to(search))
}

pub fn team_routes() -> actix_web::Scope {
    web::scope("/{team_id}/search").route("", web::get().to(team_search))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/search",
    tag = "search",
    operation_id = "search",
    params(SearchParams),
    responses(
        (status = 200, description = "Search results", body = inline(ApiResponse<SearchResults>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn search(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    query: web::Query<SearchParams>,
) -> Result<HttpResponse> {
    let query = query.into_inner();
    search_response(
        &state,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        &query,
    )
    .await
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/search",
    tag = "teams",
    operation_id = "search_team_workspace",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        SearchParams
    ),
    responses(
        (status = 200, description = "Team workspace search results", body = inline(ApiResponse<SearchResults>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_search(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
    query: web::Query<SearchParams>,
) -> Result<HttpResponse> {
    let query = query.into_inner();
    search_response(&state, team_scope(*path, claims.user_id), &query).await
}

pub(crate) async fn search_response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    query: &SearchParams,
) -> Result<HttpResponse> {
    let results = search_service::search_in_scope(state, scope, query).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(results)))
}
