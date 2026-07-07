//! 团队服务子模块：`admin`。

use crate::api::pagination::{AdminTeamSortBy, OffsetPage, SortOrder, load_offset_page};
use crate::db::repository::team_repo;
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::types::TeamMemberRole;

use super::shared::{
    archive_team_record, build_admin_team_info, build_admin_team_info_with_metadata,
    create_team_record, load_team_metadata, resolve_required_policy_group_id, resolve_target_user,
    restore_team_record, update_team_record,
};
use super::{
    AdminCreateTeamInput, AdminTeamInfo, AdminUpdateTeamInput, CreateTeamInput, UpdateTeamInput,
};

pub async fn list_admin_teams(
    state: &impl SharedRuntimeState,
    limit: u64,
    offset: u64,
    keyword: Option<&str>,
    archived: bool,
    sort_by: AdminTeamSortBy,
    sort_order: SortOrder,
) -> Result<OffsetPage<AdminTeamInfo>> {
    let page = load_offset_page(limit, offset, 100, |limit, offset| async move {
        if archived {
            team_repo::find_archived_paginated(
                state.reader_db(),
                limit,
                offset,
                keyword,
                sort_by,
                sort_order,
            )
            .await
        } else {
            team_repo::find_active_paginated(
                state.reader_db(),
                limit,
                offset,
                keyword,
                sort_by,
                sort_order,
            )
            .await
        }
    })
    .await?;
    let (creator_usernames, member_counts) = load_team_metadata(state, &page.items).await?;

    Ok(OffsetPage::new(
        page.items
            .into_iter()
            .map(|team| {
                let created_by = creator_usernames.get(&team.created_by).cloned();
                let member_count = member_counts.get(&team.id).copied().unwrap_or_default();
                build_admin_team_info_with_metadata(&team, created_by, member_count)
            })
            .collect(),
        page.total,
        page.limit,
        page.offset,
    ))
}

pub async fn get_admin_team(
    state: &impl SharedRuntimeState,
    team_id: i64,
) -> Result<AdminTeamInfo> {
    let team = team_repo::find_by_id(state.reader_db(), team_id).await?;
    build_admin_team_info(state, &team).await
}

pub async fn create_admin_team(
    state: &impl SharedRuntimeState,
    actor_user_id: i64,
    input: AdminCreateTeamInput,
) -> Result<AdminTeamInfo> {
    let team_admin = resolve_target_user(
        state,
        input.admin_user_id,
        input.admin_identifier.as_deref(),
    )
    .await?;
    if !team_admin.status.is_active() {
        return Err(AsterError::validation_error(
            "cannot create a team for a disabled user",
        ));
    }

    let policy_group_id = resolve_required_policy_group_id(state, input.policy_group_id).await?;
    let created_team = create_team_record(
        state,
        actor_user_id,
        team_admin.id,
        TeamMemberRole::Admin,
        CreateTeamInput {
            name: input.name,
            description: input.description,
        },
        policy_group_id,
        input.storage_quota,
    )
    .await?;
    build_admin_team_info(state, &created_team).await
}

pub async fn update_admin_team(
    state: &impl SharedRuntimeState,
    team_id: i64,
    input: AdminUpdateTeamInput,
) -> Result<AdminTeamInfo> {
    let team = team_repo::find_active_by_id(state.writer_db(), team_id).await?;
    let updated = update_team_record(
        state,
        team,
        UpdateTeamInput {
            name: input.name,
            description: input.description,
        },
        input.policy_group_id,
        input.storage_quota,
    )
    .await?;
    build_admin_team_info(state, &updated).await
}

pub async fn archive_admin_team(state: &impl SharedRuntimeState, team_id: i64) -> Result<()> {
    let team = team_repo::find_active_by_id(state.writer_db(), team_id).await?;
    archive_team_record(state, team).await
}

pub async fn restore_admin_team(
    state: &impl SharedRuntimeState,
    team_id: i64,
) -> Result<AdminTeamInfo> {
    let team = team_repo::find_archived_by_id(state.writer_db(), team_id).await?;
    let restored = restore_team_record(state, team).await?;
    build_admin_team_info(state, &restored).await
}
