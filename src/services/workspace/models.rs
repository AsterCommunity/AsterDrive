//! 服务模块：`workspace::models`。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::services::content::tag::TagSummary;
use crate::services::files::lock::ResourceLockState;

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct FileInfo {
    pub id: i64,
    pub name: String,
    pub folder_id: Option<i64>,
    pub team_id: Option<i64>,
    pub blob_id: i64,
    pub size: i64,
    /// Total quota bytes for the file detail view: current `size` plus all
    /// historical version sizes. `size` is only the current version size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_used: Option<i64>,
    pub owner_user_id: Option<i64>,
    pub created_by_user_id: Option<i64>,
    pub created_by_username: String,
    pub mime_type: String,
    pub extension: String,
    pub compound_extension: Option<String>,
    pub file_category: aster_forge_file_classification::FileCategory,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTime<Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub deleted_at: Option<DateTime<Utc>>,
    pub lock_state: ResourceLockState,
    pub tags: Vec<TagSummary>,
}

impl From<aster_drive_model::entities::file::Model> for FileInfo {
    fn from(model: aster_drive_model::entities::file::Model) -> Self {
        Self {
            id: model.id,
            name: model.name,
            folder_id: model.folder_id,
            team_id: model.team_id,
            blob_id: model.blob_id,
            size: model.size,
            storage_used: None,
            owner_user_id: model.owner_user_id,
            created_by_user_id: model.created_by_user_id,
            created_by_username: model.created_by_username,
            mime_type: model.mime_type,
            extension: model.extension,
            compound_extension: model.compound_extension,
            file_category: model.file_category,
            created_at: model.created_at,
            updated_at: model.updated_at,
            deleted_at: model.deleted_at,
            lock_state: ResourceLockState::Unlocked,
            tags: vec![],
        }
    }
}

impl FileInfo {
    pub fn with_tags(mut self, tags: Vec<TagSummary>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_lock_state(mut self, lock_state: ResourceLockState) -> Self {
        self.lock_state = lock_state;
        self
    }

    /// Builds a detail `FileInfo` from a file model and attaches explicit
    /// quota bytes (`storage_used`) for the current file plus its versions.
    pub fn from_model_with_storage_used(
        model: aster_drive_model::entities::file::Model,
        storage_used: i64,
    ) -> Self {
        let mut info = Self::from(model);
        info.storage_used = Some(storage_used);
        info
    }
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct FolderInfo {
    pub id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
    pub team_id: Option<i64>,
    pub owner_user_id: Option<i64>,
    pub created_by_user_id: Option<i64>,
    pub created_by_username: String,
    pub policy_id: Option<i64>,
    /// Recursive quota bytes for the folder detail view: all live files in the
    /// folder tree, including current file sizes plus historical versions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_used: Option<i64>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTime<Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub deleted_at: Option<DateTime<Utc>>,
    pub lock_state: ResourceLockState,
    pub tags: Vec<TagSummary>,
}

impl From<aster_drive_model::entities::folder::Model> for FolderInfo {
    fn from(model: aster_drive_model::entities::folder::Model) -> Self {
        Self {
            id: model.id,
            name: model.name,
            parent_id: model.parent_id,
            team_id: model.team_id,
            owner_user_id: model.owner_user_id,
            created_by_user_id: model.created_by_user_id,
            created_by_username: model.created_by_username,
            policy_id: model.policy_id,
            storage_used: None,
            created_at: model.created_at,
            updated_at: model.updated_at,
            deleted_at: model.deleted_at,
            lock_state: ResourceLockState::Unlocked,
            tags: vec![],
        }
    }
}

impl FolderInfo {
    pub fn with_tags(mut self, tags: Vec<TagSummary>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_lock_state(mut self, lock_state: ResourceLockState) -> Self {
        self.lock_state = lock_state;
        self
    }

    /// Builds a detail `FolderInfo` via `FolderInfo::from` and sets recursive
    /// quota bytes (`storage_used`) for the details endpoint.
    pub fn from_model_with_storage_used(
        model: aster_drive_model::entities::folder::Model,
        storage_used: i64,
    ) -> Self {
        let mut info = Self::from(model);
        info.storage_used = Some(storage_used);
        info
    }
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct FileVersion {
    pub id: i64,
    pub file_id: i64,
    pub public_id: String,
    pub blob_id: Option<i64>,
    pub version: i64,
    pub size: i64,
    pub mime_type: Option<String>,
    pub etag: String,
    pub creator_user_id: Option<i64>,
    pub creator_display_name: Option<String>,
    pub comment: Option<String>,
    pub reason: String,
    pub current: bool,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(utoipa::IntoParams, ToSchema)
)]
pub struct FileVersionListQuery {
    /// Maximum revisions returned per request (default 100, maximum 1000).
    pub limit: Option<u64>,
    /// Stable keyset cursor: the last sequence returned by the previous page.
    pub after_sequence: Option<i64>,
}

impl FileVersionListQuery {
    pub fn limit(self) -> u64 {
        self.limit.unwrap_or(100).clamp(1, 1000)
    }
}

impl FileVersion {
    pub fn from_revision(
        file_id: i64,
        current_revision_id: Option<i64>,
        model: aster_drive_model::entities::file_revision::Model,
    ) -> Self {
        Self {
            id: model.id,
            file_id,
            public_id: model.public_id,
            blob_id: model.blob_id,
            version: model.sequence,
            size: model.logical_size,
            mime_type: model.mime_type,
            etag: model.etag,
            creator_user_id: model.creator_user_id,
            creator_display_name: model.creator_display_name,
            comment: model.comment,
            reason: model.reason,
            current: current_revision_id == Some(model.id),
            created_at: model.created_at,
        }
    }
}
