//! 存储策略服务聚合入口。

mod groups;
mod models;
mod policies;
mod shared;

use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::audit_service::{self, AuditContext};
use crate::types::DriverType;

pub use groups::{
    create_group, delete_group, ensure_policy_groups_seeded, get_group, list_groups_paginated,
    migrate_group_users, update_group,
};
pub use models::{
    CreateStoragePolicyGroupInput, CreateStoragePolicyInput, PolicyGroupUserMigrationResult,
    StoragePolicy, StoragePolicyCapacityInfo, StoragePolicyConnectionInput, StoragePolicyGroupInfo,
    StoragePolicyGroupItemInfo, StoragePolicyGroupItemInput, StoragePolicySummaryInfo,
    UpdateStoragePolicyGroupInput, UpdateStoragePolicyInput,
};
pub(crate) use policies::capacity_info_or_status;
pub use policies::{
    capacity_info, create, delete, get, list_paginated, test_connection, test_connection_params,
    test_default_connection, update,
};

fn driver_type_audit_name(driver_type: DriverType) -> &'static str {
    match driver_type {
        DriverType::Local => "local",
        DriverType::S3 => "s3",
        DriverType::TencentCos => "tencent_cos",
        DriverType::Remote => "remote",
        DriverType::GoogleDrive => "google_drive",
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

pub async fn create_with_audit(
    state: &PrimaryAppState,
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
    state: &PrimaryAppState,
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

pub async fn delete_with_audit(
    state: &PrimaryAppState,
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

pub async fn create_group_with_audit(
    state: &PrimaryAppState,
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
    state: &PrimaryAppState,
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
    state: &PrimaryAppState,
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

pub async fn migrate_group_users_with_audit(
    state: &PrimaryAppState,
    source_group_id: i64,
    target_group_id: i64,
    audit_ctx: &AuditContext,
) -> Result<PolicyGroupUserMigrationResult> {
    let source_group = get_group(state, source_group_id).await?;
    let target_group = get_group(state, target_group_id).await?;
    let result = migrate_group_users(state, source_group_id, target_group_id).await?;
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
                migrated_assignments: result.migrated_assignments,
            })
        },
    )
    .await;
    Ok(result)
}
