use crate::config::definitions::CONFIG_REGISTRY;
use crate::config::media_processing::MEDIA_PROCESSING_REGISTRY_JSON_KEY;
use crate::config::operations::{MEDIA_METADATA_ENABLED_KEY, MEDIA_METADATA_MAX_SOURCE_BYTES_KEY};
use crate::config::system_config as shared_system_config;
use crate::config::{auth_runtime, mail};
use crate::db::repository::config_repo;
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::services::ops::audit::{self, AuditContext};
use aster_forge_api::OffsetPage;
use aster_forge_config::{ConfigSource, ConfigValueType, ConfigVisibility};
use aster_forge_config::{ConfigValue, config_value_audit_string};
use aster_forge_db::system_config;
use aster_forge_db::transaction;
use serde::Serialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct SystemConfig {
    pub id: i64,
    pub key: String,
    pub value: ConfigValue,
    pub value_type: ConfigValueType,
    pub requires_restart: bool,
    pub is_sensitive: bool,
    pub source: ConfigSource,
    pub visibility: ConfigVisibility,
    pub namespace: String,
    pub category: String,
    pub description: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub updated_by: Option<i64>,
}

impl From<system_config::Model> for SystemConfig {
    fn from(model: system_config::Model) -> Self {
        let presented = aster_forge_db::present_system_config(model, |error| {
            tracing::warn!(
                error = %error,
                "invalid stored config value; returning an empty presentation value"
            );
        });
        Self {
            id: presented.id,
            key: presented.key,
            value: presented.value,
            value_type: presented.value_type,
            requires_restart: presented.requires_restart,
            is_sensitive: presented.is_sensitive,
            source: presented.source,
            visibility: presented.visibility,
            namespace: presented.namespace,
            category: presented.category,
            description: presented.description,
            updated_at: presented.updated_at,
            updated_by: presented.updated_by,
        }
    }
}

pub async fn list_paginated(
    state: &impl SharedRuntimeState,
    limit: u64,
    offset: u64,
) -> Result<OffsetPage<SystemConfig>> {
    let limit = limit.clamp(1, 100);
    let (models, total) = config_repo::find_paginated(state.reader_db(), limit, offset).await?;
    let items = models
        .into_iter()
        .map(apply_system_config_definition)
        .map(Into::into)
        .collect();
    Ok(OffsetPage::new(items, total, limit, offset))
}

pub async fn get_by_key(state: &impl SharedRuntimeState, key: &str) -> Result<SystemConfig> {
    config_repo::find_by_key(state.reader_db(), key)
        .await?
        .map(apply_system_config_definition)
        .map(Into::into)
        .ok_or_else(|| AsterError::record_not_found(format!("config key '{key}'")))
}

pub async fn set(
    state: &impl SharedRuntimeState,
    key: &str,
    value: impl Into<ConfigValue>,
    updated_by: i64,
) -> Result<SystemConfig> {
    set_with_visibility(state, key, value, None, updated_by).await
}

pub async fn set_with_visibility(
    state: &impl SharedRuntimeState,
    key: &str,
    value: impl Into<ConfigValue>,
    visibility: Option<ConfigVisibility>,
    updated_by: i64,
) -> Result<SystemConfig> {
    validate_visibility_target(key, visibility)?;
    let value = value.into();
    let normalized_value = CONFIG_REGISTRY
        .value_to_storage_for_key(state.runtime_config().as_ref(), key, &value)
        .map_err(AsterError::from)?;

    let changed =
        upsert_config_and_apply_dependents(state, key, &normalized_value, visibility, updated_by)
            .await?;
    let config = changed
        .iter()
        .find(|item| item.key == key)
        .cloned()
        .ok_or_else(|| AsterError::internal_error(format!("saved config key '{key}' missing")))?;
    let changed_keys = changed
        .iter()
        .map(|changed_config| changed_config.key.clone())
        .collect::<Vec<_>>();
    for changed_config in &changed {
        invalidate_dependent_public_config_caches(&changed_config.key);
    }
    publish_config_reload(state, changed_keys.iter(), "upsert").await?;
    Ok(config.into())
}

pub async fn delete(state: &impl SharedRuntimeState, key: &str) -> Result<()> {
    let result: Result<()> = async {
        config_repo::delete_by_key(state.writer_db(), key).await?;
        state.runtime_config().remove(key);
        invalidate_dependent_public_config_caches(key);
        state
            .config_sync()
            .publish_reload(
                [key.to_string()],
                aster_forge_config::ConfigNotificationSource::Api,
            )
            .await
            .map_err(super::runtime::map_config_core_error)
    }
    .await;
    record_config_mutation_result(state, "delete", result.is_ok(), 1);
    result?;
    tracing::debug!(key, "deleted runtime config");
    Ok(())
}

pub async fn delete_with_audit(
    state: &impl SharedRuntimeState,
    key: &str,
    audit_ctx: &AuditContext,
) -> Result<()> {
    let config = get_by_key(state, key).await?;
    delete(state, key).await?;
    audit::log(
        state,
        audit_ctx,
        audit::AuditAction::AdminDeleteConfig,
        crate::services::ops::audit::AuditEntityType::SystemConfig,
        Some(config.id),
        Some(key),
        None,
    )
    .await;
    Ok(())
}

pub async fn set_with_audit(
    state: &impl SharedRuntimeState,
    key: &str,
    value: &ConfigValue,
    updated_by: i64,
    audit_ctx: &AuditContext,
) -> Result<SystemConfig> {
    set_with_audit_and_visibility(state, key, value, None, updated_by, audit_ctx).await
}

pub async fn set_with_audit_and_visibility(
    state: &impl SharedRuntimeState,
    key: &str,
    value: &ConfigValue,
    visibility: Option<ConfigVisibility>,
    updated_by: i64,
    audit_ctx: &AuditContext,
) -> Result<SystemConfig> {
    validate_visibility_target(key, visibility)?;
    let value = value.clone();
    let normalized_value = CONFIG_REGISTRY
        .value_to_storage_for_key(state.runtime_config().as_ref(), key, &value)
        .map_err(AsterError::from)?;

    let prior_visibility = config_repo::find_by_key(state.reader_db(), key)
        .await?
        .map(|config| config.visibility);
    let changed =
        upsert_config_and_apply_dependents(state, key, &normalized_value, visibility, updated_by)
            .await?;
    let config = changed
        .iter()
        .find(|item| item.key == key)
        .cloned()
        .ok_or_else(|| AsterError::internal_error(format!("saved config key '{key}' missing")))?;

    for changed_config in &changed {
        invalidate_dependent_public_config_caches(&changed_config.key);
    }
    publish_config_reload(
        state,
        changed.iter().map(|changed_config| &changed_config.key),
        "upsert",
    )
    .await?;

    for changed_config in &changed {
        let audit_prior_visibility = if changed_config.key == key {
            prior_visibility
        } else {
            Some(changed_config.visibility)
        };
        audit_config_update(state, audit_ctx, changed_config, audit_prior_visibility).await;
    }

    Ok(config.into())
}

async fn publish_config_reload<'a>(
    state: &impl SharedRuntimeState,
    keys: impl IntoIterator<Item = &'a String>,
    operation: &'static str,
) -> Result<()> {
    let keys = keys.into_iter().cloned().collect::<Vec<_>>();
    let changed_keys = u64::try_from(keys.len()).unwrap_or(u64::MAX);
    let result = state
        .config_sync()
        .publish_reload(keys, aster_forge_config::ConfigNotificationSource::Api)
        .await
        .map_err(super::runtime::map_config_core_error);
    record_config_mutation_result(state, operation, result.is_ok(), changed_keys);
    result
}

fn record_config_mutation_result(
    state: &impl SharedRuntimeState,
    operation: &'static str,
    ok: bool,
    changed_keys: u64,
) {
    state.metrics().record_config_mutation(
        "api",
        operation,
        if ok { "ok" } else { "error" },
        changed_keys,
    );
}

async fn audit_config_update(
    state: &impl SharedRuntimeState,
    audit_ctx: &AuditContext,
    config: &system_config::Model,
    prior_visibility: Option<ConfigVisibility>,
) {
    audit::log_with_details(
        state,
        audit_ctx,
        audit::AuditAction::ConfigUpdate,
        audit::AuditEntityType::SystemConfig,
        Some(config.id),
        Some(&config.key),
        || {
            let audit_value = if config.is_sensitive {
                "***REDACTED***".to_string()
            } else {
                config_value_audit_string(
                    config.value_type,
                    config.value.clone(),
                    config.is_sensitive,
                    |error| tracing::warn!(%error, "invalid stored config value for audit"),
                )
            };
            audit::details(audit::ConfigUpdateDetails {
                value: &audit_value,
                visibility: config.visibility,
                prior_visibility,
            })
        },
    )
    .await;
}

fn validate_visibility_target(key: &str, visibility: Option<ConfigVisibility>) -> Result<()> {
    if visibility.is_some() && CONFIG_REGISTRY.contains_key(key) {
        return Err(AsterError::validation_error(
            "visibility can only be changed for custom configuration",
        ));
    }
    Ok(())
}

async fn upsert_config_and_apply_dependents(
    state: &impl SharedRuntimeState,
    key: &str,
    value: &str,
    visibility: Option<ConfigVisibility>,
    updated_by: i64,
) -> Result<Vec<system_config::Model>> {
    let txn = transaction::begin(state.writer_db()).await?;
    let result = async {
        let saved =
            config_repo::upsert_with_options(&txn, key, value, visibility, Some(updated_by))
                .await?;
        let mut changed = vec![apply_system_config_definition(saved)];
        if key != auth_runtime::AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY
            && mail_config_change_disables_email_code_mfa(state, key, value)
        {
            let disabled = config_repo::upsert(
                &txn,
                auth_runtime::AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY,
                "false",
                updated_by,
            )
            .await?;
            changed.push(apply_system_config_definition(disabled));
        }
        Ok::<_, AsterError>(changed)
    }
    .await;

    match result {
        Ok(changed) => {
            transaction::commit(txn).await?;
            for item in &changed {
                state.runtime_config().apply(item.clone());
            }
            Ok(changed)
        }
        Err(error) => {
            transaction::rollback(txn).await?;
            Err(error)
        }
    }
}

fn mail_config_change_disables_email_code_mfa(
    state: &impl SharedRuntimeState,
    key: &str,
    value: &str,
) -> bool {
    if !matches!(
        key,
        mail::MAIL_SMTP_HOST_KEY
            | mail::MAIL_FROM_ADDRESS_KEY
            | mail::MAIL_SMTP_USERNAME_KEY
            | mail::MAIL_SMTP_PASSWORD_KEY
    ) {
        return false;
    }
    if !state.runtime_config().get_bool_or(
        auth_runtime::AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY,
        auth_runtime::DEFAULT_AUTH_EMAIL_CODE_LOGIN_ENABLED,
    ) {
        return false;
    }

    let lookup = |lookup_key: &str| {
        if lookup_key == key {
            value.to_string()
        } else {
            state.runtime_config().get(lookup_key).unwrap_or_default()
        }
    };
    let settings = aster_forge_mail::MailRuntimeSettings {
        smtp_host: lookup(mail::MAIL_SMTP_HOST_KEY),
        smtp_port: state
            .runtime_config()
            .get(mail::MAIL_SMTP_PORT_KEY)
            .and_then(|raw| raw.trim().parse().ok())
            .unwrap_or(aster_forge_mail::DEFAULT_MAIL_SMTP_PORT),
        smtp_username: lookup(mail::MAIL_SMTP_USERNAME_KEY),
        smtp_password: lookup(mail::MAIL_SMTP_PASSWORD_KEY),
        from_address: lookup(mail::MAIL_FROM_ADDRESS_KEY),
        from_name: state
            .runtime_config()
            .get(mail::MAIL_FROM_NAME_KEY)
            .unwrap_or_default(),
        encryption_enabled: state.runtime_config().get_bool_or(
            mail::MAIL_SECURITY_KEY,
            aster_forge_mail::DEFAULT_MAIL_SECURITY,
        ),
    };

    !settings.is_ready_for_delivery()
}

fn apply_system_config_definition(config: system_config::Model) -> system_config::Model {
    shared_system_config::apply_definition(config)
}

pub(super) fn invalidate_dependent_public_config_caches(key: &str) {
    match key {
        MEDIA_PROCESSING_REGISTRY_JSON_KEY => {
            super::public::invalidate_public_thumbnail_support_cache();
            super::public::invalidate_public_media_data_support_cache();
        }
        MEDIA_METADATA_ENABLED_KEY | MEDIA_METADATA_MAX_SOURCE_BYTES_KEY => {
            super::public::invalidate_public_media_data_support_cache();
        }
        _ => {}
    }
}

pub(super) fn invalidate_all_dependent_public_config_caches() {
    super::public::invalidate_public_thumbnail_support_cache();
    super::public::invalidate_public_media_data_support_cache();
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use aster_forge_config::{ConfigChangeNotifier, ConfigNotificationSource};
    use migration::Migrator;

    use super::{delete, set};
    use crate::runtime::{PrimaryAppState, SharedRuntimeState};

    async fn test_state() -> (PrimaryAppState, aster_forge_config::ConfigNotification) {
        let db = crate::db::connect_with_metrics(
            &crate::config::DatabaseConfig {
                url: "sqlite::memory:".to_string(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics::NoopMetrics::arc(),
        )
        .await
        .expect("config sync service test database should connect");
        Migrator::up(&db, None)
            .await
            .expect("config sync service test migrations should apply");
        crate::db::repository::config_repo::ensure_defaults_with_env(&db, &|_| None)
            .await
            .expect("config sync service test defaults should load");
        let runtime_config = Arc::new(crate::config::RuntimeConfig::new());
        runtime_config
            .reload(&db)
            .await
            .expect("config sync service runtime config should load");
        let cache =
            aster_forge_cache::create_cache(&aster_forge_cache::CacheConfig::default()).await;
        let notifier = Arc::new(aster_forge_config::InMemoryConfigNotifier::default());
        let subscription = notifier
            .subscribe()
            .await
            .expect("config sync service test should subscribe");
        let config_sync = aster_forge_config::ConfigSyncRuntime::with_notifier_for_test(
            super::super::runtime::CONFIG_RELOAD_NAMESPACE,
            "service-test-runtime",
            notifier as aster_forge_config::SharedConfigChangeNotifier,
        );
        let (storage_change_tx, _) = tokio::sync::broadcast::channel(
            crate::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
        );
        let (share_download_rollback, _worker) =
            crate::services::share::build_share_download_rollback_queue(
                db.clone(),
                1,
                crate::metrics::NoopMetrics::arc(),
            );

        (
            PrimaryAppState {
                db_handles: aster_forge_db::DbHandles::single(db),
                driver_registry: Arc::new(crate::storage::DriverRegistry::noop()),
                runtime_config,
                policy_snapshot: Arc::new(crate::storage::PolicySnapshot::new()),
                config: Arc::new(crate::config::Config::default()),
                cache,
                config_sync,
                metrics: crate::metrics::NoopMetrics::arc(),
                mail_sender: aster_forge_mail::memory_sender(),
                storage_change_tx,
                share_download_rollback,
                background_task_dispatch_wakeup:
                    PrimaryAppState::new_background_task_dispatch_wakeup(),
                remote_protocol: PrimaryAppState::new_remote_protocol(),
                upload_runtime: crate::runtime::PrimaryAppState::new_upload_runtime(),
            },
            subscription,
        )
    }

    async fn next_reload(
        subscription: &mut aster_forge_config::ConfigNotification,
    ) -> aster_forge_config::ConfigReloadMessage {
        tokio::time::timeout(std::time::Duration::from_secs(1), subscription.recv())
            .await
            .expect("config mutation should publish reload notification")
            .expect("config reload notification should be readable")
            .reload_message()
            .clone()
    }

    #[tokio::test]
    async fn set_publishes_api_reload_for_saved_key() {
        let (state, mut subscription) = test_state().await;

        set(&state, "custom.feature", "enabled", 1)
            .await
            .expect("custom config should save");

        let message = next_reload(&mut subscription).await;
        assert_eq!(message.keys, vec!["custom.feature"]);
        assert_eq!(message.source, ConfigNotificationSource::Api);
    }

    #[tokio::test]
    async fn delete_publishes_api_reload_for_deleted_key() {
        let (state, mut subscription) = test_state().await;
        crate::db::repository::config_repo::upsert(
            state.writer_db(),
            "custom.delete_me",
            "value",
            1,
        )
        .await
        .expect("custom config should seed");

        delete(&state, "custom.delete_me")
            .await
            .expect("custom config should delete");

        let message = next_reload(&mut subscription).await;
        assert_eq!(message.keys, vec!["custom.delete_me"]);
        assert_eq!(message.source, ConfigNotificationSource::Api);
    }

    #[tokio::test]
    async fn dependent_update_publishes_all_changed_keys_once() {
        let (state, mut subscription) = test_state().await;
        for (key, value) in [
            (crate::config::mail::MAIL_SMTP_HOST_KEY, "smtp.example.com"),
            (
                crate::config::mail::MAIL_FROM_ADDRESS_KEY,
                "drive@example.com",
            ),
            (crate::config::mail::MAIL_SMTP_USERNAME_KEY, "drive"),
            (crate::config::mail::MAIL_SMTP_PASSWORD_KEY, "secret"),
            (
                crate::config::auth_runtime::AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY,
                "true",
            ),
        ] {
            let model =
                crate::db::repository::config_repo::upsert(state.writer_db(), key, value, 1)
                    .await
                    .expect("dependent config should seed");
            state.runtime_config().apply(model);
        }

        set(&state, crate::config::mail::MAIL_SMTP_HOST_KEY, "", 1)
            .await
            .expect("mail config should update and disable email-code MFA");

        let message = next_reload(&mut subscription).await;
        assert_eq!(
            message.keys,
            vec![
                crate::config::auth_runtime::AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY.to_string(),
                crate::config::mail::MAIL_SMTP_HOST_KEY.to_string(),
            ]
        );
        assert_eq!(message.source, ConfigNotificationSource::Api);
    }

    #[tokio::test]
    async fn email_code_mfa_requires_complete_mail_settings() {
        let (state, _subscription) = test_state().await;

        let error = set(
            &state,
            crate::config::auth_runtime::AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY,
            "true",
            1,
        )
        .await
        .expect_err("email-code MFA must reject incomplete mail settings");
        assert!(
            error
                .message()
                .contains("email code MFA requires complete SMTP mail configuration")
        );

        for (key, value) in [
            (crate::config::mail::MAIL_SMTP_HOST_KEY, "smtp.example.com"),
            (
                crate::config::mail::MAIL_FROM_ADDRESS_KEY,
                "drive@example.com",
            ),
            (crate::config::mail::MAIL_SMTP_USERNAME_KEY, "drive"),
            (crate::config::mail::MAIL_SMTP_PASSWORD_KEY, "secret"),
        ] {
            let model =
                crate::db::repository::config_repo::upsert(state.writer_db(), key, value, 1)
                    .await
                    .unwrap();
            state.runtime_config().apply(model);
        }

        let saved = set(
            &state,
            crate::config::auth_runtime::AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY,
            "true",
            1,
        )
        .await
        .expect("email-code MFA should accept complete mail settings");
        assert_eq!(
            saved.value,
            aster_forge_config::ConfigValue::String("true".to_string())
        );
    }
}
