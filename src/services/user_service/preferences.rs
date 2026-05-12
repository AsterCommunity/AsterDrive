use chrono::Utc;
use chrono_tz::Tz;
use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};
use std::collections::{BTreeMap, BTreeSet};

use crate::db::repository::user_repo;
use crate::entities::user;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::types::{StoredUserConfig, UserConfig};

use super::models::{UpdatePreferencesReq, UserPreferences};

const DISPLAY_TIME_ZONE_BROWSER: &str = "browser";
const RESERVED_PREFERENCE_KEYS: &[&str] = &[
    "theme_mode",
    "color_preset",
    "view_mode",
    "browser_open_mode",
    "sort_by",
    "sort_order",
    "language",
    "display_time_zone",
    "storage_event_stream_enabled",
];

/// 从 user Model 的 config 字段解析偏好设置。
/// 空配置或解析失败返回 None，解析失败时记录日志。
pub fn parse_preferences(user: &user::Model) -> Option<UserPreferences> {
    parse_user_config(user).and_then(|config| (!config.is_empty()).then_some(config.into()))
}

/// 读取用户的偏好设置（按 ID 查询后解析）。
pub async fn get_preferences(
    state: &PrimaryAppState,
    user_id: i64,
) -> Result<Option<UserPreferences>> {
    let user = user_repo::find_by_id(&state.db, user_id).await?;
    Ok(parse_preferences(&user))
}

fn parse_user_config(user: &user::Model) -> Option<UserConfig> {
    let raw = user.config.as_ref()?;
    match raw.parse() {
        Ok(config) => Some(config),
        Err(error) => {
            tracing::warn!("failed to parse user config for user #{}: {error}", user.id);
            None
        }
    }
}

fn normalize_display_time_zone(display_time_zone: Option<String>) -> Result<Option<String>> {
    let Some(display_time_zone) = display_time_zone else {
        return Ok(None);
    };
    let trimmed = display_time_zone.trim();
    if trimmed.is_empty() {
        return Err(AsterError::validation_error(
            "display_time_zone cannot be empty",
        ));
    }
    if trimmed == DISPLAY_TIME_ZONE_BROWSER {
        return Ok(Some(DISPLAY_TIME_ZONE_BROWSER.to_string()));
    }

    trimmed.parse::<Tz>().map_aster_err_with(|| {
        AsterError::validation_error(format!("invalid display_time_zone '{trimmed}'"))
    })?;

    Ok(Some(trimmed.to_string()))
}

fn normalize_custom_preference_key(raw_key: &str) -> Result<String> {
    let key = raw_key.trim();
    if key.is_empty() {
        return Err(AsterError::validation_error(
            "custom preference key cannot be empty",
        ));
    }
    if RESERVED_PREFERENCE_KEYS.contains(&key) {
        return Err(AsterError::validation_error(format!(
            "custom preference key '{key}' conflicts with built-in preferences"
        )));
    }
    Ok(key.to_string())
}

fn normalize_custom_preferences(
    custom: BTreeMap<String, serde_json::Value>,
) -> Result<BTreeMap<String, serde_json::Value>> {
    let mut normalized = BTreeMap::new();
    for (raw_key, value) in custom {
        let key = normalize_custom_preference_key(&raw_key)?;
        if normalized.insert(key.clone(), value).is_some() {
            return Err(AsterError::validation_error(format!(
                "duplicate custom preference key '{key}' after normalization"
            )));
        }
    }
    Ok(normalized)
}

fn normalize_custom_preference_removals(remove_custom_keys: Vec<String>) -> Result<Vec<String>> {
    let mut deduped = BTreeSet::new();
    for raw_key in remove_custom_keys {
        deduped.insert(normalize_custom_preference_key(&raw_key)?);
    }
    Ok(deduped.into_iter().collect())
}

/// 将用户配置写回 DB。空配置会清空 `users.config`。
async fn save_user_config(
    state: &PrimaryAppState,
    user: user::Model,
    config: &UserConfig,
) -> Result<()> {
    let mut active = user.into_active_model();
    active.config = Set(if config.is_empty() {
        None
    } else {
        Some(StoredUserConfig::from_config(config).map_aster_err(AsterError::internal_error)?)
    });
    active.updated_at = Set(Utc::now());
    active.save(&state.db).await?;
    Ok(())
}

/// 合并更新偏好设置（只更新非 None 字段），返回完整 UserPreferences。
pub async fn update_preferences(
    state: &PrimaryAppState,
    user_id: i64,
    patch: UpdatePreferencesReq,
) -> Result<UserPreferences> {
    let UpdatePreferencesReq {
        theme_mode,
        color_preset,
        view_mode,
        browser_open_mode,
        sort_by,
        sort_order,
        language,
        display_time_zone,
        storage_event_stream_enabled,
        custom,
        remove_custom_keys,
    } = patch;
    let user = user_repo::find_by_id(&state.db, user_id).await?;
    let mut config = parse_user_config(&user).unwrap_or_default();
    let original_config = config.clone();
    let prefs = &mut config.preferences;
    let display_time_zone = normalize_display_time_zone(display_time_zone)?;
    let custom = normalize_custom_preferences(custom)?;
    let remove_custom_keys = normalize_custom_preference_removals(remove_custom_keys)?;

    if let Some(conflict_key) = remove_custom_keys
        .iter()
        .find(|key| custom.contains_key(*key))
    {
        return Err(AsterError::validation_error(format!(
            "custom preference key '{conflict_key}' cannot be updated and removed in the same request"
        )));
    }

    prefs.theme_mode = theme_mode.or(prefs.theme_mode);
    prefs.color_preset = color_preset.or(prefs.color_preset.clone());
    prefs.view_mode = view_mode.or(prefs.view_mode);
    prefs.browser_open_mode = browser_open_mode.or(prefs.browser_open_mode);
    prefs.sort_by = sort_by.or(prefs.sort_by);
    prefs.sort_order = sort_order.or(prefs.sort_order);
    prefs.language = language.or(prefs.language);
    prefs.display_time_zone = display_time_zone.or(prefs.display_time_zone.clone());
    prefs.storage_event_stream_enabled =
        storage_event_stream_enabled.or(prefs.storage_event_stream_enabled);

    for key in remove_custom_keys {
        config.extra.remove(&key);
    }
    config.extra.extend(custom);

    if config != original_config {
        save_user_config(state, user, &config).await?;
    }

    Ok(UserPreferences::from(config))
}
