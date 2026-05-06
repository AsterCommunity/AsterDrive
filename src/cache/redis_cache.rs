//! 缓存实现：`redis_cache`。

use super::CacheBackend;
use crate::errors::{AsterError, MapAsterErr};
use async_trait::async_trait;
use redis::{AsyncCommands, ExistenceCheck, SetExpiry, SetOptions};

pub struct RedisCache {
    conn: redis::aio::ConnectionManager,
    default_ttl: u64,
}

impl RedisCache {
    pub async fn new(url: &str, default_ttl: u64) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(url)?;
        let conn = redis::aio::ConnectionManager::new(client).await?;
        Ok(Self { conn, default_ttl })
    }
}

#[async_trait]
impl CacheBackend for RedisCache {
    fn backend_name(&self) -> &'static str {
        "redis"
    }

    async fn health_check(&self) -> crate::errors::Result<()> {
        let mut conn = self.conn.clone();
        let _: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_aster_err_ctx("redis cache health check", AsterError::internal_error)?;
        Ok(())
    }

    async fn get_bytes(&self, key: &str) -> Option<Vec<u8>> {
        let mut conn = self.conn.clone();
        conn.get(key).await.ok()?
    }

    async fn set_bytes(&self, key: &str, value: Vec<u8>, ttl_secs: Option<u64>) {
        let ttl = ttl_secs.unwrap_or(self.default_ttl);
        let mut conn = self.conn.clone();
        if let Err(error) = conn.set_ex::<_, _, ()>(key, value, ttl).await {
            tracing::warn!(ttl_secs = ttl, "redis cache set failed: {error}");
        }
    }

    async fn set_bytes_if_absent(&self, key: &str, value: Vec<u8>, ttl_secs: Option<u64>) -> bool {
        let ttl = ttl_secs.unwrap_or(self.default_ttl);
        let options = SetOptions::default()
            .conditional_set(ExistenceCheck::NX)
            .with_expiration(SetExpiry::EX(ttl));
        let mut conn = self.conn.clone();
        let result: redis::RedisResult<Option<String>> =
            conn.set_options(key, value, options).await;
        match result {
            Ok(Some(_)) => true,
            Ok(None) => false,
            Err(error) => {
                tracing::warn!(ttl_secs = ttl, "redis cache set-if-absent failed: {error}");
                false
            }
        }
    }

    async fn delete(&self, key: &str) {
        let mut conn = self.conn.clone();
        if let Err(error) = conn.del::<_, ()>(key).await {
            tracing::warn!("redis cache delete failed: {error}");
        }
    }

    async fn invalidate_prefix(&self, prefix: &str) {
        let mut conn = self.conn.clone();
        let pattern = format!("{prefix}*");
        let mut cursor: u64 = 0;
        loop {
            let (next_cursor, keys): (u64, Vec<String>) = match redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&pattern)
                .arg("COUNT")
                .arg(100)
                .query_async(&mut conn)
                .await
            {
                Ok(result) => result,
                Err(error) => {
                    tracing::warn!("redis cache prefix scan failed: {error}");
                    break;
                }
            };
            if !keys.is_empty()
                && let Err(error) = conn.del::<_, ()>(&keys).await
            {
                tracing::warn!(
                    matched_keys = keys.len(),
                    "redis cache prefix delete failed: {error}"
                );
            }
            cursor = next_cursor;
            if cursor == 0 {
                break;
            }
        }
    }
}
