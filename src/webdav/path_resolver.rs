//! WebDAV 子模块：`path_resolver`。

mod cache;

use sea_orm::{ConnectionTrait, DatabaseConnection};
use serde::{Deserialize, Serialize};

use crate::db::repository::{file_repo, folder_repo};
use crate::entities::{file, folder};
use crate::errors::AsterError;
use crate::runtime::SharedRuntimeState;
use crate::services::workspace_storage_service::WorkspaceStorageScope;
use crate::utils::hash;
use crate::webdav::dav::{DavPath, FsError};

pub(crate) use cache::{WEBDAV_PARENT_CACHE_PREFIX, WEBDAV_PATH_CACHE_PREFIX};

/// 路径解析结果
#[derive(Debug, Clone)]
pub enum ResolvedNode {
    /// 根目录 (parent_id = None)
    Root,
    /// 数据库中的文件夹
    Folder(folder::Model),
    /// 数据库中的文件
    File(file::Model),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum CachedResolvedNode {
    Root,
    Folder {
        id: i64,
        parent_id: Option<i64>,
        name: String,
    },
    File {
        id: i64,
        folder_id: Option<i64>,
        name: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedResolvedParent {
    parent_id: Option<i64>,
}

/// 从 DavPath 提取路径段（已解码）
fn path_segments(path: &DavPath) -> Vec<String> {
    // as_bytes() 返回不含前缀、已解码的原始字节
    let raw = path.as_bytes();
    let path_str = String::from_utf8_lossy(raw);
    path_str
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn root_cache_part(root_folder_id: Option<i64>) -> String {
    root_folder_id
        .map(|id| format!("root:{id}"))
        .unwrap_or_else(|| "root:none".to_string())
}

fn dav_path_digest(path: &DavPath) -> String {
    hash::sha256_hex(path.as_bytes())
}

fn scope_cache_part(scope: WorkspaceStorageScope) -> String {
    match scope {
        WorkspaceStorageScope::Personal { user_id } => format!("personal:{user_id}"),
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id,
        } => format!("team:{team_id}:actor:{actor_user_id}"),
    }
}

pub(super) fn resolve_path_cache_key(
    scope: WorkspaceStorageScope,
    path: &DavPath,
    root_folder_id: Option<i64>,
) -> String {
    format!(
        "{WEBDAV_PATH_CACHE_PREFIX}{}:{}:{}",
        scope_cache_part(scope),
        root_cache_part(root_folder_id),
        dav_path_digest(path)
    )
}

pub(super) fn resolve_parent_cache_key(
    scope: WorkspaceStorageScope,
    path: &DavPath,
    root_folder_id: Option<i64>,
) -> String {
    format!(
        "{WEBDAV_PARENT_CACHE_PREFIX}{}:{}:{}",
        scope_cache_part(scope),
        root_cache_part(root_folder_id),
        dav_path_digest(path)
    )
}

pub(crate) fn path_cache_personal_prefix(user_id: i64) -> String {
    format!("{WEBDAV_PATH_CACHE_PREFIX}personal:{user_id}:")
}

pub(crate) fn parent_cache_personal_prefix(user_id: i64) -> String {
    format!("{WEBDAV_PARENT_CACHE_PREFIX}personal:{user_id}:")
}

pub(crate) fn path_cache_team_prefix(team_id: i64) -> String {
    format!("{WEBDAV_PATH_CACHE_PREFIX}team:{team_id}:")
}

pub(crate) fn parent_cache_team_prefix(team_id: i64) -> String {
    format!("{WEBDAV_PARENT_CACHE_PREFIX}team:{team_id}:")
}

fn cacheable_node(node: &ResolvedNode) -> CachedResolvedNode {
    match node {
        ResolvedNode::Root => CachedResolvedNode::Root,
        ResolvedNode::Folder(folder) => CachedResolvedNode::Folder {
            id: folder.id,
            parent_id: folder.parent_id,
            name: folder.name.clone(),
        },
        ResolvedNode::File(file) => CachedResolvedNode::File {
            id: file.id,
            folder_id: file.folder_id,
            name: file.name.clone(),
        },
    }
}

fn is_missing_entity(error: &AsterError) -> bool {
    matches!(
        error,
        AsterError::RecordNotFound(_) | AsterError::FileNotFound(_) | AsterError::FolderNotFound(_)
    )
}

fn folder_chain_matches_segments(
    chain: &[folder::Model],
    root_folder_id: Option<i64>,
    segments: &[String],
) -> bool {
    if chain.is_empty() {
        return false;
    }

    let relative_start = match root_folder_id {
        Some(root_id) => {
            let Some(root_index) = chain.iter().position(|folder| folder.id == root_id) else {
                return false;
            };
            root_index.saturating_add(1)
        }
        None => {
            if chain
                .first()
                .is_none_or(|folder| folder.parent_id.is_some())
            {
                return false;
            }
            0
        }
    };

    let relative_chain = &chain[relative_start..];
    relative_chain.len() == segments.len()
        && relative_chain
            .iter()
            .zip(segments)
            .all(|(folder, segment)| folder.name == *segment)
}

async fn validate_cached_folder_path(
    db: &impl ConnectionTrait,
    scope: WorkspaceStorageScope,
    root_folder_id: Option<i64>,
    folder_id: i64,
    segments: &[String],
) -> Result<bool, FsError> {
    let chain = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            folder_repo::find_ancestor_models(db, user_id, folder_id).await
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            folder_repo::find_team_ancestor_models(db, team_id, folder_id).await
        }
    };
    let chain = match chain {
        Ok(chain) => chain,
        Err(error) if is_missing_entity(&error) => return Ok(false),
        Err(_) => return Err(FsError::GeneralFailure),
    };
    if chain.is_empty() || chain.iter().any(|folder| folder.deleted_at.is_some()) {
        return Ok(false);
    }
    for folder in &chain {
        if crate::services::workspace_storage_service::ensure_folder_scope(folder, scope).is_err() {
            return Ok(false);
        }
    }

    Ok(folder_chain_matches_segments(
        &chain,
        root_folder_id,
        segments,
    ))
}

async fn load_cached_resolved_node(
    state: &impl SharedRuntimeState,
    db: &impl ConnectionTrait,
    scope: WorkspaceStorageScope,
    path: &DavPath,
    root_folder_id: Option<i64>,
    cache_key: &str,
    cached: CachedResolvedNode,
) -> Result<Option<ResolvedNode>, FsError> {
    let segments = path_segments(path);
    match cached {
        CachedResolvedNode::Root => {
            if segments.is_empty() {
                Ok(Some(ResolvedNode::Root))
            } else {
                cache::delete_by_key(state, cache_key).await;
                Ok(None)
            }
        }
        CachedResolvedNode::Folder {
            id,
            parent_id,
            name,
        } => {
            let folder = match folder_repo::find_by_id(db, id).await {
                Ok(folder) => folder,
                Err(error) if is_missing_entity(&error) => {
                    cache::delete_by_key(state, cache_key).await;
                    return Ok(None);
                }
                Err(_) => return Err(FsError::GeneralFailure),
            };
            if folder.deleted_at.is_some()
                || crate::services::workspace_storage_service::ensure_folder_scope(&folder, scope)
                    .is_err()
            {
                cache::delete_by_key(state, cache_key).await;
                return Ok(None);
            }
            if segments.last().map(String::as_str) != Some(name.as_str())
                || folder.name != name
                || folder.parent_id != parent_id
            {
                cache::delete_by_key(state, cache_key).await;
                return Ok(None);
            }
            if !validate_cached_folder_path(db, scope, root_folder_id, id, &segments).await? {
                cache::delete_by_key(state, cache_key).await;
                return Ok(None);
            }
            Ok(Some(ResolvedNode::Folder(folder)))
        }
        CachedResolvedNode::File {
            id,
            folder_id,
            name,
        } => {
            let file = match file_repo::find_by_id(db, id).await {
                Ok(file) => file,
                Err(error) if is_missing_entity(&error) => {
                    cache::delete_by_key(state, cache_key).await;
                    return Ok(None);
                }
                Err(_) => return Err(FsError::GeneralFailure),
            };
            if file.deleted_at.is_some()
                || crate::services::workspace_storage_service::ensure_file_scope(&file, scope)
                    .is_err()
            {
                cache::delete_by_key(state, cache_key).await;
                return Ok(None);
            }
            if segments.last().map(String::as_str) != Some(name.as_str())
                || file.name != name
                || file.folder_id != folder_id
            {
                cache::delete_by_key(state, cache_key).await;
                return Ok(None);
            }
            let Some(parent_segments) = segments.get(..segments.len().saturating_sub(1)) else {
                cache::delete_by_key(state, cache_key).await;
                return Ok(None);
            };
            if !validate_cached_parent(db, scope, root_folder_id, parent_segments, file.folder_id)
                .await?
            {
                cache::delete_by_key(state, cache_key).await;
                return Ok(None);
            }
            Ok(Some(ResolvedNode::File(file)))
        }
    }
}

/// 解析 WebDAV 路径到数据库实体
///
/// 路径中的 folder 前缀通过单次递归查询解析，只有最后一个 file 候选需要额外查库。
pub async fn resolve_path(
    db: &DatabaseConnection,
    user_id: i64,
    path: &DavPath,
    root_folder_id: Option<i64>,
) -> Result<ResolvedNode, FsError> {
    resolve_path_in_scope(
        db,
        WorkspaceStorageScope::Personal { user_id },
        path,
        root_folder_id,
    )
    .await
}

pub(crate) async fn resolve_path_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    path: &DavPath,
    root_folder_id: Option<i64>,
) -> Result<ResolvedNode, FsError> {
    let segments = path_segments(path);

    if segments.is_empty() {
        return Ok(ResolvedNode::Root);
    }

    let folders = resolve_folder_chain(db, scope, root_folder_id, &segments).await?;

    if folders.len() == segments.len() {
        let Some(folder) = folders.last().cloned() else {
            return Err(FsError::NotFound);
        };
        return Ok(ResolvedNode::Folder(folder));
    }

    // Only the last segment may be a file; anything earlier must have resolved as a folder chain.
    if folders.len() + 1 < segments.len() {
        return Err(FsError::NotFound);
    }

    let current_parent = folders.last().map(|folder| folder.id).or(root_folder_id);
    let Some(last) = segments.last() else {
        return Err(FsError::NotFound);
    };

    let file = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            file_repo::find_by_name_in_folder(db, user_id, current_parent, last).await
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            file_repo::find_by_name_in_team_folder(db, team_id, current_parent, last).await
        }
    }
    .map_err(|_| FsError::GeneralFailure)?;

    if let Some(file) = file {
        return Ok(ResolvedNode::File(file));
    }

    Err(FsError::NotFound)
}

async fn resolve_folder_chain<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    root_parent_id: Option<i64>,
    segments: &[String],
) -> Result<Vec<folder::Model>, FsError> {
    match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            folder_repo::resolve_path_chain(db, user_id, root_parent_id, segments)
                .await
                .map_err(|_| FsError::GeneralFailure)
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            let mut resolved = Vec::with_capacity(segments.len());
            let mut current_parent = root_parent_id;
            for segment in segments {
                let Some(folder) =
                    folder_repo::find_by_name_in_team_parent(db, team_id, current_parent, segment)
                        .await
                        .map_err(|_| FsError::GeneralFailure)?
                else {
                    break;
                };
                current_parent = Some(folder.id);
                resolved.push(folder);
            }
            Ok(resolved)
        }
    }
}

pub async fn resolve_path_cached(
    state: &impl SharedRuntimeState,
    user_id: i64,
    path: &DavPath,
    root_folder_id: Option<i64>,
) -> Result<ResolvedNode, FsError> {
    resolve_path_cached_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        path,
        root_folder_id,
    )
    .await
}

pub(crate) async fn resolve_path_cached_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    path: &DavPath,
    root_folder_id: Option<i64>,
) -> Result<ResolvedNode, FsError> {
    // Authoritative path resolution for write-sensitive WebDAV operations.
    // PUT/MKCOL/COPY/MOVE/DELETE/LOCK preconditions must observe the writer DB
    // so a lagging reader cannot resurrect stale cache entries or miss a fresh
    // destination created earlier in the same client workflow.
    resolve_path_cached_in_scope_with_db(state, state.writer_db(), scope, path, root_folder_id)
        .await
}

pub(crate) async fn resolve_path_cached_for_read_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    path: &DavPath,
    root_folder_id: Option<i64>,
) -> Result<ResolvedNode, FsError> {
    // Metadata, listing, download, and property reads may tolerate normal
    // reader lag. Cached entries are still validated against the selected DB
    // handle before reuse.
    resolve_path_cached_in_scope_with_db(state, state.reader_db(), scope, path, root_folder_id)
        .await
}

async fn resolve_path_cached_in_scope_with_db<C: ConnectionTrait>(
    state: &impl SharedRuntimeState,
    db: &C,
    scope: WorkspaceStorageScope,
    path: &DavPath,
    root_folder_id: Option<i64>,
) -> Result<ResolvedNode, FsError> {
    let cache_key = resolve_path_cache_key(scope, path, root_folder_id);
    if let Some(cached) = cache::load_resolved_node_by_key(state, &cache_key).await
        && let Some(node) =
            load_cached_resolved_node(state, db, scope, path, root_folder_id, &cache_key, cached)
                .await?
    {
        tracing::debug!(?scope, root_folder_id, "webdav path cache hit");
        return Ok(node);
    }

    let node = resolve_path_in_scope(db, scope, path, root_folder_id).await?;
    cache::store_resolved_node_by_key(state, &cache_key, &cacheable_node(&node)).await;
    tracing::debug!(?scope, root_folder_id, "webdav path cache miss");
    Ok(node)
}

/// 解析路径的父目录，返回 (parent_folder_id, 末段文件名)
///
/// `/Documents/file.txt` → (Some(docs_id), "file.txt")
/// `/file.txt` → (None, "file.txt")
pub async fn resolve_parent(
    db: &DatabaseConnection,
    user_id: i64,
    path: &DavPath,
    root_folder_id: Option<i64>,
) -> Result<(Option<i64>, String), FsError> {
    resolve_parent_in_scope(
        db,
        WorkspaceStorageScope::Personal { user_id },
        path,
        root_folder_id,
    )
    .await
}

pub(crate) async fn resolve_parent_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    path: &DavPath,
    root_folder_id: Option<i64>,
) -> Result<(Option<i64>, String), FsError> {
    let segments = path_segments(path);

    if segments.is_empty() {
        return Err(FsError::Forbidden); // 不能操作根目录本身
    }

    let parent_segments = &segments[..segments.len() - 1];
    let folders = resolve_folder_chain(db, scope, root_folder_id, parent_segments).await?;

    if folders.len() != parent_segments.len() {
        return Err(FsError::NotFound);
    }

    let current_parent = folders.last().map(|folder| folder.id).or(root_folder_id);
    let last = segments[segments.len() - 1].clone();
    Ok((current_parent, last))
}

async fn validate_cached_parent(
    db: &impl ConnectionTrait,
    scope: WorkspaceStorageScope,
    root_folder_id: Option<i64>,
    parent_segments: &[String],
    parent_id: Option<i64>,
) -> Result<bool, FsError> {
    match parent_id {
        Some(parent_id) => {
            validate_cached_folder_path(db, scope, root_folder_id, parent_id, parent_segments).await
        }
        None => Ok(root_folder_id.is_none() && parent_segments.is_empty()),
    }
}

pub async fn resolve_parent_cached(
    state: &impl SharedRuntimeState,
    user_id: i64,
    path: &DavPath,
    root_folder_id: Option<i64>,
) -> Result<(Option<i64>, String), FsError> {
    resolve_parent_cached_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        path,
        root_folder_id,
    )
    .await
}

pub(crate) async fn resolve_parent_cached_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    path: &DavPath,
    root_folder_id: Option<i64>,
) -> Result<(Option<i64>, String), FsError> {
    let segments = path_segments(path);
    if segments.is_empty() {
        return Err(FsError::Forbidden);
    }

    let parent_segments = &segments[..segments.len() - 1];
    let cache_key = resolve_parent_cache_key(scope, path, root_folder_id);
    if let Some(cached) = cache::load_resolved_parent_by_key(state, &cache_key).await {
        if validate_cached_parent(
            state.writer_db(),
            scope,
            root_folder_id,
            parent_segments,
            cached.parent_id,
        )
        .await?
        {
            tracing::debug!(?scope, root_folder_id, "webdav parent path cache hit");
            return Ok((cached.parent_id, segments[segments.len() - 1].clone()));
        }
        cache::delete_by_key(state, &cache_key).await;
    }

    let (parent_id, name) =
        resolve_parent_in_scope(state.writer_db(), scope, path, root_folder_id).await?;
    cache::store_resolved_parent_by_key(state, &cache_key, parent_id).await;
    tracing::debug!(?scope, root_folder_id, "webdav parent path cache miss");
    Ok((parent_id, name))
}
