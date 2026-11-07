//! Product-owned admission for account storage-event streams.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::errors::{AsterError, Result};

const MAX_ACCOUNT_STREAMS: usize = 8;
const MAX_INSTANCE_STREAMS: usize = 1024;

#[derive(Clone, Default)]
pub struct StorageEventStreams(Arc<Mutex<HashMap<i64, usize>>>);

pub(crate) struct StorageEventStreamPermit {
    registry: StorageEventStreams,
    user_id: i64,
}

impl StorageEventStreams {
    /// All HTTP workers share this instance through PrimaryAppState clones.
    /// The stream owns its permit, including while its body is not being polled.
    pub(crate) fn admit(&self, user_id: i64) -> Result<StorageEventStreamPermit> {
        let mut counts = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        if counts.get(&user_id).copied().unwrap_or(0) >= MAX_ACCOUNT_STREAMS
            || counts.values().sum::<usize>() >= MAX_INSTANCE_STREAMS
        {
            return Err(AsterError::operation_resource_limit_exceeded(
                "storage event stream limit reached",
            ));
        }
        *counts.entry(user_id).or_default() += 1;
        Ok(StorageEventStreamPermit {
            registry: self.clone(),
            user_id,
        })
    }
}

impl Drop for StorageEventStreamPermit {
    fn drop(&mut self) {
        let mut counts = self
            .registry
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(count) = counts.get_mut(&self.user_id) {
            *count -= 1;
            if *count == 0 {
                counts.remove(&self.user_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_admission_is_shared_and_released_without_polling_the_stream() {
        let registry = StorageEventStreams::default();
        let other_worker = registry.clone();
        let mut permits = (0..MAX_ACCOUNT_STREAMS)
            .map(|_| registry.admit(7).unwrap())
            .collect::<Vec<_>>();
        assert!(other_worker.admit(7).is_err());
        let unrelated = other_worker.admit(8).unwrap();
        drop(permits.pop());
        let replacement = other_worker.admit(7).unwrap();
        drop((permits, replacement, unrelated));
        assert!(registry.0.lock().unwrap().is_empty());
    }

    #[test]
    fn instance_budget_is_shared_across_accounts_and_recovers_on_drop() {
        let registry = StorageEventStreams::default();
        let mut permits = (0..MAX_INSTANCE_STREAMS)
            .map(|id| registry.admit(id as i64).unwrap())
            .collect::<Vec<_>>();
        assert!(registry.admit(-1).is_err());
        drop(permits.pop());
        assert!(registry.clone().admit(-1).is_ok());
    }
}
