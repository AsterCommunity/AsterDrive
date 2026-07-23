use std::sync::Arc;

use aster_forge_db::transaction;
use aster_forge_tasks::{
    TaskClaimCandidate, TaskLease, TaskLeaseGuard, available_lane_capacity,
    spawn_task_heartbeat_with_interval,
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use tokio::time::{Duration, sleep};

use crate::config::DatabaseConfig;
use crate::db::repository::background_task_repo;
use crate::db::{self, repository::config_repo};
use crate::entities::background_task;
use crate::errors::AsterError;
use crate::runtime::SharedRuntimeState;
use crate::services::task::{SystemRuntimeTaskKind, is_task_worker_shutdown_requested};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::types::{BackgroundTaskKind, BackgroundTaskStatus, StoredTaskPayload};
use migration::Migrator;
use tokio_util::sync::CancellationToken;

use super::claim::claim_candidates_for_lane;
use super::execute::{BackgroundTaskExecutionStore, run_claimed_tasks};
use super::lane::{TaskLane, TaskLaneConfig, task_lane};

async fn build_dispatch_test_db() -> sea_orm::DatabaseConnection {
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            pool_size: 1,
            retry_count: 0,
        },
        crate::metrics::NoopMetrics::arc(),
    )
    .await
    .expect("dispatch test DB should connect");
    Migrator::up(&db, None)
        .await
        .expect("dispatch test migrations should succeed");
    config_repo::ensure_defaults_with_env(&db, &|_| None)
        .await
        .expect("dispatch test config defaults should exist");
    db
}

async fn build_dispatch_test_state() -> crate::runtime::PrimaryAppState {
    let db = build_dispatch_test_db().await;
    let cache = aster_forge_cache::create_cache(&aster_forge_cache::CacheConfig {
        ..Default::default()
    })
    .await;
    let runtime_config = Arc::new(crate::config::RuntimeConfig::new());
    runtime_config
        .reload(&db)
        .await
        .expect("dispatch test runtime config should reload");
    let (storage_change_tx, _) = tokio::sync::broadcast::channel(
        crate::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
    );
    let (share_download_rollback, _worker) =
        crate::services::share::build_share_download_rollback_queue(
            db.clone(),
            1,
            crate::metrics::NoopMetrics::arc(),
        );

    crate::runtime::PrimaryAppState {
        db_handles: aster_forge_db::DbHandles::single(db),
        driver_registry: Arc::new(crate::storage::DriverRegistry::noop()),
        runtime_config,
        policy_snapshot: Arc::new(crate::storage::PolicySnapshot::new()),
        config: Arc::new(crate::config::Config::default()),
        cache,
        config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
        metrics: crate::metrics::NoopMetrics::arc(),
        mail_sender: aster_forge_mail::memory_sender(),
        storage_change_tx,
        share_download_rollback,
        background_task_dispatch_wakeup:
            crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
        remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
    }
}

async fn insert_dispatch_test_task(
    db: &sea_orm::DatabaseConnection,
    kind: BackgroundTaskKind,
    status: BackgroundTaskStatus,
    created_offset_secs: i64,
    lease_expires_at: Option<chrono::DateTime<Utc>>,
) -> background_task::Model {
    let now = Utc::now();
    background_task::ActiveModel {
        kind: Set(kind),
        status: Set(status),
        creator_user_id: Set(None),
        team_id: Set(None),
        share_id: Set(None),
        display_name: Set(format!("dispatch-claim-{created_offset_secs}")),
        payload_json: Set(StoredTaskPayload("{}".to_string())),
        result_json: Set(None),
        runtime_json: Set(None),
        steps_json: Set(None),
        progress_current: Set(0),
        progress_total: Set(0),
        status_text: Set(None),
        attempt_count: Set(0),
        max_attempts: Set(1),
        next_run_at: Set(now - chrono::Duration::seconds(1)),
        processing_token: Set(0),
        processing_started_at: Set(match status {
            BackgroundTaskStatus::Processing => Some(now - chrono::Duration::seconds(30)),
            _ => None,
        }),
        last_heartbeat_at: Set(match status {
            BackgroundTaskStatus::Processing => Some(now - chrono::Duration::seconds(30)),
            _ => None,
        }),
        lease_expires_at: Set(lease_expires_at),
        started_at: Set(match status {
            BackgroundTaskStatus::Processing => Some(now - chrono::Duration::seconds(30)),
            _ => None,
        }),
        finished_at: Set(None),
        last_error: Set(None),
        failure_can_retry: Set(None),
        expires_at: Set(now + chrono::Duration::hours(1)),
        created_at: Set(now + chrono::Duration::seconds(created_offset_secs)),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("dispatch test task should insert")
}

async fn insert_processing_system_runtime_task(
    db: &sea_orm::DatabaseConnection,
) -> background_task::Model {
    let now = Utc::now();
    background_task::ActiveModel {
        kind: Set(BackgroundTaskKind::SystemRuntime),
        status: Set(BackgroundTaskStatus::Processing),
        creator_user_id: Set(None),
        team_id: Set(None),
        share_id: Set(None),
        display_name: Set("dispatch system runtime".to_string()),
        payload_json: Set(crate::services::task::runtime::system_runtime_payload_json(
            SystemRuntimeTaskKind::BackgroundTaskDispatch,
        )
        .expect("system runtime payload should serialize")),
        result_json: Set(None),
        runtime_json: Set(None),
        steps_json: Set(None),
        progress_current: Set(0),
        progress_total: Set(1),
        status_text: Set(Some("Processing".to_string())),
        attempt_count: Set(0),
        max_attempts: Set(1),
        next_run_at: Set(now),
        processing_token: Set(7),
        processing_started_at: Set(Some(now)),
        last_heartbeat_at: Set(Some(now)),
        lease_expires_at: Set(Some(now + chrono::Duration::seconds(60))),
        started_at: Set(Some(now)),
        finished_at: Set(None),
        last_error: Set(None),
        failure_can_retry: Set(None),
        expires_at: Set(now + chrono::Duration::hours(1)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("processing system runtime task should insert")
}

fn claim_candidate(index: usize, task: &background_task::Model) -> TaskClaimCandidate {
    TaskClaimCandidate {
        index,
        task_id: task.id,
        expected_processing_token: task.processing_token,
        next_processing_token: task.processing_token + 1,
    }
}

fn test_lane_config(lane: TaskLane, limit: usize, fast_continue: bool) -> TaskLaneConfig {
    let lock_key = match lane {
        TaskLane::Archive => crate::config::operations::BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY_KEY,
        TaskLane::Thumbnail => {
            crate::config::operations::BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY_KEY
        }
        TaskLane::OfflineDownload => {
            crate::config::operations::OFFLINE_DOWNLOAD_MAX_CONCURRENCY_KEY
        }
        TaskLane::StorageMigration => {
            crate::config::operations::BACKGROUND_TASK_STORAGE_MIGRATION_MAX_CONCURRENCY_KEY
        }
        TaskLane::Fallback => crate::config::operations::BACKGROUND_TASK_MAX_CONCURRENCY_KEY,
    };
    TaskLaneConfig {
        lane,
        kinds: super::super::registry::task_lane_kinds(lane),
        limit,
        fast_continue,
        lock_key,
    }
}

#[tokio::test]
async fn run_claimed_tasks_marks_non_retryable_task_failure() {
    let state = build_dispatch_test_state().await;
    let task = insert_processing_system_runtime_task(state.writer_db()).await;
    let lease = TaskLease::new(task.id, task.processing_token);

    let stats = run_claimed_tasks(
        &state,
        vec![(task.clone(), lease)],
        CancellationToken::new(),
    )
    .await
    .expect("non-retryable task failure should be recorded, not returned as dispatch error");

    assert_eq!(stats.claimed, 0);
    assert_eq!(stats.succeeded, 0);
    assert_eq!(stats.retried, 0);
    assert_eq!(stats.failed, 1);

    let stored = background_task_repo::find_by_id(state.writer_db(), task.id)
        .await
        .expect("failed task should still exist");
    assert_eq!(stored.status, BackgroundTaskStatus::Failed);
    assert_eq!(stored.attempt_count, 1);
    assert_eq!(stored.processing_started_at, None);
    assert_eq!(stored.last_heartbeat_at, None);
    assert_eq!(stored.lease_expires_at, None);
    assert_eq!(stored.failure_can_retry, Some(false));
    assert!(
        stored
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("should not be dispatched"))
    );
    assert!(stored.finished_at.is_some());
}

#[tokio::test]
async fn run_claimed_tasks_releases_pre_cancelled_task_without_running_handler() {
    let state = build_dispatch_test_state().await;
    let task = insert_processing_system_runtime_task(state.writer_db()).await;
    let lease = TaskLease::new(task.id, task.processing_token);
    let shutdown_token = CancellationToken::new();
    shutdown_token.cancel();

    let stats = run_claimed_tasks(&state, vec![(task.clone(), lease)], shutdown_token)
        .await
        .expect("shutdown release should be handled as a cooperative worker stop");

    assert_eq!(stats, aster_forge_tasks::DispatchStats::default());

    let stored = background_task_repo::find_by_id(state.writer_db(), task.id)
        .await
        .expect("released task should still exist");
    assert_eq!(stored.status, BackgroundTaskStatus::Retry);
    assert_eq!(stored.attempt_count, 0);
    assert_eq!(stored.processing_started_at, None);
    assert_eq!(stored.last_heartbeat_at, None);
    assert_eq!(stored.lease_expires_at, None);
    assert_eq!(stored.status_text, None);
    assert_eq!(stored.last_error, None);
    assert_eq!(stored.failure_can_retry, None);
    assert_eq!(stored.finished_at, None);
}

#[tokio::test]
async fn task_heartbeat_can_stop_while_sqlite_writer_pool_is_busy() {
    let state = build_dispatch_test_state().await;
    let task = insert_processing_system_runtime_task(state.writer_db()).await;
    let lease = TaskLease::new(task.id, task.processing_token);
    let lease_guard = TaskLeaseGuard::new(lease, Duration::from_secs(60));
    let stop_token = CancellationToken::new();
    let writer_txn = transaction::begin(state.writer_db())
        .await
        .expect("test should acquire the only SQLite writer connection");

    // Regression guard for SQLite single-writer deployments: heartbeat may be
    // waiting in pool acquire while task code holds the only writer connection,
    // but cancelling the heartbeat must still let task completion proceed.
    let heartbeat = spawn_task_heartbeat_with_interval(
        BackgroundTaskExecutionStore::new(state.clone()),
        lease_guard,
        stop_token.clone(),
        Duration::from_millis(10),
        |now| aster_forge_tasks::task_lease_expires_at(now, super::TASK_PROCESSING_STALE_SECS),
    );
    sleep(Duration::from_millis(30)).await;

    stop_token.cancel();
    tokio::time::timeout(Duration::from_millis(200), heartbeat)
        .await
        .expect("heartbeat should stop without waiting for the busy SQLite writer pool")
        .expect("heartbeat task should not panic");

    transaction::rollback(writer_txn)
        .await
        .expect("test writer transaction should roll back");
}

#[test]
fn task_lane_keeps_archive_and_thumbnail_separate() {
    assert_eq!(
        task_lane(BackgroundTaskKind::ArchiveCompress),
        TaskLane::Archive
    );
    assert_eq!(
        task_lane(BackgroundTaskKind::ArchiveExtract),
        TaskLane::Archive
    );
    assert_eq!(
        task_lane(BackgroundTaskKind::ArchivePreviewGenerate),
        TaskLane::Archive
    );
    assert_eq!(
        task_lane(BackgroundTaskKind::ThumbnailGenerate),
        TaskLane::Thumbnail
    );
    assert_eq!(
        task_lane(BackgroundTaskKind::ImagePreviewGenerate),
        TaskLane::Thumbnail
    );
    assert_eq!(
        task_lane(BackgroundTaskKind::MediaMetadataExtract),
        TaskLane::Thumbnail
    );
    assert_eq!(
        task_lane(BackgroundTaskKind::OfflineDownload),
        TaskLane::OfflineDownload
    );
    assert_eq!(
        task_lane(BackgroundTaskKind::TrashPurgeAll),
        TaskLane::Fallback
    );
    assert_eq!(
        task_lane(BackgroundTaskKind::SystemRuntime),
        TaskLane::Fallback
    );
    assert_eq!(
        task_lane(BackgroundTaskKind::StoragePolicyTempCleanup),
        TaskLane::Fallback
    );
    assert_eq!(
        task_lane(BackgroundTaskKind::StoragePolicyMigration),
        TaskLane::StorageMigration
    );
}

#[test]
fn available_lane_capacity_saturates_when_active_exceeds_limit() {
    assert_eq!(available_lane_capacity(3, 1), 2);
    assert_eq!(available_lane_capacity(3, 3), 0);
    assert_eq!(available_lane_capacity(3, 4), 0);
    assert_eq!(available_lane_capacity(3, u64::MAX), 0);
}

#[tokio::test]
async fn claim_candidates_for_lane_claims_batch_up_to_rechecked_capacity() {
    let db = build_dispatch_test_db().await;
    let tasks = [
        insert_dispatch_test_task(
            &db,
            BackgroundTaskKind::ArchiveCompress,
            BackgroundTaskStatus::Pending,
            -3,
            None,
        )
        .await,
        insert_dispatch_test_task(
            &db,
            BackgroundTaskKind::ArchiveExtract,
            BackgroundTaskStatus::Pending,
            -2,
            None,
        )
        .await,
        insert_dispatch_test_task(
            &db,
            BackgroundTaskKind::ArchiveCompress,
            BackgroundTaskStatus::Pending,
            -1,
            None,
        )
        .await,
    ];
    let candidates = tasks
        .iter()
        .enumerate()
        .map(|(index, task)| claim_candidate(index, task))
        .collect::<Vec<_>>();

    let claimed_at = Utc::now();
    let claimed = claim_candidates_for_lane(
        &db,
        test_lane_config(TaskLane::Archive, 2, true),
        &candidates,
        claimed_at - chrono::Duration::seconds(60),
        claimed_at,
        aster_forge_tasks::task_lease_expires_at(claimed_at, super::TASK_PROCESSING_STALE_SECS),
    )
    .await
    .expect("batch claim should succeed");

    assert_eq!(claimed.len(), 2);
    assert_eq!(claimed[0].task_id, tasks[0].id);
    assert_eq!(claimed[1].task_id, tasks[1].id);
    assert_eq!(claimed[0].processing_token, 1);
    assert_eq!(claimed[1].processing_token, 1);

    let stored = background_task::Entity::find()
        .all(&db)
        .await
        .expect("stored tasks should load");
    let processing = stored
        .iter()
        .filter(|task| task.status == BackgroundTaskStatus::Processing)
        .map(|task| task.id)
        .collect::<Vec<_>>();
    assert!(processing.contains(&tasks[0].id));
    assert!(processing.contains(&tasks[1].id));
    assert!(!processing.contains(&tasks[2].id));
}

#[tokio::test]
async fn claim_candidates_for_lane_skips_claim_when_rechecked_capacity_is_full() {
    let db = build_dispatch_test_db().await;
    let now = Utc::now();
    insert_dispatch_test_task(
        &db,
        BackgroundTaskKind::ThumbnailGenerate,
        BackgroundTaskStatus::Processing,
        -3,
        Some(now + chrono::Duration::seconds(60)),
    )
    .await;
    let pending = insert_dispatch_test_task(
        &db,
        BackgroundTaskKind::ThumbnailGenerate,
        BackgroundTaskStatus::Pending,
        -1,
        None,
    )
    .await;
    let candidates = vec![claim_candidate(0, &pending)];

    let claimed_at = Utc::now();
    let claimed = claim_candidates_for_lane(
        &db,
        test_lane_config(TaskLane::Thumbnail, 1, true),
        &candidates,
        claimed_at - chrono::Duration::seconds(60),
        claimed_at,
        aster_forge_tasks::task_lease_expires_at(claimed_at, super::TASK_PROCESSING_STALE_SECS),
    )
    .await
    .expect("full lane batch claim should succeed without claiming");

    assert!(claimed.is_empty());
    let stored = background_task_repo::find_by_id(&db, pending.id)
        .await
        .expect("pending task should still exist");
    assert_eq!(stored.status, BackgroundTaskStatus::Pending);
    assert_eq!(stored.processing_token, 0);
}

#[tokio::test]
async fn claim_candidates_for_lane_continues_after_stale_candidate_loses_cas() {
    let db = build_dispatch_test_db().await;
    let stale = insert_dispatch_test_task(
        &db,
        BackgroundTaskKind::ArchiveCompress,
        BackgroundTaskStatus::Pending,
        -2,
        None,
    )
    .await;
    let next = insert_dispatch_test_task(
        &db,
        BackgroundTaskKind::ArchiveCompress,
        BackgroundTaskStatus::Pending,
        -1,
        None,
    )
    .await;
    let candidates = vec![
        TaskClaimCandidate {
            index: 0,
            task_id: stale.id,
            expected_processing_token: stale.processing_token + 1,
            next_processing_token: stale.processing_token + 2,
        },
        claim_candidate(1, &next),
    ];

    let claimed_at = Utc::now();
    let claimed = claim_candidates_for_lane(
        &db,
        test_lane_config(TaskLane::Archive, 1, true),
        &candidates,
        claimed_at - chrono::Duration::seconds(60),
        claimed_at,
        aster_forge_tasks::task_lease_expires_at(claimed_at, super::TASK_PROCESSING_STALE_SECS),
    )
    .await
    .expect("batch claim should skip stale CAS misses");

    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].task_id, next.id);
    let stale = background_task_repo::find_by_id(&db, stale.id)
        .await
        .expect("stale candidate should still exist");
    assert_eq!(stale.status, BackgroundTaskStatus::Pending);
    assert_eq!(stale.processing_token, 0);
}

#[tokio::test]
async fn forge_task_context_preserves_drive_shutdown_error_code() {
    let lease = TaskLease::new(42, 7);
    let shutdown_token = CancellationToken::new();
    let context = aster_forge_tasks::TaskExecutionContext::new(
        lease,
        Duration::from_secs(60),
        shutdown_token.clone(),
    );

    shutdown_token.cancel();

    let error = context
        .ensure_active()
        .map_err(AsterError::from)
        .expect_err("cancelled shutdown token should stop the worker");
    assert!(is_task_worker_shutdown_requested(&error));
    assert_eq!(
        error.api_error_code_override(),
        Some(crate::api::api_error_code::ApiErrorCode::TaskWorkerShutdownRequested)
    );
}

#[test]
fn thumbnail_retry_only_keeps_transient_storage_errors() {
    let transient = storage_driver_error(StorageErrorKind::Transient, "remote timeout");
    let misconfigured = storage_driver_error(StorageErrorKind::Misconfigured, "missing bucket");

    assert!(
        super::super::registry::task_retry_class(BackgroundTaskKind::ThumbnailGenerate, &transient)
            .should_auto_retry()
    );
    assert!(
        !super::super::registry::task_retry_class(
            BackgroundTaskKind::ThumbnailGenerate,
            &misconfigured,
        )
        .can_manual_retry()
    );
    assert!(
        super::super::registry::task_retry_class(
            BackgroundTaskKind::ImagePreviewGenerate,
            &transient,
        )
        .should_auto_retry()
    );
    assert!(
        !super::super::registry::task_retry_class(
            BackgroundTaskKind::ImagePreviewGenerate,
            &misconfigured,
        )
        .can_manual_retry()
    );
    assert!(
        super::super::registry::task_retry_class(
            BackgroundTaskKind::MediaMetadataExtract,
            &transient,
        )
        .should_auto_retry()
    );
    assert!(
        !super::super::registry::task_retry_class(
            BackgroundTaskKind::MediaMetadataExtract,
            &misconfigured,
        )
        .can_manual_retry()
    );
}

#[test]
fn archive_validation_errors_are_not_retryable() {
    let error = AsterError::validation_error("archive entry compression ratio exceeds limit");
    let retry_class =
        super::super::registry::task_retry_class(BackgroundTaskKind::ArchiveExtract, &error);

    assert!(!retry_class.should_auto_retry());
    assert!(!retry_class.can_manual_retry());
}

#[test]
fn archive_transient_storage_errors_are_auto_retryable() {
    let error = storage_driver_error(StorageErrorKind::Transient, "remote timeout");
    let retry_class =
        super::super::registry::task_retry_class(BackgroundTaskKind::ArchiveCompress, &error);

    assert!(retry_class.should_auto_retry());
    assert!(retry_class.can_manual_retry());
}
