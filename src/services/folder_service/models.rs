//! 文件夹服务子模块：`models`。

use serde::Serialize;
use std::collections::HashSet;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::entities::{file, folder};

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct FolderAncestorItem {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct FileListItem {
    pub id: i64,
    pub name: String,
    pub size: i64,
    pub mime_type: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub is_locked: bool,
    pub is_shared: bool,
}

#[derive(Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct FolderListItem {
    pub id: i64,
    pub name: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub is_locked: bool,
    pub is_shared: bool,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct FileCursor {
    /// 排序字段值（序列化为字符串）
    pub value: String,
    pub id: i64,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct FolderContents {
    pub folders: Vec<FolderListItem>,
    pub files: Vec<FileListItem>,
    pub folders_total: u64,
    pub files_total: u64,
    /// 下一页 cursor，None 表示已到最后一页
    pub next_file_cursor: Option<FileCursor>,
}

pub fn build_file_list_items(
    files: Vec<file::Model>,
    shared_file_ids: &HashSet<i64>,
) -> Vec<FileListItem> {
    files
        .into_iter()
        .map(|file| FileListItem {
            id: file.id,
            name: file.name,
            size: file.size,
            mime_type: file.mime_type,
            updated_at: file.updated_at,
            is_locked: file.is_locked,
            is_shared: shared_file_ids.contains(&file.id),
        })
        .collect()
}

pub fn build_folder_list_items(
    folders: Vec<folder::Model>,
    shared_folder_ids: &HashSet<i64>,
) -> Vec<FolderListItem> {
    folders
        .into_iter()
        .map(|folder| FolderListItem {
            id: folder.id,
            name: folder.name,
            updated_at: folder.updated_at,
            is_locked: folder.is_locked,
            is_shared: shared_folder_ids.contains(&folder.id),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn mock_file(id: i64, name: &str, is_locked: bool) -> file::Model {
        file::Model {
            id,
            name: name.to_string(),
            folder_id: None,
            team_id: None,
            blob_id: 1,
            size: 100,
            owner_user_id: Some(1),
            created_by_user_id: Some(1),
            created_by_username: "tester".to_string(),
            mime_type: "text/plain".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
            is_locked,
        }
    }

    fn mock_folder(id: i64, name: &str, is_locked: bool) -> folder::Model {
        folder::Model {
            id,
            name: name.to_string(),
            parent_id: None,
            team_id: None,
            owner_user_id: Some(1),
            created_by_user_id: Some(1),
            created_by_username: "tester".to_string(),
            policy_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
            is_locked,
        }
    }

    #[test]
    fn build_file_list_items_maps_correctly() {
        let files = vec![mock_file(1, "a.txt", false), mock_file(2, "b.txt", true)];
        let shared: HashSet<i64> = [1].into_iter().collect();
        let items = build_file_list_items(files, &shared);

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, 1);
        assert_eq!(items[0].name, "a.txt");
        assert!(items[0].is_shared);
        assert!(!items[0].is_locked);
        assert_eq!(items[1].id, 2);
        assert!(!items[1].is_shared);
        assert!(items[1].is_locked);
    }

    #[test]
    fn build_file_list_items_empty() {
        let items: Vec<FileListItem> = build_file_list_items(vec![], &HashSet::new());
        assert!(items.is_empty());
    }

    #[test]
    fn build_folder_list_items_maps_correctly() {
        let folders = vec![mock_folder(1, "docs", false), mock_folder(2, "pics", true)];
        let shared: HashSet<i64> = [2].into_iter().collect();
        let items = build_folder_list_items(folders, &shared);

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, 1);
        assert!(!items[0].is_shared);
        assert_eq!(items[1].id, 2);
        assert!(items[1].is_shared);
        assert!(items[1].is_locked);
    }
}
