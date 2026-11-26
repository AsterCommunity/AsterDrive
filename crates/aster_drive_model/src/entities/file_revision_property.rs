//! User-defined WebDAV dead-property snapshot attached to a revision.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// WebDAV dead property (`PROPPATCH`) snapshot stored on one revision. Rows are
/// cloned from the live file properties when a revision is appended, so each
/// revision keeps the property values it was created with.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "file_revision_properties")]
pub struct Model {
    /// Revision owning this property snapshot; part of the composite primary key.
    #[sea_orm(primary_key, auto_increment = false)]
    pub revision_id: i64,
    /// Property namespace URI, such as `DAV:` or a vendor XML namespace; part of
    /// the composite primary key.
    #[sea_orm(primary_key, auto_increment = false)]
    pub namespace: String,
    /// Property name local to `namespace` (no prefix); part of the composite
    /// primary key.
    #[sea_orm(primary_key, auto_increment = false)]
    pub name: String,
    /// Serialized XML value of the property; `None` for properties without a
    /// stored value.
    pub xml_value: Option<String>,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// The revision carrying this property snapshot (FK `revision_id`,
    /// cascade on delete).
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
