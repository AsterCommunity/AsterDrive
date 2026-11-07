use super::*;
use aster_forge_http::{HttpObservation, HttpOutcome};
use std::sync::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Default)]
struct Events(Mutex<Vec<HttpObservation>>);

impl HttpObserver for Events {
    fn observe(&self, event: HttpObservation) {
        self.0.lock().unwrap().push(event);
    }
}

#[tokio::test]
async fn profile_observation_distinguishes_headers_from_eof_and_records_cancellation() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://{}/private-query?token=redacted",
        listener.local_addr().unwrap()
    );
    let (body_tx, body_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 2048];
        assert!(socket.read(&mut bytes).await.unwrap() > 0);
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        body_rx.await.unwrap();
        socket.write_all(b"body").await.unwrap();
    });
    let events = Arc::new(Events::default());
    let client = ClientConfig::new(Profile::ProviderDownload)
        .observer(events.clone())
        .no_proxy()
        .build()
        .unwrap();
    let response = client.get(&url).send().await.unwrap();
    {
        let events = events.0.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].status, Some(200));
        assert_eq!(events[0].outcome, HttpOutcome::Response);
        assert!(!format!("{:?}", events[0]).contains("private-query"));
    }
    body_tx.send(()).unwrap();
    assert_eq!(response.text().await.unwrap(), "body");
    task.await.unwrap();
    assert_eq!(
        events.0.lock().unwrap().len(),
        1,
        "body completion is owned by transfer metrics, not a second HTTP header observation"
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (received_tx, received_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 2048];
        assert!(socket.read(&mut bytes).await.unwrap() > 0);
        received_tx.send(()).unwrap();
        std::future::pending::<()>().await;
        drop(socket);
    });
    {
        let pending = client.get(url).send();
        tokio::pin!(pending);
        tokio::select! {response=&mut pending=>panic!("unexpected response {response:?}"), _=received_rx=>{}}
    }
    assert_eq!(events.0.lock().unwrap().len(), 2);
    assert_eq!(events.0.lock().unwrap()[1].outcome, HttpOutcome::Cancelled);
    server.abort();
    assert!(server.await.unwrap_err().is_cancelled());
}

#[tokio::test]
async fn profile_controls_total_and_idle_deadlines_and_streaming_opt_out() {
    for idle in [true, false] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 1024];
            assert!(socket.read(&mut bytes).await.unwrap() > 0);
            tokio::time::sleep(Duration::from_millis(100)).await;
            let _ = socket
                .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                .await;
        });
        let config = ClientConfig::new(Profile::InternalControl)
            .connect_timeout(Duration::from_secs(1))
            .no_proxy();
        let config = if idle {
            config
                .without_total_timeout()
                .read_timeout(Duration::from_millis(20))
        } else {
            config
                .without_read_timeout()
                .timeout(Duration::from_millis(20))
        };
        assert!(
            config
                .build()
                .unwrap()
                .get(url)
                .send()
                .await
                .unwrap_err()
                .is_timeout()
        );
        server.await.unwrap();
    }
    // Internal streaming and upload profiles must not impose either a total or a pre-header read deadline.
    for profile in [Profile::InternalStreaming, Profile::ProviderUpload] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 1024];
            assert!(socket.read(&mut bytes).await.unwrap() > 0);
            tokio::time::sleep(Duration::from_millis(80)).await;
            socket
                .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
        });
        let config = ClientConfig::new(profile);
        assert!(
            config.read_timeout.is_none(),
            "{profile:?} must leave upload/header timing to the protocol"
        );
        assert!(
            config.timeout.is_none(),
            "{profile:?} must not impose a whole-transfer deadline"
        );
        let client = config.no_proxy().build().unwrap();
        assert_eq!(client.get(url).send().await.unwrap().status(), 204);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn explicit_proxy_forwards_without_target_dns_but_pinning_overrides_proxy_in_both_orders() {
    for pinned in [false, true] {
        for pin_first in [false, true] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = [0; 2048];
                let len = socket.read(&mut bytes).await.unwrap();
                socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .await
                    .unwrap();
                String::from_utf8_lossy(&bytes[..len]).to_string()
            });
            let proxy = reqwest::Proxy::all(if pinned {
                "http://127.0.0.1:1".into()
            } else {
                format!("http://{addr}")
            })
            .unwrap();
            let config = ClientConfig::new(Profile::ProviderControl);
            let config = if pinned && pin_first {
                config
                    .resolve_to_addrs("provider.invalid", &[addr])
                    .proxy(proxy)
            } else if pinned {
                config
                    .proxy(proxy)
                    .resolve_to_addrs("provider.invalid", &[addr])
            } else {
                config.proxy(proxy)
            };
            let url = format!("http://provider.invalid:{}/endpoint", addr.port());
            assert_eq!(
                config
                    .build()
                    .unwrap()
                    .get(&url)
                    .send()
                    .await
                    .unwrap()
                    .status(),
                200
            );
            let wire = server.await.unwrap();
            assert!(wire.starts_with(if pinned {
                "GET /endpoint HTTP/1.1"
            } else {
                "GET http://provider.invalid:"
            }));
        }
    }
}

#[tokio::test]
async fn offline_download_pinning_ignores_environment_proxy() {
    const TARGET_ENV: &str = "ASTER_TEST_OFFLINE_PINNED_TARGET";
    #[expect(
        clippy::single_match,
        reason = "keep the child-process environment dispatch explicit"
    )]
    match std::env::var(TARGET_ENV) {
        Ok(target) => {
            let address = target.parse::<SocketAddr>().unwrap();
            let client = ClientConfig::new(Profile::OfflineDownload)
                .resolve_to_addrs("offline.invalid", &[address])
                .build()
                .unwrap();
            let response = client
                .get(format!("http://offline.invalid:{}/source", address.port()))
                .send()
                .await
                .unwrap();
            assert_eq!(response.text().await.unwrap(), "target");
            return;
        }
        Err(_) => {}
    }

    // A child process isolates proxy environment from parallel transport tests.
    let target = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_address = target.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = target.accept().await.unwrap();
        let mut input = [0; 2048];
        let read = socket.read(&mut input).await.unwrap();
        assert!(String::from_utf8_lossy(&input[..read]).starts_with("GET /source HTTP/1.1"));
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\ntarget")
            .await
            .unwrap();
    });
    let proxy = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_url = format!("http://{}", proxy.local_addr().unwrap());
    let proxy_hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let hits = proxy_hits.clone();
    let proxy_task = tokio::spawn(async move {
        let (mut socket, _) = proxy.accept().await.unwrap();
        hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let _ = socket
            .write_all(
                b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await;
    });
    let result = tokio::task::spawn_blocking(move || {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "policy_tests::offline_download_pinning_ignores_environment_proxy",
                "--nocapture",
            ])
            .env(TARGET_ENV, target_address.to_string())
            .env("NO_PROXY", "")
            .env("no_proxy", "");
        for key in [
            "HTTP_PROXY",
            "http_proxy",
            "HTTPS_PROXY",
            "https_proxy",
            "ALL_PROXY",
            "all_proxy",
        ] {
            child.env(key, &proxy_url);
        }
        child.output().unwrap()
    })
    .await
    .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    server.await.unwrap();
    assert_eq!(proxy_hits.load(std::sync::atomic::Ordering::SeqCst), 0);
    proxy_task.abort();
    assert!(proxy_task.await.unwrap_err().is_cancelled());
}

#[tokio::test]
async fn discovery_profile_returns_redirect_without_contacting_selected_target() {
    let redirected = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let destination = format!("http://{}/internal", redirected.local_addr().unwrap());
    let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let redirected_hits = hits.clone();
    let destination_server = tokio::spawn(async move {
        let (mut socket, _) = redirected.accept().await.unwrap();
        redirected_hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let _ = socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await;
    });
    let origin = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let source = format!("http://{}/discovery", origin.local_addr().unwrap());
    let redirect_location = destination.clone();
    let origin_server = tokio::spawn(async move {
        let (mut socket, _) = origin.accept().await.unwrap();
        let mut input = [0; 2048];
        assert!(socket.read(&mut input).await.unwrap() > 0);
        socket.write_all(format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: {redirect_location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
    });
    let response = ClientConfig::new(Profile::Discovery)
        .no_proxy()
        .build()
        .unwrap()
        .get(source)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 307);
    assert_eq!(
        response.headers().get(reqwest::header::LOCATION).unwrap(),
        destination.as_str()
    );
    origin_server.await.unwrap();
    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 0);
    destination_server.abort();
    assert!(destination_server.await.unwrap_err().is_cancelled());
}

#[tokio::test]
async fn extra_trust_root_does_not_disable_hostname_verification() {
    use tokio_rustls::rustls::{self, pki_types::PrivatePkcs8KeyDer};
    let cert = rcgen::generate_simple_self_signed(vec!["provider.test".into()]).unwrap();
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(
        vec![cert.cert.der().clone()],
        PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der()).into(),
    )
    .unwrap();
    let config = Arc::new(config);
    let root = reqwest::Certificate::from_der(cert.cert.der()).unwrap();
    for host in ["provider.test", "wrong.test"] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(config.clone());
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            if let Ok(mut socket) = acceptor.accept(socket).await {
                let mut bytes = [0; 2048];
                assert!(socket.read(&mut bytes).await.unwrap() > 0);
                socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .await
                    .unwrap();
                socket.shutdown().await.unwrap();
            }
        });
        let response = ClientConfig::new(Profile::ProviderControl)
            .add_root_certificate(root.clone())
            .resolve_to_addrs(host, &[addr])
            .build()
            .unwrap()
            .get(format!("https://{host}:{}/", addr.port()))
            .send()
            .await;
        assert_eq!(response.is_ok(), host == "provider.test");
        server.await.unwrap();
    }
}

#[test]
fn explicit_streaming_opt_out_clears_both_previously_configured_deadlines() {
    let config = ClientConfig::new(Profile::ProviderControl)
        .read_timeout(Duration::from_millis(10))
        .timeout(Duration::from_millis(10))
        .without_read_timeout()
        .without_total_timeout();
    assert!(config.read_timeout.is_none());
    assert!(config.timeout.is_none());
}
