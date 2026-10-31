use crate::config::operations;
use crate::runtime::SharedRuntimeState;
use aster_drive_model::types::BackgroundTaskKind;

use super::super::registry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaskLane {
    Archive,
    Thumbnail,
    OfflineDownload,
    StorageMigration,
    Fallback,
}

pub(super) type TaskLaneConfig = aster_forge_tasks::TaskLaneConfig<BackgroundTaskKind, TaskLane>;

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
            kinds: registry::task_lane_kinds(lane),
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
            cluster_limit: operations::task_cluster_concurrency(
                state.runtime_config(),
                lane_lock_key(lane),
            ),
        })
        .collect()
}

pub(super) fn task_lane(kind: BackgroundTaskKind) -> TaskLane {
    registry::task_lane(kind)
}

pub(super) fn lane_lock_key(lane: TaskLane) -> &'static str {
    match lane {
        TaskLane::Archive => crate::config::definitions::TASK_ARCHIVE_CLUSTER_CONCURRENCY_KEY,
        TaskLane::Thumbnail => crate::config::definitions::TASK_THUMBNAIL_CLUSTER_CONCURRENCY_KEY,
        TaskLane::OfflineDownload => {
            crate::config::definitions::TASK_OFFLINE_DOWNLOAD_CLUSTER_CONCURRENCY_KEY
        }
        TaskLane::StorageMigration => {
            crate::config::definitions::TASK_STORAGE_MIGRATION_CLUSTER_CONCURRENCY_KEY
        }
        TaskLane::Fallback => crate::config::definitions::TASK_FALLBACK_CLUSTER_CONCURRENCY_KEY,
    }
}
