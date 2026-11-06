use std::time::Duration as StdDuration;

use super::{TEST_BROWSER_ORIGIN, TEST_CLIENT_ID};

const DEX_TEST_CLIENT_SECRET: &str = "super-secret";
pub const DEX_TEST_IMAGE_TAG: &str = "v2.42.0";
pub const DEX_TEST_USER_EMAIL: &str = "dex-user@example.com";
pub const DEX_TEST_USER_SUBJECT: &str = "CgtkZXgtdXNlci1pZBIFbG9jYWw";

pub fn reserve_localhost_port() -> (u16, std::net::TcpListener) {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("free localhost port should be reserved");
    let port = listener
        .local_addr()
        .expect("reserved listener address should exist")
        .port();
    (port, listener)
}

pub fn dex_config(issuer: &str, provider_key: &str) -> String {
    format!(
        r#"issuer: {issuer}
storage:
  type: memory
web:
  http: 0.0.0.0:5556
oauth2:
  skipApprovalScreen: true
staticClients:
  - id: {TEST_CLIENT_ID}
    redirectURIs:
      - {TEST_BROWSER_ORIGIN}/api/v1/auth/external-auth/oidc/{provider_key}/callback
    name: AsterDrive Test
    secret: {DEX_TEST_CLIENT_SECRET}
enablePasswordDB: true
staticPasswords:
  - email: "{DEX_TEST_USER_EMAIL}"
    hash: "$2a$10$2b2cU8CPhOTaGrs1HRQuAueS7JTT5ZHsHSzYiFPm1leZck7Mc8T4W"
    username: "dex-user"
    userID: "dex-user-id"
"#
    )
}

pub async fn wait_for_dex_discovery(issuer: &str) {
    let discovery_url = format!("{issuer}/.well-known/openid-configuration");
    let client = aster_forge_http::HttpClient::builder()
        .timeout(StdDuration::from_secs(2))
        .build()
        .expect("reqwest client should build");
    let deadline = tokio::time::Instant::now() + StdDuration::from_secs(30);
    let mut last_error: Option<String>;
    loop {
        last_error = match client.get(&discovery_url).send().await {
            Ok(resp) if resp.status().is_success() => return,
            Ok(resp) => Some(format!("HTTP {}", resp.status())),
            Err(err) => Some(err.to_string()),
        };
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for Dex discovery at {discovery_url}: {}",
            last_error.unwrap_or_else(|| "unknown error".to_string())
        );
        tokio::time::sleep(StdDuration::from_millis(250)).await;
    }
}

fn absolute_location(base: &reqwest::Url, location: &str) -> reqwest::Url {
    reqwest::Url::parse(location)
        .or_else(|_| base.join(location))
        .expect("redirect location should be a valid URL")
}

async fn request_dex_redirect(
    client: &aster_forge_http::HttpClient,
    url: reqwest::Url,
) -> (reqwest::Url, Option<reqwest::Url>) {
    let resp = client
        .get(url.clone())
        .send()
        .await
        .expect("Dex redirect GET should succeed");
    if resp.status().is_redirection() {
        let location = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .expect("Dex redirect should include Location");
        return (url.clone(), Some(absolute_location(&url, location)));
    }
    panic!(
        "Dex GET {url} returned non-redirect status {}",
        resp.status()
    );
}

pub async fn complete_dex_password_login(
    issuer: &str,
    provider_key: &str,
    authorization_url: &str,
) -> String {
    let client = aster_forge_http::HttpClient::builder()
        .timeout(StdDuration::from_secs(10))
        .build()
        .expect("reqwest client should build");
    let issuer_url = reqwest::Url::parse(issuer).expect("Dex issuer URL should parse");
    let mut next =
        reqwest::Url::parse(authorization_url).expect("OIDC authorization URL should parse");
    let mut login_url = None;

    for _ in 0..6 {
        let (_, redirect) = request_dex_redirect(&client, next.clone()).await;
        let redirect = redirect.expect("Dex should keep redirecting until the login form");
        if redirect.path().ends_with("/auth/local/login") {
            login_url = Some(redirect);
            break;
        }
        assert_eq!(
            redirect.domain(),
            issuer_url.domain(),
            "Dex should not redirect to another host before login"
        );
        next = redirect;
    }

    let login_url = login_url.expect("Dex password login URL should be reached");
    let form = format!(
        "login={}&password={}",
        urlencoding::encode(DEX_TEST_USER_EMAIL),
        urlencoding::encode("password")
    );
    let resp = client
        .post(login_url.clone())
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(form)
        .send()
        .await
        .expect("Dex password POST should succeed");
    assert!(
        resp.status().is_redirection(),
        "Dex password POST should redirect, got {}",
        resp.status()
    );
    let location = resp
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .expect("Dex login redirect should include Location");
    let redirect = absolute_location(&login_url, location);
    let expected_callback =
        format!("{TEST_BROWSER_ORIGIN}/api/v1/auth/external-auth/oidc/{provider_key}/callback");
    assert_eq!(
        redirect.as_str().split('?').next(),
        Some(expected_callback.as_str()),
        "Dex should redirect back to AsterDrive callback"
    );
    redirect.to_string()
}
