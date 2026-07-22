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
pub mod entities;
pub mod errors;
pub mod metrics;
pub mod runtime;
pub mod services;
pub mod storage;
pub mod types;
pub mod webdav;
