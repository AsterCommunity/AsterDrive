//! 缓存实现：`memory`。

use super::{CacheBackend, reservation::ReservationSet};
use async_trait::async_trait;
use moka::future::Cache;
use std::sync::Arc;
use std::time::{Duration, Instant};

const MEMORY_CACHE_MAX_BYTES: u64 = 64 * 1024 * 1024;

pub struct MemoryCache {
    cache: Cache<String, MemoryCacheValue>,
    default_ttl: u64,
    reservations: ReservationSet,
}

#[derive(Clone)]
struct MemoryCacheValue {
    bytes: Vec<u8>,
    expires_at: Instant,
}

impl MemoryCacheValue {
    fn new(bytes: Vec<u8>, ttl_secs: u64) -> Self {
        let now = Instant::now();
        Self {
            bytes,
            expires_at: now
                .checked_add(Duration::from_secs(ttl_secs))
                .unwrap_or(now),
        }
    }

    fn is_expired(&self) -> bool {
        self.expires_at <= Instant::now()
    }
}

impl MemoryCache {
    pub fn new(default_ttl: u64) -> Self {
        let cache = Cache::builder()
            .max_capacity(MEMORY_CACHE_MAX_BYTES)
            .weigher(|key: &String, value: &MemoryCacheValue| {
                entry_weight(key.len(), value.bytes.len())
            })
            .time_to_live(Duration::from_secs(default_ttl))
            .build();
        Self {
            cache,
            default_ttl,
            reservations: ReservationSet::new(default_ttl),
        }
    }

    fn cache_value(&self, value: Vec<u8>, ttl_secs: Option<u64>) -> MemoryCacheValue {
        MemoryCacheValue::new(value, ttl_secs.unwrap_or(self.default_ttl))
    }
}

fn entry_weight(key_len: usize, value_len: usize) -> u32 {
    let total = key_len.saturating_add(value_len);
    u32::try_from(total).unwrap_or(u32::MAX)
}

#[async_trait]
impl CacheBackend for MemoryCache {
    fn backend_name(&self) -> &'static str {
        "memory"
    }

    async fn health_check(&self) -> crate::errors::Result<()> {
        Ok(())
    }

    async fn get_bytes(&self, key: &str) -> Option<Vec<u8>> {
        let value = self.cache.get(key).await?;
        if value.is_expired() {
            self.reservations.remove(key);
            self.cache.remove(key).await;
            return None;
        }
        Some(value.bytes)
    }

    async fn take_bytes(&self, key: &str) -> Option<Vec<u8>> {
        self.reservations.remove(key);
        let value = self.cache.remove(key).await?;
        if value.is_expired() {
            return None;
        }
        Some(value.bytes)
    }

    async fn set_bytes(&self, key: &str, value: Vec<u8>, ttl_secs: Option<u64>) {
        self.cache
            .insert(key.to_string(), self.cache_value(value, ttl_secs))
            .await;
    }

    async fn set_bytes_if_absent(&self, key: &str, value: Vec<u8>, ttl_secs: Option<u64>) -> bool {
        if self.get_bytes(key).await.is_some() {
            return false;
        }
        if !self.reservations.reserve(key, ttl_secs) {
            return false;
        }
        if self.get_bytes(key).await.is_some() {
            return false;
        }

        self.cache
            .insert(key.to_string(), self.cache_value(value, ttl_secs))
            .await;
        true
    }

    async fn delete(&self, key: &str) {
        self.reservations.remove(key);
        self.cache.remove(key).await;
    }

    async fn delete_many(&self, keys: &[String]) {
        for key in keys {
            self.delete(key).await;
        }
    }

    async fn invalidate_prefix(&self, prefix: &str) {
        self.reservations.invalidate_prefix(prefix);
        let keys: Vec<Arc<String>> = self
            .cache
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, _)| k.clone())
            .collect();
        for key in keys {
            self.cache.remove(key.as_ref()).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CacheBackend, MemoryCache, entry_weight};
    use std::sync::Arc;

    #[test]
    fn entry_weight_counts_key_and_value_bytes() {
        assert_eq!(entry_weight(3, 5), 8);
    }

    #[test]
    fn entry_weight_saturates_at_u32_max() {
        assert_eq!(entry_weight(usize::MAX, usize::MAX), u32::MAX);
    }

    #[tokio::test]
    async fn set_bytes_if_absent_allows_one_concurrent_insert() {
        let cache = Arc::new(MemoryCache::new(60));
        let mut tasks = Vec::new();
        for _ in 0..16 {
            let cache = cache.clone();
            tasks.push(tokio::spawn(async move {
                cache
                    .set_bytes_if_absent("nonce", Vec::new(), Some(60))
                    .await
            }));
        }

        let successes = futures::future::join_all(tasks)
            .await
            .into_iter()
            .map(|result| result.expect("reservation task should not panic"))
            .filter(|inserted| *inserted)
            .count();

        assert_eq!(successes, 1);
    }

    #[tokio::test]
    async fn set_bytes_if_absent_respects_existing_set_value() {
        let cache = MemoryCache::new(60);

        cache.set_bytes("nonce", b"first".to_vec(), Some(60)).await;

        assert!(
            !cache
                .set_bytes_if_absent("nonce", b"second".to_vec(), Some(60))
                .await
        );
        assert_eq!(cache.get_bytes("nonce").await, Some(b"first".to_vec()));
    }

    #[tokio::test]
    async fn set_bytes_respects_entry_ttl() {
        let cache = MemoryCache::new(60);

        cache.set_bytes("short", b"value".to_vec(), Some(0)).await;

        assert_eq!(cache.get_bytes("short").await, None);
    }

    #[tokio::test]
    async fn set_bytes_if_absent_can_replace_expired_entry() {
        let cache = MemoryCache::new(60);

        cache.set_bytes("nonce", b"expired".to_vec(), Some(0)).await;

        assert!(
            cache
                .set_bytes_if_absent("nonce", b"fresh".to_vec(), Some(60))
                .await
        );
        assert_eq!(cache.get_bytes("nonce").await, Some(b"fresh".to_vec()));
    }

    #[tokio::test]
    async fn take_bytes_consumes_existing_entry_once() {
        let cache = MemoryCache::new(60);

        cache
            .set_bytes("challenge", b"value".to_vec(), Some(60))
            .await;

        assert_eq!(cache.take_bytes("challenge").await, Some(b"value".to_vec()));
        assert_eq!(cache.take_bytes("challenge").await, None);
        assert_eq!(cache.get_bytes("challenge").await, None);
    }

    #[tokio::test]
    async fn take_bytes_returns_none_for_missing_or_expired_entry() {
        let cache = MemoryCache::new(60);

        assert_eq!(cache.take_bytes("missing").await, None);
        cache.set_bytes("expired", b"value".to_vec(), Some(0)).await;

        assert_eq!(cache.take_bytes("expired").await, None);
        assert_eq!(cache.get_bytes("expired").await, None);
    }

    #[tokio::test]
    async fn take_bytes_allows_one_concurrent_consumer() {
        let cache = Arc::new(MemoryCache::new(60));
        cache
            .set_bytes("challenge", b"value".to_vec(), Some(60))
            .await;
        let mut tasks = Vec::new();
        for _ in 0..16 {
            let cache = cache.clone();
            tasks.push(tokio::spawn(
                async move { cache.take_bytes("challenge").await },
            ));
        }

        let values = futures::future::join_all(tasks)
            .await
            .into_iter()
            .map(|result| result.expect("take task should not panic"))
            .collect::<Vec<_>>();

        assert_eq!(
            values
                .iter()
                .filter(|value| value.as_deref() == Some(b"value".as_slice()))
                .count(),
            1
        );
        assert_eq!(values.iter().filter(|value| value.is_none()).count(), 15);
    }

    #[tokio::test]
    async fn delete_many_removes_only_requested_entries() {
        let cache = MemoryCache::new(60);
        cache.set_bytes("remove:1", b"one".to_vec(), Some(60)).await;
        cache.set_bytes("remove:2", b"two".to_vec(), Some(60)).await;
        cache.set_bytes("keep", b"keep".to_vec(), Some(60)).await;

        cache
            .delete_many(&[
                "remove:1".to_string(),
                "remove:2".to_string(),
                "remove:2".to_string(),
                "missing".to_string(),
            ])
            .await;
        cache.delete_many(&[]).await;

        assert_eq!(cache.get_bytes("remove:1").await, None);
        assert_eq!(cache.get_bytes("remove:2").await, None);
        assert_eq!(cache.get_bytes("keep").await, Some(b"keep".to_vec()));
    }
}
