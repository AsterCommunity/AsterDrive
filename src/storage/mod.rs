//! AsterDrive product storage implementations and runtime adapters.
//!
//! Shared contracts, descriptors, object-key helpers, and structured errors live in the
//! `aster_drive_storage` crate. This module keeps the product-owned connectors, concrete drivers,
//! registry, policy snapshot, and remote-node runtime integration.

pub mod connectors;
pub mod drivers;
mod metrics_driver;
pub mod policy_snapshot;
pub mod registry;
pub mod remote_protocol;
pub use connectors::{
    ExecuteDraftStorageConnectorActionInput, ExecuteSavedStorageConnectorActionInput,
    StorageConnectorActionResult, StorageConnectorConnectionInput, StorageConnectorCredentialInfo,
    StorageConnectorCredentialInput, TencentCosCorsConfigResult,
    TestDraftStorageConnectorConnectionInput,
};
pub use policy_snapshot::PolicySnapshot;
pub use registry::DriverRegistry;
