//! 文件夹服务子模块：`models`。

use serde::Serialize;
use std::collections::HashSet;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::entities::{file, folder};
use crate::services::tag_service::TagSummary;
use crate::types::EntityType;

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
    /// Lowercase final extension without a leading dot. Empty when the file name has no extension.
    pub extension: String,
    /// Lowercase multi-part extension without a leading dot, such as `tar.gz`.
    /// Populated only when the file name ends with a supported compound extension.
    pub compound_extension: Option<String>,
    /// Category derived from the extension first, then MIME type as fallback.
    pub file_category: crate::types::FileCategory,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub is_locked: bool,
    pub is_shared: bool,
    pub tags: Vec<TagSummary>,
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
    pub tags: Vec<TagSummary>,
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
    build_file_list_items_with_tags(files, shared_file_ids, &std::collections::HashMap::new())
}

pub fn build_file_list_items_with_tags(
    files: Vec<file::Model>,
    shared_file_ids: &HashSet<i64>,
    tags_by_entity: &std::collections::HashMap<(EntityType, i64), Vec<TagSummary>>,
) -> Vec<FileListItem> {
    files
        .into_iter()
        .map(|file| FileListItem {
            id: file.id,
            name: file.name,
            size: file.size,
            mime_type: file.mime_type,
            extension: file.extension,
            compound_extension: file.compound_extension,
            file_category: file.file_category,
            updated_at: file.updated_at,
            is_locked: file.is_locked,
            is_shared: shared_file_ids.contains(&file.id),
            tags: tags_by_entity
                .get(&(EntityType::File, file.id))
                .cloned()
                .unwrap_or_default(),
        })
        .collect()
}

pub fn build_folder_list_items(
    folders: Vec<folder::Model>,
    shared_folder_ids: &HashSet<i64>,
) -> Vec<FolderListItem> {
    build_folder_list_items_with_tags(
        folders,
        shared_folder_ids,
        &std::collections::HashMap::new(),
    )
}

pub fn build_folder_list_items_with_tags(
    folders: Vec<folder::Model>,
    shared_folder_ids: &HashSet<i64>,
    tags_by_entity: &std::collections::HashMap<(EntityType, i64), Vec<TagSummary>>,
) -> Vec<FolderListItem> {
    folders
        .into_iter()
        .map(|folder| FolderListItem {
            id: folder.id,
            name: folder.name,
            updated_at: folder.updated_at,
            is_locked: folder.is_locked,
            is_shared: shared_folder_ids.contains(&folder.id),
            tags: tags_by_entity
                .get(&(EntityType::Folder, folder.id))
                .cloned()
                .unwrap_or_default(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    fn mock_file(
        id: i64,
        name: &str,
        is_locked: bool,
        extension: &str,
        compound_extension: Option<&str>,
        file_category: crate::types::FileCategory,
    ) -> file::Model {
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
            extension: extension.to_string(),
            compound_extension: compound_extension.map(ToOwned::to_owned),
            file_category,
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
        let files = vec![
            mock_file(
                1,
                "a.txt",
                false,
                "txt",
                None,
                crate::types::FileCategory::Document,
            ),
            mock_file(
                2,
                "backup.tar.gz",
                true,
                "gz",
                Some("tar.gz"),
                crate::types::FileCategory::Archive,
            ),
            mock_file(
                3,
                "README",
                false,
                "",
                None,
                crate::types::FileCategory::Other,
            ),
        ];
        let shared: HashSet<i64> = [1].into_iter().collect();
        let items = build_file_list_items(files, &shared);

        assert_eq!(items.len(), 3);
        assert_eq!(items[0].id, 1);
        assert_eq!(items[0].name, "a.txt");
        assert_eq!(items[0].extension, "txt");
        assert_eq!(items[0].compound_extension, None);
        assert_eq!(items[0].file_category, crate::types::FileCategory::Document);
        assert!(items[0].is_shared);
        assert!(!items[0].is_locked);
        assert_eq!(items[1].id, 2);
        assert_eq!(items[1].extension, "gz");
        assert_eq!(items[1].compound_extension.as_deref(), Some("tar.gz"));
        assert_eq!(items[1].file_category, crate::types::FileCategory::Archive);
        assert!(!items[1].is_shared);
        assert!(items[1].is_locked);
        assert_eq!(items[2].extension, "");
        assert_eq!(items[2].compound_extension, None);
        assert_eq!(items[2].file_category, crate::types::FileCategory::Other);
    }

    #[test]
    fn build_file_list_items_empty() {
        let items: Vec<FileListItem> = build_file_list_items(vec![], &HashSet::new());
        assert!(items.is_empty());
    }

    #[test]
    fn build_file_list_items_with_tags_maps_hits_and_preserves_shared_state() {
        let files = vec![
            mock_file(
                1,
                "a.txt",
                false,
                "txt",
                None,
                crate::types::FileCategory::Document,
            ),
            mock_file(
                2,
                "b.txt",
                false,
                "txt",
                None,
                crate::types::FileCategory::Document,
            ),
        ];
        let shared: HashSet<i64> = [1].into_iter().collect();
        let tag = TagSummary {
            id: 10,
            name: "Work".to_string(),
            color: "#2563eb".to_string(),
        };
        let mut tags_by_entity = HashMap::new();
        tags_by_entity.insert((EntityType::File, 1), vec![tag.clone()]);
        tags_by_entity.insert(
            (EntityType::Folder, 1),
            vec![TagSummary {
                id: 11,
                name: "Folder only".to_string(),
                color: "#059669".to_string(),
            }],
        );

        let items = build_file_list_items_with_tags(files, &shared, &tags_by_entity);

        assert!(items[0].is_shared);
        assert_eq!(items[0].tags.len(), 1);
        assert_eq!(items[0].tags[0].id, tag.id);
        assert!(!items[1].is_shared);
        assert!(items[1].tags.is_empty());
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

    #[test]
    fn build_folder_list_items_with_tags_maps_hits_and_isolates_entity_type() {
        let folders = vec![mock_folder(1, "docs", false), mock_folder(2, "pics", true)];
        let shared: HashSet<i64> = [2].into_iter().collect();
        let tag = TagSummary {
            id: 20,
            name: "Important".to_string(),
            color: "#dc2626".to_string(),
        };
        let mut tags_by_entity = HashMap::new();
        tags_by_entity.insert((EntityType::Folder, 1), vec![tag.clone()]);
        tags_by_entity.insert(
            (EntityType::File, 2),
            vec![TagSummary {
                id: 21,
                name: "File only".to_string(),
                color: "#7c3aed".to_string(),
            }],
        );

        let items = build_folder_list_items_with_tags(folders, &shared, &tags_by_entity);

        assert!(!items[0].is_shared);
        assert_eq!(items[0].tags.len(), 1);
        assert_eq!(items[0].tags[0].id, tag.id);
        assert!(items[1].is_shared);
        assert!(items[1].tags.is_empty());
    }
}
