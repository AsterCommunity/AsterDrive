//! 文件夹服务子模块：`mutation`。
//!
//! 这里负责文件夹的写操作以及详情页的派生信息：
//! - 创建、软删除、更新/移动、锁定状态。
//! - 为个人空间和团队空间共用同一套 scope-aware 实现。
//! - 详情接口额外计算 `storage_used`，但列表接口不走这段递归统计。

use std::collections::BTreeSet;

use chrono::Utc;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Set};

use crate::db::repository::{file_repo, folder_repo, version_repo};
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

const STORAGE_USED_VERSION_SUM_CHUNK_SIZE: usize = 500;
const STORAGE_USED_FOLDER_QUERY_CHUNK_SIZE: usize = 500;
const STORAGE_USED_FILE_PAGE_SIZE: u64 = 500;

/// `storage_used` 是配额口径，不是物理 blob 占用。
///
/// 文件夹统计需要把当前文件大小和历史版本大小都加进去；这里用 checked add，
/// 避免极端脏数据把 i64 加爆后静默回绕。`context` 用闭包是因为正常路径不会溢出，
/// 不该在热循环里为每个文件提前分配错误消息字符串。
fn add_checked<F, D>(total: &mut i64, value: i64, context: F) -> Result<()>
where
    F: FnOnce() -> D,
    D: std::fmt::Display,
{
    *total = total.checked_add(value).ok_or_else(|| {
        AsterError::internal_error(format!("folder storage_used overflow: {}", context()))
    })?;
    Ok(())
}

/// 读取一批文件夹下的活跃文件 id 和当前大小。
///
/// 只查 `(id, size)`，并通过 `after_id` 做 cursor 分页；调用方处理完当前页后
/// 立即释放 file ids，避免大目录详情把整棵树文件记录压进内存。
async fn load_file_id_size_page(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_ids: &[i64],
    after_id: Option<i64>,
) -> Result<Vec<file_repo::FileIdSize>> {
    let file_scope = match scope {
        WorkspaceStorageScope::Personal { user_id } => file_repo::FileScope::Personal { user_id },
        WorkspaceStorageScope::Team { team_id, .. } => file_repo::FileScope::Team { team_id },
    };
    file_repo::find_id_size_by_folders(
        state.reader_db(),
        file_scope,
        folder_ids,
        after_id,
        STORAGE_USED_FILE_PAGE_SIZE,
    )
    .await
}

/// 读取当前 BFS 层的下一层子目录 id。
///
/// 这里同样只拿 id，不加载完整 folder model；详情页只需要继续向下遍历。
async fn load_child_folder_ids(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_ids: &[i64],
) -> Result<Vec<i64>> {
    let folder_scope = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            folder_repo::FolderScope::Personal { user_id }
        }
        WorkspaceStorageScope::Team { team_id, .. } => folder_repo::FolderScope::Team { team_id },
    };
    folder_repo::find_child_ids_in_parents(state.reader_db(), folder_scope, folder_ids).await
}

/// 递归计算文件夹详情页展示的占用空间。
///
/// 算法是分层 BFS：
/// - `frontier` 是当前层文件夹 id，按固定 chunk 查询，避免过大的 `IN (...)`。
/// - 每个 chunk 内的文件按 `files.id` cursor 分页，避免 `Vec<i64>` 随目录规模膨胀。
/// - 当前文件大小即时累加；版本大小按当前页 file ids 分块汇总。
/// - 子目录 id 进入下一层，再重复。
///
/// 这段逻辑刻意不复用 `collect_folder_tree_in_scope()`：那个 helper 会收集完整树，
/// 删除/复制场景可以接受，详情页统计大目录时不该这么做。
async fn compute_folder_storage_used(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_id: i64,
) -> Result<i64> {
    let mut total = 0i64;
    let mut frontier = vec![folder_id];

    while !frontier.is_empty() {
        frontier.sort_unstable();
        frontier.dedup();
        let mut next_frontier = Vec::new();

        for folder_chunk in frontier.chunks(STORAGE_USED_FOLDER_QUERY_CHUNK_SIZE) {
            let mut after_file_id = None;
            loop {
                let files =
                    load_file_id_size_page(state, scope, folder_chunk, after_file_id).await?;
                if files.is_empty() {
                    break;
                }

                let mut file_ids = Vec::with_capacity(files.len());
                for (file_id, size) in &files {
                    add_checked(&mut total, *size, || {
                        format!("current file bytes for file #{file_id}")
                    })?;
                    file_ids.push(*file_id);
                }
                after_file_id = files.last().map(|(file_id, _)| *file_id);

                for file_id_chunk in file_ids.chunks(STORAGE_USED_VERSION_SUM_CHUNK_SIZE) {
                    let version_bytes =
                        version_repo::sum_sizes_by_file_ids(state.reader_db(), file_id_chunk)
                            .await?;
                    add_checked(&mut total, version_bytes, || {
                        format!("version bytes for folder #{folder_id}")
                    })?;
                }

                if files.len() < STORAGE_USED_FILE_PAGE_SIZE as usize {
                    break;
                }
            }

            let child_ids = load_child_folder_ids(state, scope, folder_chunk).await?;
            next_frontier.extend(child_ids);
        }

        frontier = next_frontier;
    }

    Ok(total)
}

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
        workspace_storage_service::require_team_access_with_db(
            state,
            state.writer_db(),
            team_id,
            actor_user_id,
        )
        .await?;
    }

    let name = crate::utils::normalize_validate_name(name)?;
    let created_by_username = load_scope_actor_username(state.writer_db(), scope).await?;

    let now = Utc::now();
    let created = crate::db::transaction::with_transaction(state.writer_db(), async |txn| {
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

    // 删除整棵树时先锁目录树，再确认树结构没有变化；否则并发移动/创建子目录
    // 可能导致只删除到遍历时看到的一部分节点。
    let (folder, file_count, folder_count) =
        crate::db::transaction::with_transaction(state.writer_db(), async |txn| {
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
            storage_change_service::StorageChangeKind::FolderTrashed,
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
    workspace_storage_service::verify_folder_access_for_read(state, scope, folder_id).await
}

pub(crate) async fn get_info_with_storage_used_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_id: i64,
) -> Result<FolderInfo> {
    // 先走普通详情权限校验；`storage_used` 只在用户确实能读这个文件夹时计算。
    let folder = get_info_in_scope(state, scope, folder_id).await?;
    let storage_used = compute_folder_storage_used(state, scope, folder_id).await?;
    Ok(FolderInfo::from_model_with_storage_used(
        folder,
        storage_used,
    ))
}

pub(crate) async fn update_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    id: i64,
    name: Option<String>,
    parent_id: NullablePatch<i64>,
    policy_id: NullablePatch<i64>,
) -> Result<folder::Model> {
    let db = state.writer_db();
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
        // 固定按 id 顺序加锁，降低多个移动操作互相等待时形成死锁的概率。
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
        // 目标父目录链里不能出现自己，否则会制造目录环。
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

    // 第一次遍历拿候选树，锁住目录后再遍历确认；如果目录树在加锁前后变了，
    // 重新来一轮。这样软删除不会漏掉并发插入/移动进来的子目录。
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
    // 从目标父目录一路向根走，用于校验 scope、检测环，以及决定移动时要锁哪些目录。
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
