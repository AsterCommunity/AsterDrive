use super::*;
use actix_web::{App, HttpResponse, HttpServer, web};
use base64::engine::general_purpose::URL_SAFE;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

fn test_driver(object_prefix: &str, endpoints: QiniuRegionEndpoints) -> QiniuDriver {
    QiniuDriver {
        config: QiniuDriverConfig {
            bucket: "bucket".to_string(),
            region: "z0".to_string(),
            download_domain: "https://download.example.test".to_string(),
            object_prefix: object_prefix.to_string(),
            endpoints,
            connect_timeout: Duration::from_secs(1),
            read_timeout: Duration::from_secs(1),
            operation_timeout: Duration::from_secs(1),
        },
        credentials: QiniuStaticCredentials {
            access_key: "ak".to_string(),
            secret_key: "sk".to_string(),
        },
        client: Client::new(),
    }
}

fn test_endpoints() -> QiniuRegionEndpoints {
    QiniuRegionEndpoints {
        upload: "https://up.example.test".to_string(),
        manage: "https://rs.example.test".to_string(),
        list: "https://rsf.example.test".to_string(),
    }
}

#[test]
fn upload_token_contains_scope_and_three_segments() {
    let driver = test_driver("", test_endpoints());
    let token = driver
        .upload_token("files/object", Duration::from_secs(60))
        .expect("upload token");
    assert_eq!(token.split(':').count(), 3);
    let policy_segment = token.rsplit_once(':').expect("token policy segment").1;
    let policy = URL_SAFE
        .decode(policy_segment)
        .expect("decode upload policy");
    let policy: serde_json::Value =
        serde_json::from_slice(&policy).expect("decode upload policy JSON");
    assert_eq!(policy["scope"], "bucket:files/object");
}

#[tokio::test]
async fn multipart_requests_use_upload_token_authorization() {
    let driver = test_driver("", test_endpoints());
    let request = driver
        .presigned_upload_part_request("files/object", "upload-id", 1, Duration::from_secs(60))
        .await
        .expect("presigned part request");
    let authorization = request
        .headers
        .get("authorization")
        .expect("upload token authorization header");
    assert!(authorization.starts_with("UpToken ak:"));

    let url = Url::parse("https://rs.example.test/stat/YnVja2V0OmZpbGVzL29iamVjdA")
        .expect("management URL");
    assert!(
        driver
            .management_authorization(&url)
            .expect("management authorization")
            .starts_with("QBox ak:")
    );
}

#[tokio::test]
async fn put_reader_streams_multipart_payload() {
    async fn upload_response(
        body: web::Bytes,
        received: web::Data<Arc<Mutex<Vec<u8>>>>,
    ) -> HttpResponse {
        *received.lock().expect("test body mutex") = body.to_vec();
        HttpResponse::Ok().finish()
    }

    let received = Arc::new(Mutex::new(Vec::<u8>::new()));
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", 0)).expect("test listener should bind");
    let addr = listener
        .local_addr()
        .expect("test listener should expose local address");
    let received_for_server = Arc::clone(&received);
    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(Arc::clone(&received_for_server)))
            .route("/upload", web::post().to(upload_response))
    })
    .listen(listener)
    .expect("test server should listen")
    .run();
    let handle = server.handle();
    let task = tokio::spawn(server);

    let payload = b"streamed qiniu payload".to_vec();
    let mut endpoints = test_endpoints();
    endpoints.upload = format!("http://127.0.0.1:{}/upload", addr.port());
    let driver = test_driver("files", endpoints);
    let returned = driver
        .put_reader(
            "object.txt",
            Box::new(std::io::Cursor::new(payload.clone())),
            payload.len() as i64,
        )
        .await
        .expect("streamed upload should succeed");

    assert_eq!(returned, "object.txt");
    let body = received.lock().expect("test body mutex");
    assert!(body.windows(payload.len()).any(|window| window == payload));

    handle.stop(true).await;
    let _ = task.await;
}

#[tokio::test]
async fn list_paths_stops_after_qiniu_empty_marker() {
    async fn list_response(request_count: web::Data<Arc<AtomicUsize>>) -> HttpResponse {
        let request_number = request_count.fetch_add(1, Ordering::SeqCst);
        if request_number > 0 {
            return HttpResponse::InternalServerError().finish();
        }
        HttpResponse::Ok().json(serde_json::json!({
            "items": [{ "key": "files/one.txt" }],
            "marker": ""
        }))
    }

    let request_count = Arc::new(AtomicUsize::new(0));
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", 0)).expect("test listener should bind");
    let addr = listener
        .local_addr()
        .expect("test listener should expose local address");
    let request_count_for_server = Arc::clone(&request_count);
    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(Arc::clone(&request_count_for_server)))
            .route("/list", web::get().to(list_response))
    })
    .listen(listener)
    .expect("test server should listen")
    .run();
    let handle = server.handle();
    let task = tokio::spawn(server);
    let mut endpoints = test_endpoints();
    endpoints.list = format!("http://127.0.0.1:{}", addr.port());
    let driver = test_driver("files", endpoints);

    let paths = driver
        .list_paths(None)
        .await
        .expect("empty marker should terminate listing");
    assert_eq!(paths, vec!["one.txt"]);
    assert_eq!(request_count.load(Ordering::SeqCst), 1);
    handle.stop(true).await;
    let _ = task.await;
}
