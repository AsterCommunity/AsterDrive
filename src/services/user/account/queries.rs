use crate::api::pagination::{OffsetPage, load_offset_page};
use crate::db::repository::user_repo;
use crate::entities::user;
use crate::errors::Result;
use crate::runtime::SharedRuntimeState;
use crate::services::{auth::local, user::profile};
use std::collections::{HashMap, HashSet};

use super::models::{
    MePartialResponse, MeResponse, MeResponseFields, UserInfo, UserListFilters, UserSummary,
    user_core,
};
use super::preferences::parse_preferences;

pub fn to_user_summary_with_profile(
    user: &user::Model,
    profile: profile::UserProfileInfo,
) -> UserSummary {
    UserSummary {
        id: user.id,
        username: user.username.clone(),
        profile,
    }
}

pub async fn to_user_summary(
    state: &impl SharedRuntimeState,
    user: &user::Model,
    audience: profile::AvatarAudience,
) -> Result<UserSummary> {
    Ok(to_user_summary_with_profile(
        user,
        profile::get_profile_info(state, user, audience).await?,
    ))
}

pub async fn user_summaries_by_ids(
    state: &impl SharedRuntimeState,
    user_ids: &[i64],
    audience: profile::AvatarAudience,
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

    let users = user_repo::find_by_ids(state.reader_db(), &unique_ids).await?;
    let profile_map = profile::get_profile_info_map(state, &users, audience).await?;
    let gravatar_base_url = profile::resolve_gravatar_base_url(state);

    Ok(users
        .into_iter()
        .map(|user| {
            let profile = profile_map.get(&user.id).cloned().unwrap_or_else(|| {
                profile::build_profile_info(&user, None, audience, &gravatar_base_url)
            });
            (user.id, to_user_summary_with_profile(&user, profile))
        })
        .collect())
}

pub async fn user_summary_by_id(
    state: &impl SharedRuntimeState,
    user_id: i64,
    audience: profile::AvatarAudience,
) -> Result<Option<UserSummary>> {
    match user_repo::find_by_id(state.reader_db(), user_id).await {
        Ok(user) => Ok(Some(to_user_summary(state, &user, audience).await?)),
        Err(crate::errors::AsterError::RecordNotFound(_)) => Ok(None),
        Err(err) => Err(err),
    }
}

pub async fn to_user_info(
    state: &impl SharedRuntimeState,
    user: &user::Model,
    audience: profile::AvatarAudience,
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
        must_change_password: core.must_change_password,
        storage_used: core.storage_used,
        storage_quota: core.storage_quota,
        policy_group_id: core.policy_group_id,
        created_at: core.created_at,
        updated_at: core.updated_at,
        profile: profile::get_profile_info(state, user, audience).await?,
    })
}

pub async fn to_user_infos(
    state: &impl SharedRuntimeState,
    users: Vec<user::Model>,
    audience: profile::AvatarAudience,
) -> Result<Vec<UserInfo>> {
    let profile_map = profile::get_profile_info_map(state, &users, audience).await?;
    let gravatar_base_url = profile::resolve_gravatar_base_url(state);

    Ok(users
        .into_iter()
        .map(|user| UserInfo {
            id: user.id,
            username: user.username.clone(),
            email: user.email.clone(),
            email_verified: local::is_email_verified(&user),
            pending_email: user.pending_email.clone(),
            role: user.role,
            status: user.status,
            must_change_password: user.must_change_password,
            storage_used: user.storage_used,
            storage_quota: user.storage_quota,
            policy_group_id: user.policy_group_id,
            created_at: user.created_at,
            updated_at: user.updated_at,
            profile: profile_map.get(&user.id).cloned().unwrap_or_else(|| {
                profile::build_profile_info(&user, None, audience, &gravatar_base_url)
            }),
        })
        .collect())
}

pub async fn get_me(
    state: &impl SharedRuntimeState,
    user_id: i64,
    access_token_expires_at: i64,
) -> Result<MeResponse> {
    let user = user_repo::find_by_id(state.reader_db(), user_id).await?;
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
        must_change_password: core.must_change_password,
        storage_used: core.storage_used,
        storage_quota: core.storage_quota,
        policy_group_id: core.policy_group_id,
        access_token_expires_at,
        created_at: core.created_at,
        updated_at: core.updated_at,
        preferences: prefs,
        profile: profile::get_profile_info(
            state,
            &user,
            profile::AvatarAudience::SelfUser,
        )
        .await?,
    })
}

pub async fn get_me_partial(
    state: &impl SharedRuntimeState,
    user_id: i64,
    access_token_expires_at: i64,
    fields: MeResponseFields,
) -> Result<MePartialResponse> {
    let user = user_repo::find_by_id(state.reader_db(), user_id).await?;
    let prefs = fields.preferences.then(|| parse_preferences(&user));
    let profile = if fields.profile {
        Some(
            profile::get_profile_info(
                state,
                &user,
                profile::AvatarAudience::SelfUser,
            )
            .await?,
        )
    } else {
        None
    };
    let core = user_core(&user);
    Ok(MePartialResponse {
        id: core.id,
        username: core.username,
        email: core.email,
        email_verified: core.email_verified,
        pending_email: core.pending_email,
        role: core.role,
        status: core.status,
        must_change_password: core.must_change_password,
        policy_group_id: core.policy_group_id,
        created_at: core.created_at,
        updated_at: core.updated_at,
        storage_used: fields.quota.then_some(core.storage_used),
        storage_quota: fields.quota.then_some(core.storage_quota),
        access_token_expires_at: fields.session.then_some(access_token_expires_at),
        preferences: prefs,
        profile,
    })
}

pub async fn get_self_info(state: &impl SharedRuntimeState, user_id: i64) -> Result<UserInfo> {
    let user = user_repo::find_by_id(state.reader_db(), user_id).await?;
    to_user_info(state, &user, profile::AvatarAudience::SelfUser).await
}

pub async fn list_paginated(
    state: &impl SharedRuntimeState,
    limit: u64,
    offset: u64,
    filters: UserListFilters,
) -> Result<OffsetPage<UserInfo>> {
    let page = load_offset_page(limit, offset, 100, |limit, offset| async move {
        let repo_filters = user_repo::AdminUserListFilters {
            keyword: filters.keyword.as_deref(),
            role: filters.role,
            status: filters.status,
            sort_by: filters.sort_by,
            sort_order: filters.sort_order,
        };
        user_repo::find_paginated(state.reader_db(), limit, offset, &repo_filters).await
    })
    .await?;

    Ok(OffsetPage::new(
        to_user_infos(
            state,
            page.items,
            profile::AvatarAudience::AdminUser,
        )
        .await?,
        page.total,
        page.limit,
        page.offset,
    ))
}

pub async fn get(state: &impl SharedRuntimeState, id: i64) -> Result<UserInfo> {
    let user = user_repo::find_by_id(state.reader_db(), id).await?;
    to_user_info(state, &user, profile::AvatarAudience::AdminUser).await
}
