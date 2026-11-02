//! Offline migration orchestration with read-only preview and resumable apply.
use super::model::VerificationReport;
use super::{
    apply::execute_apply_mode,
    args::DatabaseMigrateArgs,
    checkpoint::update_checkpoint,
    helpers::now_ms,
    model::{
        ApplyModeContext, DatabaseEndpointReport, DatabaseMigrationReport, MigrationMode,
        ResumeReport, StageReport, TotalsReport,
    },
    plan::{load_source_plans, plans_to_reports, total_source_rows},
    progress::ProgressReporter,
    schema::{connect_database, refresh_target_rows, validate_backends},
    verify::{verification_message, verification_ready, verify_target},
};
use crate::cli::common::database::DatabaseAccess;
use crate::cli::common::database::{backend_name, pending_migrations, redact_database_url};
use crate::errors::{AsterError, Result};
use aster_forge_utils::numbers::usize_to_i64;
use std::time::Instant;
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
    let source_db = crate::cli::common::database::connect_existing_database(
        &crate::config::DatabaseConfig {
            url: args.source_database_url.clone().into(),
            pool_size: 1,
            retry_count: 0,
        },
        DatabaseAccess::ReadOnly,
    )
    .await?;
    let target_db = if args.dry_run || args.verify_only {
        crate::cli::common::database::connect_existing_database(
            &crate::config::DatabaseConfig {
                url: args.target_database_url.clone().into(),
                pool_size: 1,
                retry_count: 0,
            },
            DatabaseAccess::ReadOnly,
        )
        .await?
    } else {
        connect_database(&args.target_database_url).await?
    };
    let source_backend = source_db.get_database_backend();
    let target_backend = target_db.get_database_backend();
    validate_backends(source_backend, target_backend)?;

    let source_pending = pending_migrations(&source_db).await?;
    if !source_pending.is_empty() {
        return Err(AsterError::validation_error(format!(
            "source database has pending migrations: {}",
            source_pending.join(", ")
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
                        target_pending_before.join(", ")
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
                    target_pending_before.join(", ")
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
