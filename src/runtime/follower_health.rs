//! Process-local evidence of successful follower control-plane communication.

use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Tracks when each follower binding last completed a successful
/// control-plane exchange with the primary node.
///
/// Evidence is process-local: it is rebuilt from live communication after a
/// restart and is never treated as an authoritative source of truth.
#[derive(Debug, Default)]
pub struct FollowerBindingHealth {
    last_success: Mutex<HashMap<i64, Instant>>,
}

impl FollowerBindingHealth {
    /// Records a successful control-plane exchange for the binding at the
    /// current instant.
    pub fn record_success(&self, binding_id: i64) {
        self.last_success.lock().insert(binding_id, Instant::now());
    }

    /// Drops evidence for bindings not in `ids`, keeping entries only for
    /// bindings that still exist in the current sync state.
    pub fn retain_bindings(&self, ids: &HashSet<i64>) {
        self.last_success.lock().retain(|id, _| ids.contains(id));
    }

    /// Returns whether the binding reported success within `max_age`.
    /// Returns `false` when the binding has never reported success.
    pub fn is_fresh(&self, binding_id: i64, max_age: Duration) -> bool {
        self.last_success
            .lock()
            .get(&binding_id)
            .is_some_and(|time| time.elapsed() < max_age)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_is_per_binding_expires_and_is_pruned() {
        let health = FollowerBindingHealth::default();
        let ttl = Duration::from_secs(90);
        assert!(!health.is_fresh(1, ttl));
        health.record_success(1);
        assert!(health.is_fresh(1, ttl));
        assert!(!health.is_fresh(2, ttl));
        health.last_success.lock().insert(1, Instant::now() - ttl);
        assert!(!health.is_fresh(1, ttl));
        health.record_success(1);
        assert!(
            health.is_fresh(1, ttl),
            "communication restores readiness after expiry"
        );
        assert!(!health.is_fresh(1, Duration::ZERO));
        health.record_success(2);
        health.retain_bindings(&[2].into_iter().collect());
        assert!(!health.is_fresh(1, ttl));
        assert!(health.is_fresh(2, ttl));
        health.retain_bindings(&HashSet::new());
        assert!(!health.is_fresh(1, ttl));
        assert!(!health.is_fresh(2, ttl));
    }
}
