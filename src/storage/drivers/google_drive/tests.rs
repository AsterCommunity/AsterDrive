use super::client::http_client;
use super::*;
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
use std::{collections::BTreeMap, io::Cursor, sync::Mutex};

mod boundaries;

#[derive(Debug)]
struct Token;
#[async_trait]
impl GoogleDriveTokenProvider for Token {
    async fn access_token(&self) -> Result<String> {
        Ok("administrator-secret-token".into())
    }
}
#[derive(Default)]
struct State {
    data: Option<Vec<u8>>,
    pending: Vec<u8>,
    requests: Vec<(String, String, BTreeMap<String, String>, usize)>,
    metadata: Option<serde_json::Value>,
    duplicates: bool,
    deleted: bool,
    aborts: usize,
}
struct Server {
    base: String,
    state: Arc<Mutex<State>>,
    handle: actix_web::dev::ServerHandle,
}
impl Server {
    fn driver(&self) -> GoogleDriveDriver {
        let mut driver = GoogleDriveDriver::new(
            Arc::new(Token),
            Some("pool".into()),
            Some("shared".into()),
            false,
            std::time::Duration::from_secs(10),
        )
        .unwrap();
        driver.api = format!("{}/drive/v3", self.base);
        driver.upload = format!("{}/upload/drive/v3/files", self.base);
        driver
    }
    async fn stop(self) {
        self.handle.stop(true).await;
    }
}
async fn spawn() -> Server {
    async fn handler(
        req: HttpRequest,
        body: web::Bytes,
        state: web::Data<Arc<Mutex<State>>>,
    ) -> HttpResponse {
        let mut s = state.lock().unwrap();
        let headers = req
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str().into(), v.to_str().unwrap().into()))
            .collect();
        s.requests.push((
            req.method().to_string(),
            format!("{}?{}", req.path(), req.query_string()),
            headers,
            body.len(),
        ));
        if req.path().starts_with("/error/") {
            let status: u16 = req.path().rsplit('/').next().unwrap().parse().unwrap();
            return HttpResponse::build(actix_web::http::StatusCode::from_u16(status).unwrap())
                .json(serde_json::json!({"error":{"message":"administrator-secret-token", "errors":[{"reason":"userRateLimitExceeded"}]}}));
        }
        assert_eq!(
            req.headers().get("authorization").unwrap(),
            "Bearer administrator-secret-token"
        );
        if req.path().starts_with("/upload/") {
            if req.method() == "POST" || req.method() == "PATCH" {
                s.metadata = Some(serde_json::from_slice(&body).unwrap());
                s.pending.clear();
                return HttpResponse::Ok().insert_header(("location", format!("http://{}/upload/drive/v3/files?uploadType=resumable&upload_id=private-session", req.connection_info().host()))).finish();
            }
            if req.method() == "DELETE" {
                s.aborts += 1;
                return HttpResponse::build(actix_web::http::StatusCode::from_u16(499).unwrap())
                    .finish();
            }
            let range = req
                .headers()
                .get("content-range")
                .unwrap()
                .to_str()
                .unwrap();
            if range == "bytes */*" {
                if s.data.is_some() {
                    return HttpResponse::Ok().json(serde_json::json!({"id":"owned"}));
                }
                let mut response = HttpResponse::PermanentRedirect();
                if !s.pending.is_empty() {
                    response.insert_header(("range", format!("bytes=0-{}", s.pending.len() - 1)));
                }
                return response.finish();
            }
            s.pending.extend_from_slice(&body);
            let total: usize = range.rsplit('/').next().unwrap().parse().unwrap();
            if s.pending.len() == total {
                s.data = Some(s.pending.clone());
                return HttpResponse::Ok().json(serde_json::json!({"id":"owned"}));
            }
            return HttpResponse::PermanentRedirect()
                .insert_header(("range", format!("bytes=0-{}", s.pending.len() - 1)))
                .finish();
        }
        if req.path().ends_with("/files") {
            let query: BTreeMap<String, String> =
                url::form_urlencoded::parse(req.query_string().as_bytes())
                    .into_owned()
                    .collect();
            assert!(query["q"].contains("appProperties has { key='asterdrive_storage_path'"));
            assert!(query["q"].contains("'pool' in parents"));
            assert_eq!(query["supportsAllDrives"], "true");
            assert_eq!(query["driveId"], "shared");
            let mut files = if let Some(data) = &s.data
                && !s.deleted
            {
                vec![
                    serde_json::json!({"id":"owned", "size":data.len().to_string(), "mimeType":"application/octet-stream"}),
                ]
            } else {
                vec![]
            };
            if s.duplicates && !files.is_empty() {
                files.push(serde_json::json!({"id":"other-owned","size":"1"}));
            }
            return HttpResponse::Ok().json(serde_json::json!({"files":files}));
        }
        if req.method() == "DELETE" {
            s.deleted = true;
            return HttpResponse::NoContent().finish();
        }
        if req.path().ends_with("/pool") {
            return HttpResponse::Ok().json(serde_json::json!({"id":"pool","name":"Storage","driveId":"shared","mimeType":"application/vnd.google-apps.folder","capabilities":{"canAddChildren":true}}));
        }
        let data = s.data.as_ref().unwrap();
        if let Some(range) = req.headers().get("range") {
            let (start, end) = range
                .to_str()
                .unwrap()
                .strip_prefix("bytes=")
                .unwrap()
                .split_once('-')
                .unwrap();
            let start: usize = start.parse().unwrap();
            let end: usize = if end.is_empty() {
                data.len() - 1
            } else {
                end.parse().unwrap()
            };
            return HttpResponse::PartialContent()
                .insert_header((
                    "content-range",
                    format!("bytes {start}-{end}/{}", data.len()),
                ))
                .body(data[start..=end].to_vec());
        }
        HttpResponse::Ok().body(data.clone())
    }
    let state = Arc::new(Mutex::new(State::default()));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app_state = state.clone();
    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(app_state.clone()))
            .app_data(web::PayloadConfig::new(CHUNK + 1024))
            .default_service(web::to(handler))
    })
    .workers(1)
    .listen(listener)
    .unwrap()
    .run();
    let handle = server.handle();
    tokio::spawn(server);
    Server {
        base,
        state,
        handle,
    }
}
#[test]
fn query_and_session_boundaries_keep_objects_and_credentials_private() {
    let driver = GoogleDriveDriver::new(
        Arc::new(Token),
        Some("pool".into()),
        None,
        false,
        std::time::Duration::from_secs(10),
    )
    .unwrap();
    assert!(
        driver
            .object_query("a'\\b")
            .unwrap()
            .contains("value='a\\'\\\\b'")
    );
    assert!(driver.object_query(&"x".repeat(124)).is_err());
    assert!(driver.object_query("").is_err());
    assert!(driver.object_query("files/a\nb").is_err());
    for url in [
        "https://attacker.test/upload/drive/v3/files?upload_id=x",
        "http://www.googleapis.com/upload/drive/v3/files?upload_id=x",
        "https://www.googleapis.com/drive/v3/files?upload_id=x",
        "https://www.googleapis.com/upload/drive/v3/files",
    ] {
        assert!(driver.validate_session_url(url).is_err());
    }
    driver
        .validate_session_url(
            "https://www.googleapis.com/upload/drive/v3/files?uploadType=resumable&upload_id=x",
        )
        .unwrap();
    assert!(!format!("{driver:?}").contains("administrator-secret-token"));
    assert!(driver.extensions().direct_download.is_none());
}
#[tokio::test]
async fn blob_lifecycle_uses_app_properties_ranges_and_owned_id() {
    let server = spawn().await;
    let driver = server.driver();
    assert_eq!(driver.validate_root().await.unwrap().0, "pool");
    assert!(!driver.exists("files/key").await.unwrap());
    driver.put("files/key", b"abcdef").await.unwrap();
    assert!(driver.exists("files/key").await.unwrap());
    assert_eq!(driver.metadata("files/key").await.unwrap().size, 6);
    assert_eq!(driver.get("files/key").await.unwrap(), b"abcdef");
    let mut stream = driver.get_range("files/key", 2, Some(3)).await.unwrap();
    let mut data = Vec::new();
    stream.read_to_end(&mut data).await.unwrap();
    assert_eq!(data, b"cde");
    driver.put("files/key", b"replacement").await.unwrap();
    assert!(
        server
            .state
            .lock()
            .unwrap()
            .requests
            .iter()
            .any(|r| r.0 == "PATCH")
    );
    driver.delete("files/key").await.unwrap();
    driver.delete("files/key").await.unwrap();
    assert_eq!(
        driver.get("files/key").await.unwrap_err().kind(),
        StorageErrorKind::NotFound
    );
    {
        let s = server.state.lock().unwrap();
        assert_eq!(
            s.requests
                .iter()
                .filter(|r| r.0 == "DELETE" && r.1.contains("/files/owned"))
                .count(),
            1
        );
        assert_eq!(
            s.metadata.as_ref().unwrap()["appProperties"][APP_KEY],
            "files/key"
        );
    }
    server.stop().await;
}
#[tokio::test]
async fn large_reader_uses_bounded_fragments_and_confirms_acknowledgements() {
    let server = spawn().await;
    let driver = server.driver();
    let size = CHUNK * 2 + 13;
    driver
        .put_reader(
            "files/large",
            Box::new(tokio::io::repeat(7).take(size as u64)),
            size as i64,
        )
        .await
        .unwrap();
    {
        let s = server.state.lock().unwrap();
        let puts: Vec<_> = s.requests.iter().filter(|r| r.0 == "PUT").collect();
        assert_eq!(
            puts.iter().map(|r| r.3).collect::<Vec<_>>(),
            [CHUNK, CHUNK, 13]
        );
        assert_eq!(
            puts[1].2["content-range"],
            format!("bytes {}-{}/{size}", CHUNK, CHUNK * 2 - 1)
        );
        assert_eq!(
            s.requests.iter().find(|r| r.0 == "POST").unwrap().2["x-upload-content-length"],
            size.to_string()
        );
    }
    server.stop().await;
}
#[tokio::test]
async fn empty_and_mismatched_readers_do_not_publish_invalid_content() {
    for (data, size) in [
        (b"".as_slice(), 0),
        (b"short".as_slice(), 6),
        (b"oversize".as_slice(), 3),
        (b"x".as_slice(), 0),
    ] {
        let server = spawn().await;
        let result = server
            .driver()
            .put_reader("files/key", Box::new(Cursor::new(data.to_vec())), size)
            .await;
        if data.is_empty() {
            result.unwrap();
            assert_eq!(server.state.lock().unwrap().data, Some(vec![]));
        } else {
            assert_eq!(result.unwrap_err().kind(), StorageErrorKind::Precondition);
            assert!(server.state.lock().unwrap().data.is_none());
            assert_eq!(server.state.lock().unwrap().aborts, 1);
        }
        server.stop().await;
    }
}
#[tokio::test]
async fn duplicate_owned_keys_fail_closed_before_delete() {
    let server = spawn().await;
    server.driver().put("files/key", b"x").await.unwrap();
    server.state.lock().unwrap().duplicates = true;
    assert_eq!(
        server
            .driver()
            .delete("files/key")
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Misconfigured
    );
    assert!(!server.state.lock().unwrap().deleted);
    server.stop().await;
}
#[tokio::test]
async fn abort_attempt_without_provider_session_never_deletes_existing_object() {
    let server = spawn().await;
    let driver = server.driver();
    driver.put("files/key", b"existing").await.unwrap();
    let attempt = StreamUploadAttempt::new("files/key", 3).unwrap();
    assert_eq!(
        driver.abort_attempt(&attempt).await.unwrap(),
        StreamUploadCleanup::NotRequired
    );
    assert_eq!(driver.get("files/key").await.unwrap(), b"existing");
    assert!(!server.state.lock().unwrap().deleted);
    server.stop().await;
}

#[tokio::test]
async fn abort_attempt_cancels_session_without_deleting_existing_object() {
    let server = spawn().await;
    let driver = server.driver();
    driver.put("files/key", b"existing").await.unwrap();
    let session = driver.create_upload_session("files/key").await.unwrap();
    let attempt = StreamUploadAttempt::new("files/key", 3).unwrap();
    attempt.set_provider_session(session.upload_url);
    assert_eq!(
        driver.abort_attempt(&attempt).await.unwrap(),
        StreamUploadCleanup::Cleaned
    );
    assert_eq!(driver.get("files/key").await.unwrap(), b"existing");
    {
        let state = server.state.lock().unwrap();
        assert_eq!(state.aborts, 1);
        assert!(!state.deleted);
    }
    server.stop().await;
}

#[tokio::test]
async fn resumable_status_and_final_fragment_follow_generic_contract() {
    let server = spawn().await;
    let driver = server.driver();
    let session = driver.create_upload_session("files/key").await.unwrap();
    assert_eq!(
        driver
            .query_upload_session(&session.upload_url)
            .await
            .unwrap()
            .next_expected_ranges,
        ["0-"]
    );
    let first = driver
        .upload_session_fragment_reader(
            &session.upload_url,
            0,
            ALIGNMENT as u64 + 1,
            Box::new(Cursor::new(vec![0; ALIGNMENT])),
            ALIGNMENT as i64,
        )
        .await
        .unwrap();
    assert!(!first.completed);
    assert_eq!(first.next_expected_ranges, [format!("{ALIGNMENT}-")]);
    let last = driver
        .upload_session_fragment_reader(
            &session.upload_url,
            ALIGNMENT as u64,
            ALIGNMENT as u64 + 1,
            Box::new(Cursor::new(vec![1])),
            1,
        )
        .await
        .unwrap();
    assert!(last.completed);
    assert!(
        driver
            .query_upload_session(&session.upload_url)
            .await
            .unwrap()
            .next_expected_ranges
            .is_empty()
    );
    driver
        .abort_upload_session(&session.upload_url)
        .await
        .unwrap();
    server.stop().await;
}
#[tokio::test]
async fn provider_errors_are_typed_and_do_not_echo_provider_secrets() {
    let server = spawn().await;
    for (status, kind) in [
        (401, StorageErrorKind::Auth),
        (403, StorageErrorKind::RateLimited),
        (404, StorageErrorKind::NotFound),
        (429, StorageErrorKind::RateLimited),
        (503, StorageErrorKind::Transient),
        (400, StorageErrorKind::Misconfigured),
    ] {
        let response = http_client()
            .unwrap()
            .get(format!("{}/error/{status}", server.base))
            .send()
            .await
            .unwrap();
        let error = map_http(response).await;
        assert_eq!(error.kind(), kind);
        assert!(!error.to_string().contains("administrator-secret-token"));
    }
    server.stop().await;
}
