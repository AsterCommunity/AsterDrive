//! CLI 共享基础设施。
//!
//! 这里集中放置各个 CLI 子命令共用的输出格式、终端配色、数据库连接和
//! 通用渲染辅助，避免每个子命令重复处理样板逻辑。

use std::io::{self, IsTerminal};

use crate::db;
use crate::db::repository::config_repo;
use crate::errors::{AsterError, Result};
use crate::types::ConfigSource;
use clap::ValueEnum;
use clap::builder::styling::{AnsiColor, Effects, Styles};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Auto,
    Json,
    PrettyJson,
    Human,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResolvedOutputFormat {
    Json,
    PrettyJson,
    Human,
}

impl OutputFormat {
    pub(super) fn resolve(self) -> ResolvedOutputFormat {
        match self {
            Self::Auto => {
                if io::stdout().is_terminal() {
                    ResolvedOutputFormat::Human
                } else {
                    ResolvedOutputFormat::Json
                }
            }
            Self::Json => ResolvedOutputFormat::Json,
            Self::PrettyJson => ResolvedOutputFormat::PrettyJson,
            Self::Human => ResolvedOutputFormat::Human,
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct SuccessEnvelope<T> {
    pub ok: bool,
    pub data: T,
}

#[derive(Debug, Serialize)]
pub(super) struct ErrorEnvelope<'a> {
    pub ok: bool,
    pub error: ErrorPayload<'a>,
}

#[derive(Debug, Serialize)]
pub(super) struct ErrorPayload<'a> {
    pub code: &'a str,
    pub error_type: &'a str,
    pub message: &'a str,
}

pub(super) fn render_success_envelope<T>(data: &T, pretty: bool) -> String
where
    T: Serialize,
{
    let envelope = SuccessEnvelope { ok: true, data };
    if pretty {
        serde_json::to_string_pretty(&envelope)
            .unwrap_or_else(|error| render_serialization_error(error, true))
    } else {
        serde_json::to_string(&envelope)
            .unwrap_or_else(|error| render_serialization_error(error, false))
    }
}

pub(super) fn render_error_json(err: &AsterError, pretty: bool) -> String {
    let envelope = ErrorEnvelope {
        ok: false,
        error: ErrorPayload {
            code: err.code(),
            error_type: err.error_type(),
            message: err.message(),
        },
    };
    if pretty {
        serde_json::to_string_pretty(&envelope)
            .unwrap_or_else(|error| render_serialization_error(error, true))
    } else {
        serde_json::to_string(&envelope)
            .unwrap_or_else(|error| render_serialization_error(error, false))
    }
}

pub(super) fn render_serialization_error(error: serde_json::Error, pretty: bool) -> String {
    let message = format!("failed to serialize CLI envelope: {error}");
    let fallback = ErrorEnvelope {
        ok: false,
        error: ErrorPayload {
            code: "serialization_error",
            error_type: "internal",
            message: &message,
        },
    };
    if pretty {
        serde_json::to_string_pretty(&fallback).unwrap_or_default()
    } else {
        serde_json::to_string(&fallback).unwrap_or_default()
    }
}

pub(super) struct CliTerminalPalette {
    enabled: bool,
}

impl CliTerminalPalette {
    pub(super) fn stdout() -> Self {
        Self {
            enabled: cli_colors_enabled(io::stdout().is_terminal()),
        }
    }

    fn wrap(&self, codes: &str, text: &str) -> String {
        if !self.enabled {
            return text.to_string();
        }
        format!("\x1b[{codes}m{text}\x1b[0m")
    }

    pub(super) fn title(&self, text: &str) -> String {
        self.wrap("1;36", text)
    }

    pub(super) fn label(&self, text: &str) -> String {
        self.wrap("1;36", text)
    }

    pub(super) fn accent(&self, text: &str) -> String {
        self.wrap("36", text)
    }

    pub(super) fn good(&self, text: &str) -> String {
        self.wrap("32", text)
    }

    pub(super) fn warn(&self, text: &str) -> String {
        self.wrap("33", text)
    }

    pub(super) fn bad(&self, text: &str) -> String {
        self.wrap("31", text)
    }

    pub(super) fn dim(&self, text: &str) -> String {
        self.wrap("2", text)
    }

    pub(super) fn status_badge(&self, status: &str) -> String {
        match status {
            "ok" | "updated" | "deleted" | "valid" => self.good("[OK]"),
            "warn" => self.warn("[WARN]"),
            "fail" | "error" => self.bad("[FAIL]"),
            _ => self.accent("[INFO]"),
        }
    }

    pub(super) fn source_badge(&self, source: ConfigSource) -> String {
        match source {
            ConfigSource::System => self.good("[system]"),
            ConfigSource::Custom => self.warn("[custom]"),
        }
    }
}

fn cli_colors_enabled(is_terminal: bool) -> bool {
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

fn cli_env_truthy(name: &str) -> bool {
    let Some(value) = std::env::var_os(name) else {
        return false;
    };
    matches!(
        value.to_string_lossy().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

pub(super) fn human_key(label: &str, palette: &CliTerminalPalette) -> String {
    palette.label(&format!("{label:<14}", label = format!("{label}:")))
}

/// Builds the clap styling palette shared by all CLI subcommands.
pub fn cli_styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Green.on_default() | Effects::BOLD)
        .usage(AnsiColor::Green.on_default() | Effects::BOLD)
        .literal(AnsiColor::Cyan.on_default() | Effects::BOLD)
        .placeholder(AnsiColor::Yellow.on_default())
}

pub(super) async fn connect_database(database_url: &str) -> Result<sea_orm::DatabaseConnection> {
    let db = db::connect_with_metrics(
        &crate::config::DatabaseConfig {
            url: database_url.to_string(),
            pool_size: 1,
            retry_count: 0,
        },
        crate::metrics::NoopMetrics::arc(),
    )
    .await?;
    config_repo::ensure_defaults_with_env(&db, &|name| std::env::var(name).ok()).await?;
    Ok(db)
}

pub(super) async fn prepare_database(database_url: &str) -> Result<sea_orm::DatabaseConnection> {
    if crate::config::try_get_config().is_none() {
        crate::config::init_config()?;
    }
    let cfg = crate::config::get_config();
    let db = db::connect_with_metrics(
        &crate::config::DatabaseConfig {
            url: database_url.to_string(),
            pool_size: 1,
            retry_count: 0,
        },
        crate::metrics::NoopMetrics::arc(),
    )
    .await?;
    crate::runtime::startup::initialize_database_state(
        &db,
        cfg.as_ref(),
        crate::config::node_mode::start_mode(cfg.as_ref()),
    )
    .await?;
    Ok(db)
}
