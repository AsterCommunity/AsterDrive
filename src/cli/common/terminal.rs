//! Terminal styling and explicit confirmation for operational writes.
use crate::cli::common::error::{CliError, CliResult};
use aster_forge_config::ConfigSource;
use std::io::{self, IsTerminal};

/// Escapes controls with JSON-compatible sequences, preserving existing JSON quoting.
pub(in crate::cli) fn escape_terminal_controls(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\n' => output.push_str(r"\n"),
            '\r' => output.push_str(r"\r"),
            '\t' => output.push_str(r"\t"),
            '\u{08}' => output.push_str(r"\b"),
            '\u{0c}' => output.push_str(r"\f"),
            control if control.is_control() => {
                output.push_str(&format!("\\u{:04x}", u32::from(control)));
            }
            ordinary => output.push(ordinary),
        }
    }
    output
}

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

#[cfg(test)]
mod tests {
    use super::escape_terminal_controls;

    #[test]
    fn terminal_text_escapes_c0_del_and_c1_but_preserves_readable_text() {
        let text = "猫's \\ path / \"quoted\"";
        assert_eq!(escape_terminal_controls(text), text);
        for code in (0..=0x1f).chain(0x7f..=0x9f) {
            let control = char::from_u32(code).unwrap();
            let escaped = escape_terminal_controls(&format!("before{control}after"));
            assert!(!escaped.chars().any(char::is_control), "{code:x}");
            assert!(escaped.starts_with("before\\"), "{escaped}");
            assert!(escaped.ends_with("after"), "{escaped}");
            assert_eq!(
                serde_json::from_str::<String>(&format!("\"{escaped}\"")).unwrap(),
                format!("before{control}after")
            );
        }
        assert_eq!(
            escape_terminal_controls("\x1b[2J\u{9b}31m\u{9d}title\u{9c}\r\n\t"),
            r"\u001b[2J\u009b31m\u009dtitle\u009c\r\n\t"
        );
    }
}
