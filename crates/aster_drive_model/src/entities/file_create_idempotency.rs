//! Persistent request/result mapping for idempotent metadata-only file creation.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "file_create_idempotencies")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub actor_user_id: i64,
    pub workspace_kind: String,
    pub workspace_id: i64,
    pub key_hash: String,
    pub request_fingerprint: String,
    pub result_file_id: Option<i64>,
    pub created_at: DateTimeUtc,
    pub expires_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::file::Entity",
        from = "Column::ResultFileId",
        to = "super::file::Column::Id"
    )]
    File,
}

impl Related<super::file::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::File.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
