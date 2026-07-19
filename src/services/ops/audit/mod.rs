//! 审计日志记录、查询和展示服务。

mod context;
mod details;
mod filters;
mod manager;
mod models;
mod presentation;
mod query;
#[cfg(test)]
mod tests;

pub use crate::types::{AuditAction, AuditEntityType};
pub use context::{AuditContext, AuditRequestInfo};
pub use details::{
    AdminBlobMaintenanceAuditDetails, AdminCreateUserDetails, AdminForceDeleteUserDetails,
    AdminTaskCleanupAuditDetails, AdminUpdateUserDetails, ArchiveSelectionAuditDetails,
    AuthSessionAuditDetails, BatchDeleteDetails, BatchTransferDetails, ConfigActionDetails,
    ConfigUpdateDetails, ExternalAuthUnlinkAuditDetails, FileAccessTokenAuditDetails,
    FileVersionAuditDetails, FolderPolicyAuditDetails, FollowerBindingAuditDetails,
    FollowerIngressProfileAuditDetails, FollowerObjectAuditDetails, InvitationAuditDetails,
    LockAuditDetails, LockCleanupAuditDetails, MailAuditDetails, MfaChallengeAuditDetails,
    MfaEmailCodeAuditDetails, PolicyGroupAuditDetails, PolicyGroupMigrationDetails,
    PropertyAuditDetails, RemoteEnrollmentAuditDetails, RemoteIngressProfileAuditDetails,
    RemoteIngressProfileDeleteAuditDetails, RemoteNodeAuditDetails,
    RemoteNodeEnrollmentTokenAuditDetails, RemoteNodeParamTestAuditDetails,
    ShareBatchDeleteDetails, ShareCreateAuditDetails, ShareDeleteAuditDetails, ShareUpdateDetails,
    StoragePolicyActionAuditDetails, StoragePolicyAuditDetails, TagAssignmentAuditDetails,
    TagAuditDetails, TaskRetryAuditDetails, TeamAuditDetails, TeamCleanupAuditDetails,
    TeamMemberAddAuditDetails, TeamMemberRemoveAuditDetails, TeamMemberUpdateAuditDetails,
    TrashPurgeAllAuditDetails, UploadCancelAuditDetails, UserAvatarSourceAuditDetails,
    UserAvatarUploadAuditDetails, UserLoginAuditDetails, UserMfaManageAuditDetails,
    UserPreferencesAuditDetails, UserProfileAuditDetails, UserWopiInfoAuditDetails,
    WorkspaceTransferCopyDetails, WorkspaceTransferMoveDetails, WorkspaceTransferScopeDetails,
    details,
};
pub use filters::{AuditLogFilterQuery, AuditLogFilters};
pub use manager::{
    AuditLogInput, log, log_with_db_and_config, log_with_details, should_record,
    should_record_with_config,
};
pub use models::{AuditLogEntry, AuditPresentation, AuditPresentationMessage, TeamAuditEntryInfo};
pub use query::{cleanup_expired, query, query_team_entries};
