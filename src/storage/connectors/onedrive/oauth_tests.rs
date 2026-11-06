use super::*;
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn endpoint(
    status: &'static str,
    body: &'static str,
    location: Option<String>,
) -> (String, tokio::task::JoinHandle<String>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://{}/tenant/oauth2/v2.0/token",
        listener.local_addr().unwrap()
    );
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0; 4096];
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
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    break;
                }
            }
        }
        let location = location.map_or_else(String::new, |url| format!("Location: {url}\r\n"));
        let response = format!(
            "HTTP/1.1 {status}\r\n{location}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
        String::from_utf8(bytes).unwrap()
    });
    (url, task)
}

fn client(endpoint: &str) -> MicrosoftGraphOAuthClient {
    let mut client = MicrosoftGraphOAuthClient::new(MicrosoftGraphCloud::Global, "common").unwrap();
    client.http =
        aster_drive_outbound::ClientConfig::new(aster_drive_outbound::Profile::ExternalAuth)
            .no_proxy()
            .build()
            .unwrap();
    client.token_endpoint = endpoint.to_owned();
    client
}

fn form(wire: &str) -> HashMap<String, String> {
    assert!(wire.starts_with("POST /tenant/oauth2/v2.0/token HTTP/1.1"));
    assert!(
        wire.to_ascii_lowercase()
            .contains("content-type: application/x-www-form-urlencoded")
    );
    let ua = wire
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(name, _)| name.eq_ignore_ascii_case("user-agent"))
                .map(|(_, value)| value.trim())
        })
        .unwrap();
    assert_eq!(ua, aster_drive_outbound::OUTBOUND_HTTP_USER_AGENT);
    url::form_urlencoded::parse(wire.split_once("\r\n\r\n").unwrap().1.as_bytes())
        .into_owned()
        .collect()
}

#[tokio::test]
async fn token_exchange_keeps_identity_pkce_secret_and_oauth_encoding() {
    for confidential in [false, true] {
        let secret = SecretString::from("secret+& value");
        let (url,task)=endpoint("200 OK",r#"{"access_token":"access","refresh_token":"rotated","token_type":"Bearer","expires_in":3600,"scope":"Files.ReadWrite","id_token":"opaque-id"}"#,None).await;
        let token = client(&url)
            .exchange_code(
                "test-client",
                confidential.then_some(&secret),
                "code+& value",
                "http://localhost/callback?x=a+b",
                "pkce+verifier",
            )
            .await
            .unwrap();
        assert_eq!(token.access_token, "access");
        assert_eq!(token.refresh_token.as_deref(), Some("rotated"));
        assert_eq!(token.expires_in, Some(3600));
        assert_eq!(token.scope.as_deref(), Some("Files.ReadWrite"));
        assert_eq!(token.id_token.as_deref(), Some("opaque-id"));
        let fields = form(&task.await.unwrap());
        assert_eq!(
            fields.get("grant_type").map(String::as_str),
            Some("authorization_code")
        );
        assert_eq!(
            fields.get("client_id").map(String::as_str),
            Some("test-client")
        );
        assert_eq!(fields.get("code").map(String::as_str), Some("code+& value"));
        assert_eq!(
            fields.get("redirect_uri").map(String::as_str),
            Some("http://localhost/callback?x=a+b")
        );
        assert_eq!(
            fields.get("code_verifier").map(String::as_str),
            Some("pkce+verifier")
        );
        assert_eq!(
            fields.get("client_secret").map(String::as_str),
            confidential.then_some("secret+& value")
        );
        assert_eq!(fields.len(), if confidential { 6 } else { 5 });
    }
}

#[tokio::test]
async fn refresh_keeps_grant_and_credentials_request_scoped() {
    for confidential in [true, false] {
        let secret = SecretString::from("refresh-secret");
        let (url, task) = endpoint(
            "200 OK",
            r#"{"access_token":"renewed","refresh_token":"new-refresh","token_type":"bearer"}"#,
            None,
        )
        .await;
        let token = client(&url)
            .refresh_token(
                "test-client",
                confidential.then_some(&secret),
                "refresh+& token",
            )
            .await
            .unwrap();
        assert_eq!(token.access_token, "renewed");
        assert_eq!(token.refresh_token.as_deref(), Some("new-refresh"));
        let fields = form(&task.await.unwrap());
        assert_eq!(
            fields.get("grant_type").map(String::as_str),
            Some("refresh_token")
        );
        assert_eq!(
            fields.get("client_id").map(String::as_str),
            Some("test-client")
        );
        assert_eq!(
            fields.get("refresh_token").map(String::as_str),
            Some("refresh+& token")
        );
        assert_eq!(
            fields.get("client_secret").map(String::as_str),
            confidential.then_some("refresh-secret")
        );
        assert!(!fields.contains_key("code"));
        assert!(!fields.contains_key("code_verifier"));
        assert_eq!(fields.len(), if confidential { 4 } else { 3 });
    }
}

#[tokio::test]
async fn token_validation_rejects_malformed_blank_and_non_bearer_tokens_in_both_flows() {
    for refresh in [false, true] {
        for (body, expected) in [
            ("not JSON", "response is invalid"),
            (r#"{"access_token":" "}"#, "missing access_token"),
            (
                r#"{"access_token":"access","token_type":"MAC"}"#,
                "unsupported token_type",
            ),
        ] {
            let (url, task) = endpoint("200 OK", body, None).await;
            let result = if refresh {
                client(&url)
                    .refresh_token("test-client", None, "refresh")
                    .await
            } else {
                client(&url)
                    .exchange_code(
                        "test-client",
                        None,
                        "code",
                        "http://localhost/cb",
                        "verifier",
                    )
                    .await
            };
            let error = result.unwrap_err();
            assert!(error.message().contains(expected), "{error}");
            task.await.unwrap();
        }
    }
}

#[tokio::test]
async fn refresh_preserves_provider_status_classes_and_stops_on_redirects() {
    for (status, kind) in [
        ("429 Too Many Requests", StorageErrorKind::RateLimited),
        ("401 Unauthorized", StorageErrorKind::Auth),
        ("403 Forbidden", StorageErrorKind::Permission),
        ("503 Service Unavailable", StorageErrorKind::Transient),
        ("400 Bad Request", StorageErrorKind::Auth),
    ] {
        let (url, task) = endpoint(
            status,
            r#"{"error":"invalid_grant","error_description":"fixture rejection"}"#,
            None,
        )
        .await;
        let error = client(&url)
            .refresh_token("test-client", None, "refresh")
            .await
            .unwrap_err();
        assert!(error.message().contains("fixture rejection"));
        assert_eq!(error.storage_error_kind(), Some(kind));
        task.await.unwrap();
    }
    let target = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    for refresh in [false, true] {
        let (url, task) = endpoint(
            "307 Temporary Redirect",
            "",
            Some(format!("http://{}/leak", target.local_addr().unwrap())),
        )
        .await;
        let result = if refresh {
            client(&url)
                .refresh_token("test-client", None, "refresh")
                .await
        } else {
            client(&url)
                .exchange_code(
                    "test-client",
                    None,
                    "code",
                    "http://localhost/cb",
                    "verifier",
                )
                .await
        };
        assert!(result.unwrap_err().message().contains("307"));
        task.await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(20), target.accept())
                .await
                .is_err()
        );
    }
}

#[test]
fn cloud_endpoint_selection_rejects_invalid_tenants_before_provider_requests() {
    for cloud in [MicrosoftGraphCloud::Global, MicrosoftGraphCloud::China] {
        let error = MicrosoftGraphOAuthClient::new(cloud, "../untrusted-host")
            .err()
            .unwrap();
        assert!(error.message().contains("invalid Microsoft Graph tenant"));
        let client = MicrosoftGraphOAuthClient::new(cloud, "common").unwrap();
        assert_eq!(
            client.token_endpoint,
            format!("{}/common/oauth2/v2.0/token", cloud.login_base_url())
        );
    }
}

#[tokio::test]
async fn connection_failures_keep_exchange_and_refresh_error_categories_distinct() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/token", listener.local_addr().unwrap());
    drop(listener);
    let client = client(&url);
    let exchange = client
        .exchange_code(
            "test-client",
            None,
            "code",
            "http://localhost/cb",
            "verifier",
        )
        .await
        .unwrap_err();
    assert!(
        exchange
            .message()
            .contains("Microsoft Graph OAuth token exchange failed")
    );
    assert!(matches!(exchange, AsterError::AuthInvalidCredentials(_)));
    let refresh = client
        .refresh_token("test-client", None, "refresh")
        .await
        .unwrap_err();
    assert!(
        refresh
            .message()
            .contains("Microsoft Graph OAuth token refresh failed")
    );
    assert_eq!(
        refresh.storage_error_kind(),
        Some(StorageErrorKind::Transient)
    );
}
