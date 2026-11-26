use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::api::pagination::{AdminUserSortBy, SortBy};
use crate::services::{auth::local, user::profile};
use aster_drive_model::entities::user;
use aster_drive_model::types::{
    BrowserOpenMode, ColorPreset, LocaleTag, PrefViewMode, ThemeMode, UserConfig,
    UserPreferences as StoredUserPreferences, UserRole, UserStatus,
};
use aster_forge_api::SortOrder;

/// API-facing user preference payload: built-in preferences plus custom frontend KV entries.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct UserPreferences {
    /// UI theme mode; `None` keeps the stored or default value.
    pub theme_mode: Option<ThemeMode>,
    /// UI color preset; `None` keeps the stored or default value.
    pub color_preset: Option<ColorPreset>,
    /// Preferred content view mode; `None` keeps the stored or default value.
    pub view_mode: Option<PrefViewMode>,
    /// How external links are opened in the browser; `None` keeps the stored value.
    pub browser_open_mode: Option<BrowserOpenMode>,
    /// Default listing sort key; `None` keeps the stored or default value.
    pub sort_by: Option<SortBy>,
    /// Default listing sort direction; `None` keeps the stored or default value.
    pub sort_order: Option<SortOrder>,
    /// UI locale tag; `None` keeps the stored or default value.
    pub language: Option<LocaleTag>,
    /// IANA time zone used for displaying timestamps; `None` keeps the stored value.
    pub display_time_zone: Option<String>,
    /// Whether storage event streaming is enabled for this account.
    pub storage_event_stream_enabled: Option<bool>,
    /// Free-form frontend key/value entries persisted outside the built-in fields.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub custom: BTreeMap<String, serde_json::Value>,
}

impl UserPreferences {
    /// True when no built-in preference and no custom entry is set.
    pub fn is_empty(&self) -> bool {
        self.theme_mode.is_none()
            && self.color_preset.is_none()
            && self.view_mode.is_none()
            && self.browser_open_mode.is_none()
            && self.sort_by.is_none()
            && self.sort_order.is_none()
            && self.language.is_none()
            && self.display_time_zone.is_none()
            && self.storage_event_stream_enabled.is_none()
            && self.custom.is_empty()
    }
}

impl From<&UserConfig> for UserPreferences {
    fn from(config: &UserConfig) -> Self {
        let StoredUserPreferences {
            theme_mode,
            color_preset,
            view_mode,
            browser_open_mode,
            sort_by,
            sort_order,
            language,
            display_time_zone,
            storage_event_stream_enabled,
        } = config.preferences.clone();

        Self {
            theme_mode,
            color_preset,
            view_mode,
            browser_open_mode,
            sort_by,
            sort_order,
            language,
            display_time_zone,
            storage_event_stream_enabled,
            custom: config.extra.clone(),
        }
    }
}

impl From<UserConfig> for UserPreferences {
    fn from(config: UserConfig) -> Self {
        let UserConfig { preferences, extra } = config;
        let StoredUserPreferences {
            theme_mode,
            color_preset,
            view_mode,
            browser_open_mode,
            sort_by,
            sort_order,
            language,
            display_time_zone,
            storage_event_stream_enabled,
        } = preferences;

        Self {
            theme_mode,
            color_preset,
            view_mode,
            browser_open_mode,
            sort_by,
            sort_order,
            language,
            display_time_zone,
            storage_event_stream_enabled,
            custom: extra,
        }
    }
}

/// PATCH request:
/// - non-null built-in fields are merged into existing preferences
/// - `custom` entries are upserted
/// - `remove_custom_keys` entries are deleted
#[derive(Debug, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct UpdatePreferencesReq {
    /// New value for the theme mode; `None` leaves it unchanged.
    pub theme_mode: Option<ThemeMode>,
    /// New value for the color preset; `None` leaves it unchanged.
    pub color_preset: Option<ColorPreset>,
    /// New value for the view mode; `None` leaves it unchanged.
    pub view_mode: Option<PrefViewMode>,
    /// New value for the browser link behavior; `None` leaves it unchanged.
    pub browser_open_mode: Option<BrowserOpenMode>,
    /// New default listing sort key; `None` leaves it unchanged.
    pub sort_by: Option<SortBy>,
    /// New default listing sort direction; `None` leaves it unchanged.
    pub sort_order: Option<SortOrder>,
    /// New UI locale tag; `None` leaves it unchanged.
    pub language: Option<LocaleTag>,
    /// New IANA display time zone; `None` leaves it unchanged.
    pub display_time_zone: Option<String>,
    /// New storage event streaming toggle; `None` leaves it unchanged.
    pub storage_event_stream_enabled: Option<bool>,
    /// Custom entries upserted into the stored key/value map.
    #[serde(default)]
    pub custom: BTreeMap<String, serde_json::Value>,
    /// Custom keys deleted from the stored map before `custom` is applied.
    #[serde(default)]
    pub remove_custom_keys: Vec<String>,
}

/// 用户信息核心字段（不含 password_hash），用于 API 响应
#[derive(Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct UserCore {
    /// Database ID of the user.
    pub id: i64,
    /// Unique login name.
    pub username: String,
    /// Email address currently bound to the account.
    pub email: String,
    /// Whether the current email address has been verified.
    pub email_verified: bool,
    /// Email awaiting verification after a change request, before it replaces `email`.
    pub pending_email: Option<String>,
    /// Account role controlling admin capabilities.
    pub role: UserRole,
    /// Account status controlling login and activity.
    pub status: UserStatus,
    /// Whether the user must change their password before continuing.
    pub must_change_password: bool,
    /// Bytes of storage currently consumed by the account.
    pub storage_used: i64,
    /// Maximum storage allowed for the account, in bytes.
    pub storage_quota: i64,
    /// Storage policy group assigned by an admin, if any.
    pub policy_group_id: Option<i64>,
    /// When the account was created (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the account row was last updated (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// /auth/me 响应：用户信息 + 偏好设置
#[derive(Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct MeResponse {
    /// Database ID of the current user.
    pub id: i64,
    /// Unique login name.
    pub username: String,
    /// Email address currently bound to the account.
    pub email: String,
    /// Whether the current email address has been verified.
    pub email_verified: bool,
    /// Email awaiting verification after a change request.
    pub pending_email: Option<String>,
    /// Account role controlling admin capabilities.
    pub role: UserRole,
    /// Account status controlling login and activity.
    pub status: UserStatus,
    /// Whether the user must change their password before continuing.
    pub must_change_password: bool,
    /// Bytes of storage currently consumed by the account.
    pub storage_used: i64,
    /// Maximum storage allowed for the account, in bytes.
    pub storage_quota: i64,
    /// Storage policy group assigned by an admin, if any.
    pub policy_group_id: Option<i64>,
    /// Unix timestamp (seconds) when the current access token expires.
    pub access_token_expires_at: i64,
    /// When the account was created (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the account row was last updated (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Stored preferences of the current user.
    pub preferences: Option<UserPreferences>,
    /// Display name and avatar resolved for the self audience.
    pub profile: profile::UserProfileInfo,
}

/// Selectable field group of the partial `/auth/me` response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeResponseField {
    /// Display name and avatar group.
    Profile,
    /// Stored preferences group.
    Preferences,
    /// Storage usage and quota group.
    Quota,
    /// Access token expiry group.
    Session,
}

/// Per-group selection of which optional parts the partial `/auth/me`
/// response should include.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeResponseFields {
    /// Include the display name and avatar group.
    pub profile: bool,
    /// Include the stored preferences group.
    pub preferences: bool,
    /// Include storage usage and quota.
    pub quota: bool,
    /// Include the access token expiry.
    pub session: bool,
}

impl MeResponseFields {
    /// Selection that includes every optional group.
    pub fn all() -> Self {
        Self {
            profile: true,
            preferences: true,
            quota: true,
            session: true,
        }
    }

    /// Builds the selection from a list of requested field groups.
    pub fn from_fields(fields: impl IntoIterator<Item = MeResponseField>) -> Self {
        let mut selected = Self {
            profile: false,
            preferences: false,
            quota: false,
            session: false,
        };
        for field in fields {
            match field {
                MeResponseField::Profile => selected.profile = true,
                MeResponseField::Preferences => selected.preferences = true,
                MeResponseField::Quota => selected.quota = true,
                MeResponseField::Session => selected.session = true,
            }
        }
        selected
    }
}

impl Default for MeResponseFields {
    fn default() -> Self {
        Self::all()
    }
}

/// Partial `/auth/me` response. The always-needed account identity is kept
/// stable, while heavier or narrow-use groups are serialized only when selected.
#[derive(Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct MePartialResponse {
    /// Database ID of the current user.
    pub id: i64,
    /// Unique login name.
    pub username: String,
    /// Email address currently bound to the account.
    pub email: String,
    /// Whether the current email address has been verified.
    pub email_verified: bool,
    /// Email awaiting verification after a change request.
    pub pending_email: Option<String>,
    /// Account role controlling admin capabilities.
    pub role: UserRole,
    /// Account status controlling login and activity.
    pub status: UserStatus,
    /// Whether the user must change their password before continuing.
    pub must_change_password: bool,
    /// Storage policy group assigned by an admin, if any.
    pub policy_group_id: Option<i64>,
    /// When the account was created (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the account row was last updated (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Storage consumed; present only when the quota group is selected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_used: Option<i64>,
    /// Maximum storage allowed, in bytes; present only when the quota group is selected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_quota: Option<i64>,
    /// Unix timestamp (seconds) when the current access token expires;
    /// present only when the session group is selected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token_expires_at: Option<i64>,
    /// Preferences of the current user; the inner `None` distinguishes
    /// "selected but unset" from "not selected".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferences: Option<Option<UserPreferences>>,
    /// Display name and avatar; present only when the profile group is selected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<profile::UserProfileInfo>,
}

/// 通用用户响应：核心字段 + profile
#[derive(Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct UserInfo {
    /// Database ID of the user.
    pub id: i64,
    /// Unique login name.
    pub username: String,
    /// Email address currently bound to the account.
    pub email: String,
    /// Whether the current email address has been verified.
    pub email_verified: bool,
    /// Email awaiting verification after a change request.
    pub pending_email: Option<String>,
    /// Account role controlling admin capabilities.
    pub role: UserRole,
    /// Account status controlling login and activity.
    pub status: UserStatus,
    /// Whether the user must change their password before continuing.
    pub must_change_password: bool,
    /// Bytes of storage currently consumed by the account.
    pub storage_used: i64,
    /// Maximum storage allowed for the account, in bytes.
    pub storage_quota: i64,
    /// Storage policy group assigned by an admin, if any.
    pub policy_group_id: Option<i64>,
    /// When the account was created (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the account row was last updated (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Display name and avatar, resolved for the target audience.
    pub profile: profile::UserProfileInfo,
}

/// Filter and sort parameters for the admin user list.
#[derive(Debug, Clone)]
pub struct UserListFilters {
    /// Case-insensitive substring matched against username and email.
    pub keyword: Option<String>,
    /// Only list users with this role.
    pub role: Option<UserRole>,
    /// Only list users with this status.
    pub status: Option<UserStatus>,
    /// Column to sort the results by.
    pub sort_by: AdminUserSortBy,
    /// Direction to sort the results in.
    pub sort_order: SortOrder,
}

impl Default for UserListFilters {
    fn default() -> Self {
        Self {
            keyword: None,
            role: None,
            status: None,
            sort_by: AdminUserSortBy::CreatedAt,
            sort_order: SortOrder::Desc,
        }
    }
}

impl UserListFilters {
    /// Builds filters from raw query parameters, trimming the keyword and
    /// dropping empty strings.
    pub fn from_inputs(
        keyword: Option<&str>,
        role: Option<UserRole>,
        status: Option<UserStatus>,
        sort_by: AdminUserSortBy,
        sort_order: SortOrder,
    ) -> Self {
        Self {
            keyword: keyword
                .map(str::trim)
                .filter(|keyword| !keyword.is_empty())
                .map(str::to_owned),
            role,
            status,
            sort_by,
            sort_order,
        }
    }
}

/// Lightweight user identity for embedding in admin list/detail responses.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct UserSummary {
    /// Database ID of the user.
    pub id: i64,
    /// Unique login name.
    pub username: String,
    /// Display name and avatar, resolved for the target audience.
    pub profile: profile::UserProfileInfo,
}

/// Projects a user row onto the API-facing [`UserCore`] shape, resolving
/// email verification through the auth policy.
pub(super) fn user_core(user: &user::Model) -> UserCore {
    UserCore {
        id: user.id,
        username: user.username.clone(),
        email: user.email.clone(),
        email_verified: local::is_email_verified(user),
        pending_email: user.pending_email.clone(),
        role: user.role,
        status: user.status,
        must_change_password: user.must_change_password,
        storage_used: user.storage_used,
        storage_quota: user.storage_quota,
        policy_group_id: user.policy_group_id,
        created_at: user.created_at,
        updated_at: user.updated_at,
    }
}
