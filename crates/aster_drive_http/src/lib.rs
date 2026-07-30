//! Shared HTTP client utilities for AsterDrive crates.
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

mod response_body;

pub use response_body::read_reqwest_body_limited;
