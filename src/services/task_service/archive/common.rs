//! 归档任务子模块：`common`。

use std::io::{Read, Write};
use std::time::Instant;

use chrono::Utc;
use sea_orm::{DatabaseConnection, Set};

use crate::db::repository::{file_repo, folder_repo};
use crate::entities::{file, folder};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::services::task_service::TaskLeaseGuard;
use crate::services::{
    folder_service,
    workspace_storage_service::{WorkspaceStorageScope, load_scope_actor_username},
};
use crate::storage::{DriverRegistry, PolicySnapshot};

use super::selection::ArchiveBuildLimits;

#[derive(Debug, Clone)]
pub(super) struct ArchiveFileEntry {
    pub(super) blob_id: i64,
    pub(super) size: i64,
    pub(super) store_without_deflate: bool,
}

impl ArchiveFileEntry {
    pub(super) fn from_file(file: &file::Model, entry_path: &str) -> Self {
        let file_name = archive_entry_file_name(entry_path);
        Self {
            blob_id: file.blob_id,
            size: file.size,
            store_without_deflate: should_store_without_deflate(file_name, &file.mime_type),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) enum ArchiveEntry {
    Directory {
        entry_path: String,
    },
    File {
        file: ArchiveFileEntry,
        entry_path: String,
    },
}

impl ArchiveEntry {
    pub(super) fn entry_path(&self) -> &str {
        match self {
            Self::Directory { entry_path } | Self::File { entry_path, .. } => entry_path,
        }
    }

    pub(super) fn is_file(&self) -> bool {
        matches!(self, Self::File { .. })
    }
}

pub(super) async fn build_folder_display_path(
    db: &DatabaseConnection,
    folder_id: i64,
) -> Result<String> {
    let mut paths = folder_service::build_folder_paths(db, &[folder_id]).await?;
    paths
        .remove(&folder_id)
        .ok_or_else(|| AsterError::record_not_found(format!("folder #{folder_id} path")))
}

pub(super) async fn build_file_display_path(
    db: &DatabaseConnection,
    folder_id: Option<i64>,
    file_name: &str,
) -> Result<String> {
    match folder_id {
        Some(folder_id) => Ok(format!(
            "{}/{}",
            build_folder_display_path(db, folder_id).await?,
            file_name
        )),
        None => Ok(format!("/{file_name}")),
    }
}

pub(super) async fn create_unique_folder_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    parent_id: Option<i64>,
    base_name: &str,
) -> Result<folder::Model> {
    let final_name =
        resolve_unique_folder_name_in_scope(state, scope, parent_id, base_name).await?;
    create_folder_exact_in_scope(state, scope, parent_id, &final_name).await
}

pub(super) async fn create_folder_exact_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    parent_id: Option<i64>,
    name: &str,
) -> Result<folder::Model> {
    let name = crate::utils::normalize_validate_name(name)?;
    let exists = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            folder_repo::find_by_name_in_parent(&state.db, user_id, parent_id, &name)
                .await?
                .is_some()
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            folder_repo::find_by_name_in_team_parent(&state.db, team_id, parent_id, &name)
                .await?
                .is_some()
        }
    };
    if exists {
        return Err(folder_repo::duplicate_name_error(&name));
    }

    let now = Utc::now();
    let created_by_username = load_scope_actor_username(&state.db, scope).await?;
    folder_repo::create(
        &state.db,
        folder::ActiveModel {
            name: Set(name),
            parent_id: Set(parent_id),
            team_id: Set(scope.team_id()),
            owner_user_id: Set(scope.owner_user_id()),
            created_by_user_id: Set(Some(scope.actor_user_id())),
            created_by_username: Set(created_by_username),
            policy_id: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
}

async fn resolve_unique_folder_name_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    parent_id: Option<i64>,
    base_name: &str,
) -> Result<String> {
    let mut candidate = crate::utils::normalize_validate_name(base_name)?;
    loop {
        let exists = match scope {
            WorkspaceStorageScope::Personal { user_id } => {
                folder_repo::find_by_name_in_parent(&state.db, user_id, parent_id, &candidate)
                    .await?
            }
            WorkspaceStorageScope::Team { team_id, .. } => {
                folder_repo::find_by_name_in_team_parent(&state.db, team_id, parent_id, &candidate)
                    .await?
            }
        };
        if exists.is_none() {
            return Ok(candidate);
        }
        candidate = crate::utils::next_copy_name(&candidate);
    }
}

pub(super) struct ArchiveSinkContext<'a> {
    pub handle: &'a tokio::runtime::Handle,
    pub db: &'a DatabaseConnection,
    pub driver_registry: &'a DriverRegistry,
    pub policy_snapshot: &'a PolicySnapshot,
    pub lease_guard: Option<&'a TaskLeaseGuard>,
}

pub(super) fn write_archive_to_sink<W, F>(
    ctx: ArchiveSinkContext<'_>,
    entries: Vec<ArchiveEntry>,
    total_bytes: i64,
    limits: ArchiveBuildLimits,
    output: W,
    mut on_progress: F,
) -> Result<(W, i64)>
where
    W: Write,
    F: FnMut(i64, &str) -> Result<()>,
{
    let mut zip = zip::ZipWriter::new_stream(output);
    let dir_options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut processed_bytes = 0_i64;
    let mut written_bytes = 0_i64;

    for entry in entries {
        ensure_task_lease_active(ctx.lease_guard)?;
        match entry {
            ArchiveEntry::Directory { entry_path } => {
                written_bytes = checked_archive_output_progress(written_bytes, 256, limits)?;
                zip.add_directory(&entry_path, dir_options)
                    .map_aster_err(AsterError::storage_driver_error)?;
            }
            ArchiveEntry::File { file, entry_path } => {
                written_bytes = checked_archive_output_progress(written_bytes, file.size, limits)?;
                let file_options = archive_file_options(file.store_without_deflate);
                zip.start_file(&entry_path, file_options)
                    .map_aster_err(AsterError::storage_driver_error)?;

                let stream = ctx.handle.block_on(async {
                    let blob = file_repo::find_blob_by_id(ctx.db, file.blob_id).await?;
                    let policy = ctx.policy_snapshot.get_policy_or_err(blob.policy_id)?;
                    let driver = ctx.driver_registry.get_driver(&policy)?;
                    driver.get_stream(&blob.storage_path).await
                })?;

                let mut reader = tokio_util::io::SyncIoBridge::new(stream);
                let copied =
                    copy_reader_to_writer_with_lease(ctx.lease_guard, &mut reader, &mut zip)?;
                processed_bytes = processed_bytes
                    .checked_add(i64::try_from(copied).map_err(|_| {
                        AsterError::internal_error(format!(
                            "copied bytes exceed i64 range: {copied}"
                        ))
                    })?)
                    .ok_or_else(|| AsterError::internal_error("archive progress overflow"))?;

                on_progress(processed_bytes, &entry_path)?;
            }
        }
    }

    let output = zip
        .finish()
        .map_aster_err(AsterError::storage_driver_error)?
        .into_inner();
    Ok((output, processed_bytes.max(total_bytes)))
}

fn checked_archive_output_progress(
    current: i64,
    added: i64,
    limits: ArchiveBuildLimits,
) -> Result<i64> {
    let next = current
        .checked_add(added)
        .ok_or_else(|| AsterError::internal_error("archive output size overflow"))?;
    if next > limits.max_temp_bytes {
        return Err(AsterError::validation_error(format!(
            "archive output size {} exceeds server limit {}",
            next, limits.max_temp_bytes
        )));
    }
    Ok(next)
}

fn archive_file_options(store_without_deflate: bool) -> zip::write::SimpleFileOptions {
    let options = zip::write::SimpleFileOptions::default();
    if store_without_deflate {
        options.compression_method(zip::CompressionMethod::Stored)
    } else {
        options.compression_method(zip::CompressionMethod::Deflated)
    }
}

fn archive_entry_file_name(entry_path: &str) -> &str {
    entry_path.rsplit('/').next().unwrap_or(entry_path)
}

const STORED_MIME_TYPES: &[&str] = &[
    "application/pdf",
    "application/zip",
    "application/x-zip-compressed",
    "application/x-7z-compressed",
    "application/vnd.rar",
    "application/x-rar-compressed",
    "application/gzip",
    "application/x-gzip",
    "application/x-xz",
    "application/zstd",
    "application/x-zstd",
];

const STORED_EXTENSIONS: &[&str] = &[
    "zip", "7z", "rar", "gz", "tgz", "bz2", "xz", "zst", "jpg", "jpeg", "png", "gif", "webp",
    "heic", "heif", "avif", "mp4", "m4v", "mov", "mkv", "webm", "mp3", "aac", "ogg", "flac", "pdf",
];

fn should_store_without_deflate(name: &str, mime_type: &str) -> bool {
    if starts_with_ignore_ascii_case(mime_type, "image/")
        || starts_with_ignore_ascii_case(mime_type, "video/")
        || starts_with_ignore_ascii_case(mime_type, "audio/")
        || contains_ignore_ascii_case(STORED_MIME_TYPES, mime_type)
    {
        return true;
    }

    let Some(extension) = name.rsplit('.').next() else {
        return false;
    };
    contains_ignore_ascii_case(STORED_EXTENSIONS, extension)
}

fn starts_with_ignore_ascii_case(value: &str, prefix: &str) -> bool {
    value
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

pub(super) fn ends_with_ignore_ascii_case(value: &str, suffix: &str) -> bool {
    value
        .get(value.len().saturating_sub(suffix.len())..)
        .is_some_and(|tail| tail.eq_ignore_ascii_case(suffix))
}

fn contains_ignore_ascii_case(values: &[&str], needle: &str) -> bool {
    values
        .iter()
        .any(|value| needle.eq_ignore_ascii_case(value))
}

pub(super) fn is_client_disconnect_error_text(error_text: &str) -> bool {
    error_text.contains("Broken pipe")
        || error_text.contains("Connection reset by peer")
        || error_text.contains("connection closed")
}

pub(super) fn copy_reader_to_writer_with_lease<R: Read, W: Write>(
    lease_guard: Option<&TaskLeaseGuard>,
    reader: &mut R,
    writer: &mut W,
) -> Result<u64> {
    copy_reader_to_writer_internal(lease_guard, reader, writer, None, None)
}

pub(super) fn copy_reader_to_writer_with_lease_and_expected_size<R: Read, W: Write>(
    lease_guard: Option<&TaskLeaseGuard>,
    reader: &mut R,
    writer: &mut W,
    expected_bytes: u64,
    context: &str,
    deadline: Option<Instant>,
) -> Result<u64> {
    copy_reader_to_writer_internal(
        lease_guard,
        reader,
        writer,
        Some((expected_bytes, context)),
        deadline,
    )
}

fn copy_reader_to_writer_internal<R: Read, W: Write>(
    lease_guard: Option<&TaskLeaseGuard>,
    reader: &mut R,
    writer: &mut W,
    expected: Option<(u64, &str)>,
    deadline: Option<Instant>,
) -> Result<u64> {
    let mut copied = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        ensure_task_lease_active(lease_guard)?;
        ensure_deadline_active(deadline)?;
        let read = reader.read(&mut buffer).map_aster_err_ctx(
            "read archive stream chunk",
            AsterError::storage_driver_error,
        )?;
        if read == 0 {
            break;
        }
        let read_u64 = crate::utils::numbers::usize_to_u64(read, "archive stream chunk size")?;
        let next_copied = copied
            .checked_add(read_u64)
            .ok_or_else(|| AsterError::internal_error("archive stream byte counter overflow"))?;
        if let Some((expected_bytes, context)) = expected
            && next_copied > expected_bytes
        {
            return Err(AsterError::validation_error(format!(
                "{context} expands beyond declared size: declared {expected_bytes} bytes"
            )));
        }
        writer.write_all(&buffer[..read]).map_aster_err_ctx(
            "write archive stream chunk",
            AsterError::storage_driver_error,
        )?;
        copied = next_copied;
    }

    if let Some((expected_bytes, context)) = expected
        && copied != expected_bytes
    {
        return Err(AsterError::validation_error(format!(
            "{context} size mismatch: declared {expected_bytes} bytes, extracted {copied} bytes"
        )));
    }

    Ok(copied)
}

fn ensure_task_lease_active(lease_guard: Option<&TaskLeaseGuard>) -> Result<()> {
    if let Some(lease_guard) = lease_guard {
        lease_guard.ensure_active()?;
    }
    Ok(())
}

fn ensure_deadline_active(deadline: Option<Instant>) -> Result<()> {
    if let Some(deadline) = deadline
        && Instant::now() > deadline
    {
        return Err(AsterError::validation_error(
            "archive operation exceeded server time limit",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read};
    use std::thread;
    use std::time::Duration;

    use crate::services::task_service::{
        TaskLease, TaskLeaseGuard, is_task_lease_renewal_timed_out,
    };

    use super::{
        copy_reader_to_writer_with_lease, copy_reader_to_writer_with_lease_and_expected_size,
    };

    struct SlowSingleChunkReader {
        chunk: Vec<u8>,
        delay: Duration,
        consumed: bool,
    }

    impl Read for SlowSingleChunkReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.consumed {
                return Ok(0);
            }
            thread::sleep(self.delay);
            let len = self.chunk.len().min(buf.len());
            buf[..len].copy_from_slice(&self.chunk[..len]);
            self.consumed = true;
            Ok(len)
        }
    }

    #[test]
    fn copy_reader_to_writer_with_lease_stops_after_renewal_timeout() {
        let lease_guard =
            TaskLeaseGuard::with_renewal_timeout(TaskLease::new(42, 7), Duration::from_millis(20));
        let mut reader = SlowSingleChunkReader {
            chunk: b"chunk".to_vec(),
            delay: Duration::from_millis(30),
            consumed: false,
        };
        let mut writer = Vec::new();

        let error = copy_reader_to_writer_with_lease(Some(&lease_guard), &mut reader, &mut writer)
            .expect_err("expired lease should stop blocking copy loop");

        assert!(is_task_lease_renewal_timed_out(&error));
        assert_eq!(writer, b"chunk");
    }

    #[test]
    fn copy_reader_to_writer_with_expected_size_rejects_oversized_stream() {
        let mut reader = Cursor::new(b"abcdef".to_vec());
        let mut writer = Vec::new();

        let error = copy_reader_to_writer_with_lease_and_expected_size(
            None,
            &mut reader,
            &mut writer,
            4,
            "archive entry 'payload.bin'",
            None,
        )
        .expect_err("oversized stream should be rejected");

        assert_eq!(error.code(), "E005");
        assert!(error.message().contains("expands beyond declared size"));
        assert!(writer.is_empty());
    }

    #[test]
    fn copy_reader_to_writer_with_expected_size_rejects_truncated_stream() {
        let mut reader = Cursor::new(b"abc".to_vec());
        let mut writer = Vec::new();

        let error = copy_reader_to_writer_with_lease_and_expected_size(
            None,
            &mut reader,
            &mut writer,
            4,
            "archive entry 'payload.bin'",
            None,
        )
        .expect_err("truncated stream should be rejected");

        assert_eq!(error.code(), "E005");
        assert!(error.message().contains("size mismatch"));
        assert_eq!(writer, b"abc");
    }
}
