use super::*;
use crate::db::repository::{background_task_repo, config_repo};
use crate::runtime::TaskRuntimeState;
use crate::services::workspace::storage::WorkspaceStorageScope;
use aster_drive_model::entities::{background_task, user};
use aster_drive_model::types::{BackgroundTaskKind, BackgroundTaskStatus, UserRole, UserStatus};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use std::sync::Arc;
use tracing::instrument::WithSubscriber;

async fn exercise_real_consumers(
    enqueue_before_start: bool,
    shared_quota_full: bool,
) -> web::Data<PrimaryAppState> {
    let state = super::super::tests::build_dispatch_test_state().await;
    for key in [
        crate::config::definitions::BACKGROUND_TASK_DISPATCH_INTERVAL_SECS_KEY,
        crate::config::definitions::BACKGROUND_TASK_DISPATCH_IDLE_MAX_INTERVAL_SECS_KEY,
    ] {
        config_repo::upsert_with_actor(state.writer_db(), key, "60", None)
            .await
            .unwrap();
    }
    config_repo::upsert_with_actor(
        state.writer_db(),
        crate::config::definitions::TASK_FALLBACK_CLUSTER_CONCURRENCY_KEY,
        "1",
        None,
    )
    .await
    .unwrap();
    state
        .runtime_config
        .reload(state.writer_db())
        .await
        .unwrap();
    let now = Utc::now();
    let owner = user::ActiveModel {
        username: Set("wakeup-owner".into()),
        email: Set("wakeup@example.test".into()),
        password_hash: Set("unused".into()),
        role: Set(UserRole::User),
        status: Set(UserStatus::Active),
        session_version: Set(0),
        email_verified_at: Set(Some(now)),
        storage_used: Set(0),
        storage_quota: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .unwrap();
    let occupied = if shared_quota_full {
        Some(
            super::super::tests::insert_dispatch_test_task(
                state.writer_db(),
                BackgroundTaskKind::TrashPurgeAll,
                BackgroundTaskStatus::Processing,
                -1,
                Some(now + chrono::Duration::minutes(1)),
            )
            .await,
        )
    } else {
        None
    };
    let scope = WorkspaceStorageScope::Personal { user_id: owner.id };
    let mut task_id = None;
    if enqueue_before_start {
        task_id = Some(
            crate::services::task::trash::create_trash_purge_all_task_in_scope(&state, scope)
                .await
                .unwrap()
                .id,
        );
    }
    let state = web::Data::new(state);
    let stop = CancellationToken::new();
    let _cancel_on_drop = stop.clone().drop_guard();
    let statuses = consumer_statuses();
    let mut runners = tokio::task::JoinSet::new();
    for (lane, status) in TASK_LANES.into_iter().zip(statuses.clone()) {
        runners.spawn(
            aster_forge_tasks::run_task_consumer(
                LaneConsumer {
                    state: state.clone(),
                    lane,
                },
                stop.clone(),
                aster_forge_tasks::TaskConsumerConfig::default(),
                status,
            )
            .with_current_subscriber(),
        );
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        while !statuses.iter().all(TaskRuntimeStatus::ready) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("all five lane subscriptions must be initialized");
    let task_id = match task_id {
        Some(id) => id,
        None => {
            crate::services::task::trash::create_trash_purge_all_task_in_scope(
                state.get_ref(),
                scope,
            )
            .await
            .unwrap()
            .id
        }
    };
    if let Some(occupied) = occupied {
        let config = super::super::lane::task_lane_configs(state.get_ref())
            .into_iter()
            .find(|config| config.lane == TaskLane::Fallback)
            .unwrap();
        assert!(
            claim_due_for_lane(state.get_ref(), config, 1)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            background_task_repo::find_by_id(state.writer_db(), task_id)
                .await
                .unwrap()
                .status,
            BackgroundTaskStatus::Pending
        );
        background_task::Entity::delete_by_id(occupied.id)
            .exec(state.writer_db())
            .await
            .unwrap();
        state.task_wakeup().notify();
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let task = background_task_repo::find_by_id(state.writer_db(), task_id)
                .await
                .unwrap();
            if task.status == BackgroundTaskStatus::Succeeded {
                break;
            }
            assert_ne!(
                task.status,
                BackgroundTaskStatus::Failed,
                "empty-trash handler must succeed"
            );
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Fallback must execute promptly; its next polling timer is 60 seconds away");
    stop.cancel();
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(report) = runners.join_next().await {
            let report = report.unwrap().unwrap();
            assert_eq!(report.claim_errors, 0);
            assert_eq!(report.execution_errors, 0);
        }
    })
    .await
    .unwrap();
    assert!(statuses.iter().all(|status| !status.is_running()));
    state
}

#[tokio::test]
async fn empty_trash_enqueue_wakes_fallback_among_five_idle_lanes() {
    exercise_real_consumers(false, false).await;
}

#[tokio::test]
async fn startup_discovers_trash_enqueued_before_consumer_subscription() {
    exercise_real_consumers(true, false).await;
}

#[tokio::test]
async fn shared_quota_blocks_claim_and_release_notification_resumes_work() {
    exercise_real_consumers(false, true).await;
}

#[tokio::test]
async fn real_consumer_diagnostics_preserve_lane_identity_and_safe_failure_boundaries() {
    use crate::services::task::spec::TrashPurgeAllTask;
    use crate::services::task::types::TrashPurgeAllTaskPayload;
    use crate::services::task::{create_typed_task_record, retry_task_in_scope};
    use aster_forge_tasks::TaskClaimCandidate;
    use aster_forge_test::logs::CapturedLogs;

    struct SamplingEnabled;
    impl aster_drive_metrics::MetricsRecorder for SamplingEnabled {
        fn enabled(&self) -> bool {
            true
        }
    }
    let logs = CapturedLogs::default();
    logs.capture_future(async {
        let state = exercise_real_consumers(false, false).await;
        let owner =
            crate::db::repository::user_repo::find_by_username(state.writer_db(), "wakeup-owner")
                .await
                .unwrap()
                .unwrap();
        let scope = WorkspaceStorageScope::Personal { user_id: owner.id };
        // Exercise the common enqueue primitive after consumers have stopped: it still
        // commits the row and emits safe diagnostics without exposing its display name.
        let task = create_typed_task_record::<TrashPurgeAllTask>(
            state.get_ref(),
            scope,
            "SECRET-display-canary",
            &TrashPurgeAllTaskPayload {},
        )
        .await
        .unwrap();
        let mut failed: background_task::ActiveModel = task.clone().into();
        failed.status = Set(BackgroundTaskStatus::Failed);
        failed.failure_can_retry = Set(Some(true));
        failed.finished_at = Set(Some(Utc::now()));
        failed.update(state.writer_db()).await.unwrap();
        retry_task_in_scope(state.get_ref(), scope, task.id)
            .await
            .unwrap();
        let consumer = LaneConsumer {
            state: state.clone(),
            lane: TaskLane::Fallback,
        };
        // A stale selection must lose CAS without changing the queued task/token.
        let claimed = super::super::claim::claim_candidates_for_lane(
            state.writer_db(),
            consumer.config(),
            1,
            &[TaskClaimCandidate {
                index: 0,
                task_id: task.id,
                expected_processing_token: 99,
                next_processing_token: 100,
            }],
            Utc::now() - chrono::Duration::minutes(1),
            Utc::now(),
            Utc::now() + chrono::Duration::minutes(1),
        )
        .await
        .unwrap();
        assert!(claimed.is_empty());
        let untouched = background_task_repo::find_by_id(state.writer_db(), task.id)
            .await
            .unwrap();
        assert_eq!(untouched.status, BackgroundTaskStatus::Pending);
        assert_eq!(untouched.processing_token, 0);
        config_repo::upsert_with_actor(
            state.writer_db(),
            crate::config::definitions::TASK_FALLBACK_CLUSTER_CONCURRENCY_KEY,
            "SECRET-quota-canary",
            None,
        )
        .await
        .unwrap();
        let error = consumer.claim(1).await.unwrap_err();
        let kind = consumer.error_kind(&error);
        assert!(!kind.contains("SECRET"));
        tracing::warn!(error_kind = kind, "consumer configuration rejected");
        // Sampling failure must not hide the original claim failure or log a raw DB error.
        let mut closed = state.get_ref().clone();
        closed.metrics = Arc::new(SamplingEnabled);
        closed.writer_db().close_by_ref().await.unwrap();
        let consumer = LaneConsumer {
            state: web::Data::new(closed),
            lane: TaskLane::Fallback,
        };
        let error = consumer.claim(1).await.unwrap_err();
        assert_eq!(consumer.error_kind(&error), "database_operation");
        tracing::warn!(
            error_kind = consumer.error_kind(&error),
            "consumer database operation rejected"
        );
        let error = crate::errors::AsterError::database_connection("SECRET-connection-canary");
        assert_eq!(consumer.error_kind(&error), "database_connection");
        tracing::warn!(
            error_kind = consumer.error_kind(&error),
            "consumer connection rejected"
        );
    })
    .await;
    let text = logs.text();
    for name in [
        "archive",
        "thumbnail",
        "offline_download",
        "storage_migration",
        "fallback",
    ] {
        assert!(
            text.contains(&format!("consumer=\"{name}\"")),
            "missing consumer {name}: {text}"
        );
    }
    for field in [
        "enqueue_committed",
        "retry_committed",
        "cas_miss",
        "failed to refresh background task pending metric",
        "database_operation",
        "database_connection",
    ] {
        assert!(text.contains(field), "missing {field}: {text}");
    }
    for canary in [
        "SECRET-display-canary",
        "SECRET-quota-canary",
        "SECRET-connection-canary",
    ] {
        assert!(!text.contains(canary), "sensitive value leaked: {text}");
    }
}
