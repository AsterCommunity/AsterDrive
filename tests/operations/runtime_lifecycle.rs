//! Exercise the production startup and assembly paths in isolated server processes.

use std::{process::Command, time::Duration};

use aster_drive_model::{entities::audit_log, types::AuditAction};
use aster_forge_test::process::{TestProcess, available_loopback_port};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter};

async fn lifecycle(mode: &str, fail_shutdown_audit: bool, role: Option<&str>) {
    let port = available_loopback_port();
    let mut command = Command::new(env!("CARGO_BIN_EXE_aster_drive"));
    // Keep LLVM_PROFILE_FILE and other coverage instrumentation variables intact.
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("ASTER__") {
            command.env_remove(key);
        }
    }
    if let Some(role) = role {
        command.args(["serve", role]);
    }
    command
        .env("ASTER__SERVER__START_MODE", mode)
        .env("ASTER__SERVER__HOST", "127.0.0.1")
        .env("ASTER__SERVER__PORT", port.to_string())
        .env("ASTER__SERVER__WORKERS", "1")
        .env("ASTER__DATABASE__URL", "sqlite://asterdrive.db?mode=rwc")
        .env("ASTER__DATABASE__POOL_SIZE", "2")
        .env("ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES", "true")
        .env("ASTER__LOGGING__LEVEL", "info");
    let mut process = TestProcess::spawn(&format!("audit-{mode}"), &mut command);
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            process.assert_running();
            if client
                .get(format!("http://127.0.0.1:{port}/health"))
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
    .unwrap_or_else(|error| panic!("server did not start: {error}\n{}", process.diagnostics()));

    let db_path = process.runtime_dir().join("data/asterdrive.db");
    let db = sea_orm::Database::connect(format!("sqlite://{}", db_path.display()))
        .await
        .unwrap();
    if role == Some("worker") {
        for path in ["/", "/api/v1/auth/login", "/dav", "/api-docs/openapi.json"] {
            let response = client
                .get(format!("http://127.0.0.1:{port}{path}"))
                .send()
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                reqwest::StatusCode::NOT_FOUND,
                "worker exposed {path}"
            );
        }
    }
    // Readiness alone does not prove the audit worker started. Wait for the partial batch.
    tokio::time::timeout(Duration::from_secs(10), async {
        while count(&db, AuditAction::ServerStart).await != 1 {
            process.assert_running();
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap_or_else(|error| panic!("startup audit missing: {error}\n{}", process.diagnostics()));
    assert_eq!(count(&db, AuditAction::ServerShutdown).await, 0);
    if fail_shutdown_audit {
        db.execute_unprepared(
            "CREATE TRIGGER fail_shutdown_audit BEFORE INSERT ON audit_logs \
             WHEN NEW.action = 'server_shutdown' \
             BEGIN SELECT RAISE(ABORT, 'injected audit failure'); END;",
        )
        .await
        .unwrap();
    }

    assert!(
        process.terminate_gracefully(Duration::from_secs(20)),
        "{}",
        process.diagnostics()
    );
    assert_eq!(count(&db, AuditAction::ServerStart).await, 1);
    assert_eq!(
        count(&db, AuditAction::ServerShutdown).await,
        u64::from(!fail_shutdown_audit)
    );
    let recovery_dir = process.runtime_dir().join("data/audit-recovery");
    if fail_shutdown_audit {
        let files = std::fs::read_dir(&recovery_dir)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(files.len(), 1, "{}", process.diagnostics());
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(files[0].path()).unwrap()).unwrap();
        assert_eq!(saved["requires_database_reconciliation"], true);
        assert_eq!(saved["batch_failure_class"], "Permanent");
        assert_eq!(saved["records"].as_array().unwrap().len(), 1);
        assert_eq!(saved["records"][0]["action"], "server_shutdown");
        assert!(
            process
                .diagnostics()
                .contains("unconfirmed audit records saved")
        );
    } else {
        assert!(
            !recovery_dir.exists(),
            "clean shutdown must not produce recovery files"
        );
        assert!(
            !process.diagnostics().contains("Error:"),
            "normal shutdown returned a process error: {}",
            process.diagnostics()
        );
    }
    // Reopen after process exit: committed rows remain durable and no child holds the pool.
    db.close().await.unwrap();
    let reopened = sea_orm::Database::connect(format!("sqlite://{}", db_path.display()))
        .await
        .unwrap();
    assert_eq!(count(&reopened, AuditAction::ServerStart).await, 1);
    assert_eq!(
        count(&reopened, AuditAction::ServerShutdown).await,
        u64::from(!fail_shutdown_audit)
    );
    reopened.close().await.unwrap();
}

async fn count(db: &sea_orm::DatabaseConnection, action: AuditAction) -> u64 {
    audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq(action))
        .count(db)
        .await
        .unwrap()
}

#[tokio::test]
async fn primary_start_and_sigterm_persist_lifecycle_audit() {
    lifecycle("primary", false, Some("all")).await;
}

#[tokio::test]
async fn follower_start_and_sigterm_persist_lifecycle_audit() {
    lifecycle("follower", false, Some("all")).await;
}

#[tokio::test]
async fn primary_failed_drain_preserves_recovery_before_exit() {
    lifecycle("primary", true, Some("all")).await;
}

#[tokio::test]
async fn follower_failed_drain_preserves_recovery_before_exit() {
    lifecycle("follower", true, Some("all")).await;
}

#[tokio::test]
async fn default_command_retains_single_process_lifecycle() {
    lifecycle("primary", false, None).await;
}

#[tokio::test]
async fn api_start_and_sigterm_persist_lifecycle_audit() {
    lifecycle("primary", false, Some("api")).await;
}

#[tokio::test]
async fn worker_start_and_sigterm_persist_lifecycle_audit_without_business_routes() {
    lifecycle("primary", false, Some("worker")).await;
}
