//! Real two-primary Storage SSE coverage.
//!
//! This test is intentionally ignored in the normal suite because it starts Docker-backed
//! PostgreSQL/Redis services and two real AsterDrive processes. Run it explicitly with
//! `cargo test --test test_multi_primary_storage_events_e2e -- --ignored --nocapture`.

use std::process::Command;
use std::time::Duration;

use aster_forge_test::{
    postgres::PostgresTestContainer,
    process::{TestProcess, available_loopback_port},
    redis::RedisTestContainer,
    suite::TestContainerSuite,
};
use futures::StreamExt;
use reqwest::{Client, StatusCode};
use tokio::time::{sleep, timeout};

const USERNAME: &str = "cross-owner";
const EMAIL: &str = "cross-primary-owner@example.com";
const PASSWORD: &str = "cross-primary-password";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker and two real AsterDrive primary processes"]
async fn storage_events_cross_primary_and_reconnect_after_redis_outage() {
    let suite = TestContainerSuite::new("aster-drive-storage-events-e2e");
    let (postgres, redis) = tokio::join!(
        PostgresTestContainer::start(&suite),
        RedisTestContainer::start(&suite),
    );
    let database = postgres
        .create_database(&format!(
            "aster_drive_sse_{}",
            uuid::Uuid::new_v4().simple()
        ))
        .await;

    let port_a = available_loopback_port();
    let port_b = available_loopback_port();
    assert_ne!(port_a, port_b, "test processes must use distinct ports");
    let base_a = format!("http://127.0.0.1:{port_a}");
    let base_b = format!("http://127.0.0.1:{port_b}");
    let binary = std::env::var_os("CARGO_BIN_EXE_aster_drive")
        .expect("integration tests must expose CARGO_BIN_EXE_aster_drive");

    let mut primary_a = spawn_primary("primary-a", &binary, port_a, database.url(), redis.url());

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(10))
        .build()
        .expect("HTTP client should build");
    wait_until_ready(&client, &base_a, &mut primary_a).await;

    let setup = client
        .post(format!("{base_a}/api/v1/auth/setup"))
        .json(&serde_json::json!({
            "username": USERNAME,
            "email": EMAIL,
            "password": PASSWORD,
        }))
        .send()
        .await
        .expect("setup request should complete");
    assert_eq!(
        setup.status(),
        StatusCode::CREATED,
        "{}",
        setup.text().await.unwrap_or_default()
    );

    let token = login_access_token(&client, &base_a).await;
    let mut primary_b = spawn_primary("primary-b", &binary, port_b, database.url(), redis.url());
    wait_until_ready(&client, &base_b, &mut primary_b).await;
    sleep(Duration::from_secs(1)).await;
    let sse_client = Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .build()
        .expect("SSE client should build without a total timeout");
    let response = sse_client
        .get(format!("{base_a}/api/v1/auth/events/storage"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("storage SSE should connect");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
    let mut stream = response.bytes_stream();

    create_folder(&client, &base_b, &token, "Cross Primary SSE").await;
    let mut sse_data = read_sse_until(&mut stream, |data| data.contains("folder.created")).await;
    assert!(
        sse_data.contains("\"kind\":\"folder.created\""),
        "{sse_data}"
    );

    redis.stop().await;
    sse_data.push_str(&read_sse_until(&mut stream, |data| data.contains("sync.required")).await);
    assert!(
        sse_data.contains("\"kind\":\"sync.required\""),
        "{sse_data}"
    );

    redis.restart().await;
    sleep(Duration::from_secs(2)).await;
    create_folder(&client, &base_b, &token, "After Redis Recovery").await;
    sse_data.push_str(&read_sse_until(&mut stream, |data| data.contains("folder.created")).await);
    assert!(
        sse_data.matches("\"kind\":\"folder.created\"").count() >= 2,
        "recovered subscription did not deliver a second folder event: {sse_data}"
    );

    primary_a.assert_running();
    primary_b.assert_running();
    database.cleanup().await;
}

fn spawn_primary(
    name: &str,
    binary: &std::ffi::OsStr,
    port: u16,
    database_url: &str,
    redis_url: &str,
) -> TestProcess {
    let mut command = Command::new(binary);
    command
        .arg("serve")
        .env("ASTER__SERVER__HOST", "127.0.0.1")
        .env("ASTER__SERVER__PORT", port.to_string())
        .env("ASTER__DATABASE__URL", database_url)
        .env("ASTER__DATABASE__POOL_SIZE", "5")
        .env("ASTER__CONFIG_SYNC__BACKEND", "redis")
        .env("ASTER__CONFIG_SYNC__ENDPOINT", redis_url)
        .env("ASTER__CONFIG_SYNC__TOPIC", "aster_drive.config_reload")
        .env(
            "ASTER__AUTH__JWT_SECRET",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .env(
            "ASTER__AUTH__SHARE_COOKIE_SECRET",
            "1123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .env(
            "ASTER__AUTH__DIRECT_LINK_SECRET",
            "2123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .env(
            "ASTER__AUTH__MFA_SECRET_KEY",
            "3123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .env(
            "ASTER__AUTH__STORAGE_CREDENTIAL_SECRET_KEY",
            "4123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .env("ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES", "true")
        .env("RUST_LOG", "aster_drive=info");
    TestProcess::spawn(name, &mut command)
}

async fn wait_until_ready(client: &Client, base_url: &str, process: &mut TestProcess) {
    let result = timeout(Duration::from_secs(90), async {
        loop {
            process.assert_running();
            match client
                .post(format!("{base_url}/api/v1/auth/check"))
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => return,
                _ => sleep(Duration::from_millis(250)).await,
            }
        }
    })
    .await;
    if result.is_err() {
        panic!(
            "AsterDrive did not become ready at {base_url}\n{}",
            process.diagnostics()
        );
    }
}

async fn login_access_token(client: &Client, base_url: &str) -> String {
    let response = client
        .post(format!("{base_url}/api/v1/auth/login"))
        .json(&serde_json::json!({
            "identifier": USERNAME,
            "password": PASSWORD,
        }))
        .send()
        .await
        .expect("login request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    for value in response.headers().get_all(reqwest::header::SET_COOKIE) {
        let cookie = value.to_str().expect("Set-Cookie should be valid UTF-8");
        if let Some(token) = cookie.strip_prefix("aster_access=") {
            return token.split(';').next().unwrap_or_default().to_string();
        }
    }
    panic!("login response did not contain an aster_access cookie");
}

async fn create_folder(client: &Client, base_url: &str, token: &str, name: &str) {
    let response = client
        .post(format!("{base_url}/api/v1/folders"))
        .bearer_auth(token)
        .json(&serde_json::json!({ "name": name }))
        .send()
        .await
        .expect("folder request should complete");
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "{}",
        response.text().await.unwrap_or_default()
    );
}

async fn read_sse_until<S>(stream: &mut S, predicate: impl Fn(&str) -> bool) -> String
where
    S: futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin,
{
    let mut data = String::new();
    timeout(Duration::from_secs(30), async {
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.expect("SSE stream should remain readable");
            data.push_str(&String::from_utf8_lossy(&chunk));
            if predicate(&data) {
                return;
            }
        }
        panic!("SSE stream ended before expected event: {data}");
    })
    .await
    .expect("timed out waiting for expected SSE event");
    data
}
