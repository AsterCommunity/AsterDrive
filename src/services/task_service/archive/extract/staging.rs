//! 归档解包任务子模块：`staging`。

use std::io::{Read, Seek, Write};
use std::path::Path;
use std::time::Instant;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use zesven::format::parser::ArchiveHeader as SevenZipArchiveHeader;
use zesven::format::streams::SubStreamsInfo as SevenZipSubStreamsInfo;
use zesven::streaming::SolidBlockStreamReader;

use crate::config::operations;
use crate::db::repository::file_repo;
use crate::entities::file;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::services::archive_service::scan::{
    ArchiveScanEntry, ArchiveScanLimits, ArchiveScanNamePolicy, ensure_archive_scan_deadline,
};
use crate::services::archive_service::seven_zip_scan::{
    map_seven_zip_entry_error, open_seven_zip_streaming_archive, scan_seven_zip_archive,
    seven_zip_streaming_config,
};
use crate::services::archive_service::zip_scan::scan_zip_archive;
use crate::services::task_service::TaskStepInfo;
use crate::services::workspace_storage_service::{self, WorkspaceStorageScope};
use crate::storage::PolicySnapshot;
use crate::types::ArchiveFilenameEncoding;

use super::super::super::TaskLeaseGuard;
use super::super::super::steps::{
    TASK_STEP_EXTRACT_ARCHIVE, set_task_step_active, set_task_step_succeeded,
};
use super::super::common::copy_reader_to_writer_with_lease_and_expected_size;

#[derive(Debug)]
pub(super) struct StagedArchiveStats {
    pub(super) total_bytes: i64,
    pub(super) total_progress: i64,
    pub(super) file_count: i64,
    pub(super) directory_count: i64,
}

#[derive(Debug, Clone, Copy)]
struct SevenZipStreamPosition {
    folder_index: usize,
    stream_index: usize,
}

#[derive(Debug, Clone, Copy)]
struct SevenZipFileWork<'a> {
    manifest_entry: &'a ArchiveScanEntry,
    stream_position: Option<SevenZipStreamPosition>,
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
        ensure_archive_scan_deadline(deadline)?;
        let mut entry = archive
            .by_index(manifest_entry.index)
            .map_aster_err_with(|| AsterError::validation_error("invalid zip archive entry"))?;
        ensure_archive_entry_matches_preflight(&entry, manifest_entry)?;
        let relative_path = &manifest_entry.relative_path;
        let target_path = Path::new(stage_root).join(relative_path);
        if manifest_entry.kind.is_dir() {
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
            crate::utils::numbers::i64_to_u64(manifest_entry.size, "archive entry size")?,
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

pub(super) fn stage_seven_zip_archive_for_extract(
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
    let scan_file = std::fs::File::open(archive_path)
        .map_aster_err_ctx("open source archive", AsterError::storage_driver_error)?;
    let scan_limits = options.limits.scan_limits();
    let scan_archive = open_seven_zip_streaming_archive(scan_file, scan_limits)?;
    let source_archive_size =
        crate::utils::numbers::i64_to_u64(options.source_archive_size, "source archive size")?;
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
    let preflight = scan_seven_zip_archive(
        &scan_archive,
        scan_limits,
        source_archive_size,
        deadline,
        ArchiveScanNamePolicy::StrictAsterName,
        |entry_size| {
            options
                .policy_resolver
                .ensure_entry_size_allowed(policy_snapshot, entry_size)
        },
    )?;
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
    drop(scan_archive);

    let mut processed_bytes = 0_i64;
    let mut file_count = 0_i64;

    let scan_file = std::fs::File::open(archive_path)
        .map_aster_err_ctx("open source archive", AsterError::storage_driver_error)?;
    let archive = open_seven_zip_streaming_archive(scan_file, scan_limits)?;
    ensure_seven_zip_entry_count_matches_preflight(
        preflight.entries.len(),
        archive.entries_list().len(),
    )?;
    let stream_header = seven_zip_block_stream_header(archive.header())?;
    let stream_positions = seven_zip_entry_stream_positions(&stream_header)?;
    let file_work = seven_zip_file_work(
        &preflight.entries,
        archive.entries_list(),
        &stream_positions,
    )?;
    drop(archive);

    let mut source_file = std::fs::File::open(archive_path)
        .map_aster_err_ctx("open source archive", AsterError::storage_driver_error)?;
    extract_seven_zip_file_work(
        &stream_header,
        &mut source_file,
        file_work,
        stage_root,
        handle,
        db,
        lease_guard,
        steps,
        &mut processed_bytes,
        &mut file_count,
        total_bytes,
        total_progress,
        options.limits.max_files,
        deadline,
        scan_limits,
    )?;

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

fn seven_zip_block_stream_header(header: &SevenZipArchiveHeader) -> Result<SevenZipArchiveHeader> {
    let mut header = header.clone();
    if header.substreams_info.is_some() {
        return Ok(header);
    }

    let Some(unpack_info) = header.unpack_info.as_ref() else {
        return Ok(header);
    };
    header.substreams_info = Some(SevenZipSubStreamsInfo {
        num_unpack_streams_in_folders: vec![1; unpack_info.folders.len()],
        unpack_sizes: unpack_info
            .folders
            .iter()
            .map(|folder| folder.final_unpack_size().unwrap_or(0))
            .collect(),
        digests: unpack_info
            .folders
            .iter()
            .map(|folder| folder.unpack_crc)
            .collect(),
    });
    Ok(header)
}

fn seven_zip_entry_stream_positions(
    header: &SevenZipArchiveHeader,
) -> Result<Vec<Option<SevenZipStreamPosition>>> {
    let Some(files_info) = header.files_info.as_ref() else {
        return Ok(Vec::new());
    };
    let substreams = header
        .substreams_info
        .as_ref()
        .ok_or_else(|| AsterError::validation_error("invalid 7z archive stream layout"))?;
    let mut positions = Vec::with_capacity(files_info.entries.len());
    let mut folder_index = 0_usize;
    let mut stream_index = 0_usize;

    for entry in &files_info.entries {
        if !entry.has_stream {
            positions.push(None);
            continue;
        }
        let streams_in_folder = substreams
            .num_unpack_streams_in_folders
            .get(folder_index)
            .copied()
            .ok_or_else(|| AsterError::validation_error("invalid 7z archive stream layout"))?;
        if streams_in_folder == 0 {
            return Err(AsterError::validation_error(
                "invalid 7z archive stream layout",
            ));
        }
        positions.push(Some(SevenZipStreamPosition {
            folder_index,
            stream_index,
        }));

        stream_index = stream_index
            .checked_add(1)
            .ok_or_else(|| AsterError::internal_error("7z stream index overflow"))?;
        if stream_index
            >= crate::utils::numbers::u64_to_usize(streams_in_folder, "7z folder stream count")?
        {
            stream_index = 0;
            folder_index = folder_index
                .checked_add(1)
                .ok_or_else(|| AsterError::internal_error("7z folder index overflow"))?;
        }
    }

    Ok(positions)
}

fn seven_zip_file_work<'a>(
    preflight_entries: &'a [ArchiveScanEntry],
    archive_entries: &[zesven::Entry],
    stream_positions: &[Option<SevenZipStreamPosition>],
) -> Result<Vec<SevenZipFileWork<'a>>> {
    let mut work = Vec::with_capacity(preflight_entries.len());
    for manifest_entry in preflight_entries {
        let entry = archive_entries
            .get(manifest_entry.index)
            .ok_or_else(|| AsterError::validation_error("invalid 7z archive entry"))?;
        ensure_seven_zip_entry_matches_preflight(entry, manifest_entry)?;
        let stream_position = stream_positions
            .get(manifest_entry.index)
            .copied()
            .ok_or_else(|| AsterError::validation_error("invalid 7z archive entry"))?;
        if !manifest_entry.kind.is_dir() && stream_position.is_none() && manifest_entry.size != 0 {
            return Err(AsterError::validation_error(
                "invalid 7z archive entry stream",
            ));
        }
        work.push(SevenZipFileWork {
            manifest_entry,
            stream_position,
        });
    }
    Ok(work)
}

#[allow(clippy::too_many_arguments)]
fn extract_seven_zip_file_work<R>(
    stream_header: &SevenZipArchiveHeader,
    source_file: &mut R,
    file_work: Vec<SevenZipFileWork<'_>>,
    stage_root: &Path,
    handle: &tokio::runtime::Handle,
    db: &sea_orm::DatabaseConnection,
    lease_guard: &TaskLeaseGuard,
    steps: &mut [TaskStepInfo],
    processed_bytes: &mut i64,
    file_count: &mut i64,
    total_bytes: i64,
    total_progress: i64,
    max_files: u64,
    deadline: Option<Instant>,
    scan_limits: ArchiveScanLimits,
) -> Result<()>
where
    R: Read + Seek + Send,
{
    let mut work_index = 0_usize;
    while work_index < file_work.len() {
        lease_guard.ensure_active()?;
        ensure_archive_scan_deadline(deadline)?;
        let work = file_work[work_index];

        if work.manifest_entry.kind.is_dir() {
            create_seven_zip_stage_output(stage_root, work.manifest_entry)?;
            work_index += 1;
            continue;
        }

        let Some(stream_position) = work.stream_position else {
            let copied = create_empty_seven_zip_stage_file(stage_root, work.manifest_entry)?;
            record_seven_zip_file_progress(
                handle,
                db,
                lease_guard,
                steps,
                &work.manifest_entry.relative_path,
                copied,
                processed_bytes,
                file_count,
                total_bytes,
                total_progress,
                max_files,
            )?;
            work_index += 1;
            continue;
        };

        let mut block_reader = SolidBlockStreamReader::new(
            stream_header,
            source_file,
            stream_position.folder_index,
            seven_zip_stream_reader_config(scan_limits)?,
        )
        .map_err(map_seven_zip_entry_error)?;

        while work_index < file_work.len() {
            lease_guard.ensure_active()?;
            ensure_archive_scan_deadline(deadline)?;
            let work = file_work[work_index];
            if work.manifest_entry.kind.is_dir() {
                create_seven_zip_stage_output(stage_root, work.manifest_entry)?;
                work_index += 1;
                continue;
            }
            let Some(position) = work.stream_position else {
                let copied = create_empty_seven_zip_stage_file(stage_root, work.manifest_entry)?;
                record_seven_zip_file_progress(
                    handle,
                    db,
                    lease_guard,
                    steps,
                    &work.manifest_entry.relative_path,
                    copied,
                    processed_bytes,
                    file_count,
                    total_bytes,
                    total_progress,
                    max_files,
                )?;
                work_index += 1;
                continue;
            };
            if position.folder_index != stream_position.folder_index {
                break;
            }
            extract_seven_zip_block_stream_entry(
                &mut block_reader,
                position,
                work.manifest_entry,
                stage_root,
                handle,
                db,
                lease_guard,
                steps,
                processed_bytes,
                file_count,
                total_bytes,
                total_progress,
                max_files,
                deadline,
            )?;
            work_index += 1;
        }
    }

    Ok(())
}

fn seven_zip_stream_reader_config(limits: ArchiveScanLimits) -> Result<zesven::StreamingConfig> {
    seven_zip_streaming_config(limits)
}

#[allow(clippy::too_many_arguments)]
fn extract_seven_zip_block_stream_entry<R>(
    block_reader: &mut SolidBlockStreamReader<'_, R>,
    stream_position: SevenZipStreamPosition,
    manifest_entry: &ArchiveScanEntry,
    stage_root: &Path,
    handle: &tokio::runtime::Handle,
    db: &sea_orm::DatabaseConnection,
    lease_guard: &TaskLeaseGuard,
    steps: &mut [TaskStepInfo],
    processed_bytes: &mut i64,
    file_count: &mut i64,
    total_bytes: i64,
    total_progress: i64,
    max_files: u64,
    deadline: Option<Instant>,
) -> Result<()>
where
    R: Read + Seek + Send,
{
    while block_reader.current_index() < stream_position.stream_index {
        block_reader
            .next_entry()
            .ok_or_else(|| AsterError::validation_error("invalid 7z archive stream"))?
            .map_err(map_seven_zip_entry_error)?;
        block_reader
            .skip_current_entry()
            .map_err(map_seven_zip_entry_error)?;
    }

    let (stream_index, declared_size) = block_reader
        .next_entry()
        .ok_or_else(|| AsterError::validation_error("invalid 7z archive stream"))?
        .map_err(map_seven_zip_entry_error)?;
    if stream_index != stream_position.stream_index {
        return Err(AsterError::validation_error(
            "invalid 7z archive stream order",
        ));
    }
    let expected_size =
        crate::utils::numbers::i64_to_u64(manifest_entry.size, "archive entry size")?;
    if declared_size != expected_size {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' declared size changed after preflight",
            manifest_entry.relative_path.display()
        )));
    }

    let Some(mut output) = create_seven_zip_stage_output(stage_root, manifest_entry)? else {
        return Err(AsterError::validation_error("invalid 7z archive entry"));
    };
    let entry_context = format!("archive entry '{}'", manifest_entry.relative_path.display());
    let copied = extract_current_seven_zip_block_entry_to_writer(
        block_reader,
        &mut output,
        lease_guard,
        expected_size,
        &entry_context,
        deadline,
    )?;
    block_reader
        .finish_entry()
        .map_err(map_seven_zip_entry_error)?;
    record_seven_zip_file_progress(
        handle,
        db,
        lease_guard,
        steps,
        &manifest_entry.relative_path,
        copied,
        processed_bytes,
        file_count,
        total_bytes,
        total_progress,
        max_files,
    )
}

fn extract_current_seven_zip_block_entry_to_writer<R, W>(
    block_reader: &mut SolidBlockStreamReader<'_, R>,
    writer: &mut W,
    lease_guard: &TaskLeaseGuard,
    expected_bytes: u64,
    context: &str,
    deadline: Option<Instant>,
) -> Result<u64>
where
    R: Read + Seek + Send,
    W: Write,
{
    let mut copied = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        lease_guard.ensure_active()?;
        ensure_archive_scan_deadline(deadline)?;
        let read = block_reader
            .read_entry_data(&mut buffer)
            .map_err(map_seven_zip_stream_read_error)?;
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
        writer.write_all(&buffer[..read]).map_aster_err_ctx(
            "write 7z archive stream chunk",
            AsterError::storage_driver_error,
        )?;
        copied = next_copied;
    }

    if copied != expected_bytes {
        return Err(AsterError::validation_error(format!(
            "{context} size mismatch: declared {expected_bytes} bytes, extracted {copied} bytes"
        )));
    }

    Ok(copied)
}

fn map_seven_zip_stream_read_error(error: std::io::Error) -> AsterError {
    if let Some(source) = error
        .get_ref()
        .and_then(|source| source.downcast_ref::<AsterError>())
    {
        return source.clone();
    }

    // SolidBlockStreamReader reports decoder/checksum failures as io::Error. The archive is
    // already downloaded to a local temp file here, so non-Aster read errors are invalid archive
    // data rather than retryable remote storage failures.
    AsterError::validation_error(format!("invalid 7z archive entry: {error}"))
}

fn create_empty_seven_zip_stage_file(
    stage_root: &Path,
    manifest_entry: &ArchiveScanEntry,
) -> Result<u64> {
    if manifest_entry.size != 0 {
        return Err(AsterError::validation_error("invalid 7z archive entry"));
    }
    create_seven_zip_stage_output(stage_root, manifest_entry)?;
    Ok(0)
}

fn create_seven_zip_stage_output(
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

#[allow(clippy::too_many_arguments)]
fn record_seven_zip_file_progress(
    handle: &tokio::runtime::Handle,
    db: &sea_orm::DatabaseConnection,
    lease_guard: &TaskLeaseGuard,
    steps: &mut [TaskStepInfo],
    relative_path: &Path,
    copied: u64,
    processed_bytes: &mut i64,
    file_count: &mut i64,
    total_bytes: i64,
    total_progress: i64,
    max_files: u64,
) -> Result<()> {
    *processed_bytes = processed_bytes
        .checked_add(crate::utils::numbers::u64_to_i64(
            copied,
            "extracted bytes",
        )?)
        .ok_or_else(|| AsterError::internal_error("archive extract progress overflow"))?;
    if *processed_bytes > total_bytes {
        return Err(AsterError::validation_error(format!(
            "archive extracted {} bytes, exceeds preflight total {}",
            *processed_bytes, total_bytes
        )));
    }
    *file_count += 1;
    if *file_count > crate::utils::numbers::u64_to_i64(max_files, "archive max file count")? {
        return Err(AsterError::validation_error(format!(
            "archive extracted {} files, exceeds preflight limit {}",
            *file_count, max_files
        )));
    }

    let status_text = format!("Extracting {}", relative_path.to_string_lossy());
    set_task_step_active(
        steps,
        TASK_STEP_EXTRACT_ARCHIVE,
        Some(&status_text),
        Some((*processed_bytes, total_bytes)),
    )?;
    handle.block_on(async {
        super::super::super::update_task_progress_db(
            db,
            lease_guard,
            *processed_bytes,
            total_progress,
            Some(&status_text),
            steps,
        )
        .await
    })
}

fn ensure_seven_zip_entry_count_matches_preflight(
    preflight_entry_count: usize,
    archive_entry_count: usize,
) -> Result<()> {
    if preflight_entry_count != archive_entry_count {
        return Err(AsterError::internal_error(format!(
            "archive preflight entry count {} differs from archive entry count {}",
            preflight_entry_count, archive_entry_count
        )));
    }
    Ok(())
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

fn ensure_seven_zip_entry_matches_preflight(
    entry: &zesven::Entry,
    manifest_entry: &ArchiveScanEntry,
) -> Result<()> {
    let is_dir = entry.is_directory;
    if is_dir != manifest_entry.kind.is_dir() {
        return Err(AsterError::validation_error(format!(
            "archive entry '{}' type changed after preflight",
            entry.path.as_str()
        )));
    }
    if !is_dir {
        let declared_size = crate::utils::numbers::u64_to_i64(entry.size, "archive entry size")?;
        if declared_size != manifest_entry.size {
            return Err(AsterError::validation_error(format!(
                "archive entry '{}' declared size changed after preflight",
                entry.path.as_str()
            )));
        }
    }
    Ok(())
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
