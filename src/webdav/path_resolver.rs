//! WebDAV 子模块：`path_resolver`。

use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

use crate::cache::CacheExt;
use crate::db::repository::{file_repo, folder_repo};
use crate::entities::{file, folder};
use crate::errors::AsterError;
use crate::runtime::PrimaryAppState;
use crate::services::folder_service;
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
    Folder { id: i64 },
    File { id: i64 },
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

fn join_path(prefix: &str, leaf: &str) -> String {
    if prefix == "/" {
        format!("/{leaf}")
    } else {
        format!("{prefix}/{leaf}")
    }
}

fn path_from_segments(segments: &[String]) -> String {
    if segments.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", segments.join("/"))
    }
}

async fn expected_full_path(
    db: &DatabaseConnection,
    root_folder_id: Option<i64>,
    segments: &[String],
) -> Result<String, FsError> {
    if let Some(root_folder_id) = root_folder_id {
        let mut paths = folder_service::build_folder_paths(db, &[root_folder_id])
            .await
            .map_err(|_| FsError::GeneralFailure)?;
        let root_path = paths
            .remove(&root_folder_id)
            .ok_or(FsError::GeneralFailure)?;
        if segments.is_empty() {
            Ok(root_path)
        } else {
            Ok(join_path(&root_path, &segments.join("/")))
        }
    } else {
        Ok(path_from_segments(segments))
    }
}

async fn folder_full_path(db: &DatabaseConnection, folder_id: i64) -> Result<String, FsError> {
    let mut paths = folder_service::build_folder_paths(db, &[folder_id])
        .await
        .map_err(|_| FsError::GeneralFailure)?;
    paths.remove(&folder_id).ok_or(FsError::GeneralFailure)
}

async fn file_full_path(db: &DatabaseConnection, file: &file::Model) -> Result<String, FsError> {
    let parent_path = if let Some(folder_id) = file.folder_id {
        folder_full_path(db, folder_id).await?
    } else {
        "/".to_string()
    };
    Ok(join_path(&parent_path, &file.name))
}

fn cacheable_node(node: &ResolvedNode) -> CachedResolvedNode {
    match node {
        ResolvedNode::Root => CachedResolvedNode::Root,
        ResolvedNode::Folder(folder) => CachedResolvedNode::Folder { id: folder.id },
        ResolvedNode::File(file) => CachedResolvedNode::File { id: file.id },
    }
}

fn is_missing_entity(error: &AsterError) -> bool {
    matches!(
        error,
        AsterError::RecordNotFound(_) | AsterError::FileNotFound(_) | AsterError::FolderNotFound(_)
    )
}

async fn load_cached_resolved_node(
    state: &PrimaryAppState,
    user_id: i64,
    root_folder_id: Option<i64>,
    path: &DavPath,
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
        CachedResolvedNode::Folder { id } => {
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
            let expected = expected_full_path(&state.db, root_folder_id, &segments).await?;
            let current = folder_full_path(&state.db, folder.id).await?;
            if current != expected {
                state.cache.delete(cache_key).await;
                return Ok(None);
            }
            Ok(Some(ResolvedNode::Folder(folder)))
        }
        CachedResolvedNode::File { id } => {
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
            let expected = expected_full_path(&state.db, root_folder_id, &segments).await?;
            let current = file_full_path(&state.db, &file).await?;
            if current != expected {
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
            load_cached_resolved_node(state, user_id, root_folder_id, path, &cache_key, cached)
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
            let folder = match folder_repo::find_by_id(&state.db, parent_id).await {
                Ok(folder) => folder,
                Err(error) if is_missing_entity(&error) => return Ok(false),
                Err(_) => return Err(FsError::GeneralFailure),
            };
            if folder.owner_user_id != Some(user_id)
                || folder.team_id.is_some()
                || folder.deleted_at.is_some()
            {
                return Ok(false);
            }
            let expected = expected_full_path(&state.db, root_folder_id, parent_segments).await?;
            let current = folder_full_path(&state.db, folder.id).await?;
            Ok(current == expected)
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
