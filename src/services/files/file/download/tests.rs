use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use actix_web::body;
use async_trait::async_trait;
use chrono::Utc;
use migration::Migrator;
use sea_orm::{ActiveModelTrait, Set};
use tokio::io::{AsyncRead, AsyncWriteExt};

use crate::config::{Config, DatabaseConfig, RuntimeConfig};
use crate::db::repository::file_repo;
use crate::entities::{file, file_blob, storage_policy, user};
use crate::runtime::PrimaryAppState;
use crate::services::files::file::DownloadDisposition;
use crate::services::{mail::sender, storage_policy::policy};
use crate::storage::BlobMetadata;
use crate::storage::traits::driver::PresignedDownloadOptions;
use crate::storage::traits::extensions::PresignedStorageDriver;
use crate::storage::{DriverRegistry, PolicySnapshot, StorageDriver};
use crate::types::{
    DriverType, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions, UserRole, UserStatus,
};
use aster_forge_cache as cache;
use aster_forge_cache::CacheConfig;
use aster_forge_utils::numbers::usize_to_i64;

use super::build::build_download_outcome_with_disposition_and_range;
use super::range::ResolvedDownloadRange;
use super::response::outcome_to_response;
use super::streaming::AbortAwareStream;
use super::types::DownloadOutcome;

fn payload_len_i64(payload: &[u8]) -> i64 {
    usize_to_i64(payload.len(), "payload_len").expect("test payload length should fit in i64")
}

#[tokio::test]
async fn abort_aware_stream_disarms_hook_on_clean_eof() {
    use futures::StreamExt;

    let flag = Arc::new(AtomicUsize::new(0));
    let flag_clone = flag.clone();
    let items: Vec<std::io::Result<bytes::Bytes>> = vec![Ok(bytes::Bytes::from_static(b"hello"))];
    let inner = futures::stream::iter(items);
    let mut stream = AbortAwareStream {
        inner,
        hook: Some(Box::new(move || {
            flag_clone.fetch_add(1, Ordering::SeqCst);
        })),
    };

    while stream.next().await.is_some() {}
    drop(stream);

    assert_eq!(
        flag.load(Ordering::SeqCst),
        0,
        "clean EOF must not fire hook"
    );
}

#[tokio::test]
async fn abort_aware_stream_fires_hook_on_drop_without_eof() {
    let flag = Arc::new(AtomicUsize::new(0));
    let flag_clone = flag.clone();
    let items: Vec<std::io::Result<bytes::Bytes>> = vec![
        Ok(bytes::Bytes::from_static(b"part1")),
        Ok(bytes::Bytes::from_static(b"part2")),
    ];
    let inner = futures::stream::iter(items);
    let stream = AbortAwareStream {
        inner,
        hook: Some(Box::new(move || {
            flag_clone.fetch_add(1, Ordering::SeqCst);
        })),
    };

    drop(stream);

    assert_eq!(
        flag.load(Ordering::SeqCst),
        1,
        "drop without EOF must fire hook exactly once"
    );
}

#[derive(Clone)]
struct CountingStreamDriver {
    bytes: Arc<Vec<u8>>,
    get_calls: Arc<AtomicUsize>,
    get_stream_calls: Arc<AtomicUsize>,
}

impl CountingStreamDriver {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes: Arc::new(bytes),
            get_calls: Arc::new(AtomicUsize::new(0)),
            get_stream_calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

#[async_trait]
impl StorageDriver for CountingStreamDriver {
    async fn put(&self, path: &str, _data: &[u8]) -> crate::errors::Result<String> {
        Ok(path.to_string())
    }

    async fn get(&self, _path: &str) -> crate::errors::Result<Vec<u8>> {
        self.get_calls.fetch_add(1, Ordering::SeqCst);
        Err(crate::errors::AsterError::storage_driver_error(
            "download stream regression: get() should not be used here",
        ))
    }

    async fn get_stream(
        &self,
        _path: &str,
    ) -> crate::errors::Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.get_stream_calls.fetch_add(1, Ordering::SeqCst);
        let (mut writer, reader) = tokio::io::duplex(self.bytes.len().max(1));
        let payload = self.bytes.as_ref().clone();
        tokio::spawn(async move {
            if let Err(error) = writer.write_all(&payload).await {
                tracing::trace!("mock stream write failed (reader dropped?): {error}");
            }
            if let Err(error) = writer.shutdown().await {
                tracing::trace!("mock stream shutdown failed: {error}");
            }
        });
        Ok(Box::new(reader))
    }

    async fn delete(&self, _path: &str) -> crate::errors::Result<()> {
        Ok(())
    }

    async fn exists(&self, _path: &str) -> crate::errors::Result<bool> {
        Ok(true)
    }

    async fn metadata(&self, _path: &str) -> crate::errors::Result<BlobMetadata> {
        Ok(BlobMetadata {
            size: self.bytes.len() as u64,
            content_type: Some("text/plain".to_string()),
        })
    }
}

impl CountingStreamDriver {
    fn with_presigned(self) -> PresignedCountingStreamDriver {
        PresignedCountingStreamDriver {
            inner: self,
            returns_url: true,
        }
    }

    fn with_unavailable_presigned(self) -> PresignedCountingStreamDriver {
        PresignedCountingStreamDriver {
            inner: self,
            returns_url: false,
        }
    }
}

#[derive(Clone)]
struct PresignedCountingStreamDriver {
    inner: CountingStreamDriver,
    returns_url: bool,
}

#[async_trait]
impl StorageDriver for PresignedCountingStreamDriver {
    async fn put(&self, path: &str, data: &[u8]) -> crate::errors::Result<String> {
        self.inner.put(path, data).await
    }

    async fn get(&self, path: &str) -> crate::errors::Result<Vec<u8>> {
        self.inner.get(path).await
    }

    async fn get_stream(
        &self,
        path: &str,
    ) -> crate::errors::Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.inner.get_stream(path).await
    }

    async fn delete(&self, path: &str) -> crate::errors::Result<()> {
        self.inner.delete(path).await
    }

    async fn exists(&self, path: &str) -> crate::errors::Result<bool> {
        self.inner.exists(path).await
    }

    async fn metadata(&self, path: &str) -> crate::errors::Result<BlobMetadata> {
        self.inner.metadata(path).await
    }

    fn extensions(&self) -> crate::storage::traits::StorageDriverExtensions<'_> {
        crate::storage::traits::StorageDriverExtensions {
            presigned: Some(self),
            ..Default::default()
        }
    }
}

#[async_trait]
impl PresignedStorageDriver for PresignedCountingStreamDriver {
    async fn presigned_url(
        &self,
        path: &str,
        _expires: Duration,
        options: PresignedDownloadOptions,
    ) -> crate::errors::Result<Option<String>> {
        if !self.returns_url {
            return Ok(None);
        }
        let mut url = reqwest::Url::parse("https://objects.example.test/download")
            .expect("mock presigned base URL should parse");
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("path", path);
            if let Some(value) = options.download_name {
                query.append_pair("download-name", &value);
            }
            if options.require_download_name_match {
                query.append_pair("require-download-name-match", "true");
            }
            if let Some(value) = options.response_cache_control {
                query.append_pair("response-cache-control", &value);
            }
            if let Some(value) = options.response_content_disposition {
                query.append_pair("response-content-disposition", &value);
            }
            if let Some(value) = options.response_content_type {
                query.append_pair("response-content-type", &value);
            }
        }
        Ok(Some(url.to_string()))
    }

    async fn presigned_put_url(
        &self,
        path: &str,
        _expires: Duration,
    ) -> crate::errors::Result<Option<String>> {
        Ok(Some(format!(
            "https://objects.example.test/upload?path={path}"
        )))
    }
}

async fn build_download_test_state(
    driver: impl StorageDriver + Clone + 'static,
    payload_size: i64,
) -> (
    PrimaryAppState,
    file::Model,
    file_blob::Model,
    impl StorageDriver + Clone + 'static,
) {
    build_download_test_state_with_policy(
        driver,
        payload_size,
        DriverType::Local,
        StoredStoragePolicyOptions::empty(),
        "text/plain",
    )
    .await
}

async fn build_download_test_state_with_policy<D>(
    driver: D,
    payload_size: i64,
    driver_type: DriverType,
    options: StoredStoragePolicyOptions,
    mime_type: &str,
) -> (PrimaryAppState, file::Model, file_blob::Model, D)
where
    D: StorageDriver + Clone + 'static,
{
    let temp_root = std::env::temp_dir().join(format!(
        "asterdrive-download-stream-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp_root).expect("download test temp root should exist");

    let db = crate::db::connect_with_metrics(
        &DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            pool_size: 1,
            retry_count: 0,
        },
        crate::metrics::NoopMetrics::arc(),
    )
    .await
    .expect("download test database should connect");
    Migrator::up(&db, None)
        .await
        .expect("download test migrations should succeed");

    let now = Utc::now();
    let policy = storage_policy::ActiveModel {
        name: Set("Download Stream Policy".to_string()),
        driver_type: Set(driver_type),
        endpoint: Set(String::new()),
        bucket: Set(String::new()),
        access_key: Set(String::new()),
        secret_key: Set(String::new()),
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
    .expect("download test policy should be inserted");

    let user = user::ActiveModel {
        username: Set("dldstream".to_string()),
        email: Set("dldstream@example.com".to_string()),
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
    .expect("download test user should be inserted");

    policy::ensure_policy_groups_seeded(&db)
        .await
        .expect("download test policy groups should be seeded");

    let policy_snapshot = Arc::new(PolicySnapshot::new());
    policy_snapshot
        .reload(&db)
        .await
        .expect("download test policy snapshot should reload");

    let driver_registry = Arc::new(DriverRegistry::noop());
    driver_registry.insert_for_test(policy.id, Arc::new(driver.clone()));

    let runtime_config = Arc::new(RuntimeConfig::new());
    let cache = cache::create_cache(&CacheConfig {
        ..Default::default()
    })
    .await;

    let mut config = Config::default();
    config.server.temp_dir = temp_root.join(".tmp").to_string_lossy().into_owned();
    config.server.upload_temp_dir = temp_root.join(".uploads").to_string_lossy().into_owned();

    let (storage_change_tx, _) = tokio::sync::broadcast::channel(
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
        metrics: crate::metrics::NoopMetrics::arc(),
        mail_sender: sender::runtime_sender(runtime_config),
        storage_change_tx,
        share_download_rollback,
        background_task_dispatch_wakeup:
            crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
        remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
        upload_runtime: crate::runtime::PrimaryAppState::new_upload_runtime(),
    };

    let blob = file_repo::create_blob(
        &db,
        file_blob::ActiveModel {
            hash: Set(format!("download-stream-{}", uuid::Uuid::new_v4())),
            size: Set(payload_size),
            policy_id: Set(policy.id),
            storage_path: Set(format!("files/{}", uuid::Uuid::new_v4())),
            ref_count: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .expect("download test blob should be inserted");

    let file = file_repo::create(
        &db,
        file::ActiveModel {
            name: Set("download.txt".to_string()),
            folder_id: Set(None),
            team_id: Set(None),
            blob_id: Set(blob.id),
            size: Set(payload_size),
            owner_user_id: Set(Some(user.id)),
            created_by_user_id: Set(Some(user.id)),
            created_by_username: Set(user.username.clone()),
            mime_type: Set(mime_type.to_string()),
            created_at: Set(now),
            updated_at: Set(now),
            deleted_at: Set(None),
            is_locked: Set(false),
            ..Default::default()
        },
    )
    .await
    .expect("download test file should be inserted");

    (state, file, blob, driver)
}

#[actix_web::test]
async fn build_stream_response_uses_get_stream_instead_of_get() {
    let payload = b"streamed download payload".to_vec();
    let driver = CountingStreamDriver::new(payload.clone());
    let get_calls = driver.get_calls.clone();
    let get_stream_calls = driver.get_stream_calls.clone();
    let (state, file, blob, _) = build_download_test_state(driver, payload_len_i64(&payload)).await;

    let outcome = build_download_outcome_with_disposition_and_range(
        &state,
        &file,
        &blob,
        DownloadDisposition::Attachment,
        None,
        None,
    )
    .await
    .expect("stream download outcome should build");

    let response = outcome_to_response(outcome);
    let body = body::to_bytes(response.into_body())
        .await
        .expect("stream response body should read");
    assert_eq!(body.as_ref(), payload.as_slice());
    assert_eq!(
        get_calls.load(Ordering::SeqCst),
        0,
        "download response must not fall back to StorageDriver::get()"
    );
    assert_eq!(
        get_stream_calls.load(Ordering::SeqCst),
        1,
        "download response should open exactly one streaming reader"
    );
}

fn presigned_download_options() -> StoredStoragePolicyOptions {
    StoredStoragePolicyOptions::from(
        r#"{"object_storage_download_strategy":"presigned"}"#.to_string(),
    )
}

fn provider_direct_download_options() -> StoredStoragePolicyOptions {
    StoredStoragePolicyOptions::from(
        r#"{"provider_download_strategy":"frontend_direct"}"#.to_string(),
    )
}

fn provider_direct_strict_filename_options() -> StoredStoragePolicyOptions {
    StoredStoragePolicyOptions::from(
        r#"{"provider_download_strategy":"frontend_direct","provider_download_filename_mode":"strict_current"}"#
            .to_string(),
    )
}

#[actix_web::test]
async fn attachment_download_redirects_to_presigned_url_with_attachment_disposition() {
    let payload = b"presigned attachment".to_vec();
    let base_driver = CountingStreamDriver::new(payload.clone());
    let get_stream_calls = base_driver.get_stream_calls.clone();
    let (state, file, blob, _) = build_download_test_state_with_policy(
        base_driver.with_presigned(),
        payload_len_i64(&payload),
        DriverType::S3,
        presigned_download_options(),
        "text/plain",
    )
    .await;

    let outcome = build_download_outcome_with_disposition_and_range(
        &state,
        &file,
        &blob,
        DownloadDisposition::Attachment,
        None,
        None,
    )
    .await
    .expect("attachment presigned outcome should build");

    let DownloadOutcome::PresignedRedirect { url } = outcome else {
        panic!("attachment downloads should redirect to presigned storage URL");
    };
    let parsed = reqwest::Url::parse(&url).expect("presigned URL should parse");
    let query = parsed
        .query_pairs()
        .into_owned()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(
        query
            .get("response-content-disposition")
            .map(String::as_str),
        Some("attachment; filename*=UTF-8''download.txt")
    );
    assert_eq!(
        query.get("download-name").map(String::as_str),
        Some("download.txt")
    );
    assert_eq!(
        query.get("response-content-type").map(String::as_str),
        Some("text/plain")
    );
    assert_eq!(
        get_stream_calls.load(Ordering::SeqCst),
        0,
        "presigned redirect must not open a backend stream"
    );
}

#[actix_web::test]
async fn safe_inline_preview_redirects_to_presigned_url_with_inline_disposition() {
    let payload = b"presigned inline".to_vec();
    let base_driver = CountingStreamDriver::new(payload.clone());
    let get_stream_calls = base_driver.get_stream_calls.clone();
    let (state, file, blob, _) = build_download_test_state_with_policy(
        base_driver.with_presigned(),
        payload_len_i64(&payload),
        DriverType::S3,
        presigned_download_options(),
        "image/webp",
    )
    .await;

    let outcome = build_download_outcome_with_disposition_and_range(
        &state,
        &file,
        &blob,
        DownloadDisposition::Inline,
        None,
        None,
    )
    .await
    .expect("safe inline presigned outcome should build");

    let DownloadOutcome::PresignedRedirect { url } = outcome else {
        panic!("safe inline previews should redirect to presigned storage URL");
    };
    let parsed = reqwest::Url::parse(&url).expect("presigned URL should parse");
    let query = parsed
        .query_pairs()
        .into_owned()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(
        query
            .get("response-content-disposition")
            .map(String::as_str),
        Some("inline; filename*=UTF-8''download.txt")
    );
    assert_eq!(
        query.get("response-content-type").map(String::as_str),
        Some("image/webp")
    );
    assert_eq!(
        get_stream_calls.load(Ordering::SeqCst),
        0,
        "presigned inline redirect must not open a backend stream"
    );
}

#[actix_web::test]
async fn onedrive_direct_download_redirects_only_when_explicitly_enabled() {
    let payload = b"provider direct download".to_vec();
    let direct_base_driver = CountingStreamDriver::new(payload.clone());
    let direct_stream_calls = direct_base_driver.get_stream_calls.clone();
    let (direct_state, direct_file, direct_blob, _) = build_download_test_state_with_policy(
        direct_base_driver.with_presigned(),
        payload_len_i64(&payload),
        DriverType::OneDrive,
        provider_direct_download_options(),
        "application/octet-stream",
    )
    .await;

    let direct = build_download_outcome_with_disposition_and_range(
        &direct_state,
        &direct_file,
        &direct_blob,
        DownloadDisposition::Attachment,
        None,
        None,
    )
    .await
    .expect("explicit OneDrive direct download should build");

    assert!(matches!(direct, DownloadOutcome::PresignedRedirect { .. }));
    assert_eq!(direct_stream_calls.load(Ordering::SeqCst), 0);

    let relay_base_driver = CountingStreamDriver::new(payload.clone());
    let relay_stream_calls = relay_base_driver.get_stream_calls.clone();
    let (relay_state, relay_file, relay_blob, _) = build_download_test_state_with_policy(
        relay_base_driver.with_presigned(),
        payload_len_i64(&payload),
        DriverType::OneDrive,
        StoredStoragePolicyOptions::empty(),
        "application/octet-stream",
    )
    .await;

    let relay = build_download_outcome_with_disposition_and_range(
        &relay_state,
        &relay_file,
        &relay_blob,
        DownloadDisposition::Attachment,
        None,
        None,
    )
    .await
    .expect("default OneDrive relay download should build");

    assert!(matches!(relay, DownloadOutcome::Stream(_)));
    assert_eq!(relay_stream_calls.load(Ordering::SeqCst), 1);
}

#[actix_web::test]
async fn onedrive_direct_download_keeps_range_request_on_redirect_path() {
    let payload = b"provider range download".to_vec();
    let base_driver = CountingStreamDriver::new(payload.clone());
    let get_stream_calls = base_driver.get_stream_calls.clone();
    let (state, file, blob, _) = build_download_test_state_with_policy(
        base_driver.with_presigned(),
        payload_len_i64(&payload),
        DriverType::OneDrive,
        provider_direct_download_options(),
        "application/octet-stream",
    )
    .await;

    let outcome = build_download_outcome_with_disposition_and_range(
        &state,
        &file,
        &blob,
        DownloadDisposition::Attachment,
        None,
        Some(
            ResolvedDownloadRange::new(3, 7, payload.len() as u64)
                .expect("test range should be valid"),
        ),
    )
    .await
    .expect("OneDrive range download should use provider redirect");

    assert!(matches!(outcome, DownloadOutcome::PresignedRedirect { .. }));
    assert_eq!(get_stream_calls.load(Ordering::SeqCst), 0);
}

#[actix_web::test]
async fn onedrive_strict_filename_mode_requires_provider_name_match() {
    let payload = b"strict filename download".to_vec();
    let base_driver = CountingStreamDriver::new(payload.clone());
    let (state, file, blob, _) = build_download_test_state_with_policy(
        base_driver.with_presigned(),
        payload_len_i64(&payload),
        DriverType::OneDrive,
        provider_direct_strict_filename_options(),
        "application/octet-stream",
    )
    .await;

    let outcome = build_download_outcome_with_disposition_and_range(
        &state,
        &file,
        &blob,
        DownloadDisposition::Attachment,
        None,
        None,
    )
    .await
    .expect("strict OneDrive download should build");

    match outcome {
        DownloadOutcome::PresignedRedirect { url } => {
            assert!(url.contains("require-download-name-match=true"));
        }
        DownloadOutcome::Stream(_) => {
            panic!("the mock provider should expose the direct-download decision");
        }
        DownloadOutcome::NotModified { .. } => {
            panic!("a fresh strict filename download cannot be not-modified");
        }
    }
}

#[actix_web::test]
async fn onedrive_direct_download_requires_runtime_temporary_url_capability() {
    let payload = b"missing provider capability".to_vec();
    let (state, file, blob, _) = build_download_test_state_with_policy(
        CountingStreamDriver::new(payload.clone()),
        payload_len_i64(&payload),
        DriverType::OneDrive,
        provider_direct_download_options(),
        "application/octet-stream",
    )
    .await;

    let error = build_download_outcome_with_disposition_and_range(
        &state,
        &file,
        &blob,
        DownloadDisposition::Attachment,
        None,
        None,
    )
    .await
    .unwrap_err();

    assert!(
        error
            .raw_message()
            .contains("presigned download not supported by driver")
    );
}

#[actix_web::test]
async fn onedrive_legacy_uuid_object_falls_back_to_same_origin_streaming() {
    let payload = b"legacy OneDrive object".to_vec();
    let base_driver = CountingStreamDriver::new(payload.clone());
    let get_stream_calls = base_driver.get_stream_calls.clone();
    let (state, file, blob, _) = build_download_test_state_with_policy(
        base_driver.with_unavailable_presigned(),
        payload_len_i64(&payload),
        DriverType::OneDrive,
        provider_direct_download_options(),
        "application/octet-stream",
    )
    .await;

    let outcome = build_download_outcome_with_disposition_and_range(
        &state,
        &file,
        &blob,
        DownloadDisposition::Attachment,
        None,
        None,
    )
    .await
    .expect("legacy OneDrive objects should use the stream fallback");

    assert!(matches!(outcome, DownloadOutcome::Stream(_)));
    assert_eq!(get_stream_calls.load(Ordering::SeqCst), 1);
}

#[actix_web::test]
async fn onedrive_direct_download_falls_back_for_conditional_and_sandboxed_inline_requests() {
    let payload = b"<script>alert(1)</script>".to_vec();
    for (mime_type, if_none_match) in [("image/webp", Some("\"stale-etag\"")), ("text/html", None)]
    {
        let base_driver = CountingStreamDriver::new(payload.clone());
        let get_stream_calls = base_driver.get_stream_calls.clone();
        let (state, file, blob, _) = build_download_test_state_with_policy(
            base_driver.with_presigned(),
            payload_len_i64(&payload),
            DriverType::OneDrive,
            provider_direct_download_options(),
            mime_type,
        )
        .await;

        let outcome = build_download_outcome_with_disposition_and_range(
            &state,
            &file,
            &blob,
            DownloadDisposition::Inline,
            if_none_match,
            None,
        )
        .await
        .expect("fallback request should stream through AsterDrive");

        assert!(matches!(outcome, DownloadOutcome::Stream(_)));
        assert_eq!(get_stream_calls.load(Ordering::SeqCst), 1);
    }
}

#[actix_web::test]
async fn conditional_miss_inline_preview_streams_instead_of_presigned_redirect() {
    let payload = b"changed presigned inline".to_vec();
    let base_driver = CountingStreamDriver::new(payload.clone());
    let get_stream_calls = base_driver.get_stream_calls.clone();
    let (state, file, blob, _) = build_download_test_state_with_policy(
        base_driver.with_presigned(),
        payload_len_i64(&payload),
        DriverType::S3,
        presigned_download_options(),
        "image/webp",
    )
    .await;

    let outcome = build_download_outcome_with_disposition_and_range(
        &state,
        &file,
        &blob,
        DownloadDisposition::Inline,
        Some("\"stale-etag\""),
        None,
    )
    .await
    .expect("conditional miss inline outcome should build");

    let DownloadOutcome::Stream(_) = outcome else {
        panic!(
            "conditional miss must stay same-origin instead of redirecting to presigned storage"
        );
    };
    assert_eq!(
        get_stream_calls.load(Ordering::SeqCst),
        1,
        "conditional miss should stream through backend"
    );
}

#[actix_web::test]
async fn sandboxed_inline_preview_does_not_redirect_to_presigned_storage() {
    let payload = b"<script>alert(1)</script>".to_vec();
    let base_driver = CountingStreamDriver::new(payload.clone());
    let get_stream_calls = base_driver.get_stream_calls.clone();
    let (state, file, blob, _) = build_download_test_state_with_policy(
        base_driver.with_presigned(),
        payload_len_i64(&payload),
        DriverType::S3,
        presigned_download_options(),
        "text/html",
    )
    .await;

    let outcome = build_download_outcome_with_disposition_and_range(
        &state,
        &file,
        &blob,
        DownloadDisposition::Inline,
        None,
        None,
    )
    .await
    .expect("sandboxed inline outcome should build");

    let response = outcome_to_response(outcome);
    assert_eq!(response.status(), actix_web::http::StatusCode::OK);
    assert_eq!(
        response.headers().get("Content-Security-Policy"),
        Some(&actix_web::http::header::HeaderValue::from_static(
            "sandbox"
        ))
    );
    assert_eq!(
        get_stream_calls.load(Ordering::SeqCst),
        1,
        "sandboxed inline preview should stream through backend to apply CSP"
    );
}
