use chrono::{Duration, Utc};

use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::files::upload::provider_session::{
    ProviderSessionSecret, encrypt_provider_session,
};
use crate::services::files::upload::responses::{
    InitUploadResponse, ProviderResumableUploadResponse,
};
use crate::services::files::upload::shared::{UniqueUuidAttempt, with_unique_upload_id};
use crate::services::workspace::storage::PolicyUploadTransport;
use crate::types::{ProviderResumableUploadStrategy, UploadMode, UploadSessionStatus};
use aster_forge_utils::numbers;

use super::context::{
    InitUploadContext, UploadSessionRecordParams, session_kind_for_transport,
    try_persist_upload_session,
};

pub(super) async fn init_provider_resumable_upload(
    state: &PrimaryAppState,
    ctx: &InitUploadContext,
) -> Result<Option<InitUploadResponse>> {
    let transport =
        crate::services::workspace::storage::resolve_policy_upload_transport(&ctx.policy)?;
    if transport
        != PolicyUploadTransport::ProviderResumable(ProviderResumableUploadStrategy::FrontendDirect)
    {
        return Ok(None);
    }

    let driver = state.driver_registry().get_driver(&ctx.policy)?;
    let provider = driver.as_provider_resumable_upload().ok_or_else(|| {
        AsterError::storage_driver_error(
            "storage driver does not expose provider resumable upload support",
        )
    })?;
    let capabilities = provider.provider_resumable_upload_capabilities();
    if !capabilities.frontend_direct_upload {
        return Err(AsterError::validation_error(
            "storage connector does not support frontend-direct provider uploads",
        ));
    }
    validate_provider_fragment_capabilities(&capabilities)?;
    let chunk_size = numbers::usize_to_i64(
        capabilities.default_fragment_size,
        "provider resumable fragment size",
    )?;
    let total_chunks =
        numbers::calc_total_chunks(ctx.total_size, chunk_size, "provider resumable upload")?;

    let response = with_unique_upload_id(|upload_id| async {
        let temp_key = format!("files/{upload_id}");
        let provider_session = provider.create_frontend_upload_session(&temp_key).await?;
        if provider_session.upload_url.trim().is_empty() {
            return Err(AsterError::storage_driver_error(
                "provider returned an empty resumable upload URL",
            ));
        }
        let secret = ProviderSessionSecret {
            provider: capabilities.provider.to_string(),
            upload_url: provider_session.upload_url.clone(),
        };
        let ciphertext = encrypt_provider_session(state, &upload_id, &secret)?;
        let default_expires_at = Utc::now() + Duration::hours(24);
        let expires_at = provider_session
            .expires_at
            .map(|value| value.min(default_expires_at))
            .unwrap_or(default_expires_at);

        let inserted = try_persist_upload_session(
            state.writer_db(),
            UploadSessionRecordParams {
                upload_id: &upload_id,
                scope: ctx.scope,
                filename: &ctx.target.filename,
                total_size: ctx.total_size,
                chunk_size,
                total_chunks,
                folder_id: ctx.target.folder_id,
                policy_id: ctx.policy.id,
                frontend_client_id: ctx.frontend_client_id.as_deref(),
                status: UploadSessionStatus::Uploading,
                session_kind: session_kind_for_transport(transport, UploadMode::ProviderResumable)?,
                object_temp_key: Some(&temp_key),
                object_multipart_id: None,
                provider_session_ciphertext: Some(&ciphertext),
                expires_at,
            },
        )
        .await;

        match inserted {
            Ok(true) => {}
            Ok(false) => {
                provider
                    .abort_frontend_upload_session(&provider_session.upload_url)
                    .await?;
                return Ok(UniqueUuidAttempt::Collision);
            }
            Err(error) => {
                if let Err(abort_error) = provider
                    .abort_frontend_upload_session(&provider_session.upload_url)
                    .await
                {
                    return Err(AsterError::storage_driver_error(format!(
                        "failed to persist provider upload session: {error}; abort error: {abort_error}"
                    )));
                }
                return Err(error);
            }
        }

        tracing::debug!(
            scope = ?ctx.scope,
            upload_id = %upload_id,
            policy_id = ctx.policy.id,
            mode = ?UploadMode::ProviderResumable,
            chunk_size,
            total_chunks,
            folder_id = ctx.target.folder_id,
            provider = capabilities.provider,
            "initialized frontend-direct provider resumable upload session"
        );

        Ok(UniqueUuidAttempt::Accepted(InitUploadResponse {
            mode: UploadMode::ProviderResumable,
            upload_id: Some(upload_id),
            chunk_size: Some(chunk_size),
            total_chunks: Some(total_chunks),
            presigned_url: None,
            presigned_headers: Default::default(),
            presigned_require_etag: None,
            provider_resumable: Some(ProviderResumableUploadResponse {
                upload_url: provider_session.upload_url,
                expires_at: provider_session.expires_at,
                next_expected_ranges: provider_session.next_expected_ranges,
            }),
        }))
    })
    .await?;

    Ok(Some(response))
}

fn validate_provider_fragment_capabilities(
    capabilities: &crate::storage::ProviderResumableUploadCapabilities,
) -> Result<()> {
    let size = capabilities.default_fragment_size;
    if size == 0
        || size < capabilities.min_fragment_size
        || size > capabilities.max_fragment_size
        || capabilities.fragment_alignment == 0
        || !size.is_multiple_of(capabilities.fragment_alignment)
    {
        return Err(AsterError::storage_driver_error(
            "provider resumable upload fragment capabilities are inconsistent",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_provider_fragment_capabilities;
    use crate::storage::ProviderResumableUploadCapabilities;

    fn capabilities() -> ProviderResumableUploadCapabilities {
        ProviderResumableUploadCapabilities {
            provider: "test",
            session_label: "test session",
            min_fragment_size: 320 * 1024,
            default_fragment_size: 10 * 1024 * 1024,
            max_fragment_size: 50 * 1024 * 1024,
            fragment_alignment: 320 * 1024,
            max_simple_upload_size: None,
            frontend_direct_upload: true,
            implicit_completion: true,
            abort_supported: true,
            status_query_supported: true,
        }
    }

    #[test]
    fn provider_fragment_capabilities_require_aligned_default() {
        assert!(validate_provider_fragment_capabilities(&capabilities()).is_ok());
        let mut invalid = capabilities();
        invalid.default_fragment_size += 1;
        assert!(validate_provider_fragment_capabilities(&invalid).is_err());
    }
}
