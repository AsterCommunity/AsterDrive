//! Public, same-origin assets owned by built-in storage connectors.

use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use actix_web::{HttpResponse, web};
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
        (status = 404, description = "Connector icon not found"),
    ),
)]
pub(crate) async fn get_connector_icon(
    state: web::Data<PrimaryAppState>,
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

    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "public, max-age=31536000, immutable"))
        .insert_header((
            "ETag",
            format!("\"connector-icon-{}-{}\"", connector_id, icon.revision),
        ))
        .content_type(icon.content_type)
        .body(icon.bytes))
}
