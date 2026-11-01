//! `database-migrate` 的批量复制与断点续传执行。
//!
//! 这里按表顺序分批读取源库、写入目标库，并在每批提交后推进检查点状态。

use std::collections::BTreeMap;

use sea_orm::sea_query::{Alias, Query};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement, TransactionTrait, Value};

use crate::cli::db_shared::{quote_ident, quote_literal, scalar_i64};
use crate::errors::{AsterError, MapAsterErr, Result};
use aster_forge_utils::numbers::{i64_to_usize, usize_to_i64};

use super::convert::decode_row_values;
use crate::cli::database_migration::checkpoint::update_checkpoint;
use crate::cli::database_migration::helpers::now_ms;
use crate::cli::database_migration::schema::{binding_kind_from_raw_type, load_column_type_rows};
use crate::cli::database_migration::ui::ProgressReporter;
use crate::cli::database_migration::{
    BindingKind, COPY_BATCH_SIZE_ENV, DEFAULT_COPY_BATCH_SIZE, FAIL_AFTER_BATCHES_ENV,
    MigrationCheckpoint, TablePlan,
};

/// Copies all planned tables in batches and persists checkpoint progress after each commit.
pub(super) async fn copy_tables_with_resume(
    source: &DatabaseConnection,
    target: &DatabaseConnection,
    plans: &[TablePlan],
    target_type_hints: &BTreeMap<String, BTreeMap<String, BindingKind>>,
    checkpoint: &mut MigrationCheckpoint,
    progress: &ProgressReporter,
) -> Result<()> {
    let batch_size = parse_positive_i64_env(COPY_BATCH_SIZE_ENV, DEFAULT_COPY_BATCH_SIZE)?;
    let fail_after_batches = fail_after_batches()?;
    let source_backend = source.get_database_backend();
    let start_table_index = i64_to_usize(
        checkpoint.current_table_index.max(0),
        "migration checkpoint current_table_index",
    )?;
    let mut committed_batches = 0_i64;

    for table_index in start_table_index..plans.len() {
        let plan = &plans[table_index];
        let type_hints = target_type_hints.get(&plan.name).ok_or_else(|| {
            AsterError::validation_error(format!(
                "missing target column type hints for table '{}'",
                plan.name
            ))
        })?;
        let mut offset = if table_index == start_table_index {
            checkpoint.current_table_offset
        } else {
            0
        };

        checkpoint.stage = "data_copy".to_string();
        checkpoint.status = "running".to_string();
        checkpoint.current_table = Some(plan.name.clone());
        checkpoint.current_table_index = usize_to_i64(table_index, "migration table index")?;
        checkpoint.updated_at_ms = now_ms();
        checkpoint.heartbeat_at_ms = checkpoint.updated_at_ms;
        update_checkpoint(target, checkpoint).await?;

        while offset < plan.source_rows {
            let rows =
                fetch_source_batch(source, source_backend, plan, type_hints, offset, batch_size)
                    .await?;
            if rows.is_empty() {
                break;
            }

            let txn = target
                .begin()
                .await
                .map_aster_err(AsterError::database_operation)?;
            insert_batch(&txn, plan, &rows).await.map_err(|error| {
                AsterError::database_operation(format!(
                    "failed to write target batch for '{}': {error}",
                    plan.name
                ))
            })?;

            let row_count = usize_to_i64(rows.len(), "source batch row count")?;
            offset += row_count;
            checkpoint.current_table = Some(plan.name.clone());
            checkpoint.current_table_index = usize_to_i64(table_index, "migration table index")?;
            checkpoint.current_table_offset = offset;
            checkpoint.copied_rows += row_count;
            checkpoint.updated_at_ms = now_ms();
            checkpoint.heartbeat_at_ms = checkpoint.updated_at_ms;
            update_checkpoint(&txn, checkpoint).await?;
            txn.commit()
                .await
                .map_aster_err(AsterError::database_operation)?;

            progress.batch(
                table_index,
                plans.len(),
                plan,
                offset,
                checkpoint.copied_rows,
                checkpoint.total_rows,
            );

            committed_batches += 1;
            if let Some(limit) = fail_after_batches
                && committed_batches >= limit
            {
                return Err(AsterError::internal_error(
                    "forced failure after committed batch for resume-path verification",
                ));
            }
        }

        checkpoint.current_table = None;
        checkpoint.current_table_index =
            usize_to_i64(table_index.saturating_add(1), "next migration table index")?;
        checkpoint.current_table_offset = 0;
        checkpoint.updated_at_ms = now_ms();
        checkpoint.heartbeat_at_ms = checkpoint.updated_at_ms;
        update_checkpoint(target, checkpoint).await?;
    }

    Ok(())
}

/// Resets target-side auto-increment sequences after table data has been copied.
pub(super) async fn reset_sequences(
    target: &DatabaseConnection,
    plans: &[TablePlan],
) -> Result<()> {
    let backend = target.get_database_backend();
    for plan in plans.iter().filter(|plan| plan.sequence_reset) {
        match backend {
            DbBackend::Postgres => {
                let table_ident = quote_ident(DbBackend::Postgres, &plan.name);
                let sql = format!(
                    "SELECT setval( \
                        pg_get_serial_sequence({}, {}), \
                        COALESCE((SELECT MAX(id) FROM {table_ident}), 1), \
                        EXISTS (SELECT 1 FROM {table_ident}) \
                    )",
                    quote_literal(&plan.name),
                    quote_literal("id"),
                );
                target
                    .execute_raw(Statement::from_string(DbBackend::Postgres, sql))
                    .await
                    .map_aster_err(AsterError::database_operation)?;
            }
            DbBackend::MySql => {
                let next_id = scalar_i64(
                    target,
                    DbBackend::MySql,
                    &format!(
                        "SELECT COALESCE(MAX(id), 0) + 1 FROM {}",
                        quote_ident(DbBackend::MySql, &plan.name)
                    ),
                )
                .await?;
                let sql = format!(
                    "ALTER TABLE {} AUTO_INCREMENT = {}",
                    quote_ident(DbBackend::MySql, &plan.name),
                    next_id.max(1)
                );
                target
                    .execute_raw(Statement::from_string(DbBackend::MySql, sql))
                    .await
                    .map_aster_err(AsterError::database_operation)?;
            }
            DbBackend::Sqlite => {}
            _ => {
                return Err(AsterError::validation_error(
                    "unsupported database backend for sequence reset",
                ));
            }
        }
    }
    Ok(())
}

pub(super) async fn load_target_type_hints(
    target: &DatabaseConnection,
    backend: DbBackend,
    plans: &[TablePlan],
) -> Result<BTreeMap<String, BTreeMap<String, BindingKind>>> {
    let mut table_hints = BTreeMap::new();
    for plan in plans {
        let rows = load_column_type_rows(target, backend, &plan.name).await?;
        let mut hints = BTreeMap::new();
        for (column_name, raw_type) in rows {
            hints.insert(column_name, binding_kind_from_raw_type(backend, &raw_type));
        }

        for column in &plan.columns {
            if !hints.contains_key(&column.name) {
                return Err(AsterError::validation_error(format!(
                    "target table '{}' is missing column '{}'",
                    plan.name, column.name
                )));
            }
        }

        table_hints.insert(plan.name.clone(), hints);
    }

    Ok(table_hints)
}

async fn fetch_source_batch(
    source: &DatabaseConnection,
    source_backend: DbBackend,
    plan: &TablePlan,
    target_type_hints: &BTreeMap<String, BindingKind>,
    offset: i64,
    limit: i64,
) -> Result<Vec<Vec<Value>>> {
    let select_columns = plan
        .columns
        .iter()
        .map(|column| quote_ident(source_backend, &column.name))
        .collect::<Vec<_>>()
        .join(", ");
    let order_by = if plan.primary_key.is_empty() {
        String::new()
    } else {
        format!(
            " ORDER BY {}",
            plan.primary_key
                .iter()
                .map(|column| quote_ident(source_backend, column))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let sql = format!(
        "SELECT {select_columns} FROM {}{order_by} LIMIT {limit} OFFSET {offset}",
        quote_ident(source_backend, &plan.name),
    );
    let rows = source
        .query_all_raw(Statement::from_string(source_backend, sql))
        .await
        .map_aster_err(AsterError::database_operation)?;

    rows.into_iter()
        .map(|row| decode_row_values(&row, plan, target_type_hints))
        .collect()
}

async fn insert_batch<C>(target: &C, plan: &TablePlan, rows: &[Vec<Value>]) -> Result<()>
where
    C: ConnectionTrait,
{
    if rows.is_empty() {
        return Ok(());
    }

    let mut insert = Query::insert();
    insert.into_table(Alias::new(plan.name.as_str()));
    insert.columns(
        plan.columns
            .iter()
            .map(|column| Alias::new(column.name.as_str())),
    );
    for values in rows {
        insert
            .values(values.iter().cloned().map(Into::into))
            .map_aster_err(AsterError::database_operation)?;
    }

    target.execute(&insert).await.map_err(|error| {
        AsterError::database_operation(format!(
            "failed to insert batch into '{}': {error}",
            plan.name
        ))
    })?;
    Ok(())
}

fn fail_after_batches() -> Result<Option<i64>> {
    if std::env::var_os(FAIL_AFTER_BATCHES_ENV).is_none() {
        return Ok(None);
    }

    Ok(Some(parse_positive_i64_env(FAIL_AFTER_BATCHES_ENV, 0)?))
}

fn parse_positive_i64_env(name: &str, default_value: i64) -> Result<i64> {
    let Some(raw) = std::env::var_os(name) else {
        return Ok(default_value);
    };
    let raw = raw.to_string_lossy();
    let value = raw.parse::<i64>().map_err(|_| {
        AsterError::validation_error(format!("environment variable {name} must be an integer"))
    })?;
    if value <= 0 {
        return Err(AsterError::validation_error(format!(
            "environment variable {name} must be greater than zero"
        )));
    }
    Ok(value)
}
