//! Terminal-only configuration selection and editing. Never print stored secrets.

use std::collections::BTreeMap;
use std::fmt;
use std::io::IsTerminal;

use inquire::{Select, validator::Validation};
use secrecy::ExposeSecret;

use super::{
    model::ConfigCommandReport,
    render,
    transfer::ImportItem,
    values::{normalize_entries, system_config_to_view, value_entry},
};
use crate::cli::common::error::CliResult;
use crate::cli::common::input::{read_terminal_value, terminal_error};
use crate::cli::common::terminal::CliTerminalPalette;
use crate::config::definitions::CONFIG_REGISTRY;
use crate::errors::{AsterError, Result};
use crate::services::ops::config::SystemConfig;
use aster_forge_config::ConfigValueType;
use aster_forge_db::system_config;

struct ConfigChoice<'a> {
    key: &'a str,
    description: &'a str,
}

impl fmt::Display for ConfigChoice<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} — {}",
            self.key.escape_debug(),
            self.description.escape_debug()
        )
    }
}

pub(super) fn select_key(current: &[system_config::Model]) -> CliResult<String> {
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Err(AsterError::validation_error(
            "configuration key required without interactive stdin and stderr; use config set KEY VALUE",
        )
        .into());
    }
    // Include unpersisted system definitions and existing custom settings, but
    // never include values in the searchable index or its terminal output.
    let mut candidates: BTreeMap<&str, &str> = current
        .iter()
        .map(|model| (model.key.as_str(), model.description.as_str()))
        .collect();
    for definition in CONFIG_REGISTRY.definitions() {
        candidates.insert(definition.key, definition.description);
    }
    let choices = candidates
        .into_iter()
        .map(|(key, description)| ConfigChoice { key, description })
        .collect();
    Select::new("Configuration key / search", choices)
        .with_page_size(10)
        .with_help_message(
            "Type to search keys and descriptions; use arrows and Enter to select; Esc cancels",
        )
        .prompt()
        .map(|choice| choice.key.to_owned())
        .map_err(terminal_error)
}

pub(super) fn unchanged_view(key: &str, current: &[system_config::Model]) -> Result<SystemConfig> {
    if let Some(model) = current.iter().find(|model| model.key == key) {
        return Ok(system_config_to_view(model.clone()));
    }
    Err(AsterError::validation_error(
        "no saved value to keep; enter a value, use --clear, or cancel",
    ))
}

pub(super) fn prompt_value(
    key: &str,
    sensitive: bool,
    current: &[system_config::Model],
) -> CliResult<Option<ImportItem>> {
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Err(AsterError::validation_error(
            "value required without interactive stdin and stderr; use config set KEY VALUE, --value-stdin, --value-file PATH, or --clear",
        ).into());
    }
    let definition = CONFIG_REGISTRY.get(key);
    let existing = current.iter().find(|model| model.key == key);
    let value_type = definition
        .map(|def| def.value_type)
        .or_else(|| existing.map(|model| model.value_type))
        .unwrap_or(ConfigValueType::String);
    let description = definition
        .map(|def| def.description)
        .or_else(|| existing.map(|model| model.description.as_str()))
        .unwrap_or("Custom setting");
    let palette = CliTerminalPalette::stderr();
    eprintln!(
        "\n{}\n{}\n{} {}",
        palette.title(&key.escape_debug().to_string()),
        palette.dim(&description.escape_debug().to_string()),
        palette.label("Type:"),
        palette.accent(value_type.as_str())
    );
    if sensitive {
        eprintln!(
            "{} {}",
            palette.label("Current value:"),
            palette.warn("[hidden sensitive value]")
        );
    } else if let Some(model) = existing {
        // JSON quoting prevents stored control sequences from altering the terminal.
        let value = if value_type.is_multiline() || model.value.contains('\n') {
            render::summarize_multiline_value(&model.value)
        } else {
            serde_json::to_string(&model.value).unwrap_or_default()
        };
        eprintln!(
            "{} {}",
            palette.label("Current value:"),
            palette.good(&value)
        );
    } else {
        eprintln!(
            "{} {}",
            palette.label("Current value:"),
            palette.dim("[not saved]")
        );
    }
    match value_type {
        ConfigValueType::Boolean => eprintln!(
            "{}",
            palette.dim("Choose true or false (1 = true, 2 = false).")
        ),
        ConfigValueType::Number => {
            eprintln!(
                "{}",
                palette.dim("Enter a number; the setting's constraints are checked before saving.")
            )
        }
        ConfigValueType::StringArray | ConfigValueType::StringEnumSet => {
            eprintln!(
                "{}",
                palette.dim("Enter a JSON array of strings, for example [\"value\"].")
            );
        }
        ConfigValueType::Multiline => {
            eprintln!(
                "{}",
                palette.dim("For multiple lines, use --value-stdin or --value-file PATH.")
            )
        }
        _ => {}
    }
    eprintln!(
        "{}",
        palette.dim(
            "Enter keeps the saved value; --clear explicitly empties it. Ctrl+C or Esc cancels."
        )
    );
    let label = if sensitive {
        "New value (masked): "
    } else {
        "New value: "
    };
    let validation_current = current.to_vec();
    let validation_key = key.to_owned();
    let has_saved_value = existing.is_some();
    let validator = move |raw: &str| {
        if raw.is_empty() {
            return Ok(if has_saved_value {
                Validation::Valid
            } else {
                Validation::Invalid("No saved value to keep; enter a value or cancel".into())
            });
        }
        // Reuse the write-path rules; all callback errors remain redacted.
        let checked = value_entry(
            &validation_key,
            interactive_value(value_type, raw),
            sensitive,
            &validation_current,
        )
        .and_then(|entry| normalize_entries(&validation_current, &[entry]));
        Ok(match checked {
            Ok(_) => Validation::Valid,
            Err(error) => Validation::Invalid(error.message().to_owned().into()),
        })
    };
    let input = read_terminal_value(sensitive, label, validator)?;
    if input.expose_secret().is_empty() {
        return Ok(None);
    }
    value_entry(
        key,
        interactive_value(value_type, input.expose_secret()),
        sensitive,
        current,
    )
    .map(Some)
    .map_err(Into::into)
}

fn interactive_value(value_type: ConfigValueType, value: &str) -> &str {
    match (value_type, value) {
        (ConfigValueType::Boolean, "1") => "true",
        (ConfigValueType::Boolean, "2") => "false",
        _ => value,
    }
}

/// Displays the safe preview on stderr before asking for write confirmation.
pub(super) fn confirm_preview(
    report: &ConfigCommandReport,
    prompt: &str,
    yes: bool,
) -> CliResult<()> {
    if !yes && std::io::stdin().is_terminal() && std::io::stderr().is_terminal() {
        eprintln!(
            "{}",
            render::render_success(crate::cli::common::output::OutputFormat::Human, report)
        );
    }
    crate::cli::common::terminal::confirm_write(prompt, yes)
}
