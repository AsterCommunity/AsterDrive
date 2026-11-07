use super::*;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

enum Mode {
    Metadata,
    Content,
    Redirect(String),
    IdleContent,
    TruncatedContent,
}

async fn server(mode: Mode) -> (String, tokio::task::JoinHandle<String>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0; 2048];
            let len = socket.read(&mut chunk).await.unwrap();
            assert!(len > 0);
            bytes.extend_from_slice(&chunk[..len]);
            if bytes.windows(4).any(|v| v == b"\r\n\r\n") {
                break;
            }
        }
        match mode {
            Mode::TruncatedContent => {
                socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10000\r\nConnection: close\r\n\r\nshort").await.unwrap();
            }
            Mode::Metadata => {
                tokio::time::sleep(Duration::from_millis(150)).await;
                let _=socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n{\"id\":\"x\"}").await;
            }
            Mode::Content => {
                socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\n")
                    .await
                    .unwrap();
                for byte in b"body" {
                    tokio::time::sleep(Duration::from_millis(40)).await;
                    socket.write_all(&[*byte]).await.unwrap();
                }
            }
            Mode::IdleContent => {
                socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\n")
                    .await
                    .unwrap();
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
            Mode::Redirect(url) => {
                socket.write_all(format!("HTTP/1.1 302 Found\r\nLocation: {url}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
            }
        }
        String::from_utf8(bytes).unwrap()
    });
    (url, task)
}

fn graph(base: &str) -> MicrosoftGraphClient {
    let mut client =
        MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(base, "private-graph-token"))
            .unwrap();
    // Scale only control deadlines so a regression completes in milliseconds. Production uses
    // the same control profile with 60 seconds; the download profile retains its production timeouts. Only proxies are disabled for fixtures.
    client.control_client =
        aster_drive_outbound::ClientConfig::new(aster_drive_outbound::Profile::ProviderControl)
            .timeout(Duration::from_millis(30))
            .no_proxy()
            .build()
            .unwrap();
    client.download_client =
        aster_drive_outbound::ClientConfig::new(aster_drive_outbound::Profile::ProviderDownload)
            .no_proxy()
            .build()
            .unwrap();
    client
}

#[tokio::test]
async fn direct_content_can_outlive_control_deadline_without_weakening_metadata_limits() {
    let (url, content) = server(Mode::Content).await;
    let client = graph(&url);
    let mut body = Vec::new();
    client
        .get_stream("/content", None, None)
        .await
        .unwrap()
        .read_to_end(&mut body)
        .await
        .unwrap();
    assert_eq!(body, b"body");
    assert!(
        content
            .await
            .unwrap()
            .contains("Bearer private-graph-token")
    );
    let (url, metadata) = server(Mode::Metadata).await;
    let client = graph(&url);
    let error = client
        .get_drive_item_by_id("drive", "item")
        .await
        .unwrap_err();
    assert_eq!(error.kind(), StorageErrorKind::Transient);
    metadata.await.unwrap();
}

#[tokio::test]
async fn redirected_transport_and_body_failures_never_expose_capability_urls() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let target = format!("http://{address}/opaque-capability?unlisted_key=private-marker");
    let (base, redirect) = server(Mode::Redirect(target)).await;
    let client = graph(&base);
    let error = client.get_bytes("/content").await.unwrap_err().to_string();
    assert!(!error.contains("private-marker"), "{error}");
    assert!(!error.contains("opaque-capability"), "{error}");
    redirect.await.unwrap();

    let (target, content) = server(Mode::TruncatedContent).await;
    let (base, redirect) = server(Mode::Redirect(format!(
        "{target}/opaque-capability?unlisted_key=private-marker"
    )))
    .await;
    let mut stream = graph(&base)
        .get_stream("/content", None, None)
        .await
        .unwrap();
    let error = stream
        .read_to_end(&mut Vec::new())
        .await
        .unwrap_err()
        .to_string();
    assert!(!error.contains("private-marker"), "{error}");
    assert!(!error.contains("opaque-capability"), "{error}");
    redirect.await.unwrap();
    content.await.unwrap();
}

#[tokio::test]
async fn redirect_content_keeps_download_profile_range_and_credential_isolation() {
    let (target, content) = server(Mode::Content).await;
    let (base, redirect) = server(Mode::Redirect(target)).await;
    let client = graph(&base);
    let mut body = Vec::new();
    client
        .get_stream("/content", Some(1), Some(4))
        .await
        .unwrap()
        .read_to_end(&mut body)
        .await
        .unwrap();
    assert_eq!(body, b"body");
    let graph_request = redirect.await.unwrap().to_ascii_lowercase();
    assert!(graph_request.contains("authorization: bearer private-graph-token"));
    assert!(!graph_request.contains("range:"));
    let target_request = content.await.unwrap().to_ascii_lowercase();
    assert!(target_request.contains("range: bytes=1-4"));
    assert!(!target_request.contains("authorization:"));
    assert!(
        target_request.contains(
            aster_drive_outbound::OUTBOUND_HTTP_USER_AGENT
                .to_ascii_lowercase()
                .as_str()
        )
    );
}

#[tokio::test]
async fn stalled_content_is_still_bounded_by_download_idle_timeout() {
    let (url, server) = server(Mode::IdleContent).await;
    let mut client = graph(&url);
    client.download_client =
        aster_drive_outbound::ClientConfig::new(aster_drive_outbound::Profile::ProviderDownload)
            .read_timeout(Duration::from_millis(25))
            .no_proxy()
            .build()
            .unwrap();
    let mut reader = client.get_stream("/content", None, None).await.unwrap();
    assert!(reader.read_to_end(&mut Vec::new()).await.is_err());
    server.await.unwrap();
}
