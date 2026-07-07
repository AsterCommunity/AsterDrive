//! `batch` API DTO 定义。

use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;
use validator::{Validate, ValidationError};

/// Batch delete files and folders.
#[derive(Deserialize, Validate)]
#[validate(schema(function = "validate_batch_delete_req"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct BatchDeleteReq {
    #[serde(default)]
    pub file_ids: Vec<i64>,
    #[serde(default)]
    pub folder_ids: Vec<i64>,
}

/// Batch move files and folders to a target folder.
#[derive(Deserialize, Validate)]
#[validate(schema(function = "validate_batch_move_req"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct BatchMoveReq {
    #[serde(default)]
    pub file_ids: Vec<i64>,
    #[serde(default)]
    pub folder_ids: Vec<i64>,
    /// Target folder ID (`None` = root directory).
    #[validate(range(min = 1, message = "target_folder_id must be greater than 0"))]
    pub target_folder_id: Option<i64>,
}

/// Batch copy files and folders to a target folder.
#[derive(Deserialize, Validate)]
#[validate(schema(function = "validate_batch_copy_req"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct BatchCopyReq {
    #[serde(default)]
    pub file_ids: Vec<i64>,
    #[serde(default)]
    pub folder_ids: Vec<i64>,
    /// Target folder ID (`None` = root directory).
    #[validate(range(min = 1, message = "target_folder_id must be greater than 0"))]
    pub target_folder_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum WorkspaceRef {
    Personal,
    Team { team_id: i64 },
}

/// Copy files and folders between workspaces.
#[derive(Deserialize, Validate)]
#[validate(schema(function = "validate_workspace_transfer_copy_req"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct WorkspaceTransferCopyReq {
    pub source_workspace: WorkspaceRef,
    #[serde(default)]
    pub file_ids: Vec<i64>,
    #[serde(default)]
    pub folder_ids: Vec<i64>,
    pub destination_workspace: WorkspaceRef,
    /// Destination folder ID (`None` = destination root directory).
    #[validate(range(min = 1, message = "target_folder_id must be greater than 0"))]
    pub target_folder_id: Option<i64>,
}

/// Request an archive download ticket for the selected files and folders.
#[derive(Debug, Deserialize, Validate)]
#[validate(schema(function = "validate_archive_download_req"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ArchiveDownloadReq {
    #[serde(default)]
    pub file_ids: Vec<i64>,
    #[serde(default)]
    pub folder_ids: Vec<i64>,
    pub archive_name: Option<String>,
}

/// Request an archive compression task for the selected files and folders.
#[derive(Debug, Deserialize, Validate)]
#[validate(schema(function = "validate_archive_compress_req"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ArchiveCompressReq {
    #[serde(default)]
    pub file_ids: Vec<i64>,
    #[serde(default)]
    pub folder_ids: Vec<i64>,
    pub archive_name: Option<String>,
    #[validate(range(min = 1, message = "target_folder_id must be greater than 0"))]
    pub target_folder_id: Option<i64>,
}

fn validate_batch_delete_req(value: &BatchDeleteReq) -> std::result::Result<(), ValidationError> {
    validate_batch_selection(&value.file_ids, &value.folder_ids)
}

fn validate_batch_move_req(value: &BatchMoveReq) -> std::result::Result<(), ValidationError> {
    validate_batch_selection(&value.file_ids, &value.folder_ids)
}

fn validate_batch_copy_req(value: &BatchCopyReq) -> std::result::Result<(), ValidationError> {
    validate_batch_selection(&value.file_ids, &value.folder_ids)
}

fn validate_workspace_transfer_copy_req(
    value: &WorkspaceTransferCopyReq,
) -> std::result::Result<(), ValidationError> {
    validate_workspace_ref(&value.source_workspace)?;
    validate_workspace_ref(&value.destination_workspace)?;
    validate_batch_selection(&value.file_ids, &value.folder_ids)
}

fn validate_archive_download_req(
    value: &ArchiveDownloadReq,
) -> std::result::Result<(), ValidationError> {
    validate_batch_selection(&value.file_ids, &value.folder_ids)?;
    validate_archive_name(value.archive_name.as_deref())
}

fn validate_archive_compress_req(
    value: &ArchiveCompressReq,
) -> std::result::Result<(), ValidationError> {
    validate_batch_selection(&value.file_ids, &value.folder_ids)?;
    validate_archive_name(value.archive_name.as_deref())
}

fn validate_batch_selection(
    file_ids: &[i64],
    folder_ids: &[i64],
) -> std::result::Result<(), ValidationError> {
    validate_positive_ids(file_ids, "file_ids")?;
    validate_positive_ids(folder_ids, "folder_ids")?;
    crate::services::files::batch::validate_batch_ids(file_ids, folder_ids)
        .map_err(|error| crate::api::dto::validation::message_validation_error(error.message()))
}

fn validate_positive_ids(
    ids: &[i64],
    field_name: &str,
) -> std::result::Result<(), ValidationError> {
    if ids.iter().any(|id| *id <= 0) {
        return Err(crate::api::dto::validation::message_validation_error(
            format!("{field_name} must contain only positive IDs"),
        ));
    }
    Ok(())
}

fn validate_workspace_ref(value: &WorkspaceRef) -> std::result::Result<(), ValidationError> {
    match value {
        WorkspaceRef::Personal => Ok(()),
        WorkspaceRef::Team { team_id } if *team_id > 0 => Ok(()),
        WorkspaceRef::Team { .. } => Err(crate::api::dto::validation::message_validation_error(
            "team_id must be greater than 0",
        )),
    }
}

fn validate_archive_name(value: Option<&str>) -> std::result::Result<(), ValidationError> {
    if let Some(name) = value.map(str::trim).filter(|name| !name.is_empty()) {
        crate::api::dto::validation::validate_name(name)?;
    }
    Ok(())
}
