//! Exercise observation SQL and the cleanup interval on the selected real backend.
use crate::common;
use actix_web::test;
use aster_drive::db::repository::{file_repo, observability_repo, policy_repo};
use aster_drive_migration::{Migrator, MigratorTrait};
use aster_drive_model::{entities::file_blob, types::file_blob::FileBlobBacking};
use chrono::Utc;
use sea_orm::{ConnectionTrait, Set, Statement};

#[actix_web::test]
async fn backlog_aggregates_cleanup_interval_and_observation_migration_round_trip() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/files/upload/init")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({"filename":"observed.bin", "total_size":42}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 201);
    let db = state.writer_db();
    let uploads = observability_repo::upload_backlog(db, Utc::now())
        .await
        .unwrap();
    assert_eq!(uploads.iter().map(|row| row.count).sum::<i64>(), 1);
    assert_eq!(
        uploads
            .iter()
            .map(|row| row.reserved.unwrap_or(0))
            .sum::<i64>(),
        42
    );
    assert!(!uploads[0].expired);
    assert!(
        observability_repo::remote_node_counts(db)
            .await
            .unwrap()
            .is_empty()
    );
    let policy = policy_repo::find_default(db).await.unwrap().unwrap();
    let now = Utc::now();
    let blob = file_repo::create_blob(
        db,
        file_blob::ActiveModel {
            hash: Set("a".repeat(64)),
            size: Set(42),
            policy_id: Set(policy.id),
            storage_path: Set(Some("observation/blob.bin".into())),
            backing: Set(FileBlobBacking::Stored),
            ref_count: Set(2),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    file_repo::decrement_blob_ref_count_by(db, blob.id, 1)
        .await
        .unwrap();
    assert!(
        file_repo::find_blob_by_id(db, blob.id)
            .await
            .unwrap()
            .cleanup_pending_since
            .is_none()
    );
    file_repo::decrement_blob_ref_counts_by(db, &[(blob.id, 1)])
        .await
        .unwrap();
    let pending = file_repo::find_blob_by_id(db, blob.id)
        .await
        .unwrap()
        .cleanup_pending_since
        .unwrap();
    assert!(file_repo::claim_blob_cleanup(db, blob.id).await.unwrap());
    assert!(
        file_repo::restore_blob_cleanup_claim(db, blob.id)
            .await
            .unwrap()
    );
    file_repo::reset_blob_ref_count_to_zero(db, blob.id)
        .await
        .unwrap();
    assert_eq!(
        file_repo::find_blob_by_id(db, blob.id)
            .await
            .unwrap()
            .cleanup_pending_since,
        Some(pending)
    );
    let backlog = observability_repo::blob_backlog(db).await.unwrap();
    assert_eq!(backlog.count, 1);
    assert_eq!(backlog.oldest, Some(pending));
    file_repo::increment_blob_ref_counts_by(db, &[(blob.id, 2)])
        .await
        .unwrap();
    assert!(
        file_repo::find_blob_by_id(db, blob.id)
            .await
            .unwrap()
            .cleanup_pending_since
            .is_none()
    );
    file_repo::decrement_blob_ref_count(db, blob.id)
        .await
        .unwrap();
    assert!(
        file_repo::find_blob_by_id(db, blob.id)
            .await
            .unwrap()
            .cleanup_pending_since
            .is_none()
    );
    file_repo::decrement_blob_ref_count(db, blob.id)
        .await
        .unwrap();
    assert!(
        file_repo::find_blob_by_id(db, blob.id)
            .await
            .unwrap()
            .cleanup_pending_since
            .is_some()
    );
    let migration = Migrator::migrations().pop().unwrap();
    let manager = aster_drive_migration::SchemaManager::new(db);
    migration.down(&manager).await.unwrap();
    let count = db
        .query_one_raw(Statement::from_string(
            db.get_database_backend(),
            "SELECT COUNT(*) AS count FROM file_blobs".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "count")
        .unwrap();
    assert_eq!(count, 1);
    migration.up(&manager).await.unwrap();
    let restored = file_repo::find_blob_by_id(db, blob.id).await.unwrap();
    assert_eq!(restored.cleanup_pending_since, Some(restored.updated_at));
    assert_eq!(observability_repo::blob_backlog(db).await.unwrap().count, 1);
}
