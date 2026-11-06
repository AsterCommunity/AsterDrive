//! Isolate subscriber/callsite state from unrelated concurrent unit-test subscribers.
use aster_drive_outbound::{ClientConfig, Profile};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Clone, Default)]
struct Logs(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Logs {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Logs {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[tokio::test]
async fn trace_has_stable_profile_identity_and_excludes_request_secrets() {
    let logs = Logs::default();
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(logs.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let profiles = [
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
    ];
    for profile in profiles {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "http://{}/private-path?secret=query-secret",
            listener.local_addr().unwrap()
        );
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 2048];
            assert!(socket.read(&mut bytes).await.unwrap() > 0);
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();
        });
        let client = ClientConfig::new(profile).no_proxy().build().unwrap();
        client
            .post(url)
            .bearer_auth("token-secret")
            .body("body-secret")
            .send()
            .await
            .unwrap();
        server.await.unwrap();
    }
    let captured = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    let events = captured
        .lines()
        .filter(|line| line.contains("outbound HTTP exchange completed"))
        .collect::<Vec<_>>();
    assert_eq!(events.len(), profiles.len());
    for (event, profile) in events.iter().zip(profiles) {
        assert!(event.contains(profile.as_str()));
        assert!(event.contains("POST"));
        assert!(event.contains("200"));
        for value in [
            "private-path",
            "query-secret",
            "token-secret",
            "body-secret",
        ] {
            assert!(!event.contains(value), "{event}");
        }
    }
}
