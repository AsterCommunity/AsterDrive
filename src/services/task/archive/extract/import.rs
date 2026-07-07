//! 归档解包任务子模块：`import`。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::entities::folder;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::services::{
    task::{
        TaskExecutionContext,
        archive::common::create_folder_exact_in_scope,
        mark_task_progress,
        steps::{TASK_STEP_IMPORT_RESULT, set_task_step_active},
        types::TaskStepInfo,
    },
    workspace::storage,
    workspace::storage::WorkspaceStorageScope,
};

#[derive(Debug, Default)]
struct StagedArchiveTree {
    directories: Vec<PathBuf>,
    files: Vec<PathBuf>,
}

#[derive(Debug, Default)]
pub(super) struct ArchiveExtractImportSummary {
    pub(super) file_ids: Vec<i64>,
    pub(super) folder_ids: Vec<i64>,
    pub(super) affected_parent_ids: Vec<Option<i64>>,
    pub(super) storage_delta: i64,
}

pub(super) async fn materialize_archive_extract_stage(
    state: &PrimaryAppState,
    context: &TaskExecutionContext,
    scope: WorkspaceStorageScope,
    stage_root: &Path,
    extracted_bytes: i64,
    root_folder: &folder::Model,
    steps: &mut [TaskStepInfo],
) -> Result<ArchiveExtractImportSummary> {
    let lease_guard = context.lease_guard();
    context.ensure_active()?;
    let tree = collect_staged_archive_tree(stage_root)?;
    let mut folder_ids = HashMap::new();
    folder_ids.insert(PathBuf::new(), root_folder.id);
    let mut summary = ArchiveExtractImportSummary {
        folder_ids: vec![root_folder.id],
        affected_parent_ids: vec![root_folder.parent_id],
        ..Default::default()
    };
    let mut imported_bytes = 0_i64;
    let total_progress = extracted_bytes
        .checked_mul(2)
        .ok_or_else(|| AsterError::internal_error("archive extract progress overflow"))?;

    for relative_dir in &tree.directories {
        context.ensure_active()?;
        let parent_relative = relative_dir.parent().unwrap_or_else(|| Path::new(""));
        let parent_id = *folder_ids.get(parent_relative).ok_or_else(|| {
            AsterError::internal_error(format!(
                "missing parent folder mapping for '{}'",
                parent_relative.display()
            ))
        })?;
        let name = relative_dir
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                AsterError::validation_error("archive directory name must be valid UTF-8")
            })?;
        let created = create_folder_exact_in_scope(state, scope, Some(parent_id), name).await?;
        summary.folder_ids.push(created.id);
        summary.affected_parent_ids.push(created.parent_id);
        folder_ids.insert(relative_dir.clone(), created.id);
    }

    for relative_file in &tree.files {
        context.ensure_active()?;
        let parent_relative = relative_file.parent().unwrap_or_else(|| Path::new(""));
        let parent_id = *folder_ids.get(parent_relative).ok_or_else(|| {
            AsterError::internal_error(format!(
                "missing parent folder mapping for '{}'",
                parent_relative.display()
            ))
        })?;
        let name = relative_file
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| AsterError::validation_error("archive file name must be valid UTF-8"))?;
        let temp_path = stage_root.join(relative_file);
        let metadata = tokio::fs::metadata(&temp_path).await.map_aster_err_ctx(
            "read extracted file metadata",
            AsterError::storage_driver_error,
        )?;
        let size = i64::try_from(metadata.len()).map_aster_err_with(|| {
            AsterError::internal_error("extracted file size exceeds i64 range")
        })?;
        let created = storage::store_from_temp_exact_name_silent_with_hints(
            state,
            storage::StoreFromTempParams::new(
                scope,
                Some(parent_id),
                name,
                &temp_path.to_string_lossy(),
                size,
            ),
            storage::StoreFromTempHints {
                operation_context: context.storage_operation_context(),
                ..Default::default()
            },
        )
        .await?;
        summary.file_ids.push(created.id);
        summary.affected_parent_ids.push(created.folder_id);
        summary.storage_delta = summary
            .storage_delta
            .checked_add(size)
            .ok_or_else(|| AsterError::internal_error("archive extract storage delta overflow"))?;
        imported_bytes = imported_bytes
            .checked_add(size)
            .ok_or_else(|| AsterError::internal_error("archive extract progress overflow"))?;
        let status_text = format!("Importing {}", relative_file.to_string_lossy());
        set_task_step_active(
            steps,
            TASK_STEP_IMPORT_RESULT,
            Some(&status_text),
            Some((imported_bytes, extracted_bytes)),
        )?;
        mark_task_progress(
            state,
            lease_guard,
            extracted_bytes
                .checked_add(imported_bytes)
                .ok_or_else(|| AsterError::internal_error("archive extract progress overflow"))?,
            total_progress,
            Some(&status_text),
            steps,
        )
        .await?;
    }

    Ok(summary)
}

fn collect_staged_archive_tree(stage_root: &Path) -> Result<StagedArchiveTree> {
    let mut tree = StagedArchiveTree::default();
    let mut stack = vec![PathBuf::new()];

    while let Some(current_relative) = stack.pop() {
        let current_dir = if current_relative.as_os_str().is_empty() {
            stage_root.to_path_buf()
        } else {
            stage_root.join(&current_relative)
        };
        let mut children = std::fs::read_dir(&current_dir)
            .map_aster_err_ctx(
                "read extracted staging directory",
                AsterError::storage_driver_error,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_aster_err_ctx(
                "read extracted staging directory entry",
                AsterError::storage_driver_error,
            )?;
        children.sort_by_key(|entry| entry.file_name());

        for child in children {
            let child_name = child.file_name();
            let child_relative = current_relative.join(&child_name);
            let file_type = child.file_type().map_aster_err_ctx(
                "read extracted staging file type",
                AsterError::storage_driver_error,
            )?;
            if file_type.is_dir() {
                tree.directories.push(child_relative.clone());
                stack.push(child_relative);
            } else if file_type.is_file() {
                tree.files.push(child_relative);
            }
        }
    }

    tree.directories.sort();
    tree.files.sort();
    Ok(tree)
}
