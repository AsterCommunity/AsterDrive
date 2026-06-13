use chrono::Utc;
use rand::RngExt;
use sea_orm::{ActiveModelTrait, Set};
use serde::Serialize;

use crate::db::repository::{
    auth_session_repo, file_repo, folder_repo, lock_repo, share_repo, upload_session_repo,
    user_repo, webdav_account_repo,
};
use crate::entities::user;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{
    audit_service::{self, AuditContext},
    auth_service, profile_service,
};
use crate::types::{UserRole, UserStatus};

use super::queries::{get, to_user_info};

const GENERATED_PASSWORD_LENGTH: usize = 24;
const GENERATED_PASSWORD_UPPERCASE: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";
const GENERATED_PASSWORD_LOWERCASE: &[u8] = b"abcdefghijkmnopqrstuvwxyz";
const GENERATED_PASSWORD_DIGITS: &[u8] = b"23456789";
const GENERATED_PASSWORD_SYMBOLS: &[u8] = b"!@#$%^&*-_+=";
const GENERATED_PASSWORD_CHARSET: &[u8] =
    b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789!@#$%^&*-_+=";

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
    pub must_change_password: Option<bool>,
    pub storage_quota: Option<i64>,
    pub policy_group_id: Option<i64>,
}

#[derive(Debug, Clone, Copy)]
struct UserAuditSnapshot {
    email_verified: bool,
    role: UserRole,
    status: UserStatus,
    must_change_password: bool,
    storage_quota: i64,
    policy_group_id: Option<i64>,
}

#[derive(Debug, Clone, Copy)]
struct UpdateAuditDiff {
    before: UserAuditSnapshot,
    after: UserAuditSnapshot,
}

#[derive(Debug, Clone, Copy)]
pub struct CreateUserInput<'a> {
    pub username: &'a str,
    pub email: &'a str,
    pub password: Option<&'a str>,
    pub must_change_password: Option<bool>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct CreateUserOutput {
    pub user: super::models::UserInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_password: Option<String>,
}

fn generate_temporary_password() -> String {
    let mut rng = rand::rng();
    let mut bytes = Vec::with_capacity(GENERATED_PASSWORD_LENGTH);
    for charset in [
        GENERATED_PASSWORD_UPPERCASE,
        GENERATED_PASSWORD_LOWERCASE,
        GENERATED_PASSWORD_DIGITS,
        GENERATED_PASSWORD_SYMBOLS,
    ] {
        let index = rng.random_range(0..charset.len());
        bytes.push(charset[index]);
    }
    while bytes.len() < GENERATED_PASSWORD_LENGTH {
        let index = rng.random_range(0..GENERATED_PASSWORD_CHARSET.len());
        bytes.push(GENERATED_PASSWORD_CHARSET[index]);
    }
    for index in (1..bytes.len()).rev() {
        let swap_index = rng.random_range(0..=index);
        bytes.swap(index, swap_index);
    }
    bytes.into_iter().map(|byte| byte as char).collect()
}
pub async fn create(
    state: &impl SharedRuntimeState,
    input: CreateUserInput<'_>,
) -> Result<CreateUserOutput> {
    let explicit_password = input.password.filter(|value| !value.trim().is_empty());
    let generated_password = explicit_password
        .is_none()
        .then(generate_temporary_password);
    let password = generated_password
        .as_deref()
        .or(explicit_password)
        .ok_or_else(|| {
            AsterError::internal_error("temporary password generation returned no password")
        })?;
    auth_service::validate_password(password)?;
    let must_change_password =
        generated_password.is_some() || input.must_change_password.unwrap_or(false);
    let user = auth_service::create_user_by_admin(
        state,
        input.username,
        input.email,
        password,
        must_change_password,
    )
    .await?;
    Ok(CreateUserOutput {
        user: get(state, user.id).await?,
        generated_password,
    })
}

pub async fn create_with_audit(
    state: &impl SharedRuntimeState,
    input: CreateUserInput<'_>,
    audit_ctx: &AuditContext,
) -> Result<CreateUserOutput> {
    let output = create(state, input).await?;
    let user = &output.user;
    let temporary_password_generated = output.generated_password.is_some();
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminCreateUser,
        crate::services::audit_service::AuditEntityType::User,
        Some(user.id),
        Some(&user.username),
        || {
            audit_service::details(audit_service::AdminCreateUserDetails {
                email: &user.email,
                email_verified: user.email_verified,
                role: user.role,
                status: user.status,
                must_change_password: user.must_change_password,
                temporary_password_generated,
                storage_quota: user.storage_quota,
                policy_group_id: user.policy_group_id,
            })
        },
    )
    .await;
    Ok(output)
}

pub async fn update(
    state: &impl SharedRuntimeState,
    input: UpdateUserInput,
) -> Result<super::models::UserInfo> {
    let (user, _) = update_with_audit_diff(state, input).await?;
    Ok(user)
}

async fn update_with_audit_diff(
    state: &impl SharedRuntimeState,
    input: UpdateUserInput,
) -> Result<(super::models::UserInfo, UpdateAuditDiff)> {
    let UpdateUserInput {
        id,
        email_verified,
        role,
        status,
        must_change_password,
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

    let existing = user_repo::find_by_id(state.writer_db(), id).await?;
    let existing_policy_group_id = existing.policy_group_id;
    let existing_email_verified = auth_service::is_email_verified(&existing);
    let before = UserAuditSnapshot {
        email_verified: existing_email_verified,
        role: existing.role,
        status: existing.status,
        must_change_password: existing.must_change_password,
        storage_quota: existing.storage_quota,
        policy_group_id: existing.policy_group_id,
    };
    let email_verified_changed =
        email_verified.is_some_and(|value| value != existing_email_verified);
    let role_changed = role.is_some_and(|value| value != existing.role);
    let status_changed = status.is_some_and(|value| value != existing.status);
    let must_change_password_changed =
        must_change_password.is_some_and(|value| value != existing.must_change_password);
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
    if let Some(must_change_password) = must_change_password {
        active.must_change_password = Set(must_change_password);
    }
    if let Some(storage_quota) = storage_quota {
        active.storage_quota = Set(storage_quota);
    }
    if let Some(group_id) = policy_group_id {
        let group =
            crate::db::repository::policy_group_repo::find_group_by_id(state.writer_db(), group_id)
                .await?;
        if !group.is_enabled {
            return Err(AsterError::validation_error(
                "cannot assign a disabled storage policy group",
            ));
        }
        let items =
            crate::db::repository::policy_group_repo::find_group_items(state.writer_db(), group_id)
                .await?;
        if items.is_empty() {
            return Err(AsterError::validation_error(
                "cannot assign a storage policy group without policies",
            ));
        }
        active.policy_group_id = Set(Some(group_id));
    }
    if status_changed || email_verified_changed || must_change_password_changed {
        active.session_version = Set(current_session_version.saturating_add(1));
    }
    active.updated_at = Set(Utc::now());
    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    let result = async {
        let updated = active
            .update(&txn)
            .await
            .map_aster_err(AsterError::database_operation)?;
        if status_changed || email_verified_changed || must_change_password_changed {
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
                .policy_snapshot()
                .set_user_policy_group(updated.id, policy_group_id);
        } else {
            state.policy_snapshot().remove_user_policy_group(updated.id);
        }
    }
    if role_changed || status_changed || email_verified_changed || must_change_password_changed {
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
    let user = to_user_info(state, &updated, profile_service::AvatarAudience::AdminUser).await?;
    let after = UserAuditSnapshot {
        email_verified: user.email_verified,
        role: user.role,
        status: user.status,
        must_change_password: user.must_change_password,
        storage_quota: user.storage_quota,
        policy_group_id: user.policy_group_id,
    };
    Ok((user, UpdateAuditDiff { before, after }))
}

pub async fn update_with_audit(
    state: &impl SharedRuntimeState,
    input: UpdateUserInput,
    audit_ctx: &AuditContext,
) -> Result<super::models::UserInfo> {
    let (user, diff) = update_with_audit_diff(state, input).await?;
    let changed_fields = changed_user_fields(diff.before, diff.after);
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminUpdateUser,
        crate::services::audit_service::AuditEntityType::User,
        Some(user.id),
        Some(&user.username),
        || {
            audit_service::details(audit_service::AdminUpdateUserDetails {
                changed_fields,
                email_verified: diff.after.email_verified,
                role: diff.after.role,
                status: diff.after.status,
                must_change_password: diff.after.must_change_password,
                storage_quota: diff.after.storage_quota,
                policy_group_id: diff.after.policy_group_id,
                previous_email_verified: diff.before.email_verified,
                previous_role: diff.before.role,
                previous_status: diff.before.status,
                previous_must_change_password: diff.before.must_change_password,
                previous_storage_quota: diff.before.storage_quota,
                previous_policy_group_id: diff.before.policy_group_id,
            })
        },
    )
    .await;
    Ok(user)
}

fn changed_user_fields(before: UserAuditSnapshot, after: UserAuditSnapshot) -> Vec<&'static str> {
    let mut fields = Vec::new();
    if before.email_verified != after.email_verified {
        fields.push("email_verified");
    }
    if before.role != after.role {
        fields.push("role");
    }
    if before.status != after.status {
        fields.push("status");
    }
    if before.must_change_password != after.must_change_password {
        fields.push("must_change_password");
    }
    if before.storage_quota != after.storage_quota {
        fields.push("storage_quota");
    }
    if before.policy_group_id != after.policy_group_id {
        fields.push("policy_group_id");
    }
    fields
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
    let db = state.writer_db();
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
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminForceDeleteUser,
        crate::services::audit_service::AuditEntityType::User,
        Some(summary.user_id),
        Some(&summary.username),
        || {
            audit_service::details(audit_service::AdminForceDeleteUserDetails {
                file_count: summary.file_count,
                folder_count: summary.folder_count,
                share_count: summary.share_count,
                webdav_account_count: summary.webdav_account_count,
                upload_session_count: summary.upload_session_count,
                lock_count: summary.lock_count,
            })
        },
    )
    .await;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::{GENERATED_PASSWORD_LENGTH, generate_temporary_password};

    #[test]
    fn generated_temporary_password_always_satisfies_character_class_policy() {
        for _ in 0..256 {
            let password = generate_temporary_password();
            assert_eq!(password.len(), GENERATED_PASSWORD_LENGTH);
            assert!(password.chars().any(|c| c.is_ascii_uppercase()));
            assert!(password.chars().any(|c| c.is_ascii_lowercase()));
            assert!(password.chars().any(|c| c.is_ascii_digit()));
            assert!(
                password
                    .chars()
                    .any(|c| c.is_ascii_graphic() && !c.is_ascii_alphanumeric())
            );
        }
    }
}
