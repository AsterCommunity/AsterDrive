//! 工具子模块：`id`。

use std::future::Future;

use crate::errors::{AsterError, Result};
use uuid::Uuid;

pub const UNIQUE_UUID_MAX_ATTEMPTS: usize = 5;

pub enum UniqueUuidAttempt<T> {
    Accepted(T),
    Collision,
}

/// 生成 UUID v4 字符串（用于 share token 等）
pub fn new_uuid() -> String {
    Uuid::new_v4().to_string()
}

/// 使用业务唯一 UUID 执行操作。调用方返回 `Collision` 时会自动换一个 UUID 重试。
pub async fn with_unique_uuid<F, Fut, T>(value_name: &str, mut try_candidate: F) -> Result<T>
where
    F: FnMut(Uuid) -> Fut,
    Fut: Future<Output = Result<UniqueUuidAttempt<T>>>,
{
    for attempt in 1..=UNIQUE_UUID_MAX_ATTEMPTS {
        let candidate = Uuid::new_v4();
        match try_candidate(candidate).await? {
            UniqueUuidAttempt::Accepted(value) => return Ok(value),
            UniqueUuidAttempt::Collision => {
                tracing::warn!(
                    value_name,
                    attempt,
                    candidate = %candidate,
                    "uuid collision, retrying"
                );
            }
        }
    }

    Err(AsterError::internal_error(format!(
        "failed to create unique {value_name} UUID after {UNIQUE_UUID_MAX_ATTEMPTS} attempts"
    )))
}

/// 生成业务 UUID，并用调用方提供的查询过滤已占用候选值。
///
/// 这个辅助函数不执行原子占用；调用方必须在后续写入路径继续依赖数据库唯一约束。
/// 需要“检查并占用”原子语义时，使用 `with_unique_uuid` 并在回调里执行写入。
pub async fn new_best_effort_uuid<F, Fut>(value_name: &str, mut is_taken: F) -> Result<Uuid>
where
    F: FnMut(Uuid) -> Fut,
    Fut: Future<Output = Result<bool>>,
{
    with_unique_uuid(value_name, |candidate| {
        let taken = is_taken(candidate);
        async move {
            if taken.await? {
                Ok(UniqueUuidAttempt::Collision)
            } else {
                Ok(UniqueUuidAttempt::Accepted(candidate))
            }
        }
    })
    .await
}

/// 生成短 token（32 字符 hex）
pub fn new_short_token() -> String {
    let id = Uuid::new_v4();
    id.simple().to_string()
}

/// 生成分享链接 token（8 位 base62: a-zA-Z0-9）
pub fn new_share_token() -> String {
    use rand::RngExt;
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::rng();
    (0..8)
        .map(|_| CHARSET[rng.random_range(0..CHARSET.len())] as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[tokio::test]
    async fn new_best_effort_uuid_returns_first_free_candidate() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let result = new_best_effort_uuid("test value", {
            let attempts = Arc::clone(&attempts);
            move |_| {
                let attempts = Arc::clone(&attempts);
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Ok(false)
                }
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn with_unique_uuid_retries_collisions_and_returns_callback_value() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let result: String = with_unique_uuid("test value", {
            let attempts = Arc::clone(&attempts);
            move |candidate| {
                let attempts = Arc::clone(&attempts);
                async move {
                    let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                    if attempt < 2 {
                        Ok(UniqueUuidAttempt::Collision)
                    } else {
                        Ok(UniqueUuidAttempt::Accepted(candidate.to_string()))
                    }
                }
            }
        })
        .await
        .expect("third candidate should be accepted");

        assert!(Uuid::parse_str(&result).is_ok());
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn with_unique_uuid_propagates_callback_errors_without_retrying() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let result: Result<String> = with_unique_uuid("test value", {
            let attempts = Arc::clone(&attempts);
            move |_| {
                let attempts = Arc::clone(&attempts);
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Err(AsterError::validation_error("candidate check failed"))
                }
            }
        })
        .await;

        let error = result.expect_err("callback error should be returned");
        assert_eq!(error.code(), "E005");
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn new_best_effort_uuid_retries_taken_candidates() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let result = new_best_effort_uuid("test value", {
            let attempts = Arc::clone(&attempts);
            move |_| {
                let attempts = Arc::clone(&attempts);
                async move {
                    let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                    Ok(attempt < 2)
                }
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn new_best_effort_uuid_stops_after_retry_budget_is_exhausted() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let result = new_best_effort_uuid("test value", {
            let attempts = Arc::clone(&attempts);
            move |_| {
                let attempts = Arc::clone(&attempts);
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Ok(true)
                }
            }
        })
        .await;

        let error = result.expect_err("all candidates were reported as taken");
        assert_eq!(error.code(), "E004");
        assert_eq!(attempts.load(Ordering::SeqCst), UNIQUE_UUID_MAX_ATTEMPTS);
    }
}
