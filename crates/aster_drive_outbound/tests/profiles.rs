use aster_drive_outbound::{ClientConfig, Profile};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn server(delay: Duration) -> (String, tokio::task::JoinHandle<String>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 4096];
        let len = socket.read(&mut bytes).await.unwrap();
        tokio::time::sleep(delay).await;
        let _=socket.write_all(b"HTTP/1.1 307 Temporary Redirect\r\nLocation: http://127.0.0.1:1/forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
        String::from_utf8_lossy(&bytes[..len]).to_string()
    });
    (url, task)
}

#[tokio::test]
async fn every_profile_uses_product_identity_and_leaves_redirect_to_protocol_owner() {
    for profile in [
        Profile::ProviderControl,
        Profile::ProviderDownload,
        Profile::ProviderUpload,
        Profile::InternalControl,
        Profile::InternalTransfer,
        Profile::InternalStreaming,
        Profile::Discovery,
        Profile::ExternalAuth,
        Profile::OfflineDownload,
        Profile::Plugin,
    ] {
        for identity in [None, Some("AsterDrive/custom-agent")] {
            let (url, task) = server(Duration::ZERO).await;
            let config = ClientConfig::new(profile);
            let config = match identity {
                Some(value) => config.user_agent(value),
                None => config,
            };
            let client = config.no_proxy().build().unwrap();
            assert_eq!(
                client.get(url).send().await.unwrap().status(),
                307,
                "{profile:?}"
            );
            let wire = task.await.unwrap();
            let actual = wire
                .lines()
                .find_map(|line| {
                    line.split_once(':')
                        .filter(|(name, _)| name.eq_ignore_ascii_case("user-agent"))
                        .map(|(_, value)| value.trim())
                })
                .unwrap();
            assert_eq!(
                actual,
                identity.unwrap_or(aster_drive_outbound::OUTBOUND_HTTP_USER_AGENT)
            );
        }
    }
}

#[tokio::test]
async fn control_deadline_and_upload_progress_owner_remain_distinct() {
    let (url, task) = server(Duration::from_millis(120)).await;
    let client = ClientConfig::new(Profile::ProviderControl)
        .user_agent("test")
        .no_proxy()
        .timeout(Duration::from_millis(25))
        .build()
        .unwrap();
    assert!(client.get(url).send().await.unwrap_err().is_timeout());
    task.await.unwrap();
    let (url, task) = server(Duration::from_millis(120)).await;
    let client = ClientConfig::new(Profile::ProviderDownload)
        .user_agent("test")
        .no_proxy()
        .read_timeout(Duration::from_millis(25))
        .build()
        .unwrap();
    assert!(client.get(url).send().await.unwrap_err().is_timeout());
    task.await.unwrap();
    let (url, task) = server(Duration::from_millis(120)).await;
    let client = ClientConfig::new(Profile::ProviderUpload)
        .user_agent("test")
        .no_proxy()
        .build()
        .unwrap();
    assert_eq!(
        client
            .post(url)
            .body("streamed-by-protocol-owner")
            .send()
            .await
            .unwrap()
            .status(),
        307
    );
    task.await.unwrap();
}

#[test]
fn invalid_identity_fails_before_sending_requests() {
    assert!(
        ClientConfig::new(Profile::ExternalAuth)
            .user_agent("bad\r\nidentity")
            .build()
            .is_err()
    );
}

#[tokio::test]
async fn pinning_a_validated_host_takes_precedence_over_a_proxy() {
    let (url, task) = server(Duration::ZERO).await;
    let addr = url.trim_start_matches("http://").parse().unwrap();
    let client = ClientConfig::new(Profile::OfflineDownload)
        .user_agent("test")
        .proxy(reqwest::Proxy::all("http://127.0.0.1:1").unwrap())
        .resolve_to_addrs("fixture.invalid", &[addr])
        .build()
        .unwrap();
    assert_eq!(
        client
            .get(format!("http://fixture.invalid:{}/", addr.port()))
            .send()
            .await
            .unwrap()
            .status(),
        307
    );
    task.await.unwrap();
}
