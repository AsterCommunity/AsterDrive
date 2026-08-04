//! WebDAV 子模块：`file`。

use std::sync::Arc;

use bytes::Bytes;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::db::repository::file_repo;
use crate::errors::Result as AsterResult;
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::ops::audit::{self, AuditContext};
use crate::services::workspace::storage::{self, WorkspaceStorageScope};
use aster_drive_storage::StorageDriver;
use aster_forge_utils::numbers::{i64_to_u64, u64_to_i64, usize_to_u64};
use aster_forge_webdav::{DavBackendError, DavBackendErrorKind, DavWriteHandle, FsError};

const RELAY_DIRECT_BUFFER_SIZE: usize = 64 * 1024;

/// WebDAV 顺序写句柄，按后端能力选择临时文件或直连流写入。
pub struct AsterDavWriteHandle {
    mode: WriteMode,
}

pub(crate) struct DavWriteOpenContext {
    pub(crate) scope: WorkspaceStorageScope,
    pub(crate) folder_id: Option<i64>,
    pub(crate) filename: String,
    pub(crate) existing_file_id: Option<i64>,
    pub(crate) declared_size: Option<u64>,
    pub(crate) submitted_lock_tokens: Vec<String>,
    pub(crate) audit_ctx: AuditContext,
    pub(crate) file_precondition: Option<storage::FileWritePrecondition>,
}

impl std::fmt::Debug for AsterDavWriteHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.mode {
            WriteMode::Write {
                filename,
                temp_path,
                ..
            } => f
                .debug_struct("AsterDavWriteHandle::Write")
                .field("filename", filename)
                .field("temp_path", temp_path)
                .finish(),
            WriteMode::WriteDirect {
                filename,
                prepared_upload,
                ..
            } => f
                .debug_struct("AsterDavWriteHandle::WriteDirect")
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

enum WriteMode {
    Write {
        state: PrimaryAppState,
        scope: WorkspaceStorageScope,
        folder_id: Option<i64>,
        filename: String,
        existing_file_id: Option<i64>,
        file_precondition: Option<storage::FileWritePrecondition>,
        submitted_lock_tokens: Vec<String>,
        audit_ctx: AuditContext,
        declared_size: Option<i64>,
        resolved_policy: Option<aster_drive_model::entities::storage_policy::Model>,
        file: Option<tokio::fs::File>,
        temp_path: String,
        hasher: Option<Sha256>,
        written: u64,
    },
    WriteDirect {
        state: PrimaryAppState,
        scope: WorkspaceStorageScope,
        folder_id: Option<i64>,
        filename: String,
        existing_file_id: Option<i64>,
        submitted_lock_tokens: Vec<String>,
        audit_ctx: AuditContext,
        declared_size: i64,
        policy: aster_drive_model::entities::storage_policy::Model,
        driver: Arc<dyn StorageDriver>,
        prepared_upload: Option<storage::PreparedNonDedupBlobUpload>,
        writer: Option<tokio::io::DuplexStream>,
        upload_task: Option<tokio::task::JoinHandle<AsterResult<String>>>,
        written: u64,
    },
}

impl AsterDavWriteHandle {
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
            DavWriteOpenContext {
                scope: WorkspaceStorageScope::Personal { user_id },
                folder_id,
                filename,
                existing_file_id,
                declared_size,
                submitted_lock_tokens: Vec::new(),
                file_precondition: None,
                audit_ctx: AuditContext {
                    user_id,
                    ip_address: None,
                    user_agent: None,
                },
            },
        )
        .await
    }

    pub(crate) async fn for_write_with_audit(
        state: PrimaryAppState,
        context: DavWriteOpenContext,
    ) -> Result<Self, FsError> {
        let DavWriteOpenContext {
            scope,
            folder_id,
            filename,
            existing_file_id,
            declared_size,
            submitted_lock_tokens,
            audit_ctx,
            file_precondition,
        } = context;
        let declared_size = declared_size.and_then(|size| i64::try_from(size).ok());
        let (file, temp_path, resolved_policy, hasher) = if let Some(size_hint) = declared_size {
            let policy = storage::resolve_policy_for_size(&state, scope, folder_id, size_hint)
                .await
                .map_err(|_| FsError::GeneralFailure)?;

            if policy.driver_type == aster_drive_model::types::DriverType::Local {
                let staging_token = format!("{}.upload", aster_forge_utils::id::new_uuid());
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
                let hasher = storage::local_content_dedup_enabled(&policy).then(Sha256::new);

                (
                    file,
                    staging_path.to_string_lossy().into_owned(),
                    Some(policy),
                    hasher,
                )
            } else if file_precondition.is_none()
                && storage::streaming_direct_upload_eligible(&policy, size_hint)
                    .map_err(|error| streaming_direct_eligibility_error(&policy, &error))?
            {
                if policy.max_file_size > 0 && size_hint > policy.max_file_size {
                    return Err(FsError::TooLarge);
                }
                storage::check_quota(state.writer_db(), scope, size_hint)
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
                let _stream_driver = driver
                    .extensions()
                    .stream_upload
                    .ok_or(FsError::GeneralFailure)?;
                let prepared_upload =
                    storage::prepare_non_dedup_blob_upload(&policy, size_hint, Some(&filename))
                        .map_err(|error| {
                            tracing::warn!(
                                policy_id = policy.id,
                                driver_type = %policy.driver_type.as_str(),
                                "failed to prepare WebDAV direct blob upload: {error}"
                            );
                            FsError::GeneralFailure
                        })?;
                let storage_path = prepared_upload.storage_path().to_string();
                let (writer, reader) = tokio::io::duplex(RELAY_DIRECT_BUFFER_SIZE);
                let driver_for_task = driver.clone();
                let storage_path_clone = storage_path.clone();
                let size_clone = size_hint;
                let upload_task = tokio::spawn(async move {
                    let Some(stream_driver) = driver_for_task.extensions().stream_upload else {
                        return Err(crate::errors::AsterError::storage_driver_error(
                            "stream upload driver is not available",
                        ));
                    };
                    stream_driver
                        .put_reader(&storage_path_clone, Box::new(reader), size_clone)
                        .await
                        .map_err(crate::errors::AsterError::from)
                });

                return Ok(Self {
                    mode: WriteMode::WriteDirect {
                        state,
                        scope,
                        folder_id,
                        filename,
                        existing_file_id,
                        submitted_lock_tokens,
                        audit_ctx,
                        declared_size: size_hint,
                        policy,
                        driver,
                        prepared_upload: Some(prepared_upload),
                        writer: Some(writer),
                        upload_task: Some(upload_task),
                        written: 0,
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
            mode: WriteMode::Write {
                state,
                scope,
                folder_id,
                filename,
                existing_file_id,
                file_precondition,
                submitted_lock_tokens,
                audit_ctx,
                declared_size,
                resolved_policy,
                file: Some(file),
                temp_path,
                hasher,
                written: 0,
            },
        })
    }

    async fn create_upload_temp_file(
        state: &impl SharedRuntimeState,
    ) -> Result<(tokio::fs::File, String), FsError> {
        let upload_temp_dir = state.config().server.upload_temp_dir.clone();
        let temp_path = aster_forge_utils::paths::temp_file_path(
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
            aster_forge_utils::fs::cleanup_temp_file(&path).await;
        });
    }
}

fn streaming_direct_eligibility_error(
    policy: &aster_drive_model::entities::storage_policy::Model,
    error: &crate::errors::AsterError,
) -> FsError {
    tracing::warn!(
        policy_id = policy.id,
        driver_type = %policy.driver_type.as_str(),
        error = %error,
        "failed to resolve WebDAV streaming direct upload eligibility"
    );
    FsError::GeneralFailure
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

impl Drop for AsterDavWriteHandle {
    fn drop(&mut self) {
        let temp_path = match &self.mode {
            WriteMode::Write { temp_path, .. } => temp_path.clone(),
            WriteMode::WriteDirect { .. } => String::new(),
        };
        if !temp_path.is_empty() {
            Self::cleanup_temp(&temp_path);
        }

        if let WriteMode::WriteDirect {
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
                    storage::cleanup_preuploaded_blob_upload(
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

impl DavWriteHandle for AsterDavWriteHandle {
    async fn write_bytes(&mut self, buf: Bytes) -> Result<(), DavBackendError> {
        let result: Result<(), FsError> = match &mut self.mode {
            WriteMode::Write {
                file,
                written,
                hasher,
                ..
            } => {
                if let Some(hasher) = hasher.as_mut() {
                    hasher.update(&buf);
                }
                file.as_mut()
                    .ok_or(FsError::GeneralFailure)?
                    .write_all(&buf)
                    .await
                    .map_err(|_| FsError::GeneralFailure)?;
                add_written_bytes(written, buf.len())?;
                Ok(())
            }
            WriteMode::WriteDirect {
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
                    return Err(FsError::BadRequest.into());
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
        };
        result.map_err(Into::into)
    }

    async fn finish(mut self) -> Result<(), DavBackendError> {
        let result: Result<(), FsError> = match &mut self.mode {
            WriteMode::Write {
                state,
                scope,
                folder_id,
                filename,
                existing_file_id,
                file_precondition,
                submitted_lock_tokens,
                audit_ctx,
                declared_size,
                resolved_policy,
                file,
                temp_path,
                hasher,
                written,
                ..
            } => {
                file.as_mut()
                    .ok_or(FsError::GeneralFailure)?
                    .flush()
                    .await
                    .map_err(|_| FsError::GeneralFailure)?;
                drop(file.take());

                let written_size = written_size_i64(*written)?;
                if let Some(expected_size) = declared_size
                    && *expected_size != written_size
                {
                    tracing::warn!(
                        expected_size,
                        written_size,
                        filename,
                        "WebDAV upload size mismatch"
                    );
                    return Err(FsError::BadRequest.into());
                }
                let precomputed_hash = hasher
                    .take()
                    .map(|hasher| aster_forge_crypto::sha256_digest_to_hex(&hasher.finalize()));
                let resolved_policy_hint = resolved_policy
                    .clone()
                    .filter(|_| declared_size == &Some(written_size));

                let audit_action = if existing_file_id.is_some() {
                    audit::AuditAction::FileEdit
                } else {
                    audit::AuditAction::FileUpload
                };
                let stored = storage::store_from_temp_with_hints(
                    state,
                    storage::StoreFromTempParams {
                        scope: *scope,
                        folder_id: *folder_id,
                        filename,
                        temp_path,
                        size: written_size,
                        existing_file_id: *existing_file_id,
                        lock_credentials:
                            crate::services::files::lock::LockMutationCredentials::SubmittedTokens(
                                submitted_lock_tokens.clone(),
                            ),
                        file_precondition: *file_precondition,
                    },
                    storage::StoreFromTempHints {
                        resolved_policy: resolved_policy_hint,
                        precomputed_hash: precomputed_hash.as_deref(),
                        actor_username: None,
                        ..Default::default()
                    },
                )
                .await
                .map_err(map_store_error)?;
                let details = crate::services::files::file::audit_location_details_for_model(
                    state, *scope, &stored,
                )
                .await;
                audit::log_with_details(
                    state,
                    audit_ctx,
                    audit_action,
                    crate::services::ops::audit::AuditEntityType::File,
                    Some(stored.id),
                    Some(&stored.name),
                    || details.clone(),
                )
                .await;
                temp_path.clear();

                Ok(())
            }
            WriteMode::WriteDirect {
                state,
                scope,
                folder_id,
                filename,
                existing_file_id,
                submitted_lock_tokens,
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
                        storage::cleanup_preuploaded_blob_upload(
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
                        storage::cleanup_preuploaded_blob_upload(
                            driver.as_ref(),
                            &prepared_upload,
                            "WebDAV direct upload size mismatch",
                        )
                        .await;
                    }
                    return Err(FsError::BadRequest.into());
                }

                let prepared_upload = prepared_upload.take().ok_or(FsError::GeneralFailure)?;
                let audit_action = if existing_file_id.is_some() {
                    audit::AuditAction::FileEdit
                } else {
                    audit::AuditAction::FileUpload
                };
                let stored = storage::store_preuploaded_nondedup(
                    state,
                    storage::StorePreuploadedNondedupParams {
                        scope: *scope,
                        folder_id: *folder_id,
                        filename,
                        size: *declared_size,
                        existing_file_id: *existing_file_id,
                        lock_credentials:
                            crate::services::files::lock::LockMutationCredentials::SubmittedTokens(
                                submitted_lock_tokens.clone(),
                            ),
                        policy,
                        preuploaded_blob: prepared_upload,
                        actor_username: None,
                    },
                )
                .await
                .map_err(map_store_error)?;
                let details = crate::services::files::file::audit_location_details_for_model(
                    state, *scope, &stored,
                )
                .await;
                audit::log_with_details(
                    state,
                    audit_ctx,
                    audit_action,
                    crate::services::ops::audit::AuditEntityType::File,
                    Some(stored.id),
                    Some(&stored.name),
                    || details.clone(),
                )
                .await;

                Ok(())
            }
        };
        result.map_err(Into::into)
    }

    async fn abort(mut self) -> Result<(), DavBackendError> {
        match &mut self.mode {
            WriteMode::Write {
                file, temp_path, ..
            } => {
                drop(file.take());
                aster_forge_utils::fs::cleanup_temp_file(&*temp_path).await;
                temp_path.clear();
            }
            WriteMode::WriteDirect {
                writer,
                upload_task,
                prepared_upload,
                driver,
                ..
            } => {
                drop(writer.take());
                if let Some(upload_task) = upload_task.take() {
                    let _ = upload_task.await;
                }
                if let Some(prepared_upload) = prepared_upload.take() {
                    storage::cleanup_preuploaded_blob_upload(
                        driver.as_ref(),
                        &prepared_upload,
                        "aborted WebDAV direct upload",
                    )
                    .await;
                }
            }
        }
        Ok(())
    }
}

fn map_store_error(error: crate::errors::AsterError) -> DavBackendError {
    tracing::warn!("WebDAV store failed: {error}");
    let kind = match &error {
        crate::errors::AsterError::FileTooLarge(_) => DavBackendErrorKind::PayloadTooLarge,
        crate::errors::AsterError::StorageQuotaExceeded(_) => {
            DavBackendErrorKind::InsufficientStorage
        }
        crate::errors::AsterError::ResourceLocked(_) => DavBackendErrorKind::Locked,
        _ if file_repo::is_any_duplicate_name_error(&error) => DavBackendErrorKind::AlreadyExists,
        _ => DavBackendErrorKind::Internal,
    };
    DavBackendError::new(kind)
}

#[cfg(test)]
mod tests;
