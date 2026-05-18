use std::time::Instant;

use crate::entities::upload_session;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::audit_service::{self, AuditContext};
use crate::services::workspace_models::FileInfo;
use crate::types::UploadSessionStatus;

use super::complete_upload_impl_with_hints;

#[derive(Clone, Copy, Default)]
pub(super) struct CompleteUploadHints<'a> {
    pub actor_username: Option<&'a str>,
}

pub(super) async fn complete_upload_impl_with_audit(
    state: &PrimaryAppState,
    session: upload_session::Model,
    parts: Option<Vec<(i32, String)>>,
    audit_ctx: &AuditContext,
    hints: CompleteUploadHints<'_>,
) -> Result<FileInfo> {
    // TODO: split the "needs actor attribution" and "needs audit log" decisions if retry/failure
    // completion paths ever get audited separately.
    let should_log = should_log_upload_completion(&session);
    let upload_id = session.id.clone();
    let complete_started_at = Instant::now();
    let file = complete_upload_impl_with_hints(state, session, parts, hints).await?;
    let complete_elapsed_ms = complete_started_at.elapsed().as_millis();
    if should_log {
        let audit_started_at = Instant::now();
        audit_service::log(
            state,
            audit_ctx,
            audit_service::AuditAction::FileUpload,
            crate::services::audit_service::AuditEntityType::File,
            Some(file.id),
            Some(&file.name),
            None,
        )
        .await;
        tracing::debug!(
            upload_id = %upload_id,
            file_id = file.id,
            complete_elapsed_ms,
            audit_elapsed_ms = audit_started_at.elapsed().as_millis(),
            total_elapsed_ms = complete_started_at.elapsed().as_millis(),
            "audited upload completion finished"
        );
    } else {
        tracing::debug!(
            upload_id = %upload_id,
            file_id = file.id,
            complete_elapsed_ms,
            total_elapsed_ms = complete_started_at.elapsed().as_millis(),
            "upload completion returned completed session without audit"
        );
    }
    Ok(file.into())
}

pub(super) fn should_log_upload_completion(session: &upload_session::Model) -> bool {
    session.status != UploadSessionStatus::Completed
}
