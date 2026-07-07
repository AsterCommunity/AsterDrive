//! 团队服务聚合入口。
//!
//! 团队相关逻辑拆成两层：
//! - `team` / `members` / `archive` 这些核心业务语义
//! - 这里的 audit 包装与导出入口
//!
//! 团队空间文件操作本身不在这里实现，而是复用 workspace/file/folder/upload 等服务。

mod admin;
mod archive;
mod members;
mod models;
mod shared;
mod team;

use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::services::{
    audit_service::{self, AuditContext},
    auth::local,
};

pub use admin::{
    archive_admin_team, create_admin_team, get_admin_team, list_admin_teams, restore_admin_team,
    update_admin_team,
};
pub use archive::cleanup_expired_archived_teams;
pub use members::{
    add_admin_member, add_member, get_admin_member, get_member, list_admin_members, list_members,
    remove_admin_member, remove_member, update_admin_member_role, update_member_role,
};
pub use models::{
    AddTeamMemberInput, AdminCreateTeamInput, AdminTeamInfo, AdminUpdateTeamInput, CreateTeamInput,
    TeamInfo, TeamMemberInfo, TeamMemberListFilters, TeamMemberPage, UpdateTeamInput,
};
pub(crate) use shared::validate_team_name;
pub use team::{
    archive_team, create_team, get_team, list_teams, list_teams_filtered, list_user_team_ids,
    restore_team, update_team,
};

// 和其他 service 一样，audit 放在聚合层，核心 team/member 逻辑保持纯业务语义。
fn team_audit_details(team: &TeamInfo) -> Option<serde_json::Value> {
    audit_service::details(audit_service::TeamAuditDetails {
        description: &team.description,
        member_count: team.member_count,
        storage_quota: team.storage_quota,
        policy_group_id: team.policy_group_id,
        archived_at: team.archived_at,
        actor_role: Some(team.my_role),
    })
}

fn admin_team_audit_details(team: &AdminTeamInfo) -> Option<serde_json::Value> {
    audit_service::details(audit_service::TeamAuditDetails {
        description: &team.description,
        member_count: team.member_count,
        storage_quota: team.storage_quota,
        policy_group_id: team.policy_group_id,
        archived_at: team.archived_at,
        actor_role: None,
    })
}

pub(crate) async fn create_team_with_audit(
    state: &impl SharedRuntimeState,
    actor_user_id: i64,
    input: CreateTeamInput,
    audit_ctx: &AuditContext,
) -> Result<TeamInfo> {
    // 当前产品约束下，普通用户不能自助创建团队；团队创建入口收敛在系统管理员。
    let snapshot = local::get_auth_snapshot(state, actor_user_id).await?;
    if !snapshot.role.is_admin() {
        return Err(AsterError::auth_forbidden(
            "team creation is restricted to system admins",
        ));
    }

    let team = create_team(state, actor_user_id, input).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::TeamCreate,
        crate::services::audit_service::AuditEntityType::Team,
        Some(team.id),
        Some(&team.name),
        || team_audit_details(&team),
    )
    .await;
    Ok(team)
}

pub(crate) async fn update_team_with_audit(
    state: &impl SharedRuntimeState,
    team_id: i64,
    actor_user_id: i64,
    input: UpdateTeamInput,
    audit_ctx: &AuditContext,
) -> Result<TeamInfo> {
    let team = update_team(state, team_id, actor_user_id, input).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::TeamUpdate,
        crate::services::audit_service::AuditEntityType::Team,
        Some(team.id),
        Some(&team.name),
        || team_audit_details(&team),
    )
    .await;
    Ok(team)
}

pub(crate) async fn archive_team_with_audit(
    state: &impl SharedRuntimeState,
    team_id: i64,
    actor_user_id: i64,
    audit_ctx: &AuditContext,
) -> Result<()> {
    let team = get_team(state, team_id, actor_user_id).await?;
    archive_team(state, team_id, actor_user_id).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::TeamArchive,
        crate::services::audit_service::AuditEntityType::Team,
        Some(team.id),
        Some(&team.name),
        || {
            audit_service::details(audit_service::TeamAuditDetails {
                description: &team.description,
                member_count: team.member_count,
                storage_quota: team.storage_quota,
                policy_group_id: team.policy_group_id,
                archived_at: Some(chrono::Utc::now()),
                actor_role: Some(team.my_role),
            })
        },
    )
    .await;
    Ok(())
}

pub(crate) async fn restore_team_with_audit(
    state: &impl SharedRuntimeState,
    team_id: i64,
    actor_user_id: i64,
    audit_ctx: &AuditContext,
) -> Result<TeamInfo> {
    let team = restore_team(state, team_id, actor_user_id).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::TeamRestore,
        crate::services::audit_service::AuditEntityType::Team,
        Some(team.id),
        Some(&team.name),
        || team_audit_details(&team),
    )
    .await;
    Ok(team)
}

pub(crate) async fn list_team_audit_entries(
    state: &impl SharedRuntimeState,
    team_id: i64,
    actor_user_id: i64,
    mut filters: audit_service::AuditLogFilters,
    limit: u64,
    offset: u64,
) -> Result<crate::api::pagination::OffsetPage<audit_service::TeamAuditEntryInfo>> {
    let team = get_team(state, team_id, actor_user_id).await?;
    if !team.my_role.can_manage_team() {
        return Err(AsterError::auth_forbidden(
            "team owner or admin role is required",
        ));
    }

    filters.entity_type = Some(crate::services::audit_service::AuditEntityType::Team);
    filters.entity_id = Some(team.id);
    audit_service::query_team_entries(state, filters, limit, offset).await
}

pub(crate) async fn add_member_with_audit(
    state: &impl SharedRuntimeState,
    team_id: i64,
    actor_user_id: i64,
    input: AddTeamMemberInput,
    audit_ctx: &AuditContext,
) -> Result<TeamMemberInfo> {
    let team = get_team(state, team_id, actor_user_id).await?;
    let member = add_member(state, team_id, actor_user_id, input).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::TeamMemberAdd,
        crate::services::audit_service::AuditEntityType::Team,
        Some(team.id),
        Some(&team.name),
        || {
            audit_service::details(audit_service::TeamMemberAddAuditDetails {
                member_user_id: member.user_id,
                member_username: &member.user.username,
                role: member.role,
                actor_role: Some(team.my_role),
            })
        },
    )
    .await;
    Ok(member)
}

pub(crate) async fn update_member_role_with_audit(
    state: &impl SharedRuntimeState,
    team_id: i64,
    actor_user_id: i64,
    member_user_id: i64,
    role: crate::types::TeamMemberRole,
    audit_ctx: &AuditContext,
) -> Result<TeamMemberInfo> {
    let team = get_team(state, team_id, actor_user_id).await?;
    let previous_member = get_member(state, team_id, actor_user_id, member_user_id)
        .await
        .ok();
    let member = update_member_role(state, team_id, actor_user_id, member_user_id, role).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::TeamMemberUpdate,
        crate::services::audit_service::AuditEntityType::Team,
        Some(team.id),
        Some(&team.name),
        || {
            audit_service::details(audit_service::TeamMemberUpdateAuditDetails {
                member_user_id: member.user_id,
                member_username: &member.user.username,
                previous_role: previous_member
                    .as_ref()
                    .map(|entry| entry.role)
                    .unwrap_or(member.role),
                next_role: member.role,
                actor_role: Some(team.my_role),
            })
        },
    )
    .await;
    Ok(member)
}

pub(crate) async fn remove_member_with_audit(
    state: &impl SharedRuntimeState,
    team_id: i64,
    actor_user_id: i64,
    member_user_id: i64,
    audit_ctx: &AuditContext,
) -> Result<()> {
    let team = get_team(state, team_id, actor_user_id).await?;
    let target_member = get_member(state, team_id, actor_user_id, member_user_id)
        .await
        .ok();
    remove_member(state, team_id, actor_user_id, member_user_id).await?;
    if let Some(member) = target_member {
        audit_service::log_with_details(
            state,
            audit_ctx,
            audit_service::AuditAction::TeamMemberRemove,
            crate::services::audit_service::AuditEntityType::Team,
            Some(team.id),
            Some(&team.name),
            || {
                audit_service::details(audit_service::TeamMemberRemoveAuditDetails {
                    member_user_id: member.user_id,
                    member_username: &member.user.username,
                    removed_role: member.role,
                    actor_role: Some(team.my_role),
                })
            },
        )
        .await;
    }
    Ok(())
}

pub(crate) async fn create_admin_team_with_audit(
    state: &impl SharedRuntimeState,
    actor_user_id: i64,
    input: AdminCreateTeamInput,
    audit_ctx: &AuditContext,
) -> Result<AdminTeamInfo> {
    let team = create_admin_team(state, actor_user_id, input).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminCreateTeam,
        crate::services::audit_service::AuditEntityType::Team,
        Some(team.id),
        Some(&team.name),
        || admin_team_audit_details(&team),
    )
    .await;
    Ok(team)
}

pub(crate) async fn update_admin_team_with_audit(
    state: &impl SharedRuntimeState,
    team_id: i64,
    input: AdminUpdateTeamInput,
    audit_ctx: &AuditContext,
) -> Result<AdminTeamInfo> {
    let team = update_admin_team(state, team_id, input).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminUpdateTeam,
        crate::services::audit_service::AuditEntityType::Team,
        Some(team.id),
        Some(&team.name),
        || admin_team_audit_details(&team),
    )
    .await;
    Ok(team)
}

pub(crate) async fn archive_admin_team_with_audit(
    state: &impl SharedRuntimeState,
    team_id: i64,
    audit_ctx: &AuditContext,
) -> Result<()> {
    let team = get_admin_team(state, team_id).await?;
    archive_admin_team(state, team_id).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminArchiveTeam,
        crate::services::audit_service::AuditEntityType::Team,
        Some(team.id),
        Some(&team.name),
        || {
            audit_service::details(audit_service::TeamAuditDetails {
                description: &team.description,
                member_count: team.member_count,
                storage_quota: team.storage_quota,
                policy_group_id: team.policy_group_id,
                archived_at: Some(chrono::Utc::now()),
                actor_role: None,
            })
        },
    )
    .await;
    Ok(())
}

pub(crate) async fn restore_admin_team_with_audit(
    state: &impl SharedRuntimeState,
    team_id: i64,
    audit_ctx: &AuditContext,
) -> Result<AdminTeamInfo> {
    let team = restore_admin_team(state, team_id).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminRestoreTeam,
        crate::services::audit_service::AuditEntityType::Team,
        Some(team.id),
        Some(&team.name),
        || admin_team_audit_details(&team),
    )
    .await;
    Ok(team)
}

pub(crate) async fn list_admin_team_audit_entries(
    state: &impl SharedRuntimeState,
    team_id: i64,
    mut filters: audit_service::AuditLogFilters,
    limit: u64,
    offset: u64,
) -> Result<crate::api::pagination::OffsetPage<audit_service::TeamAuditEntryInfo>> {
    let team = get_admin_team(state, team_id).await?;
    filters.entity_type = Some(crate::services::audit_service::AuditEntityType::Team);
    filters.entity_id = Some(team.id);
    audit_service::query_team_entries(state, filters, limit, offset).await
}

pub(crate) async fn add_admin_member_with_audit(
    state: &impl SharedRuntimeState,
    team_id: i64,
    input: AddTeamMemberInput,
    audit_ctx: &AuditContext,
) -> Result<TeamMemberInfo> {
    let team = get_admin_team(state, team_id).await?;
    let member = add_admin_member(state, team_id, input).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::TeamMemberAdd,
        crate::services::audit_service::AuditEntityType::Team,
        Some(team.id),
        Some(&team.name),
        || {
            audit_service::details(audit_service::TeamMemberAddAuditDetails {
                member_user_id: member.user_id,
                member_username: &member.user.username,
                role: member.role,
                actor_role: None,
            })
        },
    )
    .await;
    Ok(member)
}

pub(crate) async fn update_admin_member_role_with_audit(
    state: &impl SharedRuntimeState,
    team_id: i64,
    member_user_id: i64,
    role: crate::types::TeamMemberRole,
    audit_ctx: &AuditContext,
) -> Result<TeamMemberInfo> {
    let team = get_admin_team(state, team_id).await?;
    let previous_member = get_admin_member(state, team_id, member_user_id).await.ok();
    let member = update_admin_member_role(state, team_id, member_user_id, role).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::TeamMemberUpdate,
        crate::services::audit_service::AuditEntityType::Team,
        Some(team.id),
        Some(&team.name),
        || {
            audit_service::details(audit_service::TeamMemberUpdateAuditDetails {
                member_user_id: member.user_id,
                member_username: &member.user.username,
                previous_role: previous_member
                    .as_ref()
                    .map(|entry| entry.role)
                    .unwrap_or(member.role),
                next_role: member.role,
                actor_role: None,
            })
        },
    )
    .await;
    Ok(member)
}

pub(crate) async fn remove_admin_member_with_audit(
    state: &impl SharedRuntimeState,
    team_id: i64,
    member_user_id: i64,
    audit_ctx: &AuditContext,
) -> Result<()> {
    let team = get_admin_team(state, team_id).await?;
    let target_member = get_admin_member(state, team_id, member_user_id).await.ok();
    remove_admin_member(state, team_id, member_user_id).await?;
    if let Some(member) = target_member {
        audit_service::log_with_details(
            state,
            audit_ctx,
            audit_service::AuditAction::TeamMemberRemove,
            crate::services::audit_service::AuditEntityType::Team,
            Some(team.id),
            Some(&team.name),
            || {
                audit_service::details(audit_service::TeamMemberRemoveAuditDetails {
                    member_user_id: member.user_id,
                    member_username: &member.user.username,
                    removed_role: member.role,
                    actor_role: None,
                })
            },
        )
        .await;
    }
    Ok(())
}
