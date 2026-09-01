//! 数据库迁移 crate 入口。
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::unreachable,
        clippy::expect_used,
        clippy::panic,
        clippy::unimplemented,
        clippy::todo
    )
)]

use std::sync::Arc;

pub use sea_orm_migration::prelude::*;

use sea_orm_migration::sea_orm::{
    ConnectionTrait as SeaConnectionTrait, DatabaseConnection, DatabaseExecutor, DbBackend,
    DbErr as SeaDbErr, RuntimeErr, Statement,
};

mod m20260512_000001_baseline_schema;
mod m20260515_000001_add_passkeys;
mod m20260517_000001_add_external_auth;
mod m20260518_000001_add_file_type_filters;
mod m20260518_000002_expand_audit_entity_type;
mod m20260519_000001_expand_background_task_display_name;
mod m20260520_000001_add_blob_media_metadata;
mod m20260523_000001_add_mfa;
mod m20260526_000001_add_upload_session_frontend_client;
mod m20260526_000002_add_mfa_email_codes;
mod m20260527_000001_add_storage_migration_checkpoints;
mod m20260528_000001_add_storage_migration_opaque_rename_count;
mod m20260529_000001_add_remote_node_transport;
mod m20260530_000001_add_webdav_account_team_scope;
mod m20260601_000001_add_system_config_visibility;
mod m20260601_000002_add_background_task_runtime_json;
mod m20260604_000001_allow_shared_webdav_locks;
mod m20260606_000001_add_external_auth_provider_options;
mod m20260607_000001_add_user_invitations;
mod m20260608_000001_add_tags;
mod m20260610_000001_add_user_must_change_password;
mod m20260612_000001_add_storage_policy_credentials;
mod m20260618_000001_rename_upload_session_object_fields;
mod m20260619_000001_add_storage_connector_application_configs;
mod m20260620_000001_enforce_json_text_not_null;
mod m20260704_000001_rename_managed_ingress_profiles_to_remote_storage_targets;
mod m20260704_000002_add_remote_storage_target_key_to_storage_policies;
mod m20260705_000001_drop_remote_storage_target_max_file_size;
mod m20260712_000001_align_forge_audit_contract;
mod m20260712_000002_add_forge_audit_query_indexes;
mod m20260712_000003_align_forge_system_config_contract;
mod m20260712_000004_align_forge_mail_outbox_contract;
mod m20260713_000001_runtime_leases;
mod m20260713_000002_background_task_dedupe_key;
mod m20260713_000003_scheduled_tasks;
mod m20260716_000001_bind_external_auth_login_flows;
mod m20260717_000001_add_upload_session_kind;
mod m20260719_000001_add_upload_provider_session;
mod m20260723_000001_require_upload_session_kind;
mod m20260725_000001_remote_tunnel_owners;
mod m20260728_000001_provider_relay_resumable_upload;
mod m20260803_000001_refactor_resource_locks;
mod m20260803_000002_storage_policy_connector_configs;
mod m20260803_000003_add_storage_policy_connector_credentials;
mod m20260805_000001_allow_connector_policy_writes_with_legacy_schema;
mod m20260810_000001_folder_tree_operation_members;
mod m20260813_000001_canonical_file_revision_ledger;
mod m20260815_000001_virtual_empty_file_blobs;
mod m20260817_000001_add_remote_binding_control_state;
mod m20260820_000001_remove_storage_policy_legacy;
mod m20260821_000001_rename_remote_node_telemetry;
mod m20260825_000001_storage_placement_profiles;
mod m20260825_000002_upload_session_placement_binding;
mod m20260901_000001_upload_session_mime_type;
mod m20260901_000002_upload_session_folder_status_index;
pub const BASELINE_MIGRATION_NAME: &str = "m20260512_000001_baseline_schema";

const MIGRATION_TABLE: &str = "seaql_migrations";
const RESOURCE_LOCK_REFACTOR_MIGRATION: &str = "m20260803_000001_refactor_resource_locks";
const STORAGE_POLICY_CONNECTOR_CONFIGS_MIGRATION: &str =
    "m20260803_000002_storage_policy_connector_configs";
const STORAGE_POLICY_CONNECTOR_CREDENTIALS_MIGRATION: &str =
    "m20260803_000003_add_storage_policy_connector_credentials";
const POSTGRES_MIGRATION_LOCK_KEY: i64 = 0x4153_5445_5244_5249;
const MYSQL_MIGRATION_LOCK_NAME: &str = "aster_drive:database_migrations";
const MYSQL_MIGRATION_LOCK_TIMEOUT_SECONDS: u64 = 300;
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
    Current,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EmptyDatabaseState {
    Empty,
    HasObjects,
}

impl MigrationTrack {
    pub fn label(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Current => "current",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MigrationHistory {
    pub track: MigrationTrack,
    pub applied: Vec<String>,
    pub pending_current: Vec<String>,
    pub unknown_applied: Vec<String>,
}

impl MigrationHistory {
    pub fn effective_pending(&self) -> &[String] {
        &self.pending_current
    }

    pub fn has_unknown_applied(&self) -> bool {
        !self.unknown_applied.is_empty()
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
            Box::new(m20260512_000001_baseline_schema::Migration),
            Box::new(m20260515_000001_add_passkeys::Migration),
            Box::new(m20260517_000001_add_external_auth::Migration),
            Box::new(m20260518_000001_add_file_type_filters::Migration),
            Box::new(m20260518_000002_expand_audit_entity_type::Migration),
            Box::new(m20260519_000001_expand_background_task_display_name::Migration),
            Box::new(m20260520_000001_add_blob_media_metadata::Migration),
            Box::new(m20260523_000001_add_mfa::Migration),
            Box::new(m20260526_000001_add_upload_session_frontend_client::Migration),
            Box::new(m20260526_000002_add_mfa_email_codes::Migration),
            Box::new(m20260527_000001_add_storage_migration_checkpoints::Migration),
            Box::new(m20260528_000001_add_storage_migration_opaque_rename_count::Migration),
            Box::new(m20260529_000001_add_remote_node_transport::Migration),
            Box::new(m20260530_000001_add_webdav_account_team_scope::Migration),
            Box::new(m20260601_000001_add_system_config_visibility::Migration),
            Box::new(m20260601_000002_add_background_task_runtime_json::Migration),
            Box::new(m20260604_000001_allow_shared_webdav_locks::Migration),
            Box::new(m20260606_000001_add_external_auth_provider_options::Migration),
            Box::new(m20260607_000001_add_user_invitations::Migration),
            Box::new(m20260608_000001_add_tags::Migration),
            Box::new(m20260610_000001_add_user_must_change_password::Migration),
            Box::new(m20260612_000001_add_storage_policy_credentials::Migration),
            Box::new(m20260618_000001_rename_upload_session_object_fields::Migration),
            Box::new(m20260619_000001_add_storage_connector_application_configs::Migration),
            Box::new(m20260620_000001_enforce_json_text_not_null::Migration),
            Box::new(
                m20260704_000001_rename_managed_ingress_profiles_to_remote_storage_targets::Migration,
            ),
            Box::new(
                m20260704_000002_add_remote_storage_target_key_to_storage_policies::Migration,
            ),
            Box::new(m20260705_000001_drop_remote_storage_target_max_file_size::Migration),
            Box::new(m20260712_000001_align_forge_audit_contract::Migration),
            Box::new(m20260712_000002_add_forge_audit_query_indexes::Migration),
            Box::new(m20260712_000003_align_forge_system_config_contract::Migration),
            Box::new(m20260712_000004_align_forge_mail_outbox_contract::Migration),
            Box::new(m20260713_000001_runtime_leases::Migration),
            Box::new(m20260713_000002_background_task_dedupe_key::Migration),
            Box::new(m20260713_000003_scheduled_tasks::Migration),
            Box::new(m20260716_000001_bind_external_auth_login_flows::Migration),
            Box::new(m20260717_000001_add_upload_session_kind::Migration),
            Box::new(m20260719_000001_add_upload_provider_session::Migration),
            Box::new(m20260723_000001_require_upload_session_kind::Migration),
            Box::new(m20260725_000001_remote_tunnel_owners::Migration),
            Box::new(m20260728_000001_provider_relay_resumable_upload::Migration),
            Box::new(m20260803_000001_refactor_resource_locks::Migration),
            Box::new(m20260803_000002_storage_policy_connector_configs::Migration),
            Box::new(m20260803_000003_add_storage_policy_connector_credentials::Migration),
            Box::new(
                m20260805_000001_allow_connector_policy_writes_with_legacy_schema::Migration,
            ),
            Box::new(m20260810_000001_folder_tree_operation_members::Migration),
            Box::new(m20260813_000001_canonical_file_revision_ledger::Migration),
            Box::new(m20260815_000001_virtual_empty_file_blobs::Migration),
            Box::new(
                m20260817_000001_add_remote_binding_control_state::Migration,
            ),
            Box::new(m20260820_000001_remove_storage_policy_legacy::Migration),
            Box::new(m20260821_000001_rename_remote_node_telemetry::Migration),
            Box::new(m20260825_000001_storage_placement_profiles::Migration),
            Box::new(m20260825_000002_upload_session_placement_binding::Migration),
            Box::new(m20260901_000001_upload_session_mime_type::Migration),
            Box::new(m20260901_000002_upload_session_folder_status_index::Migration),
        ]
    }
}

pub fn current_migration_names() -> Vec<String> {
    <CurrentMigrator as MigratorTrait>::migrations()
        .into_iter()
        .map(|migration| migration.name().to_string())
        .collect()
}

pub async fn inspect_migration_history<C>(db: &C) -> Result<MigrationHistory, DbErr>
where
    C: SeaConnectionTrait,
{
    let applied = applied_migrations(db, db.get_database_backend()).await?;
    let current_names = current_migration_names();

    let current_lookup = current_names
        .iter()
        .map(String::as_str)
        .collect::<std::collections::HashSet<_>>();

    let unknown_applied = applied
        .iter()
        .filter(|name| !current_lookup.contains(name.as_str()))
        .cloned()
        .collect::<Vec<_>>();

    let is_current_prefix = applied.len() <= current_names.len()
        && applied
            .iter()
            .zip(current_names.iter())
            .all(|(applied_name, current_name)| applied_name == current_name);
    let is_supported_storage_refactor_history =
        is_storage_refactor_branch_history(&applied, &current_names);
    let is_supported_current_history = is_current_prefix || is_supported_storage_refactor_history;

    let pending_current = if is_supported_current_history {
        let applied_lookup = applied
            .iter()
            .map(String::as_str)
            .collect::<std::collections::HashSet<_>>();
        current_names
            .iter()
            .filter(|name| !applied_lookup.contains(name.as_str()))
            .cloned()
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let track = if applied.is_empty() {
        match inspect_empty_database_state(db).await? {
            EmptyDatabaseState::Empty => MigrationTrack::Empty,
            EmptyDatabaseState::HasObjects => MigrationTrack::Unknown,
        }
    } else if unknown_applied.is_empty() && is_supported_current_history {
        MigrationTrack::Current
    } else {
        MigrationTrack::Unknown
    };

    Ok(MigrationHistory {
        track,
        applied,
        pending_current,
        unknown_applied,
    })
}

/// Recognize databases that ran the storage refactor branch before it merged master.
///
/// That branch appended its two storage-policy migrations directly after the July
/// migration tail, while master independently appended the resource-lock refactor at
/// the same boundary. Accept only that exact, ordered branch suffix; arbitrary gaps in
/// migration history remain unsupported.
fn is_storage_refactor_branch_history(applied: &[String], current: &[String]) -> bool {
    let Some(resource_lock_index) = current
        .iter()
        .position(|name| name == RESOURCE_LOCK_REFACTOR_MIGRATION)
    else {
        return false;
    };
    let branch_migrations = [
        STORAGE_POLICY_CONNECTOR_CONFIGS_MIGRATION,
        STORAGE_POLICY_CONNECTOR_CREDENTIALS_MIGRATION,
    ];
    let common_prefix = &current[..resource_lock_index];
    if applied.len() <= common_prefix.len()
        || applied.len() > common_prefix.len() + branch_migrations.len()
        || applied[..common_prefix.len()] != *common_prefix
    {
        return false;
    }

    let applied_branch_suffix = &applied[common_prefix.len()..];
    applied_branch_suffix
        .iter()
        .map(String::as_str)
        .eq(branch_migrations
            .into_iter()
            .take(applied_branch_suffix.len()))
}

async fn inspect_empty_database_state<C>(db: &C) -> Result<EmptyDatabaseState, DbErr>
where
    C: SeaConnectionTrait,
{
    if migration_table_exists(db).await? || application_schema_exists(db).await? {
        Ok(EmptyDatabaseState::HasObjects)
    } else {
        Ok(EmptyDatabaseState::Empty)
    }
}

pub async fn apply_database_migrations(database: &DatabaseConnection) -> Result<(), DbErr> {
    if database.get_database_backend() == DbBackend::Sqlite {
        return apply_sqlite_database_migrations(database).await;
    }
    if database.get_database_backend() == DbBackend::Postgres {
        // Forge holds the process-wide advisory lock in its own transaction. Run migrations on
        // the pool rather than inside that transaction so migrations with `use_transaction(false)`
        // can commit bounded batches while the lock remains held on the dedicated checked-out
        // connection. Ordinary PostgreSQL migrations still receive SeaORM's per-migration
        // transaction according to their own `use_transaction` policy.
        let source_database = database.clone();
        return with_database_migration_lock(database, move |_lock_connection| {
            Box::pin(async move {
                let migration_database =
                    create_postgres_migration_connection(&source_database).await?;
                let result = apply_database_migrations_unlocked(DatabaseExecutor::Connection(
                    &migration_database,
                ))
                .await;
                if let Err(close_error) = migration_database.close().await {
                    tracing::warn!(%close_error, "failed to close dedicated PostgreSQL migration connection");
                }
                result
            })
        })
        .await;
    }
    with_database_migration_lock(database, |connection| {
        Box::pin(apply_database_migrations_unlocked(connection))
    })
    .await
}

async fn create_postgres_migration_connection(
    database: &DatabaseConnection,
) -> Result<DatabaseConnection, DbErr> {
    let source_pool = database.get_postgres_connection_pool();
    let connect_options = source_pool.connect_options();
    let dedicated_pool = source_pool
        .options()
        .clone()
        .max_connections(1)
        .min_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .test_before_acquire(false)
        .before_acquire(|_, _| Box::pin(async { Ok(true) }))
        .after_release(|_, _| Box::pin(async { Ok(true) }))
        .connect_with((*connect_options).clone())
        .await
        .map_err(|error| SeaDbErr::Conn(RuntimeErr::SqlxError(Arc::new(error))))?;
    Ok(dedicated_pool.into())
}

/// Run SQLite schema migrations with foreign keys disabled before Forge opens
/// its outer migration transaction.
///
/// SQLite ignores `PRAGMA foreign_keys = OFF` after a transaction has begun.
/// Parent-table rebuilds therefore need the connection-local pragma configured
/// before `with_database_migration_lock` starts its transaction. AsterDrive's
/// SQLite writer is deliberately single-connection, so the pragma setup, Forge
/// transaction, integrity check, and state restoration all use the same
/// physical connection.
async fn apply_sqlite_database_migrations(database: &DatabaseConnection) -> Result<(), DbErr> {
    let pool = database.get_sqlite_connection_pool();
    let max_connections = pool.options().get_max_connections();
    if max_connections != 1 {
        return Err(migration_state_error(format!(
            "SQLite migrations require a single-connection writer pool; configured maximum is {max_connections}"
        )));
    }

    let foreign_keys_enabled = sqlite_foreign_keys_enabled(database).await?;
    if let Err(configuration_error) = set_sqlite_foreign_keys(database, false).await {
        let restore_result = set_sqlite_foreign_keys(database, foreign_keys_enabled).await;
        return match restore_result {
            Ok(()) => Err(configuration_error),
            Err(restore_error) => Err(migration_state_error(format!(
                "{configuration_error}; additionally failed to restore SQLite foreign-key state: {restore_error}"
            ))),
        };
    }

    let operation_result = with_database_migration_lock(database, |connection| {
        Box::pin(apply_database_migrations_unlocked(connection))
    })
    .await;

    let restore_result = set_sqlite_foreign_keys(database, foreign_keys_enabled).await;
    match (operation_result, restore_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(operation_error), Ok(())) => Err(operation_error),
        (Ok(()), Err(restore_error)) => Err(restore_error),
        (Err(operation_error), Err(restore_error)) => Err(migration_state_error(format!(
            "SQLite migration failed: {operation_error}; additionally failed to restore foreign-key state: {restore_error}"
        ))),
    }
}

async fn set_sqlite_foreign_keys(
    database: &DatabaseConnection,
    enabled: bool,
) -> Result<(), DbErr> {
    let pragma = if enabled {
        "PRAGMA foreign_keys = ON"
    } else {
        "PRAGMA foreign_keys = OFF"
    };
    database.execute_unprepared(pragma).await.map_err(|error| {
        migration_state_error(format!(
            "failed to set SQLite foreign-key state to {enabled}: {error}"
        ))
    })?;
    let actual = sqlite_foreign_keys_enabled(database).await?;
    if actual != enabled {
        return Err(migration_state_error(format!(
            "SQLite foreign-key state remained {actual} after requesting {enabled}"
        )));
    }
    Ok(())
}

async fn sqlite_foreign_keys_enabled(database: &DatabaseConnection) -> Result<bool, DbErr> {
    let row = database
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA foreign_keys",
        ))
        .await?
        .ok_or_else(|| {
            migration_state_error("SQLite foreign-key status returned no row".to_string())
        })?;
    Ok(row.try_get_by_index::<i64>(0)? != 0)
}

/// Run schema-sensitive startup work under AsterDrive's database migration lock.
///
/// The 0.5.0 storage-policy credential importer uses this after ordinary SeaORM
/// migrations have completed. Keeping connector-owned credential conversion
/// under the same lock prevents concurrent instances from importing the same
/// legacy rows. The legacy schema remains in place until the 0.6.0 cleanup
/// migration tracked by issue #463.
pub async fn with_database_migration_lock<T, F>(
    database: &DatabaseConnection,
    operation: F,
) -> Result<T, DbErr>
where
    F: for<'a> FnOnce(DatabaseExecutor<'a>) -> aster_forge_db_migration::MigrationFuture<'a, T>,
{
    let options = aster_forge_db_migration::MigrationLockOptions::new(MYSQL_MIGRATION_LOCK_NAME)
        .with_postgres_advisory_key(POSTGRES_MIGRATION_LOCK_KEY)
        .with_mysql_timeout_seconds(MYSQL_MIGRATION_LOCK_TIMEOUT_SECONDS);
    aster_forge_db_migration::with_migration_lock(database, &options, operation).await
}

async fn apply_database_migrations_unlocked(database: DatabaseExecutor<'_>) -> Result<(), DbErr> {
    let history = inspect_migration_history(&database).await?;
    if history.track == MigrationTrack::Unknown {
        return Err(migration_state_error(format!(
            "database contains unknown migration versions: {}",
            unsupported_migration_versions_label(&history)
        )));
    }
    let validate_foreign_keys = !history.pending_current.is_empty();

    match history.track {
        MigrationTrack::Empty => {
            if migration_table_exists(&database).await?
                || application_schema_exists(&database).await?
            {
                return Err(migration_state_error(
                    "database contains migration metadata or application tables but migration \
                     history is empty; restore a backup or run a supported intermediate release \
                     before upgrading to this version"
                        .to_string(),
                ));
            }
            apply_current_migrations(database, validate_foreign_keys).await
        }
        MigrationTrack::Current => apply_current_migrations(database, validate_foreign_keys).await,
        MigrationTrack::Unknown => Err(migration_state_error(format!(
            "database contains unsupported migration versions: {}. Upgrade from a supported \
             release line or restore a backup before continuing",
            unsupported_migration_versions_label(&history)
        ))),
    }
}

async fn apply_current_migrations(
    database: DatabaseExecutor<'_>,
    validate_foreign_keys: bool,
) -> Result<(), DbErr> {
    match database {
        DatabaseExecutor::Connection(connection) => {
            <CurrentMigrator as MigratorTrait>::up(connection, None).await?;
            validate_sqlite_foreign_keys(connection, validate_foreign_keys).await
        }
        DatabaseExecutor::Transaction(transaction) => {
            <CurrentMigrator as MigratorTrait>::up(transaction, None).await?;
            validate_sqlite_foreign_keys(transaction, validate_foreign_keys).await
        }
        DatabaseExecutor::OwnedTransaction(transaction) => {
            <CurrentMigrator as MigratorTrait>::up(&transaction, None).await?;
            validate_sqlite_foreign_keys(&transaction, validate_foreign_keys).await
        }
    }
}

async fn validate_sqlite_foreign_keys<C>(database: &C, enabled: bool) -> Result<(), DbErr>
where
    C: SeaConnectionTrait,
{
    if !enabled || database.get_database_backend() != DbBackend::Sqlite {
        return Ok(());
    }
    let violations = database
        .query_all_raw(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA foreign_key_check",
        ))
        .await?;
    if violations.is_empty() {
        Ok(())
    } else {
        Err(migration_state_error(format!(
            "SQLite schema migrations introduced {} foreign-key violation(s)",
            violations.len()
        )))
    }
}

fn unsupported_migration_versions_label(history: &MigrationHistory) -> String {
    if !history.unknown_applied.is_empty() {
        history.unknown_applied.join(", ")
    } else if history.applied.is_empty() {
        "<empty migration history with existing schema objects>".to_string()
    } else {
        "<non-prefix migration history>".to_string()
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm_migration::SchemaManager;

    #[test]
    fn mysql_migrations_do_not_install_a_global_version_gate() {
        let source = include_str!("lib.rs");
        let gate_symbol = ["ensure", "supported", "database", "server"].join("_");
        let minimum_symbol = ["MINIMUM", "MYSQL", "VERSION"].join("_");
        let version_query = ["SELECT", "VERSION()"].join(" ");

        assert!(!source.contains(&gate_symbol));
        assert!(!source.contains(&minimum_symbol));
        assert!(!source.contains(&version_query));
    }

    #[test]
    fn mysql_property_case_projection_avoids_version_specific_ddl() {
        let source = include_str!("m20260813_000001_canonical_file_revision_ledger.rs");
        assert!(!source.contains("VIRTUAL INVISIBLE"));
        assert!(!source.contains("ALGORITHM=INSTANT"));
    }

    async fn record_applied_migration(db: &DatabaseConnection, migration_name: &str) {
        db.execute_unprepared(&format!(
            "INSERT INTO seaql_migrations (version, applied_at) VALUES ({}, 1)",
            quote_literal(migration_name)
        ))
        .await
        .expect("branch migration history row should insert");
    }

    async fn setup_storage_refactor_branch_history(
        storage_migration_count: usize,
    ) -> DatabaseConnection {
        let db = sea_orm_migration::sea_orm::Database::connect("sqlite::memory:")
            .await
            .expect("SQLite migration fixture should connect");
        let resource_lock_index = current_migration_names()
            .iter()
            .position(|name| name == RESOURCE_LOCK_REFACTOR_MIGRATION)
            .expect("resource-lock migration should be registered");
        <CurrentMigrator as MigratorTrait>::up(
            &db,
            Some(
                <u32 as std::convert::TryFrom<usize>>::try_from(resource_lock_index)
                    .expect("migration count should fit u32"),
            ),
        )
        .await
        .expect("common migration prefix should apply");

        let manager = SchemaManager::new(&db);
        if storage_migration_count >= 1 {
            m20260803_000002_storage_policy_connector_configs::Migration
                .up(&manager)
                .await
                .expect("storage connector config migration should apply");
            record_applied_migration(&db, STORAGE_POLICY_CONNECTOR_CONFIGS_MIGRATION).await;
        }
        if storage_migration_count >= 2 {
            m20260803_000003_add_storage_policy_connector_credentials::Migration
                .up(&manager)
                .await
                .expect("storage connector credential migration should apply");
            record_applied_migration(&db, STORAGE_POLICY_CONNECTOR_CREDENTIALS_MIGRATION).await;
        }
        db
    }

    async fn assert_storage_refactor_branch_history_upgrades(storage_migration_count: usize) {
        let db = setup_storage_refactor_branch_history(storage_migration_count).await;
        let history = inspect_migration_history(&db)
            .await
            .expect("branch migration history should inspect");
        assert_eq!(history.track, MigrationTrack::Current);
        assert_eq!(
            history.pending_current.first().map(String::as_str),
            Some(RESOURCE_LOCK_REFACTOR_MIGRATION)
        );
        assert_eq!(
            history.pending_current.len(),
            current_migration_names().len() - history.applied.len(),
            "resource-lock plus any storage branch tail not yet applied should remain pending"
        );

        apply_database_migrations(&db)
            .await
            .expect("recognized storage refactor branch history should upgrade");
        let upgraded = inspect_migration_history(&db)
            .await
            .expect("upgraded migration history should inspect");
        assert_eq!(upgraded.track, MigrationTrack::Current);
        assert!(upgraded.pending_current.is_empty());
        assert_eq!(upgraded.applied, current_migration_names());
        assert!(
            SchemaManager::new(&db)
                .has_table("resource_lock_namespaces")
                .await
                .expect("resource-lock namespace table existence should query")
        );
    }

    #[tokio::test]
    async fn upgrades_branch_history_after_connector_config_migration_only() {
        assert_storage_refactor_branch_history_upgrades(1).await;
    }

    #[tokio::test]
    async fn upgrades_branch_history_after_both_storage_migrations() {
        assert_storage_refactor_branch_history_upgrades(2).await;
    }

    #[tokio::test]
    async fn sqlite_migrations_preserve_disabled_foreign_key_state() {
        let db = sea_orm_migration::sea_orm::Database::connect("sqlite::memory:")
            .await
            .expect("SQLite migration fixture should connect");
        db.execute_unprepared("PRAGMA foreign_keys = OFF")
            .await
            .expect("fixture should disable foreign keys");
        assert!(!sqlite_foreign_keys_enabled(&db).await.unwrap());

        apply_database_migrations(&db)
            .await
            .expect("fresh SQLite schema should migrate");

        assert!(
            !sqlite_foreign_keys_enabled(&db).await.unwrap(),
            "migration finalization should preserve an initially disabled state"
        );
    }
}
