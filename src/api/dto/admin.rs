//! Admin-only DTOs consolidated from `src/api/routes/admin/`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::{IntoParams, ToSchema};
use validator::{Validate, ValidationError};

use crate::api::pagination::{
    AdminAuditLogSortBy, AdminFileBlobSortBy, AdminFileSortBy, AdminLockSortBy,
    AdminPolicyGroupSortBy, AdminPolicySortBy, AdminRemoteNodeSortBy, AdminShareSortBy,
    AdminTaskSortBy, AdminTeamSortBy, AdminUserSortBy,
};
use aster_forge_api::SortOrder;

// ── Users ──────────────────────────────────────────────────────────────────

/// Query parameters for the admin user list.
#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
/// Query parameters for the admin user list.
pub struct AdminUserListQuery {
    /// Case-insensitive partial match against username or email.
    pub keyword: Option<String>,
    /// Filter by account role.
    pub role: Option<aster_drive_model::types::UserRole>,
    /// Filter by account status.
    pub status: Option<aster_drive_model::types::UserStatus>,
    /// Sort key; defaults to `created_at` when omitted.
    pub sort_by: Option<AdminUserSortBy>,
    /// Sort direction; defaults to descending when omitted.
    pub sort_order: Option<SortOrder>,
}

impl AdminUserListQuery {
    /// Effective sort key, falling back to `created_at`.
    pub fn sort_by(&self) -> AdminUserSortBy {
        self.sort_by.unwrap_or(AdminUserSortBy::CreatedAt)
    }

    /// Effective sort direction, falling back to descending.
    pub fn sort_order(&self) -> SortOrder {
        self.sort_order.unwrap_or(SortOrder::Desc)
    }
}

/// Create a new user (admin operation).
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreateUserReq {
    /// Login name; must satisfy the auth username rules.
    #[validate(custom(function = "crate::api::dto::validation::validate_auth_username"))]
    pub username: String,
    /// Login email; must satisfy the auth email rules and stay unique.
    #[validate(custom(function = "crate::api::dto::validation::validate_auth_email"))]
    pub email: String,
    /// Initial account password; omit to create the account without one.
    pub password: Option<String>,
    /// Whether the user must set a new password at next sign-in.
    pub must_change_password: Option<bool>,
}

/// Create a new user invitation (admin operation).
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreateUserInvitationReq {
    /// Email address that receives the registration invitation.
    #[validate(custom(function = "crate::api::dto::validation::validate_auth_email"))]
    pub email: String,
}

/// Patch an existing user (admin operation).
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PatchUserReq {
    /// Overrides the email verification state; omit to leave unchanged.
    pub email_verified: Option<bool>,
    /// Replaces the account role; omit to leave unchanged.
    pub role: Option<aster_drive_model::types::UserRole>,
    /// Replaces the account status; omit to leave unchanged.
    pub status: Option<aster_drive_model::types::UserStatus>,
    /// Forces a password change at next sign-in; omit to leave unchanged.
    pub must_change_password: Option<bool>,
    /// New storage quota in bytes (`0` = unlimited); omit to leave unchanged.
    #[validate(range(min = 0, message = "storage_quota must be non-negative"))]
    pub storage_quota: Option<i64>,
    /// Omitted = leave unchanged. Explicit `null` is rejected because this
    /// endpoint only supports assigning a policy group, not unassigning one.
    #[serde(
        default,
        deserialize_with = "crate::api::routes::admin::common::deserialize_non_null_policy_group_id"
    )]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<i64>, nullable = false)
    )]
    #[validate(range(min = 1, message = "policy_group_id must be greater than 0"))]
    pub policy_group_id: Option<i64>,
}

/// Set or clear a folder-level storage policy binding (admin operation).
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct SetFolderPolicyReq {
    /// Policy to bind to the folder; `null` clears the existing binding.
    #[validate(range(min = 1, message = "policy_id must be greater than 0"))]
    pub policy_id: Option<i64>,
}

/// Reset a user's password (admin operation).
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ResetUserPasswordReq {
    /// New password; must satisfy the auth password rules.
    #[validate(custom(function = "crate::api::dto::validation::validate_auth_password"))]
    pub password: String,
}

// ── Policies ────────────────────────────────────────────────────────────────

/// Create a storage policy.
#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreatePolicyReq {
    /// Display name of the storage policy; must not be blank.
    #[validate(custom(function = "crate::api::dto::validation::validate_non_blank"))]
    pub name: String,
    /// Connector configuration, behavior, and credential for the backing storage.
    pub connection: crate::storage::StoragePolicyConnectionInput,
    /// Per-file upload size limit in bytes (`0` = unlimited); defaults to `0`.
    #[validate(range(min = 0, message = "max_file_size must be non-negative"))]
    pub max_file_size: Option<i64>,
    /// Multipart/upload chunk size in bytes; omit to use the connector default.
    #[validate(range(min = 1, message = "chunk_size must be greater than 0"))]
    pub chunk_size: Option<i64>,
    /// Whether this policy becomes the deployment default; defaults to `false`.
    pub is_default: Option<bool>,
    /// Allowlist of file extensions; empty or omitted means all types are allowed.
    pub allowed_types: Option<Vec<String>>,
}

/// Patch a storage policy.
#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[validate(schema(function = "validate_patch_policy"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PatchPolicyReq {
    /// New display name; must not be blank when present.
    pub name: Option<String>,
    /// Replaces the connector configuration; omit to leave unchanged.
    pub connector_config: Option<aster_drive_storage::ConnectorConfigEnvelope>,
    /// Replaces the provider-native behavior preferences; omit to leave unchanged.
    pub behavior: Option<aster_drive_storage::StoragePolicyBehaviorConfig>,
    /// Replaces the connector credential; omit to leave unchanged.
    pub credential: Option<crate::storage::StorageConnectorCredentialInput>,
    /// New per-file size limit in bytes (`0` = unlimited); omit to leave unchanged.
    #[validate(range(min = 0, message = "max_file_size must be non-negative"))]
    pub max_file_size: Option<i64>,
    /// New chunk size in bytes; omit to leave unchanged.
    #[validate(range(min = 1, message = "chunk_size must be greater than 0"))]
    pub chunk_size: Option<i64>,
    /// Promotes or demotes this policy as the deployment default; omit to leave unchanged.
    pub is_default: Option<bool>,
    /// Replaces the extension allowlist (empty = allow all); omit to leave unchanged.
    pub allowed_types: Option<Vec<String>>,
}

/// Promote one saved policy to a compatible target connector in place.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PromoteStoragePolicyConnectorReq {
    /// Connector the saved policy is promoted to.
    pub target_connector_id: aster_drive_storage::ConnectorId,
    /// Stable identifier of the target-owned promotion being applied.
    pub promotion_id: aster_drive_storage::StorageConnectorPromotionId,
}

/// Query parameters for deleting a storage policy.
#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
/// Query parameters for deleting a storage policy.
pub struct DeletePolicyQuery {
    /// When `true`, also permanently purge every remaining reference to the policy.
    #[serde(default)]
    pub force: bool,
}

/// High-risk confirmation for permanently purging every remaining policy reference.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreateStoragePolicyForcedPurgeReq {
    /// Digest returned by the latest forced-purge preview.
    pub impact_digest: String,
    /// Exact confirmation phrase returned by the latest forced-purge preview.
    pub confirmation: String,
    /// Administrator explanation retained in task and audit evidence.
    pub reason: String,
}

/// Selects which storage connector descriptors a catalog query returns.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorCatalogContext {
    /// Return every descriptor so existing policies remain inspectable.
    #[default]
    Manage,
    /// Return connectors that can create a policy in the current deployment.
    Create,
    /// Return connectors shown during first-storage setup. Callers must use
    /// `supports_initial_setup` to disable entries that require post-setup work.
    Setup,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
/// Query parameters for the storage connector descriptor catalog.
pub struct StorageConnectorCatalogQuery {
    /// Which connectors the catalog should include; defaults to [`StorageConnectorCatalogContext::Manage`].
    #[serde(default)]
    pub context: StorageConnectorCatalogContext,
}

/// Query for connector-owned admin UI localization resources.
#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
/// Query for connector-owned admin UI localization resources.
pub struct StorageConnectorLocalizationCatalogQuery {
    /// Which connectors the catalog should include; defaults to [`StorageConnectorCatalogContext::Manage`].
    #[serde(default)]
    pub context: StorageConnectorCatalogContext,
    /// BCP-47 locale tag used to resolve connector-owned UI strings.
    #[serde(default)]
    pub locale: aster_drive_model::types::LocaleTag,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
/// Query parameters for remote storage target connector descriptors.
pub struct RemoteStorageTargetConnectorCatalogQuery {
    /// BCP-47 locale tag used to resolve connector-owned UI strings.
    #[serde(default)]
    pub locale: aster_drive_model::types::LocaleTag,
}

impl From<StorageConnectorCatalogContext>
    for crate::services::storage_policy::connector_catalog::StorageConnectorCatalogContext
{
    fn from(value: StorageConnectorCatalogContext) -> Self {
        match value {
            StorageConnectorCatalogContext::Manage => Self::Manage,
            StorageConnectorCatalogContext::Create => Self::Create,
            StorageConnectorCatalogContext::Setup => Self::InitialSetup,
        }
    }
}

/// Test a storage policy connection by parameters (without saving).
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TestPolicyParamsReq {
    /// Existing policy being edited, used to resolve saved edit-form
    /// placeholders; `None` tests a purely draft connection.
    #[validate(range(min = 1, message = "policy_id must be greater than 0"))]
    pub policy_id: Option<i64>,
    /// Draft connection parameters to validate and probe.
    pub connection: crate::storage::StoragePolicyConnectionInput,
}

/// Create a remote node.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreateRemoteNodeReq {
    /// Display name of the remote node; must not be blank.
    #[validate(custom(function = "crate::api::dto::validation::validate_non_blank"))]
    pub name: String,
    /// Base URL of the follower node (http/https); empty for nodes enrolled
    /// later via reverse tunnel.
    pub base_url: Option<String>,
    /// Transport choice; `auto` is resolved from `base_url` at runtime.
    /// Defaults to `direct`.
    #[serde(default)]
    pub transport_mode: aster_drive_model::types::RemoteNodeTransportMode,
    /// Whether the node is enabled for administration and transport.
    /// Defaults to `true`.
    #[serde(default = "default_true")]
    pub is_enabled: bool,
}

/// Patch a remote node.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PatchRemoteNodeReq {
    /// New display name; omit to leave unchanged.
    pub name: Option<String>,
    /// New base URL (http/https); omit to leave unchanged.
    pub base_url: Option<String>,
    /// New transport choice; omit to leave unchanged.
    pub transport_mode: Option<aster_drive_model::types::RemoteNodeTransportMode>,
    /// New enabled flag; omit to leave unchanged.
    pub is_enabled: Option<bool>,
}

/// Test remote node connection without saving.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TestRemoteNodeParamsReq {
    /// Base URL of the node to probe (http/https); must not be blank.
    #[validate(custom(function = "crate::api::dto::validation::validate_non_blank"))]
    pub base_url: String,
    /// Managed access key sent with the probe; must not be blank.
    #[validate(custom(function = "crate::api::dto::validation::validate_non_blank"))]
    pub access_key: String,
    /// Secret key paired with `access_key` for the probe; must not be blank.
    #[validate(custom(function = "crate::api::dto::validation::validate_non_blank"))]
    pub secret_key: String,
}

/// Create a storage policy group.
#[derive(Clone, Deserialize, Validate)]
#[validate(schema(function = "validate_create_policy_group"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreatePolicyGroupReq {
    /// Display name of the group; must not be blank.
    #[validate(custom(function = "crate::api::dto::validation::validate_non_blank"))]
    pub name: String,
    /// Optional human-readable description of the group's purpose.
    pub description: Option<String>,
    /// Whether assignments in this group accept new placements; defaults to `true`.
    #[serde(default = "default_true")]
    pub is_enabled: bool,
    /// Whether new users and teams default to this group; defaults to `false`.
    #[serde(default)]
    pub is_default: bool,
    /// File-level admission checks (extensions, categories, size) applied
    /// before placement; `None` imposes no extra constraints.
    #[serde(default)]
    pub admission:
        Option<crate::services::storage_policy::policy::placement::StorageAdmissionConstraints>,
    /// Upload execution preference for uploads placed through this group;
    /// `None` lets the system choose automatically.
    #[serde(default)]
    pub execution_preference:
        Option<crate::services::storage_policy::policy::placement::UploadExecutionPreference>,
    /// Ordered routing rules matching uploads to member policies; required and
    /// must be non-empty.
    #[serde(default)]
    pub rules: Option<Vec<crate::services::storage_policy::policy::StoragePlacementRuleInput>>,
}

/// Patch a storage policy group.
#[derive(Clone, Deserialize, Validate)]
#[validate(schema(function = "validate_patch_policy_group"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PatchPolicyGroupReq {
    /// New display name; must not be blank when present.
    pub name: Option<String>,
    /// New description; omit to leave unchanged.
    pub description: Option<String>,
    /// New enabled flag; omit to leave unchanged.
    pub is_enabled: Option<bool>,
    /// New default flag; omit to leave unchanged.
    pub is_default: Option<bool>,
    /// Replaces admission checks; `None` omits the change (use an empty
    /// constraints value to clear them).
    #[serde(default)]
    pub admission:
        Option<crate::services::storage_policy::policy::placement::StorageAdmissionConstraints>,
    /// Replaces the upload execution preference; omit to leave unchanged.
    #[serde(default)]
    pub execution_preference:
        Option<crate::services::storage_policy::policy::placement::UploadExecutionPreference>,
    /// Replaces the ordered routing rules; when present the list must be
    /// non-empty.
    #[serde(default)]
    pub rules: Option<Vec<crate::services::storage_policy::policy::StoragePlacementRuleInput>>,
}

/// Migrate all user and team assignments from one policy group to another.
#[derive(Clone, Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct MigratePolicyGroupAssignmentsReq {
    /// Group that receives every migrated user and team assignment; must
    /// differ from the source group.
    #[validate(range(min = 1, message = "target_group_id must be greater than 0"))]
    pub target_group_id: i64,
}

fn default_true() -> bool {
    true
}

// ── Config ─────────────────────────────────────────────────────────────────

/// Set a system configuration value.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct SetConfigReq {
    /// New configuration value; validated against the config key's schema.
    pub value: aster_forge_config::ConfigValue,
    /// Overrides the value's visibility level; omit to keep the current one.
    pub visibility: Option<aster_forge_config::ConfigVisibility>,
}

/// Execute a config action (e.g., send test email).
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ExecuteConfigActionReq {
    /// Action to execute against the config key addressed by the request path.
    pub action: crate::services::ops::config::ConfigActionType,
    /// WOPI discovery URL used by the WOPI preview config action; overrides
    /// the persisted value for this run only.
    pub discovery_url: Option<String>,
    /// Optional transient config values for test actions such as
    /// [`ConfigActionType::TestAria2Rpc`](crate::services::ops::config::ConfigActionType::TestAria2Rpc).
    /// These override persisted config for the action without writing to the DB.
    /// Runtime action handlers validate the accepted keys for their action.
    /// Optional transient config values for test actions such as
    /// [`ConfigActionType::TestAria2Rpc`](crate::services::ops::config::ConfigActionType::TestAria2Rpc).
    /// These override persisted config for the action without writing to the DB.
    /// Runtime action handlers validate the accepted keys for their action.
    #[validate(length(max = 50, message = "draft_values cannot exceed 50 entries"))]
    pub draft_values: Option<std::collections::BTreeMap<String, String>>,
    /// Recipient mailbox for test email actions.
    pub target_email: Option<String>,
    /// Transient config value override for actions that consume the key's
    /// main value (e.g. preview apps JSON or media processing registry);
    /// overrides the persisted value for this run only.
    pub value: Option<String>,
}

/// Response from a config action execution.
#[derive(serde::Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ExecuteConfigActionResp {
    /// Human-readable execution outcome for the admin UI.
    pub message: String,
    /// Optional action-specific result payload (e.g. a resolved preview config).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

// ── Tasks ──────────────────────────────────────────────────────────────────

/// Query parameters for the admin task list.
#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
/// Query parameters for the admin task list.
pub struct AdminTaskListQuery {
    /// Filter by background task kind.
    pub kind: Option<aster_drive_model::types::BackgroundTaskKind>,
    /// Filter by background task lifecycle status.
    pub status: Option<aster_drive_model::types::BackgroundTaskStatus>,
    /// Sort key; defaults to `updated_at` when omitted.
    pub sort_by: Option<AdminTaskSortBy>,
    /// Sort direction; defaults to descending when omitted.
    pub sort_order: Option<SortOrder>,
}

impl AdminTaskListQuery {
    /// Effective sort key, falling back to `updated_at`.
    pub fn sort_by(&self) -> AdminTaskSortBy {
        self.sort_by.unwrap_or(AdminTaskSortBy::UpdatedAt)
    }

    /// Effective sort direction, falling back to descending.
    pub fn sort_order(&self) -> SortOrder {
        self.sort_order.unwrap_or(SortOrder::Desc)
    }
}

/// Cleanup completed background tasks by admin-specified conditions.
#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminTaskCleanupReq {
    /// Cutoff for task completion time; only tasks that finished strictly
    /// before this instant are removed.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub finished_before: DateTime<Utc>,
    /// Restrict cleanup to one task kind; omit to cover all kinds.
    pub kind: Option<aster_drive_model::types::BackgroundTaskKind>,
    /// Restrict cleanup to one task status; omit to cover all statuses.
    pub status: Option<aster_drive_model::types::BackgroundTaskStatus>,
}

/// Create a background task that migrates blobs from one storage policy to another.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreateStoragePolicyMigrationReq {
    /// Policy whose blobs are migrated away; must exist.
    #[validate(range(min = 1, message = "source_policy_id must be greater than 0"))]
    pub source_policy_id: i64,
    /// Policy that receives the migrated blobs; must differ from the source.
    #[validate(range(min = 1, message = "target_policy_id must be greater than 0"))]
    pub target_policy_id: i64,
    /// Selects routine migration or migration of currently recoverable source data.
    #[serde(default)]
    pub mode: crate::services::task::types::StoragePolicyMigrationMode,
    /// Plan hash returned by the latest source recovery probe.
    pub recovery_plan_hash: Option<String>,
}

/// Check a storage policy migration plan without creating a task.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct DryRunStoragePolicyMigrationReq {
    /// Policy whose blobs would be migrated; must differ from the target.
    #[validate(range(min = 1, message = "source_policy_id must be greater than 0"))]
    pub source_policy_id: i64,
    /// Policy that would receive the migrated blobs; must exist.
    #[validate(range(min = 1, message = "target_policy_id must be greater than 0"))]
    pub target_policy_id: i64,
    /// Selects routine migration or recoverable-data preflight.
    #[serde(default)]
    pub mode: crate::services::task::types::StoragePolicyMigrationMode,
    /// Optional recovery evidence to revalidate during dry-run.
    pub recovery_plan_hash: Option<String>,
}

// ── Admin Teams ─────────────────────────────────────────────────────────────

/// Query parameters for the admin team list.
#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
/// Query parameters for the admin team list.
pub struct AdminTeamListQuery {
    /// Case-insensitive partial match against team name or description.
    pub keyword: Option<String>,
    /// Filter by archive state (`true` = archived, `false` = active, the
    /// default when omitted).
    pub archived: Option<bool>,
    /// Sort key; defaults to `created_at` when omitted.
    pub sort_by: Option<AdminTeamSortBy>,
    /// Sort direction; defaults to descending when omitted.
    pub sort_order: Option<SortOrder>,
}

impl AdminTeamListQuery {
    /// Effective sort key, falling back to `created_at`.
    pub fn sort_by(&self) -> AdminTeamSortBy {
        self.sort_by.unwrap_or(AdminTeamSortBy::CreatedAt)
    }

    /// Effective sort direction, falling back to descending.
    pub fn sort_order(&self) -> SortOrder {
        self.sort_order.unwrap_or(SortOrder::Desc)
    }
}

#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
/// Query parameters for the admin storage policy list.
pub struct AdminPolicyListQuery {
    /// Case-insensitive partial match against policy name.
    pub keyword: Option<String>,
    /// Sort key; defaults to `created_at` when omitted.
    pub sort_by: Option<AdminPolicySortBy>,
    /// Sort direction; defaults to descending when omitted.
    pub sort_order: Option<SortOrder>,
}

impl AdminPolicyListQuery {
    /// Effective sort key, falling back to `created_at`.
    pub fn sort_by(&self) -> AdminPolicySortBy {
        self.sort_by.unwrap_or(AdminPolicySortBy::CreatedAt)
    }

    /// Effective sort direction, falling back to descending.
    pub fn sort_order(&self) -> SortOrder {
        self.sort_order.unwrap_or(SortOrder::Desc)
    }
}

#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
/// Query parameters for the admin storage policy group list.
pub struct AdminPolicyGroupListQuery {
    /// Sort key; defaults to `created_at` when omitted.
    pub sort_by: Option<AdminPolicyGroupSortBy>,
    /// Sort direction; defaults to descending when omitted.
    pub sort_order: Option<SortOrder>,
}

impl AdminPolicyGroupListQuery {
    /// Effective sort key, falling back to `created_at`.
    pub fn sort_by(&self) -> AdminPolicyGroupSortBy {
        self.sort_by.unwrap_or(AdminPolicyGroupSortBy::CreatedAt)
    }

    /// Effective sort direction, falling back to descending.
    pub fn sort_order(&self) -> SortOrder {
        self.sort_order.unwrap_or(SortOrder::Desc)
    }
}

#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
/// Query parameters for the admin remote node list.
pub struct AdminRemoteNodeListQuery {
    /// Sort key; defaults to `created_at` when omitted.
    pub sort_by: Option<AdminRemoteNodeSortBy>,
    /// Sort direction; defaults to descending when omitted.
    pub sort_order: Option<SortOrder>,
}

impl AdminRemoteNodeListQuery {
    /// Effective sort key, falling back to `created_at`.
    pub fn sort_by(&self) -> AdminRemoteNodeSortBy {
        self.sort_by.unwrap_or(AdminRemoteNodeSortBy::CreatedAt)
    }

    /// Effective sort direction, falling back to descending.
    pub fn sort_order(&self) -> SortOrder {
        self.sort_order.unwrap_or(SortOrder::Desc)
    }
}

#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
/// Query parameters for the admin share list.
pub struct AdminShareListQuery {
    /// Restrict to shares of one resource kind (file or folder); omit to
    /// include both.
    pub target_type: Option<aster_drive_model::types::EntityType>,
    /// Restrict to shares owned by this user (share creator).
    pub user_id: Option<i64>,
    /// Sort key; defaults to `created_at` when omitted.
    pub sort_by: Option<AdminShareSortBy>,
    /// Sort direction; defaults to descending when omitted.
    pub sort_order: Option<SortOrder>,
}

impl AdminShareListQuery {
    /// Effective sort key, falling back to `created_at`.
    pub fn sort_by(&self) -> AdminShareSortBy {
        self.sort_by.unwrap_or(AdminShareSortBy::CreatedAt)
    }

    /// Effective sort direction, falling back to descending.
    pub fn sort_order(&self) -> SortOrder {
        self.sort_order.unwrap_or(SortOrder::Desc)
    }
}

#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
/// Query parameters for the admin resource lock list.
pub struct AdminLockListQuery {
    /// Sort key; defaults to `id` when omitted.
    pub sort_by: Option<AdminLockSortBy>,
    /// Sort direction; defaults to ascending when omitted.
    pub sort_order: Option<SortOrder>,
}

impl AdminLockListQuery {
    /// Effective sort key, falling back to `id`.
    pub fn sort_by(&self) -> AdminLockSortBy {
        self.sort_by.unwrap_or(AdminLockSortBy::Id)
    }

    /// Effective sort direction, falling back to ascending.
    pub fn sort_order(&self) -> SortOrder {
        self.sort_order.unwrap_or(SortOrder::Asc)
    }
}

#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
/// Sort parameters for the admin audit log list and entity history endpoints.
pub struct AdminAuditLogSortQuery {
    /// Sort key; defaults to `created_at` when omitted.
    pub sort_by: Option<AdminAuditLogSortBy>,
    /// Sort direction; defaults to descending when omitted.
    pub sort_order: Option<SortOrder>,
}

impl AdminAuditLogSortQuery {
    /// Effective sort key, falling back to `created_at`.
    pub fn sort_by(&self) -> AdminAuditLogSortBy {
        self.sort_by.unwrap_or(AdminAuditLogSortBy::CreatedAt)
    }

    /// Effective sort direction, falling back to descending.
    pub fn sort_order(&self) -> SortOrder {
        self.sort_order.unwrap_or(SortOrder::Desc)
    }
}

// ── Admin Files / File Blobs ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
/// Query parameters for the admin file list.
pub struct AdminFileListQuery {
    /// Case-insensitive partial match against file name.
    pub name: Option<String>,
    /// Restrict to files backed by this blob.
    pub blob_id: Option<i64>,
    /// Restrict to files whose blob is stored under this policy.
    pub policy_id: Option<i64>,
    /// Restrict to files owned by this user.
    pub owner_user_id: Option<i64>,
    /// Restrict to files in this team.
    pub team_id: Option<i64>,
    /// Filter by trash state (`true` = in trash, `false` = active, the
    /// default when omitted).
    pub deleted: Option<bool>,
    /// Sort key; defaults to `created_at` when omitted.
    pub sort_by: Option<AdminFileSortBy>,
    /// Sort direction; defaults to descending when omitted.
    pub sort_order: Option<SortOrder>,
}

impl AdminFileListQuery {
    /// Effective sort key, falling back to `created_at`.
    pub fn sort_by(&self) -> AdminFileSortBy {
        self.sort_by.unwrap_or(AdminFileSortBy::CreatedAt)
    }

    /// Effective sort direction, falling back to descending.
    pub fn sort_order(&self) -> SortOrder {
        self.sort_order.unwrap_or(SortOrder::Desc)
    }
}

#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
/// Query parameters for the admin file blob list.
pub struct AdminFileBlobListQuery {
    /// Restrict to blobs referenced by files created by this user.
    pub uploader_user_id: Option<i64>,
    /// Restrict to blobs in one health state.
    pub health: Option<AdminFileBlobHealth>,
    /// Restrict to blobs with this hash classification.
    pub hash_kind: Option<AdminFileBlobHashKind>,
    /// Case-insensitive partial match against the blob hash.
    pub hash: Option<String>,
    /// Restrict to blobs stored under this policy.
    pub policy_id: Option<i64>,
    /// Case-insensitive partial match against the storage path.
    pub storage_path: Option<String>,
    /// Inclusive lower bound on the recorded reference count.
    pub ref_count_min: Option<i32>,
    /// Inclusive upper bound on the recorded reference count.
    pub ref_count_max: Option<i32>,
    /// Inclusive lower bound on blob size in bytes.
    pub size_min: Option<i64>,
    /// Inclusive upper bound on blob size in bytes.
    pub size_max: Option<i64>,
    /// Sort key; defaults to `created_at` when omitted.
    pub sort_by: Option<AdminFileBlobSortBy>,
    /// Sort direction; defaults to descending when omitted.
    pub sort_order: Option<SortOrder>,
}

impl AdminFileBlobListQuery {
    /// Effective sort key, falling back to `created_at`.
    pub fn sort_by(&self) -> AdminFileBlobSortBy {
        self.sort_by.unwrap_or(AdminFileBlobSortBy::CreatedAt)
    }

    /// Effective sort direction, falling back to descending.
    pub fn sort_order(&self) -> SortOrder {
        self.sort_order.unwrap_or(SortOrder::Desc)
    }
}

/// Compact blob snapshot embedded in admin file and version responses.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminFileBlobSummary {
    /// Blob row ID.
    pub id: i64,
    /// Content hash; SHA-256 hex for content-addressed blobs, otherwise an
    /// opaque identifier.
    pub hash: String,
    /// Stored byte count of the blob content.
    pub size: i64,
    /// Storage policy the blob is stored under.
    pub policy_id: i64,
    /// Backend-relative storage path; virtual empty blobs have none.
    pub storage_path: Option<String>,
}

/// Admin view of a file joined with its current blob and lock state.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminFileInfo {
    /// File row ID.
    pub id: i64,
    /// File name including its extension.
    pub name: String,
    /// Parent folder ID; `None` for team root-level files.
    pub folder_id: Option<i64>,
    /// Owning team ID; `None` for personal files.
    pub team_id: Option<i64>,
    /// Blob ID of the current content revision.
    pub blob_id: i64,
    /// Logical file size in bytes.
    pub size: i64,
    /// Current owner's user ID.
    pub owner_user_id: Option<i64>,
    /// Creator's user ID.
    pub created_by_user_id: Option<i64>,
    /// Creator username snapshot taken when the file row was written.
    pub created_by_username: String,
    /// Creator summary resolved live from the current user table.
    pub created_by: Option<crate::services::user::account::UserSummary>,
    /// MIME type recorded at upload time.
    pub mime_type: String,
    /// Lowercase simple extension without the leading dot.
    pub extension: String,
    /// Compound extension (e.g. `tar.gz`) without the leading dot.
    pub compound_extension: Option<String>,
    /// Content category derived from the file type.
    pub file_category: aster_forge_file_classification::FileCategory,
    /// Creation timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTime<Utc>,
    /// Last metadata update timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTime<Utc>,
    /// Trash timestamp; `None` while the file is active.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub deleted_at: Option<DateTime<Utc>>,
    /// Locks currently held on this file.
    pub lock_state: crate::services::files::lock::ResourceLockState,
    /// Snapshot of the blob backing the current content.
    pub blob: AdminFileBlobSummary,
}

/// Admin view of a single file content revision.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminFileVersionSummary {
    /// Revision row ID.
    pub id: i64,
    /// Owning file row ID.
    pub file_id: i64,
    /// Blob ID backing this revision.
    pub blob_id: i64,
    /// Monotonically increasing revision sequence number.
    pub version: i64,
    /// Logical revision size in bytes.
    pub size: i64,
    /// Revision creation timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTime<Utc>,
    /// Snapshot of the blob backing this revision.
    pub blob: AdminFileBlobSummary,
}

/// Admin view of one file with its full revision history.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminFileDetail {
    /// Flattened core file fields shared with the list endpoint.
    #[serde(flatten)]
    pub file: AdminFileInfo,
    /// All content revisions of the file, newest first.
    pub versions: Vec<AdminFileVersionSummary>,
}

/// Classifies how a blob's stored hash should be interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AdminFileBlobHashKind {
    /// 64-character hexadecimal SHA-256 content digest.
    ContentSha256,
    /// Any other connector-defined identifier that is not a SHA-256 digest.
    Opaque,
}

/// Consistency verdict derived by comparing the recorded reference count with
/// the live file and non-current version reference counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AdminFileBlobHealth {
    /// Recorded reference count matches the live reference count and is nonzero.
    Healthy,
    /// The blob has no live references.
    Orphan,
    /// Recorded reference count differs from the live reference count.
    RefCountMismatch,
    /// The blob is claimed by a pending cleanup run (recorded ref count sentinel).
    CleanupClaimed,
}

/// Admin view of a blob enriched with live reference counts and health.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminFileBlobInfo {
    /// Blob row ID.
    pub id: i64,
    /// Content hash; SHA-256 hex for content-addressed blobs, otherwise an
    /// opaque identifier.
    pub hash: String,
    /// Stored byte count of the blob content.
    pub size: i64,
    /// Storage policy the blob is stored under.
    pub policy_id: i64,
    /// Backend-relative storage path; virtual empty blobs have none.
    pub storage_path: Option<String>,
    /// Backend-relative path of the generated thumbnail.
    pub thumbnail_path: Option<String>,
    /// Processor that produced the thumbnail.
    pub thumbnail_processor: Option<String>,
    /// Processor version used to produce the thumbnail.
    pub thumbnail_version: Option<String>,
    /// Reference count recorded on the blob row; the cleanup sentinel value
    /// marks blobs claimed for deletion.
    pub ref_count: i32,
    /// Blob creation timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTime<Utc>,
    /// Blob last update timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTime<Utc>,
    /// How the stored hash should be interpreted.
    pub hash_kind: AdminFileBlobHashKind,
    /// Number of live file rows referencing this blob.
    pub file_ref_count: i64,
    /// Number of non-current revisions referencing this blob.
    pub version_ref_count: i64,
    /// `file_ref_count + version_ref_count`, used to verify `ref_count`.
    pub actual_ref_count: i64,
    /// Consistency verdict computed from `ref_count` vs `actual_ref_count`.
    pub health: AdminFileBlobHealth,
    /// Distinct users who created files referencing this blob.
    pub uploader_count: i64,
    /// Summaries of those users.
    pub uploaders: Vec<crate::services::user::account::UserSummary>,
}

/// File row currently referencing a blob, shown in the blob detail view.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminFileBlobReferenceFile {
    /// Referencing file row ID.
    pub id: i64,
    /// Referencing file name.
    pub name: String,
    /// Parent folder of the referencing file.
    pub folder_id: Option<i64>,
    /// Owning team of the referencing file.
    pub team_id: Option<i64>,
    /// Current owner's user ID.
    pub owner_user_id: Option<i64>,
    /// Creator's user ID.
    pub created_by_user_id: Option<i64>,
    /// Creator username snapshot taken when the file row was written.
    pub created_by_username: String,
    /// Creator summary resolved live from the current user table.
    pub created_by: Option<crate::services::user::account::UserSummary>,
    /// Logical file size in bytes.
    pub size: i64,
    /// MIME type recorded at upload time.
    pub mime_type: String,
    /// File creation timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTime<Utc>,
    /// File last update timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTime<Utc>,
    /// Trash timestamp; `None` while the file is active.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub deleted_at: Option<DateTime<Utc>>,
}

/// Revision referencing a blob, shown in the blob detail view.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminFileBlobReferenceVersion {
    /// Referencing revision row ID.
    pub id: i64,
    /// Owning file row ID of the revision.
    pub file_id: i64,
    /// Monotonically increasing revision sequence number.
    pub version: i64,
    /// Logical revision size in bytes.
    pub size: i64,
    /// Revision creation timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTime<Utc>,
    /// Name of the owning file, if the file row still exists.
    pub file_name: Option<String>,
    /// Owner of the referencing file, if the file row still exists.
    pub owner_user_id: Option<i64>,
    /// Team of the referencing file, if the file row still exists.
    pub team_id: Option<i64>,
    /// Trash timestamp of the referencing file; `None` while it is active.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub deleted_at: Option<DateTime<Utc>>,
    /// Whether this revision is the file's current content.
    pub is_current: bool,
}

/// Admin view of one blob with every file and revision referencing it.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminFileBlobDetail {
    /// Flattened blob fields shared with the list endpoint.
    #[serde(flatten)]
    pub blob: AdminFileBlobInfo,
    /// File rows currently referencing this blob.
    pub files: Vec<AdminFileBlobReferenceFile>,
    /// Revisions referencing this blob.
    pub file_versions: Vec<AdminFileBlobReferenceVersion>,
}

/// Request creating a blob maintenance background task.
#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreateBlobMaintenanceTaskReq {
    /// Maintenance operation to run over the selected blobs.
    pub action: crate::services::task::types::BlobMaintenanceAction,
    /// Target blob IDs; `None` targets the entire blob library.
    #[validate(length(min = 1, max = 1000, message = "blob_ids must contain 1 to 1000 items"))]
    pub blob_ids: Option<Vec<i64>>,
}

/// Create a team (admin operation).
#[derive(Debug, Deserialize, Validate)]
#[validate(schema(function = "validate_admin_team_target"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminCreateTeamReq {
    /// Team name; must satisfy the team name rules.
    #[validate(custom(function = "crate::api::dto::validation::validate_team_name"))]
    pub name: String,
    /// Optional team description.
    pub description: Option<String>,
    /// User appointed as team admin; mutually exclusive with `admin_identifier`.
    #[validate(range(min = 1, message = "admin_user_id must be greater than 0"))]
    pub admin_user_id: Option<i64>,
    /// Username or email of the appointed team admin; mutually exclusive with
    /// `admin_user_id`. Exactly one of the two is required.
    pub admin_identifier: Option<String>,
    /// Team storage quota in bytes (`0` = unlimited); omit for the system default.
    #[validate(range(min = 0, message = "storage_quota must be non-negative"))]
    pub storage_quota: Option<i64>,
    /// Policy group the team is assigned to; omit for the default group.
    #[validate(range(min = 1, message = "policy_group_id must be greater than 0"))]
    pub policy_group_id: Option<i64>,
}

/// Patch a team (admin operation).
#[derive(Debug, Deserialize, Validate)]
#[validate(schema(function = "validate_admin_patch_team"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminPatchTeamReq {
    /// New team name; must satisfy the team name rules when present.
    pub name: Option<String>,
    /// New description; omit to leave unchanged.
    pub description: Option<String>,
    /// New storage quota in bytes (`0` = unlimited); omit to leave unchanged.
    #[validate(range(min = 0, message = "storage_quota must be non-negative"))]
    pub storage_quota: Option<i64>,
    /// New policy group assignment; omit to leave unchanged. Explicit `null`
    /// is rejected because this endpoint only supports assigning a policy
    /// group, not unassigning one.
    #[serde(
        default,
        deserialize_with = "crate::api::routes::admin::common::deserialize_non_null_policy_group_id"
    )]
    #[validate(range(min = 1, message = "policy_group_id must be greater than 0"))]
    pub policy_group_id: Option<i64>,
}

/// Alias for `AdminTeamListQuery` (admin listing query).
pub type AdminListQuery = AdminTeamListQuery;

fn validate_create_policy_group(
    value: &CreatePolicyGroupReq,
) -> std::result::Result<(), ValidationError> {
    if value.rules.as_ref().is_none_or(Vec::is_empty) {
        return Err(crate::api::dto::validation::message_validation_error(
            "storage policy group must contain at least one routing rule",
        ));
    }
    if value.is_default && !value.is_enabled {
        return Err(crate::api::dto::validation::message_validation_error(
            "default storage policy group must be enabled",
        ));
    }
    Ok(())
}

fn validate_patch_policy(value: &PatchPolicyReq) -> std::result::Result<(), ValidationError> {
    if let Some(name) = value.name.as_deref() {
        crate::api::dto::validation::validate_non_blank(name)?;
    }
    Ok(())
}

fn validate_patch_policy_group(
    value: &PatchPolicyGroupReq,
) -> std::result::Result<(), ValidationError> {
    if let Some(name) = value.name.as_deref() {
        crate::api::dto::validation::validate_non_blank(name)?;
    }
    if value.rules.is_some() && value.rules.as_ref().is_none_or(Vec::is_empty) {
        return Err(crate::api::dto::validation::message_validation_error(
            "storage policy group must contain at least one routing rule",
        ));
    }
    if value.is_default == Some(true) && value.is_enabled == Some(false) {
        return Err(crate::api::dto::validation::message_validation_error(
            "default storage policy group must be enabled",
        ));
    }
    Ok(())
}

fn validate_admin_team_target(
    value: &AdminCreateTeamReq,
) -> std::result::Result<(), ValidationError> {
    let admin_identifier = value
        .admin_identifier
        .as_deref()
        .map(str::trim)
        .filter(|identifier| !identifier.is_empty());
    match (value.admin_user_id, admin_identifier) {
        (Some(_), Some(_)) => Err(crate::api::dto::validation::message_validation_error(
            "specify either user_id or identifier, not both",
        )),
        (None, None) => Err(crate::api::dto::validation::message_validation_error(
            "user_id or identifier is required",
        )),
        _ => Ok(()),
    }
}

fn validate_admin_patch_team(
    value: &AdminPatchTeamReq,
) -> std::result::Result<(), ValidationError> {
    if let Some(name) = value.name.as_deref() {
        crate::api::dto::validation::validate_team_name(name)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_policy_uses_connector_owned_authorization_application_credential() {
        let request = serde_json::from_value::<CreatePolicyReq>(serde_json::json!({
            "name": "OneDrive",
            "connection": {
                "connector_config": {
                    "format_version": 1,
                    "connector_id": "asterdrive.storage.onedrive",
                    "schema_version": 1,
                    "values": {
                        "base_path": "",
                        "provider_resumable_upload_strategy": "server_relay",
                        "provider_download_strategy": "server_relay",
                        "provider_download_filename_mode": "provider_native",
                        "cloud": "global",
                        "account_mode": "personal"
                    }
                },
                "behavior": {},
                "credential": {
                    "mode": "authorization_application",
                    "values": {
                    "client_id": "client-id",
                    "client_secret": "client-secret"
                    }
                }
            }
        }))
        .expect("connector-owned application credential should deserialize");

        assert_eq!(
            request
                .connection
                .storage
                .connector_config
                .connector_id
                .as_str(),
            "asterdrive.storage.onedrive"
        );
        assert!(matches!(
            request.connection.storage.credential,
            crate::storage::StorageConnectorCredentialInput::AuthorizationApplication(values)
                if values["client_id"] == "client-id"
                    && values["client_secret"] == "client-secret"
        ));
    }

    #[test]
    fn create_policy_rejects_legacy_flat_driver_and_application_fields() {
        let error = serde_json::from_value::<CreatePolicyReq>(serde_json::json!({
            "name": "OneDrive",
            "driver_type": "one_drive",
            "microsoft_graph": {
                "client_id": "client-id"
            }
        }))
        .err()
        .expect("stale flat policy input must not deserialize");

        assert!(error.to_string().contains("unknown field"));
    }
}
