//! 归档任务子模块：`selection`。

use std::{
    collections::{HashMap, HashSet},
    io::{self, Write},
    path::{Component, Path},
};

use actix_web::HttpResponse;
use chrono::Utc;
use futures::{Stream, StreamExt};

use super::common::{
    ArchiveEntry, ArchiveFileEntry, ArchiveSinkContext, ends_with_ignore_ascii_case,
    is_client_disconnect_error_text, write_archive_to_sink,
};
use crate::config::operations;
use crate::db::repository::{file_repo, folder_repo};
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{
    files::{batch, download_headers::DownloadDisposition, folder as folder_ops},
    share::{
        load_valid_folder_share_root, reserve_share_download_count, rollback_share_download_count,
    },
    task::types::CreateArchiveTaskParams,
    workspace::storage::{self, WorkspaceResourceScope, WorkspaceStorageScope},
};
use aster_drive_model::entities::{file, folder, share};
use aster_forge_utils::numbers::u64_to_usize;

const ARCHIVE_FOLDER_TREE_MAXIMUM_DEPTH: usize = 128;
const ARCHIVE_DOWNLOAD_PIPE_CAPACITY: usize = 64 * 1024;
const ARCHIVE_DOWNLOAD_CHUNK_SIZE: usize = 64 * 1024;

pub(crate) struct PreparedArchiveDownload {
    pub file_ids: Vec<i64>,
    pub folder_ids: Vec<i64>,
    pub archive_name: String,
}

pub(super) struct ResolvedArchiveDownload {
    pub(super) selection: batch::NormalizedSelection,
    pub(super) archive_name: String,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ArchiveBuildLimits {
    pub(super) max_entries: u64,
    pub(super) max_total_source_bytes: i64,
    pub(super) max_temp_bytes: i64,
}

impl ArchiveBuildLimits {
    pub(super) fn from_runtime_config(runtime_config: &crate::config::RuntimeConfig) -> Self {
        Self {
            max_entries: operations::archive_build_max_entries(runtime_config),
            max_total_source_bytes: operations::archive_build_max_total_source_bytes(
                runtime_config,
            ),
            max_temp_bytes: operations::archive_build_max_temp_bytes(runtime_config),
        }
    }

    fn folder_tree_traversal_limits(
        self,
        collected_entries: u64,
    ) -> Result<folder_ops::FolderTreeTraversalLimits> {
        let remaining_entries = self
            .max_entries
            .checked_sub(collected_entries)
            .ok_or_else(|| {
                AsterError::validation_error(format!(
                    "archive selection expands to {collected_entries} entries, exceeds server limit {}",
                    self.max_entries
                ))
            })?;
        if remaining_entries == 0 {
            return Err(AsterError::validation_error(format!(
                "archive selection expands to {collected_entries} entries, exceeds server limit {}",
                self.max_entries
            )));
        }
        let maximum_resources =
            u64_to_usize(remaining_entries, "archive remaining maximum entries")?;
        Ok(folder_ops::FolderTreeTraversalLimits::new(
            maximum_resources,
            maximum_resources,
            ARCHIVE_FOLDER_TREE_MAXIMUM_DEPTH,
        ))
    }
}

#[derive(Debug, Default)]
pub(super) struct ArchiveBuildStats {
    pub(super) total_source_bytes: i64,
    pub(super) estimated_output_bytes: i64,
}

#[derive(Debug)]
pub(super) struct CollectedArchiveEntries {
    pub(super) entries: Vec<ArchiveEntry>,
    pub(super) stats: ArchiveBuildStats,
}

impl CollectedArchiveEntries {
    pub(super) fn total_source_bytes(&self) -> i64 {
        self.stats.total_source_bytes
    }

    pub(super) fn estimated_output_bytes(&self) -> i64 {
        self.stats.estimated_output_bytes
    }

    pub(super) fn into_entries(self) -> Vec<ArchiveEntry> {
        self.entries
    }
}

#[derive(Debug, Default)]
struct ArchiveBuildStatsBuilder {
    entry_count: u64,
    total_source_bytes: i64,
    estimated_output_bytes: i64,
}

pub(crate) async fn stream_archive_download_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    params: CreateArchiveTaskParams,
) -> Result<HttpResponse> {
    let resolved = resolve_archive_download_in_scope(state, scope, &params).await?;
    let archive_name = resolved.archive_name.clone();
    let limits = ArchiveBuildLimits::from_runtime_config(state.runtime_config());
    let collected =
        collect_archive_entries_from_selection_in_scope(state, scope, &resolved.selection, limits)
            .await?;
    let total_bytes = collected.total_source_bytes();

    let handle = tokio::runtime::Handle::current();
    let db = state.writer_db().clone();
    let driver_registry = state.driver_registry().clone();
    let policy_snapshot = state.policy_snapshot().clone();
    let archive_name_for_worker = archive_name.clone();

    let body = archive_download_body(archive_name.clone(), move |writer| {
        let writer = tokio_util::io::SyncIoBridge::new(writer);
        let writer = std::io::BufWriter::new(writer);
        let result = write_archive_to_sink(
            ArchiveSinkContext {
                handle: &handle,
                db: &db,
                driver_registry: driver_registry.as_ref(),
                policy_snapshot: policy_snapshot.as_ref(),
                execution: None,
            },
            collected.into_entries(),
            total_bytes,
            limits,
            writer,
            |_, _| Ok(()),
        )
        .and_then(|(writer, _)| flush_archive_download_sink(writer, "archive download"));
        if let Err(error) = &result {
            let error_text = error.to_string();
            if is_client_disconnect_error_text(&error_text) {
                tracing::info!(
                    archive_name = %archive_name_for_worker,
                    "archive download stream stopped after client disconnected"
                );
            } else {
                tracing::warn!(
                    archive_name = %archive_name_for_worker,
                    error = %error_text,
                    "archive download stream failed"
                );
            }
        }
        result
    });

    Ok(HttpResponse::Ok()
        .content_type("application/zip")
        .insert_header((
            "Content-Disposition",
            DownloadDisposition::Attachment.header_value(&archive_name),
        ))
        .insert_header(("Content-Encoding", "identity"))
        .streaming(body))
}

pub(crate) async fn stream_shared_archive_download(
    state: &PrimaryAppState,
    token: &str,
    params: CreateArchiveTaskParams,
) -> Result<HttpResponse> {
    let resolved = resolve_shared_archive_download(state, token, &params).await?;
    let archive_name = resolved.archive_name.clone();
    let limits = ArchiveBuildLimits::from_runtime_config(state.runtime_config());
    reserve_share_download_count(state, &resolved.share).await?;
    let collected_result = collect_archive_entries_from_shared_selection(
        state,
        resolved.scope,
        &resolved.selection,
        limits,
    )
    .await;
    let collected = match collected_result {
        Ok(collected) => collected,
        Err(error) => {
            rollback_share_download_count(state, resolved.share.id).await;
            return Err(error);
        }
    };
    let total_bytes = collected.total_source_bytes();

    let handle = tokio::runtime::Handle::current();
    let db = state.writer_db().clone();
    let driver_registry = state.driver_registry().clone();
    let policy_snapshot = state.policy_snapshot().clone();
    let rollback_queue = state.share_download_rollback.clone();
    let share_id = resolved.share.id;
    let archive_name_for_worker = archive_name.clone();

    let body = archive_download_body(archive_name.clone(), move |writer| {
        let writer = tokio_util::io::SyncIoBridge::new(writer);
        let writer = std::io::BufWriter::new(writer);
        let result = write_archive_to_sink(
            ArchiveSinkContext {
                handle: &handle,
                db: &db,
                driver_registry: driver_registry.as_ref(),
                policy_snapshot: policy_snapshot.as_ref(),
                execution: None,
            },
            collected.into_entries(),
            total_bytes,
            limits,
            writer,
            |_, _| Ok(()),
        )
        .and_then(|(writer, _)| flush_archive_download_sink(writer, "shared archive download"));
        if let Err(error) = &result {
            let error_text = error.to_string();
            rollback_queue.enqueue(share_id);
            if is_client_disconnect_error_text(&error_text) {
                tracing::info!(
                    share_id,
                    archive_name = %archive_name_for_worker,
                    "shared archive download stream stopped after client disconnected"
                );
            } else {
                tracing::warn!(
                    share_id,
                    archive_name = %archive_name_for_worker,
                    error = %error_text,
                    "shared archive download stream failed"
                );
            }
        }
        result
    });

    Ok(HttpResponse::Ok()
        .content_type("application/zip")
        .insert_header((
            "Content-Disposition",
            DownloadDisposition::Attachment.header_value(&archive_name),
        ))
        .insert_header(("Content-Encoding", "identity"))
        .streaming(body))
}

// `ZipWriter::finish` 只把中央目录写进传入的 writer；若外层是 `BufWriter`，
// 这里必须显式冲刷并保留错误，不能交给忽略错误的析构路径。
fn flush_archive_download_sink<W: Write>(mut writer: W, context: &str) -> Result<()> {
    writer.flush().map_err(|error| {
        AsterError::storage_driver_error(format!("flush completed {context} stream: {error}"))
    })
}

fn archive_download_body<F>(
    archive_name: String,
    worker: F,
) -> impl Stream<Item = io::Result<bytes::Bytes>>
where
    F: FnOnce(tokio::io::DuplexStream) -> Result<()> + Send + 'static,
{
    let (reader, writer) = tokio::io::duplex(ARCHIVE_DOWNLOAD_PIPE_CAPACITY);
    let worker = tokio::task::spawn_blocking(move || worker(writer));
    let mut reader =
        tokio_util::io::ReaderStream::with_capacity(reader, ARCHIVE_DOWNLOAD_CHUNK_SIZE);

    async_stream::try_stream! {
        // 先持续消费有界 pipe 以维持背压；pipe EOF 之后仍需观察 worker 结果，
        // 只有 worker 成功完成才把这次响应呈现为 clean EOF。
        while let Some(chunk) = reader.next().await {
            yield chunk?;
        }

        match worker.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => Err(io::Error::other(error.to_string()))?,
            Err(error) => {
                tracing::error!(
                    archive_name = %archive_name,
                    error = %error,
                    "archive download worker terminated before clean completion"
                );
                Err(io::Error::other(format!(
                    "archive download worker terminated before clean completion: {error}"
                )))?;
            }
        }
    }
}

pub(crate) async fn prepare_shared_archive_download(
    state: &impl SharedRuntimeState,
    token: &str,
    params: &CreateArchiveTaskParams,
) -> Result<PreparedArchiveDownload> {
    let resolved = resolve_shared_archive_download(state, token, params).await?;
    let limits = ArchiveBuildLimits::from_runtime_config(state.runtime_config());
    let _ = collect_archive_entries_from_shared_selection(
        state,
        resolved.scope,
        &resolved.selection,
        limits,
    )
    .await?;
    Ok(PreparedArchiveDownload {
        file_ids: resolved.selection.file_ids,
        folder_ids: resolved.selection.folder_ids,
        archive_name: resolved.archive_name,
    })
}

pub(crate) async fn prepare_archive_download_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    params: &CreateArchiveTaskParams,
) -> Result<PreparedArchiveDownload> {
    let resolved = resolve_archive_download_in_scope(state, scope, params).await?;
    let limits = ArchiveBuildLimits::from_runtime_config(state.runtime_config());
    let _ =
        collect_archive_entries_from_selection_in_scope(state, scope, &resolved.selection, limits)
            .await?;
    Ok(PreparedArchiveDownload {
        file_ids: resolved.selection.file_ids,
        folder_ids: resolved.selection.folder_ids,
        archive_name: resolved.archive_name,
    })
}

pub(super) async fn resolve_archive_download_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    params: &CreateArchiveTaskParams,
) -> Result<ResolvedArchiveDownload> {
    ensure_archive_selection_request_in_scope(state, scope, &params.file_ids, &params.folder_ids)
        .await?;
    let selection = batch::load_normalized_selection_in_scope(
        state,
        scope,
        &params.file_ids,
        &params.folder_ids,
    )
    .await?;
    ensure_archive_selection_active(scope, &selection)?;
    let archive_name = resolve_archive_name(&params.archive_name, &selection)?;

    Ok(ResolvedArchiveDownload {
        selection,
        archive_name,
    })
}

struct ResolvedSharedArchiveDownload {
    selection: batch::NormalizedSelection,
    archive_name: String,
    share: share::Model,
    scope: WorkspaceResourceScope,
}

async fn resolve_shared_archive_download(
    state: &impl SharedRuntimeState,
    token: &str,
    params: &CreateArchiveTaskParams,
) -> Result<ResolvedSharedArchiveDownload> {
    batch::validate_batch_ids(&params.file_ids, &params.folder_ids)?;
    let (share, root_folder_id) = load_valid_folder_share_root(state, token).await?;
    let scope = match share.team_id {
        Some(team_id) => WorkspaceResourceScope::Team { team_id },
        None => WorkspaceResourceScope::Personal {
            user_id: share.user_id,
        },
    };
    let selection = load_normalized_shared_selection(
        state,
        scope,
        root_folder_id,
        &params.file_ids,
        &params.folder_ids,
    )
    .await?;
    let archive_name = resolve_archive_name(&params.archive_name, &selection)?;

    Ok(ResolvedSharedArchiveDownload {
        selection,
        archive_name,
        share,
        scope,
    })
}

async fn ensure_archive_selection_request_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    file_ids: &[i64],
    folder_ids: &[i64],
) -> Result<()> {
    storage::require_scope_access_with_db(state, state.writer_db(), scope).await?;
    batch::validate_batch_ids(file_ids, folder_ids)?;

    let file_map: HashMap<i64, file::Model> = file_repo::find_by_ids(state.writer_db(), file_ids)
        .await?
        .into_iter()
        .map(|file| (file.id, file))
        .collect();
    for &file_id in file_ids {
        let file = file_map
            .get(&file_id)
            .ok_or_else(|| AsterError::file_not_found(format!("file #{file_id}")))?;
        storage::ensure_active_file_scope(file, scope)?;
    }

    let folder_map: HashMap<i64, folder::Model> =
        folder_repo::find_by_ids(state.writer_db(), folder_ids)
            .await?
            .into_iter()
            .map(|folder| (folder.id, folder))
            .collect();
    for &folder_id in folder_ids {
        let folder = folder_map
            .get(&folder_id)
            .ok_or_else(|| AsterError::folder_not_found(format!("folder #{folder_id}")))?;
        storage::ensure_active_folder_scope(folder, scope)?;
    }

    Ok(())
}

async fn load_normalized_shared_selection(
    state: &impl SharedRuntimeState,
    scope: WorkspaceResourceScope,
    root_folder_id: i64,
    file_ids: &[i64],
    folder_ids: &[i64],
) -> Result<batch::NormalizedSelection> {
    let file_map: HashMap<i64, file::Model> = file_repo::find_by_ids(state.writer_db(), file_ids)
        .await?
        .into_iter()
        .map(|file| (file.id, file))
        .collect();
    let mut verified_file_folder_ids = HashSet::new();
    for &file_id in file_ids {
        let file = file_map
            .get(&file_id)
            .ok_or_else(|| AsterError::file_not_found(format!("file #{file_id}")))?;
        ensure_shared_file_in_scope(
            state,
            scope,
            root_folder_id,
            file,
            &mut verified_file_folder_ids,
        )
        .await?;
    }

    let folder_map: HashMap<i64, folder::Model> =
        folder_repo::find_by_ids(state.writer_db(), folder_ids)
            .await?
            .into_iter()
            .map(|folder| (folder.id, folder))
            .collect();
    for &folder_id in folder_ids {
        let folder = folder_map
            .get(&folder_id)
            .ok_or_else(|| AsterError::folder_not_found(format!("folder #{folder_id}")))?;
        ensure_shared_folder_in_scope(state, scope, root_folder_id, folder).await?;
    }

    Ok(batch::NormalizedSelection {
        file_ids: file_ids.to_vec(),
        folder_ids: folder_ids.to_vec(),
        file_map,
        folder_map,
    })
}

async fn ensure_shared_file_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceResourceScope,
    root_folder_id: i64,
    file: &file::Model,
    verified_folder_ids: &mut HashSet<i64>,
) -> Result<()> {
    storage::ensure_file_resource_scope(file, scope)?;
    if file.deleted_at.is_some() {
        return Err(AsterError::file_not_found(format!(
            "file #{} is in trash",
            file.id
        )));
    }
    let Some(folder_id) = file.folder_id else {
        return Err(AsterError::auth_forbidden(
            "file is outside shared folder scope",
        ));
    };
    if verified_folder_ids.insert(folder_id) {
        folder_ops::verify_folder_in_scope(state.writer_db(), folder_id, root_folder_id).await?;
    }
    Ok(())
}

async fn ensure_shared_folder_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceResourceScope,
    root_folder_id: i64,
    folder: &folder::Model,
) -> Result<()> {
    storage::ensure_folder_resource_scope(folder, scope)?;
    if folder.deleted_at.is_some() {
        return Err(AsterError::folder_not_found(format!(
            "folder #{} is in trash",
            folder.id
        )));
    }
    folder_ops::verify_folder_in_scope(state.writer_db(), folder.id, root_folder_id).await
}

pub(super) fn ensure_archive_selection_active(
    scope: WorkspaceStorageScope,
    selection: &batch::NormalizedSelection,
) -> Result<()> {
    for &file_id in &selection.file_ids {
        let file = selection
            .file_map
            .get(&file_id)
            .ok_or_else(|| AsterError::file_not_found(format!("file #{file_id}")))?;
        storage::ensure_active_file_scope(file, scope)?;
    }

    for &folder_id in &selection.folder_ids {
        let folder = selection
            .folder_map
            .get(&folder_id)
            .ok_or_else(|| AsterError::folder_not_found(format!("folder #{folder_id}")))?;
        storage::ensure_active_folder_scope(folder, scope)?;
    }

    Ok(())
}

async fn collect_archive_entries_from_shared_selection(
    state: &impl SharedRuntimeState,
    scope: WorkspaceResourceScope,
    selection: &batch::NormalizedSelection,
    limits: ArchiveBuildLimits,
) -> Result<CollectedArchiveEntries> {
    let mut entries = Vec::new();
    let mut stats = ArchiveBuildStatsBuilder::default();
    let mut reserved_root_names = HashSet::new();

    for &file_id in &selection.file_ids {
        let file = selection
            .file_map
            .get(&file_id)
            .ok_or_else(|| AsterError::file_not_found(format!("file #{file_id}")))?;
        let entry_path = batch::reserve_unique_name(&mut reserved_root_names, &file.name);
        record_archive_build_entry(&mut stats, &entry_path, Some(file.size), limits)?;
        entries.push(ArchiveEntry::File {
            file: ArchiveFileEntry::from_file(file, &entry_path),
            entry_path,
        });
    }

    for &folder_id in &selection.folder_ids {
        let folder = selection
            .folder_map
            .get(&folder_id)
            .ok_or_else(|| AsterError::folder_not_found(format!("folder #{folder_id}")))?;
        let archive_root = batch::reserve_unique_name(&mut reserved_root_names, &folder.name);

        let tree_limits = limits.folder_tree_traversal_limits(stats.entry_count)?;
        let (tree_files, tree_folder_ids) = folder_ops::collect_folder_tree_in_resource_scope(
            state.writer_db(),
            scope,
            folder_id,
            false,
            Some(tree_limits),
        )
        .await
        .map_err(|error| {
            if matches!(&error, AsterError::OperationResourceLimitExceeded(_))
                && error.message() == folder_ops::FOLDER_TREE_RESOURCE_LIMIT_MESSAGE
            {
                AsterError::validation_error(format!(
                    "archive selection expands to {} entries or more, exceeds server limit {}",
                    limits.max_entries.saturating_add(1),
                    limits.max_entries
                ))
            } else {
                error
            }
        })?;
        let folder_paths = folder_ops::build_folder_paths_cached(state, &tree_folder_ids).await?;
        let root_path = folder_paths
            .get(&folder_id)
            .cloned()
            .ok_or_else(|| AsterError::record_not_found(format!("folder #{folder_id} path")))?;

        for tree_folder_id in &tree_folder_ids {
            let folder_path = folder_paths.get(tree_folder_id).ok_or_else(|| {
                AsterError::record_not_found(format!("folder #{tree_folder_id} path"))
            })?;
            let entry_path = archive_directory_entry_path(&archive_root, folder_path, &root_path)?;
            record_archive_build_entry(&mut stats, &entry_path, None, limits)?;
            entries.push(ArchiveEntry::Directory { entry_path });
        }

        for file in tree_files {
            let parent_path = file
                .folder_id
                .and_then(|id| folder_paths.get(&id))
                .ok_or_else(|| {
                    AsterError::record_not_found(format!(
                        "missing parent path for file #{}",
                        file.id
                    ))
                })?;
            let relative_dir = archive_relative_dir(parent_path, &root_path)?;
            let entry_path = if relative_dir.is_empty() {
                format!("{archive_root}/{}", file.name)
            } else {
                format!("{archive_root}/{relative_dir}/{}", file.name)
            };
            record_archive_build_entry(&mut stats, &entry_path, Some(file.size), limits)?;
            entries.push(ArchiveEntry::File {
                file: ArchiveFileEntry::from_file(&file, &entry_path),
                entry_path,
            });
        }
    }

    entries.sort_by(|left, right| {
        left.entry_path()
            .cmp(right.entry_path())
            .then_with(|| left.is_file().cmp(&right.is_file()))
    });
    Ok(CollectedArchiveEntries {
        entries,
        stats: ArchiveBuildStats {
            total_source_bytes: stats.total_source_bytes,
            estimated_output_bytes: stats.estimated_output_bytes,
        },
    })
}

pub(super) async fn collect_archive_entries_from_selection_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    selection: &batch::NormalizedSelection,
    limits: ArchiveBuildLimits,
) -> Result<CollectedArchiveEntries> {
    let mut entries = Vec::new();
    let mut stats = ArchiveBuildStatsBuilder::default();
    let mut reserved_root_names = HashSet::new();

    for &file_id in &selection.file_ids {
        let file = selection
            .file_map
            .get(&file_id)
            .ok_or_else(|| AsterError::file_not_found(format!("file #{file_id}")))?;
        storage::ensure_active_file_scope(file, scope)?;
        let entry_path = batch::reserve_unique_name(&mut reserved_root_names, &file.name);
        record_archive_build_entry(&mut stats, &entry_path, Some(file.size), limits)?;
        entries.push(ArchiveEntry::File {
            file: ArchiveFileEntry::from_file(file, &entry_path),
            entry_path,
        });
    }

    for &folder_id in &selection.folder_ids {
        let folder = selection
            .folder_map
            .get(&folder_id)
            .ok_or_else(|| AsterError::folder_not_found(format!("folder #{folder_id}")))?;
        storage::ensure_active_folder_scope(folder, scope)?;
        let archive_root = batch::reserve_unique_name(&mut reserved_root_names, &folder.name);

        let tree_limits = limits.folder_tree_traversal_limits(stats.entry_count)?;
        let (tree_files, tree_folder_ids) = folder_ops::collect_folder_tree_in_scope(
            state.writer_db(),
            scope,
            folder_id,
            false,
            Some(tree_limits),
        )
        .await
        .map_err(|error| {
            if matches!(&error, AsterError::OperationResourceLimitExceeded(_))
                && error.message() == folder_ops::FOLDER_TREE_RESOURCE_LIMIT_MESSAGE
            {
                AsterError::validation_error(format!(
                    "archive selection expands to {} entries or more, exceeds server limit {}",
                    limits.max_entries.saturating_add(1),
                    limits.max_entries
                ))
            } else {
                error
            }
        })?;
        let folder_paths = folder_ops::build_folder_paths_cached(state, &tree_folder_ids).await?;
        let root_path = folder_paths
            .get(&folder_id)
            .cloned()
            .ok_or_else(|| AsterError::record_not_found(format!("folder #{folder_id} path")))?;

        for tree_folder_id in &tree_folder_ids {
            let folder_path = folder_paths.get(tree_folder_id).ok_or_else(|| {
                AsterError::record_not_found(format!("folder #{tree_folder_id} path"))
            })?;
            let entry_path = archive_directory_entry_path(&archive_root, folder_path, &root_path)?;
            record_archive_build_entry(&mut stats, &entry_path, None, limits)?;
            entries.push(ArchiveEntry::Directory { entry_path });
        }

        for file in tree_files {
            let parent_path = file
                .folder_id
                .and_then(|id| folder_paths.get(&id))
                .ok_or_else(|| {
                    AsterError::record_not_found(format!(
                        "missing parent path for file #{}",
                        file.id
                    ))
                })?;
            let relative_dir = archive_relative_dir(parent_path, &root_path)?;
            let entry_path = if relative_dir.is_empty() {
                format!("{archive_root}/{}", file.name)
            } else {
                format!("{archive_root}/{relative_dir}/{}", file.name)
            };
            record_archive_build_entry(&mut stats, &entry_path, Some(file.size), limits)?;
            entries.push(ArchiveEntry::File {
                file: ArchiveFileEntry::from_file(&file, &entry_path),
                entry_path,
            });
        }
    }

    entries.sort_by(|left, right| {
        left.entry_path()
            .cmp(right.entry_path())
            .then_with(|| left.is_file().cmp(&right.is_file()))
    });
    Ok(CollectedArchiveEntries {
        entries,
        stats: ArchiveBuildStats {
            total_source_bytes: stats.total_source_bytes,
            estimated_output_bytes: stats.estimated_output_bytes,
        },
    })
}

fn record_archive_build_entry(
    stats: &mut ArchiveBuildStatsBuilder,
    entry_path: &str,
    file_size: Option<i64>,
    limits: ArchiveBuildLimits,
) -> Result<()> {
    stats.entry_count = stats
        .entry_count
        .checked_add(1)
        .ok_or_else(|| AsterError::internal_error("archive build entry count overflow"))?;
    if stats.entry_count > limits.max_entries {
        return Err(AsterError::validation_error(format!(
            "archive selection expands to {} entries, exceeds server limit {}",
            stats.entry_count, limits.max_entries
        )));
    }

    if let Some(file_size) = file_size {
        stats.total_source_bytes = stats
            .total_source_bytes
            .checked_add(file_size)
            .ok_or_else(|| AsterError::internal_error("archive build source size overflow"))?;
        if stats.total_source_bytes > limits.max_total_source_bytes {
            return Err(AsterError::validation_error(format!(
                "archive selection source size {} exceeds server limit {}",
                stats.total_source_bytes, limits.max_total_source_bytes
            )));
        }
    }

    let path_bytes =
        aster_forge_utils::numbers::usize_to_i64(entry_path.len(), "archive entry path bytes")?;
    let source_bytes = file_size.unwrap_or(0);
    let estimated_entry_bytes = source_bytes
        .checked_add(path_bytes)
        .and_then(|value| value.checked_add(256))
        .ok_or_else(|| AsterError::internal_error("archive build temp size overflow"))?;
    stats.estimated_output_bytes = stats
        .estimated_output_bytes
        .checked_add(estimated_entry_bytes)
        .ok_or_else(|| AsterError::internal_error("archive build temp size overflow"))?;
    if stats.estimated_output_bytes > limits.max_temp_bytes {
        return Err(AsterError::validation_error(format!(
            "archive selection estimated output size {} exceeds server limit {}",
            stats.estimated_output_bytes, limits.max_temp_bytes
        )));
    }

    Ok(())
}

pub(super) async fn resolve_archive_compress_target_folder_id(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    selection: &batch::NormalizedSelection,
    requested_target_folder_id: Option<i64>,
) -> Result<Option<i64>> {
    if let Some(target_folder_id) = requested_target_folder_id {
        storage::verify_folder_access(state, scope, target_folder_id).await?;
        return Ok(Some(target_folder_id));
    }

    let mut parents = HashSet::new();
    for file_id in &selection.file_ids {
        let file = selection
            .file_map
            .get(file_id)
            .ok_or_else(|| AsterError::file_not_found(format!("file #{file_id}")))?;
        parents.insert(file.folder_id);
    }
    for folder_id in &selection.folder_ids {
        let folder = selection
            .folder_map
            .get(folder_id)
            .ok_or_else(|| AsterError::folder_not_found(format!("folder #{folder_id}")))?;
        parents.insert(folder.parent_id);
    }

    if parents.len() == 1 {
        Ok(parents.into_iter().next().unwrap_or(None))
    } else {
        Ok(None)
    }
}

fn archive_directory_entry_path(
    archive_root: &str,
    folder_path: &str,
    root_path: &str,
) -> Result<String> {
    let relative_dir = archive_relative_dir(folder_path, root_path)?;
    if relative_dir.is_empty() {
        return Ok(format!("{archive_root}/"));
    }

    Ok(format!("{archive_root}/{relative_dir}/"))
}

fn archive_relative_dir(folder_path: &str, root_path: &str) -> Result<String> {
    let relative_path = Path::new(folder_path)
        .strip_prefix(Path::new(root_path))
        .map_err(|_| {
            AsterError::internal_error(format!(
                "folder path '{folder_path}' is outside root '{root_path}'"
            ))
        })?;

    let mut parts = Vec::new();
    for component in relative_path.components() {
        match component {
            Component::Normal(part) => {
                let part = part.to_str().ok_or_else(|| {
                    AsterError::internal_error(format!(
                        "folder path '{folder_path}' contains non-UTF-8 segment"
                    ))
                })?;
                parts.push(part);
            }
            Component::CurDir => {}
            _ => {
                return Err(AsterError::internal_error(format!(
                    "folder path '{folder_path}' resolved to invalid relative path"
                )));
            }
        }
    }

    Ok(parts.join("/"))
}

fn resolve_archive_name(
    archive_name: &Option<String>,
    selection: &batch::NormalizedSelection,
) -> Result<String> {
    let base = match archive_name.as_deref().map(str::trim) {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => default_archive_name(selection),
    };
    let final_name = normalize_archive_zip_name(&base)?;
    aster_forge_validation::filename::validate_name(&final_name)?;
    Ok(final_name)
}

fn normalize_archive_zip_name(base: &str) -> Result<String> {
    if ends_with_ignore_ascii_case(base, ".zip") {
        return Ok(aster_forge_validation::filename::normalize_validate_name(
            base,
        )?);
    }

    let max_stem_len = aster_forge_validation::filename::MAX_FILENAME_LEN
        .checked_sub(".zip".len())
        .ok_or_else(|| AsterError::internal_error("archive name length limit is too small"))?;
    let stem = aster_forge_validation::filename::normalize_name(base);
    let stem = aster_forge_validation::filename::truncate_utf8_to_max_bytes(&stem, max_stem_len);
    let stem = stem.trim_end_matches([' ', '.']);
    if stem.is_empty() {
        return Err(AsterError::validation_error("name cannot be empty"));
    }
    Ok(format!("{stem}.zip"))
}

fn default_archive_name(selection: &batch::NormalizedSelection) -> String {
    if selection.folder_ids.len() == 1
        && selection.file_ids.is_empty()
        && let Some(folder) = selection.folder_map.get(&selection.folder_ids[0])
    {
        return folder.name.clone();
    }

    if selection.file_ids.len() == 1
        && selection.folder_ids.is_empty()
        && let Some(file) = selection.file_map.get(&selection.file_ids[0])
    {
        return file.name.clone();
    }

    format!("archive-{}", Utc::now().format("%Y%m%d-%H%M%S"))
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::path::PathBuf;

    use futures::StreamExt;
    use tokio::io::AsyncWriteExt;

    use crate::errors::AsterError;
    use crate::services::task::archive::common::{
        ArchiveEntry, ArchiveFileEntry, write_archive_entries_to_sink,
    };

    use super::{
        ArchiveBuildLimits, archive_directory_entry_path, archive_download_body,
        archive_relative_dir, flush_archive_download_sink, normalize_archive_zip_name,
    };

    const LARGE_STORED_ENTRY_BYTES: u64 = 512 * 1024 * 1024 + 1;
    const DEFLATED_ENTRY_BYTES: u64 = 2 * 1024 * 1024;

    struct TempArchiveFile(PathBuf);

    impl TempArchiveFile {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "aster-drive-archive-stream-{}.zip",
                uuid::Uuid::new_v4()
            )))
        }
    }

    impl Drop for TempArchiveFile {
        fn drop(&mut self) {
            if let Err(error) = std::fs::remove_file(&self.0) {
                tracing::warn!(
                    path = %self.0.display(),
                    error = %error,
                    "failed to remove archive stream test file"
                );
            }
        }
    }

    fn write_synthetic_archive(
        writer: tokio::io::DuplexStream,
        stored_bytes: u64,
        deflated_bytes: u64,
    ) -> crate::errors::Result<()> {
        let stored_size = i64::try_from(stored_bytes).expect("stored test size should fit i64");
        let deflated_size =
            i64::try_from(deflated_bytes).expect("deflated test size should fit i64");
        let total_bytes = stored_size + deflated_size;
        let entries = vec![
            ArchiveEntry::File {
                file: ArchiveFileEntry {
                    blob_id: 1,
                    size: stored_size,
                    store_without_deflate: true,
                },
                entry_path: "stored.bin".to_string(),
            },
            ArchiveEntry::File {
                file: ArchiveFileEntry {
                    blob_id: 2,
                    size: deflated_size,
                    store_without_deflate: false,
                },
                entry_path: "deflated.txt".to_string(),
            },
        ];
        let limits = ArchiveBuildLimits {
            max_entries: 2,
            max_total_source_bytes: total_bytes,
            max_temp_bytes: total_bytes + 1024 * 1024,
        };
        let writer = tokio_util::io::SyncIoBridge::new(writer);
        let writer = std::io::BufWriter::new(writer);
        let (writer, processed) = write_archive_entries_to_sink(
            entries,
            total_bytes,
            limits,
            writer,
            |_, _| Ok(()),
            |file| {
                let byte = if file.blob_id == 1 { 0xA5 } else { b'z' };
                let size = u64::try_from(file.size).map_err(|_| {
                    AsterError::internal_error("synthetic archive entry size must be non-negative")
                })?;
                Ok(Box::new(std::io::repeat(byte).take(size)))
            },
            None,
        )?;
        assert_eq!(processed, total_bytes);
        flush_archive_download_sink(writer, "synthetic archive download")
    }

    #[test]
    fn archive_folder_traversal_uses_remaining_entry_budget() {
        let limits = ArchiveBuildLimits {
            max_entries: 5,
            max_total_source_bytes: 1024,
            max_temp_bytes: 1024,
        };

        let traversal = limits.folder_tree_traversal_limits(2).unwrap();
        assert_eq!(traversal.maximum_resources, 3);
        assert_eq!(traversal.maximum_frontier, 3);
        assert_eq!(traversal.maximum_depth, 128);
        assert!(limits.folder_tree_traversal_limits(5).is_err());
        assert!(limits.folder_tree_traversal_limits(6).is_err());
    }

    #[test]
    fn archive_relative_dir_returns_empty_for_root_path() {
        assert_eq!(archive_relative_dir("/root", "/root").unwrap(), "");
    }

    #[test]
    fn archive_relative_dir_strips_root_with_path_components() {
        assert_eq!(
            archive_relative_dir("/root/nested/child", "/root").unwrap(),
            "nested/child"
        );
    }

    #[test]
    fn archive_relative_dir_rejects_shared_text_prefix_outside_root() {
        let error = archive_relative_dir("/rooted/child", "/root").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("folder path '/rooted/child' is outside root '/root'")
        );
    }

    #[test]
    fn archive_directory_entry_path_formats_root_directory() {
        assert_eq!(
            archive_directory_entry_path("archive", "/root", "/root").unwrap(),
            "archive/"
        );
    }

    #[test]
    fn archive_directory_entry_path_formats_nested_directory() {
        assert_eq!(
            archive_directory_entry_path("archive", "/root/nested/child", "/root").unwrap(),
            "archive/nested/child/"
        );
    }

    #[test]
    fn archive_directory_entry_path_rejects_path_outside_root() {
        let error = archive_directory_entry_path("archive", "/other/place", "/root").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("folder path '/other/place' is outside root '/root'")
        );
    }

    #[test]
    fn normalize_archive_zip_name_truncates_stem_before_suffix() {
        let name = normalize_archive_zip_name(
            &"a".repeat(aster_forge_validation::filename::MAX_FILENAME_LEN),
        )
        .unwrap();

        assert!(name.ends_with(".zip"));
        assert_eq!(
            name.len(),
            aster_forge_validation::filename::MAX_FILENAME_LEN
        );
        aster_forge_validation::filename::validate_name(&name).unwrap();
    }

    #[tokio::test]
    async fn archive_download_stream_finalizes_zip_larger_than_512_mib_with_bounded_buffers() {
        let temp = TempArchiveFile::new();
        let mut file = tokio::fs::File::create(&temp.0)
            .await
            .expect("large archive temp file should be created");
        let mut body = Box::pin(archive_download_body(
            "large-regression.zip".to_string(),
            move |writer| {
                write_synthetic_archive(writer, LARGE_STORED_ENTRY_BYTES, DEFLATED_ENTRY_BYTES)
            },
        ));

        while let Some(chunk) = body.next().await {
            file.write_all(&chunk.expect("archive body chunk should stream cleanly"))
                .await
                .expect("archive body chunk should be persisted");
        }
        file.flush()
            .await
            .expect("large archive temp file should flush");
        drop(file);

        let archive_size = std::fs::metadata(&temp.0)
            .expect("large archive metadata should load")
            .len();
        assert!(
            archive_size > 512 * 1024 * 1024,
            "regression archive must cross the reported failure boundary"
        );

        let path = temp.0.clone();
        tokio::task::spawn_blocking(move || {
            let file = std::fs::File::open(path).expect("large archive should reopen");
            let mut archive =
                zip::ZipArchive::new(file).expect("large archive central directory should parse");
            assert_eq!(archive.len(), 2);

            {
                let mut stored = archive
                    .by_name("stored.bin")
                    .expect("stored entry should be indexed");
                assert_eq!(stored.size(), LARGE_STORED_ENTRY_BYTES);
                assert_eq!(stored.compression(), zip::CompressionMethod::Stored);
                let mut prefix = [0_u8; 1];
                stored
                    .read_exact(&mut prefix)
                    .expect("stored entry should be readable");
                assert_eq!(prefix, [0xA5]);
            }
            {
                let mut deflated = archive
                    .by_name("deflated.txt")
                    .expect("deflated entry should be indexed");
                assert_eq!(deflated.size(), DEFLATED_ENTRY_BYTES);
                assert_eq!(deflated.compression(), zip::CompressionMethod::Deflated);
                let mut content = Vec::new();
                deflated
                    .read_to_end(&mut content)
                    .expect("deflated entry should pass decompression and CRC validation");
                assert_eq!(content.len() as u64, DEFLATED_ENTRY_BYTES);
                assert!(content.iter().all(|byte| *byte == b'z'));
            }
        })
        .await
        .expect("large archive validation worker should finish");
    }

    #[tokio::test]
    async fn archive_download_stream_surfaces_worker_error_after_partial_body() {
        let mut body = Box::pin(archive_download_body(
            "worker-error.zip".to_string(),
            |writer| {
                let mut writer = tokio_util::io::SyncIoBridge::new(writer);
                writer
                    .write_all(b"partial archive bytes")
                    .expect("partial bytes should enter the stream");
                writer.flush().expect("partial bytes should flush");
                Err(AsterError::storage_driver_error(
                    "synthetic archive finalization failure",
                ))
            },
        ));

        assert_eq!(
            body.next()
                .await
                .expect("partial chunk should exist")
                .expect("partial chunk should be readable"),
            bytes::Bytes::from_static(b"partial archive bytes")
        );
        let error = body
            .next()
            .await
            .expect("worker failure should be emitted")
            .expect_err("worker failure must not be presented as clean EOF");
        assert!(
            error
                .to_string()
                .contains("synthetic archive finalization failure")
        );
        assert!(body.next().await.is_none());
    }

    #[tokio::test]
    async fn archive_download_stream_surfaces_worker_panic() {
        let mut body = Box::pin(archive_download_body(
            "worker-panic.zip".to_string(),
            |_writer| -> crate::errors::Result<()> {
                panic!("synthetic archive worker panic");
            },
        ));

        let error = body
            .next()
            .await
            .expect("worker panic should be emitted")
            .expect_err("worker panic must not be presented as clean EOF");
        assert!(
            error
                .to_string()
                .contains("archive download worker terminated before clean completion")
        );
        assert!(body.next().await.is_none());
    }

    #[tokio::test]
    async fn dropping_archive_download_body_unblocks_backpressured_worker() {
        let (disconnect_tx, disconnect_rx) = tokio::sync::oneshot::channel();
        let mut body = Box::pin(archive_download_body(
            "client-disconnect.zip".to_string(),
            move |writer| {
                let mut writer = tokio_util::io::SyncIoBridge::new(writer);
                let chunk = [0_u8; 64 * 1024];
                loop {
                    if let Err(error) = writer.write_all(&chunk) {
                        let kind = error.kind();
                        let _ = disconnect_tx.send(kind);
                        return Err(AsterError::storage_driver_error(format!(
                            "archive client disconnected: {error}"
                        )));
                    }
                }
            },
        ));

        let first = body
            .next()
            .await
            .expect("worker should produce a chunk before disconnect")
            .expect("first chunk should be readable");
        assert_eq!(first.len(), 64 * 1024);
        drop(body);

        let kind = tokio::time::timeout(std::time::Duration::from_secs(2), disconnect_rx)
            .await
            .expect("backpressured worker should observe disconnect promptly")
            .expect("disconnect observation should be reported");
        assert_eq!(kind, std::io::ErrorKind::BrokenPipe);
    }
}
