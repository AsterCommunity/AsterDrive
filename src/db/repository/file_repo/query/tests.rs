use super::names::{add_normalization_query_variants, normalize_existing_filename};
use crate::db::repository::file_repo::FileScope;
use aster_drive_model::entities::file;
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ConnectOptions, ConnectionTrait, Database, DbBackend, Schema, Set,
};
use std::borrow::Cow;

async fn build_file_query_test_db() -> sea_orm::DatabaseConnection {
    let mut options = ConnectOptions::new("sqlite::memory:");
    options.max_connections(1);
    let db = Database::connect(options)
        .await
        .expect("file query test DB should connect");
    db.execute_unprepared("PRAGMA foreign_keys = OFF")
        .await
        .expect("file query test DB should disable unrelated foreign keys");
    let schema = Schema::new(DbBackend::Sqlite);
    db.execute(&schema.create_table_from_entity(file::Entity))
        .await
        .expect("files test table should be created");
    db
}

async fn insert_test_file(
    db: &sea_orm::DatabaseConnection,
    id: i64,
    folder_id: i64,
    owner_user_id: i64,
    deleted: bool,
) {
    let now = Utc::now();
    file::ActiveModel {
        id: Set(id),
        name: Set(format!("file-{id}.txt")),
        folder_id: Set(Some(folder_id)),
        team_id: Set(None),
        blob_id: Set(1),
        size: Set(id),
        owner_user_id: Set(Some(owner_user_id)),
        created_by_user_id: Set(Some(owner_user_id)),
        created_by_username: Set(format!("user-{owner_user_id}")),
        mime_type: Set("text/plain".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(deleted.then_some(now)),
        is_locked: Set(false),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("test file should insert");
}

async fn seed_keyset_files(db: &sea_orm::DatabaseConnection) {
    for (id, folder_id, owner_user_id, deleted) in [
        (8, 20, 7, false),
        (2, 20, 7, false),
        (5, 10, 7, false),
        (3, 10, 7, false),
        (4, 10, 8, false),
        (6, 10, 7, true),
        (9, 30, 7, false),
    ] {
        insert_test_file(db, id, folder_id, owner_user_id, deleted).await;
    }
}

fn ids(files: &[file::Model]) -> Vec<i64> {
    files.iter().map(|file| file.id).collect()
}

#[tokio::test]
async fn folders_after_id_query_pages_and_merges_in_id_order() {
    let db = build_file_query_test_db().await;
    seed_keyset_files(&db).await;
    let scope = FileScope::Personal { user_id: 7 };

    let first = super::basic::find_by_folders_after_id_in_scope(&db, scope, &[20, 10], None, 2)
        .await
        .unwrap();
    assert_eq!(ids(&first), [2, 3]);

    let second =
        super::basic::find_by_folders_after_id_in_scope(&db, scope, &[10, 20], Some(3), 10)
            .await
            .unwrap();
    assert_eq!(ids(&second), [5, 8]);

    let limited = super::basic::find_by_folders_after_id_in_scope(&db, scope, &[10, 20], None, 3)
        .await
        .unwrap();
    assert_eq!(ids(&limited), [2, 3, 5]);
    assert!(
        super::basic::find_by_folders_after_id_in_scope(&db, scope, &[], None, 3)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn all_folders_after_id_query_includes_deleted_without_crossing_scope() {
    let db = build_file_query_test_db().await;
    seed_keyset_files(&db).await;

    let files = super::basic::find_all_by_folders_after_id_in_scope(
        &db,
        FileScope::Personal { user_id: 7 },
        &[10, 20],
        None,
        10,
    )
    .await
    .unwrap();
    assert_eq!(ids(&files), [2, 3, 5, 6, 8]);
}

#[test]
fn normalization_query_variants_borrow_ascii_candidates() {
    let names = vec!["report.txt".to_string(), "report (1).txt".to_string()];
    let variants = add_normalization_query_variants(&names);

    assert!(matches!(variants, Cow::Borrowed(_)));
    assert_eq!(variants.as_ref(), names.as_slice());
}

#[test]
fn normalization_query_variants_add_unicode_forms_only_when_needed() {
    let names = vec![
        "caf\u{00e9}.txt".to_string(),
        "cafe\u{0301}.txt".to_string(),
    ];
    let variants = add_normalization_query_variants(&names);

    assert!(matches!(variants, Cow::Owned(_)));
    assert_eq!(variants.as_ref().len(), 2);
    assert!(variants.as_ref().contains(&"caf\u{00e9}.txt".to_string()));
    assert!(variants.as_ref().contains(&"cafe\u{0301}.txt".to_string()));
}

#[test]
fn normalize_existing_filename_reuses_ascii_and_nfc_names() {
    assert_eq!(
        normalize_existing_filename("report.txt".to_string()),
        "report.txt"
    );
    assert_eq!(
        normalize_existing_filename("caf\u{00e9}.txt".to_string()),
        "caf\u{00e9}.txt"
    );
    assert_eq!(
        normalize_existing_filename("cafe\u{0301}.txt".to_string()),
        "caf\u{00e9}.txt"
    );
}
