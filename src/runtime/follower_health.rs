//! Process-local evidence of successful follower control-plane communication.

use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

#[derive(Debug, Default)]
pub struct FollowerBindingHealth {
    last_success: Mutex<HashMap<i64, Instant>>,
}

impl FollowerBindingHealth {
    pub fn record_success(&self, binding_id: i64) {
        self.last_success.lock().insert(binding_id, Instant::now());
    }

    pub fn retain_bindings(&self, ids: &HashSet<i64>) {
        self.last_success.lock().retain(|id, _| ids.contains(id));
    }

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
