//! Storage policy blob migration integration tests.

#[macro_use]
mod common;

use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

use actix_web::test;
use async_trait::async_trait;
use bytes::Bytes;
use chrono::Utc;
use parking_lot::Mutex;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde_json::Value;
use testcontainers::{GenericImage, ImageExt, runners::AsyncRunner};
use tokio::io::{AsyncRead, AsyncReadExt};

use aster_drive::db::repository::{
    background_task_repo, file_repo, policy_repo, storage_migration_checkpoint_repo,
};
use aster_drive::entities::{file, file_blob, file_version, storage_policy};
use aster_drive::errors::{AsterError, MapAsterErr, Result};
use aster_drive::runtime::{PrimaryAppState, SharedRuntimeState};
use aster_drive::services::task;
use aster_drive::storage::{
    BlobMetadata, MultipartStorageDriver, StorageDriver, StorageErrorKind, StreamUploadDriver,
};
use aster_drive::types::{
    BackgroundTaskStatus, DriverType, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions,
};
use aster_drive::utils::numbers::usize_to_u64;
use aster_forge_file_classification::FileCategory;

const RUSTFS_TEST_IMAGE_TAG: &str = "1.0.0-alpha.90";

struct RustFsTestContext {
    _container: testcontainers::ContainerAsync<GenericImage>,
    endpoint: String,
    bucket: String,
}

#[derive(Clone, Copy, Default)]
enum StreamUploadFailureMode {
    #[default]
    AfterWrite,
    BeforeWrite,
}

#[derive(Clone, Default)]
struct FailingStreamUploadDriver {
    objects: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    failure_mode: StreamUploadFailureMode,
    fail_delete: bool,
}

impl FailingStreamUploadDriver {
    fn before_write() -> Self {
        Self {
            failure_mode: StreamUploadFailureMode::BeforeWrite,
            ..Default::default()
        }
    }

    fn delete_fails() -> Self {
        Self {
            fail_delete: true,
            ..Default::default()
        }
    }

    fn contains(&self, path: &str) -> bool {
        self.objects.lock().contains_key(path)
    }

    fn bytes(&self, path: &str) -> Option<Vec<u8>> {
        self.objects.lock().get(path).cloned()
    }
}

#[async_trait]
impl StorageDriver for FailingStreamUploadDriver {
    async fn put(&self, path: &str, data: &[u8]) -> Result<String> {
        self.objects.lock().insert(path.to_string(), data.to_vec());
        Ok(path.to_string())
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>> {
        self.objects
            .lock()
            .get(path)
            .cloned()
            .ok_or_else(|| AsterError::storage_driver_error(format!("missing object {path}")))
    }

    async fn get_stream(&self, path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        Ok(Box::new(Cursor::new(self.get(path).await?)))
    }

    async fn delete(&self, path: &str) -> Result<()> {
        if self.fail_delete && !path.starts_with("_aster_migration_preflight-") {
            return Err(AsterError::storage_driver_error(
                "simulated cleanup delete failure",
            ));
        }
        self.objects.lock().remove(path);
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        Ok(self.contains(path))
    }

    async fn metadata(&self, path: &str) -> Result<BlobMetadata> {
        let size =
            self.objects.lock().get(path).map(Vec::len).ok_or_else(|| {
                AsterError::storage_driver_error(format!("missing object {path}"))
            })?;
        Ok(BlobMetadata {
            size: usize_to_u64(size, "test object size")?,
            content_type: None,
        })
    }

    fn as_stream_upload(&self) -> Option<&dyn StreamUploadDriver> {
        Some(self)
    }
}

#[async_trait]
impl StreamUploadDriver for FailingStreamUploadDriver {
    async fn put_reader(
        &self,
        storage_path: &str,
        mut reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        _size: i64,
    ) -> Result<String> {
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .await
            .map_aster_err_ctx("read test upload stream", AsterError::storage_driver_error)?;
        if matches!(self.failure_mode, StreamUploadFailureMode::AfterWrite) {
            self.objects.lock().insert(storage_path.to_string(), bytes);
        }
        Err(AsterError::storage_driver_error(
            "simulated timeout after remote write",
        ))
    }

    async fn put_file(&self, storage_path: &str, local_path: &str) -> Result<String> {
        let bytes = tokio::fs::read(local_path)
            .await
            .map_aster_err(AsterError::storage_driver_error)?;
        if matches!(self.failure_mode, StreamUploadFailureMode::AfterWrite) {
            self.objects.lock().insert(storage_path.to_string(), bytes);
        }
        Err(AsterError::storage_driver_error(
            "simulated timeout after remote write",
        ))
    }
}

#[derive(Clone, Copy)]
enum MultipartCompleteMode {
    Success,
    TransientErrorAfterWrite,
    CorruptObjectAfterWrite,
}

#[derive(Clone, Copy)]
enum MultipartPartFailure {
    TransientAttempts {
        part_number: i32,
        remaining_failures: usize,
    },
    Permanent {
        part_number: i32,
    },
}

struct MultipartUploadState {
    path: String,
    parts: BTreeMap<i32, Vec<u8>>,
    aborted: bool,
}

#[derive(Default)]
struct MultipartMigrationDriverState {
    objects: HashMap<String, Vec<u8>>,
    uploads: HashMap<String, MultipartUploadState>,
    next_upload_id: usize,
    part_attempts: HashMap<i32, usize>,
    uploaded_part_sizes: Vec<usize>,
    put_reader_calls: usize,
    complete_calls: usize,
    abort_calls: usize,
    delete_calls: usize,
    complete_mode: Option<MultipartCompleteMode>,
    part_failure: Option<MultipartPartFailure>,
}

#[derive(Clone, Default)]
struct MultipartMigrationTestDriver {
    state: Arc<Mutex<MultipartMigrationDriverState>>,
}

impl MultipartMigrationTestDriver {
    fn fail_part_transient_once(part_number: i32) -> Self {
        let driver = Self::default();
        driver.state.lock().part_failure = Some(MultipartPartFailure::TransientAttempts {
            part_number,
            remaining_failures: 1,
        });
        driver
    }

    fn fail_part_permanently(part_number: i32) -> Self {
        let driver = Self::default();
        driver.state.lock().part_failure = Some(MultipartPartFailure::Permanent { part_number });
        driver
    }

    fn complete_transient_after_write() -> Self {
        let driver = Self::default();
        driver.state.lock().complete_mode = Some(MultipartCompleteMode::TransientErrorAfterWrite);
        driver
    }

    fn complete_with_corrupt_object() -> Self {
        let driver = Self::default();
        driver.state.lock().complete_mode = Some(MultipartCompleteMode::CorruptObjectAfterWrite);
        driver
    }

    fn object_bytes(&self, path: &str) -> Option<Vec<u8>> {
        self.state.lock().objects.get(path).cloned()
    }

    fn uploaded_part_sizes(&self) -> Vec<usize> {
        self.state.lock().uploaded_part_sizes.clone()
    }

    fn part_attempt_count(&self, part_number: i32) -> usize {
        self.state
            .lock()
            .part_attempts
            .get(&part_number)
            .copied()
            .unwrap_or_default()
    }

    fn put_reader_calls(&self) -> usize {
        self.state.lock().put_reader_calls
    }

    fn complete_calls(&self) -> usize {
        self.state.lock().complete_calls
    }

    fn abort_calls(&self) -> usize {
        self.state.lock().abort_calls
    }

    fn delete_calls(&self) -> usize {
        self.state.lock().delete_calls
    }
}

#[async_trait]
impl StorageDriver for MultipartMigrationTestDriver {
    async fn put(&self, path: &str, data: &[u8]) -> Result<String> {
        self.state
            .lock()
            .objects
            .insert(path.to_string(), data.to_vec());
        Ok(path.to_string())
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>> {
        self.state
            .lock()
            .objects
            .get(path)
            .cloned()
            .ok_or_else(|| AsterError::storage_driver_error(format!("missing object {path}")))
    }

    async fn get_stream(&self, path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        Ok(Box::new(Cursor::new(self.get(path).await?)))
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let mut state = self.state.lock();
        state.delete_calls += 1;
        state.objects.remove(path);
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        Ok(self.state.lock().objects.contains_key(path))
    }

    async fn metadata(&self, path: &str) -> Result<BlobMetadata> {
        let size = self
            .state
            .lock()
            .objects
            .get(path)
            .map(Vec::len)
            .ok_or_else(|| AsterError::storage_driver_error(format!("missing object {path}")))?;
        Ok(BlobMetadata {
            size: usize_to_u64(size, "test multipart object size")?,
            content_type: None,
        })
    }

    fn as_stream_upload(&self) -> Option<&dyn StreamUploadDriver> {
        Some(self)
    }

    fn as_multipart(&self) -> Option<&dyn MultipartStorageDriver> {
        Some(self)
    }
}

#[async_trait]
impl StreamUploadDriver for MultipartMigrationTestDriver {
    async fn put_reader(
        &self,
        storage_path: &str,
        mut reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        _size: i64,
    ) -> Result<String> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await.map_aster_err_ctx(
            "read unexpected stream upload",
            AsterError::storage_driver_error,
        )?;
        let mut state = self.state.lock();
        state.put_reader_calls += 1;
        state.objects.insert(storage_path.to_string(), bytes);
        Ok(storage_path.to_string())
    }

    async fn put_file(&self, storage_path: &str, local_path: &str) -> Result<String> {
        let bytes = tokio::fs::read(local_path)
            .await
            .map_aster_err(AsterError::storage_driver_error)?;
        let mut state = self.state.lock();
        state.put_reader_calls += 1;
        state.objects.insert(storage_path.to_string(), bytes);
        Ok(storage_path.to_string())
    }
}

#[async_trait]
impl MultipartStorageDriver for MultipartMigrationTestDriver {
    async fn create_multipart_upload(&self, path: &str) -> Result<String> {
        let mut state = self.state.lock();
        state.next_upload_id += 1;
        let upload_id = format!("upload-{}", state.next_upload_id);
        state.uploads.insert(
            upload_id.clone(),
            MultipartUploadState {
                path: path.to_string(),
                parts: BTreeMap::new(),
                aborted: false,
            },
        );
        Ok(upload_id)
    }

    async fn presigned_upload_part_url(
        &self,
        _path: &str,
        _upload_id: &str,
        _part_number: i32,
        _expires: Duration,
    ) -> Result<String> {
        Err(aster_drive::storage::error::storage_driver_error(
            StorageErrorKind::Unsupported,
            "test driver does not presign multipart parts",
        ))
    }

    async fn complete_multipart_upload(
        &self,
        path: &str,
        upload_id: &str,
        parts: Vec<(i32, String)>,
    ) -> Result<()> {
        let mut state = self.state.lock();
        state.complete_calls += 1;
        let upload = state.uploads.get(upload_id).ok_or_else(|| {
            AsterError::storage_driver_error(format!("missing upload {upload_id}"))
        })?;
        assert_eq!(upload.path, path);
        assert!(
            !upload.aborted,
            "aborted multipart upload must not complete"
        );

        let mut assembled = Vec::new();
        for (part_number, _etag) in parts {
            let part = upload.parts.get(&part_number).ok_or_else(|| {
                AsterError::storage_driver_error(format!("missing part {part_number}"))
            })?;
            assembled.extend_from_slice(part);
        }

        let complete_mode = state
            .complete_mode
            .unwrap_or(MultipartCompleteMode::Success);
        let stored = match complete_mode {
            MultipartCompleteMode::Success | MultipartCompleteMode::TransientErrorAfterWrite => {
                assembled
            }
            MultipartCompleteMode::CorruptObjectAfterWrite => vec![0xA5; assembled.len()],
        };
        state.objects.insert(path.to_string(), stored);

        if matches!(
            complete_mode,
            MultipartCompleteMode::TransientErrorAfterWrite
        ) {
            return Err(aster_drive::storage::error::storage_driver_error(
                StorageErrorKind::Transient,
                "simulated complete timeout after object write",
            ));
        }
        Ok(())
    }

    async fn upload_multipart_part(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        data: &[u8],
    ) -> Result<String> {
        self.upload_multipart_part_bytes(path, upload_id, part_number, Bytes::copy_from_slice(data))
            .await
    }

    async fn upload_multipart_part_bytes(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        data: Bytes,
    ) -> Result<String> {
        let mut state = self.state.lock();
        let attempts = state.part_attempts.entry(part_number).or_default();
        *attempts += 1;
        let attempt = *attempts;

        let failure = match state.part_failure.as_mut() {
            Some(MultipartPartFailure::TransientAttempts {
                part_number: failing_part,
                remaining_failures,
            }) if *failing_part == part_number && *remaining_failures > 0 => {
                *remaining_failures -= 1;
                Some(aster_drive::storage::error::storage_driver_error(
                    StorageErrorKind::Transient,
                    "simulated transient multipart part failure",
                ))
            }
            Some(MultipartPartFailure::Permanent {
                part_number: failing_part,
            }) if *failing_part == part_number => {
                Some(aster_drive::storage::error::storage_driver_error(
                    StorageErrorKind::Permission,
                    "simulated permanent multipart part failure",
                ))
            }
            _ => None,
        };
        if let Some(error) = failure {
            return Err(error);
        }

        let upload = state.uploads.get_mut(upload_id).ok_or_else(|| {
            AsterError::storage_driver_error(format!("missing upload {upload_id}"))
        })?;
        assert_eq!(upload.path, path);
        assert!(
            !upload.aborted,
            "aborted multipart upload must not accept parts"
        );
        upload.parts.insert(part_number, data.to_vec());
        state.uploaded_part_sizes.push(data.len());
        Ok(format!("etag-{part_number}-{attempt}"))
    }

    async fn abort_multipart_upload(&self, path: &str, upload_id: &str) -> Result<()> {
        let mut state = self.state.lock();
        state.abort_calls += 1;
        let upload = state.uploads.get_mut(upload_id).ok_or_else(|| {
            AsterError::storage_driver_error(format!("missing upload {upload_id}"))
        })?;
        assert_eq!(upload.path, path);
        upload.aborted = true;
        Ok(())
    }

    async fn list_uploaded_part_details(
        &self,
        path: &str,
        upload_id: &str,
    ) -> Result<Vec<aster_drive::storage::UploadedMultipartPart>> {
        let state = self.state.lock();
        let upload = state.uploads.get(upload_id).ok_or_else(|| {
            AsterError::storage_driver_error(format!("missing upload {upload_id}"))
        })?;
        if upload.path != path {
            return Err(AsterError::storage_driver_error(format!(
                "upload {upload_id} path mismatch"
            )));
        }
        Ok(upload
            .parts
            .iter()
            .map(
                |(part_number, data)| aster_drive::storage::UploadedMultipartPart {
                    part_number: *part_number,
                    size: data.len() as i64,
                },
            )
            .collect())
    }
}

async fn start_rustfs_context(bucket_prefix: &str) -> RustFsTestContext {
    let container = GenericImage::new("rustfs/rustfs", RUSTFS_TEST_IMAGE_TAG)
        .with_exposed_port(testcontainers::core::IntoContainerPort::tcp(9000))
        .with_env_var("RUSTFS_ACCESS_KEY", "rustfsadmin")
        .with_env_var("RUSTFS_SECRET_KEY", "rustfsadmin123")
        .start()
        .await
        .expect("failed to start rustfs container");

    let port = container
        .get_host_port_ipv4(9000)
        .await
        .expect("rustfs port should resolve");
    let endpoint = format!("http://127.0.0.1:{port}");
    let bucket = format!("{bucket_prefix}-{}", uuid::Uuid::new_v4().simple());
    wait_for_s3_bucket(&endpoint, &bucket).await;

    RustFsTestContext {
        _container: container,
        endpoint,
        bucket,
    }
}

async fn create_local_policy(state: &PrimaryAppState, name: &str) -> storage_policy::Model {
    let now = Utc::now();
    let base_path = format!("{}/policy-{name}", state.config.server.temp_dir);
    tokio::fs::create_dir_all(&base_path)
        .await
        .expect("policy test dir should be created");
    policy_repo::create(
        state.writer_db(),
        storage_policy::ActiveModel {
            name: Set(name.to_string()),
            driver_type: Set(DriverType::Local),
            endpoint: Set(String::new()),
            bucket: Set(String::new()),
            access_key: Set(String::new()),
            secret_key: Set(String::new()),
            base_path: Set(base_path),
            max_file_size: Set(0),
            allowed_types: Set(StoredStoragePolicyAllowedTypes::empty()),
            options: Set(StoredStoragePolicyOptions::empty()),
            is_default: Set(false),
            chunk_size: Set(5_242_880),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .expect("local policy should insert")
}

async fn create_s3_policy(
    state: &PrimaryAppState,
    name: &str,
    endpoint: &str,
    bucket: &str,
) -> storage_policy::Model {
    let now = Utc::now();
    let policy = policy_repo::create(
        state.writer_db(),
        storage_policy::ActiveModel {
            name: Set(name.to_string()),
            driver_type: Set(DriverType::S3),
            endpoint: Set(endpoint.to_string()),
            bucket: Set(bucket.to_string()),
            access_key: Set("rustfsadmin".to_string()),
            secret_key: Set("rustfsadmin123".to_string()),
            base_path: Set(format!("migration-{name}")),
            max_file_size: Set(0),
            allowed_types: Set(StoredStoragePolicyAllowedTypes::empty()),
            options: Set(StoredStoragePolicyOptions::from(
                r#"{"object_storage_upload_strategy":"relay_stream"}"#.to_string(),
            )),
            is_default: Set(false),
            chunk_size: Set(5_242_880),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .expect("s3 policy should insert");
    state.driver_registry.invalidate(policy.id);
    policy
}

fn s3_test_client(endpoint: &str) -> aws_sdk_s3::Client {
    let credentials =
        aws_credential_types::Credentials::new("rustfsadmin", "rustfsadmin123", None, None, "test");
    let config = aws_sdk_s3::Config::builder()
        .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
        .region(aws_sdk_s3::config::Region::new("us-east-1"))
        .credentials_provider(credentials)
        .endpoint_url(endpoint)
        .force_path_style(true)
        .build();
    aws_sdk_s3::Client::from_conf(config)
}

async fn try_create_s3_bucket(endpoint: &str, bucket: &str) -> std::result::Result<(), String> {
    use aws_sdk_s3::error::ProvideErrorMetadata;

    let client = s3_test_client(endpoint);
    if let Err(err) = client.create_bucket().bucket(bucket).send().await {
        let code = err
            .as_service_error()
            .and_then(|service_err| service_err.code());
        if matches!(
            code,
            Some("BucketAlreadyOwnedByYou") | Some("BucketAlreadyExists")
        ) {
            return Ok(());
        }
        return Err(err.to_string());
    }
    Ok(())
}

async fn wait_for_s3_bucket(endpoint: &str, bucket: &str) {
    let mut last_err: Option<String> = None;
    let ready = tokio::time::timeout(std::time::Duration::from_secs(45), async {
        loop {
            match tokio::time::timeout(
                std::time::Duration::from_secs(3),
                try_create_s3_bucket(endpoint, bucket),
            )
            .await
            {
                Ok(Ok(())) => break,
                Ok(Err(err)) => last_err = Some(err),
                Err(_) => last_err = Some("create_bucket attempt timed out".to_string()),
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    })
    .await;

    if ready.is_err() {
        panic!(
            "timed out waiting for S3 bucket {bucket} at {endpoint}: {}",
            last_err.unwrap_or_else(|| "unknown error".to_string())
        );
    }
}

async fn read_s3_object(endpoint: &str, bucket: &str, key: &str) -> Vec<u8> {
    let object = s3_test_client(endpoint)
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .expect("s3 object should exist");
    object
        .body
        .collect()
        .await
        .expect("s3 object body should read")
        .into_bytes()
        .to_vec()
}

async fn s3_object_exists(endpoint: &str, bucket: &str, key: &str) -> bool {
    s3_test_client(endpoint)
        .head_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .is_ok()
}

fn policy_object_key(policy: &storage_policy::Model, storage_path: &str) -> String {
    format!("{}/{}", policy.base_path.trim_matches('/'), storage_path)
}

fn multipart_test_bytes(size: usize) -> Vec<u8> {
    (0..size)
        .map(|index| u8::try_from(index % 251).expect("test byte pattern should fit u8"))
        .collect()
}

async fn create_blob_with_object(
    state: &PrimaryAppState,
    policy: &storage_policy::Model,
    bytes: &[u8],
    ref_count: i32,
) -> file_blob::Model {
    let hash = aster_drive::utils::hash::sha256_hex(bytes);
    let storage_path = aster_forge_validation::filename::storage_path_from_blob_key(&hash).unwrap();
    let full_path = std::path::Path::new(&policy.base_path).join(&storage_path);
    tokio::fs::create_dir_all(full_path.parent().expect("blob path should have parent"))
        .await
        .expect("blob parent should be created");
    tokio::fs::write(&full_path, bytes)
        .await
        .expect("blob object should be written");
    let now = Utc::now();
    file_blob::ActiveModel {
        hash: Set(hash),
        size: Set(i64::try_from(bytes.len()).expect("test bytes len should fit i64")),
        policy_id: Set(policy.id),
        storage_path: Set(storage_path),
        thumbnail_path: Set(Some("old-thumb".to_string())),
        thumbnail_processor: Set(Some("old-processor".to_string())),
        thumbnail_version: Set(Some("old-version".to_string())),
        ref_count: Set(ref_count),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("blob row should insert")
}

async fn create_blob_record_with_storage_path(
    state: &PrimaryAppState,
    policy: &storage_policy::Model,
    hash: &str,
    storage_path: &str,
    bytes: &[u8],
    ref_count: i32,
) -> file_blob::Model {
    let now = Utc::now();
    file_blob::ActiveModel {
        hash: Set(hash.to_string()),
        size: Set(i64::try_from(bytes.len()).expect("test bytes len should fit i64")),
        policy_id: Set(policy.id),
        storage_path: Set(storage_path.to_string()),
        thumbnail_path: Set(None),
        thumbnail_processor: Set(None),
        thumbnail_version: Set(None),
        ref_count: Set(ref_count),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("blob row with explicit storage path should insert")
}

async fn create_opaque_blob_with_object(
    state: &PrimaryAppState,
    policy: &storage_policy::Model,
    blob_key: &str,
    bytes: &[u8],
    ref_count: i32,
) -> file_blob::Model {
    let storage_path =
        aster_forge_validation::filename::storage_path_from_blob_key(blob_key).unwrap();
    let full_path = std::path::Path::new(&policy.base_path).join(&storage_path);
    tokio::fs::create_dir_all(full_path.parent().expect("blob path should have parent"))
        .await
        .expect("blob parent should be created");
    tokio::fs::write(&full_path, bytes)
        .await
        .expect("blob object should be written");
    let now = Utc::now();
    file_blob::ActiveModel {
        hash: Set(blob_key.to_string()),
        size: Set(i64::try_from(bytes.len()).expect("test bytes len should fit i64")),
        policy_id: Set(policy.id),
        storage_path: Set(storage_path),
        thumbnail_path: Set(None),
        thumbnail_processor: Set(None),
        thumbnail_version: Set(None),
        ref_count: Set(ref_count),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("opaque blob row should insert")
}

async fn create_file_for_blob(state: &PrimaryAppState, blob_id: i64, name: &str) -> file::Model {
    let now = Utc::now();
    file::ActiveModel {
        name: Set(name.to_string()),
        folder_id: Set(None),
        team_id: Set(None),
        blob_id: Set(blob_id),
        size: Set(1),
        owner_user_id: Set(None),
        created_by_user_id: Set(None),
        created_by_username: Set("tester".to_string()),
        mime_type: Set("text/plain".to_string()),
        extension: Set("txt".to_string()),
        compound_extension: Set(None),
        file_category: Set(FileCategory::Document),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        is_locked: Set(false),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("file row should insert")
}

async fn create_version_for_blob(
    state: &PrimaryAppState,
    file_id: i64,
    blob_id: i64,
    version: i32,
) -> file_version::Model {
    file_version::ActiveModel {
        file_id: Set(file_id),
        blob_id: Set(blob_id),
        version: Set(version),
        size: Set(1),
        created_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("file version row should insert")
}

async fn create_migration_task_via_api(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: &str,
    source_policy_id: i64,
    target_policy_id: i64,
    delete_source_after_success: bool,
) -> Value {
    let (_, body) = create_migration_task_via_api_with_status(
        app,
        token,
        source_policy_id,
        target_policy_id,
        delete_source_after_success,
    )
    .await;
    body
}

async fn create_migration_task_via_api_with_status(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: &str,
    source_policy_id: i64,
    target_policy_id: i64,
    delete_source_after_success: bool,
) -> (actix_web::http::StatusCode, Value) {
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/storage-migrations")
        .insert_header(("Cookie", common::access_cookie_header(token)))
        .insert_header(common::csrf_header_for(token))
        .set_json(serde_json::json!({
            "source_policy_id": source_policy_id,
            "target_policy_id": target_policy_id,
            "delete_source_after_success": delete_source_after_success,
        }))
        .to_request();
    let resp = test::call_service(app, req).await;
    let status = resp.status();
    let body = test::read_body_json(resp).await;
    (status, body)
}

async fn dry_run_migration_via_api(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: &str,
    source_policy_id: i64,
    target_policy_id: i64,
) -> (actix_web::http::StatusCode, Value) {
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/storage-migrations/dry-run")
        .insert_header(("Cookie", common::access_cookie_header(token)))
        .insert_header(common::csrf_header_for(token))
        .set_json(serde_json::json!({
            "source_policy_id": source_policy_id,
            "target_policy_id": target_policy_id,
        }))
        .to_request();
    let resp = test::call_service(app, req).await;
    let status = resp.status();
    let body = test::read_body_json(resp).await;
    (status, body)
}

fn assert_conflicting_storage_migration_response(
    status: actix_web::http::StatusCode,
    body: &Value,
) {
    assert_eq!(status, actix_web::http::StatusCode::BAD_REQUEST);
    let code = body["code"].as_str().expect("error code should be string");
    assert_ne!(code, "success");
    assert!(
        body["msg"]
            .as_str()
            .expect("error message should exist")
            .contains("conflicting active storage policy migration")
    );
}

async fn set_background_task_status(
    state: &PrimaryAppState,
    task_id: i64,
    status: BackgroundTaskStatus,
) {
    let mut task_update: aster_drive::entities::background_task::ActiveModel =
        background_task_repo::find_by_id(state.writer_db(), task_id)
            .await
            .expect("task should exist")
            .into();
    task_update.status = Set(status);
    if status.is_terminal() {
        task_update.finished_at = Set(Some(Utc::now()));
    }
    task_update
        .update(state.writer_db())
        .await
        .expect("task status should update");
}

#[actix_web::test]
async fn test_storage_migration_api_creates_task_and_checkpoint() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-create").await;
    let target = create_local_policy(&state, "target-create").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;

    assert_eq!(body["code"], "success");
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    assert_eq!(body["data"]["kind"], "storage_policy_migration");
    assert_eq!(
        body["data"]["payload"]["source_policy_id"].as_i64(),
        Some(source.id)
    );
    assert_eq!(
        body["data"]["payload"]["target_policy_id"].as_i64(),
        Some(target.id)
    );

    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.source_policy_id, source.id);
    assert_eq!(checkpoint.target_policy_id, target.id);
    assert_eq!(checkpoint.last_processed_blob_id, 0);
}

#[actix_web::test]
async fn test_storage_migration_dry_run_reports_preflight_summary() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-dry-run").await;
    let target = create_local_policy(&state, "target-dry-run").await;
    let source_blob = create_blob_with_object(&state, &source, b"dedup-me", 1).await;
    create_opaque_blob_with_object(&state, &source, "opaque-dry-run-key", b"opaque", 1).await;
    create_blob_with_object(&state, &target, b"dedup-me", 1).await;

    let (status, body) = dry_run_migration_via_api(&app, &token, source.id, target.id).await;

    assert_eq!(status, actix_web::http::StatusCode::OK);
    assert_eq!(body["code"], "success");
    let data = &body["data"];
    assert_eq!(data["source_policy_id"], source.id);
    assert_eq!(data["target_policy_id"], target.id);
    assert_eq!(data["source_blob_count"], 2);
    assert_eq!(
        data["source_total_bytes"].as_i64(),
        Some(source_blob.size + 6)
    );
    assert_eq!(data["content_sha256_blob_count"], 1);
    assert_eq!(data["opaque_blob_count"], 1);
    assert_eq!(data["target_matching_blob_count"], 1);
    assert_eq!(data["estimated_copy_blob_count"], 1);
    assert_eq!(data["target_supports_stream_upload"], true);
    assert_eq!(data["target_connection_ok"], true);
    assert_eq!(data["target_capacity_check"], "sufficient");
    assert_eq!(data["target_capacity"]["status"], "supported");
    assert_eq!(data["target_capacity"]["source"], "local_filesystem");
    assert!(
        data["target_capacity"]["available_bytes"]
            .as_i64()
            .expect("available bytes should be present")
            >= 6
    );
    assert_eq!(data["delete_source_after_success_supported"], false);
    assert_eq!(data["can_start"], true);
    assert_eq!(data["warnings"].as_array().unwrap(), &Vec::<Value>::new());
}

#[actix_web::test]
async fn test_storage_migration_capacity_preflight_uses_estimated_copy_bytes() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-capacity-estimate").await;
    let target = create_local_policy(&state, "target-capacity-estimate").await;

    let shared = create_blob_with_object(&state, &source, b"already-present-large", 1).await;
    let missing = create_blob_with_object(&state, &source, b"x", 1).await;
    create_blob_with_object(&state, &target, b"already-present-large", 1).await;

    let missing_summary = file_repo::summarize_missing_blobs_between_policies(
        state.writer_db(),
        source.id,
        target.id,
    )
    .await
    .expect("missing blob summary should load");
    assert_eq!(missing_summary.count, 1);
    assert_eq!(missing_summary.total_size, missing.size);

    let (status, body) = dry_run_migration_via_api(&app, &token, source.id, target.id).await;

    assert_eq!(status, actix_web::http::StatusCode::OK);
    assert_eq!(body["code"], "success");
    let data = &body["data"];
    assert_eq!(data["source_blob_count"], 2);
    assert_eq!(
        data["source_total_bytes"].as_i64(),
        Some(shared.size + missing.size)
    );
    assert_eq!(data["target_matching_blob_count"], 1);
    assert_eq!(data["estimated_copy_blob_count"], 1);
    assert_eq!(data["target_capacity_check"], "sufficient");
    assert!(
        data["target_capacity"]["available_bytes"]
            .as_i64()
            .expect("available bytes should be present")
            >= missing.size
    );
}

#[actix_web::test]
async fn test_storage_migration_dry_run_does_not_match_opaque_blob_keys() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-opaque-dry-run").await;
    let target = create_local_policy(&state, "target-opaque-dry-run").await;
    let source_blob =
        create_opaque_blob_with_object(&state, &source, "opaque-shared-key", b"same", 1).await;
    create_opaque_blob_with_object(&state, &target, "opaque-shared-key", b"same", 1).await;

    let missing_summary = file_repo::summarize_missing_blobs_between_policies(
        state.writer_db(),
        source.id,
        target.id,
    )
    .await
    .expect("missing blob summary should load");
    assert_eq!(missing_summary.count, 1);
    assert_eq!(missing_summary.total_size, source_blob.size);

    let (status, body) = dry_run_migration_via_api(&app, &token, source.id, target.id).await;

    assert_eq!(status, actix_web::http::StatusCode::OK);
    let data = &body["data"];
    assert_eq!(data["opaque_blob_count"], 1);
    assert_eq!(data["opaque_key_conflict_count"], 1);
    assert_eq!(data["target_matching_blob_count"], 0);
    assert_eq!(data["estimated_copy_blob_count"], 1);
}

#[actix_web::test]
async fn test_storage_policy_capacity_api_reports_local_filesystem_capacity() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let policy = create_local_policy(&state, "capacity-api").await;
    let blob = create_blob_with_object(&state, &policy, b"capacity-api-blob", 1).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/policies/{}/capacity", policy.id))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;

    assert_eq!(body["code"], "success");
    let data = &body["data"];
    assert_eq!(data["policy_id"], policy.id);
    assert_eq!(data["driver_type"], "local");
    assert_eq!(data["blob_count"], 1);
    assert_eq!(data["blob_total_bytes"], blob.size);
    assert_eq!(data["capacity"]["status"], "supported");
    assert_eq!(data["capacity"]["source"], "local_filesystem");
    assert!(
        data["capacity"]["total_bytes"]
            .as_i64()
            .expect("total bytes should be present")
            > 0
    );
    assert!(
        data["capacity"]["available_bytes"]
            .as_i64()
            .expect("available bytes should be present")
            > 0
    );
}

#[actix_web::test]
async fn test_storage_migration_dry_run_requires_admin() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    admin_create_user!(
        app,
        admin_token,
        "migrationdryuser",
        "migration-dry-user@example.com",
        "password123"
    );
    let (plain_token, _) = login_user!(app, "migrationdryuser", "password123");
    let source = create_local_policy(&state, "source-dry-auth").await;
    let target = create_local_policy(&state, "target-dry-auth").await;

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/storage-migrations/dry-run")
        .insert_header(("Cookie", common::access_cookie_header(&plain_token)))
        .insert_header(common::csrf_header_for(&plain_token))
        .set_json(serde_json::json!({
            "source_policy_id": source.id,
            "target_policy_id": target.id,
        }))
        .to_request();
    let err = test::try_call_service(&app, req).await.unwrap_err();
    assert_eq!(
        err.error_response().status(),
        actix_web::http::StatusCode::FORBIDDEN
    );
}

#[actix_web::test]
async fn test_storage_migration_api_rejects_source_deletion_flag() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-delete-flag").await;
    let target = create_local_policy(&state, "target-delete-flag").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, true).await;

    let code = body["code"].as_str().expect("error code should be string");
    assert_ne!(code, "success");
    assert!(
        body["msg"]
            .as_str()
            .expect("error message should exist")
            .contains("delete_source_after_success")
    );
}

#[actix_web::test]
async fn test_storage_migration_resume_reuses_checkpoint_after_failed_task() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-resume").await;
    let target = create_local_policy(&state, "target-resume").await;
    let first = create_blob_with_object(&state, &source, b"first", 1).await;
    let second = create_blob_with_object(&state, &source, b"second", 1).await;
    create_file_for_blob(&state, first.id, "first.txt").await;
    create_file_for_blob(&state, second.id, "second.txt").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let second_path = std::path::Path::new(&source.base_path).join(&second.storage_path);
    tokio::fs::write(&second_path, b"bad!!!")
        .await
        .expect("second source object should be tampered before migration starts");

    let stats = task::drain(&state)
        .await
        .expect("first migration attempt should drain");
    assert_eq!(stats.failed, 1);
    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.last_processed_blob_id, first.id);
    assert_eq!(checkpoint.migrated_blobs, 1);

    tokio::fs::write(&second_path, b"second")
        .await
        .expect("second source object should be restored before resume");
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/admin/storage-migrations/{task_id}/resume"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "pending");

    let stats = task::drain(&state)
        .await
        .expect("resumed migration task should drain");
    assert_eq!(stats.succeeded, 1);
    let final_checkpoint =
        storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
            .await
            .expect("checkpoint should exist");
    assert_eq!(final_checkpoint.stage, "complete");
    assert_eq!(final_checkpoint.last_processed_blob_id, second.id);
    assert_eq!(final_checkpoint.migrated_blobs, 2);

    let migrated_second = file_repo::find_blob_by_id(state.writer_db(), second.id)
        .await
        .expect("second blob should exist");
    assert_eq!(migrated_second.policy_id, target.id);
}

#[actix_web::test]
async fn test_storage_migration_moves_blob_to_empty_target_policy() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-move").await;
    let target = create_local_policy(&state, "target-move").await;
    let blob = create_blob_with_object(&state, &source, b"move-me", 1).await;
    create_file_for_blob(&state, blob.id, "move.txt").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let stats = task::drain(&state)
        .await
        .expect("migration task should drain");
    assert_eq!(stats.succeeded, 1);

    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("task should exist");
    assert_eq!(task.status, BackgroundTaskStatus::Succeeded);
    let migrated = file_repo::find_blob_by_id(state.writer_db(), blob.id)
        .await
        .expect("blob should still exist");
    assert_eq!(migrated.policy_id, target.id);
    assert_eq!(
        migrated.storage_path,
        aster_forge_validation::filename::storage_path_from_blob_key(&blob.hash).unwrap()
    );
    assert!(migrated.thumbnail_path.is_none());
    assert!(
        std::path::Path::new(&target.base_path)
            .join(&migrated.storage_path)
            .exists()
    );
    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.scanned_blobs, 1);
    assert_eq!(checkpoint.migrated_blobs, 1);
    assert_eq!(checkpoint.merged_blobs, 0);
    assert_eq!(checkpoint.migrated_bytes, 7);
}

#[actix_web::test]
async fn test_storage_migration_moves_opaque_local_blob_key_without_content_hash_mismatch() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-opaque").await;
    let target = create_local_policy(&state, "target-opaque").await;
    let blob = create_opaque_blob_with_object(
        &state,
        &source,
        "8a7ab9852bc34e98ac1fd29ddd94365b",
        b"opaque blob bytes",
        1,
    )
    .await;
    create_file_for_blob(&state, blob.id, "opaque.txt").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let stats = task::drain(&state)
        .await
        .expect("opaque migration task should drain");
    assert_eq!(stats.succeeded, 1);

    let migrated = file_repo::find_blob_by_id(state.writer_db(), blob.id)
        .await
        .expect("blob should still exist");
    assert_eq!(migrated.hash, blob.hash);
    assert_eq!(migrated.policy_id, target.id);
    let target_object =
        tokio::fs::read(std::path::Path::new(&target.base_path).join(
            aster_forge_validation::filename::storage_path_from_blob_key(&blob.hash).unwrap(),
        ))
        .await
        .expect("target object should exist");
    assert_eq!(target_object, b"opaque blob bytes");

    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("task should exist");
    assert_eq!(task.status, BackgroundTaskStatus::Succeeded);
    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.renamed_opaque_blobs, 0);
}

#[tokio::test]
async fn test_storage_migration_local_to_rustfs_s3_e2e() {
    let rustfs = start_rustfs_context("migration-e2e").await;

    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-rustfs-e2e").await;
    let target = create_s3_policy(
        &state,
        "target-rustfs-e2e",
        &rustfs.endpoint,
        &rustfs.bucket,
    )
    .await;
    let bytes = b"opaque local blob migrated into real rustfs s3";
    let blob = create_opaque_blob_with_object(
        &state,
        &source,
        "8a7ab9852bc34e98ac1fd29ddd94365b",
        bytes,
        1,
    )
    .await;
    create_file_for_blob(&state, blob.id, "rustfs-e2e.txt").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    assert_eq!(body["code"], "success");
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let stats = task::drain(&state)
        .await
        .expect("local to rustfs migration task should drain");
    assert_eq!(stats.succeeded, 1);

    let migrated = file_repo::find_blob_by_id(state.writer_db(), blob.id)
        .await
        .expect("blob row should remain after policy move");
    assert_eq!(migrated.hash, blob.hash);
    assert_eq!(migrated.policy_id, target.id);
    assert_eq!(
        migrated.storage_path,
        aster_forge_validation::filename::storage_path_from_blob_key(&blob.hash).unwrap()
    );
    let object_key = policy_object_key(&target, &migrated.storage_path);
    let target_bytes = read_s3_object(&rustfs.endpoint, &rustfs.bucket, &object_key).await;
    assert_eq!(target_bytes, bytes);

    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("task should exist");
    assert_eq!(task.status, BackgroundTaskStatus::Succeeded);
    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.stage, "complete");
    assert_eq!(checkpoint.scanned_blobs, 1);
    assert_eq!(checkpoint.migrated_blobs, 1);
    assert_eq!(checkpoint.migrated_bytes, bytes.len() as i64);
}

#[tokio::test]
async fn test_storage_migration_local_to_rustfs_s3_resume_after_partial_failure_e2e() {
    let rustfs = start_rustfs_context("migration-resume-e2e").await;

    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-rustfs-resume").await;
    let target = create_s3_policy(
        &state,
        "target-rustfs-resume",
        &rustfs.endpoint,
        &rustfs.bucket,
    )
    .await;
    let first_bytes = b"resume-first-original-0001";
    let second_bytes = b"resume-second-original-0001";
    let tampered_second_bytes = b"resume-second-tampered-0001";
    assert_eq!(second_bytes.len(), tampered_second_bytes.len());

    let first = create_blob_with_object(&state, &source, first_bytes, 1).await;
    let second = create_blob_with_object(&state, &source, second_bytes, 1).await;
    create_file_for_blob(&state, first.id, "resume-first.txt").await;
    create_file_for_blob(&state, second.id, "resume-second.txt").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    assert_eq!(body["code"], "success");
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");

    let second_source_path = std::path::Path::new(&source.base_path).join(&second.storage_path);
    tokio::fs::write(&second_source_path, tampered_second_bytes)
        .await
        .expect("second source object should be tampered before first run");
    let stats = task::drain(&state)
        .await
        .expect("first rustfs resume attempt should drain");
    assert_eq!(stats.failed, 1);

    let failed_task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("failed task should exist");
    assert_eq!(failed_task.status, BackgroundTaskStatus::Failed);
    assert!(
        failed_task
            .last_error
            .as_deref()
            .expect("failed task should store last_error")
            .contains("copied blob hash mismatch")
    );
    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist after failure");
    assert_eq!(checkpoint.stage, "migrate_blobs");
    assert_eq!(checkpoint.last_processed_blob_id, first.id);
    assert_eq!(checkpoint.scanned_blobs, 1);
    assert_eq!(checkpoint.migrated_blobs, 1);
    assert_eq!(checkpoint.migrated_bytes, first_bytes.len() as i64);

    let first_after_failure = file_repo::find_blob_by_id(state.writer_db(), first.id)
        .await
        .expect("first blob should remain after first run");
    assert_eq!(first_after_failure.policy_id, target.id);
    let first_key = policy_object_key(&target, &first_after_failure.storage_path);
    assert_eq!(
        read_s3_object(&rustfs.endpoint, &rustfs.bucket, &first_key).await,
        first_bytes
    );
    let failed_second_key = policy_object_key(
        &target,
        &aster_forge_validation::filename::storage_path_from_blob_key(&second.hash).unwrap(),
    );
    assert!(
        !s3_object_exists(&rustfs.endpoint, &rustfs.bucket, &failed_second_key).await,
        "failed copy should cleanup the partially written second target object"
    );

    tokio::fs::write(&second_source_path, second_bytes)
        .await
        .expect("second source object should be restored before resume");
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/admin/storage-migrations/{task_id}/resume"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "pending");

    let stats = task::drain(&state)
        .await
        .expect("resumed rustfs migration should drain");
    assert_eq!(stats.succeeded, 1);

    let second_after_resume = file_repo::find_blob_by_id(state.writer_db(), second.id)
        .await
        .expect("second blob should remain after resume");
    assert_eq!(second_after_resume.policy_id, target.id);
    let second_key = policy_object_key(&target, &second_after_resume.storage_path);
    assert_eq!(
        read_s3_object(&rustfs.endpoint, &rustfs.bucket, &second_key).await,
        second_bytes
    );

    let final_checkpoint =
        storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
            .await
            .expect("checkpoint should exist after resume");
    assert_eq!(final_checkpoint.stage, "complete");
    assert_eq!(final_checkpoint.last_processed_blob_id, second.id);
    assert_eq!(final_checkpoint.scanned_blobs, 2);
    assert_eq!(final_checkpoint.migrated_blobs, 2);
    assert_eq!(
        final_checkpoint.migrated_bytes,
        i64::try_from(first_bytes.len() + second_bytes.len())
            .expect("test byte length should fit i64")
    );
}

#[actix_web::test]
async fn test_storage_migration_crosses_batch_boundary_and_merges_existing_target_blob() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-bulk").await;
    let target = create_local_policy(&state, "target-bulk").await;
    let mut source_blobs = Vec::new();
    let mut source_bytes = Vec::new();

    for index in 0..105 {
        let bytes = format!("bulk migration payload #{index:03}").into_bytes();
        let blob = if index % 10 == 0 {
            create_opaque_blob_with_object(
                &state,
                &source,
                &format!("opaque-bulk-key-{index:03}"),
                &bytes,
                1,
            )
            .await
        } else {
            create_blob_with_object(&state, &source, &bytes, 1).await
        };
        create_file_for_blob(&state, blob.id, &format!("bulk-{index:03}.txt")).await;
        source_blobs.push(blob);
        source_bytes.push(bytes);
    }

    let merge_index = 42;
    let target_duplicate =
        create_blob_with_object(&state, &target, &source_bytes[merge_index], 3).await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    assert_eq!(body["code"], "success");
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let stats = task::drain(&state)
        .await
        .expect("bulk migration task should drain");
    assert_eq!(stats.succeeded, 1);

    for (index, blob) in source_blobs.iter().enumerate() {
        if index == merge_index {
            assert!(
                file_repo::find_blob_by_id(state.writer_db(), blob.id)
                    .await
                    .is_err(),
                "merged source blob row should be deleted"
            );
            continue;
        }

        let migrated = file_repo::find_blob_by_id(state.writer_db(), blob.id)
            .await
            .expect("migrated source blob row should remain");
        assert_eq!(migrated.policy_id, target.id);
        let target_object =
            tokio::fs::read(std::path::Path::new(&target.base_path).join(&migrated.storage_path))
                .await
                .expect("target object should exist for migrated blob");
        assert_eq!(target_object, source_bytes[index]);
    }

    let merged_target = file_repo::find_blob_by_id(state.writer_db(), target_duplicate.id)
        .await
        .expect("target duplicate blob should remain after merge");
    assert_eq!(merged_target.ref_count, 4);
    let merged_file = file::Entity::find()
        .filter(file::Column::Name.eq("bulk-042.txt"))
        .one(state.writer_db())
        .await
        .expect("merged file query should succeed")
        .expect("merged file should exist");
    assert_eq!(merged_file.blob_id, target_duplicate.id);

    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.stage, "complete");
    assert_eq!(
        checkpoint.last_processed_blob_id,
        source_blobs.last().expect("source blobs should exist").id
    );
    assert_eq!(checkpoint.scanned_blobs, 105);
    assert_eq!(checkpoint.migrated_blobs, 104);
    assert_eq!(checkpoint.merged_blobs, 1);
    assert_eq!(checkpoint.skipped_blobs, 0);
    assert_eq!(checkpoint.failed_blobs, 0);
    let expected_bytes = source_bytes.iter().try_fold(0_i64, |acc, bytes| {
        i64::try_from(bytes.len()).map(|len| acc + len)
    });
    assert_eq!(
        checkpoint.migrated_bytes,
        expected_bytes.expect("bulk test byte length should fit i64")
    );
}

#[actix_web::test]
async fn test_storage_migration_merges_when_target_blob_already_exists() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-merge").await;
    let target = create_local_policy(&state, "target-merge").await;
    let source_blob = create_blob_with_object(&state, &source, b"same-bytes", 2).await;
    let target_blob = create_blob_with_object(&state, &target, b"same-bytes", 1).await;
    let active_file = create_file_for_blob(&state, source_blob.id, "active.txt").await;
    create_version_for_blob(&state, active_file.id, source_blob.id, 1).await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let stats = task::drain(&state)
        .await
        .expect("migration task should drain");
    assert_eq!(stats.succeeded, 1);

    assert!(
        file_repo::find_blob_by_id(state.writer_db(), source_blob.id)
            .await
            .is_err(),
        "old source blob row should be deleted after merge"
    );
    let merged_target = file_repo::find_blob_by_id(state.writer_db(), target_blob.id)
        .await
        .expect("target blob should exist");
    assert_eq!(merged_target.ref_count, 3);
    let updated_file = file::Entity::find_by_id(active_file.id)
        .one(state.writer_db())
        .await
        .expect("file query should succeed")
        .expect("file should exist");
    assert_eq!(updated_file.blob_id, target_blob.id);
    let updated_version = file_version::Entity::find()
        .filter(file_version::Column::FileId.eq(active_file.id))
        .one(state.writer_db())
        .await
        .expect("version query should succeed")
        .expect("version should exist");
    assert_eq!(updated_version.blob_id, target_blob.id);
    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.scanned_blobs, 1);
    assert_eq!(checkpoint.migrated_blobs, 0);
    assert_eq!(checkpoint.merged_blobs, 1);
    assert_eq!(checkpoint.migrated_bytes, 10);
}

#[actix_web::test]
async fn test_storage_migration_fails_when_content_hash_matches_but_size_differs() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-hash-size-mismatch").await;
    let target = create_local_policy(&state, "target-hash-size-mismatch").await;
    let source_blob = create_blob_with_object(&state, &source, b"same-hash-source", 1).await;
    let target_blob = create_blob_with_object(&state, &target, b"x", 1).await;
    let mut target_update: file_blob::ActiveModel = target_blob.into();
    target_update.hash = Set(source_blob.hash.clone());
    target_update
        .update(state.writer_db())
        .await
        .expect("target hash should be forced for boundary test");

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let stats = task::drain(&state)
        .await
        .expect("migration task should drain");
    assert_eq!(stats.failed, 1);

    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("task should exist");
    assert_eq!(task.status, BackgroundTaskStatus::Failed);
    let last_error = task
        .last_error
        .as_deref()
        .expect("failed task should store last_error");
    assert!(
        last_error.contains("target blob record does not match source blob"),
        "{last_error}"
    );
    let source_after = file_repo::find_blob_by_id(state.writer_db(), source_blob.id)
        .await
        .expect("source blob should remain");
    assert_eq!(source_after.policy_id, source.id);
}

#[actix_web::test]
async fn test_storage_migration_does_not_merge_opaque_blob_key_with_same_size() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-opaque-same-size").await;
    let target = create_local_policy(&state, "target-opaque-same-size").await;
    let source_blob =
        create_opaque_blob_with_object(&state, &source, "opaque-shared-same-size", b"source", 1)
            .await;
    let target_blob =
        create_opaque_blob_with_object(&state, &target, "opaque-shared-same-size", b"target", 1)
            .await;
    create_file_for_blob(&state, source_blob.id, "opaque-source.txt").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let stats = task::drain(&state)
        .await
        .expect("opaque same-size migration task should drain");
    assert_eq!(stats.succeeded, 1);

    let migrated = file_repo::find_blob_by_id(state.writer_db(), source_blob.id)
        .await
        .expect("source blob row should be moved, not merged");
    assert_eq!(migrated.policy_id, target.id);
    assert_ne!(migrated.hash, target_blob.hash);
    assert!(migrated.hash.starts_with("migration-"));
    assert_ne!(migrated.storage_path, target_blob.storage_path);
    let migrated_bytes =
        tokio::fs::read(std::path::Path::new(&target.base_path).join(&migrated.storage_path))
            .await
            .expect("migrated opaque object should exist");
    assert_eq!(migrated_bytes, b"source");
    let target_bytes =
        tokio::fs::read(std::path::Path::new(&target.base_path).join(&target_blob.storage_path))
            .await
            .expect("existing opaque target object should remain");
    assert_eq!(target_bytes, b"target");

    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.migrated_blobs, 1);
    assert_eq!(checkpoint.merged_blobs, 0);
    assert_eq!(checkpoint.renamed_opaque_blobs, 1);
}

#[actix_web::test]
async fn test_storage_migration_does_not_merge_opaque_blob_key_with_different_size() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-opaque-different-size").await;
    let target = create_local_policy(&state, "target-opaque-different-size").await;
    let source_blob = create_opaque_blob_with_object(
        &state,
        &source,
        "opaque-shared-different-size",
        b"source-long",
        1,
    )
    .await;
    let target_blob = create_opaque_blob_with_object(
        &state,
        &target,
        "opaque-shared-different-size",
        b"target",
        1,
    )
    .await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let stats = task::drain(&state)
        .await
        .expect("opaque different-size migration task should drain");
    assert_eq!(stats.succeeded, 1);

    let migrated = file_repo::find_blob_by_id(state.writer_db(), source_blob.id)
        .await
        .expect("source blob row should be moved, not merged");
    assert_eq!(migrated.policy_id, target.id);
    assert_ne!(migrated.hash, target_blob.hash);
    assert!(migrated.hash.starts_with("migration-"));
    assert_eq!(migrated.size, source_blob.size);
    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.migrated_blobs, 1);
    assert_eq!(checkpoint.merged_blobs, 0);
    assert_eq!(checkpoint.renamed_opaque_blobs, 1);
}

#[actix_web::test]
async fn test_storage_migration_empty_source_succeeds_with_zero_counts() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-empty").await;
    let target = create_local_policy(&state, "target-empty").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let stats = task::drain(&state)
        .await
        .expect("empty migration task should drain");
    assert_eq!(stats.succeeded, 1);

    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("task should exist");
    assert_eq!(task.status, BackgroundTaskStatus::Succeeded);
    assert_eq!(task.progress_current, 0);
    assert_eq!(task.progress_total, 0);

    let result: aster_drive::services::task::types::StoragePolicyMigrationTaskResult =
        serde_json::from_str(
            task.result_json
                .as_ref()
                .map(AsRef::as_ref)
                .expect("successful migration should store result"),
        )
        .expect("result json should parse");
    assert_eq!(result.scanned_blobs, 0);
    assert_eq!(result.migrated_blobs, 0);
    assert_eq!(result.merged_blobs, 0);
    assert_eq!(result.skipped_blobs, 0);
    assert_eq!(result.failed_blobs, 0);
    assert_eq!(result.migrated_bytes, 0);

    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.stage, "complete");
    assert_eq!(checkpoint.last_processed_blob_id, 0);
    assert_eq!(checkpoint.scanned_blobs, 0);
    assert_eq!(checkpoint.migrated_blobs, 0);
    assert_eq!(checkpoint.merged_blobs, 0);
    assert_eq!(checkpoint.skipped_blobs, 0);
    assert_eq!(checkpoint.failed_blobs, 0);
}

#[actix_web::test]
async fn test_storage_migration_cleans_target_object_when_verification_fails() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-cleanup").await;
    let target = create_local_policy(&state, "target-cleanup").await;
    let blob = create_blob_with_object(&state, &source, b"cleanup-me", 1).await;
    create_file_for_blob(&state, blob.id, "cleanup.txt").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let source_full_path = std::path::Path::new(&source.base_path).join(&blob.storage_path);
    tokio::fs::write(&source_full_path, b"bad-data!!")
        .await
        .expect("source object should be tampered before migration starts");

    let stats = task::drain(&state)
        .await
        .expect("failed migration task should drain");
    assert_eq!(stats.failed, 1);

    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("task should exist");
    assert_eq!(task.status, BackgroundTaskStatus::Failed);
    assert!(
        task.last_error
            .as_deref()
            .expect("failed task should store last_error")
            .contains("copied blob hash mismatch")
    );

    let source_blob = file_repo::find_blob_by_id(state.writer_db(), blob.id)
        .await
        .expect("source blob row should remain");
    assert_eq!(source_blob.policy_id, source.id);
    let target_driver = state
        .driver_registry
        .get_driver(&target)
        .expect("target driver should exist");
    let target_path =
        aster_forge_validation::filename::storage_path_from_blob_key(&blob.hash).unwrap();
    assert!(
        !target_driver
            .exists(&target_path)
            .await
            .expect("target existence check should succeed"),
        "failed migration should cleanup the target object"
    );

    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.stage, "migrate_blobs");
    assert_eq!(checkpoint.last_processed_blob_id, 0);
    assert_eq!(checkpoint.scanned_blobs, 0);
    assert_eq!(checkpoint.migrated_blobs, 0);
}

#[actix_web::test]
async fn test_storage_migration_cleans_target_object_when_stream_upload_returns_error() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-upload-error-cleanup").await;
    let target = create_local_policy(&state, "target-upload-error-cleanup").await;
    let target_driver = FailingStreamUploadDriver::default();
    state
        .driver_registry
        .insert_for_test(target.id, Arc::new(target_driver.clone()));
    let blob = create_blob_with_object(&state, &source, b"cleanup-after-upload-error", 1).await;
    create_file_for_blob(&state, blob.id, "upload-error-cleanup.txt").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let target_path =
        aster_forge_validation::filename::storage_path_from_blob_key(&blob.hash).unwrap();

    let stats = task::drain(&state)
        .await
        .expect("failed migration task should drain");
    assert_eq!(stats.failed, 1);

    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("task should exist");
    assert_eq!(task.status, BackgroundTaskStatus::Failed);
    assert!(
        task.last_error
            .as_deref()
            .expect("failed task should store last_error")
            .contains("simulated timeout after remote write")
    );
    assert!(
        !target_driver.contains(&target_path),
        "failed stream upload should cleanup the target object left before the error"
    );

    let source_blob = file_repo::find_blob_by_id(state.writer_db(), blob.id)
        .await
        .expect("source blob row should remain");
    assert_eq!(source_blob.policy_id, source.id);
}

#[actix_web::test]
async fn test_storage_migration_stream_upload_error_without_target_object_stays_failed() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-upload-error-no-object").await;
    let target = create_local_policy(&state, "target-upload-error-no-object").await;
    let target_driver = FailingStreamUploadDriver::before_write();
    state
        .driver_registry
        .insert_for_test(target.id, Arc::new(target_driver.clone()));
    let blob = create_blob_with_object(&state, &source, b"upload-error-before-write", 1).await;
    create_file_for_blob(&state, blob.id, "upload-error-before-write.txt").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let target_path =
        aster_forge_validation::filename::storage_path_from_blob_key(&blob.hash).unwrap();

    let stats = task::drain(&state)
        .await
        .expect("failed migration task should drain");
    assert_eq!(stats.failed, 1);

    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("task should exist");
    assert_eq!(task.status, BackgroundTaskStatus::Failed);
    assert!(
        task.last_error
            .as_deref()
            .expect("failed task should store last_error")
            .contains("simulated timeout after remote write")
    );
    assert!(
        !target_driver.contains(&target_path),
        "cleanup should tolerate upload errors that left no target object"
    );
}

#[actix_web::test]
async fn test_storage_migration_stream_upload_cleanup_error_preserves_upload_error() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-upload-cleanup-delete-error").await;
    let target = create_local_policy(&state, "target-upload-cleanup-delete-error").await;
    let target_driver = FailingStreamUploadDriver::delete_fails();
    state
        .driver_registry
        .insert_for_test(target.id, Arc::new(target_driver.clone()));
    let blob = create_blob_with_object(&state, &source, b"cleanup-delete-error", 1).await;
    create_file_for_blob(&state, blob.id, "cleanup-delete-error.txt").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let target_path =
        aster_forge_validation::filename::storage_path_from_blob_key(&blob.hash).unwrap();

    let stats = task::drain(&state)
        .await
        .expect("failed migration task should drain");
    assert_eq!(stats.failed, 1);

    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("task should exist");
    assert_eq!(task.status, BackgroundTaskStatus::Failed);
    let last_error = task
        .last_error
        .as_deref()
        .expect("failed task should store last_error");
    assert!(
        last_error.contains("simulated timeout after remote write"),
        "cleanup failure should not replace the original upload error"
    );
    assert!(
        !last_error.contains("simulated cleanup delete failure"),
        "cleanup failure should stay a warning, not the task failure reason"
    );
    assert!(
        target_driver.contains(&target_path),
        "object remains when best-effort cleanup itself fails"
    );
}

#[actix_web::test]
async fn test_storage_migration_stream_upload_error_does_not_delete_referenced_target_object() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-upload-cleanup-referenced").await;
    let target = create_local_policy(&state, "target-upload-cleanup-referenced").await;
    let target_driver = FailingStreamUploadDriver::default();
    state
        .driver_registry
        .insert_for_test(target.id, Arc::new(target_driver.clone()));
    let bytes = b"referenced-target-object";
    let blob = create_blob_with_object(&state, &source, bytes, 1).await;
    create_file_for_blob(&state, blob.id, "cleanup-referenced.txt").await;
    let target_path =
        aster_forge_validation::filename::storage_path_from_blob_key(&blob.hash).unwrap();
    target_driver
        .put(&target_path, bytes)
        .await
        .expect("referenced target object should be seeded");
    let referenced_hash = aster_drive::utils::hash::sha256_hex(b"separate referenced blob row");
    let referenced = create_blob_record_with_storage_path(
        &state,
        &target,
        &referenced_hash,
        &target_path,
        bytes,
        1,
    )
    .await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");

    let stats = task::drain(&state)
        .await
        .expect("failed migration task should drain");
    assert_eq!(stats.failed, 1);

    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("task should exist");
    assert_eq!(task.status, BackgroundTaskStatus::Failed);
    assert!(
        target_driver.contains(&target_path),
        "cleanup must not delete a target object while a blob row still references its path"
    );
    assert_eq!(
        target_driver.bytes(&target_path).as_deref(),
        Some(bytes.as_slice()),
        "referenced target object bytes should remain intact"
    );
    let referenced_after = file_repo::find_blob_by_id(state.writer_db(), referenced.id)
        .await
        .expect("referenced target blob row should remain");
    assert_eq!(referenced_after.policy_id, target.id);
    assert_eq!(referenced_after.storage_path, target_path);
}

#[actix_web::test]
async fn test_storage_migration_large_blob_uses_multipart_upload() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-multipart-success").await;
    let target = create_local_policy(&state, "target-multipart-success").await;
    let target_driver = MultipartMigrationTestDriver::default();
    state
        .driver_registry
        .insert_for_test(target.id, Arc::new(target_driver.clone()));
    let bytes = multipart_test_bytes(5 * 1024 * 1024 + 17);
    let blob = create_blob_with_object(&state, &source, &bytes, 1).await;
    create_file_for_blob(&state, blob.id, "multipart-success.7z").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let target_path =
        aster_forge_validation::filename::storage_path_from_blob_key(&blob.hash).unwrap();
    let stats = task::drain(&state)
        .await
        .expect("multipart migration task should drain");
    assert_eq!(stats.succeeded, 1);

    let migrated = file_repo::find_blob_by_id(state.writer_db(), blob.id)
        .await
        .expect("blob should remain after multipart migration");
    assert_eq!(migrated.policy_id, target.id);
    assert_eq!(target_driver.put_reader_calls(), 0);
    assert_eq!(
        target_driver.uploaded_part_sizes(),
        vec![5 * 1024 * 1024, 17]
    );
    assert_eq!(
        target_driver.object_bytes(&target_path),
        Some(bytes.clone())
    );

    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("task should exist");
    assert_eq!(task.status, BackgroundTaskStatus::Succeeded);
    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.stage, "complete");
    assert_eq!(checkpoint.migrated_blobs, 1);
    assert_eq!(checkpoint.migrated_bytes, blob.size);
}

#[actix_web::test]
async fn test_storage_migration_multipart_retries_transient_part_upload() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-multipart-retry").await;
    let target = create_local_policy(&state, "target-multipart-retry").await;
    let target_driver = MultipartMigrationTestDriver::fail_part_transient_once(1);
    state
        .driver_registry
        .insert_for_test(target.id, Arc::new(target_driver.clone()));
    let bytes = multipart_test_bytes(5 * 1024 * 1024 + 3);
    let blob = create_blob_with_object(&state, &source, &bytes, 1).await;
    create_file_for_blob(&state, blob.id, "multipart-retry.7z").await;

    create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let target_path =
        aster_forge_validation::filename::storage_path_from_blob_key(&blob.hash).unwrap();
    let stats = task::drain(&state)
        .await
        .expect("multipart retry migration task should drain");
    assert_eq!(stats.succeeded, 1);

    assert_eq!(target_driver.part_attempt_count(1), 2);
    assert_eq!(target_driver.part_attempt_count(2), 1);
    assert_eq!(target_driver.abort_calls(), 0);
    assert_eq!(target_driver.object_bytes(&target_path), Some(bytes));
    let migrated = file_repo::find_blob_by_id(state.writer_db(), blob.id)
        .await
        .expect("blob should move after retry succeeds");
    assert_eq!(migrated.policy_id, target.id);
}

#[actix_web::test]
async fn test_storage_migration_multipart_part_failure_aborts_and_keeps_source_blob() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-multipart-part-fail").await;
    let target = create_local_policy(&state, "target-multipart-part-fail").await;
    let target_driver = MultipartMigrationTestDriver::fail_part_permanently(2);
    state
        .driver_registry
        .insert_for_test(target.id, Arc::new(target_driver.clone()));
    let bytes = multipart_test_bytes(5 * 1024 * 1024 + 9);
    let blob = create_blob_with_object(&state, &source, &bytes, 1).await;
    create_file_for_blob(&state, blob.id, "multipart-part-fail.7z").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let target_path =
        aster_forge_validation::filename::storage_path_from_blob_key(&blob.hash).unwrap();
    let stats = task::drain(&state)
        .await
        .expect("multipart failing task should drain");
    assert_eq!(stats.failed, 1);

    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("task should exist");
    assert_eq!(task.status, BackgroundTaskStatus::Failed);
    assert!(
        task.last_error
            .as_deref()
            .expect("failed task should store last_error")
            .contains("simulated permanent multipart part failure")
    );
    assert_eq!(target_driver.abort_calls(), 1);
    assert_eq!(target_driver.complete_calls(), 0);
    assert_eq!(target_driver.object_bytes(&target_path), None);
    let source_blob = file_repo::find_blob_by_id(state.writer_db(), blob.id)
        .await
        .expect("source blob row should remain when multipart part fails");
    assert_eq!(source_blob.policy_id, source.id);
}

#[actix_web::test]
async fn test_storage_migration_multipart_complete_timeout_accepts_existing_object() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-multipart-complete-timeout").await;
    let target = create_local_policy(&state, "target-multipart-complete-timeout").await;
    let target_driver = MultipartMigrationTestDriver::complete_transient_after_write();
    state
        .driver_registry
        .insert_for_test(target.id, Arc::new(target_driver.clone()));
    let bytes = multipart_test_bytes(5 * 1024 * 1024 + 11);
    let blob = create_blob_with_object(&state, &source, &bytes, 1).await;
    create_file_for_blob(&state, blob.id, "multipart-complete-timeout.7z").await;

    create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let target_path =
        aster_forge_validation::filename::storage_path_from_blob_key(&blob.hash).unwrap();
    let stats = task::drain(&state)
        .await
        .expect("multipart complete-timeout task should drain");
    assert_eq!(stats.succeeded, 1);

    assert_eq!(target_driver.complete_calls(), 1);
    assert_eq!(target_driver.abort_calls(), 0);
    assert_eq!(target_driver.object_bytes(&target_path), Some(bytes));
    let migrated = file_repo::find_blob_by_id(state.writer_db(), blob.id)
        .await
        .expect("blob should move when complete timeout is proven successful");
    assert_eq!(migrated.policy_id, target.id);
}

#[actix_web::test]
async fn test_storage_migration_multipart_verification_failure_cleans_object_and_rolls_back_row() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-multipart-verify-fail").await;
    let target = create_local_policy(&state, "target-multipart-verify-fail").await;
    let target_driver = MultipartMigrationTestDriver::complete_with_corrupt_object();
    state
        .driver_registry
        .insert_for_test(target.id, Arc::new(target_driver.clone()));
    let bytes = multipart_test_bytes(5 * 1024 * 1024 + 5);
    let blob = create_blob_with_object(&state, &source, &bytes, 1).await;
    create_file_for_blob(&state, blob.id, "multipart-verify-fail.7z").await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");
    let target_path =
        aster_forge_validation::filename::storage_path_from_blob_key(&blob.hash).unwrap();
    let stats = task::drain(&state)
        .await
        .expect("multipart verification failure task should drain");
    assert_eq!(stats.failed, 1);

    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("task should exist");
    assert_eq!(task.status, BackgroundTaskStatus::Failed);
    assert!(
        task.last_error
            .as_deref()
            .expect("failed task should store last_error")
            .contains("target object hash mismatch")
    );
    assert_eq!(target_driver.abort_calls(), 0);
    assert_eq!(target_driver.delete_calls(), 2);
    assert_eq!(target_driver.object_bytes(&target_path), None);
    let source_blob = file_repo::find_blob_by_id(state.writer_db(), blob.id)
        .await
        .expect("source blob row should remain after verification failure");
    assert_eq!(source_blob.policy_id, source.id);

    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.stage, "migrate_blobs");
    assert_eq!(checkpoint.last_processed_blob_id, 0);
    assert_eq!(checkpoint.migrated_blobs, 0);
}

#[actix_web::test]
async fn test_storage_migration_fails_when_policy_changes_after_task_creation() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-changed").await;
    let target = create_local_policy(&state, "target-changed").await;
    create_blob_with_object(&state, &source, b"do-not-move", 1).await;

    let body = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    let task_id = body["data"]["id"].as_i64().expect("task id should exist");

    let mut target_update: storage_policy::ActiveModel = target.clone().into();
    target_update.base_path = Set(format!("{}-changed", target.base_path));
    target_update.updated_at = Set(Utc::now() + chrono::Duration::seconds(1));
    target_update
        .update(state.writer_db())
        .await
        .expect("target policy should update");

    let stats = task::drain(&state)
        .await
        .expect("changed migration task should drain");
    assert_eq!(stats.failed, 1);

    let task = background_task_repo::find_by_id(state.writer_db(), task_id)
        .await
        .expect("task should exist");
    assert_eq!(task.status, BackgroundTaskStatus::Failed);
    assert!(
        task.last_error
            .as_deref()
            .expect("failed task should store last_error")
            .contains("storage policy changed after migration task was created")
    );

    let checkpoint = storage_migration_checkpoint_repo::get_by_task_id(state.writer_db(), task_id)
        .await
        .expect("checkpoint should exist");
    assert_eq!(checkpoint.stage, "prepare_policies");
    assert_eq!(checkpoint.last_processed_blob_id, 0);
    assert_eq!(checkpoint.scanned_blobs, 0);
    assert_eq!(checkpoint.migrated_blobs, 0);
    assert_eq!(checkpoint.merged_blobs, 0);
    assert_eq!(checkpoint.skipped_blobs, 0);
    assert_eq!(checkpoint.failed_blobs, 0);
}

#[actix_web::test]
async fn test_storage_migration_rejects_duplicate_active_pair() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-duplicate").await;
    let target = create_local_policy(&state, "target-duplicate").await;

    let first = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    assert_eq!(first["code"], "success");
    let (status, second) =
        create_migration_task_via_api_with_status(&app, &token, source.id, target.id, false).await;
    assert_conflicting_storage_migration_response(status, &second);
}

#[actix_web::test]
async fn test_storage_migration_active_conflict_matrix() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let source_out = create_local_policy(&state, "source-out").await;
    let target_in = create_local_policy(&state, "target-in").await;
    let other_for_out = create_local_policy(&state, "other-for-out").await;
    let first =
        create_migration_task_via_api(&app, &token, source_out.id, target_in.id, false).await;
    assert_eq!(first["code"], "success");

    let (status, source_with_outgoing) = create_migration_task_via_api_with_status(
        &app,
        &token,
        source_out.id,
        other_for_out.id,
        false,
    )
    .await;
    assert_conflicting_storage_migration_response(status, &source_with_outgoing);

    let (status, source_with_incoming) = create_migration_task_via_api_with_status(
        &app,
        &token,
        target_in.id,
        other_for_out.id,
        false,
    )
    .await;
    assert_conflicting_storage_migration_response(status, &source_with_incoming);

    let (status, target_with_outgoing) = create_migration_task_via_api_with_status(
        &app,
        &token,
        other_for_out.id,
        source_out.id,
        false,
    )
    .await;
    assert_conflicting_storage_migration_response(status, &target_with_outgoing);

    let allowed_second_source = create_local_policy(&state, "allowed-second-source").await;
    let target_with_incoming =
        create_migration_task_via_api(&app, &token, allowed_second_source.id, target_in.id, false)
            .await;
    assert_eq!(target_with_incoming["code"], "success");

    let (status, reverse) =
        create_migration_task_via_api_with_status(&app, &token, target_in.id, source_out.id, false)
            .await;
    assert_conflicting_storage_migration_response(status, &reverse);
}

#[actix_web::test]
async fn test_storage_migration_dry_run_uses_active_conflict_rules() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source = create_local_policy(&state, "source-dry-conflict").await;
    let target = create_local_policy(&state, "target-dry-conflict").await;
    let other = create_local_policy(&state, "other-dry-conflict").await;

    let first = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
    assert_eq!(first["code"], "success");

    let (blocked_status, blocked_body) =
        dry_run_migration_via_api(&app, &token, target.id, other.id).await;
    assert_conflicting_storage_migration_response(blocked_status, &blocked_body);

    let allowed_source = create_local_policy(&state, "allowed-dry-source").await;
    let (allowed_status, allowed_body) =
        dry_run_migration_via_api(&app, &token, allowed_source.id, target.id).await;
    assert_eq!(allowed_status, actix_web::http::StatusCode::OK);
    assert_eq!(allowed_body["code"], "success");
}

#[actix_web::test]
async fn test_storage_migration_terminal_tasks_do_not_block_new_migrations() {
    for status in [
        BackgroundTaskStatus::Succeeded,
        BackgroundTaskStatus::Failed,
        BackgroundTaskStatus::Canceled,
    ] {
        let state = common::setup().await;
        let app = create_test_app!(state.clone());
        let (token, _) = register_and_login!(app);
        let source =
            create_local_policy(&state, &format!("terminal-source-{}", status.as_str())).await;
        let target =
            create_local_policy(&state, &format!("terminal-target-{}", status.as_str())).await;
        let new_target =
            create_local_policy(&state, &format!("terminal-new-target-{}", status.as_str())).await;

        let first = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
        assert_eq!(first["code"], "success");
        let task_id = first["data"]["id"].as_i64().expect("task id should exist");
        set_background_task_status(&state, task_id, status).await;

        let second =
            create_migration_task_via_api(&app, &token, source.id, new_target.id, false).await;
        assert_eq!(second["code"], "success");
    }
}

#[actix_web::test]
async fn test_storage_migration_active_statuses_block_new_migrations() {
    for status in [
        BackgroundTaskStatus::Pending,
        BackgroundTaskStatus::Processing,
        BackgroundTaskStatus::Retry,
    ] {
        let state = common::setup().await;
        let app = create_test_app!(state.clone());
        let (token, _) = register_and_login!(app);
        let source =
            create_local_policy(&state, &format!("active-source-{}", status.as_str())).await;
        let target =
            create_local_policy(&state, &format!("active-target-{}", status.as_str())).await;
        let new_target =
            create_local_policy(&state, &format!("active-new-target-{}", status.as_str())).await;

        let first = create_migration_task_via_api(&app, &token, source.id, target.id, false).await;
        assert_eq!(first["code"], "success");
        let task_id = first["data"]["id"].as_i64().expect("task id should exist");
        set_background_task_status(&state, task_id, status).await;

        let (create_status, create_body) = create_migration_task_via_api_with_status(
            &app,
            &token,
            source.id,
            new_target.id,
            false,
        )
        .await;
        assert_conflicting_storage_migration_response(create_status, &create_body);

        let (dry_run_status, dry_run_body) =
            dry_run_migration_via_api(&app, &token, target.id, new_target.id).await;
        assert_conflicting_storage_migration_response(dry_run_status, &dry_run_body);
    }
}
