use sea_orm::DatabaseConnection;

use crate::db::repository::managed_follower_repo;
use crate::errors::Result;

pub(super) async fn persist_tunnel_error(
    db: &DatabaseConnection,
    remote_node_id: i64,
    error: String,
) -> Result<()> {
    managed_follower_repo::touch_tunnel_runtime_error(db, remote_node_id, error).await?;
    Ok(())
}
