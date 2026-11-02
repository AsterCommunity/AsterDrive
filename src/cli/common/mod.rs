//! Shared CLI capabilities, each with an explicit owner.
#[cfg(feature = "operations-cli")]
pub(super) mod context;
#[cfg(feature = "operations-cli")]
pub(super) mod database;
#[cfg(feature = "operations-cli")]
pub(super) mod error;
pub(super) mod help;
#[cfg(feature = "operations-cli")]
pub(super) mod input;
#[cfg(feature = "operations-cli")]
pub(super) mod output;
#[cfg(feature = "operations-cli")]
pub(super) mod terminal;
