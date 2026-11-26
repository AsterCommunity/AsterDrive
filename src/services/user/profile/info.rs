//! 用户资料服务子模块：`info`。

use std::collections::HashMap;

use md5::{Digest, Md5};
use serde::Serialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::runtime::SharedRuntimeState;
use aster_drive_model::entities::{user, user_profile};
use aster_drive_model::types::AvatarSource;

use super::shared::{AVATAR_SIZE_LG, AVATAR_SIZE_SM, stored_avatar_prefix};

const DEFAULT_GRAVATAR_BASE_URL: &str = "https://www.gravatar.com/avatar";

#[derive(Debug, Clone, Copy)]
/// Selects which API base paths generated avatar URLs point at.
pub enum AvatarAudience {
    /// The signed-in user viewing their own profile (`/auth/profile/...`).
    SelfUser,
    /// An admin viewing another user (`/admin/users/...`).
    AdminUser,
}

/// Avatar resolution for one user, as embedded in profile responses.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AvatarInfo {
    /// Currently active avatar source.
    pub source: AvatarSource,
    /// Avatar URL for the 512px variant; `None` when no avatar is available.
    pub url_512: Option<String>,
    /// Avatar URL for the 1024px variant; `None` when no avatar is available.
    pub url_1024: Option<String>,
    /// Monotonic version used for cache busting; 0 before the first avatar mutation.
    pub version: i32,
}

/// Display profile of a user: display name plus resolved avatar info.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct UserProfileInfo {
    /// Free-form display name; `None` falls back to the username in the UI.
    pub display_name: Option<String>,
    /// Avatar source and URLs for the available size variants.
    pub avatar: AvatarInfo,
}

/// Outcome of an avatar upload attempt.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AvatarUploadResult {
    /// Profile after the attempt: the new state when applied, otherwise the
    /// unchanged profile that superseded this submission.
    pub profile: UserProfileInfo,
    /// Whether this submission won the version check and was applied.
    pub applied: bool,
}

/// Returns the configured Gravatar base URL, falling back to the official
/// endpoint when unset or blank.
pub fn resolve_gravatar_base_url(state: &impl SharedRuntimeState) -> String {
    let base_url = state
        .runtime_config()
        .get_string_or("gravatar_base_url", DEFAULT_GRAVATAR_BASE_URL);

    if base_url.trim().is_empty() {
        DEFAULT_GRAVATAR_BASE_URL.to_string()
    } else {
        base_url
    }
}

fn gravatar_hash(email: &str) -> String {
    let normalized = email.trim().to_lowercase();
    let mut hasher = Md5::new();
    hasher.update(normalized.as_bytes());
    aster_forge_crypto::bytes_to_hex(&hasher.finalize())
}

fn gravatar_url(email: &str, size: u32, base_url: &str) -> String {
    let hash = gravatar_hash(email);
    let base = base_url.trim_end_matches('/');
    format!("{base}/{hash}?d=identicon&s={size}&r=g")
}

fn avatar_api_path(user_id: i64, version: i32, size: u32, audience: AvatarAudience) -> String {
    match audience {
        AvatarAudience::SelfUser => format!("/auth/profile/avatar/{size}?v={version}"),
        AvatarAudience::AdminUser => {
            format!("/admin/users/{user_id}/avatar/{size}?v={version}")
        }
    }
}

fn share_public_avatar_api_path(share_token: &str, version: i32, size: u32) -> String {
    format!("/s/{share_token}/avatar/{size}?v={version}")
}

fn build_avatar_info(
    user: &user::Model,
    profile: Option<&user_profile::Model>,
    audience: AvatarAudience,
    gravatar_base_url: &str,
) -> AvatarInfo {
    let source = profile
        .map(|profile| profile.avatar_source)
        .unwrap_or(AvatarSource::None);
    let version = profile.map(|profile| profile.avatar_version).unwrap_or(0);

    match source {
        AvatarSource::None => AvatarInfo {
            source,
            url_512: None,
            url_1024: None,
            version,
        },
        AvatarSource::Gravatar => AvatarInfo {
            source,
            url_512: Some(gravatar_url(&user.email, AVATAR_SIZE_SM, gravatar_base_url)),
            url_1024: Some(gravatar_url(&user.email, AVATAR_SIZE_LG, gravatar_base_url)),
            version,
        },
        AvatarSource::Upload => {
            let has_upload = stored_avatar_prefix(profile).is_some();

            AvatarInfo {
                source,
                url_512: has_upload
                    .then(|| avatar_api_path(user.id, version, AVATAR_SIZE_SM, audience)),
                url_1024: has_upload
                    .then(|| avatar_api_path(user.id, version, AVATAR_SIZE_LG, audience)),
                version,
            }
        }
    }
}

/// Assembles [`UserProfileInfo`] from a user row and its optional profile
/// row; users without a profile row get a default avatar info.
pub fn build_profile_info(
    user: &user::Model,
    profile: Option<&user_profile::Model>,
    audience: AvatarAudience,
    gravatar_base_url: &str,
) -> UserProfileInfo {
    UserProfileInfo {
        display_name: profile.and_then(|profile| profile.display_name.clone()),
        avatar: build_avatar_info(user, profile, audience, gravatar_base_url),
    }
}

/// Builds [`AvatarInfo`] for the public share page, pointing upload avatar
/// URLs at the share-token endpoint instead of authenticated API paths.
pub fn build_share_public_avatar_info(
    user: &user::Model,
    profile: Option<&user_profile::Model>,
    share_token: &str,
    gravatar_base_url: &str,
) -> AvatarInfo {
    let source = profile
        .map(|profile| profile.avatar_source)
        .unwrap_or(AvatarSource::None);
    let version = profile.map(|profile| profile.avatar_version).unwrap_or(0);

    match source {
        AvatarSource::None => AvatarInfo {
            source,
            url_512: None,
            url_1024: None,
            version,
        },
        AvatarSource::Gravatar => AvatarInfo {
            source,
            url_512: Some(gravatar_url(&user.email, AVATAR_SIZE_SM, gravatar_base_url)),
            url_1024: Some(gravatar_url(&user.email, AVATAR_SIZE_LG, gravatar_base_url)),
            version,
        },
        AvatarSource::Upload => {
            let has_upload = stored_avatar_prefix(profile).is_some();

            AvatarInfo {
                source,
                url_512: has_upload
                    .then(|| share_public_avatar_api_path(share_token, version, AVATAR_SIZE_SM)),
                url_1024: has_upload
                    .then(|| share_public_avatar_api_path(share_token, version, AVATAR_SIZE_LG)),
                version,
            }
        }
    }
}

/// Batch variant of [`build_profile_info`]: resolves profile rows for many
/// users in one reader query, keyed by user ID.
pub async fn get_profile_info_map(
    state: &impl SharedRuntimeState,
    users: &[user::Model],
    audience: AvatarAudience,
) -> crate::errors::Result<HashMap<i64, UserProfileInfo>> {
    let user_ids: Vec<i64> = users.iter().map(|user| user.id).collect();
    let profiles =
        crate::db::repository::user_profile_repo::find_by_user_ids(state.reader_db(), &user_ids)
            .await?;
    let gravatar_base_url = resolve_gravatar_base_url(state);

    Ok(users
        .iter()
        .map(|user| {
            (
                user.id,
                build_profile_info(user, profiles.get(&user.id), audience, &gravatar_base_url),
            )
        })
        .collect())
}
