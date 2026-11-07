//! 文件夹服务子模块：`hierarchy`。

use std::collections::{HashMap, HashSet};

use sea_orm::ConnectionTrait;

use crate::db::repository::folder_repo;
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::services::workspace::storage::{self, WorkspaceStorageScope};
use aster_drive_model::entities::folder;
use aster_drive_model::types::folder::MAX_FOLDER_DEPTH;

use super::{FolderAncestorItem, cache, ensure_folder_model_in_scope};

struct FolderPathEntry {
    path: String,
    chain_ids: Vec<i64>,
}

/// Load the complete active chain, including the requested folder, root first.
pub(crate) async fn load_active_chain_on<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    folder_id: Option<i64>,
) -> Result<Vec<folder::Model>> {
    let Some(folder_id) = folder_id else {
        return Ok(Vec::new());
    };
    let chain = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            folder_repo::find_ancestor_models(db, user_id, folder_id).await?
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            folder_repo::find_team_ancestor_models(db, team_id, folder_id).await?
        }
    };
    if chain.last().map(|folder| folder.id) != Some(folder_id) {
        return Err(AsterError::folder_not_found(
            "folder ancestor chain is unavailable",
        ));
    }
    for folder in &chain {
        ensure_folder_model_in_scope(folder, scope)?;
    }
    Ok(chain)
}

pub(crate) fn ensure_result_depth(parent_depth: usize, folder_levels: usize) -> Result<()> {
    if parent_depth
        .checked_add(folder_levels)
        .is_none_or(|depth| depth > MAX_FOLDER_DEPTH)
    {
        return Err(AsterError::validation_error(
            "folder hierarchy exceeds the 4096-level limit",
        ));
    }
    Ok(())
}

/// Call inside the mutation's writer transaction after acquiring its workspace guard.
pub(crate) async fn ensure_insertion_depth_on<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    parent_id: Option<i64>,
    folder_levels: usize,
) -> Result<()> {
    let chain = load_active_chain_on(db, scope, parent_id).await?;
    ensure_result_depth(chain.len(), folder_levels)
}

pub(crate) async fn ensure_subtree_placement_on<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    root_id: i64,
    parent_depth: usize,
) -> Result<()> {
    let folder_scope = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            folder_repo::FolderScope::Personal { user_id }
        }
        WorkspaceStorageScope::Team { team_id, .. } => folder_repo::FolderScope::Team { team_id },
    };
    let height = folder_repo::find_subtree_height(db, folder_scope, root_id, true).await?;
    ensure_result_depth(parent_depth, height)
}

pub(super) async fn load_folder_chain_map<C: ConnectionTrait>(
    db: &C,
    folder_ids: &[i64],
) -> Result<HashMap<i64, folder::Model>> {
    let mut loaded = HashMap::new();
    // Most path reads contain one or two parents. A bounded recursive projection
    // avoids thousands of database round trips for a supported deep hierarchy;
    // wide activity batches retain the shared frontier loading below.
    if !folder_ids.is_empty() && folder_ids.len() <= 4 {
        let seeds = folder_repo::find_by_ids(db, folder_ids).await?;
        for &id in folder_ids {
            let seed = seeds
                .iter()
                .find(|folder| folder.id == id)
                .ok_or_else(|| AsterError::folder_not_found(format!("folder #{id}")))?;
            let chain = match (seed.team_id, seed.owner_user_id) {
                (Some(team_id), _) => {
                    folder_repo::find_team_ancestor_models(db, team_id, id).await?
                }
                (None, Some(user_id)) => folder_repo::find_ancestor_models(db, user_id, id).await?,
                (None, None) => {
                    return Err(AsterError::validation_error("folder has no workspace"));
                }
            };
            loaded.extend(chain.into_iter().map(|folder| (folder.id, folder)));
        }
        return Ok(loaded);
    }
    let mut frontier: Vec<i64> = folder_ids.to_vec();
    let mut depth = 0;

    while !frontier.is_empty() {
        frontier.retain(|id| !loaded.contains_key(id));
        frontier.sort_unstable();
        frontier.dedup();
        if frontier.is_empty() {
            break;
        }
        ensure_result_depth(depth, 1)?;
        depth += 1;

        let rows = folder_repo::find_by_ids(db, &frontier).await?;
        let mut found = HashSet::with_capacity(rows.len());
        let mut next = Vec::new();

        for row in rows {
            found.insert(row.id);
            if let Some(pid) = row.parent_id
                && !loaded.contains_key(&pid)
            {
                next.push(pid);
            }
            loaded.insert(row.id, row);
        }

        if let Some(missing) = frontier.iter().find(|id| !found.contains(id)) {
            return Err(AsterError::record_not_found(format!("folder #{missing}")));
        }

        frontier = next;
    }

    Ok(loaded)
}

pub async fn build_folder_paths<C: ConnectionTrait>(
    db: &C,
    folder_ids: &[i64],
) -> Result<HashMap<i64, String>> {
    let entries = build_folder_path_entries(db, folder_ids).await?;
    Ok(entries
        .into_iter()
        .map(|(folder_id, entry)| (folder_id, entry.path))
        .collect())
}

async fn build_folder_path_entries<C: ConnectionTrait>(
    db: &C,
    folder_ids: &[i64],
) -> Result<HashMap<i64, FolderPathEntry>> {
    let chain_map = load_folder_chain_map(db, folder_ids).await?;
    build_folder_path_entries_from_chain_map(folder_ids, &chain_map)
}

fn build_folder_path_entries_from_chain_map(
    folder_ids: &[i64],
    chain_map: &HashMap<i64, folder::Model>,
) -> Result<HashMap<i64, FolderPathEntry>> {
    let mut paths = HashMap::with_capacity(folder_ids.len());

    for &folder_id in folder_ids {
        let mut parts = Vec::new();
        let mut chain_ids = Vec::new();
        let mut current_id = Some(folder_id);
        let mut seen = HashSet::new();
        while let Some(id) = current_id {
            if chain_ids.len() >= MAX_FOLDER_DEPTH || !seen.insert(id) {
                return Err(AsterError::validation_error(
                    "folder hierarchy exceeds the 4096-level limit or contains a cycle",
                ));
            }
            let folder = chain_map
                .get(&id)
                .ok_or_else(|| AsterError::record_not_found(format!("folder #{id}")))?;
            chain_ids.push(id);
            parts.push(folder.name.clone());
            current_id = folder.parent_id;
        }
        parts.reverse();
        paths.insert(
            folder_id,
            FolderPathEntry {
                path: format!("/{}", parts.join("/")),
                chain_ids,
            },
        );
    }

    Ok(paths)
}

pub async fn build_folder_paths_cached(
    state: &impl SharedRuntimeState,
    folder_ids: &[i64],
) -> Result<HashMap<i64, String>> {
    let mut ids = folder_ids.to_vec();
    ids.sort_unstable();
    ids.dedup();

    let mut paths = HashMap::with_capacity(ids.len());
    let mut cached_entries = HashMap::new();
    let mut misses = Vec::new();

    for &id in &ids {
        if let Some(cached) = cache::load_folder_path_chain(state, id).await {
            if cached.chain_ids.is_empty() {
                misses.push(id);
            } else {
                cached_entries.insert(id, cached);
            }
        } else {
            misses.push(id);
        }
    }

    if !cached_entries.is_empty() {
        let mut chain_ids: Vec<i64> = cached_entries
            .values()
            .flat_map(|entry| entry.chain_ids.iter().copied())
            .collect();
        chain_ids.sort_unstable();
        chain_ids.dedup();
        let chain_map = folder_repo::find_by_ids(state.reader_db(), &chain_ids)
            .await?
            .into_iter()
            .map(|folder| (folder.id, folder))
            .collect::<HashMap<_, _>>();

        for (&id, cached) in &cached_entries {
            match build_folder_path_entries_from_chain_map(&[id], &chain_map).and_then(
                |mut rebuilt| {
                    rebuilt
                        .remove(&id)
                        .ok_or_else(|| AsterError::record_not_found(format!("folder #{id} path")))
                },
            ) {
                Ok(entry) => {
                    if entry.chain_ids != cached.chain_ids {
                        cache::store_folder_path_chain(state, id, entry.chain_ids).await;
                    }
                    paths.insert(id, entry.path);
                }
                Err(_) => {
                    cache::invalidate_folder_path_chain(state, id).await;
                    misses.push(id);
                }
            }
        }
    }

    if misses.is_empty() {
        return Ok(paths);
    }

    misses.sort_unstable();
    misses.dedup();
    let loaded = build_folder_path_entries(state.reader_db(), &misses).await?;
    for (&id, entry) in &loaded {
        cache::store_folder_path_chain(state, id, entry.chain_ids.clone()).await;
    }
    paths.extend(
        loaded
            .into_iter()
            .map(|(folder_id, entry)| (folder_id, entry.path)),
    );
    Ok(paths)
}

pub(crate) async fn get_ancestors_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    folder_id: i64,
) -> Result<Vec<FolderAncestorItem>> {
    let folder = storage::verify_folder_access_for_read(state, scope, folder_id).await?;
    ensure_folder_model_in_scope(&folder, scope)?;

    let ancestors = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            folder_repo::find_ancestor_models(state.reader_db(), user_id, folder_id).await?
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            folder_repo::find_team_ancestor_models(state.reader_db(), team_id, folder_id).await?
        }
    };

    for ancestor in &ancestors {
        ensure_folder_model_in_scope(ancestor, scope)?;
    }
    Ok(ancestors
        .into_iter()
        .map(|folder| FolderAncestorItem {
            id: folder.id,
            name: folder.name.clone(),
            icon: super::FolderIcon::from_model(&folder),
        })
        .collect())
}

/// 获取文件夹的祖先链（从根下第一层到当前文件夹）
pub async fn get_ancestors(
    state: &impl SharedRuntimeState,
    user_id: i64,
    folder_id: i64,
) -> Result<Vec<FolderAncestorItem>> {
    get_ancestors_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        folder_id,
    )
    .await
}
