use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use actix_web::FromRequest;
use async_trait::async_trait;
use bytes::Bytes;
use chrono::{Duration, Utc};
use parking_lot::Mutex;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set, sea_query::Expr};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::Notify;

use super::*;
use crate::db::repository::{upload_session_part_repo, upload_session_repo};
use crate::services::files::upload::provider_session::{
    ProviderSessionSecret, encrypt_provider_session,
};
use aster_drive_model::entities::{upload_session, upload_session_part, user};
use aster_drive_model::types::{UploadSessionKind, UserRole, UserStatus};
use aster_drive_storage::error::storage_driver_error;
use aster_drive_storage::{
    BlobMetadata, ProviderResumableUploadCapabilities, ProviderResumableUploadSession,
    ProviderResumableUploadStatus, StorageDriverExtensions,
};

#[derive(Clone)]
enum FragmentBehavior {
    Success,
    FailBeforeCommit,
    FailAfterCommit,
    ReturnOffset(u64),
    Wait {
        started: Arc<Notify>,
        release: Arc<Notify>,
    },
}

#[derive(Clone, Copy)]
enum QueryBehavior {
    Default,
    Offset(u64),
    NotFound,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FragmentCall {
    start: u64,
    total_size: u64,
    fragment_size: i64,
    body: Vec<u8>,
}

struct MockProviderState {
    total_size: u64,
    next_offset: u64,
    object_exists: bool,
    fragment_behaviors: VecDeque<FragmentBehavior>,
    query_behaviors: VecDeque<QueryBehavior>,
    fragment_calls: Vec<FragmentCall>,
    query_calls: usize,
    abort_calls: usize,
    delete_calls: usize,
}

#[derive(Clone)]
struct MockProviderDriver {
    state: Arc<Mutex<MockProviderState>>,
}

impl MockProviderDriver {
    fn new(total_size: u64, behaviors: impl IntoIterator<Item = FragmentBehavior>) -> Self {
        Self {
            state: Arc::new(Mutex::new(MockProviderState {
                total_size,
                next_offset: 0,
                object_exists: false,
                fragment_behaviors: behaviors.into_iter().collect(),
                query_behaviors: VecDeque::new(),
                fragment_calls: Vec::new(),
                query_calls: 0,
                abort_calls: 0,
                delete_calls: 0,
            })),
        }
    }

    fn push_fragment_behavior(&self, behavior: FragmentBehavior) {
        self.state.lock().fragment_behaviors.push_back(behavior);
    }

    fn push_query_behavior(&self, behavior: QueryBehavior) {
        self.state.lock().query_behaviors.push_back(behavior);
    }

    fn set_provider_progress(&self, next_offset: u64, object_exists: bool) {
        let mut state = self.state.lock();
        state.next_offset = next_offset;
        state.object_exists = object_exists;
    }

    fn snapshot(&self) -> MockProviderSnapshot {
        let state = self.state.lock();
        MockProviderSnapshot {
            next_offset: state.next_offset,
            object_exists: state.object_exists,
            fragment_calls: state.fragment_calls.clone(),
            query_calls: state.query_calls,
            abort_calls: state.abort_calls,
            delete_calls: state.delete_calls,
        }
    }

    fn commit_fragment(
        state: &mut MockProviderState,
        start: u64,
        fragment_size: i64,
    ) -> aster_drive_storage::Result<ProviderResumableUploadFragmentOutcome> {
        if start != state.next_offset {
            return Err(storage_driver_error(
                StorageErrorKind::Precondition,
                format!(
                    "mock provider expected offset {}, got {start}",
                    state.next_offset
                ),
            ));
        }
        let size = u64::try_from(fragment_size).map_err(|_| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "invalid mock fragment size",
            )
        })?;
        state.next_offset = state.next_offset.checked_add(size).ok_or_else(|| {
            storage_driver_error(StorageErrorKind::Misconfigured, "mock offset overflow")
        })?;
        if state.next_offset > state.total_size {
            return Err(storage_driver_error(
                StorageErrorKind::Precondition,
                "mock fragment exceeds total size",
            ));
        }
        let completed = state.next_offset == state.total_size;
        if completed {
            state.object_exists = true;
        }
        Ok(ProviderResumableUploadFragmentOutcome {
            completed,
            next_expected_ranges: if !completed {
                vec![format!("{}-", state.next_offset)]
            } else {
                Default::default()
            },
        })
    }
}

#[derive(Debug)]
struct MockProviderSnapshot {
    next_offset: u64,
    object_exists: bool,
    fragment_calls: Vec<FragmentCall>,
    query_calls: usize,
    abort_calls: usize,
    delete_calls: usize,
}

#[async_trait]
impl StorageDriver for MockProviderDriver {
    async fn put(&self, path: &str, _data: &[u8]) -> aster_drive_storage::Result<String> {
        Ok(path.to_string())
    }

    async fn get(&self, _path: &str) -> aster_drive_storage::Result<Vec<u8>> {
        Ok(Vec::new())
    }

    async fn get_stream(
        &self,
        _path: &str,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        Ok(Box::new(tokio::io::empty()))
    }

    async fn delete(&self, _path: &str) -> aster_drive_storage::Result<()> {
        let mut state = self.state.lock();
        state.delete_calls += 1;
        state.object_exists = false;
        Ok(())
    }

    async fn exists(&self, _path: &str) -> aster_drive_storage::Result<bool> {
        Ok(self.state.lock().object_exists)
    }

    async fn metadata(&self, _path: &str) -> aster_drive_storage::Result<BlobMetadata> {
        let state = self.state.lock();
        if !state.object_exists {
            return Err(storage_driver_error(
                StorageErrorKind::NotFound,
                "mock object is absent",
            ));
        }
        Ok(BlobMetadata {
            size: state.total_size,
            content_type: None,
        })
    }

    fn extensions(&self) -> StorageDriverExtensions<'_> {
        StorageDriverExtensions {
            provider_resumable: Some(self),
            ..Default::default()
        }
    }
}

#[async_trait]
impl ProviderResumableUploadDriver for MockProviderDriver {
    fn provider_resumable_upload_capabilities(&self) -> ProviderResumableUploadCapabilities {
        ProviderResumableUploadCapabilities {
            provider: "mock_graph",
            session_label: "mock upload session",
            min_fragment_size: 1,
            default_fragment_size: 5,
            max_fragment_size: 50,
            fragment_alignment: 1,
            max_simple_upload_size: None,
            frontend_direct_upload: true,
            implicit_completion: true,
            abort_supported: true,
            status_query_supported: true,
        }
    }

    async fn create_upload_session(
        &self,
        _path: &str,
    ) -> aster_drive_storage::Result<ProviderResumableUploadSession> {
        Ok(ProviderResumableUploadSession {
            upload_url: "https://mock.invalid/upload?secret=redacted".to_string(),
            expires_at: None,
            next_expected_ranges: vec!["0-".to_string()],
        })
    }

    async fn query_upload_session(
        &self,
        _upload_url: &str,
    ) -> aster_drive_storage::Result<ProviderResumableUploadStatus> {
        let behavior = {
            let mut state = self.state.lock();
            state.query_calls += 1;
            state
                .query_behaviors
                .pop_front()
                .unwrap_or(QueryBehavior::Default)
        };
        match behavior {
            QueryBehavior::Default => {
                let state = self.state.lock();
                if state.object_exists && state.next_offset == state.total_size {
                    return Err(storage_driver_error(
                        StorageErrorKind::NotFound,
                        "mock upload session completed",
                    ));
                }
                Ok(ProviderResumableUploadStatus {
                    expires_at: None,
                    next_expected_ranges: vec![format!("{}-", state.next_offset)],
                })
            }
            QueryBehavior::Offset(offset) => Ok(ProviderResumableUploadStatus {
                expires_at: None,
                next_expected_ranges: vec![format!("{offset}-")],
            }),
            QueryBehavior::NotFound => Err(storage_driver_error(
                StorageErrorKind::NotFound,
                "mock upload session missing",
            )),
            QueryBehavior::Error => Err(storage_driver_error(
                StorageErrorKind::Transient,
                "mock query failed",
            )),
        }
    }

    async fn abort_upload_session(&self, _upload_url: &str) -> aster_drive_storage::Result<()> {
        self.state.lock().abort_calls += 1;
        Ok(())
    }

    async fn upload_session_fragment_reader(
        &self,
        _upload_url: &str,
        start: u64,
        total_size: u64,
        mut reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        fragment_size: i64,
    ) -> aster_drive_storage::Result<ProviderResumableUploadFragmentOutcome> {
        let behavior = self
            .state
            .lock()
            .fragment_behaviors
            .pop_front()
            .unwrap_or(FragmentBehavior::Success);
        let mut body = Vec::new();
        reader
            .read_to_end(&mut body)
            .await
            .map_err(|error| AsterError::storage_driver_error(error.to_string()))?;
        self.state.lock().fragment_calls.push(FragmentCall {
            start,
            total_size,
            fragment_size,
            body: body.clone(),
        });

        if body.len() != usize::try_from(fragment_size).unwrap_or(usize::MAX) {
            return Err(storage_driver_error(
                StorageErrorKind::Transient,
                "mock fragment body ended early",
            ));
        }

        if let FragmentBehavior::Wait { started, release } = &behavior {
            started.notify_one();
            release.notified().await;
        }

        match behavior {
            FragmentBehavior::Success | FragmentBehavior::Wait { .. } => {
                Self::commit_fragment(&mut self.state.lock(), start, fragment_size)
            }
            FragmentBehavior::FailBeforeCommit => Err(storage_driver_error(
                StorageErrorKind::Transient,
                "mock fragment failed before commit",
            )),
            FragmentBehavior::FailAfterCommit => {
                Self::commit_fragment(&mut self.state.lock(), start, fragment_size)?;
                Err(storage_driver_error(
                    StorageErrorKind::Transient,
                    "mock fragment response was lost after commit",
                ))
            }
            FragmentBehavior::ReturnOffset(offset) => Ok(ProviderResumableUploadFragmentOutcome {
                completed: false,
                next_expected_ranges: vec![format!("{offset}-")],
            }),
        }
    }
}

struct Fixture {
    state: Arc<PrimaryAppState>,
    session: upload_session::Model,
    context: ProviderRelayContext,
    driver: Arc<MockProviderDriver>,
}

impl Fixture {
    async fn new(behaviors: impl IntoIterator<Item = FragmentBehavior>) -> Self {
        let state = crate::runtime::tasks::test_support::setup_primary_state().await;
        let state = Arc::new(state.get_ref().clone());
        let now = Utc::now();
        let user = user::ActiveModel {
            username: Set(format!("relay-test-{}", uuid::Uuid::new_v4())),
            email: Set(format!("relay-{}@test.invalid", uuid::Uuid::new_v4())),
            password_hash: Set("test".to_string()),
            role: Set(UserRole::User),
            status: Set(UserStatus::Active),
            must_change_password: Set(false),
            session_version: Set(0),
            email_verified_at: Set(Some(now)),
            pending_email: Set(None),
            storage_used: Set(0),
            storage_quota: Set(0),
            policy_group_id: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            config: Set(None),
            ..Default::default()
        }
        .insert(state.writer_db())
        .await
        .expect("test user should insert");
        let mut policy = crate::storage::connectors::test_support::onedrive_policy(
            crate::storage::connectors::OneDriveAccountMode::Personal,
            None,
            None,
            None,
            aster_drive_storage::StoragePolicyBehaviorConfig::default(),
        );
        policy.name = format!("relay-policy-{}", uuid::Uuid::new_v4());
        policy.is_default = true;
        policy.chunk_size = 5;
        let policy = crate::storage::connectors::test_support::insertable_policy(policy)
            .insert(state.writer_db())
            .await
            .expect("test policy should insert");

        let upload_id = uuid::Uuid::new_v4().to_string();
        let upload_url = "https://mock.invalid/upload?secret=redacted".to_string();
        let ciphertext = encrypt_provider_session(
            state.as_ref(),
            &upload_id,
            &ProviderSessionSecret {
                provider: "mock_graph".to_string(),
                upload_url: upload_url.clone(),
            },
        )
        .expect("provider session should encrypt");
        let session = upload_session_repo::create(
            state.writer_db(),
            upload_session::ActiveModel {
                id: Set(upload_id),
                user_id: Set(user.id),
                team_id: Set(None),
                frontend_client_id: Set(None),
                filename: Set("relay.bin".to_string()),
                total_size: Set(10),
                chunk_size: Set(5),
                total_chunks: Set(2),
                received_count: Set(0),
                folder_id: Set(None),
                policy_id: Set(policy.id),
                status: Set(UploadSessionStatus::Uploading),
                session_kind: Set(UploadSessionKind::ProviderRelayResumable),
                object_temp_key: Set(Some("files/mock/relay.bin".to_string())),
                object_multipart_id: Set(None),
                provider_session_ciphertext: Set(Some(ciphertext)),
                file_id: Set(None),
                created_at: Set(now),
                expires_at: Set(now + Duration::hours(1)),
                updated_at: Set(now),
            },
        )
        .await
        .expect("test upload session should insert");
        let driver = Arc::new(MockProviderDriver::new(10, behaviors));
        let context = ProviderRelayContext {
            driver: driver.clone(),
            upload_url,
            temp_key: "files/mock/relay.bin".to_string(),
        };

        Self {
            state,
            session,
            context,
            driver,
        }
    }

    async fn current_session(&self) -> upload_session::Model {
        upload_session_repo::find_by_id(self.state.writer_db(), &self.session.id)
            .await
            .expect("session should exist")
    }

    async fn part(&self, part_number: i32) -> Option<upload_session_part::Model> {
        upload_session_part_repo::find_by_upload_and_part(
            self.state.writer_db(),
            &self.session.id,
            part_number,
        )
        .await
        .expect("part lookup should succeed")
    }
}

async fn payload_from_chunks(chunks: &[&'static [u8]]) -> actix_web::web::Payload {
    let request = actix_web::test::TestRequest::default().to_http_request();
    let (mut sender, payload) = actix_http::h1::Payload::create(false);
    for chunk in chunks {
        sender.feed_data(Bytes::from_static(chunk));
    }
    sender.feed_eof();
    let mut payload = actix_http::Payload::from(payload);
    actix_web::web::Payload::from_request(&request, &mut payload)
        .await
        .expect("test payload should extract")
}

fn expect_upload_error(result: Result<ChunkUploadResponse>, message: &str) -> AsterError {
    match result {
        Ok(_) => panic!("{message}"),
        Err(error) => error,
    }
}

#[test]
fn parses_provider_next_expected_offset_boundaries() {
    for (range, expected) in [("0-", 0), ("5-9", 5), ("18446744073709551615-", u64::MAX)] {
        assert_eq!(
            next_expected_offset(&[range.to_string()]).unwrap(),
            expected
        );
    }
    for ranges in [
        Vec::<String>::new(),
        vec![String::new()],
        vec!["-5".to_string()],
        vec![" 5-".to_string()],
        vec!["18446744073709551616-".to_string()],
    ] {
        assert!(next_expected_offset(&ranges).is_err(), "ranges={ranges:?}");
    }
}

#[tokio::test]
async fn relay_pipe_accepts_exact_body_and_rejects_short_or_long_body() {
    for (chunks, expected_size, succeeds) in [
        (vec![&b"ab"[..], &b"cd"[..]], 4_i64, true),
        (vec![&b"ab"[..]], 4_i64, false),
        (vec![&b"abcd"[..], &b"ef"[..]], 4_i64, false),
    ] {
        let payload = payload_from_chunks(&chunks).await;
        let (mut reader, writer) = tokio::io::duplex(16);
        let read_task = tokio::spawn(async move {
            let mut data = Vec::new();
            reader.read_to_end(&mut data).await.map(|_| data)
        });
        let result = pipe_payload(payload, writer, expected_size, 0).await;
        assert_eq!(result.is_ok(), succeeds);
        let relayed = read_task
            .await
            .expect("reader task should join")
            .expect("reader should finish");
        if succeeds {
            assert_eq!(relayed, b"abcd");
        }
    }
}

#[tokio::test]
async fn full_upload_is_sequential_idempotent_and_completes_final_object() {
    let fixture = Fixture::new([FragmentBehavior::Success, FragmentBehavior::Success]).await;

    let first = upload_bytes_with_context(
        fixture.state.as_ref(),
        fixture.session.clone(),
        0,
        Bytes::from_static(b"abcde"),
        &fixture.context,
    )
    .await
    .expect("first range should upload");
    assert_eq!(first.received_count, 1);

    let duplicate = upload_bytes_with_context(
        fixture.state.as_ref(),
        fixture.current_session().await,
        0,
        Bytes::from_static(b"abcde"),
        &fixture.context,
    )
    .await
    .expect("accepted range retry should be idempotent");
    assert_eq!(duplicate.received_count, 1);

    let final_response = upload_bytes_with_context(
        fixture.state.as_ref(),
        fixture.current_session().await,
        1,
        Bytes::from_static(b"fghij"),
        &fixture.context,
    )
    .await
    .expect("final range should upload");
    assert_eq!(final_response.received_count, 2);

    let snapshot = fixture.driver.snapshot();
    assert_eq!(snapshot.next_offset, 10);
    assert!(snapshot.object_exists);
    assert_eq!(snapshot.fragment_calls.len(), 2);
    assert_eq!(snapshot.fragment_calls[0].start, 0);
    assert_eq!(snapshot.fragment_calls[0].body, b"abcde");
    assert_eq!(snapshot.fragment_calls[1].start, 5);
    assert_eq!(snapshot.fragment_calls[1].body, b"fghij");
    assert_eq!(fixture.part(1).await.unwrap().etag, PROVIDER_RANGE_RECEIPT);
    assert_eq!(fixture.part(2).await.unwrap().etag, PROVIDER_RANGE_RECEIPT);
}

#[tokio::test]
async fn invalid_expired_and_out_of_order_ranges_never_reach_provider() {
    let fixture = Fixture::new([]).await;

    for chunk_number in [-1, 2, 1] {
        let error = expect_upload_error(
            upload_bytes_with_context(
                fixture.state.as_ref(),
                fixture.session.clone(),
                chunk_number,
                Bytes::from_static(b"abcde"),
                &fixture.context,
            )
            .await,
            "invalid or later ranges should be rejected",
        );
        assert!(matches!(
            error.api_error_code(),
            ApiErrorCode::UploadChunkNumberOutOfRange | ApiErrorCode::UploadChunkSessionInvalid
        ));
    }

    let mut expired = fixture.session.clone();
    expired.expires_at = Utc::now() - Duration::seconds(1);
    assert!(
        upload_bytes_with_context(
            fixture.state.as_ref(),
            expired,
            0,
            Bytes::from_static(b"abcde"),
            &fixture.context,
        )
        .await
        .is_err()
    );
    let mut failed = fixture.session.clone();
    failed.status = UploadSessionStatus::Failed;
    assert!(
        upload_bytes_with_context(
            fixture.state.as_ref(),
            failed,
            0,
            Bytes::from_static(b"abcde"),
            &fixture.context,
        )
        .await
        .is_err()
    );
    assert!(fixture.driver.snapshot().fragment_calls.is_empty());
}

#[tokio::test]
async fn failure_before_commit_releases_claim_and_retry_uploads_once() {
    let fixture = Fixture::new([FragmentBehavior::FailBeforeCommit]).await;
    let error = expect_upload_error(
        upload_bytes_with_context(
            fixture.state.as_ref(),
            fixture.session.clone(),
            0,
            Bytes::from_static(b"abcde"),
            &fixture.context,
        )
        .await,
        "pre-commit failure should surface",
    );
    assert_eq!(
        error.storage_error_kind(),
        Some(StorageErrorKind::Transient)
    );
    assert!(fixture.part(1).await.is_none(), "claim should be released");
    assert_eq!(fixture.current_session().await.received_count, 0);

    fixture
        .driver
        .push_fragment_behavior(FragmentBehavior::Success);
    let response = upload_bytes_with_context(
        fixture.state.as_ref(),
        fixture.current_session().await,
        0,
        Bytes::from_static(b"abcde"),
        &fixture.context,
    )
    .await
    .expect("retry should claim and upload the same range");
    assert_eq!(response.received_count, 1);
    assert_eq!(fixture.driver.snapshot().fragment_calls.len(), 2);
}

#[tokio::test]
async fn ambiguous_failure_after_commit_is_reconciled_without_duplicate_put() {
    let fixture = Fixture::new([FragmentBehavior::FailAfterCommit]).await;
    let response = upload_bytes_with_context(
        fixture.state.as_ref(),
        fixture.session.clone(),
        0,
        Bytes::from_static(b"abcde"),
        &fixture.context,
    )
    .await
    .expect("provider progress should prove the ambiguous range committed");

    assert_eq!(response.received_count, 1);
    let snapshot = fixture.driver.snapshot();
    assert_eq!(snapshot.fragment_calls.len(), 1);
    assert_eq!(snapshot.query_calls, 1);
    assert_eq!(fixture.part(1).await.unwrap().etag, PROVIDER_RANGE_RECEIPT);
}

#[tokio::test]
async fn ambiguous_failure_with_query_error_preserves_claim_for_later_reconciliation() {
    let fixture = Fixture::new([FragmentBehavior::FailAfterCommit]).await;
    fixture.driver.push_query_behavior(QueryBehavior::Error);
    let error = expect_upload_error(
        upload_bytes_with_context(
            fixture.state.as_ref(),
            fixture.session.clone(),
            0,
            Bytes::from_static(b"abcde"),
            &fixture.context,
        )
        .await,
        "unqueryable ambiguous result should remain pending",
    );
    assert_eq!(
        error.storage_error_kind(),
        Some(StorageErrorKind::Transient)
    );
    assert_eq!(fixture.part(1).await.unwrap().etag, "");
    assert_eq!(fixture.current_session().await.received_count, 0);

    let response = upload_bytes_with_context(
        fixture.state.as_ref(),
        fixture.current_session().await,
        0,
        Bytes::from_static(b"abcde"),
        &fixture.context,
    )
    .await
    .expect("later retry should reconcile the provider-accepted range");
    assert_eq!(response.received_count, 1);
    assert_eq!(fixture.driver.snapshot().fragment_calls.len(), 1);
}

#[tokio::test]
async fn active_claim_blocks_second_primary_until_first_put_finishes() {
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let fixture = Fixture::new([FragmentBehavior::Wait {
        started: started.clone(),
        release: release.clone(),
    }])
    .await;
    let state = fixture.state.clone();
    let session = fixture.session.clone();
    let context = fixture.context.clone();
    let first = tokio::spawn(async move {
        upload_bytes_with_context(
            state.as_ref(),
            session,
            0,
            Bytes::from_static(b"abcde"),
            &context,
        )
        .await
    });
    started.notified().await;

    let short_payload = expect_upload_error(
        upload_payload_with_context(
            fixture.state.as_ref(),
            fixture.current_session().await,
            0,
            payload_from_chunks(&[b"abc"]).await,
            &fixture.context,
        )
        .await,
        "pending claims should still validate the full request body",
    );
    assert_eq!(
        short_payload.api_error_code(),
        ApiErrorCode::UploadChunkSizeMismatch
    );

    let second = expect_upload_error(
        upload_payload_with_context(
            fixture.state.as_ref(),
            fixture.current_session().await,
            0,
            payload_from_chunks(&[b"abcde"]).await,
            &fixture.context,
        )
        .await,
        "active shared claim should reject concurrent upload after draining the payload",
    );
    assert_eq!(second.api_error_code(), ApiErrorCode::UploadChunkPending);
    assert!(second.api_error_info().retryable);
    assert_eq!(second.http_status(), actix_web::http::StatusCode::ACCEPTED);
    assert_eq!(fixture.driver.snapshot().fragment_calls.len(), 1);

    release.notify_one();
    assert_eq!(
        first
            .await
            .expect("first upload task should join")
            .expect("first upload should finish")
            .received_count,
        1
    );
}

#[tokio::test]
async fn fragment_timeout_precedes_claim_staleness_and_drops_pending_upload() {
    struct DropMarker(Arc<AtomicBool>);

    impl Drop for DropMarker {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    assert!(FRAGMENT_UPLOAD_TIMEOUT < CLAIM_STALE_AFTER);
    let fixture = Fixture::new([]).await;
    let dropped = Arc::new(AtomicBool::new(false));
    let marker = DropMarker(dropped.clone());
    let upload = async move {
        let _marker = marker;
        std::future::pending::<Result<ProviderResumableUploadFragmentOutcome>>().await
    };

    let error = upload_with_claim_heartbeat_timeout(
        fixture.state.as_ref(),
        &fixture.session.id,
        0,
        std::time::Duration::from_millis(10),
        upload,
    )
    .await
    .expect_err("a pending fragment upload should hit its independent deadline");

    assert_eq!(
        error.storage_error_kind(),
        Some(StorageErrorKind::Transient)
    );
    assert!(dropped.load(Ordering::SeqCst));
}

#[tokio::test]
async fn stale_claim_is_recovered_only_when_provider_still_expects_same_offset() {
    let fixture = Fixture::new([FragmentBehavior::Success]).await;
    assert!(
        upload_session_part_repo::try_claim_part(
            fixture.state.writer_db(),
            &fixture.session.id,
            1,
        )
        .await
        .unwrap()
    );
    upload_session_part::Entity::update_many()
        .col_expr(
            upload_session_part::Column::UpdatedAt,
            Expr::value(Utc::now() - Duration::minutes(10)),
        )
        .filter(upload_session_part::Column::UploadId.eq(&fixture.session.id))
        .filter(upload_session_part::Column::PartNumber.eq(1))
        .exec(fixture.state.writer_db())
        .await
        .expect("claim timestamp should age");

    let response = upload_bytes_with_context(
        fixture.state.as_ref(),
        fixture.session.clone(),
        0,
        Bytes::from_static(b"abcde"),
        &fixture.context,
    )
    .await
    .expect("stale unchanged claim should be reclaimed");
    assert_eq!(response.received_count, 1);

    let inconsistent = Fixture::new([]).await;
    assert!(
        upload_session_part_repo::try_claim_part(
            inconsistent.state.writer_db(),
            &inconsistent.session.id,
            1,
        )
        .await
        .unwrap()
    );
    inconsistent
        .driver
        .push_query_behavior(QueryBehavior::Offset(3));
    let error = expect_upload_error(
        upload_bytes_with_context(
            inconsistent.state.as_ref(),
            inconsistent.session.clone(),
            0,
            Bytes::from_static(b"abcde"),
            &inconsistent.context,
        )
        .await,
        "partial provider progress should preserve the claim",
    );
    assert_eq!(error.api_error_code(), ApiErrorCode::UploadSessionCorrupted);
    assert_eq!(inconsistent.part(1).await.unwrap().etag, "");
}

#[tokio::test]
async fn progress_rebuilds_all_receipts_after_final_session_disappears() {
    let fixture = Fixture::new([]).await;
    for part_number in [1, 2] {
        assert!(
            upload_session_part_repo::try_claim_part(
                fixture.state.writer_db(),
                &fixture.session.id,
                part_number,
            )
            .await
            .unwrap()
        );
    }
    fixture.driver.set_provider_progress(10, true);

    let completed =
        reconcile_progress_with_context(fixture.state.as_ref(), &fixture.session, &fixture.context)
            .await
            .expect("404 plus final object should reconcile every accepted range");
    assert_eq!(completed, vec![0, 1]);
    assert_eq!(fixture.current_session().await.received_count, 2);
    assert_eq!(fixture.part(1).await.unwrap().etag, PROVIDER_RANGE_RECEIPT);
    assert_eq!(fixture.part(2).await.unwrap().etag, PROVIDER_RANGE_RECEIPT);
}

#[tokio::test]
async fn payload_size_errors_preserve_provider_truth() {
    let short = Fixture::new([FragmentBehavior::Success]).await;
    let error = expect_upload_error(
        upload_payload_with_context(
            short.state.as_ref(),
            short.session.clone(),
            0,
            payload_from_chunks(&[b"abc"]).await,
            &short.context,
        )
        .await,
        "short body should fail",
    );
    assert_eq!(
        error.api_error_code(),
        ApiErrorCode::UploadChunkSizeMismatch
    );
    assert_eq!(short.current_session().await.received_count, 0);
    assert!(short.part(1).await.is_none());

    let long = Fixture::new([FragmentBehavior::Success]).await;
    let error = expect_upload_error(
        upload_payload_with_context(
            long.state.as_ref(),
            long.session.clone(),
            0,
            payload_from_chunks(&[b"abcde", b"f"]).await,
            &long.context,
        )
        .await,
        "long body should report the request mismatch",
    );
    assert_eq!(
        error.api_error_code(),
        ApiErrorCode::UploadChunkSizeMismatch
    );
    assert_eq!(long.current_session().await.received_count, 1);
    assert_eq!(long.part(1).await.unwrap().etag, PROVIDER_RANGE_RECEIPT);
    assert_eq!(long.driver.snapshot().fragment_calls[0].body, b"abcde");
}

#[tokio::test]
async fn inconsistent_success_response_keeps_claim_for_operator_reconciliation() {
    let fixture = Fixture::new([FragmentBehavior::ReturnOffset(3)]).await;
    let error = expect_upload_error(
        upload_bytes_with_context(
            fixture.state.as_ref(),
            fixture.session.clone(),
            0,
            Bytes::from_static(b"abcde"),
            &fixture.context,
        )
        .await,
        "provider response ending inside the range is inconsistent",
    );
    assert_eq!(error.api_error_code(), ApiErrorCode::UploadSessionCorrupted);
    assert_eq!(fixture.current_session().await.received_count, 0);
    assert_eq!(fixture.part(1).await.unwrap().etag, "");
}

#[tokio::test]
async fn explicit_query_not_found_requires_final_object_to_exist() {
    let fixture = Fixture::new([]).await;
    fixture.driver.push_query_behavior(QueryBehavior::NotFound);
    let error =
        reconcile_progress_with_context(fixture.state.as_ref(), &fixture.session, &fixture.context)
            .await
            .expect_err("missing session without object must remain an error");
    assert_eq!(error.storage_error_kind(), Some(StorageErrorKind::NotFound));
    let snapshot = fixture.driver.snapshot();
    assert_eq!(snapshot.abort_calls, 0);
    assert_eq!(snapshot.delete_calls, 0);
}
