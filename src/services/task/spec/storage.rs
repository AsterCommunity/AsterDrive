use super::{BackgroundTaskSpec, TaskProcessFuture};
use crate::entities::background_task;
use crate::runtime::PrimaryAppState;
use crate::services::task::{
    TaskExecutionContext,
    dispatch::TaskLane,
    steps::{
        TASK_STEP_CLEANUP_OBJECTS, TASK_STEP_FINISH, TASK_STEP_MIGRATE_BLOBS,
        TASK_STEP_PREPARE_SOURCES, TASK_STEP_SCAN_BLOBS, TASK_STEP_WAITING, TaskStepSpec,
    },
    storage_migration, storage_policy_cleanup,
    types::{
        StoragePolicyMigrationTaskPayload, StoragePolicyMigrationTaskResult,
        StoragePolicyTempCleanupTaskPayload, StoragePolicyTempCleanupTaskPayloadInfo,
        StoragePolicyTempCleanupTaskResult, TaskPayload, TaskResult,
    },
};
use crate::types::BackgroundTaskKind;

const STORAGE_POLICY_TEMP_CLEANUP_STEPS: &[TaskStepSpec] = &[
    TaskStepSpec {
        key: TASK_STEP_WAITING,
        title: "Waiting",
    },
    TaskStepSpec {
        key: TASK_STEP_PREPARE_SOURCES,
        title: "Prepare storage driver",
    },
    TaskStepSpec {
        key: TASK_STEP_CLEANUP_OBJECTS,
        title: "Clean temporary objects",
    },
];

const STORAGE_POLICY_MIGRATION_STEPS: &[TaskStepSpec] = &[
    TaskStepSpec {
        key: TASK_STEP_WAITING,
        title: "Waiting",
    },
    TaskStepSpec {
        key: TASK_STEP_PREPARE_SOURCES,
        title: "Prepare storage policies",
    },
    TaskStepSpec {
        key: TASK_STEP_SCAN_BLOBS,
        title: "Scan source blobs",
    },
    TaskStepSpec {
        key: TASK_STEP_MIGRATE_BLOBS,
        title: "Migrate blobs",
    },
    TaskStepSpec {
        key: TASK_STEP_FINISH,
        title: "Finish migration",
    },
];

pub(crate) struct StoragePolicyTempCleanupTask;

impl BackgroundTaskSpec for StoragePolicyTempCleanupTask {
    type Payload = StoragePolicyTempCleanupTaskPayload;
    type Result = StoragePolicyTempCleanupTaskResult;

    const KIND: BackgroundTaskKind = BackgroundTaskKind::StoragePolicyTempCleanup;

    fn step_specs() -> &'static [TaskStepSpec] {
        STORAGE_POLICY_TEMP_CLEANUP_STEPS
    }

    fn lane() -> TaskLane {
        TaskLane::Fallback
    }

    fn wrap_payload(payload: Self::Payload) -> TaskPayload {
        TaskPayload::StoragePolicyTempCleanup(StoragePolicyTempCleanupTaskPayloadInfo::from(
            payload,
        ))
    }

    fn wrap_result(result: Self::Result) -> TaskResult {
        TaskResult::StoragePolicyTempCleanup(result)
    }

    fn process<'a>(
        state: &'a PrimaryAppState,
        task: &'a background_task::Model,
        context: TaskExecutionContext,
    ) -> TaskProcessFuture<'a> {
        Box::pin(
            storage_policy_cleanup::process_storage_policy_temp_cleanup_task(state, task, context),
        )
    }
}

define_task_spec!(
    StoragePolicyMigrationTask,
    StoragePolicyMigration,
    StoragePolicyMigrationTaskPayload,
    StoragePolicyMigrationTaskResult,
    StoragePolicyMigration,
    StoragePolicyMigration,
    steps = STORAGE_POLICY_MIGRATION_STEPS,
    lane = TaskLane::StorageMigration,
    process = storage_migration::process_storage_policy_migration_task
);
