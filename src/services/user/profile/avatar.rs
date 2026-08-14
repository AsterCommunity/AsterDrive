//! 用户资料服务子模块：`avatar`。

use actix_multipart::Multipart;
use actix_web::HttpResponse;
use aster_forge_db::transaction;
use chrono::Utc;
use sea_orm::Set;

use crate::api::constants::YEAR_SECS;
use crate::config::{avatar, operations};
use crate::db::repository::{user_profile_repo, user_repo};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::media::processing;
use aster_drive_model::entities::user_profile;
use aster_drive_model::types::AvatarSource;

use super::avatar_image::{StagedAvatarUpload, stage_avatar_upload};
use super::avatar_storage::{
    avatar_staging_rendered_dir, avatar_variant_file_path, cleanup_avatar_staging,
    cleanup_local_avatar_prefix, delete_upload_objects, resolve_stored_avatar_variant_path,
    user_avatar_dir, user_avatar_prefix,
};
use super::info::{
    AvatarAudience, AvatarUploadResult, UserProfileInfo, build_profile_info,
    resolve_gravatar_base_url,
};
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
    let _publish_guard = state
        .runtime_config()
        .avatar_render_runtime()
        .acquire_publish()
        .await;
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
) -> Result<AvatarUploadResult> {
    let user = user_repo::find_by_id(state.writer_db(), user_id).await?;
    let base_profile = user_profile_repo::find_by_user_id(state.writer_db(), user_id).await?;
    let avatar_root_dir = avatar::resolve_local_avatar_root_dir(state.runtime_config())?;
    let source_limit = operations::avatar_max_upload_size_bytes(state.runtime_config());
    state
        .metrics()
        .set_avatar_budget_bytes("source_configured", source_limit as u64);
    state.metrics().set_avatar_budget_bytes(
        "source_hard_ceiling",
        operations::MAX_AVATAR_UPLOAD_SIZE_BYTES,
    );
    let staged = match stage_avatar_upload(payload, source_limit, &avatar_root_dir).await {
        Ok(staged) => staged,
        Err(error) => {
            state.metrics().record_avatar_rejection("source");
            state.metrics().record_avatar_upload("failed");
            return Err(error);
        }
    };
    state
        .metrics()
        .record_avatar_source_bytes(staged.source_size);
    let submission_id = staged.submission_id;
    let result = publish_staged_avatar(
        state,
        &user,
        base_profile.as_ref(),
        &staged,
        &avatar_root_dir,
    )
    .await;
    match &result {
        Ok(result) => state.metrics().record_avatar_upload(if result.applied {
            "applied"
        } else {
            "superseded"
        }),
        Err(_) => state.metrics().record_avatar_upload("failed"),
    }
    cleanup_avatar_staging(&avatar_root_dir, submission_id).await;
    result
}

fn avatar_revision_matches(
    base: Option<&user_profile::Model>,
    current: Option<&user_profile::Model>,
) -> bool {
    let base_version = base.map_or(0, |profile| profile.avatar_version);
    let current_version = current.map_or(0, |profile| profile.avatar_version);
    base_version == current_version
}

fn next_avatar_version(current: Option<&user_profile::Model>) -> Result<i32> {
    current.map_or(Ok(1), |profile| {
        profile
            .avatar_version
            .checked_add(1)
            .ok_or_else(|| AsterError::internal_error("avatar version exhausted"))
    })
}

async fn write_staged_avatar_variants(
    avatar_root_dir: &std::path::Path,
    staged: &StagedAvatarUpload,
    processed: &crate::services::media::processing::ProcessedAvatar,
) -> Result<std::path::PathBuf> {
    let rendered_dir = avatar_staging_rendered_dir(avatar_root_dir, staged.submission_id);
    let small_path = avatar_variant_file_path(&rendered_dir, AVATAR_SIZE_SM);
    let large_path = avatar_variant_file_path(&rendered_dir, AVATAR_SIZE_LG);

    write_local_avatar(&large_path, &processed.large_bytes).await?;
    if let Err(error) = write_local_avatar(&small_path, &processed.small_bytes).await {
        cleanup_local_avatar_prefix(&rendered_dir, avatar_root_dir).await;
        return Err(error);
    }
    Ok(rendered_dir)
}

async fn publish_staged_avatar(
    state: &PrimaryAppState,
    user: &aster_drive_model::entities::user::Model,
    base_profile: Option<&user_profile::Model>,
    staged: &StagedAvatarUpload,
    avatar_root_dir: &std::path::Path,
) -> Result<AvatarUploadResult> {
    tracing::debug!(
        user_id = user.id,
        submission_id = %staged.submission_id,
        source_bytes = staged.source_size,
        "rendering staged avatar"
    );
    let processed =
        processing::process_staged_avatar(state, &staged.file_name, staged.source_path.clone())
            .await?;
    let rendered_dir = write_staged_avatar_variants(avatar_root_dir, staged, &processed).await?;
    let output_size = i64::try_from(
        processed
            .small_bytes
            .len()
            .checked_add(processed.large_bytes.len())
            .ok_or_else(|| AsterError::internal_error("avatar output size overflow"))?,
    )
    .map_err(|_| AsterError::internal_error("avatar output size exceeds i64"))?;
    drop(processed);

    let _publish_guard = state
        .runtime_config()
        .avatar_render_runtime()
        .acquire_publish()
        .await;
    enum PublishOutcome {
        Superseded {
            user: aster_drive_model::entities::user::Model,
            current_profile: Option<user_profile::Model>,
        },
        Applied {
            user: aster_drive_model::entities::user::Model,
            saved: user_profile::Model,
            previous: Option<user_profile::Model>,
        },
    }

    let published_prefix = std::sync::Arc::new(parking_lot::Mutex::new(None));
    let callback_published_prefix = published_prefix.clone();
    let transaction_user_id = user.id;
    let transaction_base_profile = base_profile.cloned();
    let transaction_avatar_root_dir = avatar_root_dir.to_path_buf();
    let transaction_rendered_dir = rendered_dir.clone();
    let transaction_result: Result<PublishOutcome> = transaction::with_transaction_retry(
        state.writer_db(),
        &aster_forge_db::retry::RetryConfig {
            max_retries: 0,
            ..aster_forge_db::retry::RetryConfig::deadlock()
        },
        move |txn| {
            let callback_published_prefix = callback_published_prefix.clone();
            let base_profile = transaction_base_profile.clone();
            let avatar_root_dir = transaction_avatar_root_dir.clone();
            let rendered_dir = transaction_rendered_dir.clone();
            Box::pin(async move {
                let locked_user = user_repo::lock_by_id(txn, transaction_user_id).await?;
                let current_profile =
                    user_profile_repo::lock_by_user_id(txn, transaction_user_id).await?;

                if !avatar_revision_matches(base_profile.as_ref(), current_profile.as_ref()) {
                    return Ok(PublishOutcome::Superseded {
                        user: locked_user,
                        current_profile,
                    });
                }

                user_repo::check_quota(txn, transaction_user_id, output_size).await?;
                let version = next_avatar_version(current_profile.as_ref())?;
                let prefix_key = user_avatar_prefix(transaction_user_id, version);
                let prefix = user_avatar_dir(&avatar_root_dir, transaction_user_id, version);
                let Some(prefix_parent) = prefix.parent() else {
                    return Err(AsterError::storage_driver_error(
                        "avatar destination has no parent directory",
                    ));
                };
                cleanup_local_avatar_prefix(&prefix, &avatar_root_dir).await;
                tokio::fs::create_dir_all(prefix_parent)
                    .await
                    .map_aster_err_ctx(
                        "create avatar user directory",
                        AsterError::storage_driver_error,
                    )?;
                tokio::fs::rename(&rendered_dir, &prefix)
                    .await
                    .map_aster_err_ctx(
                        "publish avatar variants",
                        AsterError::storage_driver_error,
                    )?;
                *callback_published_prefix.lock() = Some(prefix);

                let now = Utc::now();
                let saved = match current_profile.clone() {
                    Some(current) => {
                        let mut active: user_profile::ActiveModel = current.into();
                        active.avatar_source = Set(AvatarSource::Upload);
                        active.avatar_key = Set(Some(prefix_key));
                        active.avatar_version = Set(version);
                        active.updated_at = Set(now);
                        user_profile_repo::update(txn, active).await?
                    }
                    None => {
                        let mut active = default_profile_active_model(transaction_user_id, now);
                        active.avatar_source = Set(AvatarSource::Upload);
                        active.avatar_key = Set(Some(prefix_key));
                        active.avatar_version = Set(version);
                        user_profile_repo::create(txn, active).await?
                    }
                };
                Ok(PublishOutcome::Applied {
                    user: locked_user,
                    saved,
                    previous: current_profile,
                })
            })
        },
        |_| false,
    )
    .await;

    let outcome = match transaction_result {
        Ok(outcome) => outcome,
        Err(error) => {
            if !error.database_commit_outcome_uncertain() {
                let prefix = published_prefix.lock().clone();
                if let Some(prefix) = prefix.as_ref() {
                    cleanup_local_avatar_prefix(prefix, avatar_root_dir).await;
                }
            }
            return Err(error);
        }
    };

    let gravatar_base_url = resolve_gravatar_base_url(state);
    match outcome {
        PublishOutcome::Superseded {
            user,
            current_profile,
        } => Ok(AvatarUploadResult {
            profile: build_profile_info(
                &user,
                current_profile.as_ref(),
                AvatarAudience::SelfUser,
                &gravatar_base_url,
            ),
            applied: false,
        }),
        PublishOutcome::Applied {
            user,
            saved,
            previous,
        } => {
            if let Some(previous) = previous.as_ref() {
                delete_upload_objects(state, previous).await;
            }
            Ok(AvatarUploadResult {
                profile: build_profile_info(
                    &user,
                    Some(&saved),
                    AvatarAudience::SelfUser,
                    &gravatar_base_url,
                ),
                applied: true,
            })
        }
    }
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

    let _publish_guard = state
        .runtime_config()
        .avatar_render_runtime()
        .acquire_publish()
        .await;
    let gravatar_base_url = resolve_gravatar_base_url(state);
    let (user, saved, previous) = transaction::with_transaction(state.writer_db(), async |txn| {
        let user = user_repo::lock_by_id(txn, user_id).await?;
        let existing = user_profile_repo::lock_by_user_id(txn, user_id).await?;

        if existing.is_none() && source == AvatarSource::None {
            return Ok::<_, AsterError>((user, None, None));
        }

        let now = Utc::now();
        let saved = match existing.clone() {
            Some(current) => {
                let next_version = next_avatar_version(Some(&current))?;
                let mut active: user_profile::ActiveModel = current.into();
                active.avatar_source = Set(source);
                active.avatar_key = Set(None);
                active.avatar_version = Set(next_version);
                active.updated_at = Set(now);
                user_profile_repo::update(txn, active).await?
            }
            None => {
                let mut active = default_profile_active_model(user_id, now);
                active.avatar_source = Set(source);
                active.avatar_version = Set(1);
                user_profile_repo::create(txn, active).await?
            }
        };
        Ok::<_, AsterError>((user, Some(saved), existing))
    })
    .await?;

    if let Some(previous) = previous.as_ref() {
        delete_upload_objects(state, previous).await;
    }

    Ok(build_profile_info(
        &user,
        saved.as_ref(),
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

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::{avatar_revision_matches, next_avatar_version};
    use aster_drive_model::entities::user_profile;
    use aster_drive_model::types::AvatarSource;

    fn profile(version: i32, source: AvatarSource) -> user_profile::Model {
        user_profile::Model {
            user_id: 42,
            display_name: None,
            wopi_user_info: None,
            avatar_source: source,
            avatar_key: None,
            avatar_version: version,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn avatar_revision_fence_tracks_avatar_mutations_not_profile_creation() {
        let profile_only = profile(0, AvatarSource::None);
        let uploaded = profile(1, AvatarSource::Upload);
        let gravatar = profile(1, AvatarSource::Gravatar);

        assert!(avatar_revision_matches(None, None));
        assert!(avatar_revision_matches(None, Some(&profile_only)));
        assert!(avatar_revision_matches(
            Some(&profile_only),
            Some(&profile_only)
        ));
        assert!(!avatar_revision_matches(None, Some(&uploaded)));
        assert!(!avatar_revision_matches(
            Some(&profile_only),
            Some(&gravatar)
        ));
    }

    #[test]
    fn avatar_version_increment_is_checked() {
        assert_eq!(next_avatar_version(None).unwrap(), 1);
        assert_eq!(
            next_avatar_version(Some(&profile(41, AvatarSource::Upload))).unwrap(),
            42
        );
        assert!(next_avatar_version(Some(&profile(i32::MAX, AvatarSource::Upload))).is_err());
    }
}
