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

use crate::entities::folder;
use crate::errors::AsterError;
use crate::errors::Result;
use crate::runtime::SharedRuntimeState;
use crate::runtime::{PrimaryAppState, StorageChangeRuntimeState};
use crate::services::ops::audit::{self, AuditContext};
use crate::services::workspace::models::FolderInfo;
use crate::services::workspace::storage::WorkspaceStorageScope;
use crate::types::NullablePatch;
use serde_json::json;

pub use access::verify_folder_access;
pub use copy::copy_folder;
pub use hierarchy::{build_folder_paths, build_folder_paths_cached, get_ancestors};
pub use listing::{FolderListParams, list, list_shared};
pub use models::{
    FileCursor, FileListItem, FolderAncestorItem, FolderContents, FolderListItem,
    build_file_list_items, build_file_list_items_with_tags, build_folder_list_items,
    build_folder_list_items_with_tags,
};
pub use mutation::{create, delete, move_folder, set_lock, update};

pub(crate) use access::{
    ensure_folder_model_in_scope, ensure_personal_folder_scope, verify_folder_in_scope,
};
pub(crate) use cache::{FOLDER_PATH_CACHE_PREFIX, folder_path_cache_key};
pub(crate) use copy::{
    copy_folder_between_scopes, copy_folder_in_scope, copy_folder_tree_in_scope,
};
pub(crate) use hierarchy::{
    get_ancestors_in_scope, invalidate_folder_path_cache, invalidate_folder_path_cache_for_ids,
};
pub(crate) use listing::list_in_scope;
pub(crate) use mutation::{
    admin_set_policy, create_in_scope, delete_in_scope, get_info_in_scope,
    get_info_with_storage_used_in_scope, set_lock_in_scope, update_in_scope,
};
pub(crate) use tree::{
    collect_folder_forest_in_resource_scope, collect_folder_forest_in_scope,
    collect_folder_tree_in_resource_scope, collect_folder_tree_in_scope,
};

// 和其他 service 一样，审计包装留在聚合层，避免核心目录逻辑被日志副作用污染。
pub(crate) async fn create_in_scope_with_audit(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    name: &str,
    parent_id: Option<i64>,
    audit_ctx: &AuditContext,
) -> Result<FolderInfo> {
    let folder = create_in_scope(state, scope, name, parent_id).await?;
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
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    folder_id: i64,
    audit_ctx: &AuditContext,
) -> Result<()> {
    let folder = get_info_in_scope(state, scope, folder_id).await?;
    let details = audit_location_details_for_model(state, scope, &folder).await;
    delete_in_scope(state, scope, folder_id).await?;
    audit::log_with_details(
        state,
        audit_ctx,
        audit::AuditAction::FolderDelete,
        crate::services::ops::audit::AuditEntityType::Folder,
        Some(folder_id),
        Some(&folder.name),
        || details.clone(),
    )
    .await;
    Ok(())
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
    Ok(folder.into())
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
    Ok(folder.into())
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
