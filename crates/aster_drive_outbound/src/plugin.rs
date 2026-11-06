//! Host-granted network access. The plugin never receives a pool or a builder.
use crate::{ClientConfig, Profile};
use futures::StreamExt;
use std::net::IpAddr;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use url::Url;

/// The host creates this only after approving a plugin's network:<origin> capability.
/// Construction validates the scope; it does not substitute for manifest authorization/audit.
#[derive(Clone, Debug)]
pub struct NetworkGrant {
    origin: Url,
}
impl NetworkGrant {
    /// Exact HTTPS origin, without wildcard, userinfo, path, query or fragment.
    /// # Errors
    /// Rejects malformed or non-origin grants.
    pub fn approved_origin(origin: &str) -> Result<Self, PluginTransportError> {
        let origin = Url::parse(origin).map_err(|_| PluginTransportError::Denied)?;
        if origin.scheme() != "https"
            || origin.host_str().is_none_or(|host| host.contains('*'))
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.path() != "/"
            || origin.query().is_some()
            || origin.fragment().is_some()
        {
            return Err(PluginTransportError::Denied);
        }
        Ok(Self { origin })
    }
    fn permits(&self, url: &Url) -> bool {
        url.origin() == self.origin.origin()
            && url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none()
    }
}

/// Host-selected resource budget. Plugins cannot increase it per request.
#[derive(Clone, Copy, Debug)]
pub struct PluginLimits {
    pub request_bytes: usize,
    pub response_bytes: usize,
    pub timeout: Duration,
}
impl Default for PluginLimits {
    fn default() -> Self {
        Self {
            request_bytes: 1024 * 1024,
            response_bytes: 1024 * 1024,
            timeout: Duration::from_secs(15),
        }
    }
}

pub struct PluginHttpRequest {
    pub method: reqwest::Method,
    pub url: String,
    pub headers: reqwest::header::HeaderMap,
    pub body: Vec<u8>,
}
pub struct PluginHttpResponse {
    pub status: u16,
    pub headers: reqwest::header::HeaderMap,
    pub body: Vec<u8>,
}

/// Safe errors intentionally contain no URL, headers, credentials, or upstream body.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PluginTransportError {
    #[error("plugin network capability denied")]
    Denied,
    #[error("plugin HTTP resource limit exceeded")]
    Limit,
    #[error("plugin HTTP request cancelled")]
    Cancelled,
    #[error("plugin HTTP deadline exceeded")]
    Timeout,
    #[error("plugin HTTP transport failed")]
    Transport,
}

pub struct PluginTransport {
    grant: NetworkGrant,
    limits: PluginLimits,
    user_agent: String,
    cancellation: CancellationToken,
}
impl PluginTransport {
    /// Cancellation belongs to the host invocation/runtime and covers DNS, headers and body.
    /// # Errors
    /// Rejects unbounded or excessive budgets before any network access.
    pub fn new(
        grant: NetworkGrant,
        limits: PluginLimits,
        cancellation: CancellationToken,
    ) -> Result<Self, PluginTransportError> {
        if limits.request_bytes == 0
            || limits.response_bytes == 0
            || limits.request_bytes > 16 * 1024 * 1024
            || limits.response_bytes > 16 * 1024 * 1024
            || limits.timeout.is_zero()
            || limits.timeout > Duration::from_secs(60)
        {
            return Err(PluginTransportError::Limit);
        }
        Ok(Self {
            grant,
            limits,
            user_agent: crate::OUTBOUND_HTTP_USER_AGENT.to_owned(),
            cancellation,
        })
    }

    /// Host-only identity override; guest request headers cannot change User-Agent.
    pub fn user_agent(mut self, value: impl Into<String>) -> Self {
        self.user_agent = value.into();
        self
    }

    /// Revalidates scope and public DNS answers for every request; redirects are never followed.
    /// # Errors
    /// Returns a redacted capability, resource, cancellation, timeout or transport error.
    pub async fn execute(
        &self,
        request: PluginHttpRequest,
    ) -> Result<PluginHttpResponse, PluginTransportError> {
        let url = Url::parse(&request.url).map_err(|_| PluginTransportError::Denied)?;
        self.validate_request(&request, &url)?;
        tokio::select! {
            biased;
            () = self.cancellation.cancelled() => Err(PluginTransportError::Cancelled),
            result = tokio::time::timeout(self.limits.timeout, self.exchange(request, url)) => result.map_err(|_| PluginTransportError::Timeout)?,
        }
    }
    fn validate_request(
        &self,
        request: &PluginHttpRequest,
        url: &Url,
    ) -> Result<(), PluginTransportError> {
        if !self.grant.permits(url)
            || !matches!(
                request.method,
                reqwest::Method::GET
                    | reqwest::Method::HEAD
                    | reqwest::Method::POST
                    | reqwest::Method::PUT
                    | reqwest::Method::PATCH
                    | reqwest::Method::DELETE
            )
            || request.headers.keys().any(|name| {
                matches!(
                    name.as_str(),
                    "host"
                        | "connection"
                        | "proxy-authorization"
                        | "proxy-connection"
                        | "transfer-encoding"
                        | "content-length"
                        | "upgrade"
                        | "te"
                        | "trailer"
                        | "user-agent"
                )
            })
        {
            return Err(PluginTransportError::Denied);
        }
        if request.body.len() > self.limits.request_bytes
            || request.headers.len() > 64
            || request
                .headers
                .iter()
                .map(|(name, value)| name.as_str().len() + value.len())
                .sum::<usize>()
                > 16 * 1024
        {
            return Err(PluginTransportError::Limit);
        }
        Ok(())
    }
    async fn exchange(
        &self,
        request: PluginHttpRequest,
        url: Url,
    ) -> Result<PluginHttpResponse, PluginTransportError> {
        let host = url.host_str().ok_or(PluginTransportError::Denied)?;
        let port = url
            .port_or_known_default()
            .ok_or(PluginTransportError::Denied)?;
        let addrs = match url.host().ok_or(PluginTransportError::Denied)? {
            url::Host::Ipv4(ip) => vec![std::net::SocketAddr::new(ip.into(), port)],
            url::Host::Ipv6(ip) => vec![std::net::SocketAddr::new(ip.into(), port)],
            url::Host::Domain(domain) => tokio::net::lookup_host((domain, port))
                .await
                .map_err(|_| PluginTransportError::Transport)?
                .collect::<Vec<_>>(),
        };
        if addrs.is_empty() || addrs.iter().any(|addr| !is_public_destination(addr.ip())) {
            return Err(PluginTransportError::Denied);
        }
        // A fresh pinned pool intentionally avoids reusing a connection across DNS authorization changes.
        let client = ClientConfig::new(Profile::Plugin)
            .user_agent(self.user_agent.clone())
            .timeout(self.limits.timeout)
            .resolve_to_addrs(host, &addrs)
            .build()
            .map_err(|_| PluginTransportError::Transport)?;
        let response = client
            .request(request.method, url)
            .headers(request.headers)
            .body(request.body)
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    PluginTransportError::Timeout
                } else {
                    PluginTransportError::Transport
                }
            })?;
        self.read_response(response).await
    }
    async fn read_response(
        &self,
        response: reqwest::Response,
    ) -> Result<PluginHttpResponse, PluginTransportError> {
        if response.status().is_redirection() {
            return Err(PluginTransportError::Denied);
        }
        if response
            .content_length()
            .is_some_and(|size| size > self.limits.response_bytes as u64)
        {
            return Err(PluginTransportError::Limit);
        }
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let mut stream = response.bytes_stream();
        let mut body = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| PluginTransportError::Transport)?;
            if chunk.len() > self.limits.response_bytes.saturating_sub(body.len()) {
                return Err(PluginTransportError::Limit);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(PluginHttpResponse {
            status,
            headers,
            body,
        })
    }
}

fn is_public_destination(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, _, _] = ip.octets();
            !(ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_multicast()
                || ip.is_broadcast()
                || ip.is_documentation()
                || a == 0
                || a >= 240
                || (a == 100 && b & 0xc0 == 0x40)
                || (a == 198 && (b == 18 || b == 19))
                || (a == 192 && b == 0)
                || (a == 192 && b == 88))
        }
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return is_public_destination(IpAddr::V4(mapped));
            }
            let segments = ip.segments();
            // Restrict to ordinary global unicast; reject translation/tunnel/special-purpose ranges.
            segments[0] & 0xe000 == 0x2000
                && segments[0] != 0x2002
                && !(segments[0] == 0x2001 && (segments[1] < 0x0200 || segments[1] == 0x0db8))
                && !(segments[0] == 0x3fff && segments[1] < 0x1000)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    fn transport(limits: PluginLimits, cancellation: CancellationToken) -> PluginTransport {
        PluginTransport::new(
            NetworkGrant::approved_origin("https://plugin.example").unwrap(),
            limits,
            cancellation,
        )
        .unwrap()
    }
    fn request(url: &str) -> PluginHttpRequest {
        PluginHttpRequest {
            method: reqwest::Method::POST,
            url: url.into(),
            headers: reqwest::header::HeaderMap::new(),
            body: Vec::new(),
        }
    }
    #[test]
    fn host_transport_inherits_product_identity_and_accepts_an_explicit_override() {
        let transport = transport(PluginLimits::default(), CancellationToken::new());
        assert_eq!(transport.user_agent, crate::OUTBOUND_HTTP_USER_AGENT);
        assert_eq!(
            transport.user_agent("PluginHost/custom").user_agent,
            "PluginHost/custom"
        );
    }

    #[test]
    fn grant_is_an_exact_https_origin_not_a_url_or_wildcard() {
        for origin in [
            "http://example.com",
            "https://user:secret@example.com",
            "https://example.com/path",
            "https://example.com/?q=secret",
            "https://example.com/#fragment",
            "https://*.example.com",
        ] {
            // URL parsers accept a wildcard as a domain; deny it explicitly at the grant boundary.
            assert!(NetworkGrant::approved_origin(origin).is_err(), "{origin}");
        }
        let grant = NetworkGrant::approved_origin("https://example.com:8443").unwrap();
        assert!(grant.permits(&Url::parse("https://example.com:8443/path?q=value").unwrap()));
        for url in [
            "https://example.com/path",
            "https://sub.example.com:8443/path",
            "https://example.com:8443@evil.example/path",
        ] {
            assert!(!grant.permits(&Url::parse(url).unwrap()));
        }
    }
    #[tokio::test]
    async fn capability_and_size_rejections_precede_dns_or_socket_io() {
        let transport = transport(
            PluginLimits {
                request_bytes: 2,
                ..PluginLimits::default()
            },
            CancellationToken::new(),
        );
        assert_eq!(
            transport
                .execute(request("https://unapproved.invalid"))
                .await
                .err(),
            Some(PluginTransportError::Denied)
        );
        let mut input = request("https://plugin.example/path");
        input.body = b"123".to_vec();
        assert_eq!(
            transport.execute(input).await.err(),
            Some(PluginTransportError::Limit)
        );
        let mut input = request("https://plugin.example/path");
        input
            .headers
            .insert("host", "internal.service".parse().unwrap());
        assert_eq!(
            transport.execute(input).await.err(),
            Some(PluginTransportError::Denied)
        );
        let mut input = request("https://plugin.example/path");
        input.method = reqwest::Method::CONNECT;
        assert_eq!(
            transport.execute(input).await.err(),
            Some(PluginTransportError::Denied)
        );
    }
    #[tokio::test]
    async fn cancellation_and_private_addresses_fail_without_an_http_request() {
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let transport = transport(PluginLimits::default(), cancellation);
        assert_eq!(
            transport
                .execute(request("https://plugin.example/path"))
                .await
                .err(),
            Some(PluginTransportError::Cancelled)
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("https://{}", listener.local_addr().unwrap());
        let transport = PluginTransport::new(
            NetworkGrant::approved_origin(&url).unwrap(),
            PluginLimits::default(),
            CancellationToken::new(),
        )
        .unwrap();
        assert_eq!(
            transport.execute(request(&url)).await.err(),
            Some(PluginTransportError::Denied)
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(30), listener.accept())
                .await
                .is_err()
        );
    }
    #[test]
    fn special_address_families_cannot_bypass_public_egress_policy() {
        use std::net::{Ipv4Addr, Ipv6Addr};
        let blocked_v4 = [
            [127, 0, 0, 1],
            [10, 2, 1, 4],
            [169, 254, 169, 254],
            [100, 67, 6, 8],
            [192, 0, 0, 1],
            [198, 18, 0, 1],
            [240, 0, 0, 1],
        ];
        for octets in blocked_v4 {
            assert!(!is_public_destination(Ipv4Addr::from(octets).into()));
        }
        for ip in ["::1", "::ffff:127.0.0.1", "fc00::1", "fe80::1"] {
            assert!(!is_public_destination(ip.parse().unwrap()));
        }
        assert!(!is_public_destination(
            Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1).into()
        ));
        for octets in [[8, 8, 8, 8], [1, 1, 1, 1]] {
            assert!(is_public_destination(Ipv4Addr::from(octets).into()));
        }
        assert!(is_public_destination(
            Ipv6Addr::new(0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888).into()
        ));
    }
    async fn response(bytes: &'static [u8]) -> reqwest::Response {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0; 2048];
            let _ = socket.read(&mut buf).await;
            socket.write_all(bytes).await.unwrap();
        });
        aster_forge_http::HttpClient::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(url)
            .send()
            .await
            .unwrap()
    }
    #[tokio::test]
    async fn body_limit_is_enforced_on_actual_chunks_and_declared_length() {
        let transport = transport(
            PluginLimits {
                response_bytes: 4,
                ..PluginLimits::default()
            },
            CancellationToken::new(),
        );
        let response =
            response(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nbody")
                .await;
        assert_eq!(
            transport.read_response(response).await.unwrap().body,
            b"body"
        );
        for bytes in [
            b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\n12345".as_slice(),
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\n12345\r\n0\r\n\r\n".as_slice(),
        ] {
            assert_eq!(transport.read_response(self::response(bytes).await).await.err(),Some(PluginTransportError::Limit));
        }
        assert_eq!(transport.read_response(self::response(b"HTTP/1.1 302 Found\r\nLocation: https://other.example\r\nContent-Length: 0\r\n\r\n").await).await.err(),Some(PluginTransportError::Denied));
    }
}
