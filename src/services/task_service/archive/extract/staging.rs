//! 归档解包任务子模块：`staging`。

use std::io::Read;
use std::path::Path;
use std::time::Instant;

use tokio::io::AsyncWriteExt;

use crate::config::operations;
use crate::db::repository::file_repo;
use crate::entities::file;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::services::{
    archive_service::{
        io::copy_async_reader_to_writer_with_expected_size,
        scan::{
            ArchiveScanEntry, ArchiveScanLimits, ArchiveScanNamePolicy,
            ensure_archive_scan_deadline,
        },
        zip_scan::scan_zip_archive,
    },
    task_service::{
        TaskLeaseGuard, TaskStepInfo,
        archive::common::copy_reader_to_writer_with_lease_and_expected_size,
        steps::{TASK_STEP_EXTRACT_ARCHIVE, set_task_step_active, set_task_step_succeeded},
        update_task_progress_db,
    },
    workspace_storage_service::{self, WorkspaceStorageScope},
};
use crate::storage::PolicySnapshot;
use crate::types::ArchiveFilenameEncoding;

#[derive(Debug)]
pub(super) struct StagedArchiveStats {
    pub(super) total_bytes: i64,
    pub(super) total_progress: i64,
    pub(super) file_count: i64,
    pub(super) directory_count: i64,
}

#[derive(Debug)]
struct ArchiveStagingProgress {
    total_bytes: i64,
    total_progress: i64,
    processed_bytes: i64,
    file_count: i64,
}

#[derive(Debug, Clone, Copy)]
struct ArchiveStagingRuntime<'a> {
    handle: &'a tokio::runtime::Handle,
    db: &'a sea_orm::DatabaseConnection,
    lease_guard: &'a TaskLeaseGuard,
}

struct ArchiveStagingProgressSink<'a, 'b> {
    runtime: ArchiveStagingRuntime<'a>,
    steps: &'b mut [TaskStepInfo],
    progress: &'b mut ArchiveStagingProgress,
    max_files: u64,
}

impl ArchiveStagingProgressSink<'_, '_> {
    fn record_file(&mut self, relative_path: &Path, copied: u64) -> Result<()> {
        self.progress.processed_bytes = self
            .progress
            .processed_bytes
            .checked_add(crate::utils::numbers::u64_to_i64(
                copied,
                "extracted bytes",
            )?)
            .ok_or_else(|| AsterError::internal_error("archive extract progress overflow"))?;
        if self.progress.processed_bytes > self.progress.total_bytes {
            return Err(AsterError::validation_error(format!(
                "archive extracted {} bytes, exceeds preflight total {}",
                self.progress.processed_bytes, self.progress.total_bytes
            )));
        }
        self.progress.file_count += 1;
        if self.progress.file_count
            > crate::utils::numbers::u64_to_i64(self.max_files, "archive max file count")?
        {
            return Err(AsterError::validation_error(format!(
                "archive extracted {} files, exceeds preflight limit {}",
                self.progress.file_count, self.max_files
            )));
        }

        let status_text = format!("Extracting {}", relative_path.to_string_lossy());
        set_task_step_active(
            self.steps,
            TASK_STEP_EXTRACT_ARCHIVE,
            Some(&status_text),
            Some((self.progress.processed_bytes, self.progress.total_bytes)),
        )?;
        self.runtime.handle.block_on(async {
            update_task_progress_db(
                self.runtime.db,
                self.runtime.lease_guard,
                self.progress.processed_bytes,
                self.progress.total_progress,
                Some(&status_text),
                self.steps,
            )
            .await
        })
    }
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
    pub(super) filename_encoding: ArchiveFilenameEncoding,
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

    fn scan_limits(self) -> ArchiveScanLimits {
        ArchiveScanLimits {
            max_uncompressed_bytes: self.max_uncompressed_bytes,
            max_entries: self.max_entries,
            max_files: self.max_files,
            max_directories: self.max_directories,
            max_depth: self.max_depth,
            max_path_bytes: self.max_path_bytes,
            max_compression_ratio: self.max_compression_ratio,
            max_entry_compression_ratio: self.max_entry_compression_ratio,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct StageArchiveForExtractParams<'a> {
    pub(super) handle: &'a tokio::runtime::Handle,
    pub(super) db: &'a sea_orm::DatabaseConnection,
    pub(super) policy_snapshot: &'a PolicySnapshot,
    pub(super) lease_guard: &'a TaskLeaseGuard,
    pub(super) archive_path: &'a Path,
    pub(super) stage_root: &'a Path,
    pub(super) options: ArchiveExtractStageOptions,
}

pub(super) async fn download_file_to_temp(
    state: &PrimaryAppState,
    source_file: &file::Model,
    temp_path: &Path,
) -> Result<()> {
    let blob = file_repo::find_blob_by_id(state.writer_db(), source_file.blob_id).await?;
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
        AsterError::validation_error,
    )
    .await?;
    output.flush().await.map_aster_err_ctx(
        "flush source archive temp file",
        AsterError::storage_driver_error,
    )?;
    Ok(())
}

pub(super) fn stage_zip_archive_for_extract(
    params: StageArchiveForExtractParams<'_>,
    steps: &mut [TaskStepInfo],
) -> Result<StagedArchiveStats> {
    let StageArchiveForExtractParams {
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
    set_archive_staging_reading_step(handle, db, lease_guard, steps, None, 0)?;
    let preflight = scan_zip_archive(
        &mut archive,
        options.limits.scan_limits(),
        deadline,
        options.filename_encoding,
        ArchiveScanNamePolicy::StrictAsterName,
        |entry_size| {
            options
                .policy_resolver
                .ensure_entry_size_allowed(policy_snapshot, entry_size)
        },
    )?;
    let mut progress = prepare_archive_staging_after_preflight(
        handle,
        db,
        lease_guard,
        steps,
        options,
        preflight.total_uncompressed_bytes,
    )?;

    let preflight_entry_count = preflight.entries.len();
    if preflight_entry_count != archive.len() {
        return Err(AsterError::internal_error(format!(
            "archive preflight entry count {} differs from archive entry count {}",
            preflight_entry_count,
            archive.len()
        )));
    }

    {
        let runtime = ArchiveStagingRuntime {
            handle,
            db,
            lease_guard,
        };
        let mut progress_sink = ArchiveStagingProgressSink {
            runtime,
            steps,
            progress: &mut progress,
            max_files: options.limits.max_files,
        };

        for manifest_entry in &preflight.entries {
            lease_guard.ensure_active()?;
            ensure_archive_scan_deadline(deadline)?;
            let mut entry = archive
                .by_index(manifest_entry.index)
                .map_aster_err_with(|| AsterError::validation_error("invalid zip archive entry"))?;
            ensure_archive_entry_matches_preflight(&entry, manifest_entry)?;
            let relative_path = &manifest_entry.relative_path;
            if manifest_entry.kind.is_dir() {
                create_archive_stage_output(stage_root, manifest_entry)?;
                continue;
            }

            let Some(mut output) = create_archive_stage_output(stage_root, manifest_entry)? else {
                return Err(AsterError::validation_error("invalid zip archive entry"));
            };
            let entry_context = format!("archive entry '{}'", relative_path.display());
            let copied = copy_reader_to_writer_with_lease_and_expected_size(
                Some(lease_guard),
                &mut entry,
                &mut output,
                crate::utils::numbers::i64_to_u64(manifest_entry.size, "archive entry size")?,
                &entry_context,
                deadline,
            )?;
            progress_sink.record_file(relative_path, copied)?;
        }
    }

    set_task_step_succeeded(
        steps,
        TASK_STEP_EXTRACT_ARCHIVE,
        Some("Archive extracted to staging"),
        Some((progress.total_bytes, progress.total_bytes)),
    )?;

    Ok(StagedArchiveStats {
        total_bytes: progress.total_bytes,
        total_progress: progress.total_progress,
        file_count: progress.file_count,
        directory_count: crate::utils::numbers::u64_to_i64(
            preflight.directory_count,
            "archive directory count",
        )?,
    })
}

fn set_archive_staging_reading_step(
    handle: &tokio::runtime::Handle,
    db: &sea_orm::DatabaseConnection,
    lease_guard: &TaskLeaseGuard,
    steps: &mut [TaskStepInfo],
    step_progress: Option<(i64, i64)>,
    total_progress: i64,
) -> Result<()> {
    set_task_step_active(
        steps,
        TASK_STEP_EXTRACT_ARCHIVE,
        Some("Reading archive"),
        step_progress,
    )?;
    handle.block_on(async {
        update_task_progress_db(
            db,
            lease_guard,
            0,
            total_progress,
            Some("Reading archive"),
            steps,
        )
        .await
    })
}

fn prepare_archive_staging_after_preflight(
    handle: &tokio::runtime::Handle,
    db: &sea_orm::DatabaseConnection,
    lease_guard: &TaskLeaseGuard,
    steps: &mut [TaskStepInfo],
    options: ArchiveExtractStageOptions,
    total_bytes: i64,
) -> Result<ArchiveStagingProgress> {
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
    set_archive_staging_reading_step(
        handle,
        db,
        lease_guard,
        steps,
        Some((0, total_bytes)),
        total_progress,
    )?;

    Ok(ArchiveStagingProgress {
        total_bytes,
        total_progress,
        processed_bytes: 0,
        file_count: 0,
    })
}

fn create_archive_stage_output(
    stage_root: &Path,
    manifest_entry: &ArchiveScanEntry,
) -> Result<Option<std::fs::File>> {
    let target_path = Path::new(stage_root).join(&manifest_entry.relative_path);
    if manifest_entry.kind.is_dir() {
        std::fs::create_dir_all(&target_path).map_aster_err_ctx(
            "create extracted directory",
            AsterError::storage_driver_error,
        )?;
        return Ok(None);
    }

    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent).map_aster_err_ctx(
            "create extracted parent directory",
            AsterError::storage_driver_error,
        )?;
    }

    std::fs::File::create(&target_path)
        .map(Some)
        .map_aster_err_ctx("create extracted file", AsterError::storage_driver_error)
}

fn ensure_archive_entry_matches_preflight<R: Read>(
    entry: &zip::read::ZipFile<'_, R>,
    manifest_entry: &ArchiveScanEntry,
) -> Result<()> {
    let is_dir = entry.is_dir();
    if is_dir != manifest_entry.kind.is_dir() {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' type changed after preflight",
            entry.name()
        )));
    }
    if !is_dir {
        let declared_size = crate::utils::numbers::u64_to_i64(entry.size(), "archive entry size")?;
        if declared_size != manifest_entry.size {
            return Err(AsterError::validation_error(format!(
                "archive entry '{}' declared size changed after preflight",
                entry.name()
            )));
        }
    }
    Ok(())
}
