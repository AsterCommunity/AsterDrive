//! SeaORM 实体定义：`file`。

use sea_orm::entity::prelude::*;
use sea_orm::{ActiveValue, ConnectionTrait, DbErr, Set};
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), schema(as = FileEntity))]
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
    /// Lowercase final extension without a leading dot. Empty when the file name has no extension.
    pub extension: String,
    /// Lowercase multi-part extension without a leading dot, such as `tar.gz`.
    /// Populated only when the file name ends with a supported compound extension.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compound_extension: Option<String>,
    /// Category derived from the extension first, then MIME type as fallback.
    pub file_category: aster_forge_file_classification::FileCategory,
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

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, db: &C, insert: bool) -> std::result::Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        if self.name.is_set() || self.mime_type.is_set() {
            let mut name = active_string_value(&self.name).map(ToOwned::to_owned);
            let mut mime_type = active_string_value(&self.mime_type).map(ToOwned::to_owned);
            if !insert
                && (name.is_none() || mime_type.is_none())
                && let Some(id) = active_i64_value(&self.id)
                && let Some(existing) = Entity::find_by_id(id).one(db).await?
            {
                if name.is_none() {
                    name = Some(existing.name);
                }
                if mime_type.is_none() {
                    mime_type = Some(existing.mime_type);
                }
            }

            if let (Some(name), Some(mime_type)) = (name.as_deref(), mime_type.as_deref()) {
                let classification =
                    aster_forge_file_classification::classify_file(name, mime_type);
                self.extension = Set(classification.extension);
                self.compound_extension = Set(classification.compound_extension);
                self.file_category = Set(classification.category);
            }
        }

        Ok(self)
    }
}

fn active_string_value(value: &ActiveValue<String>) -> Option<&str> {
    match value {
        ActiveValue::Set(value) | ActiveValue::Unchanged(value) => Some(value.as_str()),
        ActiveValue::NotSet => None,
    }
}

fn active_i64_value(value: &ActiveValue<i64>) -> Option<i64> {
    match value {
        ActiveValue::Set(value) | ActiveValue::Unchanged(value) => Some(*value),
        ActiveValue::NotSet => None,
    }
}
