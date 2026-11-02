//! Config use cases, transaction boundaries and post-commit reload delivery.
use super::{
    args::ConfigCommand,
    interaction,
    model::{ConfigChange, ConfigCommandKind, ConfigCommandPayload, ConfigCommandReport},
    transfer::{ConfigBackup, read_import_items},
    values::{
        NormalizedImportItem, load_config_views, normalize_entries, resolve_validate_entries,
        resolve_value_entry, system_config_to_view,
    },
};
use crate::cli::common::database::DatabaseAccess;
use crate::cli::common::{
    context::CliContext, database::connect_existing_database, error::CliResult,
};
use crate::db::repository::config_repo;
use crate::errors::{AsterError, Result};
use crate::services::ops::config::SystemConfig;
use aster_forge_config::ConfigSource;
use aster_forge_db::{system_config, transaction};
/// Executes a runtime configuration subcommand against the target database.
pub async fn execute_config_command(
    context: &CliContext,
    command: &ConfigCommand,
) -> CliResult<ConfigCommandReport> {
    let dry_run = match command {
        ConfigCommand::Set(args) => args.dry_run,
        ConfigCommand::Delete(args) => args.dry_run,
        ConfigCommand::Import(args) => args.dry_run,
        _ => false,
    };
    let config_sync = match command {
        ConfigCommand::Set(_) | ConfigCommand::Delete(_) | ConfigCommand::Import(_) if !dry_run => {
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
    let read_only = match command {
        ConfigCommand::Set(args) => args.dry_run,
        ConfigCommand::Delete(args) => args.dry_run,
        ConfigCommand::Import(args) => args.dry_run,
        _ => true,
    };
    let db = connect_existing_database(
        database,
        if read_only {
            DatabaseAccess::ReadOnly
        } else {
            DatabaseAccess::ReadWrite
        },
    )
    .await?;

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
            changes: Vec::new(),
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
            let current = config_repo::find_all(&db).await?;
            let normalized = normalize_entries(&current, std::slice::from_ref(&entry))?;
            let normalized_item = normalized.into_iter().next().ok_or_else(|| {
                AsterError::internal_error("single config entry normalization returned no item")
            })?;
            let action = if current.iter().any(|item| item.key == normalized_item.key) {
                "would_update"
            } else {
                "would_add"
            };
            let preview = ConfigCommandReport::preview_config(normalized_item.preview(), action);
            if args.dry_run {
                return Ok(preview);
            }
            interaction::confirm_preview(&preview, "Apply this configuration update?", args.yes)?;
            let config_sync = required_cli_config_sync(config_sync)?;
            let txn = transaction::begin(&db).await.map_err(AsterError::from)?;
            let current = config_repo::find_all(&txn).await?;
            let normalized = normalize_entries(&current, std::slice::from_ref(&entry))?;
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
            let key = args.key.required()?;
            let existing = config_repo::find_by_key(&db, &key)
                .await?
                .ok_or_else(|| AsterError::record_not_found(format!("config key '{key}'")))?;
            if existing.source == ConfigSource::System {
                return Err(
                    AsterError::auth_forbidden("cannot delete system configuration").into(),
                );
            }
            let preview = ConfigCommandReport::preview_delete(key.clone());
            if args.dry_run {
                return Ok(preview);
            }
            interaction::confirm_preview(
                &preview,
                "Delete this custom configuration key?",
                args.yes,
            )?;
            let config_sync = required_cli_config_sync(config_sync)?;
            config_repo::delete_by_key(&db, &key).await?;
            publish_cli_config_reload(config_sync, [key.clone()]).await?;
            Ok(ConfigCommandReport::delete(key))
        }
        ConfigCommand::Validate(args) => {
            let current = config_repo::find_all(&db).await?;
            let entries = resolve_validate_entries(args, &current)?;
            let normalized = normalize_entries(&current, &entries)?;
            let previews: Vec<SystemConfig> = normalized
                .iter()
                .map(NormalizedImportItem::preview)
                .collect();
            Ok(ConfigCommandReport::list(
                ConfigCommandKind::Validate,
                previews,
            ))
        }
        ConfigCommand::Import(args) => {
            let entries = read_import_items(&args.input_file)?;
            let current = config_repo::find_all(&db).await?;
            let normalized = normalize_entries(&current, &entries)?;
            {
                let changes = normalized
                    .iter()
                    .map(|item| ConfigChange {
                        action: if current.iter().any(|saved| saved.key == item.key) {
                            "would_update"
                        } else {
                            "would_add"
                        },
                        key: item.key.clone(),
                    })
                    .collect();
                let previews = normalized
                    .iter()
                    .map(NormalizedImportItem::preview)
                    .collect();
                let preview = ConfigCommandReport::preview_list(previews, changes);
                if args.dry_run {
                    return Ok(preview);
                }
                interaction::confirm_preview(
                    &preview,
                    "Import these configuration entries?",
                    args.yes,
                )?;
            }
            let config_sync = required_cli_config_sync(config_sync)?;
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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
