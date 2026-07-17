use chrono::Utc;

use crate::api::api_error_code::ApiErrorCode;
use crate::entities::upload_session;
use crate::errors::{
    AsterError, Result, upload_assembly_error_with_code, validation_error_with_code,
};
use crate::types::{UploadSessionKind, UploadSessionStatus};

#[derive(Debug)]
pub(super) enum CompletionPlan {
    ReturnCompleted,
    CompletePresigned,
    CompletePresignedMultipart { parts: Vec<(i32, String)> },
    CompleteRelayMultipart,
    CompleteChunked,
}

pub(super) fn determine_completion_plan(
    session: &upload_session::Model,
    kind: UploadSessionKind,
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

    match kind {
        UploadSessionKind::ProviderPresignedMultipart
        | UploadSessionKind::RemotePresignedMultipart => {
            let parts = parts.ok_or_else(|| {
                validation_error_with_code(
                    ApiErrorCode::UploadPartsRequired,
                    "parts required for multipart upload completion",
                )
            })?;
            Ok(CompletionPlan::CompletePresignedMultipart { parts })
        }
        UploadSessionKind::ProviderPresignedSingle | UploadSessionKind::RemotePresignedSingle => {
            Ok(CompletionPlan::CompletePresigned)
        }
        UploadSessionKind::ProviderRelayMultipart | UploadSessionKind::RemoteRelayMultipart => {
            Ok(CompletionPlan::CompleteRelayMultipart)
        }
        UploadSessionKind::OffsetStaging
        | UploadSessionKind::StreamStaging
        | UploadSessionKind::LegacyChunkFiles => {
            if session.received_count != session.total_chunks {
                return Err(upload_assembly_error_with_code(
                    ApiErrorCode::UploadIncompleteChunks,
                    format!(
                        "expected {} chunks, got {}",
                        session.total_chunks, session.received_count
                    ),
                ));
            }
            Ok(CompletionPlan::CompleteChunked)
        }
    }
}

pub(super) fn completion_plan_label(plan: &CompletionPlan) -> &'static str {
    match plan {
        CompletionPlan::ReturnCompleted => "return_completed",
        CompletionPlan::CompletePresigned => "complete_presigned",
        CompletionPlan::CompletePresignedMultipart { .. } => "complete_presigned_multipart",
        CompletionPlan::CompleteRelayMultipart => "complete_relay_multipart",
        CompletionPlan::CompleteChunked => "complete_chunked",
    }
}
