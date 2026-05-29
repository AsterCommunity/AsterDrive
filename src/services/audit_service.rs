//! 服务模块：`audit_service`。

mod context;
mod details;
mod filters;
mod manager;
mod models;
mod query;
#[cfg(test)]
mod tests;

pub use crate::types::{AuditAction, AuditEntityType};
pub use context::{AuditContext, AuditRequestInfo};
pub use details::{
    AdminBlobMaintenanceAuditDetails, AdminCreateUserDetails, AdminForceDeleteUserDetails,
    AdminTaskCleanupAuditDetails, AdminUpdateUserDetails, ArchiveSelectionAuditDetails,
    AuthSessionAuditDetails, BatchDeleteDetails, BatchTransferDetails, ConfigActionDetails,
    ConfigUpdateDetails, FileAccessTokenAuditDetails, FileVersionAuditDetails, LockAuditDetails,
    LockCleanupAuditDetails, PolicyGroupAuditDetails, PolicyGroupMigrationDetails,
    PropertyAuditDetails, RemoteIngressProfileAuditDetails, RemoteNodeAuditDetails,
    ShareBatchDeleteDetails, ShareUpdateDetails, StoragePolicyAuditDetails, TaskRetryAuditDetails,
    TeamAuditDetails, TeamCleanupAuditDetails, TeamMemberAddAuditDetails,
    TeamMemberRemoveAuditDetails, TeamMemberUpdateAuditDetails, TrashPurgeAllAuditDetails,
    UploadCancelAuditDetails, UserAvatarSourceAuditDetails, UserProfileAuditDetails, details,
};
pub use filters::{AuditLogFilterQuery, AuditLogFilters};
pub use manager::{
    flush_global_audit_log_manager, init_global_audit_log_manager, log,
    shutdown_global_audit_log_manager,
};
pub use models::{AuditLogEntry, TeamAuditEntryInfo};
pub use query::{cleanup_expired, query, query_team_entries};
