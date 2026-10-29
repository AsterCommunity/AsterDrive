//! Per-request receive bounds. Quota is a writer-side snapshot, not a reservation;
//! finalization still performs the authoritative transactional quota check.

use aster_forge_webdav::FsError;

pub(super) struct ReceiveLimits {
    max_size: u64,
    quota_remaining: Option<u64>,
}

impl ReceiveLimits {
    pub(super) fn new(
        payload_limit: u64,
        policy_limit: i64,
        storage_used: i64,
        storage_quota: i64,
    ) -> Result<Self, FsError> {
        let policy_limit = u64::try_from(policy_limit).map_err(|_| FsError::GeneralFailure)?;
        if storage_used < 0 || storage_quota < 0 {
            return Err(FsError::GeneralFailure);
        }
        let quota_remaining = if storage_quota > 0 {
            Some(
                u64::try_from(storage_quota.saturating_sub(storage_used).max(0))
                    .map_err(|_| FsError::GeneralFailure)?,
            )
        } else {
            None
        };
        Ok(Self {
            max_size: payload_limit.min(policy_limit),
            quota_remaining,
        })
    }

    pub(super) fn validate(&self, size: u64, declared_size: Option<i64>) -> Result<(), FsError> {
        if let Some(declared) = declared_size
            && size > u64::try_from(declared).map_err(|_| FsError::GeneralFailure)?
        {
            return Err(FsError::BadRequest);
        }
        if size > self.max_size {
            return Err(FsError::TooLarge);
        }
        if self
            .quota_remaining
            .is_some_and(|remaining| size > remaining)
        {
            return Err(FsError::InsufficientStorage);
        }
        Ok(())
    }

    pub(super) fn next_size(
        &self,
        written: u64,
        chunk_len: usize,
        declared_size: Option<i64>,
    ) -> Result<u64, FsError> {
        let chunk_len = u64::try_from(chunk_len).map_err(|_| FsError::TooLarge)?;
        let size = written.checked_add(chunk_len).ok_or(FsError::TooLarge)?;
        self.validate(size, declared_size)?;
        Ok(size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_limit_and_empty_chunks_are_allowed_but_overflow_is_not() {
        let limits = ReceiveLimits::new(8, 16, 2, 10).unwrap();
        assert_eq!(limits.next_size(0, 8, None), Ok(8));
        assert_eq!(limits.next_size(8, 0, None), Ok(8));
        assert_eq!(limits.next_size(8, 1, None), Err(FsError::TooLarge));
        assert_eq!(limits.next_size(u64::MAX, 1, None), Err(FsError::TooLarge));
    }

    #[test]
    fn quota_policy_and_declared_length_have_distinct_errors() {
        let limits = ReceiveLimits::new(32, 16, 2, 10).unwrap();
        assert_eq!(
            limits.next_size(8, 1, None),
            Err(FsError::InsufficientStorage)
        );
        assert_eq!(limits.next_size(16, 1, None), Err(FsError::TooLarge));
        assert_eq!(limits.next_size(8, 1, Some(8)), Err(FsError::BadRequest));
    }

    #[test]
    fn unlimited_quota_still_has_global_and_signed_size_bounds() {
        let limits = ReceiveLimits::new(8, i64::MAX, 0, 0).unwrap();
        assert_eq!(limits.validate(8, None), Ok(()));
        assert_eq!(limits.validate(9, None), Err(FsError::TooLarge));
        let limits = ReceiveLimits::new(u64::MAX, i64::MAX, 0, 0).unwrap();
        assert_eq!(limits.validate(u64::MAX, None), Err(FsError::TooLarge));
    }

    #[test]
    fn exhausted_quota_and_zero_payload_limit_only_allow_empty_content() {
        for used in [8, 9] {
            let limits = ReceiveLimits::new(32, 32, used, 8).unwrap();
            assert_eq!(limits.validate(0, None), Ok(()));
            assert_eq!(limits.validate(1, None), Err(FsError::InsufficientStorage));
        }
        let limits = ReceiveLimits::new(0, 32, 0, 0).unwrap();
        assert_eq!(limits.validate(0, None), Ok(()));
        assert_eq!(limits.validate(1, None), Err(FsError::TooLarge));
        assert!(ReceiveLimits::new(8, 8, -1, 0).is_err());
        assert!(ReceiveLimits::new(8, 8, 0, -1).is_err());
    }
}
