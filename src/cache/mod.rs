//! 缓存抽象与实现导出。

mod memory;
mod redis_cache;
mod reservation;

use crate::config::CacheConfig;
use crate::errors::Result;
use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};
use std::sync::Arc;

fn redis_backend_target(redis_url: &str) -> String {
    let Some((scheme, rest)) = redis_url.split_once("://") else {
        return "configured".to_string();
    };

    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit('@').next().unwrap_or(authority);
    if host.is_empty() {
        format!("{scheme}://configured")
    } else {
        format!("{scheme}://{host}")
    }
}

/// 通用缓存后端 trait（支持 trait object，统一使用 bytes 接口）
#[async_trait]
pub trait CacheBackend: Send + Sync {
    fn backend_name(&self) -> &'static str;
    async fn health_check(&self) -> Result<()>;
    async fn get_bytes(&self, key: &str) -> Option<Vec<u8>>;
    async fn take_bytes(&self, key: &str) -> Option<Vec<u8>>;
    async fn set_bytes(&self, key: &str, value: Vec<u8>, ttl_secs: Option<u64>);
    async fn set_bytes_if_absent(&self, key: &str, value: Vec<u8>, ttl_secs: Option<u64>) -> bool;
    async fn delete(&self, key: &str);
    async fn delete_many(&self, keys: &[String]) {
        for key in keys {
            self.delete(key).await;
        }
    }
    async fn invalidate_prefix(&self, prefix: &str);
}

/// 便捷扩展方法（自动序列化/反序列化）
pub trait CacheExt {
    fn get<T: DeserializeOwned + Send>(
        &self,
        key: &str,
    ) -> impl std::future::Future<Output = Option<T>> + Send;

    fn set<T: Serialize + Send + Sync>(
        &self,
        key: &str,
        value: &T,
        ttl_secs: Option<u64>,
    ) -> impl std::future::Future<Output = ()> + Send;

    fn take<T: DeserializeOwned + Send>(
        &self,
        key: &str,
    ) -> impl std::future::Future<Output = Option<T>> + Send;
}

impl CacheExt for dyn CacheBackend {
    async fn get<T: DeserializeOwned + Send>(&self, key: &str) -> Option<T> {
        let bytes = self.get_bytes(key).await?;
        serde_json::from_slice(&bytes).ok()
    }

    async fn set<T: Serialize + Send + Sync>(&self, key: &str, value: &T, ttl_secs: Option<u64>) {
        if let Ok(bytes) = serde_json::to_vec(value) {
            self.set_bytes(key, bytes, ttl_secs).await;
        }
    }

    async fn take<T: DeserializeOwned + Send>(&self, key: &str) -> Option<T> {
        let bytes = self.take_bytes(key).await?;
        serde_json::from_slice(&bytes).ok()
    }
}

/// 根据配置创建缓存后端
pub async fn create_cache(config: &CacheConfig) -> Arc<dyn CacheBackend> {
    match config.backend.as_str() {
        "redis" => {
            match redis_cache::RedisCache::new(&config.redis_url, config.default_ttl).await {
                Ok(cache) => {
                    tracing::info!(
                        target = %redis_backend_target(&config.redis_url),
                        "cache backend: redis"
                    );
                    Arc::new(cache)
                }
                Err(e) => {
                    tracing::warn!("redis connection failed: {e}, falling back to memory cache");
                    Arc::new(memory::MemoryCache::new(config.default_ttl))
                }
            }
        }
        _ => {
            tracing::info!("cache backend: memory (ttl={}s)", config.default_ttl);
            Arc::new(memory::MemoryCache::new(config.default_ttl))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::redis_backend_target;

    #[test]
    fn redis_backend_target_strips_credentials() {
        assert_eq!(
            redis_backend_target("redis://user:secret@example.com:6379/0"),
            "redis://example.com:6379"
        );
    }

    #[test]
    fn redis_backend_target_keeps_host_without_credentials() {
        assert_eq!(
            redis_backend_target("rediss://cache.internal:6380/1"),
            "rediss://cache.internal:6380"
        );
    }
}
