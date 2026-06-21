//! API 中间件：`rate_limit`。

use actix_governor::{
    GovernorConfig, GovernorConfigBuilder, KeyExtractor, SimpleKeyExtractionError,
};
use actix_web::dev::ServiceRequest;
use actix_web::{HttpResponse, HttpResponseBuilder};
use governor::NotUntil;
use governor::clock::{Clock, DefaultClock, QuantaInstant};
use governor::middleware::NoOpMiddleware;
use ipnet::IpNet;
use std::net::{IpAddr, Ipv4Addr};

use crate::api::response::ApiResponse;
use crate::config::RateLimitTier;

/// IP-based key extractor，429 响应返回 ApiResponse JSON 格式。
///
/// `trusted_proxies` 非空时，若 `peer_addr` 在可信 CIDR 内，则取
/// `X-Forwarded-For` 最左段（真实客户端）作为限流键；否则退回 `peer_addr`，
/// 防止伪造 XFF 绕过限流。
#[derive(Debug, Clone)]
pub struct AsterIpKeyExtractor {
    trusted: Vec<IpNet>,
}

impl AsterIpKeyExtractor {
    pub fn new(trusted_proxies: &[String]) -> Self {
        let trusted = trusted_proxies
            .iter()
            .filter_map(|s| {
                s.parse::<IpNet>()
                    .or_else(|_| s.parse::<IpAddr>().map(IpNet::from))
                    .map_err(|e| tracing::warn!("invalid trusted_proxy entry '{s}': {e}"))
                    .ok()
            })
            .collect();
        Self { trusted }
    }

    fn is_trusted(&self, ip: IpAddr) -> bool {
        self.trusted.iter().any(|net| net.contains(&ip))
    }

    fn real_ip(&self, req: &ServiceRequest, peer: IpAddr) -> IpAddr {
        if !self.trusted.is_empty() && self.is_trusted(peer) {
            let ip = req
                .headers()
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.split(',').next())
                .and_then(|p| p.trim().parse::<IpAddr>().ok());
            if let Some(ip) = ip {
                return ip;
            }
        }
        peer
    }
}

impl KeyExtractor for AsterIpKeyExtractor {
    type Key = IpAddr;
    type KeyExtractionError = SimpleKeyExtractionError<&'static str>;

    fn extract(&self, req: &ServiceRequest) -> Result<Self::Key, Self::KeyExtractionError> {
        let peer = req
            .peer_addr()
            .map(|s| s.ip())
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
        Ok(self.real_ip(req, peer))
    }

    fn exceed_rate_limit_response(
        &self,
        negative: &NotUntil<QuantaInstant>,
        _response: HttpResponseBuilder,
    ) -> HttpResponse {
        let wait_time = negative
            .wait_time_from(DefaultClock::default().now())
            .as_secs();
        let msg = format!("Too Many Requests, retry after {wait_time}s");
        HttpResponse::TooManyRequests()
            .insert_header(("Retry-After", wait_time.to_string()))
            .json(ApiResponse::<()>::error(
                crate::api::api_error_code::ApiErrorCode::RateLimited,
                &msg,
            ))
    }
}

/// 根据 tier 配置创建 Governor 实例
#[allow(clippy::expect_used)]
pub fn build_governor(
    tier: &RateLimitTier,
    trusted_proxies: &[String],
) -> GovernorConfig<AsterIpKeyExtractor, NoOpMiddleware> {
    GovernorConfigBuilder::default()
        .key_extractor(AsterIpKeyExtractor::new(trusted_proxies))
        .seconds_per_request(tier.seconds_per_request.get())
        .burst_size(tier.burst_size.get())
        .finish()
        .expect("non-zero rate limit tier should always build")
}

#[cfg(test)]
mod tests {
    use super::AsterIpKeyExtractor;
    use std::net::IpAddr;

    #[test]
    fn empty_trusted_always_uses_peer() {
        let ext = AsterIpKeyExtractor::new(&[]);
        let peer: IpAddr = "1.2.3.4".parse().unwrap();
        assert!(!ext.is_trusted(peer));
    }

    #[test]
    fn cidr_match_trusts_proxy_and_reads_xff() {
        let ext = AsterIpKeyExtractor::new(&["10.0.0.0/8".to_string()]);
        assert!(ext.is_trusted("10.0.0.1".parse().unwrap()));
        assert!(!ext.is_trusted("11.0.0.1".parse().unwrap()));
    }

    #[test]
    fn single_ip_trusted_proxy() {
        let ext = AsterIpKeyExtractor::new(&["192.168.1.1".to_string()]);
        assert!(ext.is_trusted("192.168.1.1".parse().unwrap()));
        assert!(!ext.is_trusted("192.168.1.2".parse().unwrap()));
    }
}
