//! 存储驱动实现：`remote`。

use crate::entities::{managed_follower, storage_policy};
use crate::errors::{AsterError, Result};
use crate::storage::driver::{BlobMetadata, PresignedDownloadOptions, StorageDriver};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::extensions::{
    ListStorageDriver, PresignedStorageDriver, StorageCapacityInfo, StreamUploadDriver,
};
use crate::storage::multipart::MultipartStorageDriver;
use crate::storage::object_key;
use crate::storage::remote_protocol::{RemoteStorageCapabilities, RemoteStorageClient};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, ReadBuf};

struct HashingReader {
    inner: Box<dyn AsyncRead + Unpin + Send + Sync>,
    hasher: Arc<Mutex<Sha256>>,
}

impl AsyncRead for HashingReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        match Pin::new(&mut self.inner).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                let filled = buf.filled();
                if filled.len() > before {
                    let mut hasher = self
                        .hasher
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    hasher.update(&filled[before..]);
                }
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

pub struct RemoteDriver {
    client: RemoteStorageClient,
    base_path: String,
    supports_capacity: bool,
}

impl RemoteDriver {
    const MULTIPART_UPLOADS_PREFIX: &str = "uploads";

    pub fn new(policy: &storage_policy::Model, follower: &managed_follower::Model) -> Result<Self> {
        let capabilities = RemoteStorageCapabilities::from_stored_json(&follower.last_capabilities);
        Ok(Self {
            client: RemoteStorageClient::new(
                &follower.base_url,
                &follower.access_key,
                &follower.secret_key,
            )?,
            base_path: policy.base_path.trim_matches('/').to_string(),
            supports_capacity: capabilities.supports_capacity,
        })
    }

    fn object_key(&self, path: &str) -> String {
        object_key::join_key_prefix(&self.base_path, path)
    }

    fn strip_base_path<'a>(&self, object_key: &'a str) -> Option<&'a str> {
        object_key::strip_key_prefix(&self.base_path, object_key)
    }

    fn multipart_parts_prefix(upload_id: &str) -> String {
        format!("{}/{upload_id}/parts", Self::MULTIPART_UPLOADS_PREFIX)
    }

    fn multipart_part_key(upload_id: &str, part_number: i32) -> Result<String> {
        if part_number <= 0 {
            return Err(AsterError::validation_error(format!(
                "multipart part_number must be positive, got {part_number}"
            )));
        }
        Ok(format!(
            "{}/{}",
            Self::multipart_parts_prefix(upload_id),
            part_number
        ))
    }
}

#[async_trait]
impl StorageDriver for RemoteDriver {
    async fn put(&self, path: &str, data: &[u8]) -> Result<String> {
        self.client.put_bytes(&self.object_key(path), data).await?;
        Ok(path.to_string())
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>> {
        self.client.get_bytes(&self.object_key(path)).await
    }

    async fn get_stream(&self, path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.client
            .get_stream(&self.object_key(path), None, None)
            .await
    }

    async fn get_range(
        &self,
        path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.client
            .get_stream(&self.object_key(path), Some(offset), length)
            .await
    }

    fn supports_efficient_range(&self) -> bool {
        true
    }

    async fn delete(&self, path: &str) -> Result<()> {
        self.client.delete(&self.object_key(path)).await
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        self.client.exists(&self.object_key(path)).await
    }

    async fn metadata(&self, path: &str) -> Result<BlobMetadata> {
        self.client.metadata(&self.object_key(path)).await
    }

    async fn capacity_info(&self) -> Result<StorageCapacityInfo> {
        if !self.supports_capacity {
            return Err(storage_driver_error(
                StorageErrorKind::Unsupported,
                "remote storage node does not support capacity observability",
            ));
        }
        self.client.capacity_info().await
    }

    fn as_list(&self) -> Option<&dyn ListStorageDriver> {
        Some(self)
    }

    fn as_stream_upload(&self) -> Option<&dyn StreamUploadDriver> {
        Some(self)
    }

    fn as_presigned(&self) -> Option<&dyn PresignedStorageDriver> {
        Some(self)
    }

    fn as_multipart(&self) -> Option<&dyn MultipartStorageDriver> {
        Some(self)
    }
}

#[async_trait]
impl ListStorageDriver for RemoteDriver {
    async fn list_paths(&self, prefix: Option<&str>) -> Result<Vec<String>> {
        let full_prefix = prefix.map(|value| self.object_key(value));
        let paths = self.client.list_paths(full_prefix.as_deref()).await?;
        Ok(paths
            .into_iter()
            .filter_map(|path| self.strip_base_path(&path).map(str::to_string))
            .collect())
    }
}

#[async_trait]
impl StreamUploadDriver for RemoteDriver {
    async fn put_reader(
        &self,
        storage_path: &str,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> Result<String> {
        let size = u64::try_from(size).map_err(|_| {
            storage_driver_error(
                StorageErrorKind::Precondition,
                format!("remote stream upload size must be non-negative, got {size}"),
            )
        })?;
        self.client
            .put_reader(&self.object_key(storage_path), reader, size)
            .await?;
        Ok(storage_path.to_string())
    }

    async fn put_file(&self, storage_path: &str, local_path: &str) -> Result<String> {
        let metadata = tokio::fs::metadata(local_path).await.map_err(|e| {
            AsterError::storage_driver_error(format!("remote put_file metadata: {e}"))
        })?;
        let file = tokio::fs::File::open(Path::new(local_path))
            .await
            .map_err(|e| AsterError::storage_driver_error(format!("remote put_file open: {e}")))?;
        self.put_reader(
            storage_path,
            Box::new(file),
            i64::try_from(metadata.len()).map_err(|_| {
                storage_driver_error(
                    StorageErrorKind::Precondition,
                    "remote put_file size exceeds i64 range",
                )
            })?,
        )
        .await
    }
}

#[async_trait]
impl PresignedStorageDriver for RemoteDriver {
    async fn presigned_url(
        &self,
        path: &str,
        expires: Duration,
        options: PresignedDownloadOptions,
    ) -> Result<Option<String>> {
        self.client
            .presigned_url(&self.object_key(path), expires, options)
            .map(Some)
    }

    async fn presigned_put_url(&self, path: &str, expires: Duration) -> Result<Option<String>> {
        self.client
            .presigned_put_url(&self.object_key(path), expires)
            .map(Some)
    }
}

#[async_trait]
impl MultipartStorageDriver for RemoteDriver {
    async fn create_multipart_upload(&self, _path: &str) -> Result<String> {
        Ok(crate::utils::id::new_uuid())
    }

    async fn presigned_upload_part_url(
        &self,
        _path: &str,
        upload_id: &str,
        part_number: i32,
        expires: Duration,
    ) -> Result<String> {
        let part_key = Self::multipart_part_key(upload_id, part_number)?;
        self.client
            .presigned_put_url(&self.object_key(&part_key), expires)
    }

    async fn complete_multipart_upload(
        &self,
        path: &str,
        upload_id: &str,
        mut parts: Vec<(i32, String)>,
    ) -> Result<()> {
        if parts.is_empty() {
            return Err(AsterError::validation_error(
                "multipart completion requires at least one part",
            ));
        }

        parts.sort_by_key(|(part_number, _)| *part_number);
        let mut expected_size = 0i64;
        let mut part_keys = Vec::with_capacity(parts.len());
        for (part_number, _) in parts {
            let part_key = Self::multipart_part_key(upload_id, part_number)?;
            let remote_key = self.object_key(&part_key);
            let metadata = self.client.metadata(&remote_key).await?;
            let part_size = i64::try_from(metadata.size).map_err(|_| {
                storage_driver_error(
                    StorageErrorKind::Precondition,
                    "remote multipart part size exceeds i64 range",
                )
            })?;
            expected_size = expected_size.checked_add(part_size).ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Precondition,
                    "remote multipart expected size overflow",
                )
            })?;
            part_keys.push(remote_key);
        }

        self.client
            .compose_objects(&self.object_key(path), part_keys, expected_size)
            .await?;
        Ok(())
    }

    async fn upload_multipart_part(
        &self,
        _path: &str,
        upload_id: &str,
        part_number: i32,
        data: &[u8],
    ) -> Result<String> {
        let part_key = Self::multipart_part_key(upload_id, part_number)?;
        self.client
            .put_bytes(&self.object_key(&part_key), data)
            .await?;

        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(data);
        Ok(format!("\"{}\"", hex::encode(hasher.finalize())))
    }

    async fn upload_multipart_part_reader(
        &self,
        _path: &str,
        upload_id: &str,
        part_number: i32,
        reader: Box<dyn tokio::io::AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> Result<String> {
        let part_key = Self::multipart_part_key(upload_id, part_number)?;
        let size = u64::try_from(size).map_err(|_| {
            storage_driver_error(
                StorageErrorKind::Precondition,
                "remote multipart part size cannot be negative",
            )
        })?;
        let hasher = Arc::new(Mutex::new(Sha256::new()));
        let hashing_reader = HashingReader {
            inner: reader,
            hasher: hasher.clone(),
        };
        self.client
            .put_reader(&self.object_key(&part_key), Box::new(hashing_reader), size)
            .await?;

        let digest = {
            let hasher = hasher.lock().unwrap_or_else(|error| error.into_inner());
            hasher.clone().finalize()
        };
        Ok(format!("\"{}\"", hex::encode(digest)))
    }

    async fn abort_multipart_upload(&self, _path: &str, upload_id: &str) -> Result<()> {
        let prefix = Self::multipart_parts_prefix(upload_id);
        let parts = self.list_paths(Some(&prefix)).await?;
        for part_path in parts {
            self.client.delete(&self.object_key(&part_path)).await?;
        }
        Ok(())
    }

    async fn list_uploaded_parts(&self, _path: &str, upload_id: &str) -> Result<Vec<i32>> {
        let prefix = Self::multipart_parts_prefix(upload_id);
        let mut parts = self
            .list_paths(Some(&prefix))
            .await?
            .into_iter()
            .filter_map(|path| {
                path.rsplit('/')
                    .next()
                    .and_then(|segment| segment.parse::<i32>().ok())
            })
            .collect::<Vec<_>>();
        parts.sort_unstable();
        parts.dedup();
        Ok(parts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::driver::PresignedDownloadOptions;
    use crate::storage::remote_protocol::{
        PRESIGNED_AUTH_ACCESS_KEY_QUERY, PRESIGNED_AUTH_SIGNATURE_QUERY,
        PRESIGNED_RESPONSE_CACHE_CONTROL_QUERY, PRESIGNED_RESPONSE_CONTENT_DISPOSITION_QUERY,
        PRESIGNED_RESPONSE_CONTENT_TYPE_QUERY,
    };
    use actix_web::{App, HttpResponse, HttpServer, web};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use tokio::io::AsyncReadExt;

    struct TestHttpServer {
        base_url: String,
        handle: actix_web::dev::ServerHandle,
        task: tokio::task::JoinHandle<std::io::Result<()>>,
    }

    impl TestHttpServer {
        async fn stop(self) {
            self.handle.stop(true).await;
            let _ = self.task.await;
        }
    }

    fn build_policy(base_path: &str) -> storage_policy::Model {
        let now = chrono::Utc::now();
        storage_policy::Model {
            id: 1,
            name: "remote".to_string(),
            driver_type: crate::types::DriverType::Remote,
            endpoint: String::new(),
            bucket: String::new(),
            access_key: String::new(),
            secret_key: String::new(),
            base_path: base_path.to_string(),
            remote_node_id: Some(7),
            max_file_size: 0,
            allowed_types: crate::types::StoredStoragePolicyAllowedTypes::empty(),
            options: crate::types::StoredStoragePolicyOptions::empty(),
            is_default: false,
            chunk_size: 5_242_880,
            created_at: now,
            updated_at: now,
        }
    }

    fn build_follower(base_url: &str) -> managed_follower::Model {
        build_follower_with_capabilities(base_url, "{}")
    }

    fn build_follower_with_capabilities(
        base_url: &str,
        last_capabilities: &str,
    ) -> managed_follower::Model {
        let now = chrono::Utc::now();
        managed_follower::Model {
            id: 7,
            name: "follower".to_string(),
            base_url: base_url.to_string(),
            access_key: "access-key".to_string(),
            secret_key: "secret-key".to_string(),
            is_enabled: true,
            last_capabilities: last_capabilities.to_string(),
            last_error: String::new(),
            last_checked_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn build_driver(base_url: &str, base_path: &str) -> RemoteDriver {
        RemoteDriver::new(&build_policy(base_path), &build_follower(base_url))
            .expect("remote driver should build")
    }

    fn build_driver_with_capabilities(
        base_url: &str,
        base_path: &str,
        last_capabilities: &str,
    ) -> RemoteDriver {
        RemoteDriver::new(
            &build_policy(base_path),
            &build_follower_with_capabilities(base_url, last_capabilities),
        )
        .expect("remote driver should build")
    }

    async fn spawn_list_server(
        items: Vec<String>,
        seen_prefixes: Arc<Mutex<Vec<Option<String>>>>,
    ) -> TestHttpServer {
        async fn list_objects(
            query: web::Query<HashMap<String, String>>,
            items: web::Data<Vec<String>>,
            seen_prefixes: web::Data<Arc<Mutex<Vec<Option<String>>>>>,
        ) -> HttpResponse {
            let prefix = query.get("prefix").cloned();
            seen_prefixes
                .lock()
                .expect("seen_prefixes lock should not be poisoned")
                .push(prefix);
            HttpResponse::Ok().json(serde_json::json!({
                "code": 0,
                "msg": "",
                "data": { "items": items.get_ref() }
            }))
        }

        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
            .expect("test remote listener should bind");
        let addr = listener
            .local_addr()
            .expect("test remote listener should expose local addr");
        let items_for_server = items.clone();
        let seen_for_server = seen_prefixes.clone();
        let server = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(items_for_server.clone()))
                .app_data(web::Data::new(seen_for_server.clone()))
                .route(
                    "/api/v1/internal/storage/objects",
                    web::get().to(list_objects),
                )
        })
        .listen(listener)
        .expect("test remote server should listen")
        .run();
        let handle = server.handle();
        let task = tokio::spawn(server);

        TestHttpServer {
            base_url: format!("http://127.0.0.1:{}", addr.port()),
            handle,
            task,
        }
    }

    #[test]
    fn new_trims_remote_policy_base_path() {
        let driver = build_driver("http://storage.example.com/", "/base/");

        assert_eq!(driver.base_path, "base");
        assert_eq!(driver.object_key("/files/a.txt"), "base/files/a.txt");
        assert_eq!(
            driver.strip_base_path("base/files/a.txt"),
            Some("files/a.txt")
        );
        assert_eq!(driver.strip_base_path("baseball/files/a.txt"), None);
    }

    #[tokio::test]
    async fn list_paths_sends_scoped_prefix_and_strips_base_path() {
        let seen_prefixes = Arc::new(Mutex::new(Vec::new()));
        let server = spawn_list_server(
            vec![
                "base/files/b.txt".to_string(),
                "baseball/files/ignored.txt".to_string(),
                "base/files/a.txt".to_string(),
            ],
            seen_prefixes.clone(),
        )
        .await;
        let driver = build_driver(&server.base_url, "/base/");

        let listed = driver
            .list_paths(Some("files"))
            .await
            .expect("remote list should succeed");

        assert_eq!(
            listed,
            vec!["files/b.txt".to_string(), "files/a.txt".to_string()]
        );
        assert_eq!(
            *seen_prefixes
                .lock()
                .expect("seen_prefixes lock should not be poisoned"),
            vec![Some("base/files".to_string())]
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn presigned_urls_include_base_path_response_options_and_signature() {
        let driver = build_driver("http://storage.example.com/root/", "base");

        let download_url = driver
            .presigned_url(
                "folder/file name.txt",
                Duration::from_secs(60),
                PresignedDownloadOptions {
                    response_cache_control: Some("private, max-age=60".to_string()),
                    response_content_disposition: Some(
                        "attachment; filename=\"file name.txt\"".to_string(),
                    ),
                    response_content_type: Some("text/plain".to_string()),
                },
            )
            .await
            .expect("download presigned URL should build")
            .expect("remote driver should return URL");
        let parsed = reqwest::Url::parse(&download_url).expect("download URL should parse");
        let query = parsed.query_pairs().into_owned().collect::<HashMap<_, _>>();

        assert_eq!(
            parsed.path(),
            "/root/api/v1/internal/storage/objects/base/folder/file%20name.txt"
        );
        assert_eq!(
            query
                .get(PRESIGNED_RESPONSE_CACHE_CONTROL_QUERY)
                .map(String::as_str),
            Some("private, max-age=60")
        );
        assert_eq!(
            query
                .get(PRESIGNED_RESPONSE_CONTENT_DISPOSITION_QUERY)
                .map(String::as_str),
            Some("attachment; filename=\"file name.txt\"")
        );
        assert_eq!(
            query
                .get(PRESIGNED_RESPONSE_CONTENT_TYPE_QUERY)
                .map(String::as_str),
            Some("text/plain")
        );
        assert_eq!(
            query
                .get(PRESIGNED_AUTH_ACCESS_KEY_QUERY)
                .map(String::as_str),
            Some("access-key")
        );
        assert!(query.contains_key(PRESIGNED_AUTH_SIGNATURE_QUERY));

        let put_url = driver
            .presigned_put_url("upload.bin", Duration::from_secs(60))
            .await
            .expect("PUT presigned URL should build")
            .expect("remote driver should return URL");
        let parsed_put = reqwest::Url::parse(&put_url).expect("PUT URL should parse");
        assert_eq!(
            parsed_put.path(),
            "/root/api/v1/internal/storage/objects/base/upload.bin"
        );
        assert!(
            parsed_put
                .query_pairs()
                .any(|(key, _)| key == PRESIGNED_AUTH_SIGNATURE_QUERY)
        );
    }

    #[tokio::test]
    async fn put_reader_rejects_negative_size_before_network_io() {
        let driver = build_driver("http://127.0.0.1:9", "base");

        let error = driver
            .put_reader("object.bin", Box::new(tokio::io::empty()), -1)
            .await
            .expect_err("negative size should be rejected locally");

        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Precondition)
        );
        assert!(error.message().contains("must be non-negative"));
    }

    #[tokio::test]
    async fn get_range_forwards_offset_and_length_to_remote_object_request() {
        async fn get_object(query: web::Query<HashMap<String, String>>) -> HttpResponse {
            assert_eq!(query.get("offset").map(String::as_str), Some("7"));
            assert_eq!(query.get("length").map(String::as_str), Some("5"));
            HttpResponse::Ok().body("world")
        }

        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
            .expect("test remote listener should bind");
        let addr = listener
            .local_addr()
            .expect("test remote listener should expose local addr");
        let server = HttpServer::new(move || {
            App::new().route(
                "/api/v1/internal/storage/objects/base/file.txt",
                web::get().to(get_object),
            )
        })
        .listen(listener)
        .expect("test remote server should listen")
        .run();
        let handle = server.handle();
        let task = tokio::spawn(server);
        let driver = build_driver(&format!("http://127.0.0.1:{}", addr.port()), "base");

        let mut reader = driver.get_range("file.txt", 7, Some(5)).await.unwrap();
        let mut body = Vec::new();
        reader.read_to_end(&mut body).await.unwrap();

        assert_eq!(body, b"world");
        handle.stop(true).await;
        let _ = task.await;
    }

    #[tokio::test]
    async fn v3_remote_driver_can_use_v2_node_without_capacity_support() {
        async fn get_object(query: web::Query<HashMap<String, String>>) -> HttpResponse {
            assert!(query.get("offset").is_none());
            assert!(query.get("length").is_none());
            HttpResponse::Ok().body("v2-compatible")
        }

        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
            .expect("test remote listener should bind");
        let addr = listener
            .local_addr()
            .expect("test remote listener should expose local addr");
        let server = HttpServer::new(move || {
            App::new().route(
                "/api/v1/internal/storage/objects/base/file.txt",
                web::get().to(get_object),
            )
        })
        .listen(listener)
        .expect("test remote server should listen")
        .run();
        let handle = server.handle();
        let task = tokio::spawn(server);
        let driver = build_driver_with_capabilities(
            &format!("http://127.0.0.1:{}", addr.port()),
            "base",
            r#"{
                "protocol_version":"v2",
                "min_supported_protocol_version":"v2",
                "supports_stream_upload":true,
                "supports_list":true,
                "supports_range_read":true
            }"#,
        );

        let body = driver
            .get("file.txt")
            .await
            .expect("v3 client should keep v2 object operations usable");
        assert_eq!(body, b"v2-compatible");

        let capacity_error = driver
            .capacity_info()
            .await
            .expect_err("v2 nodes should not expose v3 capacity endpoint");
        assert_eq!(
            capacity_error.storage_error_kind(),
            Some(StorageErrorKind::Unsupported)
        );
        assert!(
            capacity_error
                .message()
                .contains("does not support capacity observability")
        );

        handle.stop(true).await;
        let _ = task.await;
    }

    #[test]
    fn multipart_part_key_rejects_non_positive_part_numbers() {
        let err = RemoteDriver::multipart_part_key("upload-1", 0)
            .expect_err("zero part number should fail");

        assert!(err.message().contains("part_number must be positive"));
    }

    #[tokio::test]
    async fn list_uploaded_parts_sorts_and_deduplicates_numeric_part_keys() {
        let seen_prefixes = Arc::new(Mutex::new(Vec::new()));
        let server = spawn_list_server(
            vec![
                "base/uploads/upload-1/parts/2".to_string(),
                "base/uploads/upload-1/parts/not-a-number".to_string(),
                "base/uploads/upload-1/parts/1".to_string(),
                "base/uploads/upload-1/parts/2".to_string(),
            ],
            seen_prefixes.clone(),
        )
        .await;
        let driver = build_driver(&server.base_url, "base");

        let parts = driver
            .list_uploaded_parts("ignored.bin", "upload-1")
            .await
            .expect("remote parts should list");

        assert_eq!(parts, vec![1, 2]);
        assert_eq!(
            *seen_prefixes
                .lock()
                .expect("seen_prefixes lock should not be poisoned"),
            vec![Some("base/uploads/upload-1/parts".to_string())]
        );

        server.stop().await;
    }
}
