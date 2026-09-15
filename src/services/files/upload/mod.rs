//! Unified upload service.
//!
//! `plan` fixes metadata, placement and transport; `session` owns durable state;
//! `ingest` accepts bytes; `complete` publishes the file; `cleanup` handles all
//! terminal and recovery paths.

mod cleanup;
mod complete;
mod ingest;
pub(crate) mod plan;
mod session;

pub use cleanup::{
    ForceCleanupByPolicyResult, abandon_for_disaster_policy_purge, cancel_upload,
    cancel_upload_for_team, cleanup_before_forced_policy_delete, cleanup_expired,
};
pub use complete::{
    complete_upload, complete_upload_for_team, complete_upload_for_team_with_audit,
    complete_upload_with_audit,
};
pub(crate) use ingest::ingest_stream;
#[cfg(debug_assertions)]
pub use ingest::test_support;
pub use ingest::{
    upload_chunk, upload_chunk_bytes, upload_chunk_bytes_for_team, upload_chunk_for_team,
    upload_chunk_payload, upload_chunk_payload_for_team,
};
pub use plan::{
    InitUploadParams, init_upload, init_upload_for_team, init_upload_for_team_with_frontend_client,
    init_upload_with_frontend_client,
};
pub use session::{
    ChunkUploadResponse, InitUploadResponse, ProviderResumableUploadResponse,
    RecoverableUploadSessionResponse, UploadProgressResponse, get_progress, get_progress_for_team,
    list_recoverable_sessions, list_recoverable_sessions_for_team, presign_parts,
    presign_parts_for_team,
};

pub(crate) fn upload_status_conflict(
    actual_status: aster_drive_model::types::UploadSessionStatus,
    expected_status: aster_drive_model::types::UploadSessionStatus,
) -> crate::errors::AsterError {
    crate::errors::AsterError::conflict(format!(
        "session status is '{}', expected '{}'",
        session::shared::upload_session_status_label(actual_status),
        session::shared::upload_session_status_label(expected_status)
    ))
    .with_api_error_code(crate::api::api_error_code::ApiErrorCode::UploadStatusConflict)
}

fn audit_details_with_data_plane(
    details: Option<serde_json::Value>,
    data_plane: &'static str,
) -> serde_json::Value {
    let mut details = details.unwrap_or_else(|| serde_json::json!({}));
    if !details.is_object() {
        details = serde_json::json!({});
    }
    if let Some(object) = details.as_object_mut() {
        object.insert(
            "upload_data_plane".to_string(),
            serde_json::json!(data_plane),
        );
    }
    details
}

#[cfg(test)]
mod tests {
    use super::{audit_details_with_data_plane, upload_status_conflict};

    #[test]
    fn upload_audit_data_plane_preserves_object_and_normalizes_invalid_details() {
        let details = audit_details_with_data_plane(
            Some(serde_json::json!({"path": "/report.bin"})),
            "staged",
        );
        assert_eq!(details["path"], "/report.bin");
        assert_eq!(details["upload_data_plane"], "staged");

        for input in [None, Some(serde_json::json!(["invalid"]))] {
            let details = audit_details_with_data_plane(input, "streaming_direct");
            assert_eq!(
                details,
                serde_json::json!({"upload_data_plane": "streaming_direct"})
            );
        }
    }

    #[test]
    fn upload_status_conflict_maps_to_http_conflict_with_stable_code() {
        let error = upload_status_conflict(
            aster_drive_model::types::UploadSessionStatus::Assembling,
            aster_drive_model::types::UploadSessionStatus::Uploading,
        );

        assert_eq!(error.http_status(), actix_web::http::StatusCode::CONFLICT);
        assert_eq!(
            error.api_error_code(),
            crate::api::api_error_code::ApiErrorCode::UploadStatusConflict
        );
        assert_eq!(
            error.message(),
            "session status is 'assembling', expected 'uploading'"
        );
    }
}
