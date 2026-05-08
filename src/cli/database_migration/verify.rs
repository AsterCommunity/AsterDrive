//! `database-migrate` 的复制后数据校验。
//!
//! 这里检查行数、唯一约束和外键完整性，给出是否可以切换到目标库的判断依据。

use std::collections::BTreeSet;

use sea_orm::{ConnectionTrait, DbBackend, Statement};

use crate::cli::db_shared::{quote_ident, quote_literal, quote_sqlite_literal, scalar_i64};
use crate::errors::{AsterError, MapAsterErr, Result};

use super::{COPY_TABLE_ORDER, ConstraintCheck, CountMismatch, TablePlan, VerificationReport};

#[derive(Debug)]
struct UniqueIndex {
    table: String,
    name: String,
    columns: Vec<String>,
    is_primary: bool,
}

#[derive(Debug, Clone, Copy)]
struct ExpressionUniqueCheck {
    table: &'static str,
    constraint: &'static str,
    expressions: &'static [&'static str],
}

#[derive(Debug)]
struct ForeignKey {
    table: String,
    name: String,
    column: String,
    referenced_table: String,
    referenced_column: String,
}

pub(super) async fn verify_target<C>(target: &C, plans: &[TablePlan]) -> Result<VerificationReport>
where
    C: ConnectionTrait,
{
    let mut verification = VerificationReport {
        checked: true,
        ..Default::default()
    };
    let target_backend = target.get_database_backend();

    for plan in plans {
        let target_rows = super::helpers::count_rows(target, target_backend, &plan.name).await?;
        if target_rows != plan.source_rows {
            verification.count_mismatches.push(CountMismatch {
                table: plan.name.clone(),
                source_rows: plan.source_rows,
                target_rows,
            });
        }
    }

    let unique_indexes = load_unique_indexes(target, target_backend, plans).await?;
    let expression_checks = expression_unique_checks();
    verification.checked_unique_constraints = unique_indexes.len() + expression_checks.len();
    for index in unique_indexes {
        let violations =
            count_duplicate_groups(target, target_backend, &index.table, &index.columns).await?;
        if violations != 0 {
            verification.unique_conflicts.push(ConstraintCheck {
                table: index.table,
                constraint: if index.is_primary {
                    format!("{} (primary key)", index.name)
                } else {
                    index.name
                },
                columns: index.columns,
                violations,
            });
        }
    }

    for check in expression_checks {
        let violations =
            count_expression_duplicates(target, target_backend, check.table, check.expressions)
                .await?;
        if violations != 0 {
            verification.unique_conflicts.push(ConstraintCheck {
                table: check.table.to_string(),
                constraint: check.constraint.to_string(),
                columns: check
                    .expressions
                    .iter()
                    .map(|value| value.to_string())
                    .collect(),
                violations,
            });
        }
    }

    let foreign_keys = load_foreign_keys(target, target_backend, plans).await?;
    verification.checked_foreign_keys = foreign_keys.len();
    for foreign_key in foreign_keys {
        let violations = count_foreign_key_violations(target, target_backend, &foreign_key).await?;
        if violations != 0 {
            verification.foreign_key_violations.push(ConstraintCheck {
                table: foreign_key.table,
                constraint: foreign_key.name,
                columns: vec![foreign_key.column],
                violations,
            });
        }
    }

    Ok(verification)
}

pub(super) fn verification_ready(verification: &VerificationReport) -> bool {
    verification.count_mismatches.is_empty()
        && verification.unique_conflicts.is_empty()
        && verification.foreign_key_violations.is_empty()
}

pub(super) fn verification_message(verification: &VerificationReport, ready: bool) -> String {
    if ready {
        return format!(
            "row counts matched; checked {} unique constraints and {} foreign keys",
            verification.checked_unique_constraints, verification.checked_foreign_keys
        );
    }

    format!(
        "counts={} unique_conflicts={} foreign_key_violations={}",
        verification.count_mismatches.len(),
        verification.unique_conflicts.len(),
        verification.foreign_key_violations.len()
    )
}

async fn load_unique_indexes<C>(
    db: &C,
    backend: DbBackend,
    plans: &[TablePlan],
) -> Result<Vec<UniqueIndex>>
where
    C: ConnectionTrait,
{
    let table_filter = COPY_TABLE_ORDER
        .iter()
        .map(|table| quote_literal(table))
        .collect::<Vec<_>>()
        .join(", ");
    match backend {
        DbBackend::Postgres => {
            let sql = format!(
                "SELECT t.relname AS table_name, i.relname AS index_name, \
                        string_agg(a.attname, ',' ORDER BY cols.ordinality) AS columns, \
                        idx.indisprimary AS is_primary \
                 FROM pg_index idx \
                 JOIN pg_class t ON t.oid = idx.indrelid \
                 JOIN pg_namespace ns ON ns.oid = t.relnamespace \
                 JOIN pg_class i ON i.oid = idx.indexrelid \
                 JOIN LATERAL unnest(idx.indkey) WITH ORDINALITY AS cols(attnum, ordinality) ON TRUE \
                 JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = cols.attnum \
                 WHERE ns.nspname = current_schema() \
                   AND t.relname IN ({table_filter}) \
                   AND idx.indisunique \
                   AND idx.indexprs IS NULL \
                 GROUP BY t.relname, i.relname, idx.indisprimary \
                 ORDER BY t.relname, i.relname"
            );
            let rows = db
                .query_all_raw(Statement::from_string(backend, sql))
                .await
                .map_aster_err(AsterError::database_operation)?;
            rows.into_iter()
                .map(|row| {
                    Ok(UniqueIndex {
                        table: row
                            .try_get_by_index::<String>(0)
                            .map_aster_err(AsterError::database_operation)?,
                        name: row
                            .try_get_by_index::<String>(1)
                            .map_aster_err(AsterError::database_operation)?,
                        columns: row
                            .try_get_by_index::<String>(2)
                            .map_aster_err(AsterError::database_operation)?
                            .split(',')
                            .map(str::to_string)
                            .collect(),
                        is_primary: row
                            .try_get_by_index::<bool>(3)
                            .map_aster_err(AsterError::database_operation)?,
                    })
                })
                .collect()
        }
        DbBackend::MySql => {
            let sql = format!(
                "SELECT table_name, index_name, \
                        GROUP_CONCAT(column_name ORDER BY seq_in_index SEPARATOR ',') AS columns, \
                        CASE WHEN index_name = 'PRIMARY' THEN 1 ELSE 0 END AS is_primary \
                 FROM information_schema.statistics \
                 WHERE table_schema = DATABASE() \
                   AND table_name IN ({table_filter}) \
                   AND non_unique = 0 \
                   AND index_name NOT IN (\
                        'idx_files_unique_live_name', \
                        'idx_folders_unique_live_name', \
                        'idx_contact_verification_tokens_single_active'\
                   ) \
                 GROUP BY table_name, index_name \
                 ORDER BY table_name, index_name"
            );
            let rows = db
                .query_all_raw(Statement::from_string(backend, sql))
                .await
                .map_aster_err(AsterError::database_operation)?;
            rows.into_iter()
                .map(|row| {
                    Ok(UniqueIndex {
                        table: row
                            .try_get_by_index::<String>(0)
                            .map_aster_err(AsterError::database_operation)?,
                        name: row
                            .try_get_by_index::<String>(1)
                            .map_aster_err(AsterError::database_operation)?,
                        columns: row
                            .try_get_by_index::<String>(2)
                            .map_aster_err(AsterError::database_operation)?
                            .split(',')
                            .map(str::to_string)
                            .collect(),
                        is_primary: row
                            .try_get_by_index::<bool>(3)
                            .map_aster_err(AsterError::database_operation)?,
                    })
                })
                .collect()
        }
        DbBackend::Sqlite => load_sqlite_unique_indexes(db, plans).await,
        _ => Err(AsterError::validation_error(
            "unsupported database backend for unique index verification",
        )),
    }
}

async fn load_sqlite_unique_indexes<C>(db: &C, plans: &[TablePlan]) -> Result<Vec<UniqueIndex>>
where
    C: ConnectionTrait,
{
    let mut indexes = Vec::new();
    let mut seen = BTreeSet::new();
    for plan in plans {
        if !plan.primary_key.is_empty() {
            indexes.push(UniqueIndex {
                table: plan.name.clone(),
                name: format!("{}_primary_key", plan.name),
                columns: plan.primary_key.clone(),
                is_primary: true,
            });
            seen.insert((plan.name.clone(), format!("{}_primary_key", plan.name)));
        }

        let sql = format!("PRAGMA index_list({})", quote_sqlite_literal(&plan.name));
        let rows = db
            .query_all_raw(Statement::from_string(DbBackend::Sqlite, sql))
            .await
            .map_aster_err(AsterError::database_operation)?;

        for row in rows {
            let unique: i32 = row
                .try_get("", "unique")
                .map_aster_err(AsterError::database_operation)?;
            if unique == 0 {
                continue;
            }

            let name: String = row
                .try_get("", "name")
                .map_aster_err(AsterError::database_operation)?;
            if matches!(
                name.as_str(),
                "idx_files_unique_live_name"
                    | "idx_folders_unique_live_name"
                    | "idx_contact_verification_tokens_single_active"
            ) {
                continue;
            }

            let origin: String = row
                .try_get("", "origin")
                .map_aster_err(AsterError::database_operation)?;
            let info_sql = format!("PRAGMA index_info({})", quote_sqlite_literal(&name));
            let info_rows = db
                .query_all_raw(Statement::from_string(DbBackend::Sqlite, info_sql))
                .await
                .map_aster_err(AsterError::database_operation)?;
            let mut columns = Vec::new();
            for info_row in info_rows {
                let column_name = info_row
                    .try_get::<Option<String>>("", "name")
                    .map_aster_err(AsterError::database_operation)?;
                if let Some(column_name) = column_name {
                    columns.push(column_name);
                }
            }
            if columns.is_empty() || seen.contains(&(plan.name.clone(), name.clone())) {
                continue;
            }

            indexes.push(UniqueIndex {
                table: plan.name.clone(),
                name: name.clone(),
                columns,
                is_primary: origin == "pk",
            });
            seen.insert((plan.name.clone(), name));
        }
    }
    Ok(indexes)
}

async fn count_duplicate_groups<C>(
    db: &C,
    backend: DbBackend,
    table: &str,
    columns: &[String],
) -> Result<i64>
where
    C: ConnectionTrait,
{
    let table_ident = quote_ident(backend, table);
    let select_cols = columns
        .iter()
        .map(|column| quote_ident(backend, column))
        .collect::<Vec<_>>()
        .join(", ");
    let null_filter = columns
        .iter()
        .map(|column| format!("{} IS NOT NULL", quote_ident(backend, column)))
        .collect::<Vec<_>>()
        .join(" AND ");
    let where_clause = if null_filter.is_empty() {
        String::new()
    } else {
        format!(" WHERE {null_filter}")
    };
    let inner = format!(
        "SELECT 1 FROM {table_ident}{where_clause} GROUP BY {select_cols} HAVING COUNT(*) > 1"
    );
    let sql = format!("SELECT COUNT(*) FROM ({inner}) AS duplicate_groups");
    scalar_i64(db, backend, &sql).await
}

fn expression_unique_checks() -> &'static [ExpressionUniqueCheck] {
    &[
        ExpressionUniqueCheck {
            table: "files",
            constraint: "idx_files_unique_live_name",
            expressions: &[
                "CASE WHEN team_id IS NULL THEN 0 ELSE 1 END",
                "CASE WHEN team_id IS NULL THEN owner_user_id ELSE team_id END",
                "COALESCE(folder_id, 0)",
                "name",
                "CASE WHEN deleted_at IS NULL THEN 1 ELSE NULL END",
            ],
        },
        ExpressionUniqueCheck {
            table: "folders",
            constraint: "idx_folders_unique_live_name",
            expressions: &[
                "CASE WHEN team_id IS NULL THEN 0 ELSE 1 END",
                "CASE WHEN team_id IS NULL THEN owner_user_id ELSE team_id END",
                "COALESCE(parent_id, 0)",
                "name",
                "CASE WHEN deleted_at IS NULL THEN 1 ELSE NULL END",
            ],
        },
        ExpressionUniqueCheck {
            table: "contact_verification_tokens",
            constraint: "idx_contact_verification_tokens_single_active",
            expressions: &[
                "user_id",
                "channel",
                "purpose",
                "CASE WHEN consumed_at IS NULL THEN 1 ELSE NULL END",
            ],
        },
    ]
}

async fn count_expression_duplicates<C>(
    db: &C,
    backend: DbBackend,
    table: &str,
    expressions: &[&str],
) -> Result<i64>
where
    C: ConnectionTrait,
{
    let table_ident = quote_ident(backend, table);
    let where_clause = expressions
        .iter()
        .map(|expression| format!("({expression}) IS NOT NULL"))
        .collect::<Vec<_>>()
        .join(" AND ");
    let group_by = expressions.join(", ");
    let inner = format!(
        "SELECT 1 FROM {table_ident} WHERE {where_clause} GROUP BY {group_by} HAVING COUNT(*) > 1"
    );
    let sql = format!("SELECT COUNT(*) FROM ({inner}) AS duplicate_groups");
    scalar_i64(db, backend, &sql).await
}

async fn load_foreign_keys<C>(
    db: &C,
    backend: DbBackend,
    plans: &[TablePlan],
) -> Result<Vec<ForeignKey>>
where
    C: ConnectionTrait,
{
    let table_filter = COPY_TABLE_ORDER
        .iter()
        .map(|table| quote_literal(table))
        .collect::<Vec<_>>()
        .join(", ");
    match backend {
        DbBackend::Postgres => {
            let sql = format!(
                "SELECT tc.table_name, tc.constraint_name, kcu.column_name, \
                        ccu.table_name AS referenced_table, ccu.column_name AS referenced_column \
                 FROM information_schema.table_constraints tc \
                 JOIN information_schema.key_column_usage kcu \
                   ON tc.constraint_name = kcu.constraint_name \
                  AND tc.table_schema = kcu.table_schema \
                 JOIN information_schema.constraint_column_usage ccu \
                   ON tc.constraint_name = ccu.constraint_name \
                  AND tc.table_schema = ccu.table_schema \
                 WHERE tc.constraint_type = 'FOREIGN KEY' \
                   AND tc.table_schema = current_schema() \
                   AND tc.table_name IN ({table_filter}) \
                 ORDER BY tc.table_name, tc.constraint_name, kcu.ordinal_position"
            );
            let rows = db
                .query_all_raw(Statement::from_string(backend, sql))
                .await
                .map_aster_err(AsterError::database_operation)?;
            rows.into_iter()
                .map(|row| {
                    Ok(ForeignKey {
                        table: row
                            .try_get_by_index::<String>(0)
                            .map_aster_err(AsterError::database_operation)?,
                        name: row
                            .try_get_by_index::<String>(1)
                            .map_aster_err(AsterError::database_operation)?,
                        column: row
                            .try_get_by_index::<String>(2)
                            .map_aster_err(AsterError::database_operation)?,
                        referenced_table: row
                            .try_get_by_index::<String>(3)
                            .map_aster_err(AsterError::database_operation)?,
                        referenced_column: row
                            .try_get_by_index::<String>(4)
                            .map_aster_err(AsterError::database_operation)?,
                    })
                })
                .collect()
        }
        DbBackend::MySql => {
            let sql = format!(
                "SELECT table_name, constraint_name, column_name, \
                        referenced_table_name, referenced_column_name \
                 FROM information_schema.key_column_usage \
                 WHERE table_schema = DATABASE() \
                   AND referenced_table_name IS NOT NULL \
                   AND table_name IN ({table_filter}) \
                 ORDER BY table_name, constraint_name, ordinal_position"
            );
            let rows = db
                .query_all_raw(Statement::from_string(backend, sql))
                .await
                .map_aster_err(AsterError::database_operation)?;
            rows.into_iter()
                .map(|row| {
                    Ok(ForeignKey {
                        table: row
                            .try_get_by_index::<String>(0)
                            .map_aster_err(AsterError::database_operation)?,
                        name: row
                            .try_get_by_index::<String>(1)
                            .map_aster_err(AsterError::database_operation)?,
                        column: row
                            .try_get_by_index::<String>(2)
                            .map_aster_err(AsterError::database_operation)?,
                        referenced_table: row
                            .try_get_by_index::<String>(3)
                            .map_aster_err(AsterError::database_operation)?,
                        referenced_column: row
                            .try_get_by_index::<String>(4)
                            .map_aster_err(AsterError::database_operation)?,
                    })
                })
                .collect()
        }
        DbBackend::Sqlite => load_sqlite_foreign_keys(db, plans).await,
        _ => Err(AsterError::validation_error(
            "unsupported database backend for foreign-key verification",
        )),
    }
}

async fn load_sqlite_foreign_keys<C>(db: &C, plans: &[TablePlan]) -> Result<Vec<ForeignKey>>
where
    C: ConnectionTrait,
{
    let mut foreign_keys = Vec::new();
    for plan in plans {
        let sql = format!(
            "PRAGMA foreign_key_list({})",
            quote_sqlite_literal(&plan.name)
        );
        let rows = db
            .query_all_raw(Statement::from_string(DbBackend::Sqlite, sql))
            .await
            .map_aster_err(AsterError::database_operation)?;
        for row in rows {
            let id: i64 = row
                .try_get("", "id")
                .map_aster_err(AsterError::database_operation)?;
            let referenced_table: String = row
                .try_get("", "table")
                .map_aster_err(AsterError::database_operation)?;
            let column: String = row
                .try_get("", "from")
                .map_aster_err(AsterError::database_operation)?;
            let referenced_column: String = row
                .try_get("", "to")
                .map_aster_err(AsterError::database_operation)?;
            foreign_keys.push(ForeignKey {
                table: plan.name.clone(),
                name: format!("{}_fk_{}", plan.name, id),
                column,
                referenced_table,
                referenced_column,
            });
        }
    }
    Ok(foreign_keys)
}

async fn count_foreign_key_violations<C>(
    db: &C,
    backend: DbBackend,
    foreign_key: &ForeignKey,
) -> Result<i64>
where
    C: ConnectionTrait,
{
    let child_table = quote_ident(backend, &foreign_key.table);
    let child_column = quote_ident(backend, &foreign_key.column);
    let parent_table = quote_ident(backend, &foreign_key.referenced_table);
    let parent_column = quote_ident(backend, &foreign_key.referenced_column);
    let sql = format!(
        "SELECT COUNT(*) \
         FROM {child_table} child \
         LEFT JOIN {parent_table} parent \
           ON child.{child_column} = parent.{parent_column} \
         WHERE child.{child_column} IS NOT NULL \
           AND parent.{parent_column} IS NULL"
    );
    scalar_i64(db, backend, &sql).await
}
