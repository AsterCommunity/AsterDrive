//! 批量操作服务聚合入口。

mod copy;
mod delete;
mod movement;
mod shared;

use serde::Serialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::audit_service::{self, AuditContext};
use crate::services::workspace_storage_service::WorkspaceStorageScope;

pub use copy::{batch_copy, batch_copy_team};
pub use delete::{batch_delete, batch_delete_team};
pub use movement::{batch_move, batch_move_team};
pub use shared::validate_batch_ids;

pub(crate) use shared::{
    NormalizedSelection, load_normalized_selection_in_scope, reserve_unique_name,
};

/// 单次批量操作最大条目数
pub const MAX_BATCH_ITEMS: usize = 1000;

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct BatchResult {
    pub succeeded: u32,
    pub failed: u32,
    pub errors: Vec<BatchItemError>,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct BatchItemError {
    pub entity_type: String,
    pub entity_id: i64,
    pub error: String,
}

impl BatchResult {
    pub(super) fn new() -> Self {
        Self {
            succeeded: 0,
            failed: 0,
            errors: vec![],
        }
    }

    pub(super) fn record_success(&mut self) {
        self.succeeded += 1;
    }

    pub(super) fn record_failure(&mut self, entity_type: &str, entity_id: i64, error: String) {
        self.failed += 1;
        self.errors.push(BatchItemError {
            entity_type: entity_type.to_string(),
            entity_id,
            error,
        });
    }
}

pub(crate) async fn batch_delete_in_scope_with_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_ids: &[i64],
    folder_ids: &[i64],
    audit_ctx: &AuditContext,
) -> Result<BatchResult> {
    validate_batch_ids(file_ids, folder_ids)?;
    let result = delete::batch_delete_in_scope(state, scope, file_ids, folder_ids).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::BatchDelete,
        audit_service::AuditEntityType::Batch,
        None,
        None,
        audit_service::details(audit_service::BatchDeleteDetails {
            file_ids,
            folder_ids,
            succeeded: result.succeeded,
            failed: result.failed,
        }),
    )
    .await;
    Ok(result)
}

pub(crate) async fn batch_move_in_scope_with_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_ids: &[i64],
    folder_ids: &[i64],
    target_folder_id: Option<i64>,
    audit_ctx: &AuditContext,
) -> Result<BatchResult> {
    validate_batch_ids(file_ids, folder_ids)?;
    let result =
        movement::batch_move_in_scope(state, scope, file_ids, folder_ids, target_folder_id).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::BatchMove,
        audit_service::AuditEntityType::Batch,
        None,
        None,
        audit_service::details(audit_service::BatchTransferDetails {
            file_ids,
            folder_ids,
            target_folder_id,
            succeeded: result.succeeded,
            failed: result.failed,
        }),
    )
    .await;
    Ok(result)
}

pub(crate) async fn batch_copy_in_scope_with_audit(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_ids: &[i64],
    folder_ids: &[i64],
    target_folder_id: Option<i64>,
    audit_ctx: &AuditContext,
) -> Result<BatchResult> {
    validate_batch_ids(file_ids, folder_ids)?;
    let result =
        copy::batch_copy_in_scope(state, scope, file_ids, folder_ids, target_folder_id).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::BatchCopy,
        audit_service::AuditEntityType::Batch,
        None,
        None,
        audit_service::details(audit_service::BatchTransferDetails {
            file_ids,
            folder_ids,
            target_folder_id,
            succeeded: result.succeeded,
            failed: result.failed,
        }),
    )
    .await;
    Ok(result)
}
