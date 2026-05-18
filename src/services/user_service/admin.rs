use chrono::Utc;
use sea_orm::{ActiveModelTrait, Set};

use crate::db::repository::{
    auth_session_repo, file_repo, folder_repo, lock_repo, share_repo, upload_session_repo,
    user_repo, webdav_account_repo,
};
use crate::entities::user;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::services::{
    audit_service::{self, AuditContext},
    auth_service, profile_service,
};
use crate::types::{UserRole, UserStatus};

use super::queries::{get, to_user_info};

#[derive(Debug, Clone)]
pub struct ForceDeleteSummary {
    pub user_id: i64,
    pub username: String,
    pub file_count: usize,
    pub folder_count: usize,
    pub share_count: u64,
    pub webdav_account_count: u64,
    pub upload_session_count: u64,
    pub lock_count: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct UpdateUserInput {
    pub id: i64,
    pub email_verified: Option<bool>,
    pub role: Option<UserRole>,
    pub status: Option<UserStatus>,
    pub storage_quota: Option<i64>,
    pub policy_group_id: Option<i64>,
}

pub async fn create(
    state: &PrimaryAppState,
    username: &str,
    email: &str,
    password: &str,
) -> Result<super::models::UserInfo> {
    let user = auth_service::create_user_by_admin(state, username, email, password).await?;
    get(state, user.id).await
}

pub async fn create_with_audit(
    state: &PrimaryAppState,
    username: &str,
    email: &str,
    password: &str,
    audit_ctx: &AuditContext,
) -> Result<super::models::UserInfo> {
    let user = create(state, username, email, password).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminCreateUser,
        crate::services::audit_service::AuditEntityType::User,
        Some(user.id),
        Some(&user.username),
        audit_service::details(audit_service::AdminCreateUserDetails {
            email: &user.email,
            email_verified: user.email_verified,
            role: user.role,
            status: user.status,
            storage_quota: user.storage_quota,
            policy_group_id: user.policy_group_id,
        }),
    )
    .await;
    Ok(user)
}

pub async fn update(
    state: &PrimaryAppState,
    input: UpdateUserInput,
) -> Result<super::models::UserInfo> {
    let UpdateUserInput {
        id,
        email_verified,
        role,
        status,
        storage_quota,
        policy_group_id,
    } = input;
    if id == 1 {
        if let Some(ref status) = status
            && !status.is_active()
        {
            return Err(AsterError::validation_error(
                "cannot disable the initial admin account",
            ));
        }
        if let Some(ref role) = role
            && !role.is_admin()
        {
            return Err(AsterError::validation_error(
                "cannot demote the initial admin account",
            ));
        }
        if email_verified == Some(false) {
            return Err(AsterError::validation_error(
                "cannot unverify the initial admin account",
            ));
        }
    }

    let existing = user_repo::find_by_id(&state.db, id).await?;
    let existing_policy_group_id = existing.policy_group_id;
    let existing_email_verified = auth_service::is_email_verified(&existing);
    let email_verified_changed =
        email_verified.is_some_and(|value| value != existing_email_verified);
    let role_changed = role.is_some_and(|value| value != existing.role);
    let status_changed = status.is_some_and(|value| value != existing.status);
    let policy_group_changed =
        policy_group_id.is_some_and(|group_id| existing_policy_group_id != Some(group_id));
    let current_session_version = existing.session_version;
    let mut active: user::ActiveModel = existing.into();
    if let Some(is_verified) = email_verified
        && is_verified != existing_email_verified
    {
        active.email_verified_at = Set(is_verified.then_some(Utc::now()));
    }
    if let Some(role) = role {
        active.role = Set(role);
    }
    if let Some(status) = status {
        active.status = Set(status);
    }
    if let Some(storage_quota) = storage_quota {
        active.storage_quota = Set(storage_quota);
    }
    if let Some(group_id) = policy_group_id {
        let group =
            crate::db::repository::policy_group_repo::find_group_by_id(&state.db, group_id).await?;
        if !group.is_enabled {
            return Err(AsterError::validation_error(
                "cannot assign a disabled storage policy group",
            ));
        }
        let items =
            crate::db::repository::policy_group_repo::find_group_items(&state.db, group_id).await?;
        if items.is_empty() {
            return Err(AsterError::validation_error(
                "cannot assign a storage policy group without policies",
            ));
        }
        active.policy_group_id = Set(Some(group_id));
    }
    if status_changed || email_verified_changed {
        active.session_version = Set(current_session_version.saturating_add(1));
    }
    active.updated_at = Set(Utc::now());
    let txn = crate::db::transaction::begin(&state.db).await?;
    let result = async {
        let updated = active
            .update(&txn)
            .await
            .map_aster_err(AsterError::database_operation)?;
        if status_changed || email_verified_changed {
            auth_session_repo::delete_all_for_user(&txn, updated.id).await?;
        }
        Ok::<_, AsterError>(updated)
    }
    .await;
    let updated = match result {
        Ok(updated) => {
            crate::db::transaction::commit(txn).await?;
            updated
        }
        Err(error) => {
            crate::db::transaction::rollback(txn).await?;
            return Err(error);
        }
    };
    if policy_group_changed {
        if let Some(policy_group_id) = updated.policy_group_id {
            state
                .policy_snapshot
                .set_user_policy_group(updated.id, policy_group_id);
        } else {
            state.policy_snapshot.remove_user_policy_group(updated.id);
        }
    }
    if role_changed || status_changed || email_verified_changed {
        auth_service::invalidate_auth_snapshot_cache(state, id).await;
    }
    if status_changed
        && let Err(error) = crate::webdav::auth::invalidate_webdav_auth_for_user(state, id).await
    {
        tracing::warn!(
            user_id = id,
            "failed to invalidate WebDAV auth cache after user status update: {error}"
        );
    }
    to_user_info(state, &updated, profile_service::AvatarAudience::AdminUser).await
}

pub async fn update_with_audit(
    state: &PrimaryAppState,
    input: UpdateUserInput,
    audit_ctx: &AuditContext,
) -> Result<super::models::UserInfo> {
    let user = update(state, input).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminUpdateUser,
        crate::services::audit_service::AuditEntityType::User,
        Some(user.id),
        Some(&user.username),
        audit_service::details(audit_service::AdminUpdateUserDetails {
            email_verified: user.email_verified,
            role: user.role,
            status: user.status,
            storage_quota: user.storage_quota,
            policy_group_id: user.policy_group_id,
        }),
    )
    .await;
    Ok(user)
}

/// 强制删除用户及其所有数据（不可逆）
///
/// 级联清理顺序：
/// 1. 删除所有分享链接
/// 2. 永久删除所有文件（blob cleanup + 版本 + 缩略图 + 属性）
/// 3. 删除所有文件夹（+ 属性）
/// 4. 删除所有 WebDAV 账号
/// 5. 删除头像上传对象
/// 6. 删除用户存储策略分配
/// 7. 清理上传 session 和临时文件
/// 8. 清理资源锁
/// 9. 删除用户记录
pub async fn force_delete(
    state: &PrimaryAppState,
    target_user_id: i64,
) -> Result<ForceDeleteSummary> {
    let db = &state.db;
    let user = user_repo::find_by_id(db, target_user_id).await?;

    if target_user_id == 1 {
        return Err(AsterError::validation_error(
            "cannot delete the initial admin account",
        ));
    }

    if user.role.is_admin() {
        return Err(AsterError::validation_error(
            "cannot force-delete an admin user, demote to user first",
        ));
    }

    tracing::warn!(
        "force-deleting user #{} ({}), cascading all data",
        user.id,
        user.username
    );

    let share_count = share_repo::delete_all_by_user(db, target_user_id).await?;
    if share_count > 0 {
        crate::services::share_service::invalidate_active_share_target_cache_for_scope(
            state,
            crate::services::workspace_storage_service::WorkspaceStorageScope::Personal {
                user_id: target_user_id,
            },
        )
        .await;
        crate::services::share_service::invalidate_all_share_token_record_cache(state).await;
    }

    let all_files = file_repo::find_all_by_user(db, target_user_id).await?;
    let file_count = all_files.len();
    crate::services::file_service::batch_purge(state, all_files, target_user_id).await?;

    let all_folders = folder_repo::find_all_by_user(db, target_user_id).await?;
    let folder_count = all_folders.len();
    let folder_ids: Vec<i64> = all_folders.iter().map(|folder| folder.id).collect();
    crate::db::repository::property_repo::delete_all_for_entities(
        db,
        crate::types::EntityType::Folder,
        &folder_ids,
    )
    .await?;
    folder_repo::delete_many(db, &folder_ids).await?;
    crate::services::folder_service::invalidate_folder_path_cache(state).await;

    crate::webdav::auth::invalidate_webdav_auth_for_user(state, target_user_id).await?;
    let webdav_account_count = webdav_account_repo::delete_all_by_user(db, target_user_id).await?;

    if let Err(error) = profile_service::cleanup_avatar_upload(state, target_user_id).await {
        tracing::warn!("cleanup avatar upload for user #{target_user_id} failed: {error}");
    }

    let upload_session_count = upload_session_repo::delete_all_by_user(db, target_user_id).await?;

    let locks = lock_repo::find_by_owner(db, target_user_id).await?;
    for lock in &locks {
        if let Err(error) = crate::services::lock_service::set_entity_locked(
            db,
            lock.entity_type,
            lock.entity_id,
            false,
        )
        .await
        {
            tracing::warn!(
                lock_id = lock.id,
                "failed to unlock during user cleanup: {error}"
            );
        }
    }
    let lock_count = lock_repo::delete_all_by_owner(db, target_user_id).await?;

    user_repo::delete(db, target_user_id).await?;
    state
        .policy_snapshot
        .remove_user_policy_group(target_user_id);

    tracing::info!(
        "force-deleted user #{} ({}) and all associated data ({} files, {} folders)",
        user.id,
        user.username,
        file_count,
        folder_count,
    );

    Ok(ForceDeleteSummary {
        user_id: user.id,
        username: user.username,
        file_count,
        folder_count,
        share_count,
        webdav_account_count,
        upload_session_count,
        lock_count,
    })
}

pub async fn force_delete_with_audit(
    state: &PrimaryAppState,
    target_user_id: i64,
    audit_ctx: &AuditContext,
) -> Result<ForceDeleteSummary> {
    let summary = force_delete(state, target_user_id).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminForceDeleteUser,
        crate::services::audit_service::AuditEntityType::User,
        Some(summary.user_id),
        Some(&summary.username),
        audit_service::details(audit_service::AdminForceDeleteUserDetails {
            file_count: summary.file_count,
            folder_count: summary.folder_count,
            share_count: summary.share_count,
            webdav_account_count: summary.webdav_account_count,
            upload_session_count: summary.upload_session_count,
            lock_count: summary.lock_count,
        }),
    )
    .await;
    Ok(summary)
}
