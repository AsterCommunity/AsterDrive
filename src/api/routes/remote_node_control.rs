//! Primary-side control plane polled by enrolled followers.

use actix_web::{HttpRequest, HttpResponse, web};
use serde::Deserialize;

use crate::api::response::ApiResponse;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::remote::remote_node;
use crate::storage::remote_protocol::{
    REMOTE_NODE_BINDING_STATE_PATH, authorize_remote_node_request,
};

#[derive(Debug, Deserialize)]
pub struct BindingStateQuery {
    #[serde(default)]
    pub applied_revision: i64,
}

pub fn routes() -> impl actix_web::dev::HttpServiceFactory + use<> {
    web::scope("/internal/remote-node-control")
        .route("/binding-state", web::get().to(get_binding_state))
}

async fn get_binding_state(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    query: web::Query<BindingStateQuery>,
) -> Result<HttpResponse> {
    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or(REMOTE_NODE_BINDING_STATE_PATH);
    let remote_node = authorize_remote_node_request(
        state.get_ref(),
        req.method(),
        path_and_query,
        req.headers(),
        None,
    )
    .await?;
    let desired =
        remote_node::binding_desired_state(state.get_ref(), &remote_node, query.applied_revision)
            .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(desired)))
}
