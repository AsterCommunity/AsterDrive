use crate::config::TrustedClientNetworks;
use crate::services::auth::local::Claims;
use actix_web::HttpRequest;

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

    /// IP 一律经 `TrustedClientNetworks`（app data 注入）解析：来自不可信 peer 的
    /// 转发头被忽略，只记 TCP peer 地址。注入缺失时（如部分单测构造的请求）同样
    /// 只信 peer，绝不回退到客户端可伪造的转发头。
    pub fn from_request(req: &HttpRequest, claims: &Claims) -> Self {
        AuditRequestInfo::from_request(req).to_context(claims.user_id)
    }
}

impl AuditRequestInfo {
    pub fn from_request(req: &HttpRequest) -> Self {
        let ip_address = req
            .app_data::<actix_web::web::Data<TrustedClientNetworks>>()
            .and_then(|trusted| trusted.real_ip(req))
            .or_else(|| req.peer_addr().map(|addr| addr.ip()))
            .map(|ip| bounded_audit_value(&ip.to_string(), MAX_AUDIT_IP_ADDRESS_LEN));
        Self {
            ip_address,
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

#[cfg(test)]
mod tests {
    use actix_web::test as actix_test;
    use actix_web::web;

    use super::{AuditRequestInfo, MAX_AUDIT_IP_ADDRESS_LEN, MAX_AUDIT_USER_AGENT_LEN};
    use crate::config::{NetworkTrustConfig, TrustedClientNetworks};

    fn with_trusted(entries: &[&str]) -> web::Data<TrustedClientNetworks> {
        let config = NetworkTrustConfig {
            trusted_proxies: entries.iter().map(|s| s.to_string()).collect(),
        };
        web::Data::new(TrustedClientNetworks::from_config(&config))
    }

    #[test]
    fn forwarded_header_is_ignored_without_injected_trust() {
        let req = actix_test::TestRequest::default()
            .peer_addr("198.51.100.4:40000".parse().unwrap())
            .insert_header(("X-Forwarded-For", "203.0.113.7"))
            .to_http_request();

        let info = AuditRequestInfo::from_request(&req);

        assert_eq!(
            info.ip_address.as_deref(),
            Some("198.51.100.4"),
            "without injected trust the TCP peer wins, never the client-supplied header"
        );
    }

    #[test]
    fn forwarded_header_wins_only_from_a_trusted_peer() {
        let trusted = with_trusted(&["10.0.0.0/8"]);
        let req = actix_test::TestRequest::default()
            .app_data(trusted)
            .peer_addr("10.0.0.10:40000".parse().unwrap())
            .insert_header(("X-Forwarded-For", "203.0.113.7"))
            .to_http_request();

        let info = AuditRequestInfo::from_request(&req);

        assert_eq!(info.ip_address.as_deref(), Some("203.0.113.7"));
    }

    #[test]
    fn forwarded_header_from_an_untrusted_peer_is_dropped() {
        let trusted = with_trusted(&["10.0.0.0/8"]);
        let req = actix_test::TestRequest::default()
            .app_data(trusted)
            .peer_addr("198.51.100.4:40000".parse().unwrap())
            .insert_header(("X-Forwarded-For", "203.0.113.7"))
            .to_http_request();

        let info = AuditRequestInfo::from_request(&req);

        assert_eq!(
            info.ip_address.as_deref(),
            Some("198.51.100.4"),
            "a spoofed forwarded header from outside the trust scope must not reach audit rows"
        );
    }

    #[test]
    fn peer_ip_is_bounded_like_any_other_audit_value() {
        let long = "1".repeat(MAX_AUDIT_IP_ADDRESS_LEN + 8);
        let trusted = with_trusted(&[long.as_str()]);
        let req = actix_test::TestRequest::default()
            .app_data(trusted)
            .peer_addr("127.0.0.1:40000".parse().unwrap())
            .to_http_request();

        let info = AuditRequestInfo::from_request(&req);

        assert_eq!(
            info.ip_address.as_deref(),
            Some("127.0.0.1"),
            "IPs come from typed addresses, so unparseable trusted entries cannot plant values"
        );
    }

    #[test]
    fn user_agent_is_truncated_without_splitting_utf8() {
        let long_user_agent = "a".repeat(MAX_AUDIT_USER_AGENT_LEN + 32);
        let req = actix_test::TestRequest::default()
            .insert_header(("User-Agent", long_user_agent.as_str()))
            .to_http_request();

        let info = AuditRequestInfo::from_request(&req);

        assert_eq!(
            info.user_agent.as_deref(),
            Some(&long_user_agent[..MAX_AUDIT_USER_AGENT_LEN])
        );
    }
}
