//! 分享服务的聚合入口。
//!
//! 这里把“管理侧 share CRUD”和“公开 token 访问内容”两条路径收在同一模块下，
//! 但故意拆成不同子模块：管理逻辑走已登录 scope 校验，公开访问逻辑只认分享本身
//! 的状态与范围，不复用内部登录态。

mod access;
mod cache;
mod content;
mod management;
mod models;
mod shared;

use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::audit_service::{self, AuditContext};
use crate::services::batch_service;
use crate::services::workspace_storage_service::WorkspaceStorageScope;

pub use access::{
    PasswordVerified, check_share_password_cookie, get_share_avatar_bytes, get_share_info,
    sign_share_cookie, verify_password, verify_password_and_sign, verify_share_cookie,
};
pub use content::{
    ShareDownloadRollbackQueue, ShareDownloadRollbackWorker, build_share_download_rollback_queue,
    download_shared_file, download_shared_folder_file, get_shared_folder_file_thumbnail,
    get_shared_thumbnail, list_shared_folder, list_shared_subfolder,
    share_download_rollback_worker_task, spawn_detached_share_download_rollback_queue,
};
pub use management::{
    admin_delete_share, batch_delete_shares, batch_delete_team_shares, create_share, delete_share,
    delete_team_share, list_my_shares_paginated, list_paginated, list_team_shares_paginated,
    update_share, update_team_share, validate_batch_share_ids,
};
pub use models::{
    MyShareInfo, ShareInfo, SharePublicInfo, SharePublicOwnerInfo, ShareStatus, ShareTarget,
};

pub(crate) use access::check_share_password_cookie_ignoring_download_limit;
pub(crate) use cache::{
    find_active_file_ids_in_resource_scope, find_active_file_ids_in_scope,
    find_active_folder_ids_in_resource_scope, find_active_folder_ids_in_scope,
    invalidate_active_share_target_cache_for_resource_scope,
    invalidate_active_share_target_cache_for_scope, invalidate_all_share_token_record_cache,
};
pub(crate) use content::{download_shared_file_with_range, download_shared_folder_file_with_range};
pub(crate) use content::{
    load_preview_shared_file, load_preview_shared_folder_file,
    load_shared_file_ignoring_download_limit, load_shared_folder_file_ignoring_download_limit,
};
pub(crate) use management::{
    batch_delete_shares_in_scope, create_share_in_scope, delete_share_in_scope,
    list_shares_paginated_in_scope, update_share_in_scope,
};

// audit 包装放在入口层，而不是塞进 management 核心逻辑里。
// 这样基础 share service 仍然可以在测试和其他内部流程里被纯粹复用。
pub(crate) async fn create_share_in_scope_with_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    target: ShareTarget,
    password: Option<String>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    max_downloads: i64,
    audit_ctx: &AuditContext,
) -> Result<ShareInfo> {
    let share =
        create_share_in_scope(state, scope, target, password, expires_at, max_downloads).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::ShareCreate,
        None,
        Some(share.id),
        None,
        None,
    )
    .await;
    Ok(share)
}

pub(crate) async fn update_share_in_scope_with_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    share_id: i64,
    password: Option<String>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    max_downloads: i64,
    audit_ctx: &AuditContext,
) -> Result<ShareInfo> {
    let outcome =
        update_share_in_scope(state, scope, share_id, password, expires_at, max_downloads).await?;
    let share = outcome.share;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::ShareUpdate,
        Some("share"),
        Some(share.id),
        Some(&share.token),
        audit_service::details(audit_service::ShareUpdateDetails {
            has_password: outcome.has_password,
            expires_at: share.expires_at,
            max_downloads: share.max_downloads,
        }),
    )
    .await;
    Ok(share)
}

pub(crate) async fn delete_share_in_scope_with_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    share_id: i64,
    audit_ctx: &AuditContext,
) -> Result<()> {
    delete_share_in_scope(state, scope, share_id).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::ShareDelete,
        None,
        Some(share_id),
        None,
        None,
    )
    .await;
    Ok(())
}

pub(crate) async fn batch_delete_shares_in_scope_with_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    share_ids: &[i64],
    audit_ctx: &AuditContext,
) -> Result<batch_service::BatchResult> {
    validate_batch_share_ids(share_ids)?;
    let result = batch_delete_shares_in_scope(state, scope, share_ids).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::ShareBatchDelete,
        None,
        None,
        None,
        audit_service::details(audit_service::ShareBatchDeleteDetails {
            share_ids,
            succeeded: result.succeeded,
            failed: result.failed,
        }),
    )
    .await;
    Ok(result)
}
