//! SeaORM entity for the shared reverse-tunnel owner directory.

use sea_orm::entity::prelude::*;

/// Shared reverse-tunnel owner directory for multi-primary deployments:
/// records which primary runtime currently owns the tunnel of a follower node,
/// under a lease that must be renewed before it expires.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "remote_tunnel_owners")]
pub struct Model {
    /// Follower node whose tunnel is owned; one row per node.
    #[sea_orm(primary_key, auto_increment = false)]
    pub remote_node_id: i64,
    /// Identifier of the primary runtime instance that currently owns the
    /// tunnel.
    pub runtime_id: String,
    /// Internal address other primaries use to reach the owning runtime when
    /// handing off or proxying tunnel traffic.
    pub internal_endpoint: String,
    /// Opaque token fencing delete/release operations: a stale owner can only
    /// remove its entry while its token still matches.
    pub fencing_token: String,
    /// Lease deadline; the entry is stale once this time passes without
    /// renewal.
    pub lease_expires_at: DateTimeUtc,
    /// Last lease acquisition or renewal timestamp (UTC).
    pub updated_at: DateTimeUtc,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// The follower node whose tunnel is owned (FK `remote_node_id`,
    /// cascading).
    #[sea_orm(
        belongs_to = "super::managed_follower::Entity",
        from = "Column::RemoteNodeId",
        to = "super::managed_follower::Column::Id"
    )]
    ManagedFollower,
}

impl Related<super::managed_follower::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ManagedFollower.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
