//! Immutable workspace business activity. No cascading actor or resource relations.
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "workspace_activities")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub event_id: String,
    pub operation_id: String,
    pub team_id: Option<i64>,
    pub owner_user_id: Option<i64>,
    pub entity_type: String,
    pub entity_id: i64,
    pub entity_name: String,
    pub actor_user_id: Option<i64>,
    pub actor_name: String,
    pub action: String,
    pub source: String,
    pub details: String,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
