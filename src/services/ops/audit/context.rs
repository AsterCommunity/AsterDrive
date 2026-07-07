use crate::services::auth::local::Claims;
use actix_web::HttpRequest;
use ipnet::IpNet;
use std::net::{IpAddr, SocketAddr};

pub(super) const MAX_AUDIT_IP_ADDRESS_LEN: usize = 45;
pub(super) const MAX_AUDIT_USER_AGENT_LEN: usize = 512;

/// 从 HttpRequest 提取的审计上下文
#[derive(Debug, Clone)]
pub struct AuditContext {
    pub user_id: i64,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

/// 从 HttpRequest 提取的请求级审计元信息。
#[derive(Debug, Clone)]
pub struct AuditRequestInfo {
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

impl AuditContext {
    pub fn system() -> Self {
        Self {
            user_id: 0,
            ip_address: None,
            user_agent: None,
        }
    }

    pub fn from_request(req: &HttpRequest, claims: &Claims) -> Self {
        AuditRequestInfo::from_request(req).to_context(claims.user_id)
    }

    pub fn from_request_with_trusted_proxies(
        req: &HttpRequest,
        claims: &Claims,
        trusted_proxies: &[String],
    ) -> Self {
        AuditRequestInfo::from_request_with_trusted_proxies(req, trusted_proxies)
            .to_context(claims.user_id)
    }
}

impl AuditRequestInfo {
    pub fn from_request(req: &HttpRequest) -> Self {
        Self {
            ip_address: req
                .connection_info()
                .realip_remote_addr()
                .map(|s| bounded_audit_value(s, MAX_AUDIT_IP_ADDRESS_LEN)),
            user_agent: req
                .headers()
                .get("user-agent")
                .and_then(|v| v.to_str().ok())
                .map(|s| bounded_audit_value(s, MAX_AUDIT_USER_AGENT_LEN)),
        }
    }

    pub fn from_request_with_trusted_proxies(
        req: &HttpRequest,
        trusted_proxies: &[String],
    ) -> Self {
        Self {
            ip_address: trusted_request_ip(req, trusted_proxies)
                .map(|ip| bounded_audit_value(&ip.to_string(), MAX_AUDIT_IP_ADDRESS_LEN)),
            user_agent: req
                .headers()
                .get("user-agent")
                .and_then(|v| v.to_str().ok())
                .map(|s| bounded_audit_value(s, MAX_AUDIT_USER_AGENT_LEN)),
        }
    }

    pub fn to_context(&self, user_id: i64) -> AuditContext {
        AuditContext {
            user_id,
            ip_address: self.ip_address.clone(),
            user_agent: self.user_agent.clone(),
        }
    }
}

fn trusted_request_ip(req: &HttpRequest, trusted_proxies: &[String]) -> Option<IpAddr> {
    let peer = req.peer_addr()?.ip();
    if trusted_proxy_matches(peer, trusted_proxies)
        && let Some(forwarded_ip) = req
            .headers()
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.split(',').next())
            .and_then(parse_forwarded_ip)
    {
        return Some(forwarded_ip);
    }
    Some(peer)
}

fn parse_forwarded_ip(value: &str) -> Option<IpAddr> {
    let value = value.trim();
    value
        .parse::<IpAddr>()
        .or_else(|_| value.parse::<SocketAddr>().map(|addr| addr.ip()))
        .ok()
}

fn trusted_proxy_matches(peer: IpAddr, trusted_proxies: &[String]) -> bool {
    trusted_proxies.iter().any(|entry| {
        entry
            .parse::<IpNet>()
            .or_else(|_| entry.parse::<IpAddr>().map(IpNet::from))
            .is_ok_and(|net| net.contains(&peer))
    })
}

pub(super) fn bounded_audit_value(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_string();
    }

    let mut end = max_len;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}
