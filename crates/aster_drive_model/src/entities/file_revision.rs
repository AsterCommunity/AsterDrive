//! Immutable content and metadata snapshot in a file revision history.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "file_revisions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub public_id: String,
    pub history_id: i64,
    pub sequence: i64,
    pub predecessor_revision_id: Option<i64>,
    pub blob_id: Option<i64>,
    pub logical_size: i64,
    pub mime_type: Option<String>,
    pub etag: String,
    pub content_sha256: Option<String>,
    pub creator_user_id: Option<i64>,
    pub creator_display_name: Option<String>,
    pub comment: Option<String>,
    pub reason: String,
    pub created_at: DateTimeUtc,
    pub retired_at: Option<DateTimeUtc>,
    pub purged_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::file_revision_history::Entity",
        from = "Column::HistoryId",
        to = "super::file_revision_history::Column::Id"
    )]
    History,
    #[sea_orm(
        belongs_to = "super::file_blob::Entity",
        from = "Column::BlobId",
        to = "super::file_blob::Column::Id"
    )]
    FileBlob,
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::CreatorUserId",
        to = "super::user::Column::Id"
    )]
    Creator,
    #[sea_orm(has_many = "super::file_revision_property::Entity")]
    Properties,
}

impl Related<super::file_revision_history::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::History.def()
    }
}

impl Related<super::file_blob::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::FileBlob.def()
    }
}

impl Related<super::file_revision_property::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Properties.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
