//! 审计日志的 `details` JSON 载荷结构。
//!
//! 每个结构对应一种 `AuditAction` 的展示上下文：经 [`details`] 序列化后随
//! 审计条目持久化，查询时由 `presentation` 按 key 提取展示文案，敏感 key
//! （token、凭据、会话 ID 等）在展示前统一剔除。

use chrono::{DateTime, Utc};
use serde::Serialize;

use aster_drive_model::types::{
    BackgroundTaskKind, BackgroundTaskStatus, EntityType, MfaMethod, MfaPersistentFactorMethod,
    TeamMemberRole, UserRole, UserStatus,
};
use aster_forge_config::ConfigVisibility;

/// Audit details for a system config key update.
#[derive(Serialize)]
pub struct ConfigUpdateDetails<'a> {
    /// Rendered config value string; sensitive values are replaced with a
    /// redaction placeholder before the audit entry is written.
    pub value: &'a str,
    /// Visibility of the config value after the update.
    pub visibility: ConfigVisibility,
    /// Visibility before the update, present only when it changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prior_visibility: Option<ConfigVisibility>,
}

/// Audit details for a config action execution (e.g. a test email or a CLI probe).
#[derive(Serialize)]
pub struct ConfigActionDetails<'a> {
    /// Executed action name, serialized from `ConfigActionType`.
    pub action: &'a str,
    /// Email address targeted by the action, when the action sends mail.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_email: Option<&'a str>,
}

/// Audit details for a mail send or delivery failure.
#[derive(Serialize)]
pub struct MailAuditDetails<'a> {
    /// Recipient email address the mail was sent to.
    pub to_address: &'a str,
    /// Mail template code that produced the message.
    pub template_code: &'a str,
    /// Recipient display name, truncated to the audit field limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_name: Option<&'a str>,
    /// Rendered subject line, truncated to the audit field limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<&'a str>,
    /// Outbox row ID the mail was dispatched from, when it went through the queue.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outbox_id: Option<i64>,
    /// Delivery attempt number for outbox-driven sends, when reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_count: Option<i32>,
    /// Delivery failure error message; only set for failure audits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<&'a str>,
}

/// Audit details for a user created by an administrator.
#[derive(Serialize)]
pub struct AdminCreateUserDetails<'a> {
    /// Email address of the created user.
    pub email: &'a str,
    /// Whether the email address starts verified.
    pub email_verified: bool,
    /// Initial role assigned to the user.
    pub role: UserRole,
    /// Initial account status.
    pub status: UserStatus,
    /// Whether the user must change the initial password before other actions.
    pub must_change_password: bool,
    /// Whether the server generated a temporary password for the user.
    pub temporary_password_generated: bool,
    /// Personal storage quota in bytes; `0` = unlimited.
    pub storage_quota: i64,
    /// Policy group assigned to the user, if any.
    pub policy_group_id: Option<i64>,
}

/// Audit details for an administrator user update, capturing before/after state.
#[derive(Serialize)]
pub struct AdminUpdateUserDetails {
    /// Field names that changed, as snake_case identifiers.
    pub changed_fields: Vec<&'static str>,
    /// Email verification state after the update.
    pub email_verified: bool,
    /// Role after the update.
    pub role: UserRole,
    /// Account status after the update.
    pub status: UserStatus,
    /// Password-change requirement after the update.
    pub must_change_password: bool,
    /// Storage quota in bytes after the update; `0` = unlimited.
    pub storage_quota: i64,
    /// Policy group after the update.
    pub policy_group_id: Option<i64>,
    /// Email verification state before the update.
    pub previous_email_verified: bool,
    /// Role before the update.
    pub previous_role: UserRole,
    /// Account status before the update.
    pub previous_status: UserStatus,
    /// Password-change requirement before the update.
    pub previous_must_change_password: bool,
    /// Storage quota in bytes before the update; `0` = unlimited.
    pub previous_storage_quota: i64,
    /// Policy group before the update.
    pub previous_policy_group_id: Option<i64>,
}

/// Audit details for a forced user deletion, capturing the purged resource counts.
#[derive(Serialize)]
pub struct AdminForceDeleteUserDetails {
    /// Files deleted with the user.
    pub file_count: usize,
    /// Folders deleted with the user.
    pub folder_count: usize,
    /// Shares deleted with the user.
    pub share_count: u64,
    /// WebDAV accounts deleted with the user.
    pub webdav_account_count: u64,
    /// Upload sessions deleted with the user.
    pub upload_session_count: u64,
    /// Resource locks deleted with the user.
    pub lock_count: u64,
}

/// Audit details for a storage policy group create or update.
#[derive(Serialize)]
pub struct PolicyGroupAuditDetails {
    /// Whether the group is the default placement group.
    pub is_default: bool,
    /// Whether the group participates in placement routing.
    pub is_enabled: bool,
    /// Number of placement rules in the group.
    pub rule_count: usize,
    /// Total number of targets across all rules.
    pub target_count: usize,
    /// Placement routing revision of the group; every mutation bumps it so
    /// compiled placement snapshots refresh after the commit.
    pub routing_revision: i64,
}

/// Audit details for migrating users and teams to a different policy group.
#[derive(Serialize)]
pub struct PolicyGroupMigrationDetails<'a> {
    /// ID of the group assignments migrated from.
    pub source_group_id: i64,
    /// Name of the group assignments migrated from.
    pub source_group_name: &'a str,
    /// ID of the group assignments migrated to.
    pub target_group_id: i64,
    /// Name of the group assignments migrated to.
    pub target_group_name: &'a str,
    /// Users moved to the target group.
    pub affected_users: u64,
    /// Teams moved to the target group.
    pub affected_teams: u64,
    /// Individual assignments rewritten during the migration.
    pub migrated_assignments: u64,
}

/// Audit details for a storage policy create or update.
#[derive(Serialize)]
pub struct StoragePolicyAuditDetails<'a> {
    /// Storage connector the policy routes to.
    pub connector_id: &'a str,
    /// Maximum single-file upload size in bytes; `0` = unlimited.
    pub max_file_size: i64,
    /// Chunked upload part size in bytes.
    pub chunk_size: i64,
    /// Whether the policy is the default storage policy.
    pub is_default: bool,
}

/// Audit details for a storage policy connector promotion commit.
#[derive(Serialize)]
pub struct StoragePolicyPromotionAuditDetails<'a> {
    /// Promotion run the commit belongs to.
    pub promotion_id: &'a str,
    /// Connector the promotion copies blobs from.
    pub source_connector_id: &'a str,
    /// Connector the promotion makes primary.
    pub target_connector_id: &'a str,
    /// Blobs verified on the target connector before the commit.
    pub verified_blob_count: usize,
}

/// Audit details for a connector action executed from a storage policy.
#[derive(Serialize)]
pub struct StoragePolicyActionAuditDetails<'a> {
    /// Executed connector action identifier.
    pub action: &'a str,
    /// Connector the action ran against.
    pub connector_id: &'a str,
    /// Whether the action ran with unsaved draft values instead of saved policy values.
    pub used_draft_values: bool,
    /// Whether the action mutates state on the remote storage provider.
    pub mutates_remote_state: bool,
    /// Diagnostic category reported by the connector, when the action failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic_kind: Option<&'a str>,
    /// Stable API error code carried by the diagnostic, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic_api_code: Option<&'a str>,
    /// Whether the diagnostic marks the failure retryable, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic_retryable: Option<bool>,
}

/// Audit details for a folder storage policy assignment change.
#[derive(Serialize)]
pub struct FolderPolicyAuditDetails {
    /// Policy ID before the change, `None` when no policy was set.
    pub previous_policy_id: Option<i64>,
    /// Policy ID after the change, `None` when the override is removed.
    pub policy_id: Option<i64>,
}

/// Audit details for a batch delete operation.
#[derive(Serialize)]
pub struct BatchDeleteDetails<'a> {
    /// Workspace activity operation the batch was recorded under.
    pub workspace_activity_operation_id: &'a str,
    /// IDs of files deleted by the batch.
    pub file_ids: &'a [i64],
    /// IDs of folders deleted by the batch.
    pub folder_ids: &'a [i64],
    /// Number of entries deleted successfully.
    pub succeeded: u32,
    /// Number of entries that failed to delete.
    pub failed: u32,
}

/// Audit details for a batch copy or move within one workspace.
#[derive(Serialize)]
pub struct BatchTransferDetails<'a> {
    /// IDs of files included in the batch.
    pub file_ids: &'a [i64],
    /// IDs of folders included in the batch.
    pub folder_ids: &'a [i64],
    /// Destination folder for the batch, `None` when copying into the same parent.
    pub target_folder_id: Option<i64>,
    /// Number of entries transferred successfully.
    pub succeeded: u32,
    /// Number of entries that failed to transfer.
    pub failed: u32,
}

/// Identifies a workspace scope in cross-workspace transfer audit details.
#[derive(Serialize)]
pub struct WorkspaceTransferScopeDetails {
    /// Scope kind; `"personal"` or `"team"`.
    pub kind: &'static str,
    /// Team ID for team scopes; absent for personal scopes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<i64>,
}

/// Audit details for a cross-workspace batch copy.
#[derive(Serialize)]
pub struct WorkspaceTransferCopyDetails<'a> {
    /// Workspace the entries were copied from.
    pub source_workspace: WorkspaceTransferScopeDetails,
    /// Workspace the entries were copied into.
    pub destination_workspace: WorkspaceTransferScopeDetails,
    /// IDs of files included in the batch.
    pub file_ids: &'a [i64],
    /// IDs of folders included in the batch.
    pub folder_ids: &'a [i64],
    /// Destination folder in the target workspace, `None` for its root.
    pub target_folder_id: Option<i64>,
    /// Number of entries copied successfully.
    pub succeeded: u32,
    /// Number of entries that failed to copy.
    pub failed: u32,
}

/// Audit details for a cross-workspace batch move.
#[derive(Serialize)]
pub struct WorkspaceTransferMoveDetails<'a> {
    /// Workspace the entries were moved from.
    pub source_workspace: WorkspaceTransferScopeDetails,
    /// Workspace the entries were moved into.
    pub destination_workspace: WorkspaceTransferScopeDetails,
    /// IDs of files included in the batch.
    pub file_ids: &'a [i64],
    /// IDs of folders included in the batch.
    pub folder_ids: &'a [i64],
    /// Destination folder in the target workspace, `None` for its root.
    pub target_folder_id: Option<i64>,
    /// Number of entries moved successfully.
    pub succeeded: u32,
    /// Number of entries that failed to move.
    pub failed: u32,
}

/// Audit details for an archive operation over a file/folder selection.
#[derive(Serialize)]
pub struct ArchiveSelectionAuditDetails<'a> {
    /// IDs of files in the archived selection.
    pub file_ids: &'a [i64],
    /// IDs of folders in the archived selection.
    pub folder_ids: &'a [i64],
    /// Archive output name; an extraction folder name or download archive label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive_name: Option<&'a str>,
    /// Folder the archive was written into, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_folder_id: Option<i64>,
}

/// Audit details for a cancelled upload.
#[derive(Serialize)]
pub struct UploadCancelAuditDetails<'a> {
    /// Public ID of the upload session that was cancelled.
    pub upload_id: &'a str,
}

/// Audit details for a file access token issued by a direct link, preview
/// link, or WOPI open.
#[derive(Serialize)]
pub struct FileAccessTokenAuditDetails<'a> {
    /// Token origin; `"direct_link"`, `"preview_link"`, or `"wopi"`.
    pub source: &'a str,
    /// WOPI preview app key that opened the session; absent for other sources.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_key: Option<&'a str>,
}

/// Audit details for a custom property set or delete.
#[derive(Serialize)]
pub struct PropertyAuditDetails<'a> {
    /// Entity kind the property belongs to, as the entity type label.
    pub entity_type: &'a str,
    /// Namespace the property is registered under.
    pub namespace: &'a str,
    /// Property name within its namespace.
    pub name: &'a str,
}

/// Audit details for a tag create, update, or delete snapshot.
#[derive(Serialize)]
pub struct TagAuditDetails<'a> {
    /// Tag name in effect for the audited action.
    pub name: &'a str,
    /// Tag color in effect for the audited action.
    pub color: &'a str,
    /// Tag name before an update; absent for create and delete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_name: Option<&'a str>,
    /// Tag name after an update; absent for create and delete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_name: Option<&'a str>,
    /// Tag color before an update; absent for create and delete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_color: Option<&'a str>,
    /// Tag color after an update; absent for create and delete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_color: Option<&'a str>,
    /// Team owning the tag; absent for personal tags.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<i64>,
}

/// Audit details for attaching, detaching, or replacing tag assignments.
#[derive(Serialize)]
pub struct TagAssignmentAuditDetails<'a> {
    /// Assignment operation; `"attach"`, `"detach"`, `"replace"`,
    /// `"batch_attach"`, or `"batch_detach"`.
    pub operation: &'a str,
    /// Tag ID involved in the single-entity operations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_id: Option<i64>,
    /// Tag name involved in the single-entity operations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_name: Option<&'a str>,
    /// Tag color involved in the single-entity operations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_color: Option<&'a str>,
    /// Entity kind targeted by attach, detach, or replace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<EntityType>,
    /// Entity ID targeted by attach, detach, or replace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<i64>,
    /// Number of files in a batch operation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_count: Option<usize>,
    /// Number of folders in a batch operation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder_count: Option<usize>,
    /// Number of tags the entity carries after a replace operation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_count: Option<usize>,
    /// Team owning the tags; absent for personal scopes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<i64>,
}

/// Audit details for a file version action.
#[derive(Serialize)]
pub struct FileVersionAuditDetails {
    /// ID of the file version the action targeted.
    pub version_id: i64,
}

/// Audit details for a bulk trash purge, recorded per phase.
#[derive(Serialize)]
pub struct TrashPurgeAllAuditDetails {
    /// Purge phase; `"requested"` when the task is queued, otherwise it marks
    /// a finished purge.
    pub phase: &'static str,
    /// Number of entries purged; only present on the finished phase.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purged: Option<u32>,
    /// Team whose trash was purged; absent for personal trash.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<i64>,
}

/// Audit details for a remote node connectivity parameter test.
#[derive(Serialize)]
pub struct RemoteNodeParamTestAuditDetails<'a> {
    /// Base URL that was probed.
    pub base_url: &'a str,
    /// Whether the probe reported success.
    pub success: bool,
    /// Protocol version reported by the node.
    pub protocol_version: &'a str,
    /// Server version reported by the node, when advertised.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_version: Option<&'a str>,
    /// Whether the node advertises directory listing support.
    pub supports_list: bool,
    /// Whether the node advertises ranged reads.
    pub supports_range_read: bool,
    /// Whether the node advertises stream uploads.
    pub supports_stream_upload: bool,
    /// Whether the node advertises capacity reporting.
    pub supports_capacity: bool,
}

/// Audit details for issuing a remote node enrollment token.
#[derive(Serialize)]
pub struct RemoteNodeEnrollmentTokenAuditDetails {
    /// Expiry of the issued enrollment token.
    pub expires_at: DateTime<Utc>,
}

/// Audit details for deleting a remote storage target definition.
#[derive(Serialize)]
pub struct RemoteStorageTargetDeleteAuditDetails<'a> {
    /// Stable key identifying the deleted target.
    pub target_key: &'a str,
}

/// Audit details for a manual background task retry.
#[derive(Serialize)]
pub struct TaskRetryAuditDetails {
    /// Debug-rendered task kind that was retried.
    pub kind: String,
    /// Delivery attempt count of the task before the retry.
    pub previous_attempt_count: i32,
}

/// Audit details for an administrative background task cleanup.
#[derive(Serialize)]
pub struct AdminTaskCleanupAuditDetails {
    /// Number of task rows removed.
    pub removed: u64,
    /// Cutoff timestamp; only tasks finished before it were eligible.
    pub finished_before: DateTime<Utc>,
    /// Task kind filter applied to the cleanup, when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<BackgroundTaskKind>,
    /// Task status filter applied to the cleanup, when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<BackgroundTaskStatus>,
}

/// Audit details for creating an administrative blob maintenance task.
#[derive(Serialize)]
pub struct AdminBlobMaintenanceAuditDetails<'a> {
    /// Maintenance action the task performs.
    pub action: crate::services::task::types::BlobMaintenanceAction,
    /// Explicit blob ID subset to operate on; absent means the whole store.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob_ids: Option<&'a [i64]>,
}

/// Audit details for a batch share delete.
#[derive(Serialize)]
pub struct ShareBatchDeleteDetails<'a> {
    /// IDs of the shares included in the batch.
    pub share_ids: &'a [i64],
    /// Number of shares deleted successfully.
    pub succeeded: u32,
    /// Number of shares that failed to delete.
    pub failed: u32,
}

/// Audit details for a share update.
#[derive(Serialize)]
pub struct ShareUpdateDetails {
    /// Whether the share is password protected after the update.
    pub has_password: bool,
    /// Expiry of the share after the update; `None` = no expiry.
    pub expires_at: Option<DateTime<Utc>>,
    /// Download cap after the update; `0` = unlimited.
    pub max_downloads: i64,
}

/// Audit details for a share creation.
#[derive(Serialize)]
pub struct ShareCreateAuditDetails<'a> {
    /// Share token; redacted from audit display before it reaches clients.
    pub token: &'a str,
    /// Kind of entity the share exposes.
    pub target_type: EntityType,
    /// ID of the shared entity.
    pub target_id: i64,
    /// Team owning the share; absent for personal shares.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<i64>,
    /// Whether the share is password protected.
    pub has_password: bool,
    /// Expiry of the share; `None` = no expiry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    /// Download cap; `0` = unlimited.
    pub max_downloads: i64,
}

/// Audit details for a share deletion, carrying the share snapshot at delete time.
#[derive(Serialize)]
pub struct ShareDeleteAuditDetails<'a> {
    /// Share token; redacted from audit display before it reaches clients.
    pub token: &'a str,
    /// Kind of entity the share exposed; absent when the share row was unreadable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_type: Option<EntityType>,
    /// ID of the shared entity; absent when the share row was unreadable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<i64>,
    /// Team owning the share; absent for personal shares.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<i64>,
    /// Whether the deleted share was password protected.
    pub has_password: bool,
    /// Expiry of the deleted share; `None` = no expiry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    /// Download cap of the deleted share; `0` = unlimited.
    pub max_downloads: i64,
}

/// Audit details for auth session revocation.
#[derive(Serialize)]
pub struct AuthSessionAuditDetails<'a> {
    /// Refresh session ID that was revoked, when a single session was targeted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<&'a str>,
    /// Number of sessions removed in a revoke-others operation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub removed: Option<u64>,
    /// Whether the revoked session was the caller's current session.
    pub revoked_current: bool,
}

/// Audit details for a user profile update.
#[derive(Serialize)]
pub struct UserProfileAuditDetails<'a> {
    /// Display name set by the update; absent when it was cleared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<&'a str>,
}

/// Audit details for a user preferences update.
#[derive(Serialize)]
pub struct UserPreferencesAuditDetails<'a> {
    /// Preference field names that changed, as snake_case identifiers.
    pub changed_fields: Vec<&'a str>,
    /// Number of custom preference entries upserted.
    pub custom_upsert_count: usize,
    /// Number of custom preference entries removed.
    pub custom_remove_count: usize,
}

/// Audit details for a user avatar image upload.
#[derive(Serialize)]
pub struct UserAvatarUploadAuditDetails {
    /// Avatar source selected after the upload.
    pub source: aster_drive_model::types::AvatarSource,
    /// Avatar version the upload produced.
    pub version: i32,
    /// Whether the uploaded image replaced the current avatar; `false` when
    /// a concurrent upload superseded it.
    pub applied: bool,
}

/// Audit details for switching the user's avatar source.
#[derive(Serialize)]
pub struct UserAvatarSourceAuditDetails<'a> {
    /// Newly selected avatar source name; `"none"`, `"gravatar"`, or `"upload"`.
    pub source: &'a str,
}

/// Audit details for a user MFA factor management action.
#[derive(Serialize)]
pub struct UserMfaManageAuditDetails<'a> {
    /// MFA factor method the action operated on.
    pub method: MfaPersistentFactorMethod,
    /// Factor row ID, when the action targeted a specific factor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub factor_id: Option<i64>,
    /// Factor display name, when the action targeted a specific factor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub factor_name: Option<&'a str>,
    /// Number of factors remaining after a disable-all action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub factor_count: Option<usize>,
    /// Number of recovery codes issued or remaining, when reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_code_count: Option<usize>,
}

/// Audit details for sending an MFA email verification code.
#[derive(Serialize)]
pub struct MfaEmailCodeAuditDetails {
    /// MFA method the code belongs to; always the email-code variant here.
    pub method: MfaMethod,
    /// MFA login flow the code was issued under.
    pub flow_id: i64,
    /// Code validity window in seconds.
    pub expires_in: u64,
    /// Minimum seconds before another code can be requested.
    pub resend_after: u64,
}

/// Audit details for a WOPI `CheckUserInfo` round trip.
#[derive(Serialize)]
pub struct UserWopiInfoAuditDetails<'a> {
    /// File the WOPI session was opened against.
    pub file_id: i64,
    /// WOPI preview app key that owns the session.
    pub app_key: &'a str,
    /// Size of the user-info payload the client posted, in bytes.
    pub user_info_len: usize,
}

/// Audit details for a completed password login.
#[derive(Serialize)]
pub struct UserLoginAuditDetails<'a> {
    /// Whether the login stopped at an MFA challenge instead of completing.
    pub mfa_required: bool,
    /// Whether the account requires a password change; absent when not reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_change_required: Option<bool>,
    /// MFA method names the user can complete the challenge with.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_methods: Option<&'a [&'a str]>,
}

/// Audit details for an MFA challenge verification outcome.
#[derive(Serialize)]
pub struct MfaChallengeAuditDetails<'a> {
    /// MFA method the challenge was verified with.
    pub method: MfaMethod,
    /// MFA login flow the challenge belongs to, when tracked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow_id: Option<i64>,
    /// Attempt counter of the flow at verification time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_count: Option<i32>,
    /// Whether the account requires a password change after success; absent on failures.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_change_required: Option<bool>,
    /// Stable failure reason code; absent when verification succeeded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<&'a str>,
}

/// Audit details for a remote node create, update, or delete.
#[derive(Serialize)]
pub struct RemoteNodeAuditDetails<'a> {
    /// Base URL of the remote node.
    pub base_url: &'a str,
    /// Whether the node is enabled for follower operations.
    pub is_enabled: bool,
    /// Enrollment lifecycle status name, e.g. `"not_started"`, `"pending"`, `"redeemed"`.
    pub enrollment_status: &'a str,
}

/// Audit details for a remote storage target create or update.
#[derive(Serialize)]
pub struct RemoteStorageTargetAuditDetails<'a> {
    /// Stable key identifying the target.
    pub target_key: &'a str,
    /// Storage driver the target is backed by.
    pub driver_type: &'a str,
}

/// Audit details for a follower binding create or update.
#[derive(Serialize)]
pub struct FollowerBindingAuditDetails<'a> {
    /// Follower binding the action targeted.
    pub binding_id: i64,
    /// Binding display name after the action.
    pub name: &'a str,
    /// Whether the binding is enabled after the action.
    pub is_enabled: bool,
}

/// Stable, redacted context for a remote node reverse-tunnel lifecycle event.
///
/// The tunnel has multiple polling/streaming lanes, but the audit contract is
/// node/binding scoped. Generation counters let operators correlate a recovery
/// with the outage it follows without persisting credentials or URLs.
#[derive(Serialize)]
pub struct RemoteNodeConnectionAuditDetails<'a> {
    /// Remote node the tunnel lifecycle event belongs to.
    pub remote_node_id: i64,
    /// Follower binding the tunnel serves; currently mirrors the node ID.
    pub binding_id: i64,
    /// Transport mode name resolved for the node, e.g. `"reverse_tunnel"`.
    pub transport: &'a str,
    /// Lifecycle event reason, e.g. `"connected"` or a disconnect reason name.
    pub reason: &'a str,
    /// Connection generation: how many times the node has come online.
    pub generation: u64,
    /// Outage generation: how many outages the node has accumulated.
    pub outage_generation: u64,
    /// Streaming lanes currently active on the tunnel.
    pub active_lanes: usize,
    /// Peak number of concurrent streaming lanes observed this connection.
    pub lane_count: usize,
    /// When the lifecycle transition was observed.
    pub observed_at: DateTime<Utc>,
    /// ID of the lane that first connected this generation, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_lane_id: Option<&'a str>,
}

/// Audit details for an object write or delete on a follower binding.
#[derive(Serialize)]
pub struct FollowerObjectAuditDetails<'a> {
    /// Follower binding the object operation ran against.
    pub binding_id: i64,
    /// Object key inside the follower's backing store.
    pub object_key: &'a str,
    /// File path the object is addressed by in the drive.
    pub storage_path: &'a str,
    /// Object size in bytes, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    /// Bytes actually written, when reported by the operation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_written: Option<u64>,
    /// Whether the write completed partially, when reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial: Option<bool>,
    /// Multipart upload part identifiers, when the object was assembled in parts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parts: Option<&'a [String]>,
}

/// Audit details for a follower binding's storage target change.
#[derive(Serialize)]
pub struct FollowerStorageTargetAuditDetails<'a> {
    /// Follower binding the storage target change belongs to.
    pub binding_id: i64,
    /// Stable key identifying the target.
    pub target_key: &'a str,
    /// Storage driver the target is backed by.
    pub driver_type: &'a str,
}

/// Audit details for unlinking an external auth identity.
#[derive(Serialize)]
pub struct ExternalAuthUnlinkAuditDetails<'a> {
    /// External provider the identity was unlink from.
    pub provider_key: &'a str,
    /// OIDC issuer URL of the provider.
    pub issuer: &'a str,
    /// Subject identifier of the unlinked identity.
    pub subject: &'a str,
}

/// Audit details for a remote node enrollment redemption or acknowledgement.
#[derive(Serialize)]
pub struct RemoteEnrollmentAuditDetails<'a> {
    /// Enrollment phase; `"redeemed"` when the token is exchanged, `"acked"`
    /// when the node confirms bootstrap completion.
    pub phase: &'a str,
    /// Remote node being enrolled.
    pub remote_node_id: i64,
    /// Name of the remote node being enrolled.
    pub remote_node_name: &'a str,
    /// Whether the node is enabled after the enrollment phase.
    pub is_enabled: bool,
}

/// Audit details for a user invitation state transition.
#[derive(Serialize)]
pub struct InvitationAuditDetails<'a> {
    /// Email address the invitation was sent to.
    pub email: &'a str,
    /// Invitation status at audit time.
    pub status: aster_drive_model::types::UserInvitationStatus,
    /// User ID that created the invitation.
    pub invited_by: i64,
    /// User ID that accepted the invitation, once redeemed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted_user_id: Option<i64>,
    /// Expiry of the invitation.
    pub expires_at: DateTime<Utc>,
    /// Whether an invitation email was queued for delivery.
    pub mail_queued: bool,
}

/// Audit details for a resource lock action.
#[derive(Serialize)]
pub struct LockAuditDetails {
    /// Kind of root the lock is held on.
    pub entity_type: aster_drive_model::types::LockRootKind,
    /// ID of the locked file or folder; absent for the workspace root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<i64>,
}

/// Audit details for a stale resource lock cleanup.
#[derive(Serialize)]
pub struct LockCleanupAuditDetails {
    /// Number of expired locks removed.
    pub removed: u64,
}

/// Audit details for a team create, update, or delete snapshot.
#[derive(Serialize)]
pub struct TeamAuditDetails<'a> {
    /// Team description after the action; omitted when empty.
    #[serde(skip_serializing_if = "str::is_empty")]
    pub description: &'a str,
    /// Member count after the action.
    pub member_count: u64,
    /// Team storage quota in bytes after the action; `0` = unlimited.
    pub storage_quota: i64,
    /// Policy group assigned to the team; absent when unassigned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_group_id: Option<i64>,
    /// When the team was archived, if it is archived.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<DateTime<Utc>>,
    /// Role the acting caller holds in the team; absent for admin-scope actions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_role: Option<TeamMemberRole>,
}

/// Audit details for purging an expired archived team.
#[derive(Serialize)]
pub struct TeamCleanupAuditDetails {
    /// When the team had been archived.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<DateTime<Utc>>,
    /// Retention window in days that had elapsed before the purge.
    pub retention_days: i64,
}

/// Audit details for adding a member to a team.
#[derive(Serialize)]
pub struct TeamMemberAddAuditDetails<'a> {
    /// User added to the team.
    pub member_user_id: i64,
    /// Username of the added member.
    pub member_username: &'a str,
    /// Role granted to the new member.
    pub role: TeamMemberRole,
    /// Role the acting caller holds in the team; absent for admin-scope actions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_role: Option<TeamMemberRole>,
}

/// Audit details for a team member role change.
#[derive(Serialize)]
pub struct TeamMemberUpdateAuditDetails<'a> {
    /// User whose membership was updated.
    pub member_user_id: i64,
    /// Username of the updated member.
    pub member_username: &'a str,
    /// Role held before the change.
    pub previous_role: TeamMemberRole,
    /// Role granted by the change.
    pub next_role: TeamMemberRole,
    /// Role the acting caller holds in the team; absent for admin-scope actions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_role: Option<TeamMemberRole>,
}

/// Audit details for removing a member from a team.
#[derive(Serialize)]
pub struct TeamMemberRemoveAuditDetails<'a> {
    /// User removed from the team.
    pub member_user_id: i64,
    /// Username of the removed member.
    pub member_username: &'a str,
    /// Role the member held before removal.
    pub removed_role: TeamMemberRole,
    /// Role the acting caller holds in the team; absent for admin-scope actions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_role: Option<TeamMemberRole>,
}

/// Serializes audit details into the JSON value stored with an audit entry.
///
/// Returns `None` when serialization fails; the entry is still written
/// without details instead of failing the audited operation.
pub fn details<T: Serialize>(value: T) -> Option<serde_json::Value> {
    match serde_json::to_value(value) {
        Ok(value) => Some(value),
        Err(e) => {
            tracing::warn!("failed to serialize audit details: {e}");
            None
        }
    }
}

/// Audit details for a background task cancellation request.
#[derive(Serialize)]
pub struct TaskCancelAuditDetails {
    /// Task status observed when the cancellation was requested.
    pub status: String,
    /// Whether the changes of an already-completed task are retained.
    pub completed_changes_retained: bool,
}
