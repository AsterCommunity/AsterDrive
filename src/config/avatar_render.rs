//! Bounded runtime for synchronous avatar rendering.

use std::sync::Arc;

use tokio::sync::{Mutex, OwnedMutexGuard, OwnedSemaphorePermit, Semaphore};

use crate::errors::{AsterError, Result};

pub const DEFAULT_AVATAR_RENDER_MAX_CONCURRENCY: usize = 2;

#[derive(Clone, Debug)]
pub struct AvatarRenderRuntime {
    semaphore: Arc<Semaphore>,
    publish_lock: Arc<Mutex<()>>,
}

impl AvatarRenderRuntime {
    pub fn new(max_concurrency: usize) -> Result<Self> {
        if max_concurrency == 0 {
            return Err(AsterError::config_error(
                "avatar render max concurrency must be greater than zero",
            ));
        }
        Ok(Self {
            semaphore: Arc::new(Semaphore::new(max_concurrency)),
            publish_lock: Arc::new(Mutex::new(())),
        })
    }

    pub async fn acquire_render(&self) -> Result<OwnedSemaphorePermit> {
        self.semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|error| {
                AsterError::internal_error(format!(
                    "avatar render concurrency limiter closed: {error}"
                ))
            })
    }

    pub async fn acquire_publish(&self) -> OwnedMutexGuard<()> {
        self.publish_lock.clone().lock_owned().await
    }
}

impl Default for AvatarRenderRuntime {
    fn default() -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(DEFAULT_AVATAR_RENDER_MAX_CONCURRENCY)),
            publish_lock: Arc::new(Mutex::new(())),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn rejects_zero_concurrency() {
        assert!(AvatarRenderRuntime::new(0).is_err());
    }

    #[tokio::test]
    async fn render_work_waits_for_a_concurrency_permit() {
        let runtime = AvatarRenderRuntime::new(1).unwrap();
        let held = runtime.acquire_render().await.unwrap();
        let waiting_runtime = runtime.clone();
        let mut waiting = tokio::spawn(async move { waiting_runtime.acquire_render().await });

        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut waiting)
                .await
                .is_err()
        );
        drop(held);
        let _permit = tokio::time::timeout(Duration::from_secs(1), waiting)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}
