//! Admin-only DTOs consolidated from `src/api/routes/admin/`.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::HashSet;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::{IntoParams, ToSchema};
use validator::{Validate, ValidationError};

use crate::api::pagination::{
    AdminAuditLogSortBy, AdminLockSortBy, AdminPolicyGroupSortBy, AdminPolicySortBy,
    AdminRemoteNodeSortBy, AdminShareSortBy, AdminTaskSortBy, AdminTeamSortBy, AdminUserSortBy,
    SortOrder,
};

// ── Users ──────────────────────────────────────────────────────────────────

/// Query parameters for the admin user list.
#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct AdminUserListQuery {
    pub keyword: Option<String>,
    pub role: Option<crate::types::UserRole>,
    pub status: Option<crate::types::UserStatus>,
    pub sort_by: Option<AdminUserSortBy>,
    pub sort_order: Option<SortOrder>,
}

impl AdminUserListQuery {
    pub fn sort_by(&self) -> AdminUserSortBy {
        self.sort_by.unwrap_or(AdminUserSortBy::CreatedAt)
    }

    pub fn sort_order(&self) -> SortOrder {
        self.sort_order.unwrap_or(SortOrder::Desc)
    }
}

/// Create a new user (admin operation).
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreateUserReq {
    #[validate(custom(function = "crate::api::dto::validation::validate_auth_username"))]
    pub username: String,
    #[validate(custom(function = "crate::api::dto::validation::validate_auth_email"))]
    pub email: String,
    #[validate(custom(function = "crate::api::dto::validation::validate_auth_password"))]
    pub password: String,
}

/// Patch an existing user (admin operation).
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PatchUserReq {
    pub email_verified: Option<bool>,
    pub role: Option<crate::types::UserRole>,
    pub status: Option<crate::types::UserStatus>,
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

/// Reset a user's password (admin operation).
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ResetUserPasswordReq {
    #[validate(custom(function = "crate::api::dto::validation::validate_auth_password"))]
    pub password: String,
}

// ── Policies ────────────────────────────────────────────────────────────────

/// Create a storage policy.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreatePolicyReq {
    #[validate(custom(function = "crate::api::dto::validation::validate_non_blank"))]
    pub name: String,
    pub driver_type: crate::types::DriverType,
    pub endpoint: Option<String>,
    pub bucket: Option<String>,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
    pub base_path: Option<String>,
    #[validate(range(min = 1, message = "remote_node_id must be greater than 0"))]
    pub remote_node_id: Option<i64>,
    #[validate(range(min = 0, message = "max_file_size must be non-negative"))]
    pub max_file_size: Option<i64>,
    #[validate(range(min = 1, message = "chunk_size must be greater than 0"))]
    pub chunk_size: Option<i64>,
    pub is_default: Option<bool>,
    pub allowed_types: Option<Vec<String>>,
    #[validate(nested)]
    pub options: Option<crate::types::StoragePolicyOptions>,
}

/// Patch a storage policy.
#[derive(Deserialize, Validate)]
#[validate(schema(function = "validate_patch_policy"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PatchPolicyReq {
    pub name: Option<String>,
    pub endpoint: Option<String>,
    pub bucket: Option<String>,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
    pub base_path: Option<String>,
    #[validate(range(min = 1, message = "remote_node_id must be greater than 0"))]
    pub remote_node_id: Option<i64>,
    #[validate(range(min = 0, message = "max_file_size must be non-negative"))]
    pub max_file_size: Option<i64>,
    #[validate(range(min = 1, message = "chunk_size must be greater than 0"))]
    pub chunk_size: Option<i64>,
    pub is_default: Option<bool>,
    pub allowed_types: Option<Vec<String>>,
    #[validate(nested)]
    pub options: Option<crate::types::StoragePolicyOptions>,
}

/// Query parameters for deleting a storage policy.
#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct DeletePolicyQuery {
    #[serde(default)]
    pub force: bool,
}

/// Test a storage policy connection by parameters (without saving).
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TestPolicyParamsReq {
    pub driver_type: crate::types::DriverType,
    pub endpoint: Option<String>,
    pub bucket: Option<String>,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
    pub base_path: Option<String>,
    #[validate(range(min = 1, message = "remote_node_id must be greater than 0"))]
    pub remote_node_id: Option<i64>,
}

/// Create a remote node.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreateRemoteNodeReq {
    #[validate(custom(function = "crate::api::dto::validation::validate_non_blank"))]
    pub name: String,
    pub base_url: Option<String>,
    #[serde(default = "default_true")]
    pub is_enabled: bool,
}

/// Patch a remote node.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PatchRemoteNodeReq {
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub is_enabled: Option<bool>,
}

/// Test remote node connection without saving.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TestRemoteNodeParamsReq {
    #[validate(custom(function = "crate::api::dto::validation::validate_non_blank"))]
    pub base_url: String,
    #[validate(custom(function = "crate::api::dto::validation::validate_non_blank"))]
    pub access_key: String,
    #[validate(custom(function = "crate::api::dto::validation::validate_non_blank"))]
    pub secret_key: String,
}

/// A single item within a policy group.
#[derive(Clone, Deserialize, Validate)]
#[validate(schema(function = "validate_policy_group_item"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PolicyGroupItemReq {
    #[validate(range(min = 1, message = "policy_id must be greater than 0"))]
    pub policy_id: i64,
    #[validate(range(min = 1, message = "group item priority must be greater than 0"))]
    pub priority: i32,
    #[serde(default)]
    #[validate(range(min = 0, message = "file size rules must be non-negative"))]
    pub min_file_size: i64,
    #[serde(default)]
    #[validate(range(min = 0, message = "file size rules must be non-negative"))]
    pub max_file_size: i64,
}

/// Create a storage policy group.
#[derive(Clone, Deserialize, Validate)]
#[validate(schema(function = "validate_create_policy_group"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreatePolicyGroupReq {
    #[validate(custom(function = "crate::api::dto::validation::validate_non_blank"))]
    pub name: String,
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub is_enabled: bool,
    #[serde(default)]
    pub is_default: bool,
    #[validate(nested)]
    pub items: Vec<PolicyGroupItemReq>,
}

/// Patch a storage policy group.
#[derive(Clone, Deserialize, Validate)]
#[validate(schema(function = "validate_patch_policy_group"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PatchPolicyGroupReq {
    pub name: Option<String>,
    pub description: Option<String>,
    pub is_enabled: Option<bool>,
    pub is_default: Option<bool>,
    #[validate(nested)]
    pub items: Option<Vec<PolicyGroupItemReq>>,
}

/// Migrate all users from one policy group to another.
#[derive(Clone, Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct MigratePolicyGroupUsersReq {
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
    pub value: crate::services::config_service::SystemConfigValue,
}

/// Execute a config action (e.g., send test email).
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ExecuteConfigActionReq {
    pub action: crate::services::config_service::ConfigActionType,
    pub discovery_url: Option<String>,
    pub target_email: Option<String>,
    pub value: Option<String>,
}

/// Response from a config action execution.
#[derive(serde::Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ExecuteConfigActionResp {
    pub message: String,
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
pub struct AdminTaskListQuery {
    pub kind: Option<crate::types::BackgroundTaskKind>,
    pub status: Option<crate::types::BackgroundTaskStatus>,
    pub sort_by: Option<AdminTaskSortBy>,
    pub sort_order: Option<SortOrder>,
}

impl AdminTaskListQuery {
    pub fn sort_by(&self) -> AdminTaskSortBy {
        self.sort_by.unwrap_or(AdminTaskSortBy::UpdatedAt)
    }

    pub fn sort_order(&self) -> SortOrder {
        self.sort_order.unwrap_or(SortOrder::Desc)
    }
}

/// Cleanup completed background tasks by admin-specified conditions.
#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminTaskCleanupReq {
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub finished_before: DateTime<Utc>,
    pub kind: Option<crate::types::BackgroundTaskKind>,
    pub status: Option<crate::types::BackgroundTaskStatus>,
}

// ── Admin Teams ─────────────────────────────────────────────────────────────

/// Query parameters for the admin team list.
#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct AdminTeamListQuery {
    pub keyword: Option<String>,
    pub archived: Option<bool>,
    pub sort_by: Option<AdminTeamSortBy>,
    pub sort_order: Option<SortOrder>,
}

impl AdminTeamListQuery {
    pub fn sort_by(&self) -> AdminTeamSortBy {
        self.sort_by.unwrap_or(AdminTeamSortBy::CreatedAt)
    }

    pub fn sort_order(&self) -> SortOrder {
        self.sort_order.unwrap_or(SortOrder::Desc)
    }
}

#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct AdminPolicyListQuery {
    pub sort_by: Option<AdminPolicySortBy>,
    pub sort_order: Option<SortOrder>,
}

impl AdminPolicyListQuery {
    pub fn sort_by(&self) -> AdminPolicySortBy {
        self.sort_by.unwrap_or(AdminPolicySortBy::CreatedAt)
    }

    pub fn sort_order(&self) -> SortOrder {
        self.sort_order.unwrap_or(SortOrder::Desc)
    }
}

#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct AdminPolicyGroupListQuery {
    pub sort_by: Option<AdminPolicyGroupSortBy>,
    pub sort_order: Option<SortOrder>,
}

impl AdminPolicyGroupListQuery {
    pub fn sort_by(&self) -> AdminPolicyGroupSortBy {
        self.sort_by.unwrap_or(AdminPolicyGroupSortBy::CreatedAt)
    }

    pub fn sort_order(&self) -> SortOrder {
        self.sort_order.unwrap_or(SortOrder::Desc)
    }
}

#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct AdminRemoteNodeListQuery {
    pub sort_by: Option<AdminRemoteNodeSortBy>,
    pub sort_order: Option<SortOrder>,
}

impl AdminRemoteNodeListQuery {
    pub fn sort_by(&self) -> AdminRemoteNodeSortBy {
        self.sort_by.unwrap_or(AdminRemoteNodeSortBy::CreatedAt)
    }

    pub fn sort_order(&self) -> SortOrder {
        self.sort_order.unwrap_or(SortOrder::Desc)
    }
}

#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct AdminShareListQuery {
    pub sort_by: Option<AdminShareSortBy>,
    pub sort_order: Option<SortOrder>,
}

impl AdminShareListQuery {
    pub fn sort_by(&self) -> AdminShareSortBy {
        self.sort_by.unwrap_or(AdminShareSortBy::CreatedAt)
    }

    pub fn sort_order(&self) -> SortOrder {
        self.sort_order.unwrap_or(SortOrder::Desc)
    }
}

#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct AdminLockListQuery {
    pub sort_by: Option<AdminLockSortBy>,
    pub sort_order: Option<SortOrder>,
}

impl AdminLockListQuery {
    pub fn sort_by(&self) -> AdminLockSortBy {
        self.sort_by.unwrap_or(AdminLockSortBy::Id)
    }

    pub fn sort_order(&self) -> SortOrder {
        self.sort_order.unwrap_or(SortOrder::Asc)
    }
}

#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct AdminAuditLogSortQuery {
    pub sort_by: Option<AdminAuditLogSortBy>,
    pub sort_order: Option<SortOrder>,
}

impl AdminAuditLogSortQuery {
    pub fn sort_by(&self) -> AdminAuditLogSortBy {
        self.sort_by.unwrap_or(AdminAuditLogSortBy::CreatedAt)
    }

    pub fn sort_order(&self) -> SortOrder {
        self.sort_order.unwrap_or(SortOrder::Desc)
    }
}

/// Create a team (admin operation).
#[derive(Debug, Deserialize, Validate)]
#[validate(schema(function = "validate_admin_team_target"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminCreateTeamReq {
    #[validate(custom(function = "crate::api::dto::validation::validate_team_name"))]
    pub name: String,
    pub description: Option<String>,
    #[validate(range(min = 1, message = "admin_user_id must be greater than 0"))]
    pub admin_user_id: Option<i64>,
    pub admin_identifier: Option<String>,
    #[validate(range(min = 1, message = "policy_group_id must be greater than 0"))]
    pub policy_group_id: Option<i64>,
}

/// Patch a team (admin operation).
#[derive(Debug, Deserialize, Validate)]
#[validate(schema(function = "validate_admin_patch_team"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminPatchTeamReq {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::api::routes::admin::common::deserialize_non_null_policy_group_id"
    )]
    #[validate(range(min = 1, message = "policy_group_id must be greater than 0"))]
    pub policy_group_id: Option<i64>,
}

/// Alias for `AdminTeamListQuery` (admin listing query).
pub type AdminListQuery = AdminTeamListQuery;

fn validate_policy_group_item(
    value: &PolicyGroupItemReq,
) -> std::result::Result<(), ValidationError> {
    if value.max_file_size != 0 && value.max_file_size <= value.min_file_size {
        return Err(crate::api::dto::validation::message_validation_error(
            "max_file_size must be greater than min_file_size",
        ));
    }
    Ok(())
}

fn validate_create_policy_group(
    value: &CreatePolicyGroupReq,
) -> std::result::Result<(), ValidationError> {
    if value.items.is_empty() {
        return Err(crate::api::dto::validation::message_validation_error(
            "storage policy group must contain at least one policy",
        ));
    }
    validate_unique_policy_group_items(&value.items)?;
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
    if let Some(items) = &value.items {
        if items.is_empty() {
            return Err(crate::api::dto::validation::message_validation_error(
                "storage policy group must contain at least one policy",
            ));
        }
        validate_unique_policy_group_items(items)?;
    }
    if value.is_default == Some(true) && value.is_enabled == Some(false) {
        return Err(crate::api::dto::validation::message_validation_error(
            "default storage policy group must be enabled",
        ));
    }
    Ok(())
}

fn validate_unique_policy_group_items(
    items: &[PolicyGroupItemReq],
) -> std::result::Result<(), ValidationError> {
    let mut seen_policies = HashSet::new();
    let mut seen_priorities = HashSet::new();
    for item in items {
        if !seen_policies.insert(item.policy_id) {
            return Err(crate::api::dto::validation::message_validation_error(
                "duplicate policy_id in storage policy group items",
            ));
        }
        if !seen_priorities.insert(item.priority) {
            return Err(crate::api::dto::validation::message_validation_error(
                "duplicate priority in storage policy group items",
            ));
        }
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
