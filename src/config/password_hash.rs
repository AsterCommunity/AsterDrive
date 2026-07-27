//! 密码哈希运行时边界。
//!
//! Forge 提供同步密码学原语；Drive 在这里负责把 CPU/内存密集型 Argon2 工作放进
//! blocking 线程池，并限制单实例同时进行的密码哈希数量。

use std::sync::Arc;

use aster_forge_crypto::{
    PasswordHashPolicy, PasswordHashVerification, hash_password_with_policy,
    verify_password_with_policy,
};
use tokio::sync::Semaphore;

use crate::errors::{AsterError, Result};

pub const DEFAULT_PASSWORD_HASH_MAX_CONCURRENCY: usize = 2;

#[derive(Clone, Debug)]
pub struct PasswordHashRuntime {
    policy: PasswordHashPolicy,
    semaphore: Arc<Semaphore>,
}

impl PasswordHashRuntime {
    pub fn new(max_concurrency: usize) -> Result<Self> {
        Self::with_policy(max_concurrency, PasswordHashPolicy::default())
    }

    pub fn with_policy(max_concurrency: usize, policy: PasswordHashPolicy) -> Result<Self> {
        if max_concurrency == 0 {
            return Err(AsterError::config_error(
                "auth.password_hash_max_concurrency must be greater than zero",
            ));
        }

        Ok(Self {
            policy,
            semaphore: Arc::new(Semaphore::new(max_concurrency)),
        })
    }

    pub async fn hash_password(&self, password: &str) -> Result<String> {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|error| {
                AsterError::internal_error(format!(
                    "password hash concurrency limiter closed: {error}"
                ))
            })?;
        let password = password.to_owned();
        let policy = self.policy;

        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            hash_password_with_policy(&password, &policy)
        })
        .await
        .map_err(|error| {
            AsterError::internal_error(format!("password hash blocking task failed: {error}"))
        })?
        .map_err(AsterError::from)
    }

    pub async fn verify_password(
        &self,
        password: &str,
        stored_hash: &str,
    ) -> Result<PasswordHashVerification> {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|error| {
                AsterError::internal_error(format!(
                    "password hash concurrency limiter closed: {error}"
                ))
            })?;
        let password = password.to_owned();
        let stored_hash = stored_hash.to_owned();
        let policy = self.policy;

        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            verify_password_with_policy(&password, &stored_hash, &policy)
        })
        .await
        .map_err(|error| {
            AsterError::internal_error(format!(
                "password verification blocking task failed: {error}"
            ))
        })?
        .map_err(AsterError::from)
    }
}

impl Default for PasswordHashRuntime {
    fn default() -> Self {
        Self {
            policy: PasswordHashPolicy::default(),
            semaphore: Arc::new(Semaphore::new(DEFAULT_PASSWORD_HASH_MAX_CONCURRENCY)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use aster_forge_crypto::{
        CryptoError, PasswordHashVerificationLimits, PasswordHashWorkFactor,
        hash_password_with_policy,
    };

    use super::*;

    fn policy(memory_kib: u32) -> PasswordHashPolicy {
        PasswordHashPolicy::new(
            PasswordHashWorkFactor::new(memory_kib, 1, 1, 32).unwrap(),
            PasswordHashVerificationLimits::new(memory_kib, 1, 1, 32).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn rejects_zero_concurrency() {
        let error = PasswordHashRuntime::new(0).unwrap_err();
        assert!(error.to_string().contains("greater than zero"));
    }

    #[test]
    fn production_policy_uses_forge_default_profile() {
        let runtime = PasswordHashRuntime::new(1).unwrap();
        let work_factor = runtime.policy.work_factor();

        assert_eq!(work_factor.memory_kib(), 64 * 1024);
        assert_eq!(work_factor.iterations(), 3);
        assert_eq!(work_factor.parallelism(), 4);
        assert_eq!(work_factor.output_length(), 32);
    }

    #[tokio::test]
    async fn hash_work_waits_for_the_concurrency_permit() {
        let runtime = PasswordHashRuntime::with_policy(1, policy(8)).unwrap();
        let permit = runtime.semaphore.clone().acquire_owned().await.unwrap();
        let runtime_for_task = runtime.clone();
        let mut task =
            tokio::spawn(async move { runtime_for_task.hash_password("bounded-password").await });

        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut task)
                .await
                .is_err()
        );

        drop(permit);
        let hash = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(hash.starts_with("$argon2id$v=19$m=8,t=1,p=1$"));
    }

    #[tokio::test]
    async fn verification_reports_legacy_rehash_and_wrong_password() {
        let current_policy = policy(16);
        let legacy_policy = policy(8);
        let runtime = PasswordHashRuntime::with_policy(1, current_policy).unwrap();
        let legacy_hash = hash_password_with_policy("legacy-password", &legacy_policy).unwrap();

        let verified = runtime
            .verify_password("legacy-password", &legacy_hash)
            .await
            .unwrap();
        assert!(verified.is_valid);
        assert!(verified.needs_rehash);

        let rejected = runtime
            .verify_password("wrong-password", &legacy_hash)
            .await
            .unwrap();
        assert!(!rejected.is_valid);
        assert!(!rejected.needs_rehash);
    }

    #[tokio::test]
    async fn malformed_hash_error_releases_the_concurrency_permit() {
        let runtime = PasswordHashRuntime::with_policy(1, policy(8)).unwrap();
        let error = runtime
            .verify_password("password", "not-a-phc-string")
            .await
            .unwrap_err();
        assert!(error.to_string().contains("password hash error"));

        let hash = runtime.hash_password("next-password").await.unwrap();
        assert!(
            runtime
                .verify_password("next-password", &hash)
                .await
                .unwrap()
                .is_valid
        );
    }

    #[test]
    fn over_budget_hash_is_classified_before_verification() {
        let runtime = PasswordHashRuntime::with_policy(1, policy(8)).unwrap();
        let hash = hash_password_with_policy("password", &policy(8)).unwrap();
        let over_budget = hash.replacen("m=8", "m=9", 1);
        let error =
            verify_password_with_policy("password", &over_budget, &runtime.policy).unwrap_err();

        assert!(matches!(
            error,
            CryptoError::PasswordHashVerificationLimit {
                parameter: "m",
                actual: 9,
                maximum: 8,
            }
        ));
    }
}
