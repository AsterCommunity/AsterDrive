//! 文件夹服务聚合入口。
//!
//! 目录相关功能被拆成几块：
//! - access: scope / share 边界
//! - listing: 目录列表
//! - mutation: 新建、重命名、移动、删除
//! - tree: 递归收集整棵子树
//! - hierarchy: breadcrumb / ancestor

mod access;
mod cache;
mod copy;
mod hierarchy;
mod listing;
mod models;
mod mutation;
mod tree;

use crate::errors::AsterError;
use crate::errors::Result;
use crate::runtime::SharedRuntimeState;
use crate::runtime::{PrimaryAppState, StorageChangeRuntimeState};
use crate::services::ops::audit::{self, AuditContext};
use crate::services::task::types::TaskInfo;
use crate::services::workspace::models::FolderInfo;
use crate::services::workspace::storage::WorkspaceStorageScope;
use aster_drive_model::entities::folder;
use aster_forge_api::NullablePatch;
use serde_json::json;

pub use access::verify_folder_access;
pub use copy::copy_folder;
pub use hierarchy::{build_folder_paths, build_folder_paths_cached, get_ancestors};
pub use listing::{FolderListParams, list, list_shared};
pub use models::{
    FileCursor, FileListItem, FolderAncestorItem, FolderContents, FolderListItem,
    build_file_list_items, build_file_list_items_with_tags,
    build_file_list_items_with_tags_and_lock_states, build_folder_list_items,
    build_folder_list_items_with_tags, build_folder_list_items_with_tags_and_lock_states,
};
pub use mutation::{create, delete, move_folder, set_lock, update};
pub use tree::{
    REST_FOLDER_TREE_SYNCHRONOUS_MAXIMUM_DEPTH, REST_FOLDER_TREE_SYNCHRONOUS_MAXIMUM_FRONTIER,
    REST_FOLDER_TREE_SYNCHRONOUS_MAXIMUM_RESOURCES,
};

pub(crate) use access::{
    ensure_folder_model_in_scope, ensure_personal_folder_scope, verify_folder_in_scope,
};
pub(crate) use cache::{FOLDER_PATH_CACHE_PREFIX, folder_path_cache_key};
#[cfg(test)]
pub(crate) use copy::copy_folder_tree_in_scope;
pub(crate) use copy::{
    copy_folder_between_scopes, copy_folder_in_scope,
    copy_folder_tree_in_scope_with_user_properties,
};
pub(crate) use hierarchy::{
    get_ancestors_in_scope, invalidate_folder_path_cache, invalidate_folder_path_cache_for_ids,
};
pub(crate) use listing::list_in_scope;
pub(crate) use mutation::{
    FolderTreeDeletion, admin_set_policy, apply_locked_tree_deletion_on, create_in_scope,
    delete_in_scope, get_info_in_scope, get_info_with_storage_used_in_scope,
    lock_tree_for_deletion_on, set_lock_in_scope, update_in_scope,
};
pub(crate) use tree::{
    FOLDER_TREE_RESOURCE_LIMIT_MESSAGE, FolderTreeTraversalLimits, REST_FOLDER_TREE_LIMITS,
    collect_folder_forest_in_resource_scope, collect_folder_tree_in_resource_scope,
    collect_folder_tree_in_scope,
};

pub(crate) enum FolderTreeMutationDispatch {
    Completed,
    Queued(Box<TaskInfo>),
}

// 和其他 service 一样，审计包装留在聚合层，避免核心目录逻辑被日志副作用污染。
pub(crate) async fn create_in_scope_with_audit(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    name: &str,
    parent_id: Option<i64>,
    lock_credentials: crate::services::files::lock::LockMutationCredentials,
    audit_ctx: &AuditContext,
) -> Result<FolderInfo> {
    let folder = create_in_scope(state, scope, name, parent_id, lock_credentials).await?;
    let details = audit_location_details_for_model(state, scope, &folder).await;
    audit::log_with_details(
        state,
        audit_ctx,
        audit::AuditAction::FolderCreate,
        crate::services::ops::audit::AuditEntityType::Folder,
        Some(folder.id),
        Some(&folder.name),
        || details.clone(),
    )
    .await;
    Ok(folder.into())
}

pub(crate) async fn delete_in_scope_with_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_id: i64,
    audit_ctx: &AuditContext,
) -> Result<FolderTreeMutationDispatch> {
    let folder = get_info_in_scope(state, scope, folder_id).await?;
    let mut details = audit_location_details_for_model(state, scope, &folder)
        .await
        .unwrap_or_else(|| json!({}));
    let outcome =
        match delete_in_scope(state, scope, folder_id, Some(REST_FOLDER_TREE_LIMITS)).await {
            Ok(()) => FolderTreeMutationDispatch::Completed,
            Err(AsterError::OperationResourceLimitExceeded(_)) => {
                FolderTreeMutationDispatch::Queued(Box::new(
                    crate::services::task::folder_tree::create_folder_tree_mutation_task_in_scope(
                        state,
                        scope,
                        folder_id,
                        crate::services::task::types::FolderTreeMutationOperation::Delete,
                    )
                    .await?,
                ))
            }
            Err(error) => return Err(error),
        };
    let details_object = folder_delete_audit_details_object(&mut details)?;
    match &outcome {
        FolderTreeMutationDispatch::Completed => {
            details_object.insert("dispatch".to_string(), json!("completed"));
        }
        FolderTreeMutationDispatch::Queued(task) => {
            details_object.insert("dispatch".to_string(), json!("queued"));
            details_object.insert("task_id".to_string(), json!(task.id));
        }
    }
    audit::log_with_details(
        state,
        audit_ctx,
        audit::AuditAction::FolderDelete,
        crate::services::ops::audit::AuditEntityType::Folder,
        Some(folder_id),
        Some(&folder.name),
        || Some(details.clone()),
    )
    .await;
    Ok(outcome)
}

fn folder_delete_audit_details_object(
    details: &mut serde_json::Value,
) -> Result<&mut serde_json::Map<String, serde_json::Value>> {
    details.as_object_mut().ok_or_else(|| {
        AsterError::internal_error("folder audit location details must be a JSON object")
    })
}

pub(crate) async fn update_in_scope_with_audit(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    folder_id: i64,
    name: Option<String>,
    parent_id: NullablePatch<i64>,
    policy_id: NullablePatch<i64>,
    audit_ctx: &AuditContext,
) -> Result<FolderInfo> {
    let action = if parent_id.is_present() {
        audit::AuditAction::FolderMove
    } else if policy_id.is_present() {
        audit::AuditAction::FolderPolicyChange
    } else {
        audit::AuditAction::FolderRename
    };
    let previous_folder = get_info_in_scope(state, scope, folder_id).await?;
    let original_source_path = if matches!(
        action,
        audit::AuditAction::FolderMove | audit::AuditAction::FolderRename
    ) {
        Some(folder_path_for_audit(state, previous_folder.id).await)
    } else {
        None
    };
    let folder = update_in_scope(state, scope, folder_id, name, parent_id, policy_id).await?;
    let details = if matches!(action, audit::AuditAction::FolderPolicyChange) {
        audit::details(audit::FolderPolicyAuditDetails {
            previous_policy_id: previous_folder.policy_id,
            policy_id: folder.policy_id,
        })
    } else if let Some(original_source_path) = original_source_path {
        audit_transfer_details_for_models_with_source_path(
            state,
            scope,
            &previous_folder,
            original_source_path,
            &folder,
        )
        .await
    } else {
        audit_transfer_details_for_models(state, scope, &previous_folder, &folder).await
    };
    audit::log_with_details(
        state,
        audit_ctx,
        action,
        crate::services::ops::audit::AuditEntityType::Folder,
        Some(folder.id),
        Some(&folder.name),
        || details.clone(),
    )
    .await;
    let lock_states = crate::services::files::lock::load_for_scope(
        state,
        scope.into(),
        &[],
        std::slice::from_ref(&folder),
    )
    .await?;
    Ok(
        FolderInfo::from(folder).with_lock_state(crate::services::files::lock::state_for(
            &lock_states,
            aster_drive_model::types::EntityType::Folder,
            folder_id,
        )),
    )
}

pub async fn admin_set_policy_with_audit(
    state: &impl StorageChangeRuntimeState,
    folder_id: i64,
    policy_id: Option<i64>,
    audit_ctx: &AuditContext,
) -> Result<FolderInfo> {
    let (folder, previous_policy_id) = admin_set_policy(state, folder_id, policy_id).await?;
    audit::log_with_details(
        state,
        audit_ctx,
        audit::AuditAction::FolderPolicyChange,
        crate::services::ops::audit::AuditEntityType::Folder,
        Some(folder.id),
        Some(&folder.name),
        || {
            audit::details(audit::FolderPolicyAuditDetails {
                previous_policy_id,
                policy_id: folder.policy_id,
            })
        },
    )
    .await;
    Ok(folder.into())
}

pub(crate) async fn set_lock_in_scope_with_audit(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    folder_id: i64,
    locked: bool,
    audit_ctx: &AuditContext,
) -> Result<FolderInfo> {
    let folder = set_lock_in_scope(state, scope, folder_id, locked).await?;
    let details = audit_location_details_for_model(state, scope, &folder).await;
    audit::log_with_details(
        state,
        audit_ctx,
        if locked {
            audit::AuditAction::FolderLock
        } else {
            audit::AuditAction::FolderUnlock
        },
        crate::services::ops::audit::AuditEntityType::Folder,
        Some(folder.id),
        Some(&folder.name),
        || details.clone(),
    )
    .await;
    let lock_states = crate::services::files::lock::load_for_scope(
        state,
        scope.into(),
        &[],
        std::slice::from_ref(&folder),
    )
    .await?;
    Ok(
        FolderInfo::from(folder).with_lock_state(crate::services::files::lock::state_for(
            &lock_states,
            aster_drive_model::types::EntityType::Folder,
            folder_id,
        )),
    )
}

pub(crate) async fn copy_folder_in_scope_with_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_id: i64,
    parent_id: Option<i64>,
    audit_ctx: &AuditContext,
) -> Result<FolderInfo> {
    let source_folder = get_info_in_scope(state, scope, folder_id).await?;
    let folder = copy_folder_in_scope(state, scope, folder_id, parent_id).await?;
    let details = audit_transfer_details_for_models(state, scope, &source_folder, &folder).await;
    audit::log_with_details(
        state,
        audit_ctx,
        audit::AuditAction::FolderCopy,
        crate::services::ops::audit::AuditEntityType::Folder,
        Some(folder.id),
        Some(&folder.name),
        || details.clone(),
    )
    .await;
    Ok(folder.into())
}

pub(crate) async fn audit_location_details_for_model(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    folder: &folder::Model,
) -> Option<serde_json::Value> {
    match folder_path_for_audit(state, folder.id).await {
        Ok(path) => Some(json!({
            "parent_id": folder.parent_id,
            "path": path,
            "team_id": scope_team_id(scope),
        })),
        Err(error) => {
            tracing::warn!(
                folder_id = folder.id,
                "failed to build folder audit location details: {error}"
            );
            None
        }
    }
}

pub(crate) async fn audit_transfer_details_for_models(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    source_folder: &folder::Model,
    target_folder: &folder::Model,
) -> Option<serde_json::Value> {
    audit_transfer_details_for_models_with_source_path(
        state,
        scope,
        source_folder,
        folder_path_for_audit(state, source_folder.id).await,
        target_folder,
    )
    .await
}

async fn audit_transfer_details_for_models_with_source_path(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    source_folder: &folder::Model,
    source_path: Result<String>,
    target_folder: &folder::Model,
) -> Option<serde_json::Value> {
    let source_path = match source_path {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!(
                folder_id = source_folder.id,
                "failed to build source folder audit path: {error}"
            );
            return None;
        }
    };
    let target_path = match folder_path_for_audit(state, target_folder.id).await {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!(
                folder_id = target_folder.id,
                "failed to build target folder audit path: {error}"
            );
            return None;
        }
    };
    Some(json!({
        "source_parent_id": source_folder.parent_id,
        "source_path": source_path,
        "target_parent_id": target_folder.parent_id,
        "target_path": target_path,
        "previous_name": source_folder.name,
        "next_name": target_folder.name,
        "team_id": scope_team_id(scope),
    }))
}

async fn folder_path_for_audit(state: &impl SharedRuntimeState, folder_id: i64) -> Result<String> {
    let mut paths = build_folder_paths(state.reader_db(), &[folder_id]).await?;
    paths
        .remove(&folder_id)
        .ok_or_else(|| AsterError::record_not_found(format!("folder #{folder_id} audit path")))
}

fn scope_team_id(scope: WorkspaceStorageScope) -> Option<i64> {
    match scope {
        WorkspaceStorageScope::Personal { .. } => None,
        WorkspaceStorageScope::Team { team_id, .. } => Some(team_id),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn folder_delete_audit_details_reject_non_object_values() {
        let mut details = serde_json::json!(["unexpected"]);

        let error = super::folder_delete_audit_details_object(&mut details)
            .expect_err("folder delete audit details must remain an object");

        assert!(error.message().contains("must be a JSON object"));
    }
}
