//! SeaORM 实体定义：`file`。

use sea_orm::entity::prelude::*;
use sea_orm::{ActiveValue, ConnectionTrait, DbErr, Set};
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// File entry: name, workspace placement, current content blob, and attribution.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), schema(as = FileEntity))]
#[sea_orm(table_name = "files")]
pub struct Model {
    /// Auto-incrementing surrogate key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Display name including the extension; unique among live siblings within the same
    /// workspace folder.
    pub name: String,
    /// Parent folder; references `folders.id`. `None` places the file at the workspace
    /// root level outside any folder row.
    pub folder_id: Option<i64>,
    /// Owning team workspace; `None` for personal files. Exactly one of `team_id` and
    /// `owner_user_id` identifies the workspace root.
    pub team_id: Option<i64>,
    /// Content blob holding the current version; references `file_blobs.id`.
    pub blob_id: i64,
    /// Logical content size in bytes; mirrored from the blob and quota-accounted.
    pub size: i64,
    /// Owner account for personal files; references `users.id`. `None` for team-owned files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_user_id: Option<i64>,
    /// Account that created the file; `None` when the creator was deleted or unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_user_id: Option<i64>,
    /// Creator username snapshotted at creation so history survives account deletion.
    pub created_by_username: String,
    /// Account behind the latest modification; `None` when the modifier was deleted or unknown.
    pub modified_by_user_id: Option<i64>,
    /// Modifier username snapshotted at modification; `None` when never modified.
    pub modified_by_username: Option<String>,
    /// UTC instant of the latest content or metadata modification; `None` when never modified.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub modified_at: Option<DateTimeUtc>,
    /// MIME type of the current content, as reported at upload or reclassification.
    pub mime_type: String,
    /// Lowercase final extension without a leading dot. Empty when the file name has no extension.
    pub extension: String,
    /// Lowercase multi-part extension without a leading dot, such as `tar.gz`.
    /// Populated only when the file name ends with a supported compound extension.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compound_extension: Option<String>,
    /// Category derived from the extension first, then MIME type as fallback.
    pub file_category: aster_forge_file_classification::FileCategory,
    /// UTC instant the file entry was created.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    /// UTC instant of the latest row update.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
    /// UTC instant the file was moved to trash; `None` while the file is live. Soft delete
    /// marker — purged rows are removed physically.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub deleted_at: Option<DateTimeUtc>,
}

/// Declared database relations for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// Owning account of a personal file, matched on `owner_user_id` to `users.id`.
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::OwnerUserId",
        to = "super::user::Column::Id"
    )]
    OwnerUser,
    /// Creating account, matched on `created_by_user_id` to `users.id`.
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::CreatedByUserId",
        to = "super::user::Column::Id"
    )]
    CreatedByUser,
    /// Parent folder, matched on `folder_id` to `folders.id`.
    #[sea_orm(
        belongs_to = "super::folder::Entity",
        from = "Column::FolderId",
        to = "super::folder::Column::Id"
    )]
    Folder,
    /// Owning team workspace, matched on `team_id` to `teams.id`.
    #[sea_orm(
        belongs_to = "super::team::Entity",
        from = "Column::TeamId",
        to = "super::team::Column::Id"
    )]
    Team,
    /// Content blob of the current version, matched on `blob_id` to `file_blobs.id`.
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
        if insert {
            self.initialize_attribution();
        }
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

impl ActiveModel {
    /// Initialize attribution only for genuinely new resources, never for migrated rows.
    pub fn initialize_attribution(&mut self) {
        self.modified_by_user_id = self.created_by_user_id.clone();
        if let Some(name) = self.created_by_username.clone().take() {
            self.modified_by_username = Set(Some(name));
        }
        if let Some(at) = self.created_at.clone().take() {
            self.modified_at = Set(Some(at));
        }
    }
}
