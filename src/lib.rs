//! AsterDrive 后端 crate 入口与模块导出。
#![deny(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::unreachable,
        clippy::expect_used,
        clippy::panic
    )
)]

pub mod alloc;
pub mod api;
pub mod cache;
#[cfg(feature = "cli")]
pub mod cli;
pub mod config;
pub mod db;
pub mod entities;
pub mod errors;
pub mod external_auth;
#[cfg(feature = "metrics")]
pub mod metrics;
pub mod metrics_core;
pub mod runtime;
pub mod services;
pub mod storage;
pub mod types;
pub mod utils;
pub mod webdav;
