use std::time::Duration;

use crate::db::repository::file_repo;
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::files::file::{
    DownloadDisposition, ensure_personal_file_scope, get_info_in_scope, if_none_match_matches,
    inline_sandbox_csp, requires_inline_sandbox,
};
use crate::services::workspace::storage::WorkspaceStorageScope;
use actix_web::http::header::HeaderValue;
use aster_drive_model::entities::{file, file_blob, file_revision};
use aster_drive_storage::PresignedDownloadOptions;
use aster_forge_utils::numbers;

use super::range::ResolvedDownloadRange;
use super::types::{DownloadOutcome, StreamedFile};

const PRESIGNED_DOWNLOAD_TTL_SECS: u64 = 5 * 60;

/// Loads content and its validator from one writer-backed snapshot.
pub(crate) async fn load_current_download_snapshot(
    state: &PrimaryAppState,
    file_id: i64,
) -> Result<(file::Model, file_blob::Model, file_revision::Model)> {
    let (file, blob, revision) =
        crate::db::repository::revision_repo::find_file_blob_and_current_revision(
            state.writer_db(),
            file_id,
        )
        .await?;
    if revision.blob_id != Some(file.blob_id)
        || revision.logical_size != file.size
        || blob.id != file.blob_id
        || blob.size != file.size
    {
        return Err(AsterError::internal_error(format!(
            "file #{file_id} content projection does not match its current revision"
        )));
    }
    Ok((file, blob, revision))
}

/// Resolves Range against a writer-backed content snapshot after access checks succeed.
///
/// Parsing against an earlier projection can pair stale bounds with a newer blob and ETag;
/// parsing before access checks can disclose the current file size through range errors.
pub(crate) fn resolve_range_for_download_snapshot(
    file: &file::Model,
    range_header: Option<&HeaderValue>,
) -> Result<Option<ResolvedDownloadRange>> {
    let range = super::range::parse_range_header(range_header, file.size)?;
    Ok(range)
}

pub(crate) async fn download_in_scope_with_range_header_and_file(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    id: i64,
    file: Option<file::Model>,
    if_none_match: Option<&str>,
    range_header: Option<&HeaderValue>,
    disposition: DownloadDisposition,
) -> Result<DownloadOutcome> {
    tracing::debug!(
        scope = ?scope,
        file_id = id,
        has_if_none_match = if_none_match.is_some(),
        has_range = range_header.is_some(),
        "starting file download"
    );
    let authorized = match file {
        Some(file) => file,
        None => get_info_in_scope(state, scope, id).await?,
    };
    crate::services::workspace::storage::ensure_active_file_scope(&authorized, scope)?;
    let (file, blob, revision) = load_current_download_snapshot(state, authorized.id).await?;
    crate::services::workspace::storage::ensure_active_file_scope(&file, scope)?;
    let range = resolve_range_for_download_snapshot(&file, range_header)?;
    build_download_outcome_with_disposition_and_range(
        state,
        &file,
        &blob,
        disposition,
        if_none_match,
        range,
        &revision.etag,
    )
    .await
}

/// 下载文件（流式，不全量缓冲）
pub async fn download(
    state: &PrimaryAppState,
    id: i64,
    user_id: i64,
    if_none_match: Option<&str>,
) -> Result<DownloadOutcome> {
    download_in_scope_with_range_header_and_file(
        state,
        WorkspaceStorageScope::Personal { user_id },
        id,
        None,
        if_none_match,
        None,
        DownloadDisposition::Attachment,
    )
    .await
}

/// 下载文件（无用户校验，用于分享链接，流式）
pub async fn download_raw(
    state: &PrimaryAppState,
    id: i64,
    if_none_match: Option<&str>,
) -> Result<DownloadOutcome> {
    let db = state.reader_db();
    let file = file_repo::find_by_id(db, id).await?;
    ensure_personal_file_scope(&file)?;
    download_raw_unchecked_with_file(state, file, if_none_match).await
}

async fn download_raw_unchecked_with_file(
    state: &PrimaryAppState,
    file: file::Model,
    if_none_match: Option<&str>,
) -> Result<DownloadOutcome> {
    let (file, blob, revision) = load_current_download_snapshot(state, file.id).await?;
    ensure_personal_file_scope(&file)?;
    if file.deleted_at.is_some() {
        return Err(AsterError::file_not_found(format!(
            "file #{} is in trash",
            file.id
        )));
    }
    build_stream_outcome(state, &file, &blob, if_none_match, None, &revision.etag).await
}

/// 构建流式下载结果（Attachment disposition）
async fn build_stream_outcome(
    state: &PrimaryAppState,
    file: &file::Model,
    blob: &file_blob::Model,
    if_none_match: Option<&str>,
    range: Option<ResolvedDownloadRange>,
    revision_etag: &str,
) -> Result<DownloadOutcome> {
    build_stream_outcome_with_disposition_and_range(
        state,
        file,
        blob,
        DownloadDisposition::Attachment,
        if_none_match,
        range,
        revision_etag,
    )
    .await
}

pub(crate) async fn build_download_outcome_with_disposition_and_range(
    state: &PrimaryAppState,
    file: &file::Model,
    blob: &file_blob::Model,
    disposition: DownloadDisposition,
    if_none_match: Option<&str>,
    range: Option<ResolvedDownloadRange>,
    revision_etag: &str,
) -> Result<DownloadOutcome> {
    if let Some(if_none_match) = if_none_match
        && if_none_match_matches(if_none_match, revision_etag)
    {
        // 命中 If-None-Match 时仍走统一 outcome builder，
        // 这样 304 和 200 会共享相同的缓存头 / sandbox 头策略。
        return build_stream_outcome_with_disposition_and_range(
            state,
            file,
            blob,
            disposition,
            Some(if_none_match),
            None,
            revision_etag,
        )
        .await;
    }

    // Conditional requests that miss must stay same-origin. Otherwise the
    // browser can carry If-None-Match through the 302 to a provider download
    // URL, turning cache revalidation into a CORS preflight dependency.
    if if_none_match.is_some() {
        return build_stream_outcome_with_disposition_and_range(
            state,
            file,
            blob,
            disposition,
            None,
            range,
            revision_etag,
        )
        .await;
    }

    if blob.is_virtual_empty() {
        return build_stream_outcome_with_disposition_and_range(
            state,
            file,
            blob,
            disposition,
            None,
            range,
            revision_etag,
        )
        .await;
    }

    let policy = state.policy_snapshot().get_policy_or_err(blob.policy_id)?;
    let requires_sandbox =
        disposition == DownloadDisposition::Inline && requires_inline_sandbox(&file.mime_type);
    let should_presign = !requires_sandbox
        && crate::storage::connectors::presigned_download_enabled(
            state.driver_registry().connectors(),
            &policy,
        )?;

    if should_presign {
        // Inline previews may redirect to provider storage only for types that
        // do not require same-origin CSP sandboxing.
        if let Some(outcome) =
            build_presigned_redirect_outcome(state, &policy, file, blob, disposition).await?
        {
            return Ok(outcome);
        }
    }

    build_stream_outcome_with_disposition_and_range(
        state,
        file,
        blob,
        disposition,
        None,
        range,
        revision_etag,
    )
    .await
}

async fn build_presigned_redirect_outcome(
    state: &PrimaryAppState,
    policy: &aster_drive_model::entities::storage_policy::Model,
    file: &file::Model,
    blob: &file_blob::Model,
    disposition: DownloadDisposition,
) -> Result<Option<DownloadOutcome>> {
    if blob.is_virtual_empty() {
        return Ok(None);
    }
    let storage_path = blob.storage_path_for_connector().ok_or_else(|| {
        AsterError::internal_error(format!("stored blob #{} is missing storage_path", blob.id))
    })?;
    let driver = state.driver_registry().get_driver(policy)?;
    let presigned = driver.extensions().presigned.ok_or_else(|| {
        AsterError::storage_driver_error("presigned download not supported by driver")
    })?;

    let url = presigned
        .presigned_url(
            storage_path,
            Duration::from_secs(PRESIGNED_DOWNLOAD_TTL_SECS),
            PresignedDownloadOptions {
                download_name: Some(file.name.clone()),
                require_download_name_match:
                    crate::storage::connectors::presigned_download_requires_filename_match(
                        state.driver_registry().connectors(),
                        policy,
                    )?,
                response_cache_control: Some("private, max-age=0, must-revalidate".to_string()),
                response_content_disposition: Some(disposition.header_value(&file.name)),
                response_content_type: Some(file.mime_type.clone()),
            },
        )
        .await?;

    let Some(url) = url else {
        return Ok(None);
    };

    tracing::debug!(
        file_id = file.id,
        blob_id = blob.id,
        policy_id = blob.policy_id,
        ttl_secs = PRESIGNED_DOWNLOAD_TTL_SECS,
        connector_id = %policy.connector_id,
        "redirecting file download to provider storage URL"
    );

    Ok(Some(DownloadOutcome::PresignedRedirect { url }))
}

pub async fn build_stream_outcome_with_disposition(
    state: &PrimaryAppState,
    file: &file::Model,
    blob: &file_blob::Model,
    disposition: DownloadDisposition,
    if_none_match: Option<&str>,
    revision_etag: &str,
) -> Result<DownloadOutcome> {
    build_stream_outcome_with_disposition_and_range(
        state,
        file,
        blob,
        disposition,
        if_none_match,
        None,
        revision_etag,
    )
    .await
}

pub(crate) async fn build_stream_outcome_with_disposition_and_range(
    state: &PrimaryAppState,
    file: &file::Model,
    blob: &file_blob::Model,
    disposition: DownloadDisposition,
    if_none_match: Option<&str>,
    range: Option<ResolvedDownloadRange>,
    revision_etag: &str,
) -> Result<DownloadOutcome> {
    let requires_sandbox =
        disposition == DownloadDisposition::Inline && requires_inline_sandbox(&file.mime_type);

    if requires_sandbox {
        tracing::debug!(
            file_id = file.id,
            blob_id = blob.id,
            mime_type = %file.mime_type,
            "adding CSP sandbox for inline script-capable file"
        );
    }

    let etag = format!("\"{revision_etag}\"");
    if let Some(if_none_match) = if_none_match
        && if_none_match_matches(if_none_match, revision_etag)
    {
        tracing::debug!(
            file_id = file.id,
            blob_id = blob.id,
            disposition = ?disposition,
            "serving cached file response with 304"
        );
        return Ok(DownloadOutcome::NotModified {
            etag,
            cache_control: "private, max-age=0, must-revalidate",
            csp: if requires_sandbox {
                Some(inline_sandbox_csp())
            } else {
                None
            },
        });
    }

    let stream: Box<dyn tokio::io::AsyncRead + Unpin + Send> = if blob.is_virtual_empty() {
        Box::new(tokio::io::empty())
    } else {
        let storage_path = blob.storage_path_for_connector().ok_or_else(|| {
            AsterError::internal_error(format!("stored blob #{} is missing storage_path", blob.id))
        })?;
        let policy = state.policy_snapshot().get_policy_or_err(blob.policy_id)?;
        let driver = state.driver_registry().get_driver(&policy)?;
        // 主下载链路必须保持流式读取；不要改回 driver.get() 的全量缓冲实现。
        match range {
            Some(range) => {
                driver
                    .get_range(storage_path, range.start(), Some(range.length()))
                    .await?
            }
            None => driver.get_stream(storage_path).await?,
        }
    };

    let reader_stream = tokio_util::io::ReaderStream::with_capacity(
        stream,
        crate::storage::io_limits::DOWNLOAD_READER_BUFFER_BYTES,
    );
    let content_length = match range {
        Some(range) => numbers::u64_to_i64(range.length(), "download range length")?,
        None => blob.size,
    };

    tracing::debug!(
        file_id = file.id,
        blob_id = blob.id,
        policy_id = blob.policy_id,
        size = blob.size,
        disposition = ?disposition,
        has_range = range.is_some(),
        "building streaming file response"
    );

    Ok(DownloadOutcome::Stream(StreamedFile {
        content_type: file.mime_type.clone(),
        content_length,
        content_disposition: disposition.header_value(&file.name),
        etag,
        cache_control: "private, max-age=0, must-revalidate",
        csp: if requires_sandbox {
            Some(inline_sandbox_csp())
        } else {
            None
        },
        range,
        body: reader_stream,
        on_abort: None,
    }))
}
