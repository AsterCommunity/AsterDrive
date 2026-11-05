//! Retire expired placement and remote-target compatibility without discarding live state.
//!
//! Upgrade all writers together. Pre-placement sessions must be drained on the
//! previous release, and remote targets must already have encrypted connector state.
use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, DatabaseExecutor, DbBackend, Statement};

#[derive(DeriveMigrationName)]
pub struct Migration;

const REMOTE_LEGACY_COLUMNS: &[&str] = &[
    "driver_type",
    "endpoint",
    "bucket",
    "access_key",
    "secret_key",
    "base_path",
];
const OLD_RULE_DESCRIPTION: &str =
    "Materialized from storage_policy_group_items; TODO remove in 0.6.0";
const RULE_DESCRIPTION: &str = "Imported placement rule.";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Finish every data-dependent rejection before any backend starts DDL.
        preflight(manager).await?;
        if manager.get_database_backend() == DbBackend::Sqlite {
            return cleanup_sqlite(manager).await;
        }
        for column in ["placement_profile_id", "placement_revision"] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new("upload_sessions"))
                        .modify_column(ColumnDef::new(Alias::new(column)).big_integer().not_null())
                        .to_owned(),
                )
                .await?;
        }
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("storage_policy_group_items"))
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        for column in REMOTE_LEGACY_COLUMNS {
            if manager
                .has_column("remote_storage_targets", *column)
                .await?
            {
                manager
                    .alter_table(
                        Table::alter()
                            .table(Alias::new("remote_storage_targets"))
                            .drop_column(Alias::new(*column))
                            .to_owned(),
                    )
                    .await?;
            }
        }
        let update = Query::update()
            .table(Alias::new("storage_policy_group_rules"))
            .value(Alias::new("description"), RULE_DESCRIPTION)
            .and_where(Expr::col(Alias::new("description")).eq(OLD_RULE_DESCRIPTION))
            .to_owned();
        manager.get_connection().execute(&update).await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Migration(
            "expired storage compatibility cleanup is irreversible; restore a matching database backup to downgrade".into(),
        ))
    }
}

async fn preflight(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    for (sql, guidance) in [
        (
            "SELECT COUNT(*) FROM upload_sessions WHERE placement_profile_id IS NULL OR placement_revision IS NULL",
            "pre-placement upload sessions remain; drain/finalize/cancel and clean up those sessions on the previous release before upgrading",
        ),
        (
            "SELECT COUNT(*) FROM remote_storage_targets WHERE connector_id IS NULL OR TRIM(connector_id) = '' OR connector_config IS NULL OR TRIM(connector_config) = ''",
            "unconverted remote storage targets remain; start the previous release and complete connector conversion before upgrading",
        ),
    ] {
        let row = connection
            .query_one_raw(Statement::from_string(manager.get_database_backend(), sql))
            .await?
            .ok_or_else(|| DbErr::Migration("compatibility preflight returned no count".into()))?;
        let count: i64 = row.try_get_by_index(0)?;
        if count > 0 {
            return Err(DbErr::Migration(format!("{count} {guidance}")));
        }
    }
    for column in ["access_key", "secret_key"] {
        if manager.has_column("remote_storage_targets", column).await? {
            let sql = format!(
                "SELECT COUNT(*) FROM remote_storage_targets WHERE TRIM(COALESCE({column}, '')) <> ''"
            );
            let row = connection
                .query_one_raw(Statement::from_string(manager.get_database_backend(), sql))
                .await?
                .ok_or_else(|| {
                    DbErr::Migration("remote credential preflight returned no count".into())
                })?;
            if row.try_get_by_index::<i64>(0)? > 0 {
                return Err(DbErr::Migration("remote storage targets still contain legacy plaintext credentials; complete conversion on the previous release before upgrading".into()));
            }
        }
    }
    Ok(())
}

// SQLite cannot change nullability in place. Pin one physical connection so
// foreign_keys OFF, transactional rebuild and restoration cannot hit different
// pool members. Preserve dependent parts, all indexes/triggers, and FK mode.
async fn cleanup_sqlite(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    use sea_orm_migration::sea_orm::sqlx::{self, AssertSqlSafe, Connection};
    let statements = sqlite_cleanup_statements(manager).await?;
    let DatabaseExecutor::Connection(database) = manager.get_connection() else {
        // The product Migrator already disables foreign keys before acquiring
        // its outer migration-lock transaction. Reuse that transaction.
        let connection = manager.get_connection();
        let mode = connection
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA foreign_keys",
            ))
            .await?
            .ok_or_else(|| DbErr::Migration("missing SQLite foreign-key mode".into()))?;
        if mode.try_get_by_index::<i64>(0)? != 0 {
            return Err(DbErr::Migration("SQLite storage cleanup requires foreign keys disabled before its outer transaction".into()));
        }
        for sql in statements {
            connection.execute_unprepared(&sql).await?;
        }
        if !connection
            .query_all_raw(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA foreign_key_check",
            ))
            .await?
            .is_empty()
        {
            return Err(DbErr::Migration(
                "storage compatibility cleanup would violate foreign keys".into(),
            ));
        }
        return Ok(());
    };
    let mut guard = SqliteCleanupConnection {
        connection: database
            .get_sqlite_connection_pool()
            .acquire()
            .await
            .map_err(sqlite_error)?,
        restored: false,
    };
    let connection = &mut guard.connection;
    let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&mut **connection)
        .await
        .map_err(sqlite_error)?;
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut **connection)
        .await
        .map_err(sqlite_error)?;
    let result: Result<(), DbErr> = async {
        let mut transaction = connection.begin().await.map_err(sqlite_error)?;
        for sql in statements {
            // Only quoted schema identifiers, trusted sqlite_schema DDL and
            // SQL built by SeaQuery enter this schema-maintenance plan.
            sqlx::query(AssertSqlSafe(sql))
                .execute(&mut *transaction)
                .await
                .map_err(sqlite_error)?;
        }
        if !sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&mut *transaction)
            .await
            .map_err(sqlite_error)?
            .is_empty()
        {
            return Err(DbErr::Migration(
                "storage compatibility cleanup would violate foreign keys".into(),
            ));
        }
        transaction.commit().await.map_err(sqlite_error)
    }
    .await;
    let restore = sqlx::query(if foreign_keys == 0 {
        "PRAGMA foreign_keys = OFF"
    } else {
        "PRAGMA foreign_keys = ON"
    })
    .execute(&mut **connection)
    .await
    .map_err(sqlite_error);
    guard.restored = restore.is_ok();
    result?;
    restore?;
    Ok(())
}

async fn sqlite_cleanup_statements(manager: &SchemaManager<'_>) -> Result<Vec<String>, DbErr> {
    let connection = manager.get_connection();
    let schema = connection.query_all_raw(Statement::from_string(DbBackend::Sqlite,
        "SELECT sql FROM sqlite_schema WHERE tbl_name = 'upload_sessions' AND type IN ('index', 'trigger') AND sql IS NOT NULL"))
        .await?;
    let columns = connection
        .query_all_raw(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA table_info(upload_sessions)",
        ))
        .await?;
    let names = columns
        .iter()
        .map(|row| row.try_get::<String>("", "name").map(Alias::new))
        .collect::<Result<Vec<_>, _>>()?;
    let select = Query::select()
        .columns(names.clone())
        .from(Alias::new("upload_sessions"))
        .to_owned();
    let copy = Query::insert()
        .into_table(Alias::new("upload_sessions__placement_required"))
        .columns(names)
        .select_from(select)
        .map_err(|error| DbErr::Migration(error.to_string()))?
        .to_string(SqliteQueryBuilder);
    let mut statements = vec![
        SQLITE_UPLOAD_SESSIONS.to_string(),
        copy,
        "DROP TABLE upload_sessions".into(),
        "ALTER TABLE upload_sessions__placement_required RENAME TO upload_sessions".into(),
    ];
    for row in schema {
        statements.push(row.try_get("", "sql")?);
    }
    statements.push("DROP TABLE IF EXISTS storage_policy_group_items".into());
    for column in REMOTE_LEGACY_COLUMNS {
        if manager
            .has_column("remote_storage_targets", *column)
            .await?
        {
            statements.push(
                Table::alter()
                    .table(Alias::new("remote_storage_targets"))
                    .drop_column(Alias::new(*column))
                    .to_string(SqliteQueryBuilder),
            );
        }
    }
    statements.push(
        Query::update()
            .table(Alias::new("storage_policy_group_rules"))
            .value(Alias::new("description"), RULE_DESCRIPTION)
            .and_where(Expr::col(Alias::new("description")).eq(OLD_RULE_DESCRIPTION))
            .to_string(SqliteQueryBuilder),
    );
    Ok(statements)
}

// Cancellation must not return a connection with disabled foreign keys to the pool.
struct SqliteCleanupConnection {
    connection: sea_orm_migration::sea_orm::sqlx::pool::PoolConnection<
        sea_orm_migration::sea_orm::sqlx::Sqlite,
    >,
    restored: bool,
}

impl Drop for SqliteCleanupConnection {
    fn drop(&mut self) {
        if !self.restored {
            self.connection.close_on_drop();
        }
    }
}

fn sqlite_error(error: sea_orm_migration::sea_orm::sqlx::Error) -> DbErr {
    DbErr::Migration(format!(
        "SQLite storage compatibility cleanup failed: {error}"
    ))
}

const SQLITE_UPLOAD_SESSIONS: &str = "CREATE TABLE upload_sessions__placement_required (
    id varchar(36) NOT NULL PRIMARY KEY,
    user_id bigint NOT NULL,
    team_id bigint NULL,
    frontend_client_id varchar(36) NULL,
    filename varchar(255) NOT NULL,
    total_size bigint NOT NULL,
    chunk_size bigint NOT NULL,
    total_chunks integer NOT NULL,
    received_count integer NOT NULL DEFAULT 0,
    folder_id bigint NULL,
    policy_id bigint NOT NULL,
    status varchar(16) NOT NULL DEFAULT 'uploading',
    session_kind varchar(32) NOT NULL,
    object_temp_key text NULL,
    object_multipart_id text NULL,
    provider_session_ciphertext text NULL,
    file_id bigint NULL,
    created_at text NOT NULL,
    expires_at text NOT NULL,
    updated_at text NOT NULL,
    placement_profile_id bigint NOT NULL,
    placement_rule_id bigint NULL,
    placement_revision bigint NOT NULL,
    placement_execution_preference varchar(32) NOT NULL DEFAULT 'automatic',
    mime_type varchar(255) NOT NULL DEFAULT 'application/octet-stream',
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
)";
