//! SeaORM entity for the shared reverse-tunnel owner directory.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "remote_tunnel_owners")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub remote_node_id: i64,
    pub runtime_id: String,
    pub internal_endpoint: String,
    pub fencing_token: String,
    pub lease_expires_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
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
