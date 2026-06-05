//! 集成测试：`migration`。

use migration::{CurrentMigrator, MigratorTrait};
use sea_orm::{ConnectionTrait, Database, DatabaseConnection, DbBackend, DbErr, Statement};

async fn setup_current_schema() -> sea_orm::DatabaseConnection {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("sqlite memory database should connect");
    CurrentMigrator::up(&db, None)
        .await
        .expect("current migrations should apply");
    db
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

#[tokio::test]
async fn allow_shared_webdav_locks_down_recreates_unique_index_without_duplicates() {
    let db = setup_current_schema().await;
    insert_resource_lock(&db, "urn:uuid:one", "file", 1).await;
    insert_resource_lock(&db, "urn:uuid:two", "file", 2).await;

    CurrentMigrator::down(&db, Some(1))
        .await
        .expect("last migration should roll back when resource locks are unique");

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

    let error = CurrentMigrator::down(&db, Some(1))
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
