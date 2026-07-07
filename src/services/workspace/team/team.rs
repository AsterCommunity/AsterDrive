//! 团队本体 CRUD。
//!
//! 这里只处理 team 自身的创建、更新、归档、恢复和“当前用户能看到哪些团队”，
//! 成员增删改在 `members.rs`，管理员越权入口在 `admin.rs`。

use std::collections::HashSet;

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{team_member_repo, team_repo};
use crate::entities::{team, team_member};
use crate::errors::{Result, auth_forbidden_with_code};
use crate::runtime::SharedRuntimeState;
use crate::types::TeamMemberRole;

use super::shared::{
    archive_team_record, build_team_info, build_team_info_with_metadata, create_team_record,
    ensure_can_manage_team, load_team_metadata, require_team_membership,
    require_team_membership_for_read, resolve_required_policy_group_id, restore_team_record,
    update_team_record,
};
use super::{CreateTeamInput, TeamInfo, UpdateTeamInput};

pub async fn list_teams(
    state: &impl SharedRuntimeState,
    user_id: i64,
    archived: bool,
) -> Result<Vec<TeamInfo>> {
    list_teams_filtered(state, user_id, archived, None, None, None).await
}

pub async fn list_teams_filtered(
    state: &impl SharedRuntimeState,
    user_id: i64,
    archived: bool,
    keyword: Option<&str>,
    limit: Option<u64>,
    offset: Option<u64>,
) -> Result<Vec<TeamInfo>> {
    let memberships =
        list_user_team_memberships(state, user_id, archived, keyword, limit, offset).await?;
    if memberships.is_empty() {
        return Ok(vec![]);
    }

    let (creator_usernames, member_counts) =
        load_team_metadata(state, memberships.iter().map(|(_, team)| team)).await?;

    let mut teams = Vec::with_capacity(memberships.len());
    for (membership, team) in memberships {
        let created_by = creator_usernames.get(&team.created_by).cloned();
        let member_count = member_counts.get(&team.id).copied().unwrap_or_default();
        teams.push(build_team_info_with_metadata(
            &team,
            membership.role,
            created_by,
            member_count,
        ));
    }
    Ok(teams)
}

pub async fn list_user_team_ids(
    state: &impl SharedRuntimeState,
    user_id: i64,
    archived: bool,
) -> Result<HashSet<i64>> {
    Ok(
        list_user_team_memberships(state, user_id, archived, None, None, None)
            .await?
            .into_iter()
            .map(|(membership, _)| membership.team_id)
            .collect(),
    )
}

async fn list_user_team_memberships(
    state: &impl SharedRuntimeState,
    user_id: i64,
    archived: bool,
    keyword: Option<&str>,
    limit: Option<u64>,
    offset: Option<u64>,
) -> Result<Vec<(team_member::Model, team::Model)>> {
    // 用户视角列团队时，本质是“先看 membership，再带 team”。
    // 这样角色信息和 archived 过滤能保持一致，不会出现“能看到 team 但没有 membership”的状态。
    if archived {
        team_member_repo::list_by_user_with_archived_team_filtered(
            state.reader_db(),
            user_id,
            keyword,
            limit,
            offset,
            None,
        )
        .await
    } else {
        team_member_repo::list_by_user_with_team_filtered(
            state.reader_db(),
            user_id,
            keyword,
            limit,
            offset,
            None,
        )
        .await
    }
}

pub async fn create_team(
    state: &impl SharedRuntimeState,
    creator_user_id: i64,
    input: CreateTeamInput,
) -> Result<TeamInfo> {
    // 创建团队时会同时把创建者加入 team_members，并赋予 owner。
    // 这里返回的 TeamInfo 因此天然带 `my_role = Owner`。
    let policy_group_id = resolve_required_policy_group_id(state, None).await?;
    let created_team = create_team_record(
        state,
        creator_user_id,
        creator_user_id,
        TeamMemberRole::Owner,
        input,
        policy_group_id,
        None,
    )
    .await?;
    build_team_info(state, &created_team, TeamMemberRole::Owner).await
}

pub async fn get_team(
    state: &impl SharedRuntimeState,
    team_id: i64,
    user_id: i64,
) -> Result<TeamInfo> {
    let (team, membership) = require_team_membership_for_read(state, team_id, user_id).await?;
    build_team_info(state, &team, membership.role).await
}

pub async fn update_team(
    state: &impl SharedRuntimeState,
    team_id: i64,
    actor_user_id: i64,
    input: UpdateTeamInput,
) -> Result<TeamInfo> {
    let (team, membership) = require_team_membership(state, team_id, actor_user_id).await?;
    ensure_can_manage_team(membership.role)?;
    let updated = update_team_record(state, team, input, None, None).await?;
    build_team_info(state, &updated, membership.role).await
}

pub async fn archive_team(
    state: &impl SharedRuntimeState,
    team_id: i64,
    actor_user_id: i64,
) -> Result<()> {
    let (team, membership) = require_team_membership(state, team_id, actor_user_id).await?;
    if !membership.role.is_owner() {
        return Err(auth_forbidden_with_code(
            ApiErrorCode::TeamOwnerRequired,
            "team owner role is required",
        ));
    }

    // 归档团队不删除成员关系，恢复时仍依赖原 membership 判断谁可以解封。
    archive_team_record(state, team).await
}

pub async fn restore_team(
    state: &impl SharedRuntimeState,
    team_id: i64,
    actor_user_id: i64,
) -> Result<TeamInfo> {
    let team = team_repo::find_archived_by_id(state.writer_db(), team_id).await?;
    let membership =
        team_member_repo::find_by_team_and_user(state.writer_db(), team_id, actor_user_id)
            .await?
            .ok_or_else(|| {
                auth_forbidden_with_code(ApiErrorCode::TeamNotMember, "not a member of this team")
            })?;
    ensure_can_manage_team(membership.role)?;

    let restored = restore_team_record(state, team).await?;
    build_team_info(state, &restored, membership.role).await
}
