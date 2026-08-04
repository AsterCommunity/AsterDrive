//! 集成测试：`migration`。

use crate::common;

use aster_drive_migration::{CurrentMigrator, MigratorTrait};
use sea_orm::{ConnectionTrait, Database, DatabaseConnection, DbBackend, DbErr, Statement};

const ALLOW_SHARED_WEBDAV_LOCKS_MIGRATION: &str = "m20260604_000001_allow_shared_webdav_locks";
const RENAME_UPLOAD_SESSION_OBJECT_FIELDS_MIGRATION: &str =
    "m20260618_000001_rename_upload_session_object_fields";
const ADD_STORAGE_CONNECTOR_APPLICATION_CONFIGS_MIGRATION: &str =
    "m20260619_000001_add_storage_connector_application_configs";
const ENFORCE_JSON_TEXT_NOT_NULL_MIGRATION: &str = "m20260620_000001_enforce_json_text_not_null";
const RENAME_MANAGED_INGRESS_PROFILES_MIGRATION: &str =
    "m20260704_000001_rename_managed_ingress_profiles_to_remote_storage_targets";
const ADD_REMOTE_STORAGE_TARGET_KEY_TO_STORAGE_POLICIES_MIGRATION: &str =
    "m20260704_000002_add_remote_storage_target_key_to_storage_policies";
const DROP_REMOTE_STORAGE_TARGET_MAX_FILE_SIZE_MIGRATION: &str =
    "m20260705_000001_drop_remote_storage_target_max_file_size";
const ALIGN_FORGE_AUDIT_CONTRACT_MIGRATION: &str = "m20260712_000001_align_forge_audit_contract";
const ADD_FORGE_AUDIT_QUERY_INDEXES_MIGRATION: &str =
    "m20260712_000002_add_forge_audit_query_indexes";
const ALIGN_FORGE_SYSTEM_CONFIG_CONTRACT_MIGRATION: &str =
    "m20260712_000003_align_forge_system_config_contract";
const ALIGN_FORGE_MAIL_OUTBOX_CONTRACT_MIGRATION: &str =
    "m20260712_000004_align_forge_mail_outbox_contract";
const RUNTIME_LEASES_MIGRATION: &str = "m20260713_000001_runtime_leases";
const BIND_EXTERNAL_AUTH_LOGIN_FLOWS_MIGRATION: &str =
    "m20260716_000001_bind_external_auth_login_flows";
const ADD_UPLOAD_SESSION_KIND_MIGRATION: &str = "m20260717_000001_add_upload_session_kind";
const ADD_UPLOAD_PROVIDER_SESSION_MIGRATION: &str = "m20260719_000001_add_upload_provider_session";
const REQUIRE_UPLOAD_SESSION_KIND_MIGRATION: &str = "m20260723_000001_require_upload_session_kind";
const REMOTE_TUNNEL_OWNERS_MIGRATION: &str = "m20260725_000001_remote_tunnel_owners";
const PROVIDER_RELAY_RESUMABLE_UPLOAD_MIGRATION: &str =
    "m20260728_000001_provider_relay_resumable_upload";
const REFACTOR_RESOURCE_LOCKS_MIGRATION: &str = "m20260803_000001_refactor_resource_locks";

#[tokio::test]
async fn resource_lock_migration_backfills_workspace_and_typed_root() {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("sqlite memory database should connect");
    CurrentMigrator::up(
        &db,
        Some(steps_before_migration(REFACTOR_RESOURCE_LOCKS_MIGRATION)),
    )
    .await
    .expect("legacy resource-lock schema should apply");

    let now = "2026-08-03T00:00:00Z";
    db.execute_unprepared(&format!(
        "INSERT INTO users (id, username, email, password_hash, role, status, storage_quota, storage_used, created_at, updated_at) \
         VALUES (101, 'lock-owner', 'lock-owner@example.test', 'hash', 'user', 'active', 0, 0, '{now}', '{now}'); \
         INSERT INTO storage_policies (id, name, driver_type, created_at, updated_at) \
         VALUES (1, 'lock-test', 'local', '{now}', '{now}'); \
         INSERT INTO file_blobs (id, hash, storage_path, size, ref_count, policy_id, created_at, updated_at) \
         VALUES (201, 'lock-blob', 'lock-blob', 0, 1, 1, '{now}', '{now}'); \
         INSERT INTO files (id, name, folder_id, team_id, blob_id, size, owner_user_id, created_by_user_id, created_by_username, mime_type, extension, compound_extension, file_category, created_at, updated_at, deleted_at, is_locked) \
         VALUES (301, 'locked.txt', NULL, NULL, 201, 0, 101, 101, 'lock-owner', 'text/plain', 'txt', NULL, 'document', '{now}', '{now}', NULL, 1); \
         INSERT INTO resource_locks (token, entity_type, entity_id, path, owner_id, owner_info, timeout_at, shared, deep, created_at) \
         VALUES ('legacy-token', 'file', 301, '/locked.txt', 101, '{{\"kind\":\"webdav\",\"xml\":\"<owner/>\"}}', NULL, 0, 0, '{now}'), \
                ('legacy-ownerless-token', 'file', 301, '/locked.txt', 101, NULL, NULL, 0, 0, '{now}')"
    ))
    .await
    .expect("legacy resource-lock fixture should insert");

    CurrentMigrator::up(&db, Some(1))
        .await
        .expect("resource-lock refactor migration should apply");

    let row = db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT ns.workspace_type, ns.workspace_id, rl.root_kind, rl.root_file_id, rl.depth, rl.mode, rl.origin, rl.lockroot_path \
             FROM resource_locks rl JOIN resource_lock_namespaces ns ON ns.id = rl.namespace_id \
             WHERE rl.token = 'legacy-token'",
        ))
        .await
        .expect("migrated lock should query")
        .expect("migrated lock should exist");
    assert_eq!(row.try_get_by_index::<String>(0).unwrap(), "personal");
    assert_eq!(row.try_get_by_index::<i64>(1).unwrap(), 101);
    assert_eq!(row.try_get_by_index::<String>(2).unwrap(), "file");
    assert_eq!(row.try_get_by_index::<i64>(3).unwrap(), 301);
    assert_eq!(row.try_get_by_index::<String>(4).unwrap(), "resource");
    assert_eq!(row.try_get_by_index::<String>(5).unwrap(), "exclusive");
    assert_eq!(row.try_get_by_index::<String>(6).unwrap(), "webdav");
    assert_eq!(row.try_get_by_index::<String>(7).unwrap(), "/locked.txt");
    let ownerless_origin = db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT origin FROM resource_locks WHERE token = 'legacy-ownerless-token'",
        ))
        .await
        .expect("ownerless migrated lock should query")
        .expect("ownerless migrated lock should exist")
        .try_get_by_index::<String>(0)
        .expect("ownerless migrated origin should decode");
    assert_eq!(
        ownerless_origin, "webdav",
        "ambiguous legacy ownerless locks must require token-based authorization"
    );
    assert!(!has_column(
        &sqlite_table_columns(&db, "files").await,
        "is_locked"
    ));
    assert!(!has_column(
        &sqlite_table_columns(&db, "folders").await,
        "is_locked"
    ));

    CurrentMigrator::down(&db, Some(1))
        .await
        .expect("representable typed resource lock should downgrade");
    let legacy_row = db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT entity_type, entity_id, path, owner_id, shared, deep \
             FROM resource_locks WHERE token = 'legacy-token'",
        ))
        .await
        .expect("downgraded lock should query")
        .expect("downgraded lock should exist");
    assert_eq!(legacy_row.try_get_by_index::<String>(0).unwrap(), "file");
    assert_eq!(legacy_row.try_get_by_index::<i64>(1).unwrap(), 301);
    assert_eq!(
        legacy_row.try_get_by_index::<String>(2).unwrap(),
        "/locked.txt"
    );
    assert_eq!(legacy_row.try_get_by_index::<i64>(3).unwrap(), 101);
    assert!(!legacy_row.try_get_by_index::<bool>(4).unwrap());
    assert!(!legacy_row.try_get_by_index::<bool>(5).unwrap());
    let ownerless_legacy = db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT owner_info FROM resource_locks WHERE token = 'legacy-ownerless-token'",
        ))
        .await
        .expect("downgraded ownerless lock should query")
        .expect("downgraded ownerless lock should exist");
    assert!(
        ownerless_legacy
            .try_get_by_index::<Option<String>>(0)
            .expect("downgraded ownerless payload should decode")
            .is_none()
    );
    let file_locked = db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT is_locked FROM files WHERE id = 301",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get_by_index::<bool>(0)
        .unwrap();
    assert!(
        file_locked,
        "downgrade should restore the direct-lock projection"
    );

    CurrentMigrator::up(&db, Some(1))
        .await
        .expect("downgraded resource lock migration should reapply");
    let ownerless_origin = db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT origin FROM resource_locks WHERE token = 'legacy-ownerless-token'",
        ))
        .await
        .expect("reapplied ownerless lock should query")
        .expect("reapplied ownerless lock should exist")
        .try_get_by_index::<String>(0)
        .expect("reapplied ownerless origin should decode");
    assert_eq!(
        ownerless_origin, "webdav",
        "fail-closed ownerless origin must survive migration round-trip"
    );
}

#[tokio::test]
async fn resource_lock_migration_rejects_unresolved_legacy_target() {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("sqlite memory database should connect");
    CurrentMigrator::up(
        &db,
        Some(steps_before_migration(REFACTOR_RESOURCE_LOCKS_MIGRATION)),
    )
    .await
    .expect("legacy resource-lock schema should apply");
    db.execute_unprepared(
        "INSERT INTO resource_locks (token, entity_type, entity_id, path, owner_id, owner_info, timeout_at, shared, deep, created_at) \
         VALUES ('orphan-token', 'file', 999999, '/orphan', NULL, NULL, NULL, 0, 0, datetime('now'))",
    )
    .await
    .expect("orphan legacy lock should insert");

    let error = CurrentMigrator::up(&db, Some(1))
        .await
        .expect_err("unresolved legacy lock must block migration");
    assert!(
        error
            .to_string()
            .contains("unresolved or invalid workspace/root identity")
    );
    let count = db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COUNT(*) FROM resource_locks WHERE token = 'orphan-token'",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get_by_index::<i64>(0)
        .unwrap();
    assert_eq!(count, 1, "failed migration must retain the legacy lock row");
}

#[tokio::test]
async fn resource_lock_migration_rejects_unrepresentable_downgrade() {
    let db = setup_current_schema().await;
    db.execute_unprepared(
        "INSERT INTO users (id, username, email, password_hash, role, status, storage_quota, storage_used, created_at, updated_at) \
         VALUES (102, 'root-lock-owner', 'root-lock-owner@example.test', 'hash', 'user', 'active', 0, 0, datetime('now'), datetime('now')); \
         INSERT INTO resource_lock_namespaces (id, workspace_type, workspace_id, generation, created_at, updated_at) \
         VALUES (202, 'personal', 102, 1, datetime('now'), datetime('now')); \
         INSERT INTO resource_locks (token, namespace_id, root_kind, root_folder_id, root_file_id, depth, mode, origin, holder_user_id, owner_info, timeout_at, lockroot_path, created_at) \
         VALUES ('pathless-token', 202, 'workspace_root', NULL, NULL, 'infinity', 'exclusive', 'product', 102, NULL, NULL, NULL, datetime('now'))",
    )
    .await
    .expect("pathless typed lock fixture should insert");

    let error = CurrentMigrator::down(&db, Some(1))
        .await
        .expect_err("pathless typed lock must block downgrade");
    assert!(
        error
            .to_string()
            .contains("cannot be represented by the legacy lock schema")
    );
    assert!(has_column(
        &sqlite_table_columns(&db, "resource_locks").await,
        "namespace_id"
    ));
    let count = db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COUNT(*) FROM resource_locks WHERE token = 'pathless-token'",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get_by_index::<i64>(0)
        .unwrap();
    assert_eq!(count, 1, "failed downgrade must retain the typed lock row");
}

async fn setup_current_schema() -> sea_orm::DatabaseConnection {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("sqlite memory database should connect");
    CurrentMigrator::up(&db, None)
        .await
        .expect("current migrations should apply");
    db
}

#[tokio::test]
async fn external_auth_login_flow_browser_binding_migration_is_registered_and_reversible() {
    assert!(
        CurrentMigrator::migrations()
            .iter()
            .any(|migration| migration.name() == BIND_EXTERNAL_AUTH_LOGIN_FLOWS_MIGRATION),
        "external auth browser binding migration should be registered"
    );

    let db = setup_current_schema().await;
    let current_columns = sqlite_table_columns(&db, "external_auth_login_flows").await;
    assert!(has_column(&current_columns, "browser_binding_hash"));

    let rollback_steps = steps_to_roll_back_migration(BIND_EXTERNAL_AUTH_LOGIN_FLOWS_MIGRATION);
    CurrentMigrator::down(&db, Some(rollback_steps))
        .await
        .expect("external auth browser binding migration should roll back");
    let rolled_back_columns = sqlite_table_columns(&db, "external_auth_login_flows").await;
    assert!(!has_column(&rolled_back_columns, "browser_binding_hash"));

    CurrentMigrator::up(&db, Some(rollback_steps))
        .await
        .expect("external auth browser binding migration should reapply");
    let reapplied_columns = sqlite_table_columns(&db, "external_auth_login_flows").await;
    assert!(has_column(&reapplied_columns, "browser_binding_hash"));
}

#[tokio::test]
async fn upload_session_kind_column_migrations_are_reversible() {
    assert!(
        CurrentMigrator::migrations()
            .iter()
            .any(|migration| migration.name() == ADD_UPLOAD_SESSION_KIND_MIGRATION),
        "upload session kind migration should be registered"
    );

    let db = setup_current_schema().await;
    let current_columns = sqlite_table_columns(&db, "upload_sessions").await;
    assert!(has_column(&current_columns, "session_kind"));
    assert!(sqlite_column_is_not_null(&db, "upload_sessions", "session_kind").await);

    let rollback_steps = steps_to_roll_back_migration(ADD_UPLOAD_SESSION_KIND_MIGRATION);
    CurrentMigrator::down(&db, Some(rollback_steps))
        .await
        .expect("upload session kind migration should roll back");
    let rolled_back_columns = sqlite_table_columns(&db, "upload_sessions").await;
    assert!(!has_column(&rolled_back_columns, "session_kind"));

    CurrentMigrator::up(&db, Some(rollback_steps))
        .await
        .expect("upload session kind migration should reapply");
    let reapplied_columns = sqlite_table_columns(&db, "upload_sessions").await;
    assert!(has_column(&reapplied_columns, "session_kind"));
    assert!(sqlite_column_is_not_null(&db, "upload_sessions", "session_kind").await);
}

#[tokio::test]
async fn required_upload_session_kind_migration_rolls_back_and_reapplies() {
    assert!(
        CurrentMigrator::migrations()
            .iter()
            .any(|migration| migration.name() == REQUIRE_UPLOAD_SESSION_KIND_MIGRATION),
        "required upload session kind migration should be registered"
    );

    let db = setup_current_schema().await;
    assert!(sqlite_column_is_not_null(&db, "upload_sessions", "session_kind").await);

    let rollback_steps = steps_to_roll_back_migration(REQUIRE_UPLOAD_SESSION_KIND_MIGRATION);
    CurrentMigrator::down(&db, Some(rollback_steps))
        .await
        .expect("required upload session kind migration should roll back");
    assert!(
        !sqlite_column_is_not_null(&db, "upload_sessions", "session_kind").await,
        "0.4.x rollback should restore nullable session kind"
    );

    CurrentMigrator::up(&db, Some(rollback_steps))
        .await
        .expect("required upload session kind migration should reapply");
    assert!(sqlite_column_is_not_null(&db, "upload_sessions", "session_kind").await);
}

#[tokio::test]
async fn required_upload_session_kind_migration_rejects_null_rows_without_deleting_them() {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("sqlite memory database should connect");
    let pre_boundary_steps = steps_before_migration(REQUIRE_UPLOAD_SESSION_KIND_MIGRATION);
    CurrentMigrator::up(&db, Some(pre_boundary_steps))
        .await
        .expect("0.4.x-compatible migrations should apply");

    db.execute_unprepared("PRAGMA foreign_keys = OFF")
        .await
        .expect("foreign keys should disable for isolated legacy fixture");
    db.execute_unprepared(
        "INSERT INTO upload_sessions (\
            id, user_id, team_id, frontend_client_id, filename, total_size, chunk_size, \
            total_chunks, received_count, folder_id, policy_id, status, session_kind, \
            object_temp_key, object_multipart_id, provider_session_ciphertext, file_id, \
            created_at, expires_at, updated_at\
         ) VALUES (\
            'legacy-null-kind', 1, NULL, NULL, 'legacy.bin', 1, 1, 1, 0, NULL, 1, \
            'uploading', NULL, NULL, NULL, NULL, NULL, \
            '2026-07-23T00:00:00Z', '2026-07-24T00:00:00Z', '2026-07-23T00:00:00Z'\
         )",
    )
    .await
    .expect("legacy null-kind upload session should insert");
    db.execute_unprepared("PRAGMA foreign_keys = ON")
        .await
        .expect("foreign keys should re-enable");

    let error = CurrentMigrator::up(&db, Some(1))
        .await
        .expect_err("null-kind upload session must block the 0.5.0 migration");
    assert!(
        error
            .to_string()
            .contains("legacy or invalid upload session")
    );

    let row = db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COUNT(*) FROM upload_sessions WHERE id = 'legacy-null-kind'",
        ))
        .await
        .expect("legacy row count should query")
        .expect("legacy row count should return one row");
    assert_eq!(row.try_get_by_index::<i64>(0).unwrap(), 1);
}

#[tokio::test]
async fn required_upload_session_kind_migration_rejects_invalid_values_without_deleting_them() {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("sqlite memory database should connect");
    let pre_boundary_steps = steps_before_migration(REQUIRE_UPLOAD_SESSION_KIND_MIGRATION);
    CurrentMigrator::up(&db, Some(pre_boundary_steps))
        .await
        .expect("0.4.x-compatible migrations should apply");

    db.execute_unprepared("PRAGMA foreign_keys = OFF")
        .await
        .expect("foreign keys should disable for isolated legacy fixture");
    db.execute_unprepared(
        "INSERT INTO upload_sessions (\
            id, user_id, team_id, frontend_client_id, filename, total_size, chunk_size, \
            total_chunks, received_count, folder_id, policy_id, status, session_kind, \
            object_temp_key, object_multipart_id, provider_session_ciphertext, file_id, \
            created_at, expires_at, updated_at\
         ) VALUES (\
            'invalid-kind', 1, NULL, NULL, 'invalid.bin', 1, 1, 1, 0, NULL, 1, \
            'uploading', 'legacy_chunk_files', NULL, NULL, NULL, NULL, \
            '2026-07-23T00:00:00Z', '2026-07-24T00:00:00Z', '2026-07-23T00:00:00Z'\
         )",
    )
    .await
    .expect("legacy invalid-kind upload session should insert");
    db.execute_unprepared("PRAGMA foreign_keys = ON")
        .await
        .expect("foreign keys should re-enable");

    let error = CurrentMigrator::up(&db, Some(1))
        .await
        .expect_err("invalid session kind must block the 0.5.0 migration");
    assert!(
        error
            .to_string()
            .contains("legacy or invalid upload session")
    );

    let row = db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT session_kind FROM upload_sessions WHERE id = 'invalid-kind'",
        ))
        .await
        .expect("invalid row query should succeed")
        .expect("invalid row should remain present");
    assert_eq!(
        row.try_get_by_index::<String>(0).unwrap(),
        "legacy_chunk_files"
    );
}

#[tokio::test]
async fn upload_provider_session_migration_is_nullable_and_reversible() {
    assert!(
        CurrentMigrator::migrations()
            .iter()
            .any(|migration| migration.name() == ADD_UPLOAD_PROVIDER_SESSION_MIGRATION),
        "upload provider session migration should be registered"
    );

    let db = setup_current_schema().await;
    let current_columns = sqlite_table_columns(&db, "upload_sessions").await;
    assert!(has_column(&current_columns, "provider_session_ciphertext"));

    let rollback_steps = steps_to_roll_back_migration(ADD_UPLOAD_PROVIDER_SESSION_MIGRATION);
    CurrentMigrator::down(&db, Some(rollback_steps))
        .await
        .expect("upload provider session migration should roll back");
    let rolled_back_columns = sqlite_table_columns(&db, "upload_sessions").await;
    assert!(!has_column(
        &rolled_back_columns,
        "provider_session_ciphertext"
    ));

    CurrentMigrator::up(&db, Some(rollback_steps))
        .await
        .expect("upload provider session migration should reapply");
    let reapplied_columns = sqlite_table_columns(&db, "upload_sessions").await;
    assert!(has_column(
        &reapplied_columns,
        "provider_session_ciphertext"
    ));
}

#[tokio::test]
async fn remote_tunnel_owner_directory_migration_is_registered_and_reversible() {
    assert!(
        CurrentMigrator::migrations()
            .iter()
            .any(|migration| migration.name() == REMOTE_TUNNEL_OWNERS_MIGRATION),
        "remote tunnel owner directory migration should be registered"
    );

    let db = setup_current_schema().await;
    assert!(sqlite_table_exists(&db, "remote_tunnel_owners").await);
    let columns = sqlite_table_columns(&db, "remote_tunnel_owners").await;
    for expected in [
        "remote_node_id",
        "runtime_id",
        "internal_endpoint",
        "fencing_token",
        "lease_expires_at",
        "updated_at",
    ] {
        assert!(
            has_column(&columns, expected),
            "remote_tunnel_owners should include {expected}"
        );
    }

    let rollback_steps = steps_to_roll_back_migration(REMOTE_TUNNEL_OWNERS_MIGRATION);
    CurrentMigrator::down(&db, Some(rollback_steps))
        .await
        .expect("remote tunnel owner directory migration should roll back");
    assert!(!sqlite_table_exists(&db, "remote_tunnel_owners").await);

    CurrentMigrator::up(&db, Some(rollback_steps))
        .await
        .expect("remote tunnel owner directory migration should reapply");
    assert!(sqlite_table_exists(&db, "remote_tunnel_owners").await);
}

#[tokio::test]
async fn provider_relay_resumable_ordering_index_is_registered_and_reversible() {
    assert!(
        CurrentMigrator::migrations()
            .iter()
            .any(|migration| { migration.name() == PROVIDER_RELAY_RESUMABLE_UPLOAD_MIGRATION })
    );

    let db = setup_current_schema().await;
    let index = "idx_upload_sessions_provider_relay_ordering";
    assert!(sqlite_table_index_exists(&db, "upload_sessions", index).await);
    assert_eq!(
        sqlite_index_columns(&db, index).await,
        ["session_kind", "status", "received_count", "id"]
    );

    let rollback_steps = steps_to_roll_back_migration(PROVIDER_RELAY_RESUMABLE_UPLOAD_MIGRATION);
    CurrentMigrator::down(&db, Some(rollback_steps))
        .await
        .expect("provider relay resumable migration should roll back");
    assert!(!sqlite_table_index_exists(&db, "upload_sessions", index).await);

    CurrentMigrator::up(&db, Some(rollback_steps))
        .await
        .expect("provider relay resumable migration should reapply");
    assert!(sqlite_table_index_exists(&db, "upload_sessions", index).await);
    assert_eq!(
        sqlite_index_columns(&db, index).await,
        ["session_kind", "status", "received_count", "id"]
    );
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

fn steps_before_migration(migration_name: &str) -> u32 {
    let migrations = CurrentMigrator::migrations();
    let position = migrations
        .iter()
        .position(|migration| migration.name() == migration_name)
        .unwrap_or_else(|| panic!("{migration_name} migration should be registered"));
    u32::try_from(position).expect("migration step count should fit u32")
}

fn steps_to_roll_back_upload_session_object_fields() -> u32 {
    steps_to_roll_back_migration(RENAME_UPLOAD_SESSION_OBJECT_FIELDS_MIGRATION)
}

fn steps_to_roll_back_storage_connector_application_configs() -> u32 {
    steps_to_roll_back_migration(ADD_STORAGE_CONNECTOR_APPLICATION_CONFIGS_MIGRATION)
}

fn steps_to_roll_back_rename_managed_ingress_profiles() -> u32 {
    steps_to_roll_back_migration(RENAME_MANAGED_INGRESS_PROFILES_MIGRATION)
}

fn steps_to_roll_back_remote_storage_target_max_file_size() -> u32 {
    steps_to_roll_back_migration(DROP_REMOTE_STORAGE_TARGET_MAX_FILE_SIZE_MIGRATION)
}

fn steps_to_roll_back_storage_policy_remote_storage_target_key() -> u32 {
    steps_to_roll_back_migration(ADD_REMOTE_STORAGE_TARGET_KEY_TO_STORAGE_POLICIES_MIGRATION)
}

async fn roll_back_allow_shared_webdav_locks(
    db: &sea_orm::DatabaseConnection,
) -> Result<(), DbErr> {
    CurrentMigrator::down(db, Some(1)).await
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
    sqlite_table_index_exists(db, "resource_locks", index_name).await
}

async fn sqlite_table_index_exists(
    db: &DatabaseConnection,
    table_name: &str,
    index_name: &str,
) -> bool {
    db.query_all_raw(Statement::from_string(
        DbBackend::Sqlite,
        format!("PRAGMA index_list('{table_name}')"),
    ))
    .await
    .expect("sqlite index list should load")
    .into_iter()
    .any(|row| row.try_get_by_index::<String>(1).as_deref() == Ok(index_name))
}

async fn sqlite_index_columns(db: &DatabaseConnection, index_name: &str) -> Vec<String> {
    let mut columns = db
        .query_all_raw(Statement::from_string(
            DbBackend::Sqlite,
            format!("PRAGMA index_info('{index_name}')"),
        ))
        .await
        .expect("sqlite index column metadata should load")
        .into_iter()
        .map(|row| {
            let sequence = row
                .try_get_by_index::<i32>(0)
                .expect("sqlite PRAGMA index_info row should include sequence number");
            let name = row
                .try_get_by_index::<String>(2)
                .expect("sqlite PRAGMA index_info row should include column name");
            (sequence, name)
        })
        .collect::<Vec<_>>();
    columns.sort_by_key(|(sequence, _)| *sequence);
    columns.into_iter().map(|(_, name)| name).collect()
}

async fn mysql_table_index_exists(
    db: &DatabaseConnection,
    table_name: &str,
    index_name: &str,
) -> bool {
    db.query_one_raw(Statement::from_sql_and_values(
        DbBackend::MySql,
        "SELECT 1 FROM information_schema.statistics \
         WHERE table_schema = DATABASE() AND table_name = ? AND index_name = ? LIMIT 1",
        [table_name.into(), index_name.into()],
    ))
    .await
    .expect("mysql index lookup should load")
    .is_some()
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

#[tokio::test]
async fn forge_task_runtime_schema_uses_shared_tables_indexes_and_dedupe_column() {
    let db = setup_current_schema().await;

    assert!(sqlite_table_exists(&db, aster_forge_db::RUNTIME_LEASES_TABLE).await);
    assert!(sqlite_table_exists(&db, aster_forge_db::SCHEDULED_TASKS_TABLE).await);
    assert!(
        sqlite_table_columns(&db, "background_tasks")
            .await
            .iter()
            .any(|column| column == "dedupe_key")
    );
    assert!(
        sqlite_table_index_exists(
            &db,
            "background_tasks",
            "idx_background_tasks_dedupe_key_unique",
        )
        .await
    );
    for index in [
        "idx_scheduled_tasks_namespace_name_unique",
        "idx_scheduled_tasks_next_run",
    ] {
        assert!(
            sqlite_table_index_exists(&db, aster_forge_db::SCHEDULED_TASKS_TABLE, index).await,
            "scheduled task index {index} should exist"
        );
    }
}

#[tokio::test]
async fn forge_task_runtime_migrations_roll_back_and_reapply_as_one_contract() {
    let db = setup_current_schema().await;
    let steps = steps_to_roll_back_migration(RUNTIME_LEASES_MIGRATION);

    CurrentMigrator::down(&db, Some(steps))
        .await
        .expect("Forge task runtime migrations should roll back");
    assert!(!sqlite_table_exists(&db, aster_forge_db::RUNTIME_LEASES_TABLE).await);
    assert!(!sqlite_table_exists(&db, aster_forge_db::SCHEDULED_TASKS_TABLE).await);
    assert!(
        !sqlite_table_columns(&db, "background_tasks")
            .await
            .iter()
            .any(|column| column == "dedupe_key")
    );

    CurrentMigrator::up(&db, None)
        .await
        .expect("Forge task runtime migrations should reapply");
    assert!(sqlite_table_exists(&db, aster_forge_db::RUNTIME_LEASES_TABLE).await);
    assert!(sqlite_table_exists(&db, aster_forge_db::SCHEDULED_TASKS_TABLE).await);
    assert!(
        sqlite_table_columns(&db, "background_tasks")
            .await
            .iter()
            .any(|column| column == "dedupe_key")
    );
}

async fn sqlite_column_type_and_default(
    db: &DatabaseConnection,
    table_name: &str,
    column_name: &str,
) -> (String, Option<String>) {
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
            (
                row.try_get_by_index::<String>(2)
                    .expect("sqlite PRAGMA table_info row should include column type"),
                row.try_get_by_index::<Option<String>>(4)
                    .expect("sqlite PRAGMA table_info row should include default value"),
            )
        })
    })
    .unwrap_or_else(|| panic!("{table_name}.{column_name} should exist"))
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
async fn forge_audit_query_indexes_are_present_in_current_schema() {
    assert!(
        CurrentMigrator::migrations()
            .iter()
            .any(|migration| migration.name() == ADD_FORGE_AUDIT_QUERY_INDEXES_MIGRATION),
        "Forge audit query index migration should be registered"
    );

    let db = setup_current_schema().await;
    for index in [
        aster_forge_db::AUDIT_LOG_ACTION_CREATED_USER_INDEX,
        aster_forge_db::AUDIT_LOG_CREATED_ID_INDEX,
        aster_forge_db::AUDIT_LOG_USER_CREATED_ID_INDEX,
        aster_forge_db::AUDIT_LOG_ACTION_CREATED_ID_INDEX,
        aster_forge_db::AUDIT_LOG_ENTITY_TYPE_CREATED_ID_INDEX,
    ] {
        assert!(
            sqlite_table_index_exists(&db, aster_forge_db::AUDIT_LOGS_TABLE, index).await,
            "current audit schema should include {index}"
        );
    }
}

#[tokio::test]
async fn forge_audit_columns_match_shared_contract_and_preserve_rows() {
    assert!(
        CurrentMigrator::migrations()
            .iter()
            .any(|migration| migration.name() == ALIGN_FORGE_AUDIT_CONTRACT_MIGRATION),
        "Forge audit contract migration should be registered"
    );

    let db = Database::connect("sqlite::memory:")
        .await
        .expect("sqlite memory database should connect");
    let migration_position = CurrentMigrator::migrations()
        .iter()
        .position(|migration| migration.name() == ALIGN_FORGE_AUDIT_CONTRACT_MIGRATION)
        .expect("Forge audit contract migration should exist");
    CurrentMigrator::up(
        &db,
        Some(u32::try_from(migration_position).expect("migration count should fit u32")),
    )
    .await
    .expect("legacy Drive schema should apply");
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO audit_logs (user_id, action, entity_type, ip_address, created_at) VALUES (?, ?, ?, ?, ?)",
        [
            7_i64.into(),
            "file_upload".into(),
            "file".into(),
            "2001:db8::7".into(),
            chrono::Utc::now().into(),
        ],
    ))
    .await
    .expect("legacy audit row should insert");

    CurrentMigrator::up(&db, None)
        .await
        .expect("Forge audit contract migrations should apply");
    let (ip_type, _) = sqlite_column_type_and_default(&db, "audit_logs", "ip_address").await;
    assert_eq!(ip_type.to_ascii_lowercase(), "varchar(128)");
    let (_, user_id_default) = sqlite_column_type_and_default(&db, "audit_logs", "user_id").await;
    assert_eq!(user_id_default.as_deref(), Some("0"));
    let preserved = db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT user_id, ip_address FROM audit_logs WHERE action = 'file_upload'",
        ))
        .await
        .expect("migrated audit row should load")
        .expect("migrated audit row should remain");
    assert_eq!(preserved.try_get_by_index::<i64>(0).unwrap(), 7);
    assert_eq!(
        preserved.try_get_by_index::<String>(1).unwrap(),
        "2001:db8::7"
    );
}

#[tokio::test]
async fn forge_system_config_contract_preserves_rows_and_named_index() {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("sqlite memory database should connect");
    let migration_position = CurrentMigrator::migrations()
        .iter()
        .position(|migration| migration.name() == ALIGN_FORGE_SYSTEM_CONFIG_CONTRACT_MIGRATION)
        .expect("Forge system-config contract migration should exist");
    CurrentMigrator::up(
        &db,
        Some(u32::try_from(migration_position).expect("migration count should fit u32")),
    )
    .await
    .expect("legacy Drive schema should apply");
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO system_config (key, value, source, visibility, namespace, category, description, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        [
            "custom.viewer".into(),
            "enabled".into(),
            "custom".into(),
            "public".into(),
            "drive".into(),
            "custom".into(),
            "custom viewer flag".into(),
            chrono::Utc::now().into(),
        ],
    ))
    .await
    .expect("legacy system-config row should insert");

    CurrentMigrator::up(&db, None)
        .await
        .expect("Forge system-config contract migration should apply");
    let (namespace_type, _) =
        sqlite_column_type_and_default(&db, "system_config", "namespace").await;
    assert_eq!(namespace_type.to_ascii_lowercase(), "varchar(64)");
    let (description_type, _) =
        sqlite_column_type_and_default(&db, "system_config", "description").await;
    assert_eq!(description_type.to_ascii_lowercase(), "varchar(512)");
    assert!(
        sqlite_table_index_exists(
            &db,
            aster_forge_db::SYSTEM_CONFIG_TABLE,
            aster_forge_db::SYSTEM_CONFIG_KEY_UNIQUE_INDEX,
        )
        .await
    );
    assert!(sqlite_table_index_exists(&db, "system_config", "idx_system_config_visibility").await);
    let preserved = db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT value, visibility, namespace FROM system_config WHERE key = 'custom.viewer'",
        ))
        .await
        .expect("migrated system-config row should load")
        .expect("migrated system-config row should remain");
    assert_eq!(preserved.try_get_by_index::<String>(0).unwrap(), "enabled");
    assert_eq!(preserved.try_get_by_index::<String>(1).unwrap(), "public");
    assert_eq!(preserved.try_get_by_index::<String>(2).unwrap(), "drive");
}

#[tokio::test]
async fn forge_mail_outbox_contract_preserves_rows_and_named_indexes() {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("sqlite memory database should connect");
    let migration_position = CurrentMigrator::migrations()
        .iter()
        .position(|migration| migration.name() == ALIGN_FORGE_MAIL_OUTBOX_CONTRACT_MIGRATION)
        .expect("Forge mail-outbox contract migration should exist");
    CurrentMigrator::up(
        &db,
        Some(u32::try_from(migration_position).expect("migration count should fit u32")),
    )
    .await
    .expect("legacy Drive schema should apply");
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO mail_outbox (template_code, to_address, to_name, payload_json, status, attempt_count, next_attempt_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        [
            "user_invitation".into(),
            "invitee@example.com".into(),
            "Invitee".into(),
            r#"{"token":"preserved"}"#.into(),
            "retry".into(),
            2_i32.into(),
            chrono::Utc::now().into(),
            chrono::Utc::now().into(),
            chrono::Utc::now().into(),
        ],
    ))
    .await
    .expect("legacy mail-outbox row should insert");

    CurrentMigrator::up(&db, None)
        .await
        .expect("Forge mail-outbox contract migration should apply");
    let (template_code_type, _) =
        sqlite_column_type_and_default(&db, "mail_outbox", "template_code").await;
    assert_eq!(template_code_type.to_ascii_lowercase(), "varchar(64)");
    for index in [
        "idx_mail_outbox_due",
        "idx_mail_outbox_processing",
        "idx_mail_outbox_sent_at",
    ] {
        assert!(sqlite_table_index_exists(&db, "mail_outbox", index).await);
    }
    let preserved = db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT template_code, to_address, payload_json, status, attempt_count FROM mail_outbox WHERE to_address = 'invitee@example.com'",
        ))
        .await
        .expect("migrated mail-outbox row should load")
        .expect("migrated mail-outbox row should remain");
    assert_eq!(
        preserved.try_get_by_index::<String>(0).unwrap(),
        "user_invitation"
    );
    assert_eq!(
        preserved.try_get_by_index::<String>(1).unwrap(),
        "invitee@example.com"
    );
    assert_eq!(
        preserved.try_get_by_index::<String>(2).unwrap(),
        r#"{"token":"preserved"}"#
    );
    assert_eq!(preserved.try_get_by_index::<String>(3).unwrap(), "retry");
    assert_eq!(preserved.try_get_by_index::<i32>(4).unwrap(), 2);
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
async fn storage_policy_remote_storage_target_key_migration_round_trips_column() {
    assert!(
        CurrentMigrator::migrations().iter().any(|migration| {
            migration.name() == ADD_REMOTE_STORAGE_TARGET_KEY_TO_STORAGE_POLICIES_MIGRATION
        }),
        "storage policy remote target key migration should be registered"
    );

    let db = setup_current_schema().await;
    let current_columns = sqlite_table_columns(&db, "storage_policies").await;
    assert!(
        has_column(&current_columns, "remote_storage_target_key"),
        "current schema should include storage_policies.remote_storage_target_key"
    );
    assert!(
        sqlite_table_index_exists(
            &db,
            "storage_policies",
            "idx_storage_policies_remote_target"
        )
        .await,
        "current schema should include idx_storage_policies_remote_target"
    );

    CurrentMigrator::down(
        &db,
        Some(steps_to_roll_back_storage_policy_remote_storage_target_key()),
    )
    .await
    .expect("remote target key migration should roll back");
    let rolled_back_columns = sqlite_table_columns(&db, "storage_policies").await;
    assert!(
        !has_column(&rolled_back_columns, "remote_storage_target_key"),
        "rollback should remove storage_policies.remote_storage_target_key"
    );
    assert!(
        !sqlite_table_index_exists(
            &db,
            "storage_policies",
            "idx_storage_policies_remote_target"
        )
        .await,
        "rollback should remove idx_storage_policies_remote_target"
    );

    CurrentMigrator::up(
        &db,
        Some(steps_to_roll_back_storage_policy_remote_storage_target_key()),
    )
    .await
    .expect("remote target key migration should reapply");
    let reapplied_columns = sqlite_table_columns(&db, "storage_policies").await;
    assert!(
        has_column(&reapplied_columns, "remote_storage_target_key"),
        "reapply should restore storage_policies.remote_storage_target_key"
    );
    assert!(
        sqlite_table_index_exists(
            &db,
            "storage_policies",
            "idx_storage_policies_remote_target"
        )
        .await,
        "reapply should restore idx_storage_policies_remote_target"
    );
}

#[tokio::test]
async fn mysql_remote_storage_target_rename_migration_round_trips_indexes() {
    let should_run_mysql = std::env::var("ASTER_TEST_DATABASE_BACKEND")
        .ok()
        .map(|value| value.trim().eq_ignore_ascii_case("mysql"))
        .unwrap_or(false);
    if !should_run_mysql {
        eprintln!(
            "skipping MySQL migration index rename coverage; set ASTER_TEST_DATABASE_BACKEND=mysql"
        );
        return;
    }

    assert!(
        CurrentMigrator::migrations()
            .iter()
            .any(|migration| migration.name() == RENAME_MANAGED_INGRESS_PROFILES_MIGRATION),
        "remote storage target rename migration should be registered"
    );

    let database_url = common::mysql_test_database_url().await;
    let db = Database::connect(&database_url)
        .await
        .expect("mysql migration test database should connect");

    CurrentMigrator::up(&db, None)
        .await
        .expect("current migrations should apply on MySQL");
    assert!(
        mysql_table_index_exists(
            &db,
            "remote_storage_targets",
            "idx_remote_storage_targets_binding_target_key"
        )
        .await,
        "MySQL up should rename the target key index"
    );
    assert!(
        mysql_table_index_exists(
            &db,
            "remote_storage_targets",
            "idx_remote_storage_targets_binding_default"
        )
        .await,
        "MySQL up should rename the default index"
    );
    assert!(
        !mysql_table_index_exists(
            &db,
            "remote_storage_targets",
            "idx_managed_ingress_profiles_binding_profile_key"
        )
        .await,
        "MySQL up should remove the old profile key index name"
    );

    CurrentMigrator::down(
        &db,
        Some(steps_to_roll_back_rename_managed_ingress_profiles()),
    )
    .await
    .expect("remote storage target rename migration should roll back on MySQL");
    assert!(
        mysql_table_index_exists(
            &db,
            "managed_ingress_profiles",
            "idx_managed_ingress_profiles_binding_profile_key"
        )
        .await,
        "MySQL down should restore the legacy profile key index"
    );
    assert!(
        mysql_table_index_exists(
            &db,
            "managed_ingress_profiles",
            "idx_managed_ingress_profiles_binding_default"
        )
        .await,
        "MySQL down should restore the legacy default index"
    );
    assert!(
        !mysql_table_index_exists(
            &db,
            "managed_ingress_profiles",
            "idx_remote_storage_targets_binding_target_key"
        )
        .await,
        "MySQL down should remove the remote storage target key index name"
    );

    CurrentMigrator::up(
        &db,
        Some(steps_to_roll_back_rename_managed_ingress_profiles()),
    )
    .await
    .expect("remote storage target rename migration should reapply on MySQL");
    assert!(
        mysql_table_index_exists(
            &db,
            "remote_storage_targets",
            "idx_remote_storage_targets_binding_target_key"
        )
        .await,
        "MySQL reapply should restore the target key index name"
    );
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
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("sqlite memory database should connect");
    CurrentMigrator::up(
        &db,
        Some(steps_before_migration(ALLOW_SHARED_WEBDAV_LOCKS_MIGRATION) + 1),
    )
    .await
    .expect("shared WebDAV lock schema should apply");
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
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("sqlite memory database should connect");
    CurrentMigrator::up(
        &db,
        Some(steps_before_migration(ALLOW_SHARED_WEBDAV_LOCKS_MIGRATION) + 1),
    )
    .await
    .expect("shared WebDAV lock schema should apply");
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
