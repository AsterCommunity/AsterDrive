//! Real fragment transport with scaled deadlines; control calls keep a separate total deadline.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

enum Reply {
    Accepted,
    Complete,
    StallHeaders,
    StallBody,
    TrickleBody,
    ErrorStallBody,
}
async fn peer(reply: Reply) -> (String, tokio::task::JoinHandle<String>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://{}/preauthorized-session?token=private-url",
        listener.local_addr().unwrap()
    );
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0; 4096];
            let len = socket.read(&mut chunk).await.unwrap();
            if len == 0 {
                return String::from_utf8(bytes).unwrap();
            }
            bytes.extend_from_slice(&chunk[..len]);
            let wire = String::from_utf8_lossy(&bytes);
            if let Some(end) = wire.find("\r\n\r\n") {
                let length = wire[..end]
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
        let body = br#"{"nextExpectedRanges":["8-"]}"#;
        match reply {
            Reply::Accepted => {
                let _=socket.write_all(format!("HTTP/1.1 202 Accepted\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).as_bytes()).await;
                let _ = socket.write_all(body).await;
            }
            Reply::Complete => {
                let _ = socket
                    .write_all(
                        b"HTTP/1.1 201 Created\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await;
            }
            Reply::StallHeaders => {
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            Reply::StallBody | Reply::ErrorStallBody => {
                let status = if matches!(reply, Reply::ErrorStallBody) {
                    "503 Service Unavailable"
                } else {
                    "202 Accepted"
                };
                let _=socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).as_bytes()).await;
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            Reply::TrickleBody => {
                let _=socket.write_all(format!("HTTP/1.1 202 Accepted\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).as_bytes()).await;
                for byte in body {
                    if socket.write_all(&[*byte]).await.is_err() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(30)).await;
                }
            }
        }
        String::from_utf8(bytes).unwrap()
    });
    (url, task)
}
fn client(idle: Duration) -> MicrosoftGraphClient {
    let mut client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(
        "http://graph.invalid",
        "private-graph-token",
    ))
    .unwrap();
    client.control_client =
        aster_drive_outbound::ClientConfig::new(aster_drive_outbound::Profile::ProviderControl)
            .timeout(Duration::from_millis(20))
            .no_proxy()
            .build()
            .unwrap();
    client.upload_client =
        aster_drive_outbound::ClientConfig::new(aster_drive_outbound::Profile::ProviderUpload)
            .no_proxy()
            .build()
            .unwrap();
    client.upload_idle_timeout = idle;
    client
}
fn slow_reader(delay: Duration) -> Box<dyn AsyncRead + Unpin + Send + Sync> {
    let (mut writer, reader) = tokio::io::duplex(1);
    tokio::spawn(async move {
        for byte in b"fragment" {
            tokio::time::sleep(delay).await;
            if writer.write_all(&[*byte]).await.is_err() {
                break;
            }
        }
    });
    Box::new(reader)
}

#[tokio::test]
async fn fragment_progress_can_outlive_control_deadline_and_preserves_provider_wire_contract() {
    let client = client(Duration::from_millis(150));
    let (url, server) = peer(Reply::Accepted).await;
    let started = std::time::Instant::now();
    let result = client
        .upload_session_fragment_reader(&url, 0, 16, slow_reader(Duration::from_millis(35)), 8)
        .await
        .unwrap();
    assert!(started.elapsed() > Duration::from_millis(200));
    assert!(!result.completed);
    assert_eq!(result.next_expected_ranges, vec!["8-"]);
    let wire = server.await.unwrap();
    let (headers, body) = wire.split_once("\r\n\r\n").unwrap();
    let headers = headers.to_ascii_lowercase();
    assert!(headers.starts_with("put /preauthorized-session?token=private-url http/1.1"));
    assert!(headers.contains("content-length: 8"));
    assert!(headers.contains("content-range: bytes 0-7/16"));
    assert!(headers.contains("content-type: application/octet-stream"));
    assert!(!headers.contains("authorization:"));
    assert!(headers.contains(&aster_drive_outbound::OUTBOUND_HTTP_USER_AGENT.to_ascii_lowercase()));
    assert_eq!(body, "fragment");
    let (url, server) = peer(Reply::Complete).await;
    assert!(
        client
            .upload_session_fragment(&url, 8, 16, b"fragment".to_vec())
            .await
            .unwrap()
            .completed
    );
    server.await.unwrap();
}

#[tokio::test]
async fn stalled_upload_header_wait_and_acknowledgement_decode_each_have_a_deadline() {
    let client = client(Duration::from_millis(75));
    let (url, server) = peer(Reply::Complete).await;
    let error = client
        .upload_session_fragment_reader(&url, 0, 8, slow_reader(Duration::from_millis(300)), 8)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), StorageErrorKind::Transient);
    assert!(error.message().contains("stalled"));
    server.await.unwrap();
    for reply in [
        Reply::StallHeaders,
        Reply::StallBody,
        Reply::TrickleBody,
        Reply::ErrorStallBody,
    ] {
        let (url, server) = peer(reply).await;
        let error = client
            .upload_session_fragment(&url, 0, 8, b"fragment".to_vec())
            .await
            .unwrap_err();
        assert_eq!(error.kind(), StorageErrorKind::Transient);
        assert!(error.message().contains("timed out"), "{error}");
        assert!(!error.message().contains("private-url"));
        server.await.unwrap();
    }
}

struct PendingReader(Arc<AtomicBool>);
impl Drop for PendingReader {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
impl AsyncRead for PendingReader {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        _buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Pending
    }
}
#[tokio::test]
async fn caller_deadline_cancels_fragment_reader_before_the_upload_idle_limit() {
    let client = client(Duration::from_secs(1));
    let (url, server) = peer(Reply::Complete).await;
    let dropped = Arc::new(AtomicBool::new(false));
    let result = tokio::time::timeout(
        Duration::from_millis(30),
        client.upload_session_fragment_reader(
            &url,
            0,
            8,
            Box::new(PendingReader(dropped.clone())),
            8,
        ),
    )
    .await;
    assert!(result.is_err(), "caller deadline must remain authoritative");
    tokio::time::timeout(Duration::from_secs(1), async {
        while !dropped.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(1), server)
        .await
        .unwrap()
        .unwrap();
}
