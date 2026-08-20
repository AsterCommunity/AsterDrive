use sea_orm::DatabaseConnection;

use crate::db::repository::managed_follower_repo;
use crate::errors::Result;

pub(super) async fn persist_tunnel_error(
    db: &DatabaseConnection,
    remote_node_id: i64,
    error: String,
) -> Result<()> {
    let remote_node = managed_follower_repo::find_by_id(db, remote_node_id).await?;
    managed_follower_repo::touch_tunnel_result(
        db,
        remote_node_id,
        error,
        remote_node.tunnel_last_handshake_at,
    )
    .await?;
    Ok(())
}
