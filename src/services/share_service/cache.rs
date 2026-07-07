//! 分享状态缓存。

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::cache::CacheExt;
use crate::db::repository::share_repo;
use crate::entities::share;
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::services::workspace::storage::{WorkspaceResourceScope, WorkspaceStorageScope};
use crate::utils::hash;

const ACTIVE_SHARE_TARGET_CACHE_TTL: u64 = 60;
const SHARE_TOKEN_LOOKUP_CACHE_TTL: u64 = 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedShareTokenLookup {
    id: i64,
}

#[derive(Clone, Copy)]
enum ShareTargetKind {
    File,
    Folder,
}

impl ShareTargetKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Folder => "folder",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedActiveShareIds {
    ids: HashSet<i64>,
}

fn normalize_ids(ids: &[i64]) -> Vec<i64> {
    let mut normalized = ids.to_vec();
    normalized.sort_unstable();
    normalized.dedup();
    normalized
}

fn ids_digest(ids: &[i64]) -> String {
    let mut bytes = Vec::with_capacity(ids.len().saturating_mul(std::mem::size_of::<i64>()));
    for id in ids {
        bytes.extend_from_slice(&id.to_be_bytes());
    }
    hash::sha256_hex(&bytes)
}

fn active_share_target_cache_prefix_for_scope(scope: WorkspaceResourceScope) -> String {
    match scope {
        WorkspaceResourceScope::Personal { user_id } => {
            format!("active_share_targets:personal:{user_id}:")
        }
        WorkspaceResourceScope::Team { team_id } => {
            format!("active_share_targets:team:{team_id}:")
        }
    }
}

fn active_share_target_cache_key(
    scope: WorkspaceResourceScope,
    kind: ShareTargetKind,
    ids: &[i64],
) -> String {
    format!(
        "{}{}:{}",
        active_share_target_cache_prefix_for_scope(scope),
        kind.as_str(),
        ids_digest(ids)
    )
}

fn share_token_lookup_cache_key(token: &str) -> String {
    format!("share_token_lookup:{}", hash::sha256_hex(token.as_bytes()))
}

async fn cache_share_token_lookup(
    state: &impl SharedRuntimeState,
    cache_key: &str,
    share: &share::Model,
) {
    state
        .cache()
        .set(
            cache_key,
            &CachedShareTokenLookup { id: share.id },
            Some(SHARE_TOKEN_LOOKUP_CACHE_TTL),
        )
        .await;
}

pub(super) async fn invalidate_share_token_record_cache(
    state: &impl SharedRuntimeState,
    token: &str,
) {
    state
        .cache()
        .delete(&share_token_lookup_cache_key(token))
        .await;
}

pub(crate) async fn invalidate_all_share_token_record_cache(state: &impl SharedRuntimeState) {
    state.cache().invalidate_prefix("share_token_lookup:").await;
}

pub(super) async fn invalidate_share_token_record_cache_for_share(
    state: &impl SharedRuntimeState,
    share: &share::Model,
) {
    invalidate_share_token_record_cache(state, &share.token).await;
}

pub(super) async fn load_share_record_by_token(
    state: &impl SharedRuntimeState,
    token: &str,
) -> Result<share::Model> {
    let cache_key = share_token_lookup_cache_key(token);
    if let Some(cached) = state
        .cache()
        .get::<CachedShareTokenLookup>(&cache_key)
        .await
    {
        match share_repo::find_by_id(state.reader_db(), cached.id).await {
            Ok(share) if share.token == token => {
                tracing::debug!(share_id = share.id, "share token lookup cache hit");
                return Ok(share);
            }
            Ok(share) => {
                tracing::debug!(
                    cached_share_id = cached.id,
                    actual_token = %share.token,
                    "share token lookup cache pointed to a different token; refreshing"
                );
                state.cache().delete(&cache_key).await;
            }
            Err(AsterError::ShareNotFound(_)) => {
                tracing::debug!(
                    cached_share_id = cached.id,
                    "share token lookup cache pointed to a missing share; refreshing"
                );
                state.cache().delete(&cache_key).await;
            }
            Err(error) => return Err(error),
        }
    }

    let share = share_repo::find_by_token(state.reader_db(), token)
        .await?
        .ok_or_else(|| AsterError::share_not_found(format!("token={token}")))?;

    cache_share_token_lookup(state, &cache_key, &share).await;
    tracing::debug!(share_id = share.id, "share token lookup cache miss");
    Ok(share)
}

pub(crate) async fn invalidate_active_share_target_cache_for_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
) {
    invalidate_active_share_target_cache_for_resource_scope(state, scope.into()).await;
}

pub(crate) async fn invalidate_active_share_target_cache_for_resource_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceResourceScope,
) {
    state
        .cache()
        .invalidate_prefix(&active_share_target_cache_prefix_for_scope(scope))
        .await;
}

pub(crate) async fn invalidate_active_share_target_cache_for_share(
    state: &impl SharedRuntimeState,
    share: &share::Model,
) {
    let scope = match share.team_id {
        Some(team_id) => WorkspaceStorageScope::Team {
            team_id,
            actor_user_id: share.user_id,
        },
        None => WorkspaceStorageScope::Personal {
            user_id: share.user_id,
        },
    };
    invalidate_active_share_target_cache_for_scope(state, scope).await;
}

async fn load_active_ids(
    state: &impl SharedRuntimeState,
    scope: WorkspaceResourceScope,
    kind: ShareTargetKind,
    ids: &[i64],
) -> Result<HashSet<i64>> {
    let ids = normalize_ids(ids);
    if ids.is_empty() {
        return Ok(HashSet::new());
    }

    let cache_key = active_share_target_cache_key(scope, kind, &ids);
    if let Some(cached) = state.cache().get::<CachedActiveShareIds>(&cache_key).await {
        let active_ids = load_active_ids_from_database(state, scope, kind, &ids).await?;
        if active_ids == cached.ids {
            tracing::debug!(scope = ?scope, target_kind = kind.as_str(), "active share target cache hit");
            return Ok(active_ids);
        }
        state
            .cache()
            .set(
                &cache_key,
                &CachedActiveShareIds {
                    ids: active_ids.clone(),
                },
                Some(ACTIVE_SHARE_TARGET_CACHE_TTL),
            )
            .await;
        tracing::debug!(scope = ?scope, target_kind = kind.as_str(), "active share target cache stale; refreshed from database");
        return Ok(active_ids);
    }

    let active_ids = load_active_ids_from_database(state, scope, kind, &ids).await?;

    state
        .cache()
        .set(
            &cache_key,
            &CachedActiveShareIds {
                ids: active_ids.clone(),
            },
            Some(ACTIVE_SHARE_TARGET_CACHE_TTL),
        )
        .await;
    tracing::debug!(scope = ?scope, target_kind = kind.as_str(), "active share target cache miss");
    Ok(active_ids)
}

async fn load_active_ids_from_database(
    state: &impl SharedRuntimeState,
    scope: WorkspaceResourceScope,
    kind: ShareTargetKind,
    ids: &[i64],
) -> Result<HashSet<i64>> {
    match (scope, kind) {
        (WorkspaceResourceScope::Personal { user_id }, ShareTargetKind::File) => {
            share_repo::find_active_file_ids(state.reader_db(), user_id, ids).await
        }
        (WorkspaceResourceScope::Personal { user_id }, ShareTargetKind::Folder) => {
            share_repo::find_active_folder_ids(state.reader_db(), user_id, ids).await
        }
        (WorkspaceResourceScope::Team { team_id }, ShareTargetKind::File) => {
            share_repo::find_active_team_file_ids(state.reader_db(), team_id, ids).await
        }
        (WorkspaceResourceScope::Team { team_id }, ShareTargetKind::Folder) => {
            share_repo::find_active_team_folder_ids(state.reader_db(), team_id, ids).await
        }
    }
}

pub(crate) async fn find_active_file_ids_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    file_ids: &[i64],
) -> Result<HashSet<i64>> {
    find_active_file_ids_in_resource_scope(state, scope.into(), file_ids).await
}

pub(crate) async fn find_active_folder_ids_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    folder_ids: &[i64],
) -> Result<HashSet<i64>> {
    find_active_folder_ids_in_resource_scope(state, scope.into(), folder_ids).await
}

pub(crate) async fn find_active_file_ids_in_resource_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceResourceScope,
    file_ids: &[i64],
) -> Result<HashSet<i64>> {
    load_active_ids(state, scope, ShareTargetKind::File, file_ids).await
}

pub(crate) async fn find_active_folder_ids_in_resource_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceResourceScope,
    folder_ids: &[i64],
) -> Result<HashSet<i64>> {
    load_active_ids(state, scope, ShareTargetKind::Folder, folder_ids).await
}
