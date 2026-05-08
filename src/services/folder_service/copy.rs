//! 文件夹服务子模块：`copy`。

use std::collections::HashMap;

use chrono::Utc;
use sea_orm::Set;

use crate::db::repository::{file_repo, folder_repo};
use crate::entities::folder;
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::{
    storage_change_service,
    workspace_models::FolderInfo,
    workspace_storage_service::{self, WorkspaceStorageScope, load_scope_actor_username},
};

use super::ensure_folder_model_in_scope;

const MAX_COPY_NAME_RETRIES: usize = 32;

#[derive(Clone, Copy)]
struct FrontierFolderCopy {
    src_folder_id: i64,
    dest_folder_id: i64,
}

struct PlannedChildFolderCopy {
    src_folder_id: i64,
    dest_parent_id: i64,
    dest_name: String,
    policy_id: Option<i64>,
}

async fn copy_frontier_files_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    frontier: &[FrontierFolderCopy],
) -> Result<()> {
    if frontier.is_empty() {
        return Ok(());
    }

    let db = &state.db;
    let src_folder_ids: Vec<i64> = frontier.iter().map(|item| item.src_folder_id).collect();
    let dest_by_src: HashMap<i64, i64> = frontier
        .iter()
        .map(|item| (item.src_folder_id, item.dest_folder_id))
        .collect();

    let files = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            file_repo::find_by_folders(db, user_id, &src_folder_ids).await?
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            file_repo::find_by_team_folders(db, team_id, &src_folder_ids).await?
        }
    };
    let copy_specs: Vec<crate::services::file_service::BatchDuplicateFileRecordTargetSpec> = files
        .into_iter()
        .map(|file| {
            let src_folder_id = file.folder_id.ok_or_else(|| {
                AsterError::internal_error(format!(
                    "folder copy encountered root file #{} in batched frontier load",
                    file.id
                ))
            })?;
            let dest_folder_id = dest_by_src.get(&src_folder_id).copied().ok_or_else(|| {
                AsterError::internal_error(format!(
                    "missing destination folder mapping for source folder #{src_folder_id}"
                ))
            })?;
            Ok(
                crate::services::file_service::BatchDuplicateFileRecordTargetSpec {
                    dest_name: file.name.clone(),
                    src: file,
                    dest_folder_id: Some(dest_folder_id),
                },
            )
        })
        .collect::<Result<_>>()?;

    crate::services::file_service::batch_duplicate_file_records_to_mixed_folders_in_scope(
        state,
        scope,
        &copy_specs,
    )
    .await
}

async fn load_frontier_child_plans_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    frontier: &[FrontierFolderCopy],
) -> Result<Vec<PlannedChildFolderCopy>> {
    if frontier.is_empty() {
        return Ok(vec![]);
    }

    let db = &state.db;
    let src_folder_ids: Vec<i64> = frontier.iter().map(|item| item.src_folder_id).collect();
    let dest_by_src: HashMap<i64, i64> = frontier
        .iter()
        .map(|item| (item.src_folder_id, item.dest_folder_id))
        .collect();

    let children = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            folder_repo::find_children_in_parents(db, user_id, &src_folder_ids).await?
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            folder_repo::find_team_children_in_parents(db, team_id, &src_folder_ids).await?
        }
    };
    if children.is_empty() {
        return Ok(vec![]);
    }

    children
        .iter()
        .map(|child| {
            let src_parent_id = child.parent_id.ok_or_else(|| {
                AsterError::internal_error(format!(
                    "child folder #{} has no parent during frontier copy",
                    child.id
                ))
            })?;
            let dest_parent_id = dest_by_src.get(&src_parent_id).copied().ok_or_else(|| {
                AsterError::internal_error(format!(
                    "missing destination parent mapping for source folder #{src_parent_id}"
                ))
            })?;
            Ok(PlannedChildFolderCopy {
                src_folder_id: child.id,
                dest_parent_id,
                dest_name: child.name.clone(),
                policy_id: child.policy_id,
            })
        })
        .collect()
}

async fn create_frontier_children_from_plans_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    child_plans: Vec<PlannedChildFolderCopy>,
) -> Result<Vec<FrontierFolderCopy>> {
    if child_plans.is_empty() {
        return Ok(vec![]);
    }

    let db = &state.db;
    let mut dest_parent_ids: Vec<i64> =
        child_plans.iter().map(|plan| plan.dest_parent_id).collect();
    dest_parent_ids.sort_unstable();
    dest_parent_ids.dedup();

    let now = Utc::now();
    let created_by_username = load_scope_actor_username(db, scope).await?;
    let models: Vec<folder::ActiveModel> = child_plans
        .iter()
        .map(|plan| folder::ActiveModel {
            name: Set(plan.dest_name.clone()),
            parent_id: Set(Some(plan.dest_parent_id)),
            team_id: Set(scope.team_id()),
            owner_user_id: Set(scope.owner_user_id()),
            created_by_user_id: Set(Some(scope.actor_user_id())),
            created_by_username: Set(created_by_username.clone()),
            policy_id: Set(plan.policy_id),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        })
        .collect();
    folder_repo::create_many(db, models).await?;

    let created_children = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            folder_repo::find_children_in_parents(db, user_id, &dest_parent_ids).await?
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            folder_repo::find_team_children_in_parents(db, team_id, &dest_parent_ids).await?
        }
    };
    let created_by_parent_and_name: HashMap<(i64, String), i64> = created_children
        .into_iter()
        .filter_map(|child| {
            child
                .parent_id
                .map(|parent_id| ((parent_id, child.name), child.id))
        })
        .collect();

    child_plans
        .into_iter()
        .map(|plan| {
            let key = (plan.dest_parent_id, plan.dest_name);
            let dest_folder_id =
                created_by_parent_and_name
                    .get(&key)
                    .copied()
                    .ok_or_else(|| {
                        AsterError::internal_error(format!(
                            "failed to reload copied folder '{}' under parent #{}",
                            key.1, key.0
                        ))
                    })?;
            Ok(FrontierFolderCopy {
                src_folder_id: plan.src_folder_id,
                dest_folder_id,
            })
        })
        .collect()
}

pub(crate) fn recursive_copy_folder_in_scope<'a>(
    state: &'a PrimaryAppState,
    scope: WorkspaceStorageScope,
    src_folder_id: i64,
    dest_parent_id: Option<i64>,
    dest_name: &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<folder::Model>> + Send + 'a>> {
    Box::pin(async move {
        let db = &state.db;
        let now = Utc::now();
        let src_folder = folder_repo::find_by_id(db, src_folder_id).await?;
        ensure_folder_model_in_scope(&src_folder, scope)?;
        let created_by_username = load_scope_actor_username(db, scope).await?;

        let new_folder = folder_repo::create(
            db,
            folder::ActiveModel {
                name: Set(dest_name.to_string()),
                parent_id: Set(dest_parent_id),
                team_id: Set(scope.team_id()),
                owner_user_id: Set(scope.owner_user_id()),
                created_by_user_id: Set(Some(scope.actor_user_id())),
                created_by_username: Set(created_by_username),
                policy_id: Set(src_folder.policy_id),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await?;

        let mut frontier = vec![FrontierFolderCopy {
            src_folder_id,
            dest_folder_id: new_folder.id,
        }];
        while !frontier.is_empty() {
            // 先并发完成当前层的“文件批量复制”和“下一层子目录读取”，
            // 但把子目录真正写库放在文件复制成功之后，避免扩大失败时的半成品范围。
            let ((), child_plans) = tokio::try_join!(
                copy_frontier_files_in_scope(state, scope, &frontier),
                load_frontier_child_plans_in_scope(state, scope, &frontier),
            )?;
            frontier =
                create_frontier_children_from_plans_in_scope(state, scope, child_plans).await?;
        }

        Ok(new_folder)
    })
}

pub(crate) async fn copy_folder_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    src_id: i64,
    dest_parent_id: Option<i64>,
) -> Result<folder::Model> {
    let db = &state.db;
    tracing::debug!(
        scope = ?scope,
        src_folder_id = src_id,
        dest_parent_id,
        "copying folder tree"
    );
    let src = workspace_storage_service::verify_folder_access(state, scope, src_id).await?;

    if let Some(parent_id) = dest_parent_id {
        workspace_storage_service::verify_folder_access(state, scope, parent_id).await?;

        let mut cursor = Some(parent_id);
        while let Some(cur_id) = cursor {
            if cur_id == src_id {
                return Err(AsterError::validation_error(
                    "cannot copy folder into its own subfolder",
                ));
            }
            let current = folder_repo::find_by_id(db, cur_id).await?;
            ensure_folder_model_in_scope(&current, scope)?;
            cursor = current.parent_id;
        }
    }

    let mut dest_name = src.name.clone();
    for _ in 0..MAX_COPY_NAME_RETRIES {
        let exists = match scope {
            WorkspaceStorageScope::Personal { user_id } => {
                folder_repo::find_by_name_in_parent(db, user_id, dest_parent_id, &dest_name)
                    .await?
                    .is_some()
            }
            WorkspaceStorageScope::Team { team_id, .. } => {
                folder_repo::find_by_name_in_team_parent(db, team_id, dest_parent_id, &dest_name)
                    .await?
                    .is_some()
            }
        };

        if exists {
            dest_name = crate::utils::next_copy_name(&dest_name);
            continue;
        }

        match recursive_copy_folder_in_scope(state, scope, src_id, dest_parent_id, &dest_name).await
        {
            Ok(copied) => {
                storage_change_service::publish(
                    state,
                    storage_change_service::StorageChangeEvent::new(
                        storage_change_service::StorageChangeKind::FolderCreated,
                        scope,
                        vec![],
                        vec![copied.id],
                        vec![copied.parent_id],
                    ),
                );
                super::invalidate_folder_path_cache(state).await;
                tracing::debug!(
                    scope = ?scope,
                    src_folder_id = src_id,
                    copied_folder_id = copied.id,
                    parent_id = copied.parent_id,
                    name = %copied.name,
                    "copied folder tree"
                );
                return Ok(copied);
            }
            Err(err) if folder_repo::is_duplicate_name_error(&err, &dest_name) => {
                dest_name = crate::utils::next_copy_name(&dest_name);
            }
            Err(err) => return Err(err),
        }
    }

    Err(AsterError::validation_error(format!(
        "failed to allocate a unique copy name for '{}'",
        src.name
    )))
}

/// 复制文件夹（递归复制所有文件和子文件夹）
///
/// `dest_parent_id = None` 表示复制到根目录。
pub async fn copy_folder(
    state: &PrimaryAppState,
    src_id: i64,
    user_id: i64,
    dest_parent_id: Option<i64>,
) -> Result<FolderInfo> {
    copy_folder_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        src_id,
        dest_parent_id,
    )
    .await
    .map(Into::into)
}
