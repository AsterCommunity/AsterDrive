//! 团队成员管理。
//!
//! 这里最重要的不是 CRUD 本身，而是角色变更和移除时的安全边界：
//! - 只有 manager 能管理成员
//! - owner 相关操作只能由 owner 执行
//! - 不能把团队降成“没有 owner”或“没有任何 manager”

use chrono::Utc;
use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{team_member_repo, team_repo, user_repo};
use crate::entities::team_member;
use crate::errors::{AsterError, Result, auth_forbidden_with_code};
use crate::runtime::SharedRuntimeState;
use crate::services::workspace::storage;
use crate::types::TeamMemberRole;

use super::shared::{
    build_team_member_info, ensure_can_manage_team, ensure_not_last_manager, ensure_not_last_owner,
    existing_team_member_error, load_team_member_page, map_member_create_error,
    require_team_membership, resolve_target_user,
};
use super::{AddTeamMemberInput, TeamMemberInfo, TeamMemberListFilters, TeamMemberPage};

pub async fn list_admin_members(
    state: &impl SharedRuntimeState,
    team_id: i64,
    filters: TeamMemberListFilters,
    limit: u64,
    offset: u64,
) -> Result<TeamMemberPage> {
    team_repo::find_by_id(state.writer_db(), team_id).await?;
    load_team_member_page(state, team_id, &filters, limit, offset).await
}

async fn invalidate_webdav_auth_for_team_member_change(
    state: &impl SharedRuntimeState,
    team_id: i64,
    member_user_id: i64,
    operation: &'static str,
) {
    if let Err(first_error) =
        crate::webdav::auth::invalidate_webdav_auth_for_team_member(state, team_id, member_user_id)
            .await
    {
        tracing::warn!(
            team_id,
            member_user_id,
            operation,
            "failed to invalidate WebDAV auth cache after team member change, retrying: {first_error}"
        );
        if let Err(error) = crate::webdav::auth::invalidate_webdav_auth_for_team_member(
            state,
            team_id,
            member_user_id,
        )
        .await
        {
            tracing::error!(
                team_id,
                member_user_id,
                operation,
                "failed to invalidate WebDAV auth cache after retry: {error}"
            );
        }
    }
}

pub async fn get_admin_member(
    state: &impl SharedRuntimeState,
    team_id: i64,
    member_user_id: i64,
) -> Result<TeamMemberInfo> {
    team_repo::find_by_id(state.writer_db(), team_id).await?;
    let membership =
        team_member_repo::find_by_team_and_user(state.writer_db(), team_id, member_user_id)
            .await?
            .ok_or_else(|| {
                AsterError::record_not_found(format!("team member user #{member_user_id}"))
            })?;
    let user = user_repo::find_by_id(state.writer_db(), member_user_id).await?;
    build_team_member_info(state, membership, user).await
}

pub async fn add_admin_member(
    state: &impl SharedRuntimeState,
    team_id: i64,
    input: AddTeamMemberInput,
) -> Result<TeamMemberInfo> {
    let target_user =
        resolve_target_user(state, input.user_id, input.identifier.as_deref()).await?;
    if !target_user.status.is_active() {
        return Err(AsterError::validation_error(
            "cannot add a disabled user to a team",
        ));
    }

    let now = Utc::now();
    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    team_repo::lock_active_by_id(&txn, team_id).await?;

    // 先锁团队再查 membership，避免并发添加把同一用户重复插入 team_members。
    if team_member_repo::find_by_team_and_user(&txn, team_id, target_user.id)
        .await?
        .is_some()
    {
        return Err(existing_team_member_error());
    }

    let membership = team_member::ActiveModel {
        team_id: Set(team_id),
        user_id: Set(target_user.id),
        role: Set(input.role),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&txn)
    .await
    .map_err(map_member_create_error)?;
    crate::db::transaction::commit(txn).await?;
    storage::invalidate_team_access_cache_for_member(state, team_id, target_user.id).await;

    build_team_member_info(state, membership, target_user).await
}

pub async fn update_admin_member_role(
    state: &impl SharedRuntimeState,
    team_id: i64,
    member_user_id: i64,
    role: TeamMemberRole,
) -> Result<TeamMemberInfo> {
    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    team_repo::lock_active_by_id(&txn, team_id).await?;

    let target_membership = team_member_repo::find_by_team_and_user(&txn, team_id, member_user_id)
        .await?
        .ok_or_else(|| {
            AsterError::record_not_found(format!("team member user #{member_user_id}"))
        })?;

    if target_membership.role.is_owner() && !role.is_owner() {
        ensure_not_last_owner(&txn, team_id).await?;
    }
    if target_membership.role.can_manage_team() && !role.can_manage_team() {
        ensure_not_last_manager(&txn, team_id).await?;
    }

    let mut active = target_membership.clone().into_active_model();
    active.role = Set(role);
    active.updated_at = Set(Utc::now());
    let updated = team_member_repo::update(&txn, active).await?;
    let target_user = user_repo::find_by_id(&txn, member_user_id).await?;
    crate::db::transaction::commit(txn).await?;
    storage::invalidate_team_access_cache_for_member(state, team_id, member_user_id).await;
    invalidate_webdav_auth_for_team_member_change(
        state,
        team_id,
        member_user_id,
        "admin_team_member_role_update",
    )
    .await;
    build_team_member_info(state, updated, target_user).await
}

pub async fn remove_admin_member(
    state: &impl SharedRuntimeState,
    team_id: i64,
    member_user_id: i64,
) -> Result<()> {
    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    team_repo::lock_active_by_id(&txn, team_id).await?;

    let target_membership = team_member_repo::find_by_team_and_user(&txn, team_id, member_user_id)
        .await?
        .ok_or_else(|| {
            AsterError::record_not_found(format!("team member user #{member_user_id}"))
        })?;

    if target_membership.role.is_owner() {
        ensure_not_last_owner(&txn, team_id).await?;
    }
    if target_membership.role.can_manage_team() {
        ensure_not_last_manager(&txn, team_id).await?;
    }

    team_member_repo::delete(&txn, target_membership.id).await?;
    crate::db::transaction::commit(txn).await?;
    storage::invalidate_team_access_cache_for_member(state, team_id, member_user_id).await;
    invalidate_webdav_auth_for_team_member_change(
        state,
        team_id,
        member_user_id,
        "admin_team_member_removal",
    )
    .await;
    tracing::debug!(
        team_id,
        member_user_id,
        membership_id = target_membership.id,
        removed_role = ?target_membership.role,
        "admin removed team member"
    );
    Ok(())
}

pub async fn list_members(
    state: &impl SharedRuntimeState,
    team_id: i64,
    actor_user_id: i64,
    filters: TeamMemberListFilters,
    limit: u64,
    offset: u64,
) -> Result<TeamMemberPage> {
    require_team_membership(state, team_id, actor_user_id).await?;
    load_team_member_page(state, team_id, &filters, limit, offset).await
}

pub async fn get_member(
    state: &impl SharedRuntimeState,
    team_id: i64,
    actor_user_id: i64,
    member_user_id: i64,
) -> Result<TeamMemberInfo> {
    require_team_membership(state, team_id, actor_user_id).await?;
    let membership =
        team_member_repo::find_by_team_and_user(state.writer_db(), team_id, member_user_id)
            .await?
            .ok_or_else(|| {
                AsterError::record_not_found(format!("team member user #{member_user_id}"))
            })?;
    let user = user_repo::find_by_id(state.writer_db(), member_user_id).await?;
    build_team_member_info(state, membership, user).await
}

pub async fn add_member(
    state: &impl SharedRuntimeState,
    team_id: i64,
    actor_user_id: i64,
    input: AddTeamMemberInput,
) -> Result<TeamMemberInfo> {
    let target_user =
        resolve_target_user(state, input.user_id, input.identifier.as_deref()).await?;
    if !target_user.status.is_active() {
        return Err(AsterError::validation_error(
            "cannot add a disabled user to a team",
        ));
    }

    let now = Utc::now();
    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    team_repo::lock_active_by_id(&txn, team_id).await?;

    let actor_membership = team_member_repo::find_by_team_and_user(&txn, team_id, actor_user_id)
        .await?
        .ok_or_else(|| {
            auth_forbidden_with_code(ApiErrorCode::TeamNotMember, "not a member of this team")
        })?;
    ensure_can_manage_team(actor_membership.role)?;
    // admin 可以添加普通成员 / admin，但不能凭空再造 owner。
    if !actor_membership.role.is_owner() && input.role.is_owner() {
        return Err(auth_forbidden_with_code(
            ApiErrorCode::TeamOwnerRequired,
            "only a team owner can assign owner role",
        ));
    }

    if team_member_repo::find_by_team_and_user(&txn, team_id, target_user.id)
        .await?
        .is_some()
    {
        return Err(existing_team_member_error());
    }

    let membership = team_member::ActiveModel {
        team_id: Set(team_id),
        user_id: Set(target_user.id),
        role: Set(input.role),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&txn)
    .await
    .map_err(map_member_create_error)?;
    crate::db::transaction::commit(txn).await?;
    storage::invalidate_team_access_cache_for_member(state, team_id, target_user.id).await;

    build_team_member_info(state, membership, target_user).await
}

pub async fn update_member_role(
    state: &impl SharedRuntimeState,
    team_id: i64,
    actor_user_id: i64,
    member_user_id: i64,
    role: TeamMemberRole,
) -> Result<TeamMemberInfo> {
    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    team_repo::lock_active_by_id(&txn, team_id).await?;

    let actor_membership = team_member_repo::find_by_team_and_user(&txn, team_id, actor_user_id)
        .await?
        .ok_or_else(|| {
            auth_forbidden_with_code(ApiErrorCode::TeamNotMember, "not a member of this team")
        })?;
    ensure_can_manage_team(actor_membership.role)?;

    let target_membership = team_member_repo::find_by_team_and_user(&txn, team_id, member_user_id)
        .await?
        .ok_or_else(|| {
            AsterError::record_not_found(format!("team member user #{member_user_id}"))
        })?;

    // owner 相关角色变更比普通 admin 更严格：
    // 非 owner 既不能降 owner，也不能把别人升成 owner。
    if !actor_membership.role.is_owner() && (target_membership.role.is_owner() || role.is_owner()) {
        return Err(auth_forbidden_with_code(
            ApiErrorCode::TeamOwnerRequired,
            "only a team owner can manage owner memberships",
        ));
    }

    if target_membership.role.is_owner() && !role.is_owner() {
        ensure_not_last_owner(&txn, team_id).await?;
    }
    if target_membership.role.can_manage_team() && !role.can_manage_team() {
        ensure_not_last_manager(&txn, team_id).await?;
    }

    let mut active = target_membership.clone().into_active_model();
    active.role = Set(role);
    active.updated_at = Set(Utc::now());
    let updated = team_member_repo::update(&txn, active).await?;
    let target_user = user_repo::find_by_id(&txn, member_user_id).await?;
    crate::db::transaction::commit(txn).await?;
    storage::invalidate_team_access_cache_for_member(state, team_id, member_user_id).await;
    invalidate_webdav_auth_for_team_member_change(
        state,
        team_id,
        member_user_id,
        "team_member_role_update",
    )
    .await;
    build_team_member_info(state, updated, target_user).await
}

pub async fn remove_member(
    state: &impl SharedRuntimeState,
    team_id: i64,
    actor_user_id: i64,
    member_user_id: i64,
) -> Result<()> {
    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    team_repo::lock_active_by_id(&txn, team_id).await?;

    let actor_membership = team_member_repo::find_by_team_and_user(&txn, team_id, actor_user_id)
        .await?
        .ok_or_else(|| {
            auth_forbidden_with_code(ApiErrorCode::TeamNotMember, "not a member of this team")
        })?;
    let target_membership = team_member_repo::find_by_team_and_user(&txn, team_id, member_user_id)
        .await?
        .ok_or_else(|| {
            AsterError::record_not_found(format!("team member user #{member_user_id}"))
        })?;

    // 成员可以自行退出团队；只有在“替别人移除成员”时才强制要求 manager 权限。
    if actor_user_id != member_user_id {
        ensure_can_manage_team(actor_membership.role)?;
        if !actor_membership.role.is_owner() && target_membership.role.is_owner() {
            return Err(auth_forbidden_with_code(
                ApiErrorCode::TeamOwnerRequired,
                "only a team owner can remove an owner",
            ));
        }
    }

    if target_membership.role.is_owner() {
        ensure_not_last_owner(&txn, team_id).await?;
    }
    if target_membership.role.can_manage_team() {
        ensure_not_last_manager(&txn, team_id).await?;
    }

    team_member_repo::delete(&txn, target_membership.id).await?;
    crate::db::transaction::commit(txn).await?;
    storage::invalidate_team_access_cache_for_member(state, team_id, member_user_id).await;
    invalidate_webdav_auth_for_team_member_change(
        state,
        team_id,
        member_user_id,
        "team_member_removal",
    )
    .await;
    tracing::debug!(
        team_id,
        actor_user_id,
        member_user_id,
        membership_id = target_membership.id,
        removed_role = ?target_membership.role,
        self_leave = actor_user_id == member_user_id,
        "removed team member"
    );
    Ok(())
}
