//! Statically declared command owners.
pub(super) mod build_info;
pub(super) mod completions;
#[cfg(feature = "operations-cli")]
pub(super) mod config;
#[cfg(feature = "operations-cli")]
pub(super) mod database_migration;
#[cfg(feature = "operations-cli")]
pub(super) mod doctor;
#[cfg(feature = "operations-cli")]
pub(super) mod hash_password;
#[cfg(feature = "operations-cli")]
pub(super) mod node;
