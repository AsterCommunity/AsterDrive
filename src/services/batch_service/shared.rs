//! 批量操作服务子模块：`shared`。

use std::collections::{HashMap, HashSet};

use crate::db::repository::{file_repo, folder_repo};
use crate::entities::{file, folder};
use crate::errors::{AsterError, Result, display_error};
use crate::runtime::PrimaryAppState;
use crate::services::workspace_storage_service::{self, WorkspaceStorageScope};

use super::MAX_BATCH_ITEMS;

/// 校验批量操作参数：至少一个 ID，不超过上限
pub fn validate_batch_ids(file_ids: &[i64], folder_ids: &[i64]) -> Result<()> {
    if file_ids.is_empty() && folder_ids.is_empty() {
        return Err(AsterError::validation_error(
            "at least one file or folder ID is required",
        ));
    }
    if file_ids.len() + folder_ids.len() > MAX_BATCH_ITEMS {
        return Err(AsterError::validation_error(format!(
            "batch size cannot exceed {MAX_BATCH_ITEMS} items",
        )));
    }
    Ok(())
}

fn build_file_map(files: Vec<file::Model>) -> HashMap<i64, file::Model> {
    let mut map = HashMap::with_capacity(files.len());
    for file in files {
        map.insert(file.id, file);
    }
    map
}

fn build_folder_map(folders: Vec<folder::Model>) -> HashMap<i64, folder::Model> {
    let mut map = HashMap::with_capacity(folders.len());
    for folder in folders {
        map.insert(folder.id, folder);
    }
    map
}

async fn find_files_by_ids_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    ids: &[i64],
) -> Result<Vec<file::Model>> {
    match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            file_repo::find_by_ids_in_personal_scope(&state.db, user_id, ids).await
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            file_repo::find_by_ids_in_team_scope(&state.db, team_id, ids).await
        }
    }
}

async fn find_folders_by_ids_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    ids: &[i64],
) -> Result<Vec<folder::Model>> {
    match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            folder_repo::find_by_ids_in_personal_scope(&state.db, user_id, ids).await
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            folder_repo::find_by_ids_in_team_scope(&state.db, team_id, ids).await
        }
    }
}

pub(crate) struct NormalizedSelection {
    pub file_ids: Vec<i64>,
    pub folder_ids: Vec<i64>,
    pub file_map: HashMap<i64, file::Model>,
    pub folder_map: HashMap<i64, folder::Model>,
}

async fn load_folder_hierarchy_map(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_map: &HashMap<i64, file::Model>,
    folder_map: &HashMap<i64, folder::Model>,
) -> Result<HashMap<i64, Option<i64>>> {
    let mut hierarchy = HashMap::with_capacity(folder_map.len());
    for (id, folder) in folder_map {
        hierarchy.insert(*id, folder.parent_id);
    }
    let mut frontier: HashSet<i64> = folder_map
        .values()
        .filter_map(|folder| folder.parent_id)
        .chain(file_map.values().filter_map(|file| file.folder_id))
        .filter(|folder_id| !hierarchy.contains_key(folder_id))
        .collect();

    while !frontier.is_empty() {
        let ids: Vec<i64> = frontier.drain().collect();
        let rows = find_folders_by_ids_in_scope(state, scope, &ids).await?;
        for row in rows {
            let parent_id = row.parent_id;
            let id = row.id;
            if hierarchy.insert(id, parent_id).is_none()
                && let Some(parent_id) = parent_id
                && !hierarchy.contains_key(&parent_id)
            {
                frontier.insert(parent_id);
            }
        }
    }

    Ok(hierarchy)
}

fn has_selected_ancestor(
    start_folder_id: Option<i64>,
    selected_folder_ids: &HashSet<i64>,
    hierarchy: &HashMap<i64, Option<i64>>,
) -> bool {
    let mut current = start_folder_id;
    while let Some(folder_id) = current {
        if selected_folder_ids.contains(&folder_id) {
            return true;
        }
        current = hierarchy.get(&folder_id).copied().flatten();
    }
    false
}

fn normalize_selection(
    file_ids: &[i64],
    folder_ids: &[i64],
    file_map: &HashMap<i64, file::Model>,
    folder_map: &HashMap<i64, folder::Model>,
    hierarchy: &HashMap<i64, Option<i64>>,
) -> (Vec<i64>, Vec<i64>) {
    let selected_folder_ids: HashSet<i64> = folder_ids.iter().copied().collect();

    let normalized_folder_ids = folder_ids
        .iter()
        .copied()
        .filter(|folder_id| {
            let Some(folder) = folder_map.get(folder_id) else {
                return true;
            };
            !has_selected_ancestor(folder.parent_id, &selected_folder_ids, hierarchy)
        })
        .collect();

    let normalized_file_ids = file_ids
        .iter()
        .copied()
        .filter(|file_id| {
            let Some(file) = file_map.get(file_id) else {
                return true;
            };
            !has_selected_ancestor(file.folder_id, &selected_folder_ids, hierarchy)
        })
        .collect();

    (normalized_file_ids, normalized_folder_ids)
}

pub(crate) async fn load_normalized_selection_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_ids: &[i64],
    folder_ids: &[i64],
) -> Result<NormalizedSelection> {
    workspace_storage_service::require_scope_access(state, scope).await?;
    validate_batch_ids(file_ids, folder_ids)?;

    let file_map = build_file_map(find_files_by_ids_in_scope(state, scope, file_ids).await?);
    let folder_map =
        build_folder_map(find_folders_by_ids_in_scope(state, scope, folder_ids).await?);
    let hierarchy = load_folder_hierarchy_map(state, scope, &file_map, &folder_map).await?;
    let (normalized_file_ids, normalized_folder_ids) =
        normalize_selection(file_ids, folder_ids, &file_map, &folder_map, &hierarchy);

    Ok(NormalizedSelection {
        file_ids: normalized_file_ids,
        folder_ids: normalized_folder_ids,
        file_map,
        folder_map,
    })
}

pub(crate) fn reserve_unique_name(
    reserved_names: &mut HashSet<String>,
    original_name: &str,
) -> String {
    let mut candidate = original_name.to_string();
    while !reserved_names.insert(candidate.clone()) {
        candidate = crate::utils::next_copy_name(&candidate);
    }
    candidate
}

pub(super) async fn load_target_folder_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    target_folder_id: Option<i64>,
) -> std::result::Result<Option<folder::Model>, String> {
    let Some(folder_id) = target_folder_id else {
        return Ok(None);
    };

    workspace_storage_service::verify_folder_access(state, scope, folder_id)
        .await
        .map(Some)
        .map_err(display_error)
}

pub(super) async fn load_folder_ancestor_ids_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    target_folder: Option<&folder::Model>,
) -> Result<HashSet<i64>> {
    let mut ancestors = HashSet::new();
    let mut current = target_folder.cloned();

    while let Some(folder) = current {
        workspace_storage_service::ensure_active_folder_scope(&folder, scope)?;
        ancestors.insert(folder.id);
        current = match folder.parent_id {
            Some(parent_id) => Some(folder_repo::find_by_id(&state.db, parent_id).await?),
            None => None,
        };
    }

    Ok(ancestors)
}
