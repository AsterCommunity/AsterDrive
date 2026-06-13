//! API 路由：`tags`。

use crate::api::dto::{
    BatchTagBindingReq, CreateTagReq, DEFAULT_TAG_LIMIT, EntityTagsPath, PatchTagReq,
    ReplaceEntityTagsReq, TagEntityPath, TagListQuery, TagPath, validate_request,
};
use crate::api::middleware::auth::JwtAuth;
use crate::api::middleware::rate_limit;
#[cfg(all(debug_assertions, feature = "openapi"))]
use crate::api::pagination::OffsetPage;
use crate::api::pagination::{LimitOffsetQuery, MAX_PAGE_SIZE};
use crate::api::response::ApiResponse;
use crate::api::routes::team_scope;
use crate::config::{NetworkTrustConfig, RateLimitConfig};
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
#[cfg(all(debug_assertions, feature = "openapi"))]
use crate::services::tag_service::TagInfo;
use crate::services::{
    audit_service,
    auth_service::Claims,
    tag_service::{self, EntityTags, MinimalTagInfo},
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

    web::scope("/tags")
        .wrap(JwtAuth)
        .wrap(Condition::new(rl.enabled, Governor::new(&limiter)))
        .route("", web::get().to(list_tags))
        .route("", web::post().to(create_tag))
        .route("/{tag_id}", web::patch().to(patch_tag))
        .route("/{tag_id}", web::delete().to(delete_tag))
        .route("/{tag_id}/batch", web::put().to(batch_attach_tag))
        .route("/{tag_id}/batch", web::delete().to(batch_detach_tag))
        .route(
            "/{tag_id}/{entity_type}/{entity_id}",
            web::put().to(attach_tag),
        )
        .route(
            "/{tag_id}/{entity_type}/{entity_id}",
            web::delete().to(detach_tag),
        )
        .route(
            "/{entity_type}/{entity_id}",
            web::get().to(list_entity_tags),
        )
        .route(
            "/{entity_type}/{entity_id}",
            web::put().to(replace_entity_tags),
        )
}

pub fn team_routes() -> actix_web::Scope {
    web::scope("/{team_id}/tags")
        .route("", web::get().to(team_list_tags))
        .route("", web::post().to(team_create_tag))
        .route("/{tag_id}", web::patch().to(team_patch_tag))
        .route("/{tag_id}", web::delete().to(team_delete_tag))
        .route("/{tag_id}/batch", web::put().to(team_batch_attach_tag))
        .route("/{tag_id}/batch", web::delete().to(team_batch_detach_tag))
        .route(
            "/{tag_id}/{entity_type}/{entity_id}",
            web::put().to(team_attach_tag),
        )
        .route(
            "/{tag_id}/{entity_type}/{entity_id}",
            web::delete().to(team_detach_tag),
        )
        .route(
            "/{entity_type}/{entity_id}",
            web::get().to(team_list_entity_tags),
        )
        .route(
            "/{entity_type}/{entity_id}",
            web::put().to(team_replace_entity_tags),
        )
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/tags",
    tag = "tags",
    operation_id = "list_tags",
    params(LimitOffsetQuery, TagListQuery),
    responses(
        (status = 200, description = "Tags visible in the personal workspace", body = inline(ApiResponse<OffsetPage<TagInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn list_tags(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    page: web::Query<LimitOffsetQuery>,
    query: web::Query<TagListQuery>,
) -> Result<HttpResponse> {
    validate_request(&*query)?;
    list_tags_response(
        state.get_ref(),
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        &page,
        &query,
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/tags",
    tag = "tags",
    operation_id = "create_tag",
    request_body = CreateTagReq,
    responses(
        (status = 201, description = "Tag created", body = inline(ApiResponse<TagInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn create_tag(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<CreateTagReq>,
) -> Result<HttpResponse> {
    let body = body.into_inner();
    validate_request(&body)?;
    create_tag_response(
        state.get_ref(),
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        &body,
        Some(&audit_service::AuditContext::from_request(&req, &claims)),
    )
    .await
}

#[api_docs_macros::path(
    patch,
    path = "/api/v1/tags/{tag_id}",
    tag = "tags",
    operation_id = "patch_tag",
    params(TagPath),
    request_body = PatchTagReq,
    responses(
        (status = 200, description = "Tag updated", body = inline(ApiResponse<TagInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "Tag not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn patch_tag(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<TagPath>,
    body: web::Json<PatchTagReq>,
) -> Result<HttpResponse> {
    let path = path.into_inner();
    let body = body.into_inner();
    validate_request(&path)?;
    validate_request(&body)?;
    patch_tag_response(
        state.get_ref(),
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        path.tag_id,
        &body,
        Some(&audit_service::AuditContext::from_request(&req, &claims)),
    )
    .await
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/tags/{tag_id}",
    tag = "tags",
    operation_id = "delete_tag",
    params(TagPath),
    responses(
        (status = 200, description = "Tag deleted"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "Tag not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_tag(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<TagPath>,
) -> Result<HttpResponse> {
    let path = path.into_inner();
    validate_request(&path)?;
    delete_tag_response(
        state.get_ref(),
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        path.tag_id,
        Some(&audit_service::AuditContext::from_request(&req, &claims)),
    )
    .await
}

pub async fn attach_tag(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<TagEntityPath>,
) -> Result<HttpResponse> {
    let path = path.into_inner();
    validate_request(&path)?;
    attach_tag_response(
        state.get_ref(),
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        path,
        Some(&audit_service::AuditContext::from_request(&req, &claims)),
    )
    .await
}

pub async fn detach_tag(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<TagEntityPath>,
) -> Result<HttpResponse> {
    let path = path.into_inner();
    validate_request(&path)?;
    detach_tag_response(
        state.get_ref(),
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        path,
        Some(&audit_service::AuditContext::from_request(&req, &claims)),
    )
    .await
}

pub async fn replace_entity_tags(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<EntityTagsPath>,
    body: web::Json<ReplaceEntityTagsReq>,
) -> Result<HttpResponse> {
    let path = path.into_inner();
    let body = body.into_inner();
    validate_request(&path)?;
    validate_request(&body)?;
    replace_entity_tags_response(
        state.get_ref(),
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        path,
        &body,
        Some(&audit_service::AuditContext::from_request(&req, &claims)),
    )
    .await
}

pub async fn list_entity_tags(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<EntityTagsPath>,
) -> Result<HttpResponse> {
    let path = path.into_inner();
    validate_request(&path)?;
    list_entity_tags_response(
        state.get_ref(),
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        path,
    )
    .await
}

pub async fn batch_attach_tag(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<TagPath>,
    body: web::Json<BatchTagBindingReq>,
) -> Result<HttpResponse> {
    let path = path.into_inner();
    let body = body.into_inner();
    validate_request(&path)?;
    validate_request(&body)?;
    batch_attach_tag_response(
        state.get_ref(),
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        path.tag_id,
        &body,
        Some(&audit_service::AuditContext::from_request(&req, &claims)),
    )
    .await
}

pub async fn batch_detach_tag(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<TagPath>,
    body: web::Json<BatchTagBindingReq>,
) -> Result<HttpResponse> {
    let path = path.into_inner();
    let body = body.into_inner();
    validate_request(&path)?;
    validate_request(&body)?;
    batch_detach_tag_response(
        state.get_ref(),
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        path.tag_id,
        &body,
        Some(&audit_service::AuditContext::from_request(&req, &claims)),
    )
    .await
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/tags",
    tag = "teams",
    operation_id = "list_team_tags",
    params(("team_id" = i64, Path, description = "Team ID"), LimitOffsetQuery, TagListQuery),
    responses(
        (status = 200, description = "Team workspace tags", body = inline(ApiResponse<OffsetPage<TagInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn team_list_tags(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
    page: web::Query<LimitOffsetQuery>,
    query: web::Query<TagListQuery>,
) -> Result<HttpResponse> {
    validate_request(&*query)?;
    list_tags_response(
        state.get_ref(),
        team_scope(*path, claims.user_id),
        &page,
        &query,
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/tags",
    tag = "teams",
    operation_id = "create_team_tag",
    params(("team_id" = i64, Path, description = "Team ID")),
    request_body = CreateTagReq,
    responses(
        (status = 201, description = "Team tag created", body = inline(ApiResponse<TagInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn team_create_tag(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<CreateTagReq>,
) -> Result<HttpResponse> {
    let body = body.into_inner();
    validate_request(&body)?;
    create_tag_response(
        state.get_ref(),
        team_scope(*path, claims.user_id),
        &body,
        Some(&audit_service::AuditContext::from_request(&req, &claims)),
    )
    .await
}

#[api_docs_macros::path(
    patch,
    path = "/api/v1/teams/{team_id}/tags/{tag_id}",
    tag = "teams",
    operation_id = "patch_team_tag",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("tag_id" = i64, Path, description = "Tag ID"),
    ),
    request_body = PatchTagReq,
    responses(
        (status = 200, description = "Team tag updated", body = inline(ApiResponse<TagInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Tag not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn team_patch_tag(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
    body: web::Json<PatchTagReq>,
) -> Result<HttpResponse> {
    let (team_id, tag_id) = path.into_inner();
    let body = body.into_inner();
    validate_request(&body)?;
    patch_tag_response(
        state.get_ref(),
        team_scope(team_id, claims.user_id),
        tag_id,
        &body,
        Some(&audit_service::AuditContext::from_request(&req, &claims)),
    )
    .await
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/teams/{team_id}/tags/{tag_id}",
    tag = "teams",
    operation_id = "delete_team_tag",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("tag_id" = i64, Path, description = "Tag ID"),
    ),
    responses(
        (status = 200, description = "Team tag deleted"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Tag not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn team_delete_tag(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
) -> Result<HttpResponse> {
    let (team_id, tag_id) = path.into_inner();
    delete_tag_response(
        state.get_ref(),
        team_scope(team_id, claims.user_id),
        tag_id,
        Some(&audit_service::AuditContext::from_request(&req, &claims)),
    )
    .await
}

pub async fn team_attach_tag(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64, crate::types::EntityType, i64)>,
) -> Result<HttpResponse> {
    let (team_id, tag_id, entity_type, entity_id) = path.into_inner();
    attach_tag_response(
        state.get_ref(),
        team_scope(team_id, claims.user_id),
        TagEntityPath {
            tag_id,
            entity_type,
            entity_id,
        },
        Some(&audit_service::AuditContext::from_request(&req, &claims)),
    )
    .await
}

pub async fn team_detach_tag(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64, crate::types::EntityType, i64)>,
) -> Result<HttpResponse> {
    let (team_id, tag_id, entity_type, entity_id) = path.into_inner();
    detach_tag_response(
        state.get_ref(),
        team_scope(team_id, claims.user_id),
        TagEntityPath {
            tag_id,
            entity_type,
            entity_id,
        },
        Some(&audit_service::AuditContext::from_request(&req, &claims)),
    )
    .await
}

pub async fn team_replace_entity_tags(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, crate::types::EntityType, i64)>,
    body: web::Json<ReplaceEntityTagsReq>,
) -> Result<HttpResponse> {
    let (team_id, entity_type, entity_id) = path.into_inner();
    let body = body.into_inner();
    validate_request(&body)?;
    replace_entity_tags_response(
        state.get_ref(),
        team_scope(team_id, claims.user_id),
        EntityTagsPath {
            entity_type,
            entity_id,
        },
        &body,
        Some(&audit_service::AuditContext::from_request(&req, &claims)),
    )
    .await
}

pub async fn team_list_entity_tags(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<(i64, crate::types::EntityType, i64)>,
) -> Result<HttpResponse> {
    let (team_id, entity_type, entity_id) = path.into_inner();
    list_entity_tags_response(
        state.get_ref(),
        team_scope(team_id, claims.user_id),
        EntityTagsPath {
            entity_type,
            entity_id,
        },
    )
    .await
}

pub async fn team_batch_attach_tag(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
    body: web::Json<BatchTagBindingReq>,
) -> Result<HttpResponse> {
    let (team_id, tag_id) = path.into_inner();
    let body = body.into_inner();
    validate_request(&body)?;
    batch_attach_tag_response(
        state.get_ref(),
        team_scope(team_id, claims.user_id),
        tag_id,
        &body,
        Some(&audit_service::AuditContext::from_request(&req, &claims)),
    )
    .await
}

pub async fn team_batch_detach_tag(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
    body: web::Json<BatchTagBindingReq>,
) -> Result<HttpResponse> {
    let (team_id, tag_id) = path.into_inner();
    let body = body.into_inner();
    validate_request(&body)?;
    batch_detach_tag_response(
        state.get_ref(),
        team_scope(team_id, claims.user_id),
        tag_id,
        &body,
        Some(&audit_service::AuditContext::from_request(&req, &claims)),
    )
    .await
}

async fn list_tags_response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    page: &LimitOffsetQuery,
    query: &TagListQuery,
) -> Result<HttpResponse> {
    let page = tag_service::list_page_in_scope(
        state,
        scope,
        page.limit_or(DEFAULT_TAG_LIMIT, MAX_PAGE_SIZE),
        page.offset(),
        query.q.as_deref(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(page)))
}

async fn create_tag_response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    body: &CreateTagReq,
    audit_ctx: Option<&audit_service::AuditContext>,
) -> Result<HttpResponse> {
    let tag = tag_service::create_in_scope(state, scope, &body.name, &body.color).await?;
    if let Some(audit_ctx) = audit_ctx {
        let details = audit_service::details(audit_service::TagAuditDetails {
            name: &tag.name,
            color: &tag.color,
            previous_name: None,
            next_name: None,
            previous_color: None,
            next_color: None,
            team_id: scope.team_id(),
        });
        audit_service::log_with_details(
            state,
            audit_ctx,
            audit_service::AuditAction::TagCreate,
            audit_service::AuditEntityType::Tag,
            Some(tag.id),
            Some(&tag.name),
            || details.clone(),
        )
        .await;
    }
    Ok(HttpResponse::Created().json(ApiResponse::ok(tag)))
}

async fn patch_tag_response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    tag_id: i64,
    body: &PatchTagReq,
    audit_ctx: Option<&audit_service::AuditContext>,
) -> Result<HttpResponse> {
    let previous = if audit_ctx.is_some() {
        Some(tag_service::get_basic_in_scope(state, scope, tag_id).await?)
    } else {
        None
    };
    let tag = tag_service::update_in_scope(
        state,
        scope,
        tag_id,
        body.name.as_deref(),
        body.color.as_deref(),
    )
    .await?;
    if let Some(audit_ctx) = audit_ctx {
        let details = audit_service::details(audit_service::TagAuditDetails {
            name: &tag.name,
            color: &tag.color,
            previous_name: previous.as_ref().map(|tag| tag.name.as_str()),
            next_name: Some(&tag.name),
            previous_color: previous.as_ref().map(|tag| tag.color.as_str()),
            next_color: Some(&tag.color),
            team_id: scope.team_id(),
        });
        audit_service::log_with_details(
            state,
            audit_ctx,
            audit_service::AuditAction::TagUpdate,
            audit_service::AuditEntityType::Tag,
            Some(tag.id),
            Some(&tag.name),
            || details.clone(),
        )
        .await;
    }
    Ok(HttpResponse::Ok().json(ApiResponse::ok(tag)))
}

async fn delete_tag_response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    tag_id: i64,
    audit_ctx: Option<&audit_service::AuditContext>,
) -> Result<HttpResponse> {
    let tag = if audit_ctx.is_some() {
        Some(tag_service::get_basic_in_scope(state, scope, tag_id).await?)
    } else {
        None
    };
    tag_service::delete_in_scope(state, scope, tag_id).await?;
    if let Some(audit_ctx) = audit_ctx {
        let entity_name = tag.as_ref().map(|tag| tag.name.as_str());
        let details = tag.as_ref().and_then(|tag| {
            audit_service::details(audit_service::TagAuditDetails {
                name: &tag.name,
                color: &tag.color,
                previous_name: None,
                next_name: None,
                previous_color: None,
                next_color: None,
                team_id: scope.team_id(),
            })
        });
        audit_service::log_with_details(
            state,
            audit_ctx,
            audit_service::AuditAction::TagDelete,
            audit_service::AuditEntityType::Tag,
            Some(tag_id),
            entity_name,
            || details.clone(),
        )
        .await;
    }
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

async fn attach_tag_response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    path: TagEntityPath,
    audit_ctx: Option<&audit_service::AuditContext>,
) -> Result<HttpResponse> {
    let entity_type = path.entity_type;
    let entity_id = path.entity_id;
    let tag_id = path.tag_id;
    let tag = if audit_ctx.is_some() {
        Some(tag_service::get_basic_in_scope(state, scope, tag_id).await?)
    } else {
        None
    };
    let tags = tag_service::attach_to_entity_in_scope(state, scope, tag_id, entity_type, entity_id)
        .await?;
    if let Some(audit_ctx) = audit_ctx {
        let details = tag_assignment_audit_details(TagAssignmentAuditInput {
            operation: "attach",
            scope,
            tag: tag.as_ref(),
            entity_type: Some(entity_type),
            entity_id: Some(entity_id),
            file_count: None,
            folder_count: None,
            tag_count: None,
        });
        audit_service::log_with_details(
            state,
            audit_ctx,
            audit_service::AuditAction::TagAttach,
            audit_service::AuditEntityType::from_entity_type(entity_type),
            Some(entity_id),
            tag.as_ref().map(|tag| tag.name.as_str()),
            || details.clone(),
        )
        .await;
    }
    Ok(entity_tags_response(tags))
}

async fn detach_tag_response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    path: TagEntityPath,
    audit_ctx: Option<&audit_service::AuditContext>,
) -> Result<HttpResponse> {
    let entity_type = path.entity_type;
    let entity_id = path.entity_id;
    let tag_id = path.tag_id;
    let tag = if audit_ctx.is_some() {
        Some(tag_service::get_basic_in_scope(state, scope, tag_id).await?)
    } else {
        None
    };
    let tags =
        tag_service::detach_from_entity_in_scope(state, scope, tag_id, entity_type, entity_id)
            .await?;
    if let Some(audit_ctx) = audit_ctx {
        let details = tag_assignment_audit_details(TagAssignmentAuditInput {
            operation: "detach",
            scope,
            tag: tag.as_ref(),
            entity_type: Some(entity_type),
            entity_id: Some(entity_id),
            file_count: None,
            folder_count: None,
            tag_count: None,
        });
        audit_service::log_with_details(
            state,
            audit_ctx,
            audit_service::AuditAction::TagDetach,
            audit_service::AuditEntityType::from_entity_type(entity_type),
            Some(entity_id),
            tag.as_ref().map(|tag| tag.name.as_str()),
            || details.clone(),
        )
        .await;
    }
    Ok(entity_tags_response(tags))
}

async fn replace_entity_tags_response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    path: EntityTagsPath,
    body: &ReplaceEntityTagsReq,
    audit_ctx: Option<&audit_service::AuditContext>,
) -> Result<HttpResponse> {
    let entity_type = path.entity_type;
    let entity_id = path.entity_id;
    let tags = tag_service::replace_entity_tags_in_scope(
        state,
        scope,
        entity_type,
        entity_id,
        &body.tag_ids,
    )
    .await?;
    if let Some(audit_ctx) = audit_ctx {
        let details = tag_assignment_audit_details(TagAssignmentAuditInput {
            operation: "replace",
            scope,
            tag: None,
            entity_type: Some(entity_type),
            entity_id: Some(entity_id),
            file_count: None,
            folder_count: None,
            tag_count: Some(tags.tags.len()),
        });
        audit_service::log_with_details(
            state,
            audit_ctx,
            audit_service::AuditAction::TagAttach,
            audit_service::AuditEntityType::from_entity_type(entity_type),
            Some(entity_id),
            Some("replace"),
            || details.clone(),
        )
        .await;
    }
    Ok(entity_tags_response(tags))
}

async fn list_entity_tags_response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    path: EntityTagsPath,
) -> Result<HttpResponse> {
    let tags =
        tag_service::list_entity_tags_in_scope(state, scope, path.entity_type, path.entity_id)
            .await?;
    Ok(entity_tags_response(tags))
}

async fn batch_attach_tag_response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    tag_id: i64,
    body: &BatchTagBindingReq,
    audit_ctx: Option<&audit_service::AuditContext>,
) -> Result<HttpResponse> {
    let tag = if audit_ctx.is_some() {
        Some(tag_service::get_basic_in_scope(state, scope, tag_id).await?)
    } else {
        None
    };
    tag_service::batch_attach_in_scope(state, scope, tag_id, &body.file_ids, &body.folder_ids)
        .await?;
    if let Some(audit_ctx) = audit_ctx {
        let details = tag_assignment_audit_details(TagAssignmentAuditInput {
            operation: "batch_attach",
            scope,
            tag: tag.as_ref(),
            entity_type: None,
            entity_id: None,
            file_count: Some(body.file_ids.len()),
            folder_count: Some(body.folder_ids.len()),
            tag_count: None,
        });
        audit_service::log_with_details(
            state,
            audit_ctx,
            audit_service::AuditAction::TagAttach,
            audit_service::AuditEntityType::Tag,
            Some(tag_id),
            tag.as_ref().map(|tag| tag.name.as_str()),
            || details.clone(),
        )
        .await;
    }
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

async fn batch_detach_tag_response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    tag_id: i64,
    body: &BatchTagBindingReq,
    audit_ctx: Option<&audit_service::AuditContext>,
) -> Result<HttpResponse> {
    let tag = if audit_ctx.is_some() {
        Some(tag_service::get_basic_in_scope(state, scope, tag_id).await?)
    } else {
        None
    };
    tag_service::batch_detach_in_scope(state, scope, tag_id, &body.file_ids, &body.folder_ids)
        .await?;
    if let Some(audit_ctx) = audit_ctx {
        let details = tag_assignment_audit_details(TagAssignmentAuditInput {
            operation: "batch_detach",
            scope,
            tag: tag.as_ref(),
            entity_type: None,
            entity_id: None,
            file_count: Some(body.file_ids.len()),
            folder_count: Some(body.folder_ids.len()),
            tag_count: None,
        });
        audit_service::log_with_details(
            state,
            audit_ctx,
            audit_service::AuditAction::TagDetach,
            audit_service::AuditEntityType::Tag,
            Some(tag_id),
            tag.as_ref().map(|tag| tag.name.as_str()),
            || details.clone(),
        )
        .await;
    }
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

fn entity_tags_response(tags: EntityTags) -> HttpResponse {
    HttpResponse::Ok().json(ApiResponse::ok(tags))
}

struct TagAssignmentAuditInput<'a> {
    operation: &'a str,
    scope: WorkspaceStorageScope,
    tag: Option<&'a MinimalTagInfo>,
    entity_type: Option<EntityType>,
    entity_id: Option<i64>,
    file_count: Option<usize>,
    folder_count: Option<usize>,
    tag_count: Option<usize>,
}

fn tag_assignment_audit_details(input: TagAssignmentAuditInput<'_>) -> Option<serde_json::Value> {
    audit_service::details(audit_service::TagAssignmentAuditDetails {
        operation: input.operation,
        tag_id: input.tag.map(|tag| tag.id),
        tag_name: input.tag.map(|tag| tag.name.as_str()),
        tag_color: input.tag.map(|tag| tag.color.as_str()),
        entity_type: input.entity_type,
        entity_id: input.entity_id,
        file_count: input.file_count,
        folder_count: input.folder_count,
        tag_count: input.tag_count,
        team_id: input.scope.team_id(),
    })
}
