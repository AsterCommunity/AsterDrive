use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use chrono::Utc;
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use tokio::time::{Duration, sleep};

use crate::config::DatabaseConfig;
use crate::db::repository::background_task_repo;
use crate::db::{self, repository::config_repo, transaction};
use crate::entities::background_task;
use crate::errors::AsterError;
use crate::runtime::SharedRuntimeState;
use crate::services::task::{
    SystemRuntimeTaskKind, TaskExecutionContext, TaskLease, TaskLeaseGuard, is_task_lease_lost,
    is_task_lease_renewal_timed_out, is_task_worker_shutdown_requested,
};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::types::{BackgroundTaskKind, BackgroundTaskStatus, StoredTaskPayload};
use migration::Migrator;
use tokio_util::sync::CancellationToken;

use super::claim::{TaskClaimCandidate, available_lane_capacity, claim_candidates_for_lane};
use super::execute::{
    evaluate_heartbeat_result, run_claimed_tasks, run_with_concurrency_limit,
    spawn_task_heartbeat_with_interval, task_retry_class,
};
use super::lane::{TaskLane, TaskLaneConfig, task_lane};

async fn build_dispatch_test_db() -> sea_orm::DatabaseConnection {
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            pool_size: 1,
            retry_count: 0,
        },
        crate::metrics_core::NoopMetrics::arc(),
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
    let cache = crate::cache::create_cache(&crate::config::CacheConfig {
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
            crate::metrics_core::NoopMetrics::arc(),
        );

    crate::runtime::PrimaryAppState {
        db_handles: crate::db::DbHandles::single(db),
        driver_registry: Arc::new(crate::storage::DriverRegistry::noop()),
        runtime_config,
        policy_snapshot: Arc::new(crate::storage::PolicySnapshot::new()),
        config: Arc::new(crate::config::Config::default()),
        cache,
        metrics: crate::metrics_core::NoopMetrics::arc(),
        mail_sender: crate::services::mail::sender::memory_sender(),
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
        payload_json: Set(
            crate::services::task::runtime::system_runtime_payload_json(
                SystemRuntimeTaskKind::BackgroundTaskDispatch,
            )
            .expect("system runtime payload should serialize"),
        ),
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

#[tokio::test]
async fn run_with_concurrency_limit_caps_parallelism() {
    let in_flight = Arc::new(AtomicUsize::new(0));
    let max_in_flight = Arc::new(AtomicUsize::new(0));

    let mut results = run_with_concurrency_limit(vec![1, 2, 3, 4, 5], 2, {
        let in_flight = in_flight.clone();
        let max_in_flight = max_in_flight.clone();
        move |value| {
            let in_flight = in_flight.clone();
            let max_in_flight = max_in_flight.clone();
            async move {
                let current = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                if let Err(e) =
                    max_in_flight.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |existing| {
                        Some(existing.max(current))
                    })
                {
                    tracing::trace!("max_in_flight fetch_update failed: {e}");
                }
                sleep(Duration::from_millis(20)).await;
                in_flight.fetch_sub(1, Ordering::SeqCst);
                value * 2
            }
        }
    })
    .await;

    results.sort_unstable();
    assert_eq!(results, vec![2, 4, 6, 8, 10]);
    assert_eq!(max_in_flight.load(Ordering::SeqCst), 2);
    assert_eq!(in_flight.load(Ordering::SeqCst), 0);
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

    assert_eq!(stats, super::DispatchStats::default());

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
    let lease_guard = TaskLeaseGuard::with_renewal_timeout(lease, Duration::from_secs(60));
    let stop_token = CancellationToken::new();
    let writer_txn = transaction::begin(state.writer_db())
        .await
        .expect("test should acquire the only SQLite writer connection");

    // Regression guard for SQLite single-writer deployments: heartbeat may be
    // waiting in pool acquire while task code holds the only writer connection,
    // but cancelling the heartbeat must still let task completion proceed.
    let heartbeat = spawn_task_heartbeat_with_interval(
        state.clone(),
        task.id,
        task.processing_token,
        lease_guard,
        stop_token.clone(),
        Duration::from_millis(10),
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

    let claimed = claim_candidates_for_lane(
        &db,
        TaskLaneConfig {
            lane: TaskLane::Archive,
            limit: 2,
            fast_continue: true,
        },
        &candidates,
        Utc::now() - chrono::Duration::seconds(60),
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

    let claimed = claim_candidates_for_lane(
        &db,
        TaskLaneConfig {
            lane: TaskLane::Thumbnail,
            limit: 1,
            fast_continue: true,
        },
        &candidates,
        Utc::now() - chrono::Duration::seconds(60),
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

    let claimed = claim_candidates_for_lane(
        &db,
        TaskLaneConfig {
            lane: TaskLane::Archive,
            limit: 1,
            fast_continue: true,
        },
        &candidates,
        Utc::now() - chrono::Duration::seconds(60),
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

#[test]
fn evaluate_heartbeat_result_keeps_retrying_after_transient_error() {
    let lease = TaskLease::new(42, 7);
    let lease_guard = TaskLeaseGuard::with_renewal_timeout(lease, Duration::from_secs(60));
    let result =
        evaluate_heartbeat_result(&lease_guard, Err(AsterError::database_operation("boom")));
    assert!(result.is_ok());
}

#[test]
fn evaluate_heartbeat_result_reports_lease_loss_when_claim_replaced() {
    let lease = TaskLease::new(42, 7);
    let lease_guard = TaskLeaseGuard::with_renewal_timeout(lease, Duration::from_secs(60));
    let error = evaluate_heartbeat_result(&lease_guard, Ok(false))
        .expect_err("heartbeat mismatch should report lease loss");
    assert!(is_task_lease_lost(&error));
}

#[tokio::test]
async fn evaluate_heartbeat_result_stops_worker_after_renewal_timeout() {
    let lease = TaskLease::new(42, 7);
    let lease_guard = TaskLeaseGuard::with_renewal_timeout(lease, Duration::from_millis(20));
    sleep(Duration::from_millis(30)).await;

    let error =
        evaluate_heartbeat_result(&lease_guard, Err(AsterError::database_operation("boom")))
            .expect_err("expired renewal window should stop the worker");
    assert!(is_task_lease_renewal_timed_out(&error));
}

#[tokio::test]
async fn task_execution_context_reports_shutdown_request() {
    let lease = TaskLease::new(42, 7);
    let shutdown_token = CancellationToken::new();
    let context = TaskExecutionContext::new(lease, shutdown_token.clone());

    shutdown_token.cancel();

    let error = context
        .ensure_active()
        .expect_err("cancelled shutdown token should stop the worker");
    assert!(is_task_worker_shutdown_requested(&error));
    assert_eq!(
        error.api_error_code_override(),
        Some(crate::api::api_error_code::ApiErrorCode::TaskWorkerShutdownRequested)
    );

    let error = context
        .ensure_active()
        .expect_err("shutdown termination should remain sticky");
    assert!(is_task_worker_shutdown_requested(&error));
}

#[tokio::test]
async fn task_execution_context_shutdown_is_visible_through_cloned_lease_guard() {
    let lease = TaskLease::new(42, 7);
    let shutdown_token = CancellationToken::new();
    let context = TaskExecutionContext::new(lease, shutdown_token.clone());
    let lease_guard = context.lease_guard().clone();

    shutdown_token.cancel();

    let error = lease_guard
        .ensure_active()
        .expect_err("cloned lease guard should inherit shutdown cancellation");
    assert!(is_task_worker_shutdown_requested(&error));

    let error = context
        .ensure_active()
        .expect_err("shutdown observed through a clone should remain sticky");
    assert!(is_task_worker_shutdown_requested(&error));
}

#[tokio::test]
async fn task_execution_context_shutdown_requested_waits_until_cancelled() {
    let lease = TaskLease::new(42, 7);
    let shutdown_token = CancellationToken::new();
    let context = TaskExecutionContext::new(lease, shutdown_token.clone());

    assert!(
        tokio::time::timeout(Duration::from_millis(10), context.shutdown_requested())
            .await
            .is_err()
    );

    shutdown_token.cancel();
    let error = context
        .shutdown_requested()
        .await
        .expect_err("cancelled token should resolve as a shutdown request");
    assert!(is_task_worker_shutdown_requested(&error));
}

#[tokio::test]
async fn task_execution_context_sleep_wakes_on_shutdown() {
    let lease = TaskLease::new(42, 7);
    let shutdown_token = CancellationToken::new();
    let context = TaskExecutionContext::new(lease, shutdown_token.clone());

    shutdown_token.cancel();

    let error = context
        .sleep_or_shutdown(Duration::from_secs(60))
        .await
        .expect_err("cancelled shutdown token should interrupt sleeps");
    assert!(is_task_worker_shutdown_requested(&error));
}

#[tokio::test]
async fn task_execution_context_sleep_without_shutdown_completes_normally() {
    let lease = TaskLease::new(42, 7);
    let lease_guard = TaskLeaseGuard::with_renewal_timeout(lease, Duration::from_secs(60));
    let context = TaskExecutionContext::with_lease_guard(lease_guard, CancellationToken::new());

    tokio::time::timeout(
        Duration::from_millis(50),
        context.sleep_or_shutdown(Duration::from_millis(1)),
    )
    .await
    .expect("sleep without shutdown token should complete")
    .expect("sleep without shutdown token should not report shutdown");
}

#[test]
fn thumbnail_retry_only_keeps_transient_storage_errors() {
    let transient = storage_driver_error(StorageErrorKind::Transient, "remote timeout");
    let misconfigured = storage_driver_error(StorageErrorKind::Misconfigured, "missing bucket");

    assert!(
        task_retry_class(BackgroundTaskKind::ThumbnailGenerate, &transient).should_auto_retry()
    );
    assert!(
        !task_retry_class(BackgroundTaskKind::ThumbnailGenerate, &misconfigured).can_manual_retry()
    );
    assert!(
        task_retry_class(BackgroundTaskKind::ImagePreviewGenerate, &transient).should_auto_retry()
    );
    assert!(
        !task_retry_class(BackgroundTaskKind::ImagePreviewGenerate, &misconfigured)
            .can_manual_retry()
    );
    assert!(
        task_retry_class(BackgroundTaskKind::MediaMetadataExtract, &transient).should_auto_retry()
    );
    assert!(
        !task_retry_class(BackgroundTaskKind::MediaMetadataExtract, &misconfigured)
            .can_manual_retry()
    );
}

#[test]
fn archive_validation_errors_are_not_retryable() {
    let error = AsterError::validation_error("archive entry compression ratio exceeds limit");
    let retry_class = task_retry_class(BackgroundTaskKind::ArchiveExtract, &error);

    assert!(!retry_class.should_auto_retry());
    assert!(!retry_class.can_manual_retry());
}

#[test]
fn archive_transient_storage_errors_are_auto_retryable() {
    let error = storage_driver_error(StorageErrorKind::Transient, "remote timeout");
    let retry_class = task_retry_class(BackgroundTaskKind::ArchiveCompress, &error);

    assert!(retry_class.should_auto_retry());
    assert!(retry_class.can_manual_retry());
}
