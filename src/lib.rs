//! AsterDrive 后端 crate 入口与模块导出。
#![deny(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::unreachable,
        clippy::expect_used,
        clippy::panic,
        clippy::unimplemented,
        clippy::todo
    )
)]

pub mod api;
pub mod build_info;
#[cfg(feature = "cli")]
pub mod cli;
pub mod config;
pub mod db;
pub mod errors;
mod http;
pub mod metrics;
pub(crate) mod ownership;
pub mod runtime;
pub mod services;
pub mod storage;
pub mod webdav;

pub use aster_drive_model::{entities, types};
