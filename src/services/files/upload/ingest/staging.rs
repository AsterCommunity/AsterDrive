//! Local staging-file contract for server-managed chunked uploads.
//!
//! Server-managed sessions use a format-specific `.offset-staging-v1` file. Init preallocates it
//! to `total_size`; each Chunk PUT writes its range at
//! `chunk_number * chunk_size`. The database receipt table is the durable completion index, while
//! the staging file may still contain unwritten sparse ranges until every receipt exists.
//!
//! The persisted `session_kind` is the session-format discriminator used by Chunk PUT and Complete;
//! temporary directory contents are never used to infer it.

use std::io::{ErrorKind, SeekFrom};
#[cfg(unix)]
use std::path::Path;
use std::path::{Path as StdPath, PathBuf};

use tokio::io::AsyncSeekExt;

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::upload_session_repo;
use crate::errors::{AsterError, MapAsterErr, Result, chunk_upload_error_with_code};
use crate::runtime::SharedRuntimeState;
use aster_drive_model::entities::upload_session;
use aster_forge_utils::numbers::i64_to_u64;
use aster_forge_utils::paths;

pub(crate) const CHUNK_RECEIPT_ETAG: &str = "aster-drive-offset-staging-receipt-v1";
const OFFSET_STAGING_FILE_NAME: &str = ".offset-staging-v1";

pub(crate) fn file_path(state: &impl SharedRuntimeState, upload_id: &str) -> String {
    file_path_in_upload_temp_dir(&state.config().server.upload_temp_dir, upload_id)
}

pub(crate) fn file_path_in_upload_temp_dir(upload_temp_dir: &str, upload_id: &str) -> String {
    let session_temp_dir = paths::upload_temp_dir(upload_temp_dir, upload_id);
    paths::temp_file_path(&session_temp_dir, OFFSET_STAGING_FILE_NAME)
}

pub(crate) async fn prepare(
    state: &crate::runtime::PrimaryAppState,
    _admission: &mut crate::storage::staging_capacity::StagingCapacityState,
    upload_id: &str,
    total_size: i64,
) -> Result<()> {
    let root = PathBuf::from(&state.config().server.upload_temp_dir);
    let path = PathBuf::from(file_path(state, upload_id));
    let allocation =
        physically_allocate(&root, &path, total_size, true, staging_safety_floor(state)).await;
    if allocation.is_err() {
        state
            .metrics()
            .record_upload_staging_capacity_admission("unavailable");
    }
    match allocation? {
        PhysicalAllocation::Allocated { additional_bytes } => {
            state
                .metrics()
                .record_upload_staging_capacity_admission("sufficient");
            tracing::debug!(
                upload_id,
                total_size,
                additional_bytes,
                "physically reserved upload staging capacity"
            );
        }
        PhysicalAllocation::Insufficient {
            additional_bytes,
            available_bytes,
            safety_floor_bytes,
        } => {
            state
                .metrics()
                .record_upload_staging_capacity_admission("insufficient");
            return Err(staging_capacity_insufficient_error(
                additional_bytes,
                available_bytes,
                safety_floor_bytes,
            ));
        }
    }
    sync_parent_directory(path.to_string_lossy().as_ref()).await?;
    Ok(())
}

pub(crate) async fn preflight(
    state: &crate::runtime::PrimaryAppState,
    admission: &mut crate::storage::staging_capacity::StagingCapacityState,
    total_size: i64,
) -> Result<()> {
    let root = PathBuf::from(&state.config().server.upload_temp_dir);
    if admission.needs_recovery(&root) {
        if let Err(error) = recover_active_reservations(state, &root, None).await {
            record_recovery_error(state, &error);
            return Err(error);
        }
        admission.mark_recovered(root.clone());
    }

    let required_bytes = i64_to_u64(total_size, "chunk staging total size")?;
    let available_bytes = match available_space(&root).await {
        Ok(available_bytes) => available_bytes,
        Err(error) => {
            state
                .metrics()
                .record_upload_staging_capacity_admission("unavailable");
            return Err(error);
        }
    };
    let safety_floor_bytes = staging_safety_floor(state);
    if matches!(
        assess_physical_allocation(required_bytes, 0, available_bytes, safety_floor_bytes),
        PhysicalAllocation::Insufficient { .. }
    ) {
        state
            .metrics()
            .record_upload_staging_capacity_admission("insufficient");
        return Err(staging_capacity_insufficient_error(
            required_bytes,
            available_bytes,
            safety_floor_bytes,
        ));
    }
    Ok(())
}

pub(crate) async fn ensure_reservations_recovered(
    state: &crate::runtime::PrimaryAppState,
) -> Result<()> {
    let root = PathBuf::from(&state.config().server.upload_temp_dir);
    let mut admission = state.driver_registry().staging_capacity().lock().await;
    if !admission.needs_recovery(&root) {
        return Ok(());
    }
    ensure_staging_root(&root).await?;
    if let Err(error) = recover_active_reservations(state, &root, None).await {
        record_recovery_error(state, &error);
        return Err(error);
    }
    admission.mark_recovered(root);
    Ok(())
}

pub(crate) async fn ensure_session_reservation(
    state: &crate::runtime::PrimaryAppState,
    session: &upload_session::Model,
) -> Result<()> {
    let root = PathBuf::from(&state.config().server.upload_temp_dir);
    let _admission = state.driver_registry().staging_capacity().lock().await;
    ensure_staging_root(&root).await?;
    let path = PathBuf::from(file_path(state, &session.id));
    if !tokio::fs::try_exists(&path).await.map_aster_err_ctx(
        "inspect current staging reservation",
        |message| {
            crate::errors::upload_assembly_error_with_code(
                ApiErrorCode::UploadAssemblyIoFailed,
                message,
            )
        },
    )? {
        return Err(crate::errors::upload_assembly_error_with_code(
            ApiErrorCode::UploadAssemblyIoFailed,
            "current upload staging reservation file is missing",
        ));
    }
    let metadata = tokio::fs::metadata(&path).await.map_aster_err_ctx(
        "inspect current staging reservation size",
        |message| {
            crate::errors::upload_assembly_error_with_code(
                ApiErrorCode::UploadAssemblyIoFailed,
                message,
            )
        },
    )?;
    let expected_size = i64_to_u64(session.total_size, "chunk staging total size")?;
    if !metadata.is_file() || metadata.len() != expected_size {
        return Err(crate::errors::upload_assembly_error_with_code(
            ApiErrorCode::UploadAssemblyIoFailed,
            format!(
                "current upload staging file size mismatch: expected {expected_size}, got {}",
                metadata.len()
            ),
        ));
    }
    match physically_allocate(
        &root,
        &path,
        session.total_size,
        false,
        staging_safety_floor(state),
    )
    .await
    {
        Ok(PhysicalAllocation::Allocated { additional_bytes }) => {
            if additional_bytes > 0 {
                state
                    .metrics()
                    .record_upload_staging_capacity_admission("recovered");
            }
            Ok(())
        }
        Ok(PhysicalAllocation::Insufficient {
            additional_bytes,
            available_bytes,
            safety_floor_bytes,
        }) => {
            state
                .metrics()
                .record_upload_staging_capacity_admission("insufficient");
            Err(staging_capacity_insufficient_error(
                additional_bytes,
                available_bytes,
                safety_floor_bytes,
            ))
        }
        Err(error) => {
            state
                .metrics()
                .record_upload_staging_capacity_admission("unavailable");
            Err(error)
        }
    }
}

async fn ensure_staging_root(root: &StdPath) -> Result<()> {
    tokio::fs::create_dir_all(root)
        .await
        .map_aster_err_ctx("create upload staging root", |message| {
            chunk_upload_error_with_code(ApiErrorCode::UploadTempDirCreateFailed, message)
        })
}

async fn available_space(root: &StdPath) -> Result<u64> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut probe = root.as_path();
        loop {
            match std::fs::metadata(probe) {
                Ok(_) => return aster_fs::available_space(probe),
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    probe = probe.parent().ok_or_else(|| {
                        std::io::Error::new(
                            ErrorKind::NotFound,
                            "upload staging root has no existing ancestor",
                        )
                    })?;
                }
                Err(error) => return Err(error),
            }
        }
    })
    .await
    .map_err(|error| {
        chunk_upload_error_with_code(
            ApiErrorCode::UploadTempFileWriteFailed,
            format!("upload staging capacity task failed: {error}"),
        )
    })?
    .map_err(|error| {
        chunk_upload_error_with_code(
            ApiErrorCode::UploadTempFileWriteFailed,
            format!("inspect upload staging capacity: {error}"),
        )
    })
}

fn record_recovery_error(state: &impl SharedRuntimeState, error: &AsterError) {
    if error.api_error_code() != ApiErrorCode::UploadStagingCapacityInsufficient {
        state
            .metrics()
            .record_upload_staging_capacity_admission("unavailable");
    }
}

fn staging_safety_floor(state: &impl SharedRuntimeState) -> u64 {
    state.config().server.upload_temp_min_free_bytes
}

async fn recover_active_reservations(
    state: &crate::runtime::PrimaryAppState,
    root: &StdPath,
    excluded_upload_id: Option<&str>,
) -> Result<()> {
    let sessions = upload_session_repo::find_active_staged(state.writer_db()).await?;
    for session in sessions {
        if excluded_upload_id == Some(session.id.as_str()) {
            continue;
        }
        let path = PathBuf::from(file_path(state, &session.id));
        let exists = tokio::fs::try_exists(&path).await.map_aster_err_ctx(
            "inspect recovered chunk staging file",
            |message| {
                chunk_upload_error_with_code(ApiErrorCode::UploadTempFileWriteFailed, message)
            },
        )?;
        if !exists && session.received_count > 0 {
            return Err(crate::errors::upload_assembly_error_with_code(
                ApiErrorCode::UploadSessionCorrupted,
                format!(
                    "active staged upload {} has {} receipt(s) but its reservation file is missing",
                    session.id, session.received_count
                ),
            ));
        }
        if !exists {
            let session_temp_dir =
                paths::upload_temp_dir(&state.config().server.upload_temp_dir, &session.id);
            tokio::fs::create_dir_all(&session_temp_dir)
                .await
                .map_aster_err_ctx("recreate chunk staging directory", |message| {
                    chunk_upload_error_with_code(ApiErrorCode::UploadTempDirCreateFailed, message)
                })?;
        }
        match physically_allocate(
            root,
            &path,
            session.total_size,
            !exists,
            staging_safety_floor(state),
        )
        .await?
        {
            PhysicalAllocation::Allocated { additional_bytes } => {
                if additional_bytes > 0 {
                    state
                        .metrics()
                        .record_upload_staging_capacity_admission("recovered");
                    tracing::info!(
                        upload_id = %session.id,
                        total_size = session.total_size,
                        additional_bytes,
                        "recovered physical upload staging reservation"
                    );
                }
            }
            PhysicalAllocation::Insufficient {
                additional_bytes,
                available_bytes,
                safety_floor_bytes,
            } => {
                state
                    .metrics()
                    .record_upload_staging_capacity_admission("insufficient");
                return Err(staging_capacity_insufficient_error(
                    additional_bytes,
                    available_bytes,
                    safety_floor_bytes,
                ));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PhysicalAllocation {
    Allocated {
        additional_bytes: u64,
    },
    Insufficient {
        additional_bytes: u64,
        available_bytes: u64,
        safety_floor_bytes: u64,
    },
}

fn assess_physical_allocation(
    total_size: u64,
    allocated_size: u64,
    available_bytes: u64,
    safety_floor_bytes: u64,
) -> PhysicalAllocation {
    let additional_bytes = total_size.saturating_sub(allocated_size.min(total_size));
    if additional_bytes <= available_bytes.saturating_sub(safety_floor_bytes) {
        PhysicalAllocation::Allocated { additional_bytes }
    } else {
        PhysicalAllocation::Insufficient {
            additional_bytes,
            available_bytes,
            safety_floor_bytes,
        }
    }
}

async fn physically_allocate(
    root: &StdPath,
    path: &StdPath,
    total_size: i64,
    create_new: bool,
    safety_floor_bytes: u64,
) -> Result<PhysicalAllocation> {
    let total_size = i64_to_u64(total_size, "chunk staging total size")?;
    let root = root.to_path_buf();
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true);
        if create_new {
            options.create_new(true);
        }
        let file = options.open(&path).map_err(|error| {
            chunk_upload_error_with_code(
                ApiErrorCode::UploadTempFileCreateFailed,
                format!("create chunk staging file: {error}"),
            )
        })?;
        let allocated_size = aster_fs::FileExt::allocated_size(&file).map_err(|error| {
            chunk_upload_error_with_code(
                ApiErrorCode::UploadTempFileWriteFailed,
                format!("inspect chunk staging allocation: {error}"),
            )
        })?;
        let available_bytes = aster_fs::available_space(&root).map_err(|error| {
            chunk_upload_error_with_code(
                ApiErrorCode::UploadTempFileWriteFailed,
                format!("inspect upload staging capacity: {error}"),
            )
        })?;
        let assessment = assess_physical_allocation(
            total_size,
            allocated_size,
            available_bytes,
            safety_floor_bytes,
        );
        let PhysicalAllocation::Allocated { .. } = assessment else {
            return Ok(assessment);
        };
        if let Err(error) = aster_fs::FileExt::allocate(&file, total_size) {
            let available_after_error = aster_fs::available_space(&root).unwrap_or(available_bytes);
            if error.kind() == ErrorKind::StorageFull
                || matches!(
                    assess_physical_allocation(
                        total_size,
                        allocated_size,
                        available_after_error,
                        safety_floor_bytes,
                    ),
                    PhysicalAllocation::Insufficient { .. }
                )
            {
                return Ok(PhysicalAllocation::Insufficient {
                    additional_bytes: total_size.saturating_sub(allocated_size.min(total_size)),
                    available_bytes: available_after_error,
                    safety_floor_bytes,
                });
            }
            return Err(chunk_upload_error_with_code(
                ApiErrorCode::UploadTempFileWriteFailed,
                format!("physically allocate chunk staging file: {error}"),
            ));
        }
        let reserved_size = aster_fs::FileExt::allocated_size(&file).map_err(|error| {
            chunk_upload_error_with_code(
                ApiErrorCode::UploadTempFileWriteFailed,
                format!("verify chunk staging allocation: {error}"),
            )
        })?;
        if reserved_size < total_size {
            return Err(chunk_upload_error_with_code(
                ApiErrorCode::UploadTempFileWriteFailed,
                format!("filesystem reserved only {reserved_size} of {total_size} staging bytes"),
            ));
        }
        if let PhysicalAllocation::Allocated { additional_bytes } = assessment
            && additional_bytes > 0
        {
            let available_after_allocation = aster_fs::available_space(&root).map_err(|error| {
                chunk_upload_error_with_code(
                    ApiErrorCode::UploadTempFileWriteFailed,
                    format!("verify upload staging safety floor: {error}"),
                )
            })?;
            if available_after_allocation < safety_floor_bytes {
                return Ok(PhysicalAllocation::Insufficient {
                    additional_bytes,
                    available_bytes: available_after_allocation,
                    safety_floor_bytes,
                });
            }
        }
        file.sync_all().map_err(|error| {
            chunk_upload_error_with_code(
                ApiErrorCode::UploadTempFileWriteFailed,
                format!("sync chunk staging file: {error}"),
            )
        })?;
        Ok(assessment)
    })
    .await
    .map_err(|error| {
        chunk_upload_error_with_code(
            ApiErrorCode::UploadTempFileWriteFailed,
            format!("physical staging allocation task failed: {error}"),
        )
    })?
}

fn staging_capacity_insufficient_error(
    required_bytes: u64,
    available_bytes: u64,
    safety_floor_bytes: u64,
) -> AsterError {
    AsterError::upload_staging_capacity_insufficient(format!(
        "upload staging requires {required_bytes} additional bytes, but the filesystem has \
         {available_bytes} bytes available with a {safety_floor_bytes}-byte safety floor"
    ))
}

#[cfg(unix)]
async fn sync_parent_directory(path: &str) -> Result<()> {
    let parent = Path::new(path).parent().ok_or_else(|| {
        chunk_upload_error_with_code(
            ApiErrorCode::UploadTempFileWriteFailed,
            "chunk staging file has no parent directory",
        )
    })?;
    let directory = tokio::fs::File::open(parent)
        .await
        .map_aster_err_ctx("open chunk staging directory", |message| {
            chunk_upload_error_with_code(ApiErrorCode::UploadTempFileWriteFailed, message)
        })?;
    directory
        .sync_all()
        .await
        .map_aster_err_ctx("sync chunk staging directory", |message| {
            chunk_upload_error_with_code(ApiErrorCode::UploadTempFileWriteFailed, message)
        })
}

#[cfg(not(unix))]
async fn sync_parent_directory(_path: &str) -> Result<()> {
    Ok(())
}

pub(crate) fn chunk_receipt_etag() -> &'static str {
    CHUNK_RECEIPT_ETAG
}

pub(crate) fn chunk_receipt_matches(
    receipt: &aster_drive_model::entities::upload_session_part::Model,
    expected_part_number: i32,
    expected_size: i64,
) -> bool {
    receipt.part_number == expected_part_number
        && receipt.etag == CHUNK_RECEIPT_ETAG
        && receipt.size == expected_size
}

pub(crate) async fn open_for_chunk_write(
    state: &impl SharedRuntimeState,
    session: &upload_session::Model,
    chunk_number: i32,
) -> Result<tokio::fs::File> {
    let chunk_offset = i64::from(chunk_number)
        .checked_mul(session.chunk_size)
        .ok_or_else(|| {
            chunk_upload_error_with_code(
                ApiErrorCode::UploadChunkSizeOverflow,
                "chunk staging offset exceeds i64 range",
            )
        })?;
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .open(file_path(state, &session.id))
        .await
        .map_aster_err_ctx("open chunk staging file", |message| {
            chunk_upload_error_with_code(ApiErrorCode::UploadChunkPersistFailed, message)
        })?;
    file.seek(SeekFrom::Start(i64_to_u64(
        chunk_offset,
        "chunk staging offset",
    )?))
    .await
    .map_aster_err_ctx("seek chunk staging file", |message| {
        chunk_upload_error_with_code(ApiErrorCode::UploadChunkPersistFailed, message)
    })?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::{PhysicalAllocation, assess_physical_allocation, physically_allocate};
    use aster_fs::FileExt;

    #[test]
    fn physical_allocation_assessment_preserves_floor_and_accepts_exact_fit() {
        assert_eq!(
            assess_physical_allocation(100, 20, 90, 10),
            PhysicalAllocation::Allocated {
                additional_bytes: 80
            }
        );
        assert_eq!(
            assess_physical_allocation(101, 20, 90, 10),
            PhysicalAllocation::Insufficient {
                additional_bytes: 81,
                available_bytes: 90,
                safety_floor_bytes: 10,
            }
        );
        assert_eq!(
            assess_physical_allocation(100, 100, 0, u64::MAX),
            PhysicalAllocation::Allocated {
                additional_bytes: 0
            }
        );
    }

    #[tokio::test]
    async fn physical_allocation_reserves_blocks_and_reuses_existing_reservation() {
        let root =
            std::env::temp_dir().join(format!("aster-staging-allocation-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("staging.bin");

        let first = physically_allocate(&root, &path, 1024 * 1024, true, 0)
            .await
            .unwrap();
        assert_eq!(
            first,
            PhysicalAllocation::Allocated {
                additional_bytes: 1024 * 1024
            }
        );
        let file = std::fs::File::open(&path).unwrap();
        assert!(file.allocated_size().unwrap() >= 1024 * 1024);
        assert_eq!(file.metadata().unwrap().len(), 1024 * 1024);

        let second = physically_allocate(&root, &path, 1024 * 1024, false, 0)
            .await
            .unwrap();
        assert_eq!(
            second,
            PhysicalAllocation::Allocated {
                additional_bytes: 0
            }
        );

        std::fs::remove_dir_all(root).unwrap();
    }
}
