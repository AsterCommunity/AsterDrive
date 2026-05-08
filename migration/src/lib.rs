//! 数据库迁移 crate 入口。
#![deny(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub use sea_orm_migration::prelude::*;

use sea_orm_migration::sea_orm::{
    ConnectionTrait as SeaConnectionTrait, DatabaseConnection, DbBackend, Statement,
    TransactionTrait,
};

mod legacy;
mod m20260502_000001_baseline_schema;
mod m20260508_000001_split_file_folder_owner_provenance;
mod search_acceleration;
mod time;

pub const BASELINE_MIGRATION_NAME: &str = "m20260502_000001_baseline_schema";
pub const MYSQL_UTC_DATETIME_FIX_MIGRATION_NAME: &str =
    legacy::MYSQL_UTC_DATETIME_FIX_MIGRATION_NAME;

const MIGRATION_TABLE: &str = "seaql_migrations";
const APPLICATION_SCHEMA_SENTINELS: &[&str] = &[
    "users",
    "storage_policies",
    "folders",
    "files",
    "system_config",
];

pub struct Migrator;
pub struct CurrentMigrator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationTrack {
    Empty,
    Baseline,
    Alpha25Complete,
    Alpha25Incomplete,
    Mixed,
}

impl MigrationTrack {
    pub fn label(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Baseline => "baseline",
            Self::Alpha25Complete => "alpha25_complete",
            Self::Alpha25Incomplete => "alpha25_incomplete",
            Self::Mixed => "mixed",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MigrationHistory {
    pub track: MigrationTrack,
    pub applied: Vec<String>,
    pub pending_current: Vec<String>,
    pub pending_alpha25: Vec<String>,
    pub unknown_applied: Vec<String>,
}

impl MigrationHistory {
    pub fn effective_pending(&self) -> &[String] {
        match self.track {
            MigrationTrack::Alpha25Incomplete => &self.pending_alpha25,
            MigrationTrack::Empty
            | MigrationTrack::Baseline
            | MigrationTrack::Alpha25Complete
            | MigrationTrack::Mixed => &self.pending_current,
        }
    }

    pub fn has_unknown_applied(&self) -> bool {
        !self.unknown_applied.is_empty()
    }

    pub fn has_inconsistent_baseline_stamp(&self) -> bool {
        self.track == MigrationTrack::Mixed
    }

    pub fn is_alpha25_incomplete(&self) -> bool {
        self.track == MigrationTrack::Alpha25Incomplete
    }
}

impl Migrator {
    pub async fn up(database: &DatabaseConnection, steps: Option<u32>) -> Result<(), DbErr> {
        match steps {
            Some(step_count) => {
                <CurrentMigrator as MigratorTrait>::up(database, Some(step_count)).await
            }
            None => apply_database_migrations(database).await,
        }
    }
}

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        <CurrentMigrator as MigratorTrait>::migrations()
    }
}

#[async_trait::async_trait]
impl MigratorTrait for CurrentMigrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260502_000001_baseline_schema::Migration),
            Box::new(m20260508_000001_split_file_folder_owner_provenance::Migration),
        ]
    }
}

pub fn current_migration_names() -> Vec<String> {
    <CurrentMigrator as MigratorTrait>::migrations()
        .into_iter()
        .map(|migration| migration.name().to_string())
        .collect()
}

pub fn alpha25_migration_names() -> Vec<String> {
    legacy::ALPHA25_MIGRATION_NAMES
        .iter()
        .map(|name| (*name).to_string())
        .collect()
}

pub fn recognized_migration_names() -> Vec<String> {
    let mut names = alpha25_migration_names();
    for current in current_migration_names() {
        if !names.iter().any(|name| name == &current) {
            names.push(current);
        }
    }
    names
}

pub async fn inspect_migration_history<C>(db: &C) -> Result<MigrationHistory, DbErr>
where
    C: SeaConnectionTrait,
{
    let applied = applied_migrations(db, db.get_database_backend()).await?;
    let current_names = current_migration_names();
    let alpha25_names = alpha25_migration_names();

    let applied_lookup = applied
        .iter()
        .map(String::as_str)
        .collect::<std::collections::HashSet<_>>();
    let current_lookup = current_names
        .iter()
        .map(String::as_str)
        .collect::<std::collections::HashSet<_>>();
    let alpha25_lookup = alpha25_names
        .iter()
        .map(String::as_str)
        .collect::<std::collections::HashSet<_>>();

    let unknown_applied = applied
        .iter()
        .filter(|name| {
            !current_lookup.contains(name.as_str()) && !alpha25_lookup.contains(name.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();

    let pending_current = current_names
        .iter()
        .filter(|name| !applied_lookup.contains(name.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let pending_alpha25 = alpha25_names
        .iter()
        .filter(|name| !applied_lookup.contains(name.as_str()))
        .cloned()
        .collect::<Vec<_>>();

    let baseline_applied = applied_lookup.contains(BASELINE_MIGRATION_NAME);
    let has_alpha25_rows = applied
        .iter()
        .any(|name| alpha25_lookup.contains(name.as_str()));

    let track = if applied.is_empty() {
        MigrationTrack::Empty
    } else if baseline_applied {
        if has_alpha25_rows {
            MigrationTrack::Mixed
        } else {
            MigrationTrack::Baseline
        }
    } else if has_alpha25_rows && pending_alpha25.is_empty() {
        MigrationTrack::Alpha25Complete
    } else if has_alpha25_rows {
        MigrationTrack::Alpha25Incomplete
    } else {
        MigrationTrack::Mixed
    };

    Ok(MigrationHistory {
        track,
        applied,
        pending_current,
        pending_alpha25,
        unknown_applied,
    })
}

pub async fn apply_database_migrations(database: &DatabaseConnection) -> Result<(), DbErr> {
    let history = inspect_migration_history(database).await?;
    if history.has_unknown_applied() {
        return Err(migration_state_error(format!(
            "database contains unknown migration versions: {}",
            history.unknown_applied.join(", ")
        )));
    }

    match history.track {
        MigrationTrack::Empty => {
            if migration_table_exists(database).await?
                || application_schema_exists(database).await?
            {
                return Err(migration_state_error(
                    "database contains migration metadata or application tables but migration \
                     history is empty; first upgrade to v0.0.1-alpha.25 and apply all migrations, \
                     then upgrade to this version"
                        .to_string(),
                ));
            }
            <CurrentMigrator as MigratorTrait>::up(database, None).await?;
            Ok(())
        }
        MigrationTrack::Baseline => {
            <CurrentMigrator as MigratorTrait>::up(database, None).await?;
            Ok(())
        }
        MigrationTrack::Alpha25Complete => {
            validate_alpha25_rebase_schema(database).await?;
            rewrite_migration_history_to_baseline(database).await?;
            <CurrentMigrator as MigratorTrait>::up(database, None).await
        }
        MigrationTrack::Alpha25Incomplete => Err(migration_state_error(format!(
            "database migration history is not fully upgraded to v0.0.1-alpha.25; \
             first run v0.0.1-alpha.25 and apply all migrations, then upgrade to this version. \
             missing migrations: {}",
            history.pending_alpha25.join(", ")
        ))),
        MigrationTrack::Mixed => Err(migration_state_error(
            "database migration history mixes rebased baseline and pre-rebase alpha migrations; \
             restore a backup or contact maintainers before continuing"
                .to_string(),
        )),
    }
}

async fn validate_alpha25_rebase_schema(database: &DatabaseConnection) -> Result<(), DbErr> {
    let backend = database.get_database_backend();
    for table_name in [
        "auth_sessions",
        "managed_ingress_profiles",
        "master_bindings",
    ] {
        if !table_exists(database, backend, table_name).await? {
            return Err(rebase_required_error(format!(
                "expected table '{table_name}' is missing"
            )));
        }
    }

    if column_exists(database, backend, "master_bindings", "ingress_policy_id").await? {
        return Err(rebase_required_error(
            "master_bindings.ingress_policy_id still exists".to_string(),
        ));
    }

    if backend == DbBackend::MySql {
        let timestamp_columns = scalar_i64(
            database,
            backend,
            "SELECT COUNT(*) \
             FROM information_schema.columns \
             WHERE table_schema = DATABASE() \
               AND table_name <> 'seaql_migrations' \
               AND data_type = 'timestamp'",
        )
        .await?;
        if timestamp_columns != 0 {
            return Err(rebase_required_error(format!(
                "MySQL schema still has {timestamp_columns} TIMESTAMP column(s)"
            )));
        }
    }

    Ok(())
}

async fn rewrite_migration_history_to_baseline(database: &DatabaseConnection) -> Result<(), DbErr> {
    let backend = database.get_database_backend();
    let txn = database.begin().await?;
    txn.execute_unprepared(&format!(
        "DELETE FROM {}",
        quote_ident(backend, MIGRATION_TABLE)
    ))
    .await?;

    let sql = format!(
        "INSERT INTO {} ({}, {}) VALUES (?, ?)",
        quote_ident(backend, MIGRATION_TABLE),
        quote_ident(backend, "version"),
        quote_ident(backend, "applied_at"),
    );
    let applied_at = current_unix_timestamp()?;
    txn.execute_raw(Statement::from_sql_and_values(
        backend,
        sql,
        [BASELINE_MIGRATION_NAME.into(), applied_at.into()],
    ))
    .await?;
    txn.commit().await?;

    Ok(())
}

async fn application_schema_exists<C>(db: &C) -> Result<bool, DbErr>
where
    C: SeaConnectionTrait,
{
    for table_name in APPLICATION_SCHEMA_SENTINELS {
        if table_exists(db, db.get_database_backend(), table_name).await? {
            return Ok(true);
        }
    }

    Ok(false)
}

async fn migration_table_exists<C>(db: &C) -> Result<bool, DbErr>
where
    C: SeaConnectionTrait,
{
    table_exists(db, db.get_database_backend(), MIGRATION_TABLE).await
}

async fn column_exists<C>(
    db: &C,
    backend: DbBackend,
    table_name: &str,
    column_name: &str,
) -> Result<bool, DbErr>
where
    C: SeaConnectionTrait,
{
    let sql = match backend {
        DbBackend::Sqlite => format!(
            "SELECT CASE WHEN EXISTS(SELECT 1 FROM pragma_table_info({}) WHERE name = {}) THEN 1 ELSE 0 END",
            quote_literal(table_name),
            quote_literal(column_name)
        ),
        DbBackend::Postgres => format!(
            "SELECT CASE WHEN EXISTS(SELECT 1 FROM information_schema.columns \
             WHERE table_schema = current_schema() \
               AND table_name = {} \
               AND column_name = {}) THEN 1 ELSE 0 END",
            quote_literal(table_name),
            quote_literal(column_name)
        ),
        DbBackend::MySql => format!(
            "SELECT CASE WHEN EXISTS(SELECT 1 FROM information_schema.columns \
             WHERE table_schema = DATABASE() \
               AND table_name = {} \
               AND column_name = {}) THEN 1 ELSE 0 END",
            quote_literal(table_name),
            quote_literal(column_name)
        ),
        _ => {
            return Err(migration_state_error(
                "unsupported database backend for migration column inspection".to_string(),
            ));
        }
    };

    scalar_i64(db, backend, &sql).await.map(|value| value != 0)
}

async fn scalar_i64<C>(db: &C, backend: DbBackend, sql: &str) -> Result<i64, DbErr>
where
    C: SeaConnectionTrait,
{
    let row = db
        .query_one_raw(Statement::from_string(backend, sql.to_string()))
        .await?
        .ok_or_else(|| migration_state_error("scalar query returned no rows".to_string()))?;

    if let Ok(value) = row.try_get_by_index::<i64>(0) {
        return Ok(value);
    }
    if let Ok(value) = row.try_get_by_index::<i32>(0) {
        return Ok(i64::from(value));
    }
    if let Ok(value) = row.try_get_by_index::<bool>(0) {
        return Ok(if value { 1 } else { 0 });
    }

    Err(migration_state_error(
        "failed to decode scalar query result".to_string(),
    ))
}

async fn applied_migrations<C>(db: &C, backend: DbBackend) -> Result<Vec<String>, DbErr>
where
    C: SeaConnectionTrait,
{
    if !table_exists(db, backend, MIGRATION_TABLE).await? {
        return Ok(Vec::new());
    }

    let sql = format!(
        "SELECT {} FROM {} ORDER BY {}",
        quote_ident(backend, "version"),
        quote_ident(backend, MIGRATION_TABLE),
        quote_ident(backend, "version")
    );
    let rows = db
        .query_all_raw(Statement::from_string(backend, sql))
        .await?;

    rows.into_iter()
        .map(|row| row.try_get_by_index::<String>(0))
        .collect()
}

async fn table_exists<C>(db: &C, backend: DbBackend, table_name: &str) -> Result<bool, DbErr>
where
    C: SeaConnectionTrait,
{
    let sql = match backend {
        DbBackend::Sqlite => format!(
            "SELECT CASE WHEN EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = {}) THEN 1 ELSE 0 END",
            quote_literal(table_name)
        ),
        DbBackend::Postgres => format!(
            "SELECT CASE WHEN EXISTS(SELECT 1 FROM information_schema.tables \
             WHERE table_schema = current_schema() AND table_name = {}) THEN 1 ELSE 0 END",
            quote_literal(table_name)
        ),
        DbBackend::MySql => format!(
            "SELECT CASE WHEN EXISTS(SELECT 1 FROM information_schema.tables \
             WHERE table_schema = DATABASE() AND table_name = {}) THEN 1 ELSE 0 END",
            quote_literal(table_name)
        ),
        _ => {
            return Err(migration_state_error(
                "unsupported database backend for migration table inspection".to_string(),
            ));
        }
    };

    let row = db
        .query_one_raw(Statement::from_string(backend, sql))
        .await?
        .ok_or_else(|| {
            migration_state_error("table existence query returned no rows".to_string())
        })?;

    if let Ok(value) = row.try_get_by_index::<i64>(0) {
        return Ok(value != 0);
    }
    if let Ok(value) = row.try_get_by_index::<i32>(0) {
        return Ok(value != 0);
    }
    if let Ok(value) = row.try_get_by_index::<bool>(0) {
        return Ok(value);
    }

    Err(migration_state_error(
        "failed to decode table existence query result".to_string(),
    ))
}

fn current_unix_timestamp() -> Result<i64, DbErr> {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| {
            migration_state_error(format!("system clock is before UNIX epoch: {error}"))
        })?;
    <i64 as std::convert::TryFrom<u64>>::try_from(duration.as_secs()).map_err(|_| {
        migration_state_error("current UNIX timestamp does not fit into i64".to_string())
    })
}

fn quote_ident(backend: DbBackend, ident: &str) -> String {
    match backend {
        DbBackend::MySql => format!("`{}`", ident.replace('`', "``")),
        DbBackend::Postgres | DbBackend::Sqlite => {
            format!("\"{}\"", ident.replace('"', "\"\""))
        }
        _ => format!("\"{}\"", ident.replace('"', "\"\"")),
    }
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn migration_state_error(message: String) -> DbErr {
    DbErr::Custom(message)
}

fn rebase_required_error(detail: String) -> DbErr {
    migration_state_error(format!(
        "database schema is not ready for migration rebase ({detail}); first upgrade to \
         v0.0.1-alpha.25 and apply all migrations, then upgrade to this version"
    ))
}
