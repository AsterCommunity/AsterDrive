use super::audit::should_log_upload_completion;
use super::plan::{CompletionPlan, determine_completion_plan};

use crate::api::subcode::ApiSubcode;
use crate::entities::upload_session;
use crate::types::UploadSessionStatus;

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
        s3_temp_key: None,
        s3_multipart_id: None,
        file_id: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
    }
}

#[test]
fn determine_completion_plan_marks_previous_failure_with_subcode() {
    let err = determine_completion_plan(&mock_session(UploadSessionStatus::Failed), None)
        .expect_err("failed session should not continue");

    assert_eq!(err.code(), "E057");
    assert_eq!(
        err.api_error_subcode(),
        Some(ApiSubcode::UploadPreviousFailure)
    );
}

#[test]
fn determine_completion_plan_rejects_expired_active_session() {
    let mut session = mock_session(UploadSessionStatus::Presigned);
    session.expires_at = chrono::Utc::now() - chrono::Duration::seconds(1);

    let err = determine_completion_plan(&session, None).expect_err("expired session should fail");

    assert_eq!(err.code(), "E055");
}

#[test]
fn determine_completion_plan_requires_parts_for_presigned_multipart() {
    let mut session = mock_session(UploadSessionStatus::Presigned);
    session.s3_multipart_id = Some("mp-1".to_string());

    let err =
        determine_completion_plan(&session, None).expect_err("multipart complete needs parts");

    assert_eq!(err.code(), "E005");
    assert_eq!(
        err.api_error_subcode(),
        Some(ApiSubcode::UploadPartsRequired)
    );
}

#[test]
fn determine_completion_plan_marks_incomplete_chunks_with_subcode() {
    let mut session = mock_session(UploadSessionStatus::Uploading);
    session.received_count = 2;

    let err = determine_completion_plan(&session, None).expect_err("missing chunks should fail");

    assert_eq!(err.code(), "E057");
    assert_eq!(
        err.api_error_subcode(),
        Some(ApiSubcode::UploadIncompleteChunks)
    );
}

#[test]
fn determine_completion_plan_returns_chunk_assembly_when_all_chunks_arrived() {
    let plan = determine_completion_plan(&mock_session(UploadSessionStatus::Uploading), None)
        .expect("complete session should produce plan");

    assert!(matches!(plan, CompletionPlan::AssembleChunks));
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
