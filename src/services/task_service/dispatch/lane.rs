use crate::config::operations;
use crate::runtime::PrimaryAppState;
use crate::types::BackgroundTaskKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TaskLane {
    Archive,
    Thumbnail,
    Fallback,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct TaskLaneConfig {
    pub(super) lane: TaskLane,
    pub(super) limit: usize,
    pub(super) fast_continue: bool,
}

const ARCHIVE_TASK_KINDS: [BackgroundTaskKind; 3] = [
    BackgroundTaskKind::ArchiveCompress,
    BackgroundTaskKind::ArchiveExtract,
    BackgroundTaskKind::ArchivePreviewGenerate,
];
const THUMBNAIL_TASK_KINDS: [BackgroundTaskKind; 2] = [
    BackgroundTaskKind::ThumbnailGenerate,
    BackgroundTaskKind::MediaMetadataExtract,
];
const FALLBACK_TASK_KINDS: [BackgroundTaskKind; 3] = [
    BackgroundTaskKind::SystemRuntime,
    BackgroundTaskKind::StoragePolicyTempCleanup,
    BackgroundTaskKind::TrashPurgeAll,
];
pub(super) const TASK_LANES: [TaskLane; 3] =
    [TaskLane::Archive, TaskLane::Thumbnail, TaskLane::Fallback];
pub(super) fn task_lane_configs(state: &PrimaryAppState) -> Vec<TaskLaneConfig> {
    TASK_LANES
        .into_iter()
        .map(|lane| TaskLaneConfig {
            lane,
            limit: match lane {
                TaskLane::Archive => {
                    operations::background_task_archive_max_concurrency(&state.runtime_config)
                }
                TaskLane::Thumbnail => {
                    operations::background_task_thumbnail_max_concurrency(&state.runtime_config)
                }
                TaskLane::Fallback => {
                    operations::background_task_max_concurrency(&state.runtime_config)
                }
            },
            fast_continue: matches!(lane, TaskLane::Archive | TaskLane::Thumbnail),
        })
        .collect()
}

impl TaskLaneConfig {
    pub(super) fn kinds(self) -> &'static [BackgroundTaskKind] {
        task_lane_kinds(self.lane)
    }

    pub(super) fn lock_key(self) -> &'static str {
        match self.lane {
            TaskLane::Archive => operations::BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY_KEY,
            TaskLane::Thumbnail => operations::BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY_KEY,
            TaskLane::Fallback => operations::BACKGROUND_TASK_MAX_CONCURRENCY_KEY,
        }
    }
}

pub(super) fn task_lane(kind: BackgroundTaskKind) -> TaskLane {
    match kind {
        BackgroundTaskKind::ArchiveCompress
        | BackgroundTaskKind::ArchiveExtract
        | BackgroundTaskKind::ArchivePreviewGenerate => TaskLane::Archive,
        BackgroundTaskKind::ThumbnailGenerate | BackgroundTaskKind::MediaMetadataExtract => {
            TaskLane::Thumbnail
        }
        BackgroundTaskKind::StoragePolicyTempCleanup
        | BackgroundTaskKind::TrashPurgeAll
        | BackgroundTaskKind::SystemRuntime => TaskLane::Fallback,
    }
}

pub(super) fn task_lane_kinds(lane: TaskLane) -> &'static [BackgroundTaskKind] {
    match lane {
        TaskLane::Archive => &ARCHIVE_TASK_KINDS,
        TaskLane::Thumbnail => &THUMBNAIL_TASK_KINDS,
        TaskLane::Fallback => &FALLBACK_TASK_KINDS,
    }
}
