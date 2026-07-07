//! 存储策略服务聚合入口。

mod groups;
mod models;
mod policies;
mod shared;

use crate::errors::Result;
use crate::runtime::{RemoteProtocolRuntimeState, SharedRuntimeState, TaskRuntimeState};
use crate::services::ops::audit::{self, AuditContext};
use crate::types::DriverType;

pub use crate::storage::{
    StorageConnectorActionDescriptor, StorageConnectorActionEndpoint, StorageConnectorActionKind,
    StorageConnectorAffordanceAction, StorageConnectorCapabilities, StorageConnectorCredentialMode,
    StorageConnectorFieldDescriptor, StorageConnectorFieldKind, StorageConnectorFieldScope,
    StorageConnectorUploadWorkflows, StoragePolicyExecutableAction,
};
pub use groups::{
    create_group, delete_group, ensure_policy_groups_seeded, get_group, list_groups_paginated,
    migrate_group_assignments, update_group,
};
pub use models::{
    ConfigureTencentCosCorsInput, CreateStoragePolicyGroupInput, CreateStoragePolicyInput,
    ExecuteDraftStoragePolicyActionInput, ExecuteSavedStoragePolicyActionInput,
    PolicyGroupAssignmentMigrationResult, PromoteS3CompatiblePolicyDriverInput, StoragePolicy,
    StoragePolicyActionResult, StoragePolicyActionType, StoragePolicyCapacityInfo,
    StoragePolicyConnectionInput, StoragePolicyDiagnostic, StoragePolicyGroupInfo,
    StoragePolicyGroupItemInfo, StoragePolicyGroupItemInput, StoragePolicySummaryInfo,
    TencentCosCorsConfigResult, TestDraftStoragePolicyConnectionInput,
    UpdateStoragePolicyGroupInput, UpdateStoragePolicyInput,
};
pub(crate) use policies::capacity_info_or_status;
pub use policies::{
    capacity_info, create, delete, execute_draft_action, execute_saved_action, get, list_paginated,
    promote_s3_compatible_driver, test_connection, test_connection_params, test_default_connection,
    update,
};

fn policy_audit_details(policy: &StoragePolicy) -> Option<serde_json::Value> {
    audit::details(audit::StoragePolicyAuditDetails {
        driver_type: policy.driver_type.as_str(),
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
    diagnostic: Option<&StoragePolicyDiagnostic>,
) -> Option<serde_json::Value> {
    audit::details(audit::StoragePolicyActionAuditDetails {
        action: action.as_str(),
        driver_type: driver_type.as_str(),
        used_draft_values,
        mutates_remote_state: action.mutates_remote_state(),
        diagnostic_kind: diagnostic.map(|diagnostic| diagnostic.kind.as_str()),
        diagnostic_api_code: diagnostic.map(|diagnostic| diagnostic.api_code.as_str()),
        diagnostic_retryable: diagnostic.map(|diagnostic| diagnostic.retryable),
    })
}

pub async fn create_with_audit(
    state: &(impl RemoteProtocolRuntimeState + Sync),
    input: CreateStoragePolicyInput,
    audit_ctx: &AuditContext,
) -> Result<StoragePolicy> {
    let policy = create(state, input).await?;
    audit::log_with_details(
        state,
        audit_ctx,
        audit::AuditAction::AdminCreatePolicy,
        crate::services::ops::audit::AuditEntityType::StoragePolicy,
        Some(policy.id),
        Some(&policy.name),
        || policy_audit_details(&policy),
    )
    .await;
    Ok(policy)
}

pub async fn update_with_audit(
    state: &(impl RemoteProtocolRuntimeState + Sync),
    id: i64,
    input: UpdateStoragePolicyInput,
    audit_ctx: &AuditContext,
) -> Result<StoragePolicy> {
    let policy = update(state, id, input).await?;
    audit::log_with_details(
        state,
        audit_ctx,
        audit::AuditAction::AdminUpdatePolicy,
        crate::services::ops::audit::AuditEntityType::StoragePolicy,
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
    audit::log_with_details(
        state,
        audit_ctx,
        audit::AuditAction::AdminUpdatePolicy,
        crate::services::ops::audit::AuditEntityType::StoragePolicy,
        Some(policy.id),
        Some(&policy.name),
        || policy_audit_details(&policy),
    )
    .await;
    Ok(policy)
}

pub async fn delete_with_audit(
    state: &(impl TaskRuntimeState + Sync),
    id: i64,
    force: bool,
    audit_ctx: &AuditContext,
) -> Result<()> {
    let policy = get(state, id).await?;
    delete(state, id, force).await?;
    audit::log_with_details(
        state,
        audit_ctx,
        audit::AuditAction::AdminDeletePolicy,
        crate::services::ops::audit::AuditEntityType::StoragePolicy,
        Some(policy.id),
        Some(&policy.name),
        || policy_audit_details(&policy),
    )
    .await;
    Ok(())
}

pub async fn execute_saved_action_with_audit(
    state: &(impl SharedRuntimeState + Sync),
    id: i64,
    input: ExecuteSavedStoragePolicyActionInput,
    audit_ctx: &AuditContext,
) -> Result<StoragePolicyActionResult> {
    let policy = get(state, id).await?;
    let action = input.action;
    let result = execute_saved_action(state, id, input).await;
    let error_diagnostic;
    let diagnostic = match &result {
        Ok(result) => result.diagnostic.as_ref(),
        Err(error) => {
            error_diagnostic = StoragePolicyDiagnostic::from_error(error);
            error_diagnostic.as_ref()
        }
    };
    audit::log_with_details(
        state,
        audit_ctx,
        audit::AuditAction::AdminTriggerStorageAction,
        crate::services::ops::audit::AuditEntityType::StoragePolicy,
        Some(policy.id),
        Some(&policy.name),
        || policy_action_audit_details(action, policy.driver_type, false, diagnostic),
    )
    .await;
    result
}

pub async fn execute_draft_action_with_audit(
    state: &(impl RemoteProtocolRuntimeState + Sync),
    input: ExecuteDraftStoragePolicyActionInput,
    audit_ctx: &AuditContext,
) -> Result<StoragePolicyActionResult> {
    let action = input.action;
    let driver_type = input.connection.driver_type;
    let result = execute_draft_action(state, input).await;
    let error_diagnostic;
    let diagnostic = match &result {
        Ok(result) => result.diagnostic.as_ref(),
        Err(error) => {
            error_diagnostic = StoragePolicyDiagnostic::from_error(error);
            error_diagnostic.as_ref()
        }
    };
    audit::log_with_details(
        state,
        audit_ctx,
        audit::AuditAction::AdminTriggerStorageAction,
        crate::services::ops::audit::AuditEntityType::StoragePolicy,
        None,
        None,
        || policy_action_audit_details(action, driver_type, true, diagnostic),
    )
    .await;
    result
}

pub async fn create_group_with_audit(
    state: &impl SharedRuntimeState,
    input: CreateStoragePolicyGroupInput,
    audit_ctx: &AuditContext,
) -> Result<StoragePolicyGroupInfo> {
    let group = create_group(state, input).await?;
    audit::log_with_details(
        state,
        audit_ctx,
        audit::AuditAction::AdminCreatePolicyGroup,
        crate::services::ops::audit::AuditEntityType::PolicyGroup,
        Some(group.id),
        Some(&group.name),
        || {
            audit::details(audit::PolicyGroupAuditDetails {
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
    audit::log_with_details(
        state,
        audit_ctx,
        audit::AuditAction::AdminUpdatePolicyGroup,
        crate::services::ops::audit::AuditEntityType::PolicyGroup,
        Some(group.id),
        Some(&group.name),
        || {
            audit::details(audit::PolicyGroupAuditDetails {
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
    audit::log_with_details(
        state,
        audit_ctx,
        audit::AuditAction::AdminDeletePolicyGroup,
        crate::services::ops::audit::AuditEntityType::PolicyGroup,
        Some(group.id),
        Some(&group.name),
        || {
            audit::details(audit::PolicyGroupAuditDetails {
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
    audit::log_with_details(
        state,
        audit_ctx,
        audit::AuditAction::AdminMigratePolicyGroupUsers,
        crate::services::ops::audit::AuditEntityType::PolicyGroup,
        Some(source_group.id),
        Some(&source_group.name),
        || {
            audit::details(audit::PolicyGroupMigrationDetails {
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
