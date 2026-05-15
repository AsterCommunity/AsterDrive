//! WebDAV 子模块：`path_resolver`。

use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use crate::cache::CacheExt;
use crate::db::repository::{file_repo, folder_repo};
use crate::entities::{file, folder};
use crate::errors::AsterError;
use crate::runtime::PrimaryAppState;
use crate::utils::hash;
use crate::webdav::dav::{DavPath, FsError};

const WEBDAV_PATH_CACHE_TTL: u64 = 30;
pub(crate) const WEBDAV_PATH_CACHE_PREFIX: &str = "webdav_path:";
pub(crate) const WEBDAV_PARENT_CACHE_PREFIX: &str = "webdav_parent:";

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

fn resolve_path_cache_key(user_id: i64, path: &DavPath, root_folder_id: Option<i64>) -> String {
    format!(
        "{WEBDAV_PATH_CACHE_PREFIX}{user_id}:{}:{}",
        root_cache_part(root_folder_id),
        dav_path_digest(path)
    )
}

fn resolve_parent_cache_key(user_id: i64, path: &DavPath, root_folder_id: Option<i64>) -> String {
    format!(
        "{WEBDAV_PARENT_CACHE_PREFIX}{user_id}:{}:{}",
        root_cache_part(root_folder_id),
        dav_path_digest(path)
    )
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
    state: &PrimaryAppState,
    user_id: i64,
    root_folder_id: Option<i64>,
    folder_id: i64,
    segments: &[String],
) -> Result<bool, FsError> {
    let chain = match folder_repo::find_ancestor_models(&state.db, user_id, folder_id).await {
        Ok(chain) => chain,
        Err(error) if is_missing_entity(&error) => return Ok(false),
        Err(_) => return Err(FsError::GeneralFailure),
    };
    if chain.is_empty()
        || chain.iter().any(|folder| {
            folder.owner_user_id != Some(user_id)
                || folder.team_id.is_some()
                || folder.deleted_at.is_some()
        })
    {
        return Ok(false);
    }

    Ok(folder_chain_matches_segments(
        &chain,
        root_folder_id,
        segments,
    ))
}

async fn load_cached_resolved_node(
    state: &PrimaryAppState,
    user_id: i64,
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
                state.cache.delete(cache_key).await;
                Ok(None)
            }
        }
        CachedResolvedNode::Folder {
            id,
            parent_id,
            name,
        } => {
            let folder = match folder_repo::find_by_id(&state.db, id).await {
                Ok(folder) => folder,
                Err(error) if is_missing_entity(&error) => {
                    state.cache.delete(cache_key).await;
                    return Ok(None);
                }
                Err(_) => return Err(FsError::GeneralFailure),
            };
            if folder.owner_user_id != Some(user_id)
                || folder.team_id.is_some()
                || folder.deleted_at.is_some()
            {
                state.cache.delete(cache_key).await;
                return Ok(None);
            }
            if segments.last().map(String::as_str) != Some(name.as_str())
                || folder.name != name
                || folder.parent_id != parent_id
            {
                state.cache.delete(cache_key).await;
                return Ok(None);
            }
            if !validate_cached_folder_path(state, user_id, root_folder_id, id, &segments).await? {
                state.cache.delete(cache_key).await;
                return Ok(None);
            }
            Ok(Some(ResolvedNode::Folder(folder)))
        }
        CachedResolvedNode::File {
            id,
            folder_id,
            name,
        } => {
            let file = match file_repo::find_by_id(&state.db, id).await {
                Ok(file) => file,
                Err(error) if is_missing_entity(&error) => {
                    state.cache.delete(cache_key).await;
                    return Ok(None);
                }
                Err(_) => return Err(FsError::GeneralFailure),
            };
            if file.owner_user_id != Some(user_id)
                || file.team_id.is_some()
                || file.deleted_at.is_some()
            {
                state.cache.delete(cache_key).await;
                return Ok(None);
            }
            if segments.last().map(String::as_str) != Some(name.as_str())
                || file.name != name
                || file.folder_id != folder_id
            {
                state.cache.delete(cache_key).await;
                return Ok(None);
            }
            let Some(parent_segments) = segments.get(..segments.len().saturating_sub(1)) else {
                state.cache.delete(cache_key).await;
                return Ok(None);
            };
            if !validate_cached_parent(
                state,
                user_id,
                root_folder_id,
                parent_segments,
                file.folder_id,
            )
            .await?
            {
                state.cache.delete(cache_key).await;
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
    let segments = path_segments(path);

    if segments.is_empty() {
        return Ok(ResolvedNode::Root);
    }

    let folders = folder_repo::resolve_path_chain(db, user_id, root_folder_id, &segments)
        .await
        .map_err(|_| FsError::GeneralFailure)?;

    if folders.len() == segments.len() {
        return Ok(ResolvedNode::Folder(
            folders
                .last()
                .cloned()
                .expect("non-empty path must have a last segment"),
        ));
    }

    // Only the last segment may be a file; anything earlier must have resolved as a folder chain.
    if folders.len() + 1 < segments.len() {
        return Err(FsError::NotFound);
    }

    let current_parent = folders.last().map(|folder| folder.id).or(root_folder_id);
    let last = segments
        .last()
        .expect("non-empty path must have a last segment");

    if let Some(file) = file_repo::find_by_name_in_folder(db, user_id, current_parent, last)
        .await
        .map_err(|_| FsError::GeneralFailure)?
    {
        return Ok(ResolvedNode::File(file));
    }

    Err(FsError::NotFound)
}

pub async fn resolve_path_cached(
    state: &PrimaryAppState,
    user_id: i64,
    path: &DavPath,
    root_folder_id: Option<i64>,
) -> Result<ResolvedNode, FsError> {
    let cache_key = resolve_path_cache_key(user_id, path, root_folder_id);
    if let Some(cached) = state.cache.get::<CachedResolvedNode>(&cache_key).await
        && let Some(node) =
            load_cached_resolved_node(state, user_id, path, root_folder_id, &cache_key, cached)
                .await?
    {
        tracing::debug!(user_id, root_folder_id, "webdav path cache hit");
        return Ok(node);
    }

    let node = resolve_path(&state.db, user_id, path, root_folder_id).await?;
    state
        .cache
        .set(
            &cache_key,
            &cacheable_node(&node),
            Some(WEBDAV_PATH_CACHE_TTL),
        )
        .await;
    tracing::debug!(user_id, root_folder_id, "webdav path cache miss");
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
    let segments = path_segments(path);

    if segments.is_empty() {
        return Err(FsError::Forbidden); // 不能操作根目录本身
    }

    let parent_segments = &segments[..segments.len() - 1];
    let folders = folder_repo::resolve_path_chain(db, user_id, root_folder_id, parent_segments)
        .await
        .map_err(|_| FsError::GeneralFailure)?;

    if folders.len() != parent_segments.len() {
        return Err(FsError::NotFound);
    }

    let current_parent = folders.last().map(|folder| folder.id).or(root_folder_id);
    let last = segments[segments.len() - 1].clone();
    Ok((current_parent, last))
}

async fn validate_cached_parent(
    state: &PrimaryAppState,
    user_id: i64,
    root_folder_id: Option<i64>,
    parent_segments: &[String],
    parent_id: Option<i64>,
) -> Result<bool, FsError> {
    match parent_id {
        Some(parent_id) => {
            validate_cached_folder_path(state, user_id, root_folder_id, parent_id, parent_segments)
                .await
        }
        None => Ok(root_folder_id.is_none() && parent_segments.is_empty()),
    }
}

pub async fn resolve_parent_cached(
    state: &PrimaryAppState,
    user_id: i64,
    path: &DavPath,
    root_folder_id: Option<i64>,
) -> Result<(Option<i64>, String), FsError> {
    let segments = path_segments(path);
    if segments.is_empty() {
        return Err(FsError::Forbidden);
    }

    let parent_segments = &segments[..segments.len() - 1];
    let cache_key = resolve_parent_cache_key(user_id, path, root_folder_id);
    if let Some(cached) = state.cache.get::<CachedResolvedParent>(&cache_key).await {
        if validate_cached_parent(
            state,
            user_id,
            root_folder_id,
            parent_segments,
            cached.parent_id,
        )
        .await?
        {
            tracing::debug!(user_id, root_folder_id, "webdav parent path cache hit");
            return Ok((cached.parent_id, segments[segments.len() - 1].clone()));
        }
        state.cache.delete(&cache_key).await;
    }

    let (parent_id, name) = resolve_parent(&state.db, user_id, path, root_folder_id).await?;
    state
        .cache
        .set(
            &cache_key,
            &CachedResolvedParent { parent_id },
            Some(WEBDAV_PATH_CACHE_TTL),
        )
        .await;
    tracing::debug!(user_id, root_folder_id, "webdav parent path cache miss");
    Ok((parent_id, name))
}
