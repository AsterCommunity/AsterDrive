use actix_web::{HttpResponse, http::header};
use chrono::Utc;

use crate::services::ops::audit::{AuditExportKind, PreparedAuditCsvExport};

fn safe_filename_component(value: &str) -> String {
    let normalized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    let normalized = normalized.trim_matches('_');
    if normalized.is_empty() {
        "audit".to_string()
    } else {
        normalized.to_string()
    }
}

pub(crate) fn response(export: PreparedAuditCsvExport) -> HttpResponse {
    let export_name = match export.kind {
        AuditExportKind::System => "system".to_string(),
        AuditExportKind::Team { team_id } => format!("team_{team_id}"),
    };
    let filename = format!(
        "asterdrive_audit_{}_{}.csv",
        safe_filename_component(&export_name),
        Utc::now().format("%Y%m%dT%H%M%SZ")
    );

    HttpResponse::Ok()
        .insert_header((header::CONTENT_TYPE, "text/csv; charset=utf-8"))
        .insert_header((
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        ))
        .insert_header(("X-Audit-Export-Rows", export.total.to_string()))
        .streaming(export.stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_components_strip_header_metacharacters() {
        assert_eq!(safe_filename_component("team_42"), "team_42");
        assert_eq!(safe_filename_component("../../bad\r\nname"), "bad__name");
        assert_eq!(safe_filename_component("***"), "audit");
    }
}
