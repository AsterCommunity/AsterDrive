use crate::config::{Config, DatabaseConfig, RuntimeConfig};
use crate::db::repository::{file_repo, folder_repo};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{mail::sender, storage_policy::policy};
use crate::storage::{DriverRegistry, PolicySnapshot};
use crate::webdav::backend::AsterDavFs;
use crate::webdav::handlers::properties::handle_propfind;
use crate::webdav::handlers::transfer::{handle_get_head, handle_put};
use actix_web::body::to_bytes;
use actix_web::http::{StatusCode, header};
use actix_web::{FromRequest, HttpRequest, web};
use aster_drive_migration::Migrator;
use aster_drive_model::entities::{file, file_blob, folder as folder_entity, storage_policy, user};
use aster_drive_model::types::{
    DriverType, ObjectStorageUploadStrategy, StoragePolicyOptions, StoredStoragePolicyAllowedTypes,
    UserRole, UserStatus, serialize_storage_policy_options,
};
use aster_drive_storage::{BlobMetadata, StorageDriver, StreamUploadDriver};
use aster_forge_cache as cache;
use aster_forge_cache::CacheConfig;
use aster_forge_webdav::{
    DavBackendError, DavEvent, DavEventSink, DavLock, DavLockAcquireRequest, DavLockError,
    DavLockSystem, DavMethod, DavMutationCredentials, DavMutationOperation, DavMutationTargetRole,
    DavPath, FsError, LsFuture,
};
use aster_forge_webdav::{DavXmlElement as Element, DavXmlNode as XMLNode};
use async_trait::async_trait;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, Set};
use std::collections::HashMap;
use std::io::{self, Cursor};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, ReadBuf};

fn parsed_request_head(req: &HttpRequest) -> aster_forge_webdav::DavRequestHead {
    aster_forge_webdav::actix::request_head(req, "/webdav")
        .expect("test request head should parse")
        .expect("test method should be supported")
}

fn capability_snapshot(
    resource: aster_forge_webdav::DavResourceState,
) -> aster_forge_webdav::DavCapabilitySnapshot {
    crate::webdav::capability::DriveDavCapabilityProvider::snapshot_for(resource)
        .expect("test capability declaration should be valid")
}

#[derive(Default)]
struct CapturingDavEventSink {
    events: Mutex<Vec<DavEvent>>,
}

impl DavEventSink for CapturingDavEventSink {
    fn publish(&self, event: &DavEvent) -> Result<(), aster_forge_webdav::DavObservationError> {
        self.events
            .lock()
            .expect("event sink should lock")
            .push(event.clone());
        Ok(())
    }
}

async fn build_webdav_test_state(
    driver_type: DriverType,
    options: aster_drive_model::types::StoredStoragePolicyOptions,
    driver: Arc<dyn StorageDriver>,
) -> (PrimaryAppState, user::Model, storage_policy::Model, PathBuf) {
    let temp_root = std::env::temp_dir().join(format!(
        "asterdrive-webdav-handler-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp_root).expect("webdav handler temp root should exist");

    let db = crate::db::connect_with_metrics(
        &DatabaseConfig {
            url: "sqlite::memory:".into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .expect("webdav handler database should connect");
    Migrator::up(&db, None)
        .await
        .expect("webdav handler migrations should succeed");

    let now = Utc::now();
    let policy = storage_policy::ActiveModel {
        name: Set("WebDAV Test Policy".to_string()),
        driver_type: Set(driver_type),
        endpoint: Set("https://mock-storage.example".to_string()),
        bucket: Set("mock-bucket".to_string()),
        access_key: Set("mock-access".to_string()),
        secret_key: Set("mock-secret".to_string()),
        base_path: Set(temp_root.to_string_lossy().into_owned()),
        max_file_size: Set(0),
        allowed_types: Set(StoredStoragePolicyAllowedTypes::empty()),
        options: Set(options),
        is_default: Set(true),
        chunk_size: Set(5_242_880),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("webdav handler policy should be inserted");

    let user = user::ActiveModel {
        username: Set("davhdl".to_string()),
        email: Set("davhdl@example.com".to_string()),
        password_hash: Set("unused".to_string()),
        role: Set(UserRole::User),
        status: Set(UserStatus::Active),
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
    .insert(&db)
    .await
    .expect("webdav handler user should be inserted");

    policy::ensure_policy_groups_seeded(&db)
        .await
        .expect("webdav handler policy groups should be seeded");

    let policy_snapshot = Arc::new(PolicySnapshot::new());
    policy_snapshot
        .reload(&db)
        .await
        .expect("webdav handler policy snapshot should reload");

    let driver_registry = Arc::new(DriverRegistry::noop());
    driver_registry.insert_for_test(policy.id, driver);

    let runtime_config = Arc::new(RuntimeConfig::new());
    let cache = cache::create_cache(&CacheConfig {
        ..Default::default()
    })
    .await;

    let mut config = Config::default();
    config.server.temp_dir = temp_root.join(".tmp").to_string_lossy().into_owned();
    config.server.upload_temp_dir = temp_root.join(".uploads").to_string_lossy().into_owned();

    let storage_change_bus = crate::services::events::storage_change::StorageChangeBus::new(
        crate::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
    );
    let share_download_rollback =
        crate::services::share::spawn_detached_share_download_rollback_queue(
            db.clone(),
            crate::config::operations::share_download_rollback_queue_capacity(&runtime_config),
        );

    let state = PrimaryAppState {
        db_handles: aster_forge_db::DbHandles::single(db.clone()),
        driver_registry,
        runtime_config: runtime_config.clone(),
        policy_snapshot,
        config: Arc::new(config),
        cache,
        config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
        metrics: aster_drive_metrics::NoopMetrics::arc(),
        mail_sender: sender::runtime_sender(runtime_config),
        storage_change_bus,
        share_download_rollback,
        background_task_dispatch_wakeup:
            crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
        remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
    };

    (state, user, policy, temp_root)
}

async fn create_root_file(
    state: &PrimaryAppState,
    user_id: i64,
    policy_id: i64,
    filename: &str,
    size: i64,
    storage_path: &str,
) -> (file::Model, file_blob::Model) {
    let now = Utc::now();
    let blob = file_repo::create_blob(
        state.writer_db(),
        file_blob::ActiveModel {
            hash: Set(format!("webdav-blob-{}", uuid::Uuid::new_v4())),
            size: Set(size),
            policy_id: Set(policy_id),
            storage_path: Set(storage_path.to_string()),
            ref_count: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .expect("webdav handler blob should be inserted");

    let file = file_repo::create(
        state.writer_db(),
        file::ActiveModel {
            name: Set(filename.to_string()),
            folder_id: Set(None),
            team_id: Set(None),
            blob_id: Set(blob.id),
            size: Set(size),
            owner_user_id: Set(Some(user_id)),
            created_by_user_id: Set(Some(user_id)),
            created_by_username: Set("tester".to_string()),
            mime_type: Set("text/plain".to_string()),
            created_at: Set(now),
            updated_at: Set(now),
            deleted_at: Set(None),
            ..Default::default()
        },
    )
    .await
    .expect("webdav handler file should be inserted");

    (file, blob)
}

async fn create_test_folder(
    state: &PrimaryAppState,
    user: &user::Model,
    name: &str,
    parent_id: Option<i64>,
) -> folder_entity::Model {
    let now = Utc::now();
    folder_entity::ActiveModel {
        name: Set(name.to_string()),
        parent_id: Set(parent_id),
        team_id: Set(None),
        owner_user_id: Set(Some(user.id)),
        created_by_user_id: Set(Some(user.id)),
        created_by_username: Set(user.username.clone()),
        policy_id: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("test folder should insert")
}

async fn create_file_in_folder(
    state: &PrimaryAppState,
    user_id: i64,
    policy_id: i64,
    folder_id: i64,
    filename: &str,
) -> file::Model {
    let (created, _) = create_root_file(
        state,
        user_id,
        policy_id,
        filename,
        1,
        &format!("files/{}", uuid::Uuid::new_v4()),
    )
    .await;
    file::ActiveModel {
        id: Set(created.id),
        folder_id: Set(Some(folder_id)),
        ..Default::default()
    }
    .update(state.writer_db())
    .await
    .expect("test file should move into its folder")
}

fn mutation_conditions<'a>(
    headers: &'a http::HeaderMap,
    method: DavMethod,
    target: &'a DavPath,
) -> crate::webdav::backend::DavMutationConditions<'a> {
    crate::webdav::backend::DavMutationConditions {
        prefix: "/webdav",
        if_header: None,
        request_scheme: "http",
        request_host: "localhost",
        http_headers: headers,
        http_method: method,
        http_target: target,
    }
}

fn assert_forbidden_mutation(result: Result<(), crate::webdav::backend::AsterDavMutationError>) {
    let is_forbidden = matches!(
        &result,
        Err(crate::webdav::backend::AsterDavMutationError::FileSystem(
            FsError::Forbidden
        ))
    );
    assert!(
        is_forbidden,
        "expected FileSystem(Forbidden), got {result:?}"
    );
}

fn assert_locked_mutation(
    result: Result<(), crate::webdav::backend::AsterDavMutationError>,
    expected_lock_root: &DavPath,
) {
    let is_expected_lock = matches!(
        &result,
        Err(crate::webdav::backend::AsterDavMutationError::Locked(lock_root))
            if lock_root == expected_lock_root
    );
    assert!(
        is_expected_lock,
        "expected lock conflict at {expected_lock_root:?}, got {result:?}"
    );
}

#[derive(Clone, Default)]
struct NoopLockSystem {
    discover_many_calls: Arc<AtomicUsize>,
    delay_from_discover_many_call: Option<(usize, Duration)>,
}

impl NoopLockSystem {
    fn delaying_from(call: usize, delay: Duration) -> Self {
        Self {
            discover_many_calls: Arc::new(AtomicUsize::new(0)),
            delay_from_discover_many_call: Some((call, delay)),
        }
    }
}

impl DavLockSystem for NoopLockSystem {
    fn lock(
        &self,
        _request: aster_forge_webdav::DavLockAcquireRequest<'_>,
    ) -> LsFuture<'_, Result<aster_forge_webdav::DavLockAcquireResult, DavLockError>> {
        Box::pin(async { panic!("lock should not be called in these WebDAV handler tests") })
    }

    fn unlock(
        &self,
        _path: &aster_forge_webdav::DavPath,
        _token: &str,
    ) -> LsFuture<'_, Result<(), DavLockError>> {
        Box::pin(async { Ok(()) })
    }

    fn refresh(
        &self,
        _path: &aster_forge_webdav::DavPath,
        _token: &str,
        _timeout: Option<Duration>,
    ) -> LsFuture<'_, Result<DavLock, DavLockError>> {
        Box::pin(async { panic!("refresh should not be called in these WebDAV handler tests") })
    }

    fn check(
        &self,
        _path: &aster_forge_webdav::DavPath,
        _principal: Option<&str>,
        _ignore_principal: bool,
        _deep: bool,
        _submitted_tokens: &[String],
    ) -> LsFuture<'_, Result<(), DavLockError>> {
        Box::pin(async { Ok(()) })
    }

    fn discover(
        &self,
        _path: &aster_forge_webdav::DavPath,
    ) -> LsFuture<'_, Result<Vec<DavLock>, DavBackendError>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn discover_many<'a>(
        &'a self,
        paths: &'a [aster_forge_webdav::DavPath],
    ) -> LsFuture<'a, Result<HashMap<aster_forge_webdav::DavPath, Vec<DavLock>>, DavBackendError>>
    {
        Box::pin(async move {
            let call = self.discover_many_calls.fetch_add(1, Ordering::SeqCst) + 1;
            if let Some((delay_from, delay)) = self.delay_from_discover_many_call
                && call >= delay_from
            {
                tokio::time::sleep(delay).await;
            }
            Ok(paths
                .iter()
                .cloned()
                .map(|path| (path, Vec::new()))
                .collect())
        })
    }

    fn conflicting_locks(
        &self,
        _path: &aster_forge_webdav::DavPath,
        _deep: bool,
    ) -> LsFuture<'_, Result<Vec<DavLock>, DavBackendError>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn delete(
        &self,
        _path: &aster_forge_webdav::DavPath,
    ) -> LsFuture<'_, Result<(), DavLockError>> {
        Box::pin(async { Ok(()) })
    }
}

struct OneChunkThenErrorReader {
    yielded_first_chunk: bool,
    end_with_error: bool,
    dropped: Arc<AtomicBool>,
}

impl AsyncRead for OneChunkThenErrorReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), io::Error>> {
        if !self.yielded_first_chunk {
            self.yielded_first_chunk = true;
            buf.put_slice(b"abc");
            return Poll::Ready(Ok(()));
        }
        if self.end_with_error {
            Poll::Ready(Err(io::Error::other(
                "intentional trailing read failure for direct-stream regression test",
            )))
        } else {
            Poll::Ready(Ok(()))
        }
    }
}

impl Drop for OneChunkThenErrorReader {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

#[derive(Clone)]
struct TrailingErrorStreamDriver {
    get_stream_calls: Arc<AtomicUsize>,
    get_range_calls: Arc<AtomicUsize>,
    reader_dropped: Arc<AtomicBool>,
    end_with_error: bool,
}

impl Default for TrailingErrorStreamDriver {
    fn default() -> Self {
        Self {
            get_stream_calls: Arc::new(AtomicUsize::new(0)),
            get_range_calls: Arc::new(AtomicUsize::new(0)),
            reader_dropped: Arc::new(AtomicBool::new(false)),
            end_with_error: true,
        }
    }
}

impl TrailingErrorStreamDriver {
    fn ending_with_eof() -> Self {
        Self {
            end_with_error: false,
            ..Self::default()
        }
    }
}

#[async_trait]
impl StorageDriver for TrailingErrorStreamDriver {
    async fn put(&self, path: &str, _data: &[u8]) -> aster_drive_storage::Result<String> {
        Ok(path.to_string())
    }

    async fn get(&self, _path: &str) -> aster_drive_storage::Result<Vec<u8>> {
        Err(aster_drive_storage::StorageError::new(
            aster_drive_storage::StorageErrorKind::Unsupported,
            "WebDAV direct-stream test should not use get()",
        ))
    }

    async fn get_stream(
        &self,
        _path: &str,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.get_stream_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(OneChunkThenErrorReader {
            yielded_first_chunk: false,
            end_with_error: self.end_with_error,
            dropped: self.reader_dropped.clone(),
        }))
    }

    async fn get_range(
        &self,
        _path: &str,
        _offset: u64,
        _length: Option<u64>,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.get_range_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(Cursor::new(b"bc".to_vec())))
    }

    async fn delete(&self, _path: &str) -> aster_drive_storage::Result<()> {
        Ok(())
    }

    async fn exists(&self, _path: &str) -> aster_drive_storage::Result<bool> {
        Ok(true)
    }

    async fn metadata(&self, _path: &str) -> aster_drive_storage::Result<BlobMetadata> {
        Ok(BlobMetadata {
            size: 3,
            content_type: Some("text/plain".to_string()),
        })
    }
}

#[derive(Clone, Default)]
struct CountingDirectUploadDriver {
    objects: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    put_file_calls: Arc<AtomicUsize>,
    put_reader_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl StorageDriver for CountingDirectUploadDriver {
    async fn put(&self, path: &str, data: &[u8]) -> aster_drive_storage::Result<String> {
        self.objects
            .lock()
            .expect("direct upload test driver lock should succeed")
            .insert(path.to_string(), data.to_vec());
        Ok(path.to_string())
    }

    async fn get(&self, path: &str) -> aster_drive_storage::Result<Vec<u8>> {
        Ok(self
            .objects
            .lock()
            .expect("direct upload test driver lock should succeed")
            .get(path)
            .cloned()
            .unwrap_or_default())
    }

    async fn get_stream(
        &self,
        path: &str,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        let payload = self
            .objects
            .lock()
            .expect("direct upload test driver lock should succeed")
            .get(path)
            .cloned()
            .unwrap_or_default();
        let (mut writer, reader) = tokio::io::duplex(payload.len().max(1));
        tokio::spawn(async move {
            if let Err(error) = writer.write_all(&payload).await {
                tracing::trace!("mock direct upload stream write failed: {error}");
            }
            if let Err(error) = writer.shutdown().await {
                tracing::trace!("mock direct upload stream shutdown failed: {error}");
            }
        });
        Ok(Box::new(reader))
    }

    async fn delete(&self, path: &str) -> aster_drive_storage::Result<()> {
        self.objects
            .lock()
            .expect("direct upload test driver lock should succeed")
            .remove(path);
        Ok(())
    }

    async fn exists(&self, path: &str) -> aster_drive_storage::Result<bool> {
        Ok(self
            .objects
            .lock()
            .expect("direct upload test driver lock should succeed")
            .contains_key(path))
    }

    async fn metadata(&self, path: &str) -> aster_drive_storage::Result<BlobMetadata> {
        let size = self
            .objects
            .lock()
            .expect("direct upload test driver lock should succeed")
            .get(path)
            .map(|bytes| u64::try_from(bytes.len()).expect("mock object size should fit u64"))
            .unwrap_or(0);
        Ok(BlobMetadata {
            size,
            content_type: Some("text/plain".to_string()),
        })
    }

    fn extensions(&self) -> aster_drive_storage::traits::StorageDriverExtensions<'_> {
        aster_drive_storage::traits::StorageDriverExtensions {
            stream_upload: Some(self),
            ..Default::default()
        }
    }
}

#[async_trait]
impl StreamUploadDriver for CountingDirectUploadDriver {
    async fn put_file(
        &self,
        storage_path: &str,
        local_path: &str,
    ) -> aster_drive_storage::Result<String> {
        self.put_file_calls.fetch_add(1, Ordering::SeqCst);
        let data = tokio::fs::read(local_path).await.map_err(|error| {
            crate::errors::AsterError::storage_driver_error(format!(
                "direct upload test put_file failed: {error}"
            ))
        })?;
        self.objects
            .lock()
            .expect("direct upload test driver lock should succeed")
            .insert(storage_path.to_string(), data);
        Ok(storage_path.to_string())
    }

    async fn put_reader(
        &self,
        storage_path: &str,
        mut reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        _size: i64,
    ) -> aster_drive_storage::Result<String> {
        self.put_reader_calls.fetch_add(1, Ordering::SeqCst);
        let mut data = Vec::new();
        reader.read_to_end(&mut data).await.map_err(|error| {
            crate::errors::AsterError::storage_driver_error(format!(
                "direct upload test put_reader failed: {error}"
            ))
        })?;
        self.objects
            .lock()
            .expect("direct upload test driver lock should succeed")
            .insert(storage_path.to_string(), data);
        Ok(storage_path.to_string())
    }
}

#[actix_web::test]
async fn handle_get_returns_response_before_consuming_the_storage_stream() {
    let driver = TrailingErrorStreamDriver::default();
    let get_stream_calls = driver.get_stream_calls.clone();
    let reader_dropped = driver.reader_dropped.clone();
    let (state, user, policy, temp_root) = build_webdav_test_state(
        DriverType::Local,
        aster_drive_model::types::StoredStoragePolicyOptions::empty(),
        Arc::new(driver),
    )
    .await;
    create_root_file(
        &state,
        user.id,
        policy.id,
        "streamed.txt",
        3,
        "files/streamed.txt",
    )
    .await;

    let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
    let req = actix_web::test::TestRequest::get()
        .uri("/webdav/streamed.txt")
        .to_http_request();
    let lock_system = NoopLockSystem::default();
    let request_head = parsed_request_head(&req);
    let capabilities = capability_snapshot(aster_forge_webdav::DavResourceState::File);
    let response = handle_get_head(
        &req,
        &request_head,
        &dav_fs,
        &lock_system,
        "/webdav",
        false,
        &capabilities,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        get_stream_calls.load(Ordering::SeqCst),
        1,
        "GET should open exactly one streaming reader from storage"
    );
    let body = to_bytes(response.into_body())
        .await
        .expect("the exact three-byte body must finish before the trailing driver error");
    assert_eq!(body.as_ref(), b"abc");
    assert!(reader_dropped.load(Ordering::SeqCst));

    drop(state);
    let _ = std::fs::remove_dir_all(temp_root);
}

#[actix_web::test]
async fn handle_get_range_uses_driver_range_without_opening_full_stream() {
    let driver = TrailingErrorStreamDriver::default();
    let get_stream_calls = driver.get_stream_calls.clone();
    let get_range_calls = driver.get_range_calls.clone();
    let (state, user, policy, temp_root) = build_webdav_test_state(
        DriverType::Local,
        aster_drive_model::types::StoredStoragePolicyOptions::empty(),
        Arc::new(driver),
    )
    .await;
    create_root_file(
        &state,
        user.id,
        policy.id,
        "range.txt",
        3,
        "files/range.txt",
    )
    .await;

    let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
    let req = actix_web::test::TestRequest::get()
        .uri("/webdav/range.txt")
        .insert_header((header::RANGE, "bytes=1-2"))
        .to_http_request();
    let lock_system = NoopLockSystem::default();
    let request_head = parsed_request_head(&req);
    let capabilities = capability_snapshot(aster_forge_webdav::DavResourceState::File);
    let event_sink = Arc::new(CapturingDavEventSink::default());
    let observation = crate::webdav::observation::DavObservation::new(
        request_head.clone(),
        Instant::now(),
        event_sink.clone(),
    );
    let response = crate::webdav::observation::scope(observation.clone(), async {
        handle_get_head(
            &req,
            &request_head,
            &dav_fs,
            &lock_system,
            "/webdav",
            false,
            &capabilities,
        )
        .await
    })
    .await;
    let response = crate::webdav::observation::observe_response(response, observation);

    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        get_range_calls.load(Ordering::SeqCst),
        1,
        "range GET should delegate to StorageDriver::get_range"
    );
    assert_eq!(
        get_stream_calls.load(Ordering::SeqCst),
        0,
        "range GET must not open a full-object stream"
    );
    let body = to_bytes(response.into_body())
        .await
        .expect("the exact range body should be readable");
    assert_eq!(body.as_ref(), b"bc");
    let events = event_sink.events.lock().expect("event sink should lock");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].outcome.status(), 206);
    assert_eq!(events[0].observations.bytes_sent, Some(2));
    assert_eq!(events[0].observations.requested_ranges, Some(1));
    assert_eq!(events[0].observations.served_ranges, Some(1));
    assert_eq!(events[0].observations.resources, Some(1));
    assert_eq!(events[0].observations.backend_open_count, Some(1));
    assert_eq!(events[0].observations.backend_call_count, Some(2));
    assert_eq!(
        events[0].observations.stream,
        Some(aster_forge_webdav::DavStreamOutcome::Completed)
    );

    drop(state);
    let _ = std::fs::remove_dir_all(temp_root);
}

#[actix_web::test]
async fn handle_get_fails_transfer_on_driver_error_or_early_eof() {
    for (label, driver) in [
        ("reader error", TrailingErrorStreamDriver::default()),
        ("early EOF", TrailingErrorStreamDriver::ending_with_eof()),
    ] {
        let (state, user, policy, temp_root) = build_webdav_test_state(
            DriverType::Local,
            aster_drive_model::types::StoredStoragePolicyOptions::empty(),
            Arc::new(driver),
        )
        .await;
        create_root_file(
            &state,
            user.id,
            policy.id,
            "short.txt",
            4,
            "files/short.txt",
        )
        .await;

        let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
        let req = actix_web::test::TestRequest::get()
            .uri("/webdav/short.txt")
            .to_http_request();
        let request_head = parsed_request_head(&req);
        let response = handle_get_head(
            &req,
            &request_head,
            &dav_fs,
            &NoopLockSystem::default(),
            "/webdav",
            false,
            &capability_snapshot(aster_forge_webdav::DavResourceState::File),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            to_bytes(response.into_body()).await.is_err(),
            "{label} before the declared length must fail the response body"
        );

        drop(state);
        let _ = std::fs::remove_dir_all(temp_root);
    }
}

#[actix_web::test]
async fn dropping_get_body_drops_the_unread_storage_reader() {
    let driver = TrailingErrorStreamDriver::default();
    let reader_dropped = driver.reader_dropped.clone();
    let (state, user, policy, temp_root) = build_webdav_test_state(
        DriverType::Local,
        aster_drive_model::types::StoredStoragePolicyOptions::empty(),
        Arc::new(driver),
    )
    .await;
    create_root_file(
        &state,
        user.id,
        policy.id,
        "cancelled.txt",
        4,
        "files/cancelled.txt",
    )
    .await;

    let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
    let req = actix_web::test::TestRequest::get()
        .uri("/webdav/cancelled.txt")
        .to_http_request();
    let request_head = parsed_request_head(&req);
    let response = handle_get_head(
        &req,
        &request_head,
        &dav_fs,
        &NoopLockSystem::default(),
        "/webdav",
        false,
        &capability_snapshot(aster_forge_webdav::DavResourceState::File),
    )
    .await;

    assert!(!reader_dropped.load(Ordering::SeqCst));
    drop(response);
    assert!(reader_dropped.load(Ordering::SeqCst));

    drop(state);
    let _ = std::fs::remove_dir_all(temp_root);
}

#[actix_web::test]
async fn handle_get_multi_range_opens_each_final_range_once() {
    let driver = TrailingErrorStreamDriver::default();
    let get_stream_calls = driver.get_stream_calls.clone();
    let get_range_calls = driver.get_range_calls.clone();
    let (state, user, policy, temp_root) = build_webdav_test_state(
        DriverType::Local,
        aster_drive_model::types::StoredStoragePolicyOptions::empty(),
        Arc::new(driver),
    )
    .await;
    create_root_file(
        &state,
        user.id,
        policy.id,
        "multi-range.txt",
        200,
        "files/multi-range.txt",
    )
    .await;

    let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
    let req = actix_web::test::TestRequest::get()
        .uri("/webdav/multi-range.txt")
        .insert_header((header::RANGE, "bytes=0-1,100-101"))
        .to_http_request();
    let request_head = parsed_request_head(&req);
    let response = handle_get_head(
        &req,
        &request_head,
        &dav_fs,
        &NoopLockSystem::default(),
        "/webdav",
        false,
        &capability_snapshot(aster_forge_webdav::DavResourceState::File),
    )
    .await;

    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(get_range_calls.load(Ordering::SeqCst), 2);
    assert_eq!(get_stream_calls.load(Ordering::SeqCst), 0);
    to_bytes(response.into_body())
        .await
        .expect("both exact multipart range bodies should be readable");

    drop(state);
    let _ = std::fs::remove_dir_all(temp_root);
}

#[actix_web::test]
async fn propfind_href_is_percent_encoded_and_xml_parseable() {
    let driver = CountingDirectUploadDriver::default();
    let (state, user, policy, temp_root) = build_webdav_test_state(
        DriverType::Local,
        aster_drive_model::types::StoredStoragePolicyOptions::empty(),
        std::sync::Arc::new(driver),
    )
    .await;
    let filename = "测试 文件 & report.txt";
    create_root_file(
        &state,
        user.id,
        policy.id,
        filename,
        4,
        "files/weird-name.txt",
    )
    .await;

    let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
    let lock_system = NoopLockSystem::default();
    let encoded_uri = format!("/webdav{}", super::encode_href(&format!("/{filename}")));
    let req = actix_web::test::TestRequest::default()
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").expect("valid method"))
        .uri(&encoded_uri)
        .insert_header((header::HeaderName::from_static("depth"), "0"))
        .to_http_request();

    let request_head = parsed_request_head(&req);
    let capabilities = capability_snapshot(aster_forge_webdav::DavResourceState::File);
    let response = handle_propfind(
        &request_head,
        &dav_fs,
        &lock_system,
        "/webdav",
        &[],
        &capabilities,
        crate::webdav::handlers::properties::PROPFIND_MAXIMUM_DURATION,
    )
    .await;

    assert_eq!(response.status(), StatusCode::from_u16(207).unwrap());
    let body = to_bytes(response.into_body())
        .await
        .expect("PROPFIND response body should be readable");

    let mut hrefs = Vec::new();
    let root = Element::parse_reader(Cursor::new(body.as_ref()))
        .expect("PROPFIND XML should parse cleanly");
    collect_href_text(&root, &mut hrefs);

    assert_eq!(hrefs.len(), 1);
    let decoded = percent_encoding::percent_decode_str(&hrefs[0])
        .decode_utf8_lossy()
        .into_owned();
    assert_eq!(decoded, format!("/webdav/{filename}"));

    drop(state);
    let _ = std::fs::remove_dir_all(temp_root);
}

#[actix_web::test]
async fn propfind_declares_requested_dav_prefix_for_rclone_size_check() {
    let driver = CountingDirectUploadDriver::default();
    let (state, user, policy, temp_root) = build_webdav_test_state(
        DriverType::Local,
        aster_drive_model::types::StoredStoragePolicyOptions::empty(),
        std::sync::Arc::new(driver),
    )
    .await;
    create_root_file(
        &state,
        user.id,
        policy.id,
        "rclone-size.txt",
        129106,
        "files/rclone.txt",
    )
    .await;

    let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
    let lock_system = NoopLockSystem::default();
    let req = actix_web::test::TestRequest::default()
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").expect("valid method"))
        .uri("/webdav/rclone-size.txt")
        .insert_header((header::HeaderName::from_static("depth"), "0"))
        .to_http_request();
    let body = br#"<?xml version="1.0"?>
<d:propfind xmlns:d="DAV:">
  <d:prop>
    <d:displayname/>
    <d:getlastmodified/>
    <d:getcontentlength/>
    <d:quota-used-bytes/>
    <d:resourcetype/>
  </d:prop>
</d:propfind>"#;

    let request_head = parsed_request_head(&req);
    let capabilities = capability_snapshot(aster_forge_webdav::DavResourceState::File);
    let response = handle_propfind(
        &request_head,
        &dav_fs,
        &lock_system,
        "/webdav",
        body,
        &capabilities,
        crate::webdav::handlers::properties::PROPFIND_MAXIMUM_DURATION,
    )
    .await;

    assert_eq!(response.status(), StatusCode::from_u16(207).unwrap());
    let body = to_bytes(response.into_body())
        .await
        .expect("PROPFIND response body should be readable");
    let body_text = String::from_utf8(body.to_vec()).expect("PROPFIND XML should be utf-8");
    assert!(
        body_text.contains("xmlns:d=\"DAV:\""),
        "PROPFIND response must declare echoed lowercase DAV prefix: {body_text}"
    );
    assert!(
        body_text.contains("<d:getcontentlength xmlns:d=\"DAV:\">129106</d:getcontentlength>"),
        "PROPFIND response should expose file size under the requested DAV prefix: {body_text}"
    );
    assert!(
        body_text.contains("<d:quota-used-bytes xmlns:d=\"DAV:\""),
        "missing DAV props should also declare the echoed lowercase DAV prefix: {body_text}"
    );
    Element::parse_reader(Cursor::new(body_text.as_bytes()))
        .expect("PROPFIND XML should parse cleanly");

    drop(state);
    let _ = std::fs::remove_dir_all(temp_root);
}

#[actix_web::test]
async fn propfind_allprop_keeps_default_dav_prefix_xml_parseable() {
    let driver = CountingDirectUploadDriver::default();
    let (state, user, policy, temp_root) = build_webdav_test_state(
        DriverType::Local,
        aster_drive_model::types::StoredStoragePolicyOptions::empty(),
        std::sync::Arc::new(driver),
    )
    .await;
    create_root_file(
        &state,
        user.id,
        policy.id,
        "allprop.txt",
        42,
        "files/allprop.txt",
    )
    .await;

    let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
    let lock_system = NoopLockSystem::default();
    let req = actix_web::test::TestRequest::default()
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").expect("valid method"))
        .uri("/webdav/allprop.txt")
        .insert_header((header::HeaderName::from_static("depth"), "0"))
        .to_http_request();

    let request_head = parsed_request_head(&req);
    let capabilities = capability_snapshot(aster_forge_webdav::DavResourceState::File);
    let response = handle_propfind(
        &request_head,
        &dav_fs,
        &lock_system,
        "/webdav",
        &[],
        &capabilities,
        crate::webdav::handlers::properties::PROPFIND_MAXIMUM_DURATION,
    )
    .await;

    assert_eq!(response.status(), StatusCode::from_u16(207).unwrap());
    let body = to_bytes(response.into_body())
        .await
        .expect("PROPFIND response body should be readable");
    let body_text = String::from_utf8(body.to_vec()).expect("PROPFIND XML should be utf-8");
    assert!(
        body_text.contains("<D:multistatus xmlns:D=\"DAV:\">"),
        "allprop response should declare the canonical DAV prefix at the root: {body_text}"
    );
    assert!(
        body_text.contains("<D:getcontentlength>42</D:getcontentlength>"),
        "allprop response should expose file size under the canonical DAV prefix: {body_text}"
    );
    assert!(
        !body_text.contains("xmlns:D=\"DAV:\" xmlns:D=\"DAV:\""),
        "allprop response should not duplicate the canonical DAV namespace declaration: {body_text}"
    );
    Element::parse_reader(Cursor::new(body_text.as_bytes()))
        .expect("PROPFIND XML should parse cleanly");

    drop(state);
    let _ = std::fs::remove_dir_all(temp_root);
}

#[actix_web::test]
async fn propfind_zero_duration_returns_503_before_streaming_with_no_store() {
    let driver = CountingDirectUploadDriver::default();
    let (state, user, _policy, temp_root) = build_webdav_test_state(
        DriverType::Local,
        aster_drive_model::types::StoredStoragePolicyOptions::empty(),
        Arc::new(driver),
    )
    .await;
    let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
    let req = actix_web::test::TestRequest::default()
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").expect("valid method"))
        .uri("/webdav/")
        .insert_header((header::HeaderName::from_static("depth"), "0"))
        .to_http_request();
    let request_head = parsed_request_head(&req);
    let capabilities = capability_snapshot(aster_forge_webdav::DavResourceState::Collection);

    let response = handle_propfind(
        &request_head,
        &dav_fs,
        &NoopLockSystem::default(),
        "/webdav",
        &[],
        &capabilities,
        Duration::ZERO,
    )
    .await;

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );

    drop(state);
    let _ = std::fs::remove_dir_all(temp_root);
}

#[actix_web::test]
async fn propfind_child_lock_preload_timeout_ends_started_stream_with_error() {
    let driver = CountingDirectUploadDriver::default();
    let (state, user, _policy, temp_root) = build_webdav_test_state(
        DriverType::Local,
        aster_drive_model::types::StoredStoragePolicyOptions::empty(),
        Arc::new(driver),
    )
    .await;
    let now = Utc::now();
    folder_entity::ActiveModel {
        name: Set("deadline-child".to_string()),
        parent_id: Set(None),
        team_id: Set(None),
        owner_user_id: Set(Some(user.id)),
        created_by_user_id: Set(Some(user.id)),
        created_by_username: Set(user.username.clone()),
        policy_id: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("deadline child should insert");

    let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
    let lock_system = NoopLockSystem::delaying_from(2, Duration::from_secs(1));
    let req = actix_web::test::TestRequest::default()
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").expect("valid method"))
        .uri("/webdav/")
        .insert_header((header::HeaderName::from_static("depth"), "1"))
        .to_http_request();
    let request_head = parsed_request_head(&req);
    let capabilities = capability_snapshot(aster_forge_webdav::DavResourceState::Collection);
    let body = br#"<D:propfind xmlns:D="DAV:"><D:prop><D:lockdiscovery/></D:prop></D:propfind>"#;

    let response = handle_propfind(
        &request_head,
        &dav_fs,
        &lock_system,
        "/webdav",
        body,
        &capabilities,
        Duration::from_millis(500),
    )
    .await;

    assert_eq!(response.status(), StatusCode::MULTI_STATUS);
    let error = to_bytes(response.into_body())
        .await
        .expect_err("child preload timeout must terminate an already-started stream");
    assert!(
        error.to_string().contains("WebDAV response stream failed"),
        "unexpected stream error: {error}"
    );
    assert_eq!(lock_system.discover_many_calls.load(Ordering::SeqCst), 2);

    drop(state);
    let _ = std::fs::remove_dir_all(temp_root);
}

#[actix_web::test]
async fn folder_tree_limits_enforce_exact_resource_frontier_and_depth_boundaries() {
    use crate::services::files::folder::{FolderTreeTraversalLimits, collect_folder_tree_in_scope};
    use crate::services::workspace::storage::WorkspaceStorageScope;

    let driver = CountingDirectUploadDriver::default();
    let (state, user, policy, temp_root) = build_webdav_test_state(
        DriverType::Local,
        aster_drive_model::types::StoredStoragePolicyOptions::empty(),
        Arc::new(driver),
    )
    .await;
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let root = create_test_folder(&state, &user, "budget-root", None).await;
    let child_a = create_test_folder(&state, &user, "child-a", Some(root.id)).await;
    let child_b = create_test_folder(&state, &user, "child-b", Some(root.id)).await;
    let _grandchild = create_test_folder(&state, &user, "grandchild", Some(child_a.id)).await;
    create_root_file(
        &state,
        user.id,
        policy.id,
        "budget-file.txt",
        1,
        "files/budget-file.txt",
    )
    .await;
    file::ActiveModel {
        id: Set(file_repo::find_by_name_in_folder(
            state.writer_db(),
            user.id,
            None,
            "budget-file.txt",
        )
        .await
        .expect("budget file lookup")
        .expect("budget file")
        .id),
        folder_id: Set(Some(child_b.id)),
        ..Default::default()
    }
    .update(state.writer_db())
    .await
    .expect("budget file should move under child");

    let exact = FolderTreeTraversalLimits::new(5, 2, 2);
    let (files, folders) =
        collect_folder_tree_in_scope(state.writer_db(), scope, root.id, false, Some(exact))
            .await
            .expect("exact resource, frontier and depth limits should succeed");
    assert_eq!(files.len(), 1);
    assert_eq!(folders.len(), 4);

    for limits in [
        FolderTreeTraversalLimits::new(4, 2, 2),
        FolderTreeTraversalLimits::new(5, 1, 2),
        FolderTreeTraversalLimits::new(5, 2, 1),
    ] {
        let error =
            collect_folder_tree_in_scope(state.writer_db(), scope, root.id, false, Some(limits))
                .await
                .expect_err("limit plus one should fail");
        assert!(matches!(
            error,
            crate::errors::AsterError::OperationResourceLimitExceeded(_)
        ));
    }

    drop(state);
    let _ = std::fs::remove_dir_all(temp_root);
}

#[actix_web::test]
async fn folder_tree_limits_bound_deleted_file_loading_before_collection() {
    use crate::services::files::folder::{FolderTreeTraversalLimits, collect_folder_tree_in_scope};
    use crate::services::workspace::storage::WorkspaceStorageScope;

    let driver = CountingDirectUploadDriver::default();
    let (state, user, policy, temp_root) = build_webdav_test_state(
        DriverType::Local,
        aster_drive_model::types::StoredStoragePolicyOptions::empty(),
        Arc::new(driver),
    )
    .await;
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let root = create_test_folder(&state, &user, "deleted-budget-root", None).await;
    let (active, _) = create_root_file(
        &state,
        user.id,
        policy.id,
        "active-budget.txt",
        1,
        "files/active-budget.txt",
    )
    .await;
    let (deleted, _) = create_root_file(
        &state,
        user.id,
        policy.id,
        "deleted-budget.txt",
        1,
        "files/deleted-budget.txt",
    )
    .await;
    file::ActiveModel {
        id: Set(active.id),
        folder_id: Set(Some(root.id)),
        ..Default::default()
    }
    .update(state.writer_db())
    .await
    .expect("active file should move under test folder");
    file::ActiveModel {
        id: Set(deleted.id),
        folder_id: Set(Some(root.id)),
        deleted_at: Set(Some(Utc::now())),
        ..Default::default()
    }
    .update(state.writer_db())
    .await
    .expect("deleted file should move under test folder");

    let exact = FolderTreeTraversalLimits::new(2, 1, 1);
    let (files, folders) =
        collect_folder_tree_in_scope(state.writer_db(), scope, root.id, false, Some(exact))
            .await
            .expect("active-only traversal should fit the exact budget");
    assert_eq!(files.len(), 1);
    assert_eq!(folders, [root.id]);

    let error = collect_folder_tree_in_scope(state.writer_db(), scope, root.id, true, Some(exact))
        .await
        .expect_err("deleted file at remaining limit plus one should fail during loading");
    assert!(matches!(
        error,
        crate::errors::AsterError::OperationResourceLimitExceeded(_)
    ));

    drop(state);
    let _ = std::fs::remove_dir_all(temp_root);
}

#[actix_web::test]
async fn bounded_delete_and_copy_fail_before_any_tree_write() {
    use crate::services::files::folder::{self, FolderTreeTraversalLimits};
    use crate::services::workspace::storage::WorkspaceStorageScope;

    let driver = CountingDirectUploadDriver::default();
    let (state, user, _policy, temp_root) = build_webdav_test_state(
        DriverType::Local,
        aster_drive_model::types::StoredStoragePolicyOptions::empty(),
        Arc::new(driver),
    )
    .await;
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let root = create_test_folder(&state, &user, "write-boundary-root", None).await;
    let child = create_test_folder(&state, &user, "child", Some(root.id)).await;
    let one_resource = Some(FolderTreeTraversalLimits::new(1, 1, 8));

    let copy_error = folder::copy_folder_tree_in_scope(
        &state,
        scope,
        root.id,
        None,
        "copy-must-not-exist",
        one_resource,
    )
    .await
    .expect_err("bounded copy should reject source tree before creating destination");
    assert!(matches!(
        copy_error,
        crate::errors::AsterError::OperationResourceLimitExceeded(_)
    ));
    assert!(
        folder_repo::find_by_name_in_parent(
            state.writer_db(),
            user.id,
            None,
            "copy-must-not-exist",
        )
        .await
        .expect("destination lookup")
        .is_none()
    );

    let delete_error = folder::delete_in_scope(&state, scope, root.id, one_resource)
        .await
        .expect_err("bounded delete should reject tree before soft-delete writes");
    assert!(matches!(
        delete_error,
        crate::errors::AsterError::OperationResourceLimitExceeded(_)
    ));
    for id in [root.id, child.id] {
        let current = folder_repo::find_by_id(state.writer_db(), id)
            .await
            .expect("folder should remain");
        assert!(current.deleted_at.is_none());
    }

    drop(state);
    let _ = std::fs::remove_dir_all(temp_root);
}

#[actix_web::test]
async fn mutation_port_moves_collection_and_refreshes_cached_paths() {
    use crate::services::events::storage_change::StorageChangeKind;
    use crate::webdav::backend::path_resolver::{ResolvedNode, resolve_path};

    let driver = CountingDirectUploadDriver::default();
    let (state, user, _policy, temp_root) = build_webdav_test_state(
        DriverType::Local,
        aster_drive_model::types::StoredStoragePolicyOptions::empty(),
        Arc::new(driver),
    )
    .await;
    let source = create_test_folder(&state, &user, "atomic-folder-source", None).await;
    let child = create_test_folder(&state, &user, "child", Some(source.id)).await;
    let destination_parent = create_test_folder(&state, &user, "atomic-folder-parent", None).await;
    let source_path = DavPath::new("/atomic-folder-source/").unwrap();
    let source_child_path = DavPath::new("/atomic-folder-source/child/").unwrap();
    let destination_path = DavPath::new("/atomic-folder-parent/moved/").unwrap();
    let destination_child_path = DavPath::new("/atomic-folder-parent/moved/child/").unwrap();
    let dav_fs = AsterDavFs::new(state.clone(), user.id, None);

    dav_fs
        .metadata_for_write(&source_child_path)
        .await
        .expect("source child path should be cached before the move");
    let mut events = state.storage_change_bus.subscribe();
    let headers = http::HeaderMap::new();
    dav_fs
        .move_with_locks(
            &source_path,
            &destination_path,
            mutation_conditions(&headers, DavMethod::Move, &source_path),
        )
        .await
        .expect("the backend mutation port should move a collection atomically");

    let event = tokio::time::timeout(Duration::from_secs(1), events.recv())
        .await
        .expect("folder move storage event should arrive")
        .expect("folder move storage event channel should remain open");
    assert_eq!(event.kind, StorageChangeKind::FolderUpdated);
    assert_eq!(event.folder_ids, vec![source.id]);
    assert_eq!(event.affected_parent_ids, vec![destination_parent.id]);

    assert!(matches!(
        resolve_path(state.writer_db(), user.id, &source_path, None).await,
        Err(FsError::NotFound)
    ));
    assert!(matches!(
        resolve_path(state.writer_db(), user.id, &destination_path, None).await,
        Ok(ResolvedNode::Folder(folder)) if folder.id == source.id
    ));
    assert!(matches!(
        resolve_path(state.writer_db(), user.id, &destination_child_path, None).await,
        Ok(ResolvedNode::Folder(folder)) if folder.id == child.id
    ));
    assert!(matches!(
        dav_fs.metadata_for_write(&source_child_path).await,
        Err(FsError::NotFound)
    ));
    dav_fs
        .metadata_for_write(&destination_child_path)
        .await
        .expect("the moved descendant should resolve through the refreshed cache path");

    drop(state);
    let _ = std::fs::remove_dir_all(temp_root);
}

#[actix_web::test]
async fn mutation_port_copy_file_replaces_collection_tree() {
    use crate::webdav::backend::path_resolver::{ResolvedNode, resolve_path};

    let driver = CountingDirectUploadDriver::default();
    let (state, user, policy, temp_root) = build_webdav_test_state(
        DriverType::Local,
        aster_drive_model::types::StoredStoragePolicyOptions::empty(),
        Arc::new(driver),
    )
    .await;
    let (source, _) = create_root_file(
        &state,
        user.id,
        policy.id,
        "atomic-copy-source.txt",
        4,
        "files/atomic-copy-source.txt",
    )
    .await;
    let overwritten = create_test_folder(&state, &user, "atomic-copy-target", None).await;
    let overwritten_child = create_test_folder(&state, &user, "nested", Some(overwritten.id)).await;
    let overwritten_file =
        create_file_in_folder(&state, user.id, policy.id, overwritten_child.id, "old.txt").await;
    let source_path = DavPath::new("/atomic-copy-source.txt").unwrap();
    let destination_path = DavPath::new("/atomic-copy-target").unwrap();
    let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
    let headers = http::HeaderMap::new();

    dav_fs
        .copy_file_with_locks(
            &source_path,
            &destination_path,
            mutation_conditions(&headers, DavMethod::Copy, &source_path),
        )
        .await
        .expect("copying a file should replace an unlocked destination collection tree");

    let copied = match resolve_path(state.writer_db(), user.id, &destination_path, None)
        .await
        .expect("the replacement destination should resolve")
    {
        ResolvedNode::File(file) => file,
        other => panic!("expected a replacement file, got {other:?}"),
    };
    assert_ne!(copied.id, source.id);
    assert_eq!(copied.blob_id, source.blob_id);
    assert_eq!(
        file_repo::find_blob_by_id(state.writer_db(), source.blob_id)
            .await
            .expect("the shared blob should remain available")
            .ref_count,
        2,
        "copying a file should increment the shared blob reference count"
    );
    assert!(matches!(
        resolve_path(state.writer_db(), user.id, &source_path, None).await,
        Ok(ResolvedNode::File(file)) if file.id == source.id
    ));
    assert!(
        folder_repo::find_by_id(state.writer_db(), overwritten.id)
            .await
            .expect("overwritten folder row should remain available for trash history")
            .deleted_at
            .is_some()
    );
    assert!(
        folder_repo::find_by_id(state.writer_db(), overwritten_child.id)
            .await
            .expect("overwritten child folder should remain available for trash history")
            .deleted_at
            .is_some()
    );
    assert!(
        file_repo::find_by_id(state.writer_db(), overwritten_file.id)
            .await
            .expect("overwritten descendant file should remain available for trash history")
            .deleted_at
            .is_some()
    );

    drop(state);
    let _ = std::fs::remove_dir_all(temp_root);
}

#[actix_web::test]
async fn mutation_port_move_file_replaces_collection_tree() {
    use crate::webdav::backend::path_resolver::{ResolvedNode, resolve_path};

    let driver = CountingDirectUploadDriver::default();
    let (state, user, policy, temp_root) = build_webdav_test_state(
        DriverType::Local,
        aster_drive_model::types::StoredStoragePolicyOptions::empty(),
        Arc::new(driver),
    )
    .await;
    let (source, _) = create_root_file(
        &state,
        user.id,
        policy.id,
        "atomic-move-source.txt",
        4,
        "files/atomic-move-source.txt",
    )
    .await;
    let overwritten = create_test_folder(&state, &user, "atomic-move-target", None).await;
    let overwritten_child = create_test_folder(&state, &user, "nested", Some(overwritten.id)).await;
    let overwritten_file =
        create_file_in_folder(&state, user.id, policy.id, overwritten_child.id, "old.txt").await;
    let source_path = DavPath::new("/atomic-move-source.txt").unwrap();
    let destination_path = DavPath::new("/atomic-move-target").unwrap();
    let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
    let headers = http::HeaderMap::new();

    dav_fs
        .move_with_locks(
            &source_path,
            &destination_path,
            mutation_conditions(&headers, DavMethod::Move, &source_path),
        )
        .await
        .expect("moving a file should replace an unlocked destination collection tree");

    assert!(matches!(
        resolve_path(state.writer_db(), user.id, &source_path, None).await,
        Err(FsError::NotFound)
    ));
    assert!(matches!(
        resolve_path(state.writer_db(), user.id, &destination_path, None).await,
        Ok(ResolvedNode::File(file)) if file.id == source.id
    ));
    assert!(
        folder_repo::find_by_id(state.writer_db(), overwritten.id)
            .await
            .expect("overwritten folder row should remain available for trash history")
            .deleted_at
            .is_some()
    );
    assert!(
        folder_repo::find_by_id(state.writer_db(), overwritten_child.id)
            .await
            .expect("overwritten child folder should remain available for trash history")
            .deleted_at
            .is_some()
    );
    assert!(
        file_repo::find_by_id(state.writer_db(), overwritten_file.id)
            .await
            .expect("overwritten descendant file should remain available for trash history")
            .deleted_at
            .is_some()
    );

    drop(state);
    let _ = std::fs::remove_dir_all(temp_root);
}

#[actix_web::test]
async fn mutation_port_revalidates_canonical_literal_percent_parents() {
    use crate::webdav::backend::lock::DbLockSystem;
    use crate::webdav::backend::path_resolver::resolve_path;

    let driver = CountingDirectUploadDriver::default();
    let (state, user, policy, temp_root) = build_webdav_test_state(
        DriverType::Local,
        aster_drive_model::types::StoredStoragePolicyOptions::empty(),
        Arc::new(driver),
    )
    .await;

    let copy_parent = create_test_folder(&state, &user, "literal-%FF-copy", None).await;
    let collection_parent = create_test_folder(&state, &user, "literal-%FF-collection", None).await;
    let delete_parent = create_test_folder(&state, &user, "literal-%FF-delete", None).await;
    let move_source_parent =
        create_test_folder(&state, &user, "literal-%FF-move-source", None).await;
    let move_destination_parent =
        create_test_folder(&state, &user, "literal-%FF-move-destination", None).await;
    let collection_source =
        create_test_folder(&state, &user, "literal-parent-collection-source", None).await;

    let (copy_source, _) = create_root_file(
        &state,
        user.id,
        policy.id,
        "literal-parent-copy-source.txt",
        1,
        "files/literal-parent-copy-source.txt",
    )
    .await;
    let delete_source = create_file_in_folder(
        &state,
        user.id,
        policy.id,
        delete_parent.id,
        "delete-source.txt",
    )
    .await;
    let move_source = create_file_in_folder(
        &state,
        user.id,
        policy.id,
        move_source_parent.id,
        "move-source.txt",
    )
    .await;
    let (move_destination_source, _) = create_root_file(
        &state,
        user.id,
        policy.id,
        "literal-parent-move-destination-source.txt",
        1,
        "files/literal-parent-move-destination-source.txt",
    )
    .await;

    let copy_parent_path = DavPath::new("/literal-%25FF-copy/").unwrap();
    let collection_parent_path = DavPath::new("/literal-%25FF-collection/").unwrap();
    let delete_parent_path = DavPath::new("/literal-%25FF-delete/").unwrap();
    let move_source_parent_path = DavPath::new("/literal-%25FF-move-source/").unwrap();
    let move_destination_parent_path = DavPath::new("/literal-%25FF-move-destination/").unwrap();
    for (path, expected_id) in [
        (&copy_parent_path, copy_parent.id),
        (&collection_parent_path, collection_parent.id),
        (&delete_parent_path, delete_parent.id),
        (&move_source_parent_path, move_source_parent.id),
        (&move_destination_parent_path, move_destination_parent.id),
    ] {
        assert!(path.as_str().contains("%FF"));
        assert!(matches!(
            resolve_path(state.writer_db(), user.id, path, None).await,
            Ok(crate::webdav::backend::path_resolver::ResolvedNode::Folder(folder))
                if folder.id == expected_id
        ));
    }

    let lock_system = DbLockSystem::new(state.clone(), user.id, None);
    for path in [
        &copy_parent_path,
        &collection_parent_path,
        &delete_parent_path,
        &move_source_parent_path,
        &move_destination_parent_path,
    ] {
        let created = lock_system
            .lock(DavLockAcquireRequest {
                path,
                principal: None,
                owner: None,
                timeout: Some(Duration::from_secs(120)),
                shared: false,
                deep: false,
                credentials: DavMutationCredentials::default(),
            })
            .await
            .expect("literal-percent parent lock should be created");
        assert_eq!(created.lock.path.as_ref(), path);
    }

    let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
    let headers = http::HeaderMap::new();

    let copy_source_path = DavPath::new("/literal-parent-copy-source.txt").unwrap();
    let copy_destination_path = DavPath::new("/literal-%25FF-copy/copied.txt").unwrap();
    assert_locked_mutation(
        dav_fs
            .copy_file_with_locks(
                &copy_source_path,
                &copy_destination_path,
                mutation_conditions(&headers, DavMethod::Copy, &copy_source_path),
            )
            .await,
        &copy_parent_path,
    );

    let collection_source_path = DavPath::new("/literal-parent-collection-source/").unwrap();
    let collection_destination_path = DavPath::new("/literal-%25FF-collection/prepared/").unwrap();
    assert_locked_mutation(
        dav_fs
            .prepare_collection_with_locks(
                &collection_source_path,
                &collection_destination_path,
                DavMutationOperation::Copy,
                mutation_conditions(&headers, DavMethod::Copy, &collection_source_path),
            )
            .await,
        &collection_parent_path,
    );

    let delete_source_path = DavPath::new("/literal-%25FF-delete/delete-source.txt").unwrap();
    assert_locked_mutation(
        dav_fs
            .delete_with_locks(
                &delete_source_path,
                false,
                DavMutationOperation::Delete,
                DavMutationTargetRole::Source,
                mutation_conditions(&headers, DavMethod::Delete, &delete_source_path),
            )
            .await,
        &delete_parent_path,
    );

    let move_source_path = DavPath::new("/literal-%25FF-move-source/move-source.txt").unwrap();
    let move_source_destination_path = DavPath::new("/moved-from-literal-parent.txt").unwrap();
    assert_locked_mutation(
        dav_fs
            .move_with_locks(
                &move_source_path,
                &move_source_destination_path,
                mutation_conditions(&headers, DavMethod::Move, &move_source_path),
            )
            .await,
        &move_source_parent_path,
    );

    let move_destination_source_path =
        DavPath::new("/literal-parent-move-destination-source.txt").unwrap();
    let move_destination_path = DavPath::new("/literal-%25FF-move-destination/moved.txt").unwrap();
    assert_locked_mutation(
        dav_fs
            .move_with_locks(
                &move_destination_source_path,
                &move_destination_path,
                mutation_conditions(&headers, DavMethod::Move, &move_destination_source_path),
            )
            .await,
        &move_destination_parent_path,
    );

    for (path, expected_id) in [
        (&copy_source_path, copy_source.id),
        (&delete_source_path, delete_source.id),
        (&move_source_path, move_source.id),
        (&move_destination_source_path, move_destination_source.id),
    ] {
        assert!(matches!(
            resolve_path(state.writer_db(), user.id, path, None).await,
            Ok(crate::webdav::backend::path_resolver::ResolvedNode::File(file))
                if file.id == expected_id
        ));
    }
    assert!(matches!(
        resolve_path(
            state.writer_db(),
            user.id,
            &collection_source_path,
            None
        )
        .await,
        Ok(crate::webdav::backend::path_resolver::ResolvedNode::Folder(folder))
            if folder.id == collection_source.id
    ));
    for destination in [
        &copy_destination_path,
        &collection_destination_path,
        &move_source_destination_path,
        &move_destination_path,
    ] {
        assert!(matches!(
            resolve_path(state.writer_db(), user.id, destination, None).await,
            Err(FsError::NotFound)
        ));
    }

    drop(state);
    let _ = std::fs::remove_dir_all(temp_root);
}

#[actix_web::test]
async fn mutation_port_rejects_invalid_resource_shapes_without_writes() {
    use crate::webdav::backend::path_resolver::{ResolvedNode, resolve_path};

    let driver = CountingDirectUploadDriver::default();
    let (state, user, policy, temp_root) = build_webdav_test_state(
        DriverType::Local,
        aster_drive_model::types::StoredStoragePolicyOptions::empty(),
        Arc::new(driver),
    )
    .await;
    let source_folder = create_test_folder(&state, &user, "shape-folder", None).await;
    let (source_file, _) = create_root_file(
        &state,
        user.id,
        policy.id,
        "shape-file.txt",
        1,
        "files/shape-file.txt",
    )
    .await;
    let (_deletable_file, _) = create_root_file(
        &state,
        user.id,
        policy.id,
        "source-cleanup.txt",
        1,
        "files/source-cleanup.txt",
    )
    .await;
    let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
    let folder_path = DavPath::new("/shape-folder/").unwrap();
    let file_path = DavPath::new("/shape-file.txt").unwrap();
    let cleanup_path = DavPath::new("/source-cleanup.txt").unwrap();
    let missing_path = DavPath::new("/shape-destination").unwrap();
    let root_path = DavPath::new("/").unwrap();
    let headers = http::HeaderMap::new();

    assert_forbidden_mutation(
        dav_fs
            .copy_file_with_locks(
                &folder_path,
                &missing_path,
                mutation_conditions(&headers, DavMethod::Copy, &folder_path),
            )
            .await,
    );
    assert_forbidden_mutation(
        dav_fs
            .prepare_collection_with_locks(
                &file_path,
                &missing_path,
                DavMutationOperation::Copy,
                mutation_conditions(&headers, DavMethod::Copy, &file_path),
            )
            .await,
    );
    assert_forbidden_mutation(
        dav_fs
            .delete_with_locks(
                &file_path,
                true,
                DavMutationOperation::Delete,
                DavMutationTargetRole::Source,
                mutation_conditions(&headers, DavMethod::Delete, &file_path),
            )
            .await,
    );
    assert_forbidden_mutation(
        dav_fs
            .delete_with_locks(
                &folder_path,
                false,
                DavMutationOperation::Delete,
                DavMutationTargetRole::Source,
                mutation_conditions(&headers, DavMethod::Delete, &folder_path),
            )
            .await,
    );
    assert_forbidden_mutation(
        dav_fs
            .move_with_locks(
                &root_path,
                &missing_path,
                mutation_conditions(&headers, DavMethod::Move, &root_path),
            )
            .await,
    );
    assert_forbidden_mutation(
        dav_fs
            .copy_file_with_locks(
                &file_path,
                &root_path,
                mutation_conditions(&headers, DavMethod::Copy, &file_path),
            )
            .await,
    );
    assert_forbidden_mutation(
        dav_fs
            .move_with_locks(
                &file_path,
                &root_path,
                mutation_conditions(&headers, DavMethod::Move, &file_path),
            )
            .await,
    );
    assert_forbidden_mutation(
        dav_fs
            .prepare_collection_with_locks(
                &folder_path,
                &root_path,
                DavMutationOperation::Move,
                mutation_conditions(&headers, DavMethod::Move, &folder_path),
            )
            .await,
    );

    dav_fs
        .delete_with_locks(
            &cleanup_path,
            false,
            DavMutationOperation::Move,
            DavMutationTargetRole::Source,
            mutation_conditions(&headers, DavMethod::Move, &cleanup_path),
        )
        .await
        .expect("source cleanup should delete a file without destination-delete audit semantics");

    assert!(matches!(
        resolve_path(state.writer_db(), user.id, &folder_path, None).await,
        Ok(ResolvedNode::Folder(folder)) if folder.id == source_folder.id
    ));
    assert!(matches!(
        resolve_path(state.writer_db(), user.id, &file_path, None).await,
        Ok(ResolvedNode::File(file)) if file.id == source_file.id
    ));
    assert!(matches!(
        resolve_path(state.writer_db(), user.id, &cleanup_path, None).await,
        Err(FsError::NotFound)
    ));

    drop(state);
    let _ = std::fs::remove_dir_all(temp_root);
}

fn collect_href_text(element: &Element, hrefs: &mut Vec<String>) {
    if (element.name == "href" || element.name == "D:href")
        && let Some(text) = element.text()
    {
        hrefs.push(text);
    }

    for child in &element.children {
        if let XMLNode::Element(child) = child {
            collect_href_text(child, hrefs);
        }
    }
}

#[actix_web::test]
async fn handle_head_does_not_open_the_storage_stream() {
    let driver = TrailingErrorStreamDriver::default();
    let get_stream_calls = driver.get_stream_calls.clone();
    let (state, user, policy, temp_root) = build_webdav_test_state(
        DriverType::Local,
        aster_drive_model::types::StoredStoragePolicyOptions::empty(),
        Arc::new(driver),
    )
    .await;
    create_root_file(&state, user.id, policy.id, "head.txt", 3, "files/head.txt").await;

    let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
    let req = actix_web::test::TestRequest::default()
        .method(actix_web::http::Method::HEAD)
        .uri("/webdav/head.txt")
        .to_http_request();
    let lock_system = NoopLockSystem::default();
    let request_head = parsed_request_head(&req);
    let capabilities = capability_snapshot(aster_forge_webdav::DavResourceState::File);
    let response = handle_get_head(
        &req,
        &request_head,
        &dav_fs,
        &lock_system,
        "/webdav",
        true,
        &capabilities,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        get_stream_calls.load(Ordering::SeqCst),
        0,
        "HEAD should return metadata without opening the storage stream"
    );

    drop(state);
    let _ = std::fs::remove_dir_all(temp_root);
}

#[actix_web::test]
async fn handle_put_with_content_length_uses_direct_s3_stream_upload() {
    let driver = CountingDirectUploadDriver::default();
    let put_file_calls = driver.put_file_calls.clone();
    let put_reader_calls = driver.put_reader_calls.clone();
    let options = serialize_storage_policy_options(&StoragePolicyOptions {
        object_storage_upload_strategy: Some(ObjectStorageUploadStrategy::RelayStream),
        ..Default::default()
    })
    .expect("direct upload policy options should serialize");
    let (state, user, _policy, temp_root) =
        build_webdav_test_state(DriverType::S3, options, Arc::new(driver.clone())).await;

    let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
    let lock_system = NoopLockSystem::default();
    let system_file_policy = crate::webdav::system_file::SystemFileBlockPolicy::from_runtime_config(
        &state.runtime_config,
    );
    let body = "webdav direct stream upload";
    let (req, mut dev_payload) = actix_web::test::TestRequest::put()
        .uri("/webdav/direct.txt")
        .insert_header((header::CONTENT_LENGTH, body.len().to_string()))
        .set_payload(body)
        .to_http_parts();
    let mut payload = web::Payload::from_request(&req, &mut dev_payload)
        .await
        .expect("webdav test payload should extract");
    let request_head = parsed_request_head(&req);
    let capabilities = capability_snapshot(aster_forge_webdav::DavResourceState::Unmapped);
    let response = handle_put(
        &req,
        &request_head,
        &dav_fs,
        &lock_system,
        "/webdav",
        &system_file_policy,
        &mut payload,
        &capabilities,
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(
        put_reader_calls.load(Ordering::SeqCst),
        1,
        "known-size WebDAV PUT should use StorageDriver::put_reader()"
    );
    assert_eq!(
        put_file_calls.load(Ordering::SeqCst),
        0,
        "known-size WebDAV PUT should not fall back to StorageDriver::put_file()"
    );

    let stored = file_repo::find_by_name_in_folder(state.writer_db(), user.id, None, "direct.txt")
        .await
        .expect("stored WebDAV file lookup should succeed")
        .expect("direct WebDAV PUT should create a file");
    assert_eq!(
        stored.size,
        i64::try_from(body.len()).expect("request body length should fit i64")
    );
    assert!(
        driver
            .objects
            .lock()
            .expect("direct upload test driver lock should succeed")
            .values()
            .any(|bytes| bytes.as_slice() == body.as_bytes()),
        "direct stream upload should persist the request payload"
    );

    drop(state);
    let _ = std::fs::remove_dir_all(temp_root);
}
