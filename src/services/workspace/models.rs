//! 服务模块：`workspace::models`。
//!
//! 定义 workspace 域对 API 层暴露的文件、目录与版本视图模型，以及从实体模型到视图的转换。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::services::content::tag::TagSummary;
use crate::services::files::lock::ResourceLockState;

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
/// File view model returned by list and detail endpoints.
pub struct FileInfo {
    /// File ID.
    pub id: i64,
    /// File name including its extension, unique within the parent folder.
    pub name: String,
    /// Parent folder ID; `None` at the workspace root.
    pub folder_id: Option<i64>,
    /// Owning team ID; `None` in a personal workspace.
    pub team_id: Option<i64>,
    /// Blob holding the current version content.
    pub blob_id: i64,
    /// Current version size in bytes.
    pub size: i64,
    /// Total quota bytes for the file detail view: current `size` plus all
    /// historical version sizes. `size` is only the current version size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_used: Option<i64>,
    /// Workspace owner user ID; `None` for files in a team workspace.
    pub owner_user_id: Option<i64>,
    /// Creator user ID.
    pub created_by_user_id: Option<i64>,
    /// Creator username denormalized at creation time.
    pub created_by_username: String,
    /// Creator identity resolved for display; `None` when both actor columns are absent.
    pub created_by: Option<super::activity::ActivityIdentity>,
    /// Last modifier identity resolved for display; `None` when the file was never modified.
    pub modified_by: Option<super::activity::ActivityIdentity>,
    /// Last modifier user ID; `None` when the file was never modified.
    pub modified_by_user_id: Option<i64>,
    /// Last modifier username; `None` when the file was never modified.
    pub modified_by_username: Option<String>,
    /// Last modification time; `None` when the file was never modified after creation.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub modified_at: Option<DateTime<Utc>>,
    /// MIME type of the current version.
    pub mime_type: String,
    /// Lowercase extension without the leading dot; empty when the name has none.
    pub extension: String,
    /// Full extension including intermediate parts (e.g. `tar.gz`); `None` for a single or no extension.
    pub compound_extension: Option<String>,
    /// Content category derived from the file type for classification-driven behavior.
    pub file_category: aster_forge_file_classification::FileCategory,
    /// Creation time.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTime<Utc>,
    /// Last row update time, including metadata-only changes.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    /// Soft-delete time; omitted while the file is live.
    pub deleted_at: Option<DateTime<Utc>>,
    /// Active resource lock on the file, surfaced so clients can gate mutation affordances.
    pub lock_state: ResourceLockState,
    /// Tags attached to the file; empty on list views that skip tag loading.
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
            created_by: super::activity::snapshot_identity(
                model.created_by_user_id,
                Some(&model.created_by_username),
            ),
            modified_by: super::activity::snapshot_identity(
                model.modified_by_user_id,
                model.modified_by_username.as_deref(),
            ),
            created_by_username: model.created_by_username,
            modified_by_user_id: model.modified_by_user_id,
            modified_by_username: model.modified_by_username,
            modified_at: model.modified_at,
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
    /// Attaches tag summaries; used by endpoints that load tags after the base conversion.
    pub fn with_tags(mut self, tags: Vec<TagSummary>) -> Self {
        self.tags = tags;
        self
    }

    /// Attaches the file's lock state; the entity conversion cannot derive it on its own.
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
/// Folder view model returned by list and detail endpoints.
pub struct FolderInfo {
    /// Folder ID.
    pub id: i64,
    /// Folder name, unique among siblings.
    pub name: String,
    /// Parent folder ID; `None` at the workspace root.
    pub parent_id: Option<i64>,
    /// Owning team ID; `None` in a personal workspace.
    pub team_id: Option<i64>,
    /// Workspace owner user ID; `None` for folders in a team workspace.
    pub owner_user_id: Option<i64>,
    /// Creator user ID.
    pub created_by_user_id: Option<i64>,
    /// Creator username denormalized at creation time.
    pub created_by_username: String,
    /// Creator identity resolved for display; `None` when both actor columns are absent.
    pub created_by: Option<super::activity::ActivityIdentity>,
    /// Last modifier identity resolved for display; `None` when the folder was never modified.
    pub modified_by: Option<super::activity::ActivityIdentity>,
    /// Last modifier user ID; `None` when the folder was never modified.
    pub modified_by_user_id: Option<i64>,
    /// Last modifier username; `None` when the folder was never modified.
    pub modified_by_username: Option<String>,
    /// Last modification time; `None` when the folder was never modified after creation.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub modified_at: Option<DateTime<Utc>>,
    /// Storage policy applied to the folder; `None` inherits the workspace default.
    pub policy_id: Option<i64>,
    /// Folder icon kind and optional custom icon key resolved from the model.
    pub icon: crate::services::files::folder::FolderIcon,
    /// Recursive quota bytes for the folder detail view: all live files in the
    /// folder tree, including current file sizes plus historical versions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_used: Option<i64>,
    /// Creation time.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTime<Utc>,
    /// Last row update time, including metadata-only changes.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    /// Soft-delete time; omitted while the folder is live.
    pub deleted_at: Option<DateTime<Utc>>,
    /// Active resource lock on the folder or its subtree, surfaced for mutation gating.
    pub lock_state: ResourceLockState,
    /// Tags attached to the folder; empty on list views that skip tag loading.
    pub tags: Vec<TagSummary>,
}

impl From<aster_drive_model::entities::folder::Model> for FolderInfo {
    fn from(model: aster_drive_model::entities::folder::Model) -> Self {
        let icon = crate::services::files::folder::FolderIcon::from_model(&model);
        Self {
            id: model.id,
            name: model.name,
            parent_id: model.parent_id,
            team_id: model.team_id,
            owner_user_id: model.owner_user_id,
            created_by_user_id: model.created_by_user_id,
            created_by: super::activity::snapshot_identity(
                model.created_by_user_id,
                Some(&model.created_by_username),
            ),
            modified_by: super::activity::snapshot_identity(
                model.modified_by_user_id,
                model.modified_by_username.as_deref(),
            ),
            created_by_username: model.created_by_username,
            modified_by_user_id: model.modified_by_user_id,
            modified_by_username: model.modified_by_username,
            modified_at: model.modified_at,
            policy_id: model.policy_id,
            icon,
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
    /// Attaches tag summaries; used by endpoints that load tags after the base conversion.
    pub fn with_tags(mut self, tags: Vec<TagSummary>) -> Self {
        self.tags = tags;
        self
    }

    /// Attaches the folder's lock state; the entity conversion cannot derive it on its own.
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
/// File revision view model returned by the version list endpoint.
pub struct FileVersion {
    /// Revision row ID.
    pub id: i64,
    /// Owning file ID.
    pub file_id: i64,
    /// Opaque public identifier safe to expose outside the workspace.
    pub public_id: String,
    /// Blob holding this revision's content; `None` after the underlying blob was garbage collected.
    pub blob_id: Option<i64>,
    /// Monotonically increasing sequence number of this revision within the file.
    pub version: i64,
    /// Logical size of this revision in bytes.
    pub size: i64,
    /// MIME type captured for this revision; `None` when not recorded.
    pub mime_type: Option<String>,
    /// ETag of this revision's content.
    pub etag: String,
    /// Creator user ID; `None` for system-created revisions.
    pub creator_user_id: Option<i64>,
    /// Creator display name denormalized at creation time; `None` for system-created revisions.
    pub creator_display_name: Option<String>,
    /// Client-supplied revision comment; `None` when absent.
    pub comment: Option<String>,
    /// Why this revision exists, such as an upload or a restore.
    pub reason: String,
    /// Whether this revision is the file's current version.
    pub current: bool,
    /// Revision creation time.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(utoipa::IntoParams, ToSchema)
)]
/// Keyset pagination parameters for the file version list.
pub struct FileVersionListQuery {
    /// Maximum revisions returned per request (default 100, maximum 1000).
    pub limit: Option<u64>,
    /// Stable keyset cursor: the last sequence returned by the previous page.
    pub after_sequence: Option<i64>,
}

impl FileVersionListQuery {
    /// Applies the default and clamps the requested limit into the 1..=1000 range.
    pub fn limit(self) -> u64 {
        self.limit.unwrap_or(100).clamp(1, 1000)
    }
}

impl FileVersion {
    /// Converts a revision entity and marks `current` against the file's current revision ID.
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
