//! Database retry policy for storage finalization transactions.

use std::time::Duration;

use sea_orm::{DatabaseConnection, DbBackend};

use crate::errors::AsterError;

const MYSQL_DEADLOCK_MAX_RETRIES: usize = 3;
const MYSQL_DEADLOCK_INITIAL_BACKOFF: Duration = Duration::from_millis(5);
const MYSQL_DEADLOCK_MAX_BACKOFF: Duration = Duration::from_millis(50);

/// Waits before another transaction attempt when `error` is MySQL error 1213.
///
/// Returns `true` only when the caller should start a fresh database transaction. Callers keep
/// external storage side effects outside their retry loop.
pub(crate) async fn retry_mysql_deadlock(
    db: &DatabaseConnection,
    attempt: usize,
    error: &AsterError,
) -> bool {
    if db.get_database_backend() != DbBackend::MySql
        || error.code() != "E002"
        || error.database_error_kind() != Some(aster_forge_db::DatabaseErrorKind::Deadlock)
        || attempt >= MYSQL_DEADLOCK_MAX_RETRIES
    {
        return false;
    }

    let delay = MYSQL_DEADLOCK_INITIAL_BACKOFF
        .checked_mul(1_u32 << attempt.min(4))
        .unwrap_or(MYSQL_DEADLOCK_MAX_BACKOFF)
        .min(MYSQL_DEADLOCK_MAX_BACKOFF);
    tracing::warn!(
        attempt = attempt + 1,
        max_retries = MYSQL_DEADLOCK_MAX_RETRIES,
        delay_ms = delay.as_millis(),
        "retrying storage transaction after MySQL deadlock"
    );
    tokio::time::sleep(delay).await;
    true
}

#[cfg(test)]
mod tests {
    use crate::errors::AsterError;

    #[test]
    fn structured_mysql_error_code_distinguishes_deadlock_from_message_text() {
        let deadlock = AsterError::database_operation("localized driver message")
            .with_database_error_kind(aster_forge_db::DatabaseErrorKind::Deadlock);
        assert_eq!(
            deadlock.database_error_kind(),
            Some(aster_forge_db::DatabaseErrorKind::Deadlock)
        );

        let text_only = AsterError::database_operation(
            "1213 Deadlock found when trying to get lock; try restarting transaction",
        );
        assert_eq!(text_only.database_error_kind(), None);
    }
}
