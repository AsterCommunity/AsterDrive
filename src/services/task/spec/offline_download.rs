use crate::services::task::{
    dispatch::TaskLane,
    offline_download as offline_download_task,
    retry::TaskRetryPolicy,
    steps::{
        TASK_STEP_DOWNLOAD_SOURCE, TASK_STEP_STORE_RESULT, TASK_STEP_VALIDATE_SOURCE,
        TASK_STEP_VERIFY_SOURCE, TASK_STEP_WAITING, TaskStepSpec,
    },
    types::{
        OfflineDownloadTaskPayload, OfflineDownloadTaskPayloadInfo, OfflineDownloadTaskResult,
    },
};
use url::Url;

const OFFLINE_DOWNLOAD_STEPS: &[TaskStepSpec] = &[
    TaskStepSpec {
        key: TASK_STEP_WAITING,
        title: "Waiting",
    },
    TaskStepSpec {
        key: TASK_STEP_VALIDATE_SOURCE,
        title: "Validate source",
    },
    TaskStepSpec {
        key: TASK_STEP_DOWNLOAD_SOURCE,
        title: "Download source file",
    },
    TaskStepSpec {
        key: TASK_STEP_VERIFY_SOURCE,
        title: "Verify source file",
    },
    TaskStepSpec {
        key: TASK_STEP_STORE_RESULT,
        title: "Import file",
    },
];

fn offline_download_payload_info(
    payload: OfflineDownloadTaskPayload,
) -> OfflineDownloadTaskPayloadInfo {
    OfflineDownloadTaskPayloadInfo {
        filename: payload.filename,
        target_folder_id: payload.target_folder_id,
        expected_sha256: payload.expected_sha256,
        source_display_url: payload.source_display_url.unwrap_or_else(|| {
            Url::parse(&payload.url)
                .map(|url| offline_download_task::redact_url_for_display(&url))
                .unwrap_or_else(|_| "external link".to_string())
        }),
    }
}

define_task_spec!(
    OfflineDownloadTask,
    OfflineDownload,
    OfflineDownloadTaskPayload,
    OfflineDownloadTaskResult,
    OfflineDownload,
    OfflineDownload,
    steps = OFFLINE_DOWNLOAD_STEPS,
    lane = TaskLane::OfflineDownload,
    process = offline_download_task::process_offline_download_task,
    retry = offline_download_task::OfflineDownloadRetryPolicy,
    payload_wrap = offline_download_payload_info
);
