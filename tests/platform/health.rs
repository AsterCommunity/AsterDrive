//! 集成测试：`health`。

use crate::common;

use actix_web::test;
use aster_drive::db::repository::policy_repo;
use aster_drive::services::storage_policy::policy;
use aster_drive_model::entities::storage_policy;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, Set};
use serde_json::Value;

#[actix_web::test]
async fn follower_readiness_tracks_each_enabled_binding_and_setup() {
    use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
    use aster_drive::services::remote::{binding_control, master_binding};
    use aster_drive_model::types::ResolvedRemoteTransport;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    struct ReadinessProbe(Arc<AtomicBool>);
    impl aster_forge_metrics::DbMetricsRecorder for ReadinessProbe {
        fn enabled(&self) -> bool {
            true
        }
        fn record_db_query(&self, _: &aster_forge_metrics::DbQueryMetric) {}
    }
    impl aster_forge_metrics::MetricsRecorder for ReadinessProbe {
        fn role_readiness(&self, ready: bool) {
            self.0.store(ready, Ordering::SeqCst);
        }
    }
    impl aster_drive_metrics::MetricsRecorder for ReadinessProbe {
        fn forge_recorder(&self) -> aster_forge_metrics::SharedMetricsRecorder {
            Arc::new(Self(self.0.clone()))
        }
    }
    let ready_metric = Arc::new(AtomicBool::new(false));
    let mut state = common::setup().await.follower_view();
    state.metrics = Arc::new(ReadinessProbe(ready_metric.clone()));
    let app = test::init_service(App::new().app_data(web::Data::new(state.clone())).route(
        "/health/ready",
        web::get().to(aster_drive::api::routes::health::follower_ready),
    ))
    .await;
    let response = test::call_service(
        &app,
        test::TestRequest::get().uri("/health/ready").to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["data"]["status"], "needs_binding");
    assert!(ready_metric.load(Ordering::SeqCst));

    let reachable = Arc::new(AtomicBool::new(true));
    let server_reachable = reachable.clone();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = HttpServer::new(move || {
        let reachable = server_reachable.clone();
        App::new().route(
            "/api/v1/internal/remote-node-control/binding-state",
            web::get().to(move |req: HttpRequest| {
                let reachable = reachable.clone();
                async move {
                    if !reachable.load(Ordering::SeqCst) {
                        return HttpResponse::BadGateway().finish();
                    }
                    let reverse = req
                        .headers()
                        .get("x-aster-access-key")
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .ends_with("reverse_tunnel");
                    HttpResponse::Ok().json(aster_drive::api::response::success(
                        aster_drive::storage::remote_protocol::RemoteBindingDesiredState {
                            name: "master".to_string(),
                            is_enabled: true,
                            resolved_transport: if reverse {
                                ResolvedRemoteTransport::ReverseTunnel
                            } else {
                                ResolvedRemoteTransport::Direct
                            },
                            desired_revision: 1,
                        },
                    ))
                }
            }),
        )
    })
    .listen(listener)
    .unwrap()
    .run();
    let handle = server.handle();
    let server_task = tokio::spawn(server);
    let client =
        aster_drive_outbound::ClientConfig::new(aster_drive_outbound::Profile::InternalControl)
            .build()
            .unwrap();
    let mut ids = Vec::new();
    for transport in [
        ResolvedRemoteTransport::Direct,
        ResolvedRemoteTransport::ReverseTunnel,
    ] {
        let (binding, _) = master_binding::upsert_from_enrollment(
            state.writer_db(),
            master_binding::UpsertMasterBindingInput {
                name: "master".into(),
                master_url: url.clone(),
                access_key: format!("readiness-{}", transport.as_str()),
                secret_key: "test-secret".into(),
                is_enabled: true,
            },
        )
        .await
        .unwrap();
        ids.push(binding.id);
        let response = test::call_service(
            &app,
            test::TestRequest::get().uri("/health/ready").to_request(),
        )
        .await;
        assert_eq!(
            response.status(),
            503,
            "an unobserved enabled binding is unavailable"
        );
        assert!(!ready_metric.load(Ordering::SeqCst));
        assert!(binding_control::reconcile_all(&state, &client).await);
        let response = test::call_service(
            &app,
            test::TestRequest::get().uri("/health/ready").to_request(),
        )
        .await;
        assert_eq!(response.status(), 200);
        let body: Value = test::read_body_json(response).await;
        assert_eq!(body["data"]["status"], "ready");
        assert!(ready_metric.load(Ordering::SeqCst));
    }
    reachable.store(false, Ordering::SeqCst);
    assert!(!binding_control::reconcile_all(&state, &client).await);
    assert_eq!(
        test::call_service(
            &app,
            test::TestRequest::get().uri("/health/ready").to_request()
        )
        .await
        .status(),
        200,
        "transient failures retain recent success"
    );
    // Simulate loss of per-binding evidence without wall-clock sleeps.
    state
        .binding_health
        .retain_bindings(&[ids[0]].into_iter().collect());
    assert_eq!(
        test::call_service(
            &app,
            test::TestRequest::get().uri("/health/ready").to_request()
        )
        .await
        .status(),
        503,
        "one healthy binding cannot mask another unavailable master"
    );
    for id in ids {
        let mut binding: aster_drive_model::entities::master_binding::ActiveModel =
            aster_drive::db::repository::master_binding_repo::find_by_id(state.writer_db(), id)
                .await
                .unwrap()
                .into();
        binding.is_enabled = Set(false);
        binding.update(state.writer_db()).await.unwrap();
    }
    let response = test::call_service(
        &app,
        test::TestRequest::get().uri("/health/ready").to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["data"]["status"], "needs_binding");
    handle.stop(true).await;
    server_task.await.unwrap().unwrap();
}

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
    common::create_test_account(
        &state,
        "healthadmin",
        "health-admin@example.com",
        "password123",
    )
    .await
    .expect("health readiness setup should complete");
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
    common::create_test_account(
        &state,
        "healthadmin",
        "health-admin@example.com",
        "password123",
    )
    .await
    .expect("health readiness setup should complete");
    let default_policy = policy_repo::find_default(state.writer_db())
        .await
        .unwrap()
        .expect("default policy should exist");
    let blocked_base_path =
        std::path::Path::new(&common::local_policy_base_path(&default_policy)).join("not-a-dir");
    std::fs::write(&blocked_base_path, b"block local driver parent dir").unwrap();

    let mut active: storage_policy::ActiveModel = default_policy.clone().into();
    active.storage_config = Set(common::with_local_policy_base_path(
        &default_policy,
        blocked_base_path.to_string_lossy().into_owned(),
    ));
    active.updated_at = Set(Utc::now());
    active.update(state.writer_db()).await.unwrap();

    state.driver_registry.invalidate(default_policy.id);
    state
        .driver_registry
        .reload_policy_snapshot(&state.policy_snapshot, state.writer_db())
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
async fn test_health_ready_returns_needs_storage_when_default_policy_is_missing() {
    let state = common::setup().await;
    common::create_test_account(
        &state,
        "healthadmin",
        "health-admin@example.com",
        "password123",
    )
    .await
    .expect("health readiness setup should complete");
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
        .driver_registry
        .reload_policy_snapshot(&state.policy_snapshot, state.writer_db())
        .await
        .unwrap();

    let app = create_test_app!(state);

    let req = test::TestRequest::get().uri("/health/ready").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "needs_storage");
}

#[actix_web::test]
async fn test_health_ready_does_not_probe_s3_network() {
    let state = common::setup().await;
    common::create_test_account(
        &state,
        "healthadmin",
        "health-admin@example.com",
        "password123",
    )
    .await
    .expect("health readiness setup should complete");
    policy::create(
        &state,
        policy::CreateStoragePolicyInput {
            name: "Readiness S3".to_string(),
            connection: common::s3_connection(
                "http://127.0.0.1:9",
                "ready-probe",
                "health-ready",
                "test-access-key",
                "test-secret-key",
            ),
            max_file_size: 0,
            chunk_size: None,
            is_default: true,
            allowed_types: None,
        },
    )
    .await
    .expect("S3 readiness fixture should be created through the connector service");

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
