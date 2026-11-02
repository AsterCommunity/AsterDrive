//! Operational diagnostics and deterministic repair adapters.
pub(in crate::cli) mod args;
mod checks;
pub(in crate::cli) mod execute;
pub(in crate::cli) mod render;
mod report;
mod storage_scan;
