//! Database access for the reverse-tunnel owner directory.

use crate::entities::remote_tunnel_owner;
use crate::errors::{AsterError, Result};
use sea_orm::{ConnectionTrait, EntityTrait, Set, sea_query::OnConflict};

pub async fn find_by_remote_node_id<C: ConnectionTrait>(
    db: &C,
    remote_node_id: i64,
) -> Result<Option<remote_tunnel_owner::Model>> {
    remote_tunnel_owner::Entity::find_by_id(remote_node_id)
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn upsert<C: ConnectionTrait>(
    db: &C,
    owner: remote_tunnel_owner::Model,
) -> Result<remote_tunnel_owner::Model> {
    remote_tunnel_owner::Entity::insert(remote_tunnel_owner::ActiveModel {
        remote_node_id: Set(owner.remote_node_id),
        runtime_id: Set(owner.runtime_id),
        internal_endpoint: Set(owner.internal_endpoint),
        fencing_token: Set(owner.fencing_token),
        lease_expires_at: Set(owner.lease_expires_at),
        updated_at: Set(owner.updated_at),
    })
    .on_conflict(
        OnConflict::column(remote_tunnel_owner::Column::RemoteNodeId)
            .update_columns([
                remote_tunnel_owner::Column::RuntimeId,
                remote_tunnel_owner::Column::InternalEndpoint,
                remote_tunnel_owner::Column::FencingToken,
                remote_tunnel_owner::Column::LeaseExpiresAt,
                remote_tunnel_owner::Column::UpdatedAt,
            ])
            .to_owned(),
    )
    .exec(db)
    .await
    .map_err(AsterError::from)?;

    find_by_remote_node_id(db, owner.remote_node_id)
        .await?
        .ok_or_else(|| AsterError::record_not_found("remote tunnel owner after upsert"))
}

pub async fn delete_if_fencing_token<C: ConnectionTrait>(
    db: &C,
    remote_node_id: i64,
    fencing_token: &str,
) -> Result<bool> {
    use sea_orm::{ColumnTrait, QueryFilter};

    let result = remote_tunnel_owner::Entity::delete_many()
        .filter(remote_tunnel_owner::Column::RemoteNodeId.eq(remote_node_id))
        .filter(remote_tunnel_owner::Column::FencingToken.eq(fencing_token))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}
