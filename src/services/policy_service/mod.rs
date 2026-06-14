//! 存储策略服务聚合入口。

mod groups;
mod models;
mod policies;
mod shared;

use crate::errors::Result;
use crate::runtime::{RemoteProtocolRuntimeState, SharedRuntimeState, TaskRuntimeState};
use crate::services::audit_service::{self, AuditContext};
use crate::types::DriverType;

pub use groups::{
    create_group, delete_group, ensure_policy_groups_seeded, get_group, list_groups_paginated,
    migrate_group_assignments, update_group,
};
pub use models::{
    ConfigureTencentCosCorsInput, CreateStoragePolicyGroupInput, CreateStoragePolicyInput,
    ExecuteDraftStoragePolicyActionInput, ExecuteSavedStoragePolicyActionInput,
    PolicyGroupAssignmentMigrationResult, PromoteS3CompatiblePolicyDriverInput, StoragePolicy,
    StoragePolicyActionResult, StoragePolicyActionType, StoragePolicyCapacityInfo,
    StoragePolicyConnectionInput, StoragePolicyGroupInfo, StoragePolicyGroupItemInfo,
    StoragePolicyGroupItemInput, StoragePolicySummaryInfo, TencentCosCorsConfigResult,
    UpdateStoragePolicyGroupInput, UpdateStoragePolicyInput,
};
pub(crate) use policies::capacity_info_or_status;
pub use policies::{
    capacity_info, configure_tencent_cos_cors, configure_tencent_cos_cors_for_policy, create,
    delete, execute_draft_action, execute_saved_action, get, list_paginated,
    promote_s3_compatible_driver, test_connection, test_connection_params, test_default_connection,
    update,
};

fn driver_type_audit_name(driver_type: DriverType) -> &'static str {
    match driver_type {
        DriverType::Local => "local",
        DriverType::S3 => "s3",
        DriverType::AzureBlob => "azure_blob",
        DriverType::TencentCos => "tencent_cos",
        DriverType::Remote => "remote",
    }
}

fn policy_audit_details(policy: &StoragePolicy) -> Option<serde_json::Value> {
    audit_service::details(audit_service::StoragePolicyAuditDetails {
        driver_type: driver_type_audit_name(policy.driver_type),
        remote_node_id: policy.remote_node_id,
        max_file_size: policy.max_file_size,
        chunk_size: policy.chunk_size,
        is_default: policy.is_default,
    })
}

fn policy_action_audit_details(
    action: StoragePolicyActionType,
    driver_type: DriverType,
    used_draft_values: bool,
) -> Option<serde_json::Value> {
    audit_service::details(audit_service::StoragePolicyActionAuditDetails {
        action: action.as_str(),
        driver_type: driver_type_audit_name(driver_type),
        used_draft_values,
        mutates_remote_state: action.mutates_remote_state(),
    })
}

pub async fn create_with_audit(
    state: &impl SharedRuntimeState,
    input: CreateStoragePolicyInput,
    audit_ctx: &AuditContext,
) -> Result<StoragePolicy> {
    let policy = create(state, input).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminCreatePolicy,
        crate::services::audit_service::AuditEntityType::StoragePolicy,
        Some(policy.id),
        Some(&policy.name),
        || policy_audit_details(&policy),
    )
    .await;
    Ok(policy)
}

pub async fn update_with_audit(
    state: &impl SharedRuntimeState,
    id: i64,
    input: UpdateStoragePolicyInput,
    audit_ctx: &AuditContext,
) -> Result<StoragePolicy> {
    let policy = update(state, id, input).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminUpdatePolicy,
        crate::services::audit_service::AuditEntityType::StoragePolicy,
        Some(policy.id),
        Some(&policy.name),
        || policy_audit_details(&policy),
    )
    .await;
    Ok(policy)
}

pub async fn promote_s3_compatible_driver_with_audit(
    state: &impl SharedRuntimeState,
    id: i64,
    input: PromoteS3CompatiblePolicyDriverInput,
    audit_ctx: &AuditContext,
) -> Result<StoragePolicy> {
    let policy = promote_s3_compatible_driver(state, id, input).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminUpdatePolicy,
        crate::services::audit_service::AuditEntityType::StoragePolicy,
        Some(policy.id),
        Some(&policy.name),
        || policy_audit_details(&policy),
    )
    .await;
    Ok(policy)
}

pub async fn delete_with_audit(
    state: &impl TaskRuntimeState,
    id: i64,
    force: bool,
    audit_ctx: &AuditContext,
) -> Result<()> {
    let policy = get(state, id).await?;
    delete(state, id, force).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminDeletePolicy,
        crate::services::audit_service::AuditEntityType::StoragePolicy,
        Some(policy.id),
        Some(&policy.name),
        || policy_audit_details(&policy),
    )
    .await;
    Ok(())
}

pub async fn execute_saved_action_with_audit(
    state: &impl SharedRuntimeState,
    id: i64,
    input: ExecuteSavedStoragePolicyActionInput,
    request_origin: Option<&str>,
    audit_ctx: &AuditContext,
) -> Result<StoragePolicyActionResult> {
    let policy = get(state, id).await?;
    let action = input.action;
    let result = execute_saved_action(state, id, input, request_origin).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminTriggerStorageAction,
        crate::services::audit_service::AuditEntityType::StoragePolicy,
        Some(policy.id),
        Some(&policy.name),
        || policy_action_audit_details(action, policy.driver_type, false),
    )
    .await;
    Ok(result)
}

pub async fn execute_draft_action_with_audit(
    state: &impl RemoteProtocolRuntimeState,
    input: ExecuteDraftStoragePolicyActionInput,
    request_origin: Option<&str>,
    audit_ctx: &AuditContext,
) -> Result<StoragePolicyActionResult> {
    let action = input.action;
    let driver_type = input.connection.driver_type;
    let result = execute_draft_action(state, input, request_origin).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminTriggerStorageAction,
        crate::services::audit_service::AuditEntityType::StoragePolicy,
        None,
        None,
        || policy_action_audit_details(action, driver_type, true),
    )
    .await;
    Ok(result)
}

pub async fn create_group_with_audit(
    state: &impl SharedRuntimeState,
    input: CreateStoragePolicyGroupInput,
    audit_ctx: &AuditContext,
) -> Result<StoragePolicyGroupInfo> {
    let group = create_group(state, input).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminCreatePolicyGroup,
        crate::services::audit_service::AuditEntityType::PolicyGroup,
        Some(group.id),
        Some(&group.name),
        || {
            audit_service::details(audit_service::PolicyGroupAuditDetails {
                is_default: group.is_default,
                is_enabled: group.is_enabled,
                item_count: group.items.len(),
            })
        },
    )
    .await;
    Ok(group)
}

pub async fn update_group_with_audit(
    state: &impl SharedRuntimeState,
    id: i64,
    input: UpdateStoragePolicyGroupInput,
    audit_ctx: &AuditContext,
) -> Result<StoragePolicyGroupInfo> {
    let group = update_group(state, id, input).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminUpdatePolicyGroup,
        crate::services::audit_service::AuditEntityType::PolicyGroup,
        Some(group.id),
        Some(&group.name),
        || {
            audit_service::details(audit_service::PolicyGroupAuditDetails {
                is_default: group.is_default,
                is_enabled: group.is_enabled,
                item_count: group.items.len(),
            })
        },
    )
    .await;
    Ok(group)
}

pub async fn delete_group_with_audit(
    state: &impl SharedRuntimeState,
    id: i64,
    audit_ctx: &AuditContext,
) -> Result<()> {
    let group = get_group(state, id).await?;
    delete_group(state, id).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminDeletePolicyGroup,
        crate::services::audit_service::AuditEntityType::PolicyGroup,
        Some(group.id),
        Some(&group.name),
        || {
            audit_service::details(audit_service::PolicyGroupAuditDetails {
                is_default: group.is_default,
                is_enabled: group.is_enabled,
                item_count: group.items.len(),
            })
        },
    )
    .await;
    Ok(())
}

pub async fn migrate_group_assignments_with_audit(
    state: &impl SharedRuntimeState,
    source_group_id: i64,
    target_group_id: i64,
    audit_ctx: &AuditContext,
) -> Result<PolicyGroupAssignmentMigrationResult> {
    let source_group = get_group(state, source_group_id).await?;
    let target_group = get_group(state, target_group_id).await?;
    let result = migrate_group_assignments(state, source_group_id, target_group_id).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminMigratePolicyGroupUsers,
        crate::services::audit_service::AuditEntityType::PolicyGroup,
        Some(source_group.id),
        Some(&source_group.name),
        || {
            audit_service::details(audit_service::PolicyGroupMigrationDetails {
                source_group_id: source_group.id,
                source_group_name: &source_group.name,
                target_group_id: target_group.id,
                target_group_name: &target_group.name,
                affected_users: result.affected_users,
                affected_teams: result.affected_teams,
                migrated_assignments: result.migrated_assignments,
            })
        },
    )
    .await;
    Ok(result)
}
