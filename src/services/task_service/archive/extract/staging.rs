//! 归档解包任务子模块：`staging`。

use std::collections::HashSet;
use std::io::{Read, Seek};
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::config::operations;
use crate::db::repository::file_repo;
use crate::entities::file;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::services::task_service::TaskStepInfo;
use crate::services::workspace_storage_service::{self, WorkspaceStorageScope};
use crate::storage::PolicySnapshot;

use super::super::super::TaskLeaseGuard;
use super::super::super::steps::{
    TASK_STEP_EXTRACT_ARCHIVE, set_task_step_active, set_task_step_succeeded,
};
use super::super::common::copy_reader_to_writer_with_lease_and_expected_size;

const UNIX_FILE_TYPE_MASK: u32 = 0o170000;
const UNIX_REGULAR_FILE_MODE: u32 = 0o100000;
const UNIX_DIRECTORY_MODE: u32 = 0o040000;

#[derive(Debug)]
pub(super) struct StagedArchiveStats {
    pub(super) total_bytes: i64,
    pub(super) total_progress: i64,
    pub(super) file_count: i64,
    pub(super) directory_count: i64,
}

#[derive(Debug)]
struct ArchivePreflightStats {
    total_uncompressed_bytes: i64,
    directory_count: u64,
    entries: Vec<ArchivePreflightEntry>,
}

#[derive(Debug)]
struct ArchivePreflightEntry {
    index: usize,
    relative_path: PathBuf,
    is_dir: bool,
    declared_size: i64,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ArchiveExtractPolicyResolver {
    Personal { user_id: i64 },
    Team { policy_group_id: i64 },
}

impl ArchiveExtractPolicyResolver {
    fn ensure_entry_size_allowed(
        self,
        policy_snapshot: &PolicySnapshot,
        entry_size: i64,
    ) -> Result<()> {
        let policy = match self {
            Self::Personal { user_id } => {
                policy_snapshot.resolve_user_policy_for_size(user_id, entry_size)?
            }
            Self::Team { policy_group_id } => {
                policy_snapshot.resolve_policy_in_group(policy_group_id, entry_size)?
            }
        };
        if policy.max_file_size > 0 && entry_size > policy.max_file_size {
            return Err(AsterError::file_too_large(format!(
                "archive entry size {} exceeds limit {}",
                entry_size, policy.max_file_size
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ArchiveExtractStageOptions {
    pub(super) scope: WorkspaceStorageScope,
    pub(super) policy_resolver: ArchiveExtractPolicyResolver,
    pub(super) source_archive_size: i64,
    pub(super) max_staging_bytes: i64,
    pub(super) limits: ArchiveExtractLimits,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ArchiveExtractLimits {
    pub(super) max_source_bytes: i64,
    pub(super) max_uncompressed_bytes: i64,
    pub(super) max_entries: u64,
    pub(super) max_files: u64,
    pub(super) max_directories: u64,
    pub(super) max_depth: u64,
    pub(super) max_path_bytes: u64,
    pub(super) max_compression_ratio: u64,
    pub(super) max_entry_compression_ratio: u64,
    pub(super) max_duration_secs: u64,
}

impl ArchiveExtractLimits {
    pub(super) fn from_runtime_config(runtime_config: &crate::config::RuntimeConfig) -> Self {
        Self {
            max_source_bytes: operations::archive_extract_max_source_bytes(runtime_config),
            max_uncompressed_bytes: operations::archive_extract_max_uncompressed_bytes(
                runtime_config,
            ),
            max_entries: operations::archive_extract_max_entries(runtime_config),
            max_files: operations::archive_extract_max_files(runtime_config),
            max_directories: operations::archive_extract_max_directories(runtime_config),
            max_depth: operations::archive_extract_max_depth(runtime_config),
            max_path_bytes: operations::archive_extract_max_path_bytes(runtime_config),
            max_compression_ratio: operations::archive_extract_max_compression_ratio(
                runtime_config,
            ),
            max_entry_compression_ratio: operations::archive_extract_max_entry_compression_ratio(
                runtime_config,
            ),
            max_duration_secs: operations::archive_extract_max_duration_secs(runtime_config),
        }
    }

    fn deadline(self) -> Option<Instant> {
        Instant::now().checked_add(std::time::Duration::from_secs(self.max_duration_secs))
    }
}

#[derive(Clone, Copy)]
pub(super) struct StageZipArchiveForExtractParams<'a> {
    pub(super) handle: &'a tokio::runtime::Handle,
    pub(super) db: &'a sea_orm::DatabaseConnection,
    pub(super) policy_snapshot: &'a PolicySnapshot,
    pub(super) lease_guard: &'a TaskLeaseGuard,
    pub(super) archive_path: &'a str,
    pub(super) stage_root: &'a str,
    pub(super) options: ArchiveExtractStageOptions,
}

pub(super) async fn download_file_to_temp(
    state: &PrimaryAppState,
    source_file: &file::Model,
    temp_path: &Path,
) -> Result<()> {
    let blob = file_repo::find_blob_by_id(&state.db, source_file.blob_id).await?;
    let policy = state.policy_snapshot.get_policy_or_err(blob.policy_id)?;
    let driver = state.driver_registry.get_driver(&policy)?;
    let mut stream = driver.get_stream(&blob.storage_path).await?;
    let mut output = tokio::fs::File::create(temp_path).await.map_aster_err_ctx(
        "create source archive temp file",
        AsterError::storage_driver_error,
    )?;
    copy_async_reader_to_writer_with_expected_size(
        &mut stream,
        &mut output,
        crate::utils::numbers::i64_to_u64(source_file.size, "source archive size")?,
        "source archive",
    )
    .await?;
    output.flush().await.map_aster_err_ctx(
        "flush source archive temp file",
        AsterError::storage_driver_error,
    )?;
    Ok(())
}

pub(super) fn stage_zip_archive_for_extract(
    params: StageZipArchiveForExtractParams<'_>,
    steps: &mut [TaskStepInfo],
) -> Result<StagedArchiveStats> {
    let StageZipArchiveForExtractParams {
        handle,
        db,
        policy_snapshot,
        lease_guard,
        archive_path,
        stage_root,
        options,
    } = params;
    let file = std::fs::File::open(archive_path)
        .map_aster_err_ctx("open source archive", AsterError::storage_driver_error)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_aster_err_with(|| AsterError::validation_error("invalid zip archive"))?;
    let deadline = options.limits.deadline();
    set_task_step_active(
        steps,
        TASK_STEP_EXTRACT_ARCHIVE,
        Some("Reading archive"),
        None,
    )?;
    handle.block_on(async {
        super::super::super::update_task_progress_db(
            db,
            lease_guard,
            0,
            0,
            Some("Reading archive"),
            steps,
        )
        .await
    })?;
    let preflight =
        inspect_zip_archive_for_extract(&mut archive, options, policy_snapshot, deadline)?;
    let total_bytes = preflight.total_uncompressed_bytes;
    let total_staging_bytes = options
        .source_archive_size
        .checked_add(total_bytes)
        .ok_or_else(|| AsterError::internal_error("archive extract staging size overflow"))?;
    if total_staging_bytes > options.max_staging_bytes {
        return Err(AsterError::validation_error(format!(
            "archive extract staging requires {} bytes (source {} + extracted {}), exceeds server limit {}",
            total_staging_bytes,
            options.source_archive_size,
            total_bytes,
            options.max_staging_bytes
        )));
    }
    if total_bytes > 0 {
        handle.block_on(async {
            workspace_storage_service::check_quota(db, options.scope, total_bytes).await
        })?;
    }
    let total_progress = total_bytes
        .checked_mul(2)
        .ok_or_else(|| AsterError::internal_error("archive extract progress overflow"))?;
    set_task_step_active(
        steps,
        TASK_STEP_EXTRACT_ARCHIVE,
        Some("Reading archive"),
        Some((0, total_bytes)),
    )?;
    handle.block_on(async {
        super::super::super::update_task_progress_db(
            db,
            lease_guard,
            0,
            total_progress,
            Some("Reading archive"),
            steps,
        )
        .await
    })?;

    let mut processed_bytes = 0_i64;
    let mut file_count = 0_i64;

    let preflight_entry_count = preflight.entries.len();
    if preflight_entry_count != archive.len() {
        return Err(AsterError::internal_error(format!(
            "archive preflight entry count {} differs from archive entry count {}",
            preflight_entry_count,
            archive.len()
        )));
    }

    for manifest_entry in &preflight.entries {
        lease_guard.ensure_active()?;
        ensure_archive_extract_deadline(deadline)?;
        let mut entry = archive
            .by_index(manifest_entry.index)
            .map_aster_err_with(|| AsterError::validation_error("invalid zip archive entry"))?;
        ensure_archive_entry_matches_preflight(&entry, manifest_entry)?;
        let relative_path = &manifest_entry.relative_path;
        let target_path = Path::new(stage_root).join(&relative_path);
        if manifest_entry.is_dir {
            std::fs::create_dir_all(&target_path).map_aster_err_ctx(
                "create extracted directory",
                AsterError::storage_driver_error,
            )?;
            continue;
        }

        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent).map_aster_err_ctx(
                "create extracted parent directory",
                AsterError::storage_driver_error,
            )?;
        }

        let mut output = std::fs::File::create(&target_path)
            .map_aster_err_ctx("create extracted file", AsterError::storage_driver_error)?;
        let entry_context = format!("archive entry '{}'", relative_path.display());
        let copied = copy_reader_to_writer_with_lease_and_expected_size(
            Some(lease_guard),
            &mut entry,
            &mut output,
            crate::utils::numbers::i64_to_u64(manifest_entry.declared_size, "archive entry size")?,
            &entry_context,
            deadline,
        )?;
        processed_bytes = processed_bytes
            .checked_add(crate::utils::numbers::u64_to_i64(
                copied,
                "extracted bytes",
            )?)
            .ok_or_else(|| AsterError::internal_error("archive extract progress overflow"))?;
        if processed_bytes > total_bytes {
            return Err(AsterError::validation_error(format!(
                "archive extracted {} bytes, exceeds preflight total {}",
                processed_bytes, total_bytes
            )));
        }
        file_count += 1;
        if file_count
            > crate::utils::numbers::u64_to_i64(options.limits.max_files, "archive max file count")?
        {
            return Err(AsterError::validation_error(format!(
                "archive extracted {} files, exceeds preflight limit {}",
                file_count, options.limits.max_files
            )));
        }

        let status_text = format!("Extracting {}", relative_path.to_string_lossy());
        set_task_step_active(
            steps,
            TASK_STEP_EXTRACT_ARCHIVE,
            Some(&status_text),
            Some((processed_bytes, total_bytes)),
        )?;
        handle.block_on(async {
            super::super::super::update_task_progress_db(
                db,
                lease_guard,
                processed_bytes,
                total_progress,
                Some(&status_text),
                steps,
            )
            .await
        })?;
    }

    set_task_step_succeeded(
        steps,
        TASK_STEP_EXTRACT_ARCHIVE,
        Some("Archive extracted to staging"),
        Some((total_bytes, total_bytes)),
    )?;

    Ok(StagedArchiveStats {
        total_bytes,
        total_progress,
        file_count,
        directory_count: crate::utils::numbers::u64_to_i64(
            preflight.directory_count,
            "archive directory count",
        )?,
    })
}

fn inspect_zip_archive_for_extract<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    options: ArchiveExtractStageOptions,
    policy_snapshot: &PolicySnapshot,
    deadline: Option<Instant>,
) -> Result<ArchivePreflightStats> {
    let limits = options.limits;
    let entry_count = crate::utils::numbers::usize_to_u64(archive.len(), "archive entry count")?;
    if entry_count > limits.max_entries {
        return Err(AsterError::validation_error(format!(
            "archive contains {} entries, exceeds server limit {}",
            entry_count, limits.max_entries
        )));
    }

    let mut total_uncompressed_bytes = 0_i64;
    let mut total_compressed_bytes = 0_u64;
    let mut file_count = 0_u64;
    let mut seen_paths = HashSet::new();
    let mut directory_paths = HashSet::new();
    let mut file_paths = HashSet::new();
    let mut entries = Vec::with_capacity(archive.len());

    for index in 0..archive.len() {
        ensure_archive_extract_deadline(deadline)?;
        let entry = archive
            .by_index_raw(index)
            .map_aster_err_with(|| AsterError::validation_error("invalid zip archive entry"))?;
        validate_zip_entry_supported(&entry)?;
        let enclosed_path = entry.enclosed_name().ok_or_else(|| {
            AsterError::validation_error(format!(
                "archive entry '{}' contains unsafe path",
                entry.name()
            ))
        })?;
        let relative_path = normalize_archive_entry_path(&enclosed_path)?;
        validate_archive_entry_path_limits(&relative_path, limits)?;
        ensure_archive_entry_path_not_conflicting(
            &relative_path,
            entry.is_dir(),
            &mut seen_paths,
            &directory_paths,
            &file_paths,
        )?;

        if entry.is_dir() {
            insert_directory_path_with_limit(&relative_path, &mut directory_paths, limits)?;
            entries.push(ArchivePreflightEntry {
                index,
                relative_path,
                is_dir: true,
                declared_size: 0,
            });
            continue;
        }

        if let Some(parent) = relative_path.parent() {
            insert_directory_path_with_limit(parent, &mut directory_paths, limits)?;
        }
        file_count = file_count
            .checked_add(1)
            .ok_or_else(|| AsterError::internal_error("archive file count overflow"))?;
        if file_count > limits.max_files {
            return Err(AsterError::validation_error(format!(
                "archive contains {} files, exceeds server limit {}",
                file_count, limits.max_files
            )));
        }

        let entry_size = crate::utils::numbers::u64_to_i64(entry.size(), "archive entry size")?;
        options
            .policy_resolver
            .ensure_entry_size_allowed(policy_snapshot, entry_size)?;
        validate_archive_entry_compression_ratio(
            entry.size(),
            entry.compressed_size(),
            limits.max_entry_compression_ratio,
            &relative_path,
        )?;
        total_uncompressed_bytes = total_uncompressed_bytes
            .checked_add(entry_size)
            .ok_or_else(|| AsterError::internal_error("archive extract size overflow"))?;
        if total_uncompressed_bytes > limits.max_uncompressed_bytes {
            return Err(AsterError::validation_error(format!(
                "archive uncompressed size {} exceeds server limit {}",
                total_uncompressed_bytes, limits.max_uncompressed_bytes
            )));
        }
        total_compressed_bytes = total_compressed_bytes
            .checked_add(entry.compressed_size())
            .ok_or_else(|| AsterError::internal_error("archive compressed size overflow"))?;
        file_paths.insert(relative_path.clone());
        entries.push(ArchivePreflightEntry {
            index,
            relative_path,
            is_dir: false,
            declared_size: entry_size,
        });
    }

    validate_total_archive_compression_ratio(
        total_uncompressed_bytes,
        total_compressed_bytes,
        limits.max_compression_ratio,
    )?;

    Ok(ArchivePreflightStats {
        total_uncompressed_bytes,
        directory_count: directory_paths.len().try_into().map_aster_err_with(|| {
            AsterError::internal_error("directory count exceeds u64 range")
        })?,
        entries,
    })
}

fn ensure_archive_extract_deadline(deadline: Option<Instant>) -> Result<()> {
    if let Some(deadline) = deadline
        && Instant::now() > deadline
    {
        return Err(AsterError::validation_error(
            "archive extraction exceeded server time limit",
        ));
    }
    Ok(())
}

fn ensure_archive_entry_matches_preflight<R: Read>(
    entry: &zip::read::ZipFile<'_, R>,
    manifest_entry: &ArchivePreflightEntry,
) -> Result<()> {
    let is_dir = entry.is_dir();
    if is_dir != manifest_entry.is_dir {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' type changed after preflight",
            entry.name()
        )));
    }
    if !is_dir {
        let declared_size = crate::utils::numbers::u64_to_i64(entry.size(), "archive entry size")?;
        if declared_size != manifest_entry.declared_size {
            return Err(AsterError::validation_error(format!(
                "archive entry '{}' declared size changed after preflight",
                entry.name()
            )));
        }
    }
    Ok(())
}

fn validate_zip_entry_supported<R: Read>(entry: &zip::read::ZipFile<'_, R>) -> Result<()> {
    if entry.encrypted() {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' is encrypted; encrypted ZIP entries are not supported",
            entry.name()
        )));
    }
    if entry.is_symlink() {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' is a symbolic link; symbolic links are not supported",
            entry.name()
        )));
    }
    if let Some(mode) = entry.unix_mode() {
        let file_type = mode & UNIX_FILE_TYPE_MASK;
        if file_type != 0 && file_type != UNIX_REGULAR_FILE_MODE && file_type != UNIX_DIRECTORY_MODE
        {
            return Err(AsterError::validation_error(format!(
                "archive entry '{}' is a special file; only regular files and directories are supported",
                entry.name()
            )));
        }
    }
    if !entry.is_file() && !entry.is_dir() {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' is not a regular file or directory",
            entry.name()
        )));
    }
    match entry.compression() {
        zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated => Ok(()),
        method => Err(AsterError::validation_error(format!(
            "archive entry '{}' uses unsupported compression method {method:?}",
            entry.name()
        ))),
    }
}

fn validate_archive_entry_path_limits(
    relative_path: &Path,
    limits: ArchiveExtractLimits,
) -> Result<()> {
    let depth = crate::utils::numbers::usize_to_u64(
        relative_path.components().count(),
        "archive entry path depth",
    )?;
    if depth > limits.max_depth {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' depth {} exceeds server limit {}",
            relative_path.display(),
            depth,
            limits.max_depth
        )));
    }

    let path_bytes = crate::utils::numbers::usize_to_u64(
        relative_path.to_string_lossy().len(),
        "archive entry path bytes",
    )?;
    if path_bytes > limits.max_path_bytes {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' path length {} exceeds server limit {}",
            relative_path.display(),
            path_bytes,
            limits.max_path_bytes
        )));
    }

    Ok(())
}

fn ensure_archive_entry_path_not_conflicting(
    relative_path: &Path,
    is_dir: bool,
    seen_paths: &mut HashSet<PathBuf>,
    directory_paths: &HashSet<PathBuf>,
    file_paths: &HashSet<PathBuf>,
) -> Result<()> {
    if !seen_paths.insert(relative_path.to_path_buf()) {
        return Err(AsterError::validation_error(format!(
            "archive contains duplicate entry path '{}'",
            relative_path.display()
        )));
    }

    if is_dir {
        for ancestor in relative_path.ancestors().skip(1) {
            if ancestor.as_os_str().is_empty() {
                break;
            }
            if file_paths.contains(ancestor) {
                return Err(AsterError::validation_error(format!(
                    "archive directory '{}' is inside file entry '{}'",
                    relative_path.display(),
                    ancestor.display()
                )));
            }
        }
        for file_path in file_paths {
            if file_path != relative_path && file_path.starts_with(relative_path) {
                return Err(AsterError::validation_error(format!(
                    "archive directory '{}' conflicts with file '{}'",
                    relative_path.display(),
                    file_path.display()
                )));
            }
        }
        return Ok(());
    }

    if directory_paths.contains(relative_path) {
        return Err(AsterError::validation_error(format!(
            "archive file '{}' conflicts with a directory of the same path",
            relative_path.display()
        )));
    }
    for ancestor in relative_path.ancestors().skip(1) {
        if ancestor.as_os_str().is_empty() {
            break;
        }
        if file_paths.contains(ancestor) {
            return Err(AsterError::validation_error(format!(
                "archive file '{}' is inside file entry '{}'",
                relative_path.display(),
                ancestor.display()
            )));
        }
    }

    Ok(())
}

fn insert_directory_path_with_limit(
    path: &Path,
    directory_paths: &mut HashSet<PathBuf>,
    limits: ArchiveExtractLimits,
) -> Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        if let Component::Normal(name) = component {
            current.push(name);
            if directory_paths.insert(current.clone()) {
                let count = crate::utils::numbers::usize_to_u64(
                    directory_paths.len(),
                    "archive directory count",
                )?;
                if count > limits.max_directories {
                    return Err(AsterError::validation_error(format!(
                        "archive contains {} directories, exceeds server limit {}",
                        count, limits.max_directories
                    )));
                }
            }
        }
    }
    Ok(())
}

fn validate_archive_entry_compression_ratio(
    uncompressed_size: u64,
    compressed_size: u64,
    max_ratio: u64,
    relative_path: &Path,
) -> Result<()> {
    if uncompressed_size == 0 {
        return Ok(());
    }
    if compressed_size == 0 {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' has zero compressed bytes for {} uncompressed bytes",
            relative_path.display(),
            uncompressed_size
        )));
    }
    if compression_ratio_exceeds(uncompressed_size, compressed_size, max_ratio)? {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' compression ratio exceeds server limit {}",
            relative_path.display(),
            max_ratio
        )));
    }
    Ok(())
}

fn validate_total_archive_compression_ratio(
    total_uncompressed_bytes: i64,
    total_compressed_bytes: u64,
    max_ratio: u64,
) -> Result<()> {
    if total_uncompressed_bytes <= 0 {
        return Ok(());
    }
    if total_compressed_bytes == 0 {
        return Err(AsterError::validation_error(
            "archive has zero compressed file bytes for non-empty contents",
        ));
    }
    let total_uncompressed_bytes =
        crate::utils::numbers::i64_to_u64(total_uncompressed_bytes, "archive uncompressed size")?;
    if compression_ratio_exceeds(total_uncompressed_bytes, total_compressed_bytes, max_ratio)? {
        return Err(AsterError::validation_error(format!(
            "archive total compression ratio exceeds server limit {}",
            max_ratio
        )));
    }
    Ok(())
}

fn compression_ratio_exceeds(
    uncompressed_size: u64,
    compressed_size: u64,
    max_ratio: u64,
) -> Result<bool> {
    let allowed = u128::from(compressed_size)
        .checked_mul(u128::from(max_ratio))
        .ok_or_else(|| AsterError::internal_error("archive compression ratio overflow"))?;
    Ok(u128::from(uncompressed_size) > allowed)
}

async fn copy_async_reader_to_writer_with_expected_size<R, W>(
    reader: &mut R,
    writer: &mut W,
    expected_bytes: u64,
    context: &str,
) -> Result<u64>
where
    R: AsyncRead + Unpin + ?Sized,
    W: AsyncWrite + Unpin,
{
    let mut copied = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let read = reader.read(&mut buffer).await.map_aster_err_ctx(
            "read bounded archive stream chunk",
            AsterError::storage_driver_error,
        )?;
        if read == 0 {
            break;
        }

        let read_u64 = crate::utils::numbers::usize_to_u64(read, "archive stream chunk size")?;
        let next_copied = copied
            .checked_add(read_u64)
            .ok_or_else(|| AsterError::internal_error("archive stream byte counter overflow"))?;
        if next_copied > expected_bytes {
            return Err(AsterError::validation_error(format!(
                "{context} expands beyond declared size: declared {expected_bytes} bytes"
            )));
        }

        writer.write_all(&buffer[..read]).await.map_aster_err_ctx(
            "write bounded archive stream chunk",
            AsterError::storage_driver_error,
        )?;
        copied = next_copied;
    }

    if copied != expected_bytes {
        return Err(AsterError::validation_error(format!(
            "{context} size mismatch: declared {expected_bytes} bytes, downloaded {copied} bytes"
        )));
    }

    Ok(copied)
}

fn normalize_archive_entry_path(path: &Path) -> Result<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => {
                let name = name.to_str().ok_or_else(|| {
                    AsterError::validation_error("archive entry name must be valid UTF-8")
                })?;
                crate::utils::validate_name(name)?;
                normalized.push(name);
            }
            _ => {
                return Err(AsterError::validation_error(format!(
                    "archive entry '{}' contains invalid path component",
                    path.display()
                )));
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        return Err(AsterError::validation_error(
            "archive entry path cannot be empty",
        ));
    }
    Ok(normalized)
}
