//! 运行时子模块：`startup`。
mod common;
mod follower;
mod primary;

pub use common::initialize_database_state;
pub use follower::{PreparedFollowerRuntime, prepare_follower};
pub use primary::{PreparedPrimaryRuntime, prepare_primary};
