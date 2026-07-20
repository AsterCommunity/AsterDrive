//! 配置模块导出与全局入口。

pub mod audit;
pub mod auth_runtime;
pub mod avatar;
pub mod bool_like;
pub mod branding;
pub mod cors;
pub mod definitions;
pub mod deployment;
mod loader;
pub mod local_email_policy;
pub mod mail;
pub mod media_processing;
pub mod node_mode;
pub mod offline_download;
pub mod operations;
pub(crate) mod paths;
mod runtime_config;
mod schema;
pub mod site_url;
pub mod system_config;
pub mod webdav;
pub mod wopi;

pub use deployment::{DeploymentConfig, DeploymentProfile};
pub use loader::ConfigLoadReport;
pub use runtime_config::RuntimeConfig;
pub use schema::{
    AuthConfig, Config, DatabaseConfig, NetworkTrustConfig, RateLimitConfig, RateLimitTier,
    ServerConfig, ServerFollowerConfig, WebDavConfig,
};

use std::sync::Arc;
use std::sync::OnceLock;

static CONFIG: OnceLock<Arc<Config>> = OnceLock::new();

pub const OUTBOUND_HTTP_USER_AGENT: &str = concat!("AsterDrive/", env!("CARGO_PKG_VERSION"));

pub fn ensure_default_config_for_current_dir(
    default: &Config,
) -> crate::errors::Result<std::path::PathBuf> {
    loader::ensure_default_config_for_current_dir(default)
}

pub fn init_config() -> crate::errors::Result<ConfigLoadReport> {
    let loaded = loader::load()?;
    CONFIG.get_or_init(|| Arc::new(loaded.config));
    Ok(loaded.report)
}

pub fn load_config_read_only() -> crate::errors::Result<Config> {
    loader::load_read_only()
}

#[allow(clippy::expect_used)]
pub fn get_config() -> Arc<Config> {
    CONFIG
        .get()
        .expect("Config not initialized. Call init_config() first.")
        .clone()
}

/// 尝试获取配置，未初始化时返回 None（用于可选功能如 WebDAV 在测试环境下跳过）
pub fn try_get_config() -> Option<Arc<Config>> {
    CONFIG.get().cloned()
}

/// 测试环境用：手动设置全局配置（OnceLock 只接受第一次调用）
pub fn set_config_for_test(config: Arc<Config>) -> Result<(), Arc<Config>> {
    CONFIG.set(config)
}
