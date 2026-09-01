use super::audit::should_log_upload_completion;
use super::plan::{CompletionPlan, completion_plan_label, determine_completion_plan};

use crate::api::api_error_code::ApiErrorCode;
use aster_drive_model::entities::upload_session;
use aster_drive_model::types::{UploadSessionKind, UploadSessionStatus};

fn mock_session(status: UploadSessionStatus) -> upload_session::Model {
    upload_session::Model {
        id: "test-upload".to_string(),
        user_id: 1,
        team_id: None,
        frontend_client_id: None,
        filename: "demo.bin".to_string(),
        mime_type: "application/octet-stream".to_string(),
        total_size: 12,
        chunk_size: 4,
        total_chunks: 3,
        received_count: 3,
        folder_id: None,
        policy_id: 1,
        placement_profile_id: None,
        placement_rule_id: None,
        placement_revision: None,
        placement_execution_preference: "automatic".to_string(),
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
fn determine_completion_plan_maps_every_upload_transport() {
    let uploading = mock_session(UploadSessionStatus::Uploading);
    for (kind, parts, expected_label) in [
        (
            UploadSessionKind::ProviderPresignedSingle,
            None,
            "complete_presigned",
        ),
        (
            UploadSessionKind::RemotePresignedSingle,
            None,
            "complete_presigned",
        ),
        (
            UploadSessionKind::ProviderRelayMultipart,
            None,
            "complete_relay_multipart",
        ),
        (
            UploadSessionKind::RemoteRelayMultipart,
            None,
            "complete_relay_multipart",
        ),
        (
            UploadSessionKind::ProviderDirectResumable,
            None,
            "complete_provider_resumable",
        ),
        (UploadSessionKind::StreamStaging, None, "complete_chunked"),
        (
            UploadSessionKind::RemotePresignedMultipart,
            Some(vec![(1, "etag-1".to_string())]),
            "complete_presigned_multipart",
        ),
    ] {
        let plan = determine_completion_plan(&uploading, kind, parts)
            .unwrap_or_else(|error| panic!("{kind:?} should map to a completion plan: {error}"));
        assert_eq!(completion_plan_label(&plan), expected_label, "{kind:?}");
    }
}

#[test]
fn determine_completion_plan_handles_completed_assembling_and_stream_states() {
    let completed = determine_completion_plan(
        &mock_session(UploadSessionStatus::Completed),
        UploadSessionKind::Stream,
        None,
    )
    .expect("completed stream should replay its result");
    assert!(matches!(completed, CompletionPlan::ReturnCompleted));
    assert_eq!(completion_plan_label(&completed), "return_completed");

    let assembling = determine_completion_plan(
        &mock_session(UploadSessionStatus::Assembling),
        UploadSessionKind::RemoteRelayMultipart,
        None,
    )
    .expect_err("assembling upload should ask the caller to retry");
    assert_eq!(assembling.code(), "E061");
    assert_eq!(assembling.api_error_code(), ApiErrorCode::UploadAssembling);

    let stream = determine_completion_plan(
        &mock_session(UploadSessionStatus::Uploading),
        UploadSessionKind::Stream,
        None,
    )
    .expect_err("stream completion requires the body endpoint first");
    assert_eq!(
        stream.api_error_code_override(),
        Some(ApiErrorCode::UploadIncompleteChunks)
    );
}

#[test]
fn provider_relay_resumable_requires_all_provider_ranges_before_finalization() {
    let mut session = mock_session(UploadSessionStatus::Uploading);
    session.session_kind = UploadSessionKind::ProviderRelayResumable;
    session.object_temp_key = Some("files/provider-relay".to_string());
    session.provider_session_ciphertext = Some("encrypted-provider-session".to_string());
    session.received_count = 2;
    assert!(
        determine_completion_plan(&session, UploadSessionKind::ProviderRelayResumable, None,)
            .is_err()
    );

    session.received_count = session.total_chunks;
    let plan = determine_completion_plan(&session, UploadSessionKind::ProviderRelayResumable, None)
        .expect("all provider ranges should finalize through provider resumable contract");
    assert!(matches!(plan, CompletionPlan::CompleteProviderResumable));
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
