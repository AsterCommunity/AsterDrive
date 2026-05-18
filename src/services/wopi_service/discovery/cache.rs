use std::sync::LazyLock;
use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};
use moka::future::Cache;

use crate::config::wopi;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::utils::OUTBOUND_HTTP_USER_AGENT;

use super::parser::parse_discovery_xml;
use super::types::{CachedWopiDiscovery, WopiDiscovery};

static DISCOVERY_CACHE: LazyLock<Cache<String, CachedWopiDiscovery>> =
    LazyLock::new(|| Cache::builder().max_capacity(128).build());

static DISCOVERY_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(build_discovery_client);

fn build_discovery_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(StdDuration::from_secs(5))
        .user_agent(OUTBOUND_HTTP_USER_AGENT)
        .build()
        .expect("wopi discovery client should initialize")
}

pub(super) async fn load_discovery(
    state: &PrimaryAppState,
    discovery_url: &str,
) -> Result<WopiDiscovery> {
    let cached = DISCOVERY_CACHE.get(discovery_url).await;
    if let Some(cached) = cached.as_ref()
        && cached.cached_at + discovery_cache_ttl(&state.runtime_config) > Utc::now()
    {
        return Ok(cached.discovery.clone());
    }

    let response = match DISCOVERY_CLIENT
        .get(discovery_url)
        .send()
        .await
        .map_aster_err_ctx(
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

    let body = response.text().await.map_aster_err_ctx(
        "failed to read WOPI discovery",
        AsterError::validation_error,
    )?;
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
    use super::build_discovery_client;
    use crate::utils::OUTBOUND_HTTP_USER_AGENT;
    use reqwest::header::USER_AGENT;

    #[test]
    fn discovery_client_sets_user_agent() {
        let request = build_discovery_client()
            .get("http://example.com/hosting/discovery")
            .build()
            .expect("request should build");
        let user_agent = request
            .headers()
            .get(USER_AGENT)
            .and_then(|value| value.to_str().ok())
            .expect("user-agent header should be present");

        assert_eq!(user_agent, OUTBOUND_HTTP_USER_AGENT);
    }
}
