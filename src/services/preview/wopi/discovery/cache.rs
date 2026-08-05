use std::sync::LazyLock;
use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};
use moka::future::Cache;

use crate::config::OUTBOUND_HTTP_USER_AGENT;
use crate::config::wopi;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::SharedRuntimeState;
use aster_forge_http::read_reqwest_body_limited;

use super::parser::{WOPI_DISCOVERY_XML_MAX_BYTES, parse_discovery_xml};
use super::types::{CachedWopiDiscovery, WopiDiscovery};

static DISCOVERY_CACHE: LazyLock<Cache<String, CachedWopiDiscovery>> =
    LazyLock::new(|| Cache::builder().max_capacity(128).build());

static DISCOVERY_CLIENT: LazyLock<std::result::Result<reqwest::Client, String>> =
    LazyLock::new(build_discovery_client);

fn build_discovery_client() -> std::result::Result<reqwest::Client, String> {
    build_discovery_client_with_user_agent(OUTBOUND_HTTP_USER_AGENT)
}

fn build_discovery_client_with_user_agent(
    user_agent: &str,
) -> std::result::Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(StdDuration::from_secs(5))
        .user_agent(user_agent)
        .build()
        .map_err(|error| error.to_string())
}

pub(super) async fn load_discovery(
    state: &impl SharedRuntimeState,
    discovery_url: &str,
) -> Result<WopiDiscovery> {
    load_discovery_with_runtime_config(state.runtime_config(), discovery_url).await
}

async fn load_discovery_with_runtime_config(
    runtime_config: &crate::config::RuntimeConfig,
    discovery_url: &str,
) -> Result<WopiDiscovery> {
    let cached = DISCOVERY_CACHE.get(discovery_url).await;
    if let Some(cached) = cached.as_ref()
        && cached.cached_at + discovery_cache_ttl(runtime_config) > Utc::now()
    {
        return Ok(cached.discovery.clone());
    }

    let client = DISCOVERY_CLIENT.as_ref().map_err(|error| {
        AsterError::internal_error(format!(
            "failed to initialize WOPI discovery client: {error}"
        ))
    })?;

    let response = match client.get(discovery_url).send().await.map_aster_err_ctx(
        "failed to fetch WOPI discovery",
        AsterError::validation_error,
    ) {
        Ok(response) => response,
        Err(error) => {
            if let Some(cached) = cached.as_ref() {
                tracing::warn!(
                    discovery_url,
                    error = %error,
                    "using stale WOPI discovery cache after refresh failure"
                );
                return Ok(cached.discovery.clone());
            }
            return Err(error);
        }
    };

    if !response.status().is_success() {
        if let Some(cached) = cached.as_ref() {
            tracing::warn!(
                discovery_url,
                status = %response.status(),
                "using stale WOPI discovery cache after non-success refresh"
            );
            return Ok(cached.discovery.clone());
        }
        return Err(AsterError::validation_error(format!(
            "WOPI discovery returned HTTP {}",
            response.status()
        )));
    }

    let parsed = match parse_discovery_response(response).await {
        Ok(parsed) => parsed,
        Err(error) => {
            if let Some(cached) = cached.as_ref() {
                tracing::warn!(
                    discovery_url,
                    error = %error,
                    "using stale WOPI discovery cache after response processing failure"
                );
                return Ok(cached.discovery.clone());
            }
            return Err(error);
        }
    };

    DISCOVERY_CACHE
        .insert(
            discovery_url.to_string(),
            CachedWopiDiscovery {
                discovery: parsed.clone(),
                cached_at: Utc::now(),
            },
        )
        .await;
    Ok(parsed)
}

async fn parse_discovery_response(response: reqwest::Response) -> Result<WopiDiscovery> {
    let body = read_reqwest_body_limited(
        response,
        "WOPI discovery response body",
        WOPI_DISCOVERY_XML_MAX_BYTES,
        AsterError::validation_error,
    )
    .await?;
    let body = std::str::from_utf8(&body).map_aster_err_ctx(
        "WOPI discovery response is not UTF-8",
        AsterError::validation_error,
    )?;
    parse_discovery_xml(body)
}

fn discovery_cache_ttl(runtime_config: &crate::config::RuntimeConfig) -> Duration {
    let ttl_secs = wopi::discovery_cache_ttl_secs(runtime_config);
    Duration::seconds(i64::try_from(ttl_secs).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use crate::config::OUTBOUND_HTTP_USER_AGENT;
    use crate::services::preview::wopi::discovery::types::{
        CachedWopiDiscovery, WopiDiscovery, WopiDiscoveryAction,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::{
        DISCOVERY_CACHE, WOPI_DISCOVERY_XML_MAX_BYTES, build_discovery_client,
        build_discovery_client_with_user_agent, load_discovery_with_runtime_config,
    };

    async fn response_url(headers: String, body: Vec<u8>) -> String {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("test listener should bind");
        let addr = listener
            .local_addr()
            .expect("test listener should expose local addr");
        tokio::spawn(async move {
            let (mut socket, _) = listener
                .accept()
                .await
                .expect("test server should accept request");
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            loop {
                let read = socket
                    .read(&mut buffer)
                    .await
                    .expect("test server should read request");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            socket
                .write_all(headers.as_bytes())
                .await
                .expect("test server should write headers");
            socket
                .write_all(&body)
                .await
                .expect("test server should write body");
        });
        format!("http://{addr}/hosting/discovery")
    }

    fn stale_discovery() -> CachedWopiDiscovery {
        CachedWopiDiscovery {
            discovery: WopiDiscovery {
                actions: vec![WopiDiscoveryAction {
                    action: "view".to_string(),
                    app_icon_url: None,
                    app_name: Some("Cached Word".to_string()),
                    ext: Some("docx".to_string()),
                    mime: None,
                    urlsrc: "https://cached.example.com/view?".to_string(),
                }],
                proof_keys: None,
            },
            cached_at: Utc::now() - Duration::days(1),
        }
    }

    #[tokio::test]
    async fn discovery_client_sets_user_agent() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("test listener should bind");
        let addr = listener
            .local_addr()
            .expect("test listener should expose local addr");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener
                .accept()
                .await
                .expect("test server should accept request");
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            loop {
                let read = socket
                    .read(&mut buffer)
                    .await
                    .expect("test server should read request");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            socket
                .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                .await
                .expect("test server should write response");
            String::from_utf8(request).expect("request should be utf-8")
        });

        build_discovery_client()
            .expect("discovery client should build")
            .get(format!("http://{addr}/hosting/discovery"))
            .send()
            .await
            .expect("request should be sent");
        let raw_request = server.await.expect("test server task should complete");
        let user_agent = raw_request
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("user-agent")
                    .then(|| value.trim())
            })
            .expect("user-agent header should be present");

        assert_eq!(user_agent, OUTBOUND_HTTP_USER_AGENT);
    }

    #[test]
    fn discovery_client_reports_invalid_user_agent() {
        let error = build_discovery_client_with_user_agent("bad\r\nuser-agent")
            .expect_err("invalid user-agent should fail client construction");

        assert!(
            error.contains("header") || error.contains("builder"),
            "unexpected client build error: {error}"
        );
    }

    #[tokio::test]
    async fn stale_cache_covers_response_size_and_utf8_failures() {
        let cases = [
            (
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    WOPI_DISCOVERY_XML_MAX_BYTES + 1
                ),
                Vec::new(),
                "WOPI discovery response body",
            ),
            (
                "HTTP/1.1 200 OK\r\nContent-Length: 1\r\nConnection: close\r\n\r\n".to_string(),
                vec![0xff],
                "WOPI discovery response is not UTF-8",
            ),
        ];

        for (headers, body, expected_context) in cases {
            let discovery_url = response_url(headers.clone(), body.clone()).await;
            DISCOVERY_CACHE
                .insert(discovery_url.clone(), stale_discovery())
                .await;

            let discovery = load_discovery_with_runtime_config(
                &crate::config::RuntimeConfig::new(),
                &discovery_url,
            )
            .await
            .expect("response processing failure should use stale discovery");

            assert_eq!(
                discovery.actions[0].app_name.as_deref(),
                Some("Cached Word")
            );
            DISCOVERY_CACHE.invalidate(&discovery_url).await;

            let discovery_url = response_url(headers, body).await;
            let error = load_discovery_with_runtime_config(
                &crate::config::RuntimeConfig::new(),
                &discovery_url,
            )
            .await
            .expect_err("response processing failure without stale discovery should propagate");
            assert!(
                error.message().contains(expected_context),
                "unexpected response processing error: {}",
                error.message()
            );
        }
    }
}
