//! AsterDrive rate-limit response adapter.

use actix_governor::GovernorConfig;
use actix_web::http::StatusCode;
use actix_web::{HttpResponse, HttpResponseBuilder};
use governor::middleware::NoOpMiddleware;

use crate::api::api_error_code::ApiErrorCode;
use crate::api::response::ApiResponse;
use crate::config::RateLimitTier;
use aster_forge_actix_middleware::rate_limit::TrustedProxyIpKeyExtractor;

/// Builds a trusted-proxy-aware Governor config with AsterDrive's API envelope.
pub fn build_governor(
    tier: &RateLimitTier,
    trusted_proxies: &[String],
) -> GovernorConfig<TrustedProxyIpKeyExtractor, NoOpMiddleware> {
    aster_forge_actix_middleware::rate_limit::build_ip_governor_config_with_rejection_response(
        tier.seconds_per_request,
        tier.burst_size,
        trusted_proxies,
        rate_limit_response,
    )
}

fn rate_limit_response(wait_time: u64, mut response: HttpResponseBuilder) -> HttpResponse {
    let message = format!("Too Many Requests, retry after {wait_time}s");
    response
        .status(StatusCode::TOO_MANY_REQUESTS)
        .insert_header(("Retry-After", wait_time.to_string()))
        .json(ApiResponse::<()>::error(
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
}
