use super::{
    AdminTaskFilters, TerminalTaskCleanupFilters, count_active_processing_by_kinds,
    delete_terminal_by_filters, find_paginated_all_filtered, list_claimable_by_kinds,
    release_processing,
};
use crate::api::pagination::{AdminTaskSortBy, SortOrder};
use crate::config::DatabaseConfig;
use crate::entities::background_task;
use crate::types::{BackgroundTaskKind, BackgroundTaskStatus, StoredTaskPayload, StoredTaskSteps};
use chrono::{Duration, Utc};
use migration::Migrator;
use sea_orm::{ActiveModelTrait, EntityTrait, Set};

async fn build_test_db() -> sea_orm::DatabaseConnection {
    let db = crate::db::connect_with_metrics(
        &DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            pool_size: 1,
            retry_count: 0,
        },
        crate::metrics_core::NoopMetrics::arc(),
    )
    .await
    .expect("background task repo test DB should connect");
    Migrator::up(&db, None)
        .await
        .expect("background task repo test migrations should succeed");
    db
}

async fn insert_task(
    db: &sea_orm::DatabaseConnection,
    kind: BackgroundTaskKind,
    status: BackgroundTaskStatus,
    finished_at: Option<chrono::DateTime<chrono::Utc>>,
    updated_at: chrono::DateTime<chrono::Utc>,
) -> background_task::Model {
    let created_at = updated_at - Duration::hours(1);
    let task_name = match kind {
        BackgroundTaskKind::ArchiveCompress => "archive-compress",
        BackgroundTaskKind::ArchiveExtract => "archive-extract",
        BackgroundTaskKind::ArchivePreviewGenerate => "archive-preview-generate",
        BackgroundTaskKind::ThumbnailGenerate => "thumbnail-generate",
        BackgroundTaskKind::MediaMetadataExtract => "media-metadata-extract",
        BackgroundTaskKind::TrashPurgeAll => "trash-purge-all",
        BackgroundTaskKind::StoragePolicyTempCleanup => "storage-policy-temp-cleanup",
        BackgroundTaskKind::StoragePolicyMigration => "storage-policy-migration",
        BackgroundTaskKind::BlobMaintenance => "blob-maintenance",
        BackgroundTaskKind::OfflineDownload => "offline-download",
        BackgroundTaskKind::SystemRuntime => "task-cleanup",
    };
    let payload_json = match kind {
        BackgroundTaskKind::ArchiveCompress => serde_json::json!({
            "file_ids": [],
            "folder_ids": [],
            "archive_name": "repo-test.zip",
            "target_folder_id": null,
        }),
        BackgroundTaskKind::ArchiveExtract => serde_json::json!({
            "file_id": 1,
            "source_file_name": "repo-test.zip",
            "target_folder_id": null,
            "output_folder_name": "repo-test",
        }),
        BackgroundTaskKind::ArchivePreviewGenerate => serde_json::json!({
            "file_id": 1,
            "source_file_name": "repo-test.zip",
            "source_blob_id": 1,
            "source_hash": "hash",
            "limit_signature": "source=1",
        }),
        BackgroundTaskKind::ThumbnailGenerate => serde_json::json!({
            "blob_id": 1,
            "blob_hash": "hash",
            "source_file_name": "repo-test.png",
            "source_mime_type": "image/png",
            "processor": "image_magick",
        }),
        BackgroundTaskKind::MediaMetadataExtract => serde_json::json!({
            "blob_id": 1,
            "blob_hash": "hash",
            "source_file_name": "repo-test.png",
            "source_mime_type": "image/png",
            "kind": "image",
        }),
        BackgroundTaskKind::TrashPurgeAll => serde_json::json!({}),
        BackgroundTaskKind::StoragePolicyTempCleanup => serde_json::json!({
            "policy": {
                "id": 1,
                "name": "Deleted policy",
                "driver_type": "local",
                "endpoint": "",
                "bucket": "",
                "access_key": "",
                "secret_key": "",
                "base_path": "/tmp/asterdrive-deleted-policy",
                "remote_node_id": null,
                "max_file_size": 0,
                "allowed_types": "[]",
                "options": "{}",
                "is_default": false,
                "chunk_size": 5_242_880,
            },
            "remote_node": null,
            "temp_keys": ["files/temp-object"],
            "multipart_uploads": [],
        }),
        BackgroundTaskKind::StoragePolicyMigration => serde_json::json!({
            "source_policy_id": 1,
            "target_policy_id": 2,
            "delete_source_after_success": false,
            "plan_hash": "hash",
            "source_policy_updated_at": "2026-01-01T00:00:00Z",
            "target_policy_updated_at": "2026-01-01T00:00:00Z",
        }),
        BackgroundTaskKind::BlobMaintenance => serde_json::json!({
            "action": "integrity_check",
            "blob_ids": [1],
        }),
        BackgroundTaskKind::OfflineDownload => serde_json::json!({
            "url": "https://example.com/repo-test.bin",
            "filename": "repo-test.bin",
            "target_folder_id": null,
            "expected_sha256": null,
            "source_display_url": "https://example.com/repo-test.bin",
        }),
        BackgroundTaskKind::SystemRuntime => serde_json::json!({
            "task_name": task_name,
        }),
    };

    background_task::ActiveModel {
        kind: Set(kind),
        status: Set(status),
        creator_user_id: Set(Some(7)),
        team_id: Set(None),
        share_id: Set(None),
        display_name: Set(format!("{kind:?}-{status:?}")),
        payload_json: Set(StoredTaskPayload(payload_json.to_string())),
        result_json: Set(None),
        runtime_json: Set(None),
        steps_json: Set(Some(StoredTaskSteps("[]".to_string()))),
        progress_current: Set(0),
        progress_total: Set(0),
        status_text: Set(None),
        attempt_count: Set(0),
        max_attempts: Set(1),
        next_run_at: Set(created_at),
        processing_token: Set(0),
        processing_started_at: Set(None),
        last_heartbeat_at: Set(None),
        lease_expires_at: Set(None),
        started_at: Set(None),
        finished_at: Set(finished_at),
        last_error: Set(None),
        failure_can_retry: Set(None),
        expires_at: Set(updated_at + Duration::hours(24)),
        created_at: Set(created_at),
        updated_at: Set(updated_at),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("background task test row should insert")
}

async fn set_task_lease(
    db: &sea_orm::DatabaseConnection,
    task: background_task::Model,
    lease_expires_at: chrono::DateTime<chrono::Utc>,
) -> background_task::Model {
    let mut active: background_task::ActiveModel = task.into();
    active.lease_expires_at = Set(Some(lease_expires_at));
    active
        .update(db)
        .await
        .expect("background task test lease should update")
}

#[tokio::test]
async fn release_processing_requeues_task_without_counting_attempt() {
    let db = build_test_db().await;
    let now = Utc::now();
    let task = insert_task(
        &db,
        BackgroundTaskKind::OfflineDownload,
        BackgroundTaskStatus::Processing,
        None,
        now,
    )
    .await;
    let mut active: background_task::ActiveModel = task.clone().into();
    active.processing_token = Set(7);
    active.processing_started_at = Set(Some(now));
    active.last_heartbeat_at = Set(Some(now));
    active.lease_expires_at = Set(Some(now + Duration::seconds(60)));
    active.status_text = Set(Some("Downloading source file".to_string()));
    let task = active
        .update(&db)
        .await
        .expect("processing task should update");
    let released_at = now + Duration::seconds(5);

    let released = release_processing(
        &db,
        task.id,
        task.processing_token,
        released_at,
        BackgroundTaskStatus::Retry,
    )
    .await
    .expect("processing task should release");

    assert!(released);
    let stored = background_task::Entity::find_by_id(task.id)
        .one(&db)
        .await
        .expect("released task should load")
        .expect("released task should exist");
    assert_eq!(stored.status, BackgroundTaskStatus::Retry);
    assert_eq!(stored.attempt_count, 0);
    assert_eq!(stored.next_run_at, released_at);
    assert_eq!(stored.processing_started_at, None);
    assert_eq!(stored.last_heartbeat_at, None);
    assert_eq!(stored.lease_expires_at, None);
    assert_eq!(stored.status_text, None);
}

#[tokio::test]
async fn release_processing_rejects_terminal_target_status() {
    let db = build_test_db().await;
    let now = Utc::now();
    let task = insert_task(
        &db,
        BackgroundTaskKind::OfflineDownload,
        BackgroundTaskStatus::Processing,
        None,
        now,
    )
    .await;

    let error = release_processing(
        &db,
        task.id,
        task.processing_token,
        now,
        BackgroundTaskStatus::Succeeded,
    )
    .await
    .expect_err("release target status must be non-terminal and dispatchable");

    assert!(error.message().contains("pending or retry"));
}

#[tokio::test]
async fn release_processing_uses_processing_token_fence() {
    let db = build_test_db().await;
    let now = Utc::now();
    let task = insert_task(
        &db,
        BackgroundTaskKind::OfflineDownload,
        BackgroundTaskStatus::Processing,
        None,
        now,
    )
    .await;
    let mut active: background_task::ActiveModel = task.clone().into();
    active.processing_token = Set(7);
    active.processing_started_at = Set(Some(now));
    active.last_heartbeat_at = Set(Some(now));
    active.lease_expires_at = Set(Some(now + Duration::seconds(60)));
    let task = active
        .update(&db)
        .await
        .expect("processing task should update");

    let released = release_processing(
        &db,
        task.id,
        task.processing_token + 1,
        now + Duration::seconds(5),
        BackgroundTaskStatus::Retry,
    )
    .await
    .expect("stale token release should not fail DB execution");

    assert!(!released);
    let stored = background_task::Entity::find_by_id(task.id)
        .one(&db)
        .await
        .expect("task should load")
        .expect("task should exist");
    assert_eq!(stored.status, BackgroundTaskStatus::Processing);
    assert_eq!(stored.processing_token, task.processing_token);
    assert_eq!(stored.lease_expires_at, task.lease_expires_at);
}

#[tokio::test]
async fn find_paginated_all_filtered_applies_kind_and_status() {
    let db = build_test_db().await;
    let now = Utc::now();
    let wanted = insert_task(
        &db,
        BackgroundTaskKind::ArchiveExtract,
        BackgroundTaskStatus::Failed,
        Some(now - Duration::hours(2)),
        now - Duration::minutes(5),
    )
    .await;
    insert_task(
        &db,
        BackgroundTaskKind::ArchiveExtract,
        BackgroundTaskStatus::Processing,
        None,
        now - Duration::minutes(4),
    )
    .await;
    insert_task(
        &db,
        BackgroundTaskKind::SystemRuntime,
        BackgroundTaskStatus::Failed,
        Some(now - Duration::hours(3)),
        now - Duration::minutes(3),
    )
    .await;

    let (items, total) = find_paginated_all_filtered(
        &db,
        20,
        0,
        &AdminTaskFilters {
            kind: Some(BackgroundTaskKind::ArchiveExtract),
            status: Some(BackgroundTaskStatus::Failed),
        },
        AdminTaskSortBy::UpdatedAt,
        SortOrder::Desc,
    )
    .await
    .expect("filtered admin task query should succeed");

    assert_eq!(total, 1);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, wanted.id);
}

#[tokio::test]
async fn count_active_processing_by_kinds_only_counts_unexpired_leases() {
    let db = build_test_db().await;
    let now = Utc::now();
    let active_archive = insert_task(
        &db,
        BackgroundTaskKind::ArchiveCompress,
        BackgroundTaskStatus::Processing,
        None,
        now - Duration::minutes(5),
    )
    .await;
    set_task_lease(&db, active_archive, now + Duration::seconds(30)).await;
    let stale_archive = insert_task(
        &db,
        BackgroundTaskKind::ArchiveExtract,
        BackgroundTaskStatus::Processing,
        None,
        now - Duration::minutes(4),
    )
    .await;
    set_task_lease(&db, stale_archive, now - Duration::seconds(1)).await;
    insert_task(
        &db,
        BackgroundTaskKind::ArchiveExtract,
        BackgroundTaskStatus::Processing,
        None,
        now - Duration::minutes(3),
    )
    .await;
    let active_thumbnail = insert_task(
        &db,
        BackgroundTaskKind::ThumbnailGenerate,
        BackgroundTaskStatus::Processing,
        None,
        now - Duration::minutes(2),
    )
    .await;
    set_task_lease(&db, active_thumbnail, now + Duration::seconds(30)).await;

    let archive_active = count_active_processing_by_kinds(
        &db,
        now,
        &[
            BackgroundTaskKind::ArchiveCompress,
            BackgroundTaskKind::ArchiveExtract,
        ],
    )
    .await
    .expect("archive active count should succeed");
    let thumbnail_active =
        count_active_processing_by_kinds(&db, now, &[BackgroundTaskKind::ThumbnailGenerate])
            .await
            .expect("thumbnail active count should succeed");

    assert_eq!(archive_active, 1);
    assert_eq!(thumbnail_active, 1);
}

#[tokio::test]
async fn list_claimable_by_kinds_filters_lane_kinds() {
    let db = build_test_db().await;
    let now = Utc::now();
    let archive = insert_task(
        &db,
        BackgroundTaskKind::ArchiveExtract,
        BackgroundTaskStatus::Pending,
        None,
        now - Duration::minutes(5),
    )
    .await;
    let thumbnail = insert_task(
        &db,
        BackgroundTaskKind::ThumbnailGenerate,
        BackgroundTaskStatus::Pending,
        None,
        now - Duration::minutes(4),
    )
    .await;

    let claimable = list_claimable_by_kinds(
        &db,
        now,
        now - Duration::seconds(60),
        &[
            BackgroundTaskKind::ArchiveCompress,
            BackgroundTaskKind::ArchiveExtract,
        ],
        10,
    )
    .await
    .expect("claimable kind filter should succeed");
    let ids = claimable
        .into_iter()
        .map(|task| task.id)
        .collect::<Vec<_>>();

    assert!(ids.contains(&archive.id));
    assert!(!ids.contains(&thumbnail.id));
}

#[tokio::test]
async fn delete_terminal_by_filters_only_removes_matching_completed_tasks() {
    let db = build_test_db().await;
    let now = Utc::now();
    let old_succeeded = insert_task(
        &db,
        BackgroundTaskKind::SystemRuntime,
        BackgroundTaskStatus::Succeeded,
        Some(now - Duration::hours(72)),
        now - Duration::hours(72),
    )
    .await;
    let old_failed = insert_task(
        &db,
        BackgroundTaskKind::SystemRuntime,
        BackgroundTaskStatus::Failed,
        Some(now - Duration::hours(60)),
        now - Duration::hours(60),
    )
    .await;
    let recent_failed = insert_task(
        &db,
        BackgroundTaskKind::SystemRuntime,
        BackgroundTaskStatus::Failed,
        Some(now - Duration::hours(4)),
        now - Duration::hours(4),
    )
    .await;
    let other_kind = insert_task(
        &db,
        BackgroundTaskKind::ArchiveExtract,
        BackgroundTaskStatus::Failed,
        Some(now - Duration::hours(60)),
        now - Duration::hours(60),
    )
    .await;
    let active_task = insert_task(
        &db,
        BackgroundTaskKind::SystemRuntime,
        BackgroundTaskStatus::Processing,
        None,
        now - Duration::hours(80),
    )
    .await;

    let removed = delete_terminal_by_filters(
        &db,
        &TerminalTaskCleanupFilters {
            finished_before: now - Duration::hours(24),
            kind: Some(BackgroundTaskKind::SystemRuntime),
            status: Some(BackgroundTaskStatus::Failed),
        },
    )
    .await
    .expect("task cleanup delete should succeed");

    assert_eq!(removed, 1);

    let remaining_ids = background_task::Entity::find()
        .all(&db)
        .await
        .expect("remaining tasks should load")
        .into_iter()
        .map(|task| task.id)
        .collect::<Vec<_>>();

    assert!(remaining_ids.contains(&old_succeeded.id));
    assert!(!remaining_ids.contains(&old_failed.id));
    assert!(remaining_ids.contains(&recent_failed.id));
    assert!(remaining_ids.contains(&other_kind.id));
    assert!(remaining_ids.contains(&active_task.id));
}
