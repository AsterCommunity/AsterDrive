//! `database-migrate` 的应用执行流程。
//!
//! 这里串联目标库 schema 准备、检查点初始化、批量复制、序列重置和
//! 复制后的完整性校验。

mod convert;
mod copy;

use migration::Migrator;
use sea_orm::DatabaseConnection;

use crate::errors::{AsterError, MapAsterErr, Result};
use crate::utils::numbers::usize_to_i64;

use self::copy::{copy_tables_with_resume, load_target_type_hints, reset_sequences};
use super::checkpoint::{
    ensure_checkpoint_table, initialize_checkpoint, mark_checkpoint_failed, resume_message,
    update_checkpoint,
};
use super::helpers::now_ms;
use super::schema::{refresh_target_rows, total_source_rows};
use super::verify::{verification_message, verification_ready, verify_target};
use super::{ApplyExecution, ApplyModeContext, MigrationCheckpoint};
use crate::cli::db_shared::{join_strings, pending_migrations};

/// Applies migrations, copies data with resume support, then verifies the target.
pub(super) async fn execute_apply_mode(ctx: ApplyModeContext<'_>) -> Result<ApplyExecution> {
    ctx.progress
        .stage("structure_prepare", "preparing target schema");
    Migrator::up(ctx.target_db, None)
        .await
        .map_aster_err(AsterError::database_operation)?;
    let target_backend = ctx.target_db.get_database_backend();
    let target_pending_after = pending_migrations(ctx.target_db).await?;
    if !target_pending_after.is_empty() {
        return Err(AsterError::database_operation(format!(
            "target database still has pending migrations after prepare: {}",
            join_strings(&target_pending_after)
        )));
    }

    ensure_checkpoint_table(ctx.target_db).await?;
    let mut checkpoint = initialize_checkpoint(ctx.args, ctx.target_db, ctx.source_plans).await?;
    let resumed = checkpoint.resumed;
    ctx.progress.stage(
        "resume",
        if resumed {
            resume_message(&checkpoint.checkpoint)
        } else {
            "starting a new migration checkpoint".to_string()
        },
    );

    let target_type_hints =
        match load_target_type_hints(ctx.target_db, target_backend, ctx.source_plans).await {
            Ok(value) => value,
            Err(error) => {
                mark_checkpoint_failed_best_effort(
                    ctx.target_db,
                    &mut checkpoint.checkpoint,
                    &error,
                )
                .await;
                return Err(error);
            }
        };

    if let Err(error) = copy_tables_with_resume(
        ctx.source_db,
        ctx.target_db,
        ctx.source_plans,
        &target_type_hints,
        &mut checkpoint.checkpoint,
        ctx.progress,
    )
    .await
    {
        mark_checkpoint_failed_best_effort(ctx.target_db, &mut checkpoint.checkpoint, &error).await;
        return Err(error);
    }

    if let Err(error) = reset_sequences(ctx.target_db, ctx.source_plans).await {
        mark_checkpoint_failed_best_effort(ctx.target_db, &mut checkpoint.checkpoint, &error).await;
        return Err(error);
    }

    checkpoint.checkpoint.stage = "verification".to_string();
    checkpoint.checkpoint.status = "running".to_string();
    checkpoint.checkpoint.current_table = None;
    checkpoint.checkpoint.current_table_index =
        usize_to_i64(ctx.source_plans.len(), "source plan count")?;
    checkpoint.checkpoint.current_table_offset = 0;
    checkpoint.checkpoint.updated_at_ms = now_ms();
    checkpoint.checkpoint.heartbeat_at_ms = checkpoint.checkpoint.updated_at_ms;
    if let Err(error) = update_checkpoint(ctx.target_db, &checkpoint.checkpoint).await {
        mark_checkpoint_failed_best_effort(ctx.target_db, &mut checkpoint.checkpoint, &error).await;
        return Err(error);
    }

    ctx.progress
        .stage("verification", "running post-copy verification");
    let verification = match verify_target(ctx.target_db, ctx.source_plans).await {
        Ok(value) => value,
        Err(error) => {
            mark_checkpoint_failed_best_effort(ctx.target_db, &mut checkpoint.checkpoint, &error)
                .await;
            return Err(error);
        }
    };
    let ready_to_cutover = verification_ready(&verification);

    if let Err(error) = refresh_target_rows(ctx.target_db, ctx.table_reports).await {
        mark_checkpoint_failed_best_effort(ctx.target_db, &mut checkpoint.checkpoint, &error).await;
        return Err(error);
    }
    for report in &mut *ctx.table_reports {
        report.copied_rows = report.target_rows;
    }

    checkpoint.checkpoint.status = if ready_to_cutover {
        "completed".to_string()
    } else {
        "attention".to_string()
    };
    checkpoint.checkpoint.stage = if ready_to_cutover {
        "complete".to_string()
    } else {
        "verification".to_string()
    };
    checkpoint.checkpoint.current_table = None;
    checkpoint.checkpoint.current_table_index =
        usize_to_i64(ctx.source_plans.len(), "source plan count")?;
    checkpoint.checkpoint.current_table_offset = 0;
    checkpoint.checkpoint.copied_rows = total_source_rows(ctx.source_plans);
    checkpoint.checkpoint.last_error = None;
    checkpoint.checkpoint.updated_at_ms = now_ms();
    checkpoint.checkpoint.heartbeat_at_ms = checkpoint.checkpoint.updated_at_ms;

    let stages = vec![
        super::StageReport {
            name: "structure_prepare",
            status: "ok",
            message: if ctx.target_pending_before.is_empty() {
                "target schema already matched current migrations".to_string()
            } else {
                format!(
                    "applied {} pending migrations",
                    ctx.target_pending_before.len()
                )
            },
        },
        super::StageReport {
            name: "data_copy",
            status: "ok",
            message: if resumed {
                format!(
                    "copied {} tables and {} rows (resumed from checkpoint)",
                    ctx.source_plans.len(),
                    total_source_rows(ctx.source_plans)
                )
            } else {
                format!(
                    "copied {} tables and {} rows",
                    ctx.source_plans.len(),
                    total_source_rows(ctx.source_plans)
                )
            },
        },
        super::StageReport {
            name: "verification",
            status: if ready_to_cutover { "ok" } else { "attention" },
            message: verification_message(&verification, ready_to_cutover),
        },
    ];
    ctx.progress.stage(
        "verification",
        verification_message(&verification, ready_to_cutover),
    );

    Ok(ApplyExecution {
        target_pending_after,
        verification,
        ready_to_cutover,
        stages,
        checkpoint: checkpoint.checkpoint,
        resumed,
    })
}

async fn mark_checkpoint_failed_best_effort(
    target_db: &DatabaseConnection,
    checkpoint: &mut MigrationCheckpoint,
    error: &AsterError,
) {
    if let Err(checkpoint_error) = mark_checkpoint_failed(target_db, checkpoint, error).await {
        tracing::warn!(
            migration_key = checkpoint.migration_key,
            original_error = %error,
            checkpoint_error = %checkpoint_error,
            "failed to persist migration checkpoint failure state"
        );
    }
}
