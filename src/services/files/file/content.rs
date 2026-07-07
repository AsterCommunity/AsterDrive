//! 文件服务子模块：`content`。

use actix_web::web::{Bytes, Payload};
use futures::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncWriteExt, BufWriter};

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{
    AsterError, MapAsterErr, Result, file_upload_error_with_code, precondition_failed_with_code,
    validation_error_with_code,
};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{
    storage_policy::policy::StoragePolicy,
    workspace::models::FileInfo,
    workspace::storage::{
        self, NewFileMode, StoreFromTempHints, StoreFromTempParams, WorkspaceStorageScope,
        WorkspaceUploadHints,
    },
};

use crate::utils::numbers::usize_to_i64;

pub(crate) struct StreamedTempUpload {
    pub temp_path: String,
    pub size: i64,
    pub resolved_policy: Option<crate::entities::storage_policy::Model>,
    pub precomputed_hash: Option<String>,
}

fn upload_temp_dir_create_failed(message: String) -> AsterError {
    file_upload_error_with_code(ApiErrorCode::UploadTempDirCreateFailed, message)
}

fn upload_temp_file_create_failed(message: String) -> AsterError {
    file_upload_error_with_code(ApiErrorCode::UploadTempFileCreateFailed, message)
}

fn upload_temp_file_write_failed(message: String) -> AsterError {
    file_upload_error_with_code(ApiErrorCode::UploadTempFileWriteFailed, message)
}

fn upload_temp_file_flush_failed(message: String) -> AsterError {
    file_upload_error_with_code(ApiErrorCode::UploadTempFileFlushFailed, message)
}

pub(crate) async fn stream_request_body_to_temp_upload(
    state: &PrimaryAppState,
    payload: &mut Payload,
    resolved_policy_hint: Option<crate::entities::storage_policy::Model>,
    declared_size: Option<i64>,
) -> Result<StreamedTempUpload> {
    let (temp_path, should_hash) = if let Some(policy) = resolved_policy_hint
        .as_ref()
        .filter(|policy| policy.driver_type == crate::types::DriverType::Local)
    {
        let staging_token = format!("{}.upload", uuid::Uuid::new_v4());
        let staging_path =
            crate::storage::drivers::local::upload_staging_path(policy, &staging_token)
                .map_aster_err_ctx(
                    "resolve local staging path",
                    AsterError::storage_driver_error,
                )?;
        if let Some(parent) = staging_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_aster_err(AsterError::storage_driver_error)?;
        }
        (
            staging_path.to_string_lossy().into_owned(),
            storage::local_content_dedup_enabled(policy),
        )
    } else {
        let temp_dir = &state.config().server.temp_dir;
        let runtime_temp_dir = crate::utils::paths::runtime_temp_dir(temp_dir);
        let temp_path = crate::utils::paths::runtime_temp_file_path(
            temp_dir,
            &uuid::Uuid::new_v4().to_string(),
        );
        tokio::fs::create_dir_all(&runtime_temp_dir)
            .await
            .map_aster_err_ctx("create temp dir", upload_temp_dir_create_failed)?;
        (temp_path, false)
    };

    let temp_file = tokio::fs::File::create(&temp_path)
        .await
        .map_aster_err_ctx("create temp", upload_temp_file_create_failed)?;
    let mut temp_file = BufWriter::new(temp_file);
    let mut size: i64 = 0;
    let mut hasher = should_hash.then(Sha256::new);

    let write_result = async {
        while let Some(chunk) = payload.next().await {
            let chunk = chunk.map_aster_err_with(|| {
                validation_error_with_code(
                    ApiErrorCode::UploadRequestBodyReadFailed,
                    "failed to read request body",
                )
            })?;
            if let Some(hasher) = hasher.as_mut() {
                hasher.update(&chunk);
            }
            temp_file
                .write_all(&chunk)
                .await
                .map_aster_err_ctx("write temp", upload_temp_file_write_failed)?;
            size = size
                .checked_add(usize_to_i64(chunk.len(), "request body chunk length")?)
                .ok_or_else(|| {
                    file_upload_error_with_code(
                        ApiErrorCode::UploadRequestBodySizeOverflow,
                        "accumulated request body size overflows i64",
                    )
                })?;
        }
        temp_file
            .flush()
            .await
            .map_aster_err_ctx("flush temp", upload_temp_file_flush_failed)?;
        Ok::<(), AsterError>(())
    }
    .await;

    drop(temp_file);

    if let Err(error) = write_result {
        crate::utils::cleanup_temp_file(&temp_path).await;
        return Err(error);
    }

    if let Some(declared_size) = declared_size
        && size != declared_size
    {
        crate::utils::cleanup_temp_file(&temp_path).await;
        return Err(validation_error_with_code(
            ApiErrorCode::UploadRequestSizeMismatch,
            "request body length does not match declared size",
        ));
    }

    let precomputed_hash =
        hasher.map(|hasher| crate::utils::hash::sha256_digest_to_hex(&hasher.finalize()));

    Ok(StreamedTempUpload {
        temp_path,
        size,
        resolved_policy: resolved_policy_hint,
        precomputed_hash,
    })
}

/// 从临时文件存储 blob 并创建文件记录
///
/// 公共函数，REST upload 和 WebDAV flush 都调用。
/// - local 开启 `content_dedup` 时流式计算 sha256（不加载全文件到内存）
/// - 策略检查 + 配额检查 + 按策略决定是否做 blob 去重
/// - `put_file` 零拷贝（LocalDriver rename）
/// - 创建/覆盖文件记录
///
/// `existing_file_id`: Some 时覆盖现有文件，None 时新建
///
/// 返回创建/更新的文件记录。临时文件可能被 put_file rename 走，调用方不要依赖它存在。
/// `skip_lock_check`: WebDAV 持锁者写入时为 true（WebDAV handler 已验证 lock token）
pub struct StoreFromTempRequest<'a> {
    pub folder_id: Option<i64>,
    pub filename: &'a str,
    pub temp_path: &'a str,
    pub size: i64,
    pub existing_file_id: Option<i64>,
    pub skip_lock_check: bool,
}

impl<'a> StoreFromTempRequest<'a> {
    pub fn new(folder_id: Option<i64>, filename: &'a str, temp_path: &'a str, size: i64) -> Self {
        Self {
            folder_id,
            filename,
            temp_path,
            size,
            existing_file_id: None,
            skip_lock_check: false,
        }
    }

    pub fn overwrite(mut self, existing_file_id: i64) -> Self {
        self.existing_file_id = Some(existing_file_id);
        self
    }

    pub fn skip_lock_check(mut self) -> Self {
        self.skip_lock_check = true;
        self
    }
}

pub async fn store_from_temp(
    state: &PrimaryAppState,
    user_id: i64,
    request: StoreFromTempRequest<'_>,
) -> Result<FileInfo> {
    storage::store_from_temp_internal(
        state,
        StoreFromTempParams {
            scope: WorkspaceStorageScope::Personal { user_id },
            folder_id: request.folder_id,
            filename: request.filename,
            temp_path: request.temp_path,
            size: request.size,
            existing_file_id: request.existing_file_id,
            skip_lock_check: request.skip_lock_check,
        },
        StoreFromTempHints::default(),
        NewFileMode::ResolveUnique,
        true,
    )
    .await
    .map(Into::into)
}

/// 上传文件（REST API，multipart）
pub async fn upload(
    state: &PrimaryAppState,
    user_id: i64,
    payload: &mut actix_multipart::Multipart,
    folder_id: Option<i64>,
    relative_path: Option<&str>,
    declared_size: Option<i64>,
) -> Result<FileInfo> {
    storage::upload_with_hints(
        state,
        WorkspaceStorageScope::Personal { user_id },
        payload,
        folder_id,
        relative_path,
        declared_size,
        WorkspaceUploadHints::default(),
    )
    .await
    .map(Into::into)
}

pub(crate) async fn update_content_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
    body: Bytes,
    if_match: Option<&str>,
) -> Result<(crate::entities::file::Model, String)> {
    let db = state.writer_db();
    tracing::debug!(
        scope = ?scope,
        file_id,
        content_size = body.len(),
        has_if_match = if_match.is_some(),
        "updating file content"
    );
    let f = storage::verify_file_access(state, scope, file_id).await?;

    if f.is_locked {
        let lock = crate::db::repository::lock_repo::find_by_entity(
            db,
            crate::types::EntityType::File,
            file_id,
        )
        .await?;
        if let Some(lock) = lock
            && lock.owner_id != Some(scope.actor_user_id())
        {
            return Err(AsterError::resource_locked(
                "file is locked by another user",
            ));
        }
    }

    let current_blob = crate::db::repository::file_repo::find_blob_by_id(db, f.blob_id).await?;
    if let Some(etag) = if_match {
        let expected = etag.trim_matches('"');
        if !expected.eq_ignore_ascii_case(&current_blob.hash) {
            return Err(precondition_failed_with_code(
                ApiErrorCode::FileEtagMismatch,
                "file has been modified (ETag mismatch)",
            ));
        }
    }

    let size = usize_to_i64(body.len(), "body length")?;
    let resolved_policy = storage::resolve_policy_for_size(state, scope, f.folder_id, size).await?;
    let result = if resolved_policy.driver_type == crate::types::DriverType::Local {
        let should_dedup = storage::local_content_dedup_enabled(&resolved_policy);
        let staging_token = format!("{}.upload", uuid::Uuid::new_v4());
        let staging_path =
            crate::storage::drivers::local::upload_staging_path(&resolved_policy, &staging_token)
                .map_aster_err_ctx(
                "resolve local staging path",
                AsterError::storage_driver_error,
            )?;
        if let Some(parent) = staging_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_aster_err(AsterError::storage_driver_error)?;
        }
        tokio::fs::write(&staging_path, &body)
            .await
            .map_aster_err(AsterError::storage_driver_error)?;

        let precomputed_hash = should_dedup.then(|| {
            let mut hasher = Sha256::new();
            hasher.update(&body);
            crate::utils::hash::sha256_digest_to_hex(&hasher.finalize())
        });
        let staging_path = staging_path.to_string_lossy().into_owned();
        let result = storage::store_from_temp_with_hints(
            state,
            StoreFromTempParams::new(scope, f.folder_id, &f.name, &staging_path, size)
                .overwrite(file_id)
                .skip_lock_check(),
            StoreFromTempHints {
                resolved_policy: Some(resolved_policy),
                precomputed_hash: precomputed_hash.as_deref(),
                actor_username: None,
                ..Default::default()
            },
        )
        .await;
        crate::utils::cleanup_temp_file(&staging_path).await;
        result
    } else {
        let temp_dir = &state.config().server.temp_dir;
        let runtime_temp_dir = crate::utils::paths::runtime_temp_dir(temp_dir);
        let temp_path = crate::utils::paths::runtime_temp_file_path(
            temp_dir,
            &uuid::Uuid::new_v4().to_string(),
        );
        tokio::fs::create_dir_all(&runtime_temp_dir)
            .await
            .map_aster_err(AsterError::storage_driver_error)?;
        tokio::fs::write(&temp_path, &body)
            .await
            .map_aster_err(AsterError::storage_driver_error)?;

        let result = storage::store_from_temp_internal(
            state,
            StoreFromTempParams::new(scope, f.folder_id, &f.name, &temp_path, size)
                .overwrite(file_id)
                .skip_lock_check(),
            StoreFromTempHints::default(),
            NewFileMode::ResolveUnique,
            true,
        )
        .await;
        crate::utils::cleanup_temp_file(&temp_path).await;
        result
    };

    let updated = result?;
    let new_blob = crate::db::repository::file_repo::find_blob_by_id(db, updated.blob_id).await?;
    tracing::debug!(
        scope = ?scope,
        file_id = updated.id,
        blob_id = updated.blob_id,
        size = updated.size,
        "updated file content"
    );
    Ok((updated, new_blob.hash.clone()))
}

pub(crate) async fn update_content_stream_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
    payload: &mut Payload,
    declared_size: Option<i64>,
    if_match: Option<&str>,
) -> Result<(crate::entities::file::Model, String)> {
    let db = state.writer_db();
    tracing::debug!(
        scope = ?scope,
        file_id,
        declared_size,
        has_if_match = if_match.is_some(),
        "streaming file content update"
    );
    let f = storage::verify_file_access(state, scope, file_id).await?;

    if f.is_locked {
        let lock = crate::db::repository::lock_repo::find_by_entity(
            db,
            crate::types::EntityType::File,
            file_id,
        )
        .await?;
        if let Some(lock) = lock
            && lock.owner_id != Some(scope.actor_user_id())
        {
            return Err(AsterError::resource_locked(
                "file is locked by another user",
            ));
        }
    }

    let current_blob = crate::db::repository::file_repo::find_blob_by_id(db, f.blob_id).await?;
    if let Some(etag) = if_match {
        let expected = etag.trim_matches('"');
        if !expected.eq_ignore_ascii_case(&current_blob.hash) {
            return Err(precondition_failed_with_code(
                ApiErrorCode::FileEtagMismatch,
                "file has been modified (ETag mismatch)",
            ));
        }
    }

    let resolved_policy_hint = match declared_size {
        Some(size) => {
            Some(storage::resolve_policy_for_size(state, scope, f.folder_id, size).await?)
        }
        None => None,
    };
    let streamed =
        stream_request_body_to_temp_upload(state, payload, resolved_policy_hint, declared_size)
            .await?;
    let StreamedTempUpload {
        temp_path,
        size,
        resolved_policy,
        precomputed_hash,
    } = streamed;

    let result = storage::store_from_temp_with_hints(
        state,
        StoreFromTempParams::new(scope, f.folder_id, &f.name, &temp_path, size)
            .overwrite(file_id)
            .skip_lock_check(),
        StoreFromTempHints {
            resolved_policy,
            precomputed_hash: precomputed_hash.as_deref(),
            actor_username: None,
            ..Default::default()
        },
    )
    .await;
    crate::utils::cleanup_temp_file(&temp_path).await;

    let updated = result?;
    let new_blob = crate::db::repository::file_repo::find_blob_by_id(db, updated.blob_id).await?;
    tracing::debug!(
        scope = ?scope,
        file_id = updated.id,
        blob_id = updated.blob_id,
        size = updated.size,
        "completed streamed file content update"
    );
    Ok((updated, new_blob.hash.clone()))
}

/// 覆盖文件内容（REST API 编辑入口）
///
/// 支持 ETag 乐观锁（If-Match）+ 悲观锁检查（is_locked）。
/// 自动创建版本历史。返回 (更新后的 file, 新 blob hash)。
pub async fn update_content(
    state: &PrimaryAppState,
    file_id: i64,
    user_id: i64,
    body: Bytes,
    if_match: Option<&str>,
) -> Result<(FileInfo, String)> {
    update_content_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        file_id,
        body,
        if_match,
    )
    .await
    .map(|(file, hash)| (file.into(), hash))
}

pub async fn resolve_policy_for_size(
    state: &PrimaryAppState,
    user_id: i64,
    folder_id: Option<i64>,
    file_size: i64,
) -> Result<StoragePolicy> {
    storage::resolve_policy_for_size(
        state,
        WorkspaceStorageScope::Personal { user_id },
        folder_id,
        file_size,
    )
    .await
    .map(StoragePolicy::from)
}

/// 直接创建空文件（0 字节），不走 multipart upload 流程。
///
/// - 校验文件名
/// - 解析存储策略
/// - 只有 local 显式开启 `content_dedup` 时才复用空文件固定 sha256
/// - 其余路径都为每个文件分配独立 blob
/// - 创建文件记录并更新配额（0 字节不影响配额）
pub async fn create_empty(
    state: &PrimaryAppState,
    user_id: i64,
    folder_id: Option<i64>,
    filename: &str,
) -> Result<FileInfo> {
    storage::create_empty(
        state,
        WorkspaceStorageScope::Personal { user_id },
        folder_id,
        filename,
    )
    .await
    .map(Into::into)
}
