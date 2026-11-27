//! 产品级时间常量，无层内依赖，供 api、services、storage 等共同引用。

/// 1 hour in seconds.
pub const HOUR_SECS: u64 = 60 * 60;

/// 1 year in seconds (365 days).
pub const YEAR_SECS: u64 = 365 * 24 * HOUR_SECS;
