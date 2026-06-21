//! `aster_drive config` 的聚合入口。
//!
//! 这里负责定义配置子命令、参数解析、读写/校验流程和结果渲染；
//! 底层持久化与配置规则仍复用 `config_repo` 和 `system_config` 相关逻辑。

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use crate::config::system_config as shared_system_config;
use crate::db::repository::config_repo;
use crate::entities::system_config;
use crate::errors::{AsterError, Result};
use crate::services::config_service::{SystemConfig, SystemConfigValue};
use crate::types::{SystemConfigSource, SystemConfigValueType};
use crate::utils::char_count;
use chrono::Utc;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

use super::shared::{
    CliTerminalPalette, OutputFormat, ResolvedOutputFormat, connect_database, human_key,
    render_error_json, render_success_envelope,
};

#[derive(Debug, Clone, Subcommand)]
pub enum ConfigCommand {
    /// List runtime config entries
    List,
    /// Get a runtime config entry
    Get(KeyArgs),
    /// Set a runtime config entry
    Set(KeyValueArgs),
    /// Delete a custom runtime config entry
    Delete(KeyArgs),
    /// Validate runtime config input without writing it
    Validate(ValidateArgs),
    /// Export runtime config entries
    Export,
    /// Import runtime config entries
    Import(FileArgs),
}

#[derive(Debug, Clone, Args)]
pub struct KeyArgs {
    #[arg(long, env = "ASTER_CLI_CONFIG_KEY")]
    pub key: String,
}

#[derive(Debug, Clone, Args)]
pub struct KeyValueArgs {
    #[arg(long, env = "ASTER_CLI_CONFIG_KEY")]
    pub key: String,
    #[arg(long, env = "ASTER_CLI_CONFIG_VALUE")]
    pub value: String,
}

#[derive(Debug, Clone, Args)]
pub struct FileArgs {
    #[arg(long, env = "ASTER_CLI_INPUT_FILE")]
    pub input_file: PathBuf,
}

#[derive(Debug, Clone, Args)]
pub struct ValidateArgs {
    #[arg(long, env = "ASTER_CLI_CONFIG_KEY")]
    pub key: Option<String>,
    #[arg(long, env = "ASTER_CLI_CONFIG_VALUE")]
    pub value: Option<String>,
    #[arg(long, env = "ASTER_CLI_INPUT_FILE")]
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
}

#[derive(Debug)]
enum ConfigCommandPayload {
    List(ConfigListOutput),
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

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ImportItem {
    key: String,
    value: SystemConfigValue,
}

#[derive(Debug, Clone)]
struct NormalizedImportItem {
    key: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct ImportFilePayload {
    configs: Vec<ImportItem>,
}

/// Executes a runtime configuration subcommand against the target database.
pub async fn execute_config_command(
    database_url: &str,
    command: &ConfigCommand,
) -> Result<ConfigCommandReport> {
    let db = connect_database(database_url).await?;

    match command {
        ConfigCommand::List | ConfigCommand::Export => {
            let configs = load_config_views(&db).await?;
            Ok(ConfigCommandReport::list(
                match command {
                    ConfigCommand::List => ConfigCommandKind::List,
                    ConfigCommand::Export => ConfigCommandKind::Export,
                    _ => ConfigCommandKind::List,
                },
                configs,
            ))
        }
        ConfigCommand::Get(args) => {
            let config = config_repo::find_by_key(&db, &args.key)
                .await?
                .map(system_config_to_view)
                .ok_or_else(|| {
                    AsterError::record_not_found(format!("config key '{}'", args.key))
                })?;
            Ok(ConfigCommandReport::config(ConfigCommandKind::Get, config))
        }
        ConfigCommand::Set(args) => {
            let normalized = normalize_entries(
                build_value_lookup(&config_repo::find_all(&db).await?),
                &[ImportItem {
                    key: args.key.clone(),
                    value: parse_cli_config_value(&args.key, &args.value)?,
                }],
            )?;
            let normalized_item = normalized.into_iter().next().ok_or_else(|| {
                AsterError::internal_error("single config entry normalization returned no item")
            })?;
            let saved = config_repo::upsert_with_actor(
                &db,
                &normalized_item.key,
                &normalized_item.value,
                None,
            )
            .await?;
            Ok(ConfigCommandReport::config(
                ConfigCommandKind::Set,
                system_config_to_view(saved),
            ))
        }
        ConfigCommand::Delete(args) => {
            config_repo::delete_by_key(&db, &args.key).await?;
            Ok(ConfigCommandReport::delete(args.key.clone()))
        }
        ConfigCommand::Validate(args) => {
            let entries = resolve_validate_entries(args)?;
            let current_lookup = build_value_lookup(&config_repo::find_all(&db).await?);
            let normalized = normalize_entries(current_lookup, &entries)?;
            let previews: Vec<SystemConfig> = normalized
                .into_iter()
                .map(|item| preview_system_config(&item.key, &item.value))
                .collect();
            Ok(ConfigCommandReport::list(
                ConfigCommandKind::Validate,
                previews,
            ))
        }
        ConfigCommand::Import(args) => {
            let entries = read_import_items(&args.input_file)?;
            let txn = crate::db::transaction::begin(&db).await?;
            let current_lookup = build_value_lookup(&config_repo::find_all(&txn).await?);
            let normalized = normalize_entries(current_lookup, &entries)?;
            let mut saved = Vec::with_capacity(normalized.len());
            for item in normalized {
                let model =
                    config_repo::upsert_with_actor(&txn, &item.key, &item.value, None).await?;
                saved.push(system_config_to_view(model));
            }
            crate::db::transaction::commit(txn).await?;
            Ok(ConfigCommandReport::list(ConfigCommandKind::Import, saved))
        }
    }
}

/// Renders a successful config command result in the requested output format.
pub fn render_success(format: OutputFormat, data: &ConfigCommandReport) -> String {
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
        ConfigCommandPayload::List(payload) => render_success_envelope(payload, pretty),
        ConfigCommandPayload::Config(payload) => render_success_envelope(payload, pretty),
        ConfigCommandPayload::Delete(payload) => render_success_envelope(payload, pretty),
    }
}

fn render_config_human(report: &ConfigCommandReport) -> String {
    let palette = CliTerminalPalette::stdout();
    match &report.payload {
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

    if let SystemConfigValue::String(value) = &config.value
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
    let label = if looks_like_json(trimmed) {
        "json value"
    } else if looks_like_html(trimmed) {
        "html template"
    } else {
        "multiline value"
    };
    format!("<{label}: {line_count} lines, {chars} chars>")
}

fn looks_like_json(value: &str) -> bool {
    value.starts_with('{') || value.starts_with('[')
}

fn looks_like_html(value: &str) -> bool {
    value.starts_with('<')
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
        _ => "Configurations",
    }
}

fn config_entry_title(kind: ConfigCommandKind) -> &'static str {
    match kind {
        ConfigCommandKind::Get => "Configuration value",
        ConfigCommandKind::Set => "Configuration updated",
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

fn build_value_lookup(models: &[system_config::Model]) -> HashMap<String, String> {
    models
        .iter()
        .map(|model| (model.key.clone(), model.value.clone()))
        .collect()
}

fn normalize_entries(
    mut current_lookup: HashMap<String, String>,
    entries: &[ImportItem],
) -> Result<Vec<NormalizedImportItem>> {
    let mut seen_keys = BTreeSet::new();
    let mut storage_entries = Vec::with_capacity(entries.len());

    for entry in entries {
        if !seen_keys.insert(entry.key.clone()) {
            return Err(AsterError::validation_error(format!(
                "duplicate config key '{}' in input",
                entry.key
            )));
        }
        let value_type = shared_system_config::get_definition(&entry.key)
            .map(|def| def.value_type)
            .unwrap_or(SystemConfigValueType::String);
        let value = entry.value.to_storage_for_type(value_type)?;
        current_lookup.insert(entry.key.clone(), value.clone());
        storage_entries.push(NormalizedImportItem {
            key: entry.key.clone(),
            value,
        });
    }

    for entry in &storage_entries {
        if let Some(def) = shared_system_config::get_definition(&entry.key) {
            shared_system_config::validate_value_type(def.value_type, &entry.value)?;
        }
    }

    storage_entries
        .iter()
        .map(|entry| {
            let value = if shared_system_config::get_definition(&entry.key).is_some() {
                shared_system_config::normalize_system_value(
                    &current_lookup,
                    &entry.key,
                    &entry.value,
                )?
            } else {
                entry.value.clone()
            };

            Ok(NormalizedImportItem {
                key: entry.key.clone(),
                value,
            })
        })
        .collect()
}

fn resolve_validate_entries(args: &ValidateArgs) -> Result<Vec<ImportItem>> {
    match (&args.input_file, &args.key, &args.value) {
        (Some(path), None, None) => read_import_items(path),
        (None, Some(key), Some(value)) => Ok(vec![ImportItem {
            key: key.clone(),
            value: parse_cli_config_value(key, value)?,
        }]),
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) => Err(AsterError::validation_error(
            "validate accepts either ASTER_CLI_INPUT_FILE or ASTER_CLI_CONFIG_KEY + ASTER_CLI_CONFIG_VALUE",
        )),
        _ => Err(AsterError::validation_error(
            "validate requires ASTER_CLI_INPUT_FILE or ASTER_CLI_CONFIG_KEY + ASTER_CLI_CONFIG_VALUE",
        )),
    }
}

fn parse_cli_config_value(key: &str, value: &str) -> Result<SystemConfigValue> {
    if !shared_system_config::get_definition(key)
        .map(|def| def.value_type.is_string_array())
        .unwrap_or(false)
    {
        return Ok(SystemConfigValue::String(value.to_string()));
    }

    let values = serde_json::from_str::<Vec<String>>(value).map_err(|error| {
        AsterError::validation_error(format!(
            "config key '{key}' expects a JSON array of strings: {error}"
        ))
    })?;
    Ok(SystemConfigValue::StringArray(values))
}

fn format_config_value(value: &SystemConfigValue) -> String {
    match value {
        SystemConfigValue::String(value) => value.clone(),
        SystemConfigValue::StringArray(values) => serde_json::to_string(values)
            .unwrap_or_else(|_| "<invalid string_array value>".to_string()),
    }
}

fn read_import_items(path: &Path) -> Result<Vec<ImportItem>> {
    let content = std::fs::read_to_string(path).map_err(|error| {
        AsterError::config_error(format!(
            "failed to read input file '{}': {error}",
            path.display()
        ))
    })?;

    if let Ok(items) = serde_json::from_str::<Vec<ImportItem>>(&content) {
        return Ok(items);
    }

    let payload = serde_json::from_str::<ImportFilePayload>(&content).map_err(|error| {
        AsterError::validation_error(format!(
            "input file '{}' must be a JSON array or {{\"configs\": [...]}} payload: {error}",
            path.display()
        ))
    })?;
    Ok(payload.configs)
}

fn preview_system_config(key: &str, value: &str) -> SystemConfig {
    let model = if let Some(def) = shared_system_config::get_definition(key) {
        system_config::Model {
            id: 0,
            key: key.to_string(),
            value: value.to_string(),
            value_type: def.value_type,
            requires_restart: def.requires_restart,
            is_sensitive: def.is_sensitive,
            source: SystemConfigSource::System,
            visibility: crate::types::SystemConfigVisibility::Private,
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
            value_type: SystemConfigValueType::String,
            requires_restart: false,
            is_sensitive: false,
            source: SystemConfigSource::Custom,
            visibility: crate::types::SystemConfigVisibility::Private,
            namespace: String::new(),
            category: String::new(),
            description: String::new(),
            updated_at: Utc::now(),
            updated_by: None,
        }
    };

    system_config_to_view(model)
}
