//! Exercise private dispatch with loopback fixtures; public execute must still reject loopback.
use super::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn transport(limits: PluginLimits, cancellation: CancellationToken) -> PluginTransport {
    PluginTransport::new(
        NetworkGrant::approved_origin("https://plugin.example").unwrap(),
        limits,
        cancellation,
    )
    .unwrap()
}
fn request(url: &str) -> PluginHttpRequest {
    PluginHttpRequest {
        method: reqwest::Method::POST,
        url: url.into(),
        headers: Default::default(),
        body: Vec::new(),
    }
}

async fn endpoint(
    response: &'static [u8],
    delay: Duration,
) -> (Url, tokio::task::JoinHandle<String>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = Url::parse(&format!(
        "http://{}/hook?key=private-query",
        listener.local_addr().unwrap()
    ))
    .unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0; 2048];
            let len = socket.read(&mut chunk).await.unwrap();
            assert!(len > 0);
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
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + length {
                    break;
                }
            }
        }
        tokio::time::sleep(delay).await;
        let _ = socket.write_all(response).await;
        String::from_utf8(bytes).unwrap()
    });
    (url, server)
}

#[tokio::test]
async fn dispatch_preserves_body_headers_and_product_identity_without_retaining_credentials() {
    for identity in [None, Some("Host/custom")] {
        let transport = transport(PluginLimits::default(), CancellationToken::new());
        let transport = match identity {
            Some(identity) => transport.user_agent(identity),
            None => transport,
        };
        let (url, server) = endpoint(b"HTTP/1.1 202 Accepted\r\nContent-Length: 4\r\nX-Receipt: yes\r\nConnection: close\r\n\r\nbody", Duration::ZERO).await;
        let host = url.host_str().unwrap();
        let addr = url.socket_addrs(|| None).unwrap();
        let client = transport.client_for_destination(host, &addr).unwrap();
        let mut input = request(url.as_str());
        input
            .headers
            .insert("authorization", "Bearer scoped-token".parse().unwrap());
        input
            .headers
            .insert("content-type", "application/json".parse().unwrap());
        input.body = br#"{"event":"file.created"}"#.to_vec();
        let response = transport
            .within_budget(transport.send(&client, input, url))
            .await
            .unwrap();
        assert_eq!(response.status, 202);
        assert_eq!(response.headers["x-receipt"], "yes");
        assert_eq!(response.body, b"body");
        let wire = server.await.unwrap();
        assert!(wire.starts_with("POST /hook?key=private-query HTTP/1.1"));
        assert!(wire.contains("Bearer scoped-token"));
        assert!(wire.ends_with(r#"{"event":"file.created"}"#));
        let actual = wire
            .lines()
            .find_map(|line| {
                line.split_once(':')
                    .filter(|(name, _)| name.eq_ignore_ascii_case("user-agent"))
                    .map(|(_, value)| value.trim())
            })
            .unwrap();
        assert_eq!(actual, identity.unwrap_or(crate::OUTBOUND_HTTP_USER_AGENT));
    }
}

#[tokio::test]
async fn dispatcher_errors_are_redacted_and_distinguish_transport_timeout_and_body_failure() {
    let transport = transport(PluginLimits::default(), CancellationToken::new());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = Url::parse(&format!(
        "http://{}/private-token",
        listener.local_addr().unwrap()
    ))
    .unwrap();
    drop(listener);
    let client = ClientConfig::new(Profile::Plugin)
        .no_proxy()
        .build()
        .unwrap();
    let error = transport
        .send(&client, request(url.as_str()), url)
        .await
        .err()
        .unwrap();
    assert_eq!(error, PluginTransportError::Transport);
    assert!(!format!("{error:?}: {error}").contains("private-token"));

    let (url, server) = endpoint(b"", Duration::from_millis(150)).await;
    let client = ClientConfig::new(Profile::Plugin)
        .no_proxy()
        .timeout(Duration::from_millis(25))
        .build()
        .unwrap();
    assert_eq!(
        transport
            .send(&client, request(url.as_str()), url)
            .await
            .err(),
        Some(PluginTransportError::Timeout)
    );
    server.await.unwrap();

    let (url, server) = endpoint(
        b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nx",
        Duration::ZERO,
    )
    .await;
    let client = ClientConfig::new(Profile::Plugin)
        .no_proxy()
        .build()
        .unwrap();
    assert_eq!(
        transport
            .send(&client, request(url.as_str()), url)
            .await
            .err(),
        Some(PluginTransportError::Transport)
    );
    server.await.unwrap();

    let bad_identity = transport.user_agent("bad\r\nua");
    assert_eq!(
        bad_identity
            .client_for_destination("localhost", &[])
            .err()
            .unwrap(),
        PluginTransportError::Transport
    );
}

struct Dropped(Arc<AtomicBool>);
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn host_deadline_and_cancellation_drop_an_inflight_exchange() {
    for cancelled in [false, true] {
        let cancellation = CancellationToken::new();
        let transport = transport(
            PluginLimits {
                timeout: Duration::from_millis(20),
                ..Default::default()
            },
            cancellation.clone(),
        );
        let dropped = Arc::new(AtomicBool::new(false));
        let guard = Dropped(dropped.clone());
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let exchange = async move {
            let _guard = guard;
            started_tx.send(()).unwrap();
            std::future::pending::<Result<(), PluginTransportError>>().await
        };
        let cancel = tokio::spawn(async move {
            started_rx.await.unwrap();
            if cancelled {
                cancellation.cancel();
            }
        });
        let result = transport.within_budget(exchange).await;
        assert_eq!(
            result,
            Err(if cancelled {
                PluginTransportError::Cancelled
            } else {
                PluginTransportError::Timeout
            })
        );
        assert!(dropped.load(Ordering::SeqCst));
        cancel.await.unwrap();
    }
}

#[tokio::test]
async fn public_entrypoint_rejects_domain_and_ipv6_private_destinations_before_connecting() {
    for origin in ["https://localhost", "https://[::1]"] {
        let transport = PluginTransport::new(
            NetworkGrant::approved_origin(origin).unwrap(),
            PluginLimits::default(),
            CancellationToken::new(),
        )
        .unwrap();
        assert_eq!(
            transport.execute(request(origin)).await.err(),
            Some(PluginTransportError::Denied)
        );
    }
    let transport = transport(PluginLimits::default(), CancellationToken::new());
    assert_eq!(
        transport.execute(request("bad URL")).await.err(),
        Some(PluginTransportError::Denied)
    );
    assert_eq!(
        transport
            .exchange(
                request("data:text/plain,body"),
                Url::parse("data:text/plain,body").unwrap()
            )
            .await
            .err(),
        Some(PluginTransportError::Denied)
    );
}

#[tokio::test]
async fn grant_budgets_and_header_limits_fail_before_any_provider_io() {
    for limits in [
        PluginLimits {
            request_bytes: 0,
            ..Default::default()
        },
        PluginLimits {
            response_bytes: 0,
            ..Default::default()
        },
        PluginLimits {
            request_bytes: 16 * 1024 * 1024 + 1,
            ..Default::default()
        },
        PluginLimits {
            response_bytes: 16 * 1024 * 1024 + 1,
            ..Default::default()
        },
        PluginLimits {
            timeout: Duration::ZERO,
            ..Default::default()
        },
        PluginLimits {
            timeout: Duration::from_secs(61),
            ..Default::default()
        },
    ] {
        assert_eq!(
            PluginTransport::new(
                NetworkGrant::approved_origin("https://plugin.example").unwrap(),
                limits,
                CancellationToken::new()
            )
            .err(),
            Some(PluginTransportError::Limit)
        );
    }
    let transport = transport(
        PluginLimits {
            request_bytes: 2,
            ..Default::default()
        },
        CancellationToken::new(),
    );
    let url = Url::parse("https://plugin.example/hook").unwrap();
    let mut input = request(url.as_str());
    input.body = b"12".to_vec();
    assert!(transport.validate_request(&input, &url).is_ok());
    for name in [
        "host",
        "connection",
        "proxy-authorization",
        "proxy-connection",
        "transfer-encoding",
        "content-length",
        "upgrade",
        "te",
        "trailer",
        "user-agent",
    ] {
        let mut input = request(url.as_str());
        input.headers.insert(
            reqwest::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            "value".parse().unwrap(),
        );
        assert_eq!(
            transport.execute(input).await.err(),
            Some(PluginTransportError::Denied)
        );
    }
    let mut input = request(url.as_str());
    for n in 0..65 {
        input.headers.insert(
            reqwest::header::HeaderName::from_bytes(format!("x-{n}").as_bytes()).unwrap(),
            "v".parse().unwrap(),
        );
    }
    assert_eq!(
        transport.execute(input).await.err(),
        Some(PluginTransportError::Limit)
    );
    let mut input = request(url.as_str());
    input
        .headers
        .insert("x-overflow", "x".repeat(16 * 1024).parse().unwrap());
    assert_eq!(
        transport.execute(input).await.err(),
        Some(PluginTransportError::Limit)
    );
}

#[tokio::test]
async fn approved_public_literal_reaches_pool_validation_without_sending_a_request() {
    // A malformed UA fails client construction before transport. This exercises the real public
    // address authorization path without contacting an external provider or permitting loopback.
    let transport = PluginTransport::new(
        NetworkGrant::approved_origin("https://1.1.1.1").unwrap(),
        PluginLimits::default(),
        CancellationToken::new(),
    )
    .unwrap()
    .user_agent("bad\r\nidentity");
    assert_eq!(
        transport
            .execute(request("https://1.1.1.1/hook"))
            .await
            .err(),
        Some(PluginTransportError::Transport)
    );
}

#[tokio::test]
async fn conditional_get_preserves_304_and_cache_headers_without_authorizing_redirects() {
    let transport = transport(PluginLimits::default(), CancellationToken::new());
    let client = ClientConfig::new(Profile::Plugin)
        .no_proxy()
        .build()
        .unwrap();
    let (url,server)=endpoint(b"HTTP/1.1 304 Not Modified\r\nETag: \"unchanged\"\r\nCache-Control: private, max-age=60\r\nConnection: close\r\n\r\n",Duration::ZERO).await;
    let mut input = request(url.as_str());
    input.method = reqwest::Method::GET;
    input
        .headers
        .insert("if-none-match", "\"unchanged\"".parse().unwrap());
    let response = transport.send(&client, input, url).await.unwrap();
    assert_eq!(response.status, 304);
    assert!(response.body.is_empty());
    assert_eq!(response.headers["etag"], "\"unchanged\"");
    assert_eq!(response.headers["cache-control"], "private, max-age=60");
    assert!(
        server
            .await
            .unwrap()
            .contains("if-none-match: \"unchanged\"")
    );
    for method in [
        reqwest::Method::OPTIONS,
        reqwest::Method::CONNECT,
        reqwest::Method::TRACE,
        reqwest::Method::from_bytes(b"CUSTOM").unwrap(),
    ] {
        let mut input = request("https://plugin.example/hook");
        input.method = method;
        assert_eq!(
            transport.execute(input).await.err(),
            Some(PluginTransportError::Denied)
        );
    }
}
