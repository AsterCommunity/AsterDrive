//! 用户资料服务子模块：`shared`。

use chrono::{DateTime, Utc};
use sea_orm::Set;

use crate::entities::user_profile;
use crate::types::AvatarSource;

pub(crate) const MAX_AVATAR_DECODE_ALLOC: u64 = 128 * 1024 * 1024;
pub(crate) const AVATAR_SIZE_SM: u32 = 512;
pub(crate) const AVATAR_SIZE_LG: u32 = 1024;

pub(super) fn stored_avatar_prefix(profile: Option<&user_profile::Model>) -> Option<&str> {
    profile
        .and_then(|profile| profile.avatar_key.as_deref())
        .map(str::trim)
        .filter(|prefix| !prefix.is_empty())
}

pub(super) fn default_profile_active_model(
    user_id: i64,
    now: DateTime<Utc>,
) -> user_profile::ActiveModel {
    user_profile::ActiveModel {
        user_id: Set(user_id),
        display_name: Set(None),
        wopi_user_info: Set(None),
        avatar_source: Set(AvatarSource::None),
        avatar_key: Set(None),
        avatar_version: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
    }
}
