//! 批量操作服务子模块：`movement`。

use std::collections::{BTreeSet, HashMap, HashSet};

use chrono::Utc;
use sea_orm::ConnectionTrait;

use crate::db::repository::{file_repo, folder_repo};
use crate::entities::folder;
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{
    events::storage_change,
    workspace::storage::{self, WorkspaceStorageScope},
};

use super::shared::{load_folder_ancestor_ids_in_scope, load_target_folder_in_scope};
use super::{BatchResult, NormalizedSelection, load_normalized_selection_in_scope};

async fn load_target_file_name_map(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    target_folder_id: Option<i64>,
    names: &[String],
) -> Result<HashMap<String, i64>> {
    let files = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            file_repo::find_by_names_in_folder(state.reader_db(), user_id, target_folder_id, names)
                .await?
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            file_repo::find_by_names_in_team_folder(
                state.reader_db(),
                team_id,
                target_folder_id,
                names,
            )
            .await?
        }
    };
    Ok(files
        .into_iter()
        .map(|file| (crate::utils::normalize_name(&file.name), file.id))
        .collect())
}

async fn load_target_folder_name_map(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    target_folder_id: Option<i64>,
    names: &[String],
) -> Result<HashMap<String, i64>> {
    let folders = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            folder_repo::find_by_names_in_parent(
                state.reader_db(),
                user_id,
                target_folder_id,
                names,
            )
            .await?
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            folder_repo::find_by_names_in_team_parent(
                state.reader_db(),
                team_id,
                target_folder_id,
                names,
            )
            .await?
        }
    };
    Ok(folders
        .into_iter()
        .map(|folder| (crate::utils::normalize_name(&folder.name), folder.id))
        .collect())
}

pub(crate) async fn batch_move_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_ids: &[i64],
    folder_ids: &[i64],
    target_folder_id: Option<i64>,
) -> Result<BatchResult> {
    let mut result = BatchResult::new();
    let NormalizedSelection {
        file_ids: normalized_file_ids,
        folder_ids: normalized_folder_ids,
        file_map,
        folder_map,
    } = load_normalized_selection_in_scope(state, scope, file_ids, folder_ids).await?;

    let (target_folder, target_error) =
        match load_target_folder_in_scope(state, scope, target_folder_id).await {
            Ok(folder) => (folder, None),
            Err(error) => (None, Some(error)),
        };

    let mut target_file_names = HashMap::new();
    let mut target_folder_names = HashMap::new();
    let mut target_ancestor_ids = HashSet::new();
    if target_error.is_none() {
        let mut seen_file_lookup_names = HashSet::new();
        let target_file_lookup_names: Vec<String> = normalized_file_ids
            .iter()
            .flat_map(|id| file_map.get(id))
            .map(|file| file.name.clone())
            .filter(|name| seen_file_lookup_names.insert(name.clone()))
            .collect();
        let mut seen_folder_lookup_names = HashSet::new();
        let target_folder_lookup_names: Vec<String> = normalized_folder_ids
            .iter()
            .flat_map(|id| folder_map.get(id))
            .map(|folder| folder.name.clone())
            .filter(|name| seen_folder_lookup_names.insert(name.clone()))
            .collect();
        target_file_names =
            load_target_file_name_map(state, scope, target_folder_id, &target_file_lookup_names)
                .await?;
        target_folder_names = load_target_folder_name_map(
            state,
            scope,
            target_folder_id,
            &target_folder_lookup_names,
        )
        .await?;
        target_ancestor_ids =
            load_folder_ancestor_ids_in_scope(state, scope, target_folder.as_ref()).await?;
    }

    let mut file_ids_to_move = HashSet::new();
    let mut folder_ids_to_move = HashSet::new();

    for &id in &normalized_file_ids {
        let Some(file) = file_map.get(&id) else {
            result.record_failure(
                "file",
                id,
                AsterError::file_not_found(format!("file #{id}")).to_string(),
            );
            continue;
        };
        if let Err(err) = storage::ensure_active_file_scope(file, scope) {
            result.record_failure("file", id, err.to_string());
            continue;
        }
        if file.is_locked {
            result.record_failure(
                "file",
                id,
                AsterError::resource_locked("file is locked").to_string(),
            );
            continue;
        }
        if let Some(error) = target_error.as_ref() {
            result.record_failure("file", id, error.clone());
            continue;
        }
        let normalized_name = crate::utils::normalize_name(&file.name);
        if matches!(target_file_names.get(&normalized_name), Some(existing_id) if *existing_id != file.id)
        {
            result.record_failure(
                "file",
                id,
                AsterError::validation_error(format!(
                    "file '{}' already exists in target folder",
                    file.name
                ))
                .to_string(),
            );
            continue;
        }

        result.record_success();
        if file.folder_id != target_folder_id {
            file_ids_to_move.insert(file.id);
        }
        target_file_names.insert(normalized_name, file.id);
    }

    for &id in &normalized_folder_ids {
        let Some(folder) = folder_map.get(&id) else {
            result.record_failure(
                "folder",
                id,
                AsterError::record_not_found(format!("folder #{id}")).to_string(),
            );
            continue;
        };
        if let Err(err) = storage::ensure_active_folder_scope(folder, scope) {
            result.record_failure("folder", id, err.to_string());
            continue;
        }
        if folder.is_locked {
            result.record_failure(
                "folder",
                id,
                AsterError::resource_locked("folder is locked").to_string(),
            );
            continue;
        }
        if target_folder_id == Some(folder.id) {
            result.record_failure(
                "folder",
                id,
                AsterError::validation_error("cannot move folder into itself").to_string(),
            );
            continue;
        }
        if let Some(error) = target_error.as_ref() {
            result.record_failure("folder", id, error.clone());
            continue;
        }
        if target_ancestor_ids.contains(&folder.id) {
            result.record_failure(
                "folder",
                id,
                AsterError::validation_error("cannot move folder into its own subfolder")
                    .to_string(),
            );
            continue;
        }
        let normalized_name = crate::utils::normalize_name(&folder.name);
        if matches!(target_folder_names.get(&normalized_name), Some(existing_id) if *existing_id != folder.id)
        {
            result.record_failure(
                "folder",
                id,
                AsterError::validation_error(format!(
                    "folder '{}' already exists in target folder",
                    folder.name
                ))
                .to_string(),
            );
            continue;
        }

        result.record_success();
        if folder.parent_id != target_folder_id {
            folder_ids_to_move.insert(folder.id);
        }
        target_folder_names.insert(normalized_name, folder.id);
    }

    if !file_ids_to_move.is_empty() || !folder_ids_to_move.is_empty() {
        let now = Utc::now();
        let file_ids_to_move: Vec<i64> = file_ids_to_move.into_iter().collect();
        let folder_ids_to_move: Vec<i64> = folder_ids_to_move.into_iter().collect();
        let file_parent_ids: Vec<Option<i64>> = file_ids_to_move
            .iter()
            .flat_map(|id| file_map.get(id).into_iter())
            .flat_map(|file| [file.folder_id, target_folder_id])
            .collect();
        let folder_parent_ids: Vec<Option<i64>> = folder_ids_to_move
            .iter()
            .flat_map(|id| folder_map.get(id).into_iter())
            .flat_map(|folder| [folder.parent_id, target_folder_id])
            .collect();

        let txn = crate::db::transaction::begin(state.writer_db()).await?;
        revalidate_batch_folder_move(&txn, scope, &folder_ids_to_move, target_folder_id).await?;
        file_repo::move_many_to_folder(&txn, &file_ids_to_move, target_folder_id, now).await?;
        folder_repo::move_many_to_parent(&txn, &folder_ids_to_move, target_folder_id, now).await?;
        crate::db::transaction::commit(txn).await?;

        if !file_ids_to_move.is_empty() {
            storage_change::publish(
                state,
                storage_change::StorageChangeEvent::new(
                    storage_change::StorageChangeKind::FileUpdated,
                    scope,
                    file_ids_to_move,
                    vec![],
                    file_parent_ids,
                ),
            );
        }
        if !folder_ids_to_move.is_empty() {
            storage_change::publish(
                state,
                storage_change::StorageChangeEvent::new(
                    storage_change::StorageChangeKind::FolderUpdated,
                    scope,
                    vec![],
                    folder_ids_to_move,
                    folder_parent_ids,
                ),
            );
        }
    }

    Ok(result)
}

async fn revalidate_batch_folder_move<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    folder_ids_to_move: &[i64],
    target_folder_id: Option<i64>,
) -> Result<()> {
    let initial_target_chain = load_folder_chain_in_scope(db, scope, target_folder_id).await?;
    let mut lock_ids: Vec<i64> = folder_ids_to_move.to_vec();
    lock_ids.extend(initial_target_chain.iter().map(|folder| folder.id));
    lock_folder_ids_in_order(db, &lock_ids).await?;

    let target_chain = load_folder_chain_in_scope(db, scope, target_folder_id).await?;
    for &folder_id in folder_ids_to_move {
        let folder = folder_repo::lock_by_id(db, folder_id).await?;
        storage::ensure_active_folder_scope(&folder, scope)?;
        if folder.is_locked {
            return Err(AsterError::resource_locked("folder is locked"));
        }
        if target_chain.iter().any(|ancestor| ancestor.id == folder_id) {
            return Err(AsterError::validation_error(
                "cannot move folder into its own subfolder",
            ));
        }
    }

    Ok(())
}

async fn load_folder_chain_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    start_id: Option<i64>,
) -> Result<Vec<folder::Model>> {
    let mut chain = Vec::new();
    let mut seen = BTreeSet::new();
    let mut cursor = start_id;
    while let Some(folder_id) = cursor {
        if !seen.insert(folder_id) {
            return Err(AsterError::validation_error(
                "folder hierarchy contains a cycle",
            ));
        }
        let folder = folder_repo::find_by_id(db, folder_id).await?;
        storage::ensure_active_folder_scope(&folder, scope)?;
        cursor = folder.parent_id;
        chain.push(folder);
    }
    Ok(chain)
}

async fn lock_folder_ids_in_order<C: ConnectionTrait>(db: &C, ids: &[i64]) -> Result<()> {
    let mut ids: Vec<i64> = ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    for id in ids {
        folder_repo::lock_by_id(db, id).await?;
    }
    Ok(())
}

/// 批量移动（target_folder_id = None 表示移到根目录）
pub async fn batch_move(
    state: &PrimaryAppState,
    user_id: i64,
    file_ids: &[i64],
    folder_ids: &[i64],
    target_folder_id: Option<i64>,
) -> Result<BatchResult> {
    batch_move_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        file_ids,
        folder_ids,
        target_folder_id,
    )
    .await
}

/// 团队空间批量移动（target_folder_id = None 表示移到团队根目录）
pub async fn batch_move_team(
    state: &PrimaryAppState,
    team_id: i64,
    user_id: i64,
    file_ids: &[i64],
    folder_ids: &[i64],
    target_folder_id: Option<i64>,
) -> Result<BatchResult> {
    batch_move_in_scope(
        state,
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id: user_id,
        },
        file_ids,
        folder_ids,
        target_folder_id,
    )
    .await
}
