use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use actix_web::body;
use async_trait::async_trait;
use chrono::Utc;
use migration::Migrator;
use sea_orm::{ActiveModelTrait, Set};
use tokio::io::{AsyncRead, AsyncWriteExt};

use crate::cache;
use crate::config::{CacheConfig, Config, DatabaseConfig, RuntimeConfig};
use crate::db::repository::file_repo;
use crate::entities::{file, file_blob, storage_policy, user};
use crate::runtime::PrimaryAppState;
use crate::services::file_service::DownloadDisposition;
use crate::services::{mail_service, policy_service};
use crate::storage::driver::BlobMetadata;
use crate::storage::{DriverRegistry, PolicySnapshot, StorageDriver};
use crate::types::{DriverType, StoredStoragePolicyAllowedTypes, UserRole, UserStatus};

use super::build::build_stream_outcome_with_disposition;
use super::response::outcome_to_response;
use super::streaming::AbortAwareStream;

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

async fn build_download_test_state(
    driver: CountingStreamDriver,
    payload_size: i64,
) -> (
    PrimaryAppState,
    file::Model,
    file_blob::Model,
    CountingStreamDriver,
) {
    let temp_root = std::env::temp_dir().join(format!(
        "asterdrive-download-stream-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp_root).expect("download test temp root should exist");

    let db = crate::db::connect(&DatabaseConfig {
        url: "sqlite::memory:".to_string(),
        pool_size: 1,
        retry_count: 0,
    })
    .await
    .expect("download test database should connect");
    Migrator::up(&db, None)
        .await
        .expect("download test migrations should succeed");

    let now = Utc::now();
    let policy = storage_policy::ActiveModel {
        name: Set("Download Stream Policy".to_string()),
        driver_type: Set(DriverType::Local),
        endpoint: Set(String::new()),
        bucket: Set(String::new()),
        access_key: Set(String::new()),
        secret_key: Set(String::new()),
        base_path: Set(temp_root.to_string_lossy().into_owned()),
        max_file_size: Set(0),
        allowed_types: Set(StoredStoragePolicyAllowedTypes::empty()),
        options: Set(crate::types::StoredStoragePolicyOptions::empty()),
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

    policy_service::ensure_policy_groups_seeded(&db)
        .await
        .expect("download test policy groups should be seeded");

    let policy_snapshot = Arc::new(PolicySnapshot::new());
    policy_snapshot
        .reload(&db)
        .await
        .expect("download test policy snapshot should reload");

    let driver_registry = Arc::new(DriverRegistry::new());
    driver_registry.insert_for_test(policy.id, Arc::new(driver.clone()));

    let runtime_config = Arc::new(RuntimeConfig::new());
    let cache = cache::create_cache(&CacheConfig {
        enabled: false,
        ..Default::default()
    })
    .await;

    let mut config = Config::default();
    config.server.temp_dir = temp_root.join(".tmp").to_string_lossy().into_owned();
    config.server.upload_temp_dir = temp_root.join(".uploads").to_string_lossy().into_owned();

    let (storage_change_tx, _) = tokio::sync::broadcast::channel(
        crate::services::storage_change_service::STORAGE_CHANGE_CHANNEL_CAPACITY,
    );
    let share_download_rollback =
        crate::services::share_service::spawn_detached_share_download_rollback_queue(
            db.clone(),
            crate::config::operations::share_download_rollback_queue_capacity(&runtime_config),
        );

    let state = PrimaryAppState {
        db: db.clone(),
        driver_registry,
        runtime_config: runtime_config.clone(),
        policy_snapshot,
        config: Arc::new(config),
        cache,
        mail_sender: mail_service::runtime_sender(runtime_config),
        storage_change_tx,
        share_download_rollback,
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
            mime_type: Set("text/plain".to_string()),
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
    let (state, file, blob, _) = build_download_test_state(driver, payload.len() as i64).await;

    let outcome = build_stream_outcome_with_disposition(
        &state,
        &file,
        &blob,
        DownloadDisposition::Attachment,
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
