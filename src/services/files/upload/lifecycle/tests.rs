use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;
use tokio::io::AsyncRead;

use super::*;
use crate::storage::error::storage_driver_error;
use crate::storage::{
    BlobMetadata, ProviderResumableUploadCapabilities, ProviderResumableUploadDriver,
    ProviderResumableUploadFragmentOutcome, ProviderResumableUploadSession,
    ProviderResumableUploadStatus, StorageDriverExtensions, StorageErrorKind,
};

#[derive(Clone, Copy)]
enum MockResult {
    Ok,
    Error(StorageErrorKind),
}

struct MockCleanupState {
    expose_provider: bool,
    abort_result: MockResult,
    delete_result: MockResult,
    exists_result: std::result::Result<bool, StorageErrorKind>,
    events: Vec<String>,
}

#[derive(Clone)]
struct MockCleanupDriver {
    state: Arc<Mutex<MockCleanupState>>,
}

impl MockCleanupDriver {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockCleanupState {
                expose_provider: true,
                abort_result: MockResult::Ok,
                delete_result: MockResult::Ok,
                exists_result: Ok(false),
                events: Vec::new(),
            })),
        }
    }

    fn configure(
        &self,
        abort_result: MockResult,
        delete_result: MockResult,
        exists_result: std::result::Result<bool, StorageErrorKind>,
    ) {
        let mut state = self.state.lock();
        state.abort_result = abort_result;
        state.delete_result = delete_result;
        state.exists_result = exists_result;
    }

    fn hide_provider_extension(&self) {
        self.state.lock().expose_provider = false;
    }

    fn events(&self) -> Vec<String> {
        self.state.lock().events.clone()
    }
}

fn mock_error(kind: StorageErrorKind, operation: &str) -> AsterError {
    storage_driver_error(kind, format!("mock {operation} error"))
}

#[async_trait]
impl StorageDriver for MockCleanupDriver {
    async fn put(&self, path: &str, _data: &[u8]) -> Result<String> {
        Ok(path.to_string())
    }

    async fn get(&self, _path: &str) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }

    async fn get_stream(&self, _path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        Ok(Box::new(tokio::io::empty()))
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let mut state = self.state.lock();
        state.events.push(format!("delete:{path}"));
        match state.delete_result {
            MockResult::Ok => Ok(()),
            MockResult::Error(kind) => Err(mock_error(kind, "delete")),
        }
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let mut state = self.state.lock();
        state.events.push(format!("exists:{path}"));
        state
            .exists_result
            .map_err(|kind| mock_error(kind, "exists"))
    }

    async fn metadata(&self, _path: &str) -> Result<BlobMetadata> {
        Ok(BlobMetadata {
            size: 0,
            content_type: None,
        })
    }

    fn extensions(&self) -> StorageDriverExtensions<'_> {
        StorageDriverExtensions {
            provider_resumable: self.state.lock().expose_provider.then_some(self),
            ..Default::default()
        }
    }
}

#[async_trait]
impl ProviderResumableUploadDriver for MockCleanupDriver {
    fn provider_resumable_upload_capabilities(&self) -> ProviderResumableUploadCapabilities {
        ProviderResumableUploadCapabilities {
            provider: "mock",
            session_label: "mock session",
            min_fragment_size: 1,
            default_fragment_size: 1,
            max_fragment_size: 1,
            fragment_alignment: 1,
            max_simple_upload_size: None,
            frontend_direct_upload: false,
            implicit_completion: true,
            abort_supported: true,
            status_query_supported: true,
        }
    }

    async fn create_upload_session(&self, _path: &str) -> Result<ProviderResumableUploadSession> {
        Err(AsterError::storage_driver_error(
            "mock create is not used by lifecycle tests",
        ))
    }

    async fn query_upload_session(
        &self,
        _upload_url: &str,
    ) -> Result<ProviderResumableUploadStatus> {
        Err(AsterError::storage_driver_error(
            "mock query is not used by lifecycle tests",
        ))
    }

    async fn abort_upload_session(&self, upload_url: &str) -> Result<()> {
        let mut state = self.state.lock();
        state.events.push(format!("abort:{upload_url}"));
        match state.abort_result {
            MockResult::Ok => Ok(()),
            MockResult::Error(kind) => Err(mock_error(kind, "abort")),
        }
    }

    async fn upload_session_fragment_reader(
        &self,
        _upload_url: &str,
        _start: u64,
        _total_size: u64,
        _reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        _fragment_size: i64,
    ) -> Result<ProviderResumableUploadFragmentOutcome> {
        Err(AsterError::storage_driver_error(
            "mock upload is not used by lifecycle tests",
        ))
    }
}

async fn cleanup(
    driver: &MockCleanupDriver,
    kind: UploadSessionKind,
    upload_url: Option<&str>,
) -> UploadRemoteCleanupOutcome {
    cleanup_resolved_remote_upload_state(
        "upload-1",
        Some(kind),
        driver,
        upload_url,
        "files/temp.bin",
        None,
    )
    .await
}

#[tokio::test]
async fn provider_resumable_cleanup_aborts_before_deleting_for_direct_and_relay_modes() {
    for kind in [
        UploadSessionKind::ProviderDirectResumable,
        UploadSessionKind::ProviderRelayResumable,
    ] {
        let driver = MockCleanupDriver::new();
        assert_eq!(
            cleanup(&driver, kind, Some("https://provider.invalid/session")).await,
            UploadRemoteCleanupOutcome::Complete
        );
        assert_eq!(
            driver.events(),
            [
                "abort:https://provider.invalid/session",
                "delete:files/temp.bin"
            ]
        );
    }
}

#[tokio::test]
async fn provider_session_not_found_still_deletes_the_temp_object() {
    let driver = MockCleanupDriver::new();
    driver.configure(
        MockResult::Error(StorageErrorKind::NotFound),
        MockResult::Ok,
        Ok(false),
    );

    assert_eq!(
        cleanup(
            &driver,
            UploadSessionKind::ProviderRelayResumable,
            Some("expired-session")
        )
        .await,
        UploadRemoteCleanupOutcome::Complete
    );
    assert_eq!(
        driver.events(),
        ["abort:expired-session", "delete:files/temp.bin"]
    );
}

#[tokio::test]
async fn retryable_abort_error_keeps_remote_object_for_retry() {
    let driver = MockCleanupDriver::new();
    driver.configure(
        MockResult::Error(StorageErrorKind::Transient),
        MockResult::Ok,
        Ok(false),
    );

    assert_eq!(
        cleanup(
            &driver,
            UploadSessionKind::ProviderRelayResumable,
            Some("active-session")
        )
        .await,
        UploadRemoteCleanupOutcome::DeferredRetry
    );
    assert_eq!(driver.events(), ["abort:active-session"]);
}

#[tokio::test]
async fn delete_error_is_complete_only_when_existence_check_proves_absence() {
    for (exists, expected) in [
        (false, UploadRemoteCleanupOutcome::Complete),
        (true, UploadRemoteCleanupOutcome::DeferredRetry),
    ] {
        let driver = MockCleanupDriver::new();
        driver.configure(
            MockResult::Ok,
            MockResult::Error(StorageErrorKind::Transient),
            Ok(exists),
        );

        assert_eq!(
            cleanup(
                &driver,
                UploadSessionKind::ProviderRelayResumable,
                Some("session")
            )
            .await,
            expected
        );
        assert_eq!(
            driver.events(),
            [
                "abort:session",
                "delete:files/temp.bin",
                "exists:files/temp.bin"
            ]
        );
    }
}

#[tokio::test]
async fn unverifiable_terminal_delete_error_requires_intervention() {
    let driver = MockCleanupDriver::new();
    driver.configure(
        MockResult::Ok,
        MockResult::Error(StorageErrorKind::Permission),
        Err(StorageErrorKind::Transient),
    );

    assert_eq!(
        cleanup(
            &driver,
            UploadSessionKind::ProviderRelayResumable,
            Some("session")
        )
        .await,
        UploadRemoteCleanupOutcome::DeferredIntervention
    );
}

#[tokio::test]
async fn provider_cleanup_requires_both_session_url_and_provider_extension() {
    let missing_url = MockCleanupDriver::new();
    assert_eq!(
        cleanup(
            &missing_url,
            UploadSessionKind::ProviderRelayResumable,
            None
        )
        .await,
        UploadRemoteCleanupOutcome::DeferredIntervention
    );
    assert!(missing_url.events().is_empty());

    let missing_extension = MockCleanupDriver::new();
    missing_extension.hide_provider_extension();
    assert_eq!(
        cleanup(
            &missing_extension,
            UploadSessionKind::ProviderRelayResumable,
            Some("session")
        )
        .await,
        UploadRemoteCleanupOutcome::DeferredIntervention
    );
    assert!(missing_extension.events().is_empty());
}

#[tokio::test]
async fn non_provider_cleanup_skips_abort_and_deletes_directly() {
    let driver = MockCleanupDriver::new();
    assert_eq!(
        cleanup(&driver, UploadSessionKind::ProviderPresignedSingle, None).await,
        UploadRemoteCleanupOutcome::Complete
    );
    assert_eq!(driver.events(), ["delete:files/temp.bin"]);
}
