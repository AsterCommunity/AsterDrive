//! 文件夹服务子模块：`hierarchy`。

use std::collections::{HashMap, HashSet};

use sea_orm::ConnectionTrait;

use crate::db::repository::folder_repo;
use crate::entities::folder;
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::services::workspace_storage_service::{self, WorkspaceStorageScope};

use super::{FolderAncestorItem, cache, ensure_folder_model_in_scope};

struct FolderPathEntry {
    path: String,
    chain_ids: Vec<i64>,
}

pub(crate) async fn invalidate_folder_path_cache(state: &impl SharedRuntimeState) {
    cache::invalidate_all_folder_path_chains(state).await;
}

pub(crate) async fn invalidate_folder_path_cache_for_ids(
    state: &impl SharedRuntimeState,
    folder_ids: &[i64],
) {
    cache::invalidate_folder_path_chains(state, folder_ids).await;
}

pub(super) async fn load_folder_chain_map<C: ConnectionTrait>(
    db: &C,
    folder_ids: &[i64],
) -> Result<HashMap<i64, folder::Model>> {
    let mut loaded = HashMap::new();
    let mut frontier: Vec<i64> = folder_ids.to_vec();

    while !frontier.is_empty() {
        frontier.retain(|id| !loaded.contains_key(id));
        frontier.sort_unstable();
        frontier.dedup();
        if frontier.is_empty() {
            break;
        }

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
        while let Some(id) = current_id {
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
    let folder =
        workspace_storage_service::verify_folder_access_for_read(state, scope, folder_id).await?;
    ensure_folder_model_in_scope(&folder, scope)?;

    let ancestors = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            folder_repo::find_ancestor_models(state.reader_db(), user_id, folder_id).await?
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            folder_repo::find_team_ancestor_models(state.reader_db(), team_id, folder_id).await?
        }
    };

    Ok(ancestors
        .into_iter()
        .map(|folder| FolderAncestorItem {
            id: folder.id,
            name: folder.name,
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
