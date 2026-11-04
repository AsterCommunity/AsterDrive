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
            "format_version": 1, "connector_id": "asterdrive.storage.azure_blob", "schema_version": 1,
            "values": {"endpoint": endpoint, "bucket": "archive", "base_path": "test", "object_storage_upload_strategy": "presigned", "object_storage_download_strategy": "presigned"}
        }, "behavior": {}, "credential": {"mode": "static", "values": {"azure_blob_account_name": "account", "azure_blob_account_key": "c2VjcmV0"}}
    })
}

#[derive(Default)]
struct ServiceState {
    properties: String,
    requests: Vec<(String, String)>,
    writes: usize,
}

struct BlobService {
    endpoint: String,
    state: Arc<Mutex<ServiceState>>,
    handle: actix_web::dev::ServerHandle,
    thread: std::thread::JoinHandle<()>,
}

impl BlobService {
    fn start() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/account", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(ServiceState {
            properties: "<StorageServiceProperties><Cors/></StorageServiceProperties>".into(),
            ..Default::default()
        }));
        let server_state = state.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        let thread =
            std::thread::spawn(move || {
                actix_web::rt::System::new().block_on(async move {
                    let server =
                        HttpServer::new(move || {
                            App::new()
                        .app_data(web::Data::from(server_state.clone()))
                        .default_service(web::to(
                            |request: HttpRequest,
                             body: web::Bytes,
                             state: web::Data<Mutex<ServiceState>>| async move {
                                assert_eq!(
                                    request.uri().to_string(),
                                    "/account/?restype=service&comp=properties"
                                );
                                let mut state = state.lock().unwrap();
                                state.requests.push((
                                    request.method().to_string(),
                                    request.headers().get("authorization").unwrap()
                                        .to_str()
                                        .unwrap()
                                        .to_owned(),
                                ));
                                if request.method() == "PUT" {
                                    state.properties = String::from_utf8(body.to_vec()).unwrap();
                                    state.writes += 1;
                                    HttpResponse::Accepted()
                                        .insert_header(("x-ms-request-id", "azure-api-write"))
                                        .finish()
                                } else {
                                    assert_eq!(request.method(), "GET");
                                    HttpResponse::Ok()
                                        .insert_header(("x-ms-request-id", "azure-api-read"))
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
async fn azure_cors_saved_and_draft_execute_once_with_current_target_and_saved_credentials() {
    let service = BlobService::start();
    let new_service = BlobService::start();
    let state = common::setup().await;
    configure_origins(&state, r#"["https://DRIVE.example.test:443"]"#);
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
    assert!(catalog_text.contains("configure_azure_blob_cors"));
    assert!(!catalog_text.contains("supports_preview"));

    let request = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(json!({"name":"Azure CORS saved", "connection":connection(&service.endpoint)}))
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), 201);
    let created: Value = test::read_body_json(response).await;
    let id = created["data"]["id"].as_i64().unwrap();
    assert!(
        service.state.lock().unwrap().requests.is_empty(),
        "saving must not configure CORS"
    );

    let saved_uri = format!("/api/v1/admin/policies/{id}/action");
    let request = test::TestRequest::post()
        .uri(&saved_uri)
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(json!({"action_id":"configure_azure_blob_cors", "values":{}}))
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), 200);
    let saved: Value = test::read_body_json(response).await;
    assert_eq!(saved["data"]["output"]["account"], "account");
    assert_eq!(
        saved["data"]["output"]["allowed_origins"],
        json!(["https://drive.example.test:443"])
    );
    assert_eq!(saved["data"]["output"]["changed"], true);
    assert_eq!(saved["data"]["output"]["request_id"], "azure-api-write");
    assert!(saved["data"].get("confirmation").is_none());
    assert!(!saved.to_string().contains("c2VjcmV0"));
    assert_eq!(service.state.lock().unwrap().writes, 1);

    let mut model = runtime.get_model(site_url::PUBLIC_SITE_URL_KEY).unwrap();
    model.value = r#"["https://changed.example.test"]"#.into();
    runtime.apply(model);
    let mut draft = connection(&new_service.endpoint);
    draft["connector_config"]["values"]["bucket"] = json!("new-container");
    draft["credential"]["values"] = json!({});
    let request = test::TestRequest::post().uri("/api/v1/admin/policies/action")
        .insert_header(("Cookie", common::access_cookie_header(&token))).insert_header(common::csrf_header_for(&token))
        .set_json(json!({"action_id":"configure_azure_blob_cors", "values":{}, "policy_id":id, "connection":draft})).to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), 200);
    let result: Value = test::read_body_json(response).await;
    assert_eq!(
        result["data"]["output"]["allowed_origins"],
        json!(["https://changed.example.test"])
    );
    assert_eq!(
        result["data"]["output"]["service_endpoint"],
        format!("{}/", new_service.endpoint)
    );
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
            .all(|(_, auth)| auth.starts_with("SharedKey account:"))
    );

    admin_create_user!(
        app,
        token,
        "azure_reader",
        "azure-cors-reader@example.test",
        "password123"
    );
    let login = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(json!({"identifier":"azure_reader", "password":"password123"}))
        .to_request();
    let response = test::call_service(&app, login).await;
    assert_eq!(response.status(), 200);
    let reader = common::extract_cookie(&response, "aster_access").unwrap();
    for (uri, body) in [
        (
            saved_uri.as_str(),
            json!({"action_id":"configure_azure_blob_cors", "values":{}}),
        ),
        (
            "/api/v1/admin/policies/action",
            json!({"action_id":"configure_azure_blob_cors", "connection":connection(&service.endpoint)}),
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
async fn azure_cors_rejects_missing_and_invalid_origins_before_network() {
    let service = BlobService::start();
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
            .set_json(json!({"action_id":"configure_azure_blob_cors", "connection":connection(&service.endpoint)})).to_request();
        assert_eq!(test::call_service(&app, request).await.status(), 400);
    }
    assert!(service.state.lock().unwrap().requests.is_empty());
    service.stop().await;
}
