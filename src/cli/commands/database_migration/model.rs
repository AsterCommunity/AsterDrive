//! Cross-database plans, checkpoints and execution reports.
use super::{args::DatabaseMigrateArgs, progress::ProgressReporter};
use aster_forge_utils::numbers::i64_to_usize;
use chrono::{DateTime, FixedOffset};
use sea_orm::DatabaseConnection;
use serde::Serialize;
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum MigrationMode {
    Apply,
    DryRun,
    VerifyOnly,
}

impl MigrationMode {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Apply => "apply",
            Self::DryRun => "dry_run",
            Self::VerifyOnly => "verify_only",
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct DatabaseEndpointReport {
    pub(super) database_url: String,
    pub(super) backend: String,
    pub(super) pending_migrations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct StageReport {
    pub(super) name: &'static str,
    pub(super) status: &'static str,
    pub(super) message: String,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct TableReport {
    pub(super) name: String,
    pub(super) primary_key: Vec<String>,
    pub(super) source_rows: i64,
    pub(super) target_rows: i64,
    pub(super) copied_rows: i64,
    pub(super) sequence_reset: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct CountMismatch {
    pub(super) table: String,
    pub(super) source_rows: i64,
    pub(super) target_rows: i64,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct ConstraintCheck {
    pub(super) table: String,
    pub(super) constraint: String,
    pub(super) columns: Vec<String>,
    pub(super) violations: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(super) struct VerificationReport {
    pub(super) checked: bool,
    pub(super) checked_unique_constraints: usize,
    pub(super) checked_foreign_keys: usize,
    pub(super) count_mismatches: Vec<CountMismatch>,
    pub(super) unique_conflicts: Vec<ConstraintCheck>,
    pub(super) foreign_key_violations: Vec<ConstraintCheck>,
}

#[derive(Debug, Serialize)]
pub(super) struct TotalsReport {
    pub(super) tables: usize,
    pub(super) source_rows: i64,
    pub(super) target_rows: i64,
    pub(super) copied_rows: i64,
    pub(super) duration_ms: u128,
}

#[derive(Debug, Serialize)]
pub(super) struct ResumeReport {
    pub(super) enabled: bool,
    pub(super) resumed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) migration_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) stage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) current_table: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) current_table_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) current_table_offset: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) copied_rows: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) total_rows: Option<i64>,
}

impl ResumeReport {
    pub(super) fn disabled() -> Self {
        Self {
            enabled: false,
            resumed: false,
            migration_key: None,
            status: None,
            stage: None,
            current_table: None,
            current_table_index: None,
            current_table_offset: None,
            copied_rows: None,
            total_rows: None,
        }
    }

    pub(super) fn from_checkpoint(checkpoint: &MigrationCheckpoint, resumed: bool) -> Self {
        Self {
            enabled: true,
            resumed,
            migration_key: Some(checkpoint.migration_key.clone()),
            status: Some(checkpoint.status.clone()),
            stage: Some(checkpoint.stage.clone()),
            current_table: checkpoint.current_table.clone(),
            current_table_index: i64_to_usize(
                checkpoint.current_table_index.max(0),
                "migration checkpoint current_table_index",
            )
            .ok(),
            current_table_offset: Some(checkpoint.current_table_offset),
            copied_rows: Some(checkpoint.copied_rows),
            total_rows: Some(checkpoint.total_rows),
        }
    }
}

#[derive(Debug, Serialize)]
pub(in crate::cli) struct DatabaseMigrationReport {
    pub(super) mode: MigrationMode,
    pub(super) ready_to_cutover: bool,
    pub(super) rolled_back: bool,
    pub(super) source: DatabaseEndpointReport,
    pub(super) target: DatabaseEndpointReport,
    pub(super) stages: Vec<StageReport>,
    pub(super) tables: Vec<TableReport>,
    pub(super) verification: VerificationReport,
    pub(super) totals: TotalsReport,
    pub(super) resume: ResumeReport,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct ColumnSchema {
    pub(super) name: String,
    pub(super) raw_type: String,
    pub(super) pk_order: i32,
    pub(super) binding_kind: BindingKind,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct TablePlan {
    pub(super) name: String,
    pub(super) columns: Vec<ColumnSchema>,
    pub(super) primary_key: Vec<String>,
    pub(super) source_rows: i64,
    pub(super) sequence_reset: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub(super) enum BindingKind {
    Bool,
    Int32,
    Int64,
    Float64,
    Json,
    String,
    Bytes,
    TimestampWithTimeZone,
}

#[derive(Debug)]
pub(super) enum CellValue {
    Null,
    Bool(bool),
    Int64(i64),
    Float64(f64),
    Json(serde_json::Value),
    String(String),
    Bytes(Vec<u8>),
    Timestamp(DateTime<FixedOffset>),
}

#[derive(Debug, Clone)]
pub(super) struct MigrationCheckpoint {
    pub(super) migration_key: String,
    pub(super) source_database_url: String,
    pub(super) target_database_url: String,
    pub(super) mode: String,
    pub(super) status: String,
    pub(super) stage: String,
    pub(super) current_table: Option<String>,
    pub(super) current_table_index: i64,
    pub(super) current_table_offset: i64,
    pub(super) copied_rows: i64,
    pub(super) total_rows: i64,
    pub(super) plan_json: String,
    pub(super) result_json: Option<String>,
    pub(super) last_error: Option<String>,
    pub(super) heartbeat_at_ms: i64,
    pub(super) updated_at_ms: i64,
}

#[derive(Debug)]
pub(super) struct ApplyExecution {
    pub(super) target_pending_after: Vec<String>,
    pub(super) verification: VerificationReport,
    pub(super) ready_to_cutover: bool,
    pub(super) stages: Vec<StageReport>,
    pub(super) checkpoint: MigrationCheckpoint,
    pub(super) resumed: bool,
}

pub(super) struct ApplyModeContext<'a> {
    pub(super) args: &'a DatabaseMigrateArgs,
    pub(super) source_db: &'a DatabaseConnection,
    pub(super) target_db: &'a DatabaseConnection,
    pub(super) source_plans: &'a [TablePlan],
    pub(super) table_reports: &'a mut [TableReport],
    pub(super) target_pending_before: &'a [String],
    pub(super) progress: &'a ProgressReporter,
}
