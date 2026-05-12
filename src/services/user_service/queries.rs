use crate::api::pagination::{AdminUserSortBy, OffsetPage, SortOrder, load_offset_page};
use crate::db::repository::user_repo;
use crate::entities::user;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{auth_service, profile_service};
use crate::types::{UserRole, UserStatus};
use std::collections::{HashMap, HashSet};

use super::models::{MeResponse, UserInfo, UserSummary, user_core};
use super::preferences::parse_preferences;

pub fn to_user_summary_with_profile(
    user: &user::Model,
    profile: profile_service::UserProfileInfo,
) -> UserSummary {
    UserSummary {
        id: user.id,
        username: user.username.clone(),
        profile,
    }
}

pub async fn to_user_summary(
    state: &PrimaryAppState,
    user: &user::Model,
    audience: profile_service::AvatarAudience,
) -> Result<UserSummary> {
    Ok(to_user_summary_with_profile(
        user,
        profile_service::get_profile_info(state, user, audience).await?,
    ))
}

pub async fn user_summaries_by_ids(
    state: &PrimaryAppState,
    user_ids: &[i64],
    audience: profile_service::AvatarAudience,
) -> Result<HashMap<i64, UserSummary>> {
    let unique_ids: Vec<i64> = user_ids
        .iter()
        .copied()
        .filter(|id| *id > 0)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    if unique_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let users = user_repo::find_by_ids(&state.db, &unique_ids).await?;
    let profile_map = profile_service::get_profile_info_map(state, &users, audience).await?;
    let gravatar_base_url = profile_service::resolve_gravatar_base_url(state);

    Ok(users
        .into_iter()
        .map(|user| {
            let profile = profile_map.get(&user.id).cloned().unwrap_or_else(|| {
                profile_service::build_profile_info(&user, None, audience, &gravatar_base_url)
            });
            (user.id, to_user_summary_with_profile(&user, profile))
        })
        .collect())
}

pub async fn user_summary_by_id(
    state: &PrimaryAppState,
    user_id: i64,
    audience: profile_service::AvatarAudience,
) -> Result<Option<UserSummary>> {
    match user_repo::find_by_id(&state.db, user_id).await {
        Ok(user) => Ok(Some(to_user_summary(state, &user, audience).await?)),
        Err(crate::errors::AsterError::RecordNotFound(_)) => Ok(None),
        Err(err) => Err(err),
    }
}

pub async fn to_user_info(
    state: &PrimaryAppState,
    user: &user::Model,
    audience: profile_service::AvatarAudience,
) -> Result<UserInfo> {
    let core = user_core(user);
    Ok(UserInfo {
        id: core.id,
        username: core.username,
        email: core.email,
        email_verified: core.email_verified,
        pending_email: core.pending_email,
        role: core.role,
        status: core.status,
        storage_used: core.storage_used,
        storage_quota: core.storage_quota,
        policy_group_id: core.policy_group_id,
        created_at: core.created_at,
        updated_at: core.updated_at,
        profile: profile_service::get_profile_info(state, user, audience).await?,
    })
}

pub async fn to_user_infos(
    state: &PrimaryAppState,
    users: Vec<user::Model>,
    audience: profile_service::AvatarAudience,
) -> Result<Vec<UserInfo>> {
    let profile_map = profile_service::get_profile_info_map(state, &users, audience).await?;
    let gravatar_base_url = profile_service::resolve_gravatar_base_url(state);

    Ok(users
        .into_iter()
        .map(|user| UserInfo {
            id: user.id,
            username: user.username.clone(),
            email: user.email.clone(),
            email_verified: auth_service::is_email_verified(&user),
            pending_email: user.pending_email.clone(),
            role: user.role,
            status: user.status,
            storage_used: user.storage_used,
            storage_quota: user.storage_quota,
            policy_group_id: user.policy_group_id,
            created_at: user.created_at,
            updated_at: user.updated_at,
            profile: profile_map.get(&user.id).cloned().unwrap_or_else(|| {
                profile_service::build_profile_info(&user, None, audience, &gravatar_base_url)
            }),
        })
        .collect())
}

pub async fn get_me(
    state: &PrimaryAppState,
    user_id: i64,
    access_token_expires_at: i64,
) -> Result<MeResponse> {
    let user = user_repo::find_by_id(&state.db, user_id).await?;
    let prefs = parse_preferences(&user);
    let core = user_core(&user);
    Ok(MeResponse {
        id: core.id,
        username: core.username,
        email: core.email,
        email_verified: core.email_verified,
        pending_email: core.pending_email,
        role: core.role,
        status: core.status,
        storage_used: core.storage_used,
        storage_quota: core.storage_quota,
        policy_group_id: core.policy_group_id,
        access_token_expires_at,
        created_at: core.created_at,
        updated_at: core.updated_at,
        preferences: prefs,
        profile: profile_service::get_profile_info(
            state,
            &user,
            profile_service::AvatarAudience::SelfUser,
        )
        .await?,
    })
}

pub async fn get_self_info(state: &PrimaryAppState, user_id: i64) -> Result<UserInfo> {
    let user = user_repo::find_by_id(&state.db, user_id).await?;
    to_user_info(state, &user, profile_service::AvatarAudience::SelfUser).await
}

pub async fn list_paginated(
    state: &PrimaryAppState,
    limit: u64,
    offset: u64,
    keyword: Option<&str>,
    role: Option<UserRole>,
    status: Option<UserStatus>,
    sort_by: AdminUserSortBy,
    sort_order: SortOrder,
) -> Result<OffsetPage<UserInfo>> {
    let page = load_offset_page(limit, offset, 100, |limit, offset| async move {
        user_repo::find_paginated(
            &state.db, limit, offset, keyword, role, status, sort_by, sort_order,
        )
        .await
    })
    .await?;

    Ok(OffsetPage::new(
        to_user_infos(
            state,
            page.items,
            profile_service::AvatarAudience::AdminUser,
        )
        .await?,
        page.total,
        page.limit,
        page.offset,
    ))
}

pub async fn get(state: &PrimaryAppState, id: i64) -> Result<UserInfo> {
    let user = user_repo::find_by_id(&state.db, id).await?;
    to_user_info(state, &user, profile_service::AvatarAudience::AdminUser).await
}
