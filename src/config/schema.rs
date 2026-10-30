//! 配置子模块：`schema`。

use serde::{Deserialize, Serialize};
use std::num::{NonZeroU32, NonZeroU64};

use aster_forge_cache::CacheConfig;
use aster_forge_config::ConfigSyncConfig;
use aster_forge_logging::LoggingConfig;

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub observability: crate::config::metrics::ObservabilityConfig,
    #[serde(default)]
    pub deployment: crate::config::deployment::DeploymentConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default = "default_config_sync")]
    pub config_sync: ConfigSyncConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub webdav: WebDavConfig,
    #[serde(default)]
    pub network_trust: NetworkTrustConfig,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
}

fn default_config_sync() -> ConfigSyncConfig {
    ConfigSyncConfig {
        backend: aster_forge_config::CONFIG_SYNC_BACKEND_DISABLED.to_string(),
        topic: "aster_drive.config_reload".to_string(),
        ..ConfigSyncConfig::default()
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServerConfig {
    #[serde(default = "ServerConfig::default_host")]
    pub host: String,
    #[serde(default = "ServerConfig::default_port")]
    pub port: u16,
    /// 0 = num_cpus
    #[serde(default)]
    pub workers: usize,
    #[serde(default = "ServerConfig::default_temp_dir")]
    pub temp_dir: String,
    #[serde(default = "ServerConfig::default_upload_temp_dir")]
    pub upload_temp_dir: String,
    /// Bytes kept free on the upload staging filesystem after each physical reservation.
    #[serde(default = "ServerConfig::default_upload_temp_min_free_bytes")]
    pub upload_temp_min_free_bytes: u64,
    #[serde(default)]
    pub follower: ServerFollowerConfig,
    /// 节点静态启动角色。改动后需要重启进程。
    #[serde(default)]
    pub start_mode: crate::config::node_mode::NodeRuntimeMode,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServerFollowerConfig {
    /// follower 受 primary 托管的 local remote storage target 根目录。
    /// primary 下发的本地落点只能在这个根目录下使用相对路径。
    #[serde(
        default = "ServerFollowerConfig::default_remote_storage_target_local_root",
        alias = "managed_ingress_local_root"
    )]
    pub remote_storage_target_local_root: String,
}

impl Default for ServerFollowerConfig {
    fn default() -> Self {
        Self {
            remote_storage_target_local_root: Self::default_remote_storage_target_local_root(),
        }
    }
}

impl ServerFollowerConfig {
    fn default_remote_storage_target_local_root() -> String {
        "remote-storage-targets".to_string()
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: Self::default_host(),
            port: Self::default_port(),
            workers: 0,
            temp_dir: Self::default_temp_dir(),
            upload_temp_dir: Self::default_upload_temp_dir(),
            upload_temp_min_free_bytes: Self::default_upload_temp_min_free_bytes(),
            follower: ServerFollowerConfig::default(),
            start_mode: crate::config::node_mode::NodeRuntimeMode::Primary,
        }
    }
}

impl ServerConfig {
    fn default_host() -> String {
        "127.0.0.1".to_string()
    }
    fn default_port() -> u16 {
        3000
    }
    fn default_temp_dir() -> String {
        crate::config::paths::DEFAULT_CONFIG_TEMP_DIR.to_string()
    }
    fn default_upload_temp_dir() -> String {
        crate::config::paths::DEFAULT_CONFIG_UPLOAD_TEMP_DIR.to_string()
    }
    fn default_upload_temp_min_free_bytes() -> u64 {
        256 * 1024 * 1024
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DatabaseConfig {
    #[serde(default = "DatabaseConfig::default_url")]
    pub url: aster_forge_db::DatabaseUrl,
    #[serde(default = "DatabaseConfig::default_pool_size")]
    pub pool_size: u32,
    #[serde(default = "DatabaseConfig::default_retry_count")]
    pub retry_count: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: Self::default_url(),
            pool_size: Self::default_pool_size(),
            retry_count: Self::default_retry_count(),
        }
    }
}

impl DatabaseConfig {
    fn default_url() -> aster_forge_db::DatabaseUrl {
        crate::config::paths::DEFAULT_CONFIG_SQLITE_DATABASE_URL.into()
    }
    fn default_pool_size() -> u32 {
        10
    }
    fn default_retry_count() -> u32 {
        3
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AuthConfig {
    #[serde(default = "AuthConfig::default_jwt_secret")]
    pub jwt_secret: String,
    #[serde(default = "AuthConfig::default_share_cookie_secret")]
    pub share_cookie_secret: String,
    #[serde(default = "AuthConfig::default_direct_link_secret")]
    pub direct_link_secret: String,
    pub mfa_secret_key: String,
    pub storage_credential_secret_key: String,
    pub webdav_auth_cache_secret: String,
    #[serde(default = "AuthConfig::default_password_hash_max_concurrency")]
    pub password_hash_max_concurrency: usize,
    /// 首次初始化 system_config 时，是否把 auth_cookie_secure 设为 false；默认允许 HTTP 首启。
    #[serde(default = "AuthConfig::default_bootstrap_insecure_cookies")]
    pub bootstrap_insecure_cookies: bool,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt_secret: Self::default_jwt_secret(),
            share_cookie_secret: Self::default_share_cookie_secret(),
            direct_link_secret: Self::default_direct_link_secret(),
            mfa_secret_key: Self::default_mfa_secret_key(),
            storage_credential_secret_key: Self::default_storage_credential_secret_key(),
            webdav_auth_cache_secret: Self::default_webdav_auth_cache_secret(),
            password_hash_max_concurrency: Self::default_password_hash_max_concurrency(),
            bootstrap_insecure_cookies: Self::default_bootstrap_insecure_cookies(),
        }
    }
}

impl AuthConfig {
    pub(crate) fn validate(&self) -> crate::errors::Result<()> {
        let minimum = aster_forge_auth::JwtAlgorithm::Hs256.minimum_key_bytes();
        let actual = self.jwt_secret.len();
        if actual < minimum {
            return Err(crate::errors::AsterError::config_error(format!(
                "auth.jwt_secret must be at least {minimum} bytes for HS256 (got {actual} bytes)"
            )));
        }
        Ok(())
    }

    fn random_hex_secret() -> String {
        use rand::RngExt;
        let mut rng = rand::rng();
        let bytes: [u8; 32] = rng.random();
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
    fn default_jwt_secret() -> String {
        Self::random_hex_secret()
    }
    fn default_share_cookie_secret() -> String {
        Self::random_hex_secret()
    }
    fn default_direct_link_secret() -> String {
        Self::random_hex_secret()
    }
    fn default_mfa_secret_key() -> String {
        Self::random_hex_secret()
    }
    fn default_storage_credential_secret_key() -> String {
        Self::random_hex_secret()
    }
    fn default_webdav_auth_cache_secret() -> String {
        Self::random_hex_secret()
    }
    fn default_password_hash_max_concurrency() -> usize {
        aster_forge_auth::DEFAULT_PASSWORD_HASH_MAX_CONCURRENCY
    }
    fn default_bootstrap_insecure_cookies() -> bool {
        true
    }
}

/// WebDAV 静态配置（config.toml）
///
/// 运行时配置通过 system_config 表管理：
/// - `webdav_enabled`: 是否启用 (默认 "true")
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WebDavConfig {
    /// 路由前缀，改了要重启
    #[serde(default = "WebDavConfig::default_prefix")]
    pub prefix: String,
    /// PUT 累计接收字节硬上限，包含 chunked / 未知长度请求；改动需要重启。
    /// 存储策略与 workspace 剩余配额会进一步限制接收大小。
    #[serde(default = "WebDavConfig::default_payload_limit")]
    pub payload_limit: u64,
    /// XML 类 WebDAV 请求体上限，改了要重启。用于 REPORT/PROPFIND/PROPPATCH/LOCK/VERSION-CONTROL。
    #[serde(default = "WebDavConfig::default_xml_payload_limit")]
    pub xml_payload_limit: u64,
}

impl Default for WebDavConfig {
    fn default() -> Self {
        Self {
            prefix: Self::default_prefix(),
            payload_limit: Self::default_payload_limit(),
            xml_payload_limit: Self::default_xml_payload_limit(),
        }
    }
}

impl WebDavConfig {
    fn default_prefix() -> String {
        "/webdav".to_string()
    }
    fn default_payload_limit() -> u64 {
        10_737_418_240 // 10 GB 硬上限
    }
    fn default_xml_payload_limit() -> u64 {
        1_048_576 // 1 MiB XML 请求体上限
    }
}

/// 网络信任配置（config.toml）
///
/// 这组受信代理信息会影响真实客户端 IP 的判定，供限流、认证审计等模块共用。
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct NetworkTrustConfig {
    /// 受信任的上游代理 IP 列表（CIDR 格式或单 IP）。
    #[serde(default)]
    pub trusted_proxies: Vec<String>,
}

/// `NetworkTrustConfig` 的解析产物：App 构建时转换一次，经 `app_data` 注入，
/// 请求侧按可信代理网络解析真实客户端 IP；来自不可信 peer 的
/// `X-Forwarded-For` 一律忽略，只记 TCP peer 地址。
#[derive(Clone)]
pub struct TrustedClientNetworks {
    proxies: std::sync::Arc<[ipnet::IpNet]>,
}

impl TrustedClientNetworks {
    /// 坏条目解析失败时由 Forge helper 记 warn 并跳过，不阻断启动。
    pub fn from_config(config: &NetworkTrustConfig) -> Self {
        Self {
            proxies: aster_forge_utils::net::parse_trusted_proxies(&config.trusted_proxies).into(),
        }
    }

    /// 空列表 = 无反代或未配置：真实客户端就是 TCP peer。
    pub fn real_ip(&self, req: &actix_web::HttpRequest) -> Option<std::net::IpAddr> {
        let peer = req.peer_addr()?.ip();
        Some(
            aster_forge_middleware::actix::client_ip::real_ip_from_trusted_headers(
                req.headers(),
                peer,
                &self.proxies,
            ),
        )
    }
}

/// Rate limiting 配置
///
/// 四个层级，不同接口类别不同阈值：
/// - `auth`: 认证/密码验证（最严格，防暴力破解）
/// - `public`: 公开分享匿名访问
/// - `api`: 已认证一般读写操作
/// - `write`: 高成本写操作（批量/管理）
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RateLimitConfig {
    #[serde(default = "RateLimitConfig::default_enabled")]
    pub enabled: bool,
    #[serde(default = "RateLimitConfig::default_auth")]
    pub auth: RateLimitTier,
    #[serde(default = "RateLimitConfig::default_public")]
    pub public: RateLimitTier,
    #[serde(default = "RateLimitConfig::default_api")]
    pub api: RateLimitTier,
    #[serde(default = "RateLimitConfig::default_write")]
    pub write: RateLimitTier,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: Self::default_enabled(),
            auth: Self::default_auth(),
            public: Self::default_public(),
            api: Self::default_api(),
            write: Self::default_write(),
        }
    }
}

impl RateLimitConfig {
    fn default_enabled() -> bool {
        false
    }
    fn default_auth() -> RateLimitTier {
        RateLimitTier {
            seconds_per_request: nonzero_u64_or_min(2),
            burst_size: nonzero_u32_or_min(5),
        }
    }
    fn default_public() -> RateLimitTier {
        RateLimitTier {
            seconds_per_request: nonzero_u64_or_min(1),
            burst_size: nonzero_u32_or_min(30),
        }
    }
    fn default_api() -> RateLimitTier {
        RateLimitTier {
            seconds_per_request: nonzero_u64_or_min(1),
            burst_size: nonzero_u32_or_min(120),
        }
    }
    fn default_write() -> RateLimitTier {
        RateLimitTier {
            seconds_per_request: nonzero_u64_or_min(2),
            burst_size: nonzero_u32_or_min(10),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RateLimitTier {
    #[serde(default = "RateLimitTier::default_seconds")]
    pub seconds_per_request: NonZeroU64,
    #[serde(default = "RateLimitTier::default_burst")]
    pub burst_size: NonZeroU32,
}

impl Default for RateLimitTier {
    fn default() -> Self {
        Self {
            seconds_per_request: Self::default_seconds(),
            burst_size: Self::default_burst(),
        }
    }
}

impl RateLimitTier {
    fn default_seconds() -> NonZeroU64 {
        nonzero_u64_or_min(1)
    }
    fn default_burst() -> NonZeroU32 {
        nonzero_u32_or_min(60)
    }
}

fn nonzero_u64_or_min(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).unwrap_or(NonZeroU64::MIN)
}

fn nonzero_u32_or_min(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).unwrap_or(NonZeroU32::MIN)
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn upload_staging_safety_floor_defaults_and_accepts_zero() {
        let default: Config = toml::from_str("").unwrap();
        assert_eq!(default.server.upload_temp_min_free_bytes, 256 * 1024 * 1024);

        let disabled: Config =
            toml::from_str("[server]\nupload_temp_min_free_bytes = 0\n").unwrap();
        assert_eq!(disabled.server.upload_temp_min_free_bytes, 0);

        assert!(
            toml::from_str::<Config>("[server]\nupload_temp_min_free_bytes = -1\n").is_err(),
            "negative safety floors must be rejected by the static config schema"
        );
    }
}

#[cfg(test)]
mod trusted_networks_tests {
    use actix_web::test as actix_test;

    use super::{NetworkTrustConfig, TrustedClientNetworks};

    fn config_for(entries: &[&str]) -> NetworkTrustConfig {
        NetworkTrustConfig {
            trusted_proxies: entries.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn from_config_skips_invalid_entries_and_accepts_plain_ips() {
        let trusted = TrustedClientNetworks::from_config(&config_for(&[
            "10.0.0.0/8",
            "not-a-network",
            "172.16.5.9",
        ]));
        let req = actix_test::TestRequest::default()
            .peer_addr("10.1.2.3:40000".parse().unwrap())
            .insert_header(("X-Forwarded-For", "203.0.113.7"))
            .to_http_request();
        assert_eq!(
            trusted.real_ip(&req),
            Some(std::net::IpAddr::from([203, 0, 113, 7])),
            "plain IP entries and CIDR entries both count as trusted proxies"
        );
    }

    #[test]
    fn spoofed_chain_behind_an_untrusted_proxy_never_wins() {
        let trusted = TrustedClientNetworks::from_config(&config_for(&["10.0.0.0/8"]));
        let req = actix_test::TestRequest::default()
            .peer_addr("10.0.0.5:40000".parse().unwrap())
            .insert_header(("X-Forwarded-For", "6.6.6.6, 198.51.100.2"))
            .to_http_request();
        assert_eq!(
            trusted.real_ip(&req),
            Some(std::net::IpAddr::from([198, 51, 100, 2])),
            "right-to-left walk must stop at the first untrusted hop, not the forged leftmost"
        );
    }

    #[test]
    fn trusted_proxy_chain_walks_from_the_peer_and_stops_at_first_untrusted_hop() {
        let trusted =
            TrustedClientNetworks::from_config(&config_for(&["10.0.0.0/8", "172.16.0.0/12"]));
        let req = actix_test::TestRequest::default()
            .peer_addr("10.0.0.5:40000".parse().unwrap())
            .insert_header(("X-Forwarded-For", "6.6.6.6, 198.51.100.2, 172.16.0.4"))
            .to_http_request();

        assert_eq!(
            trusted.real_ip(&req),
            Some(std::net::IpAddr::from([198, 51, 100, 2]))
        );
    }

    #[test]
    fn malformed_forwarded_chain_fails_closed_to_the_tcp_peer() {
        let trusted = TrustedClientNetworks::from_config(&config_for(&["10.0.0.0/8"]));
        let req = actix_test::TestRequest::default()
            .peer_addr("10.0.0.5:40000".parse().unwrap())
            .insert_header(("X-Forwarded-For", "203.0.113.7, invalid, 10.0.0.9"))
            .to_http_request();

        assert_eq!(
            trusted.real_ip(&req),
            Some(std::net::IpAddr::from([10, 0, 0, 5]))
        );
    }

    #[test]
    fn empty_config_pins_real_ip_to_the_tcp_peer() {
        let trusted = TrustedClientNetworks::from_config(&config_for(&[]));
        let req = actix_test::TestRequest::default()
            .peer_addr("198.51.100.4:40000".parse().unwrap())
            .insert_header(("X-Forwarded-For", "203.0.113.7"))
            .to_http_request();
        assert_eq!(
            trusted.real_ip(&req),
            Some(std::net::IpAddr::from([198, 51, 100, 4])),
            "without trusted proxies the forwarded header must never win"
        );
    }

    #[test]
    fn missing_peer_address_yields_no_ip() {
        let trusted = TrustedClientNetworks::from_config(&config_for(&["10.0.0.0/8"]));
        let req = actix_test::TestRequest::default()
            .insert_header(("X-Forwarded-For", "203.0.113.7"))
            .to_http_request();
        assert_eq!(trusted.real_ip(&req), None);
    }
}
