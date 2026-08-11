use aster_forge_tasks::TaskStepSpec;

use crate::services::task::{
    blob_maintenance,
    dispatch::TaskLane,
    folder_tree,
    steps::{
        TASK_STEP_CHECK_BLOBS, TASK_STEP_CLEANUP_OBJECTS, TASK_STEP_FINISH, TASK_STEP_FOLDER_TREE,
        TASK_STEP_PURGE_TRASH, TASK_STEP_RECONCILE_REFS, TASK_STEP_SCAN_BLOBS, TASK_STEP_WAITING,
    },
    trash,
    types::{
        BlobMaintenanceTaskPayload, BlobMaintenanceTaskResult, FolderTreeMutationTaskPayload,
        FolderTreeMutationTaskResult, TrashPurgeAllTaskPayload, TrashPurgeAllTaskResult,
    },
};

const TRASH_PURGE_STEPS: &[TaskStepSpec] = &[
    TaskStepSpec {
        key: TASK_STEP_WAITING,
        title: "Waiting",
    },
    TaskStepSpec {
        key: TASK_STEP_PURGE_TRASH,
        title: "Purge trash",
    },
];

const FOLDER_TREE_STEPS: &[TaskStepSpec] = &[
    TaskStepSpec {
        key: TASK_STEP_WAITING,
        title: "Waiting",
    },
    TaskStepSpec {
        key: TASK_STEP_FOLDER_TREE,
        title: "Mutate folder tree",
    },
];

const BLOB_MAINTENANCE_STEPS: &[TaskStepSpec] = &[
    TaskStepSpec {
        key: TASK_STEP_WAITING,
        title: "Waiting",
    },
    TaskStepSpec {
        key: TASK_STEP_SCAN_BLOBS,
        title: "Load blob records",
    },
    TaskStepSpec {
        key: TASK_STEP_CHECK_BLOBS,
        title: "Check storage objects",
    },
    TaskStepSpec {
        key: TASK_STEP_RECONCILE_REFS,
        title: "Reconcile references",
    },
    TaskStepSpec {
        key: TASK_STEP_CLEANUP_OBJECTS,
        title: "Clean orphan blobs",
    },
    TaskStepSpec {
        key: TASK_STEP_FINISH,
        title: "Finish maintenance",
    },
];

define_task_spec!(
    TrashPurgeAllTask,
    TrashPurgeAll,
    TrashPurgeAllTaskPayload,
    TrashPurgeAllTaskResult,
    TrashPurgeAll,
    TrashPurgeAll,
    steps = TRASH_PURGE_STEPS,
    lane = TaskLane::Fallback,
    process = trash::process_trash_purge_all_task
);

define_task_spec!(
    FolderTreeMutationTask,
    FolderTreeMutation,
    FolderTreeMutationTaskPayload,
    FolderTreeMutationTaskResult,
    FolderTreeMutation,
    FolderTreeMutation,
    steps = FOLDER_TREE_STEPS,
    lane = TaskLane::Fallback,
    process = folder_tree::process_folder_tree_mutation_task
);

define_task_spec!(
    BlobMaintenanceTask,
    BlobMaintenance,
    BlobMaintenanceTaskPayload,
    BlobMaintenanceTaskResult,
    BlobMaintenance,
    BlobMaintenance,
    steps = BLOB_MAINTENANCE_STEPS,
    lane = TaskLane::Fallback,
    process = blob_maintenance::process_blob_maintenance_task,
    max_attempts = 1
);
