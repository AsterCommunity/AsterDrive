//! 集成测试：`health`。

#[macro_use]
mod common;

use actix_web::test;
use aster_drive::api::error_code::ErrorCode;
use aster_drive::db::repository::policy_repo;
use aster_drive::entities::storage_policy;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, Set};
use serde_json::Value;

#[actix_web::test]
async fn test_health() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let req = test::TestRequest::get().uri("/health").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "ok");
}

#[actix_web::test]
async fn test_health_ready() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let req = test::TestRequest::get().uri("/health/ready").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "ready");
}

#[actix_web::test]
async fn test_health_ready_redacts_database_error() {
    let state = common::setup().await;
    let db = state.db.clone();
    let app = create_test_app!(state);

    db.close_by_ref().await.unwrap();

    let req = test::TestRequest::get().uri("/health/ready").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 503);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        serde_json::json!(ErrorCode::DatabaseError as i32)
    );
    assert_eq!(body["msg"], "Database unavailable");
}

#[actix_web::test]
async fn test_health_ready_returns_503_when_default_storage_is_unavailable() {
    let state = common::setup().await;
    let default_policy = policy_repo::find_default(&state.db)
        .await
        .unwrap()
        .expect("default policy should exist");
    let blocked_base_path = std::path::Path::new(&default_policy.base_path).join("not-a-dir");
    std::fs::write(&blocked_base_path, b"block local driver parent dir").unwrap();

    let mut active: storage_policy::ActiveModel = default_policy.clone().into();
    active.base_path = Set(blocked_base_path.to_string_lossy().into_owned());
    active.updated_at = Set(Utc::now());
    active.update(&state.db).await.unwrap();

    state.driver_registry.invalidate(default_policy.id);
    state.policy_snapshot.reload(&state.db).await.unwrap();

    let app = create_test_app!(state);

    let req = test::TestRequest::get().uri("/health/ready").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 503);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["msg"], "Storage unavailable");
    // Local filesystem probe failures can classify into different storage subcodes
    // depending on the OS errno, but they should always redact to E031 here.
    assert_eq!(body["error"]["internal_code"], "E031");
}

#[actix_web::test]
async fn test_health_ready_returns_503_when_default_storage_policy_is_missing() {
    let state = common::setup().await;
    let default_policy = policy_repo::find_default(&state.db)
        .await
        .unwrap()
        .expect("default policy should exist");

    let mut active: storage_policy::ActiveModel = default_policy.clone().into();
    active.is_default = Set(false);
    active.updated_at = Set(Utc::now());
    active.update(&state.db).await.unwrap();

    state.driver_registry.invalidate(default_policy.id);
    state.policy_snapshot.reload(&state.db).await.unwrap();

    let app = create_test_app!(state);

    let req = test::TestRequest::get().uri("/health/ready").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 503);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        serde_json::json!(ErrorCode::StoragePolicyNotFound as i32)
    );
    assert_eq!(body["msg"], "Storage unavailable");
}

#[actix_web::test]
async fn test_health_ready_does_not_probe_s3_network() {
    let state = common::setup().await;
    let default_policy = policy_repo::find_default(&state.db)
        .await
        .unwrap()
        .expect("default policy should exist");

    let mut active: storage_policy::ActiveModel = default_policy.clone().into();
    active.driver_type = Set(aster_drive::types::DriverType::S3);
    active.endpoint = Set("http://127.0.0.1:9".to_string());
    active.bucket = Set("ready-probe".to_string());
    active.access_key = Set("test-access-key".to_string());
    active.secret_key = Set("test-secret-key".to_string());
    active.options = Set(aster_drive::types::StoredStoragePolicyOptions(
        r#"{"s3_connect_timeout_secs":1,"s3_read_timeout_secs":1,"s3_operation_timeout_secs":1}"#
            .to_string(),
    ));
    active.updated_at = Set(Utc::now());
    active.update(&state.db).await.unwrap();

    state.driver_registry.invalidate(default_policy.id);
    state.policy_snapshot.reload(&state.db).await.unwrap();

    let app = create_test_app!(state);

    let req = test::TestRequest::get().uri("/health/ready").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "ready");
}
