use chrono::Utc;

use crate::db::repository::background_task_repo;
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::types::BackgroundTaskStatus;

use crate::config::operations;

use super::{DispatchStats, TASK_DRAIN_MAX_ROUNDS, dispatch_due};

pub async fn drain(state: &PrimaryAppState) -> Result<DispatchStats> {
    let mut total = DispatchStats::default();

    for _ in 0..TASK_DRAIN_MAX_ROUNDS {
        let stats = dispatch_due(state).await?;
        let claimed = stats.claimed;
        total.claimed += stats.claimed;
        total.succeeded += stats.succeeded;
        total.retried += stats.retried;
        total.failed += stats.failed;
        if claimed > 0 {
            continue;
        }

        if background_task_repo::count_processing(state.writer_db()).await? == 0 {
            break;
        }

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    Ok(total)
}

pub async fn cleanup_expired(state: &impl SharedRuntimeState) -> Result<u64> {
    let mut cleaned = cleanup_expired_in_root(state, &state.config().server.temp_dir).await?;
    if let Some(temp_root) = operations::offline_download_temp_dir(state.runtime_config())
        && temp_root != state.config().server.temp_dir
    {
        cleaned += cleanup_expired_in_root(state, &temp_root).await?;
    }
    Ok(cleaned)
}

async fn cleanup_expired_in_root(state: &impl SharedRuntimeState, temp_root: &str) -> Result<u64> {
    let now = Utc::now();
    let tasks_root = crate::utils::paths::temp_file_path(temp_root, "tasks");
    let mut entries = match tokio::fs::read_dir(&tasks_root).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(AsterError::storage_driver_error(format!(
                "read task temp root {tasks_root}: {error}"
            )));
        }
    };
    let mut cleaned = 0;

    while let Some(entry) = entries.next_entry().await.map_err(|error| {
        AsterError::storage_driver_error(format!("iterate task temp root {tasks_root}: {error}"))
    })? {
        let path = entry.path();
        let path_display = path.to_string_lossy().to_string();
        let file_type = entry.file_type().await.map_err(|error| {
            AsterError::storage_driver_error(format!(
                "read task temp entry type {path_display}: {error}"
            ))
        })?;
        if !file_type.is_dir() {
            continue;
        }

        let dir_name = entry.file_name();
        let Some(dir_name) = dir_name.to_str() else {
            tracing::warn!(path = %path_display, "skipping task temp dir with non-utf8 name");
            continue;
        };
        let Ok(task_id) = dir_name.parse::<i64>() else {
            tracing::warn!(path = %path_display, "skipping task temp dir with invalid task id");
            continue;
        };

        // 这里只删“产物目录”，不删 background_task 记录：
        // - 终态且 expires_at 已到的任务：删 temp 目录，保留历史行；
        // - 数据库里已经没有任务行的孤儿目录：直接删，避免长期泄露磁盘。
        let should_cleanup =
            match background_task_repo::find_by_id(state.writer_db(), task_id).await {
                Ok(task) => {
                    task.expires_at <= now
                        && matches!(
                            task.status,
                            BackgroundTaskStatus::Succeeded
                                | BackgroundTaskStatus::Failed
                                | BackgroundTaskStatus::Canceled
                        )
                }
                Err(AsterError::RecordNotFound(_)) => {
                    tracing::warn!(
                        task_id,
                        path = %path_display,
                        "cleaning orphaned task temp dir without task record"
                    );
                    true
                }
                Err(error) => return Err(error),
            };
        if !should_cleanup {
            continue;
        }

        crate::utils::cleanup_temp_dir(&path_display).await;
        let still_exists = tokio::fs::try_exists(&path).await.map_err(|error| {
            AsterError::storage_driver_error(format!(
                "verify task temp dir cleanup {path_display}: {error}"
            ))
        })?;
        if still_exists {
            tracing::warn!(
                task_id,
                path = %path_display,
                "task temp dir still exists after cleanup attempt"
            );
            continue;
        }

        cleaned += 1;
    }

    Ok(cleaned)
}
