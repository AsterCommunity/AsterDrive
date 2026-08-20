use std::collections::{HashMap, HashSet};
use std::pin::Pin;

use bytes::Bytes;
use chrono::SecondsFormat;
use futures::Stream;
use serde::Serialize;

use crate::api::pagination::AdminAuditLogSortBy;
use crate::db::repository::{audit_log_repo, user_repo};
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use aster_drive_model::entities::audit_log;
use aster_drive_model::types::TeamMemberRole;
use aster_forge_api::SortOrder;

use super::AuditLogFilters;

pub const AUDIT_CSV_EXPORT_MAX_ROWS: u64 = 100_000;
pub const AUDIT_CSV_EXPORT_BATCH_SIZE: u64 = 500;

const CSV_HEADERS: [&str; 16] = [
    "id",
    "created_at",
    "actor_user_id",
    "actor_username",
    "action",
    "entity_type",
    "entity_id",
    "entity_name",
    "detail",
    "ip_address",
    "user_agent",
    "member_user_id",
    "member_username",
    "role",
    "previous_role",
    "next_role",
];

pub type AuditCsvStream = Pin<Box<dyn Stream<Item = Result<Bytes>> + Send + 'static>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditExportKind {
    System,
    Team { team_id: i64 },
}

impl AuditExportKind {
    fn log_name(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Team { .. } => "team",
        }
    }
}

pub struct PreparedAuditCsvExport {
    pub kind: AuditExportKind,
    pub total: u64,
    pub stream: AuditCsvStream,
}

#[derive(Serialize)]
struct AuditCsvRow {
    id: i64,
    created_at: String,
    actor_user_id: i64,
    actor_username: Option<String>,
    action: String,
    entity_type: String,
    entity_id: Option<i64>,
    entity_name: Option<String>,
    detail: Option<String>,
    ip_address: Option<String>,
    user_agent: Option<String>,
    member_user_id: Option<i64>,
    member_username: Option<String>,
    role: Option<&'static str>,
    previous_role: Option<&'static str>,
    next_role: Option<&'static str>,
}

struct ExportProgress {
    kind: AuditExportKind,
    expected: u64,
    sent: u64,
    completed: bool,
    failed: bool,
}

impl Drop for ExportProgress {
    fn drop(&mut self) {
        if !self.completed && !self.failed {
            tracing::warn!(
                export_kind = self.kind.log_name(),
                team_id = match self.kind {
                    AuditExportKind::Team { team_id } => Some(team_id),
                    AuditExportKind::System => None,
                },
                expected_rows = self.expected,
                streamed_rows = self.sent,
                "audit CSV export stream cancelled before completion"
            );
        }
    }
}

fn export_query(
    filters: AuditLogFilters,
    sort_by: AdminAuditLogSortBy,
    sort_order: SortOrder,
) -> audit_log_repo::AuditLogExportQuery {
    audit_log_repo::AuditLogExportQuery {
        user_id: filters.user_id,
        action: filters.action,
        entity_type: filters.entity_type.map(|value| value.as_str().to_string()),
        entity_id: filters.entity_id,
        after: filters.after,
        before: filters.before,
        sort_by,
        sort_order,
    }
}

fn csv_header() -> Result<Bytes> {
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(Vec::new());
    writer
        .write_record(CSV_HEADERS)
        .map_err(|error| AsterError::internal_error(format!("write audit CSV header: {error}")))?;
    writer
        .into_inner()
        .map(Bytes::from)
        .map_err(|error| AsterError::internal_error(format!("finalize audit CSV header: {error}")))
}

fn csv_chunk(rows: Vec<AuditCsvRow>) -> Result<Bytes> {
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(Vec::new());
    for row in rows {
        writer.serialize(row).map_err(|error| {
            AsterError::internal_error(format!("serialize audit CSV row: {error}"))
        })?;
    }
    writer
        .into_inner()
        .map(Bytes::from)
        .map_err(|error| AsterError::internal_error(format!("finalize audit CSV chunk: {error}")))
}

fn is_sensitive_detail_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    if matches!(
        normalized.as_str(),
        "haspassword" | "mustchangepassword" | "temporarypasswordgenerated"
    ) {
        return false;
    }

    [
        "password",
        "passwd",
        "token",
        "secret",
        "credential",
        "authorization",
        "cookie",
        "recoverycode",
        "privatekey",
        "accesskey",
        "apikey",
        "session",
        "mfa",
        "otp",
        "totp",
        "bearer",
        "appkey",
        "wopikey",
        "sharetoken",
        "storagecredential",
    ]
    .iter()
    .any(|sensitive| normalized.contains(sensitive))
}

fn neutralize_csv_formula(value: String) -> String {
    if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value
    }
}

fn redact_sensitive_details(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            object.retain(|key, _| !is_sensitive_detail_key(key));
            for nested in object.values_mut() {
                redact_sensitive_details(nested);
            }
        }
        serde_json::Value::Array(values) => {
            for nested in values {
                redact_sensitive_details(nested);
            }
        }
        _ => {}
    }
}

fn parse_raw_details(raw: Option<&str>, audit_log_id: i64) -> Option<serde_json::Value> {
    let raw = raw?;
    match serde_json::from_str(raw) {
        Ok(mut details) => {
            redact_sensitive_details(&mut details);
            Some(details)
        }
        Err(error) => {
            tracing::warn!(
                audit_log_id,
                %error,
                "omitting invalid audit details from CSV export"
            );
            None
        }
    }
}

fn parse_details(model: &audit_log::Model) -> Option<serde_json::Value> {
    parse_raw_details(model.details.as_deref(), model.id)
}

pub fn sanitize_details(raw: Option<&str>) -> Option<String> {
    parse_raw_details(raw, 0).map(|details| details.to_string())
}

pub fn sanitize_entity_name(entity_type: &str, value: Option<String>) -> Option<String> {
    (entity_type != "share")
        .then(|| value.map(neutralize_csv_formula))
        .flatten()
}

fn detail_i64(details: Option<&serde_json::Value>, key: &str) -> Option<i64> {
    details?.get(key)?.as_i64()
}

fn detail_role(details: Option<&serde_json::Value>, key: &str) -> Option<TeamMemberRole> {
    serde_json::from_value(details?.get(key)?.clone()).ok()
}

fn role_name(role: Option<TeamMemberRole>) -> Option<&'static str> {
    match role {
        Some(TeamMemberRole::Owner) => Some("owner"),
        Some(TeamMemberRole::Admin) => Some("admin"),
        Some(TeamMemberRole::Member) => Some("member"),
        None => None,
    }
}

fn parsed_details_for_batch(models: &[audit_log::Model]) -> Vec<Option<serde_json::Value>> {
    models.iter().map(parse_details).collect()
}

fn ensure_export_size(total: u64) -> Result<()> {
    if total > AUDIT_CSV_EXPORT_MAX_ROWS {
        return Err(AsterError::operation_resource_limit_exceeded(format!(
            "audit CSV export matched {total} rows; the maximum is {AUDIT_CSV_EXPORT_MAX_ROWS}"
        )));
    }
    Ok(())
}

fn member_user_id(details: Option<&serde_json::Value>) -> Option<i64> {
    detail_i64(details, "member_user_id")
}

fn rows_for_batch(
    models: &[audit_log::Model],
    parsed_details: &[Option<serde_json::Value>],
    usernames: &HashMap<i64, String>,
) -> Vec<AuditCsvRow> {
    models
        .iter()
        .zip(parsed_details)
        .map(|(model, details)| {
            let member_user_id = member_user_id(details.as_ref());
            let role = detail_role(details.as_ref(), "role")
                .or_else(|| detail_role(details.as_ref(), "removed_role"));
            AuditCsvRow {
                id: model.id,
                created_at: model
                    .created_at
                    .to_rfc3339_opts(SecondsFormat::Millis, true),
                actor_user_id: model.user_id,
                actor_username: usernames
                    .get(&model.user_id)
                    .cloned()
                    .map(neutralize_csv_formula),
                action: model.action.as_str().to_string(),
                entity_type: model.entity_type.clone(),
                entity_id: model.entity_id,
                entity_name: sanitize_entity_name(&model.entity_type, model.entity_name.clone()),
                detail: details
                    .as_ref()
                    .map(serde_json::Value::to_string)
                    .map(neutralize_csv_formula),
                ip_address: model.ip_address.clone(),
                user_agent: model.user_agent.clone().map(neutralize_csv_formula),
                member_user_id,
                member_username: member_user_id
                    .and_then(|id| usernames.get(&id).cloned())
                    .map(neutralize_csv_formula),
                role: role_name(role),
                previous_role: role_name(detail_role(details.as_ref(), "previous_role")),
                next_role: role_name(detail_role(details.as_ref(), "next_role")),
            }
        })
        .collect()
}

async fn usernames_for_batch(
    state: &PrimaryAppState,
    models: &[audit_log::Model],
    parsed_details: &[Option<serde_json::Value>],
) -> Result<HashMap<i64, String>> {
    let mut ids = HashSet::new();
    for (model, details) in models.iter().zip(parsed_details) {
        ids.insert(model.user_id);
        if let Some(id) = member_user_id(details.as_ref()) {
            ids.insert(id);
        }
    }
    let ids = ids.into_iter().filter(|id| *id > 0).collect::<Vec<_>>();
    Ok(user_repo::find_by_ids(state.writer_db(), &ids)
        .await?
        .into_iter()
        .map(|user| (user.id, user.username))
        .collect())
}

pub async fn prepare_csv_export(
    state: PrimaryAppState,
    kind: AuditExportKind,
    filters: AuditLogFilters,
    sort_by: AdminAuditLogSortBy,
    sort_order: SortOrder,
) -> Result<PreparedAuditCsvExport> {
    aster_forge_audit::flush_global_audit_log_manager().await;
    let query = export_query(filters, sort_by, sort_order);
    let snapshot = audit_log_repo::export_snapshot(state.writer_db(), &query).await?;
    let total = snapshot.map_or(0, |snapshot| snapshot.total);
    if let Err(error) = ensure_export_size(total) {
        tracing::warn!(
            export_kind = kind.log_name(),
            total,
            limit = AUDIT_CSV_EXPORT_MAX_ROWS,
            "audit CSV export rejected because the row limit was exceeded"
        );
        return Err(error);
    }

    let stream = Box::pin(async_stream::try_stream! {
        let mut progress = ExportProgress {
            kind,
            expected: total,
            sent: 0,
            completed: false,
            failed: false,
        };
        let header = csv_header().inspect_err(|_| {
            progress.failed = true;
        })?;
        yield header;

        if let Some(snapshot) = snapshot {
            let mut cursor = None;
            loop {
                let models = audit_log_repo::find_export_page(
                    state.writer_db(),
                    &query,
                    snapshot,
                    cursor.as_ref(),
                    AUDIT_CSV_EXPORT_BATCH_SIZE,
                )
                .await
                .inspect_err(|error| {
                    progress.failed = true;
                    tracing::error!(
                        export_kind = kind.log_name(),
                        streamed_rows = progress.sent,
                        %error,
                        "audit CSV export database stream failed"
                    );
                })?;
                if models.is_empty() {
                    break;
                }

                let parsed_details = parsed_details_for_batch(&models);
                let usernames = usernames_for_batch(&state, &models, &parsed_details)
                    .await
                    .inspect_err(|error| {
                        progress.failed = true;
                        tracing::error!(
                            export_kind = kind.log_name(),
                            streamed_rows = progress.sent,
                            %error,
                            "audit CSV export user lookup failed"
                        );
                    })?;
                let batch_len = u64::try_from(models.len()).map_err(|error| {
                    progress.failed = true;
                    AsterError::internal_error(format!("audit CSV batch length overflow: {error}"))
                })?;
                cursor = models.last().cloned();
                let chunk = csv_chunk(rows_for_batch(&models, &parsed_details, &usernames))
                    .inspect_err(|_| {
                        progress.failed = true;
                    })?;
                yield chunk;
                progress.sent += batch_len;

                if batch_len < AUDIT_CSV_EXPORT_BATCH_SIZE {
                    break;
                }
            }
        }

        progress.completed = true;
        if progress.sent == progress.expected {
            tracing::info!(
                export_kind = kind.log_name(),
                rows = progress.sent,
                "audit CSV export stream completed"
            );
        } else {
            tracing::warn!(
                export_kind = kind.log_name(),
                expected_rows = progress.expected,
                streamed_rows = progress.sent,
                "audit CSV export completed after the snapshot changed"
            );
        }
    });

    if total == 0 {
        tracing::info!(
            export_kind = kind.log_name(),
            "prepared empty audit CSV export"
        );
    }
    Ok(PreparedAuditCsvExport {
        kind,
        total,
        stream,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_detail_keys_are_removed_recursively_without_dropping_safe_flags() {
        let mut value = serde_json::json!({
            "password": "plain",
            "has_password": true,
            "nested": {
                "access_token": "token-value",
                "client_secret": "secret-value",
                "safe": "kept"
            },
            "items": [{"authorization": "Bearer value", "count": 2}],
            "temporary_password_generated": true
        });

        redact_sensitive_details(&mut value);

        assert_eq!(value["has_password"], true);
        assert_eq!(value["temporary_password_generated"], true);
        assert_eq!(value["nested"]["safe"], "kept");
        assert_eq!(value["items"][0]["count"], 2);
        let encoded = value.to_string();
        for secret in ["plain", "token-value", "secret-value", "Bearer value"] {
            assert!(!encoded.contains(secret));
        }
    }

    #[test]
    fn csv_writer_preserves_fixed_columns_and_rfc4180_escaping() {
        let row = AuditCsvRow {
            id: 1,
            created_at: "2026-08-20T12:00:00.000Z".to_string(),
            actor_user_id: 7,
            actor_username: Some("猫,\"admin\"\nname".to_string()),
            action: "team_update".to_string(),
            entity_type: "team".to_string(),
            entity_id: Some(9),
            entity_name: Some("A, B".to_string()),
            detail: Some("{\"note\":\"line 1\\nline 2\"}".to_string()),
            ip_address: None,
            user_agent: Some("agent\r\nnext".to_string()),
            member_user_id: None,
            member_username: None,
            role: None,
            previous_role: None,
            next_role: None,
        };
        let mut bytes = csv_header().unwrap().to_vec();
        bytes.extend(csv_chunk(vec![row]).unwrap());
        assert!(!bytes.starts_with(&[0xef, 0xbb, 0xbf]));

        let mut reader = csv::Reader::from_reader(bytes.as_slice());
        assert_eq!(reader.headers().unwrap().len(), CSV_HEADERS.len());
        let records = reader
            .records()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].len(), CSV_HEADERS.len());
        assert_eq!(records[0].get(3), Some("猫,\"admin\"\nname"));
        assert_eq!(records[0].get(9), Some(""));
        assert_eq!(records[0].get(10), Some("agent\r\nnext"));
    }

    #[test]
    fn export_row_limit_is_inclusive_at_the_boundary() {
        assert!(ensure_export_size(AUDIT_CSV_EXPORT_MAX_ROWS).is_ok());
        let error = ensure_export_size(AUDIT_CSV_EXPORT_MAX_ROWS + 1).unwrap_err();
        assert_eq!(
            error.api_error_code().as_str(),
            "operation.resource_limit_exceeded"
        );
        assert!(error.message().contains("100000"));
    }

    #[test]
    fn csv_formula_prefixes_are_neutralized() {
        assert_eq!(
            neutralize_csv_formula("=HYPERLINK(\"x\")".to_string()),
            "'=HYPERLINK(\"x\")"
        );
        assert_eq!(neutralize_csv_formula("ordinary".to_string()), "ordinary");
    }
}
