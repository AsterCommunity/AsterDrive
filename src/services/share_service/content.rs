//! 分享服务子模块：`content`。

use crate::db::repository::{file_repo, share_repo};
use crate::entities::{file, share};
use crate::errors::{AsterError, Result};
use crate::metrics_core::SharedMetricsRecorder;
use crate::runtime::PrimaryAppState;
use crate::services::file_service::ResolvedDownloadRange;
use crate::services::{
    file_service, folder_service, media_metadata_service, media_processing_service, task_service,
};
use sea_orm::DatabaseConnection;
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use tokio::sync::mpsc::{self, error::TrySendError};
use tokio_util::sync::CancellationToken;

use super::cache::{
    invalidate_active_share_target_cache_for_share, invalidate_share_token_record_cache_for_share,
};
use super::shared::{
    load_share_file_resource, load_shared_folder_file_target,
    load_shared_folder_file_target_ignoring_download_limit, load_shared_subfolder_target,
    load_usable_share_ignoring_download_limit, load_valid_folder_share_root, load_valid_share,
};

#[derive(Clone)]
pub struct ShareDownloadRollbackQueue {
    db: DatabaseConnection,
    sender: mpsc::Sender<DownloadCountRollbackJob>,
    overflow: Arc<parking_lot::Mutex<HashMap<i64, u64>>>,
    stats: Arc<DownloadCountRollbackStats>,
    metrics: SharedMetricsRecorder,
}

pub struct ShareDownloadRollbackWorker {
    db: DatabaseConnection,
    receiver: mpsc::Receiver<DownloadCountRollbackJob>,
    overflow: Arc<parking_lot::Mutex<HashMap<i64, u64>>>,
    stats: Arc<DownloadCountRollbackStats>,
    metrics: SharedMetricsRecorder,
}

#[derive(Clone, Copy)]
struct DownloadCountRollbackJob {
    share_id: i64,
    count: u64,
}

#[derive(Default)]
struct DownloadCountRollbackStats {
    pending: AtomicU64,
}

impl DownloadCountRollbackStats {
    fn enqueue(&self, count: u64) -> u64 {
        self.pending.fetch_add(count, Ordering::SeqCst) + count
    }

    fn complete(&self, count: u64) -> u64 {
        self.pending.fetch_sub(count, Ordering::SeqCst) - count
    }
}

pub fn build_share_download_rollback_queue(
    db: DatabaseConnection,
    capacity: usize,
    metrics: SharedMetricsRecorder,
) -> (ShareDownloadRollbackQueue, ShareDownloadRollbackWorker) {
    let (sender, receiver) = mpsc::channel(capacity);
    let overflow = Arc::new(parking_lot::Mutex::new(HashMap::new()));
    let stats = Arc::new(DownloadCountRollbackStats::default());

    (
        ShareDownloadRollbackQueue {
            db: db.clone(),
            sender,
            overflow: overflow.clone(),
            stats: stats.clone(),
            metrics: metrics.clone(),
        },
        ShareDownloadRollbackWorker {
            db,
            receiver,
            overflow,
            stats,
            metrics,
        },
    )
}

pub fn spawn_detached_share_download_rollback_queue(
    db: DatabaseConnection,
    capacity: usize,
) -> ShareDownloadRollbackQueue {
    let (queue, worker) =
        build_share_download_rollback_queue(db, capacity, crate::metrics_core::NoopMetrics::arc());
    drop(tokio::spawn(run_share_download_rollback_worker(
        worker, None,
    )));
    queue
}

pub fn share_download_rollback_worker_task(
    shutdown_token: CancellationToken,
    worker: ShareDownloadRollbackWorker,
) -> impl std::future::Future<Output = ()> + Send + 'static {
    run_share_download_rollback_worker(worker, Some(shutdown_token))
}

impl ShareDownloadRollbackQueue {
    pub fn enqueue(&self, share_id: i64) {
        let job = DownloadCountRollbackJob { share_id, count: 1 };
        let pending = self.stats.enqueue(job.count);
        self.metrics.set_share_download_rollback_pending(pending);

        match self.sender.try_send(job) {
            Ok(()) => self
                .metrics
                .record_share_download_rollback_event("queued", job.count),
            Err(TrySendError::Full(job)) => {
                self.push_overflow(job);
                self.metrics
                    .record_share_download_rollback_event("overflow", job.count);
            }
            Err(TrySendError::Closed(job)) => {
                self.metrics
                    .record_share_download_rollback_event("fallback_spawn", job.count);
                self.spawn_fallback(job);
            }
        }
    }

    fn push_overflow(&self, job: DownloadCountRollbackJob) {
        *self.overflow.lock().entry(job.share_id).or_default() += job.count;
    }

    fn spawn_fallback(&self, job: DownloadCountRollbackJob) {
        let db = self.db.clone();
        let stats = self.stats.clone();
        let metrics = self.metrics.clone();
        tracing::warn!(
            share_id = job.share_id,
            rollback_count = job.count,
            "download rollback worker unavailable; falling back to direct task spawn"
        );
        drop(tokio::spawn(async move {
            apply_download_count_rollback(&db, &stats, &metrics, job.share_id, job.count).await;
        }));
    }
}

async fn run_share_download_rollback_worker(
    mut worker: ShareDownloadRollbackWorker,
    shutdown_token: Option<CancellationToken>,
) {
    let mut shutting_down = false;

    loop {
        let mut batch = take_overflow_batch(&worker.overflow);

        if batch.is_empty() && shutting_down {
            drain_receiver_into_batch(&mut worker.receiver, &mut batch);
            merge_batch(&mut batch, take_overflow_batch(&worker.overflow));
            if batch.is_empty() {
                break;
            }
        }

        if batch.is_empty() {
            tokio::select! {
                biased;
                _ = async {
                    match &shutdown_token {
                        Some(token) => token.cancelled().await,
                        None => std::future::pending::<()>().await,
                    }
                } => {
                    shutting_down = true;
                    continue;
                }
                received = worker.receiver.recv() => match received {
                    Some(job) => {
                        push_job(&mut batch, job);
                        drain_receiver_into_batch(&mut worker.receiver, &mut batch);
                        merge_batch(&mut batch, take_overflow_batch(&worker.overflow));
                    }
                    None => {
                        shutting_down = true;
                        continue;
                    }
                }
            }
        } else {
            drain_receiver_into_batch(&mut worker.receiver, &mut batch);
            merge_batch(&mut batch, take_overflow_batch(&worker.overflow));
        }

        for (share_id, count) in batch {
            apply_download_count_rollback(
                &worker.db,
                &worker.stats,
                &worker.metrics,
                share_id,
                count,
            )
            .await;
        }
    }
}

async fn apply_download_count_rollback(
    db: &DatabaseConnection,
    stats: &DownloadCountRollbackStats,
    metrics: &SharedMetricsRecorder,
    share_id: i64,
    count: u64,
) {
    let event = match share_repo::decrement_download_count_by(db, share_id, count).await {
        Ok(true) => "processed_ok",
        Ok(false) => "processed_noop",
        Err(error) => {
            tracing::warn!(
                share_id,
                rollback_count = count,
                "failed to roll back download count on client abort: {error}"
            );
            "processed_error"
        }
    };

    let pending = stats.complete(count);
    metrics.record_share_download_rollback_event(event, count);
    metrics.set_share_download_rollback_pending(pending);
}

fn take_overflow_batch(overflow: &parking_lot::Mutex<HashMap<i64, u64>>) -> HashMap<i64, u64> {
    let mut guard = overflow.lock();
    std::mem::take(&mut *guard)
}

fn merge_batch(target: &mut HashMap<i64, u64>, source: HashMap<i64, u64>) {
    for (share_id, count) in source {
        *target.entry(share_id).or_default() += count;
    }
}

fn drain_receiver_into_batch(
    receiver: &mut mpsc::Receiver<DownloadCountRollbackJob>,
    batch: &mut HashMap<i64, u64>,
) {
    while let Ok(job) = receiver.try_recv() {
        push_job(batch, job);
    }
}

fn push_job(batch: &mut HashMap<i64, u64>, job: DownloadCountRollbackJob) {
    *batch.entry(job.share_id).or_default() += job.count;
}

pub async fn download_shared_file_with_range(
    state: &PrimaryAppState,
    token: &str,
    if_none_match: Option<&str>,
    range: Option<ResolvedDownloadRange>,
) -> Result<file_service::DownloadOutcome> {
    let share = load_valid_share(state, token).await?;
    let file = load_share_file_resource(state, &share).await?;
    download_share_resource_with_disposition(
        state,
        &share,
        &file,
        file_service::DownloadDisposition::Attachment,
        if_none_match,
        range,
    )
    .await
}

pub async fn download_shared_folder_file_with_range(
    state: &PrimaryAppState,
    token: &str,
    file_id: i64,
    if_none_match: Option<&str>,
    range: Option<ResolvedDownloadRange>,
) -> Result<file_service::DownloadOutcome> {
    let (share, file) = load_shared_folder_file_target(state, token, file_id).await?;
    download_share_resource_with_disposition(
        state,
        &share,
        &file,
        file_service::DownloadDisposition::Attachment,
        if_none_match,
        range,
    )
    .await
}

pub async fn list_shared_folder(
    state: &PrimaryAppState,
    token: &str,
    params: &folder_service::FolderListParams,
) -> Result<folder_service::FolderContents> {
    let (_, folder_id) = load_valid_folder_share_root(state, token).await?;
    tracing::debug!(
        folder_id,
        folder_limit = params.folder_limit,
        folder_offset = params.folder_offset,
        file_limit = params.file_limit,
        has_file_cursor = params.file_cursor.is_some(),
        sort_by = ?params.sort_by,
        sort_order = ?params.sort_order,
        "listing shared folder root"
    );

    let contents = folder_service::list_shared(state, folder_id, params).await?;
    tracing::debug!(
        folder_id,
        folders_total = contents.folders_total,
        files_total = contents.files_total,
        returned_folders = contents.folders.len(),
        returned_files = contents.files.len(),
        "listed shared folder root"
    );
    Ok(contents)
}

async fn load_or_enqueue_thumbnail(
    state: &PrimaryAppState,
    file: &file::Model,
) -> Result<Option<file_service::ThumbnailResult>> {
    let blob = file_repo::find_blob_by_id(state.writer_db(), file.blob_id).await?;
    let thumbnail = media_processing_service::load_thumbnail_if_exists(
        state,
        &blob,
        &file.name,
        &file.mime_type,
    )
    .await
    .map_err(media_processing_service::map_thumbnail_request_error)?;

    match thumbnail {
        Some(thumbnail) => Ok(Some(file_service::ThumbnailResult {
            data: thumbnail.data,
            blob_hash: blob.hash,
            thumbnail_processor: Some(thumbnail.thumbnail_processor),
            thumbnail_version: Some(thumbnail.thumbnail_version),
        })),
        None => {
            task_service::thumbnail::ensure_thumbnail_task(
                state,
                &blob,
                &file.name,
                &file.mime_type,
            )
            .await
            .map_err(media_processing_service::map_thumbnail_request_error)?;
            Ok(None)
        }
    }
}

pub async fn get_shared_thumbnail(
    state: &PrimaryAppState,
    token: &str,
) -> Result<Option<file_service::ThumbnailResult>> {
    let share = load_valid_share(state, token).await?;
    tracing::debug!(share_id = share.id, "loading shared thumbnail");
    let file = load_share_file_resource(state, &share).await?;
    let thumbnail = load_or_enqueue_thumbnail(state, &file).await?;
    tracing::debug!(
        share_id = share.id,
        file_id = file.id,
        ready = thumbnail.is_some(),
        "loaded shared thumbnail state"
    );
    Ok(thumbnail)
}

pub async fn get_shared_image_preview(
    state: &PrimaryAppState,
    token: &str,
) -> Result<Option<file_service::ImagePreviewResult>> {
    let share = load_valid_share(state, token).await?;
    tracing::debug!(share_id = share.id, "loading shared image preview");
    let file = load_share_file_resource(state, &share).await?;
    file_service::image_preview_for_file(state, &file).await
}

pub async fn get_shared_media_metadata(
    state: &PrimaryAppState,
    token: &str,
) -> Result<media_metadata_service::MediaMetadataLookup> {
    let share = load_valid_share(state, token).await?;
    tracing::debug!(share_id = share.id, "loading shared media metadata");
    let file = load_share_file_resource(state, &share).await?;
    let metadata = media_metadata_service::get_for_file(state, &file).await?;
    tracing::debug!(
        share_id = share.id,
        file_id = file.id,
        pending = matches!(
            metadata,
            media_metadata_service::MediaMetadataLookup::Pending
        ),
        "loaded shared media metadata state"
    );
    Ok(metadata)
}

pub async fn get_shared_folder_file_thumbnail(
    state: &PrimaryAppState,
    token: &str,
    file_id: i64,
) -> Result<Option<file_service::ThumbnailResult>> {
    let (_, file) = load_shared_folder_file_target(state, token, file_id).await?;
    tracing::debug!(file_id = file.id, "loading shared folder file thumbnail");

    let thumbnail = load_or_enqueue_thumbnail(state, &file).await?;
    tracing::debug!(
        file_id = file.id,
        ready = thumbnail.is_some(),
        "loaded shared folder file thumbnail state"
    );
    Ok(thumbnail)
}

pub async fn get_shared_folder_file_image_preview(
    state: &PrimaryAppState,
    token: &str,
    file_id: i64,
) -> Result<Option<file_service::ImagePreviewResult>> {
    let (_, file) = load_shared_folder_file_target(state, token, file_id).await?;
    tracing::debug!(
        file_id = file.id,
        "loading shared folder file image preview"
    );
    file_service::image_preview_for_file(state, &file).await
}

pub async fn get_shared_folder_file_media_metadata(
    state: &PrimaryAppState,
    token: &str,
    file_id: i64,
) -> Result<media_metadata_service::MediaMetadataLookup> {
    let (_, file) = load_shared_folder_file_target(state, token, file_id).await?;
    tracing::debug!(
        file_id = file.id,
        "loading shared folder file media metadata"
    );
    let metadata = media_metadata_service::get_for_file(state, &file).await?;
    tracing::debug!(
        file_id = file.id,
        pending = matches!(
            metadata,
            media_metadata_service::MediaMetadataLookup::Pending
        ),
        "loaded shared folder file media metadata state"
    );
    Ok(metadata)
}

pub(crate) async fn load_preview_shared_file(
    state: &PrimaryAppState,
    token: &str,
) -> Result<(share::Model, crate::entities::file::Model)> {
    let share = load_valid_share(state, token).await?;
    let file = load_share_file_resource(state, &share).await?;
    Ok((share, file))
}

pub(crate) async fn load_preview_shared_folder_file(
    state: &PrimaryAppState,
    token: &str,
    file_id: i64,
) -> Result<(share::Model, crate::entities::file::Model)> {
    load_shared_folder_file_target(state, token, file_id).await
}

pub(crate) async fn load_shared_file_ignoring_download_limit(
    state: &PrimaryAppState,
    token: &str,
) -> Result<(share::Model, crate::entities::file::Model)> {
    let share = load_usable_share_ignoring_download_limit(state, token).await?;
    let file = load_share_file_resource(state, &share).await?;
    Ok((share, file))
}

pub(crate) async fn load_shared_folder_file_ignoring_download_limit(
    state: &PrimaryAppState,
    token: &str,
    file_id: i64,
) -> Result<(share::Model, crate::entities::file::Model)> {
    load_shared_folder_file_target_ignoring_download_limit(state, token, file_id).await
}

pub async fn list_shared_subfolder(
    state: &PrimaryAppState,
    token: &str,
    folder_id: i64,
    params: &folder_service::FolderListParams,
) -> Result<folder_service::FolderContents> {
    let (_, target) = load_shared_subfolder_target(state, token, folder_id).await?;
    tracing::debug!(
        folder_id = target.id,
        folder_limit = params.folder_limit,
        folder_offset = params.folder_offset,
        file_limit = params.file_limit,
        has_file_cursor = params.file_cursor.is_some(),
        sort_by = ?params.sort_by,
        sort_order = ?params.sort_order,
        "listing shared subfolder"
    );

    let contents = folder_service::list_shared(state, target.id, params).await?;
    tracing::debug!(
        folder_id = target.id,
        folders_total = contents.folders_total,
        files_total = contents.files_total,
        returned_folders = contents.folders.len(),
        returned_files = contents.files.len(),
        "listed shared subfolder"
    );
    Ok(contents)
}

async fn download_share_resource_with_disposition(
    state: &PrimaryAppState,
    share: &share::Model,
    file: &crate::entities::file::Model,
    disposition: file_service::DownloadDisposition,
    if_none_match: Option<&str>,
    range: Option<ResolvedDownloadRange>,
) -> Result<file_service::DownloadOutcome> {
    tracing::debug!(
        share_id = share.id,
        file_id = file.id,
        disposition = ?disposition,
        has_if_none_match = if_none_match.is_some(),
        "starting shared file download"
    );
    let blob = file_repo::find_blob_by_id(state.writer_db(), file.blob_id).await?;

    if let Some(if_none_match) = if_none_match
        && file_service::if_none_match_matches(if_none_match, &blob.hash)
    {
        tracing::debug!(
            share_id = share.id,
            file_id = file.id,
            "shared file download satisfied by ETag"
        );
        return file_service::build_download_outcome_with_disposition_and_range(
            state,
            file,
            &blob,
            disposition,
            Some(if_none_match),
            None,
        )
        .await;
    }

    match share_repo::increment_download_count(state.writer_db(), share.id).await {
        Ok(true) => {
            invalidate_share_token_record_cache_for_share(state, share).await;
            if share.max_downloads > 0
                && share.download_count.saturating_add(1) >= share.max_downloads
            {
                invalidate_active_share_target_cache_for_share(state, share).await;
            }
        }
        Ok(false) => {
            return Err(AsterError::share_download_limit("download limit reached"));
        }
        Err(error) => {
            tracing::warn!(
                share_id = share.id,
                "failed to increment download count: {error}"
            );
            return Err(error);
        }
    }

    match file_service::build_download_outcome_with_disposition_and_range(
        state,
        file,
        &blob,
        disposition,
        None,
        range,
    )
    .await
    {
        Ok(mut outcome) => {
            // 如果是流式响应，挂一个 abort hook：客户端中途断连导致 body 未读到 EOF 就 drop 时，
            // 回滚刚才的 increment，避免 `download_count` 虚增、提前触碰 `max_downloads`。
            // NotModified/PresignedRedirect 一次性响应不需要挂 hook。
            if let file_service::DownloadOutcome::Stream(ref mut s) = outcome {
                let queue = state.share_download_rollback.clone();
                let share_id = share.id;
                s.on_abort = Some(Box::new(move || {
                    queue.enqueue(share_id);
                }));
            }
            tracing::debug!(
                share_id = share.id,
                file_id = file.id,
                "completed shared file download"
            );
            Ok(outcome)
        }
        Err(error) => {
            match share_repo::decrement_download_count_by(state.writer_db(), share.id, 1).await {
                Ok(true) => {}
                Ok(false) => {
                    tracing::warn!(
                        share_id = share.id,
                        "failed to roll back download count after response build failure"
                    );
                }
                Err(rollback_error) => {
                    tracing::warn!(
                        share_id = share.id,
                        "failed to roll back download count after response build failure: {rollback_error}"
                    );
                }
            }
            Err(error)
        }
    }
}
