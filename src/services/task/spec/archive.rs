use crate::services::task::{
    archive,
    dispatch::TaskLane,
    retry::TaskRetryPolicy,
    steps::{
        TASK_STEP_BUILD_ARCHIVE, TASK_STEP_DOWNLOAD_SOURCE, TASK_STEP_EXTRACT_ARCHIVE,
        TASK_STEP_IMPORT_RESULT, TASK_STEP_PERSIST_MANIFEST, TASK_STEP_PREPARE_SOURCES,
        TASK_STEP_SCAN_ARCHIVE, TASK_STEP_STORE_RESULT, TASK_STEP_WAITING, TaskStepSpec,
    },
    types::{
        ArchiveCompressTaskPayload, ArchiveCompressTaskResult, ArchiveExtractTaskPayload,
        ArchiveExtractTaskResult, ArchivePreviewTaskPayload, ArchivePreviewTaskResult,
    },
};

const ARCHIVE_COMPRESS_STEPS: &[TaskStepSpec] = &[
    TaskStepSpec {
        key: TASK_STEP_WAITING,
        title: "Waiting",
    },
    TaskStepSpec {
        key: TASK_STEP_PREPARE_SOURCES,
        title: "Prepare sources",
    },
    TaskStepSpec {
        key: TASK_STEP_BUILD_ARCHIVE,
        title: "Build archive",
    },
    TaskStepSpec {
        key: TASK_STEP_STORE_RESULT,
        title: "Save archive",
    },
];

const ARCHIVE_EXTRACT_STEPS: &[TaskStepSpec] = &[
    TaskStepSpec {
        key: TASK_STEP_WAITING,
        title: "Waiting",
    },
    TaskStepSpec {
        key: TASK_STEP_DOWNLOAD_SOURCE,
        title: "Download source archive",
    },
    TaskStepSpec {
        key: TASK_STEP_EXTRACT_ARCHIVE,
        title: "Extract archive",
    },
    TaskStepSpec {
        key: TASK_STEP_IMPORT_RESULT,
        title: "Import extracted files",
    },
];

const ARCHIVE_PREVIEW_STEPS: &[TaskStepSpec] = &[
    TaskStepSpec {
        key: TASK_STEP_WAITING,
        title: "Waiting",
    },
    TaskStepSpec {
        key: TASK_STEP_DOWNLOAD_SOURCE,
        title: "Download source archive",
    },
    TaskStepSpec {
        key: TASK_STEP_SCAN_ARCHIVE,
        title: "Scan archive manifest",
    },
    TaskStepSpec {
        key: TASK_STEP_PERSIST_MANIFEST,
        title: "Persist manifest",
    },
];

define_task_spec!(
    ArchiveCompressTask,
    ArchiveCompress,
    ArchiveCompressTaskPayload,
    ArchiveCompressTaskResult,
    ArchiveCompress,
    ArchiveCompress,
    steps = ARCHIVE_COMPRESS_STEPS,
    lane = TaskLane::Archive,
    process = archive::process_archive_compress_task,
    retry = archive::ArchiveCompressRetryPolicy
);

define_task_spec!(
    ArchiveExtractTask,
    ArchiveExtract,
    ArchiveExtractTaskPayload,
    ArchiveExtractTaskResult,
    ArchiveExtract,
    ArchiveExtract,
    steps = ARCHIVE_EXTRACT_STEPS,
    lane = TaskLane::Archive,
    process = archive::process_archive_extract_task,
    retry = archive::ArchiveExtractRetryPolicy
);

define_task_spec!(
    ArchivePreviewGenerateTask,
    ArchivePreviewGenerate,
    ArchivePreviewTaskPayload,
    ArchivePreviewTaskResult,
    ArchivePreviewGenerate,
    ArchivePreviewGenerate,
    steps = ARCHIVE_PREVIEW_STEPS,
    lane = TaskLane::Archive,
    process = archive::process_archive_preview_task,
    retry = archive::ArchivePreviewRetryPolicy
);
