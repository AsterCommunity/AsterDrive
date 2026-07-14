//! `aster_drive config` 的聚合入口。
//!
//! 这里负责定义配置子命令、参数解析、读写/校验流程和结果渲染；
//! 底层持久化与配置规则仍复用 `config_repo` 和 `system_config` 相关逻辑。

use aster_forge_db::transaction;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

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
    value: ConfigValue,
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
    let config_sync = match command {
        ConfigCommand::Set(_) | ConfigCommand::Delete(_) | ConfigCommand::Import(_) => {
            Some(build_cli_config_sync_runtime()?)
        }
        _ => None,
    };
    execute_config_command_with_runtime(database_url, command, config_sync.as_ref()).await
}

async fn execute_config_command_with_runtime(
    database_url: &str,
    command: &ConfigCommand,
    config_sync: Option<&aster_forge_config::ConfigSyncRuntime>,
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
            let config_sync = required_cli_config_sync(config_sync)?;
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
            publish_cli_config_reload(config_sync, [saved.key.clone()]).await?;
            Ok(ConfigCommandReport::config(
                ConfigCommandKind::Set,
                system_config_to_view(saved),
            ))
        }
        ConfigCommand::Delete(args) => {
            let config_sync = required_cli_config_sync(config_sync)?;
            config_repo::delete_by_key(&db, &args.key).await?;
            publish_cli_config_reload(config_sync, [args.key.clone()]).await?;
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
            let config_sync = required_cli_config_sync(config_sync)?;
            let entries = read_import_items(&args.input_file)?;
            let txn = transaction::begin(&db).await?;
            let current_lookup = build_value_lookup(&config_repo::find_all(&txn).await?);
            let normalized = normalize_entries(current_lookup, &entries)?;
            let mut saved = Vec::with_capacity(normalized.len());
            for item in normalized {
                let model =
                    config_repo::upsert_with_actor(&txn, &item.key, &item.value, None).await?;
                saved.push(system_config_to_view(model));
            }
            transaction::commit(txn).await?;
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

fn build_cli_config_sync_runtime() -> Result<aster_forge_config::ConfigSyncRuntime> {
    if crate::config::try_get_config().is_none() {
        crate::config::init_config()?;
    }
    let config = crate::config::get_config();
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
        .map_err(crate::services::ops::config::runtime::map_config_core_error)
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
        let value_type = CONFIG_REGISTRY
            .get(&entry.key)
            .map(|def| def.value_type)
            .unwrap_or(ConfigValueType::String);
        let value = entry.value.to_storage_for_type(value_type)?;
        current_lookup.insert(entry.key.clone(), value.clone());
        storage_entries.push(NormalizedImportItem {
            key: entry.key.clone(),
            value,
        });
    }

    storage_entries
        .iter()
        .map(|entry| {
            let value = if CONFIG_REGISTRY.contains_key(&entry.key) {
                CONFIG_REGISTRY
                    .normalize_value(&current_lookup, &entry.key, &entry.value)
                    .map_err(AsterError::from)?
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

fn parse_cli_config_value(key: &str, value: &str) -> Result<ConfigValue> {
    if !CONFIG_REGISTRY
        .get(key)
        .map(|def| def.value_type.is_string_array())
        .unwrap_or(false)
    {
        return Ok(ConfigValue::String(value.to_string()));
    }

    let values = serde_json::from_str::<Vec<String>>(value).map_err(|error| {
        AsterError::validation_error(format!(
            "config key '{key}' expects a JSON array of strings: {error}"
        ))
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

    use aster_forge_config::{ConfigChangeNotifier, ConfigNotificationSource};
    use migration::Migrator;

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
                url: url.clone(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics::NoopMetrics::arc(),
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
            &database_url,
            &ConfigCommand::Set(KeyValueArgs {
                key: "custom.cli_set".to_string(),
                value: "enabled".to_string(),
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
            &database_url,
            &ConfigCommand::Set(KeyValueArgs {
                key: "custom.cli_delete".to_string(),
                value: "enabled".to_string(),
            }),
            Some(&runtime),
        )
        .await
        .expect("config CLI seed should succeed");
        let _ = next_reload(&mut subscription).await;

        execute_config_command_with_runtime(
            &database_url,
            &ConfigCommand::Delete(KeyArgs {
                key: "custom.cli_delete".to_string(),
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
            r#"[{"key":"custom.cli_b","value":"b"},{"key":"custom.cli_a","value":"a"}]"#,
        )
        .expect("config CLI import fixture should write");
        let (runtime, mut subscription) = test_runtime().await;

        execute_config_command_with_runtime(
            &database_url,
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
}
