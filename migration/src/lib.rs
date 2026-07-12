//! 数据库迁移 crate 入口。
#![deny(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
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

pub use sea_orm_migration::prelude::*;

use sea_orm_migration::sea_orm::{
    ConnectionTrait as SeaConnectionTrait, DatabaseConnection, DbBackend, Statement,
};

mod column;
mod index_helpers;
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
mod search_acceleration;
mod time;

pub const BASELINE_MIGRATION_NAME: &str = "m20260512_000001_baseline_schema";

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

    let pending_current = if is_current_prefix {
        current_names
            .iter()
            .skip(applied.len())
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
    } else if unknown_applied.is_empty() && is_current_prefix {
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
    let history = inspect_migration_history(database).await?;
    if history.track == MigrationTrack::Unknown {
        return Err(migration_state_error(format!(
            "database contains unknown migration versions: {}",
            unsupported_migration_versions_label(&history)
        )));
    }

    match history.track {
        MigrationTrack::Empty => {
            if migration_table_exists(database).await?
                || application_schema_exists(database).await?
            {
                return Err(migration_state_error(
                    "database contains migration metadata or application tables but migration \
                     history is empty; restore a backup or run a supported intermediate release \
                     before upgrading to this version"
                        .to_string(),
                ));
            }
            <CurrentMigrator as MigratorTrait>::up(database, None).await?;
            Ok(())
        }
        MigrationTrack::Current => {
            <CurrentMigrator as MigratorTrait>::up(database, None).await?;
            Ok(())
        }
        MigrationTrack::Unknown => Err(migration_state_error(format!(
            "database contains unsupported migration versions: {}. Upgrade from a supported \
             release line or restore a backup before continuing",
            unsupported_migration_versions_label(&history)
        ))),
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
