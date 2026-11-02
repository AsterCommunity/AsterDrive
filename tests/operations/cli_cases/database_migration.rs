//! CLI database migration integration contracts.
use super::*;

#[tokio::test]
async fn database_migration_dry_run_does_not_create_or_change_target() {
    let source = setup_database_url().await;
    let directory = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!(
            "aster-cli-migration-preview-{}",
            uuid::Uuid::new_v4()
        )),
        "CLI migration dry-run fixture",
    );
    std::fs::create_dir_all(directory.path()).unwrap();
    let path = directory.path().join("target.db");
    let target = format!("sqlite://{}?mode=rwc", path.display());
    let run = || {
        run_aster_drive(&[
            "database-migrate",
            "--source-database-url",
            &source,
            "--target-database-url",
            &target,
            "--dry-run",
        ])
    };
    let missing = run();
    assert!(!missing.status.success());
    assert!(!path.exists());
    std::fs::write(&path, []).unwrap();
    let preview = run();
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let json: Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(json["data"]["mode"], "dry_run");
    assert!(std::fs::read(&path).unwrap().is_empty());
}

#[tokio::test]
async fn test_migrations_use_current_baseline_for_fresh_install() {
    let database_url = setup_empty_database_url("asterdrive-cli-fresh-baseline-test").await;
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: database_url.clone().into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    Migrator::up(&db, None).await.unwrap();
    let versions = applied_migration_versions(&db, DbBackend::Sqlite).await;
    assert_eq!(
        versions,
        aster_drive_migration::current_migration_names(),
        "fresh install should stamp all current migrations"
    );
    for column in [
        "last_probe_error",
        "last_probe_at",
        "tunnel_runtime_error",
        "tunnel_last_handshake_at",
    ] {
        assert!(
            column_exists(&db, DbBackend::Sqlite, "managed_followers", column).await,
            "fresh schema should contain renamed remote-node telemetry column {column}"
        );
    }
    for column in [
        "last_error",
        "last_checked_at",
        "tunnel_last_error",
        "tunnel_last_seen_at",
    ] {
        assert!(
            !column_exists(&db, DbBackend::Sqlite, "managed_followers", column).await,
            "fresh schema should not retain legacy remote-node telemetry column {column}"
        );
    }
}

#[tokio::test]
async fn test_remote_node_telemetry_rename_preserves_values_on_sqlite_up_and_down() {
    let database_url =
        setup_empty_database_url("asterdrive-cli-remote-node-telemetry-rename-test").await;
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: database_url.into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    let migrations = CurrentMigrator::migrations();
    let rename_index = migrations
        .iter()
        .position(|migration| migration.name() == "m20260821_000001_rename_remote_node_telemetry")
        .expect("remote-node telemetry rename migration should be registered");
    CurrentMigrator::up(
        &db,
        Some(u32::try_from(rename_index).expect("migration index should fit u32")),
    )
    .await
    .unwrap();

    db.execute_unprepared(
        "INSERT INTO managed_followers \
         (name, base_url, access_key, secret_key, is_enabled, last_capabilities, \
          last_error, last_checked_at, tunnel_last_error, tunnel_last_seen_at, created_at, updated_at) \
         VALUES ('legacy-node', '', 'legacy-access', 'legacy-secret', 1, '{\"v\":1}', \
                 'probe failed', '2026-08-20T01:02:03Z', 'tunnel failed', '2026-08-20T02:03:04Z', \
                 '2026-08-20T01:02:03Z', '2026-08-20T01:02:03Z')",
    )
    .await
    .unwrap();

    CurrentMigrator::up(&db, None).await.unwrap();
    assert_eq!(
        scalar_string(
            &db,
            DbBackend::Sqlite,
            "SELECT last_probe_error FROM managed_followers WHERE name = 'legacy-node'",
        )
        .await,
        "probe failed"
    );
    assert_eq!(
        scalar_string(
            &db,
            DbBackend::Sqlite,
            "SELECT tunnel_runtime_error FROM managed_followers WHERE name = 'legacy-node'",
        )
        .await,
        "tunnel failed"
    );
    assert_eq!(
        scalar_i64(
            &db,
            DbBackend::Sqlite,
            "SELECT COUNT(*) FROM managed_followers WHERE name = 'legacy-node' AND last_probe_at IS NOT NULL",
        )
        .await,
        1
    );
    assert_eq!(
        scalar_string(
            &db,
            DbBackend::Sqlite,
            "SELECT tunnel_last_handshake_at FROM managed_followers WHERE name = 'legacy-node'",
        )
        .await,
        "2026-08-20T02:03:04Z"
    );
    assert_eq!(
        scalar_i64(
            &db,
            DbBackend::Sqlite,
            "SELECT COUNT(*) FROM managed_followers WHERE name = 'legacy-node' AND tunnel_last_handshake_at IS NULL",
        )
        .await,
        0
    );

    let rollback_steps =
        u32::try_from(migrations.len() - rename_index).expect("rollback step count should fit u32");
    CurrentMigrator::down(&db, Some(rollback_steps))
        .await
        .unwrap();
    assert_eq!(
        scalar_string(
            &db,
            DbBackend::Sqlite,
            "SELECT last_error FROM managed_followers WHERE name = 'legacy-node'",
        )
        .await,
        "probe failed"
    );
    assert_eq!(
        scalar_string(
            &db,
            DbBackend::Sqlite,
            "SELECT tunnel_last_error FROM managed_followers WHERE name = 'legacy-node'",
        )
        .await,
        "tunnel failed"
    );
    assert_eq!(
        scalar_string(
            &db,
            DbBackend::Sqlite,
            "SELECT last_checked_at FROM managed_followers WHERE name = 'legacy-node'",
        )
        .await,
        "2026-08-20T01:02:03Z"
    );
    assert_eq!(
        scalar_string(
            &db,
            DbBackend::Sqlite,
            "SELECT tunnel_last_seen_at FROM managed_followers WHERE name = 'legacy-node'",
        )
        .await,
        "2026-08-20T02:03:04Z"
    );
    assert!(
        column_exists(
            &db,
            DbBackend::Sqlite,
            "managed_followers",
            "last_checked_at"
        )
        .await
    );
}

#[tokio::test]
async fn test_migration_backfills_storage_migration_result_renamed_opaque_count() {
    let database_url =
        setup_empty_database_url("asterdrive-cli-storage-migration-result-backfill-test").await;
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: database_url.into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();

    let migration_count_before_opaque_backfill = u32::try_from(
        aster_drive_migration::current_migration_names()
            .iter()
            .position(|name| name.contains("add_storage_migration_opaque_rename_count"))
            .expect("opaque rename count migration should exist"),
    )
    .expect("migration count should fit u32");
    CurrentMigrator::up(&db, Some(migration_count_before_opaque_backfill))
        .await
        .unwrap();

    let old_result = serde_json::json!({
        "source_policy_id": 1,
        "target_policy_id": 2,
        "scanned_blobs": 3,
        "migrated_blobs": 2,
        "merged_blobs": 1,
        "skipped_blobs": 0,
        "failed_blobs": 0,
        "migrated_bytes": 4096
    });
    let now = Utc::now();
    let payload_json = serde_json::json!({
        "source_policy_id": 1,
        "target_policy_id": 2,
        "plan_hash": "0".repeat(64),
        "source_policy_updated_at": now,
        "target_policy_updated_at": now
    })
    .to_string();
    // This fixture intentionally targets the historical schema before
    // `runtime_json` existed, so it must not use the current ActiveModel.
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO background_tasks (
            kind, status, creator_user_id, team_id, share_id, display_name,
            payload_json, result_json, steps_json, progress_current, progress_total,
            status_text, attempt_count, max_attempts, next_run_at, processing_token,
            processing_started_at, last_heartbeat_at, lease_expires_at, started_at,
            finished_at, last_error, failure_can_retry, expires_at, created_at, updated_at
        ) VALUES (
            ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
        )",
        vec![
            aster_drive_model::types::BackgroundTaskKind::StoragePolicyMigration
                .as_str()
                .into(),
            aster_drive_model::types::BackgroundTaskStatus::Succeeded
                .as_str()
                .into(),
            Option::<i64>::None.into(),
            Option::<i64>::None.into(),
            Option::<i64>::None.into(),
            "legacy storage migration".into(),
            payload_json.into(),
            Some(old_result.to_string()).into(),
            Option::<String>::None.into(),
            3_i64.into(),
            3_i64.into(),
            Option::<String>::None.into(),
            1_i64.into(),
            3_i64.into(),
            now.into(),
            0_i64.into(),
            Option::<chrono::DateTime<Utc>>::None.into(),
            Option::<chrono::DateTime<Utc>>::None.into(),
            Option::<chrono::DateTime<Utc>>::None.into(),
            Some(now).into(),
            Some(now).into(),
            Option::<String>::None.into(),
            Option::<bool>::None.into(),
            (now + Duration::hours(24)).into(),
            now.into(),
            now.into(),
        ],
    ))
    .await
    .unwrap();

    CurrentMigrator::up(&db, None).await.unwrap();

    let task = aster_drive_model::entities::background_task::Entity::find()
        .one(&db)
        .await
        .unwrap()
        .expect("legacy task should remain");
    let result = task
        .result_json
        .as_ref()
        .expect("legacy task result should remain");
    let parsed: aster_drive::services::task::types::StoragePolicyMigrationTaskResult =
        serde_json::from_str(result.as_ref())
            .expect("backfilled storage migration result should match current schema");
    assert_eq!(parsed.renamed_opaque_blobs, 0);
}

#[tokio::test]
async fn test_migrations_reject_unsupported_historical_migration_history() {
    let database_url = setup_empty_database_url("asterdrive-cli-old-migration-test").await;
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: database_url.clone().into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    Migrator::up(&db, None).await.unwrap();
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO seaql_migrations (version, applied_at) VALUES (?, ?)",
        ["m20260502_000001_baseline_schema".into(), 1_i64.into()],
    ))
    .await
    .unwrap();
    db.close().await.unwrap();

    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: database_url.into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    let error = Migrator::up(&db, None)
        .await
        .expect_err("unsupported historical migration rows should be rejected");
    let stderr = error.to_string();
    assert!(
        stderr.contains("m20260502_000001_baseline_schema"),
        "error should include the unsupported migration name: {stderr}"
    );
    assert!(
        stderr.contains("unknown migration versions"),
        "error should explain that historical migration rows are no longer accepted: {stderr}"
    );
}

#[tokio::test]
async fn test_migrations_reject_non_prefix_current_migration_history() {
    let database_url = setup_empty_database_url("asterdrive-cli-non-prefix-migration-test").await;
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: database_url.into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    Migrator::up(&db, None).await.unwrap();
    let current_names = aster_drive_migration::current_migration_names();
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "DELETE FROM seaql_migrations WHERE version = ?",
        [current_names[0].clone().into()],
    ))
    .await
    .unwrap();

    let history = aster_drive_migration::inspect_migration_history(&db)
        .await
        .unwrap();
    assert_eq!(
        history.track,
        aster_drive_migration::MigrationTrack::Unknown
    );

    let error = Migrator::up(&db, None)
        .await
        .expect_err("non-prefix migration history should be rejected");
    let stderr = error.to_string();
    assert!(
        stderr.contains("non-prefix migration history"),
        "error should identify the non-prefix migration history: {stderr}"
    );
}

#[tokio::test]
async fn test_migrations_reject_existing_schema_with_empty_history() {
    let database_url = setup_empty_database_url("asterdrive-cli-empty-history-schema-test").await;
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: database_url.into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    db.execute_raw(Statement::from_string(
        DbBackend::Sqlite,
        "CREATE TABLE users (id INTEGER PRIMARY KEY)".to_string(),
    ))
    .await
    .unwrap();

    let history = aster_drive_migration::inspect_migration_history(&db)
        .await
        .unwrap();
    assert_eq!(
        history.track,
        aster_drive_migration::MigrationTrack::Unknown
    );

    let error = Migrator::up(&db, None)
        .await
        .expect_err("existing schema with empty history should be rejected");
    let stderr = error.to_string();
    assert!(
        stderr.contains("empty migration history with existing schema objects"),
        "error should identify existing schema objects without migration history: {stderr}"
    );
}

#[tokio::test]
async fn test_root_binary_database_migrate_rejects_current_history_with_missing_runtime_lease_table()
 {
    let source_database_url = setup_database_url().await;
    let target_database_url = setup_database_url().await;
    let target_db = db::connect_with_metrics(
        &DatabaseConfig {
            url: target_database_url.clone().into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    target_db
        .execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "DROP TABLE runtime_leases",
        ))
        .await
        .unwrap();
    target_db.close().await.unwrap();

    let output = run_aster_drive(&[
        "database-migrate",
        "--verify-only",
        "--source-database-url",
        &source_database_url,
        "--target-database-url",
        &target_database_url,
    ]);

    assert!(
        !output.status.success(),
        "verify-only must reject migration history/schema drift"
    );
    let error_json: Value = serde_json::from_slice(&output.stderr)
        .expect("schema drift failure should remain structured JSON");
    assert!(
        error_json["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains(aster_forge_db::RUNTIME_LEASES_TABLE)),
        "schema drift error should name the missing runtime lease table: {error_json}"
    );
}

#[tokio::test]
async fn test_root_binary_database_migrate_sqlite_to_postgres_happy_path() {
    let source_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-migrate-{}.db",
        uuid::Uuid::new_v4()
    ));
    let source_database_url = format!("sqlite://{}?mode=rwc", source_db_path.display());
    let file_id = seed_migration_fixture(&source_database_url).await;

    let target_database_url = common::postgres_test_database_url().await;

    let output = run_aster_drive(&[
        "database-migrate",
        "--source-database-url",
        &source_database_url,
        "--target-database-url",
        &target_database_url,
    ]);
    assert!(
        output.status.success(),
        "database-migrate stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output_json: Value =
        serde_json::from_slice(&output.stdout).expect("database-migrate output should be json");
    assert_eq!(output_json["ok"], true);
    assert_eq!(output_json["data"]["mode"], "apply");
    assert_eq!(output_json["data"]["ready_to_cutover"], true);
    assert_eq!(output_json["data"]["rolled_back"], false);
    assert_eq!(output_json["data"]["resume"]["enabled"], true);
    assert_eq!(output_json["data"]["resume"]["resumed"], false);
    let migrated_tables = output_json["data"]["tables"]
        .as_array()
        .expect("database-migrate tables should be an array");
    assert!(
        migrated_tables
            .iter()
            .any(|table| table["name"] == aster_forge_db::SCHEDULED_TASKS_TABLE)
    );
    assert!(
        migrated_tables
            .iter()
            .any(|table| table["name"] == aster_forge_db::RUNTIME_LEASES_TABLE)
    );
    assert_eq!(
        output_json["data"]["source"]["database_url"],
        format!(
            "sqlite:///.../{}?mode=rwc",
            source_db_path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("source db file name should be valid utf-8")
        )
    );
    assert_eq!(
        output_json["data"]["target"]["database_url"],
        redact_database_url(&target_database_url)
    );

    let target_db = db::connect_with_metrics(
        &DatabaseConfig {
            url: target_database_url.clone().into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    let checkpoint_source_url = scalar_string(
        &target_db,
        DbBackend::Postgres,
        "SELECT source_database_url FROM aster_cli_database_migrations LIMIT 1",
    )
    .await;
    let checkpoint_target_url = scalar_string(
        &target_db,
        DbBackend::Postgres,
        "SELECT target_database_url FROM aster_cli_database_migrations LIMIT 1",
    )
    .await;
    assert_eq!(
        checkpoint_source_url,
        output_json["data"]["source"]["database_url"]
    );
    assert_eq!(
        checkpoint_target_url,
        output_json["data"]["target"]["database_url"]
    );

    assert_migrated_fixture(&target_database_url, DbBackend::Postgres, file_id).await;
}

#[tokio::test]
async fn test_root_binary_database_migrate_postgres_to_mysql_with_progress() {
    let source = async {
        let database_url = common::postgres_test_database_url().await;
        let file_id = seed_migration_fixture(&database_url).await;
        (database_url, file_id)
    };
    let ((source_database_url, file_id), target_database_url) =
        tokio::join!(source, common::mysql_test_database_url());

    let output = run_aster_drive_with_env(
        &[
            "database-migrate",
            "--source-database-url",
            &source_database_url,
            "--target-database-url",
            &target_database_url,
        ],
        &[
            ("ASTER_CLI_PROGRESS", "1"),
            ("ASTER_CLI_COPY_BATCH_SIZE", "1"),
        ],
    );
    assert!(
        output.status.success(),
        "database-migrate stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output_json: Value =
        serde_json::from_slice(&output.stdout).expect("database-migrate output should be json");
    assert_eq!(output_json["ok"], true);
    assert_eq!(output_json["data"]["ready_to_cutover"], true);
    assert_eq!(output_json["data"]["resume"]["resumed"], false);

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf-8");
    assert!(stderr.contains("[database-migrate] data_copy:"));

    assert_migrated_fixture(&target_database_url, DbBackend::MySql, file_id).await;
}

#[tokio::test]
async fn test_root_binary_database_migrate_mysql_to_sqlite_happy_path() {
    let source_database_url = common::mysql_test_database_url().await;
    let file_id = seed_migration_fixture(&source_database_url).await;

    let target_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-migrate-target-{}.db",
        uuid::Uuid::new_v4()
    ));
    let target_database_url = format!("sqlite://{}?mode=rwc", target_db_path.display());

    let output = run_aster_drive(&[
        "database-migrate",
        "--source-database-url",
        &source_database_url,
        "--target-database-url",
        &target_database_url,
    ]);
    assert!(
        output.status.success(),
        "database-migrate stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output_json: Value =
        serde_json::from_slice(&output.stdout).expect("database-migrate output should be json");
    assert_eq!(output_json["ok"], true);
    assert_eq!(output_json["data"]["ready_to_cutover"], true);

    assert_migrated_fixture(&target_database_url, DbBackend::Sqlite, file_id).await;
}

#[tokio::test]
async fn test_root_binary_database_migrate_sqlite_resume_from_checkpoint() {
    let source_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-resume-source-{}.db",
        uuid::Uuid::new_v4()
    ));
    let source_database_url = format!("sqlite://{}?mode=rwc", source_db_path.display());
    let file_id = seed_migration_fixture(&source_database_url).await;

    let target_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-resume-target-{}.db",
        uuid::Uuid::new_v4()
    ));
    let target_database_url = format!("sqlite://{}?mode=rwc", target_db_path.display());

    let first_output = run_aster_drive_with_env(
        &[
            "database-migrate",
            "--source-database-url",
            &source_database_url,
            "--target-database-url",
            &target_database_url,
        ],
        &[
            ("ASTER_CLI_COPY_BATCH_SIZE", "1"),
            ("ASTER_CLI_FAIL_AFTER_BATCHES", "1"),
        ],
    );
    assert!(
        !first_output.status.success(),
        "first migration should fail to exercise resume"
    );
    let error_json: Value =
        serde_json::from_slice(&first_output.stderr).expect("error stderr should stay json");
    assert_eq!(error_json["ok"], false);
    assert!(
        error_json["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("forced failure")
    );

    let target_db = db::connect_with_metrics(
        &DatabaseConfig {
            url: target_database_url.clone().into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    let checkpoint_rows = scalar_i64(
        &target_db,
        DbBackend::Sqlite,
        "SELECT COUNT(*) FROM aster_cli_database_migrations",
    )
    .await;
    assert_eq!(checkpoint_rows, 1);
    let checkpoint_source_url = scalar_string(
        &target_db,
        DbBackend::Sqlite,
        "SELECT source_database_url FROM aster_cli_database_migrations LIMIT 1",
    )
    .await;
    let checkpoint_target_url = scalar_string(
        &target_db,
        DbBackend::Sqlite,
        "SELECT target_database_url FROM aster_cli_database_migrations LIMIT 1",
    )
    .await;
    assert_ne!(checkpoint_source_url, source_database_url);
    assert_ne!(checkpoint_target_url, target_database_url);
    assert!(checkpoint_source_url.contains("/.../"));
    assert!(checkpoint_target_url.contains("/.../"));
    assert!(
        checkpoint_source_url.contains(
            source_db_path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("source db file name should be valid utf-8")
        )
    );
    assert!(
        checkpoint_target_url.contains(
            target_db_path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("target db file name should be valid utf-8")
        )
    );

    let second_output = run_aster_drive_with_env(
        &[
            "database-migrate",
            "--source-database-url",
            &source_database_url,
            "--target-database-url",
            &target_database_url,
        ],
        &[("ASTER_CLI_COPY_BATCH_SIZE", "1")],
    );
    assert!(
        second_output.status.success(),
        "resume stderr: {}",
        String::from_utf8_lossy(&second_output.stderr)
    );

    let output_json: Value =
        serde_json::from_slice(&second_output.stdout).expect("resume output should be json");
    assert_eq!(output_json["ok"], true);
    assert_eq!(output_json["data"]["ready_to_cutover"], true);
    assert_eq!(output_json["data"]["resume"]["enabled"], true);
    assert_eq!(output_json["data"]["resume"]["resumed"], true);

    assert_migrated_fixture(&target_database_url, DbBackend::Sqlite, file_id).await;
}

#[tokio::test]
async fn test_root_binary_database_migrate_sqlite_urls_without_mode_default_to_rwc() {
    let source_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-source-no-mode-{}.db",
        uuid::Uuid::new_v4()
    ));
    let source_database_url_with_mode = format!("sqlite://{}?mode=rwc", source_db_path.display());
    let file_id = seed_migration_fixture(&source_database_url_with_mode).await;
    let source_database_url = format!("sqlite://{}", source_db_path.display());

    let target_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-target-no-mode-{}.db",
        uuid::Uuid::new_v4()
    ));
    let target_database_url = format!("sqlite://{}", target_db_path.display());

    let output = run_aster_drive(&[
        "database-migrate",
        "--source-database-url",
        &source_database_url,
        "--target-database-url",
        &target_database_url,
    ]);
    assert!(
        output.status.success(),
        "database-migrate stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output_json: Value =
        serde_json::from_slice(&output.stdout).expect("database-migrate output should be json");
    assert_eq!(output_json["ok"], true);
    assert_eq!(output_json["data"]["ready_to_cutover"], true);

    assert_migrated_fixture(
        &format!("{target_database_url}?mode=rwc"),
        DbBackend::Sqlite,
        file_id,
    )
    .await;
}

#[tokio::test]
async fn test_root_binary_database_migrate_allows_consumed_contact_verification_history() {
    let source_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-contact-history-source-{}.db",
        uuid::Uuid::new_v4()
    ));
    let source_database_url = format!("sqlite://{}?mode=rwc", source_db_path.display());
    let file_id = seed_migration_fixture(&source_database_url).await;
    seed_contact_verification_history(&source_database_url).await;

    let target_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-contact-history-target-{}.db",
        uuid::Uuid::new_v4()
    ));
    let target_database_url = format!("sqlite://{}?mode=rwc", target_db_path.display());

    let output = run_aster_drive(&[
        "database-migrate",
        "--source-database-url",
        &source_database_url,
        "--target-database-url",
        &target_database_url,
    ]);
    assert!(
        output.status.success(),
        "database-migrate stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output_json: Value =
        serde_json::from_slice(&output.stdout).expect("database-migrate output should be json");
    assert_eq!(output_json["ok"], true);
    assert_eq!(output_json["data"]["ready_to_cutover"], true);
    assert_eq!(
        output_json["data"]["verification"]["unique_conflicts"]
            .as_array()
            .map(Vec::len),
        Some(0)
    );

    let target_db = db::connect_with_metrics(
        &DatabaseConfig {
            url: target_database_url.clone().into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    let active_password_reset_tokens = scalar_i64(
        &target_db,
        DbBackend::Sqlite,
        "SELECT COUNT(*) FROM contact_verification_tokens \
         WHERE channel = 'email' AND purpose = 'password_reset' AND consumed_at IS NULL",
    )
    .await;
    let historical_contact_change_tokens = scalar_i64(
        &target_db,
        DbBackend::Sqlite,
        "SELECT COUNT(*) FROM contact_verification_tokens \
         WHERE channel = 'email' AND purpose = 'contact_change' AND consumed_at IS NOT NULL",
    )
    .await;
    assert_eq!(active_password_reset_tokens, 1);
    assert_eq!(historical_contact_change_tokens, 2);

    assert_migrated_fixture(&target_database_url, DbBackend::Sqlite, file_id).await;
}

#[tokio::test]
async fn test_root_binary_database_migrate_human_output_is_readable() {
    let source_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-human-source-{}.db",
        uuid::Uuid::new_v4()
    ));
    let source_database_url = format!("sqlite://{}?mode=rwc", source_db_path.display());
    let file_id = seed_migration_fixture(&source_database_url).await;

    let target_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-human-target-{}.db",
        uuid::Uuid::new_v4()
    ));
    let target_database_url = format!("sqlite://{}?mode=rwc", target_db_path.display());

    let output = run_aster_drive_with_env(
        &[
            "database-migrate",
            "--output-format",
            "human",
            "--source-database-url",
            &source_database_url,
            "--target-database-url",
            &target_database_url,
        ],
        &[("ASTER_CLI_PROGRESS", "1")],
    );
    assert!(
        output.status.success(),
        "database-migrate stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("human stdout should be utf-8");
    assert!(stdout.contains("Database migration complete"));
    assert!(stdout.contains("Stages:"));
    assert!(stdout.contains("Source:"));
    assert!(stdout.contains("Target:"));
    assert!(stdout.contains("Cutover:"));
    assert!(stdout.contains("[OK] ready"));
    assert!(stdout.contains("Verification:"));
    assert!(serde_json::from_str::<Value>(&stdout).is_err());

    let stderr = String::from_utf8(output.stderr).expect("human stderr should be utf-8");
    assert!(stderr.contains("[database-migrate] data_copy: ["));

    assert_migrated_fixture(&target_database_url, DbBackend::Sqlite, file_id).await;
}

#[tokio::test]
async fn test_root_binary_database_migrate_human_output_supports_forced_color() {
    let source_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-human-color-source-{}.db",
        uuid::Uuid::new_v4()
    ));
    let source_database_url = format!("sqlite://{}?mode=rwc", source_db_path.display());
    let _ = seed_migration_fixture(&source_database_url).await;

    let target_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-human-color-target-{}.db",
        uuid::Uuid::new_v4()
    ));
    let target_database_url = format!("sqlite://{}?mode=rwc", target_db_path.display());

    let output = run_aster_drive_with_env(
        &[
            "database-migrate",
            "--output-format",
            "human",
            "--source-database-url",
            &source_database_url,
            "--target-database-url",
            &target_database_url,
        ],
        &[("ASTER_CLI_PROGRESS", "1"), ("CLICOLOR_FORCE", "1")],
    );
    assert!(
        output.status.success(),
        "database-migrate stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("forced-color stdout should be utf-8");
    let stderr = String::from_utf8(output.stderr).expect("forced-color stderr should be utf-8");
    assert!(stdout.contains("\u{1b}["));
    assert!(stderr.contains("\u{1b}["));
}

#[tokio::test]
async fn migration_preflight_checks_legacy_data_before_accepting_only_empty_legacy_tables() {
    let source = setup_database_url().await;
    let target = setup_database_url().await;
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: source.clone().into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    let run = || {
        run_aster_drive(&[
            "database-migrate",
            "--source-database-url",
            &source,
            "--target-database-url",
            &target,
            "--dry-run",
        ])
    };
    for table in [
        "storage_policy_credentials",
        "storage_connector_application_configs",
    ] {
        db.execute_unprepared(&format!(
            "CREATE TABLE {table} (id INTEGER PRIMARY KEY, secret TEXT)"
        ))
        .await
        .unwrap();
        let empty = run();
        assert!(
            empty.status.success(),
            "{}",
            String::from_utf8_lossy(&empty.stderr)
        );
        let report: Value = serde_json::from_slice(&empty.stdout).unwrap();
        assert!(
            !report["data"]["tables"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["name"] == table)
        );
        db.execute_unprepared(&format!(
            "INSERT INTO {table} VALUES (1, 'legacy-secret-marker')"
        ))
        .await
        .unwrap();
        let populated = run();
        assert!(!populated.status.success());
        assert!(populated.stdout.is_empty());
        let error: Value = serde_json::from_slice(&populated.stderr).unwrap();
        let message = error["error"]["message"].as_str().unwrap();
        assert!(
            message.contains("unmigrated rows")
                && message.contains(table)
                && message.contains("0.5.x"),
            "{message}"
        );
        assert!(!message.contains("legacy-secret-marker"));
        assert_eq!(
            scalar_i64(
                &db,
                DbBackend::Sqlite,
                &format!("SELECT COUNT(*) FROM {table}")
            )
            .await,
            1
        );
        db.execute_unprepared(&format!("DELETE FROM {table}"))
            .await
            .unwrap();
    }
    db.execute_unprepared("ALTER TABLE storage_policies ADD COLUMN access_key TEXT")
        .await
        .unwrap();
    db.execute_unprepared("INSERT INTO storage_policies (name, connector_id, access_key, created_at, updated_at) VALUES ('legacy', 'asterdrive.storage.local', 'static-secret-marker', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)").await.unwrap();
    let static_credentials = run();
    assert!(!static_credentials.status.success());
    let error: Value = serde_json::from_slice(&static_credentials.stderr).unwrap();
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unmigrated static storage credential")
    );
    assert!(!String::from_utf8_lossy(&static_credentials.stderr).contains("static-secret-marker"));
    db.execute_unprepared("DELETE FROM storage_policies WHERE name = 'legacy'")
        .await
        .unwrap();
    db.execute_unprepared("CREATE TABLE unknown_product_data (id INTEGER)")
        .await
        .unwrap();
    let unknown = run();
    assert!(!unknown.status.success());
    let error: Value = serde_json::from_slice(&unknown.stderr).unwrap();
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unsupported tables")
    );
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unknown_product_data")
    );
    db.close().await.unwrap();
}
