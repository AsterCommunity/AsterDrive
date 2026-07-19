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
    let provider = driver.extensions().provider_resumable.ok_or_else(|| {
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
    let session_kind = session_kind_for_transport(transport, UploadMode::ProviderResumable)?;

    let response = with_unique_upload_id(|upload_id| async {
        let temp_key = crate::services::workspace::storage::nondedup_storage_path_for_policy(
            &ctx.policy,
            &upload_id,
            Some(&ctx.target.filename),
        )?;
        let provider_session = provider.create_frontend_upload_session(&temp_key).await?;
        if provider_session.upload_url.trim().is_empty() {
            let error = AsterError::storage_driver_error(
                "provider returned an empty resumable upload URL",
            );
            if let Err(delete_error) = driver.delete(&temp_key).await {
                return Err(AsterError::storage_driver_error(format!(
                    "{error}; failed to delete provider upload object: {delete_error}"
                )));
            }
            return Err(error);
        }
        let secret = ProviderSessionSecret {
            provider: capabilities.provider.to_string(),
            upload_url: provider_session.upload_url.clone(),
        };
        let ciphertext = match encrypt_provider_session(state, &upload_id, &secret) {
            Ok(ciphertext) => ciphertext,
            Err(error) => {
                if let Err(cleanup_error) = cleanup_provider_session_after_init_error(
                    driver.as_ref(),
                    provider,
                    &provider_session.upload_url,
                    &temp_key,
                    "provider session encryption failure",
                )
                .await
                {
                    return Err(AsterError::storage_driver_error(format!(
                        "failed to encrypt provider upload session: {error}; cleanup error: {cleanup_error}"
                    )));
                }
                return Err(error);
            }
        };
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
                session_kind,
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
                cleanup_provider_session_after_init_error(
                    driver.as_ref(),
                    provider,
                    &provider_session.upload_url,
                    &temp_key,
                    "upload id collision",
                )
                .await?;
                return Ok(UniqueUuidAttempt::Collision);
            }
            Err(error) => {
                if let Err(cleanup_error) = cleanup_provider_session_after_init_error(
                    driver.as_ref(),
                    provider,
                    &provider_session.upload_url,
                    &temp_key,
                    "upload session persistence failure",
                )
                .await
                {
                    return Err(AsterError::storage_driver_error(format!(
                        "failed to persist provider upload session: {error}; cleanup error: {cleanup_error}"
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

async fn cleanup_provider_session_after_init_error(
    driver: &dyn crate::storage::StorageDriver,
    provider: &dyn crate::storage::ProviderResumableUploadDriver,
    upload_url: &str,
    temp_key: &str,
    context: &str,
) -> Result<()> {
    let abort_error = provider
        .abort_frontend_upload_session(upload_url)
        .await
        .err();
    let delete_error = driver.delete(temp_key).await.err();
    match (abort_error, delete_error) {
        (None, None) => Ok(()),
        (Some(abort_error), None) => Err(AsterError::storage_driver_error(format!(
            "failed to abort provider upload session after {context}: {abort_error}"
        ))),
        (None, Some(delete_error)) => Err(AsterError::storage_driver_error(format!(
            "failed to delete provider upload object after {context}: {delete_error}"
        ))),
        (Some(abort_error), Some(delete_error)) => Err(AsterError::storage_driver_error(format!(
            "failed to cleanup provider upload after {context}: abort error: {abort_error}; delete error: {delete_error}"
        ))),
    }
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
    use super::{
        cleanup_provider_session_after_init_error, validate_provider_fragment_capabilities,
    };
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::errors::{AsterError, Result};
    use crate::storage::{
        BlobMetadata, ProviderResumableUploadCapabilities, ProviderResumableUploadDriver,
        ProviderResumableUploadSession, ProviderResumableUploadStatus, StorageDriver,
    };

    struct CleanupDriver {
        delete_calls: AtomicUsize,
        fail_delete: bool,
    }

    #[async_trait]
    impl StorageDriver for CleanupDriver {
        async fn put(&self, path: &str, _data: &[u8]) -> Result<String> {
            Ok(path.to_string())
        }

        async fn get(&self, _path: &str) -> Result<Vec<u8>> {
            Ok(Vec::new())
        }

        async fn get_stream(
            &self,
            _path: &str,
        ) -> Result<Box<dyn tokio::io::AsyncRead + Unpin + Send>> {
            Ok(Box::new(tokio::io::empty()))
        }

        async fn delete(&self, _path: &str) -> Result<()> {
            self.delete_calls.fetch_add(1, Ordering::SeqCst);
            if self.fail_delete {
                return Err(AsterError::storage_driver_error("delete failed"));
            }
            Ok(())
        }

        async fn exists(&self, _path: &str) -> Result<bool> {
            Ok(false)
        }

        async fn metadata(&self, _path: &str) -> Result<BlobMetadata> {
            Ok(BlobMetadata {
                size: 0,
                content_type: None,
            })
        }
    }

    struct CleanupProvider {
        abort_calls: AtomicUsize,
        fail_abort: bool,
    }

    #[async_trait]
    impl ProviderResumableUploadDriver for CleanupProvider {
        fn provider_resumable_upload_capabilities(&self) -> ProviderResumableUploadCapabilities {
            capabilities()
        }

        async fn create_frontend_upload_session(
            &self,
            _path: &str,
        ) -> Result<ProviderResumableUploadSession> {
            unreachable!("cleanup test does not create sessions")
        }

        async fn query_frontend_upload_session(
            &self,
            _upload_url: &str,
        ) -> Result<ProviderResumableUploadStatus> {
            unreachable!("cleanup test does not query sessions")
        }

        async fn abort_frontend_upload_session(&self, _upload_url: &str) -> Result<()> {
            self.abort_calls.fetch_add(1, Ordering::SeqCst);
            if self.fail_abort {
                return Err(AsterError::storage_driver_error("abort failed"));
            }
            Ok(())
        }
    }

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

    #[test]
    fn provider_fragment_capabilities_reject_zero_and_out_of_range_values() {
        let mutations: [fn(&mut ProviderResumableUploadCapabilities); 4] = [
            |value: &mut ProviderResumableUploadCapabilities| value.default_fragment_size = 0,
            |value: &mut ProviderResumableUploadCapabilities| {
                value.default_fragment_size = value.min_fragment_size - 1
            },
            |value: &mut ProviderResumableUploadCapabilities| {
                value.default_fragment_size = value.max_fragment_size + 1
            },
            |value: &mut ProviderResumableUploadCapabilities| value.fragment_alignment = 0,
        ];
        for mutate in mutations {
            let mut invalid = capabilities();
            mutate(&mut invalid);
            assert!(validate_provider_fragment_capabilities(&invalid).is_err());
        }
    }

    #[tokio::test]
    async fn init_error_cleanup_aborts_session_and_deletes_temp_object() {
        let driver = CleanupDriver {
            delete_calls: AtomicUsize::new(0),
            fail_delete: false,
        };
        let provider = CleanupProvider {
            abort_calls: AtomicUsize::new(0),
            fail_abort: false,
        };

        cleanup_provider_session_after_init_error(
            &driver,
            &provider,
            "https://upload.example/session",
            "files/upload/file.txt",
            "test",
        )
        .await
        .expect("both cleanup operations should succeed");

        assert_eq!(provider.abort_calls.load(Ordering::SeqCst), 1);
        assert_eq!(driver.delete_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn init_error_cleanup_attempts_delete_even_when_abort_fails() {
        let driver = CleanupDriver {
            delete_calls: AtomicUsize::new(0),
            fail_delete: false,
        };
        let provider = CleanupProvider {
            abort_calls: AtomicUsize::new(0),
            fail_abort: true,
        };

        let error = cleanup_provider_session_after_init_error(
            &driver,
            &provider,
            "https://upload.example/session",
            "files/upload/file.txt",
            "test",
        )
        .await
        .expect_err("abort failure should be reported after delete is attempted");

        assert!(error.message().contains("abort failed"));
        assert_eq!(provider.abort_calls.load(Ordering::SeqCst), 1);
        assert_eq!(driver.delete_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn init_error_cleanup_reports_combined_failures() {
        let driver = CleanupDriver {
            delete_calls: AtomicUsize::new(0),
            fail_delete: true,
        };
        let provider = CleanupProvider {
            abort_calls: AtomicUsize::new(0),
            fail_abort: true,
        };

        let error = cleanup_provider_session_after_init_error(
            &driver,
            &provider,
            "https://upload.example/session",
            "files/upload/file.txt",
            "test",
        )
        .await
        .expect_err("both cleanup failures should be preserved");

        assert!(error.message().contains("abort failed"));
        assert!(error.message().contains("delete failed"));
        assert_eq!(provider.abort_calls.load(Ordering::SeqCst), 1);
        assert_eq!(driver.delete_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn init_error_cleanup_reports_delete_failure_after_successful_abort() {
        let driver = CleanupDriver {
            delete_calls: AtomicUsize::new(0),
            fail_delete: true,
        };
        let provider = CleanupProvider {
            abort_calls: AtomicUsize::new(0),
            fail_abort: false,
        };

        let error = cleanup_provider_session_after_init_error(
            &driver,
            &provider,
            "https://upload.example/session",
            "files/upload/file.txt",
            "test",
        )
        .await
        .expect_err("delete failure should be reported after abort succeeds");

        assert!(error.message().contains("delete failed"));
        assert_eq!(provider.abort_calls.load(Ordering::SeqCst), 1);
        assert_eq!(driver.delete_calls.load(Ordering::SeqCst), 1);
    }
}
