use crate::config::operations;
use crate::runtime::SharedRuntimeState;
use crate::types::BackgroundTaskKind;

use super::super::registry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::services::task) enum TaskLane {
    Archive,
    Thumbnail,
    OfflineDownload,
    StorageMigration,
    Fallback,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct TaskLaneConfig {
    pub(super) lane: TaskLane,
    pub(super) limit: usize,
    pub(super) fast_continue: bool,
}

pub(super) const TASK_LANES: [TaskLane; 5] = [
    TaskLane::Archive,
    TaskLane::Thumbnail,
    TaskLane::OfflineDownload,
    TaskLane::StorageMigration,
    TaskLane::Fallback,
];
pub(super) fn task_lane_configs(state: &impl SharedRuntimeState) -> Vec<TaskLaneConfig> {
    TASK_LANES
        .into_iter()
        .map(|lane| TaskLaneConfig {
            lane,
            limit: match lane {
                TaskLane::Archive => {
                    operations::background_task_archive_max_concurrency(state.runtime_config())
                }
                TaskLane::Thumbnail => {
                    operations::background_task_thumbnail_max_concurrency(state.runtime_config())
                }
                TaskLane::OfflineDownload => {
                    operations::offline_download_max_concurrency(state.runtime_config())
                }
                TaskLane::StorageMigration => {
                    operations::background_task_storage_migration_max_concurrency(
                        state.runtime_config(),
                    )
                }
                TaskLane::Fallback => {
                    operations::background_task_max_concurrency(state.runtime_config())
                }
            },
            fast_continue: matches!(lane, TaskLane::Archive | TaskLane::Thumbnail),
        })
        .collect()
}

impl TaskLaneConfig {
    pub(super) fn kinds(self) -> &'static [BackgroundTaskKind] {
        registry::task_lane_kinds(self.lane)
    }

    pub(super) fn lock_key(self) -> &'static str {
        match self.lane {
            TaskLane::Archive => operations::BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY_KEY,
            TaskLane::Thumbnail => operations::BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY_KEY,
            TaskLane::OfflineDownload => operations::OFFLINE_DOWNLOAD_MAX_CONCURRENCY_KEY,
            TaskLane::StorageMigration => {
                operations::BACKGROUND_TASK_STORAGE_MIGRATION_MAX_CONCURRENCY_KEY
            }
            TaskLane::Fallback => operations::BACKGROUND_TASK_MAX_CONCURRENCY_KEY,
        }
    }
}

pub(super) fn task_lane(kind: BackgroundTaskKind) -> TaskLane {
    registry::task_lane(kind)
}
