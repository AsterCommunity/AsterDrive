use std::sync::{Arc, Mutex};

use actix_web::{App, HttpRequest, HttpResponse, HttpServer, test, web};
use aster_drive::config::site_url;
use aster_drive::runtime::SharedRuntimeState;
use aster_forge_db::system_config;
use chrono::Utc;
use serde_json::{Value, json};

use crate::common;

fn configure_origins(state: &impl SharedRuntimeState, value: &str) {
    state.runtime_config().apply(system_config::Model {
        id: 1,
        key: site_url::PUBLIC_SITE_URL_KEY.into(),
        value: value.into(),
        value_type: aster_forge_config::ConfigValueType::String,
        requires_restart: false,
        is_sensitive: false,
        source: aster_forge_config::ConfigSource::System,
        visibility: aster_forge_config::ConfigVisibility::Private,
        namespace: String::new(),
        category: "site".into(),
        description: "test".into(),
        updated_at: Utc::now(),
        updated_by: None,
    });
}

fn connection(endpoint: &str) -> Value {
    json!({
        "connector_config": {
            "format_version": 1, "connector_id": "asterdrive.storage.huawei_obs", "schema_version": 1,
            "values": {"endpoint": endpoint, "bucket": "archive", "obs_use_cname": true, "base_path": "test", "object_storage_upload_strategy": "presigned", "object_storage_download_strategy": "presigned"}
        }, "behavior": {}, "credential": {"mode": "static", "values": {"obs_access_key_id": "account", "obs_secret_access_key": "test-secret"}}
    })
}

#[derive(Default)]
struct ServiceState {
    properties: String,
    requests: Vec<(String, String)>,
    writes: usize,
    object_probes: usize,
}

struct ObsService {
    endpoint: String,
    state: Arc<Mutex<ServiceState>>,
    handle: actix_web::dev::ServerHandle,
    thread: std::thread::JoinHandle<()>,
}

impl ObsService {
    fn start() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(ServiceState {
            properties: "<CORSConfiguration/>".into(),
            ..Default::default()
        }));
        let server_state = state.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            actix_web::rt::System::new().block_on(async move {
                let server = HttpServer::new(move || {
                    App::new()
                        .app_data(web::Data::from(server_state.clone()))
                        .default_service(web::to(
                            |request: HttpRequest,
                             body: web::Bytes,
                             state: web::Data<Mutex<ServiceState>>| async move {
                                if request.query_string() != "cors" {
                                    state.lock().unwrap().object_probes += 1;
                                    return HttpResponse::NotFound().finish();
                                }
                                assert_eq!(request.uri().to_string(), "/?cors");
                                assert_eq!(
                                    request
                                        .headers()
                                        .get("user-agent")
                                        .unwrap()
                                        .to_str()
                                        .unwrap(),
                                    aster_drive_outbound::OUTBOUND_HTTP_USER_AGENT
                                );
                                let mut state = state.lock().unwrap();
                                state.requests.push((
                                    request.method().to_string(),
                                    request
                                        .headers()
                                        .get("authorization")
                                        .unwrap()
                                        .to_str()
                                        .unwrap()
                                        .to_owned(),
                                ));
                                if request.method() == "PUT" {
                                    state.properties = String::from_utf8(body.to_vec()).unwrap();
                                    state.writes += 1;
                                    HttpResponse::Accepted()
                                        .insert_header(("x-obs-request-id", "obs-api-write"))
                                        .finish()
                                } else {
                                    assert_eq!(request.method(), "GET");
                                    HttpResponse::Ok()
                                        .insert_header(("x-obs-request-id", "obs-api-read"))
                                        .body(state.properties.clone())
                                }
                            },
                        ))
                })
                .workers(1)
                .listen(listener)
                .unwrap()
                .run();
                sender.send(server.handle()).unwrap();
                server.await.unwrap();
            });
        });
        Self {
            endpoint,
            state,
            handle: receiver.recv().unwrap(),
            thread,
        }
    }

    async fn stop(self) {
        self.handle.stop(true).await;
        tokio::task::spawn_blocking(move || self.thread.join().unwrap())
            .await
            .unwrap();
    }
}

#[actix_web::test]
async fn obs_cors_saved_and_draft_execute_once_with_current_target_and_saved_credentials() {
    let service = ObsService::start();
    let new_service = ObsService::start();
    let state = common::setup().await;
    configure_origins(
        &state,
        r#"["https://DRIVE.example.test:443","https://drive.example.test","http://drive.example.test:80","http://drive.example.test","https://drive.example.test:8443"]"#,
    );
    let runtime = state.runtime_config().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let request = test::TestRequest::get()
        .uri("/api/v1/admin/policies/storage-drivers")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), 200);
    let catalog: Value = test::read_body_json(response).await;
    let catalog_text = serde_json::to_string(&catalog).unwrap();
    assert!(catalog_text.contains("configure_huawei_obs_cors"));
    assert!(!catalog_text.contains("supports_preview"));
    let descriptor = catalog["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|descriptor| descriptor["connector_id"] == "asterdrive.storage.huawei_obs")
        .unwrap();
    let action = descriptor["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["action_id"] == "configure_huawei_obs_cors")
        .unwrap();
    assert_eq!(action["requires_confirmation"], true);
    assert_eq!(action["mutates_remote_state"], true);
    assert!(action["fields"].is_null());
    assert_eq!(
        action["endpoints"],
        json!([
            "execute_draft_storage_policy_action",
            "execute_saved_storage_policy_action"
        ])
    );

    let request = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(json!({"name":"OBS CORS saved", "connection":connection(&service.endpoint)}))
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), 201);
    let created: Value = test::read_body_json(response).await;
    let id = created["data"]["id"].as_i64().unwrap();
    assert!(
        service.state.lock().unwrap().requests.is_empty(),
        "saving must not configure CORS"
    );

    // A deliberately unsuccessful ordinary object probe must never invoke CORS.
    for (uri, body) in [
        (format!("/api/v1/admin/policies/{id}/test"), json!({})),
        (
            "/api/v1/admin/policies/test".into(),
            json!({"connection":connection(&service.endpoint)}),
        ),
    ] {
        let request = test::TestRequest::post()
            .uri(&uri)
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(body)
            .to_request();
        let response = test::call_service(&app, request).await;
        assert!(!response.status().is_success());
        assert!(service.state.lock().unwrap().requests.is_empty());
    }
    assert!(service.state.lock().unwrap().object_probes > 0);

    let saved_uri = format!("/api/v1/admin/policies/{id}/action");
    let request = test::TestRequest::post()
        .uri(&saved_uri)
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(json!({"action_id":"configure_huawei_obs_cors", "values":{}}))
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), 200);
    let saved: Value = test::read_body_json(response).await;
    assert_eq!(saved["data"]["output"]["bucket"], "archive");
    assert_eq!(
        saved["data"]["output"]["origins"],
        json!([
            "https://drive.example.test",
            "http://drive.example.test",
            "https://drive.example.test:8443"
        ])
    );
    assert_eq!(saved["data"]["output"]["changed"], true);
    assert_eq!(saved["data"]["output"]["request_id"], "obs-api-write");
    assert!(saved["data"].get("confirmation").is_none());
    assert!(!saved.to_string().contains("test-secret"));
    assert_eq!(service.state.lock().unwrap().writes, 1);

    let request = test::TestRequest::post()
        .uri(&saved_uri)
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(json!({"action_id":"configure_huawei_obs_cors", "values":{}}))
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), 200);
    let repeated: Value = test::read_body_json(response).await;
    assert_eq!(repeated["data"]["output"]["changed"], false);
    assert_eq!(repeated["data"]["output"]["preserved_rule_count"], 1);
    assert_eq!(service.state.lock().unwrap().writes, 1);

    let mut model = runtime.get_model(site_url::PUBLIC_SITE_URL_KEY).unwrap();
    model.value = r#"["https://changed.example.test"]"#.into();
    runtime.apply(model);
    let mut draft = connection(&new_service.endpoint);
    draft["connector_config"]["values"]["bucket"] = json!("new-container");
    draft["credential"]["values"] = json!({});
    let request = test::TestRequest::post().uri("/api/v1/admin/policies/action")
        .insert_header(("Cookie", common::access_cookie_header(&token))).insert_header(common::csrf_header_for(&token))
        .set_json(json!({"action_id":"configure_huawei_obs_cors", "values":{}, "policy_id":id, "connection":draft})).to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), 200);
    let result: Value = test::read_body_json(response).await;
    assert_eq!(
        result["data"]["output"]["origins"],
        json!(["https://changed.example.test"])
    );
    assert_eq!(result["data"]["output"]["bucket"], "new-container");
    assert_eq!(
        service.state.lock().unwrap().writes,
        1,
        "draft must not write the old endpoint"
    );
    assert_eq!(new_service.state.lock().unwrap().writes, 1);
    assert!(
        new_service
            .state
            .lock()
            .unwrap()
            .requests
            .iter()
            .all(|(_, auth)| auth.starts_with("OBS account:"))
    );

    admin_create_user!(
        app,
        token,
        "obs_reader",
        "obs-cors-reader@example.test",
        "password123"
    );
    let login = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(json!({"identifier":"obs_reader", "password":"password123"}))
        .to_request();
    let response = test::call_service(&app, login).await;
    assert_eq!(response.status(), 200);
    let reader = common::extract_cookie(&response, "aster_access").unwrap();
    for (uri, body) in [
        (
            saved_uri.as_str(),
            json!({"action_id":"configure_huawei_obs_cors", "values":{}}),
        ),
        (
            "/api/v1/admin/policies/action",
            json!({"action_id":"configure_huawei_obs_cors", "connection":connection(&service.endpoint)}),
        ),
    ] {
        let request = test::TestRequest::post()
            .uri(uri)
            .insert_header(("Cookie", common::access_cookie_header(&reader)))
            .insert_header(common::csrf_header_for(&reader))
            .set_json(body)
            .to_request();
        assert_service_status!(app, request, 403);
    }
    assert_eq!(service.state.lock().unwrap().writes, 1);
    service.stop().await;
    new_service.stop().await;
}

#[actix_web::test]
async fn obs_cors_rejects_missing_and_invalid_origins_before_network() {
    let service = ObsService::start();
    let state = common::setup().await;
    let runtime = state.runtime_config().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    for value in [
        None,
        Some("[]"),
        Some(r#"["*"]"#),
        Some(r#"["https://valid.example.test","invalid"]"#),
    ] {
        if let Some(value) = value {
            runtime.apply(system_config::Model {
                id: 1,
                key: site_url::PUBLIC_SITE_URL_KEY.into(),
                value: value.into(),
                value_type: aster_forge_config::ConfigValueType::String,
                requires_restart: false,
                is_sensitive: false,
                source: aster_forge_config::ConfigSource::System,
                visibility: aster_forge_config::ConfigVisibility::Private,
                namespace: String::new(),
                category: "site".into(),
                description: "test".into(),
                updated_at: Utc::now(),
                updated_by: None,
            });
        }
        let request = test::TestRequest::post().uri("/api/v1/admin/policies/action")
            .insert_header(("Cookie", common::access_cookie_header(&token))).insert_header(common::csrf_header_for(&token))
            .set_json(json!({"action_id":"configure_huawei_obs_cors", "connection":connection(&service.endpoint)})).to_request();
        let response = test::call_service(&app, request).await;
        assert_eq!(response.status(), 400);
        match value {
            None | Some("[]") => {
                let body: Value = test::read_body_json(response).await;
                assert_eq!(body["code"], "policy.action_parameter_required");
            }
            _ => {}
        }
    }
    assert!(service.state.lock().unwrap().requests.is_empty());
    service.stop().await;
}
