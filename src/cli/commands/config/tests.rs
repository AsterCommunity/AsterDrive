use crate::cli::common::database::DatabaseAccess;
use std::sync::Arc;

use aster_drive_migration::Migrator;
use aster_forge_config::{ConfigChangeNotifier, ConfigNotificationSource};

use super::{
    super::args::{ConfigCommand, FileArgs, KeyArgs, KeyValueArgs},
    execute_config_command_with_runtime,
};

async fn test_database(label: &str) -> (String, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "aster-drive-config-cli-{label}-{}.db",
        uuid::Uuid::new_v4()
    ));
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let db = crate::db::connect_with_metrics(
        &crate::config::DatabaseConfig {
            url: url.clone().into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .expect("config CLI test database should connect");
    Migrator::up(&db, None)
        .await
        .expect("config CLI test migrations should apply");
    db.close()
        .await
        .expect("config CLI test database should close");
    (url, path)
}

async fn test_runtime() -> (
    aster_forge_config::ConfigSyncRuntime,
    aster_forge_config::ConfigNotification,
) {
    let notifier = Arc::new(aster_forge_config::InMemoryConfigNotifier::default());
    let subscription = notifier
        .subscribe()
        .await
        .expect("config CLI test should subscribe");
    let runtime = aster_forge_config::ConfigSyncRuntime::with_notifier_for_test(
        crate::services::ops::config::runtime::CONFIG_RELOAD_NAMESPACE,
        "cli-test-runtime",
        notifier as aster_forge_config::SharedConfigChangeNotifier,
    );
    (runtime, subscription)
}

async fn next_reload(
    subscription: &mut aster_forge_config::ConfigNotification,
) -> aster_forge_config::ConfigReloadMessage {
    tokio::time::timeout(std::time::Duration::from_secs(1), subscription.recv())
        .await
        .expect("config CLI mutation should publish reload notification")
        .expect("config CLI reload notification should be readable")
        .reload_message()
        .clone()
}

#[tokio::test]
async fn dry_runs_preserve_config_rows_and_publish_no_reload() {
    let (url, path) = test_database("dry-run").await;
    let database = crate::config::DatabaseConfig {
        url: url.into(),
        pool_size: 1,
        retry_count: 0,
    };
    let db = super::connect_existing_database(&database, DatabaseAccess::ReadWrite)
        .await
        .unwrap();
    let (runtime, mut subscription) = test_runtime().await;
    let mut seeded = super::config_repo::upsert_with_options(
        &db,
        "custom.secret",
        "credential-marker",
        None,
        None,
    )
    .await
    .unwrap();
    use sea_orm::{ActiveModelTrait, Set};
    let mut active: aster_forge_db::system_config::ActiveModel = seeded.clone().into();
    active.is_sensitive = Set(true);
    seeded = active.update(&db).await.unwrap();
    let input = aster_forge_utils::raii::TempFileGuard::new(
        std::env::temp_dir().join(format!("aster-cli-preview-{}.json", uuid::Uuid::new_v4())),
        "CLI dry-run backup",
    );
    std::fs::write(input.path(), r#"{"format":"asterdrive.runtime-config","version":1,"configs":[{"key":"custom.import","value":"new","source":"custom","visibility":"private","is_sensitive":false}],"omitted_sensitive_keys":[]}"#).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(input.path(), std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    for command in [
        ConfigCommand::Set(KeyValueArgs {
            key: KeyArgs {
                key: Some("custom.secret".into()),
                ..Default::default()
            },
            input: super::super::args::ValueInput {
                value: Some("credential-marker".parse().unwrap()),
                ..Default::default()
            },
            dry_run: true,
            yes: false,
        }),
        ConfigCommand::Delete(super::super::args::DeleteArgs {
            key: KeyArgs {
                key: Some("custom.secret".into()),
                ..Default::default()
            },
            dry_run: true,
            yes: false,
        }),
        ConfigCommand::Import(FileArgs {
            input_file: input.path().into(),
            dry_run: true,
            yes: false,
        }),
    ] {
        let report = execute_config_command_with_runtime(&database, &command, Some(&runtime))
            .await
            .unwrap();
        for format in [
            crate::cli::common::output::OutputFormat::Human,
            crate::cli::common::output::OutputFormat::Json,
            crate::cli::common::output::OutputFormat::PrettyJson,
        ] {
            assert!(
                !super::super::render::render_success(format, &report)
                    .contains("credential-marker")
            );
        }
        let rows = super::config_repo::find_all(&db).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], seeded);
    }
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(30), subscription.recv())
            .await
            .is_err()
    );
    db.close().await.unwrap();
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn set_publishes_cli_reload_after_database_write() {
    let (database_url, database_path) = test_database("set").await;
    let (runtime, mut subscription) = test_runtime().await;

    execute_config_command_with_runtime(
        &crate::config::DatabaseConfig {
            url: database_url.clone().into(),
            pool_size: 1,
            retry_count: 0,
        },
        &ConfigCommand::Set(KeyValueArgs {
            key: KeyArgs {
                key: Some("custom.cli_set".to_string()),
                ..Default::default()
            },
            input: super::super::args::ValueInput {
                value: Some("enabled".parse().unwrap()),
                ..Default::default()
            },
            yes: true,
            ..Default::default()
        }),
        Some(&runtime),
    )
    .await
    .expect("config CLI set should succeed");

    let message = next_reload(&mut subscription).await;
    assert_eq!(message.keys, vec!["custom.cli_set"]);
    assert_eq!(message.source, ConfigNotificationSource::Cli);
    std::fs::remove_file(database_path).expect("config CLI test database should clean up");
}

#[tokio::test]
async fn delete_publishes_cli_reload_after_database_delete() {
    let (database_url, database_path) = test_database("delete").await;
    let (runtime, mut subscription) = test_runtime().await;
    execute_config_command_with_runtime(
        &crate::config::DatabaseConfig {
            url: database_url.clone().into(),
            pool_size: 1,
            retry_count: 0,
        },
        &ConfigCommand::Set(KeyValueArgs {
            key: KeyArgs {
                key: Some("custom.cli_delete".to_string()),
                ..Default::default()
            },
            input: super::super::args::ValueInput {
                value: Some("enabled".parse().unwrap()),
                ..Default::default()
            },
            yes: true,
            ..Default::default()
        }),
        Some(&runtime),
    )
    .await
    .expect("config CLI seed should succeed");
    let _ = next_reload(&mut subscription).await;

    execute_config_command_with_runtime(
        &crate::config::DatabaseConfig {
            url: database_url.clone().into(),
            pool_size: 1,
            retry_count: 0,
        },
        &ConfigCommand::Delete(super::super::args::DeleteArgs {
            key: KeyArgs {
                key: Some("custom.cli_delete".to_string()),
                ..Default::default()
            },
            yes: true,
            ..Default::default()
        }),
        Some(&runtime),
    )
    .await
    .expect("config CLI delete should succeed");

    let message = next_reload(&mut subscription).await;
    assert_eq!(message.keys, vec!["custom.cli_delete"]);
    assert_eq!(message.source, ConfigNotificationSource::Cli);
    std::fs::remove_file(database_path).expect("config CLI test database should clean up");
}

#[tokio::test]
async fn import_publishes_one_deduplicated_cli_reload_for_all_keys() {
    let (database_url, database_path) = test_database("import").await;
    let input_file = std::env::temp_dir().join(format!(
        "aster-drive-config-cli-import-{}.json",
        uuid::Uuid::new_v4()
    ));
    std::fs::write(
            &input_file,
            r#"{"format":"asterdrive.runtime-config","version":1,"configs":[{"key":"custom.cli_b","value":"b","source":"custom","visibility":"private","is_sensitive":false},{"key":"custom.cli_a","value":"a","source":"custom","visibility":"private","is_sensitive":false}],"omitted_sensitive_keys":[]}"#,
        )
        .expect("config CLI import fixture should write");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&input_file, std::fs::Permissions::from_mode(0o600))
            .expect("config CLI import fixture should be private");
    }
    let (runtime, mut subscription) = test_runtime().await;

    execute_config_command_with_runtime(
        &crate::config::DatabaseConfig {
            url: database_url.clone().into(),
            pool_size: 1,
            retry_count: 0,
        },
        &ConfigCommand::Import(FileArgs {
            input_file: input_file.clone(),
            dry_run: false,
            yes: true,
        }),
        Some(&runtime),
    )
    .await
    .expect("config CLI import should succeed");

    let message = next_reload(&mut subscription).await;
    assert_eq!(message.keys, vec!["custom.cli_a", "custom.cli_b"]);
    assert_eq!(message.source, ConfigNotificationSource::Cli);
    std::fs::remove_file(input_file).expect("config CLI import fixture should clean up");
    std::fs::remove_file(database_path).expect("config CLI test database should clean up");
}

#[tokio::test]
async fn database_failure_rolls_back_prior_import_writes_and_publishes_no_reload() {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let (database_url, database_path) = test_database("rollback").await;
    let database = crate::config::DatabaseConfig {
        url: database_url.into(),
        pool_size: 1,
        retry_count: 0,
    };
    let db = super::connect_existing_database(&database, DatabaseAccess::ReadWrite)
        .await
        .unwrap();
    db.execute_raw(Statement::from_string(DbBackend::Sqlite,
            "CREATE TRIGGER reject_cli_import BEFORE INSERT ON system_config WHEN NEW.key = 'custom.reject' BEGIN SELECT RAISE(ABORT, 'database rejects second item'); END"))
            .await.unwrap();
    let file = aster_forge_utils::raii::TempFileGuard::new(
        std::env::temp_dir().join(format!("aster-cli-rollback-{}.json", uuid::Uuid::new_v4())),
        "CLI rollback fixture",
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(file.path())
            .unwrap();
    }
    std::fs::write(file.path(), r#"{"format":"asterdrive.runtime-config","version":1,"configs":[{"key":"custom.first","value":"written then rolled back","source":"custom","visibility":"private","is_sensitive":false},{"key":"custom.reject","value":"credential-marker","source":"custom","visibility":"private","is_sensitive":true}],"omitted_sensitive_keys":[]}"#).unwrap();
    let (runtime, mut subscription) = test_runtime().await;
    let result = execute_config_command_with_runtime(
        &database,
        &ConfigCommand::Import(FileArgs {
            input_file: file.path().into(),
            dry_run: false,
            yes: true,
        }),
        Some(&runtime),
    )
    .await;
    let error = result.unwrap_err();
    assert!(!format!("{error:?}").contains("credential-marker"));
    assert!(
        crate::db::repository::config_repo::find_by_key(&db, "custom.first")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(30), subscription.recv())
            .await
            .is_err()
    );
    db.close().await.unwrap();
    std::fs::remove_file(database_path).unwrap();
}

#[tokio::test]
async fn read_commands_and_validate_do_not_insert_defaults_or_mutate_rows() {
    let (database_url, database_path) = test_database("no-read-side-effects").await;
    let database = crate::config::DatabaseConfig {
        url: database_url.into(),
        pool_size: 1,
        retry_count: 0,
    };
    let db = super::connect_existing_database(&database, DatabaseAccess::ReadWrite)
        .await
        .unwrap();
    assert!(
        crate::db::repository::config_repo::find_all(&db)
            .await
            .unwrap()
            .is_empty()
    );
    for command in [
        ConfigCommand::List,
        ConfigCommand::Export(super::super::args::ExportArgs {
            include_sensitive: true,
        }),
        ConfigCommand::Validate(super::super::args::ValidateArgs {
            key: KeyArgs {
                key: Some("custom.preview".into()),
                ..Default::default()
            },
            input_file: None,
            input: super::super::args::ValueInput {
                value: Some("preview".parse().unwrap()),
                ..Default::default()
            },
        }),
    ] {
        execute_config_command_with_runtime(&database, &command, None)
            .await
            .unwrap();
        assert!(
            crate::db::repository::config_repo::find_all(&db)
                .await
                .unwrap()
                .is_empty()
        );
    }
    let context = crate::cli::common::context::CliContext {
        config: crate::config::Config::default(),
        database: database.clone(),
    };
    crate::cli::commands::doctor::execute::execute_doctor_command(
        &crate::cli::commands::doctor::args::DoctorArgs {
            database_url: None,
            strict: false,
            deep: false,
            fix: false,
            dry_run: false,
            yes: false,
            scopes: Vec::new(),
            policy_id: None,
        },
        &context,
    )
    .await;
    assert!(
        crate::db::repository::config_repo::find_all(&db)
            .await
            .unwrap()
            .is_empty()
    );
    db.close().await.unwrap();
    std::fs::remove_file(database_path).unwrap();
}
