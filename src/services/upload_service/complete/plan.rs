use chrono::Utc;

use crate::api::api_error_code::ApiErrorCode;
use crate::entities::upload_session;
use crate::errors::{
    AsterError, Result, upload_assembly_error_with_code, validation_error_with_code,
};
use crate::types::UploadSessionStatus;

#[derive(Debug)]
pub(super) enum CompletionPlan {
    ReturnCompleted,
    CompletePresigned,
    CompletePresignedMultipart { parts: Vec<(i32, String)> },
    CompleteRelayMultipart,
    AssembleChunks,
}

pub(super) fn determine_completion_plan(
    session: &upload_session::Model,
    parts: Option<Vec<(i32, String)>>,
) -> Result<CompletionPlan> {
    if session.status == UploadSessionStatus::Completed {
        return Ok(CompletionPlan::ReturnCompleted);
    }

    if session.status == UploadSessionStatus::Assembling {
        return Err(AsterError::upload_assembling(
            "upload is being processed, please wait and retry in a few seconds",
        ));
    }

    if session.expires_at <= Utc::now() {
        return Err(AsterError::upload_session_expired("session expired"));
    }

    if session.status == UploadSessionStatus::Failed {
        return Err(upload_assembly_error_with_code(
            ApiErrorCode::UploadPreviousFailure,
            "upload assembly failed previously; please start a new upload",
        ));
    }

    if session.status == UploadSessionStatus::Presigned {
        if session.object_multipart_id.is_some() {
            let parts = parts.ok_or_else(|| {
                validation_error_with_code(
                    ApiErrorCode::UploadPartsRequired,
                    "parts required for multipart upload completion",
                )
            })?;
            return Ok(CompletionPlan::CompletePresignedMultipart { parts });
        }

        // presigned 单文件没有分片清单，只需要校验 temp object 真实存在且大小匹配。
        return Ok(CompletionPlan::CompletePresigned);
    }

    if session.status == UploadSessionStatus::Uploading && session.object_multipart_id.is_some() {
        // relay multipart 的 completed parts 由服务端在 chunk 阶段自行收集，
        // complete 时无需客户端再次回传。
        return Ok(CompletionPlan::CompleteRelayMultipart);
    }

    if session.received_count != session.total_chunks {
        return Err(upload_assembly_error_with_code(
            ApiErrorCode::UploadIncompleteChunks,
            format!(
                "expected {} chunks, got {}",
                session.total_chunks, session.received_count
            ),
        ));
    }

    Ok(CompletionPlan::AssembleChunks)
}

pub(super) fn completion_plan_label(plan: &CompletionPlan) -> &'static str {
    match plan {
        CompletionPlan::ReturnCompleted => "return_completed",
        CompletionPlan::CompletePresigned => "complete_presigned",
        CompletionPlan::CompletePresignedMultipart { .. } => "complete_presigned_multipart",
        CompletionPlan::CompleteRelayMultipart => "complete_relay_multipart",
        CompletionPlan::AssembleChunks => "assemble_chunks",
    }
}
