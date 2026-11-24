//! AsterDrive rate-limit response adapter.

use actix_web::{HttpResponse, HttpResponseBuilder};

use crate::api::api_error_code::ApiErrorCode;
use crate::config::RateLimitTier;
use aster_forge_middleware::actix::rate_limit::{
    RateLimiterConfig, TrustedProxyIpKeyExtractor,
    build_ip_rate_limit_config_with_rejection_response,
};

/// Builds a trusted-proxy-aware limiter config with AsterDrive's API envelope.
pub fn build_rate_limit(
    tier: &RateLimitTier,
    trusted_proxies: &[String],
) -> RateLimiterConfig<TrustedProxyIpKeyExtractor> {
    build_ip_rate_limit_config_with_rejection_response(
        tier.seconds_per_request,
        tier.burst_size,
        trusted_proxies,
        rate_limit_response,
    )
}

/// Construct once per server, then clone the shared governor counters into each HTTP worker.
#[derive(Clone)]
pub struct AuthRouteLimiters {
    pub(crate) auth: RateLimiterConfig<TrustedProxyIpKeyExtractor>,
    pub(crate) api: RateLimiterConfig<TrustedProxyIpKeyExtractor>,
}

impl AuthRouteLimiters {
    pub fn new(
        config: &crate::config::RateLimitConfig,
        trust: &crate::config::NetworkTrustConfig,
    ) -> Self {
        Self {
            auth: build_rate_limit(&config.auth, &trust.trusted_proxies),
            api: build_rate_limit(&config.api, &trust.trusted_proxies),
        }
    }
}

impl Default for AuthRouteLimiters {
    fn default() -> Self {
        Self::new(&Default::default(), &Default::default())
    }
}

fn rate_limit_response(wait_time: u64, mut response: HttpResponseBuilder) -> HttpResponse {
    let message = format!("Too Many Requests, retry after {wait_time}s");
    response
        // Forge sets Retry-After. Preserve Drive's other rejection header with the same delay.
        .insert_header(("X-RateLimit-After", wait_time.to_string()))
        .json(crate::api::response::failure(
            ApiErrorCode::RateLimited,
            &message,
        ))
}

#[cfg(test)]
mod tests {
    use super::{ApiErrorCode, AuthRouteLimiters, build_rate_limit};
    use actix_web::{App, HttpResponse, http::StatusCode, middleware::Condition, test, web};
    use aster_forge_middleware::actix::rate_limit::RateLimit;
    use std::num::{NonZeroU32, NonZeroU64};

    #[actix_web::test]
    async fn limiter_preserves_burst_and_trusted_proxy_ip_buckets() {
        let limiter = build_rate_limit(
            &crate::config::RateLimitTier {
                seconds_per_request: NonZeroU64::new(3600).unwrap(),
                burst_size: NonZeroU32::new(2).unwrap(),
            },
            &["10.0.0.0/8".into()],
        );
        let app = test::init_service(
            App::new().service(
                web::resource("/probe")
                    .wrap(RateLimit::new(&limiter))
                    .route(web::post().to(HttpResponse::Ok)),
            ),
        )
        .await;
        // Trusted peers share client buckets; untrusted or absent peers ignore forwarded headers.
        for (peer, forwarded, expected) in [
            (Some("10.0.0.1:1234"), "192.0.2.10, 10.1.0.1", 200),
            (Some("10.0.0.2:1234"), "192.0.2.10", 200),
            (Some("10.0.0.1:1234"), "192.0.2.10", 429),
            (Some("10.0.0.1:1234"), "192.0.2.11", 200),
            (Some("192.0.2.10:1234"), "192.0.2.11", 429),
            (None, "192.0.2.10", 200),
        ] {
            let mut request = test::TestRequest::post()
                .uri("/probe")
                .insert_header(("X-Forwarded-For", forwarded));
            if let Some(peer) = peer {
                request = request.peer_addr(peer.parse().unwrap());
            }
            let response = test::call_service(&app, request.to_request()).await;
            assert_eq!(
                response.status(),
                expected,
                "peer={peer:?}, xff={forwarded}"
            );
            if response.status() == StatusCode::TOO_MANY_REQUESTS {
                let retry_after = response.headers().get("Retry-After").unwrap();
                assert_eq!(
                    response.headers().get("X-RateLimit-After"),
                    Some(retry_after)
                );
                let delay: u64 = retry_after.to_str().unwrap().parse().unwrap();
                assert!((1..=3600).contains(&delay));
                let payload: serde_json::Value = test::read_body_json(response).await;
                assert_eq!(payload["code"], ApiErrorCode::RateLimited.as_str());
                assert_eq!(
                    payload["msg"],
                    format!("Too Many Requests, retry after {delay}s")
                );
            } else {
                assert!(!response.headers().contains_key("Retry-After"));
                assert!(!response.headers().contains_key("X-RateLimit-After"));
            }
        }
    }

    #[actix_web::test]
    async fn independent_worker_apps_consume_the_same_auth_allowance() {
        let config = crate::config::RateLimitConfig {
            enabled: true,
            auth: crate::config::RateLimitTier {
                seconds_per_request: NonZeroU64::new(3600).unwrap(),
                burst_size: NonZeroU32::new(1).unwrap(),
            },
            ..Default::default()
        };
        let shared = AuthRouteLimiters::new(&config, &Default::default());
        let worker_one = shared.clone();
        let worker_two = shared.clone();
        let app_one = test::init_service(
            App::new().service(
                web::resource("/probe")
                    .wrap(Condition::new(
                        config.enabled,
                        RateLimit::new(&worker_one.auth),
                    ))
                    .route(web::post().to(|| async { HttpResponse::Ok().finish() })),
            ),
        )
        .await;
        let app_two = test::init_service(
            App::new().service(
                web::resource("/probe")
                    .wrap(Condition::new(
                        config.enabled,
                        RateLimit::new(&worker_two.auth),
                    ))
                    .route(web::post().to(|| async { HttpResponse::Ok().finish() })),
            ),
        )
        .await;
        let peer = "192.0.2.10:1234".parse().unwrap();
        assert_eq!(
            test::call_service(
                &app_one,
                test::TestRequest::post()
                    .uri("/probe")
                    .peer_addr(peer)
                    .to_request()
            )
            .await
            .status(),
            200
        );
        assert_eq!(
            test::call_service(
                &app_two,
                test::TestRequest::post()
                    .uri("/probe")
                    .peer_addr(peer)
                    .to_request()
            )
            .await
            .status(),
            429
        );
        let disabled = test::init_service(
            App::new().service(
                web::resource("/probe")
                    .wrap(Condition::new(false, RateLimit::new(&shared.auth)))
                    .route(web::post().to(|| async { HttpResponse::Ok().finish() })),
            ),
        )
        .await;
        assert_eq!(
            test::call_service(
                &disabled,
                test::TestRequest::post()
                    .uri("/probe")
                    .peer_addr(peer)
                    .to_request()
            )
            .await
            .status(),
            200
        );
    }
}
