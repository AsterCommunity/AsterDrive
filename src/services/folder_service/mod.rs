//! 文件夹服务聚合入口。
//!
//! 目录相关功能被拆成几块：
//! - access: scope / share 边界
//! - listing: 目录列表
//! - mutation: 新建、重命名、移动、删除
//! - tree: 递归收集整棵子树
//! - hierarchy: breadcrumb / ancestor

mod access;
mod copy;
mod hierarchy;
mod listing;
mod models;
mod mutation;
mod tree;

use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::audit_service::{self, AuditContext};
use crate::services::workspace_models::FolderInfo;
use crate::services::workspace_storage_service::WorkspaceStorageScope;
use crate::types::NullablePatch;

pub use access::verify_folder_access;
pub use copy::copy_folder;
pub use hierarchy::{build_folder_paths, build_folder_paths_cached, get_ancestors};
pub use listing::{FolderListParams, list, list_shared};
pub use models::{
    FileCursor, FileListItem, FolderAncestorItem, FolderContents, FolderListItem,
    build_file_list_items, build_folder_list_items,
};
pub use mutation::{create, delete, move_folder, set_lock, update};

pub(crate) use access::{
    ensure_folder_model_in_scope, ensure_personal_folder_scope, verify_folder_in_scope,
};
pub(crate) use copy::{copy_folder_in_scope, copy_folder_tree_in_scope};
pub(crate) use hierarchy::{
    FOLDER_PATH_CACHE_PREFIX, get_ancestors_in_scope, invalidate_folder_path_cache,
};
pub(crate) use listing::list_in_scope;
pub(crate) use mutation::{
    create_in_scope, delete_in_scope, get_info_with_storage_used_in_scope, set_lock_in_scope,
    update_in_scope,
};
pub(crate) use tree::{
    collect_folder_forest_in_resource_scope, collect_folder_forest_in_scope,
    collect_folder_tree_in_scope,
};

// 和其他 service 一样，审计包装留在聚合层，避免核心目录逻辑被日志副作用污染。
pub(crate) async fn create_in_scope_with_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    name: &str,
    parent_id: Option<i64>,
    audit_ctx: &AuditContext,
) -> Result<FolderInfo> {
    let folder = create_in_scope(state, scope, name, parent_id).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::FolderCreate,
        crate::services::audit_service::AuditEntityType::Folder,
        Some(folder.id),
        Some(&folder.name),
        None,
    )
    .await;
    Ok(folder.into())
}

pub(crate) async fn delete_in_scope_with_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_id: i64,
    audit_ctx: &AuditContext,
) -> Result<()> {
    delete_in_scope(state, scope, folder_id).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::FolderDelete,
        crate::services::audit_service::AuditEntityType::Folder,
        Some(folder_id),
        None,
        None,
    )
    .await;
    Ok(())
}

pub(crate) async fn update_in_scope_with_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_id: i64,
    name: Option<String>,
    parent_id: NullablePatch<i64>,
    policy_id: NullablePatch<i64>,
    audit_ctx: &AuditContext,
) -> Result<FolderInfo> {
    let action = if parent_id.is_present() {
        audit_service::AuditAction::FolderMove
    } else if policy_id.is_present() {
        audit_service::AuditAction::FolderPolicyChange
    } else {
        audit_service::AuditAction::FolderRename
    };
    let folder = update_in_scope(state, scope, folder_id, name, parent_id, policy_id).await?;
    audit_service::log(
        state,
        audit_ctx,
        action,
        crate::services::audit_service::AuditEntityType::Folder,
        Some(folder.id),
        Some(&folder.name),
        None,
    )
    .await;
    Ok(folder.into())
}

pub(crate) async fn set_lock_in_scope_with_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_id: i64,
    locked: bool,
    audit_ctx: &AuditContext,
) -> Result<FolderInfo> {
    let folder = set_lock_in_scope(state, scope, folder_id, locked).await?;
    audit_service::log(
        state,
        audit_ctx,
        if locked {
            audit_service::AuditAction::FolderLock
        } else {
            audit_service::AuditAction::FolderUnlock
        },
        crate::services::audit_service::AuditEntityType::Folder,
        Some(folder.id),
        Some(&folder.name),
        None,
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
    let folder = copy_folder_in_scope(state, scope, folder_id, parent_id).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::FolderCopy,
        crate::services::audit_service::AuditEntityType::Folder,
        Some(folder.id),
        Some(&folder.name),
        None,
    )
    .await;
    Ok(folder.into())
}
