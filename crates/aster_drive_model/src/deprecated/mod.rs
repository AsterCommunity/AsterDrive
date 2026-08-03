//! Legacy database models exclusive to the AsterDrive 0.5.0 upgrade path.
//!
//! These modules exist only for upgrade-time data migration and will be
//! completely removed in AsterDrive 0.6.0.

#[deprecated(
    since = "0.5.0",
    note = "legacy migration-only entity; scheduled for removal in AsterDrive 0.6.0"
)]
pub mod storage_connector_application_config;

#[deprecated(
    since = "0.5.0",
    note = "legacy migration-only entity; scheduled for removal in AsterDrive 0.6.0"
)]
pub mod storage_policy_credential;
