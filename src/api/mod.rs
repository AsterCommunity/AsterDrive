//! API 层模块导出。

pub mod api_error_code;
mod common;
pub mod constants;
pub mod dto;
pub mod error_code;
mod follower;
pub mod middleware;
#[cfg(all(debug_assertions, feature = "openapi"))]
pub mod openapi;
pub mod pagination;
mod primary;
pub(crate) mod request_auth;
pub mod response;
pub mod routes;
pub mod subcode;

pub use follower::configure_follower;
pub use primary::configure_primary;
