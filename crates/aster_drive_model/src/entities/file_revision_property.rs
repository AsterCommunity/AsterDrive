//! User-defined WebDAV dead-property snapshot attached to a revision.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "file_revision_properties")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub revision_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub namespace: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub name: String,
    pub xml_value: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::file_revision::Entity",
        from = "Column::RevisionId",
        to = "super::file_revision::Column::Id"
    )]
    Revision,
}

impl Related<super::file_revision::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Revision.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
