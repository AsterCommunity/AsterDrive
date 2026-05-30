//! 后台任务服务子模块：`archive`。

mod common;
mod compress;
mod extract;
mod preview;
mod selection;

pub(crate) use compress::create_archive_compress_task_in_scope;
pub(crate) use extract::create_archive_extract_task_in_scope;
pub(crate) use preview::ensure_archive_preview_task;
pub(crate) use selection::{prepare_archive_download_in_scope, stream_archive_download_in_scope};

use crate::entities::background_task;
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;

use super::TaskLeaseGuard;
use super::retry::{TaskRetryClass, TaskRetryPolicy, default_retry_class};

pub(super) struct ArchiveCompressRetryPolicy;

impl TaskRetryPolicy for ArchiveCompressRetryPolicy {
    fn retry_class(error: &AsterError) -> TaskRetryClass {
        match error {
            AsterError::ValidationError(_)
            | AsterError::FileTooLarge(_)
            | AsterError::FileTypeNotAllowed(_) => TaskRetryClass::Never,
            _ => default_retry_class(error),
        }
    }
}

pub(super) struct ArchiveExtractRetryPolicy;

impl TaskRetryPolicy for ArchiveExtractRetryPolicy {
    fn retry_class(error: &AsterError) -> TaskRetryClass {
        match error {
            AsterError::ValidationError(_)
            | AsterError::FileTooLarge(_)
            | AsterError::FileTypeNotAllowed(_)
            | AsterError::UnsupportedDriver(_) => TaskRetryClass::Never,
            _ => default_retry_class(error),
        }
    }
}

pub(super) struct ArchivePreviewRetryPolicy;

impl TaskRetryPolicy for ArchivePreviewRetryPolicy {
    fn retry_class(error: &AsterError) -> TaskRetryClass {
        match error {
            AsterError::ValidationError(_)
            | AsterError::FileTooLarge(_)
            | AsterError::FileTypeNotAllowed(_)
            | AsterError::UnsupportedDriver(_) => TaskRetryClass::Never,
            _ => default_retry_class(error),
        }
    }
}

pub(super) async fn process_archive_compress_task(
    state: &PrimaryAppState,
    task: &background_task::Model,
    lease_guard: TaskLeaseGuard,
) -> Result<()> {
    compress::process_archive_compress_task(state, task, lease_guard).await
}

pub(super) async fn process_archive_extract_task(
    state: &PrimaryAppState,
    task: &background_task::Model,
    lease_guard: TaskLeaseGuard,
) -> Result<()> {
    extract::process_archive_extract_task(state, task, lease_guard).await
}

pub(super) async fn process_archive_preview_task(
    state: &PrimaryAppState,
    task: &background_task::Model,
    lease_guard: TaskLeaseGuard,
) -> Result<()> {
    preview::process_archive_preview_task(state, task, lease_guard).await
}
