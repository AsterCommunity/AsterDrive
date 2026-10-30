use super::{denied, load, persist, record, require_target, types::*};
use crate::{
    api::{
        api_error_code::ApiErrorCode,
        dto::files::{
            FileResourceDeliveryMode, FileResourceHandleRequest, FileResourcePurpose,
            FileResourceRepresentation,
        },
    },
    db::repository::{file_repo, revision_repo},
    errors::{AsterError, Result},
    runtime::PrimaryAppState,
    services::{
        auth::local::Claims, files::file, ops::audit, workspace::scope::ensure_file_resource_scope,
    },
};
use actix_web::http::header::HeaderValue;
use aster_drive_model::entities::{file as file_model, file_blob, file_revision};
use sea_orm::{DbBackend, IsolationLevel, TransactionTrait};

/// The file, revision and content must be selected from a single writer snapshot.
async fn snapshot(
    state: &PrimaryAppState,
    file_id: i64,
    revision_id: Option<i64>,
) -> Result<(file_model::Model, file_blob::Model, file_revision::Model)> {
    if revision_id.is_none() {
        return file::load_current_download_snapshot(state, file_id).await;
    }
    let db = state.writer_db();
    let txn = match db.get_database_backend() {
        DbBackend::Postgres | DbBackend::MySql => {
            db.begin_with_config(Some(IsolationLevel::RepeatableRead), None)
                .await
        }
        _ => db.begin().await,
    }?;
    let mut file = file_repo::find_by_id(&txn, file_id).await?;
    let revision = revision_repo::find_by_id_for_file(
        &txn,
        file_id,
        revision_id.ok_or_else(|| AsterError::file_not_found("revision unavailable"))?,
    )
    .await?
    .filter(|r| r.purged_at.is_none())
    .ok_or_else(|| AsterError::file_not_found("revision unavailable"))?;
    let blob = file_repo::find_blob_by_id(
        &txn,
        revision
            .blob_id
            .ok_or_else(|| AsterError::file_not_found("revision content unavailable"))?,
    )
    .await?;
    if blob.size != revision.logical_size {
        return Err(AsterError::internal_error("revision content size mismatch"));
    }
    file.blob_id = blob.id;
    file.size = revision.logical_size;
    if let Some(mime_type) = &revision.mime_type {
        file.mime_type = mime_type.clone();
    }
    txn.commit().await?;
    Ok((file, blob, revision))
}

pub async fn init_download(
    state: &PrimaryAppState,
    claims: &Claims,
    ctx: &audit::AuditContext,
    file_id: i64,
    request: &AdminDownloadInitRequest,
) -> Result<AdminDownloadInfo> {
    let payload = load(state, claims, &request.access_id).await?;
    if payload.download.is_some() {
        return Err(denied(ApiErrorCode::AdminContentContextInvalid));
    }
    let action = if request.revision_id.is_some() {
        audit::AuditAction::AdminRevisionDownload
    } else {
        audit::AuditAction::AdminFileDownload
    };
    let result = prepare(
        state,
        claims,
        ctx,
        &payload,
        action,
        file_id,
        request.revision_id,
    )
    .await;
    if result.is_err() {
        file::record_download_failure_metric(state, "admin");
        record(
            state.writer_db(),
            ctx,
            &payload,
            action,
            (Some(file_id), request.revision_id),
            "failed",
            false,
        )
        .await?;
    }
    result
}

async fn prepare(
    state: &PrimaryAppState,
    claims: &Claims,
    ctx: &audit::AuditContext,
    payload: &AccessPayload,
    action: audit::AuditAction,
    file_id: i64,
    revision_id: Option<i64>,
) -> Result<AdminDownloadInfo> {
    let (file, blob, revision) = snapshot(state, file_id, revision_id).await?;
    ensure_file_resource_scope(&file, require_target(state, payload).await?)?;
    let mut download_payload = payload.clone();
    download_payload.download = Some(DownloadTarget {
        file_id,
        revision_id: revision.id,
    });
    let grant_id = uuid::Uuid::new_v4().simple().to_string();
    let path = format!("/admin/content-access/{grant_id}/download");
    let mut resource = file::resolve_file_resource_handle_for_file(
        state,
        &file,
        &blob,
        file::FileResourcePathSet {
            download: path,
            image_preview: String::new(),
            thumbnail: String::new(),
        },
        &FileResourceHandleRequest {
            purpose: FileResourcePurpose::Download,
            delivery_mode: FileResourceDeliveryMode::DirectUrl,
            representation: FileResourceRepresentation::Original,
        },
        Some(match payload.scope_kind {
            AdminScopeKind::Personal => "personal",
            AdminScopeKind::Team => "team",
        }),
        Some(&revision.etag),
    )
    .await?;
    // URL resolution can contact a provider; keep it outside the database transaction.
    // No URL or durable grant is released until authority and mandatory audit both succeed.
    super::require_actor(state, claims).await?;
    require_target(state, payload).await?;
    let txn = state.writer_db().begin().await?;
    let grant = persist(&txn, claims, &download_payload, &grant_id).await?;
    record(
        &txn,
        ctx,
        payload,
        action,
        (Some(file_id), Some(revision.id)),
        "initialized",
        false,
    )
    .await?;
    txn.commit().await?;
    if resource.lifecycle.is_none() {
        resource.lifecycle = Some(crate::api::dto::files::FileResourceLifecycleInfo {
            expires_at: grant.expires_at,
        });
    }
    if resource.request.redirect_policy
        == crate::api::dto::files::FileResourceRedirectPolicy::MayCrossOrigin
    {
        file::record_download_metric(
            state,
            "admin",
            &file::DownloadOutcome::DirectRedirect {
                url: resource.request.url.clone(),
            },
        );
    }
    Ok(AdminDownloadInfo {
        resource,
        file_id,
        revision_id: revision.id,
        name: file.name,
        size: file.size,
    })
}

pub async fn download(
    state: &PrimaryAppState,
    claims: &Claims,
    id: &str,
    if_none_match: Option<&str>,
    range: Option<&HeaderValue>,
) -> Result<file::DownloadOutcome> {
    let payload = load(state, claims, id).await?;
    let target = payload
        .download
        .as_ref()
        .ok_or_else(|| denied(ApiErrorCode::AdminContentContextInvalid))?;
    let result = async {
        let (file, blob, revision) =
            snapshot(state, target.file_id, Some(target.revision_id)).await?;
        ensure_file_resource_scope(&file, require_target(state, &payload).await?)?;
        let range = file::resolve_range_for_download_snapshot(&file, range)?;
        // The browser has already initialized access. A same-origin fallback stays streamed.
        file::build_stream_outcome_with_disposition_and_range(
            state,
            &file,
            &blob,
            file::DownloadDisposition::Attachment,
            if_none_match,
            range,
            &revision.etag,
        )
        .await
    }
    .await;
    // Initialization already committed the required audit. Range retries are transport only.
    file::record_download_result(state, "admin", range.is_some(), async { result }).await
}
