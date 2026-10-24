use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use aster_drive_model::entities::managed_follower;

pub async fn authorize_tunnel_request<S: SharedRuntimeState>(
    state: &S,
    method: &actix_web::http::Method,
    path_and_query: &str,
    headers: &actix_web::http::header::HeaderMap,
    content_length: Option<u64>,
) -> Result<managed_follower::Model> {
    let remote_node = crate::storage::remote_protocol::authorize_remote_node_request(
        state,
        method,
        path_and_query,
        headers,
        content_length,
    )
    .await?;
    if !remote_node.is_enabled {
        return Err(AsterError::validation_error("remote node is disabled"));
    }
    super::ensure_reverse_tunnel_transport(&remote_node)?;
    Ok(remote_node)
}
