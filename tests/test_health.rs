//! 集成测试：`health`。

#[macro_use]
mod common;

use actix_web::test;
use aster_drive::db::repository::policy_repo;
use aster_drive::entities::storage_policy;
use aster_drive::runtime::SharedRuntimeState;
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
    assert_eq!(body["status"], "ok");
    assert!(
        body.get("version").is_none(),
        "public health leaked version"
    );
    assert!(
        body.get("build_time").is_none(),
        "public health leaked build time"
    );
    assert!(
        body.get("data").is_none(),
        "public health should use a minimal probe response"
    );
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
    assert!(
        body["data"].get("version").is_none(),
        "readiness leaked version"
    );
    assert!(
        body["data"].get("build_time").is_none(),
        "readiness leaked build time"
    );
}

#[actix_web::test]
async fn test_health_ready_redacts_database_error() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);

    db.close_by_ref().await.unwrap();

    let req = test::TestRequest::get().uri("/health/ready").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 503);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "database.error");
    assert_eq!(body["msg"], "Database unavailable");
    assert!(
        body.get("error").is_some(),
        "database readiness errors should keep generic retry metadata"
    );
}

#[actix_web::test]
async fn test_health_ready_returns_503_when_default_storage_is_unavailable() {
    let state = common::setup().await;
    let default_policy = policy_repo::find_default(state.writer_db())
        .await
        .unwrap()
        .expect("default policy should exist");
    let blocked_base_path = std::path::Path::new(&default_policy.base_path).join("not-a-dir");
    std::fs::write(&blocked_base_path, b"block local driver parent dir").unwrap();

    let mut active: storage_policy::ActiveModel = default_policy.clone().into();
    active.base_path = Set(blocked_base_path.to_string_lossy().into_owned());
    active.updated_at = Set(Utc::now());
    active.update(state.writer_db()).await.unwrap();

    state.driver_registry.invalidate(default_policy.id);
    state
        .policy_snapshot
        .reload(state.writer_db())
        .await
        .unwrap();

    let app = create_test_app!(state);

    let req = test::TestRequest::get().uri("/health/ready").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 503);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["msg"], "Storage unavailable");
    assert!(
        body.get("error").is_none(),
        "storage readiness errors must not expose driver details"
    );
    // Local filesystem probe failures can classify into different storage codes
    // depending on the OS errno, but they should still use the storage namespace.
    assert!(
        body["code"]
            .as_str()
            .is_some_and(|code| code.starts_with("storage."))
    );
}

#[actix_web::test]
async fn test_health_ready_returns_503_when_default_storage_policy_is_missing() {
    let state = common::setup().await;
    let default_policy = policy_repo::find_default(state.writer_db())
        .await
        .unwrap()
        .expect("default policy should exist");

    let mut active: storage_policy::ActiveModel = default_policy.clone().into();
    active.is_default = Set(false);
    active.updated_at = Set(Utc::now());
    active.update(state.writer_db()).await.unwrap();

    state.driver_registry.invalidate(default_policy.id);
    state
        .policy_snapshot
        .reload(state.writer_db())
        .await
        .unwrap();

    let app = create_test_app!(state);

    let req = test::TestRequest::get().uri("/health/ready").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 503);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "storage.policy_not_found");
    assert_eq!(body["msg"], "Storage unavailable");
    assert!(
        body.get("error").is_none(),
        "storage readiness errors must not expose policy or driver details"
    );
}

#[actix_web::test]
async fn test_health_ready_does_not_probe_s3_network() {
    let state = common::setup().await;
    let default_policy = policy_repo::find_default(state.writer_db())
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
    active.update(state.writer_db()).await.unwrap();

    state.driver_registry.invalidate(default_policy.id);
    state
        .policy_snapshot
        .reload(state.writer_db())
        .await
        .unwrap();

    let app = create_test_app!(state);

    let req = test::TestRequest::get().uri("/health/ready").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "ready");
}

#[actix_web::test]
async fn test_admin_system_info_exposes_build_metadata_only_after_auth() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/system-info")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["version"], env!("CARGO_PKG_VERSION"));
    assert!(body["data"]["build_time"].as_str().is_some());
}

#[actix_web::test]
async fn test_admin_system_info_requires_auth() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/system-info")
        .to_request();
    assert_service_status!(app, req, 401);
}
