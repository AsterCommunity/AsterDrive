//! 回收站服务子模块：`models`。

use serde::Serialize;

#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// File entry shown in a trash listing.
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TrashFileItem {
    /// File id.
    pub id: i64,
    /// File name at deletion time.
    pub name: String,
    /// Logical size in bytes.
    pub size: i64,
    /// Declared MIME type of the current blob.
    pub mime_type: String,
    /// Original creation timestamp.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last update timestamp before deletion.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// When the file is purged automatically, i.e. deletion plus the
    /// configured retention days.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: chrono::DateTime<chrono::Utc>,
    /// Active lock state carried over from before deletion.
    pub lock_state: crate::services::files::lock::ResourceLockState,
    /// Display path of the folder the file was deleted from, `/` for the root.
    pub original_path: String,
}

/// Folder entry shown in a trash listing.
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TrashFolderItem {
    /// Folder id.
    pub id: i64,
    /// Folder name at deletion time.
    pub name: String,
    /// Folder icon resolved from the persisted icon pair.
    pub icon: crate::services::files::folder::FolderIcon,
    /// Original creation timestamp.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last update timestamp before deletion.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// When the folder is purged automatically, i.e. deletion plus the
    /// configured retention days.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: chrono::DateTime<chrono::Utc>,
    /// Active lock state carried over from before deletion.
    pub lock_state: crate::services::files::lock::ResourceLockState,
    /// Display path of the parent folder the folder was deleted from,
    /// `/` for the root.
    pub original_path: String,
}

/// Page of a trash listing, with separate folder and file sections.
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TrashContents {
    /// Trashed folder entries of the requested page.
    pub folders: Vec<TrashFolderItem>,
    /// Trashed file entries of the requested page.
    pub files: Vec<TrashFileItem>,
    /// Total number of trashed folders.
    pub folders_total: u64,
    /// Total number of trashed files.
    pub files_total: u64,
    /// 下一页 cursor，None 表示已到最后一页
    pub next_file_cursor: Option<TrashFileCursor>,
}

/// Opaque cursor for trash file pagination.
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TrashFileCursor {
    /// Client-facing sort key: when the entry expires from trash.
    pub expires_at: chrono::DateTime<chrono::Utc>,
    /// File id used to break ties between entries with equal expiry.
    pub id: i64,
}
