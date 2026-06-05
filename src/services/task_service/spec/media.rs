use crate::services::task_service::{
    dispatch::TaskLane,
    media_metadata,
    retry::TaskRetryPolicy,
    steps::{
        TASK_STEP_EXTRACT_METADATA, TASK_STEP_INSPECT_SOURCE, TASK_STEP_PERSIST_METADATA,
        TASK_STEP_PERSIST_THUMBNAIL, TASK_STEP_RENDER_THUMBNAIL, TASK_STEP_WAITING, TaskStepSpec,
    },
    thumbnail,
    types::{
        ImagePreviewGenerateTaskPayload, ImagePreviewGenerateTaskResult,
        MediaMetadataExtractTaskPayload, MediaMetadataExtractTaskResult,
        ThumbnailGenerateTaskPayload, ThumbnailGenerateTaskResult,
    },
};

const THUMBNAIL_STEPS: &[TaskStepSpec] = &[
    TaskStepSpec {
        key: TASK_STEP_WAITING,
        title: "Waiting",
    },
    TaskStepSpec {
        key: TASK_STEP_INSPECT_SOURCE,
        title: "Inspect source blob",
    },
    TaskStepSpec {
        key: TASK_STEP_RENDER_THUMBNAIL,
        title: "Render thumbnail",
    },
    TaskStepSpec {
        key: TASK_STEP_PERSIST_THUMBNAIL,
        title: "Persist thumbnail",
    },
];

const MEDIA_METADATA_STEPS: &[TaskStepSpec] = &[
    TaskStepSpec {
        key: TASK_STEP_WAITING,
        title: "Waiting",
    },
    TaskStepSpec {
        key: TASK_STEP_INSPECT_SOURCE,
        title: "Inspect source blob",
    },
    TaskStepSpec {
        key: TASK_STEP_EXTRACT_METADATA,
        title: "Extract metadata",
    },
    TaskStepSpec {
        key: TASK_STEP_PERSIST_METADATA,
        title: "Persist metadata",
    },
];

define_task_spec!(
    ThumbnailGenerateTask,
    ThumbnailGenerate,
    ThumbnailGenerateTaskPayload,
    ThumbnailGenerateTaskResult,
    ThumbnailGenerate,
    ThumbnailGenerate,
    steps = THUMBNAIL_STEPS,
    lane = TaskLane::Thumbnail,
    process = thumbnail::process_thumbnail_generate_task,
    max_attempts = 1,
    retry = thumbnail::ThumbnailRetryPolicy
);

define_task_spec!(
    ImagePreviewGenerateTask,
    ImagePreviewGenerate,
    ImagePreviewGenerateTaskPayload,
    ImagePreviewGenerateTaskResult,
    ImagePreviewGenerate,
    ImagePreviewGenerate,
    steps = THUMBNAIL_STEPS,
    lane = TaskLane::Thumbnail,
    process = thumbnail::process_image_preview_generate_task,
    max_attempts = 1,
    retry = thumbnail::ThumbnailRetryPolicy
);

define_task_spec!(
    MediaMetadataExtractTask,
    MediaMetadataExtract,
    MediaMetadataExtractTaskPayload,
    MediaMetadataExtractTaskResult,
    MediaMetadataExtract,
    MediaMetadataExtract,
    steps = MEDIA_METADATA_STEPS,
    lane = TaskLane::Thumbnail,
    process = media_metadata::process_media_metadata_extract_task,
    max_attempts = 3,
    retry = media_metadata::MediaMetadataRetryPolicy
);
