//! Explicit terminal progress sink for the offline migration lifecycle.
use super::{model::TablePlan, plan::PROGRESS_ENV};
use crate::cli::common::terminal::{CliTerminalPalette, cli_colors_enabled, cli_env_truthy};
use aster_forge_utils::numbers::{u64_to_usize, u128_to_u64, usize_to_u64};
use parking_lot::Mutex;
use std::io::{self, IsTerminal, Write};
const PROGRESS_BAR_WIDTH: usize = 28;
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
            enabled: interactive || cli_env_truthy(PROGRESS_ENV),
            interactive,
            colorized: cli_colors_enabled(interactive),
            state: Mutex::new(ProgressState::default()),
        }
    }

    pub(super) fn stage(&self, stage: &str, message: impl AsRef<str>) {
        if !self.enabled {
            return;
        }

        self.clear_active_line();
        let palette = CliTerminalPalette::new(self.colorized);
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
        let palette = CliTerminalPalette::new(self.colorized);
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

fn format_percent(current: i64, total: i64) -> String {
    if total <= 0 {
        return "0.0%".to_string();
    }
    format!("{:.1}%", (current as f64 / total as f64) * 100.0)
}

fn progress_bar(current: i64, total: i64, width: usize, palette: &CliTerminalPalette) -> String {
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
