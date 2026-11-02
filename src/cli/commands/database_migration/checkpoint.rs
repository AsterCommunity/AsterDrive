//! `database-migrate` 的断点续传检查点管理。
//!
//! 这里负责创建、加载、更新和失败标记迁移检查点，让中断后的数据复制
//! 可以从已提交的位置继续。

use sea_orm::{
    ConnectionTrait, DatabaseConnection, Statement,
    entity::prelude::DeriveIden,
    sea_query::{Alias, Expr, ExprTrait, Query, Value},
};

use crate::cli::common::database::{quote_ident, redact_database_url};
use crate::errors::{AsterError, MapAsterErr, Result};
use aster_forge_crypto::sha256_hex;

use super::args::DatabaseMigrateArgs;
use super::helpers::now_ms;
use super::model::{MigrationCheckpoint, MigrationMode, TablePlan};
use super::plan::CHECKPOINT_TABLE;
use super::plan::total_source_rows;
use super::schema::ensure_target_empty;

#[derive(DeriveIden)]
enum CheckpointColumn {
    MigrationKey,
    SourceDatabaseUrl,
    TargetDatabaseUrl,
    Mode,
    Status,
    Stage,
    CurrentTable,
    CurrentTableIndex,
    CurrentTableOffset,
    CopiedRows,
    TotalRows,
    PlanJson,
    ResultJson,
    LastError,
    HeartbeatAtMs,
    UpdatedAtMs,
}

#[derive(Debug)]
pub(super) struct InitializedCheckpoint {
    pub(super) checkpoint: MigrationCheckpoint,
    pub(super) resumed: bool,
}

/// Creates the checkpoint table in the target database when it does not exist.
pub(super) async fn ensure_checkpoint_table(target: &DatabaseConnection) -> Result<()> {
    let backend = target.get_database_backend();
    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {} (\
            {} VARCHAR(64) PRIMARY KEY, \
            {} TEXT NOT NULL, \
            {} TEXT NOT NULL, \
            {} VARCHAR(32) NOT NULL, \
            {} VARCHAR(32) NOT NULL, \
            {} VARCHAR(32) NOT NULL, \
            {} VARCHAR(255) NULL, \
            {} BIGINT NOT NULL DEFAULT 0, \
            {} BIGINT NOT NULL DEFAULT 0, \
            {} BIGINT NOT NULL DEFAULT 0, \
            {} BIGINT NOT NULL DEFAULT 0, \
            {} TEXT NOT NULL, \
            {} TEXT NULL, \
            {} TEXT NULL, \
            {} BIGINT NOT NULL DEFAULT 0, \
            {} BIGINT NOT NULL DEFAULT 0\
        )",
        quote_ident(backend, CHECKPOINT_TABLE),
        quote_ident(backend, "migration_key"),
        quote_ident(backend, "source_database_url"),
        quote_ident(backend, "target_database_url"),
        quote_ident(backend, "mode"),
        quote_ident(backend, "status"),
        quote_ident(backend, "stage"),
        quote_ident(backend, "current_table"),
        quote_ident(backend, "current_table_index"),
        quote_ident(backend, "current_table_offset"),
        quote_ident(backend, "copied_rows"),
        quote_ident(backend, "total_rows"),
        quote_ident(backend, "plan_json"),
        quote_ident(backend, "result_json"),
        quote_ident(backend, "last_error"),
        quote_ident(backend, "heartbeat_at_ms"),
        quote_ident(backend, "updated_at_ms"),
    );

    target
        .execute_raw(Statement::from_string(backend, sql))
        .await
        .map_aster_err(AsterError::database_operation)?;
    Ok(())
}

/// Loads an existing apply-mode checkpoint or initializes a new one for this plan.
pub(super) async fn initialize_checkpoint(
    args: &DatabaseMigrateArgs,
    target: &DatabaseConnection,
    plans: &[TablePlan],
) -> Result<InitializedCheckpoint> {
    let migration_key = build_migration_key(
        &args.source_database_url,
        &args.target_database_url,
        MigrationMode::Apply,
    );
    let total_rows = total_source_rows(plans);
    let plan_json = serde_json::to_string(plans).map_err(|error| {
        AsterError::internal_error(format!("failed to serialize migration plan: {error}"))
    })?;

    if let Some(mut checkpoint) = load_checkpoint(target, &migration_key).await? {
        if checkpoint.plan_json != plan_json
            && (checkpoint.copied_rows != 0 || checkpoint.current_table_index != 0)
        {
            return Err(AsterError::validation_error(
                "existing checkpoint does not match the current source plan; target must be reset before retrying",
            ));
        }

        checkpoint.plan_json = plan_json;
        checkpoint.status = "running".to_string();
        checkpoint.last_error = None;
        checkpoint.total_rows = total_rows;
        checkpoint.updated_at_ms = now_ms();
        checkpoint.heartbeat_at_ms = checkpoint.updated_at_ms;
        update_checkpoint(target, &checkpoint).await?;
        return Ok(InitializedCheckpoint {
            checkpoint,
            resumed: true,
        });
    }

    ensure_target_empty(target, plans).await?;
    let now = now_ms();
    let checkpoint = MigrationCheckpoint {
        migration_key,
        source_database_url: redact_database_url(&args.source_database_url),
        target_database_url: redact_database_url(&args.target_database_url),
        mode: MigrationMode::Apply.as_str().to_string(),
        status: "running".to_string(),
        stage: "data_copy".to_string(),
        current_table: None,
        current_table_index: 0,
        current_table_offset: 0,
        copied_rows: 0,
        total_rows,
        plan_json,
        result_json: None,
        last_error: None,
        heartbeat_at_ms: now,
        updated_at_ms: now,
    };
    insert_checkpoint(target, &checkpoint).await?;
    Ok(InitializedCheckpoint {
        checkpoint,
        resumed: false,
    })
}

pub(super) async fn update_checkpoint<C>(db: &C, checkpoint: &MigrationCheckpoint) -> Result<()>
where
    C: ConnectionTrait,
{
    let statement = Query::update()
        .table(Alias::new(CHECKPOINT_TABLE))
        .value(
            CheckpointColumn::SourceDatabaseUrl,
            checkpoint.source_database_url.clone(),
        )
        .value(
            CheckpointColumn::TargetDatabaseUrl,
            checkpoint.target_database_url.clone(),
        )
        .value(CheckpointColumn::Mode, checkpoint.mode.clone())
        .value(CheckpointColumn::Status, checkpoint.status.clone())
        .value(CheckpointColumn::Stage, checkpoint.stage.clone())
        .value(
            CheckpointColumn::CurrentTable,
            optional_string_expr(&checkpoint.current_table),
        )
        .value(
            CheckpointColumn::CurrentTableIndex,
            checkpoint.current_table_index,
        )
        .value(
            CheckpointColumn::CurrentTableOffset,
            checkpoint.current_table_offset,
        )
        .value(CheckpointColumn::CopiedRows, checkpoint.copied_rows)
        .value(CheckpointColumn::TotalRows, checkpoint.total_rows)
        .value(CheckpointColumn::PlanJson, checkpoint.plan_json.clone())
        .value(
            CheckpointColumn::ResultJson,
            optional_string_expr(&checkpoint.result_json),
        )
        .value(
            CheckpointColumn::LastError,
            optional_string_expr(&checkpoint.last_error),
        )
        .value(CheckpointColumn::HeartbeatAtMs, checkpoint.heartbeat_at_ms)
        .value(CheckpointColumn::UpdatedAtMs, checkpoint.updated_at_ms)
        .and_where(
            Expr::col(CheckpointColumn::MigrationKey)
                .eq(Expr::val(checkpoint.migration_key.clone())),
        )
        .to_owned();

    db.execute(&statement)
        .await
        .map_aster_err(AsterError::database_operation)?;
    Ok(())
}

pub(super) async fn mark_checkpoint_failed(
    target: &DatabaseConnection,
    checkpoint: &mut MigrationCheckpoint,
    error: &AsterError,
) -> Result<()> {
    checkpoint.status = "failed".to_string();
    checkpoint.last_error = Some(error.message().to_string());
    checkpoint.updated_at_ms = now_ms();
    checkpoint.heartbeat_at_ms = checkpoint.updated_at_ms;
    update_checkpoint(target, checkpoint).await
}

pub(super) fn resume_message(checkpoint: &MigrationCheckpoint) -> String {
    match checkpoint.current_table.as_deref() {
        Some(table) => format!(
            "resuming checkpoint {} at {} offset {} ({}/{})",
            checkpoint.migration_key,
            table,
            checkpoint.current_table_offset,
            checkpoint.copied_rows,
            checkpoint.total_rows
        ),
        None => format!(
            "resuming checkpoint {} at table index {} ({}/{})",
            checkpoint.migration_key,
            checkpoint.current_table_index,
            checkpoint.copied_rows,
            checkpoint.total_rows
        ),
    }
}

async fn load_checkpoint(
    target: &DatabaseConnection,
    migration_key: &str,
) -> Result<Option<MigrationCheckpoint>> {
    let backend = target.get_database_backend();
    let migration_key_placeholder = migration_key_placeholder(backend);
    let sql = format!(
        "SELECT \
            {migration_key_col}, {source_col}, {target_col}, {mode_col}, {status_col}, {stage_col}, \
            {current_table_col}, {current_table_index_col}, {current_table_offset_col}, \
            {copied_rows_col}, {total_rows_col}, {plan_json_col}, {result_json_col}, \
            {last_error_col}, {heartbeat_col}, {updated_col} \
         FROM {table_name} \
         WHERE {migration_key_col} = {migration_key_placeholder}",
        migration_key_col = quote_ident(backend, "migration_key"),
        source_col = quote_ident(backend, "source_database_url"),
        target_col = quote_ident(backend, "target_database_url"),
        mode_col = quote_ident(backend, "mode"),
        status_col = quote_ident(backend, "status"),
        stage_col = quote_ident(backend, "stage"),
        current_table_col = quote_ident(backend, "current_table"),
        current_table_index_col = quote_ident(backend, "current_table_index"),
        current_table_offset_col = quote_ident(backend, "current_table_offset"),
        copied_rows_col = quote_ident(backend, "copied_rows"),
        total_rows_col = quote_ident(backend, "total_rows"),
        plan_json_col = quote_ident(backend, "plan_json"),
        result_json_col = quote_ident(backend, "result_json"),
        last_error_col = quote_ident(backend, "last_error"),
        heartbeat_col = quote_ident(backend, "heartbeat_at_ms"),
        updated_col = quote_ident(backend, "updated_at_ms"),
        table_name = quote_ident(backend, CHECKPOINT_TABLE),
    );

    let Some(row) = target
        .query_one_raw(Statement::from_sql_and_values(
            backend,
            sql,
            [migration_key.into()],
        ))
        .await
        .map_aster_err(AsterError::database_operation)?
    else {
        return Ok(None);
    };

    Ok(Some(MigrationCheckpoint {
        migration_key: row
            .try_get_by_index::<String>(0)
            .map_aster_err(AsterError::database_operation)?,
        source_database_url: redact_database_url(
            &row.try_get_by_index::<String>(1)
                .map_aster_err(AsterError::database_operation)?,
        ),
        target_database_url: redact_database_url(
            &row.try_get_by_index::<String>(2)
                .map_aster_err(AsterError::database_operation)?,
        ),
        mode: row
            .try_get_by_index::<String>(3)
            .map_aster_err(AsterError::database_operation)?,
        status: row
            .try_get_by_index::<String>(4)
            .map_aster_err(AsterError::database_operation)?,
        stage: row
            .try_get_by_index::<String>(5)
            .map_aster_err(AsterError::database_operation)?,
        current_table: row
            .try_get_by_index::<Option<String>>(6)
            .map_aster_err(AsterError::database_operation)?,
        current_table_index: row
            .try_get_by_index::<i64>(7)
            .map_aster_err(AsterError::database_operation)?,
        current_table_offset: row
            .try_get_by_index::<i64>(8)
            .map_aster_err(AsterError::database_operation)?,
        copied_rows: row
            .try_get_by_index::<i64>(9)
            .map_aster_err(AsterError::database_operation)?,
        total_rows: row
            .try_get_by_index::<i64>(10)
            .map_aster_err(AsterError::database_operation)?,
        plan_json: row
            .try_get_by_index::<String>(11)
            .map_aster_err(AsterError::database_operation)?,
        result_json: row
            .try_get_by_index::<Option<String>>(12)
            .map_aster_err(AsterError::database_operation)?,
        last_error: row
            .try_get_by_index::<Option<String>>(13)
            .map_aster_err(AsterError::database_operation)?,
        heartbeat_at_ms: row
            .try_get_by_index::<i64>(14)
            .map_aster_err(AsterError::database_operation)?,
        updated_at_ms: row
            .try_get_by_index::<i64>(15)
            .map_aster_err(AsterError::database_operation)?,
    }))
}

async fn insert_checkpoint<C>(db: &C, checkpoint: &MigrationCheckpoint) -> Result<()>
where
    C: ConnectionTrait,
{
    let statement = Query::insert()
        .into_table(Alias::new(CHECKPOINT_TABLE))
        .columns([
            CheckpointColumn::MigrationKey,
            CheckpointColumn::SourceDatabaseUrl,
            CheckpointColumn::TargetDatabaseUrl,
            CheckpointColumn::Mode,
            CheckpointColumn::Status,
            CheckpointColumn::Stage,
            CheckpointColumn::CurrentTable,
            CheckpointColumn::CurrentTableIndex,
            CheckpointColumn::CurrentTableOffset,
            CheckpointColumn::CopiedRows,
            CheckpointColumn::TotalRows,
            CheckpointColumn::PlanJson,
            CheckpointColumn::ResultJson,
            CheckpointColumn::LastError,
            CheckpointColumn::HeartbeatAtMs,
            CheckpointColumn::UpdatedAtMs,
        ])
        .values([
            checkpoint.migration_key.clone().into(),
            checkpoint.source_database_url.clone().into(),
            checkpoint.target_database_url.clone().into(),
            checkpoint.mode.clone().into(),
            checkpoint.status.clone().into(),
            checkpoint.stage.clone().into(),
            optional_string_expr(&checkpoint.current_table),
            checkpoint.current_table_index.into(),
            checkpoint.current_table_offset.into(),
            checkpoint.copied_rows.into(),
            checkpoint.total_rows.into(),
            checkpoint.plan_json.clone().into(),
            optional_string_expr(&checkpoint.result_json),
            optional_string_expr(&checkpoint.last_error),
            checkpoint.heartbeat_at_ms.into(),
            checkpoint.updated_at_ms.into(),
        ])
        .map_aster_err(AsterError::database_operation)?
        .to_owned();

    db.execute(&statement)
        .await
        .map_aster_err(AsterError::database_operation)?;
    Ok(())
}

fn optional_string_expr(value: &Option<String>) -> Expr {
    Expr::value(Value::String(value.clone()))
}

fn migration_key_placeholder(backend: sea_orm::DbBackend) -> &'static str {
    match backend {
        sea_orm::DbBackend::Postgres => "$1",
        sea_orm::DbBackend::MySql | sea_orm::DbBackend::Sqlite => "?",
        _ => "?",
    }
}

fn build_migration_key(
    source_database_url: &str,
    target_database_url: &str,
    mode: MigrationMode,
) -> String {
    let key = format!(
        "{}\n{}\n{}",
        source_database_url,
        target_database_url,
        mode.as_str()
    );
    sha256_hex(key.as_bytes())
}

#[cfg(test)]
mod tests {
    use sea_orm::{Database, DbBackend};

    use super::*;

    #[tokio::test]
    async fn checkpoint_insert_update_bind_values_without_sql_literal_concatenation() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        ensure_checkpoint_table(&db).await.unwrap();

        let mut checkpoint = MigrationCheckpoint {
            migration_key: "checkpoint-key".to_string(),
            source_database_url: "sqlite:///.../source's.db?mode=rwc".to_string(),
            target_database_url: "sqlite:///.../target\\path.db?mode=rwc".to_string(),
            mode: "apply".to_string(),
            status: "running".to_string(),
            stage: "data_copy".to_string(),
            current_table: Some("folders'quoted".to_string()),
            current_table_index: 1,
            current_table_offset: 2,
            copied_rows: 3,
            total_rows: 4,
            plan_json: r#"[{"name":"files'quoted"}]"#.to_string(),
            result_json: None,
            last_error: Some("first line\nsecond line's detail".to_string()),
            heartbeat_at_ms: 5,
            updated_at_ms: 6,
        };

        insert_checkpoint(&db, &checkpoint).await.unwrap();
        let loaded = load_checkpoint(&db, &checkpoint.migration_key)
            .await
            .unwrap()
            .expect("checkpoint should be inserted");
        assert_eq!(loaded.source_database_url, checkpoint.source_database_url);
        assert_eq!(loaded.target_database_url, checkpoint.target_database_url);
        assert_eq!(loaded.current_table, checkpoint.current_table);
        assert_eq!(loaded.plan_json, checkpoint.plan_json);
        assert_eq!(loaded.result_json, None);
        assert_eq!(loaded.last_error, checkpoint.last_error);

        checkpoint.status = "failed".to_string();
        checkpoint.current_table = None;
        checkpoint.result_json = Some(r#"{"ready":false,"note":"quoted'value"}"#.to_string());
        checkpoint.last_error = None;
        checkpoint.copied_rows = 9;
        checkpoint.heartbeat_at_ms = 10;
        checkpoint.updated_at_ms = 11;
        update_checkpoint(&db, &checkpoint).await.unwrap();

        let updated = load_checkpoint(&db, &checkpoint.migration_key)
            .await
            .unwrap()
            .expect("checkpoint should still exist");
        assert_eq!(updated.status, "failed");
        assert_eq!(updated.current_table, None);
        assert_eq!(updated.result_json, checkpoint.result_json);
        assert_eq!(updated.last_error, None);
        assert_eq!(updated.copied_rows, 9);
        assert_eq!(updated.heartbeat_at_ms, 10);
        assert_eq!(updated.updated_at_ms, 11);

        assert_eq!(db.get_database_backend(), DbBackend::Sqlite);
    }

    #[test]
    fn checkpoint_migration_key_placeholder_matches_backend() {
        assert_eq!(migration_key_placeholder(DbBackend::Postgres), "$1");
        assert_eq!(migration_key_placeholder(DbBackend::MySql), "?");
        assert_eq!(migration_key_placeholder(DbBackend::Sqlite), "?");
    }
}
