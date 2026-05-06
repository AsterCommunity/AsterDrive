//! 缓存实现：`noop`。

use super::{CacheBackend, reservation::ReservationSet};
use async_trait::async_trait;

pub struct NoopCache {
    reservations: ReservationSet,
}

impl NoopCache {
    pub fn new(default_ttl: u64) -> Self {
        Self {
            reservations: ReservationSet::new(default_ttl),
        }
    }
}

#[async_trait]
impl CacheBackend for NoopCache {
    fn backend_name(&self) -> &'static str {
        "noop"
    }

    async fn health_check(&self) -> crate::errors::Result<()> {
        Ok(())
    }

    async fn get_bytes(&self, _key: &str) -> Option<Vec<u8>> {
        None
    }

    async fn set_bytes(&self, _key: &str, _value: Vec<u8>, _ttl_secs: Option<u64>) {}

    async fn set_bytes_if_absent(&self, key: &str, _value: Vec<u8>, ttl_secs: Option<u64>) -> bool {
        self.reservations.reserve(key, ttl_secs)
    }

    async fn delete(&self, key: &str) {
        self.reservations.remove(key);
    }

    async fn invalidate_prefix(&self, prefix: &str) {
        self.reservations.invalidate_prefix(prefix);
    }
}
