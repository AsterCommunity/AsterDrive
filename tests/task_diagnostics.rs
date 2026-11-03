//! Keep diagnostic capture isolated from unrelated concurrent integration tests.
#[macro_use]
#[path = "common/mod.rs"]
mod common;

use actix_web::test;
use aster_drive::db::repository::{background_task_repo, config_repo};
use aster_drive::services::task;
use aster_drive_model::entities::background_task;
use aster_drive_model::types::BackgroundTaskStatus;
use aster_forge_test::logs::CapturedLogs;
use sea_orm::{ActiveModelTrait, EntityTrait, NotSet, Set};
use serde_json::Value;

#[actix_web::test]
async fn claim_diagnostics_distinguish_quota_from_empty_queue_and_keep_transaction_context() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let request = test::TestRequest::delete()
        .uri("/api/v1/trash")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), 200);
    let response: Value = test::read_body_json(response).await;
    let task_id = response["data"]["id"].as_i64().unwrap();
    let pending = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .unwrap();
    let mut occupied: background_task::ActiveModel = pending.clone().into();
    occupied.id = NotSet;
    occupied.display_name = Set("SECRET-task-title-canary".into());
    occupied.status = Set(BackgroundTaskStatus::Processing);
    occupied.lease_expires_at = Set(Some(chrono::Utc::now() + chrono::Duration::minutes(1)));
    occupied.processing_token = Set(1);
    let occupied = occupied.insert(state.writer_db()).await.unwrap();
    config_repo::upsert_with_actor(
        state.writer_db(),
        "task_fallback_cluster_concurrency",
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
    let logs = CapturedLogs::default();
    logs.capture_future(async {
        let blocked = task::dispatch_due(&state).await.unwrap();
        assert_eq!(blocked.claimed, 0);
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
        let completed = task::dispatch_due(&state).await.unwrap();
        assert_eq!(completed.succeeded, 1);
        assert_eq!(task::dispatch_due(&state).await.unwrap().claimed, 0);
        let mut retryable: background_task::ActiveModel = pending.clone().into();
        retryable.id = NotSet;
        let retryable = retryable.insert(state.writer_db()).await.unwrap();
        config_repo::upsert_with_actor(
            state.writer_db(),
            "task_fallback_cluster_concurrency",
            "SECRET-quota-canary",
            None,
        )
        .await
        .unwrap();
        assert!(task::dispatch_due(&state).await.is_err());
        let untouched = background_task_repo::find_by_id(state.writer_db(), retryable.id)
            .await
            .unwrap();
        assert_eq!(untouched.status, BackgroundTaskStatus::Pending);
        assert_eq!(untouched.processing_token, 0);
    })
    .await;
    let text = logs.text();
    for field in [
        "operation=\"background_task_claim\"",
        "transaction_id=",
        "caller_file=",
        "claim.rs",
        "phase=\"quota_lock\"",
        "phase=\"selection\"",
        "phase=\"cas\"",
        "shared_quota_full",
        "no_candidates",
        "elapsed_seconds=",
    ] {
        assert!(text.contains(field), "missing {field}: {text}");
    }
    assert!(!text.contains("SECRET-task-title-canary"));
    assert!(!text.contains("SECRET-quota-canary"));
    assert_eq!(
        background_task_repo::find_by_id(state.writer_db(), task_id)
            .await
            .unwrap()
            .status,
        BackgroundTaskStatus::Succeeded
    );
}
