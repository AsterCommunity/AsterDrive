//! 文件夹服务子模块：`mutation`。

use std::collections::BTreeSet;

use chrono::Utc;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Set};

use crate::db::repository::{file_repo, folder_repo};
use crate::entities::{file, folder};
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::{
    storage_change_service,
    workspace_models::FolderInfo,
    workspace_storage_service::{self, WorkspaceStorageScope, load_scope_actor_username},
};
use crate::types::NullablePatch;

use super::{collect_folder_tree_in_scope, ensure_folder_model_in_scope};

pub(crate) async fn create_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    name: &str,
    parent_id: Option<i64>,
) -> Result<folder::Model> {
    tracing::debug!(
        scope = ?scope,
        parent_id,
        name = %name,
        "creating folder"
    );
    if let WorkspaceStorageScope::Team {
        team_id,
        actor_user_id,
    } = scope
    {
        workspace_storage_service::require_team_access(state, team_id, actor_user_id).await?;
    }

    let name = crate::utils::normalize_validate_name(name)?;
    let created_by_username = load_scope_actor_username(&state.db, scope).await?;

    let now = Utc::now();
    let created = crate::db::transaction::with_transaction(&state.db, async |txn| {
        if let Some(pid) = parent_id {
            let parent = folder_repo::lock_by_id(txn, pid).await?;
            ensure_folder_model_in_scope(&parent, scope)?;
        }

        if find_folder_by_name_in_scope(txn, scope, parent_id, &name)
            .await?
            .is_some()
        {
            return Err(folder_repo::duplicate_name_error(&name));
        }

        folder_repo::create(
            txn,
            folder::ActiveModel {
                name: Set(name),
                parent_id: Set(parent_id),
                team_id: Set(scope.team_id()),
                owner_user_id: Set(scope.owner_user_id()),
                created_by_user_id: Set(Some(scope.actor_user_id())),
                created_by_username: Set(created_by_username),
                policy_id: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await
    })
    .await?;
    storage_change_service::publish(
        state,
        storage_change_service::StorageChangeEvent::new(
            storage_change_service::StorageChangeKind::FolderCreated,
            scope,
            vec![],
            vec![created.id],
            vec![created.parent_id],
        ),
    );
    super::invalidate_folder_path_cache(state).await;
    tracing::debug!(
        scope = ?scope,
        folder_id = created.id,
        parent_id = created.parent_id,
        name = %created.name,
        "created folder"
    );
    Ok(created)
}

pub async fn create(
    state: &PrimaryAppState,
    user_id: i64,
    name: &str,
    parent_id: Option<i64>,
) -> Result<FolderInfo> {
    create_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        name,
        parent_id,
    )
    .await
    .map(Into::into)
}

pub(crate) async fn delete_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_id: i64,
) -> Result<()> {
    tracing::debug!(scope = ?scope, folder_id, "soft deleting folder tree");
    let now = Utc::now();

    let (folder, file_count, folder_count) =
        crate::db::transaction::with_transaction(&state.db, async |txn| {
            let folder = folder_repo::lock_by_id(txn, folder_id).await?;
            ensure_folder_model_in_scope(&folder, scope)?;
            if folder.is_locked {
                return Err(AsterError::resource_locked("folder is locked"));
            }

            let (files, folder_ids) =
                collect_locked_folder_tree_in_scope(txn, scope, folder_id).await?;
            let file_count = files.len();
            let folder_count = folder_ids.len();
            let file_ids: Vec<i64> = files.into_iter().map(|f| f.id).collect();
            file_repo::soft_delete_many(txn, &file_ids, now).await?;
            folder_repo::soft_delete_many(txn, &folder_ids, now).await?;
            Ok((folder, file_count, folder_count))
        })
        .await?;
    storage_change_service::publish(
        state,
        storage_change_service::StorageChangeEvent::new(
            storage_change_service::StorageChangeKind::FolderDeleted,
            scope,
            vec![],
            vec![folder.id],
            vec![folder.parent_id],
        ),
    );
    super::invalidate_folder_path_cache(state).await;
    tracing::debug!(
        scope = ?scope,
        folder_id = folder.id,
        parent_id = folder.parent_id,
        file_count,
        folder_count,
        "soft deleted folder tree"
    );
    Ok(())
}

/// 删除文件夹（软删除 → 回收站，递归标记子项）
pub async fn delete(state: &PrimaryAppState, id: i64, user_id: i64) -> Result<()> {
    delete_in_scope(state, WorkspaceStorageScope::Personal { user_id }, id).await
}

pub(crate) async fn get_info_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_id: i64,
) -> Result<folder::Model> {
    workspace_storage_service::verify_folder_access(state, scope, folder_id).await
}

pub(crate) async fn update_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    id: i64,
    name: Option<String>,
    parent_id: NullablePatch<i64>,
    policy_id: NullablePatch<i64>,
) -> Result<folder::Model> {
    let db = &state.db;
    tracing::debug!(
        scope = ?scope,
        folder_id = id,
        target_name = name.as_deref().unwrap_or(""),
        parent_patch = ?parent_id,
        policy_patch = ?policy_id,
        "updating folder metadata"
    );
    if let NullablePatch::Value(pid) = parent_id
        && pid == id
    {
        return Err(AsterError::validation_error(
            "cannot move folder into itself",
        ));
    }

    let name = match name {
        Some(name) => Some(crate::utils::normalize_validate_name(&name)?),
        None => None,
    };

    let (updated, previous_parent_id) = crate::db::transaction::with_transaction(db, async |txn| {
        let preview = folder_repo::find_by_id(txn, id).await?;
        ensure_folder_model_in_scope(&preview, scope)?;
        let preview_target_parent = match parent_id {
            NullablePatch::Absent => preview.parent_id,
            NullablePatch::Null => None,
            NullablePatch::Value(pid) => Some(pid),
        };
        let initial_target_chain =
            load_folder_chain_in_scope(txn, scope, preview_target_parent).await?;
        let mut lock_ids = vec![id];
        lock_ids.extend(initial_target_chain.iter().map(|folder| folder.id));
        lock_folder_ids_in_order(txn, &lock_ids).await?;

        let current = folder_repo::lock_by_id(txn, id).await?;
        ensure_folder_model_in_scope(&current, scope)?;
        if current.is_locked {
            return Err(AsterError::resource_locked("folder is locked"));
        }

        let target_parent = match parent_id {
            NullablePatch::Absent => current.parent_id,
            NullablePatch::Null => None,
            NullablePatch::Value(pid) => Some(pid),
        };
        let target_chain = load_folder_chain_in_scope(txn, scope, target_parent).await?;
        if target_chain.iter().any(|folder| folder.id == id) {
            return Err(AsterError::validation_error(
                "cannot move folder into its own subfolder",
            ));
        }

        let final_name = name.clone().unwrap_or_else(|| current.name.clone());
        if let Some(existing) =
            find_folder_by_name_in_scope(txn, scope, target_parent, &final_name).await?
            && existing.id != id
        {
            return Err(folder_repo::duplicate_name_error(&final_name));
        }

        let previous_parent_id = current.parent_id;
        let mut active: folder::ActiveModel = current.into();
        if let Some(n) = name.clone() {
            active.name = Set(n);
        }
        match parent_id {
            NullablePatch::Absent => {}
            NullablePatch::Null => active.parent_id = Set(None),
            NullablePatch::Value(pid) => active.parent_id = Set(Some(pid)),
        }
        match policy_id {
            NullablePatch::Absent => {}
            NullablePatch::Null => active.policy_id = Set(None),
            NullablePatch::Value(pid) => active.policy_id = Set(Some(pid)),
        }
        active.updated_at = Set(Utc::now());
        let updated = active
            .update(txn)
            .await
            .map_err(|err| folder_repo::map_name_db_err(err, &final_name))?;
        Ok((updated, previous_parent_id))
    })
    .await?;
    storage_change_service::publish(
        state,
        storage_change_service::StorageChangeEvent::new(
            storage_change_service::StorageChangeKind::FolderUpdated,
            scope,
            vec![],
            vec![updated.id],
            vec![previous_parent_id, updated.parent_id],
        ),
    );
    if name.is_some() || parent_id.is_present() {
        super::invalidate_folder_path_cache(state).await;
    }
    tracing::debug!(
        scope = ?scope,
        folder_id = updated.id,
        parent_id = updated.parent_id,
        name = %updated.name,
        policy_id = updated.policy_id,
        "updated folder metadata"
    );
    Ok(updated)
}

async fn find_folder_by_name_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    parent_id: Option<i64>,
    name: &str,
) -> Result<Option<folder::Model>> {
    match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            folder_repo::find_by_name_in_parent(db, user_id, parent_id, name).await
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            folder_repo::find_by_name_in_team_parent(db, team_id, parent_id, name).await
        }
    }
}

async fn collect_locked_folder_tree_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    folder_id: i64,
) -> Result<(Vec<file::Model>, Vec<i64>)> {
    const MAX_STABILIZATION_ATTEMPTS: usize = 8;

    for _ in 0..MAX_STABILIZATION_ATTEMPTS {
        let (_files, folder_ids) =
            collect_folder_tree_in_scope(db, scope, folder_id, false).await?;
        lock_folder_ids_in_order(db, &folder_ids).await?;

        let (confirmed_files, confirmed_folder_ids) =
            collect_folder_tree_in_scope(db, scope, folder_id, false).await?;
        let locked_ids: BTreeSet<i64> = folder_ids.iter().copied().collect();
        let confirmed_ids: BTreeSet<i64> = confirmed_folder_ids.iter().copied().collect();
        if locked_ids == confirmed_ids {
            return Ok((confirmed_files, confirmed_folder_ids));
        }
    }

    Err(AsterError::internal_error(
        "folder tree did not stabilize while acquiring delete locks",
    ))
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
        ensure_folder_model_in_scope(&folder, scope)?;
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

pub async fn update(
    state: &PrimaryAppState,
    id: i64,
    user_id: i64,
    name: Option<String>,
    parent_id: NullablePatch<i64>,
    policy_id: NullablePatch<i64>,
) -> Result<FolderInfo> {
    update_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        id,
        name,
        parent_id,
        policy_id,
    )
    .await
    .map(Into::into)
}

/// 移动文件夹到指定父文件夹（None = 根目录）
///
/// 与 `update()` 的区别：`update()` 用 `NullablePatch<i64>` 区分
/// “未传字段”和“显式传 null”，而本函数的 `target_parent_id: None`
/// 明确表示“移到根目录”。
pub async fn move_folder(
    state: &PrimaryAppState,
    id: i64,
    user_id: i64,
    target_parent_id: Option<i64>,
) -> Result<FolderInfo> {
    update_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        id,
        None,
        match target_parent_id {
            Some(parent_id) => NullablePatch::Value(parent_id),
            None => NullablePatch::Null,
        },
        NullablePatch::Absent,
    )
    .await
    .map(Into::into)
}

pub(crate) async fn set_lock_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_id: i64,
    locked: bool,
) -> Result<folder::Model> {
    use crate::services::lock_service;
    use crate::types::EntityType;

    tracing::debug!(
        scope = ?scope,
        folder_id,
        locked,
        "setting folder lock state"
    );
    workspace_storage_service::verify_folder_access(state, scope, folder_id).await?;

    if locked {
        lock_service::lock(
            state,
            EntityType::Folder,
            folder_id,
            Some(scope.actor_user_id()),
            None,
            None,
        )
        .await?;
    } else {
        lock_service::unlock(state, EntityType::Folder, folder_id, scope.actor_user_id()).await?;
    }

    let folder = workspace_storage_service::verify_folder_access(state, scope, folder_id).await?;
    tracing::debug!(
        scope = ?scope,
        folder_id = folder.id,
        locked = folder.is_locked,
        "updated folder lock state"
    );
    Ok(folder)
}

/// 设置/解除文件夹锁，返回更新后的文件夹信息
pub async fn set_lock(
    state: &PrimaryAppState,
    folder_id: i64,
    user_id: i64,
    locked: bool,
) -> Result<FolderInfo> {
    set_lock_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        folder_id,
        locked,
    )
    .await
    .map(Into::into)
}
