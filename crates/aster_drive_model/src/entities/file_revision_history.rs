//! Canonical revision history owned by one file identity.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "file_revision_histories")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub public_id: String,
    pub file_id: Option<i64>,
    pub current_revision_id: Option<i64>,
    pub next_sequence: i64,
    pub deltav_controlled_at: Option<DateTimeUtc>,
    pub deltav_root_revision_id: Option<i64>,
    pub created_at: DateTimeUtc,
    pub retired_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::file::Entity",
        from = "Column::FileId",
        to = "super::file::Column::Id"
    )]
    File,
    #[sea_orm(has_many = "super::file_revision::Entity")]
    Revisions,
}

impl Related<super::file::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::File.def()
    }
}

impl Related<super::file_revision::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Revisions.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
