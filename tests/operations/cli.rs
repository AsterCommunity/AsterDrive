//! 集成测试：运维 CLI。

#![cfg(feature = "operations-cli")]

use crate::common;

use actix_web::test as actix_test;
use std::process::Command;

use aster_drive::config::DatabaseConfig;
use aster_drive::db;
use aster_drive::db::repository::{
    contact_verification_token_repo, file_create_idempotency_repo, file_repo,
    follower_enrollment_session_repo, managed_follower_repo, master_binding_repo, mfa_factor_repo,
    mfa_login_flow_repo, mfa_recovery_code_repo, mfa_totp_setup_flow_repo, policy_repo,
    property_repo, storage_policy_connector_credential_repo, tag_repo, user_repo,
};
use aster_drive_migration::{CurrentMigrator, Migrator, MigratorTrait};
use aster_drive_model::entities::{
    admin_content_access, auth_session, contact_verification_token, follower_enrollment_session,
    managed_follower, master_binding, mfa_factor, mfa_login_flow, mfa_recovery_code,
    mfa_totp_setup_flow, passkey, storage_policy, user_invitation,
};
use aster_drive_model::types::{
    EntityType, MfaFirstFactor, MfaPersistentFactorMethod, RemoteDownloadStrategy,
    RemoteUploadStrategy, StoredPasskeyCredential, StoredStoragePolicyAllowedTypes, TagScopeType,
    UserInvitationStatus, VerificationChannel, VerificationPurpose,
};
use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, Set, Statement,
};
use serde_json::Value;

fn aster_drive_bin() -> &'static str {
    env!("CARGO_BIN_EXE_aster_drive")
}

const MIGRATION_REMOTE_NODE_NAME: &str = "MigratedRemoteNode";
const MIGRATION_REMOTE_POLICY_NAME: &str = "MigratedRemotePolicy";
const MIGRATION_REMOTE_POLICY_CIPHERTEXT: &str = "encrypted-migration-credential";
const MIGRATION_MASTER_BINDING_NAME: &str = "MigratedMasterBinding";
const MIGRATION_MASTER_STORAGE_NAMESPACE: &str = "mb_migrate_remote_space";
const MIGRATION_TAG_NAME: &str = "Migrated Tag";
const MIGRATION_TAG_NORMALIZED_NAME: &str = "migrated tag";
const MIGRATION_TAG_COLOR: &str = "#2563eb";
const MIGRATION_TAG_PROPERTY_NAMESPACE: &str = "system.tags";
const MIGRATION_SCHEDULED_TASK_ID: &str = "aster_drive:migration_fixture";
const MIGRATION_RUNTIME_LEASE_ID: &str = "aster_drive.migration_fixture";
const MIGRATION_FILE_CREATE_KEY_HASH: &str =
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const MIGRATION_FILE_CREATE_FINGERPRINT: &str =
    "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

async fn setup_database_url() -> String {
    let db_path =
        std::env::temp_dir().join(format!("asterdrive-cli-test-{}.db", uuid::Uuid::new_v4()));
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: url.clone().into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    Migrator::up(&db, None).await.unwrap();
    aster_drive::db::repository::config_repo::ensure_defaults_with_env(&db, &|_| None)
        .await
        .unwrap();
    db.close().await.unwrap();
    url
}

async fn setup_empty_database_url(prefix: &str) -> String {
    let db_path = std::env::temp_dir().join(format!("{prefix}-{}.db", uuid::Uuid::new_v4()));
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: url.clone().into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    db.close().await.unwrap();
    url
}

async fn setup_ready_database_url() -> String {
    let db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-ready-test-{}.db",
        uuid::Uuid::new_v4()
    ));
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let _state = common::setup_with_database_url(&url).await;
    url
}

fn run_aster_drive(args: &[&str]) -> std::process::Output {
    run_aster_drive_with_env(args, &[])
}

fn run_aster_drive_with_env(args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    Command::new(aster_drive_bin())
        .args(args)
        .env("ASTER_CLI_YES", "true")
        .envs(envs.iter().copied())
        .output()
        .expect("aster_drive binary should run")
}

fn run_aster_drive_with_env_in_dir(
    args: &[&str],
    envs: &[(&str, &str)],
    current_dir: &std::path::Path,
) -> std::process::Output {
    Command::new(aster_drive_bin())
        .args(args)
        .env("ASTER_CLI_YES", "true")
        .envs(envs.iter().copied())
        .current_dir(current_dir)
        .output()
        .expect("aster_drive binary should run")
}

fn redact_database_url(database_url: &str) -> String {
    if database_url == "sqlite::memory:" {
        return database_url.to_string();
    }

    if database_url.starts_with("sqlite:") {
        let Some(path_and_query) = database_url.strip_prefix("sqlite://") else {
            return database_url.to_string();
        };
        let (path, query) = path_and_query
            .split_once('?')
            .map_or((path_and_query, None), |(path, query)| (path, Some(query)));
        let filename = std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path);
        return match query {
            Some(query) => format!("sqlite:///.../{filename}?{query}"),
            None => format!("sqlite:///.../{filename}"),
        };
    }

    let Some((scheme, rest)) = database_url.split_once("://") else {
        return database_url.to_string();
    };
    let Some((_authority, suffix)) = rest.rsplit_once('@') else {
        return database_url.to_string();
    };

    format!("{scheme}://***@{suffix}")
}

async fn scalar_i64(db: &DatabaseConnection, backend: DbBackend, sql: &str) -> i64 {
    db.query_one_raw(Statement::from_string(backend, sql))
        .await
        .unwrap()
        .unwrap()
        .try_get_by_index(0)
        .unwrap()
}

async fn scalar_string(db: &DatabaseConnection, backend: DbBackend, sql: &str) -> String {
    db.query_one_raw(Statement::from_string(backend, sql))
        .await
        .unwrap()
        .unwrap()
        .try_get_by_index(0)
        .unwrap()
}

async fn column_exists(
    db: &DatabaseConnection,
    backend: DbBackend,
    table: &str,
    column: &str,
) -> bool {
    let sql = match backend {
        DbBackend::Sqlite => {
            format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = '{column}'")
        }
        DbBackend::Postgres => {
            format!(
                "SELECT COUNT(*) FROM information_schema.columns \
                 WHERE table_schema = 'public' \
                   AND table_name = '{table}' \
                   AND column_name = '{column}'"
            )
        }
        DbBackend::MySql => {
            format!(
                "SELECT COUNT(*) FROM information_schema.columns \
                 WHERE table_schema = DATABASE() \
                   AND table_name = '{table}' \
                   AND column_name = '{column}'"
            )
        }
        backend => panic!("unsupported test database backend: {backend:?}"),
    };
    scalar_i64(db, backend, &sql).await > 0
}

async fn applied_migration_versions(db: &DatabaseConnection, backend: DbBackend) -> Vec<String> {
    db.query_all_raw(Statement::from_string(
        backend,
        "SELECT version FROM seaql_migrations ORDER BY version",
    ))
    .await
    .unwrap()
    .into_iter()
    .map(|row| row.try_get_by_index::<String>(0).unwrap())
    .collect()
}

#[path = "cli_cases/fixtures.rs"]
mod fixtures;
use fixtures::{
    assert_migrated_fixture, seed_contact_verification_history, seed_migration_fixture,
};
#[path = "cli_cases/completions.rs"]
mod completions;
#[path = "cli_cases/config.rs"]
mod config;
#[path = "cli_cases/database_migration.rs"]
mod database_migration;
#[path = "cli_cases/doctor.rs"]
mod doctor;
#[path = "cli_cases/fast_paths.rs"]
mod fast_paths;
#[path = "cli_cases/help.rs"]
mod help;
#[path = "cli_cases/input_and_redaction.rs"]
mod input_and_redaction;
#[path = "cli_cases/node.rs"]
mod node;
