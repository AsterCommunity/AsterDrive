//! 工作空间存储服务子模块：`blob_upload`。

use sea_orm::ConnectionTrait;
use std::path::{Component, Path, PathBuf};
use tokio::io::AsyncRead;

use super::{StorageOperationContext, create_nondedup_blob_with_key, create_opaque_nondedup_blob};
use crate::entities::file_blob;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::storage::StorageConnectorObjectNamingMode;
use crate::storage::connectors::{StorageConnectorUploadTransport, resolve_policy_object_naming};

#[derive(Debug, Clone)]
pub(crate) enum PreparedNonDedupBlobUpload {
    Local {
        base_path: PathBuf,
        blob_key: String,
        storage_path: String,
        size: i64,
        policy_id: i64,
    },
    Opaque {
        upload_id: String,
        hash_prefix: &'static str,
        storage_path: String,
        size: i64,
        policy_id: i64,
    },
}

impl PreparedNonDedupBlobUpload {
    pub(crate) fn storage_path(&self) -> &str {
        match self {
            Self::Local { storage_path, .. } | Self::Opaque { storage_path, .. } => storage_path,
        }
    }

    pub(crate) fn size(&self) -> i64 {
        match self {
            Self::Local { size, .. } | Self::Opaque { size, .. } => *size,
        }
    }

    pub(crate) fn policy_id(&self) -> i64 {
        match self {
            Self::Local { policy_id, .. } | Self::Opaque { policy_id, .. } => *policy_id,
        }
    }

    pub(crate) fn ensure_matches(&self, size: i64, policy_id: i64, context: &str) -> Result<()> {
        if size < 0 {
            return Err(AsterError::validation_error(format!(
                "{context} size must be non-negative, got {size}",
            )));
        }
        if self.size() != size {
            return Err(AsterError::validation_error(format!(
                "preuploaded blob size {} does not match {context} size {size}",
                self.size(),
            )));
        }
        if self.policy_id() != policy_id {
            return Err(AsterError::validation_error(format!(
                "preuploaded blob policy {} does not match {context} policy {policy_id}",
                self.policy_id(),
            )));
        }
        Ok(())
    }
}

pub(crate) fn prepare_non_dedup_blob_upload(
    policy: &crate::entities::storage_policy::Model,
    size: i64,
    filename: Option<&str>,
) -> Result<PreparedNonDedupBlobUpload> {
    match crate::storage::connectors::resolve_policy_upload_transport(policy)? {
        StorageConnectorUploadTransport::Local => {
            let blob_key = aster_forge_utils::id::new_short_token();
            Ok(PreparedNonDedupBlobUpload::Local {
                base_path: crate::storage::drivers::local::effective_base_path(policy),
                storage_path: aster_forge_validation::filename::storage_path_from_blob_key(
                    &blob_key,
                )?,
                blob_key,
                size,
                policy_id: policy.id,
            })
        }
        transport => {
            let upload_id = aster_forge_utils::id::new_uuid();
            let hash_prefix = transport.opaque_blob_hash_prefix().ok_or_else(|| {
                AsterError::validation_error(format!(
                    "storage policy driver '{}' cannot prepare opaque blob uploads without an opaque hash prefix",
                    policy.driver_type.as_str()
                ))
            })?;
            let storage_path = nondedup_storage_path_for_policy(policy, &upload_id, filename)?;
            Ok(PreparedNonDedupBlobUpload::Opaque {
                storage_path,
                upload_id,
                hash_prefix,
                size,
                policy_id: policy.id,
            })
        }
    }
}

pub(crate) fn nondedup_storage_path_for_policy(
    policy: &crate::entities::storage_policy::Model,
    upload_id: &str,
    filename: Option<&str>,
) -> Result<String> {
    match resolve_policy_object_naming(policy)? {
        StorageConnectorObjectNamingMode::OpaqueUuid => {
            let upload_id = uuid::Uuid::parse_str(upload_id)
                .map_err(|_| AsterError::validation_error("upload id must be a UUID"))?;
            Ok(format!("files/{upload_id}"))
        }
        StorageConnectorObjectNamingMode::OriginalFilename => {
            original_filename_storage_path(upload_id, filename)
        }
    }
}

fn original_filename_storage_path(upload_id: &str, filename: Option<&str>) -> Result<String> {
    let upload_id = uuid::Uuid::parse_str(upload_id)
        .map_err(|_| AsterError::validation_error("upload id must be a UUID"))?;
    let Some(filename) = filename else {
        return Ok(format!("files/{upload_id}"));
    };
    let filename = aster_forge_validation::filename::normalize_validate_name(filename)?;
    Ok(format!("files/{upload_id}/{filename}"))
}

#[cfg(test)]
mod storage_path_tests {
    use super::{nondedup_storage_path_for_policy, original_filename_storage_path};
    use crate::types::DriverType;

    const UPLOAD_ID: &str = "550e8400-e29b-41d4-a716-446655440000";

    fn policy(driver_type: DriverType) -> crate::entities::storage_policy::Model {
        let now = chrono::Utc::now();
        crate::entities::storage_policy::Model {
            id: 1,
            name: "test".to_string(),
            driver_type,
            endpoint: String::new(),
            bucket: String::new(),
            access_key: String::new(),
            secret_key: String::new(),
            base_path: String::new(),
            remote_node_id: None,
            remote_storage_target_key: None,
            max_file_size: 0,
            allowed_types: crate::types::StoredStoragePolicyAllowedTypes::empty(),
            options: crate::types::StoredStoragePolicyOptions::empty(),
            is_default: false,
            chunk_size: 5_242_880,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn original_filename_path_keeps_uuid_namespace_and_filename() {
        assert_eq!(
            original_filename_storage_path(UPLOAD_ID, Some("video name.mp4")).unwrap(),
            "files/550e8400-e29b-41d4-a716-446655440000/video name.mp4"
        );
    }

    #[test]
    fn original_filename_path_keeps_same_name_uploads_isolated_by_uuid() {
        let first = original_filename_storage_path(UPLOAD_ID, Some("same-name.mp4")).unwrap();
        let second = original_filename_storage_path(
            "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
            Some("same-name.mp4"),
        )
        .unwrap();

        assert_ne!(first, second);
        assert!(first.ends_with("/same-name.mp4"));
        assert!(second.ends_with("/same-name.mp4"));
    }

    #[test]
    fn original_filename_path_supports_legacy_layout_and_rejects_bad_inputs() {
        assert_eq!(
            original_filename_storage_path(UPLOAD_ID, None).unwrap(),
            "files/550e8400-e29b-41d4-a716-446655440000"
        );
        assert!(original_filename_storage_path("../escape", Some("video.mp4")).is_err());
        assert!(original_filename_storage_path(UPLOAD_ID, Some("bad/name.mp4")).is_err());
        assert!(original_filename_storage_path(UPLOAD_ID, Some("bad\\name.mp4")).is_err());
        assert!(original_filename_storage_path(UPLOAD_ID, Some("")).is_err());
    }

    #[test]
    fn original_filename_path_normalizes_unicode_filename() {
        assert_eq!(
            original_filename_storage_path(UPLOAD_ID, Some("cafe\u{301}.txt")).unwrap(),
            "files/550e8400-e29b-41d4-a716-446655440000/caf\u{e9}.txt"
        );
    }

    #[test]
    fn opaque_uuid_path_ignores_provider_filename_layout() {
        assert_eq!(
            nondedup_storage_path_for_policy(
                &policy(DriverType::S3),
                UPLOAD_ID,
                Some("../../ignored.txt"),
            )
            .unwrap(),
            "files/550e8400-e29b-41d4-a716-446655440000"
        );
        assert!(
            nondedup_storage_path_for_policy(
                &policy(DriverType::S3),
                "not-a-uuid",
                Some("ignored.txt"),
            )
            .is_err()
        );
    }

    #[test]
    fn policy_object_naming_capability_selects_path_layout_explicitly() {
        assert_eq!(
            nondedup_storage_path_for_policy(
                &policy(DriverType::OneDrive),
                UPLOAD_ID,
                Some("video.mp4"),
            )
            .unwrap(),
            "files/550e8400-e29b-41d4-a716-446655440000/video.mp4"
        );
        assert_eq!(
            nondedup_storage_path_for_policy(
                &policy(DriverType::S3),
                UPLOAD_ID,
                Some("video.mp4"),
            )
            .unwrap(),
            "files/550e8400-e29b-41d4-a716-446655440000"
        );
    }
}

fn normalize_absolute_cleanup_path(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
            Component::Normal(part) => normalized.push(part),
        }
    }

    Some(normalized)
}

fn normalize_cleanup_root(path: &Path) -> Option<PathBuf> {
    if path.is_absolute() {
        return normalize_absolute_cleanup_path(path);
    }

    let current_dir = std::env::current_dir().ok()?;
    normalize_absolute_cleanup_path(&current_dir.join(path))
}

async fn cleanup_empty_local_blob_dirs(prefix_dir: &Path, root_dir: &Path) {
    let Some(mut current) = normalize_cleanup_root(prefix_dir) else {
        tracing::warn!(
            "skip blob dir cleanup for unresolved prefix {}",
            prefix_dir.display()
        );
        return;
    };
    let Some(root_dir) = normalize_cleanup_root(root_dir) else {
        tracing::warn!(
            "skip blob dir cleanup for unresolved root {}",
            root_dir.display()
        );
        return;
    };

    if current == root_dir || !current.starts_with(&root_dir) {
        tracing::warn!(
            "skip blob dir cleanup outside storage root: prefix={}, root={}",
            current.display(),
            root_dir.display()
        );
        return;
    }

    while current != root_dir {
        match tokio::fs::remove_dir(&current).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => break,
            Err(error) => {
                tracing::warn!("failed to cleanup blob dir {}: {error}", current.display());
                break;
            }
        }

        let Some(parent) = current.parent() else {
            break;
        };
        current = parent.to_path_buf();
    }
}

pub(crate) async fn cleanup_preuploaded_blob_upload(
    driver: &dyn crate::storage::StorageDriver,
    prepared: &PreparedNonDedupBlobUpload,
    reason: &str,
) {
    match prepared {
        PreparedNonDedupBlobUpload::Local {
            base_path,
            storage_path,
            ..
        } => {
            let full_path = base_path.join(storage_path.trim_start_matches('/'));
            match tokio::fs::remove_file(&full_path).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    tracing::warn!(
                        storage_path = %storage_path,
                        full_path = %full_path.display(),
                        "failed to cleanup preuploaded local blob after {reason}: {error}"
                    );
                    return;
                }
            }

            if let Some(parent) = full_path.parent() {
                cleanup_empty_local_blob_dirs(parent, base_path).await;
            }
        }
        PreparedNonDedupBlobUpload::Opaque { .. } => {
            if let Err(cleanup_err) = driver.delete(prepared.storage_path()).await {
                tracing::warn!(
                    storage_path = %prepared.storage_path(),
                    "failed to cleanup preuploaded blob after {reason}: {cleanup_err}"
                );
            }
        }
    }
}

pub(crate) async fn upload_temp_file_to_prepared_blob(
    driver: &dyn crate::storage::StorageDriver,
    prepared: &PreparedNonDedupBlobUpload,
    temp_path: &str,
) -> Result<()> {
    let stream_driver = driver.extensions().stream_upload.ok_or_else(|| {
        crate::errors::AsterError::storage_driver_error("stream upload not supported")
    })?;

    if let Err(error) = stream_driver
        .put_file(prepared.storage_path(), temp_path)
        .await
    {
        cleanup_preuploaded_blob_upload(driver, prepared, "upload error").await;
        return Err(error);
    }

    Ok(())
}

pub(crate) async fn upload_temp_file_to_prepared_blob_cancellable(
    driver: &dyn crate::storage::StorageDriver,
    prepared: &PreparedNonDedupBlobUpload,
    temp_path: &str,
    operation_context: &StorageOperationContext,
) -> Result<()> {
    if let PreparedNonDedupBlobUpload::Local {
        base_path,
        storage_path,
        size,
        ..
    } = prepared
    {
        return upload_temp_file_to_local_prepared_blob_cancellable(
            base_path,
            storage_path,
            *size,
            temp_path,
            operation_context,
        )
        .await;
    }

    operation_context.checkpoint()?;
    let file = tokio::fs::File::open(temp_path).await.map_aster_err_ctx(
        "open temp file for upload",
        AsterError::storage_driver_error,
    )?;
    let reader = operation_context.wrap_reader(Box::new(file));
    upload_reader_to_prepared_blob_with_context(
        driver,
        prepared,
        reader,
        prepared_size(prepared),
        operation_context,
    )
    .await
}

async fn upload_temp_file_to_local_prepared_blob_cancellable(
    base_path: &Path,
    storage_path: &str,
    size: i64,
    temp_path: &str,
    operation_context: &StorageOperationContext,
) -> Result<()> {
    operation_context.checkpoint()?;
    let full_path = base_path.join(storage_path.trim_start_matches('/'));
    if let Some(parent) = full_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_aster_err(AsterError::storage_driver_error)?;
    }
    operation_context.checkpoint()?;

    let result = match tokio::fs::hard_link(temp_path, &full_path).await {
        Ok(()) => validate_local_prepared_blob(&full_path, size, operation_context).await,
        Err(link_error) => {
            copy_temp_file_to_local_prepared_blob(
                temp_path,
                &full_path,
                size,
                link_error,
                operation_context,
            )
            .await
        }
    };

    if let Err(error) = result {
        cleanup_local_prepared_blob(
            base_path,
            &full_path,
            storage_path,
            "cancellable upload error",
        )
        .await;
        return Err(error);
    }

    if let Err(error) = operation_context.checkpoint() {
        cleanup_local_prepared_blob(
            base_path,
            &full_path,
            storage_path,
            "cancellation after local upload",
        )
        .await;
        return Err(error);
    }
    Ok(())
}

async fn copy_temp_file_to_local_prepared_blob(
    temp_path: &str,
    full_path: &Path,
    size: i64,
    link_error: std::io::Error,
    operation_context: &StorageOperationContext,
) -> Result<()> {
    operation_context.checkpoint()?;
    let copied = crate::storage::drivers::local::copy_file_with_checkpoint(
        Path::new(temp_path),
        full_path,
        || operation_context.checkpoint(),
        "local upload",
    )
    .await
    .map_err(|error| {
        AsterError::storage_driver_error(format!(
            "copy local upload after hardlink failed ({link_error}): {}",
            error.message()
        ))
    })?;
    let copied = i64::try_from(copied).map_err(|_| {
        AsterError::storage_driver_error("local upload copied size exceeds i64 range")
    })?;

    if copied != size {
        return Err(AsterError::storage_driver_error(format!(
            "local upload copy size mismatch: expected {size}, copied {copied}"
        )));
    }

    Ok(())
}

async fn validate_local_prepared_blob(
    full_path: &Path,
    size: i64,
    operation_context: &StorageOperationContext,
) -> Result<()> {
    let expected_size = aster_forge_utils::numbers::i64_to_u64(size, "local upload blob size")?;
    let metadata = tokio::fs::metadata(full_path).await.map_aster_err_ctx(
        "inspect local upload blob",
        AsterError::storage_driver_error,
    )?;
    if metadata.len() != expected_size {
        return Err(AsterError::storage_driver_error(format!(
            "local upload size mismatch: expected {expected_size}, actual {}",
            metadata.len()
        )));
    }
    operation_context.checkpoint()
}

async fn cleanup_local_prepared_blob(
    base_path: &Path,
    full_path: &Path,
    storage_path: &str,
    reason: &str,
) {
    match tokio::fs::remove_file(full_path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(
                storage_path,
                full_path = %full_path.display(),
                "failed to cleanup local prepared blob after {reason}: {error}"
            );
            return;
        }
    }

    if let Some(parent) = full_path.parent() {
        cleanup_empty_local_blob_dirs(parent, base_path).await;
    }
}

pub(crate) async fn upload_reader_to_prepared_blob(
    driver: &dyn crate::storage::StorageDriver,
    prepared: &PreparedNonDedupBlobUpload,
    reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
    size: i64,
) -> Result<()> {
    let stream_driver = driver.extensions().stream_upload.ok_or_else(|| {
        crate::errors::AsterError::storage_driver_error("stream upload not supported")
    })?;

    if let Err(error) = stream_driver
        .put_reader(prepared.storage_path(), reader, size)
        .await
    {
        cleanup_preuploaded_blob_upload(driver, prepared, "stream upload error").await;
        return Err(error);
    }

    Ok(())
}

async fn upload_reader_to_prepared_blob_with_context(
    driver: &dyn crate::storage::StorageDriver,
    prepared: &PreparedNonDedupBlobUpload,
    reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
    size: i64,
    operation_context: &StorageOperationContext,
) -> Result<()> {
    let stream_driver = driver.extensions().stream_upload.ok_or_else(|| {
        crate::errors::AsterError::storage_driver_error("stream upload not supported")
    })?;

    operation_context.checkpoint()?;
    if let Err(error) = stream_driver
        .put_reader(prepared.storage_path(), reader, size)
        .await
    {
        cleanup_preuploaded_blob_upload(driver, prepared, "stream upload error").await;
        operation_context.checkpoint()?;
        return Err(error);
    }
    if let Err(error) = operation_context.checkpoint() {
        cleanup_preuploaded_blob_upload(driver, prepared, "cancellation after stream upload").await;
        return Err(error);
    }

    Ok(())
}

fn prepared_size(prepared: &PreparedNonDedupBlobUpload) -> i64 {
    match prepared {
        PreparedNonDedupBlobUpload::Local { size, .. }
        | PreparedNonDedupBlobUpload::Opaque { size, .. } => *size,
    }
}

pub(crate) async fn persist_preuploaded_blob<C: ConnectionTrait>(
    db: &C,
    prepared: &PreparedNonDedupBlobUpload,
) -> Result<file_blob::Model> {
    match prepared {
        PreparedNonDedupBlobUpload::Local {
            blob_key,
            storage_path,
            size,
            policy_id,
            ..
        } => create_nondedup_blob_with_key(db, *size, *policy_id, blob_key, storage_path).await,
        PreparedNonDedupBlobUpload::Opaque {
            upload_id,
            hash_prefix,
            size,
            policy_id,
            ..
        } => create_opaque_nondedup_blob(db, *size, *policy_id, hash_prefix, upload_id).await,
    }
}
