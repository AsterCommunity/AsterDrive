//! Finalize the storage-policy schema introduced in AsterDrive 0.5.1.
//!
//! The 0.5.x application migration must have imported every legacy secret
//! before this migration is allowed to remove the old stores.  Checks are
//! deliberately completed before any DDL so a rejected upgrade leaves the
//! schema and data untouched.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, DbBackend, Statement};

const LEGACY_POLICY_COLUMNS: &[&str] = &[
    "driver_type",
    "endpoint",
    "bucket",
    "access_key",
    "secret_key",
    "base_path",
    "remote_node_id",
    "remote_storage_target_key",
    "options",
];

const LEGACY_TABLES: &[&str] = &[
    "storage_policy_credentials",
    "storage_connector_application_configs",
];

const LEGACY_INDEXES: &[&str] = &[
    "idx_storage_policies_remote_target",
    "idx_storage_policies_remote_node_id",
];

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let connection = manager.get_connection();
        ensure_no_unmigrated_credentials(manager).await?;
        drop_legacy_tables(manager).await?;
        drop_legacy_policy_constraints(manager).await?;
        drop_legacy_policy_columns(manager).await?;
        // Keep the final schema explicit: if a future historical branch adds
        // one of these columns again, this migration still removes it.
        let remaining = existing_legacy_columns(manager).await?;
        if !remaining.is_empty() {
            return Err(DbErr::Migration(format!(
                "storage policy legacy columns remain after cleanup: {}",
                remaining.join(", ")
            )));
        }
        // A backend may keep foreign-key metadata after dropping a column;
        // force SQLite to validate the resulting schema before recording the
        // migration.  Other backends validate this during ALTER TABLE.
        if connection.get_database_backend() == DbBackend::Sqlite {
            let violations = connection
                .query_all_raw(Statement::from_string(
                    DbBackend::Sqlite,
                    "PRAGMA foreign_key_check",
                ))
                .await?;
            if !violations.is_empty() {
                return Err(DbErr::Migration(format!(
                    "storage policy legacy cleanup left {} foreign-key violation(s)",
                    violations.len()
                )));
            }
        }
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Historical compatibility tables and columns are intentionally not
        // recreated.  They contained secrets and have no safe downgrade.
        Ok(())
    }
}

async fn ensure_no_unmigrated_credentials(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    let backend = manager.get_database_backend();

    for table in LEGACY_TABLES {
        if !manager.has_table(*table).await? {
            continue;
        }
        let count = count_rows(connection, backend, table).await?;
        if count > 0 {
            return Err(DbErr::Migration(format!(
                "storage policy legacy table {table} contains {count} row(s); start the database successfully on AsterDrive 0.5.0 before upgrading to 0.5.1"
            )));
        }
    }

    if !manager.has_table("storage_policies").await? {
        return Ok(());
    }
    let columns = existing_legacy_columns(manager).await?;
    let static_columns = columns
        .iter()
        .filter(|column| **column == "access_key" || **column == "secret_key")
        .copied()
        .collect::<Vec<_>>();
    if static_columns.is_empty() {
        return Ok(());
    }
    let predicates = static_columns
        .iter()
        .map(|column| format!("TRIM(COALESCE({column}, '')) <> ''"))
        .collect::<Vec<_>>();
    let sql = format!(
        "SELECT COUNT(*) FROM storage_policies WHERE {}",
        predicates.join(" OR ")
    );
    let count = scalar_count(connection, backend, &sql).await?;
    if count > 0 {
        return Err(DbErr::Migration(format!(
            "storage_policies contains {count} unmigrated static credential row(s); start the database successfully on AsterDrive 0.5.0 before upgrading to 0.5.1"
        )));
    }
    Ok(())
}

async fn existing_legacy_columns(manager: &SchemaManager<'_>) -> Result<Vec<&'static str>, DbErr> {
    let mut columns = Vec::new();
    for column in LEGACY_POLICY_COLUMNS {
        if manager.has_column("storage_policies", *column).await? {
            columns.push(*column);
        }
    }
    Ok(columns)
}

async fn drop_legacy_tables(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for table in LEGACY_TABLES {
        if manager.has_table(*table).await? {
            manager
                .drop_table(Table::drop().table(Alias::new(*table)).to_owned())
                .await?;
        }
    }
    Ok(())
}

async fn drop_legacy_policy_constraints(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if !manager.has_table("storage_policies").await? {
        return Ok(());
    }
    let backend = manager.get_database_backend();
    let connection = manager.get_connection();
    let mysql_constraint_present = if backend == DbBackend::MySql {
        mysql_constraint_exists(connection, "fk_storage_policies_remote_node_id").await?
    } else {
        false
    };
    match backend {
        DbBackend::Sqlite => {}
        DbBackend::Postgres => {
            connection
                .execute_unprepared("ALTER TABLE \"storage_policies\" DROP CONSTRAINT IF EXISTS \"fk_storage_policies_remote_node_id\"")
                .await?;
        }
        DbBackend::MySql if mysql_constraint_present => {
            connection
                .execute_unprepared("ALTER TABLE `storage_policies` DROP FOREIGN KEY `fk_storage_policies_remote_node_id`")
                .await?;
        }
        DbBackend::MySql => {}
        _ => {}
    }
    for index in LEGACY_INDEXES {
        drop_index_if_present(manager, index).await?;
    }
    Ok(())
}

async fn drop_legacy_policy_columns(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if !manager.has_table("storage_policies").await? {
        return Ok(());
    }
    let sqlite_foreign_keys = if manager.get_database_backend() == DbBackend::Sqlite {
        let row = manager
            .get_connection()
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA foreign_keys",
            ))
            .await?
            .ok_or_else(|| DbErr::Migration("SQLite foreign-key status returned no row".into()))?;
        let enabled = row.try_get_by_index::<i32>(0)? != 0;
        if enabled {
            manager
                .get_connection()
                .execute_unprepared("PRAGMA foreign_keys = OFF")
                .await?;
        }
        Some(enabled)
    } else {
        None
    };
    let result = async {
        for column in LEGACY_POLICY_COLUMNS {
            if manager.has_column("storage_policies", *column).await? {
                manager
                    .alter_table(
                        Table::alter()
                            .table(Alias::new("storage_policies"))
                            .drop_column(Alias::new(*column))
                            .to_owned(),
                    )
                    .await?;
            }
        }
        Ok::<(), DbErr>(())
    }
    .await;
    if sqlite_foreign_keys == Some(true) {
        manager
            .get_connection()
            .execute_unprepared("PRAGMA foreign_keys = ON")
            .await?;
    }
    result
}

async fn drop_index_if_present(manager: &SchemaManager<'_>, index: &str) -> Result<(), DbErr> {
    let backend = manager.get_database_backend();
    let sql = match backend {
        DbBackend::Sqlite => format!("DROP INDEX IF EXISTS \"{index}\""),
        DbBackend::Postgres => format!("DROP INDEX IF EXISTS \"{index}\""),
        DbBackend::MySql => {
            if !mysql_index_exists(manager.get_connection(), index).await? {
                return Ok(());
            }
            format!("DROP INDEX `{index}` ON `storage_policies`")
        }
        _ => return Ok(()),
    };
    manager.get_connection().execute_unprepared(&sql).await?;
    Ok(())
}

async fn count_rows<C: ConnectionTrait>(
    connection: &C,
    backend: DbBackend,
    table: &str,
) -> Result<i64, DbErr> {
    scalar_count(
        connection,
        backend,
        &format!("SELECT COUNT(*) FROM {table}"),
    )
    .await
}

async fn scalar_count<C: ConnectionTrait>(
    connection: &C,
    backend: DbBackend,
    sql: &str,
) -> Result<i64, DbErr> {
    let row = connection
        .query_one_raw(Statement::from_string(backend, sql.to_owned()))
        .await?
        .ok_or_else(|| DbErr::Migration(format!("count query returned no row: {sql}")))?;
    row.try_get_by_index(0)
}

async fn mysql_index_exists<C: ConnectionTrait>(
    connection: &C,
    index: &str,
) -> Result<bool, DbErr> {
    let sql = format!(
        "SELECT COUNT(*) FROM information_schema.statistics WHERE table_schema = DATABASE() AND table_name = 'storage_policies' AND index_name = '{index}'"
    );
    Ok(scalar_count(connection, DbBackend::MySql, &sql).await? > 0)
}

async fn mysql_constraint_exists<C: ConnectionTrait>(
    connection: &C,
    constraint: &str,
) -> Result<bool, DbErr> {
    let sql = format!(
        "SELECT COUNT(*) FROM information_schema.table_constraints WHERE table_schema = DATABASE() AND table_name = 'storage_policies' AND constraint_name = '{constraint}'"
    );
    Ok(scalar_count(connection, DbBackend::MySql, &sql).await? > 0)
}
