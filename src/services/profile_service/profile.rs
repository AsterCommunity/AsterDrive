//! 用户资料服务子模块：`profile`。

use chrono::Utc;
use sea_orm::Set;

use crate::db::repository::{user_profile_repo, user_repo};
use crate::entities::{user, user_profile};
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::utils::char_count;

use super::info::{AvatarAudience, UserProfileInfo, build_profile_info, resolve_gravatar_base_url};
use super::shared::default_profile_active_model;

fn normalize_display_name(value: &str) -> Result<Option<String>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    if char_count(trimmed) > 64 {
        return Err(AsterError::validation_error(
            "display name must be 64 characters or fewer",
        ));
    }

    Ok(Some(trimmed.to_string()))
}

pub async fn get_profile_info(
    state: &impl SharedRuntimeState,
    user: &user::Model,
    audience: AvatarAudience,
) -> Result<UserProfileInfo> {
    let profile = user_profile_repo::find_by_user_id(state.reader_db(), user.id).await?;
    let gravatar_base_url = resolve_gravatar_base_url(state);
    Ok(build_profile_info(
        user,
        profile.as_ref(),
        audience,
        &gravatar_base_url,
    ))
}

pub async fn update_profile(
    state: &impl SharedRuntimeState,
    user_id: i64,
    display_name: Option<String>,
) -> Result<UserProfileInfo> {
    let user = user_repo::find_by_id(state.writer_db(), user_id).await?;
    let existing = user_profile_repo::find_by_user_id(state.writer_db(), user_id).await?;
    let gravatar_base_url = resolve_gravatar_base_url(state);

    let Some(display_name) = display_name else {
        return Ok(build_profile_info(
            &user,
            existing.as_ref(),
            AvatarAudience::SelfUser,
            &gravatar_base_url,
        ));
    };

    let normalized = normalize_display_name(&display_name)?;
    let now = Utc::now();

    let saved = match existing {
        Some(current) => {
            if current.display_name == normalized {
                current
            } else {
                let mut active: user_profile::ActiveModel = current.into();
                active.display_name = Set(normalized);
                active.updated_at = Set(now);
                user_profile_repo::update(state.writer_db(), active).await?
            }
        }
        None => {
            if normalized.is_none() {
                return Ok(build_profile_info(
                    &user,
                    None,
                    AvatarAudience::SelfUser,
                    &gravatar_base_url,
                ));
            }

            let mut active = default_profile_active_model(user_id, now);
            active.display_name = Set(normalized);
            user_profile_repo::create(state.writer_db(), active).await?
        }
    };

    Ok(build_profile_info(
        &user,
        Some(&saved),
        AvatarAudience::SelfUser,
        &gravatar_base_url,
    ))
}

pub async fn get_wopi_user_info(
    state: &impl SharedRuntimeState,
    user_id: i64,
) -> Result<Option<String>> {
    Ok(
        user_profile_repo::find_by_user_id(state.reader_db(), user_id)
            .await?
            .and_then(|profile| profile.wopi_user_info),
    )
}

pub async fn update_wopi_user_info(
    state: &impl SharedRuntimeState,
    user_id: i64,
    wopi_user_info: String,
) -> Result<()> {
    user_repo::find_by_id(state.writer_db(), user_id).await?;
    let existing = user_profile_repo::find_by_user_id(state.writer_db(), user_id).await?;
    let now = Utc::now();

    match existing {
        Some(current) => {
            if current.wopi_user_info == Some(wopi_user_info.clone()) {
                return Ok(());
            }

            let mut active: user_profile::ActiveModel = current.into();
            active.wopi_user_info = Set(Some(wopi_user_info));
            active.updated_at = Set(now);
            user_profile_repo::update(state.writer_db(), active).await?;
        }
        None => {
            let mut active = default_profile_active_model(user_id, now);
            active.wopi_user_info = Set(Some(wopi_user_info));
            user_profile_repo::create(state.writer_db(), active).await?;
        }
    }

    Ok(())
}
