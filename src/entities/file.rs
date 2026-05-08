//! SeaORM 实体定义：`file`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), schema(as = FileInfo))]
#[sea_orm(table_name = "files")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub name: String,
    pub folder_id: Option<i64>,
    pub team_id: Option<i64>,
    pub blob_id: i64,
    pub size: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_user_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_user_id: Option<i64>,
    pub created_by_username: String,
    pub mime_type: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub deleted_at: Option<DateTimeUtc>,
    #[serde(default)]
    pub is_locked: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::OwnerUserId",
        to = "super::user::Column::Id"
    )]
    OwnerUser,
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::CreatedByUserId",
        to = "super::user::Column::Id"
    )]
    CreatedByUser,
    #[sea_orm(
        belongs_to = "super::folder::Entity",
        from = "Column::FolderId",
        to = "super::folder::Column::Id"
    )]
    Folder,
    #[sea_orm(
        belongs_to = "super::team::Entity",
        from = "Column::TeamId",
        to = "super::team::Column::Id"
    )]
    Team,
    #[sea_orm(
        belongs_to = "super::file_blob::Entity",
        from = "Column::BlobId",
        to = "super::file_blob::Column::Id"
    )]
    FileBlob,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::OwnerUser.def()
    }
}

impl Related<super::folder::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Folder.def()
    }
}

impl Related<super::team::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Team.def()
    }
}

impl Related<super::file_blob::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::FileBlob.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
