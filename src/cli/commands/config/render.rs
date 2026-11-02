//! Pure human and JSON rendering for config reports.
use super::model::{
    ConfigCommandKind, ConfigCommandPayload, ConfigCommandReport, ConfigListOutput, DeleteOutput,
};
use crate::cli::common::{
    output::{OutputFormat, ResolvedOutputFormat, render_error_json, render_success_envelope},
    terminal::{CliTerminalPalette, human_key},
};
use crate::errors::AsterError;
use crate::services::ops::config::SystemConfig;
use aster_forge_config::ConfigValue;
use aster_forge_validation::filename::char_count;
/// Renders a successful config command result in the requested output format.
pub(in crate::cli) fn render_success(format: OutputFormat, data: &ConfigCommandReport) -> String {
    if let ConfigCommandPayload::Backup(backup) = &data.payload {
        return serde_json::to_string_pretty(backup)
            .unwrap_or_else(|_| "{\"error\":\"cannot serialize configuration backup\"}".into());
    }
    if matches!(data.kind, ConfigCommandKind::Preview) {
        if matches!(format.resolve(), ResolvedOutputFormat::Human) {
            let mut output = render_config_human(data);
            output.push_str("\n\nPlanned changes (no writes):");
            for change in &data.changes {
                output.push_str(&format!("\n  {} {}", change.action, change.key));
            }
            if data.changes.is_empty() {
                output.push_str("\n  No changes.");
            }
            return output;
        }
        let payload = serde_json::json!({ "preview": &data.payload, "changes": &data.changes, "dry_run": true });
        return render_success_envelope(
            &payload,
            matches!(format.resolve(), ResolvedOutputFormat::PrettyJson),
        );
    }
    match format.resolve() {
        ResolvedOutputFormat::Json => render_config_json(data, false),
        ResolvedOutputFormat::PrettyJson => render_config_json(data, true),
        ResolvedOutputFormat::Human => render_config_human(data),
    }
}

/// Renders a config command failure in the requested output format.
pub(in crate::cli) fn render_error(format: OutputFormat, err: &AsterError) -> String {
    match format.resolve() {
        ResolvedOutputFormat::Json => render_error_json(err, false),
        ResolvedOutputFormat::PrettyJson => render_error_json(err, true),
        ResolvedOutputFormat::Human => render_error_human(err),
    }
}

pub(super) fn render_config_json(report: &ConfigCommandReport, pretty: bool) -> String {
    match &report.payload {
        ConfigCommandPayload::Backup(payload) => {
            serde_json::to_string_pretty(payload).unwrap_or_default()
        }
        ConfigCommandPayload::List(payload) => render_success_envelope(payload, pretty),
        ConfigCommandPayload::Config(payload) => render_success_envelope(payload, pretty),
        ConfigCommandPayload::Delete(payload) => render_success_envelope(payload, pretty),
    }
}

pub(super) fn render_config_human(report: &ConfigCommandReport) -> String {
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
        ConfigCommandPayload::Delete(payload) => {
            render_config_delete_human(report.kind, payload, &palette)
        }
    }
}

pub(super) fn render_config_list_human(
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

pub(super) fn format_config_list_value(
    config: &SystemConfig,
    palette: &CliTerminalPalette,
) -> String {
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

// Escape only controls so ordinary text, including Unicode and paths, stays readable.
fn format_config_value(value: &ConfigValue) -> String {
    let value = match value {
        ConfigValue::String(value) => value.clone(),
        ConfigValue::StringArray(values) => serde_json::to_string(values)
            .unwrap_or_else(|_| "<invalid string_array value>".to_string()),
    };
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_control() {
            output.extend(character.escape_debug());
        } else {
            output.push(character);
        }
    }
    output
}

pub(super) fn summarize_multiline_value(value: &str) -> String {
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

pub(super) fn render_config_entry_human(
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
            ConfigCommandKind::Preview =>
                format!("{} planned, not written", palette.status_badge("planned")),
            ConfigCommandKind::Unchanged => format!("{} unchanged", palette.status_badge("ok")),
            _ => format!("{} loaded", palette.status_badge("ok")),
        }
    ));

    lines.join("\n")
}

pub(super) fn render_config_delete_human(
    kind: ConfigCommandKind,
    payload: &DeleteOutput,
    palette: &CliTerminalPalette,
) -> String {
    [
        palette.title(if matches!(kind, ConfigCommandKind::Preview) {
            "Configuration change preview"
        } else {
            "Configuration deleted"
        }),
        palette.dim("--------------------------------------------------"),
        format!("{} {}", human_key("Key", palette), payload.key),
        format!(
            "{} {} {}",
            human_key("Status", palette),
            palette.status_badge(if matches!(kind, ConfigCommandKind::Preview) {
                "planned"
            } else {
                "deleted"
            }),
            if payload.deleted {
                "deleted"
            } else if matches!(kind, ConfigCommandKind::Preview) {
                "would delete"
            } else {
                "not deleted"
            }
        ),
    ]
    .join("\n")
}

pub(in crate::cli) fn render_error_human(err: &AsterError) -> String {
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

pub(super) fn config_list_title(kind: ConfigCommandKind) -> &'static str {
    match kind {
        ConfigCommandKind::List => "Configuration list",
        ConfigCommandKind::Export => "Configuration export",
        ConfigCommandKind::Import => "Configuration import complete",
        ConfigCommandKind::Validate => "Configuration validation preview",
        ConfigCommandKind::Unchanged => "Configuration unchanged",
        ConfigCommandKind::Preview => "Configuration change preview",
        _ => "Configurations",
    }
}

pub(super) fn config_entry_title(kind: ConfigCommandKind) -> &'static str {
    match kind {
        ConfigCommandKind::Get => "Configuration value",
        ConfigCommandKind::Set => "Configuration updated",
        ConfigCommandKind::Unchanged => "Configuration unchanged",
        ConfigCommandKind::Validate => "Configuration validation preview",
        ConfigCommandKind::Preview => "Configuration change preview",
        _ => "Configuration",
    }
}

pub(super) fn yes_no_label(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::commands::config::values::preview_system_config;

    #[test]
    fn human_values_escape_terminal_controls_without_changing_json_or_multiline_summaries() {
        let raw = "猫's \\ path\x1b[2J\x1b]0;spoofed\x07\r\t\u{009b}31m";
        let config = preview_system_config("custom.terminal", raw);
        let palette = CliTerminalPalette::new(false);
        let entry = render_config_entry_human(ConfigCommandKind::Preview, &config, &palette);
        let list = render_config_list_human(
            ConfigCommandKind::Preview,
            &ConfigListOutput {
                count: 1,
                configs: vec![config.clone()],
            },
            &palette,
        );
        for output in [entry, list] {
            assert!(
                !output.chars().any(|c| c.is_control() && c != '\n'),
                "{output:?}"
            );
            assert!(output.contains("猫's \\ path"));
            assert!(output.contains(r"\u{1b}[2J"));
            assert!(output.contains(r"\u{1b}]0;spoofed\u{7}\r\t\u{9b}31m"));
        }
        // JSON serialization already escapes C0 controls, but can preserve C1 CSI/OSC.
        let array_value = ConfigValue::StringArray(vec![raw.to_owned()]);
        let rendered_array = format_config_value(&array_value);
        assert!(!rendered_array.chars().any(char::is_control));
        assert!(rendered_array.contains(r"\u{9b}31m"));
        let report = ConfigCommandReport::preview_config(config, "would_insert");
        for format in [OutputFormat::Json, OutputFormat::PrettyJson] {
            let output: serde_json::Value =
                serde_json::from_str(&render_success(format, &report)).unwrap();
            assert_eq!(output["data"]["preview"]["data"]["value"], raw);
        }
        let multiline = preview_system_config("custom.multiline", "line one\nline two\x1b[2J");
        assert_eq!(
            format_config_list_value(&multiline, &palette),
            summarize_multiline_value("line one\nline two\x1b[2J")
        );
    }

    #[test]
    fn human_entry_status_distinguishes_planned_unchanged_and_completed_actions() {
        let config = preview_system_config("custom.status", "value");
        let palette = CliTerminalPalette::new(false);
        for (kind, expected) in [
            (ConfigCommandKind::Preview, "planned, not written"),
            (ConfigCommandKind::Unchanged, "unchanged"),
            (ConfigCommandKind::Set, "updated"),
            (ConfigCommandKind::Validate, "valid preview"),
            (ConfigCommandKind::Get, "loaded"),
        ] {
            let output = render_config_entry_human(kind, &config, &palette);
            assert!(
                output.lines().last().unwrap().ends_with(expected),
                "{output}"
            );
        }
    }
}
