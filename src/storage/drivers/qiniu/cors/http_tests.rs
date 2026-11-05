use super::*;
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct State {
    rules: Vec<CorsRule>,
    get_status: u16,
    put_status: u16,
    invalid_body: Option<Vec<u8>>,
    requests: Vec<(String, String)>,
    writes: usize,
    corrupt_readback: bool,
    readback_status: u16,
}

struct Fixture {
    driver: QiniuDriver,
    state: Arc<Mutex<State>>,
    handle: actix_web::dev::ServerHandle,
}

impl Fixture {
    async fn new(state: State) -> Self {
        let state = Arc::new(Mutex::new(state));
        let captured = state.clone();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = HttpServer::new(move || {
            let captured = captured.clone();
            App::new().default_service(web::to(move |req: HttpRequest, body: web::Bytes| {
                let captured = captured.clone();
                async move {
                    let mut state = captured.lock().unwrap();
                    let method = req.method().as_str();
                    let operation = match method { "GET" => "get", "POST" => "set", _ => panic!("unexpected method") };
                    assert_eq!(req.path(), format!("/corsRules/{operation}/archive"));
                    assert!(req.query_string().is_empty());
                    let content_type = req.headers().get("content-type").unwrap().to_str().unwrap();
                    assert_eq!(content_type, if method == "POST" {"application/json"} else {"application/x-www-form-urlencoded"});
                    let date = req.headers().get("x-qiniu-date").unwrap().to_str().unwrap();
                    let canonical = format!("{method} {}\nHost: {}\nContent-Type: {content_type}\nX-Qiniu-Date: {date}\n\n{}", req.path(), req.connection_info().host(), std::str::from_utf8(&body).unwrap());
                    let driver = super::super::tests::driver();
                    assert_eq!(req.headers().get("authorization").unwrap().to_str().unwrap(), format!("Qiniu {}", driver.sign(canonical.as_bytes()).unwrap()));
                    state.requests.push((method.to_string(), req.path().to_string()));
                    let status = if method == "GET" {
                        if state.writes > 0 && state.readback_status != 0 {state.readback_status} else {state.get_status}
                    } else {state.put_status};
                    let status = if status == 0 {200} else {status};
                    if status != 200 {
                        return HttpResponse::build(actix_web::http::StatusCode::from_u16(status).unwrap())
                            .insert_header(("X-Reqid", "cors-request-123"))
                            .body(vec![255, b'S', b'K']);
                    }
                    if method == "POST" {
                        state.writes += 1;
                        state.rules = serde_json::from_slice(&body).unwrap();
                        return HttpResponse::Ok().insert_header(("X-Reqid", "cors-write-123")).finish();
                    }
                    if let Some(body) = &state.invalid_body { return HttpResponse::Ok().body(body.clone()); }
                    HttpResponse::Ok().insert_header(("X-Reqid", "cors-read-123"))
                        .json(if state.corrupt_readback && state.writes > 0 {vec![]} else {state.rules.clone()})
                }
            }))
        }).workers(1).listen(listener).unwrap().run();
        let handle = server.handle();
        tokio::spawn(server);
        let driver = super::super::tests::http_fixture_driver(&base).await;
        Self {
            driver,
            state,
            handle,
        }
    }

    async fn stop(self) {
        self.handle.stop(true).await;
    }
}

fn origins() -> Vec<String> {
    vec!["https://drive.test".into()]
}

#[tokio::test]
async fn uc_native_cors_preserves_order_and_repeated_application_is_a_noop() {
    let other = asterdrive_rule(&["https://other.test".into()]);
    let fixture = Fixture::new(State {
        rules: vec![other.clone()],
        ..Default::default()
    })
    .await;
    // RS/upload/download service values deliberately point elsewhere. CORS may
    // use only the independently configured UC management endpoint.
    *fixture.driver.endpoints.lock().await = None;
    let first = fixture
        .driver
        .configure_asterdrive_cors(&origins())
        .await
        .unwrap();
    assert!(first.changed);
    assert_eq!(first.preserved_rule_count, 1);
    assert_eq!(first.bucket, "archive");
    assert_eq!(first.allowed_origins, origins());
    assert_eq!(first.request_id.as_deref(), Some("cors-write-123"));
    let second = fixture
        .driver
        .configure_asterdrive_cors(&origins())
        .await
        .unwrap();
    assert!(!second.changed);
    assert_eq!(second.preserved_rule_count, 2);
    {
        let state = fixture.state.lock().unwrap();
        assert_eq!(state.writes, 1);
        assert_eq!(state.rules[0], other);
        assert_eq!(
            state
                .requests
                .iter()
                .map(|r| r.0.as_str())
                .collect::<Vec<_>>(),
            vec!["GET", "POST", "GET", "GET"]
        );
    }
    fixture.stop().await;
}

#[tokio::test]
async fn configures_empty_bucket_and_reads_back_complete_configuration() {
    let fixture = Fixture::new(State::default()).await;
    assert!(
        fixture
            .driver
            .configure_asterdrive_cors(&origins())
            .await
            .unwrap()
            .changed
    );
    assert_eq!(fixture.state.lock().unwrap().writes, 1);
    fixture.stop().await;
}

#[tokio::test]
async fn failures_keep_http_identity_without_echoing_binary_provider_messages() {
    for method in ["GET", "POST"] {
        for (status, kind) in [
            (400, StorageErrorKind::Misconfigured),
            (401, StorageErrorKind::Auth),
            (403, StorageErrorKind::Permission),
            (404, StorageErrorKind::NotFound),
            (429, StorageErrorKind::RateLimited),
            (503, StorageErrorKind::Transient),
        ] {
            let fixture = Fixture::new(State {
                get_status: if method == "GET" { status } else { 200 },
                put_status: if method == "POST" { status } else { 200 },
                ..Default::default()
            })
            .await;
            let error = fixture
                .driver
                .configure_asterdrive_cors(&origins())
                .await
                .unwrap_err();
            assert_eq!(error.kind(), kind);
            assert!(error.message().contains(&status.to_string()));
            assert!(error.message().contains("cors-request-123"));
            assert!(!error.message().contains("SK"));
            if matches!(status, 401 | 403) {
                assert!(error.message().contains("permission to read and set CORS"));
            }
            assert_eq!(fixture.state.lock().unwrap().writes, 0);
            fixture.stop().await;
        }
    }
}

#[tokio::test]
async fn invalid_unknown_oversized_or_shadowed_configurations_never_write() {
    let mut shadow = asterdrive_rule(&origins());
    shadow.exposed_header.clear();
    for (rules, invalid_body) in [
        (vec![], Some(b"not JSON MY_SECRET_KEY".to_vec())),
        (vec![], Some(vec![255])),
        (vec![], Some(vec![b' '; MAX_JSON_BYTES+1])),
        (vec![], Some(br#"[{"allowed_origin":["https://drive.test"],"allowed_method":["GET"],"future":true}]"#.to_vec())),
        (vec![shadow], None),
        (vec![asterdrive_rule(&["https://other.test".into()]);10], None),
    ] {
        let fixture = Fixture::new(State {rules, invalid_body, ..Default::default()}).await;
        let error = fixture.driver.configure_asterdrive_cors(&origins()).await.unwrap_err();
        assert!(!error.message().contains("MY_SECRET_KEY"));
        assert_eq!(fixture.state.lock().unwrap().writes, 0);
        assert_eq!(fixture.state.lock().unwrap().requests.len(), 1);
        fixture.stop().await;
    }
}

#[tokio::test]
async fn invalid_origin_fails_before_network_and_readback_mismatch_is_not_success() {
    let fixture = Fixture::new(State {
        corrupt_readback: true,
        ..Default::default()
    })
    .await;
    assert!(fixture.driver.configure_asterdrive_cors(&[]).await.is_err());
    assert!(fixture.state.lock().unwrap().requests.is_empty());
    let error = fixture
        .driver
        .configure_asterdrive_cors(&origins())
        .await
        .unwrap_err();
    assert_eq!(error.kind(), StorageErrorKind::Precondition);
    assert_eq!(fixture.state.lock().unwrap().writes, 1);
    fixture.stop().await;
}

#[tokio::test]
async fn no_write_for_existing_covering_rule_even_at_provider_rule_limit() {
    let fixture = Fixture::new(State {
        rules: vec![asterdrive_rule(&origins()); 10],
        ..Default::default()
    })
    .await;
    assert!(
        !fixture
            .driver
            .configure_asterdrive_cors(&origins())
            .await
            .unwrap()
            .changed
    );
    assert_eq!(fixture.state.lock().unwrap().writes, 0);
    fixture.stop().await;
}

#[tokio::test]
async fn merge_size_limit_is_checked_before_write_and_readback_failure_reports_mutation() {
    let mut large = asterdrive_rule(&["https://other.test".into()]);
    large.allowed_header = vec![String::new()];
    let base_len = serde_json::to_vec(&vec![large.clone()]).unwrap().len();
    large.allowed_header[0] = "x".repeat(MAX_JSON_BYTES - base_len);
    assert_eq!(
        serde_json::to_vec(&vec![large.clone()]).unwrap().len(),
        MAX_JSON_BYTES
    );
    let fixture = Fixture::new(State {
        rules: vec![large],
        ..Default::default()
    })
    .await;
    let error = fixture
        .driver
        .configure_asterdrive_cors(&origins())
        .await
        .unwrap_err();
    assert!(error.message().contains("merged Kodo CORS"));
    assert_eq!(fixture.state.lock().unwrap().writes, 0);
    fixture.stop().await;

    let fixture = Fixture::new(State {
        readback_status: 403,
        ..Default::default()
    })
    .await;
    let error = fixture
        .driver
        .configure_asterdrive_cors(&origins())
        .await
        .unwrap_err();
    assert_eq!(error.kind(), StorageErrorKind::Permission);
    assert!(
        error
            .message()
            .contains("accepted the CORS write but verification failed")
    );
    assert!(error.message().contains("cors-request-123"));
    assert_eq!(fixture.state.lock().unwrap().writes, 1);
    fixture.stop().await;
}

#[tokio::test]
async fn uc_requests_reject_redirects_and_invalid_ids_are_never_returned() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let hits = Arc::new(AtomicUsize::new(0));
    let captured = hits.clone();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = HttpServer::new(move || {
        let captured = captured.clone();
        App::new().default_service(web::to(move |req: HttpRequest| {
            let captured = captured.clone();
            async move {
                captured.fetch_add(1, Ordering::SeqCst);
                if req.path().starts_with("/corsRules") {
                    HttpResponse::Found()
                        .insert_header(("Location", "/unexpected"))
                        .insert_header(("X-Reqid", "https://private.test/?token=SECRET"))
                        .finish()
                } else {
                    panic!("credential-bearing redirect was followed")
                }
            }
        }))
    })
    .workers(1)
    .listen(listener)
    .unwrap()
    .run();
    let handle = server.handle();
    tokio::spawn(server);
    let driver = super::super::tests::http_fixture_driver(&base).await;
    let error = driver
        .configure_asterdrive_cors(&origins())
        .await
        .unwrap_err();
    assert!(error.message().contains("302"));
    assert!(!error.message().contains("SECRET"));
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    handle.stop(true).await;
}
