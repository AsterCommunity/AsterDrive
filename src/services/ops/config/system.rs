use crate::api::pagination::OffsetPage;
use crate::config::definitions::ALL_CONFIGS;
use crate::config::media_processing::MEDIA_PROCESSING_REGISTRY_JSON_KEY;
use crate::config::operations::{MEDIA_METADATA_ENABLED_KEY, MEDIA_METADATA_MAX_SOURCE_BYTES_KEY};
use crate::config::system_config as shared_system_config;
use crate::config::{auth_runtime, mail, mail::RuntimeMailSettings};
use crate::db::repository::config_repo;
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::services::ops::audit::{self, AuditContext};
use crate::types::{ConfigSource, ConfigValueType, ConfigVisibility};
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
    let value_type = ALL_CONFIGS
        .iter()
        .find(|def| def.key == key)
        .map(|def| def.value_type)
        .unwrap_or(ConfigValueType::String);
    let mut normalized_value = value.to_storage_for_type(value_type)?;

    if let Some(def) = ALL_CONFIGS.iter().find(|def| def.key == key) {
        validate_value_type(def.value_type, &normalized_value)?;
        normalized_value = normalize_system_value(state, key, &normalized_value)?;
    }
    validate_config_dependencies(state, key, &normalized_value)?;

    let changed =
        upsert_config_and_apply_dependents(state, key, &normalized_value, visibility, updated_by)
            .await?;
    let config = changed
        .iter()
        .find(|item| item.key == key)
        .cloned()
        .ok_or_else(|| AsterError::internal_error(format!("saved config key '{key}' missing")))?;
    for changed_config in changed {
        invalidate_dependent_public_config_caches(&changed_config.key);
    }
    Ok(config.into())
}

pub async fn delete(state: &impl SharedRuntimeState, key: &str) -> Result<()> {
    config_repo::delete_by_key(state.writer_db(), key).await?;
    state.runtime_config().remove(key);
    invalidate_dependent_public_config_caches(key);
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
    let value_type = ALL_CONFIGS
        .iter()
        .find(|def| def.key == key)
        .map(|def| def.value_type)
        .unwrap_or(ConfigValueType::String);
    let mut normalized_value = value.to_storage_for_type(value_type)?;

    if let Some(def) = ALL_CONFIGS.iter().find(|def| def.key == key) {
        validate_value_type(def.value_type, &normalized_value)?;
        normalized_value = normalize_system_value(state, key, &normalized_value)?;
    }
    validate_config_dependencies(state, key, &normalized_value)?;

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
        let audit_prior_visibility = if changed_config.key == key {
            prior_visibility
        } else {
            Some(changed_config.visibility)
        };
        audit_config_update(state, audit_ctx, changed_config, audit_prior_visibility).await;
    }

    Ok(config.into())
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

fn validate_value_type(value_type: ConfigValueType, value: &str) -> Result<()> {
    shared_system_config::validate_value_type(value_type, value)
}

fn validate_visibility_target(key: &str, visibility: Option<ConfigVisibility>) -> Result<()> {
    if visibility.is_some() && ALL_CONFIGS.iter().any(|def| def.key == key) {
        return Err(AsterError::validation_error(
            "visibility can only be changed for custom configuration",
        ));
    }
    Ok(())
}

fn normalize_system_value(
    state: &impl SharedRuntimeState,
    key: &str,
    value: &str,
) -> Result<String> {
    shared_system_config::normalize_system_value(state.runtime_config().as_ref(), key, value)
}

fn validate_config_dependencies(
    state: &impl SharedRuntimeState,
    key: &str,
    value: &str,
) -> Result<()> {
    if key != auth_runtime::AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY || value != "true" {
        return Ok(());
    }

    let settings = RuntimeMailSettings::from_runtime_config(state.runtime_config());
    if settings.is_ready_for_delivery() {
        return Ok(());
    }

    Err(AsterError::validation_error(
        "email code MFA requires complete SMTP mail configuration",
    ))
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
    let settings = RuntimeMailSettings {
        smtp_host: lookup(mail::MAIL_SMTP_HOST_KEY),
        smtp_port: state
            .runtime_config()
            .get(mail::MAIL_SMTP_PORT_KEY)
            .and_then(|raw| raw.trim().parse().ok())
            .unwrap_or(mail::DEFAULT_MAIL_SMTP_PORT),
        smtp_username: lookup(mail::MAIL_SMTP_USERNAME_KEY),
        smtp_password: lookup(mail::MAIL_SMTP_PASSWORD_KEY),
        from_address: lookup(mail::MAIL_FROM_ADDRESS_KEY),
        from_name: state
            .runtime_config()
            .get(mail::MAIL_FROM_NAME_KEY)
            .unwrap_or_default(),
        encryption_enabled: state
            .runtime_config()
            .get_bool_or(mail::MAIL_SECURITY_KEY, mail::DEFAULT_MAIL_SECURITY),
    };

    !settings.is_ready_for_delivery()
}

fn apply_system_config_definition(config: system_config::Model) -> system_config::Model {
    shared_system_config::apply_definition(config)
}

fn invalidate_dependent_public_config_caches(key: &str) {
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
