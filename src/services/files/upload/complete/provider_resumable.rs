use crate::api::api_error_code::ApiErrorCode;
use crate::entities::{file, upload_session};
use crate::errors::{Result, upload_assembly_error_with_code};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::files::upload::shared::run_upload_completion_stage;
use crate::types::UploadSessionStatus;

use super::contract::VerifiedUploadedBlob;
use super::object_multipart::{
    ensure_uploaded_object_size, finalize_verified_opaque_upload_session, opaque_upload_file_hash,
};

pub(super) async fn complete_provider_resumable_upload(
    state: &PrimaryAppState,
    session: upload_session::Model,
    actor_username: Option<&str>,
) -> Result<file::Model> {
    let temp_key = session.object_temp_key.as_deref().ok_or_else(|| {
        upload_assembly_error_with_code(
            ApiErrorCode::UploadSessionCorrupted,
            "provider resumable session is missing object_temp_key",
        )
    })?;
    let policy = state
        .policy_snapshot()
        .get_policy_or_err(session.policy_id)?;
    let driver = state.driver_registry().get_driver(&policy)?;
    let actual_size = ensure_uploaded_object_size(
        driver.as_ref(),
        temp_key,
        session.total_size,
        "provider upload object was not found; the final range may not have committed",
    )
    .await?;

    run_upload_completion_stage(
        state.writer_db(),
        &session,
        UploadSessionStatus::Uploading,
        "completed provider resumable upload session",
        async {
            let verified = VerifiedUploadedBlob::precommitted_provider_object(
                actual_size,
                policy.id,
                temp_key.to_string(),
                opaque_upload_file_hash(&policy, &session)?,
            )?;
            finalize_verified_opaque_upload_session(
                state,
                &session,
                driver.as_ref(),
                &verified,
                actor_username,
            )
            .await
        },
    )
    .await
}
