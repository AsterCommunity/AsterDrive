use crate::cache;
use crate::config::{CacheConfig, Config, DatabaseConfig, RuntimeConfig};
use crate::db::repository::file_repo;
use crate::entities::{file, file_blob, storage_policy, user};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{mail_service, policy_service};
use crate::storage::BlobMetadata;
use crate::storage::{DriverRegistry, PolicySnapshot, StorageDriver, StreamUploadDriver};
use crate::types::{
    DriverType, S3UploadStrategy, StoragePolicyOptions, StoredStoragePolicyAllowedTypes, UserRole,
    UserStatus, serialize_storage_policy_options,
};
use crate::webdav::dav::{DavLock, DavLockError, DavLockSystem, LsFuture};
use crate::webdav::fs::AsterDavFs;
use crate::webdav::props::handle_propfind;
use crate::webdav::transfer::{handle_get_head, handle_put};
use actix_web::body::to_bytes;
use actix_web::http::{StatusCode, header};
use actix_web::{FromRequest, web};
use async_trait::async_trait;
use chrono::Utc;
use migration::Migrator;
use sea_orm::{ActiveModelTrait, Set};
use std::collections::HashMap;
use std::io::{self, Cursor};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, ReadBuf};
use xmltree::{Element, XMLNode};

async fn build_webdav_test_state(
    driver_type: DriverType,
    options: crate::types::StoredStoragePolicyOptions,
    driver: Arc<dyn StorageDriver>,
) -> (PrimaryAppState, user::Model, storage_policy::Model, PathBuf) {
    let temp_root = std::env::temp_dir().join(format!(
        "asterdrive-webdav-handler-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp_root).expect("webdav handler temp root should exist");

    let db = crate::db::connect_with_metrics(
        &DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            pool_size: 1,
            retry_count: 0,
        },
        crate::metrics_core::NoopMetrics::arc(),
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

    policy_service::ensure_policy_groups_seeded(&db)
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

    let (storage_change_tx, _) = tokio::sync::broadcast::channel(
        crate::services::storage_change_service::STORAGE_CHANGE_CHANNEL_CAPACITY,
    );
    let share_download_rollback =
        crate::services::share_service::spawn_detached_share_download_rollback_queue(
            db.clone(),
            crate::config::operations::share_download_rollback_queue_capacity(&runtime_config),
        );

    let state = PrimaryAppState {
        db_handles: crate::db::DbHandles::single(db.clone()),
        driver_registry,
        runtime_config: runtime_config.clone(),
        policy_snapshot,
        config: Arc::new(config),
        cache,
        metrics: crate::metrics_core::NoopMetrics::arc(),
        mail_sender: mail_service::runtime_sender(runtime_config),
        storage_change_tx,
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
            is_locked: Set(false),
            ..Default::default()
        },
    )
    .await
    .expect("webdav handler file should be inserted");

    (file, blob)
}

struct NoopLockSystem;

impl DavLockSystem for NoopLockSystem {
    fn lock(
        &self,
        _path: &crate::webdav::dav::DavPath,
        _principal: Option<&str>,
        _owner: Option<&xmltree::Element>,
        _timeout: Option<Duration>,
        _shared: bool,
        _deep: bool,
    ) -> LsFuture<'_, Result<DavLock, DavLockError>> {
        Box::pin(async { panic!("lock should not be called in these WebDAV handler tests") })
    }

    fn unlock(
        &self,
        _path: &crate::webdav::dav::DavPath,
        _token: &str,
    ) -> LsFuture<'_, Result<(), ()>> {
        Box::pin(async { Ok(()) })
    }

    fn refresh(
        &self,
        _path: &crate::webdav::dav::DavPath,
        _token: &str,
        _timeout: Option<Duration>,
    ) -> LsFuture<'_, Result<DavLock, ()>> {
        Box::pin(async { panic!("refresh should not be called in these WebDAV handler tests") })
    }

    fn check(
        &self,
        _path: &crate::webdav::dav::DavPath,
        _principal: Option<&str>,
        _ignore_principal: bool,
        _deep: bool,
        _submitted_tokens: &[String],
    ) -> LsFuture<'_, Result<(), DavLock>> {
        Box::pin(async { Ok(()) })
    }

    fn discover(&self, _path: &crate::webdav::dav::DavPath) -> LsFuture<'_, Vec<DavLock>> {
        Box::pin(async { Vec::new() })
    }

    fn conflicting_locks(
        &self,
        _path: &crate::webdav::dav::DavPath,
        _deep: bool,
    ) -> LsFuture<'_, Vec<DavLock>> {
        Box::pin(async { Vec::new() })
    }

    fn delete(&self, _path: &crate::webdav::dav::DavPath) -> LsFuture<'_, Result<(), ()>> {
        Box::pin(async { Ok(()) })
    }
}

struct OneChunkThenErrorReader {
    yielded_first_chunk: bool,
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
        Poll::Ready(Err(io::Error::other(
            "intentional trailing read failure for direct-stream regression test",
        )))
    }
}

#[derive(Clone, Default)]
struct TrailingErrorStreamDriver {
    get_stream_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl StorageDriver for TrailingErrorStreamDriver {
    async fn put(&self, path: &str, _data: &[u8]) -> crate::errors::Result<String> {
        Ok(path.to_string())
    }

    async fn get(&self, _path: &str) -> crate::errors::Result<Vec<u8>> {
        Err(crate::errors::AsterError::storage_driver_error(
            "WebDAV direct-stream test should not use get()",
        ))
    }

    async fn get_stream(
        &self,
        _path: &str,
    ) -> crate::errors::Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.get_stream_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(OneChunkThenErrorReader {
            yielded_first_chunk: false,
        }))
    }

    async fn delete(&self, _path: &str) -> crate::errors::Result<()> {
        Ok(())
    }

    async fn exists(&self, _path: &str) -> crate::errors::Result<bool> {
        Ok(true)
    }

    async fn metadata(&self, _path: &str) -> crate::errors::Result<BlobMetadata> {
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
    async fn put(&self, path: &str, data: &[u8]) -> crate::errors::Result<String> {
        self.objects
            .lock()
            .expect("direct upload test driver lock should succeed")
            .insert(path.to_string(), data.to_vec());
        Ok(path.to_string())
    }

    async fn get(&self, path: &str) -> crate::errors::Result<Vec<u8>> {
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
    ) -> crate::errors::Result<Box<dyn AsyncRead + Unpin + Send>> {
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

    async fn delete(&self, path: &str) -> crate::errors::Result<()> {
        self.objects
            .lock()
            .expect("direct upload test driver lock should succeed")
            .remove(path);
        Ok(())
    }

    async fn exists(&self, path: &str) -> crate::errors::Result<bool> {
        Ok(self
            .objects
            .lock()
            .expect("direct upload test driver lock should succeed")
            .contains_key(path))
    }

    async fn metadata(&self, path: &str) -> crate::errors::Result<BlobMetadata> {
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

    fn as_stream_upload(&self) -> Option<&dyn StreamUploadDriver> {
        Some(self)
    }
}

#[async_trait]
impl StreamUploadDriver for CountingDirectUploadDriver {
    async fn put_file(
        &self,
        storage_path: &str,
        local_path: &str,
    ) -> crate::errors::Result<String> {
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
    ) -> crate::errors::Result<String> {
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
    let (state, user, policy, temp_root) = build_webdav_test_state(
        DriverType::Local,
        crate::types::StoredStoragePolicyOptions::empty(),
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
    let lock_system = NoopLockSystem;
    let response = handle_get_head(&req, &dav_fs, &lock_system, "/webdav", false).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        get_stream_calls.load(Ordering::SeqCst),
        1,
        "GET should open exactly one streaming reader from storage"
    );

    drop(state);
    let _ = std::fs::remove_dir_all(temp_root);
}

#[actix_web::test]
async fn propfind_href_is_percent_encoded_and_xml_parseable() {
    let driver = CountingDirectUploadDriver::default();
    let (state, user, policy, temp_root) = build_webdav_test_state(
        DriverType::Local,
        crate::types::StoredStoragePolicyOptions::empty(),
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
    let lock_system = NoopLockSystem;
    let encoded_uri = format!("/webdav{}", super::encode_href(&format!("/{filename}")));
    let req = actix_web::test::TestRequest::default()
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").expect("valid method"))
        .uri(&encoded_uri)
        .insert_header((header::HeaderName::from_static("depth"), "0"))
        .to_http_request();

    let response = handle_propfind(&req, &dav_fs, &lock_system, "/webdav", &[]).await;

    assert_eq!(response.status(), StatusCode::from_u16(207).unwrap());
    let body = to_bytes(response.into_body())
        .await
        .expect("PROPFIND response body should be readable");

    let mut hrefs = Vec::new();
    let root =
        Element::parse(Cursor::new(body.as_ref())).expect("PROPFIND XML should parse cleanly");
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
        crate::types::StoredStoragePolicyOptions::empty(),
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
    let lock_system = NoopLockSystem;
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

    let response = handle_propfind(&req, &dav_fs, &lock_system, "/webdav", body).await;

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
        body_text.contains("<d:quota-used-bytes xmlns:d=\"DAV:\" />"),
        "missing DAV props should also declare the echoed lowercase DAV prefix: {body_text}"
    );
    Element::parse(Cursor::new(body_text.as_bytes())).expect("PROPFIND XML should parse cleanly");

    drop(state);
    let _ = std::fs::remove_dir_all(temp_root);
}

#[actix_web::test]
async fn propfind_allprop_keeps_default_dav_prefix_xml_parseable() {
    let driver = CountingDirectUploadDriver::default();
    let (state, user, policy, temp_root) = build_webdav_test_state(
        DriverType::Local,
        crate::types::StoredStoragePolicyOptions::empty(),
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
    let lock_system = NoopLockSystem;
    let req = actix_web::test::TestRequest::default()
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").expect("valid method"))
        .uri("/webdav/allprop.txt")
        .insert_header((header::HeaderName::from_static("depth"), "0"))
        .to_http_request();

    let response = handle_propfind(&req, &dav_fs, &lock_system, "/webdav", &[]).await;

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
    Element::parse(Cursor::new(body_text.as_bytes())).expect("PROPFIND XML should parse cleanly");

    drop(state);
    let _ = std::fs::remove_dir_all(temp_root);
}

fn collect_href_text(element: &Element, hrefs: &mut Vec<String>) {
    if (element.name == "href" || element.name == "D:href")
        && let Some(text) = element.get_text()
    {
        hrefs.push(text.into_owned());
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
        crate::types::StoredStoragePolicyOptions::empty(),
        Arc::new(driver),
    )
    .await;
    create_root_file(&state, user.id, policy.id, "head.txt", 3, "files/head.txt").await;

    let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
    let req = actix_web::test::TestRequest::default()
        .method(actix_web::http::Method::HEAD)
        .uri("/webdav/head.txt")
        .to_http_request();
    let lock_system = NoopLockSystem;
    let response = handle_get_head(&req, &dav_fs, &lock_system, "/webdav", true).await;

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
        s3_upload_strategy: Some(S3UploadStrategy::RelayStream),
        ..Default::default()
    })
    .expect("direct upload policy options should serialize");
    let (state, user, _policy, temp_root) =
        build_webdav_test_state(DriverType::S3, options, Arc::new(driver.clone())).await;

    let dav_fs = AsterDavFs::new(state.clone(), user.id, None);
    let lock_system = NoopLockSystem;
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
    let response = handle_put(
        &req,
        &dav_fs,
        &lock_system,
        "/webdav",
        &system_file_policy,
        &mut payload,
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
