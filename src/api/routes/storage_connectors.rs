//! Public, same-origin assets owned by built-in storage connectors.

use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use actix_web::{HttpRequest, HttpResponse, http::header, web};
use aster_drive_storage::ConnectorId;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct IconQuery {
    v: Option<String>,
}

pub fn routes() -> actix_web::Scope {
    web::scope("/storage/connectors")
        .route("/{connector_id}/icon", web::get().to(get_connector_icon))
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/storage/connectors/{connector_id}/icon",
    tag = "storage",
    operation_id = "get_storage_connector_icon",
    params(
        ("connector_id" = String, Path, description = "Stable connector id"),
        ("v" = Option<String>, Query, description = "Asset revision")
    ),
    responses(
        (status = 200, description = "Connector icon asset"),
        (status = 304, description = "Connector icon is unchanged"),
        (status = 404, description = "Connector icon not found"),
    ),
)]
pub(crate) async fn get_connector_icon(
    state: web::Data<PrimaryAppState>,
    request: HttpRequest,
    path: web::Path<String>,
    query: web::Query<IconQuery>,
) -> Result<HttpResponse> {
    let connector_id = ConnectorId::declared(path.into_inner());
    let Some(icon) = state.driver_registry().connectors().icon(&connector_id) else {
        return Ok(HttpResponse::NotFound().finish());
    };
    if query
        .v
        .as_deref()
        .is_some_and(|revision| revision != icon.revision)
    {
        return Ok(HttpResponse::NotFound().finish());
    }
    let cache_control = if query.v.is_some() {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    let etag = format!("\"connector-icon-{}-{}\"", connector_id, icon.revision);
    if if_none_match_matches(request.headers().get(header::IF_NONE_MATCH), &etag) {
        return Ok(HttpResponse::NotModified()
            .insert_header((header::ETAG, etag))
            .insert_header((header::CACHE_CONTROL, cache_control))
            .finish());
    }

    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", cache_control))
        .insert_header((header::ETAG, etag))
        .content_type(icon.content_type)
        .body(icon.bytes))
}

fn if_none_match_matches(value: Option<&header::HeaderValue>, current_etag: &str) -> bool {
    let Some(value) = value.and_then(|value| value.to_str().ok()) else {
        return false;
    };
    value.split(',').map(str::trim).any(|candidate| {
        candidate == "*"
            || candidate == current_etag
            || candidate
                .strip_prefix("W/")
                .is_some_and(|weak| weak == current_etag)
    })
}
