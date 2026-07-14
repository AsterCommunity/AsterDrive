//! 仓储模块：`config_repo`。

use crate::config::bool_like::parse_bool_like;
use crate::config::definitions::{
    CONFIG_REGISTRY, DEPRECATED_SYSTEM_CONFIG_KEYS, MEDIA_PROCESSING_REGISTRY_JSON_KEY,
};
use crate::config::media_processing;
use crate::errors::{AsterError, Result};
use crate::services::preview::apps;
use crate::types::MediaProcessorKind;
use aster_forge_config::ConfigVisibility;
use aster_forge_db::pagination::fetch_offset_page;
use aster_forge_db::system_config::{
    self, Entity as SystemConfig, SystemConfigDbBinding, SystemConfigUpsert,
};
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryOrder, Set,
};

static STORE: SystemConfigDbBinding =
    SystemConfigDbBinding::new(&CONFIG_REGISTRY, DEPRECATED_SYSTEM_CONFIG_KEYS);

const BOOTSTRAP_ENABLE_VIPS_CLI_ENV: &str = "ASTER_BOOTSTRAP_ENABLE_VIPS_CLI";
const BOOTSTRAP_ENABLE_FFMPEG_CLI_ENV: &str = "ASTER_BOOTSTRAP_ENABLE_FFMPEG_CLI";
const BOOTSTRAP_ENABLE_FFPROBE_CLI_ENV: &str = "ASTER_BOOTSTRAP_ENABLE_FFPROBE_CLI";
const BOOTSTRAP_MEDIA_PROCESSOR_ENV_FLAGS: &[(MediaProcessorKind, &str)] = &[
    (MediaProcessorKind::VipsCli, BOOTSTRAP_ENABLE_VIPS_CLI_ENV),
    (
        MediaProcessorKind::FfmpegCli,
        BOOTSTRAP_ENABLE_FFMPEG_CLI_ENV,
    ),
    (
        MediaProcessorKind::FfprobeCli,
        BOOTSTRAP_ENABLE_FFPROBE_CLI_ENV,
    ),
];

fn map_store_error(error: aster_forge_db::DbError) -> AsterError {
    let message = error.to_string();
    if message.contains("cannot delete system configuration") {
        return AsterError::auth_forbidden("cannot delete system configuration");
    }
    if let Some(key) = config_key_from_message(&message) {
        return AsterError::record_not_found(format!("config key '{key}'"));
    }
    AsterError::from(error)
}

fn config_key_from_message(message: &str) -> Option<&str> {
    let prefix = "config key '";
    let start = message.find(prefix)? + prefix.len();
    let rest = &message[start..];
    let end = rest.find('\'')?;
    Some(&rest[..end])
}

fn map_store_result<T>(result: aster_forge_db::Result<T>) -> Result<T> {
    result.map_err(map_store_error)
}

pub async fn find_all<C: ConnectionTrait>(db: &C) -> Result<Vec<system_config::Model>> {
    map_store_result(STORE.find_all(db).await)
}

pub async fn find_paginated(
    db: &DatabaseConnection,
    limit: u64,
    offset: u64,
) -> Result<(Vec<system_config::Model>, u64)> {
    fetch_offset_page(
        db,
        SystemConfig::find().order_by_asc(system_config::Column::Id),
        limit,
        offset,
    )
    .await
}

pub async fn find_by_key<C: ConnectionTrait>(
    db: &C,
    key: &str,
) -> Result<Option<system_config::Model>> {
    map_store_result(STORE.find_by_key(db, key).await)
}

pub async fn find_visible_custom(
    db: &DatabaseConnection,
    include_authenticated: bool,
) -> Result<Vec<system_config::Model>> {
    map_store_result(STORE.find_visible_custom(db, include_authenticated).await)
}

pub async fn lock_by_key<C: ConnectionTrait>(db: &C, key: &str) -> Result<()> {
    map_store_result(STORE.lock_by_key(db, key).await)
}

pub async fn upsert<C: ConnectionTrait>(
    db: &C,
    key: &str,
    value: &str,
    updated_by: i64,
) -> Result<system_config::Model> {
    upsert_with_actor(db, key, value, Some(updated_by)).await
}

pub async fn upsert_with_actor<C: ConnectionTrait>(
    db: &C,
    key: &str,
    value: &str,
    updated_by: Option<i64>,
) -> Result<system_config::Model> {
    upsert_with_options(db, key, value, None, updated_by).await
}

pub async fn upsert_with_options<C: ConnectionTrait>(
    db: &C,
    key: &str,
    value: &str,
    visibility: Option<ConfigVisibility>,
    updated_by: Option<i64>,
) -> Result<system_config::Model> {
    map_store_result(
        STORE
            .upsert(
                db,
                SystemConfigUpsert {
                    key,
                    value,
                    visibility,
                    updated_by,
                },
            )
            .await,
    )
}

pub async fn delete_by_key<C: ConnectionTrait>(db: &C, key: &str) -> Result<()> {
    map_store_result(STORE.delete_by_key(db, key).await)
}

pub async fn ensure_system_value_if_missing<C: ConnectionTrait>(
    db: &C,
    key: &str,
    value: &str,
) -> Result<bool> {
    map_store_result(STORE.ensure_system_value_if_missing(db, key, value).await)
}

fn bootstrap_media_processing_registry_default_value<F>(get_env: &F) -> String
where
    F: Fn(&str) -> Option<String>,
{
    let enabled_processors = bootstrap_enabled_media_processors(get_env);

    if enabled_processors.is_empty() {
        return media_processing::default_media_processing_registry_json();
    }

    let mut config = media_processing::default_media_processing_registry();
    for processor in &mut config.processors {
        if enabled_processors.contains(&processor.kind) {
            processor.enabled = true;
        }
    }

    serde_json::to_string_pretty(&config).unwrap_or_else(|error| {
        tracing::warn!(%error, "failed to serialize bootstrapped media processing registry");
        media_processing::default_media_processing_registry_json()
    })
}

fn bootstrap_enabled_media_processors<F>(get_env: &F) -> Vec<MediaProcessorKind>
where
    F: Fn(&str) -> Option<String>,
{
    BOOTSTRAP_MEDIA_PROCESSOR_ENV_FLAGS
        .iter()
        .filter_map(|(kind, env_name)| env_flag_enabled(get_env, env_name).then_some(*kind))
        .collect()
}

fn env_flag_enabled<F>(get_env: &F, name: &str) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    let value = get_env(name);
    match value.as_deref() {
        Some(raw) => match parse_bool_like(raw) {
            Some(parsed) => parsed,
            None => {
                tracing::warn!("invalid boolean for {}: {}", name, raw);
                false
            }
        },
        None => false,
    }
}

/// 确保所有系统配置存在，同步元信息（不覆盖用户修改的 value）
pub async fn ensure_defaults_with_env<C, F>(db: &C, get_env: &F) -> Result<usize>
where
    C: ConnectionTrait,
    F: Fn(&str) -> Option<String>,
{
    ensure_defaults_inner(db, get_env).await
}

async fn ensure_defaults_inner<C, F>(db: &C, get_env: &F) -> Result<usize>
where
    C: ConnectionTrait,
    F: Fn(&str) -> Option<String>,
{
    let media_inserted = map_store_result(
        STORE
            .ensure_system_value_if_missing(
                db,
                MEDIA_PROCESSING_REGISTRY_JSON_KEY,
                &bootstrap_media_processing_registry_default_value(get_env),
            )
            .await,
    )?;
    let inserted = map_store_result(STORE.ensure_defaults(db).await)?;
    if !media_inserted {
        normalize_existing_product_config(db, MEDIA_PROCESSING_REGISTRY_JSON_KEY, |active| {
            normalize_existing_media_processing_registry_config_value(active)
        })
        .await?;
    }
    normalize_existing_product_config(
        db,
        crate::config::cors::CORS_ALLOWED_ORIGINS_KEY,
        normalize_existing_cors_allowed_origins_config_value,
    )
    .await?;
    normalize_existing_product_config(db, apps::PREVIEW_APPS_CONFIG_KEY, |active| {
        normalize_existing_preview_apps_config_value(active)
    })
    .await?;

    Ok(inserted + usize::from(media_inserted))
}

async fn normalize_existing_product_config<C, F>(db: &C, key: &str, normalize: F) -> Result<()>
where
    C: ConnectionTrait,
    F: FnOnce(&mut system_config::ActiveModel),
{
    let existing = find_by_key(db, key)
        .await?
        .ok_or_else(|| AsterError::record_not_found(format!("config key '{key}'")))?;
    let mut active: system_config::ActiveModel = existing.into();
    normalize(&mut active);
    active.update(db).await.map_err(AsterError::from)?;
    Ok(())
}

fn normalize_existing_cors_allowed_origins_config_value(active: &mut system_config::ActiveModel) {
    let existing = match &active.value {
        sea_orm::ActiveValue::Set(value) | sea_orm::ActiveValue::Unchanged(value) => value.clone(),
        sea_orm::ActiveValue::NotSet => return,
    };

    match crate::config::cors::normalize_existing_allowed_origins_config_value(&existing) {
        Ok(normalized) if normalized != existing => active.value = Set(normalized),
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(
                error = %error,
                key = crate::config::cors::CORS_ALLOWED_ORIGINS_KEY,
                "failed to migrate legacy CORS origins; clearing invalid whitelist"
            );
            active.value = Set("[]".to_string());
        }
    }
}

fn normalize_existing_media_processing_registry_config_value(
    active: &mut system_config::ActiveModel,
) {
    let existing = match &active.value {
        sea_orm::ActiveValue::Set(value) | sea_orm::ActiveValue::Unchanged(value) => value.clone(),
        sea_orm::ActiveValue::NotSet => return,
    };

    match media_processing::normalize_existing_media_processing_registry_config_value(&existing) {
        Ok(normalized) if normalized != existing => {
            active.value = Set(normalized);
        }
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(
                error = %error,
                key = MEDIA_PROCESSING_REGISTRY_JSON_KEY,
                "failed to normalize existing media processing registry during default config sync"
            );
        }
    }
}

fn normalize_existing_preview_apps_config_value(active: &mut system_config::ActiveModel) {
    let existing = match &active.value {
        sea_orm::ActiveValue::Set(value) | sea_orm::ActiveValue::Unchanged(value) => value.clone(),
        sea_orm::ActiveValue::NotSet => return,
    };

    match apps::public_preview_apps_config_has_missing_required_builtins(&existing) {
        Ok(false) => {}
        Ok(true) => match apps::normalize_public_preview_apps_config_value(&existing) {
            Ok(normalized) => {
                active.value = Set(normalized);
            }
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    key = apps::PREVIEW_APPS_CONFIG_KEY,
                    "failed to normalize existing preview app registry during default config sync"
                );
            }
        },
        Err(error) => {
            tracing::warn!(
                error = %error,
                key = apps::PREVIEW_APPS_CONFIG_KEY,
                "failed to normalize existing preview app registry during default config sync"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DatabaseConfig;
    use crate::db;
    use crate::services::preview::apps::PREVIEW_APPS_CONFIG_KEY;
    use migration::Migrator;

    async fn setup_db() -> sea_orm::DatabaseConnection {
        let db = db::connect_with_metrics(
            &DatabaseConfig {
                url: "sqlite::memory:".to_string(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics::NoopMetrics::arc(),
        )
        .await
        .expect("config repo test DB should connect");
        Migrator::up(&db, None)
            .await
            .expect("config repo migrations should succeed");
        db
    }

    async fn media_processing_registry_config(
        db: &sea_orm::DatabaseConnection,
    ) -> media_processing::MediaProcessingRegistryConfig {
        let stored = find_by_key(db, MEDIA_PROCESSING_REGISTRY_JSON_KEY)
            .await
            .expect("media processing config lookup should succeed")
            .expect("media processing config should exist");
        serde_json::from_str(&stored.value)
            .expect("stored media processing config should be valid JSON")
    }

    #[tokio::test]
    async fn ensure_defaults_keeps_media_processing_registry_default_when_bootstrap_env_disabled() {
        let db = setup_db().await;

        ensure_defaults_with_env(&db, &|_| None)
            .await
            .expect("ensure_defaults should succeed");

        let stored = find_by_key(&db, MEDIA_PROCESSING_REGISTRY_JSON_KEY)
            .await
            .expect("media processing config lookup should succeed")
            .expect("media processing config should exist");

        assert_eq!(
            stored.value,
            media_processing::default_media_processing_registry_json()
        );
    }

    #[tokio::test]
    async fn ensure_defaults_bootstraps_cli_processors_without_losing_default_bindings() {
        let db = setup_db().await;

        ensure_defaults_with_env(&db, &|name| match name {
            BOOTSTRAP_ENABLE_VIPS_CLI_ENV
            | BOOTSTRAP_ENABLE_FFMPEG_CLI_ENV
            | BOOTSTRAP_ENABLE_FFPROBE_CLI_ENV => Some("1".to_string()),
            _ => None,
        })
        .await
        .expect("ensure_defaults should succeed");

        let config = media_processing_registry_config(&db).await;
        let vips =
            media_processing::processor_config_for_kind(&config, MediaProcessorKind::VipsCli)
                .expect("vips config should exist");
        let ffmpeg =
            media_processing::processor_config_for_kind(&config, MediaProcessorKind::FfmpegCli)
                .expect("ffmpeg config should exist");
        let ffprobe =
            media_processing::processor_config_for_kind(&config, MediaProcessorKind::FfprobeCli)
                .expect("ffprobe config should exist");

        assert!(vips.enabled);
        assert_eq!(
            vips.extensions,
            media_processing::default_processor_config_for_kind(MediaProcessorKind::VipsCli)
                .extensions
        );
        assert_eq!(
            vips.config.command.as_deref(),
            Some(media_processing::DEFAULT_VIPS_COMMAND)
        );

        assert!(ffmpeg.enabled);
        assert_eq!(
            ffmpeg.extensions,
            media_processing::default_processor_config_for_kind(MediaProcessorKind::FfmpegCli)
                .extensions
        );
        assert_eq!(
            ffmpeg.config.command.as_deref(),
            Some(media_processing::DEFAULT_FFMPEG_COMMAND)
        );

        assert!(ffprobe.enabled);
        assert_eq!(
            ffprobe.extensions,
            media_processing::default_processor_config_for_kind(MediaProcessorKind::FfprobeCli)
                .extensions
        );
        assert_eq!(
            ffprobe.config.command.as_deref(),
            Some(media_processing::DEFAULT_FFPROBE_COMMAND)
        );
    }

    #[tokio::test]
    async fn ensure_defaults_ignores_invalid_bootstrap_media_processor_flags() {
        let db = setup_db().await;

        ensure_defaults_with_env(&db, &|name| match name {
            BOOTSTRAP_ENABLE_VIPS_CLI_ENV => Some("definitely".to_string()),
            _ => None,
        })
        .await
        .expect("ensure_defaults should succeed");

        let config = media_processing_registry_config(&db).await;
        let vips =
            media_processing::processor_config_for_kind(&config, MediaProcessorKind::VipsCli)
                .expect("vips config should exist");

        assert!(!vips.enabled);
    }

    #[tokio::test]
    async fn ensure_defaults_does_not_override_existing_media_processing_registry() {
        let db = setup_db().await;
        let existing = r#"{
  "version": 1,
  "processors": [
    {
      "kind": "vips_cli",
      "enabled": false,
      "extensions": [
        "heic"
      ],
      "config": {
        "command": "vips"
      }
    },
    {
      "kind": "ffmpeg_cli",
      "enabled": true,
      "extensions": [
        "mp4"
      ],
      "config": {
        "command": "ffmpeg"
      }
    },
    {
      "kind": "images",
      "enabled": true
    }
  ]
}"#;

        ensure_system_value_if_missing(&db, MEDIA_PROCESSING_REGISTRY_JSON_KEY, existing)
            .await
            .expect("initial media processing config insert should succeed");

        ensure_defaults_with_env(&db, &|name| match name {
            BOOTSTRAP_ENABLE_VIPS_CLI_ENV
            | BOOTSTRAP_ENABLE_FFMPEG_CLI_ENV
            | BOOTSTRAP_ENABLE_FFPROBE_CLI_ENV => Some("1".to_string()),
            _ => None,
        })
        .await
        .expect("ensure_defaults should succeed");

        let config = media_processing_registry_config(&db).await;
        let vips =
            media_processing::processor_config_for_kind(&config, MediaProcessorKind::VipsCli)
                .expect("vips config should exist");
        let ffmpeg =
            media_processing::processor_config_for_kind(&config, MediaProcessorKind::FfmpegCli)
                .expect("ffmpeg config should exist");
        let images =
            media_processing::processor_config_for_kind(&config, MediaProcessorKind::Images)
                .expect("images config should exist");

        assert_eq!(
            config.version,
            media_processing::MEDIA_PROCESSING_REGISTRY_VERSION
        );
        assert!(!vips.enabled);
        assert_eq!(vips.extensions, vec!["heic".to_string()]);
        assert_eq!(
            vips.config.command.as_deref(),
            Some(media_processing::DEFAULT_VIPS_COMMAND)
        );
        assert!(ffmpeg.enabled);
        assert_eq!(ffmpeg.extensions, vec!["mp4".to_string()]);
        assert_eq!(
            ffmpeg.config.command.as_deref(),
            Some(media_processing::DEFAULT_FFMPEG_COMMAND)
        );
        assert!(images.enabled);
    }

    #[tokio::test]
    async fn ensure_defaults_migrates_legacy_cors_origin_values_to_string_arrays() {
        let db = setup_db().await;
        ensure_system_value_if_missing(
            &db,
            crate::config::cors::CORS_ALLOWED_ORIGINS_KEY,
            "https://b.example.com,chrome-extension://iikmkjmpaadaobahmlepeloendndfphd,https://b.example.com",
        )
        .await
        .expect("legacy CORS config insert should succeed");

        ensure_defaults_with_env(&db, &|_| None)
            .await
            .expect("ensure_defaults should migrate legacy CORS origins");

        let stored = find_by_key(&db, crate::config::cors::CORS_ALLOWED_ORIGINS_KEY)
            .await
            .expect("CORS config lookup should succeed")
            .expect("CORS config should exist");
        assert_eq!(
            stored.value_type,
            aster_forge_config::ConfigValueType::StringArray
        );
        assert_eq!(
            stored.value,
            r#"["chrome-extension://iikmkjmpaadaobahmlepeloendndfphd","https://b.example.com"]"#
        );
    }

    #[tokio::test]
    async fn ensure_defaults_clears_invalid_legacy_cors_origin_values() {
        let db = setup_db().await;
        ensure_system_value_if_missing(
            &db,
            crate::config::cors::CORS_ALLOWED_ORIGINS_KEY,
            "ftp://backup.example.com",
        )
        .await
        .expect("invalid legacy CORS config insert should succeed");

        ensure_defaults_with_env(&db, &|_| None)
            .await
            .expect("ensure_defaults should safely migrate invalid CORS origins");

        let stored = find_by_key(&db, crate::config::cors::CORS_ALLOWED_ORIGINS_KEY)
            .await
            .expect("CORS config lookup should succeed")
            .expect("CORS config should exist");
        assert_eq!(
            stored.value_type,
            aster_forge_config::ConfigValueType::StringArray
        );
        assert_eq!(stored.value, "[]");
    }

    #[tokio::test]
    async fn ensure_defaults_restores_missing_preview_builtins_without_overwriting_existing_apps() {
        let db = setup_db().await;
        let existing = r#"{
  "version": 2,
  "apps": [
    {
      "key": "builtin.image",
      "provider": "builtin",
      "icon": "/custom/image.svg",
      "labels": {
        "en": "Custom image"
      }
    },
    {
      "key": "custom.viewer",
      "provider": "url_template",
      "icon": "https://viewer.example.com/icon.svg",
      "enabled": true,
      "labels": {
        "en": "Viewer"
      },
      "extensions": [
        "txt"
      ],
      "config": {
        "mode": "iframe",
        "url_template": "https://viewer.example.com/?src={{file_preview_url}}"
      }
    }
  ]
}"#;

        ensure_system_value_if_missing(&db, PREVIEW_APPS_CONFIG_KEY, existing)
            .await
            .expect("initial preview app config insert should succeed");

        ensure_defaults_with_env(&db, &|_| None)
            .await
            .expect("ensure_defaults should succeed");

        let stored = find_by_key(&db, PREVIEW_APPS_CONFIG_KEY)
            .await
            .expect("preview app config lookup should succeed")
            .expect("preview app config should exist");
        let config: apps::PublicPreviewAppsConfig =
            serde_json::from_str(&stored.value).expect("stored preview apps should parse");

        assert!(config.apps.iter().any(|app| {
            app.key == "builtin.image"
                && app.icon == "/custom/image.svg"
                && app
                    .labels
                    .get("en")
                    .is_some_and(|label| label == "Custom image")
        }));
        assert!(config.apps.iter().any(|app| app.key == "custom.viewer"));
        assert!(config.apps.iter().any(|app| {
            app.key == "builtin.archive" && app.extensions.iter().any(|ext| ext == "zip")
        }));
        assert!(config.apps.iter().any(|app| app.key == "builtin.code"));
    }
}
