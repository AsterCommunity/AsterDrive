//! Terminal styling and explicit confirmation for operational writes.
use crate::cli::common::error::{CliError, CliResult};
use aster_forge_config::ConfigSource;
use std::io::{self, IsTerminal};
pub(in crate::cli) struct CliTerminalPalette {
    enabled: bool,
}

impl CliTerminalPalette {
    pub(in crate::cli) fn new(enabled: bool) -> Self {
        Self { enabled }
    }
    pub(in crate::cli) fn stdout() -> Self {
        Self {
            enabled: cli_colors_enabled(io::stdout().is_terminal()),
        }
    }

    pub(in crate::cli) fn stderr() -> Self {
        Self {
            enabled: cli_colors_enabled(io::stderr().is_terminal()),
        }
    }

    fn wrap(&self, codes: &str, text: &str) -> String {
        if !self.enabled {
            return text.to_string();
        }
        format!("\x1b[{codes}m{text}\x1b[0m")
    }

    pub(in crate::cli) fn title(&self, text: &str) -> String {
        self.wrap("1;36", text)
    }

    pub(in crate::cli) fn label(&self, text: &str) -> String {
        self.wrap("1;36", text)
    }

    pub(in crate::cli) fn accent(&self, text: &str) -> String {
        self.wrap("36", text)
    }

    pub(in crate::cli) fn good(&self, text: &str) -> String {
        self.wrap("32", text)
    }

    pub(in crate::cli) fn warn(&self, text: &str) -> String {
        self.wrap("33", text)
    }

    pub(in crate::cli) fn bad(&self, text: &str) -> String {
        self.wrap("31", text)
    }

    pub(in crate::cli) fn dim(&self, text: &str) -> String {
        self.wrap("2", text)
    }

    pub(in crate::cli) fn status_badge(&self, status: &str) -> String {
        match status {
            "ok" | "updated" | "deleted" | "valid" => self.good("[OK]"),
            "warn" => self.warn("[WARN]"),
            "fail" | "error" => self.bad("[FAIL]"),
            _ => self.accent("[INFO]"),
        }
    }

    pub(in crate::cli) fn source_badge(&self, source: ConfigSource) -> String {
        match source {
            ConfigSource::System => self.good("[system]"),
            ConfigSource::Custom => self.warn("[custom]"),
        }
    }
}

pub(in crate::cli) fn cli_colors_enabled(is_terminal: bool) -> bool {
    if cli_env_truthy("CLICOLOR_FORCE") {
        return true;
    }
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if std::env::var_os("TERM")
        .is_some_and(|value| value.to_string_lossy().eq_ignore_ascii_case("dumb"))
    {
        return false;
    }
    is_terminal
}

pub(in crate::cli) fn cli_env_truthy(name: &str) -> bool {
    let Some(value) = std::env::var_os(name) else {
        return false;
    };
    matches!(
        value.to_string_lossy().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

pub(in crate::cli) fn human_key(label: &str, palette: &CliTerminalPalette) -> String {
    palette.label(&format!("{label:<14}", label = format!("{label}:")))
}

/// Confirms an operational write, requiring an explicit opt-in in automation.
pub(in crate::cli) fn confirm_write(message: &str, yes: bool) -> CliResult<()> {
    if yes {
        return Ok(());
    }
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return Err(crate::errors::AsterError::validation_error(
            "this write requires an interactive terminal; pass --yes to confirm in automation",
        )
        .into());
    }
    let confirmed = inquire::Confirm::new(message)
        .with_default(false)
        .prompt()
        .map_err(super::input::terminal_error)?;
    if confirmed {
        Ok(())
    } else {
        Err(CliError::Canceled)
    }
}
