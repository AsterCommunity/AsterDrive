//! 用户资料服务子模块：`avatar`。

use actix_multipart::Multipart;
use actix_web::HttpResponse;
use chrono::Utc;
use sea_orm::Set;

use crate::api::constants::YEAR_SECS;
use crate::config::{avatar, operations};
use crate::db::repository::{user_profile_repo, user_repo};
use crate::entities::user_profile;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::media::processing;
use crate::types::AvatarSource;

use super::avatar_image::read_avatar_upload;
use super::avatar_storage::{
    avatar_variant_file_path, cleanup_local_avatar_prefix, delete_upload_objects,
    resolve_stored_avatar_variant_path, user_avatar_dir, user_avatar_prefix,
};
use super::info::{AvatarAudience, UserProfileInfo, build_profile_info, resolve_gravatar_base_url};
use super::shared::{
    AVATAR_SIZE_LG, AVATAR_SIZE_SM, default_profile_active_model, stored_avatar_prefix,
};

async fn write_local_avatar(path: &std::path::Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_aster_err(AsterError::storage_driver_error)?;
    }

    tokio::fs::write(path, data)
        .await
        .map_aster_err(AsterError::storage_driver_error)?;
    Ok(())
}

pub async fn cleanup_avatar_upload(state: &impl SharedRuntimeState, user_id: i64) -> Result<()> {
    let profile = user_profile_repo::find_by_user_id(state.writer_db(), user_id).await?;
    if let Some(profile) = profile.as_ref() {
        delete_upload_objects(state, profile).await;
    }
    Ok(())
}

pub async fn upload_avatar(
    state: &PrimaryAppState,
    user_id: i64,
    payload: &mut Multipart,
) -> Result<UserProfileInfo> {
    let user = user_repo::find_by_id(state.writer_db(), user_id).await?;
    let existing = user_profile_repo::find_by_user_id(state.writer_db(), user_id).await?;
    let upload_data = read_avatar_upload(
        payload,
        operations::avatar_max_upload_size_bytes(state.runtime_config()),
    )
    .await?;
    let processed_avatar =
        processing::process_avatar_upload(state, &upload_data.file_name, upload_data.bytes).await?;
    user_repo::check_quota(
        state.writer_db(),
        user_id,
        i64::try_from(processed_avatar.small_bytes.len() + processed_avatar.large_bytes.len())
            .unwrap_or(i64::MAX),
    )
    .await?;
    let version = existing
        .as_ref()
        .map(|profile| profile.avatar_version.saturating_add(1))
        .unwrap_or(1);
    let avatar_root_dir = avatar::resolve_local_avatar_root_dir(state.runtime_config())?;
    let prefix_key = user_avatar_prefix(user_id, version);
    let prefix = user_avatar_dir(&avatar_root_dir, user_id, version);
    let small_path = avatar_variant_file_path(&prefix, AVATAR_SIZE_SM);
    let large_path = avatar_variant_file_path(&prefix, AVATAR_SIZE_LG);

    write_local_avatar(&small_path, &processed_avatar.small_bytes).await?;
    if let Err(e) = write_local_avatar(&large_path, &processed_avatar.large_bytes).await {
        cleanup_local_avatar_prefix(&prefix, &avatar_root_dir).await;
        return Err(e);
    }

    let now = Utc::now();
    let saved = match existing.clone() {
        Some(current) => {
            let mut active: user_profile::ActiveModel = current.into();
            active.avatar_source = Set(AvatarSource::Upload);
            active.avatar_key = Set(Some(prefix_key.clone()));
            active.avatar_version = Set(version);
            active.updated_at = Set(now);
            user_profile_repo::update(state.writer_db(), active).await
        }
        None => {
            let mut active = default_profile_active_model(user_id, now);
            active.avatar_source = Set(AvatarSource::Upload);
            active.avatar_key = Set(Some(prefix_key.clone()));
            active.avatar_version = Set(version);
            user_profile_repo::create(state.writer_db(), active).await
        }
    };

    let saved = match saved {
        Ok(model) => model,
        Err(err) => {
            cleanup_local_avatar_prefix(&prefix, &avatar_root_dir).await;
            return Err(err);
        }
    };

    if let Some(previous) = existing.as_ref() {
        delete_upload_objects(state, previous).await;
    }

    let gravatar_base_url = resolve_gravatar_base_url(state);
    Ok(build_profile_info(
        &user,
        Some(&saved),
        AvatarAudience::SelfUser,
        &gravatar_base_url,
    ))
}

pub async fn set_avatar_source(
    state: &impl SharedRuntimeState,
    user_id: i64,
    source: AvatarSource,
) -> Result<UserProfileInfo> {
    if source == AvatarSource::Upload {
        return Err(AsterError::validation_error(
            "upload avatar source must use the upload endpoint",
        ));
    }

    let user = user_repo::find_by_id(state.writer_db(), user_id).await?;
    let existing = user_profile_repo::find_by_user_id(state.writer_db(), user_id).await?;
    let gravatar_base_url = resolve_gravatar_base_url(state);

    if existing.is_none() && source == AvatarSource::None {
        return Ok(build_profile_info(
            &user,
            None,
            AvatarAudience::SelfUser,
            &gravatar_base_url,
        ));
    }

    let now = Utc::now();
    let saved = match existing.clone() {
        Some(current) => {
            let next_version = current.avatar_version.saturating_add(1);
            let mut active: user_profile::ActiveModel = current.into();
            active.avatar_source = Set(source);
            active.avatar_key = Set(None);
            active.avatar_version = Set(next_version);
            active.updated_at = Set(now);
            user_profile_repo::update(state.writer_db(), active).await?
        }
        None => {
            let mut active = default_profile_active_model(user_id, now);
            active.avatar_source = Set(source);
            user_profile_repo::create(state.writer_db(), active).await?
        }
    };

    if let Some(previous) = existing.as_ref() {
        delete_upload_objects(state, previous).await;
    }

    Ok(build_profile_info(
        &user,
        Some(&saved),
        AvatarAudience::SelfUser,
        &gravatar_base_url,
    ))
}

fn validate_avatar_size(size: u32) -> Result<u32> {
    match size {
        AVATAR_SIZE_SM | AVATAR_SIZE_LG => Ok(size),
        _ => Err(AsterError::validation_error(
            "avatar size must be 512 or 1024",
        )),
    }
}

pub async fn get_avatar_bytes(
    state: &impl SharedRuntimeState,
    user_id: i64,
    size: u32,
) -> Result<Vec<u8>> {
    let size = validate_avatar_size(size)?;
    user_repo::find_by_id(state.reader_db(), user_id).await?;
    let profile = user_profile_repo::find_by_user_id(state.reader_db(), user_id)
        .await?
        .ok_or_else(|| AsterError::record_not_found(format!("profile for user #{user_id}")))?;

    if profile.avatar_source != AvatarSource::Upload {
        return Err(AsterError::record_not_found(format!(
            "user #{user_id} does not have an uploaded avatar"
        )));
    }

    stored_avatar_prefix(Some(&profile))
        .ok_or_else(|| AsterError::record_not_found("avatar key missing"))?;
    let avatar_root_dir = avatar::resolve_local_avatar_root_dir(state.runtime_config())?;
    let path =
        resolve_stored_avatar_variant_path(&avatar_root_dir, &profile, size).ok_or_else(|| {
            tracing::warn!(
                user_id = profile.user_id,
                avatar_version = profile.avatar_version,
                "reject invalid stored avatar key"
            );
            AsterError::record_not_found("avatar key invalid")
        })?;
    tokio::fs::read(&path).await.map_aster_err_with(|| {
        AsterError::record_not_found(format!("avatar object {}", path.display()))
    })
}

pub fn avatar_image_response(bytes: Vec<u8>) -> HttpResponse {
    HttpResponse::Ok()
        .content_type("image/webp")
        .insert_header((
            "Cache-Control",
            format!("public, max-age={YEAR_SECS}, immutable"),
        ))
        .body(bytes)
}
