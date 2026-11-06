//! Separate pooled profiles while keeping Google status/Range/session rules in the driver.
use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn driver(base: &str) -> GoogleDriveDriver {
    let mut driver = GoogleDriveDriver::new(
        Arc::new(Token),
        Some("pool".into()),
        None,
        false,
        String::new(),
        Duration::from_secs(10),
    )
    .unwrap();

    driver.api = format!("{base}/drive/v3");
    driver.upload = format!("{base}/upload/drive/v3/files");

    driver.control_client =
        aster_drive_outbound::ClientConfig::new(aster_drive_outbound::Profile::ProviderControl)
            .timeout(Duration::from_millis(25))
            .no_proxy()
            .build()
            .unwrap();
    driver.download_client =
        aster_drive_outbound::ClientConfig::new(aster_drive_outbound::Profile::ProviderDownload)
            .read_timeout(Duration::from_millis(250))
            .no_proxy()
            .build()
            .unwrap();
    driver.upload_client =
        aster_drive_outbound::ClientConfig::new(aster_drive_outbound::Profile::ProviderUpload)
            .no_proxy()
            .build()
            .unwrap();
    driver.upload_idle_timeout = Duration::from_millis(150);
    driver
}

async fn upload_peer(
    status: &'static str,
    body: &'static str,
    delay: Duration,
    stall_body: bool,
) -> (String, tokio::task::JoinHandle<String>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0; 8192];
            let len = socket.read(&mut chunk).await.unwrap();
            assert!(len > 0);
            bytes.extend_from_slice(&chunk[..len]);
            let source = String::from_utf8_lossy(&bytes);
            if let Some(end) = source.find("\r\n\r\n") {
                let length = source[..end]
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|value| value.parse::<usize>().ok())
                    })
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    break;
                }
            }
        }

        tokio::time::sleep(delay).await;
        let header = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nRange: bytes=0-262143\r\nLocation: http://127.0.0.1:1/never-follow\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = socket.write_all(header.as_bytes()).await;
        if stall_body {
            tokio::time::sleep(Duration::from_millis(300)).await;
        } else {
            let _ = socket.write_all(body.as_bytes()).await;
        }
        String::from_utf8(bytes).unwrap()
    });

    (base, server)
}

#[tokio::test]
async fn resumable_upload_reuses_upload_pool_and_decodes_308_instead_of_following_it() {
    let (base, server) = upload_peer(
        "308 Resume Incomplete",
        "",
        Duration::from_millis(80),
        false,
    )
    .await;
    let driver = driver(&base);
    let url = format!(
        "{}?uploadType=resumable&upload_id=private-session",
        driver.upload
    );
    let outcome = driver
        .upload_bytes(&url, 0, (ALIGNMENT * 2) as u64, vec![b'x'; ALIGNMENT])
        .await
        .unwrap();

    assert!(!outcome.completed);
    assert_eq!(outcome.next_expected_ranges, vec![format!("{ALIGNMENT}-")]);

    let request = server.await.unwrap();
    let (headers, body) = request.split_once("\r\n\r\n").unwrap();
    let headers = headers.to_ascii_lowercase();
    assert!(headers.starts_with(
        "put /upload/drive/v3/files?uploadtype=resumable&upload_id=private-session http/1.1"
    ));
    assert!(headers.contains("content-range: bytes 0-262143/524288"));
    assert!(headers.contains("authorization: bearer administrator-secret-token"));
    assert!(headers.contains(&aster_drive_outbound::OUTBOUND_HTTP_USER_AGENT.to_ascii_lowercase()));
    assert_eq!(body.len(), ALIGNMENT);
}

#[tokio::test]
async fn upload_wait_and_error_body_use_shared_progress_and_acknowledgement_deadlines() {
    for (status, stall_body, delay, expected) in [
        ("200 OK", false, Duration::from_millis(300), "stalled"),
        (
            "503 Service Unavailable",
            true,
            Duration::ZERO,
            "response timed out",
        ),
    ] {
        let (base, server) =
            upload_peer(status, "provider-private-message", delay, stall_body).await;
        let mut driver = driver(&base);
        driver.upload_idle_timeout = Duration::from_millis(50);
        let error = driver
            .upload_bytes(
                &format!("{}?upload_id=private-session", driver.upload),
                0,
                4,
                b"body".to_vec(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.kind(), StorageErrorKind::Transient);
        assert!(error.message().contains(expected), "{error}");
        assert!(!error.message().contains("private-session"));
        assert!(!error.message().contains("provider-private-message"));
        server.await.unwrap();
    }
}

#[tokio::test]
async fn metadata_deadline_does_not_cap_downloads_that_keep_yielding_bytes() {
    async fn handler(req: HttpRequest) -> HttpResponse {
        if req.query_string().contains("alt=media") {
            let bytes = futures::stream::unfold(0, async |index| {
                if index == 12 {
                    return None;
                }
                tokio::time::sleep(Duration::from_millis(80)).await;
                Some((
                    Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"x")),
                    index + 1,
                ))
            });
            return HttpResponse::Ok().streaming(bytes);
        }
        HttpResponse::Ok().json(serde_json::json!({"files":[{"id":"owned","size":"12"}]}))
    }

    #[derive(Default)]
    struct Outcomes(Mutex<Vec<aster_forge_http::HttpOutcome>>);

    impl aster_forge_http::HttpObserver for Outcomes {
        fn observe(&self, event: aster_forge_http::HttpObservation) {
            self.0.lock().unwrap().push(event.outcome);
        }
    }

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = HttpServer::new(|| App::new().default_service(web::to(handler)))
        .workers(1)
        .listen(listener)
        .unwrap()
        .run();
    let handle = server.handle();
    let task = tokio::spawn(server);
    let mut download_driver = driver(&base);
    // Allow cold local connection setup under parallel compilation; the streamed body still
    // outlives this control budget by almost a factor of two. A 25ms setup budget was flaky.
    download_driver.control_client =
        aster_drive_outbound::ClientConfig::new(aster_drive_outbound::Profile::ProviderControl)
            .timeout(Duration::from_millis(500))
            .no_proxy()
            .build()
            .unwrap();
    download_driver.download_client =
        aster_drive_outbound::ClientConfig::new(aster_drive_outbound::Profile::ProviderDownload)
            .read_timeout(Duration::from_secs(1))
            .no_proxy()
            .build()
            .unwrap();
    let mut bytes = Vec::new();
    download_driver
        .get_stream("object")
        .await
        .unwrap()
        .read_to_end(&mut bytes)
        .await
        .unwrap();
    assert_eq!(bytes, b"xxxxxxxxxxxx");
    handle.stop(true).await;
    task.await.unwrap().unwrap();

    let gate = Arc::new(crate::storage::test_support::ResponseGate::default());
    let peer = crate::storage::test_support::HttpFixture::new([
        crate::storage::test_support::HttpReply::json(200, serde_json::json!({"files":[]}))
            .gated(gate.clone()),
    ])
    .await;
    let outcomes = Arc::new(Outcomes::default());
    let mut driver = driver(&peer.base);
    driver.control_client =
        aster_drive_outbound::ClientConfig::new(aster_drive_outbound::Profile::ProviderControl)
            .timeout(Duration::from_millis(25))
            .no_proxy()
            .observer(outcomes.clone())
            .build()
            .unwrap();
    let error = tokio::time::timeout(Duration::from_secs(2), driver.metadata("object"))
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.kind(), StorageErrorKind::Transient);
    // reqwest's sanitized Display omits the timeout source; verify the typed observation.
    assert_eq!(
        *outcomes.0.lock().unwrap(),
        vec![aster_forge_http::HttpOutcome::Timeout]
    );
    gate.release.notify_one();
    peer.stop().await;
}
