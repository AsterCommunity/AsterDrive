//! Process-local coordination for physical upload-staging reservations.
//!
//! Staged upload kinds are rejected in cluster deployments, so one Primary owns the configured
//! staging root. This coordinator serializes capacity observation and file allocation within that
//! Primary and remembers which root has had durable upload-session state recovered.

use std::path::{Path, PathBuf};

use tokio::sync::{Mutex, MutexGuard};

#[derive(Default)]
pub(crate) struct StagingCapacityCoordinator {
    state: Mutex<StagingCapacityState>,
}

#[derive(Default)]
pub(crate) struct StagingCapacityState {
    recovered_root: Option<PathBuf>,
}

impl StagingCapacityCoordinator {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) async fn lock(&self) -> MutexGuard<'_, StagingCapacityState> {
        self.state.lock().await
    }
}

impl StagingCapacityState {
    pub(crate) fn needs_recovery(&self, root: &Path) -> bool {
        self.recovered_root.as_deref() != Some(root)
    }

    pub(crate) fn mark_recovered(&mut self, root: PathBuf) {
        self.recovered_root = Some(root);
    }
}

#[cfg(test)]
mod tests {
    use super::StagingCapacityCoordinator;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[tokio::test]
    async fn admission_lock_serializes_concurrent_reservations() {
        let coordinator = Arc::new(StagingCapacityCoordinator::new());
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(tokio::sync::Barrier::new(3));
        let mut tasks = tokio::task::JoinSet::new();

        for _ in 0..2 {
            let coordinator = coordinator.clone();
            let active = active.clone();
            let max_active = max_active.clone();
            let barrier = barrier.clone();
            tasks.spawn(async move {
                barrier.wait().await;
                let _guard = coordinator.lock().await;
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                max_active.fetch_max(current, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                active.fetch_sub(1, Ordering::SeqCst);
            });
        }
        barrier.wait().await;
        while let Some(result) = tasks.join_next().await {
            result.unwrap();
        }

        assert_eq!(max_active.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn recovery_marker_is_scoped_to_the_configured_root() {
        let coordinator = StagingCapacityCoordinator::new();
        let first = std::path::PathBuf::from("/tmp/staging-a");
        let second = std::path::PathBuf::from("/tmp/staging-b");
        let mut state = coordinator.lock().await;

        assert!(state.needs_recovery(&first));
        state.mark_recovered(first.clone());
        assert!(!state.needs_recovery(&first));
        assert!(state.needs_recovery(&second));
    }
}
