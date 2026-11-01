//! CLI 共享基础设施。
//!
//! 这里集中放置各个 CLI 子命令共用的输出格式、终端配色、数据库连接和
//! 通用渲染辅助，避免每个子命令重复处理样板逻辑。

use std::io::{self, IsTerminal};

use crate::db;
use crate::errors::{AsterError, Result};
use aster_forge_config::ConfigSource;
use clap::ValueEnum;
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

    pub(super) fn stderr() -> Self {
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

/// Confirms an operational write, requiring an explicit opt-in in automation.
pub(super) fn confirm_write(message: &str, yes: bool) -> super::CliResult<()> {
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
        Err(super::CliError::Canceled)
    }
}

pub(super) async fn connect_existing_database(
    database: &crate::config::DatabaseConfig,
    read_only: bool,
) -> Result<sea_orm::DatabaseConnection> {
    let mut database = database.clone();
    if let aster_forge_db::DatabaseUrl::Url(url) = &mut database.url
        && let Some(rest) = url
            .strip_prefix("sqlite://")
            .or_else(|| url.strip_prefix("sqlite:"))
    {
        let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
        if !path.is_empty()
            && path != ":memory:"
            && !query.split('&').any(|part| part == "mode=memory")
        {
            if !std::path::Path::new(path).is_file() {
                return Err(crate::errors::AsterError::config_error(
                    "SQLite database does not exist; check the instance location or --database-url",
                ));
            }
            let mut params: Vec<&str> = query
                .split('&')
                .filter(|p| !p.is_empty() && !p.starts_with("mode="))
                .collect();
            params.push(if read_only { "mode=ro" } else { "mode=rw" });
            *url = format!("sqlite://{path}?{}", params.join("&"));
        }
    }
    if read_only
        && let Some(url) = database
            .url
            .as_url()
            .filter(|url| url.starts_with("sqlite:"))
    {
        // The normal Forge writer connection enables WAL, which writes even
        // when the caller only runs SELECTs. Offline previews must preserve
        // the existing file and its journal mode, including an empty target.
        let mut options = sea_orm::ConnectOptions::new(url);
        options
            .max_connections(1)
            .min_connections(1)
            .sqlx_logging(false);
        return sea_orm::SqlxSqliteConnector::connect(options)
            .await
            .map_err(Into::into);
    }
    db::connect_with_metrics(&database, aster_drive_metrics::NoopMetrics::arc()).await
}

pub(super) async fn prepare_database_config(
    database: &crate::config::DatabaseConfig,
    config: &crate::config::Config,
) -> Result<sea_orm::DatabaseConnection> {
    let database = crate::config::DatabaseConfig {
        url: database.url.clone(),
        pool_size: 1,
        retry_count: 0,
    };
    let db = db::connect_with_metrics(&database, aster_drive_metrics::NoopMetrics::arc()).await?;
    crate::runtime::startup::initialize_database_state(
        &db,
        config,
        crate::config::node_mode::start_mode(config),
    )
    .await?;
    Ok(db)
}
