use super::*;
use aster_forge_metrics::MetricsProvider;
use aster_forge_tasks::ClaimedTaskExecutionStore;

async fn state_with_metrics() -> (crate::runtime::PrimaryAppState, MetricsProvider) {
    let mut state = build_dispatch_test_state().await;
    let provider = MetricsProvider::builder()
        .with_prometheus()
        .unwrap()
        .build()
        .unwrap();
    state.metrics = aster_drive_metrics::create_metrics_recorder(provider.clone()).unwrap();
    (state, provider)
}

async fn sample(provider: &MetricsProvider, name: &str, labels: &[&str]) -> f64 {
    let output = provider
        .export_prometheus(Duration::from_secs(1))
        .await
        .unwrap();
    output
        .lines()
        .find(|line| {
            line.strip_prefix(name)
                .is_some_and(|rest| rest.starts_with('{') || rest.starts_with(' '))
                && labels.iter().all(|label| line.contains(label))
        })
        .unwrap_or_else(|| panic!("missing {name} {labels:?}: {output}"))
        .rsplit_once(' ')
        .unwrap()
        .1
        .parse()
        .unwrap()
}

async fn claimed_empty_trash_task(
    state: &crate::runtime::PrimaryAppState,
) -> (background_task::Model, TaskLease) {
    let now = Utc::now();
    let user = user::ActiveModel {
        username: Set("handler-metrics".into()),
        email: Set("handler-metrics@example.test".into()),
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
    let info = crate::services::task::trash::create_trash_purge_all_task_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id: user.id },
    )
    .await
    .unwrap();
    let mut claimed = claim_due_for_lane(state, test_lane_config(TaskLane::Fallback, 1, false))
        .await
        .unwrap();
    let (task, lease) = claimed.pop().unwrap();
    assert_eq!(task.id, info.id);
    (task, lease)
}

#[tokio::test]
async fn handler_return_metrics_preserve_success_and_error_results() {
    let (state, provider) = state_with_metrics().await;
    let (task, lease) = claimed_empty_trash_task(&state).await;
    let store = BackgroundTaskExecutionStore::new(state.clone());
    store
        .process_task(
            &task,
            TaskExecutionContext::new(lease, Duration::from_secs(60), CancellationToken::new()),
        )
        .await
        .unwrap();
    let stored = background_task_repo::find_by_id(state.writer_db(), task.id)
        .await
        .unwrap();
    assert_eq!(stored.status, BackgroundTaskStatus::Succeeded);
    assert_eq!(
        sample(
            &provider,
            "aster_task_handler_active",
            &["kind=\"trash_purge_all\""]
        )
        .await,
        0.0
    );
    assert_eq!(
        sample(
            &provider,
            "aster_task_handler_duration_seconds_count",
            &["kind=\"trash_purge_all\"", "result=\"returned_ok\""]
        )
        .await,
        1.0
    );

    let error_task = insert_processing_system_runtime_task(state.writer_db()).await;
    let error = store
        .process_task(
            &error_task,
            TaskExecutionContext::new(
                TaskLease::new(error_task.id, error_task.processing_token),
                Duration::from_secs(60),
                CancellationToken::new(),
            ),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, AsterError::InternalError(_)));
    assert_eq!(
        sample(
            &provider,
            "aster_task_handler_active",
            &["kind=\"system_runtime\""]
        )
        .await,
        0.0
    );
    assert_eq!(
        sample(
            &provider,
            "aster_task_handler_duration_seconds_count",
            &["kind=\"system_runtime\"", "result=\"returned_error\""]
        )
        .await,
        1.0
    );
    // A handler return is not a persisted terminal transition.
    assert_eq!(
        background_task_repo::find_by_id(state.writer_db(), error_task.id)
            .await
            .unwrap()
            .status,
        BackgroundTaskStatus::Processing
    );
    provider.shutdown(Duration::from_secs(1)).await.unwrap();
}

#[tokio::test]
async fn dropped_handler_future_releases_active_once_and_records_no_return() {
    use sea_orm::TransactionTrait;
    let (state, provider) = state_with_metrics().await;
    let (task, lease) = claimed_empty_trash_task(&state).await;
    let transaction = state.writer_db().begin().await.unwrap();
    let store = BackgroundTaskExecutionStore::new(state.clone());
    let mut handler = store.process_task(
        &task,
        TaskExecutionContext::new(lease, Duration::from_secs(60), CancellationToken::new()),
    );
    // The one-connection writer is held. Poll through the guard creation until
    // the real handler waits on its first database write, then cancel by drop.
    assert!(futures::poll!(handler.as_mut()).is_pending());
    assert_eq!(
        sample(
            &provider,
            "aster_task_handler_active",
            &["kind=\"trash_purge_all\""]
        )
        .await,
        1.0
    );
    drop(handler);
    assert_eq!(
        sample(
            &provider,
            "aster_task_handler_active",
            &["kind=\"trash_purge_all\""]
        )
        .await,
        0.0
    );
    assert_eq!(
        sample(
            &provider,
            "aster_task_handler_duration_seconds_count",
            &["kind=\"trash_purge_all\"", "result=\"dropped\""]
        )
        .await,
        1.0
    );
    let raw = provider
        .export_prometheus(Duration::from_secs(1))
        .await
        .unwrap();
    assert!(!raw.contains("result=\"returned_ok\""));
    assert!(!raw.contains("result=\"returned_error\""));
    transaction.rollback().await.unwrap();
    assert_eq!(
        background_task_repo::find_by_id(state.writer_db(), task.id)
            .await
            .unwrap()
            .status,
        BackgroundTaskStatus::Processing
    );
    provider.shutdown(Duration::from_secs(1)).await.unwrap();
}

#[tokio::test]
async fn pending_snapshot_marks_failure_without_overwriting_last_good_and_recovers_to_zero() {
    let (mut state, provider) = state_with_metrics().await;
    for status in [
        BackgroundTaskStatus::Pending,
        BackgroundTaskStatus::Retry,
        BackgroundTaskStatus::Processing,
    ] {
        insert_dispatch_test_task(
            state.writer_db(),
            BackgroundTaskKind::SystemRuntime,
            status,
            0,
            None,
        )
        .await;
    }
    super::super::refresh_pending_metric(&state).await;
    assert_eq!(sample(&provider, "aster_task_pending", &[]).await, 2.0);
    assert_eq!(
        sample(
            &provider,
            "aster_drive_observability_snapshot_valid",
            &["source=\"tasks\""]
        )
        .await,
        1.0
    );
    let last_good = sample(
        &provider,
        "aster_drive_observability_snapshot_last_success_timestamp_seconds",
        &["source=\"tasks\""],
    )
    .await;
    assert!(last_good > 0.0);
    let handles = state.db_handles.clone();
    let failed_db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
    failed_db.get_sqlite_connection_pool().close().await;
    state.db_handles = aster_forge_db::DbHandles::single(failed_db);
    super::super::refresh_pending_metric(&state).await;
    assert_eq!(sample(&provider, "aster_task_pending", &[]).await, 2.0);
    assert_eq!(
        sample(
            &provider,
            "aster_drive_observability_snapshot_valid",
            &["source=\"tasks\""]
        )
        .await,
        0.0
    );
    assert_eq!(
        sample(
            &provider,
            "aster_drive_observability_snapshot_last_success_timestamp_seconds",
            &["source=\"tasks\""]
        )
        .await,
        last_good
    );
    state.db_handles = handles;
    background_task::Entity::delete_many()
        .exec(state.writer_db())
        .await
        .unwrap();
    super::super::refresh_pending_metric(&state).await;
    assert_eq!(sample(&provider, "aster_task_pending", &[]).await, 0.0);
    assert_eq!(
        sample(
            &provider,
            "aster_drive_observability_snapshot_valid",
            &["source=\"tasks\""]
        )
        .await,
        1.0
    );
    provider.shutdown(Duration::from_secs(1)).await.unwrap();
}
