//! WebDAV 子模块：`file`。

use std::io::SeekFrom;
use std::sync::Arc;

use bytes::Bytes;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

use crate::db::repository::file_repo;
use crate::errors::Result as AsterResult;
use crate::runtime::PrimaryAppState;
use crate::services::audit_service::{self, AuditContext};
use crate::services::workspace_storage_service::{self, WorkspaceStorageScope};
use crate::storage::StorageDriver;
use crate::utils::numbers::{i64_to_u64, u64_to_i64, usize_to_u64};
use crate::webdav::dav::{DavFile, DavMetaData, FsError, FsFuture};
use crate::webdav::metadata::AsterDavMeta;

const RELAY_DIRECT_BUFFER_SIZE: usize = 64 * 1024;

/// DavFile 实现，按后端能力选择临时文件或直连流写入
pub struct AsterDavFile {
    mode: FileMode,
}

impl std::fmt::Debug for AsterDavFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.mode {
            FileMode::Write {
                filename,
                temp_path,
                ..
            } => f
                .debug_struct("AsterDavFile::Write")
                .field("filename", filename)
                .field("temp_path", temp_path)
                .finish(),
            FileMode::WriteDirect {
                filename,
                prepared_upload,
                ..
            } => f
                .debug_struct("AsterDavFile::WriteDirect")
                .field("filename", filename)
                .field(
                    "storage_path",
                    &prepared_upload
                        .as_ref()
                        .map(|prepared| prepared.storage_path().to_string()),
                )
                .finish(),
        }
    }
}

enum FileMode {
    Write {
        state: PrimaryAppState,
        user_id: i64,
        folder_id: Option<i64>,
        filename: String,
        existing_file_id: Option<i64>,
        audit_ctx: AuditContext,
        declared_size: Option<i64>,
        resolved_policy: Option<crate::entities::storage_policy::Model>,
        file: tokio::fs::File,
        temp_path: String,
        hasher: Option<Sha256>,
        written: u64,
        meta: AsterDavMeta,
    },
    WriteDirect {
        state: PrimaryAppState,
        user_id: i64,
        folder_id: Option<i64>,
        filename: String,
        existing_file_id: Option<i64>,
        audit_ctx: AuditContext,
        declared_size: i64,
        policy: crate::entities::storage_policy::Model,
        driver: Arc<dyn StorageDriver>,
        prepared_upload: Option<workspace_storage_service::PreparedNonDedupBlobUpload>,
        writer: Option<tokio::io::DuplexStream>,
        upload_task: Option<tokio::task::JoinHandle<AsterResult<String>>>,
        written: u64,
        meta: AsterDavMeta,
    },
}

impl AsterDavFile {
    /// 创建写模式文件（持有临时文件句柄）
    pub async fn for_write(
        state: PrimaryAppState,
        user_id: i64,
        folder_id: Option<i64>,
        filename: String,
        existing_file_id: Option<i64>,
        declared_size: Option<u64>,
    ) -> Result<Self, FsError> {
        Self::for_write_with_audit(
            state,
            user_id,
            folder_id,
            filename,
            existing_file_id,
            declared_size,
            AuditContext {
                user_id,
                ip_address: None,
                user_agent: None,
            },
        )
        .await
    }

    pub async fn for_write_with_audit(
        state: PrimaryAppState,
        user_id: i64,
        folder_id: Option<i64>,
        filename: String,
        existing_file_id: Option<i64>,
        declared_size: Option<u64>,
        audit_ctx: AuditContext,
    ) -> Result<Self, FsError> {
        let declared_size = declared_size.and_then(|size| i64::try_from(size).ok());
        let (file, temp_path, resolved_policy, hasher) = if let Some(size_hint) = declared_size {
            let policy = workspace_storage_service::resolve_policy_for_size(
                &state,
                WorkspaceStorageScope::Personal { user_id },
                folder_id,
                size_hint,
            )
            .await
            .map_err(|_| FsError::GeneralFailure)?;

            if policy.driver_type == crate::types::DriverType::Local {
                let staging_token = format!("{}.upload", crate::utils::id::new_uuid());
                let staging_path =
                    crate::storage::drivers::local::upload_staging_path(&policy, &staging_token)
                        .map_err(|_| FsError::GeneralFailure)?;
                if let Some(parent) = staging_path.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(|_| FsError::GeneralFailure)?;
                }
                let file = tokio::fs::File::create(&staging_path)
                    .await
                    .map_err(|_| FsError::GeneralFailure)?;
                let hasher = workspace_storage_service::local_content_dedup_enabled(&policy)
                    .then(Sha256::new);

                (
                    file,
                    staging_path.to_string_lossy().into_owned(),
                    Some(policy),
                    hasher,
                )
            } else if workspace_storage_service::streaming_direct_upload_eligible(
                &policy, size_hint,
            ) {
                if policy.max_file_size > 0 && size_hint > policy.max_file_size {
                    return Err(FsError::TooLarge);
                }
                workspace_storage_service::check_quota(
                    state.writer_db(),
                    WorkspaceStorageScope::Personal { user_id },
                    size_hint,
                )
                .await
                .map_err(|error| match error {
                    crate::errors::AsterError::StorageQuotaExceeded(_) => {
                        FsError::InsufficientStorage
                    }
                    _ => FsError::GeneralFailure,
                })?;

                let driver = state
                    .driver_registry
                    .get_driver(&policy)
                    .map_err(|_| FsError::GeneralFailure)?;
                let _stream_driver = driver.as_stream_upload().ok_or(FsError::GeneralFailure)?;
                let prepared_upload =
                    workspace_storage_service::prepare_non_dedup_blob_upload(&policy, size_hint);
                let storage_path = prepared_upload.storage_path().to_string();
                let (writer, reader) = tokio::io::duplex(RELAY_DIRECT_BUFFER_SIZE);
                let driver_for_task = driver.clone();
                let storage_path_clone = storage_path.clone();
                let size_clone = size_hint;
                let upload_task = tokio::spawn(async move {
                    let stream_driver = driver_for_task
                        .as_stream_upload()
                        .expect("stream driver should be available");
                    stream_driver
                        .put_reader(&storage_path_clone, Box::new(reader), size_clone)
                        .await
                });

                return Ok(Self {
                    mode: FileMode::WriteDirect {
                        state,
                        user_id,
                        folder_id,
                        filename,
                        existing_file_id,
                        audit_ctx,
                        declared_size: size_hint,
                        policy,
                        driver,
                        prepared_upload: Some(prepared_upload),
                        writer: Some(writer),
                        upload_task: Some(upload_task),
                        written: 0,
                        meta: AsterDavMeta::root(),
                    },
                });
            } else {
                let (file, temp_path) = Self::create_upload_temp_file(&state).await?;
                (file, temp_path, None, None)
            }
        } else {
            let (file, temp_path) = Self::create_upload_temp_file(&state).await?;
            (file, temp_path, None, None)
        };

        Ok(Self {
            mode: FileMode::Write {
                state,
                user_id,
                folder_id,
                filename,
                existing_file_id,
                audit_ctx,
                declared_size,
                resolved_policy,
                file,
                temp_path,
                hasher,
                written: 0,
                meta: AsterDavMeta::root(),
            },
        })
    }

    async fn create_upload_temp_file(
        state: &PrimaryAppState,
    ) -> Result<(tokio::fs::File, String), FsError> {
        let upload_temp_dir = state.config.server.upload_temp_dir.clone();
        let temp_path = crate::utils::paths::temp_file_path(
            &upload_temp_dir,
            &format!("webdav-{}.upload", uuid::Uuid::new_v4()),
        );
        tokio::fs::create_dir_all(&upload_temp_dir)
            .await
            .map_err(|_| FsError::GeneralFailure)?;
        let file = tokio::fs::File::create(&temp_path)
            .await
            .map_err(|_| FsError::GeneralFailure)?;
        Ok((file, temp_path))
    }

    /// 清理临时文件（best-effort，异步后台执行）
    fn cleanup_temp(temp_path: &str) {
        let path = temp_path.to_string();
        tokio::spawn(async move {
            crate::utils::cleanup_temp_file(&path).await;
        });
    }
}

fn add_written_bytes(written: &mut u64, chunk_len: usize) -> Result<u64, FsError> {
    let chunk_len =
        usize_to_u64(chunk_len, "webdav written chunk").map_err(|_| FsError::GeneralFailure)?;
    let next_written = written
        .checked_add(chunk_len)
        .ok_or(FsError::GeneralFailure)?;
    *written = next_written;
    Ok(next_written)
}

fn declared_size_u64(value: i64) -> Result<u64, FsError> {
    i64_to_u64(value, "webdav declared_size").map_err(|_| FsError::GeneralFailure)
}

fn written_size_i64(value: u64) -> Result<i64, FsError> {
    u64_to_i64(value, "webdav written bytes").map_err(|_| FsError::GeneralFailure)
}

impl Drop for AsterDavFile {
    fn drop(&mut self) {
        let temp_path = match &self.mode {
            FileMode::Write { temp_path, .. } => temp_path.clone(),
            FileMode::WriteDirect { .. } => String::new(),
        };
        if !temp_path.is_empty() {
            Self::cleanup_temp(&temp_path);
        }

        if let FileMode::WriteDirect {
            writer,
            upload_task,
            prepared_upload,
            driver,
            ..
        } = &mut self.mode
        {
            drop(writer.take());
            if let (Some(upload_task), Some(prepared_upload)) =
                (upload_task.take(), prepared_upload.take())
            {
                let driver = driver.clone();
                tokio::spawn(async move {
                    let _ = upload_task.await;
                    workspace_storage_service::cleanup_preuploaded_blob_upload(
                        driver.as_ref(),
                        &prepared_upload,
                        "dropped WebDAV direct upload",
                    )
                    .await;
                });
            }
        }
    }
}

impl DavFile for AsterDavFile {
    fn metadata<'a>(&'a mut self) -> FsFuture<'a, Box<dyn DavMetaData>> {
        let meta: Box<dyn DavMetaData> = match &self.mode {
            FileMode::Write { meta, .. } => Box::new(meta.clone()),
            FileMode::WriteDirect { meta, .. } => Box::new(meta.clone()),
        };
        Box::pin(async move { Ok(meta) })
    }

    fn read_bytes(&mut self, count: usize) -> FsFuture<'_, Bytes> {
        let _ = count;
        Box::pin(async move {
            match &mut self.mode {
                FileMode::Write { .. } | FileMode::WriteDirect { .. } => Err(FsError::Forbidden),
            }
        })
    }

    fn write_bytes(&mut self, buf: Bytes) -> FsFuture<'_, ()> {
        Box::pin(async move {
            match &mut self.mode {
                FileMode::Write {
                    file,
                    written,
                    hasher,
                    ..
                } => {
                    if let Some(hasher) = hasher.as_mut() {
                        hasher.update(&buf);
                    }
                    file.write_all(&buf)
                        .await
                        .map_err(|_| FsError::GeneralFailure)?;
                    add_written_bytes(written, buf.len())?;
                    Ok(())
                }
                FileMode::WriteDirect {
                    writer,
                    written,
                    declared_size,
                    ..
                } => {
                    let chunk_len = usize_to_u64(buf.len(), "webdav direct write chunk")
                        .map_err(|_| FsError::GeneralFailure)?;
                    let next_written = written
                        .checked_add(chunk_len)
                        .ok_or(FsError::GeneralFailure)?;
                    if next_written > declared_size_u64(*declared_size)? {
                        return Err(FsError::GeneralFailure);
                    }
                    writer
                        .as_mut()
                        .ok_or(FsError::GeneralFailure)?
                        .write_all(&buf)
                        .await
                        .map_err(|_| FsError::GeneralFailure)?;
                    *written = next_written;
                    Ok(())
                }
            }
        })
    }

    fn write_buf(&mut self, mut buf: Box<dyn bytes::Buf + Send>) -> FsFuture<'_, ()> {
        Box::pin(async move {
            match &mut self.mode {
                FileMode::Write {
                    file,
                    written,
                    hasher,
                    ..
                } => {
                    while buf.has_remaining() {
                        let chunk = buf.chunk();
                        if let Some(hasher) = hasher.as_mut() {
                            hasher.update(chunk);
                        }
                        file.write_all(chunk)
                            .await
                            .map_err(|_| FsError::GeneralFailure)?;
                        add_written_bytes(written, chunk.len())?;
                        let len = chunk.len();
                        buf.advance(len);
                    }
                    Ok(())
                }
                FileMode::WriteDirect {
                    writer,
                    written,
                    declared_size,
                    ..
                } => {
                    while buf.has_remaining() {
                        let chunk = buf.chunk();
                        let chunk_len = usize_to_u64(chunk.len(), "webdav direct write chunk")
                            .map_err(|_| FsError::GeneralFailure)?;
                        let next_written = written
                            .checked_add(chunk_len)
                            .ok_or(FsError::GeneralFailure)?;
                        if next_written > declared_size_u64(*declared_size)? {
                            return Err(FsError::GeneralFailure);
                        }
                        writer
                            .as_mut()
                            .ok_or(FsError::GeneralFailure)?
                            .write_all(chunk)
                            .await
                            .map_err(|_| FsError::GeneralFailure)?;
                        *written = next_written;
                        let len = chunk.len();
                        buf.advance(len);
                    }
                    Ok(())
                }
            }
        })
    }

    fn seek(&mut self, pos: SeekFrom) -> FsFuture<'_, u64> {
        Box::pin(async move {
            match &mut self.mode {
                FileMode::Write { file, .. } => {
                    file.seek(pos).await.map_err(|_| FsError::GeneralFailure)
                }
                FileMode::WriteDirect { written, .. } => match pos {
                    SeekFrom::Current(0) => Ok(*written),
                    _ => Err(FsError::GeneralFailure),
                },
            }
        })
    }

    fn flush(&mut self) -> FsFuture<'_, ()> {
        Box::pin(async move {
            match &mut self.mode {
                FileMode::Write {
                    state,
                    user_id,
                    folder_id,
                    filename,
                    existing_file_id,
                    audit_ctx,
                    declared_size,
                    resolved_policy,
                    file,
                    temp_path,
                    hasher,
                    written,
                    ..
                } => {
                    file.flush().await.map_err(|_| FsError::GeneralFailure)?;

                    let written_size = written_size_i64(*written)?;
                    let precomputed_hash = hasher
                        .take()
                        .map(|hasher| crate::utils::hash::sha256_digest_to_hex(&hasher.finalize()));
                    let resolved_policy_hint = resolved_policy
                        .clone()
                        .filter(|_| declared_size == &Some(written_size));

                    let audit_action = if existing_file_id.is_some() {
                        audit_service::AuditAction::FileEdit
                    } else {
                        audit_service::AuditAction::FileUpload
                    };
                    let stored = workspace_storage_service::store_from_temp_with_hints(
                        state,
                        workspace_storage_service::StoreFromTempParams {
                            scope: WorkspaceStorageScope::Personal { user_id: *user_id },
                            folder_id: *folder_id,
                            filename,
                            temp_path,
                            size: written_size,
                            existing_file_id: *existing_file_id,
                            skip_lock_check: true, // WebDAV: handler 已验证 lock token
                        },
                        workspace_storage_service::StoreFromTempHints {
                            resolved_policy: resolved_policy_hint,
                            precomputed_hash: precomputed_hash.as_deref(),
                            actor_username: None,
                        },
                    )
                    .await
                    .map_err(map_store_error)?;
                    audit_service::log(
                        state,
                        audit_ctx,
                        audit_action,
                        crate::services::audit_service::AuditEntityType::File,
                        Some(stored.id),
                        Some(&stored.name),
                        Some(serde_json::json!({ "source": "webdav" })),
                    )
                    .await;

                    Ok(())
                }
                FileMode::WriteDirect {
                    state,
                    user_id,
                    folder_id,
                    filename,
                    existing_file_id,
                    audit_ctx,
                    declared_size,
                    policy,
                    driver,
                    prepared_upload,
                    writer,
                    upload_task,
                    written,
                    ..
                } => {
                    writer
                        .take()
                        .ok_or(FsError::GeneralFailure)?
                        .shutdown()
                        .await
                        .map_err(|_| FsError::GeneralFailure)?;

                    let upload_result = upload_task
                        .take()
                        .ok_or(FsError::GeneralFailure)?
                        .await
                        .map_err(|_| FsError::GeneralFailure)?;
                    if let Err(error) = upload_result {
                        if let Some(prepared_upload) = prepared_upload.take() {
                            workspace_storage_service::cleanup_preuploaded_blob_upload(
                                driver.as_ref(),
                                &prepared_upload,
                                "WebDAV direct upload error",
                            )
                            .await;
                        }
                        return Err(map_store_error(error));
                    }

                    if *written != declared_size_u64(*declared_size)? {
                        if let Some(prepared_upload) = prepared_upload.take() {
                            workspace_storage_service::cleanup_preuploaded_blob_upload(
                                driver.as_ref(),
                                &prepared_upload,
                                "WebDAV direct upload size mismatch",
                            )
                            .await;
                        }
                        return Err(FsError::GeneralFailure);
                    }

                    let prepared_upload = prepared_upload.take().ok_or(FsError::GeneralFailure)?;
                    let audit_action = if existing_file_id.is_some() {
                        audit_service::AuditAction::FileEdit
                    } else {
                        audit_service::AuditAction::FileUpload
                    };
                    let stored = workspace_storage_service::store_preuploaded_nondedup(
                        state,
                        workspace_storage_service::StorePreuploadedNondedupParams {
                            scope: WorkspaceStorageScope::Personal { user_id: *user_id },
                            folder_id: *folder_id,
                            filename,
                            size: *declared_size,
                            existing_file_id: *existing_file_id,
                            skip_lock_check: true,
                            policy,
                            preuploaded_blob: prepared_upload,
                            actor_username: None,
                        },
                    )
                    .await
                    .map_err(map_store_error)?;
                    audit_service::log(
                        state,
                        audit_ctx,
                        audit_action,
                        crate::services::audit_service::AuditEntityType::File,
                        Some(stored.id),
                        Some(&stored.name),
                        Some(serde_json::json!({ "source": "webdav" })),
                    )
                    .await;

                    Ok(())
                }
            }
        })
    }
}

fn map_store_error(error: crate::errors::AsterError) -> FsError {
    tracing::warn!("WebDAV store failed: {error}");
    match &error {
        crate::errors::AsterError::FileTooLarge(_) => FsError::TooLarge,
        crate::errors::AsterError::StorageQuotaExceeded(_) => FsError::InsufficientStorage,
        _ if file_repo::is_any_duplicate_name_error(&error) => FsError::Exists,
        _ => FsError::GeneralFailure,
    }
}

#[cfg(test)]
mod tests {
    use super::AsterDavFile;
    use crate::cache;
    use crate::config::{CacheConfig, Config, DatabaseConfig, RuntimeConfig};
    use crate::db::repository::file_repo;
    use crate::entities::{storage_policy, user};
    use crate::runtime::PrimaryAppState;
    use crate::services::mail_service;
    use crate::storage::driver::BlobMetadata;
    use crate::storage::{DriverRegistry, PolicySnapshot, StorageDriver, StreamUploadDriver};
    use crate::types::{DriverType, UserRole, UserStatus};
    use crate::webdav::dav::DavFile;
    use async_trait::async_trait;
    use bytes::Bytes;
    use chrono::Utc;
    use migration::Migrator;
    use sea_orm::{ActiveModelTrait, Set};
    use std::collections::{BTreeMap, HashMap};
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncRead, AsyncReadExt};

    #[derive(Clone, Default)]
    struct MockDirectS3Driver {
        objects: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    }

    #[async_trait]
    impl StorageDriver for MockDirectS3Driver {
        async fn put(&self, path: &str, data: &[u8]) -> crate::errors::Result<String> {
            self.objects
                .lock()
                .expect("mock direct S3 driver lock should succeed")
                .insert(path.to_string(), data.to_vec());
            Ok(path.to_string())
        }

        async fn get(&self, path: &str) -> crate::errors::Result<Vec<u8>> {
            Ok(self
                .objects
                .lock()
                .expect("mock direct S3 driver lock should succeed")
                .get(path)
                .cloned()
                .unwrap_or_default())
        }

        async fn get_stream(
            &self,
            _path: &str,
        ) -> crate::errors::Result<Box<dyn AsyncRead + Unpin + Send>> {
            Ok(Box::new(tokio::io::empty()))
        }

        async fn delete(&self, path: &str) -> crate::errors::Result<()> {
            self.objects
                .lock()
                .expect("mock direct S3 driver lock should succeed")
                .remove(path);
            Ok(())
        }

        async fn exists(&self, path: &str) -> crate::errors::Result<bool> {
            Ok(self
                .objects
                .lock()
                .expect("mock direct S3 driver lock should succeed")
                .contains_key(path))
        }

        async fn metadata(&self, path: &str) -> crate::errors::Result<BlobMetadata> {
            let size = self
                .objects
                .lock()
                .expect("mock direct S3 driver lock should succeed")
                .get(path)
                .map(|bytes| u64::try_from(bytes.len()).expect("mock object size should fit u64"))
                .unwrap_or(0);
            Ok(BlobMetadata {
                size,
                content_type: None,
            })
        }

        fn as_stream_upload(&self) -> Option<&dyn StreamUploadDriver> {
            Some(self)
        }
    }

    #[async_trait]
    impl StreamUploadDriver for MockDirectS3Driver {
        async fn put_file(
            &self,
            storage_path: &str,
            local_path: &str,
        ) -> crate::errors::Result<String> {
            let data = tokio::fs::read(local_path).await.map_err(|error| {
                crate::errors::AsterError::storage_driver_error(format!(
                    "mock direct S3 put_file failed: {error}"
                ))
            })?;
            self.objects
                .lock()
                .expect("mock direct S3 driver lock should succeed")
                .insert(storage_path.to_string(), data);
            Ok(storage_path.to_string())
        }

        async fn put_reader(
            &self,
            storage_path: &str,
            mut reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
            _size: i64,
        ) -> crate::errors::Result<String> {
            let mut data = Vec::new();
            reader.read_to_end(&mut data).await.map_err(|error| {
                crate::errors::AsterError::storage_driver_error(format!(
                    "mock direct S3 reader failed: {error}"
                ))
            })?;
            self.objects
                .lock()
                .expect("mock direct S3 driver lock should succeed")
                .insert(storage_path.to_string(), data);
            Ok(storage_path.to_string())
        }
    }

    fn snapshot_dir_tree(path: &Path) -> std::io::Result<BTreeMap<String, bool>> {
        fn walk(
            root: &Path,
            current: &Path,
            entries: &mut BTreeMap<String, bool>,
        ) -> std::io::Result<()> {
            for entry in std::fs::read_dir(current)? {
                let entry = entry?;
                let path = entry.path();
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let file_type = entry.file_type()?;
                if file_type.is_dir() {
                    entries.insert(format!("{relative}/"), true);
                    walk(root, &path, entries)?;
                } else {
                    entries.insert(relative, false);
                }
            }
            Ok(())
        }

        let mut entries = BTreeMap::new();
        if !path.exists() {
            return Ok(entries);
        }
        walk(path, path, &mut entries)?;
        Ok(entries)
    }

    async fn build_s3_direct_test_state() -> (PrimaryAppState, user::Model, MockDirectS3Driver) {
        let temp_root = std::env::temp_dir().join(format!(
            "asterdrive-webdav-file-direct-s3-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&temp_root).expect("temp root should be created");

        let db = crate::db::connect_with_metrics(
            &DatabaseConfig {
                url: "sqlite::memory:".to_string(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics_core::NoopMetrics::arc(),
        )
        .await
        .expect("test database connection should succeed");
        Migrator::up(&db, None)
            .await
            .expect("test migrations should succeed");

        let now = Utc::now();
        let policy = storage_policy::ActiveModel {
            name: Set("Direct S3 Policy".to_string()),
            driver_type: Set(DriverType::S3),
            endpoint: Set("https://mock-s3.example".to_string()),
            bucket: Set("mock-bucket".to_string()),
            access_key: Set("mock-access".to_string()),
            secret_key: Set("mock-secret".to_string()),
            base_path: Set(String::new()),
            max_file_size: Set(0),
            allowed_types: Set(crate::types::StoredStoragePolicyAllowedTypes::empty()),
            options: Set(crate::types::StoredStoragePolicyOptions::empty()),
            is_default: Set(true),
            chunk_size: Set(5_242_880),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("test S3 policy should be inserted");

        let user = user::ActiveModel {
            username: Set("webdavs3writer".to_string()),
            email: Set("webdavs3writer@example.com".to_string()),
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
        .expect("test user should be inserted");

        crate::services::policy_service::ensure_policy_groups_seeded(&db)
            .await
            .expect("policy groups should be seeded for direct S3 test");

        let runtime_config = Arc::new(RuntimeConfig::new());
        let cache = cache::create_cache(&CacheConfig {
            enabled: false,
            ..Default::default()
        })
        .await;

        let mut config = Config::default();
        config.server.temp_dir = temp_root.join(".tmp").to_string_lossy().into_owned();
        config.server.upload_temp_dir = temp_root.join(".uploads").to_string_lossy().into_owned();

        let policy_snapshot = Arc::new(PolicySnapshot::new());
        policy_snapshot
            .reload(&db)
            .await
            .expect("policy snapshot should reload");

        let driver_registry = Arc::new(DriverRegistry::noop());
        let mock_driver = MockDirectS3Driver::default();
        driver_registry.insert_for_test(policy.id, Arc::new(mock_driver.clone()));

        let (storage_change_tx, _) = tokio::sync::broadcast::channel(
            crate::services::storage_change_service::STORAGE_CHANGE_CHANNEL_CAPACITY,
        );
        let share_download_rollback =
            crate::services::share_service::spawn_detached_share_download_rollback_queue(
                db.clone(),
                crate::config::operations::share_download_rollback_queue_capacity(&runtime_config),
            );

        let state = PrimaryAppState {
            db_handles: crate::db::DbHandles::single(db),
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
        };

        (state, user, mock_driver)
    }

    #[tokio::test]
    async fn known_size_s3_write_avoids_runtime_temp_files() {
        let (state, user, driver) = build_s3_direct_test_state().await;
        let runtime_temp_dir = crate::utils::paths::runtime_temp_dir(&state.config.server.temp_dir);
        let before = snapshot_dir_tree(Path::new(&runtime_temp_dir)).unwrap();
        let payload = b"stream direct to s3";

        let mut dav_file = AsterDavFile::for_write(
            state.clone(),
            user.id,
            None,
            "direct-s3.txt".to_string(),
            None,
            Some(u64::try_from(payload.len()).expect("payload length should fit u64")),
        )
        .await
        .expect("S3 direct WebDAV file should initialize");
        dav_file
            .write_bytes(Bytes::copy_from_slice(payload))
            .await
            .expect("S3 direct WebDAV write should succeed");
        dav_file
            .flush()
            .await
            .expect("S3 direct WebDAV flush should succeed");

        let after = snapshot_dir_tree(Path::new(&runtime_temp_dir)).unwrap();
        assert_eq!(
            after, before,
            "known-size S3 WebDAV write should not create runtime temp files"
        );

        let stored =
            file_repo::find_by_name_in_folder(state.writer_db(), user.id, None, "direct-s3.txt")
                .await
                .expect("stored file lookup should succeed")
                .expect("S3 direct WebDAV flush should create a file");
        assert_eq!(
            stored.size,
            i64::try_from(payload.len()).expect("payload length should fit i64")
        );

        let objects = driver
            .objects
            .lock()
            .expect("mock direct S3 driver lock should succeed");
        assert_eq!(
            objects.len(),
            1,
            "direct S3 path should upload exactly one object"
        );
        assert!(
            objects.values().any(|bytes| bytes.as_slice() == payload),
            "uploaded object should match the WebDAV payload"
        );
    }
}
