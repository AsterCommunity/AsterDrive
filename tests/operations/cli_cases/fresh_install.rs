//! Exercise the released startup path from a genuinely empty database, without schema templates.
use super::*;
use aster_forge_test::process::{TestProcess, available_loopback_port};
use std::time::Duration;

async fn connect(database_url: &str) -> DatabaseConnection {
    db::connect_with_metrics(
        &DatabaseConfig {
            url: database_url.into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap()
}

fn server_command(database_url: &str, mode: &str, port: u16) -> Command {
    let mut command = Command::new(aster_drive_bin());
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("ASTER_") {
            command.env_remove(key);
        }
    }
    command
        .args(["serve", "all"])
        .env("ASTER__SERVER__START_MODE", mode)
        .env("ASTER__SERVER__HOST", "127.0.0.1")
        .env("ASTER__SERVER__PORT", port.to_string())
        .env("ASTER__SERVER__WORKERS", "1")
        .env("ASTER__DATABASE__URL", database_url)
        .env("ASTER__DATABASE__POOL_SIZE", "1")
        .env("ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES", "true");
    command
}

async fn wait_for_server(process: &mut TestProcess, client: &reqwest::Client, base: &str) {
    tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            process.assert_running();
            if client
                .get(format!("{base}/health"))
                .send()
                .await
                .is_ok_and(|response| response.status().is_success())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap_or_else(|error| {
        panic!(
            "fresh server did not start: {error}\n{}",
            process.diagnostics()
        )
    });
}

async fn assert_current_schema(database_url: &str) {
    let database = connect(database_url).await;
    let history = aster_drive_migration::inspect_migration_history(&database)
        .await
        .unwrap();
    assert_eq!(
        history.applied,
        aster_drive_migration::current_migration_names()
    );
    assert!(history.pending_current.is_empty());
    assert!(history.unknown_applied.is_empty());
    let schema = aster_drive_migration::SchemaManager::new(&database);
    assert!(
        !schema
            .has_table("storage_policy_group_items")
            .await
            .unwrap()
    );
    for column in [
        "driver_type",
        "endpoint",
        "bucket",
        "access_key",
        "secret_key",
        "base_path",
    ] {
        assert!(
            !schema
                .has_column("remote_storage_targets", column)
                .await
                .unwrap()
        );
    }
    let rows = aster_forge_db::system_config::Entity::find()
        .all(&database)
        .await
        .unwrap();
    let config = rows
        .iter()
        .find(|row| row.key == "audit_log_recorded_actions")
        .unwrap();
    let actions =
        aster_drive::config::audit::parse_recorded_actions_config_value(&config.value).unwrap();
    assert!(
        actions.contains(&aster_drive_model::types::AuditAction::AdminCreateRemoteStorageTarget)
    );
    assert!(!config.value.contains("ingress_profile"));
    assert_eq!(
        aster_drive::services::system_setup::state(&database)
            .await
            .unwrap(),
        aster_drive::services::system_setup::SystemSetupState::NeedsAdmin
    );
    database.close().await.unwrap();
}

async fn assert_primary_setup(client: &reqwest::Client, base: &str, expected: &str) {
    let response = client
        .post(format!("{base}/api/v1/auth/check"))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["data"]["setup_state"], expected);
    let response = client
        .get(format!("{base}/health/ready"))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["data"]["status"], expected);
}

async fn fresh_server_installs_and_restarts(backend: DbBackend, mode: &str) {
    let port = available_loopback_port();
    let external_url = match backend {
        DbBackend::Postgres => Some(common::postgres_empty_test_database_url().await),
        DbBackend::MySql => Some(common::mysql_empty_test_database_url().await),
        DbBackend::Sqlite => None,
        _ => unreachable!(),
    };
    if let Some(url) = external_url.as_ref() {
        let database = connect(url).await;
        let history = aster_drive_migration::inspect_migration_history(&database)
            .await
            .unwrap();
        assert_eq!(history.track, aster_drive_migration::MigrationTrack::Empty);
        assert!(history.applied.is_empty());
        database.close().await.unwrap();
    }
    let url = external_url
        .as_deref()
        .unwrap_or("sqlite://fresh.db?mode=rwc");
    let mut command = server_command(url, mode, port);
    let mut process = TestProcess::spawn(
        &format!("fresh-{backend:?}-{mode}").to_lowercase(),
        &mut command,
    );
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    wait_for_server(&mut process, &client, &base).await;
    let sqlite_url = format!(
        "sqlite://{}?mode=rw",
        process.runtime_dir().join("data/fresh.db").display()
    );
    let inspect_url = external_url.as_deref().unwrap_or(&sqlite_url);
    assert_current_schema(inspect_url).await;
    if mode == "primary" {
        assert_primary_setup(&client, &base, "needs_admin").await;
        let response = client.post(format!("{base}/api/v1/auth/setup"))
            .json(&serde_json::json!({"username":"freshadmin","email":"freshadmin@example.test","password":"Fresh-deployment-password-123"}))
            .send().await.unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::CREATED,
            "{}",
            response.text().await.unwrap()
        );
        assert_primary_setup(&client, &base, "needs_storage").await;
    }
    assert!(
        process.terminate_gracefully(Duration::from_secs(20)),
        "{}",
        process.diagnostics()
    );
    // TestProcess gives each launch a new directory. Reuse the absolute database
    // path so restart proves idempotence, rather than accidentally installing again.
    let mut restart = TestProcess::spawn(
        &format!("restart-{backend:?}-{mode}").to_lowercase(),
        &mut server_command(inspect_url, mode, port),
    );
    wait_for_server(&mut restart, &client, &base).await;
    let database = connect(inspect_url).await;
    let history = aster_drive_migration::inspect_migration_history(&database)
        .await
        .unwrap();
    assert_eq!(
        history.applied,
        aster_drive_migration::current_migration_names()
    );
    assert!(history.pending_current.is_empty());
    if mode == "primary" {
        assert_primary_setup(&client, &base, "needs_storage").await;
    }
    database.close().await.unwrap();
    assert!(
        restart.terminate_gracefully(Duration::from_secs(20)),
        "{}",
        restart.diagnostics()
    );
}

#[tokio::test]
async fn fresh_deployment_sqlite_primary() {
    fresh_server_installs_and_restarts(DbBackend::Sqlite, "primary").await;
}
#[tokio::test]
async fn fresh_deployment_sqlite_follower() {
    fresh_server_installs_and_restarts(DbBackend::Sqlite, "follower").await;
}
#[tokio::test]
async fn fresh_deployment_postgres_primary() {
    fresh_server_installs_and_restarts(DbBackend::Postgres, "primary").await;
}
#[tokio::test]
async fn fresh_deployment_postgres_follower() {
    fresh_server_installs_and_restarts(DbBackend::Postgres, "follower").await;
}
#[tokio::test]
async fn fresh_deployment_mysql_primary() {
    fresh_server_installs_and_restarts(DbBackend::MySql, "primary").await;
}
#[tokio::test]
async fn fresh_deployment_mysql_follower() {
    fresh_server_installs_and_restarts(DbBackend::MySql, "follower").await;
}
