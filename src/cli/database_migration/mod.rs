//! `aster_drive database-migrate` 的聚合入口。
//!
//! 这里负责协调跨数据库迁移的预检、结构准备、断点续传、数据复制、
//! 校验和最终报告渲染；具体实现拆分在 `apply`、`schema`、`verify`
//! 等子模块。

mod apply;
mod checkpoint;
mod helpers;
mod schema;
mod ui;
mod verify;

use std::time::Instant;

use chrono::{DateTime, FixedOffset};
use clap::Args;
use sea_orm::DatabaseConnection;
use serde::Serialize;

use crate::errors::{AsterError, Result};
use crate::utils::numbers::{i64_to_usize, usize_to_i64};

use self::ui::ProgressReporter;
use crate::cli::db_shared::{backend_name, join_strings, pending_migrations, redact_database_url};
use apply::execute_apply_mode;
use checkpoint::update_checkpoint;
use helpers::now_ms;
use schema::{
    connect_database, load_source_plans, plans_to_reports, refresh_target_rows, total_source_rows,
    validate_backends,
};
use verify::{verification_message, verification_ready, verify_target};

pub use self::ui::{
    DatabaseMigrateOutputFormat, render_database_migration_error, render_database_migration_success,
};

const COPY_TABLE_ORDER: &[&str] = &[
    "managed_followers",
    "storage_policies",
    "storage_policy_groups",
    "storage_policy_group_items",
    "follower_enrollment_sessions",
    "users",
    "user_profiles",
    "user_invitations",
    "auth_sessions",
    "passkeys",
    "mfa_factors",
    "mfa_recovery_codes",
    "mfa_login_flows",
    "mfa_email_codes",
    "mfa_totp_setup_flows",
    "teams",
    "team_members",
    "tags",
    "folders",
    "webdav_accounts",
    "file_blobs",
    "blob_media_metadata",
    "files",
    "file_versions",
    "shares",
    "upload_sessions",
    "upload_session_parts",
    "contact_verification_tokens",
    "external_auth_providers",
    "external_auth_identities",
    "external_auth_login_flows",
    "external_auth_email_verification_flows",
    "master_bindings",
    "managed_ingress_profiles",
    "system_config",
    "audit_logs",
    "mail_outbox",
    "background_tasks",
    "storage_migration_checkpoints",
    "entity_properties",
    "resource_locks",
    "wopi_sessions",
];

const MIGRATION_TABLE: &str = "seaql_migrations";
const CHECKPOINT_TABLE: &str = "aster_cli_database_migrations";
const DEFAULT_COPY_BATCH_SIZE: i64 = 200;
const PROGRESS_ENV: &str = "ASTER_CLI_PROGRESS";
const COPY_BATCH_SIZE_ENV: &str = "ASTER_CLI_COPY_BATCH_SIZE";
const FAIL_AFTER_BATCHES_ENV: &str = "ASTER_CLI_FAIL_AFTER_BATCHES";

#[derive(Debug, Clone, Args)]
pub struct DatabaseMigrateArgs {
    #[arg(long, env = "ASTER_CLI_SOURCE_DATABASE_URL")]
    pub source_database_url: String,
    #[arg(long, env = "ASTER_CLI_TARGET_DATABASE_URL")]
    pub target_database_url: String,
    #[arg(long, env = "ASTER_CLI_DRY_RUN", default_value_t = false)]
    pub dry_run: bool,
    #[arg(long, env = "ASTER_CLI_VERIFY_ONLY", default_value_t = false)]
    pub verify_only: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum MigrationMode {
    Apply,
    DryRun,
    VerifyOnly,
}

impl MigrationMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Apply => "apply",
            Self::DryRun => "dry_run",
            Self::VerifyOnly => "verify_only",
        }
    }
}

#[derive(Debug, Serialize)]
struct DatabaseEndpointReport {
    database_url: String,
    backend: String,
    pending_migrations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct StageReport {
    name: &'static str,
    status: &'static str,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
struct TableReport {
    name: String,
    primary_key: Vec<String>,
    source_rows: i64,
    target_rows: i64,
    copied_rows: i64,
    sequence_reset: bool,
}

#[derive(Debug, Clone, Serialize)]
struct CountMismatch {
    table: String,
    source_rows: i64,
    target_rows: i64,
}

#[derive(Debug, Clone, Serialize)]
struct ConstraintCheck {
    table: String,
    constraint: String,
    columns: Vec<String>,
    violations: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
struct VerificationReport {
    checked: bool,
    checked_unique_constraints: usize,
    checked_foreign_keys: usize,
    count_mismatches: Vec<CountMismatch>,
    unique_conflicts: Vec<ConstraintCheck>,
    foreign_key_violations: Vec<ConstraintCheck>,
}

#[derive(Debug, Serialize)]
struct TotalsReport {
    tables: usize,
    source_rows: i64,
    target_rows: i64,
    copied_rows: i64,
    duration_ms: u128,
}

#[derive(Debug, Serialize)]
struct ResumeReport {
    enabled: bool,
    resumed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    migration_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_table: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_table_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_table_offset: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    copied_rows: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total_rows: Option<i64>,
}

impl ResumeReport {
    fn disabled() -> Self {
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

    fn from_checkpoint(checkpoint: &MigrationCheckpoint, resumed: bool) -> Self {
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
pub struct DatabaseMigrationReport {
    mode: MigrationMode,
    ready_to_cutover: bool,
    rolled_back: bool,
    source: DatabaseEndpointReport,
    target: DatabaseEndpointReport,
    stages: Vec<StageReport>,
    tables: Vec<TableReport>,
    verification: VerificationReport,
    totals: TotalsReport,
    resume: ResumeReport,
}

#[derive(Debug, Clone, Serialize)]
struct ColumnSchema {
    name: String,
    raw_type: String,
    pk_order: i32,
    binding_kind: BindingKind,
}

#[derive(Debug, Clone, Serialize)]
struct TablePlan {
    name: String,
    columns: Vec<ColumnSchema>,
    primary_key: Vec<String>,
    source_rows: i64,
    sequence_reset: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
enum BindingKind {
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
enum CellValue {
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
struct MigrationCheckpoint {
    migration_key: String,
    source_database_url: String,
    target_database_url: String,
    mode: String,
    status: String,
    stage: String,
    current_table: Option<String>,
    current_table_index: i64,
    current_table_offset: i64,
    copied_rows: i64,
    total_rows: i64,
    plan_json: String,
    result_json: Option<String>,
    last_error: Option<String>,
    heartbeat_at_ms: i64,
    updated_at_ms: i64,
}

#[derive(Debug)]
struct ApplyExecution {
    target_pending_after: Vec<String>,
    verification: VerificationReport,
    ready_to_cutover: bool,
    stages: Vec<StageReport>,
    checkpoint: MigrationCheckpoint,
    resumed: bool,
}

struct ApplyModeContext<'a> {
    args: &'a DatabaseMigrateArgs,
    source_db: &'a DatabaseConnection,
    target_db: &'a DatabaseConnection,
    source_plans: &'a [TablePlan],
    table_reports: &'a mut [TableReport],
    target_pending_before: &'a [String],
    progress: &'a ProgressReporter,
}

/// Executes an end-to-end cross-database migration or verification flow.
pub async fn execute_database_migration(
    args: &DatabaseMigrateArgs,
) -> Result<DatabaseMigrationReport> {
    if args.dry_run && args.verify_only {
        return Err(AsterError::validation_error(
            "dry-run and verify-only cannot be enabled at the same time",
        ));
    }

    if args.source_database_url == args.target_database_url {
        return Err(AsterError::validation_error(
            "source and target database URLs must not be identical",
        ));
    }

    let mode = if args.dry_run {
        MigrationMode::DryRun
    } else if args.verify_only {
        MigrationMode::VerifyOnly
    } else {
        MigrationMode::Apply
    };

    let started_at = Instant::now();
    let progress = ProgressReporter::new();
    let source_db = connect_database(&args.source_database_url).await?;
    let target_db = connect_database(&args.target_database_url).await?;
    let source_backend = source_db.get_database_backend();
    let target_backend = target_db.get_database_backend();
    validate_backends(source_backend, target_backend)?;

    let source_pending = pending_migrations(&source_db).await?;
    if !source_pending.is_empty() {
        return Err(AsterError::validation_error(format!(
            "source database has pending migrations: {}",
            join_strings(&source_pending)
        )));
    }

    let source_plans = load_source_plans(&source_db).await?;
    let source_rows_total = total_source_rows(&source_plans);
    let target_pending_before = pending_migrations(&target_db).await?;

    progress.stage(
        "preflight",
        format!(
            "source={} target={} pending_source=0 pending_target={}",
            backend_name(source_backend),
            backend_name(target_backend),
            target_pending_before.len()
        ),
    );

    let mut stages = vec![StageReport {
        name: "preflight",
        status: "ok",
        message: format!(
            "source={} target={} pending_source=0 pending_target={}",
            backend_name(source_backend),
            backend_name(target_backend),
            target_pending_before.len()
        ),
    }];
    let mut table_reports = plans_to_reports(&source_plans);
    let mut verification = VerificationReport::default();
    let mut ready_to_cutover = false;
    let mut target_pending_after = target_pending_before.clone();
    let mut resume = ResumeReport::disabled();

    match mode {
        MigrationMode::DryRun => {
            progress.stage(
                "structure_prepare",
                if target_pending_before.is_empty() {
                    "target schema already matches current migrations".to_string()
                } else {
                    format!(
                        "would apply {} pending migrations",
                        target_pending_before.len()
                    )
                },
            );
            stages.push(StageReport {
                name: "structure_prepare",
                status: "planned",
                message: if target_pending_before.is_empty() {
                    "target schema already matches current migrations".to_string()
                } else {
                    format!(
                        "would apply {} pending migrations: {}",
                        target_pending_before.len(),
                        join_strings(&target_pending_before)
                    )
                },
            });
            stages.push(StageReport {
                name: "data_copy",
                status: "planned",
                message: format!(
                    "would copy {} tables and {} rows",
                    source_plans.len(),
                    source_rows_total
                ),
            });
            stages.push(StageReport {
                name: "verification",
                status: "skipped",
                message: "dry-run does not mutate target or execute post-copy verification"
                    .to_string(),
            });
        }
        MigrationMode::VerifyOnly => {
            if !target_pending_before.is_empty() {
                return Err(AsterError::validation_error(format!(
                    "verify-only requires target schema to be current, pending migrations: {}",
                    join_strings(&target_pending_before)
                )));
            }

            progress.stage(
                "structure_prepare",
                "target schema is current; verify-only skips migrations",
            );
            stages.push(StageReport {
                name: "structure_prepare",
                status: "ok",
                message: "target schema is current; verify-only skips migrations".to_string(),
            });
            stages.push(StageReport {
                name: "data_copy",
                status: "skipped",
                message: "verify-only skips data copy".to_string(),
            });

            verification = verify_target(&target_db, &source_plans).await?;
            ready_to_cutover = verification_ready(&verification);
            refresh_target_rows(&target_db, &mut table_reports).await?;
            progress.stage(
                "verification",
                verification_message(&verification, ready_to_cutover),
            );
            stages.push(StageReport {
                name: "verification",
                status: if ready_to_cutover { "ok" } else { "attention" },
                message: verification_message(&verification, ready_to_cutover),
            });
        }
        MigrationMode::Apply => {
            let apply = execute_apply_mode(ApplyModeContext {
                args,
                source_db: &source_db,
                target_db: &target_db,
                source_plans: &source_plans,
                table_reports: &mut table_reports,
                target_pending_before: &target_pending_before,
                progress: &progress,
            })
            .await?;
            target_pending_after = apply.target_pending_after;
            verification = apply.verification;
            ready_to_cutover = apply.ready_to_cutover;
            stages.extend(apply.stages);

            let provisional_report = DatabaseMigrationReport {
                mode,
                ready_to_cutover,
                rolled_back: false,
                source: DatabaseEndpointReport {
                    database_url: redact_database_url(&args.source_database_url),
                    backend: backend_name(source_backend).to_string(),
                    pending_migrations: source_pending.clone(),
                },
                target: DatabaseEndpointReport {
                    database_url: redact_database_url(&args.target_database_url),
                    backend: backend_name(target_backend).to_string(),
                    pending_migrations: target_pending_after.clone(),
                },
                stages: stages.clone(),
                tables: table_reports.clone(),
                verification: verification.clone(),
                totals: TotalsReport {
                    tables: source_plans.len(),
                    source_rows: source_rows_total,
                    target_rows: table_reports.iter().map(|report| report.target_rows).sum(),
                    copied_rows: table_reports.iter().map(|report| report.copied_rows).sum(),
                    duration_ms: started_at.elapsed().as_millis(),
                },
                resume: ResumeReport::from_checkpoint(&apply.checkpoint, apply.resumed),
            };

            let result_json = serde_json::to_string(&provisional_report).map_err(|error| {
                AsterError::internal_error(format!(
                    "failed to serialize database migration report: {error}"
                ))
            })?;
            let mut final_checkpoint = apply.checkpoint;
            final_checkpoint.status = if ready_to_cutover {
                "completed".to_string()
            } else {
                "attention".to_string()
            };
            final_checkpoint.stage = if ready_to_cutover {
                "complete".to_string()
            } else {
                "verification".to_string()
            };
            final_checkpoint.result_json = Some(result_json);
            final_checkpoint.last_error = None;
            final_checkpoint.current_table = None;
            final_checkpoint.current_table_index =
                usize_to_i64(source_plans.len(), "source plan count")?;
            final_checkpoint.current_table_offset = 0;
            final_checkpoint.copied_rows =
                table_reports.iter().map(|report| report.copied_rows).sum();
            final_checkpoint.updated_at_ms = now_ms();
            final_checkpoint.heartbeat_at_ms = final_checkpoint.updated_at_ms;
            update_checkpoint(&target_db, &final_checkpoint).await?;
            resume = ResumeReport::from_checkpoint(&final_checkpoint, apply.resumed);
        }
    }

    let report = DatabaseMigrationReport {
        mode,
        ready_to_cutover,
        rolled_back: false,
        source: DatabaseEndpointReport {
            database_url: redact_database_url(&args.source_database_url),
            backend: backend_name(source_backend).to_string(),
            pending_migrations: source_pending,
        },
        target: DatabaseEndpointReport {
            database_url: redact_database_url(&args.target_database_url),
            backend: backend_name(target_backend).to_string(),
            pending_migrations: target_pending_after,
        },
        stages,
        tables: table_reports.clone(),
        verification,
        totals: TotalsReport {
            tables: source_plans.len(),
            source_rows: source_rows_total,
            target_rows: table_reports.iter().map(|report| report.target_rows).sum(),
            copied_rows: table_reports.iter().map(|report| report.copied_rows).sum(),
            duration_ms: started_at.elapsed().as_millis(),
        },
        resume,
    };

    Ok(report)
}
