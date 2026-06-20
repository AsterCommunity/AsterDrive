//! 配置子模块：`schema`。

use serde::{Deserialize, Serialize};
use std::num::{NonZeroU32, NonZeroU64};

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub webdav: WebDavConfig,
    #[serde(default)]
    pub network_trust: NetworkTrustConfig,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
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
    #[serde(default)]
    pub follower: ServerFollowerConfig,
    /// 节点静态启动角色。改动后需要重启进程。
    #[serde(default)]
    pub start_mode: crate::config::node_mode::NodeRuntimeMode,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServerFollowerConfig {
    /// follower 受 primary 托管的 local ingress profile 根目录。
    /// primary 下发的本地落点只能在这个根目录下使用相对路径。
    #[serde(default = "ServerFollowerConfig::default_managed_ingress_local_root")]
    pub managed_ingress_local_root: String,
}

impl Default for ServerFollowerConfig {
    fn default() -> Self {
        Self {
            managed_ingress_local_root: Self::default_managed_ingress_local_root(),
        }
    }
}

impl ServerFollowerConfig {
    fn default_managed_ingress_local_root() -> String {
        "managed-ingress".to_string()
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
        crate::utils::paths::DEFAULT_CONFIG_TEMP_DIR.to_string()
    }
    fn default_upload_temp_dir() -> String {
        crate::utils::paths::DEFAULT_CONFIG_UPLOAD_TEMP_DIR.to_string()
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DatabaseConfig {
    #[serde(default = "DatabaseConfig::default_url")]
    pub url: String,
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
    fn default_url() -> String {
        crate::utils::paths::DEFAULT_CONFIG_SQLITE_DATABASE_URL.to_string()
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
    /// 首次初始化 system_config 时，是否把 auth_cookie_secure 设为 false。
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
            bootstrap_insecure_cookies: Self::default_bootstrap_insecure_cookies(),
        }
    }
}

impl AuthConfig {
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
    fn default_bootstrap_insecure_cookies() -> bool {
        false
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CacheConfig {
    #[serde(default = "CacheConfig::default_backend")]
    pub backend: String, // "memory" | "redis"
    #[serde(default)]
    pub redis_url: String,
    #[serde(default = "CacheConfig::default_ttl")]
    pub default_ttl: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            backend: Self::default_backend(),
            redis_url: String::new(),
            default_ttl: Self::default_ttl(),
        }
    }
}

impl CacheConfig {
    fn default_backend() -> String {
        "memory".to_string()
    }
    fn default_ttl() -> u64 {
        3600
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LoggingConfig {
    #[serde(default = "LoggingConfig::default_level")]
    pub level: String,
    #[serde(default = "LoggingConfig::default_format")]
    pub format: String, // "text" | "json"
    #[serde(default)]
    pub file: String, // 留空 = stdout only
    /// 启用日志轮转（按天），仅在 file 非空时生效
    #[serde(default = "LoggingConfig::default_enable_rotation")]
    pub enable_rotation: bool,
    /// 保留的历史日志文件数量
    #[serde(default = "LoggingConfig::default_max_backups")]
    pub max_backups: u32,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: Self::default_level(),
            format: Self::default_format(),
            file: String::new(),
            enable_rotation: Self::default_enable_rotation(),
            max_backups: Self::default_max_backups(),
        }
    }
}

impl LoggingConfig {
    fn default_level() -> String {
        "info".to_string()
    }
    fn default_format() -> String {
        "text".to_string()
    }
    fn default_enable_rotation() -> bool {
        true
    }
    fn default_max_backups() -> u32 {
        5
    }
}

/// WebDAV 静态配置（config.toml）
///
/// 运行时配置通过 system_config 表管理：
/// - `webdav_enabled`: 是否启用 (默认 "true")
/// - `webdav_max_upload_size`: 软上传限制字节数 (默认 "1073741824" = 1GB)
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WebDavConfig {
    /// 路由前缀，改了要重启
    #[serde(default = "WebDavConfig::default_prefix")]
    pub prefix: String,
    /// actix payload 硬上限，改了要重启。运行时软限制从 DB 读。
    #[serde(default = "WebDavConfig::default_payload_limit")]
    pub payload_limit: u64,
    /// XML 类 WebDAV 请求体上限，改了要重启。仅用于 REPORT/PROPFIND/PROPPATCH/LOCK。
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
            seconds_per_request: NonZeroU64::new(2).expect("rate limit default must be non-zero"),
            burst_size: NonZeroU32::new(5).expect("rate limit default must be non-zero"),
        }
    }
    fn default_public() -> RateLimitTier {
        RateLimitTier {
            seconds_per_request: NonZeroU64::new(1).expect("rate limit default must be non-zero"),
            burst_size: NonZeroU32::new(30).expect("rate limit default must be non-zero"),
        }
    }
    fn default_api() -> RateLimitTier {
        RateLimitTier {
            seconds_per_request: NonZeroU64::new(1).expect("rate limit default must be non-zero"),
            burst_size: NonZeroU32::new(120).expect("rate limit default must be non-zero"),
        }
    }
    fn default_write() -> RateLimitTier {
        RateLimitTier {
            seconds_per_request: NonZeroU64::new(2).expect("rate limit default must be non-zero"),
            burst_size: NonZeroU32::new(10).expect("rate limit default must be non-zero"),
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
        NonZeroU64::new(1).expect("rate limit default must be non-zero")
    }
    fn default_burst() -> NonZeroU32 {
        NonZeroU32::new(60).expect("rate limit default must be non-zero")
    }
}
