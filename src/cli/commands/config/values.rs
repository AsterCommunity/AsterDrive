//! Config input resolution and shared product normalization rules.
use super::{
    args::{ValidateArgs, ValueInput},
    interaction,
    transfer::{self, ImportItem, read_import_items},
};
use crate::cli::common::error::CliResult;
use crate::cli::common::input::{SecretArg, read_secret};
use crate::config::{definitions::CONFIG_REGISTRY, system_config as shared_system_config};
use crate::db::repository::config_repo;
use crate::errors::{AsterError, Result};
use crate::services::ops::config::SystemConfig;
use aster_forge_config::{ConfigSource, ConfigValue, ConfigValueType};
use aster_forge_db::system_config;
use chrono::Utc;
use secrecy::ExposeSecret;
use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
};
#[derive(Clone)]
pub(super) struct NormalizedImportItem {
    pub(super) key: String,
    pub(super) value: String,
    pub(super) visibility: aster_forge_config::ConfigVisibility,
    pub(super) is_sensitive: bool,
}

impl NormalizedImportItem {
    pub(super) fn preview(&self) -> SystemConfig {
        let mut view = preview_system_config(&self.key, &self.value);
        view.visibility = self.visibility;
        if self.is_sensitive {
            view.value = ConfigValue::redacted();
            view.is_sensitive = true;
        }
        view
    }
}

pub(super) async fn load_config_views(
    db: &sea_orm::DatabaseConnection,
) -> Result<Vec<SystemConfig>> {
    Ok(config_repo::find_all(db)
        .await?
        .into_iter()
        .map(system_config_to_view)
        .collect())
}

pub(super) fn system_config_to_view(model: system_config::Model) -> SystemConfig {
    shared_system_config::apply_definition(model).into()
}

pub(super) fn normalize_entries(
    current: &[system_config::Model],
    entries: &[ImportItem],
) -> Result<Vec<NormalizedImportItem>> {
    let mut current_lookup: HashMap<String, String> = current
        .iter()
        .map(|model| (model.key.clone(), model.value.clone()))
        .collect();
    let mut seen_keys = BTreeSet::new();
    let mut storage_entries = Vec::with_capacity(entries.len());

    for entry in entries {
        if !seen_keys.insert(entry.key.clone()) {
            return Err(AsterError::validation_error(format!(
                "duplicate config key '{}' in input",
                entry.key
            )));
        }
        transfer::validate_item(entry)?;
        if let Some(existing) = current.iter().find(|model| model.key == entry.key)
            && ((existing.is_sensitive && !entry.is_sensitive) || existing.source != entry.source)
        {
            return Err(AsterError::validation_error(
                "input cannot change existing configuration ownership or remove sensitivity",
            ));
        }
        let value_type = CONFIG_REGISTRY
            .get(&entry.key)
            .map(|def| def.value_type)
            .unwrap_or(ConfigValueType::String);
        let value = entry.value.to_storage_for_type(value_type).map_err(|_| {
            AsterError::validation_error(
                "configuration value has an invalid type (values redacted)",
            )
        })?;
        current_lookup.insert(entry.key.clone(), value.clone());
        storage_entries.push(NormalizedImportItem {
            key: entry.key.clone(),
            value,
            visibility: entry.visibility,
            is_sensitive: entry.is_sensitive,
        });
    }

    storage_entries
        .iter()
        .map(|entry| {
            let value = if CONFIG_REGISTRY.contains_key(&entry.key) {
                CONFIG_REGISTRY
                    .normalize_value(&current_lookup, &entry.key, &entry.value)
                    .map_err(|_| {
                        AsterError::validation_error(
                            "configuration validation failed (values redacted)",
                        )
                    })?
            } else {
                entry.value.clone()
            };

            Ok(NormalizedImportItem {
                key: entry.key.clone(),
                value,
                visibility: entry.visibility,
                is_sensitive: entry.is_sensitive,
            })
        })
        .collect()
}

pub(super) fn resolve_value_entry(
    key: &str,
    input: &ValueInput,
    current: &[system_config::Model],
) -> CliResult<Option<ImportItem>> {
    let definition = CONFIG_REGISTRY.get(key);
    let existing = current.iter().find(|model| model.key == key);
    let sensitive = definition.is_some_and(|def| def.is_sensitive)
        || existing.is_some_and(|model| model.is_sensitive);
    if input.interactive() {
        return interaction::prompt_value(key, sensitive, current);
    }
    let value = read_value_input(input)?;
    Ok(Some(value_entry(
        key,
        value.expose_secret(),
        sensitive,
        current,
    )?))
}

pub(super) fn value_entry(
    key: &str,
    value: &str,
    sensitive: bool,
    current: &[system_config::Model],
) -> Result<ImportItem> {
    let definition = CONFIG_REGISTRY.get(key);
    let existing = current.iter().find(|model| model.key == key);
    Ok(ImportItem {
        key: key.into(),
        value: parse_cli_config_value(key, value)?,
        source: if definition.is_some() {
            ConfigSource::System
        } else {
            ConfigSource::Custom
        },
        visibility: definition
            .map(|def| def.visibility)
            .or_else(|| existing.map(|model| model.visibility))
            .unwrap_or_default(),
        is_sensitive: sensitive,
    })
}

pub(super) fn resolve_validate_entries(
    args: &ValidateArgs,
    current: &[system_config::Model],
) -> CliResult<Vec<ImportItem>> {
    let explicit_key = args.key.positional_key.is_some() || args.key.key.is_some();
    let explicit_value = args.input.explicitly_supplied();
    if let Some(path) = &args.input_file {
        if explicit_key || explicit_value {
            return Err(AsterError::validation_error(
                "validate requires either --input-file or KEY with a value input source",
            )
            .into());
        }
        return read_import_items(path).map_err(Into::into);
    }
    let environment_file = if explicit_key || explicit_value {
        None
    } else {
        std::env::var_os("ASTER_CLI_INPUT_FILE").map(PathBuf::from)
    };
    let key = args.key.resolve()?;
    match (&environment_file, key.as_deref(), args.input.supplied()) {
        (Some(path), None, false) => read_import_items(path).map_err(Into::into),
        (None, Some(key), _) => {
            let entry = match resolve_value_entry(key, &args.input, current)? {
                Some(entry) => entry,
                None => {
                    let saved = current
                        .iter()
                        .find(|model| model.key == key)
                        .ok_or_else(|| {
                            AsterError::validation_error("no saved value to validate")
                        })?;
                    let sensitive = saved.is_sensitive
                        || CONFIG_REGISTRY.get(key).is_some_and(|def| def.is_sensitive);
                    // Enter keeps the saved value, but validation must still check it.
                    value_entry(key, &saved.value, sensitive, current)?
                }
            };
            Ok(vec![entry])
        }
        _ => Err(AsterError::validation_error(
            "validate requires either --input-file or --key with a value input source",
        )
        .into()),
    }
}

pub(super) fn parse_cli_config_value(key: &str, value: &str) -> Result<ConfigValue> {
    if !CONFIG_REGISTRY
        .get(key)
        .map(|def| def.value_type.is_string_array())
        .unwrap_or(false)
    {
        return Ok(ConfigValue::String(value.to_string()));
    }

    let values = serde_json::from_str::<Vec<String>>(value).map_err(|_| {
        AsterError::validation_error(
            "configuration expects a JSON array of strings (values redacted)",
        )
    })?;
    Ok(ConfigValue::StringArray(values))
}

pub(super) fn preview_system_config(key: &str, value: &str) -> SystemConfig {
    let model = if let Some(def) = CONFIG_REGISTRY.get(key) {
        system_config::Model {
            id: 0,
            key: key.to_string(),
            value: value.to_string(),
            value_type: def.value_type,
            requires_restart: def.requires_restart,
            is_sensitive: def.is_sensitive,
            source: ConfigSource::System,
            visibility: aster_forge_config::ConfigVisibility::Private,
            namespace: String::new(),
            category: def.category.to_string(),
            description: def.description.to_string(),
            updated_at: Utc::now(),
            updated_by: None,
        }
    } else {
        system_config::Model {
            id: 0,
            key: key.to_string(),
            value: value.to_string(),
            value_type: ConfigValueType::String,
            requires_restart: false,
            is_sensitive: false,
            source: ConfigSource::Custom,
            visibility: aster_forge_config::ConfigVisibility::Private,
            namespace: String::new(),
            category: String::new(),
            description: String::new(),
            updated_at: Utc::now(),
            updated_by: None,
        }
    };

    system_config_to_view(model)
}

fn read_value_input(input: &ValueInput) -> Result<secrecy::SecretString> {
    if input.clear {
        return Ok(String::new().into());
    }
    let environment = if !input.explicitly_supplied() {
        std::env::var("ASTER_CLI_CONFIG_VALUE")
            .ok()
            .map(SecretArg::from)
    } else {
        None
    };
    read_secret(
        input
            .positional_value
            .as_ref()
            .or(input.value.as_ref())
            .or(environment.as_ref()),
        input.value_stdin,
        input.value_file.as_deref(),
    )
}
