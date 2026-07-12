//! 分享管理侧逻辑。
//!
//! 这里处理的是“谁可以创建、查看、删除 share”，而不是公开 token 访问。
//! 因此所有入口都先建立在 `WorkspaceStorageScope` 之上，再进入 share 记录本身。

use aster_forge_db::transaction;
use std::collections::HashMap;

use chrono::Utc;
use sea_orm::{DatabaseConnection, Set};

use crate::api::pagination::{AdminShareSortBy, load_offset_page};
use crate::db::repository::{file_repo, folder_repo, share_repo};
use crate::entities::share;
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::services::{
    files::batch,
    user::account,
    user::profile,
    workspace::storage::{self, WorkspaceStorageScope},
};
use crate::utils::{hash, id};
use aster_forge_api::{OffsetPage, SortOrder};

use super::cache::{
    invalidate_active_share_target_cache_for_scope, invalidate_active_share_target_cache_for_share,
    invalidate_share_token_record_cache, invalidate_share_token_record_cache_for_share,
};
use super::models::{
    MyShareInfo, ShareInfo, ShareTarget, ShareUpdateOutcome, share_info_from_model,
    share_target_for_share,
};
use super::shared::{
    load_share_in_scope, lock_share_resource_in_scope, remaining_downloads, resolve_share_resource,
    resolve_share_status, validate_max_downloads,
};

pub(crate) async fn create_share_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    target: ShareTarget,
    password: Option<String>,
    expires_at: Option<chrono::DateTime<Utc>>,
    max_downloads: i64,
) -> Result<ShareInfo> {
    let db = state.writer_db();
    let (file_id, folder_id) = target.into_ids();
    tracing::debug!(
        scope = ?scope,
        target = ?target,
        has_password = password.as_ref().is_some_and(|value| !value.is_empty()),
        has_expiry = expires_at.is_some(),
        max_downloads,
        "creating share"
    );
    storage::require_scope_access_with_db(state, db, scope).await?;

    validate_max_downloads(max_downloads)?;

    let password_hash = match password {
        Some(ref value) if !value.is_empty() => Some(hash::hash_password(value)?),
        _ => None,
    };

    let txn = transaction::begin(db).await?;
    lock_share_resource_in_scope(&txn, scope, file_id, folder_id).await?;

    // share 对同一资源只允许保留一条活跃记录。
    // 这里在锁住目标资源后再检查，可避免并发创建时双写活跃 share。
    let existing = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            share_repo::find_active_by_resource(&txn, user_id, file_id, folder_id).await?
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            share_repo::find_active_by_team_resource(&txn, team_id, file_id, folder_id).await?
        }
    };

    if let Some(existing) = existing {
        let is_expired = existing.expires_at.is_some_and(|exp| exp < Utc::now());
        if !is_expired {
            return Err(AsterError::validation_error(
                "an active share already exists for this resource",
            ));
        }
        share_repo::delete(&txn, existing.id).await?;
    }

    let now = Utc::now();
    let model = share::ActiveModel {
        token: Set(id::new_short_token()),
        user_id: Set(scope.actor_user_id()),
        team_id: Set(scope.team_id()),
        file_id: Set(file_id),
        folder_id: Set(folder_id),
        password: Set(password_hash),
        expires_at: Set(expires_at),
        max_downloads: Set(max_downloads),
        download_count: Set(0),
        view_count: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let created = share_repo::create(&txn, model).await?;
    transaction::commit(txn).await?;
    invalidate_active_share_target_cache_for_scope(state, scope).await;
    tracing::debug!(
        scope = ?scope,
        share_id = created.id,
        target = ?target,
        "created share"
    );
    share_info_from_model_with_user(state, created).await
}

pub async fn create_share(
    state: &impl SharedRuntimeState,
    user_id: i64,
    target: ShareTarget,
    password: Option<String>,
    expires_at: Option<chrono::DateTime<Utc>>,
    max_downloads: i64,
) -> Result<ShareInfo> {
    create_share_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        target,
        password,
        expires_at,
        max_downloads,
    )
    .await
}

pub(crate) async fn list_shares_paginated_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    limit: u64,
    offset: u64,
) -> Result<OffsetPage<MyShareInfo>> {
    tracing::debug!(
        scope = ?scope,
        limit,
        offset,
        "listing paginated shares"
    );
    storage::require_scope_access_with_db(state, state.writer_db(), scope).await?;
    let page = load_offset_page(limit, offset, 100, |limit, offset| async move {
        let (shares, total) = match scope {
            WorkspaceStorageScope::Personal { user_id } => {
                share_repo::find_by_user_paginated(state.reader_db(), user_id, limit, offset)
                    .await?
            }
            WorkspaceStorageScope::Team { team_id, .. } => {
                share_repo::find_by_team_paginated(state.reader_db(), team_id, limit, offset)
                    .await?
            }
        };
        let items = build_my_share_infos(state.reader_db(), shares).await?;
        Ok((items, total))
    })
    .await?;
    tracing::debug!(
        scope = ?scope,
        total = page.total,
        returned = page.items.len(),
        limit = page.limit,
        offset = page.offset,
        "listed paginated shares"
    );
    Ok(page)
}

pub(crate) async fn delete_share_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    share_id: i64,
) -> Result<()> {
    tracing::debug!(scope = ?scope, share_id, "deleting share");
    let share = load_share_in_scope(state, scope, share_id).await?;
    share_repo::delete(state.writer_db(), share_id).await?;
    invalidate_active_share_target_cache_for_scope(state, scope).await;
    invalidate_share_token_record_cache_for_share(state, &share).await;
    tracing::debug!(scope = ?scope, share_id, "deleted share");
    Ok(())
}

pub(crate) async fn update_share_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    share_id: i64,
    password: Option<String>,
    expires_at: Option<chrono::DateTime<Utc>>,
    max_downloads: i64,
) -> Result<ShareUpdateOutcome> {
    tracing::debug!(
        scope = ?scope,
        share_id,
        update_password = password.is_some(),
        has_expiry = expires_at.is_some(),
        max_downloads,
        "updating share"
    );
    validate_max_downloads(max_downloads)?;

    let existing = load_share_in_scope(state, scope, share_id).await?;
    let existing_token = existing.token.clone();
    let has_password = match password.as_deref() {
        Some(value) => !value.is_empty(),
        None => existing.password.is_some(),
    };
    let mut active: share::ActiveModel = existing.into();

    if let Some(password) = password {
        active.password = if password.is_empty() {
            Set(None)
        } else {
            Set(Some(hash::hash_password(&password)?))
        };
    }

    active.expires_at = Set(expires_at);
    active.max_downloads = Set(max_downloads);
    active.updated_at = Set(Utc::now());

    let updated = share_info_from_model_with_user(
        state,
        share_repo::update(state.writer_db(), active).await?,
    )
    .await?;
    invalidate_active_share_target_cache_for_scope(state, scope).await;
    invalidate_share_token_record_cache(state, &existing_token).await;
    tracing::debug!(
        scope = ?scope,
        share_id = updated.id,
        max_downloads = updated.max_downloads,
        has_expiry = updated.expires_at.is_some(),
        "updated share"
    );
    Ok(ShareUpdateOutcome {
        share: updated,
        has_password,
    })
}

pub(crate) async fn batch_delete_shares_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    share_ids: &[i64],
) -> Result<batch::BatchResult> {
    tracing::debug!(
        scope = ?scope,
        share_count = share_ids.len(),
        "batch deleting shares"
    );
    storage::require_scope_access(state, scope).await?;
    let mut result = batch::BatchResult {
        succeeded: 0,
        failed: 0,
        errors: vec![],
    };

    let scoped_shares = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            share_repo::find_by_ids_in_personal_scope(state.writer_db(), user_id, share_ids).await?
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            share_repo::find_by_ids_in_team_scope(state.writer_db(), team_id, share_ids).await?
        }
    };
    let share_map: HashMap<i64, share::Model> = scoped_shares
        .into_iter()
        .map(|share| (share.id, share))
        .collect();
    let mut ids_to_delete = Vec::new();
    let mut deleted_once = std::collections::HashSet::new();

    for &id in share_ids {
        if share_map.contains_key(&id) && deleted_once.insert(id) {
            result.succeeded += 1;
            ids_to_delete.push(id);
        } else {
            result.failed += 1;
            result.errors.push(batch::BatchItemError {
                entity_type: "share".to_string(),
                entity_id: id,
                error: AsterError::share_not_found(format!("share #{id}")).to_string(),
            });
        }
    }

    if !ids_to_delete.is_empty() {
        let txn = transaction::begin(state.writer_db()).await?;
        share_repo::delete_many(&txn, &ids_to_delete).await?;
        transaction::commit(txn).await?;
        invalidate_active_share_target_cache_for_scope(state, scope).await;
        for share_id in &ids_to_delete {
            if let Some(share) = share_map.get(share_id) {
                invalidate_share_token_record_cache_for_share(state, share).await;
            }
        }
    }

    tracing::debug!(
        scope = ?scope,
        succeeded = result.succeeded,
        failed = result.failed,
        "batch deleted shares"
    );
    Ok(result)
}

pub async fn list_my_shares_paginated(
    state: &impl SharedRuntimeState,
    user_id: i64,
    limit: u64,
    offset: u64,
) -> Result<OffsetPage<MyShareInfo>> {
    list_shares_paginated_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        limit,
        offset,
    )
    .await
}

pub async fn list_team_shares_paginated(
    state: &impl SharedRuntimeState,
    team_id: i64,
    user_id: i64,
    limit: u64,
    offset: u64,
) -> Result<OffsetPage<MyShareInfo>> {
    list_shares_paginated_in_scope(
        state,
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id: user_id,
        },
        limit,
        offset,
    )
    .await
}

pub async fn delete_share(
    state: &impl SharedRuntimeState,
    share_id: i64,
    user_id: i64,
) -> Result<()> {
    delete_share_in_scope(state, WorkspaceStorageScope::Personal { user_id }, share_id).await
}

pub async fn delete_team_share(
    state: &impl SharedRuntimeState,
    team_id: i64,
    share_id: i64,
    user_id: i64,
) -> Result<()> {
    delete_share_in_scope(
        state,
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id: user_id,
        },
        share_id,
    )
    .await
}

pub async fn update_share(
    state: &impl SharedRuntimeState,
    share_id: i64,
    user_id: i64,
    password: Option<String>,
    expires_at: Option<chrono::DateTime<Utc>>,
    max_downloads: i64,
) -> Result<ShareInfo> {
    update_share_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        share_id,
        password,
        expires_at,
        max_downloads,
    )
    .await
    .map(|outcome| outcome.share)
}

pub async fn update_team_share(
    state: &impl SharedRuntimeState,
    team_id: i64,
    share_id: i64,
    user_id: i64,
    password: Option<String>,
    expires_at: Option<chrono::DateTime<Utc>>,
    max_downloads: i64,
) -> Result<ShareInfo> {
    update_share_in_scope(
        state,
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id: user_id,
        },
        share_id,
        password,
        expires_at,
        max_downloads,
    )
    .await
    .map(|outcome| outcome.share)
}

pub fn validate_batch_share_ids(share_ids: &[i64]) -> Result<()> {
    if share_ids.is_empty() {
        return Err(AsterError::validation_error(
            "at least one share ID is required",
        ));
    }
    if share_ids.len() > batch::MAX_BATCH_ITEMS {
        return Err(AsterError::validation_error(format!(
            "batch size cannot exceed {} items",
            batch::MAX_BATCH_ITEMS
        )));
    }
    Ok(())
}

pub async fn batch_delete_shares(
    state: &impl SharedRuntimeState,
    user_id: i64,
    share_ids: &[i64],
) -> Result<batch::BatchResult> {
    validate_batch_share_ids(share_ids)?;
    batch_delete_shares_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        share_ids,
    )
    .await
}

pub async fn batch_delete_team_shares(
    state: &impl SharedRuntimeState,
    team_id: i64,
    user_id: i64,
    share_ids: &[i64],
) -> Result<batch::BatchResult> {
    validate_batch_share_ids(share_ids)?;
    batch_delete_shares_in_scope(
        state,
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id: user_id,
        },
        share_ids,
    )
    .await
}

pub async fn list_paginated(
    state: &impl SharedRuntimeState,
    limit: u64,
    offset: u64,
    sort_by: AdminShareSortBy,
    sort_order: SortOrder,
) -> Result<OffsetPage<ShareInfo>> {
    load_offset_page(limit, offset, 100, |limit, offset| async move {
        let (items, total) =
            share_repo::find_paginated(state.reader_db(), limit, offset, sort_by, sort_order)
                .await?;
        let items = share_infos_from_models(state, items).await?;
        Ok((items, total))
    })
    .await
}

pub async fn admin_delete_share(state: &impl SharedRuntimeState, share_id: i64) -> Result<()> {
    let share = share_repo::find_by_id(state.writer_db(), share_id).await?;
    share_repo::delete(state.writer_db(), share_id).await?;
    invalidate_active_share_target_cache_for_share(state, &share).await;
    invalidate_share_token_record_cache_for_share(state, &share).await;
    Ok(())
}

async fn build_my_share_infos(
    db: &DatabaseConnection,
    shares: Vec<share::Model>,
) -> Result<Vec<MyShareInfo>> {
    let mut file_ids = Vec::new();
    let mut folder_ids = Vec::new();
    for share in &shares {
        match share_target_for_share(share)? {
            ShareTarget {
                r#type: crate::types::EntityType::File,
                id,
            } => file_ids.push(id),
            ShareTarget {
                r#type: crate::types::EntityType::Folder,
                id,
            } => folder_ids.push(id),
        }
    }

    let files = file_repo::find_by_ids(db, &file_ids).await?;
    let folders = folder_repo::find_by_ids(db, &folder_ids).await?;

    let file_map: HashMap<i64, crate::entities::file::Model> =
        files.into_iter().map(|file| (file.id, file)).collect();
    let folder_map: HashMap<i64, crate::entities::folder::Model> = folders
        .into_iter()
        .map(|folder| (folder.id, folder))
        .collect();

    let mut items = Vec::with_capacity(shares.len());
    for share in shares {
        let (resource_id, resource_name, resource_type, resource_deleted) =
            resolve_share_resource(&share, &file_map, &folder_map)?;
        let status = resolve_share_status(&share, resource_deleted);
        let remaining_downloads = remaining_downloads(share.max_downloads, share.download_count);

        items.push(MyShareInfo {
            id: share.id,
            token: share.token,
            resource_id,
            resource_name,
            resource_type,
            resource_deleted,
            has_password: share.password.is_some(),
            status,
            expires_at: share.expires_at,
            max_downloads: share.max_downloads,
            download_count: share.download_count,
            view_count: share.view_count,
            remaining_downloads,
            created_at: share.created_at,
            updated_at: share.updated_at,
        });
    }

    Ok(items)
}

async fn share_infos_from_models(
    state: &impl SharedRuntimeState,
    shares: Vec<share::Model>,
) -> Result<Vec<ShareInfo>> {
    let user_ids: Vec<i64> = shares.iter().map(|share| share.user_id).collect();
    let users =
        account::user_summaries_by_ids(state, &user_ids, profile::AvatarAudience::AdminUser)
            .await?;

    shares
        .into_iter()
        .map(|share| {
            let user = users.get(&share.user_id).cloned();
            share_info_from_model(share, user)
        })
        .collect()
}

async fn share_info_from_model_with_user(
    state: &impl SharedRuntimeState,
    share: share::Model,
) -> Result<ShareInfo> {
    let user =
        account::user_summary_by_id(state, share.user_id, profile::AvatarAudience::AdminUser)
            .await?;
    share_info_from_model(share, user)
}
