//! `database-migrate` 的共享辅助函数。
//!
//! 这里集中放置时间戳、布尔值和数据库迁移子模块间复用的小工具。

use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, FixedOffset};
use sea_orm::{ConnectionTrait, DbBackend};

use crate::cli::db_shared::{quote_ident, scalar_i64};
use crate::errors::Result;

pub(super) async fn count_rows<C>(db: &C, backend: DbBackend, table_name: &str) -> Result<i64>
where
    C: ConnectionTrait,
{
    scalar_i64(
        db,
        backend,
        &format!("SELECT COUNT(*) FROM {}", quote_ident(backend, table_name)),
    )
    .await
}

pub(super) fn now_ms() -> i64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    i64::try_from(millis).unwrap_or(i64::MAX)
}

pub(super) fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

pub(super) fn parse_timestamp(value: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(value).ok()
}
