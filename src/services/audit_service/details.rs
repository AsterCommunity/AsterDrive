use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::types::{
    BackgroundTaskKind, BackgroundTaskStatus, EntityType, TeamMemberRole, UserRole, UserStatus,
};

#[derive(Serialize)]
pub struct ConfigUpdateDetails<'a> {
    pub value: &'a str,
}

#[derive(Serialize)]
pub struct ConfigActionDetails<'a> {
    pub action: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_email: Option<&'a str>,
}

#[derive(Serialize)]
pub struct AdminCreateUserDetails<'a> {
    pub email: &'a str,
    pub email_verified: bool,
    pub role: UserRole,
    pub status: UserStatus,
    pub storage_quota: i64,
    pub policy_group_id: Option<i64>,
}

#[derive(Serialize)]
pub struct AdminUpdateUserDetails {
    pub email_verified: bool,
    pub role: UserRole,
    pub status: UserStatus,
    pub storage_quota: i64,
    pub policy_group_id: Option<i64>,
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
pub struct FileVersionAuditDetails {
    pub version_id: i64,
}

#[derive(Serialize)]
pub struct TrashPurgeAllAuditDetails {
    pub purged: u32,
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
    pub action: crate::services::task_service::BlobMaintenanceAction,
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
pub struct UserAvatarSourceAuditDetails<'a> {
    pub source: &'a str,
}

#[derive(Serialize)]
pub struct RemoteNodeAuditDetails<'a> {
    pub base_url: &'a str,
    pub is_enabled: bool,
    pub enrollment_status: &'a str,
}

#[derive(Serialize)]
pub struct RemoteIngressProfileAuditDetails<'a> {
    pub profile_key: &'a str,
    pub driver_type: &'a str,
    pub is_default: bool,
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
