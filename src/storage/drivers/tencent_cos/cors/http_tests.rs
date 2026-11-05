use super::*;
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
use std::collections::VecDeque;
use std::sync::Mutex;

struct Reply {
    method: &'static str,
    status: u16,
    body: Vec<u8>,
}

struct Fixture {
    driver: TencentCosDriver,
    pending: Arc<Mutex<VecDeque<Reply>>>,
    requests: Arc<Mutex<Vec<Vec<u8>>>>,
    handle: actix_web::dev::ServerHandle,
}

impl Fixture {
    async fn new(replies: Vec<Reply>) -> Self {
        let pending = Arc::new(Mutex::new(VecDeque::from(replies)));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let queue = pending.clone();
        let captured = requests.clone();
        let server = HttpServer::new(move || {
            let queue = queue.clone();
            let captured = captured.clone();
            App::new().default_service(web::to(move |req: HttpRequest, body: web::Bytes| {
                let queue = queue.clone();
                let captured = captured.clone();
                async move {
                    assert_eq!(req.uri().to_string(), "/?cors");
                    assert!(
                        req.headers()
                            .get("authorization")
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .contains("q-url-param-list=cors")
                    );
                    if req.method() == "PUT" {
                        assert_eq!(
                            req.headers().get("content-type").unwrap(),
                            "application/xml"
                        );
                        assert_eq!(
                            req.headers().get("content-md5").unwrap().to_str().unwrap(),
                            content_md5_base64(&body)
                        );
                    }
                    captured.lock().unwrap().push(body.to_vec());
                    let reply = queue
                        .lock()
                        .unwrap()
                        .pop_front()
                        .expect("unexpected cloud write/request");
                    assert_eq!(req.method().as_str(), reply.method);
                    HttpResponse::build(
                        actix_web::http::StatusCode::from_u16(reply.status).unwrap(),
                    )
                    .insert_header(("x-cos-request-id", "header-request-123"))
                    .body(reply.body)
                }
            }))
        })
        .workers(1)
        .listen(listener)
        .unwrap()
        .run();
        let handle = server.handle();
        tokio::spawn(server);
        let mut config = super::super::tests::sample_config(&endpoint, "archive-123");
        config.use_cname = true;
        Self {
            driver: TencentCosDriver::new(config, super::super::tests::sample_credentials())
                .unwrap(),
            pending,
            requests,
            handle,
        }
    }

    async fn finish(self, expected_requests: usize) {
        assert!(self.pending.lock().unwrap().is_empty());
        assert_eq!(self.requests.lock().unwrap().len(), expected_requests);
        self.handle.stop(true).await;
    }
}

fn reply(method: &'static str, status: u16, body: impl AsRef<[u8]>) -> Reply {
    Reply {
        method,
        status,
        body: body.as_ref().to_vec(),
    }
}

#[tokio::test]
async fn cors_http_failure_keeps_status_request_id_and_permission_before_utf8() {
    for method in ["GET", "PUT"] {
        for (status, expected) in [
            (400, StorageErrorKind::Misconfigured),
            (401, StorageErrorKind::Auth),
            (403, StorageErrorKind::Permission),
            (409, StorageErrorKind::Precondition),
            (412, StorageErrorKind::Precondition),
            (429, StorageErrorKind::RateLimited),
            (503, StorageErrorKind::Transient),
        ] {
            let mut replies = Vec::new();
            if method == "PUT" {
                replies.push(reply("GET", 200, "<CORSConfiguration/>"));
            }
            replies.push(reply(
                method,
                status,
                b"\xffSECRETEXAMPLE ?q-signature=private",
            ));
            let fixture = Fixture::new(replies).await;
            let error = fixture
                .driver
                .configure_asterdrive_cors(&["https://drive.test".into()])
                .await
                .unwrap_err();
            assert_eq!(error.storage_error_kind(), Some(expected));
            assert!(error.message().contains(&status.to_string()), "{error}");
            assert!(error.message().contains("header-request-123"), "{error}");
            assert!(!error.message().contains("SECRETEXAMPLE"));
            assert!(!error.message().contains("q-signature"));
            if status == 403 {
                assert!(error.message().contains(if method == "GET" {
                    "name/cos:GetBucketCORS"
                } else {
                    "name/cos:PutBucketCORS"
                }));
            }
            fixture.finish(if method == "GET" { 1 } else { 2 }).await;
        }
    }
}

#[tokio::test]
async fn cors_read_failure_never_writes_or_treats_missing_bucket_as_empty() {
    for (status, body) in [
        (404, b"<Error><Code>NoSuchBucket</Code></Error>".as_slice()),
        (404, b"not XML".as_slice()),
        (200, b"\xff".as_slice()),
        (200, b"<Error/>".as_slice()),
        (200, b"<CORSConfiguration>".as_slice()),
    ] {
        let fixture = Fixture::new(vec![reply("GET", status, body)]).await;
        assert!(
            fixture
                .driver
                .configure_asterdrive_cors(&["https://drive.test".into()])
                .await
                .is_err()
        );
        fixture.finish(1).await;
    }
}

#[tokio::test]
async fn cors_http_bounds_success_and_error_bodies_without_losing_failure_identity() {
    for (method, status, limit) in [
        ("GET", 200, COS_CORS_XML_MAX_BYTES),
        ("GET", 503, COS_ERROR_XML_MAX_BYTES),
        ("PUT", 403, COS_ERROR_XML_MAX_BYTES),
    ] {
        let mut replies = Vec::new();
        if method == "PUT" {
            replies.push(reply("GET", 200, "<CORSConfiguration/>"));
        }
        replies.push(reply(method, status, vec![b'x'; limit + 1]));
        let fixture = Fixture::new(replies).await;
        let error = fixture
            .driver
            .configure_asterdrive_cors(&["https://drive.test".into()])
            .await
            .unwrap_err();
        if status != 200 {
            assert!(error.message().contains(&status.to_string()));
            assert!(error.message().contains("header-request-123"));
        }
        fixture.finish(if method == "GET" { 1 } else { 2 }).await;
    }
}

#[tokio::test]
async fn cors_absent_configuration_and_existing_rules_use_signed_merge() {
    for existing in [
        "<Error><Code>NoSuchCORSConfiguration</Code></Error>",
        "<CORSConfiguration><CORSRule><ID>other-app</ID><AllowedOrigin>https://other.test</AllowedOrigin><AllowedMethod>GET</AllowedMethod></CORSRule><CORSRule><ID>asterdrive-presigned-access</ID><AllowedOrigin>https://old.test</AllowedOrigin><AllowedMethod>GET</AllowedMethod></CORSRule></CORSConfiguration>",
    ] {
        let missing = existing.starts_with("<Error>");
        let fixture = Fixture::new(vec![
            reply("GET", if missing { 404 } else { 200 }, existing),
            reply("PUT", 200, ""),
        ])
        .await;
        let result = fixture
            .driver
            .configure_asterdrive_cors(&["https://drive.test".into()])
            .await
            .unwrap();
        assert_eq!(result.request_id.as_deref(), Some("header-request-123"));
        assert_eq!(result.preserved_rule_count, usize::from(!missing));
        assert_eq!(result.replaced_existing_rule, !missing);
        let body = fixture.requests.lock().unwrap()[1].clone();
        let merged = parse_cors_configuration(std::str::from_utf8(&body).unwrap()).unwrap();
        assert_eq!(merged.response_vary, Some(true));
        assert_eq!(merged.rules.len(), if missing { 1 } else { 2 });
        assert_eq!(
            merged.rules.last().unwrap(),
            &asterdrive_cors_rule(&["https://drive.test".into()])
        );
        if !missing {
            assert_eq!(merged.rules[0].id.as_deref(), Some("other-app"));
        }
        fixture.finish(2).await;
    }
}

#[tokio::test]
async fn cors_shadowed_put_rule_without_etag_never_writes() {
    let shadow = CosCorsRule {
        id: Some("other-app".into()),
        allowed_origins: vec!["*".into()],
        allowed_methods: vec!["PUT".into()],
        allowed_headers: vec!["*".into()],
        expose_headers: vec!["Content-Length".into()],
        max_age_seconds: Some(600),
    };
    let body = build_cors_configuration_xml(&CosCorsConfiguration {
        rules: vec![shadow],
        response_vary: None,
    })
    .unwrap();
    let fixture = Fixture::new(vec![reply("GET", 200, body)]).await;
    let error = fixture
        .driver
        .configure_asterdrive_cors(&["https://drive.test".into()])
        .await
        .unwrap_err();
    assert_eq!(
        error.storage_error_kind(),
        Some(StorageErrorKind::Precondition)
    );
    assert!(error.message().contains("no rules were written"));
    fixture.finish(1).await;
}

#[tokio::test]
async fn cors_merge_that_exceeds_provider_limits_never_sends_put() {
    let other = "<CORSRule><AllowedOrigin>https://other.test</AllowedOrigin><AllowedMethod>GET</AllowedMethod></CORSRule>";
    let full_rules = format!(
        "<CORSConfiguration>{}</CORSConfiguration>",
        other.repeat(100)
    );
    let template = "<CORSConfiguration><CORSRule><ID></ID><AllowedOrigin>https://other.test</AllowedOrigin><AllowedMethod>GET</AllowedMethod></CORSRule></CORSConfiguration>";
    let near_limit = template.replace(
        "<ID></ID>",
        &format!(
            "<ID>{}</ID>",
            "x".repeat(COS_CORS_XML_MAX_BYTES - template.len() - 100)
        ),
    );
    for xml in [
        full_rules,
        near_limit,
        "<CORSConfiguration><FutureFlag/></CORSConfiguration>".into(),
    ] {
        let fixture = Fixture::new(vec![reply("GET", 200, xml)]).await;
        assert!(
            fixture
                .driver
                .configure_asterdrive_cors(&["https://drive.test".into()])
                .await
                .is_err()
        );
        fixture.finish(1).await;
    }
}
