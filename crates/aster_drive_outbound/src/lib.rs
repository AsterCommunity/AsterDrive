//! Drive-owned transport policy. Provider protocols and credentials stay with callers.
mod plugin;
mod upload;
pub use plugin::{
    NetworkGrant, PluginHttpRequest, PluginHttpResponse, PluginLimits, PluginTransport,
    PluginTransportError,
};
pub use upload::{UploadError, send_upload};

use aster_forge_http::{HttpClient, HttpObservation, HttpObserver};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

/// Canonical product identity, shared by all Drive workspace packages at the same version.
/// Keep this in the outbound policy crate so transport does not depend on the main application.
pub const OUTBOUND_HTTP_USER_AGENT: &str = concat!("AsterDrive/", env!("CARGO_PKG_VERSION"));

/// Purpose is mandatory and participates in telemetry; profiles never permit redirects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    ProviderControl,
    ProviderDownload,
    ProviderUpload,
    InternalControl,
    InternalTransfer,
    /// Full-duplex tunnel proxy; cancellation/deadlines belong to the owning tunnel.
    InternalStreaming,
    Discovery,
    ExternalAuth,
    OfflineDownload,
    Plugin,
}

impl Profile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProviderControl => "provider_control",
            Self::ProviderDownload => "provider_download",
            Self::ProviderUpload => "provider_upload",
            Self::InternalControl => "internal_control",
            Self::InternalTransfer => "internal_transfer",
            Self::InternalStreaming => "internal_streaming",
            Self::Discovery => "discovery",
            Self::ExternalAuth => "external_auth",
            Self::OfflineDownload => "offline_download",
            Self::Plugin => "plugin",
        }
    }
}

/// Trusted built-in transport configuration. No raw builder or insecure-TLS switch.
/// Request-specific credentials must never be installed as default headers.
pub struct ClientConfig {
    profile: Profile,
    user_agent: String,
    connect_timeout: Duration,
    read_timeout: Option<Duration>,
    timeout: Option<Duration>,
    proxy: ProxyPolicy,
    roots: Vec<reqwest::Certificate>,
    pinned: Option<(String, Vec<SocketAddr>)>,
    observer: Option<Arc<dyn HttpObserver>>,
}

enum ProxyPolicy {
    System,
    Disabled,
    Explicit(Box<reqwest::Proxy>),
}

impl ClientConfig {
    /// Select a purpose and inherit the standard AsterDrive product identity.
    pub fn new(profile: Profile) -> Self {
        let (connect_timeout, read_timeout, timeout) = match profile {
            Profile::ProviderControl => (5, Some(30), Some(60)),
            Profile::ProviderDownload => (5, Some(30), None),
            // A caller-owned progress watchdog bounds streamed upload/acknowledgement.
            Profile::ProviderUpload => (5, None, None),
            Profile::InternalControl => (5, Some(30), Some(60)),
            Profile::InternalTransfer => (5, Some(30), Some(3600)),
            Profile::InternalStreaming => (5, None, None),
            Profile::Discovery => (5, Some(5), Some(5)),
            Profile::ExternalAuth => (5, Some(15), Some(15)),
            Profile::OfflineDownload => (5, None, Some(3600)),
            Profile::Plugin => (5, Some(15), Some(15)),
        };
        Self {
            profile,
            user_agent: OUTBOUND_HTTP_USER_AGENT.to_owned(),
            connect_timeout: Duration::from_secs(connect_timeout),
            read_timeout: read_timeout.map(Duration::from_secs),
            timeout: timeout.map(Duration::from_secs),
            proxy: if matches!(profile, Profile::OfflineDownload | Profile::Plugin) {
                ProxyPolicy::Disabled
            } else {
                ProxyPolicy::System
            },
            roots: Vec::new(),
            pinned: None,
            observer: None,
        }
    }

    /// Override product identity for this client without changing its transport profile.
    /// Invalid header values fail at build time, before any request is sent.
    pub fn user_agent(mut self, value: impl Into<String>) -> Self {
        self.user_agent = value.into();
        self
    }

    pub fn connect_timeout(mut self, value: Duration) -> Self {
        self.connect_timeout = value;
        self
    }

    pub fn read_timeout(mut self, value: Duration) -> Self {
        self.read_timeout = Some(value);
        self
    }

    pub fn timeout(mut self, value: Duration) -> Self {
        self.timeout = Some(value);
        self
    }

    /// Used only when preserving a reviewed streaming protocol's existing idle policy.
    pub fn without_read_timeout(mut self) -> Self {
        self.read_timeout = None;
        self
    }

    pub fn without_total_timeout(mut self) -> Self {
        self.timeout = None;
        self
    }

    pub fn no_proxy(mut self) -> Self {
        self.proxy = ProxyPolicy::Disabled;
        self
    }

    /// A trusted administrator's explicit proxy, never exposed to plugin input.
    pub fn proxy(mut self, proxy: reqwest::Proxy) -> Self {
        self.proxy = ProxyPolicy::Explicit(Box::new(proxy));
        self
    }

    /// Adds trust without disabling certificate or hostname verification.
    pub fn add_root_certificate(mut self, certificate: reqwest::Certificate) -> Self {
        self.roots.push(certificate);
        self
    }

    /// Pins a caller-validated destination. Disables proxies so DNS pinning is effective.
    pub fn resolve_to_addrs(mut self, host: &str, addrs: &[SocketAddr]) -> Self {
        self.pinned = Some((host.to_owned(), addrs.to_vec()));
        self.proxy = ProxyPolicy::Disabled;
        self
    }

    /// The sink sees only method/status/outcome/duration, never request secrets or URLs.
    pub fn observer(mut self, observer: Arc<dyn HttpObserver>) -> Self {
        self.observer = Some(observer);
        self
    }

    /// Builds one reusable pool for a policy/driver/runtime instance. Clone the client to share it.
    /// # Errors
    /// Returns reqwest's configuration error. No request has been sent.
    pub fn build(self) -> Result<HttpClient, reqwest::Error> {
        let mut builder = HttpClient::builder()
            .user_agent(self.user_agent)
            .connect_timeout(self.connect_timeout)
            .without_read_timeout()
            .without_total_timeout()
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(8);
        if let Some(value) = self.read_timeout {
            builder = builder.read_timeout(value);
        }
        if let Some(value) = self.timeout {
            builder = builder.timeout(value);
        }
        // DNS pinning is authoritative even if proxy() was called after resolve_to_addrs().
        // Forge enforces the same rule; express the product policy here as well.
        let proxy = if self.pinned.is_some() {
            ProxyPolicy::Disabled
        } else {
            self.proxy
        };
        builder = match proxy {
            ProxyPolicy::System => builder,
            ProxyPolicy::Disabled => builder.no_proxy(),
            ProxyPolicy::Explicit(proxy) => builder.proxy(*proxy),
        };
        for certificate in self.roots {
            builder = builder.add_root_certificate(certificate);
        }
        if let Some((host, addrs)) = self.pinned {
            builder = builder.resolve_to_addrs(&host, &addrs);
        }
        builder
            .observer(Arc::new(Observer {
                profile: self.profile,
                sink: self.observer,
            }))
            .build()
    }
}

struct Observer {
    profile: Profile,
    sink: Option<Arc<dyn HttpObserver>>,
}

impl HttpObserver for Observer {
    fn observe(&self, event: HttpObservation) {
        // Response-header duration is intentionally distinct from the file-transfer EOF metrics.
        tracing::debug!(target: "aster_drive_outbound", profile = self.profile.as_str(), method = %event.method,
            status = event.status, outcome = ?event.outcome, duration_seconds = event.duration.as_secs_f64(),
            "outbound HTTP exchange completed");
        if let Some(sink) = &self.sink {
            sink.observe(event);
        }
    }
}

#[cfg(test)]
mod policy_tests;
