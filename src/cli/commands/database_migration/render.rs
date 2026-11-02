//! Pure migration report rendering with the shared CLI output contract.
use super::model::{DatabaseMigrationReport, MigrationMode};
use crate::cli::common::{
    output::{OutputFormat, ResolvedOutputFormat, render_error_json, render_success_envelope},
    terminal::CliTerminalPalette,
};
use crate::errors::AsterError;
/// Renders a successful migration report in the selected output format.
pub fn render_database_migration_success(
    format: OutputFormat,
    report: &DatabaseMigrationReport,
) -> String {
    match format.resolve() {
        ResolvedOutputFormat::Json => render_success_envelope(report, false),
        ResolvedOutputFormat::PrettyJson => render_success_envelope(report, true),
        ResolvedOutputFormat::Human => render_database_migration_human(report),
    }
}

/// Renders a migration failure in the selected output format.
pub fn render_database_migration_error(format: OutputFormat, err: &AsterError) -> String {
    match format.resolve() {
        ResolvedOutputFormat::Json => render_error_json(err, false),
        ResolvedOutputFormat::PrettyJson => render_error_json(err, true),
        ResolvedOutputFormat::Human => {
            let palette = CliTerminalPalette::stdout();
            format!(
                "{}\n{} {}",
                palette.bad("Database migration failed"),
                palette.label("Error:"),
                err.message()
            )
        }
    }
}

fn human_key(palette: &CliTerminalPalette, label: &str) -> String {
    let padded = format!("{label:<14}", label = format!("{label}:"));
    palette.label(&padded)
}

fn verification_summary(report: &DatabaseMigrationReport, palette: &CliTerminalPalette) -> String {
    if report.ready_to_cutover {
        return format!(
            "{} checked {} unique constraints and {} foreign keys",
            migration_status_badge(palette, "ok"),
            report.verification.checked_unique_constraints,
            report.verification.checked_foreign_keys
        );
    }

    format!(
        "{} {} count mismatch(es), {} unique conflict(s), {} foreign-key violation(s)",
        migration_status_badge(palette, "attention"),
        report.verification.count_mismatches.len(),
        report.verification.unique_conflicts.len(),
        report.verification.foreign_key_violations.len()
    )
}

fn render_database_migration_human(report: &DatabaseMigrationReport) -> String {
    let palette = CliTerminalPalette::stdout();
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
                migration_status_badge(&palette, "ok")
            } else {
                migration_status_badge(&palette, "attention")
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
            migration_status_badge(&palette, stage.status),
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
            migration_status_badge(
                &palette,
                report.resume.status.as_deref().unwrap_or("unknown")
            ),
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
    palette: &CliTerminalPalette,
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

fn migration_status_badge(palette: &CliTerminalPalette, status: &str) -> String {
    match status {
        "ok" | "completed" => palette.good("[OK]"),
        "attention" => palette.warn("[WARN]"),
        "planned" => palette.dim("[PLAN]"),
        "skipped" => palette.dim("[SKIP]"),
        "running" => palette.accent("[RUN]"),
        "failed" => palette.bad("[FAIL]"),
        _ => palette.accent("[INFO]"),
    }
}
