//! `database-migrate` 的终端输出与进度展示。
//!
//! 这里根据输出格式渲染 JSON / Human 报告，并负责可选的进度条与阶段日志。

use std::io::{self, IsTerminal, Write};

use clap::ValueEnum;
use parking_lot::Mutex;
use serde::Serialize;

use crate::errors::AsterError;
use crate::utils::numbers::{u64_to_usize, u128_to_u64, usize_to_u64};

use super::{DatabaseMigrationReport, MigrationMode, PROGRESS_ENV, TablePlan};
use crate::cli::shared::render_serialization_error;

const PROGRESS_BAR_WIDTH: usize = 28;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DatabaseMigrateOutputFormat {
    Auto,
    Json,
    PrettyJson,
    Human,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolvedDatabaseMigrateOutputFormat {
    Json,
    PrettyJson,
    Human,
}

impl DatabaseMigrateOutputFormat {
    fn resolve(self) -> ResolvedDatabaseMigrateOutputFormat {
        match self {
            Self::Auto => {
                if io::stdout().is_terminal() {
                    ResolvedDatabaseMigrateOutputFormat::Human
                } else {
                    ResolvedDatabaseMigrateOutputFormat::Json
                }
            }
            Self::Json => ResolvedDatabaseMigrateOutputFormat::Json,
            Self::PrettyJson => ResolvedDatabaseMigrateOutputFormat::PrettyJson,
            Self::Human => ResolvedDatabaseMigrateOutputFormat::Human,
        }
    }
}

#[derive(Debug, Serialize)]
struct DatabaseMigrationSuccessEnvelope<'a> {
    ok: bool,
    data: &'a DatabaseMigrationReport,
}

#[derive(Debug, Serialize)]
struct DatabaseMigrationErrorEnvelope<'a> {
    ok: bool,
    error: DatabaseMigrationErrorPayload<'a>,
}

#[derive(Debug, Serialize)]
struct DatabaseMigrationErrorPayload<'a> {
    code: &'a str,
    error_type: &'a str,
    message: &'a str,
}

/// Renders a successful migration report in the selected output format.
pub fn render_database_migration_success(
    format: DatabaseMigrateOutputFormat,
    report: &DatabaseMigrationReport,
) -> String {
    match format.resolve() {
        ResolvedDatabaseMigrateOutputFormat::Json => {
            serde_json::to_string(&DatabaseMigrationSuccessEnvelope {
                ok: true,
                data: report,
            })
            .unwrap_or_else(|error| render_serialization_error(error, false))
        }
        ResolvedDatabaseMigrateOutputFormat::PrettyJson => {
            serde_json::to_string_pretty(&DatabaseMigrationSuccessEnvelope {
                ok: true,
                data: report,
            })
            .unwrap_or_else(|error| render_serialization_error(error, true))
        }
        ResolvedDatabaseMigrateOutputFormat::Human => render_database_migration_human(report),
    }
}

/// Renders a migration failure in the selected output format.
pub fn render_database_migration_error(
    format: DatabaseMigrateOutputFormat,
    err: &AsterError,
) -> String {
    match format.resolve() {
        ResolvedDatabaseMigrateOutputFormat::Json => {
            serde_json::to_string(&DatabaseMigrationErrorEnvelope {
                ok: false,
                error: DatabaseMigrationErrorPayload {
                    code: err.code(),
                    error_type: err.error_type(),
                    message: err.message(),
                },
            })
            .unwrap_or_else(|error| render_serialization_error(error, false))
        }
        ResolvedDatabaseMigrateOutputFormat::PrettyJson => {
            serde_json::to_string_pretty(&DatabaseMigrationErrorEnvelope {
                ok: false,
                error: DatabaseMigrationErrorPayload {
                    code: err.code(),
                    error_type: err.error_type(),
                    message: err.message(),
                },
            })
            .unwrap_or_else(|error| render_serialization_error(error, true))
        }
        ResolvedDatabaseMigrateOutputFormat::Human => {
            let palette = TerminalPalette::stdout();
            format!(
                "{}\n{} {}",
                palette.bad("Database migration failed"),
                palette.label("Error:"),
                err.message()
            )
        }
    }
}

#[derive(Debug, Default)]
struct ProgressState {
    active_line: bool,
}

pub(super) struct ProgressReporter {
    enabled: bool,
    interactive: bool,
    colorized: bool,
    state: Mutex<ProgressState>,
}

impl ProgressReporter {
    pub(super) fn new() -> Self {
        let interactive = io::stderr().is_terminal();
        Self {
            enabled: interactive || env_truthy(PROGRESS_ENV),
            interactive,
            colorized: colors_enabled(interactive),
            state: Mutex::new(ProgressState::default()),
        }
    }

    pub(super) fn stage(&self, stage: &str, message: impl AsRef<str>) {
        if !self.enabled {
            return;
        }

        self.clear_active_line();
        let palette = TerminalPalette::new(self.colorized);
        eprintln!(
            "{} {}: {}",
            palette.dim("[database-migrate]"),
            palette.accent(stage),
            message.as_ref()
        );
    }

    fn clear_active_line(&self) {
        if !self.interactive {
            return;
        }

        let mut state = self.state.lock();
        if !state.active_line {
            return;
        }

        eprint!("\r\x1b[2K");
        let _ = io::stderr().flush();
        state.active_line = false;
    }

    fn write_progress_line(&self, line: &str) {
        if !self.interactive {
            eprintln!("{line}");
            return;
        }

        let mut state = self.state.lock();
        eprint!("\r\x1b[2K{line}");
        let _ = io::stderr().flush();
        state.active_line = true;
    }

    fn batch_line(
        &self,
        table_index: usize,
        table_count: usize,
        plan: &TablePlan,
        table_copied: i64,
        overall_copied: i64,
        total_rows: i64,
    ) -> String {
        let palette = TerminalPalette::new(self.colorized);
        let table_pct = format_percent(table_copied, plan.source_rows);
        let overall_pct = format_percent(overall_copied, total_rows);
        let bar = progress_bar(overall_copied, total_rows, PROGRESS_BAR_WIDTH, &palette);
        format!(
            "{} {}: [{}] {} {overall_copied}/{total_rows} rows | {}/{} {} {}",
            palette.dim("[database-migrate]"),
            palette.accent("data_copy"),
            bar,
            palette.good(&overall_pct),
            table_index + 1,
            table_count,
            palette.accent(&plan.name),
            palette.accent(&table_pct)
        )
    }

    pub(super) fn batch(
        &self,
        table_index: usize,
        table_count: usize,
        plan: &TablePlan,
        table_copied: i64,
        overall_copied: i64,
        total_rows: i64,
    ) {
        if !self.enabled {
            return;
        }

        let line = self.batch_line(
            table_index,
            table_count,
            plan,
            table_copied,
            overall_copied,
            total_rows,
        );
        self.write_progress_line(&line);
    }
}

impl Drop for ProgressReporter {
    fn drop(&mut self) {
        if !self.interactive {
            return;
        }

        let mut state = self.state.lock();
        if !state.active_line {
            return;
        }

        eprintln!();
        state.active_line = false;
    }
}

#[derive(Debug, Clone, Copy)]
struct TerminalPalette {
    enabled: bool,
}

impl TerminalPalette {
    fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    fn stdout() -> Self {
        Self::new(colors_enabled(io::stdout().is_terminal()))
    }

    fn wrap(&self, codes: &str, text: &str) -> String {
        if !self.enabled {
            return text.to_string();
        }
        format!("\x1b[{codes}m{text}\x1b[0m")
    }

    fn title(&self, text: &str) -> String {
        self.wrap("1;36", text)
    }

    fn label(&self, text: &str) -> String {
        self.wrap("1;36", text)
    }

    fn accent(&self, text: &str) -> String {
        self.wrap("36", text)
    }

    fn good(&self, text: &str) -> String {
        self.wrap("32", text)
    }

    fn warn(&self, text: &str) -> String {
        self.wrap("33", text)
    }

    fn bad(&self, text: &str) -> String {
        self.wrap("31", text)
    }

    fn dim(&self, text: &str) -> String {
        self.wrap("2", text)
    }

    fn status_badge(&self, status: &str) -> String {
        match status {
            "ok" | "completed" => self.good("[OK]"),
            "attention" => self.warn("[WARN]"),
            "planned" => self.dim("[PLAN]"),
            "skipped" => self.dim("[SKIP]"),
            "running" => self.accent("[RUN]"),
            "failed" => self.bad("[FAIL]"),
            _ => self.accent("[INFO]"),
        }
    }
}

fn colors_enabled(is_terminal: bool) -> bool {
    if env_truthy("CLICOLOR_FORCE") {
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

fn env_truthy(name: &str) -> bool {
    let Some(value) = std::env::var_os(name) else {
        return false;
    };
    matches!(
        value.to_string_lossy().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn format_percent(current: i64, total: i64) -> String {
    if total <= 0 {
        return "0.0%".to_string();
    }
    format!("{:.1}%", (current as f64 / total as f64) * 100.0)
}

fn progress_bar(current: i64, total: i64, width: usize, palette: &TerminalPalette) -> String {
    if width == 0 {
        return String::new();
    }

    let filled = if total <= 0 {
        0
    } else {
        let total_u64 = u64::try_from(total).unwrap_or(0);
        let current_u64 = u64::try_from(current.clamp(0, total)).unwrap_or(0);
        let width_u64 = usize_to_u64(width, "progress bar width").unwrap_or(u64::MAX);
        let scaled = u128::from(current_u64)
            .saturating_mul(u128::from(width_u64))
            .saturating_add(u128::from(total_u64 / 2))
            / u128::from(total_u64);
        let filled_u64 = u128_to_u64(scaled, "progress bar filled width").unwrap_or(width_u64);
        u64_to_usize(filled_u64.min(width_u64), "progress bar filled width").unwrap_or(width)
    };
    let filled_bar = "#".repeat(filled);
    let empty_bar = "-".repeat(width - filled);
    format!("{}{}", palette.good(&filled_bar), palette.dim(&empty_bar))
}

fn human_key(palette: &TerminalPalette, label: &str) -> String {
    let padded = format!("{label:<14}", label = format!("{label}:"));
    palette.label(&padded)
}

fn verification_summary(report: &DatabaseMigrationReport, palette: &TerminalPalette) -> String {
    if report.ready_to_cutover {
        return format!(
            "{} checked {} unique constraints and {} foreign keys",
            palette.status_badge("ok"),
            report.verification.checked_unique_constraints,
            report.verification.checked_foreign_keys
        );
    }

    format!(
        "{} {} count mismatch(es), {} unique conflict(s), {} foreign-key violation(s)",
        palette.status_badge("attention"),
        report.verification.count_mismatches.len(),
        report.verification.unique_conflicts.len(),
        report.verification.foreign_key_violations.len()
    )
}

fn render_database_migration_human(report: &DatabaseMigrationReport) -> String {
    let palette = TerminalPalette::stdout();
    let mode = match report.mode {
        MigrationMode::Apply => "apply",
        MigrationMode::DryRun => "dry-run",
        MigrationMode::VerifyOnly => "verify-only",
    };
    let mut lines = vec![
        palette.title(human_report_title(report)),
        palette.dim("--------------------------------------------------"),
        format!("{} {}", human_key(&palette, "Mode"), mode),
        format!(
            "{} {}  {}",
            human_key(&palette, "Source"),
            report.source.backend,
            report.source.database_url
        ),
        format!(
            "{} {}  {}",
            human_key(&palette, "Target"),
            report.target.backend,
            report.target.database_url
        ),
        format!("{} {}", human_key(&palette, "Tables"), report.totals.tables),
        format!(
            "{} {}/{} copied",
            human_key(&palette, "Rows"),
            report.totals.copied_rows,
            report.totals.source_rows
        ),
        format!(
            "{} {}",
            human_key(&palette, "Duration"),
            format_duration_ms(report.totals.duration_ms)
        ),
        format!(
            "{} {} {}",
            human_key(&palette, "Cutover"),
            if report.ready_to_cutover {
                palette.status_badge("ok")
            } else {
                palette.status_badge("attention")
            },
            if report.ready_to_cutover {
                "ready"
            } else {
                "blocked by verification issues"
            }
        ),
        format!(
            "{} {}",
            human_key(&palette, "Verification"),
            verification_summary(report, &palette)
        ),
        String::new(),
        palette.label("Stages:"),
    ];

    for stage in &report.stages {
        let stage_name = format!("{:<18}", stage.name);
        lines.push(format!(
            "  {} {} {}",
            palette.status_badge(stage.status),
            palette.accent(&stage_name),
            stage.message
        ));
    }

    if report.resume.enabled {
        lines.push(String::new());
        lines.push(palette.label("Resume:"));
        lines.push(format!(
            "  {} {} checkpoint",
            human_key(&palette, "Checkpoint"),
            if report.resume.resumed {
                palette.accent("resumed existing")
            } else {
                palette.accent("created new")
            }
        ));
        lines.push(format!(
            "  {} {}",
            human_key(&palette, "Stage"),
            report.resume.stage.as_deref().unwrap_or("unknown")
        ));
        lines.push(format!(
            "  {} {} {}",
            human_key(&palette, "Status"),
            palette.status_badge(report.resume.status.as_deref().unwrap_or("unknown")),
            report.resume.status.as_deref().unwrap_or("unknown")
        ));
        lines.push(format!(
            "  {} {}/{}",
            human_key(&palette, "Rows"),
            report.resume.copied_rows.unwrap_or(0),
            report.resume.total_rows.unwrap_or(0)
        ));
    }

    if !report.ready_to_cutover {
        lines.push(String::new());
        lines.push(palette.label("Verification details:"));
        append_verification_details(&mut lines, report, &palette);
    }

    lines.join("\n")
}

fn human_report_title(report: &DatabaseMigrationReport) -> &'static str {
    match report.mode {
        MigrationMode::DryRun => "Database migration dry run complete",
        MigrationMode::VerifyOnly if report.ready_to_cutover => "Database verification complete",
        MigrationMode::VerifyOnly => "Database verification needs attention",
        MigrationMode::Apply if report.ready_to_cutover => "Database migration complete",
        MigrationMode::Apply => "Database migration complete with attention",
    }
}

fn append_verification_details(
    lines: &mut Vec<String>,
    report: &DatabaseMigrationReport,
    palette: &TerminalPalette,
) {
    if report.verification.count_mismatches.is_empty()
        && report.verification.unique_conflicts.is_empty()
        && report.verification.foreign_key_violations.is_empty()
    {
        lines.push(format!("  {}", palette.dim("none")));
        return;
    }

    for mismatch in &report.verification.count_mismatches {
        lines.push(format!(
            "  {} count mismatch on {}: source={} target={}",
            palette.bad("!"),
            mismatch.table,
            mismatch.source_rows,
            mismatch.target_rows
        ));
    }
    for conflict in &report.verification.unique_conflicts {
        lines.push(format!(
            "  {} unique conflict on {}.{} ({}) -> {} violating group(s)",
            palette.bad("!"),
            conflict.table,
            conflict.constraint,
            conflict.columns.join(", "),
            conflict.violations
        ));
    }
    for violation in &report.verification.foreign_key_violations {
        lines.push(format!(
            "  {} foreign-key violation on {}.{} via {} -> {} row(s)",
            palette.bad("!"),
            violation.table,
            violation.columns.join(", "),
            violation.constraint,
            violation.violations
        ));
    }
}

fn format_duration_ms(duration_ms: u128) -> String {
    if duration_ms >= 60_000 {
        return format!("{:.1}m", duration_ms as f64 / 60_000.0);
    }
    if duration_ms >= 1_000 {
        return format!("{:.2}s", duration_ms as f64 / 1_000.0);
    }
    format!("{duration_ms}ms")
}
