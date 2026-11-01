//! `aster_drive config` 的聚合入口。
//!
//! 这里负责定义配置子命令、参数解析、读写/校验流程和结果渲染；
//! 底层持久化与配置规则仍复用 `config_repo` 和 `system_config` 相关逻辑。

use aster_forge_db::transaction;
use secrecy::{ExposeSecret, SecretString};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

mod interaction;
mod transfer;
use super::CliResult;
use super::context::CliContext;
use super::input::{SecretArg, read_secret};
use transfer::{ConfigBackup, ImportItem, read_import_items};

use crate::config::definitions::CONFIG_REGISTRY;
use crate::config::system_config as shared_system_config;
use crate::db::repository::config_repo;
use crate::errors::{AsterError, Result};
use crate::services::ops::config::SystemConfig;
use aster_forge_config::ConfigValue;
use aster_forge_config::{ConfigSource, ConfigValueType};
use aster_forge_db::system_config;
use aster_forge_validation::filename::char_count;
use chrono::Utc;
use clap::{Args, Subcommand};
use serde::Serialize;

use super::shared::{
    CliTerminalPalette, OutputFormat, ResolvedOutputFormat, connect_existing_database, human_key,
    render_error_json, render_success_envelope,
};

#[derive(Debug, Clone, Subcommand)]
pub enum ConfigCommand {
    /// List saved runtime settings
    List,
    /// Show one runtime setting (sensitive values are hidden)
    Get(KeyArgs),
    /// Update one runtime setting
    Set(KeyValueArgs),
    /// Remove a custom runtime setting
    Delete(KeyArgs),
    /// Check a value or backup without saving changes
    Validate(ValidateArgs),
    /// Write an importable JSON backup (secrets omitted by default)
    Export(ExportArgs),
    /// Restore settings from a versioned JSON backup
    Import(FileArgs),
}

#[derive(Debug, Clone, Default, Args)]
#[group(skip)]
pub struct KeyArgs {
    /// Runtime setting key
    #[arg(index = 1, value_name = "KEY", conflicts_with = "key")]
    pub positional_key: Option<String>,
    /// Runtime setting key (also accepts ASTER_CLI_CONFIG_KEY)
    #[arg(long, value_name = "KEY")]
    pub key: Option<String>,
}

impl KeyArgs {
    fn resolve(&self) -> Result<Option<String>> {
        if self.positional_key.is_some() && self.key.is_some() {
            return Err(AsterError::validation_error(
                "provide KEY or --key, not both",
            ));
        }
        let key = self
            .positional_key
            .clone()
            .or_else(|| self.key.clone())
            .or_else(|| std::env::var("ASTER_CLI_CONFIG_KEY").ok());
        if key.as_ref().is_some_and(|key| key.trim().is_empty()) {
            return Err(AsterError::validation_error(
                "configuration key cannot be empty",
            ));
        }
        Ok(key)
    }

    fn required(&self) -> Result<String> {
        self.resolve()?.ok_or_else(|| {
            AsterError::validation_error(
                "configuration key required; use config get KEY or config delete KEY",
            )
        })
    }
}

#[derive(Debug, Clone, Default, Args)]
#[group(id = "config_value_input", multiple = false)]
pub struct ValueInput {
    /// Explicit value (prefer terminal, stdin or private files for secrets)
    #[arg(index = 2, value_name = "VALUE", allow_negative_numbers = true)]
    pub positional_value: Option<SecretArg>,
    /// Explicit value (secrets may be exposed in shell history and process listings)
    #[arg(long)]
    pub value: Option<SecretArg>,
    /// Read a value from stdin; remove one trailing line ending
    #[arg(long)]
    pub value_stdin: bool,
    /// Read a value from a private UTF-8 file; remove one trailing line ending
    #[arg(long)]
    pub value_file: Option<std::path::PathBuf>,
    /// Prompt in the terminal (sensitive input displays asterisks)
    #[arg(long)]
    pub value_prompt: bool,
    /// Store an empty value, subject to the setting's validation rules
    #[arg(long)]
    pub clear: bool,
}

impl ValueInput {
    pub fn supplied(&self) -> bool {
        self.explicitly_supplied() || std::env::var_os("ASTER_CLI_CONFIG_VALUE").is_some()
    }

    fn explicitly_supplied(&self) -> bool {
        self.positional_value.is_some()
            || self.value.is_some()
            || self.value_stdin
            || self.value_file.is_some()
            || self.value_prompt
            || self.clear
    }

    fn interactive(&self) -> bool {
        self.value_prompt || !self.supplied()
    }

    pub fn read(&self) -> Result<SecretString> {
        if self.clear {
            return Ok(String::new().into());
        }
        let environment = if !self.explicitly_supplied() {
            std::env::var("ASTER_CLI_CONFIG_VALUE")
                .ok()
                .map(SecretArg::from)
        } else {
            None
        };
        read_secret(
            self.positional_value
                .as_ref()
                .or(self.value.as_ref())
                .or(environment.as_ref()),
            self.value_stdin,
            self.value_file.as_deref(),
        )
    }
}

#[derive(Debug, Clone, Args)]
pub struct KeyValueArgs {
    #[command(flatten)]
    pub key: KeyArgs,
    #[command(flatten)]
    pub input: ValueInput,
}

#[derive(Debug, Clone, Args)]
pub struct ExportArgs {
    /// Include plaintext secrets in the backup; protect the resulting file
    #[arg(long)]
    pub include_sensitive: bool,
}

#[derive(Debug, Clone, Args)]
pub struct FileArgs {
    #[arg(long, env = "ASTER_CLI_INPUT_FILE")]
    pub input_file: PathBuf,
}

#[derive(Debug, Clone, Args)]
pub struct ValidateArgs {
    #[command(flatten)]
    pub key: KeyArgs,
    #[command(flatten)]
    pub input: ValueInput,
    #[arg(long)]
    pub input_file: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
pub struct ConfigListOutput {
    count: usize,
    configs: Vec<SystemConfig>,
}

#[derive(Debug, Serialize)]
pub struct DeleteOutput {
    key: String,
    deleted: bool,
}

#[derive(Debug, Clone, Copy)]
enum ConfigCommandKind {
    List,
    Get,
    Set,
    Delete,
    Validate,
    Export,
    Import,
    Unchanged,
}

#[derive(Debug)]
enum ConfigCommandPayload {
    List(ConfigListOutput),
    Backup(ConfigBackup),
    Config(SystemConfig),
    Delete(DeleteOutput),
}

#[derive(Debug)]
pub struct ConfigCommandReport {
    kind: ConfigCommandKind,
    payload: ConfigCommandPayload,
}

impl ConfigCommandReport {
    fn list(kind: ConfigCommandKind, configs: Vec<SystemConfig>) -> Self {
        Self {
            kind,
            payload: ConfigCommandPayload::List(ConfigListOutput {
                count: configs.len(),
                configs,
            }),
        }
    }

    fn config(kind: ConfigCommandKind, config: SystemConfig) -> Self {
        Self {
            kind,
            payload: ConfigCommandPayload::Config(config),
        }
    }

    fn delete(key: String) -> Self {
        Self {
            kind: ConfigCommandKind::Delete,
            payload: ConfigCommandPayload::Delete(DeleteOutput { key, deleted: true }),
        }
    }
}

#[derive(Clone)]
struct NormalizedImportItem {
    key: String,
    value: String,
    visibility: aster_forge_config::ConfigVisibility,
    is_sensitive: bool,
}

/// Executes a runtime configuration subcommand against the target database.
pub async fn execute_config_command(
    context: &CliContext,
    command: &ConfigCommand,
) -> CliResult<ConfigCommandReport> {
    let config_sync = match command {
        ConfigCommand::Set(_) | ConfigCommand::Delete(_) | ConfigCommand::Import(_) => {
            Some(build_cli_config_sync_runtime(&context.config)?)
        }
        _ => None,
    };
    execute_config_command_with_runtime(&context.database, command, config_sync.as_ref()).await
}

async fn execute_config_command_with_runtime(
    database: &crate::config::DatabaseConfig,
    command: &ConfigCommand,
    config_sync: Option<&aster_forge_config::ConfigSyncRuntime>,
) -> CliResult<ConfigCommandReport> {
    let db = connect_existing_database(database).await?;

    match command {
        ConfigCommand::List => Ok(ConfigCommandReport::list(
            ConfigCommandKind::List,
            load_config_views(&db).await?,
        )),
        ConfigCommand::Export(args) => Ok(ConfigCommandReport {
            kind: ConfigCommandKind::Export,
            payload: ConfigCommandPayload::Backup(ConfigBackup::from_models(
                config_repo::find_all(&db).await?,
                args.include_sensitive,
            )?),
        }),
        ConfigCommand::Get(args) => {
            let key = args.required()?;
            let config = config_repo::find_by_key(&db, &key)
                .await?
                .map(system_config_to_view)
                .ok_or_else(|| AsterError::record_not_found(format!("config key '{key}'")))?;
            Ok(ConfigCommandReport::config(ConfigCommandKind::Get, config))
        }
        ConfigCommand::Set(args) => {
            let config_sync = required_cli_config_sync(config_sync)?;
            let current = config_repo::find_all(&db).await?;
            let key = match args.key.resolve()? {
                Some(key) => key,
                None => interaction::select_key(&current)?,
            };
            let Some(entry) = resolve_value_entry(&key, &args.input, &current)? else {
                return Ok(ConfigCommandReport::config(
                    ConfigCommandKind::Unchanged,
                    interaction::unchanged_view(&key, &current)?,
                ));
            };
            let txn = transaction::begin(&db).await.map_err(AsterError::from)?;
            let current = config_repo::find_all(&txn).await?;
            let normalized = normalize_entries(&current, &[entry])?;
            let normalized_item = normalized.into_iter().next().ok_or_else(|| {
                AsterError::internal_error("single config entry normalization returned no item")
            })?;
            let saved = save_item(&txn, &normalized_item).await?;
            transaction::commit(txn).await.map_err(AsterError::from)?;
            publish_cli_config_reload(config_sync, [saved.key.clone()]).await?;
            Ok(ConfigCommandReport::config(
                ConfigCommandKind::Set,
                system_config_to_view(saved),
            ))
        }
        ConfigCommand::Delete(args) => {
            let config_sync = required_cli_config_sync(config_sync)?;
            let key = args.required()?;
            config_repo::delete_by_key(&db, &key).await?;
            publish_cli_config_reload(config_sync, [key.clone()]).await?;
            Ok(ConfigCommandReport::delete(key))
        }
        ConfigCommand::Validate(args) => {
            let current = config_repo::find_all(&db).await?;
            let entries = resolve_validate_entries(args, &current)?;
            let normalized = normalize_entries(&current, &entries)?;
            let previews: Vec<SystemConfig> = normalized
                .into_iter()
                .map(|item| {
                    let mut view = preview_system_config(&item.key, &item.value);
                    view.visibility = item.visibility;
                    if item.is_sensitive {
                        view.value = ConfigValue::redacted();
                        view.is_sensitive = true;
                    }
                    view
                })
                .collect();
            Ok(ConfigCommandReport::list(
                ConfigCommandKind::Validate,
                previews,
            ))
        }
        ConfigCommand::Import(args) => {
            let config_sync = required_cli_config_sync(config_sync)?;
            let entries = read_import_items(&args.input_file)?;
            let txn = transaction::begin(&db).await.map_err(AsterError::from)?;
            let current = config_repo::find_all(&txn).await?;
            let normalized = normalize_entries(&current, &entries)?;
            let mut saved = Vec::with_capacity(normalized.len());
            for item in normalized {
                let model = save_item(&txn, &item).await?;
                saved.push(system_config_to_view(model));
            }
            transaction::commit(txn).await.map_err(AsterError::from)?;
            publish_cli_config_reload(config_sync, saved.iter().map(|config| config.key.clone()))
                .await?;
            Ok(ConfigCommandReport::list(ConfigCommandKind::Import, saved))
        }
    }
}

fn required_cli_config_sync(
    runtime: Option<&aster_forge_config::ConfigSyncRuntime>,
) -> Result<&aster_forge_config::ConfigSyncRuntime> {
    runtime.ok_or_else(|| AsterError::internal_error("config sync runtime is missing"))
}

async fn save_item(
    txn: &sea_orm::DatabaseTransaction,
    item: &NormalizedImportItem,
) -> Result<system_config::Model> {
    use sea_orm::{ActiveModelTrait, Set};
    if let Some(existing) = config_repo::find_by_key(txn, &item.key).await?
        && existing.is_sensitive
        && !item.is_sensitive
    {
        return Err(AsterError::validation_error(
            "cannot remove sensitivity from an existing configuration",
        ));
    }
    let model =
        config_repo::upsert_with_options(txn, &item.key, &item.value, Some(item.visibility), None)
            .await
            .map_err(|error| {
                if item.is_sensitive {
                    AsterError::database_operation(
                        "failed to write sensitive configuration (values redacted)",
                    )
                } else {
                    error
                }
            })?;
    if model.source == ConfigSource::Custom && model.is_sensitive != item.is_sensitive {
        let mut active: system_config::ActiveModel = model.into();
        active.is_sensitive = Set(item.is_sensitive);
        return active.update(txn).await.map_err(|_| {
            AsterError::database_operation(
                "failed to persist configuration sensitivity (values redacted)",
            )
        });
    }
    Ok(model)
}

fn build_cli_config_sync_runtime(
    config: &crate::config::Config,
) -> Result<aster_forge_config::ConfigSyncRuntime> {
    aster_forge_config::build_config_sync_runtime(
        &config.config_sync,
        crate::services::ops::config::runtime::CONFIG_RELOAD_NAMESPACE,
    )
    .map_err(crate::services::ops::config::runtime::map_config_core_error)
}

async fn publish_cli_config_reload(
    runtime: &aster_forge_config::ConfigSyncRuntime,
    keys: impl IntoIterator<Item = impl Into<String>>,
) -> Result<()> {
    let keys = keys.into_iter().map(Into::into).collect::<Vec<_>>();
    if keys.is_empty() {
        return Ok(());
    }
    runtime
        .publish_reload(keys, aster_forge_config::ConfigNotificationSource::Cli)
        .await
        .map_err(|_| AsterError::config_error(
            "configuration was saved, but reload notification failed; check config_sync or restart the affected instances"))
}

/// Renders a successful config command result in the requested output format.
pub fn render_success(format: OutputFormat, data: &ConfigCommandReport) -> String {
    if let ConfigCommandPayload::Backup(backup) = &data.payload {
        return serde_json::to_string_pretty(backup)
            .unwrap_or_else(|_| "{\"error\":\"cannot serialize configuration backup\"}".into());
    }
    match format.resolve() {
        ResolvedOutputFormat::Json => render_config_json(data, false),
        ResolvedOutputFormat::PrettyJson => render_config_json(data, true),
        ResolvedOutputFormat::Human => render_config_human(data),
    }
}

/// Renders a config command failure in the requested output format.
pub fn render_error(format: OutputFormat, err: &AsterError) -> String {
    match format.resolve() {
        ResolvedOutputFormat::Json => render_error_json(err, false),
        ResolvedOutputFormat::PrettyJson => render_error_json(err, true),
        ResolvedOutputFormat::Human => render_error_human(err),
    }
}

fn render_config_json(report: &ConfigCommandReport, pretty: bool) -> String {
    match &report.payload {
        ConfigCommandPayload::Backup(payload) => {
            serde_json::to_string_pretty(payload).unwrap_or_default()
        }
        ConfigCommandPayload::List(payload) => render_success_envelope(payload, pretty),
        ConfigCommandPayload::Config(payload) => render_success_envelope(payload, pretty),
        ConfigCommandPayload::Delete(payload) => render_success_envelope(payload, pretty),
    }
}

fn render_config_human(report: &ConfigCommandReport) -> String {
    let palette = CliTerminalPalette::stdout();
    match &report.payload {
        ConfigCommandPayload::Backup(payload) => {
            serde_json::to_string_pretty(payload).unwrap_or_default()
        }
        ConfigCommandPayload::List(payload) => {
            render_config_list_human(report.kind, payload, &palette)
        }
        ConfigCommandPayload::Config(payload) => {
            render_config_entry_human(report.kind, payload, &palette)
        }
        ConfigCommandPayload::Delete(payload) => render_config_delete_human(payload, &palette),
    }
}

fn render_config_list_human(
    kind: ConfigCommandKind,
    payload: &ConfigListOutput,
    palette: &CliTerminalPalette,
) -> String {
    let mut lines = vec![
        palette.title(config_list_title(kind)),
        palette.dim("--------------------------------------------------"),
        format!("{} {}", human_key("Count", palette), payload.count),
    ];

    if payload.configs.is_empty() {
        lines.push(String::new());
        lines.push(palette.dim("No configuration entries."));
        return lines.join("\n");
    }

    lines.push(String::new());
    lines.push(palette.label("Entries:"));
    for config in &payload.configs {
        let rendered_value = format_config_list_value(config, palette);
        lines.push(format!(
            "  {} {} = {}",
            palette.source_badge(config.source),
            palette.accent(&config.key),
            rendered_value
        ));
        lines.push(format!(
            "    type={} category={} restart={} sensitive={}",
            config.value_type,
            config.category,
            yes_no_label(config.requires_restart),
            yes_no_label(config.is_sensitive)
        ));
    }

    lines.join("\n")
}

fn format_config_list_value(config: &SystemConfig, palette: &CliTerminalPalette) -> String {
    if config.is_sensitive {
        return if config.value.is_empty() {
            palette.dim("[empty sensitive value]")
        } else {
            palette.warn("[hidden sensitive value]")
        };
    }

    if let ConfigValue::String(value) = &config.value
        && (config.value_type.is_multiline() || value.contains('\n'))
    {
        return palette.dim(&summarize_multiline_value(value));
    }

    format_config_value(&config.value)
}

fn summarize_multiline_value(value: &str) -> String {
    let trimmed = value.trim();
    let line_count = value.lines().count();
    let chars = char_count(value);
    let label = if trimmed.starts_with('{') || trimmed.starts_with('[') {
        "json value"
    } else if trimmed.starts_with('<') {
        "html template"
    } else {
        "multiline value"
    };
    format!("<{label}: {line_count} lines, {chars} chars>")
}

fn render_config_entry_human(
    kind: ConfigCommandKind,
    payload: &SystemConfig,
    palette: &CliTerminalPalette,
) -> String {
    let mut lines = vec![
        palette.title(config_entry_title(kind)),
        palette.dim("--------------------------------------------------"),
        format!("{} {}", human_key("Key", palette), payload.key),
        format!(
            "{} {}",
            human_key("Value", palette),
            format_config_value(&payload.value)
        ),
        format!("{} {}", human_key("Type", palette), payload.value_type),
        format!(
            "{} {} {}",
            human_key("Source", palette),
            palette.source_badge(payload.source),
            payload.source
        ),
        format!(
            "{} {}",
            human_key("Restart", palette),
            yes_no_label(payload.requires_restart)
        ),
        format!(
            "{} {}",
            human_key("Sensitive", palette),
            yes_no_label(payload.is_sensitive)
        ),
        format!("{} {}", human_key("Category", palette), payload.category),
        format!(
            "{} {}",
            human_key("Updated", palette),
            payload
                .updated_at
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        ),
    ];

    if !payload.namespace.is_empty() {
        lines.push(format!(
            "{} {}",
            human_key("Namespace", palette),
            payload.namespace
        ));
    }
    if !payload.description.is_empty() {
        lines.push(format!(
            "{} {}",
            human_key("Description", palette),
            payload.description
        ));
    }

    lines.push(format!(
        "{} {}",
        human_key("Status", palette),
        match kind {
            ConfigCommandKind::Set => format!("{} updated", palette.status_badge("updated")),
            ConfigCommandKind::Validate =>
                format!("{} valid preview", palette.status_badge("valid")),
            _ => format!("{} loaded", palette.status_badge("ok")),
        }
    ));

    lines.join("\n")
}

fn render_config_delete_human(payload: &DeleteOutput, palette: &CliTerminalPalette) -> String {
    [
        palette.title("Configuration deleted"),
        palette.dim("--------------------------------------------------"),
        format!("{} {}", human_key("Key", palette), payload.key),
        format!(
            "{} {} {}",
            human_key("Status", palette),
            palette.status_badge("deleted"),
            if payload.deleted {
                "deleted"
            } else {
                "not deleted"
            }
        ),
    ]
    .join("\n")
}

fn render_error_human(err: &AsterError) -> String {
    let palette = CliTerminalPalette::stdout();
    [
        palette.bad("Config command failed"),
        palette.dim("--------------------------------------------------"),
        format!("{} {}", human_key("Code", &palette), err.code()),
        format!("{} {}", human_key("Type", &palette), err.error_type()),
        format!("{} {}", human_key("Message", &palette), err.message()),
    ]
    .join("\n")
}

fn config_list_title(kind: ConfigCommandKind) -> &'static str {
    match kind {
        ConfigCommandKind::List => "Configuration list",
        ConfigCommandKind::Export => "Configuration export",
        ConfigCommandKind::Import => "Configuration import complete",
        ConfigCommandKind::Validate => "Configuration validation preview",
        ConfigCommandKind::Unchanged => "Configuration unchanged",
        _ => "Configurations",
    }
}

fn config_entry_title(kind: ConfigCommandKind) -> &'static str {
    match kind {
        ConfigCommandKind::Get => "Configuration value",
        ConfigCommandKind::Set => "Configuration updated",
        ConfigCommandKind::Unchanged => "Configuration unchanged",
        ConfigCommandKind::Validate => "Configuration validation preview",
        _ => "Configuration",
    }
}

fn yes_no_label(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

async fn load_config_views(db: &sea_orm::DatabaseConnection) -> Result<Vec<SystemConfig>> {
    Ok(config_repo::find_all(db)
        .await?
        .into_iter()
        .map(system_config_to_view)
        .collect())
}

fn system_config_to_view(model: system_config::Model) -> SystemConfig {
    shared_system_config::apply_definition(model).into()
}

fn normalize_entries(
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

fn resolve_value_entry(
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
    let value = input.read()?;
    Ok(Some(value_entry(
        key,
        value.expose_secret(),
        sensitive,
        current,
    )?))
}

fn value_entry(
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

fn resolve_validate_entries(
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

fn parse_cli_config_value(key: &str, value: &str) -> Result<ConfigValue> {
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

fn format_config_value(value: &ConfigValue) -> String {
    match value {
        ConfigValue::String(value) => value.clone(),
        ConfigValue::StringArray(values) => serde_json::to_string(values)
            .unwrap_or_else(|_| "<invalid string_array value>".to_string()),
    }
}

fn preview_system_config(key: &str, value: &str) -> SystemConfig {
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use aster_drive_migration::Migrator;
    use aster_forge_config::{ConfigChangeNotifier, ConfigNotificationSource};

    use super::{
        ConfigCommand, FileArgs, KeyArgs, KeyValueArgs, execute_config_command_with_runtime,
    };

    async fn test_database(label: &str) -> (String, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "aster-drive-config-cli-{label}-{}.db",
            uuid::Uuid::new_v4()
        ));
        let url = format!("sqlite://{}?mode=rwc", path.display());
        let db = crate::db::connect_with_metrics(
            &crate::config::DatabaseConfig {
                url: url.clone().into(),
                pool_size: 1,
                retry_count: 0,
            },
            aster_drive_metrics::NoopMetrics::arc(),
        )
        .await
        .expect("config CLI test database should connect");
        Migrator::up(&db, None)
            .await
            .expect("config CLI test migrations should apply");
        db.close()
            .await
            .expect("config CLI test database should close");
        (url, path)
    }

    async fn test_runtime() -> (
        aster_forge_config::ConfigSyncRuntime,
        aster_forge_config::ConfigNotification,
    ) {
        let notifier = Arc::new(aster_forge_config::InMemoryConfigNotifier::default());
        let subscription = notifier
            .subscribe()
            .await
            .expect("config CLI test should subscribe");
        let runtime = aster_forge_config::ConfigSyncRuntime::with_notifier_for_test(
            crate::services::ops::config::runtime::CONFIG_RELOAD_NAMESPACE,
            "cli-test-runtime",
            notifier as aster_forge_config::SharedConfigChangeNotifier,
        );
        (runtime, subscription)
    }

    async fn next_reload(
        subscription: &mut aster_forge_config::ConfigNotification,
    ) -> aster_forge_config::ConfigReloadMessage {
        tokio::time::timeout(std::time::Duration::from_secs(1), subscription.recv())
            .await
            .expect("config CLI mutation should publish reload notification")
            .expect("config CLI reload notification should be readable")
            .reload_message()
            .clone()
    }

    #[tokio::test]
    async fn set_publishes_cli_reload_after_database_write() {
        let (database_url, database_path) = test_database("set").await;
        let (runtime, mut subscription) = test_runtime().await;

        execute_config_command_with_runtime(
            &crate::config::DatabaseConfig {
                url: database_url.clone().into(),
                pool_size: 1,
                retry_count: 0,
            },
            &ConfigCommand::Set(KeyValueArgs {
                key: KeyArgs {
                    key: Some("custom.cli_set".to_string()),
                    ..Default::default()
                },
                input: super::ValueInput {
                    value: Some("enabled".parse().unwrap()),
                    ..Default::default()
                },
            }),
            Some(&runtime),
        )
        .await
        .expect("config CLI set should succeed");

        let message = next_reload(&mut subscription).await;
        assert_eq!(message.keys, vec!["custom.cli_set"]);
        assert_eq!(message.source, ConfigNotificationSource::Cli);
        std::fs::remove_file(database_path).expect("config CLI test database should clean up");
    }

    #[tokio::test]
    async fn delete_publishes_cli_reload_after_database_delete() {
        let (database_url, database_path) = test_database("delete").await;
        let (runtime, mut subscription) = test_runtime().await;
        execute_config_command_with_runtime(
            &crate::config::DatabaseConfig {
                url: database_url.clone().into(),
                pool_size: 1,
                retry_count: 0,
            },
            &ConfigCommand::Set(KeyValueArgs {
                key: KeyArgs {
                    key: Some("custom.cli_delete".to_string()),
                    ..Default::default()
                },
                input: super::ValueInput {
                    value: Some("enabled".parse().unwrap()),
                    ..Default::default()
                },
            }),
            Some(&runtime),
        )
        .await
        .expect("config CLI seed should succeed");
        let _ = next_reload(&mut subscription).await;

        execute_config_command_with_runtime(
            &crate::config::DatabaseConfig {
                url: database_url.clone().into(),
                pool_size: 1,
                retry_count: 0,
            },
            &ConfigCommand::Delete(KeyArgs {
                key: Some("custom.cli_delete".to_string()),
                ..Default::default()
            }),
            Some(&runtime),
        )
        .await
        .expect("config CLI delete should succeed");

        let message = next_reload(&mut subscription).await;
        assert_eq!(message.keys, vec!["custom.cli_delete"]);
        assert_eq!(message.source, ConfigNotificationSource::Cli);
        std::fs::remove_file(database_path).expect("config CLI test database should clean up");
    }

    #[tokio::test]
    async fn import_publishes_one_deduplicated_cli_reload_for_all_keys() {
        let (database_url, database_path) = test_database("import").await;
        let input_file = std::env::temp_dir().join(format!(
            "aster-drive-config-cli-import-{}.json",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(
            &input_file,
            r#"{"format":"asterdrive.runtime-config","version":1,"configs":[{"key":"custom.cli_b","value":"b","source":"custom","visibility":"private","is_sensitive":false},{"key":"custom.cli_a","value":"a","source":"custom","visibility":"private","is_sensitive":false}],"omitted_sensitive_keys":[]}"#,
        )
        .expect("config CLI import fixture should write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&input_file, std::fs::Permissions::from_mode(0o600))
                .expect("config CLI import fixture should be private");
        }
        let (runtime, mut subscription) = test_runtime().await;

        execute_config_command_with_runtime(
            &crate::config::DatabaseConfig {
                url: database_url.clone().into(),
                pool_size: 1,
                retry_count: 0,
            },
            &ConfigCommand::Import(FileArgs {
                input_file: input_file.clone(),
            }),
            Some(&runtime),
        )
        .await
        .expect("config CLI import should succeed");

        let message = next_reload(&mut subscription).await;
        assert_eq!(message.keys, vec!["custom.cli_a", "custom.cli_b"]);
        assert_eq!(message.source, ConfigNotificationSource::Cli);
        std::fs::remove_file(input_file).expect("config CLI import fixture should clean up");
        std::fs::remove_file(database_path).expect("config CLI test database should clean up");
    }

    #[tokio::test]
    async fn database_failure_rolls_back_prior_import_writes_and_publishes_no_reload() {
        use sea_orm::{ConnectionTrait, DbBackend, Statement};
        let (database_url, database_path) = test_database("rollback").await;
        let database = crate::config::DatabaseConfig {
            url: database_url.into(),
            pool_size: 1,
            retry_count: 0,
        };
        let db = super::connect_existing_database(&database).await.unwrap();
        db.execute_raw(Statement::from_string(DbBackend::Sqlite,
            "CREATE TRIGGER reject_cli_import BEFORE INSERT ON system_config WHEN NEW.key = 'custom.reject' BEGIN SELECT RAISE(ABORT, 'database rejects second item'); END"))
            .await.unwrap();
        let file = aster_forge_utils::raii::TempFileGuard::new(
            std::env::temp_dir().join(format!("aster-cli-rollback-{}.json", uuid::Uuid::new_v4())),
            "CLI rollback fixture",
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(file.path())
                .unwrap();
        }
        std::fs::write(file.path(), r#"{"format":"asterdrive.runtime-config","version":1,"configs":[{"key":"custom.first","value":"written then rolled back","source":"custom","visibility":"private","is_sensitive":false},{"key":"custom.reject","value":"credential-marker","source":"custom","visibility":"private","is_sensitive":true}],"omitted_sensitive_keys":[]}"#).unwrap();
        let (runtime, mut subscription) = test_runtime().await;
        let result = execute_config_command_with_runtime(
            &database,
            &ConfigCommand::Import(FileArgs {
                input_file: file.path().into(),
            }),
            Some(&runtime),
        )
        .await;
        let error = result.unwrap_err();
        assert!(!format!("{error:?}").contains("credential-marker"));
        assert!(
            crate::db::repository::config_repo::find_by_key(&db, "custom.first")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), subscription.recv())
                .await
                .is_err()
        );
        db.close().await.unwrap();
        std::fs::remove_file(database_path).unwrap();
    }

    #[tokio::test]
    async fn read_commands_and_validate_do_not_insert_defaults_or_mutate_rows() {
        let (database_url, database_path) = test_database("no-read-side-effects").await;
        let database = crate::config::DatabaseConfig {
            url: database_url.into(),
            pool_size: 1,
            retry_count: 0,
        };
        let db = super::connect_existing_database(&database).await.unwrap();
        assert!(
            crate::db::repository::config_repo::find_all(&db)
                .await
                .unwrap()
                .is_empty()
        );
        for command in [
            ConfigCommand::List,
            ConfigCommand::Export(super::ExportArgs {
                include_sensitive: true,
            }),
            ConfigCommand::Validate(super::ValidateArgs {
                key: KeyArgs {
                    key: Some("custom.preview".into()),
                    ..Default::default()
                },
                input_file: None,
                input: super::ValueInput {
                    value: Some("preview".parse().unwrap()),
                    ..Default::default()
                },
            }),
        ] {
            execute_config_command_with_runtime(&database, &command, None)
                .await
                .unwrap();
            assert!(
                crate::db::repository::config_repo::find_all(&db)
                    .await
                    .unwrap()
                    .is_empty()
            );
        }
        let context = crate::cli::CliContext {
            config: crate::config::Config::default(),
            database: database.clone(),
        };
        crate::cli::execute_doctor_command(
            &crate::cli::DoctorArgs {
                database_url: None,
                strict: false,
                deep: false,
                fix: false,
                scopes: Vec::new(),
                policy_id: None,
            },
            &context,
        )
        .await;
        assert!(
            crate::db::repository::config_repo::find_all(&db)
                .await
                .unwrap()
                .is_empty()
        );
        db.close().await.unwrap();
        std::fs::remove_file(database_path).unwrap();
    }
}
