use std::sync::LazyLock;
use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};
use moka::future::Cache;

use crate::config::OUTBOUND_HTTP_USER_AGENT;
use crate::config::wopi;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::SharedRuntimeState;
use crate::storage::http_body::read_reqwest_response_body_limited;
use aster_forge_xml::XmlSafetyPolicy;

use super::parser::parse_discovery_xml;
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
    let cached = DISCOVERY_CACHE.get(discovery_url).await;
    if let Some(cached) = cached.as_ref()
        && cached.cached_at + discovery_cache_ttl(state.runtime_config()) > Utc::now()
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

    let body = read_reqwest_response_body_limited(
        response,
        "failed to read WOPI discovery",
        XmlSafetyPolicy::untrusted().max_input_bytes,
        AsterError::validation_error,
    )
    .await?;
    let body = String::from_utf8(body)
        .map_err(|_| AsterError::validation_error("WOPI discovery response is not UTF-8"))?;
    let parsed = match parse_discovery_xml(&body) {
        Ok(parsed) => parsed,
        Err(error) => {
            if let Some(cached) = cached.as_ref() {
                tracing::warn!(
                    discovery_url,
                    error = %error,
                    "using stale WOPI discovery cache after parse failure"
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

fn discovery_cache_ttl(runtime_config: &crate::config::RuntimeConfig) -> Duration {
    let ttl_secs = wopi::discovery_cache_ttl_secs(runtime_config);
    Duration::seconds(i64::try_from(ttl_secs).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use super::{build_discovery_client, build_discovery_client_with_user_agent};
    use crate::config::OUTBOUND_HTTP_USER_AGENT;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
}
