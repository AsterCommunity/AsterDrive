//! `database-migrate` 的 schema 发现与迁移计划构建。
//!
//! 这里负责连接数据库、识别后端、校验源库表集合，并把源表元数据整理成
//! 可执行的复制计划。

use std::collections::{BTreeMap, BTreeSet};

use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};

use crate::cli::db_shared::{backend_name, join_strings, quote_literal, quote_sqlite_literal};
use crate::db;
use crate::errors::{AsterError, MapAsterErr, Result};

use super::helpers::count_rows;
use super::{
    BindingKind, CHECKPOINT_TABLE, COPY_TABLE_ORDER, ColumnSchema, MIGRATION_TABLE, TablePlan,
    TableReport,
};

pub(super) async fn connect_database(database_url: &str) -> Result<DatabaseConnection> {
    db::connect_with_metrics(
        &crate::config::DatabaseConfig {
            url: database_url.to_string(),
            pool_size: 1,
            retry_count: 0,
        },
        crate::metrics::NoopMetrics::arc(),
    )
    .await
}

pub(super) fn validate_backends(
    source_backend: DbBackend,
    target_backend: DbBackend,
) -> Result<()> {
    for (role, backend) in [("source", source_backend), ("target", target_backend)] {
        if !matches!(
            backend,
            DbBackend::Sqlite | DbBackend::Postgres | DbBackend::MySql
        ) {
            return Err(AsterError::validation_error(format!(
                "{role} backend must be sqlite, postgres, or mysql, got {}",
                backend_name(backend)
            )));
        }
    }

    Ok(())
}

/// Loads the ordered source table plans that drive copy and verification stages.
pub(super) async fn load_source_plans(source: &DatabaseConnection) -> Result<Vec<TablePlan>> {
    let backend = source.get_database_backend();
    let existing_tables = source_table_names(source, backend).await?;
    validate_source_tables(backend, &existing_tables)?;

    let existing_lookup: BTreeSet<&str> = existing_tables.iter().map(String::as_str).collect();
    let mut plans = Vec::with_capacity(COPY_TABLE_ORDER.len());
    for table in COPY_TABLE_ORDER {
        if !existing_lookup.contains(*table) {
            return Err(AsterError::validation_error(format!(
                "source database is missing expected table '{}'",
                table
            )));
        }
        plans.push(load_table_plan(source, backend, table).await?);
    }
    Ok(plans)
}

pub(super) fn plans_to_reports(plans: &[TablePlan]) -> Vec<TableReport> {
    plans
        .iter()
        .map(|plan| TableReport {
            name: plan.name.clone(),
            primary_key: plan.primary_key.clone(),
            source_rows: plan.source_rows,
            target_rows: 0,
            copied_rows: 0,
            sequence_reset: plan.sequence_reset,
        })
        .collect()
}

pub(super) async fn ensure_target_empty<C>(target: &C, plans: &[TablePlan]) -> Result<()>
where
    C: ConnectionTrait,
{
    let backend = target.get_database_backend();
    let mut non_empty = Vec::new();
    for plan in plans {
        let count = count_rows(target, backend, &plan.name).await?;
        if count != 0 {
            non_empty.push(format!("{}={count}", plan.name));
        }
    }

    if non_empty.is_empty() {
        return Ok(());
    }

    Err(AsterError::validation_error(format!(
        "target database must be empty before migration; found rows in {}",
        non_empty.join(", ")
    )))
}

pub(super) async fn refresh_target_rows(
    target: &DatabaseConnection,
    reports: &mut [TableReport],
) -> Result<()> {
    let backend = target.get_database_backend();
    for report in reports {
        report.target_rows = count_rows(target, backend, &report.name).await?;
    }
    Ok(())
}

pub(super) async fn load_column_type_rows<C>(
    db: &C,
    backend: DbBackend,
    table_name: &str,
) -> Result<Vec<(String, String)>>
where
    C: ConnectionTrait,
{
    let sql = match backend {
        DbBackend::Sqlite => format!("PRAGMA table_info({})", quote_sqlite_literal(table_name)),
        DbBackend::Postgres => format!(
            "SELECT column_name, udt_name \
             FROM information_schema.columns \
             WHERE table_schema = current_schema() AND table_name = {} \
             ORDER BY ordinal_position",
            quote_literal(table_name)
        ),
        DbBackend::MySql => format!(
            "SELECT column_name, column_type \
             FROM information_schema.columns \
             WHERE table_schema = DATABASE() AND table_name = {} \
             ORDER BY ordinal_position",
            quote_literal(table_name)
        ),
        _ => {
            return Err(AsterError::validation_error(
                "unsupported database backend for type hints",
            ));
        }
    };

    let rows = db
        .query_all_raw(Statement::from_string(backend, sql))
        .await
        .map_aster_err(AsterError::database_operation)?;

    match backend {
        DbBackend::Sqlite => rows
            .into_iter()
            .map(|row| {
                let name: String = row
                    .try_get("", "name")
                    .map_aster_err(AsterError::database_operation)?;
                let raw_type: String = row
                    .try_get("", "type")
                    .map_aster_err(AsterError::database_operation)?;
                Ok((name, raw_type))
            })
            .collect(),
        _ => rows
            .into_iter()
            .map(|row| {
                let name = row
                    .try_get_by_index::<String>(0)
                    .map_aster_err(AsterError::database_operation)?;
                let raw_type = row
                    .try_get_by_index::<String>(1)
                    .map_aster_err(AsterError::database_operation)?;
                Ok((name, raw_type))
            })
            .collect(),
    }
}

pub(super) fn binding_kind_from_raw_type(backend: DbBackend, raw_type: &str) -> BindingKind {
    let normalized = raw_type.to_ascii_lowercase();
    match backend {
        DbBackend::Postgres => {
            if normalized == "bool" {
                BindingKind::Bool
            } else if normalized == "int8" {
                BindingKind::Int64
            } else if matches!(normalized.as_str(), "int2" | "int4") {
                BindingKind::Int32
            } else if normalized == "float4" || normalized == "float8" || normalized == "numeric" {
                BindingKind::Float64
            } else if normalized == "bytea" {
                BindingKind::Bytes
            } else if matches!(normalized.as_str(), "json" | "jsonb") {
                BindingKind::Json
            } else if normalized.contains("timestamp") || normalized == "timestamptz" {
                BindingKind::TimestampWithTimeZone
            } else {
                BindingKind::String
            }
        }
        DbBackend::MySql => {
            if normalized.starts_with("tinyint(1)") || normalized == "boolean" {
                BindingKind::Bool
            } else if normalized.starts_with("bigint") {
                BindingKind::Int64
            } else if normalized.contains("int") {
                BindingKind::Int32
            } else if normalized.contains("double")
                || normalized.contains("float")
                || normalized.contains("decimal")
            {
                BindingKind::Float64
            } else if normalized.contains("blob") || normalized.contains("binary") {
                BindingKind::Bytes
            } else if normalized == "json" {
                BindingKind::Json
            } else if normalized.contains("timestamp") || normalized.contains("datetime") {
                BindingKind::TimestampWithTimeZone
            } else {
                BindingKind::String
            }
        }
        DbBackend::Sqlite => {
            if normalized.contains("bool") {
                BindingKind::Bool
            } else if normalized.contains("json") {
                BindingKind::Json
            } else if normalized.contains("timestamp") || normalized.contains("datetime") {
                BindingKind::TimestampWithTimeZone
            } else if normalized.contains("blob") {
                BindingKind::Bytes
            } else if normalized.contains("double")
                || normalized.contains("float")
                || normalized.contains("real")
                || normalized.contains("decimal")
            {
                BindingKind::Float64
            } else if normalized.contains("int") {
                BindingKind::Int64
            } else {
                BindingKind::String
            }
        }
        _ => BindingKind::String,
    }
}

pub(super) fn total_source_rows(plans: &[TablePlan]) -> i64 {
    plans.iter().map(|plan| plan.source_rows).sum()
}

async fn source_table_names<C>(source: &C, backend: DbBackend) -> Result<Vec<String>>
where
    C: ConnectionTrait,
{
    let sql = match backend {
        DbBackend::Sqlite => "SELECT name FROM sqlite_master \
                              WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
            .to_string(),
        DbBackend::Postgres => "SELECT table_name FROM information_schema.tables \
                                WHERE table_schema = current_schema() AND table_type = 'BASE TABLE' \
                                ORDER BY table_name"
            .to_string(),
        DbBackend::MySql => "SELECT table_name FROM information_schema.tables \
                             WHERE table_schema = DATABASE() AND table_type = 'BASE TABLE' \
                             ORDER BY table_name"
            .to_string(),
        _ => {
            return Err(AsterError::validation_error(
                "unsupported source backend for table discovery",
            ));
        }
    };

    let rows = source
        .query_all_raw(Statement::from_string(backend, sql))
        .await
        .map_aster_err(AsterError::database_operation)?;
    rows.into_iter()
        .map(|row| {
            row.try_get_by_index::<String>(0)
                .map_aster_err(AsterError::database_operation)
        })
        .collect()
}

fn validate_source_tables(backend: DbBackend, existing_tables: &[String]) -> Result<()> {
    let known: BTreeSet<&str> = COPY_TABLE_ORDER
        .iter()
        .copied()
        .chain([MIGRATION_TABLE, CHECKPOINT_TABLE])
        .collect();
    let unexpected: Vec<String> = existing_tables
        .iter()
        .filter(|table| {
            !(known.contains(table.as_str())
                || backend == DbBackend::Sqlite && db::sqlite_search::is_sqlite_search_table(table))
        })
        .cloned()
        .collect();

    if !unexpected.is_empty() {
        return Err(AsterError::validation_error(format!(
            "source database contains unsupported tables that would not be migrated: {}",
            join_strings(&unexpected)
        )));
    }

    Ok(())
}

async fn load_table_plan<C>(db: &C, backend: DbBackend, table_name: &str) -> Result<TablePlan>
where
    C: ConnectionTrait,
{
    let columns = match backend {
        DbBackend::Sqlite => load_sqlite_columns(db, table_name).await?,
        DbBackend::Postgres => load_postgres_columns(db, table_name).await?,
        DbBackend::MySql => load_mysql_columns(db, table_name).await?,
        _ => {
            return Err(AsterError::validation_error(
                "unsupported source backend for schema inspection",
            ));
        }
    };

    if columns.is_empty() {
        return Err(AsterError::validation_error(format!(
            "source table '{}' has no columns",
            table_name
        )));
    }

    let mut primary_key_pairs: Vec<(i32, String)> = columns
        .iter()
        .filter(|column| column.pk_order > 0)
        .map(|column| (column.pk_order, column.name.clone()))
        .collect();
    primary_key_pairs.sort_by_key(|(pk_order, _)| *pk_order);
    let primary_key = primary_key_pairs
        .into_iter()
        .map(|(_, name)| name)
        .collect::<Vec<_>>();
    let source_rows = count_rows(db, backend, table_name).await?;
    let sequence_reset = columns.iter().any(|column| {
        column.name == "id" && column.pk_order == 1 && binding_kind_is_integer(column.binding_kind)
    });

    Ok(TablePlan {
        name: table_name.to_string(),
        columns,
        primary_key,
        source_rows,
        sequence_reset,
    })
}

async fn load_sqlite_columns<C>(db: &C, table_name: &str) -> Result<Vec<ColumnSchema>>
where
    C: ConnectionTrait,
{
    let sql = format!("PRAGMA table_info({})", quote_sqlite_literal(table_name));
    let rows = db
        .query_all_raw(Statement::from_string(DbBackend::Sqlite, sql))
        .await
        .map_aster_err(AsterError::database_operation)?;

    rows.into_iter()
        .map(|row| {
            let name: String = row
                .try_get("", "name")
                .map_aster_err(AsterError::database_operation)?;
            let raw_type: String = row
                .try_get("", "type")
                .map_aster_err(AsterError::database_operation)?;
            let pk_order: i32 = row
                .try_get("", "pk")
                .map_aster_err(AsterError::database_operation)?;
            Ok(ColumnSchema {
                name,
                binding_kind: binding_kind_from_raw_type(DbBackend::Sqlite, &raw_type),
                raw_type,
                pk_order,
            })
        })
        .collect()
}

async fn load_postgres_columns<C>(db: &C, table_name: &str) -> Result<Vec<ColumnSchema>>
where
    C: ConnectionTrait,
{
    let sql = format!(
        "SELECT column_name, udt_name \
         FROM information_schema.columns \
         WHERE table_schema = current_schema() AND table_name = {} \
         ORDER BY ordinal_position",
        quote_literal(table_name)
    );
    let pk_lookup = load_primary_key_lookup(
        db,
        DbBackend::Postgres,
        &format!(
            "SELECT kcu.column_name, kcu.ordinal_position \
             FROM information_schema.table_constraints tc \
             JOIN information_schema.key_column_usage kcu \
               ON tc.constraint_name = kcu.constraint_name \
              AND tc.table_schema = kcu.table_schema \
             WHERE tc.constraint_type = 'PRIMARY KEY' \
               AND tc.table_schema = current_schema() \
               AND tc.table_name = {} \
             ORDER BY kcu.ordinal_position",
            quote_literal(table_name)
        ),
    )
    .await?;

    let rows = db
        .query_all_raw(Statement::from_string(DbBackend::Postgres, sql))
        .await
        .map_aster_err(AsterError::database_operation)?;

    rows.into_iter()
        .map(|row| {
            let name = row
                .try_get_by_index::<String>(0)
                .map_aster_err(AsterError::database_operation)?;
            let raw_type = row
                .try_get_by_index::<String>(1)
                .map_aster_err(AsterError::database_operation)?;
            Ok(ColumnSchema {
                pk_order: *pk_lookup.get(&name).unwrap_or(&0),
                binding_kind: binding_kind_from_raw_type(DbBackend::Postgres, &raw_type),
                name,
                raw_type,
            })
        })
        .collect()
}

async fn load_mysql_columns<C>(db: &C, table_name: &str) -> Result<Vec<ColumnSchema>>
where
    C: ConnectionTrait,
{
    let sql = format!(
        "SELECT column_name, column_type \
         FROM information_schema.columns \
         WHERE table_schema = DATABASE() AND table_name = {} \
         ORDER BY ordinal_position",
        quote_literal(table_name)
    );
    let pk_lookup = load_primary_key_lookup(
        db,
        DbBackend::MySql,
        &format!(
            "SELECT column_name, ordinal_position \
             FROM information_schema.key_column_usage \
             WHERE table_schema = DATABASE() \
               AND table_name = {} \
               AND constraint_name = 'PRIMARY' \
             ORDER BY ordinal_position",
            quote_literal(table_name)
        ),
    )
    .await?;

    let rows = db
        .query_all_raw(Statement::from_string(DbBackend::MySql, sql))
        .await
        .map_aster_err(AsterError::database_operation)?;

    rows.into_iter()
        .map(|row| {
            let name = row
                .try_get_by_index::<String>(0)
                .map_aster_err(AsterError::database_operation)?;
            let raw_type = row
                .try_get_by_index::<String>(1)
                .map_aster_err(AsterError::database_operation)?;
            Ok(ColumnSchema {
                pk_order: *pk_lookup.get(&name).unwrap_or(&0),
                binding_kind: binding_kind_from_raw_type(DbBackend::MySql, &raw_type),
                name,
                raw_type,
            })
        })
        .collect()
}

async fn load_primary_key_lookup<C>(
    db: &C,
    backend: DbBackend,
    sql: &str,
) -> Result<BTreeMap<String, i32>>
where
    C: ConnectionTrait,
{
    let rows = db
        .query_all_raw(Statement::from_string(backend, sql))
        .await
        .map_aster_err(AsterError::database_operation)?;
    let mut lookup = BTreeMap::new();
    for row in rows {
        let column_name = row
            .try_get_by_index::<String>(0)
            .map_aster_err(AsterError::database_operation)?;
        let ordinal = if let Ok(value) = row.try_get_by_index::<i32>(1) {
            value
        } else if let Ok(value) = row.try_get_by_index::<u32>(1) {
            i32::try_from(value).map_err(|_| {
                AsterError::database_operation(format!(
                    "primary key ordinal position {value} does not fit into i32"
                ))
            })?
        } else {
            return Err(AsterError::database_operation(
                "failed to decode primary key ordinal position".to_string(),
            ));
        };
        lookup.insert(column_name, ordinal);
    }
    Ok(lookup)
}

fn binding_kind_is_integer(kind: BindingKind) -> bool {
    matches!(kind, BindingKind::Int32 | BindingKind::Int64)
}
