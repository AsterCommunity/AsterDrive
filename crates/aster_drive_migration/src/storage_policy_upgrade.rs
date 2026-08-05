//! 0.5.0 startup-only storage policy schema finalization.
//!
//! The historical connector-envelope migration must retain the legacy policy
//! columns long enough for the application-level credential importer to read
//! and encrypt them. Once that importer commits, the runtime calls this
//! idempotent finalizer before the server starts accepting requests. The
//! legacy columns and their indexes are removed here rather than being put
//! back into the production SeaORM entity.
//!
//! This module is specific to the AsterDrive 0.5.0 upgrade path and is
//! scheduled for removal with the legacy schema in 0.6.0.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, DbBackend};

const STORAGE_POLICIES: &str = "storage_policies";
const LEGACY_INDEXES: &[&str] = &[
    "idx_storage_policies_remote_target",
    "idx_storage_policies_remote_node_id",
];
const LEGACY_COLUMNS: &[&str] = &[
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
const REMOTE_NODE_FOREIGN_KEY: &str = "fk_storage_policies_remote_node_id";

/// Remove the legacy `storage_policies` columns after the 0.5.0 credential
/// import has committed. Calling this more than once is safe, which matters if
/// startup is interrupted after some DDL statements have completed.
pub async fn finalize_storage_policy_upgrade<'c, C>(database: C) -> Result<(), DbErr>
where
    C: IntoSchemaManagerConnection<'c>,
{
    let manager = SchemaManager::new(database);
    let database = manager.get_connection();
    if !manager.has_table(STORAGE_POLICIES).await? {
        return Ok(());
    }

    if manager
        .has_column(STORAGE_POLICIES, "remote_node_id")
        .await?
        && database.get_database_backend() != DbBackend::Sqlite
        && foreign_key_exists(database).await?
    {
        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .name(REMOTE_NODE_FOREIGN_KEY)
                    .table(Alias::new(STORAGE_POLICIES))
                    .to_owned(),
            )
            .await?;
    }

    // MySQL requires the foreign key to be removed before its supporting index.
    for index in LEGACY_INDEXES {
        if manager.has_index(STORAGE_POLICIES, *index).await? {
            manager
                .drop_index(
                    Index::drop()
                        .name(*index)
                        .table(Alias::new(STORAGE_POLICIES))
                        .to_owned(),
                )
                .await?;
        }
    }

    for column in LEGACY_COLUMNS {
        if manager.has_column(STORAGE_POLICIES, *column).await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new(STORAGE_POLICIES))
                        .drop_column(Alias::new(*column))
                        .to_owned(),
                )
                .await?;
        }
    }

    Ok(())
}

async fn foreign_key_exists<C>(database: &C) -> Result<bool, DbErr>
where
    C: ConnectionTrait,
{
    use sea_orm_migration::sea_orm::{DbBackend, Statement};

    let statement = match database.get_database_backend() {
        DbBackend::Postgres => Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT COUNT(*) FROM information_schema.table_constraints WHERE table_schema = current_schema() AND table_name = $1 AND constraint_name = $2",
            [STORAGE_POLICIES.into(), REMOTE_NODE_FOREIGN_KEY.into()],
        ),
        DbBackend::MySql => Statement::from_sql_and_values(
            DbBackend::MySql,
            "SELECT COUNT(*) FROM information_schema.table_constraints WHERE constraint_schema = DATABASE() AND table_name = ? AND constraint_name = ?",
            [STORAGE_POLICIES.into(), REMOTE_NODE_FOREIGN_KEY.into()],
        ),
        backend => {
            return Err(DbErr::BackendNotSupported {
                db: backend.as_str(),
                ctx: "storage policy legacy foreign-key finalization",
            });
        }
    };

    let row = database
        .query_one_raw(statement)
        .await?
        .ok_or_else(|| DbErr::Custom("foreign-key metadata query returned no row".to_string()))?;
    let count: i64 = row.try_get_by_index(0)?;
    Ok(count > 0)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use sea_orm_migration::sea_orm::{
        ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement, TransactionTrait,
    };

    use super::*;
    use crate::{Migrator, with_database_migration_lock};

    const CURRENT_COLUMNS: &[&str] = &[
        "id",
        "name",
        "connector_id",
        "storage_config",
        "max_file_size",
        "allowed_types",
        "is_default",
        "chunk_size",
        "created_at",
        "updated_at",
    ];

    async fn migrated_database() -> DatabaseConnection {
        let database = Database::connect("sqlite::memory:")
            .await
            .expect("storage policy finalizer test database should connect");
        Migrator::up(&database, None)
            .await
            .expect("storage policy finalizer test schema should migrate");
        database
    }

    async fn storage_policy_columns<C>(database: &C) -> BTreeSet<String>
    where
        C: ConnectionTrait,
    {
        database
            .query_all_raw(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA table_info('storage_policies')".to_string(),
            ))
            .await
            .expect("storage policy columns should be readable")
            .into_iter()
            .map(|row| {
                row.try_get_by_index(1)
                    .expect("SQLite table_info row should contain a column name")
            })
            .collect()
    }

    async fn insert_legacy_policy(database: &DatabaseConnection) {
        database
            .execute_unprepared(
                r#"INSERT INTO storage_policies (
                    id, name, driver_type, endpoint, bucket, access_key, secret_key,
                    base_path, remote_node_id, remote_storage_target_key, max_file_size,
                    allowed_types, options, is_default, chunk_size, created_at, updated_at,
                    connector_id, storage_config
                ) VALUES (
                    41, 'preserved-policy', 'local', '', '', '', '', './data/uploads',
                    NULL, NULL, 0, '[]', '{}', 1, 5242880,
                    '2026-08-04T00:00:00Z', '2026-08-04T00:00:00Z',
                    'asterdrive.storage.local',
                    '{"format_version":1,"connector":{"format_version":1,"connector_id":"asterdrive.storage.local","schema_version":1,"values":{"base_path":"./data/uploads","content_dedup":false}},"behavior":{"format_version":1,"schema_version":1,"values":{}}}'
                )"#,
            )
            .await
            .expect("legacy storage policy fixture should insert");
    }

    fn expected_current_columns() -> BTreeSet<String> {
        CURRENT_COLUMNS
            .iter()
            .map(|column| (*column).to_string())
            .collect()
    }

    #[tokio::test]
    async fn removes_all_legacy_columns_and_preserves_current_policy_data() {
        let database = migrated_database().await;
        insert_legacy_policy(&database).await;

        finalize_storage_policy_upgrade(&database)
            .await
            .expect("complete storage policy schema should finalize");

        assert_eq!(
            storage_policy_columns(&database).await,
            expected_current_columns()
        );
        let row = database
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT name, connector_id, storage_config, is_default FROM storage_policies WHERE id = 41"
                    .to_string(),
            ))
            .await
            .expect("preserved storage policy should be queryable")
            .expect("preserved storage policy should still exist");
        assert_eq!(
            row.try_get_by_index::<String>(0).unwrap(),
            "preserved-policy"
        );
        assert_eq!(
            row.try_get_by_index::<String>(1).unwrap(),
            "asterdrive.storage.local"
        );
        assert!(
            row.try_get_by_index::<String>(2)
                .unwrap()
                .contains("./data/uploads")
        );
        assert!(row.try_get_by_index::<bool>(3).unwrap());
    }

    #[tokio::test]
    async fn finalized_schema_accepts_current_storage_policy_insert_and_is_idempotent() {
        let database = migrated_database().await;
        finalize_storage_policy_upgrade(&database).await.unwrap();
        finalize_storage_policy_upgrade(&database).await.unwrap();

        database
            .execute_unprepared(
                r#"INSERT INTO storage_policies (
                    name, connector_id, storage_config, max_file_size, allowed_types,
                    is_default, chunk_size, created_at, updated_at
                ) VALUES (
                    'current-policy', 'asterdrive.storage.local',
                    '{"format_version":1,"connector":{"format_version":1,"connector_id":"asterdrive.storage.local","schema_version":1,"values":{"base_path":"./data/uploads","content_dedup":false}},"behavior":{"format_version":1,"schema_version":1,"values":{}}}',
                    0, '[]', 0, 5242880, '2026-08-04T00:00:00Z', '2026-08-04T00:00:00Z'
                )"#,
            )
            .await
            .expect("current storage policy shape should insert after finalization");
        let count = database
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) FROM storage_policies WHERE name = 'current-policy'".to_string(),
            ))
            .await
            .unwrap()
            .unwrap()
            .try_get_by_index::<i64>(0)
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn resumes_after_indexes_and_some_legacy_columns_were_already_removed() {
        let database = migrated_database().await;
        database
            .execute_unprepared("DROP INDEX idx_storage_policies_remote_target")
            .await
            .unwrap();
        database
            .execute_unprepared("ALTER TABLE storage_policies DROP COLUMN endpoint")
            .await
            .unwrap();
        database
            .execute_unprepared("ALTER TABLE storage_policies DROP COLUMN bucket")
            .await
            .unwrap();

        finalize_storage_policy_upgrade(&database)
            .await
            .expect("partially finalized schema should resume");

        assert_eq!(
            storage_policy_columns(&database).await,
            expected_current_columns()
        );
    }

    #[tokio::test]
    async fn missing_storage_policy_table_is_a_noop() {
        let database = Database::connect("sqlite::memory:").await.unwrap();
        finalize_storage_policy_upgrade(&database)
            .await
            .expect("missing storage policy table should not require cleanup");
    }

    #[tokio::test]
    async fn migration_lock_rolls_back_finalization_when_later_upgrade_work_fails() {
        let database = migrated_database().await;
        let columns_before = storage_policy_columns(&database).await;

        let error = with_database_migration_lock(&database, |connection| {
            Box::pin(async {
                finalize_storage_policy_upgrade(connection).await?;
                Err::<(), _>(DbErr::Custom(
                    "synthetic post-finalization failure".to_string(),
                ))
            })
        })
        .await
        .expect_err("upgrade callback failure should abort the migration transaction");

        assert!(
            error
                .to_string()
                .contains("synthetic post-finalization failure")
        );
        assert_eq!(storage_policy_columns(&database).await, columns_before);
    }

    #[tokio::test]
    async fn finalization_does_not_modify_unrelated_tables() {
        let database = migrated_database().await;
        database
            .execute_unprepared(
                "CREATE TABLE storage_upgrade_marker (id INTEGER PRIMARY KEY, value TEXT NOT NULL)",
            )
            .await
            .unwrap();
        database
            .execute_unprepared("INSERT INTO storage_upgrade_marker (id, value) VALUES (1, 'keep')")
            .await
            .unwrap();

        finalize_storage_policy_upgrade(&database).await.unwrap();

        let value = database
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT value FROM storage_upgrade_marker WHERE id = 1".to_string(),
            ))
            .await
            .unwrap()
            .unwrap()
            .try_get_by_index::<String>(0)
            .unwrap();
        assert_eq!(value, "keep");
    }

    #[tokio::test]
    async fn finalizer_accepts_the_transaction_connection_used_by_startup() {
        let database = migrated_database().await;
        let transaction = database.begin().await.unwrap();
        finalize_storage_policy_upgrade(&transaction).await.unwrap();
        transaction.commit().await.unwrap();

        assert_eq!(
            storage_policy_columns(&database).await,
            expected_current_columns()
        );
    }
}
