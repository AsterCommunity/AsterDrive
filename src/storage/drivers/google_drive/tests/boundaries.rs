//! Exercise provider requests and cleanup side effects, not only local helpers.
use super::*;
use crate::storage::test_support::{HttpFixture, HttpReply};
use serde_json::json;

fn driver(peer: &HttpFixture) -> GoogleDriveDriver {
    let mut driver = GoogleDriveDriver::new(
        Arc::new(Token),
        Some("pool".into()),
        Some("shared".into()),
        false,
        String::new(),
        std::time::Duration::from_secs(10),
    )
    .unwrap();
    driver.api = format!("{}/drive/v3", peer.base);
    driver.upload = format!("{}/upload/drive/v3/files", peer.base);
    driver
}
fn file(id: &str) -> serde_json::Value {
    json!({"id":id,"size":"6","mimeType":"application/octet-stream"})
}
fn files(items: Vec<serde_json::Value>) -> HttpReply {
    HttpReply::json(200, json!({"files":items}))
}
fn session(peer: &HttpFixture) -> String {
    format!(
        "{}/upload/drive/v3/files?uploadType=resumable&upload_id=private-session",
        peer.base
    )
}

#[tokio::test]
async fn underlying_reader_failures_are_transient_and_failed_cleanup_can_be_retried() {
    struct FailedReader;
    impl AsyncRead for FailedReader {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            _: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "upstream disconnected",
            )))
        }
    }
    let peer = HttpFixture::new([
        files(vec![]),
        HttpReply::text(200, "").header(
            "location",
            "{base}/upload/drive/v3/files?upload_id=private-session",
        ),
        HttpReply::text(503, ""),
        HttpReply::text(499, ""),
    ])
    .await;
    let driver = driver(&peer);
    let attempt = StreamUploadAttempt::new("files/key", 4).unwrap();
    assert_eq!(
        driver
            .stage_attempt(&attempt, Box::new(FailedReader))
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Transient
    );
    assert_eq!(
        driver.abort_attempt(&attempt).await.unwrap(),
        StreamUploadCleanup::Cleaned
    );
    assert_eq!(
        peer.requests()
            .iter()
            .filter(|r| r.method == "DELETE")
            .count(),
        2
    );
    assert!(
        peer.requests()
            .iter()
            .all(|r| r.method != "DELETE" || r.path == "/upload/drive/v3/files")
    );
    peer.stop().await;
}

#[tokio::test]
async fn session_creation_rejects_missing_untrusted_and_malformed_locations() {
    for reply in [
        HttpReply::text(200, ""),
        HttpReply::text(200, "").header(
            "location",
            "https://attacker.invalid/?upload_id=private-session",
        ),
        HttpReply::text(200, "").header("location", "not a URL private-session"),
        HttpReply::text(503, "private-session"),
    ] {
        let peer = HttpFixture::new([files(vec![]), reply]).await;
        let error = driver(&peer)
            .create_upload_session("files/key")
            .await
            .unwrap_err();
        assert!(!error.to_string().contains("private-session"));
        assert_eq!(peer.requests().len(), 2);
        peer.stop().await;
    }
}

#[tokio::test]
async fn streaming_copy_and_local_file_upload_use_the_same_exact_size_protocol() {
    let peer = HttpFixture::new([
        files(vec![file("source")]),
        files(vec![file("source")]),
        HttpReply::text(200, "abcdef"),
        files(vec![]),
        HttpReply::text(200, "").header(
            "location",
            "{base}/upload/drive/v3/files?upload_id=private-session",
        ),
        HttpReply::json(200, json!({"id":"copied"})),
    ])
    .await;
    assert_eq!(
        driver(&peer)
            .copy_object("files/source", "files/destination")
            .await
            .unwrap(),
        "files/destination"
    );
    let requests = peer.requests();
    assert_eq!(requests.last().unwrap().body, b"abcdef");
    assert_eq!(
        requests.last().unwrap().headers["content-range"],
        "bytes 0-5/6"
    );
    peer.stop().await;
    let server = spawn().await;
    let directory = aster_forge_test::temp::TestTempDir::new("google-drive-put-file");
    let path = directory.path().join("upload.bin");
    std::fs::write(&path, b"local-file").unwrap();
    let driver = server.driver();
    driver
        .put_file("files/key", path.to_str().unwrap())
        .await
        .unwrap();
    assert_eq!(driver.get("files/key").await.unwrap(), b"local-file");
    server.stop().await;
}

#[tokio::test]
async fn zero_length_ranges_and_overflow_do_not_start_remote_reads() {
    let peer = HttpFixture::new([]).await;
    let driver = driver(&peer);
    let mut stream = driver.get_range("files/key", 0, Some(0)).await.unwrap();
    let mut data = Vec::new();
    stream.read_to_end(&mut data).await.unwrap();
    assert!(data.is_empty());
    assert_eq!(
        driver
            .get_range("files/key", u64::MAX, Some(2))
            .await
            .err()
            .unwrap()
            .kind(),
        StorageErrorKind::Precondition
    );
    assert!(peer.requests().is_empty());
    peer.stop().await;
}

#[tokio::test]
async fn failed_existing_key_attempt_and_repeated_abort_never_delete_the_destination() {
    let server = spawn().await;
    let driver = server.driver();
    driver.put("files/key", b"existing").await.unwrap();
    let attempt = StreamUploadAttempt::new("files/key", 10).unwrap();
    assert!(
        driver
            .stage_attempt(&attempt, Box::new(Cursor::new(b"short".to_vec())))
            .await
            .is_err()
    );
    assert_eq!(
        driver.abort_attempt(&attempt).await.unwrap(),
        StreamUploadCleanup::NotRequired
    );
    assert_eq!(
        driver.abort_attempt(&attempt).await.unwrap(),
        StreamUploadCleanup::NotRequired
    );
    assert_eq!(driver.get("files/key").await.unwrap(), b"existing");
    assert!(!server.state.lock().unwrap().deleted);
    server.stop().await;
}

#[tokio::test]
async fn successful_attempt_clears_session_without_an_extra_abort_or_object_delete() {
    let server = spawn().await;
    let driver = server.driver();
    let attempt = StreamUploadAttempt::new("files/key", 3).unwrap();
    driver
        .stage_attempt(&attempt, Box::new(Cursor::new(b"abc".to_vec())))
        .await
        .unwrap();
    assert_eq!(driver.commit_attempt(&attempt).await.unwrap(), "files/key");
    assert_eq!(
        driver.abort_attempt(&attempt).await.unwrap(),
        StreamUploadCleanup::NotRequired
    );
    assert_eq!(driver.get("files/key").await.unwrap(), b"abc");
    assert_eq!(server.state.lock().unwrap().aborts, 0);
    server.stop().await;
}

#[tokio::test]
async fn abort_failure_retains_the_session_for_a_safe_retry() {
    let peer = HttpFixture::new([
        HttpReply::text(503, "private-session"),
        HttpReply::text(499, ""),
    ])
    .await;
    let driver = driver(&peer);
    let attempt = StreamUploadAttempt::new("files/key", 4).unwrap();
    attempt.set_provider_session(session(&peer));
    assert_eq!(
        driver.abort_attempt(&attempt).await.unwrap_err().kind(),
        StorageErrorKind::Transient
    );
    assert_eq!(
        driver.abort_attempt(&attempt).await.unwrap(),
        StreamUploadCleanup::Cleaned
    );
    assert_eq!(
        driver.abort_attempt(&attempt).await.unwrap(),
        StreamUploadCleanup::NotRequired
    );
    assert!(
        peer.requests()
            .iter()
            .all(|r| r.method == "DELETE" && r.path == "/upload/drive/v3/files")
    );
    peer.stop().await;
}

#[tokio::test]
async fn cancelled_abort_does_not_lose_the_cleanup_capability() {
    use crate::storage::test_support::ResponseGate;
    let gate = Arc::new(ResponseGate::default());
    let peer = HttpFixture::new([
        HttpReply::text(204, "").gated(gate.clone()),
        HttpReply::text(499, ""),
    ])
    .await;
    let driver = driver(&peer);
    let attempt = StreamUploadAttempt::new("files/key", 4).unwrap();
    attempt.set_provider_session(session(&peer));
    let mut abort = Box::pin(driver.abort_attempt(&attempt));
    tokio::select! { result = &mut abort => panic!("unexpected abort completion: {result:?}"), () = gate.entered.notified() => {} }
    drop(abort);
    gate.release.notify_one();
    assert_eq!(
        driver.abort_attempt(&attempt).await.unwrap(),
        StreamUploadCleanup::Cleaned
    );
    assert_eq!(peer.requests().len(), 2);
    peer.stop().await;
}

#[tokio::test]
async fn cancelled_stage_preserves_a_session_handle_without_deleting_the_object() {
    struct PendingReader(Arc<tokio::sync::Notify>);
    impl AsyncRead for PendingReader {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            _: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            self.0.notify_one();
            std::task::Poll::Pending
        }
    }
    let peer = HttpFixture::new([
        files(vec![]),
        HttpReply::text(200, "").header(
            "location",
            "{base}/upload/drive/v3/files?upload_id=private-session",
        ),
        HttpReply::text(499, ""),
    ])
    .await;
    let driver = driver(&peer);
    let attempt = StreamUploadAttempt::new("files/key", 4).unwrap();
    let entered = Arc::new(tokio::sync::Notify::new());
    let stage_driver = driver.clone();
    let stage_attempt = attempt.clone();
    let signal = entered.clone();
    let stage = tokio::spawn(async move {
        stage_driver
            .stage_attempt(&stage_attempt, Box::new(PendingReader(signal)))
            .await
    });
    entered.notified().await;
    stage.abort();
    assert!(stage.await.unwrap_err().is_cancelled());
    assert_eq!(
        driver.abort_attempt(&attempt).await.unwrap(),
        StreamUploadCleanup::Cleaned
    );
    assert!(
        peer.requests()
            .iter()
            .all(|r| r.method != "DELETE" || r.path == "/upload/drive/v3/files")
    );
    peer.stop().await;
}

#[tokio::test]
async fn short_pages_are_followed_before_proving_uniqueness_or_absence() {
    let peer = HttpFixture::new([
        HttpReply::json(200, json!({"files":[],"nextPageToken":"page-2"})),
        HttpReply::json(
            200,
            json!({"files":[file("owned")],"nextPageToken":"page-3"}),
        ),
        files(vec![]),
    ])
    .await;
    assert!(driver(&peer).exists("files/key").await.unwrap());
    let requests = peer.requests();
    assert!(!requests[0].query.contains_key("pageToken"));
    assert_eq!(requests[1].query["pageToken"], "page-2");
    assert_eq!(requests[2].query["pageToken"], "page-3");
    assert!(
        requests
            .iter()
            .all(|r| r.query["corpora"] == "drive" && r.query["driveId"] == "shared")
    );
    peer.stop().await;
}

#[tokio::test]
async fn ambiguous_or_incomplete_searches_cannot_mutate_any_object() {
    let cases = [
        (
            vec![files(vec![file("one"), file("two")])],
            StorageErrorKind::Misconfigured,
        ),
        (
            vec![
                HttpReply::json(200, json!({"files":[file("one")],"nextPageToken":"two"})),
                files(vec![file("two")]),
            ],
            StorageErrorKind::Misconfigured,
        ),
        (
            vec![HttpReply::json(
                200,
                json!({"files":[],"incompleteSearch":true}),
            )],
            StorageErrorKind::Transient,
        ),
        (
            vec![
                HttpReply::json(200, json!({"files":[],"nextPageToken":"same"})),
                HttpReply::json(200, json!({"files":[],"nextPageToken":"same"})),
            ],
            StorageErrorKind::Transient,
        ),
        (vec![files(vec![file("")])], StorageErrorKind::Transient),
    ];
    for (replies, kind) in cases {
        let peer = HttpFixture::new(replies).await;
        assert_eq!(
            driver(&peer).delete("files/key").await.unwrap_err().kind(),
            kind
        );
        assert!(peer.requests().iter().all(|r| r.method == "GET"));
        peer.stop().await;
    }
    let replies = (0..MAX_LOOKUP_PAGES)
        .map(|n| HttpReply::json(200, json!({"files":[],"nextPageToken":format!("page-{n}")})));
    let peer = HttpFixture::new(replies).await;
    assert_eq!(
        driver(&peer)
            .put("files/key", b"x")
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Transient
    );
    assert_eq!(peer.requests().len(), MAX_LOOKUP_PAGES);
    assert!(peer.requests().iter().all(|r| r.method == "GET"));
    peer.stop().await;
}

#[tokio::test]
async fn root_validation_rejects_wrong_type_drive_and_permissions() {
    for (body, kind) in [
        (
            json!({"id":"pool","mimeType":"application/octet-stream","capabilities":{"canAddChildren":true}}),
            StorageErrorKind::Misconfigured,
        ),
        (
            json!({"id":"pool","mimeType":"application/vnd.google-apps.folder","driveId":"other","capabilities":{"canAddChildren":true}}),
            StorageErrorKind::Misconfigured,
        ),
        (
            json!({"id":"pool","mimeType":"application/vnd.google-apps.folder","driveId":"shared","capabilities":{"canAddChildren":false}}),
            StorageErrorKind::Permission,
        ),
        (
            json!({"id":"pool","mimeType":"application/vnd.google-apps.folder","driveId":"shared"}),
            StorageErrorKind::Permission,
        ),
    ] {
        let peer = HttpFixture::new([HttpReply::json(200, body)]).await;
        assert_eq!(
            driver(&peer).validate_root().await.unwrap_err().kind(),
            kind
        );
        assert_eq!(peer.requests()[0].query["supportsAllDrives"], "true");
        peer.stop().await;
    }
}

#[tokio::test]
async fn appdata_and_account_roots_use_their_own_spaces_and_parent_scope() {
    for (appdata, root) in [(true, "appDataFolder"), (false, "root")] {
        let peer = HttpFixture::new([files(vec![])]).await;
        let mut driver = driver(&peer);
        driver.root_folder_id = None;
        driver.shared_drive_id = None;
        driver.app_data_folder = appdata;
        assert!(!driver.exists("files/key").await.unwrap());
        let req = &peer.requests()[0];
        assert_eq!(
            req.query["spaces"],
            if appdata { "appDataFolder" } else { "drive" }
        );
        assert!(req.query["q"].contains(&format!("'{root}' in parents")));
        assert!(!req.query.contains_key("driveId"));
        assert!(!req.query.contains_key("corpora"));
        peer.stop().await;
    }
    let peer = HttpFixture::new([files(vec![])]).await;
    let mut driver = driver(&peer);
    driver.root_folder_id = None;
    driver.shared_drive_id = None;
    driver.app_data_folder = true;
    assert_eq!(driver.validate_root().await.unwrap().0, "appDataFolder");
    assert_eq!(peer.requests()[0].query["spaces"], "appDataFolder");
    peer.stop().await;
    assert!(
        GoogleDriveDriver::new(
            Arc::new(Token),
            Some("pool".into()),
            None,
            true,
            String::new(),
            std::time::Duration::from_secs(10)
        )
        .is_err()
    );
}

#[test]
fn logical_keys_enforce_utf8_bytes_and_session_urls_enforce_origin_and_shape() {
    let driver = GoogleDriveDriver::new(
        Arc::new(Token),
        None,
        None,
        false,
        String::new(),
        std::time::Duration::from_secs(10),
    )
    .unwrap();
    let max = 124 - APP_KEY.len();
    driver
        .object_query(&"x".repeat(max), driver.root())
        .unwrap();
    assert!(
        driver
            .object_query(&"x".repeat(max + 1), driver.root())
            .is_err()
    );
    assert!(
        driver
            .object_query(&"猫".repeat(max / 3 + 1), driver.root())
            .is_err()
    );
    for path in ["", "a\0b", "a\rb", "a\nb", "a\tb"] {
        assert!(driver.object_query(path, driver.root()).is_err());
    }
    for url in [
        "https://www.googleapis.com.evil.test/upload/drive/v3/files?upload_id=x",
        "https://www.googleapis.com:444/upload/drive/v3/files?upload_id=x",
        "https://user:password@www.googleapis.com/upload/drive/v3/files?upload_id=x",
        "https://www.googleapis.com/upload/drive/v3/files?upload_id=x#fragment",
        "https://www.googleapis.com/upload/drive/v3/files?upload_id=",
        "https://www.googleapis.com/upload/drive/v3/files?upload_id=x&upload_id=y",
        "https://www.googleapis.com/upload/drive/v3/files?upload_id=x&uploadType=media",
        "https://www.googleapis.com/upload/drive/v3/files/../else?upload_id=x",
        "https://www.googleapis.com/upload/drive/v3/files/file/child?upload_id=x",
    ] {
        assert!(driver.validate_session_url(url).is_err(), "accepted {url}");
    }
    driver
        .validate_session_url(
            "https://www.googleapis.com/upload/drive/v3/files/file_ID-1?upload_id=x",
        )
        .unwrap();
}

#[tokio::test]
async fn untrusted_sessions_and_invalid_fragments_make_no_provider_requests() {
    let peer = HttpFixture::new([]).await;
    let driver = driver(&peer);
    for (start, total, size) in [
        (0, 1, -1),
        (0, 1, (CHUNK + 1) as i64),
        (0, 4, 0),
        (0, 4, 1),
        (u64::MAX, u64::MAX, 1),
        (4, 4, 1),
    ] {
        let err = driver
            .upload_session_fragment_reader(
                &session(&peer),
                start,
                total,
                Box::new(Cursor::new(vec![
                    0;
                    usize::try_from(size.max(0))
                        .unwrap()
                        .min(CHUNK + 1)
                ])),
                size,
            )
            .await
            .unwrap_err();
        assert_eq!(err.kind(), StorageErrorKind::Precondition);
    }
    assert!(
        driver
            .query_upload_session("https://attacker.test/?upload_id=x")
            .await
            .is_err()
    );
    assert!(
        driver
            .abort_upload_session("https://attacker.test/?upload_id=x")
            .await
            .is_err()
    );
    assert!(
        driver
            .upload_session_fragment_reader(
                "https://attacker.test/?upload_id=x",
                0,
                1,
                Box::new(tokio::io::repeat(0)),
                1
            )
            .await
            .is_err()
    );
    assert!(
        driver
            .put_reader("files/key", Box::new(tokio::io::empty()), -1)
            .await
            .is_err()
    );
    assert!(peer.requests().is_empty());
    peer.stop().await;
}

#[tokio::test]
async fn upload_acknowledgements_cannot_claim_unsent_or_premature_completion() {
    for reply in [
        HttpReply::text(200, ""),
        HttpReply::text(308, "").header("range", "bytes=1-2"),
        HttpReply::text(308, "").header("range", "bytes=0-18446744073709551615"),
        HttpReply::text(308, "").header("range", format!("bytes=0-{ALIGNMENT}")),
    ] {
        let peer = HttpFixture::new([reply]).await;
        assert_eq!(
            driver(&peer)
                .upload_session_fragment_reader(
                    &session(&peer),
                    0,
                    ALIGNMENT as u64 + 1,
                    Box::new(Cursor::new(vec![0; ALIGNMENT])),
                    ALIGNMENT as i64
                )
                .await
                .unwrap_err()
                .kind(),
            StorageErrorKind::Transient
        );
        peer.stop().await;
    }
    for (header, expected) in [(None, "0-"), (Some("bytes=0-42"), "43-")] {
        let mut reply = HttpReply::text(308, "");
        if let Some(header) = header {
            reply = reply.header("range", header);
        }
        let peer = HttpFixture::new([reply]).await;
        assert_eq!(
            driver(&peer)
                .upload_session_fragment_reader(
                    &session(&peer),
                    0,
                    ALIGNMENT as u64 + 1,
                    Box::new(Cursor::new(vec![0; ALIGNMENT])),
                    ALIGNMENT as i64
                )
                .await
                .unwrap()
                .next_expected_ranges,
            [expected]
        );
        peer.stop().await;
    }
}

#[tokio::test]
async fn status_expiry_and_abort_responses_follow_the_resumable_contract() {
    for (reply, expected) in [
        (HttpReply::text(200, ""), vec![]),
        (HttpReply::text(201, ""), vec![]),
        (HttpReply::text(308, ""), vec!["0-"]),
        (
            HttpReply::text(308, "").header("range", "bytes=0-42"),
            vec!["43-"],
        ),
    ] {
        let peer = HttpFixture::new([reply]).await;
        assert_eq!(
            driver(&peer)
                .query_upload_session(&session(&peer))
                .await
                .unwrap()
                .next_expected_ranges,
            expected
        );
        let req = &peer.requests()[0];
        assert_eq!(req.method, "PUT");
        assert_eq!(req.headers["content-range"], "bytes */*");
        assert!(req.body.is_empty());
        peer.stop().await;
    }
    for (status, kind) in [
        (404, StorageErrorKind::NotFound),
        (410, StorageErrorKind::NotFound),
        (429, StorageErrorKind::RateLimited),
        (503, StorageErrorKind::Transient),
    ] {
        let peer = HttpFixture::new([HttpReply::text(status, "")]).await;
        assert_eq!(
            driver(&peer)
                .query_upload_session(&session(&peer))
                .await
                .unwrap_err()
                .kind(),
            kind
        );
        peer.stop().await;
    }
    for status in [200, 204, 404, 410, 499] {
        let peer = HttpFixture::new([HttpReply::text(status, "")]).await;
        driver(&peer)
            .abort_upload_session(&session(&peer))
            .await
            .unwrap();
        peer.stop().await;
    }
}

#[tokio::test]
async fn ignored_ranges_invalid_metadata_and_provider_errors_remain_typed_and_redacted() {
    let peer = HttpFixture::new([files(vec![file("owned")]), HttpReply::text(200, "abcdef")]).await;
    assert_eq!(
        driver(&peer)
            .get_range("files/key", 2, Some(2))
            .await
            .err()
            .unwrap()
            .kind(),
        StorageErrorKind::Transient
    );
    assert_eq!(peer.requests()[1].headers["range"], "bytes=2-3");
    peer.stop().await;
    for size in [
        json!(null),
        json!("-1"),
        json!("not-a-size"),
        json!("18446744073709551616"),
    ] {
        let peer = HttpFixture::new([files(vec![json!({"id":"owned","size":size})])]).await;
        assert_eq!(
            driver(&peer)
                .metadata("files/key")
                .await
                .unwrap_err()
                .kind(),
            StorageErrorKind::Transient
        );
        peer.stop().await;
    }
    for (status, reason, kind) in [
        (401, "authError", StorageErrorKind::Auth),
        (
            403,
            "insufficientFilePermissions",
            StorageErrorKind::Permission,
        ),
        (403, "userRateLimitExceeded", StorageErrorKind::RateLimited),
        (403, "rateLimitExceeded", StorageErrorKind::RateLimited),
        (403, "dailyLimitExceeded", StorageErrorKind::RateLimited),
        (412, "conditionNotMet", StorageErrorKind::Precondition),
        (416, "invalidRange", StorageErrorKind::Precondition),
        (500, "backendError", StorageErrorKind::Transient),
    ] {
        let peer = HttpFixture::new([HttpReply::json(
            status,
            json!({"error":{"message":"administrator-secret-token","errors":[{"reason":reason}]}}),
        )])
        .await;
        let err = driver(&peer).exists("files/key").await.unwrap_err();
        assert_eq!(err.kind(), kind);
        assert!(!err.to_string().contains("administrator-secret-token"));
        peer.stop().await;
    }
}

#[tokio::test]
async fn capacity_uses_total_account_usage_instead_of_drive_only_usage() {
    use aster_drive_storage::StorageCapacityStatus;
    for (limit, usage, available) in [
        ("1000", "250", 750),
        ("1000", "1200", 0),
        ("0", "0", 0),
        ("9223372036854775807", "0", i64::MAX),
    ] {
        let peer=HttpFixture::new([HttpReply::json(200,json!({"storageQuota":{"limit":limit,"usage":usage,"usageInDrive":"1","usageInDriveTrash":"0"}}))]).await;
        let mut driver = driver(&peer);
        driver.shared_drive_id = None;
        let capacity = driver.capacity_info().await.unwrap();
        assert_eq!(capacity.status, StorageCapacityStatus::Supported);
        assert_eq!(capacity.available_bytes, Some(available));
        assert_eq!(capacity.total_bytes, Some(limit.parse().unwrap()));
        assert_eq!(capacity.used_bytes, Some(usage.parse().unwrap()));
        assert_eq!(capacity.source, "google_drive_account");
        assert_eq!(peer.requests()[0].path, "/drive/v3/about");
        assert_eq!(
            peer.requests()[0].query["fields"],
            "storageQuota(limit,usage)"
        );
        peer.stop().await;
    }
}

#[tokio::test]
async fn unbounded_or_unobservable_capacity_is_never_reported_as_zero_free_space() {
    use aster_drive_storage::StorageCapacityStatus;
    for (body, status) in [
        (
            json!({"storageQuota":{"usage":"25"}}),
            StorageCapacityStatus::Unsupported,
        ),
        (
            json!({"storageQuota":{}}),
            StorageCapacityStatus::Unsupported,
        ),
        (
            json!({"storageQuota":{"limit":"1000"}}),
            StorageCapacityStatus::Unavailable,
        ),
        (json!({}), StorageCapacityStatus::Unavailable),
    ] {
        let peer = HttpFixture::new([HttpReply::json(200, body)]).await;
        let mut driver = driver(&peer);
        driver.shared_drive_id = None;
        let capacity = driver.capacity_info().await.unwrap();
        assert_eq!(capacity.status, status);
        assert!(capacity.available_bytes.is_none());
        peer.stop().await;
    }
    // A foreign shared drive is not bounded by this administrator's My Drive quota.
    let peer = HttpFixture::new([]).await;
    let capacity = driver(&peer).capacity_info().await.unwrap();
    assert_eq!(capacity.status, StorageCapacityStatus::Unsupported);
    assert_eq!(capacity.source, "google_drive_shared_drive");
    assert!(peer.requests().is_empty());
    peer.stop().await;
}

#[tokio::test]
async fn capacity_rejects_malformed_numbers_and_preserves_provider_error_kinds() {
    for value in [
        json!("-1"),
        json!("not-bytes"),
        json!("9223372036854775808"),
        json!(10),
    ] {
        for field in ["limit", "usage"] {
            let mut quota = json!({"limit":"100","usage":"20"});
            quota[field] = value.clone();
            let peer =
                HttpFixture::new([HttpReply::json(200, json!({"storageQuota":quota}))]).await;
            let mut driver = driver(&peer);
            driver.shared_drive_id = None;
            assert_eq!(
                driver.capacity_info().await.unwrap_err().kind(),
                StorageErrorKind::Transient
            );
            peer.stop().await;
        }
    }
    for (status, kind) in [
        (401, StorageErrorKind::Auth),
        (403, StorageErrorKind::Permission),
        (429, StorageErrorKind::RateLimited),
        (503, StorageErrorKind::Transient),
    ] {
        let peer = HttpFixture::new([HttpReply::text(status, "private-secret")]).await;
        let mut driver = driver(&peer);
        driver.shared_drive_id = None;
        let error = driver.capacity_info().await.unwrap_err();
        assert_eq!(error.kind(), kind);
        assert!(!error.to_string().contains("private-secret"));
        peer.stop().await;
    }
}

#[test]
fn capacity_probe_policy_uses_the_configured_network_budget() {
    let driver = GoogleDriveDriver::new(
        Arc::new(Token),
        None,
        None,
        false,
        String::new(),
        Duration::from_secs(17),
    )
    .unwrap();
    let policy = driver.capacity_probe_policy();
    assert_eq!(policy.probe_timeout, Duration::from_secs(17));
    assert_eq!(policy.fresh_for, Duration::from_secs(30));
    assert_eq!(policy.stale_for, Duration::from_secs(300));
    policy.validate().unwrap();
}
