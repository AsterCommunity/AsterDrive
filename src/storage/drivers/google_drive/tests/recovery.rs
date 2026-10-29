//! Failed transfers must preserve their typed error and close only their session.
use super::*;
use crate::storage::test_support::{HttpFixture, HttpReply};
use serde_json::json;

fn driver(peer: &HttpFixture) -> GoogleDriveDriver {
    let mut driver = GoogleDriveDriver::new(
        Arc::new(Token),
        Some("pool".into()),
        None,
        false,
        String::new(),
        Duration::from_secs(10),
    )
    .unwrap();
    driver.api = format!("{}/drive/v3", peer.base);
    driver.upload = format!("{}/upload/drive/v3/files", peer.base);
    driver
}
fn lookup() -> HttpReply {
    HttpReply::json(200, json!({"files":[{"id":"owned","size":"6"}]}))
}

#[tokio::test]
async fn incomplete_stream_acknowledgement_aborts_only_its_session() {
    let peer = HttpFixture::new([
        HttpReply::json(200, json!({"files":[]})),
        HttpReply::text(200, "").header(
            "location",
            "{base}/upload/drive/v3/files?upload_id=private-session",
        ),
        HttpReply::text(308, "").header("range", "bytes=0-2"),
        HttpReply::text(499, ""),
    ])
    .await;
    let attempt = StreamUploadAttempt::new("files/key", 6).unwrap();
    let driver = driver(&peer);
    let error = driver
        .stage_attempt(&attempt, Box::new(Cursor::new(b"abcdef".to_vec())))
        .await
        .unwrap_err();
    assert_eq!(error.kind(), StorageErrorKind::Transient);
    assert_eq!(
        driver.abort_attempt(&attempt).await.unwrap(),
        StreamUploadCleanup::NotRequired
    );
    let requests = peer.requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[2].headers["content-range"], "bytes 0-5/6");
    assert_eq!(requests[3].method, "DELETE");
    assert_eq!(requests[3].query["upload_id"], "private-session");
    peer.stop().await;
}

#[tokio::test]
async fn failed_fragment_and_download_preserve_http_error_kinds() {
    for (status, kind) in [
        (408, StorageErrorKind::Transient),
        (401, StorageErrorKind::Auth),
        (403, StorageErrorKind::Permission),
        (429, StorageErrorKind::RateLimited),
        (503, StorageErrorKind::Transient),
    ] {
        let peer = HttpFixture::new([HttpReply::text(status, "private-session")]).await;
        let upload_driver = driver(&peer);
        let url = format!("{}?upload_id=private-session", upload_driver.upload);
        let error = upload_driver
            .upload_session_fragment_reader(
                &url,
                0,
                6,
                Box::new(Cursor::new(b"abcdef".to_vec())),
                6,
            )
            .await
            .unwrap_err();
        assert_eq!(error.kind(), kind);
        assert!(!error.to_string().contains("private-session"));
        assert_eq!(peer.requests().len(), 1);
        peer.stop().await;
        let peer = HttpFixture::new([
            lookup(),
            HttpReply::text(status, "administrator-secret-token"),
        ])
        .await;
        let error = driver(&peer).get("files/key").await.unwrap_err();
        assert_eq!(error.kind(), kind);
        assert!(!error.to_string().contains("administrator-secret-token"));
        peer.stop().await;
    }
}

#[tokio::test]
async fn disconnected_provider_is_retryable_and_does_not_expose_its_session_url() {
    let peer = HttpFixture::new([]).await;
    let driver = driver(&peer);
    let url = format!("{}?upload_id=private-session", driver.upload);
    peer.stop().await;
    let error = driver.query_upload_session(&url).await.unwrap_err();
    assert_eq!(error.kind(), StorageErrorKind::Transient);
    assert!(!error.to_string().contains("private-session"));
    assert!(!error.to_string().contains("upload_id"));
}

#[tokio::test]
async fn missing_local_upload_and_oversized_copy_fail_before_session_creation() {
    let peer = HttpFixture::new([]).await;
    let directory = aster_forge_test::temp::TestTempDir::new("google-drive-missing-upload");
    let missing = directory.path().join("missing");
    assert_eq!(
        driver(&peer)
            .put_file("files/key", missing.to_str().unwrap())
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Transient
    );
    assert!(peer.requests().is_empty());
    peer.stop().await;
    let peer = HttpFixture::new([
        HttpReply::json(
            200,
            json!({"files":[{"id":"owned","size":(i64::MAX as u64 + 1).to_string()}]}),
        ),
        lookup(),
        HttpReply::text(200, "abcdef"),
    ])
    .await;
    assert_eq!(
        driver(&peer)
            .copy_object("files/source", "files/dest")
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Precondition
    );
    assert!(peer.requests().iter().all(|r| r.method == "GET"));
    peer.stop().await;
}

#[tokio::test]
async fn invalid_upload_endpoint_is_configuration_failure_before_network_io() {
    let peer = HttpFixture::new([]).await;
    let mut driver = driver(&peer);
    driver.upload = "not an absolute URL".into();
    let error = driver
        .query_upload_session(
            "https://www.googleapis.com/upload/drive/v3/files?upload_id=private-session",
        )
        .await
        .unwrap_err();
    assert_eq!(error.kind(), StorageErrorKind::Misconfigured);
    assert!(peer.requests().is_empty());
    peer.stop().await;
}

#[tokio::test]
async fn truncated_download_body_returns_a_retryable_error_instead_of_partial_bytes() {
    use tokio::io::AsyncWriteExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        // HTTP framing is deliberately broken on the second request; a normal
        // buffered mock response cannot exercise failure while reading a body.
        for body in [Some(r#"{"files":[{"id":"owned","size":"6"}]}"#), None] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                request.push(stream.read_u8().await.unwrap());
            }
            let response = match body {
                Some(body) => format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                ),
                None => {
                    "HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nabc".into()
                }
            };
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.shutdown().await.unwrap();
        }
    });
    let mut driver = GoogleDriveDriver::new(
        Arc::new(Token),
        None,
        None,
        false,
        String::new(),
        Duration::from_secs(10),
    )
    .unwrap();
    driver.api = format!("{base}/drive/v3");
    let error = driver.get("files/key").await.unwrap_err();
    assert_eq!(error.kind(), StorageErrorKind::Transient);
    assert!(!error.to_string().contains("administrator-secret-token"));
    server.await.unwrap();
}

#[tokio::test]
async fn browser_coordinator_reconciles_invisible_ranges_lost_responses_and_final_response_loss() {
    use aster_drive_storage::{
        BrowserUploadContext, BrowserUploadDriver, BrowserUploadObservation, BrowserUploadReceipt,
    };
    // The browser cannot distinguish an absent Range from a CORS-hidden Range.
    for status in [0, 308, 416, 503] {
        let peer = HttpFixture::new([HttpReply::text(308, "").header("range", "bytes=0-2")]).await;
        let driver = driver(&peer);
        let protocol = driver.protocol();
        let url = format!("{}?upload_id=private-session", driver.upload);
        let context = || BrowserUploadContext {
            protocol: &protocol,
            upload_url: &url,
            storage_path: "files/key",
            total_size: 6,
            fragment_size: 6,
        };
        let request = crate::storage::drivers::browser_resumable::transition(
            context(),
            0,
            false,
            vec!["range".into()],
        )
        .unwrap()
        .request
        .unwrap();
        let observation = BrowserUploadObservation {
            status,
            headers: BTreeMap::new(),
            body: String::new(),
            body_truncated: false,
        };
        let next = driver
            .advance(
                context(),
                serde_json::Value::Null,
                Some(BrowserUploadReceipt {
                    request: &request,
                    observation: &observation,
                }),
            )
            .await
            .unwrap();
        assert_eq!(next.confirmed_offset, 3);
        assert!(!next.provider_completed);
        assert_eq!(next.request.unwrap().body.unwrap().start, 3);
        let requests = peer.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].headers["content-range"], "bytes */*");
        peer.stop().await;
    }
    let peer = HttpFixture::new([HttpReply::text(200, "{\"id\":\"owned\"}")]).await;
    let driver = driver(&peer);
    let protocol = driver.protocol();
    let url = format!("{}?upload_id=private-session", driver.upload);
    let result = driver
        .advance(
            BrowserUploadContext {
                protocol: &protocol,
                upload_url: &url,
                storage_path: "files/key",
                total_size: 6,
                fragment_size: 6,
            },
            serde_json::Value::Null,
            None,
        )
        .await
        .unwrap();
    assert_eq!(result.confirmed_offset, 6);
    assert!(result.provider_completed);
    assert!(result.request.is_none());
    peer.stop().await;
}

#[tokio::test]
async fn browser_expired_session_requires_an_object_of_the_expected_size() {
    use aster_drive_storage::{BrowserUploadContext, BrowserUploadDriver};
    for size in ["3", "6"] {
        let peer = HttpFixture::new([
            HttpReply::text(404, "expired"),
            HttpReply::json(200, json!({"files":[{"id":"owned","size":size}]})),
        ])
        .await;
        let driver = driver(&peer);
        let protocol = driver.protocol();
        let url = format!("{}?upload_id=private-session", driver.upload);
        let result = driver
            .advance(
                BrowserUploadContext {
                    protocol: &protocol,
                    upload_url: &url,
                    storage_path: "files/key",
                    total_size: 6,
                    fragment_size: 6,
                },
                serde_json::Value::Null,
                None,
            )
            .await;
        if size == "6" {
            assert!(result.unwrap().provider_completed);
        } else {
            assert!(result.is_err());
        }
        peer.stop().await;
    }
}
