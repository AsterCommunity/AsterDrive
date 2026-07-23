use super::audit::should_log_upload_completion;
use super::plan::{CompletionPlan, determine_completion_plan};

use crate::api::api_error_code::ApiErrorCode;
use crate::entities::upload_session;
use crate::types::{UploadSessionKind, UploadSessionStatus};

fn mock_session(status: UploadSessionStatus) -> upload_session::Model {
    upload_session::Model {
        id: "test-upload".to_string(),
        user_id: 1,
        team_id: None,
        frontend_client_id: None,
        filename: "demo.bin".to_string(),
        total_size: 12,
        chunk_size: 4,
        total_chunks: 3,
        received_count: 3,
        folder_id: None,
        policy_id: 1,
        status,
        session_kind: UploadSessionKind::OffsetStaging,
        object_temp_key: None,
        object_multipart_id: None,
        provider_session_ciphertext: None,
        file_id: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
    }
}

#[test]
fn determine_completion_plan_marks_previous_failure_with_code() {
    let err = determine_completion_plan(
        &mock_session(UploadSessionStatus::Failed),
        UploadSessionKind::OffsetStaging,
        None,
    )
    .expect_err("failed session should not continue");

    assert_eq!(err.code(), "E057");
    assert_eq!(
        err.api_error_code_override(),
        Some(ApiErrorCode::UploadPreviousFailure)
    );
}

#[test]
fn determine_completion_plan_rejects_expired_active_session() {
    let mut session = mock_session(UploadSessionStatus::Presigned);
    session.expires_at = chrono::Utc::now() - chrono::Duration::seconds(1);

    let err = determine_completion_plan(&session, UploadSessionKind::ProviderPresignedSingle, None)
        .expect_err("expired session should fail");

    assert_eq!(err.code(), "E055");
}

#[test]
fn determine_completion_plan_requires_parts_for_presigned_multipart() {
    let mut session = mock_session(UploadSessionStatus::Presigned);
    session.object_multipart_id = Some("mp-1".to_string());

    let err = determine_completion_plan(
        &session,
        UploadSessionKind::ProviderPresignedMultipart,
        None,
    )
    .expect_err("multipart complete needs parts");

    assert_eq!(err.code(), "E005");
    assert_eq!(
        err.api_error_code_override(),
        Some(ApiErrorCode::UploadPartsRequired)
    );
}

#[test]
fn determine_completion_plan_marks_incomplete_chunks_with_code() {
    let mut session = mock_session(UploadSessionStatus::Uploading);
    session.received_count = 2;

    let err = determine_completion_plan(&session, UploadSessionKind::OffsetStaging, None)
        .expect_err("missing chunks should fail");

    assert_eq!(err.code(), "E057");
    assert_eq!(
        err.api_error_code_override(),
        Some(ApiErrorCode::UploadIncompleteChunks)
    );
}

#[test]
fn determine_completion_plan_returns_chunked_completion_when_all_chunks_arrived() {
    let plan = determine_completion_plan(
        &mock_session(UploadSessionStatus::Uploading),
        UploadSessionKind::OffsetStaging,
        None,
    )
    .expect("complete session should produce plan");

    assert!(matches!(plan, CompletionPlan::CompleteChunked));
}

#[test]
fn should_log_upload_completion_skips_completed_retry() {
    assert!(!should_log_upload_completion(&mock_session(
        UploadSessionStatus::Completed
    )));
    assert!(should_log_upload_completion(&mock_session(
        UploadSessionStatus::Presigned
    )));
    assert!(should_log_upload_completion(&mock_session(
        UploadSessionStatus::Uploading
    )));
}
