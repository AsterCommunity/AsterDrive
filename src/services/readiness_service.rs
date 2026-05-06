//! 服务模块：`readiness_service`。
//!
//! 保留旧模块名，实际实现集中在 `health_service`。

pub use crate::services::health_service::{
    check_follower_ready, check_primary_ready, ping_database,
};
