//! AsterDrive rate-limit response adapter.

use actix_governor::GovernorConfig;
use actix_web::http::StatusCode;
use actix_web::{HttpResponse, HttpResponseBuilder};
use governor::middleware::NoOpMiddleware;

use crate::api::api_error_code::ApiErrorCode;
use crate::config::RateLimitTier;
use aster_forge_middleware::actix::rate_limit::TrustedProxyIpKeyExtractor;

/// Builds a trusted-proxy-aware Governor config with AsterDrive's API envelope.
pub fn build_governor(
    tier: &RateLimitTier,
    trusted_proxies: &[String],
) -> GovernorConfig<TrustedProxyIpKeyExtractor, NoOpMiddleware> {
    aster_forge_middleware::actix::rate_limit::build_ip_governor_config_with_rejection_response(
        tier.seconds_per_request,
        tier.burst_size,
        trusted_proxies,
        rate_limit_response,
    )
}

/// Construct once per server, then clone the shared governor counters into each HTTP worker.
#[derive(Clone)]
pub struct AuthRouteLimiters {
    pub(crate) auth: GovernorConfig<TrustedProxyIpKeyExtractor, NoOpMiddleware>,
    pub(crate) api: GovernorConfig<TrustedProxyIpKeyExtractor, NoOpMiddleware>,
}

impl AuthRouteLimiters {
    pub fn new(
        config: &crate::config::RateLimitConfig,
        trust: &crate::config::NetworkTrustConfig,
    ) -> Self {
        Self {
            auth: build_governor(&config.auth, &trust.trusted_proxies),
            api: build_governor(&config.api, &trust.trusted_proxies),
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
        .status(StatusCode::TOO_MANY_REQUESTS)
        .insert_header(("Retry-After", wait_time.to_string()))
        .json(crate::api::response::failure(
            ApiErrorCode::RateLimited,
            &message,
        ))
}

#[cfg(test)]
mod tests {
    use super::{ApiErrorCode, rate_limit_response};
    use actix_web::{body, http::StatusCode};

    #[actix_web::test]
    async fn rejection_response_uses_drive_error_envelope() {
        let response = rate_limit_response(7, actix_web::HttpResponse::build(StatusCode::OK));
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers().get("Retry-After").unwrap(), "7");

        let body = body::to_bytes(response.into_body()).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["code"], ApiErrorCode::RateLimited.as_str());
        assert_eq!(payload["msg"], "Too Many Requests, retry after 7s");
    }

    #[actix_web::test]
    async fn independent_worker_apps_consume_the_same_auth_allowance() {
        use super::AuthRouteLimiters;
        use actix_governor::Governor;
        use actix_web::{App, HttpResponse, middleware::Condition, test, web};
        let config = crate::config::RateLimitConfig {
            enabled: true,
            auth: crate::config::RateLimitTier {
                seconds_per_request: std::num::NonZeroU64::new(3600).unwrap(),
                burst_size: std::num::NonZeroU32::new(1).unwrap(),
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
                        Governor::new(&worker_one.auth),
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
                        Governor::new(&worker_two.auth),
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
                    .wrap(Condition::new(false, Governor::new(&shared.auth)))
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
