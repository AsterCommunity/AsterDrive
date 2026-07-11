use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::types::{
    BackgroundTaskKind, BackgroundTaskStatus, ConfigVisibility, EntityType, MfaMethod,
    MfaPersistentFactorMethod, TeamMemberRole, UserRole, UserStatus,
};

#[derive(Serialize)]
pub struct ConfigUpdateDetails<'a> {
    pub value: &'a str,
    pub visibility: ConfigVisibility,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prior_visibility: Option<ConfigVisibility>,
}

#[derive(Serialize)]
pub struct ConfigActionDetails<'a> {
    pub action: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_email: Option<&'a str>,
}

#[derive(Serialize)]
pub struct MailAuditDetails<'a> {
    pub to_address: &'a str,
    pub template_code: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outbox_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<&'a str>,
}

#[derive(Serialize)]
pub struct AdminCreateUserDetails<'a> {
    pub email: &'a str,
    pub email_verified: bool,
    pub role: UserRole,
    pub status: UserStatus,
    pub must_change_password: bool,
    pub temporary_password_generated: bool,
    pub storage_quota: i64,
    pub policy_group_id: Option<i64>,
}

#[derive(Serialize)]
pub struct AdminUpdateUserDetails {
    pub changed_fields: Vec<&'static str>,
    pub email_verified: bool,
    pub role: UserRole,
    pub status: UserStatus,
    pub must_change_password: bool,
    pub storage_quota: i64,
    pub policy_group_id: Option<i64>,
    pub previous_email_verified: bool,
    pub previous_role: UserRole,
    pub previous_status: UserStatus,
    pub previous_must_change_password: bool,
    pub previous_storage_quota: i64,
    pub previous_policy_group_id: Option<i64>,
}

#[derive(Serialize)]
pub struct AdminForceDeleteUserDetails {
    pub file_count: usize,
    pub folder_count: usize,
    pub share_count: u64,
    pub webdav_account_count: u64,
    pub upload_session_count: u64,
    pub lock_count: u64,
}

#[derive(Serialize)]
pub struct PolicyGroupAuditDetails {
    pub is_default: bool,
    pub is_enabled: bool,
    pub item_count: usize,
}

#[derive(Serialize)]
pub struct PolicyGroupMigrationDetails<'a> {
    pub source_group_id: i64,
    pub source_group_name: &'a str,
    pub target_group_id: i64,
    pub target_group_name: &'a str,
    pub affected_users: u64,
    pub affected_teams: u64,
    pub migrated_assignments: u64,
}

#[derive(Serialize)]
pub struct StoragePolicyAuditDetails<'a> {
    pub driver_type: &'a str,
    pub remote_node_id: Option<i64>,
    pub max_file_size: i64,
    pub chunk_size: i64,
    pub is_default: bool,
}

#[derive(Serialize)]
pub struct StoragePolicyActionAuditDetails<'a> {
    pub action: &'a str,
    pub driver_type: &'a str,
    pub used_draft_values: bool,
    pub mutates_remote_state: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic_kind: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic_api_code: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic_retryable: Option<bool>,
}

#[derive(Serialize)]
pub struct FolderPolicyAuditDetails {
    pub previous_policy_id: Option<i64>,
    pub policy_id: Option<i64>,
}

#[derive(Serialize)]
pub struct BatchDeleteDetails<'a> {
    pub file_ids: &'a [i64],
    pub folder_ids: &'a [i64],
    pub succeeded: u32,
    pub failed: u32,
}

#[derive(Serialize)]
pub struct BatchTransferDetails<'a> {
    pub file_ids: &'a [i64],
    pub folder_ids: &'a [i64],
    pub target_folder_id: Option<i64>,
    pub succeeded: u32,
    pub failed: u32,
}

#[derive(Serialize)]
pub struct WorkspaceTransferScopeDetails {
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<i64>,
}

#[derive(Serialize)]
pub struct WorkspaceTransferCopyDetails<'a> {
    pub source_workspace: WorkspaceTransferScopeDetails,
    pub destination_workspace: WorkspaceTransferScopeDetails,
    pub file_ids: &'a [i64],
    pub folder_ids: &'a [i64],
    pub target_folder_id: Option<i64>,
    pub succeeded: u32,
    pub failed: u32,
}

#[derive(Serialize)]
pub struct ArchiveSelectionAuditDetails<'a> {
    pub file_ids: &'a [i64],
    pub folder_ids: &'a [i64],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_folder_id: Option<i64>,
}

#[derive(Serialize)]
pub struct UploadCancelAuditDetails<'a> {
    pub upload_id: &'a str,
}

#[derive(Serialize)]
pub struct FileAccessTokenAuditDetails<'a> {
    pub source: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_key: Option<&'a str>,
}

#[derive(Serialize)]
pub struct PropertyAuditDetails<'a> {
    pub entity_type: &'a str,
    pub namespace: &'a str,
    pub name: &'a str,
}

#[derive(Serialize)]
pub struct TagAuditDetails<'a> {
    pub name: &'a str,
    pub color: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_color: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_color: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<i64>,
}

#[derive(Serialize)]
pub struct TagAssignmentAuditDetails<'a> {
    pub operation: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_color: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<EntityType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<i64>,
}

#[derive(Serialize)]
pub struct FileVersionAuditDetails {
    pub version_id: i64,
}

#[derive(Serialize)]
pub struct TrashPurgeAllAuditDetails {
    pub phase: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purged: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<i64>,
}

#[derive(Serialize)]
pub struct RemoteNodeParamTestAuditDetails<'a> {
    pub base_url: &'a str,
    pub success: bool,
    pub protocol_version: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_version: Option<&'a str>,
    pub supports_list: bool,
    pub supports_range_read: bool,
    pub supports_stream_upload: bool,
    pub supports_capacity: bool,
}

#[derive(Serialize)]
pub struct RemoteNodeEnrollmentTokenAuditDetails {
    pub expires_at: DateTime<Utc>,
}

#[derive(Serialize)]
// TODO(remote-storage-target): detail type name follows the stable audit action
// names. Payload fields already use target_key.
pub struct RemoteIngressProfileDeleteAuditDetails<'a> {
    pub target_key: &'a str,
}

#[derive(Serialize)]
pub struct TaskRetryAuditDetails {
    pub kind: String,
    pub previous_attempt_count: i32,
}

#[derive(Serialize)]
pub struct AdminTaskCleanupAuditDetails {
    pub removed: u64,
    pub finished_before: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<BackgroundTaskKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<BackgroundTaskStatus>,
}

#[derive(Serialize)]
pub struct AdminBlobMaintenanceAuditDetails<'a> {
    pub action: crate::services::task::types::BlobMaintenanceAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob_ids: Option<&'a [i64]>,
}

#[derive(Serialize)]
pub struct ShareBatchDeleteDetails<'a> {
    pub share_ids: &'a [i64],
    pub succeeded: u32,
    pub failed: u32,
}

#[derive(Serialize)]
pub struct ShareUpdateDetails {
    pub has_password: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub max_downloads: i64,
}

#[derive(Serialize)]
pub struct ShareCreateAuditDetails<'a> {
    pub token: &'a str,
    pub target_type: EntityType,
    pub target_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<i64>,
    pub has_password: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    pub max_downloads: i64,
}

#[derive(Serialize)]
pub struct ShareDeleteAuditDetails<'a> {
    pub token: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_type: Option<EntityType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<i64>,
    pub has_password: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    pub max_downloads: i64,
}

#[derive(Serialize)]
pub struct AuthSessionAuditDetails<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub removed: Option<u64>,
    pub revoked_current: bool,
}

#[derive(Serialize)]
pub struct UserProfileAuditDetails<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<&'a str>,
}

#[derive(Serialize)]
pub struct UserPreferencesAuditDetails<'a> {
    pub changed_fields: Vec<&'a str>,
    pub custom_upsert_count: usize,
    pub custom_remove_count: usize,
}

#[derive(Serialize)]
pub struct UserAvatarUploadAuditDetails {
    pub source: crate::types::AvatarSource,
    pub version: i32,
}

#[derive(Serialize)]
pub struct UserAvatarSourceAuditDetails<'a> {
    pub source: &'a str,
}

#[derive(Serialize)]
pub struct UserMfaManageAuditDetails<'a> {
    pub method: MfaPersistentFactorMethod,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub factor_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub factor_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub factor_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_code_count: Option<usize>,
}

#[derive(Serialize)]
pub struct MfaEmailCodeAuditDetails {
    pub method: MfaMethod,
    pub flow_id: i64,
    pub expires_in: u64,
    pub resend_after: u64,
}

#[derive(Serialize)]
pub struct UserWopiInfoAuditDetails<'a> {
    pub file_id: i64,
    pub app_key: &'a str,
    pub user_info_len: usize,
}

#[derive(Serialize)]
pub struct UserLoginAuditDetails<'a> {
    pub mfa_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_change_required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_methods: Option<&'a [&'a str]>,
}

#[derive(Serialize)]
pub struct MfaChallengeAuditDetails<'a> {
    pub method: MfaMethod,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_change_required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<&'a str>,
}

#[derive(Serialize)]
pub struct RemoteNodeAuditDetails<'a> {
    pub base_url: &'a str,
    pub is_enabled: bool,
    pub enrollment_status: &'a str,
}

#[derive(Serialize)]
// TODO(remote-storage-target): detail type name follows the stable audit action
// names. Payload fields already use target_key.
pub struct RemoteIngressProfileAuditDetails<'a> {
    pub target_key: &'a str,
    pub driver_type: &'a str,
    pub is_default: bool,
}

#[derive(Serialize)]
pub struct FollowerBindingAuditDetails<'a> {
    pub binding_id: i64,
    pub name: &'a str,
    pub is_enabled: bool,
}

#[derive(Serialize)]
pub struct FollowerObjectAuditDetails<'a> {
    pub binding_id: i64,
    pub object_key: &'a str,
    pub storage_path: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_written: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parts: Option<&'a [String]>,
}

#[derive(Serialize)]
// TODO(remote-storage-target): detail type name follows the stable audit action
// names. Payload fields already use target_key.
pub struct FollowerIngressProfileAuditDetails<'a> {
    pub binding_id: i64,
    pub target_key: &'a str,
    pub driver_type: &'a str,
    pub is_default: bool,
}

#[derive(Serialize)]
pub struct ExternalAuthUnlinkAuditDetails<'a> {
    pub provider_key: &'a str,
    pub issuer: &'a str,
    pub subject: &'a str,
}

#[derive(Serialize)]
pub struct RemoteEnrollmentAuditDetails<'a> {
    pub phase: &'a str,
    pub remote_node_id: i64,
    pub remote_node_name: &'a str,
    pub is_enabled: bool,
}

#[derive(Serialize)]
pub struct InvitationAuditDetails<'a> {
    pub email: &'a str,
    pub status: crate::types::UserInvitationStatus,
    pub invited_by: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted_user_id: Option<i64>,
    pub expires_at: DateTime<Utc>,
    pub mail_queued: bool,
}

#[derive(Serialize)]
pub struct LockAuditDetails {
    pub entity_type: EntityType,
    pub entity_id: i64,
}

#[derive(Serialize)]
pub struct LockCleanupAuditDetails {
    pub removed: u64,
}

#[derive(Serialize)]
pub struct TeamAuditDetails<'a> {
    #[serde(skip_serializing_if = "str::is_empty")]
    pub description: &'a str,
    pub member_count: u64,
    pub storage_quota: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_group_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_role: Option<TeamMemberRole>,
}

#[derive(Serialize)]
pub struct TeamCleanupAuditDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<DateTime<Utc>>,
    pub retention_days: i64,
}

#[derive(Serialize)]
pub struct TeamMemberAddAuditDetails<'a> {
    pub member_user_id: i64,
    pub member_username: &'a str,
    pub role: TeamMemberRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_role: Option<TeamMemberRole>,
}

#[derive(Serialize)]
pub struct TeamMemberUpdateAuditDetails<'a> {
    pub member_user_id: i64,
    pub member_username: &'a str,
    pub previous_role: TeamMemberRole,
    pub next_role: TeamMemberRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_role: Option<TeamMemberRole>,
}

#[derive(Serialize)]
pub struct TeamMemberRemoveAuditDetails<'a> {
    pub member_user_id: i64,
    pub member_username: &'a str,
    pub removed_role: TeamMemberRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_role: Option<TeamMemberRole>,
}

pub fn details<T: Serialize>(value: T) -> Option<serde_json::Value> {
    match serde_json::to_value(value) {
        Ok(value) => Some(value),
        Err(e) => {
            tracing::warn!("failed to serialize audit details: {e}");
            None
        }
    }
}
