//! Split file/folder personal ownership from creator provenance.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, DbBackend, Statement};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        match manager.get_database_backend() {
            DbBackend::Sqlite => migrate_sqlite(manager).await,
            DbBackend::Postgres | DbBackend::MySql => migrate_sql_backend(manager).await,
            backend => Err(DbErr::Migration(format!(
                "unsupported database backend for owner/provenance split: {backend:?}"
            ))),
        }
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Migration(
            "owner/provenance split is not safely reversible because creator users may have been deleted"
                .to_string(),
        ))
    }
}

async fn migrate_sql_backend(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let files_have_user_id = column_exists(manager, "files", "user_id").await?;
    let folders_have_user_id = column_exists(manager, "folders", "user_id").await?;
    if !files_have_user_id && !folders_have_user_id {
        ensure_split_columns_present(manager, "folders").await?;
        ensure_split_columns_present(manager, "files").await?;
        finalize_sql_backend_split_schema(manager).await?;
        return Ok(());
    }

    for (table, has_user_id) in [
        ("folders", folders_have_user_id),
        ("files", files_have_user_id),
    ] {
        if has_user_id {
            add_split_columns_if_missing(manager, table).await?;
        } else {
            ensure_split_columns_present(manager, table).await?;
        }
    }

    if folders_have_user_id {
        backfill_table(manager, "folders").await?;
    }
    if files_have_user_id {
        backfill_table(manager, "files").await?;
    }

    for (table, column) in [("folders", "user_id"), ("files", "user_id")] {
        if column_exists(manager, table, column).await? {
            drop_foreign_keys_for_column(manager, table, column).await?;
        }
    }

    drop_legacy_sql_indexes(manager, files_have_user_id, folders_have_user_id).await?;

    if folders_have_user_id && column_exists(manager, "folders", "user_id").await? {
        exec(manager, "ALTER TABLE folders DROP COLUMN user_id").await?;
    }
    if files_have_user_id && column_exists(manager, "files", "user_id").await? {
        exec(manager, "ALTER TABLE files DROP COLUMN user_id").await?;
    }

    finalize_sql_backend_split_schema(manager).await?;
    Ok(())
}

async fn finalize_sql_backend_split_schema(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    create_shared_indexes(manager).await?;
    create_live_name_unique_indexes(manager).await?;
    add_split_foreign_keys(manager).await?;

    Ok(())
}

async fn migrate_sqlite(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let files_have_user_id = column_exists(manager, "files", "user_id").await?;
    let folders_have_user_id = column_exists(manager, "folders", "user_id").await?;
    if !files_have_user_id && !folders_have_user_id {
        ensure_split_columns_present(manager, "folders").await?;
        ensure_split_columns_present(manager, "files").await?;
        finalize_sqlite_split_schema(manager).await?;
        return Ok(());
    }

    exec(manager, "PRAGMA foreign_keys=OFF").await?;
    let migration_result =
        migrate_sqlite_with_foreign_keys_off(manager, files_have_user_id, folders_have_user_id)
            .await;
    let reenabling_result = exec(manager, "PRAGMA foreign_keys=ON").await;
    migration_result?;
    reenabling_result?;

    Ok(())
}

async fn migrate_sqlite_with_foreign_keys_off(
    manager: &SchemaManager<'_>,
    files_have_user_id: bool,
    folders_have_user_id: bool,
) -> Result<(), DbErr> {
    for statement in [
        "DROP TRIGGER IF EXISTS trg_files_name_fts_ai",
        "DROP TRIGGER IF EXISTS trg_files_name_fts_ad",
        "DROP TRIGGER IF EXISTS trg_files_name_fts_au",
        "DROP TRIGGER IF EXISTS trg_folders_name_fts_ai",
        "DROP TRIGGER IF EXISTS trg_folders_name_fts_ad",
        "DROP TRIGGER IF EXISTS trg_folders_name_fts_au",
        "DROP INDEX IF EXISTS idx_files_unique_live_name",
        "DROP INDEX IF EXISTS idx_folders_unique_live_name",
        "DROP INDEX IF EXISTS idx_folders_user_deleted_parent_name",
        "DROP INDEX IF EXISTS idx_files_user_deleted_folder_name",
        "DROP INDEX IF EXISTS idx_folders_user_deleted_at_id",
        "DROP INDEX IF EXISTS idx_files_user_deleted_at_id",
        "DROP INDEX IF EXISTS idx_files_team_id",
        "DROP INDEX IF EXISTS idx_files_team_deleted_folder_name",
        "DROP INDEX IF EXISTS idx_folders_team_id",
        "DROP INDEX IF EXISTS idx_folders_team_deleted_parent_name",
        "DROP INDEX IF EXISTS idx_files_blob_id",
    ] {
        exec(manager, statement).await?;
    }

    if folders_have_user_id {
        rebuild_sqlite_folders(manager).await?;
    } else {
        ensure_split_columns_present(manager, "folders").await?;
    }

    if files_have_user_id {
        rebuild_sqlite_files(manager).await?;
    } else {
        ensure_split_columns_present(manager, "files").await?;
    }

    finalize_sqlite_split_schema(manager).await
}

async fn finalize_sqlite_split_schema(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    create_shared_indexes(manager).await?;
    create_sqlite_scope_indexes(manager).await?;
    create_live_name_unique_indexes(manager).await?;
    recreate_sqlite_name_fts(manager).await?;

    Ok(())
}

async fn rebuild_sqlite_folders(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    exec(
        manager,
        "CREATE TABLE folders_new ( \
            id integer NOT NULL PRIMARY KEY AUTOINCREMENT, \
            name varchar(255) NOT NULL, \
            parent_id bigint NULL, \
            team_id bigint NULL, \
            owner_user_id bigint NULL, \
            created_by_user_id bigint NULL, \
            created_by_username varchar(255) NOT NULL DEFAULT '', \
            policy_id bigint NULL, \
            created_at timestamp_with_timezone_text NOT NULL, \
            updated_at timestamp_with_timezone_text NOT NULL, \
            deleted_at timestamp_with_timezone_text NULL, \
            is_locked boolean NOT NULL DEFAULT false, \
            FOREIGN KEY (owner_user_id) REFERENCES users (id) ON DELETE SET NULL, \
            FOREIGN KEY (created_by_user_id) REFERENCES users (id) ON DELETE SET NULL, \
            FOREIGN KEY (policy_id) REFERENCES storage_policies (id) ON DELETE SET NULL, \
            FOREIGN KEY (parent_id) REFERENCES folders (id) ON DELETE SET NULL \
        )",
    )
    .await?;
    exec(
        manager,
        "INSERT INTO folders_new ( \
            id, name, parent_id, team_id, owner_user_id, created_by_user_id, \
            created_by_username, policy_id, created_at, updated_at, deleted_at, is_locked \
        ) \
        SELECT \
            folders.id, folders.name, folders.parent_id, folders.team_id, \
            CASE WHEN folders.team_id IS NULL THEN folders.user_id ELSE NULL END, \
            folders.user_id, \
            COALESCE(users.username, ''), \
            folders.policy_id, folders.created_at, folders.updated_at, \
            folders.deleted_at, folders.is_locked \
        FROM folders \
        LEFT JOIN users ON users.id = folders.user_id",
    )
    .await?;
    exec(manager, "DROP TABLE folders").await?;
    exec(manager, "ALTER TABLE folders_new RENAME TO folders").await
}

async fn rebuild_sqlite_files(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    exec(
        manager,
        "CREATE TABLE files_new ( \
            id integer NOT NULL PRIMARY KEY AUTOINCREMENT, \
            name varchar(255) NOT NULL, \
            folder_id bigint NULL, \
            team_id bigint NULL, \
            blob_id bigint NOT NULL, \
            size bigint NOT NULL DEFAULT 0, \
            owner_user_id bigint NULL, \
            created_by_user_id bigint NULL, \
            created_by_username varchar(255) NOT NULL DEFAULT '', \
            mime_type varchar(128) NOT NULL, \
            created_at timestamp_with_timezone_text NOT NULL, \
            updated_at timestamp_with_timezone_text NOT NULL, \
            deleted_at timestamp_with_timezone_text NULL, \
            is_locked boolean NOT NULL DEFAULT false, \
            FOREIGN KEY (folder_id) REFERENCES folders (id) ON DELETE SET NULL, \
            FOREIGN KEY (blob_id) REFERENCES file_blobs (id) ON DELETE RESTRICT, \
            FOREIGN KEY (owner_user_id) REFERENCES users (id) ON DELETE SET NULL, \
            FOREIGN KEY (created_by_user_id) REFERENCES users (id) ON DELETE SET NULL \
        )",
    )
    .await?;
    exec(
        manager,
        "INSERT INTO files_new ( \
            id, name, folder_id, team_id, blob_id, size, owner_user_id, \
            created_by_user_id, created_by_username, mime_type, created_at, updated_at, \
            deleted_at, is_locked \
        ) \
        SELECT \
            files.id, files.name, files.folder_id, files.team_id, files.blob_id, files.size, \
            CASE WHEN files.team_id IS NULL THEN files.user_id ELSE NULL END, \
            files.user_id, \
            COALESCE(users.username, ''), \
            files.mime_type, files.created_at, files.updated_at, files.deleted_at, files.is_locked \
        FROM files \
        LEFT JOIN users ON users.id = files.user_id",
    )
    .await?;
    exec(manager, "DROP TABLE files").await?;
    exec(manager, "ALTER TABLE files_new RENAME TO files").await
}

async fn add_split_columns_if_missing(
    manager: &SchemaManager<'_>,
    table: &str,
) -> Result<(), DbErr> {
    add_column_if_missing(manager, table, "owner_user_id", "BIGINT NULL").await?;
    add_column_if_missing(manager, table, "created_by_user_id", "BIGINT NULL").await?;
    add_column_if_missing(
        manager,
        table,
        "created_by_username",
        "VARCHAR(255) NOT NULL DEFAULT ''",
    )
    .await
}

async fn ensure_split_columns_present(
    manager: &SchemaManager<'_>,
    table: &str,
) -> Result<(), DbErr> {
    for column in ["owner_user_id", "created_by_user_id", "created_by_username"] {
        if !column_exists(manager, table, column).await? {
            return Err(DbErr::Migration(format!(
                "{table}.{column} is missing after user_id was removed; cannot safely resume owner/provenance split"
            )));
        }
    }
    Ok(())
}

async fn backfill_table(manager: &SchemaManager<'_>, table: &str) -> Result<(), DbErr> {
    exec(
        manager,
        &format!(
            "UPDATE {table} \
             SET owner_user_id = CASE WHEN team_id IS NULL THEN user_id ELSE NULL END, \
                 created_by_user_id = user_id, \
                 created_by_username = COALESCE((SELECT username FROM users WHERE users.id = {table}.user_id), '')"
        ),
    )
    .await
}

async fn drop_legacy_sql_indexes(
    manager: &SchemaManager<'_>,
    files_have_user_id: bool,
    folders_have_user_id: bool,
) -> Result<(), DbErr> {
    if folders_have_user_id {
        for index in [
            "idx_folders_unique_live_name",
            "idx_folders_user_deleted_parent_name",
            "idx_folders_user_deleted_at_id",
        ] {
            drop_index_if_exists(manager, "folders", index).await?;
        }
    }
    if files_have_user_id {
        for index in [
            "idx_files_unique_live_name",
            "idx_files_user_deleted_folder_name",
            "idx_files_user_deleted_at_id",
        ] {
            drop_index_if_exists(manager, "files", index).await?;
        }
    }
    Ok(())
}

async fn add_split_foreign_keys(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for (table, column, constraint) in [
        ("folders", "owner_user_id", "fk_folders_owner_user_id"),
        (
            "folders",
            "created_by_user_id",
            "fk_folders_created_by_user_id",
        ),
        ("files", "owner_user_id", "fk_files_owner_user_id"),
        ("files", "created_by_user_id", "fk_files_created_by_user_id"),
    ] {
        if !foreign_key_exists(manager, table, constraint).await? {
            exec(
                manager,
                &format!(
                    "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({}) ON DELETE SET NULL",
                    quote_ident(manager.get_database_backend(), table),
                    quote_ident(manager.get_database_backend(), constraint),
                    quote_ident(manager.get_database_backend(), column),
                    quote_ident(manager.get_database_backend(), "users"),
                    quote_ident(manager.get_database_backend(), "id"),
                ),
            )
            .await?;
        }
    }
    Ok(())
}

async fn create_shared_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for (table, index, columns) in [
        (
            "folders",
            "idx_folders_owner_deleted_parent_name",
            "owner_user_id, deleted_at, parent_id, name",
        ),
        (
            "files",
            "idx_files_owner_deleted_folder_name",
            "owner_user_id, deleted_at, folder_id, name",
        ),
        (
            "folders",
            "idx_folders_owner_deleted_at_id",
            "owner_user_id, deleted_at DESC, id ASC",
        ),
        (
            "files",
            "idx_files_owner_deleted_at_id",
            "owner_user_id, deleted_at DESC, id ASC",
        ),
        (
            "folders",
            "idx_folders_created_by_user_id",
            "created_by_user_id",
        ),
        (
            "files",
            "idx_files_created_by_user_id",
            "created_by_user_id",
        ),
    ] {
        create_index_if_missing(manager, table, index, columns).await?;
    }
    Ok(())
}

async fn create_sqlite_scope_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for (table, index, columns) in [
        ("files", "idx_files_team_id", "team_id"),
        (
            "files",
            "idx_files_team_deleted_folder_name",
            "team_id, deleted_at, folder_id, name",
        ),
        ("folders", "idx_folders_team_id", "team_id"),
        (
            "folders",
            "idx_folders_team_deleted_parent_name",
            "team_id, deleted_at, parent_id, name",
        ),
        ("files", "idx_files_blob_id", "blob_id"),
    ] {
        create_index_if_missing(manager, table, index, columns).await?;
    }
    Ok(())
}

async fn create_live_name_unique_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    match manager.get_database_backend() {
        DbBackend::Sqlite | DbBackend::Postgres => {
            create_unique_index_if_missing(
                manager,
                "files",
                "idx_files_unique_live_name",
                "CREATE UNIQUE INDEX idx_files_unique_live_name \
                 ON files ( \
                    (CASE WHEN team_id IS NULL THEN 0 ELSE 1 END), \
                    (CASE WHEN team_id IS NULL THEN owner_user_id ELSE team_id END), \
                    (COALESCE(folder_id, 0)), \
                    name, \
                     (CASE WHEN deleted_at IS NULL THEN 1 ELSE NULL END) \
                 )",
            )
            .await?;
            create_unique_index_if_missing(
                manager,
                "folders",
                "idx_folders_unique_live_name",
                "CREATE UNIQUE INDEX idx_folders_unique_live_name \
                 ON folders ( \
                    (CASE WHEN team_id IS NULL THEN 0 ELSE 1 END), \
                    (CASE WHEN team_id IS NULL THEN owner_user_id ELSE team_id END), \
                    (COALESCE(parent_id, 0)), \
                    name, \
                     (CASE WHEN deleted_at IS NULL THEN 1 ELSE NULL END) \
                 )",
            )
            .await
        }
        DbBackend::MySql => {
            create_unique_index_if_missing(
                manager,
                "files",
                "idx_files_unique_live_name",
                "CREATE UNIQUE INDEX idx_files_unique_live_name \
                 ON files ( \
                    ((CASE WHEN team_id IS NULL THEN 0 ELSE 1 END)), \
                    ((CASE WHEN team_id IS NULL THEN owner_user_id ELSE team_id END)), \
                    ((COALESCE(folder_id, 0))), \
                    name, \
                     ((CASE WHEN deleted_at IS NULL THEN 1 ELSE NULL END)) \
                 )",
            )
            .await?;
            create_unique_index_if_missing(
                manager,
                "folders",
                "idx_folders_unique_live_name",
                "CREATE UNIQUE INDEX idx_folders_unique_live_name \
                 ON folders ( \
                    ((CASE WHEN team_id IS NULL THEN 0 ELSE 1 END)), \
                    ((CASE WHEN team_id IS NULL THEN owner_user_id ELSE team_id END)), \
                    ((COALESCE(parent_id, 0))), \
                    name, \
                    ((CASE WHEN deleted_at IS NULL THEN 1 ELSE NULL END)) \
                 )",
            )
            .await
        }
        backend => Err(DbErr::Migration(format!(
            "unsupported database backend for live-name unique indexes: {backend:?}"
        ))),
    }
}

async fn create_unique_index_if_missing(
    manager: &SchemaManager<'_>,
    table: &str,
    index: &str,
    sql: &str,
) -> Result<(), DbErr> {
    if index_exists(manager, table, index).await? {
        return Ok(());
    }
    exec(manager, sql).await
}

async fn recreate_sqlite_name_fts(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for statement in [
        "CREATE VIRTUAL TABLE IF NOT EXISTS files_name_fts USING fts5(name, tokenize='trigram')",
        "DELETE FROM files_name_fts",
        "INSERT INTO files_name_fts(rowid, name) SELECT id, name FROM files",
        "CREATE TRIGGER IF NOT EXISTS trg_files_name_fts_ai \
         AFTER INSERT ON files BEGIN \
           INSERT INTO files_name_fts(rowid, name) VALUES (new.id, new.name); \
         END",
        "CREATE TRIGGER IF NOT EXISTS trg_files_name_fts_ad \
         AFTER DELETE ON files BEGIN \
           DELETE FROM files_name_fts WHERE rowid = old.id; \
         END",
        "CREATE TRIGGER IF NOT EXISTS trg_files_name_fts_au \
         AFTER UPDATE OF name ON files BEGIN \
           UPDATE files_name_fts SET name = new.name WHERE rowid = new.id; \
         END",
        "CREATE VIRTUAL TABLE IF NOT EXISTS folders_name_fts USING fts5(name, tokenize='trigram')",
        "DELETE FROM folders_name_fts",
        "INSERT INTO folders_name_fts(rowid, name) SELECT id, name FROM folders",
        "CREATE TRIGGER IF NOT EXISTS trg_folders_name_fts_ai \
         AFTER INSERT ON folders BEGIN \
           INSERT INTO folders_name_fts(rowid, name) VALUES (new.id, new.name); \
         END",
        "CREATE TRIGGER IF NOT EXISTS trg_folders_name_fts_ad \
         AFTER DELETE ON folders BEGIN \
           DELETE FROM folders_name_fts WHERE rowid = old.id; \
         END",
        "CREATE TRIGGER IF NOT EXISTS trg_folders_name_fts_au \
         AFTER UPDATE OF name ON folders BEGIN \
           UPDATE folders_name_fts SET name = new.name WHERE rowid = new.id; \
         END",
    ] {
        exec(manager, statement).await?;
    }
    Ok(())
}

async fn add_column_if_missing(
    manager: &SchemaManager<'_>,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), DbErr> {
    if column_exists(manager, table, column).await? {
        return Ok(());
    }
    exec(
        manager,
        &format!(
            "ALTER TABLE {} ADD COLUMN {} {definition}",
            quote_ident(manager.get_database_backend(), table),
            quote_ident(manager.get_database_backend(), column),
        ),
    )
    .await
}

async fn drop_foreign_keys_for_column(
    manager: &SchemaManager<'_>,
    table: &str,
    column: &str,
) -> Result<(), DbErr> {
    let backend = manager.get_database_backend();
    let constraints = foreign_keys_for_column(manager, table, column).await?;
    for constraint in constraints {
        let sql = match backend {
            DbBackend::Postgres => format!(
                "ALTER TABLE {} DROP CONSTRAINT {}",
                quote_ident(backend, table),
                quote_ident(backend, &constraint),
            ),
            DbBackend::MySql => format!(
                "ALTER TABLE {} DROP FOREIGN KEY {}",
                quote_ident(backend, table),
                quote_ident(backend, &constraint),
            ),
            _ => continue,
        };
        exec(manager, &sql).await?;
    }
    Ok(())
}

async fn foreign_keys_for_column(
    manager: &SchemaManager<'_>,
    table: &str,
    column: &str,
) -> Result<Vec<String>, DbErr> {
    let backend = manager.get_database_backend();
    let sql = match backend {
        DbBackend::Postgres => format!(
            "SELECT c.conname \
             FROM pg_constraint c \
             JOIN pg_class t ON t.oid = c.conrelid \
             JOIN pg_namespace n ON n.oid = t.relnamespace \
             JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(c.conkey) \
             WHERE c.contype = 'f' \
               AND n.nspname = current_schema() \
               AND t.relname = {} \
               AND a.attname = {}",
            quote_literal(table),
            quote_literal(column),
        ),
        DbBackend::MySql => format!(
            "SELECT constraint_name \
             FROM information_schema.key_column_usage \
             WHERE table_schema = DATABASE() \
               AND table_name = {} \
               AND column_name = {} \
               AND referenced_table_name IS NOT NULL",
            quote_literal(table),
            quote_literal(column),
        ),
        _ => return Ok(Vec::new()),
    };
    string_rows(manager, &sql).await
}

async fn create_index_if_missing(
    manager: &SchemaManager<'_>,
    table: &str,
    index: &str,
    columns: &str,
) -> Result<(), DbErr> {
    if index_exists(manager, table, index).await? {
        return Ok(());
    }
    exec(
        manager,
        &format!("CREATE INDEX {index} ON {table} ({columns})"),
    )
    .await
}

async fn drop_index_if_exists(
    manager: &SchemaManager<'_>,
    table: &str,
    index: &str,
) -> Result<(), DbErr> {
    if !index_exists(manager, table, index).await? {
        return Ok(());
    }
    let backend = manager.get_database_backend();
    let sql = match backend {
        DbBackend::MySql => format!(
            "DROP INDEX {} ON {}",
            quote_ident(backend, index),
            quote_ident(backend, table),
        ),
        _ => format!("DROP INDEX IF EXISTS {}", quote_ident(backend, index)),
    };
    exec(manager, &sql).await
}

async fn column_exists(
    manager: &SchemaManager<'_>,
    table: &str,
    column: &str,
) -> Result<bool, DbErr> {
    let backend = manager.get_database_backend();
    let sql = match backend {
        DbBackend::Sqlite => format!(
            "SELECT CASE WHEN EXISTS(SELECT 1 FROM pragma_table_info({}) WHERE name = {}) THEN 1 ELSE 0 END",
            quote_literal(table),
            quote_literal(column),
        ),
        DbBackend::Postgres => format!(
            "SELECT CASE WHEN EXISTS(SELECT 1 FROM information_schema.columns \
             WHERE table_schema = current_schema() \
               AND table_name = {} \
               AND column_name = {}) THEN 1 ELSE 0 END",
            quote_literal(table),
            quote_literal(column),
        ),
        DbBackend::MySql => format!(
            "SELECT CASE WHEN EXISTS(SELECT 1 FROM information_schema.columns \
             WHERE table_schema = DATABASE() \
               AND table_name = {} \
               AND column_name = {}) THEN 1 ELSE 0 END",
            quote_literal(table),
            quote_literal(column),
        ),
        backend => {
            return Err(DbErr::Migration(format!(
                "unsupported backend for column inspection: {backend:?}"
            )));
        }
    };
    scalar_bool(manager, &sql).await
}

async fn index_exists(
    manager: &SchemaManager<'_>,
    table: &str,
    index: &str,
) -> Result<bool, DbErr> {
    let backend = manager.get_database_backend();
    let sql = match backend {
        DbBackend::Sqlite => format!(
            "SELECT CASE WHEN EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = {}) THEN 1 ELSE 0 END",
            quote_literal(index),
        ),
        DbBackend::Postgres => format!(
            "SELECT CASE WHEN EXISTS( \
                SELECT 1 \
                FROM pg_class c \
                JOIN pg_namespace n ON n.oid = c.relnamespace \
                WHERE c.relkind = 'i' \
                  AND n.nspname = current_schema() \
                  AND c.relname = {} \
             ) THEN 1 ELSE 0 END",
            quote_literal(index),
        ),
        DbBackend::MySql => format!(
            "SELECT CASE WHEN EXISTS( \
                SELECT 1 FROM information_schema.statistics \
                WHERE table_schema = DATABASE() \
                  AND table_name = {} \
                  AND index_name = {} \
             ) THEN 1 ELSE 0 END",
            quote_literal(table),
            quote_literal(index),
        ),
        backend => {
            return Err(DbErr::Migration(format!(
                "unsupported backend for index inspection: {backend:?}"
            )));
        }
    };
    scalar_bool(manager, &sql).await
}

async fn foreign_key_exists(
    manager: &SchemaManager<'_>,
    table: &str,
    constraint: &str,
) -> Result<bool, DbErr> {
    let backend = manager.get_database_backend();
    let sql = match backend {
        DbBackend::Postgres => format!(
            "SELECT CASE WHEN EXISTS( \
                SELECT 1 \
                FROM pg_constraint c \
                JOIN pg_class t ON t.oid = c.conrelid \
                JOIN pg_namespace n ON n.oid = t.relnamespace \
                WHERE c.contype = 'f' \
                  AND n.nspname = current_schema() \
                  AND t.relname = {} \
                  AND c.conname = {} \
             ) THEN 1 ELSE 0 END",
            quote_literal(table),
            quote_literal(constraint),
        ),
        DbBackend::MySql => format!(
            "SELECT CASE WHEN EXISTS( \
                SELECT 1 FROM information_schema.table_constraints \
                WHERE table_schema = DATABASE() \
                  AND table_name = {} \
                  AND constraint_name = {} \
                  AND constraint_type = 'FOREIGN KEY' \
             ) THEN 1 ELSE 0 END",
            quote_literal(table),
            quote_literal(constraint),
        ),
        _ => return Ok(false),
    };
    scalar_bool(manager, &sql).await
}

async fn scalar_bool(manager: &SchemaManager<'_>, sql: &str) -> Result<bool, DbErr> {
    let backend = manager.get_database_backend();
    let row = manager
        .get_connection()
        .query_one_raw(Statement::from_string(backend, sql.to_string()))
        .await?
        .ok_or_else(|| DbErr::Migration("scalar query returned no rows".to_string()))?;

    if let Ok(value) = row.try_get_by_index::<i64>(0) {
        return Ok(value != 0);
    }
    if let Ok(value) = row.try_get_by_index::<i32>(0) {
        return Ok(value != 0);
    }
    if let Ok(value) = row.try_get_by_index::<bool>(0) {
        return Ok(value);
    }

    Err(DbErr::Migration(
        "failed to decode scalar query result".to_string(),
    ))
}

async fn string_rows(manager: &SchemaManager<'_>, sql: &str) -> Result<Vec<String>, DbErr> {
    let backend = manager.get_database_backend();
    manager
        .get_connection()
        .query_all_raw(Statement::from_string(backend, sql.to_string()))
        .await?
        .into_iter()
        .map(|row| row.try_get_by_index::<String>(0))
        .collect()
}

async fn exec(manager: &SchemaManager<'_>, sql: &str) -> Result<(), DbErr> {
    manager.get_connection().execute_unprepared(sql).await?;
    Ok(())
}

fn quote_ident(backend: DbBackend, ident: &str) -> String {
    match backend {
        DbBackend::MySql => format!("`{}`", ident.replace('`', "``")),
        _ => format!("\"{}\"", ident.replace('"', "\"\"")),
    }
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm_migration::sea_orm::Database;

    #[tokio::test]
    async fn sqlite_migration_preserves_team_resources_and_backfills_provenance() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        db.execute_unprepared("PRAGMA foreign_keys=ON")
            .await
            .unwrap();
        for statement in [
            "CREATE TABLE users (id integer NOT NULL PRIMARY KEY, username varchar(255) NOT NULL)",
            "CREATE TABLE storage_policies (id integer NOT NULL PRIMARY KEY)",
            "CREATE TABLE teams (id integer NOT NULL PRIMARY KEY)",
            "CREATE TABLE file_blobs (id integer NOT NULL PRIMARY KEY, ref_count integer NOT NULL)",
            "CREATE TABLE folders ( \
                id integer NOT NULL PRIMARY KEY, \
                name varchar(255) NOT NULL, \
                parent_id bigint NULL, \
                team_id bigint NULL, \
                user_id bigint NOT NULL, \
                policy_id bigint NULL, \
                created_at timestamp_with_timezone_text NOT NULL, \
                updated_at timestamp_with_timezone_text NOT NULL, \
                deleted_at timestamp_with_timezone_text NULL, \
                is_locked boolean NOT NULL DEFAULT false, \
                FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE \
            )",
            "CREATE TABLE files ( \
                id integer NOT NULL PRIMARY KEY, \
                name varchar(255) NOT NULL, \
                folder_id bigint NULL, \
                team_id bigint NULL, \
                blob_id bigint NOT NULL, \
                size bigint NOT NULL DEFAULT 0, \
                user_id bigint NOT NULL, \
                mime_type varchar(128) NOT NULL, \
                created_at timestamp_with_timezone_text NOT NULL, \
                updated_at timestamp_with_timezone_text NOT NULL, \
                deleted_at timestamp_with_timezone_text NULL, \
                is_locked boolean NOT NULL DEFAULT false, \
                FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE \
            )",
            "CREATE VIRTUAL TABLE files_name_fts USING fts5(name, tokenize='trigram')",
            "CREATE VIRTUAL TABLE folders_name_fts USING fts5(name, tokenize='trigram')",
            "INSERT INTO users (id, username) VALUES (1, 'owner'), (2, 'uploader')",
            "INSERT INTO storage_policies (id) VALUES (1)",
            "INSERT INTO teams (id) VALUES (10)",
            "INSERT INTO file_blobs (id, ref_count) VALUES (100, 1)",
            "INSERT INTO folders (id, name, parent_id, team_id, user_id, policy_id, created_at, updated_at, deleted_at, is_locked) \
             VALUES (200, 'team-folder', NULL, 10, 2, NULL, '2026-05-08T00:00:00Z', '2026-05-08T00:00:00Z', NULL, false)",
            "INSERT INTO files (id, name, folder_id, team_id, blob_id, size, user_id, mime_type, created_at, updated_at, deleted_at, is_locked) \
             VALUES (300, 'team-file.txt', 200, 10, 100, 14, 2, 'text/plain', '2026-05-08T00:00:00Z', '2026-05-08T00:00:00Z', NULL, false)",
        ] {
            db.execute_unprepared(statement).await.unwrap();
        }

        let schema_manager = SchemaManager::new(&db);
        migrate_sqlite(&schema_manager).await.unwrap();
        db.execute_unprepared("DELETE FROM users WHERE id = 2")
            .await
            .unwrap();

        let file_row = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT team_id, owner_user_id, created_by_user_id, created_by_username \
                 FROM files WHERE id = 300"
                    .to_string(),
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(file_row.try_get_by_index::<i64>(0).unwrap(), 10);
        assert!(
            file_row
                .try_get_by_index::<Option<i64>>(1)
                .unwrap()
                .is_none()
        );
        assert!(
            file_row
                .try_get_by_index::<Option<i64>>(2)
                .unwrap()
                .is_none()
        );
        assert_eq!(file_row.try_get_by_index::<String>(3).unwrap(), "uploader");

        let blob_row = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT ref_count FROM file_blobs WHERE id = 100".to_string(),
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(blob_row.try_get_by_index::<i32>(0).unwrap(), 1);

        let indexes = db
            .query_all_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT name FROM sqlite_master \
                 WHERE type = 'index' \
                   AND name IN ('idx_files_owner_deleted_folder_name', 'idx_folders_owner_deleted_parent_name')"
                    .to_string(),
            ))
            .await
            .unwrap()
            .into_iter()
            .map(|row| row.try_get_by_index::<String>(0).unwrap())
            .collect::<Vec<_>>();
        assert!(
            indexes
                .iter()
                .any(|name| name == "idx_files_owner_deleted_folder_name")
        );
        assert!(
            indexes
                .iter()
                .any(|name| name == "idx_folders_owner_deleted_parent_name")
        );
    }

    #[tokio::test]
    async fn sqlite_migration_rerun_restores_indexes_and_fts_triggers_after_partial_resume() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        db.execute_unprepared("PRAGMA foreign_keys=ON")
            .await
            .unwrap();
        for statement in [
            "CREATE TABLE users (id integer NOT NULL PRIMARY KEY, username varchar(255) NOT NULL)",
            "CREATE TABLE storage_policies (id integer NOT NULL PRIMARY KEY)",
            "CREATE TABLE teams (id integer NOT NULL PRIMARY KEY)",
            "CREATE TABLE file_blobs (id integer NOT NULL PRIMARY KEY, ref_count integer NOT NULL)",
            "CREATE TABLE folders ( \
                id integer NOT NULL PRIMARY KEY, \
                name varchar(255) NOT NULL, \
                parent_id bigint NULL, \
                team_id bigint NULL, \
                user_id bigint NOT NULL, \
                policy_id bigint NULL, \
                created_at timestamp_with_timezone_text NOT NULL, \
                updated_at timestamp_with_timezone_text NOT NULL, \
                deleted_at timestamp_with_timezone_text NULL, \
                is_locked boolean NOT NULL DEFAULT false, \
                FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE \
            )",
            "CREATE TABLE files ( \
                id integer NOT NULL PRIMARY KEY, \
                name varchar(255) NOT NULL, \
                folder_id bigint NULL, \
                team_id bigint NULL, \
                blob_id bigint NOT NULL, \
                size bigint NOT NULL DEFAULT 0, \
                user_id bigint NOT NULL, \
                mime_type varchar(128) NOT NULL, \
                created_at timestamp_with_timezone_text NOT NULL, \
                updated_at timestamp_with_timezone_text NOT NULL, \
                deleted_at timestamp_with_timezone_text NULL, \
                is_locked boolean NOT NULL DEFAULT false, \
                FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE \
            )",
            "CREATE VIRTUAL TABLE files_name_fts USING fts5(name, tokenize='trigram')",
            "CREATE VIRTUAL TABLE folders_name_fts USING fts5(name, tokenize='trigram')",
            "INSERT INTO users (id, username) VALUES (1, 'owner')",
            "INSERT INTO storage_policies (id) VALUES (1)",
            "INSERT INTO file_blobs (id, ref_count) VALUES (100, 1)",
            "INSERT INTO folders (id, name, parent_id, team_id, user_id, policy_id, created_at, updated_at, deleted_at, is_locked) \
             VALUES (200, 'docs', NULL, NULL, 1, NULL, '2026-05-08T00:00:00Z', '2026-05-08T00:00:00Z', NULL, false)",
            "INSERT INTO files (id, name, folder_id, team_id, blob_id, size, user_id, mime_type, created_at, updated_at, deleted_at, is_locked) \
             VALUES (300, 'readme.txt', 200, NULL, 100, 6, 1, 'text/plain', '2026-05-08T00:00:00Z', '2026-05-08T00:00:00Z', NULL, false)",
        ] {
            db.execute_unprepared(statement).await.unwrap();
        }

        let schema_manager = SchemaManager::new(&db);
        migrate_sqlite(&schema_manager).await.unwrap();

        for statement in [
            "DROP INDEX idx_files_unique_live_name",
            "DROP INDEX idx_folders_unique_live_name",
            "DROP INDEX idx_files_owner_deleted_folder_name",
            "DROP TRIGGER trg_files_name_fts_ai",
            "DELETE FROM files_name_fts",
        ] {
            db.execute_unprepared(statement).await.unwrap();
        }

        migrate_sqlite(&schema_manager).await.unwrap();

        for (table, index) in [
            ("files", "idx_files_unique_live_name"),
            ("folders", "idx_folders_unique_live_name"),
            ("files", "idx_files_owner_deleted_folder_name"),
        ] {
            assert!(
                index_exists(&schema_manager, table, index).await.unwrap(),
                "expected {index} to be recreated"
            );
        }
        assert!(
            scalar_bool(
                &schema_manager,
                "SELECT CASE WHEN EXISTS( \
                    SELECT 1 FROM sqlite_master \
                    WHERE type = 'trigger' AND name = 'trg_files_name_fts_ai' \
                 ) THEN 1 ELSE 0 END",
            )
            .await
            .unwrap()
        );
        let fts_row_count = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) FROM files_name_fts".to_string(),
            ))
            .await
            .unwrap()
            .unwrap()
            .try_get_by_index::<i64>(0)
            .unwrap();
        assert_eq!(fts_row_count, 1);
    }

    #[tokio::test]
    async fn sqlite_migration_reenables_foreign_keys_after_failure() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        db.execute_unprepared("PRAGMA foreign_keys=ON")
            .await
            .unwrap();
        for statement in [
            "CREATE TABLE users (id integer NOT NULL PRIMARY KEY, username varchar(255) NOT NULL)",
            "CREATE TABLE storage_policies (id integer NOT NULL PRIMARY KEY)",
            "CREATE TABLE folders ( \
                id integer NOT NULL PRIMARY KEY, \
                name varchar(255) NOT NULL, \
                parent_id bigint NULL, \
                team_id bigint NULL, \
                user_id bigint NOT NULL, \
                policy_id bigint NULL, \
                created_at timestamp_with_timezone_text NOT NULL, \
                updated_at timestamp_with_timezone_text NOT NULL, \
                deleted_at timestamp_with_timezone_text NULL, \
                FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE \
            )",
        ] {
            db.execute_unprepared(statement).await.unwrap();
        }

        let schema_manager = SchemaManager::new(&db);
        let error = migrate_sqlite(&schema_manager)
            .await
            .expect_err("invalid old schema should fail during table rebuild");
        assert!(error.to_string().contains("is_locked"));

        let foreign_keys_enabled = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA foreign_keys".to_string(),
            ))
            .await
            .unwrap()
            .unwrap()
            .try_get_by_index::<i64>(0)
            .unwrap();
        assert_eq!(foreign_keys_enabled, 1);
    }
}
