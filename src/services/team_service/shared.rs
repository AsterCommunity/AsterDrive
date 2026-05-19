//! 团队服务内部共用 helper。
//!
//! 这层主要负责：
//! - Team / TeamMember DTO 构建
//! - 常见权限断言
//! - “至少保留一个 owner/manager”这类跨入口共用约束

use std::collections::{HashMap, HashSet};

use chrono::Utc;
use sea_orm::{ConnectionTrait, DbErr, IntoActiveModel, Set, SqlErr};

use crate::api::subcode::ApiSubcode;
use crate::config::operations;
use crate::db::repository::{policy_group_repo, team_member_repo, team_repo, user_repo};
use crate::entities::{team, team_member, user};
use crate::errors::{
    AsterError, Result, auth_forbidden_with_subcode, validation_error_with_subcode,
};
use crate::runtime::PrimaryAppState;
use crate::services::{profile_service, user_service};
use crate::types::TeamMemberRole;

use super::{
    AdminTeamInfo, CreateTeamInput, TeamInfo, TeamMemberInfo, TeamMemberListFilters,
    TeamMemberPage, UpdateTeamInput,
};

fn map_team_member_create_db_err(err: DbErr) -> AsterError {
    if matches!(err.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
        existing_team_member_error()
    } else {
        AsterError::from(err)
    }
}

pub(super) fn existing_team_member_error() -> AsterError {
    validation_error_with_subcode(
        ApiSubcode::TeamMemberExists,
        "user is already a team member",
    )
}

pub(crate) fn validate_team_name(name: &str) -> Result<String> {
    let normalized = name.trim();
    if normalized.is_empty() {
        return Err(AsterError::validation_error("team name cannot be empty"));
    }
    if normalized.chars().count() > 128 {
        return Err(AsterError::validation_error(
            "team name must be at most 128 characters",
        ));
    }
    Ok(normalized.to_string())
}

fn normalize_description(description: Option<&str>) -> String {
    description.unwrap_or_default().trim().to_string()
}

fn default_team_storage_quota(state: &PrimaryAppState) -> i64 {
    let raw = state.runtime_config.get("default_storage_quota");
    let Some(raw) = raw.as_deref() else {
        return 0;
    };

    match raw.trim().parse::<i64>() {
        Ok(value) if value >= 0 => value,
        Ok(_) => {
            tracing::warn!("invalid default_storage_quota value '{}', using 0", raw);
            0
        }
        Err(_) => {
            tracing::warn!("invalid default_storage_quota value '{}', using 0", raw);
            0
        }
    }
}

async fn load_creator_summary(
    state: &PrimaryAppState,
    team: &team::Model,
) -> Result<Option<user_service::UserSummary>> {
    let creator = user_service::user_summary_by_id(
        state,
        team.created_by,
        profile_service::AvatarAudience::AdminUser,
    )
    .await?;
    if creator.is_none() {
        tracing::warn!(
            team_id = team.id,
            created_by = team.created_by,
            "team creator missing"
        );
    }
    Ok(creator)
}

pub(super) async fn build_team_info(
    state: &PrimaryAppState,
    team: &team::Model,
    my_role: TeamMemberRole,
) -> Result<TeamInfo> {
    let creator = load_creator_summary(state, team).await?;
    let member_count = team_member_repo::count_by_team(&state.db, team.id).await?;

    Ok(build_team_info_with_metadata(
        team,
        my_role,
        creator,
        member_count,
    ))
}

pub(super) fn build_team_info_with_metadata(
    team: &team::Model,
    my_role: TeamMemberRole,
    created_by: Option<user_service::UserSummary>,
    member_count: u64,
) -> TeamInfo {
    TeamInfo {
        id: team.id,
        name: team.name.clone(),
        description: team.description.clone(),
        created_by,
        my_role,
        member_count,
        storage_used: team.storage_used,
        storage_quota: team.storage_quota,
        policy_group_id: team.policy_group_id,
        created_at: team.created_at,
        updated_at: team.updated_at,
        archived_at: team.archived_at,
    }
}

pub(super) async fn build_admin_team_info(
    state: &PrimaryAppState,
    team: &team::Model,
) -> Result<AdminTeamInfo> {
    let creator = load_creator_summary(state, team).await?;
    let member_count = team_member_repo::count_by_team(&state.db, team.id).await?;

    Ok(build_admin_team_info_with_metadata(
        team,
        creator,
        member_count,
    ))
}

pub(super) fn build_admin_team_info_with_metadata(
    team: &team::Model,
    created_by: Option<user_service::UserSummary>,
    member_count: u64,
) -> AdminTeamInfo {
    AdminTeamInfo {
        id: team.id,
        name: team.name.clone(),
        description: team.description.clone(),
        created_by,
        member_count,
        storage_used: team.storage_used,
        storage_quota: team.storage_quota,
        policy_group_id: team.policy_group_id,
        created_at: team.created_at,
        updated_at: team.updated_at,
        archived_at: team.archived_at,
    }
}

pub(super) async fn build_team_member_info(
    state: &PrimaryAppState,
    membership: team_member::Model,
    user: user::Model,
) -> Result<TeamMemberInfo> {
    let profile =
        profile_service::get_profile_info(state, &user, profile_service::AvatarAudience::AdminUser)
            .await?;
    let user_summary = user_service::to_user_summary_with_profile(&user, profile);

    Ok(TeamMemberInfo {
        id: membership.id,
        team_id: membership.team_id,
        user_id: user.id,
        email: user.email,
        user: user_summary,
        status: user.status,
        role: membership.role,
        created_at: membership.created_at,
        updated_at: membership.updated_at,
    })
}

async fn build_team_member_infos(
    state: &PrimaryAppState,
    rows: Vec<(team_member::Model, user::Model)>,
) -> Result<Vec<TeamMemberInfo>> {
    let users: Vec<user::Model> = rows.iter().map(|(_, user)| user.clone()).collect();
    let profile_map = profile_service::get_profile_info_map(
        state,
        &users,
        profile_service::AvatarAudience::AdminUser,
    )
    .await?;
    let gravatar_base_url = profile_service::resolve_gravatar_base_url(state);

    rows.into_iter()
        .map(|(membership, user)| {
            let profile = profile_map.get(&user.id).cloned().unwrap_or_else(|| {
                profile_service::build_profile_info(
                    &user,
                    None,
                    profile_service::AvatarAudience::AdminUser,
                    &gravatar_base_url,
                )
            });
            let user_summary = user_service::to_user_summary_with_profile(&user, profile);
            Ok(TeamMemberInfo {
                id: membership.id,
                team_id: membership.team_id,
                user_id: user.id,
                email: user.email,
                user: user_summary,
                status: user.status,
                role: membership.role,
                created_at: membership.created_at,
                updated_at: membership.updated_at,
            })
        })
        .collect()
}

fn build_team_member_page(
    items: Vec<TeamMemberInfo>,
    total: u64,
    limit: u64,
    offset: u64,
    owner_count: u64,
    manager_count: u64,
) -> TeamMemberPage {
    TeamMemberPage {
        items,
        total,
        limit,
        offset,
        owner_count,
        manager_count,
    }
}

pub(super) async fn load_team_member_page(
    state: &PrimaryAppState,
    team_id: i64,
    filters: &TeamMemberListFilters,
    limit: u64,
    offset: u64,
) -> Result<TeamMemberPage> {
    // 成员列表页除了 items 之外，还顺手带回 owner/admin 计数，
    // 这样前端改角色时可以直接展示“至少保留一个管理员”的上下文信息。
    let effective_limit = limit.clamp(
        1,
        operations::team_member_list_max_limit(&state.runtime_config),
    );
    let repo_filters = team_member_repo::TeamMemberPageFilters {
        role: filters.role,
        status: filters.status,
        keyword: filters.keyword.as_deref(),
        sort_by: filters.sort_by,
        sort_order: filters.sort_order,
    };
    let ((rows, total), role_counts) = tokio::try_join!(
        team_member_repo::list_page_by_team_with_user(
            &state.db,
            team_id,
            effective_limit,
            offset,
            &repo_filters,
        ),
        team_member_repo::count_by_team_grouped_by_role(&state.db, team_id),
    )?;
    let mut owner_count = 0;
    let mut admin_count = 0;
    for (role, count) in role_counts {
        match role {
            TeamMemberRole::Owner => owner_count = count,
            TeamMemberRole::Admin => admin_count = count,
            TeamMemberRole::Member => {}
        }
    }

    let items = build_team_member_infos(state, rows).await?;

    Ok(build_team_member_page(
        items,
        total,
        effective_limit,
        offset,
        owner_count,
        owner_count + admin_count,
    ))
}

pub(super) async fn resolve_target_user(
    state: &PrimaryAppState,
    user_id: Option<i64>,
    identifier: Option<&str>,
) -> Result<user::Model> {
    match (user_id, identifier.map(str::trim).filter(|s| !s.is_empty())) {
        (Some(_), Some(_)) => Err(AsterError::validation_error(
            "specify either user_id or identifier, not both",
        )),
        (None, None) => Err(AsterError::validation_error(
            "user_id or identifier is required",
        )),
        (Some(user_id), None) => user_repo::find_by_id(&state.db, user_id).await,
        (None, Some(identifier)) => {
            if let Some(user) = user_repo::find_by_username(&state.db, identifier).await? {
                return Ok(user);
            }
            if let Some(user) = user_repo::find_by_email(&state.db, identifier).await? {
                return Ok(user);
            }
            Err(AsterError::record_not_found(format!("user '{identifier}'")))
        }
    }
}

pub(super) async fn require_team_membership(
    state: &PrimaryAppState,
    team_id: i64,
    user_id: i64,
) -> Result<(team::Model, team_member::Model)> {
    // 这里故意只接受 active team。
    // 对归档团队的访问需要走专门恢复 / admin 流程，避免普通团队 API 混入 archived 语义。
    let team = team_repo::find_active_by_id(&state.db, team_id).await?;
    let membership = team_member_repo::find_by_team_and_user(&state.db, team_id, user_id)
        .await?
        .ok_or_else(|| {
            auth_forbidden_with_subcode(ApiSubcode::TeamNotMember, "not a member of this team")
        })?;
    Ok((team, membership))
}

pub(super) fn ensure_can_manage_team(role: TeamMemberRole) -> Result<()> {
    if !role.can_manage_team() {
        return Err(auth_forbidden_with_subcode(
            ApiSubcode::TeamAdminOrOwnerRequired,
            "team owner or admin role is required",
        ));
    }
    Ok(())
}

pub(super) async fn ensure_not_last_owner<C: ConnectionTrait>(db: &C, team_id: i64) -> Result<()> {
    // owner 是团队最终兜底权限，任何降级/移除操作都不能把数量减到 0。
    let owner_count =
        team_member_repo::count_by_team_and_role(db, team_id, TeamMemberRole::Owner).await?;
    if owner_count <= 1 {
        return Err(AsterError::validation_error(
            "team must keep at least one owner",
        ));
    }
    Ok(())
}

pub(super) async fn ensure_not_last_manager<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
) -> Result<()> {
    // manager = owner + admin。很多团队管理操作只要求“还有一个能管事的人”，
    // 因此这里的约束比 owner 更宽一点。
    let owner_count =
        team_member_repo::count_by_team_and_role(db, team_id, TeamMemberRole::Owner).await?;
    let admin_count =
        team_member_repo::count_by_team_and_role(db, team_id, TeamMemberRole::Admin).await?;
    if owner_count + admin_count <= 1 {
        return Err(AsterError::validation_error(
            "team must keep at least one admin or owner",
        ));
    }
    Ok(())
}

pub(super) async fn load_team_metadata<'a>(
    state: &PrimaryAppState,
    teams: impl IntoIterator<Item = &'a team::Model>,
) -> Result<(HashMap<i64, user_service::UserSummary>, HashMap<i64, u64>)> {
    let mut creator_ids = HashSet::new();
    let mut team_ids = HashSet::new();
    for team in teams {
        creator_ids.insert(team.created_by);
        team_ids.insert(team.id);
    }

    if team_ids.is_empty() {
        return Ok((HashMap::new(), HashMap::new()));
    }

    let creator_ids: Vec<i64> = creator_ids.into_iter().collect();
    let team_ids: Vec<i64> = team_ids.into_iter().collect();
    let (creators, member_counts) = tokio::try_join!(
        user_service::user_summaries_by_ids(
            state,
            &creator_ids,
            profile_service::AvatarAudience::AdminUser,
        ),
        team_member_repo::count_by_team_ids(&state.db, &team_ids),
    )?;

    Ok((creators, member_counts))
}

pub(super) async fn ensure_assignable_policy_group(
    state: &PrimaryAppState,
    group_id: i64,
) -> Result<()> {
    let group = policy_group_repo::find_group_by_id(&state.db, group_id).await?;
    if !group.is_enabled {
        return Err(AsterError::validation_error(
            "cannot assign a disabled storage policy group",
        ));
    }

    let items = policy_group_repo::find_group_items(&state.db, group_id).await?;
    if items.is_empty() {
        return Err(AsterError::validation_error(
            "cannot assign a storage policy group without policies",
        ));
    }

    Ok(())
}

pub(super) async fn resolve_required_policy_group_id(
    state: &PrimaryAppState,
    policy_group_id: Option<i64>,
) -> Result<i64> {
    let group_id = match policy_group_id {
        Some(group_id) => group_id,
        None => state
            .policy_snapshot
            .system_default_policy_group()
            .map(|group| group.id)
            .ok_or_else(|| {
                AsterError::validation_error(
                    "no system default storage policy group configured; provide policy_group_id when creating a team",
                )
            })?,
    };

    ensure_assignable_policy_group(state, group_id).await?;
    Ok(group_id)
}

pub(super) async fn create_team_record(
    state: &PrimaryAppState,
    created_by_user_id: i64,
    initial_member_user_id: i64,
    initial_member_role: TeamMemberRole,
    input: CreateTeamInput,
    policy_group_id: i64,
) -> Result<team::Model> {
    let name = validate_team_name(&input.name)?;
    let description = normalize_description(input.description.as_deref());
    let storage_quota = default_team_storage_quota(state);
    let now = Utc::now();

    let txn = crate::db::transaction::begin(&state.db).await?;
    let created_team = team_repo::create(
        &txn,
        team::ActiveModel {
            name: Set(name),
            description: Set(description),
            created_by: Set(created_by_user_id),
            storage_used: Set(0),
            storage_quota: Set(storage_quota),
            policy_group_id: Set(Some(policy_group_id)),
            created_at: Set(now),
            updated_at: Set(now),
            archived_at: Set(None),
            ..Default::default()
        },
    )
    .await?;
    team_member_repo::create(
        &txn,
        team_member::ActiveModel {
            team_id: Set(created_team.id),
            user_id: Set(initial_member_user_id),
            role: Set(initial_member_role),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await?;
    crate::db::transaction::commit(txn).await?;
    crate::services::workspace_storage_service::invalidate_team_access_cache_for_member(
        state,
        created_team.id,
        initial_member_user_id,
    )
    .await;
    tracing::info!(
        team_id = created_team.id,
        created_by_user_id,
        initial_member_user_id,
        initial_member_role = ?initial_member_role,
        policy_group_id,
        "created team"
    );

    Ok(created_team)
}

pub(super) async fn update_team_record(
    state: &PrimaryAppState,
    team: team::Model,
    input: UpdateTeamInput,
    policy_group_id: Option<i64>,
) -> Result<team::Model> {
    let mut active = team.into_active_model();
    if let Some(name) = input.name {
        active.name = Set(validate_team_name(&name)?);
    }
    if let Some(description) = input.description {
        active.description = Set(normalize_description(Some(&description)));
    }
    if let Some(policy_group_id) = policy_group_id {
        ensure_assignable_policy_group(state, policy_group_id).await?;
        active.policy_group_id = Set(Some(policy_group_id));
    }
    active.updated_at = Set(Utc::now());

    let updated = team_repo::update(&state.db, active).await?;
    crate::services::workspace_storage_service::invalidate_team_access_cache_for_team(
        state, updated.id,
    )
    .await;
    tracing::debug!(
        team_id = updated.id,
        policy_group_id = updated.policy_group_id,
        "updated team"
    );
    Ok(updated)
}

pub(super) async fn archive_team_record(state: &PrimaryAppState, team: team::Model) -> Result<()> {
    let team_id = team.id;
    let team_name = team.name.clone();
    let mut active = team.into_active_model();
    let now = Utc::now();
    active.archived_at = Set(Some(now));
    active.updated_at = Set(now);
    team_repo::update(&state.db, active).await?;
    crate::services::workspace_storage_service::invalidate_team_access_cache_for_team(
        state, team_id,
    )
    .await;
    tracing::info!(
        team_id,
        team_name = %team_name,
        archived_at = %now,
        "archived team"
    );
    Ok(())
}

pub(super) async fn restore_team_record(
    state: &PrimaryAppState,
    team: team::Model,
) -> Result<team::Model> {
    let team_id = team.id;
    let team_name = team.name.clone();
    let mut active = team.into_active_model();
    let now = Utc::now();
    active.archived_at = Set(None);
    active.updated_at = Set(now);
    let restored = team_repo::update(&state.db, active).await?;
    crate::services::workspace_storage_service::invalidate_team_access_cache_for_team(
        state,
        restored.id,
    )
    .await;
    tracing::info!(
        team_id,
        team_name = %team_name,
        "restored archived team"
    );
    Ok(restored)
}

pub(super) fn map_member_create_error(err: DbErr) -> AsterError {
    map_team_member_create_db_err(err)
}
