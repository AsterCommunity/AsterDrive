//! 集成测试：`migration`。

use migration::{CurrentMigrator, MigratorTrait};
use sea_orm::{ConnectionTrait, Database, DatabaseConnection, DbBackend, DbErr, Statement};

const ALLOW_SHARED_WEBDAV_LOCKS_MIGRATION: &str = "m20260604_000001_allow_shared_webdav_locks";
const RENAME_UPLOAD_SESSION_OBJECT_FIELDS_MIGRATION: &str =
    "m20260618_000001_rename_upload_session_object_fields";
const ADD_STORAGE_CONNECTOR_APPLICATION_CONFIGS_MIGRATION: &str =
    "m20260619_000001_add_storage_connector_application_configs";
const ENFORCE_JSON_TEXT_NOT_NULL_MIGRATION: &str = "m20260620_000001_enforce_json_text_not_null";
const DROP_REMOTE_STORAGE_TARGET_MAX_FILE_SIZE_MIGRATION: &str =
    "m20260705_000001_drop_remote_storage_target_max_file_size";

async fn setup_current_schema() -> sea_orm::DatabaseConnection {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("sqlite memory database should connect");
    CurrentMigrator::up(&db, None)
        .await
        .expect("current migrations should apply");
    db
}

fn steps_to_roll_back_migration(migration_name: &str) -> u32 {
    let migrations = CurrentMigrator::migrations();
    let position = migrations
        .iter()
        .position(|migration| migration.name() == migration_name)
        .unwrap_or_else(|| panic!("{migration_name} migration should be registered"));
    u32::try_from(migrations.len() - position)
        .expect("migration rollback step count should fit u32")
}

fn steps_to_roll_back_allow_shared_webdav_locks() -> u32 {
    steps_to_roll_back_migration(ALLOW_SHARED_WEBDAV_LOCKS_MIGRATION)
}

fn steps_to_roll_back_upload_session_object_fields() -> u32 {
    steps_to_roll_back_migration(RENAME_UPLOAD_SESSION_OBJECT_FIELDS_MIGRATION)
}

fn steps_to_roll_back_storage_connector_application_configs() -> u32 {
    steps_to_roll_back_migration(ADD_STORAGE_CONNECTOR_APPLICATION_CONFIGS_MIGRATION)
}

fn steps_to_roll_back_remote_storage_target_max_file_size() -> u32 {
    steps_to_roll_back_migration(DROP_REMOTE_STORAGE_TARGET_MAX_FILE_SIZE_MIGRATION)
}

async fn roll_back_allow_shared_webdav_locks(
    db: &sea_orm::DatabaseConnection,
) -> Result<(), DbErr> {
    CurrentMigrator::down(db, Some(steps_to_roll_back_allow_shared_webdav_locks())).await
}

async fn insert_resource_lock(
    db: &sea_orm::DatabaseConnection,
    token: &str,
    entity_type: &str,
    entity_id: i64,
) {
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        r#"
        INSERT INTO resource_locks (
            token, entity_type, entity_id, path, owner_id, owner_info,
            timeout_at, shared, deep, created_at
        )
        VALUES (?, ?, ?, ?, NULL, NULL, NULL, 0, 0, datetime('now'))
        "#,
        [
            token.into(),
            entity_type.into(),
            entity_id.into(),
            format!("/locks/{entity_type}/{entity_id}/{token}").into(),
        ],
    ))
    .await
    .expect("resource lock fixture should insert");
}

async fn sqlite_index_exists(db: &DatabaseConnection, index_name: &str) -> bool {
    db.query_all_raw(Statement::from_string(
        DbBackend::Sqlite,
        "PRAGMA index_list('resource_locks')",
    ))
    .await
    .expect("sqlite index list should load")
    .into_iter()
    .any(|row| row.try_get_by_index::<String>(1).as_deref() == Ok(index_name))
}

async fn sqlite_table_columns(db: &DatabaseConnection, table_name: &str) -> Vec<String> {
    db.query_all_raw(Statement::from_string(
        DbBackend::Sqlite,
        format!("PRAGMA table_info('{table_name}')"),
    ))
    .await
    .expect("sqlite table column list should load")
    .into_iter()
    .map(|row| {
        row.try_get_by_index::<String>(1)
            .expect("sqlite PRAGMA table_info row should include column name")
    })
    .collect()
}

async fn sqlite_table_exists(db: &DatabaseConnection, table_name: &str) -> bool {
    db.query_all_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?",
        [table_name.into()],
    ))
    .await
    .expect("sqlite table lookup should load")
    .into_iter()
    .next()
    .is_some()
}

async fn sqlite_column_is_not_null(
    db: &DatabaseConnection,
    table_name: &str,
    column_name: &str,
) -> bool {
    db.query_all_raw(Statement::from_string(
        DbBackend::Sqlite,
        format!("PRAGMA table_info('{table_name}')"),
    ))
    .await
    .expect("sqlite table column metadata should load")
    .into_iter()
    .find_map(|row| {
        let name = row
            .try_get_by_index::<String>(1)
            .expect("sqlite PRAGMA table_info row should include column name");
        (name == column_name).then(|| {
            row.try_get_by_index::<i32>(3)
                .expect("sqlite PRAGMA table_info row should include notnull flag")
                != 0
        })
    })
    .unwrap_or(false)
}

fn has_column(columns: &[String], expected: &str) -> bool {
    columns.iter().any(|column| column == expected)
}

#[tokio::test]
async fn json_text_columns_are_not_null_in_current_schema() {
    assert!(
        CurrentMigrator::migrations()
            .iter()
            .any(|migration| migration.name() == ENFORCE_JSON_TEXT_NOT_NULL_MIGRATION),
        "JSON text constraint migration should be registered"
    );

    let db = setup_current_schema().await;
    for (table, column) in [
        ("external_auth_providers", "options"),
        ("storage_policy_credentials", "metadata"),
        ("storage_policy_authorization_flows", "context"),
        ("storage_connector_application_configs", "metadata"),
    ] {
        assert!(
            sqlite_column_is_not_null(&db, table, column).await,
            "{table}.{column} should be NOT NULL"
        );
    }
}

#[tokio::test]
async fn storage_connector_application_config_migration_adds_canonical_config_table() {
    assert!(
        CurrentMigrator::migrations().iter().any(
            |migration| migration.name() == ADD_STORAGE_CONNECTOR_APPLICATION_CONFIGS_MIGRATION
        ),
        "application config migration should be registered"
    );

    let db = setup_current_schema().await;
    assert!(
        sqlite_table_exists(&db, "storage_connector_application_configs").await,
        "current schema should include storage_connector_application_configs"
    );
    let current_columns = sqlite_table_columns(&db, "storage_connector_application_configs").await;
    for expected in [
        "id",
        "policy_id",
        "provider",
        "tenant_id",
        "scopes",
        "client_id",
        "client_secret_ciphertext",
        "metadata",
        "created_at",
        "updated_at",
    ] {
        assert!(has_column(&current_columns, expected), "missing {expected}");
    }

    CurrentMigrator::down(
        &db,
        Some(steps_to_roll_back_storage_connector_application_configs()),
    )
    .await
    .expect("application config migration should roll back");
    assert!(
        !sqlite_table_exists(&db, "storage_connector_application_configs").await,
        "rollback should remove storage_connector_application_configs"
    );

    CurrentMigrator::up(
        &db,
        Some(steps_to_roll_back_storage_connector_application_configs()),
    )
    .await
    .expect("application config migration should reapply");
    assert!(
        sqlite_table_exists(&db, "storage_connector_application_configs").await,
        "reapply should recreate storage_connector_application_configs"
    );
}

#[tokio::test]
async fn upload_session_object_field_migration_renames_legacy_columns() {
    assert!(
        CurrentMigrator::migrations()
            .iter()
            .any(|migration| migration.name() == RENAME_UPLOAD_SESSION_OBJECT_FIELDS_MIGRATION),
        "object field rename migration should be registered"
    );

    let db = setup_current_schema().await;
    let current_columns = sqlite_table_columns(&db, "upload_sessions").await;
    assert!(has_column(&current_columns, "object_temp_key"));
    assert!(has_column(&current_columns, "object_multipart_id"));
    assert!(!has_column(&current_columns, "s3_temp_key"));
    assert!(!has_column(&current_columns, "s3_multipart_id"));

    CurrentMigrator::down(&db, Some(steps_to_roll_back_upload_session_object_fields()))
        .await
        .expect("object field rename migration should roll back");
    let rolled_back_columns = sqlite_table_columns(&db, "upload_sessions").await;
    assert!(has_column(&rolled_back_columns, "s3_temp_key"));
    assert!(has_column(&rolled_back_columns, "s3_multipart_id"));
    assert!(!has_column(&rolled_back_columns, "object_temp_key"));
    assert!(!has_column(&rolled_back_columns, "object_multipart_id"));

    CurrentMigrator::up(&db, Some(steps_to_roll_back_upload_session_object_fields()))
        .await
        .expect("object field rename migration should reapply");
    let reapplied_columns = sqlite_table_columns(&db, "upload_sessions").await;
    assert!(has_column(&reapplied_columns, "object_temp_key"));
    assert!(has_column(&reapplied_columns, "object_multipart_id"));
    assert!(!has_column(&reapplied_columns, "s3_temp_key"));
    assert!(!has_column(&reapplied_columns, "s3_multipart_id"));
}

#[tokio::test]
async fn remote_storage_target_max_file_size_migration_removes_target_level_limit() {
    assert!(
        CurrentMigrator::migrations().iter().any(
            |migration| migration.name() == DROP_REMOTE_STORAGE_TARGET_MAX_FILE_SIZE_MIGRATION
        ),
        "remote storage target max_file_size drop migration should be registered"
    );

    let db = setup_current_schema().await;
    let current_columns = sqlite_table_columns(&db, "remote_storage_targets").await;
    assert!(has_column(&current_columns, "target_key"));
    assert!(
        !has_column(&current_columns, "max_file_size"),
        "current schema should not store target-level max_file_size"
    );

    CurrentMigrator::down(
        &db,
        Some(steps_to_roll_back_remote_storage_target_max_file_size()),
    )
    .await
    .expect("max_file_size drop migration should roll back");
    let rolled_back_columns = sqlite_table_columns(&db, "remote_storage_targets").await;
    assert!(
        has_column(&rolled_back_columns, "max_file_size"),
        "rollback should restore the legacy target-level max_file_size column"
    );

    CurrentMigrator::up(
        &db,
        Some(steps_to_roll_back_remote_storage_target_max_file_size()),
    )
    .await
    .expect("max_file_size drop migration should reapply");
    let reapplied_columns = sqlite_table_columns(&db, "remote_storage_targets").await;
    assert!(
        !has_column(&reapplied_columns, "max_file_size"),
        "reapply should remove target-level max_file_size again"
    );
}

#[tokio::test]
async fn allow_shared_webdav_locks_down_recreates_unique_index_without_duplicates() {
    let db = setup_current_schema().await;
    insert_resource_lock(&db, "urn:uuid:one", "file", 1).await;
    insert_resource_lock(&db, "urn:uuid:two", "file", 2).await;

    roll_back_allow_shared_webdav_locks(&db)
        .await
        .expect("migration should roll back when resource locks are unique");

    let duplicate_insert = db
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            r#"
            INSERT INTO resource_locks (
                token, entity_type, entity_id, path, owner_id, owner_info,
                timeout_at, shared, deep, created_at
            )
            VALUES (?, 'file', 1, '/locks/file/1/duplicate', NULL, NULL, NULL, 0, 0, datetime('now'))
            "#,
            ["urn:uuid:duplicate".into()],
        ))
        .await;

    assert!(
        duplicate_insert.is_err(),
        "rollback should restore the unique resource_locks(entity_type, entity_id) index"
    );
}

#[tokio::test]
async fn allow_shared_webdav_locks_down_reports_duplicate_entity_locks() {
    let db = setup_current_schema().await;
    insert_resource_lock(&db, "urn:uuid:one", "file", 1).await;
    insert_resource_lock(&db, "urn:uuid:two", "file", 1).await;

    let error = roll_back_allow_shared_webdav_locks(&db)
        .await
        .expect_err("duplicates should block rollback");
    let DbErr::Migration(message) = error else {
        panic!("expected migration error, got {error:?}");
    };

    assert!(message.contains("cannot recreate unique index idx_resource_locks_entity"));
    assert!(message.contains("file:1 (2 locks)"));
    assert!(
        sqlite_index_exists(&db, "idx_resource_locks_entity").await,
        "failed rollback must not drop idx_resource_locks_entity before duplicate validation"
    );
}
