//! Upload execution resources owned by the primary application runtime.

use std::sync::Arc;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::errors::{AsterError, Result};

const CHUNK_ASSEMBLY_TO_LOCAL_TEMP_FILE_CONCURRENCY: usize = 1;

#[derive(Debug)]
pub struct UploadRuntime {
    chunk_assembly_to_local_temp_file: Arc<Semaphore>,
}

impl UploadRuntime {
    pub fn new() -> Self {
        Self {
            chunk_assembly_to_local_temp_file: Arc::new(Semaphore::new(
                CHUNK_ASSEMBLY_TO_LOCAL_TEMP_FILE_CONCURRENCY,
            )),
        }
    }

    pub(crate) async fn acquire_chunk_assembly_to_local_temp_file(
        &self,
    ) -> Result<OwnedSemaphorePermit> {
        self.chunk_assembly_to_local_temp_file
            .clone()
            .acquire_owned()
            .await
            .map_err(|error| {
                AsterError::internal_error(format!(
                    "chunk assembly to local temp file limiter closed: {error}"
                ))
            })
    }
}

impl Default for UploadRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::UploadRuntime;

    fn update_peak(peak: &AtomicUsize, current: usize) {
        let mut observed = peak.load(Ordering::SeqCst);
        while current > observed {
            match peak.compare_exchange(observed, current, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => break,
                Err(actual) => observed = actual,
            }
        }
    }

    #[tokio::test]
    async fn chunk_assembly_to_local_temp_file_is_serialized() {
        let runtime = UploadRuntime::new();
        let first = runtime
            .acquire_chunk_assembly_to_local_temp_file()
            .await
            .expect("first assembly permit should be available");

        assert!(
            tokio::time::timeout(
                Duration::from_millis(20),
                runtime.acquire_chunk_assembly_to_local_temp_file(),
            )
            .await
            .is_err(),
            "second assembly should wait while the first permit is held"
        );

        drop(first);

        let second = tokio::time::timeout(
            Duration::from_millis(100),
            runtime.acquire_chunk_assembly_to_local_temp_file(),
        )
        .await
        .expect("assembly permit should be released by RAII")
        .expect("assembly limiter should remain open");
        drop(second);
    }

    #[tokio::test]
    async fn concurrent_chunk_assemblies_to_local_temp_file_never_exceed_one() {
        let runtime = Arc::new(UploadRuntime::new());
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();

        for _ in 0..8 {
            let runtime = runtime.clone();
            let active = active.clone();
            let peak = peak.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = runtime
                    .acquire_chunk_assembly_to_local_temp_file()
                    .await
                    .expect("assembly permit should be available eventually");
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                update_peak(&peak, current);
                tokio::time::sleep(Duration::from_millis(5)).await;
                active.fetch_sub(1, Ordering::SeqCst);
            }));
        }

        for task in tasks {
            task.await.expect("assembly task should not panic");
        }

        assert_eq!(active.load(Ordering::SeqCst), 0);
        assert_eq!(peak.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cancelled_waiter_does_not_consume_the_next_permit() {
        let runtime = Arc::new(UploadRuntime::new());
        let first = runtime
            .acquire_chunk_assembly_to_local_temp_file()
            .await
            .expect("first assembly permit should be available");

        let waiting_runtime = runtime.clone();
        let waiter = tokio::spawn(async move {
            waiting_runtime
                .acquire_chunk_assembly_to_local_temp_file()
                .await
                .expect("cancelled waiter should only fail by cancellation")
        });
        tokio::task::yield_now().await;
        waiter.abort();
        let cancelled = waiter.await;
        assert!(cancelled.is_err_and(|error| error.is_cancelled()));

        drop(first);

        let next = tokio::time::timeout(
            Duration::from_millis(100),
            runtime.acquire_chunk_assembly_to_local_temp_file(),
        )
        .await
        .expect("cancelled waiter should not block the next assembly")
        .expect("assembly limiter should remain open");
        drop(next);
    }

    #[tokio::test]
    async fn permit_is_released_when_guarded_work_fails() {
        let runtime = UploadRuntime::new();
        let guarded_result = async {
            let _permit = runtime.acquire_chunk_assembly_to_local_temp_file().await?;
            Err::<(), crate::errors::AsterError>(crate::errors::AsterError::internal_error(
                "synthetic assembly failure",
            ))
        }
        .await;
        assert!(guarded_result.is_err());

        let next = tokio::time::timeout(
            Duration::from_millis(100),
            runtime.acquire_chunk_assembly_to_local_temp_file(),
        )
        .await
        .expect("failed guarded work should release its permit")
        .expect("assembly limiter should remain open");
        drop(next);
    }
}
