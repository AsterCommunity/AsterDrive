use crate::api::pagination::{OffsetPage, load_offset_page};
use crate::config::definitions::ALL_CONFIGS;
use crate::config::media_processing::MEDIA_PROCESSING_REGISTRY_JSON_KEY;
use crate::config::operations::{MEDIA_METADATA_ENABLED_KEY, MEDIA_METADATA_MAX_SOURCE_BYTES_KEY};
use crate::config::system_config as shared_system_config;
use crate::config::{auth_runtime, mail, mail::RuntimeMailSettings};
use crate::db::repository::config_repo;
use crate::entities::system_config;
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::audit_service::{self, AuditContext};
use crate::types::{SystemConfigSource, SystemConfigValueType};
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum SystemConfigValue {
    String(String),
    StringArray(Vec<String>),
}

impl SystemConfigValue {
    fn from_storage(value_type: SystemConfigValueType, value: String) -> Self {
        if value_type != SystemConfigValueType::StringArray {
            return Self::String(value);
        }

        match serde_json::from_str::<Vec<String>>(&value) {
            Ok(items) => Self::StringArray(items),
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "invalid stored string_array config value; returning an empty array"
                );
                Self::StringArray(Vec::new())
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::String(value) => value.trim().is_empty(),
            Self::StringArray(values) => values.is_empty(),
        }
    }

    pub fn to_storage_for_type(&self, value_type: SystemConfigValueType) -> Result<String> {
        match (value_type, self) {
            (SystemConfigValueType::StringArray, Self::StringArray(values)) => {
                serde_json::to_string(values).map_err(|error| {
                    AsterError::internal_error(format!(
                        "failed to serialize string_array config value: {error}"
                    ))
                })
            }
            (SystemConfigValueType::StringArray, Self::String(_)) => Err(
                AsterError::validation_error("string_array config value must be a JSON array"),
            ),
            (_, Self::String(value)) => Ok(value.clone()),
            (_, Self::StringArray(_)) => Err(AsterError::validation_error(
                "string array values are only supported for string_array config keys",
            )),
        }
    }

    pub fn to_audit_string(&self) -> String {
        match self {
            Self::String(value) => value.clone(),
            Self::StringArray(values) => serde_json::to_string(values)
                .unwrap_or_else(|_| "<invalid string_array value>".to_string()),
        }
    }
}

impl From<&str> for SystemConfigValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}

impl From<&String> for SystemConfigValue {
    fn from(value: &String) -> Self {
        Self::String(value.clone())
    }
}

impl From<String> for SystemConfigValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<Vec<String>> for SystemConfigValue {
    fn from(value: Vec<String>) -> Self {
        Self::StringArray(value)
    }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct SystemConfig {
    pub id: i64,
    pub key: String,
    pub value: SystemConfigValue,
    pub value_type: SystemConfigValueType,
    pub requires_restart: bool,
    pub is_sensitive: bool,
    pub source: SystemConfigSource,
    pub namespace: String,
    pub category: String,
    pub description: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub updated_by: Option<i64>,
}

impl From<system_config::Model> for SystemConfig {
    fn from(model: system_config::Model) -> Self {
        // 敏感配置值在 API 响应中脱敏
        let value = if model.is_sensitive {
            SystemConfigValue::String("***REDACTED***".to_string())
        } else {
            SystemConfigValue::from_storage(model.value_type, model.value)
        };
        Self {
            id: model.id,
            key: model.key,
            value,
            value_type: model.value_type,
            requires_restart: model.requires_restart,
            is_sensitive: model.is_sensitive,
            source: model.source,
            namespace: model.namespace,
            category: model.category,
            description: model.description,
            updated_at: model.updated_at,
            updated_by: model.updated_by,
        }
    }
}

pub async fn list_paginated(
    state: &PrimaryAppState,
    limit: u64,
    offset: u64,
) -> Result<OffsetPage<SystemConfig>> {
    let page = load_offset_page(limit, offset, 100, |limit, offset| async move {
        config_repo::find_paginated(state.reader_db(), limit, offset).await
    })
    .await?;
    let items = page
        .items
        .into_iter()
        .map(apply_system_config_definition)
        .map(Into::into)
        .collect();
    Ok(OffsetPage::new(items, page.total, page.limit, page.offset))
}

pub async fn get_by_key(state: &PrimaryAppState, key: &str) -> Result<SystemConfig> {
    config_repo::find_by_key(state.reader_db(), key)
        .await?
        .map(apply_system_config_definition)
        .map(Into::into)
        .ok_or_else(|| AsterError::record_not_found(format!("config key '{key}'")))
}

pub async fn set(
    state: &PrimaryAppState,
    key: &str,
    value: impl Into<SystemConfigValue>,
    updated_by: i64,
) -> Result<SystemConfig> {
    let value = value.into();
    let value_type = ALL_CONFIGS
        .iter()
        .find(|def| def.key == key)
        .map(|def| def.value_type)
        .unwrap_or(SystemConfigValueType::String);
    let mut normalized_value = value.to_storage_for_type(value_type)?;

    if let Some(def) = ALL_CONFIGS.iter().find(|def| def.key == key) {
        validate_value_type(def.value_type, &normalized_value)?;
        normalized_value = normalize_system_value(state, key, &normalized_value)?;
    }
    validate_config_dependencies(state, key, &normalized_value)?;

    let changed =
        upsert_config_and_apply_dependents(state, key, &normalized_value, updated_by).await?;
    let config = changed
        .iter()
        .find(|item| item.key == key)
        .cloned()
        .ok_or_else(|| AsterError::internal_error(format!("saved config key '{key}' missing")))?;
    for changed_key in changed {
        invalidate_dependent_public_config_caches(&changed_key.key);
    }
    Ok(config.into())
}

pub async fn delete(state: &PrimaryAppState, key: &str) -> Result<()> {
    config_repo::delete_by_key(state.writer_db(), key).await?;
    state.runtime_config.remove(key);
    invalidate_dependent_public_config_caches(key);
    tracing::debug!(key, "deleted runtime config");
    Ok(())
}

pub async fn delete_with_audit(
    state: &PrimaryAppState,
    key: &str,
    audit_ctx: &AuditContext,
) -> Result<()> {
    let config = get_by_key(state, key).await?;
    delete(state, key).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminDeleteConfig,
        crate::services::audit_service::AuditEntityType::SystemConfig,
        Some(config.id),
        Some(key),
        None,
    )
    .await;
    Ok(())
}

pub async fn set_with_audit(
    state: &PrimaryAppState,
    key: &str,
    value: &SystemConfigValue,
    updated_by: i64,
    audit_ctx: &AuditContext,
) -> Result<SystemConfig> {
    let config = set(state, key, value.clone(), updated_by).await?;
    // 敏感配置值在审计日志中脱敏
    let audit_value = if config.is_sensitive {
        "***REDACTED***".to_string()
    } else {
        value.to_audit_string()
    };
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::ConfigUpdate,
        audit_service::AuditEntityType::SystemConfig,
        None,
        Some(key),
        audit_service::details(audit_service::ConfigUpdateDetails {
            value: &audit_value,
        }),
    )
    .await;
    Ok(config)
}

fn validate_value_type(value_type: SystemConfigValueType, value: &str) -> Result<()> {
    shared_system_config::validate_value_type(value_type, value)
}

fn normalize_system_value(state: &PrimaryAppState, key: &str, value: &str) -> Result<String> {
    shared_system_config::normalize_system_value(&state.runtime_config, key, value)
}

fn validate_config_dependencies(state: &PrimaryAppState, key: &str, value: &str) -> Result<()> {
    if key != auth_runtime::AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY || value != "true" {
        return Ok(());
    }

    let settings = RuntimeMailSettings::from_runtime_config(&state.runtime_config);
    if settings.is_ready_for_delivery() {
        return Ok(());
    }

    Err(AsterError::validation_error(
        "email code MFA requires complete SMTP mail configuration",
    ))
}

async fn upsert_config_and_apply_dependents(
    state: &PrimaryAppState,
    key: &str,
    value: &str,
    updated_by: i64,
) -> Result<Vec<system_config::Model>> {
    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    let result = async {
        let saved = config_repo::upsert(&txn, key, value, updated_by).await?;
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
            crate::db::transaction::commit(txn).await?;
            for item in &changed {
                state.runtime_config.apply(item.clone());
            }
            Ok(changed)
        }
        Err(error) => {
            crate::db::transaction::rollback(txn).await?;
            Err(error)
        }
    }
}

fn mail_config_change_disables_email_code_mfa(
    state: &PrimaryAppState,
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
    if !state.runtime_config.get_bool_or(
        auth_runtime::AUTH_EMAIL_CODE_LOGIN_ENABLED_KEY,
        auth_runtime::DEFAULT_AUTH_EMAIL_CODE_LOGIN_ENABLED,
    ) {
        return false;
    }

    let lookup = |lookup_key: &str| {
        if lookup_key == key {
            value.to_string()
        } else {
            state.runtime_config.get(lookup_key).unwrap_or_default()
        }
    };
    let settings = RuntimeMailSettings {
        smtp_host: lookup(mail::MAIL_SMTP_HOST_KEY),
        smtp_port: state
            .runtime_config
            .get(mail::MAIL_SMTP_PORT_KEY)
            .and_then(|raw| raw.trim().parse().ok())
            .unwrap_or(mail::DEFAULT_MAIL_SMTP_PORT),
        smtp_username: lookup(mail::MAIL_SMTP_USERNAME_KEY),
        smtp_password: lookup(mail::MAIL_SMTP_PASSWORD_KEY),
        from_address: lookup(mail::MAIL_FROM_ADDRESS_KEY),
        from_name: state
            .runtime_config
            .get(mail::MAIL_FROM_NAME_KEY)
            .unwrap_or_default(),
        encryption_enabled: state
            .runtime_config
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
