//! 回收站服务子模块：`models`。

use serde::Serialize;

#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TrashFileItem {
    pub id: i64,
    pub name: String,
    pub size: i64,
    pub mime_type: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub is_locked: bool,
    pub original_path: String,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TrashFolderItem {
    pub id: i64,
    pub name: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub is_locked: bool,
    pub original_path: String,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TrashContents {
    pub folders: Vec<TrashFolderItem>,
    pub files: Vec<TrashFileItem>,
    pub folders_total: u64,
    pub files_total: u64,
    /// 下一页 cursor，None 表示已到最后一页
    pub next_file_cursor: Option<TrashFileCursor>,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TrashFileCursor {
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub id: i64,
}
