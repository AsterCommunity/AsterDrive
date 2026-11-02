//! Pure doctor report presentation.
use super::report::{DoctorReport, DoctorStatus};
use crate::cli::common::{
    output::{OutputFormat, ResolvedOutputFormat, render_success_envelope},
    terminal::{CliTerminalPalette, human_key},
};
/// Renders a successful doctor report in the requested output format.
pub(in crate::cli) fn render_doctor_success(format: OutputFormat, report: &DoctorReport) -> String {
    match format.resolve() {
        ResolvedOutputFormat::Json => render_success_envelope(report, false),
        ResolvedOutputFormat::PrettyJson => render_success_envelope(report, true),
        ResolvedOutputFormat::Human => render_doctor_human(report),
    }
}

pub(super) fn render_doctor_human(report: &DoctorReport) -> String {
    let palette = CliTerminalPalette::stdout();
    let mut lines = vec![
        palette.title("System doctor"),
        palette.dim("--------------------------------------------------"),
        format!(
            "{} {}",
            human_key("Database", &palette),
            report.database_url
        ),
        format!(
            "{} {}",
            human_key("Backend", &palette),
            report.backend.as_deref().unwrap_or("unknown")
        ),
        format!(
            "{} {}",
            human_key("Mode", &palette),
            doctor_mode_label(report)
        ),
        format!(
            "{} {}",
            human_key("Scope", &palette),
            if report.deep {
                doctor_scope_label(report)
            } else {
                "default".to_string()
            }
        ),
        format!(
            "{} {} {}",
            human_key("Status", &palette),
            palette.status_badge(report.status.as_str()),
            doctor_status_label(report.status)
        ),
        format!(
            "{} {} total, {} ok, {} warn, {} fail",
            human_key("Checks", &palette),
            report.summary.total,
            report.summary.ok,
            report.summary.warn,
            report.summary.fail
        ),
    ];

    if report.checks.is_empty() {
        lines.push(String::new());
        lines.push(palette.dim("No checks were executed."));
        return lines.join("\n");
    }

    lines.push(String::new());
    lines.push(palette.label("Checks:"));
    for check in &report.checks {
        lines.push(format!(
            "  {} {}",
            palette.status_badge(check.status.as_str()),
            check.label
        ));
        lines.push(format!("    {}", check.summary));
        for detail in &check.details {
            lines.push(format!("    {}", palette.dim(detail)));
        }
        if let Some(suggestion) = &check.suggestion {
            lines.push(format!(
                "    {} {}",
                palette.label("hint:"),
                palette.accent(suggestion)
            ));
        }
    }

    lines.join("\n")
}

pub(super) fn doctor_mode_label(report: &DoctorReport) -> String {
    let mut parts = Vec::new();
    parts.push(if report.strict { "strict" } else { "standard" });
    if report.deep {
        parts.push("deep");
    }
    if report.fix {
        parts.push("fix");
    }
    parts.join(" + ")
}

pub(super) fn doctor_scope_label(report: &DoctorReport) -> String {
    let mut label = if report.scopes.is_empty() {
        "default".to_string()
    } else {
        report
            .scopes
            .iter()
            .map(|scope| scope.label())
            .collect::<Vec<_>>()
            .join(", ")
    };
    if let Some(policy_id) = report.policy_id {
        label.push_str(&format!(" | policy_id={policy_id}"));
    }
    label
}

pub(super) fn doctor_status_label(status: DoctorStatus) -> &'static str {
    match status {
        DoctorStatus::Ok => "ready",
        DoctorStatus::Warn => "attention",
        DoctorStatus::Fail => "failed",
    }
}
