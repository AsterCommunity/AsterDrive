//! 归档任务子模块：`selection`。

use std::{
    collections::{HashMap, HashSet},
    path::{Component, Path},
};

use actix_web::HttpResponse;
use chrono::Utc;

use super::common::{
    ArchiveEntry, ArchiveFileEntry, ArchiveSinkContext, ends_with_ignore_ascii_case,
    is_client_disconnect_error_text, write_archive_to_sink,
};
use crate::config::operations;
use crate::db::repository::{file_repo, folder_repo};
use crate::entities::{file, folder, share};
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{
    batch_service,
    download_headers::DownloadDisposition,
    folder_service, share_service,
    task_service::types::CreateArchiveTaskParams,
    workspace_storage_service::{self, WorkspaceResourceScope, WorkspaceStorageScope},
};

pub(crate) struct PreparedArchiveDownload {
    pub file_ids: Vec<i64>,
    pub folder_ids: Vec<i64>,
    pub archive_name: String,
}

pub(super) struct ResolvedArchiveDownload {
    pub(super) selection: batch_service::NormalizedSelection,
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

    let (reader, writer) = tokio::io::duplex(64 * 1024);
    let handle = tokio::runtime::Handle::current();
    let db = state.writer_db().clone();
    let driver_registry = state.driver_registry().clone();
    let policy_snapshot = state.policy_snapshot().clone();
    let archive_name_for_worker = archive_name.clone();

    drop(tokio::task::spawn_blocking(move || {
        let writer = tokio_util::io::SyncIoBridge::new(writer);
        let writer = std::io::BufWriter::new(writer);
        if let Err(error) = write_archive_to_sink(
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
        ) {
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
    }));

    let reader_stream = tokio_util::io::ReaderStream::with_capacity(reader, 64 * 1024);

    Ok(HttpResponse::Ok()
        .content_type("application/zip")
        .insert_header((
            "Content-Disposition",
            DownloadDisposition::Attachment.header_value(&archive_name),
        ))
        .insert_header(("Content-Encoding", "identity"))
        .streaming(reader_stream))
}

pub(crate) async fn stream_shared_archive_download(
    state: &PrimaryAppState,
    token: &str,
    params: CreateArchiveTaskParams,
) -> Result<HttpResponse> {
    let resolved = resolve_shared_archive_download(state, token, &params).await?;
    let archive_name = resolved.archive_name.clone();
    let limits = ArchiveBuildLimits::from_runtime_config(state.runtime_config());
    share_service::reserve_share_download_count(state, &resolved.share).await?;
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
            share_service::rollback_share_download_count(state, resolved.share.id).await;
            return Err(error);
        }
    };
    let total_bytes = collected.total_source_bytes();

    let (reader, writer) = tokio::io::duplex(64 * 1024);
    let handle = tokio::runtime::Handle::current();
    let db = state.writer_db().clone();
    let driver_registry = state.driver_registry().clone();
    let policy_snapshot = state.policy_snapshot().clone();
    let rollback_queue = state.share_download_rollback.clone();
    let share_id = resolved.share.id;
    let archive_name_for_worker = archive_name.clone();

    drop(tokio::task::spawn_blocking(move || {
        let writer = tokio_util::io::SyncIoBridge::new(writer);
        let writer = std::io::BufWriter::new(writer);
        if let Err(error) = write_archive_to_sink(
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
        ) {
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
    }));

    let reader_stream = tokio_util::io::ReaderStream::with_capacity(reader, 64 * 1024);

    Ok(HttpResponse::Ok()
        .content_type("application/zip")
        .insert_header((
            "Content-Disposition",
            format!(r#"attachment; filename="{}""#, archive_name),
        ))
        .insert_header(("Content-Encoding", "identity"))
        .streaming(reader_stream))
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
    let selection = batch_service::load_normalized_selection_in_scope(
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
    selection: batch_service::NormalizedSelection,
    archive_name: String,
    share: share::Model,
    scope: WorkspaceResourceScope,
}

async fn resolve_shared_archive_download(
    state: &impl SharedRuntimeState,
    token: &str,
    params: &CreateArchiveTaskParams,
) -> Result<ResolvedSharedArchiveDownload> {
    batch_service::validate_batch_ids(&params.file_ids, &params.folder_ids)?;
    let (share, root_folder_id) = share_service::load_valid_folder_share_root(state, token).await?;
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
    workspace_storage_service::require_scope_access_with_db(state, state.writer_db(), scope)
        .await?;
    batch_service::validate_batch_ids(file_ids, folder_ids)?;

    let file_map: HashMap<i64, file::Model> = file_repo::find_by_ids(state.writer_db(), file_ids)
        .await?
        .into_iter()
        .map(|file| (file.id, file))
        .collect();
    for &file_id in file_ids {
        let file = file_map
            .get(&file_id)
            .ok_or_else(|| AsterError::file_not_found(format!("file #{file_id}")))?;
        workspace_storage_service::ensure_active_file_scope(file, scope)?;
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
        workspace_storage_service::ensure_active_folder_scope(folder, scope)?;
    }

    Ok(())
}

async fn load_normalized_shared_selection(
    state: &impl SharedRuntimeState,
    scope: WorkspaceResourceScope,
    root_folder_id: i64,
    file_ids: &[i64],
    folder_ids: &[i64],
) -> Result<batch_service::NormalizedSelection> {
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

    Ok(batch_service::NormalizedSelection {
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
    workspace_storage_service::ensure_file_resource_scope(file, scope)?;
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
        folder_service::verify_folder_in_scope(state.writer_db(), folder_id, root_folder_id)
            .await?;
    }
    Ok(())
}

async fn ensure_shared_folder_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceResourceScope,
    root_folder_id: i64,
    folder: &folder::Model,
) -> Result<()> {
    workspace_storage_service::ensure_folder_resource_scope(folder, scope)?;
    if folder.deleted_at.is_some() {
        return Err(AsterError::folder_not_found(format!(
            "folder #{} is in trash",
            folder.id
        )));
    }
    folder_service::verify_folder_in_scope(state.writer_db(), folder.id, root_folder_id).await
}

pub(super) fn ensure_archive_selection_active(
    scope: WorkspaceStorageScope,
    selection: &batch_service::NormalizedSelection,
) -> Result<()> {
    for &file_id in &selection.file_ids {
        let file = selection
            .file_map
            .get(&file_id)
            .ok_or_else(|| AsterError::file_not_found(format!("file #{file_id}")))?;
        workspace_storage_service::ensure_active_file_scope(file, scope)?;
    }

    for &folder_id in &selection.folder_ids {
        let folder = selection
            .folder_map
            .get(&folder_id)
            .ok_or_else(|| AsterError::folder_not_found(format!("folder #{folder_id}")))?;
        workspace_storage_service::ensure_active_folder_scope(folder, scope)?;
    }

    Ok(())
}

async fn collect_archive_entries_from_shared_selection(
    state: &impl SharedRuntimeState,
    scope: WorkspaceResourceScope,
    selection: &batch_service::NormalizedSelection,
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
        let entry_path = batch_service::reserve_unique_name(&mut reserved_root_names, &file.name);
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
        let archive_root =
            batch_service::reserve_unique_name(&mut reserved_root_names, &folder.name);

        let (tree_files, tree_folder_ids) = folder_service::collect_folder_tree_in_resource_scope(
            state.writer_db(),
            scope,
            folder_id,
            false,
        )
        .await?;
        let folder_paths =
            folder_service::build_folder_paths_cached(state, &tree_folder_ids).await?;
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
    selection: &batch_service::NormalizedSelection,
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
        workspace_storage_service::ensure_active_file_scope(file, scope)?;
        let entry_path = batch_service::reserve_unique_name(&mut reserved_root_names, &file.name);
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
        workspace_storage_service::ensure_active_folder_scope(folder, scope)?;
        let archive_root =
            batch_service::reserve_unique_name(&mut reserved_root_names, &folder.name);

        let (tree_files, tree_folder_ids) = folder_service::collect_folder_tree_in_scope(
            state.writer_db(),
            scope,
            folder_id,
            false,
        )
        .await?;
        let folder_paths =
            folder_service::build_folder_paths_cached(state, &tree_folder_ids).await?;
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
        crate::utils::numbers::usize_to_i64(entry_path.len(), "archive entry path bytes")?;
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
    selection: &batch_service::NormalizedSelection,
    requested_target_folder_id: Option<i64>,
) -> Result<Option<i64>> {
    if let Some(target_folder_id) = requested_target_folder_id {
        workspace_storage_service::verify_folder_access(state, scope, target_folder_id).await?;
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
    selection: &batch_service::NormalizedSelection,
) -> Result<String> {
    let base = match archive_name.as_deref().map(str::trim) {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => default_archive_name(selection),
    };
    let final_name = normalize_archive_zip_name(&base)?;
    crate::utils::validate_name(&final_name)?;
    Ok(final_name)
}

fn normalize_archive_zip_name(base: &str) -> Result<String> {
    if ends_with_ignore_ascii_case(base, ".zip") {
        return crate::utils::normalize_validate_name(base);
    }

    let max_stem_len = crate::utils::MAX_FILENAME_LEN
        .checked_sub(".zip".len())
        .ok_or_else(|| AsterError::internal_error("archive name length limit is too small"))?;
    let stem = crate::utils::normalize_name(base);
    let stem = crate::utils::truncate_utf8_to_max_bytes(&stem, max_stem_len);
    let stem = stem.trim_end_matches([' ', '.']);
    if stem.is_empty() {
        return Err(AsterError::validation_error("name cannot be empty"));
    }
    Ok(format!("{stem}.zip"))
}

fn default_archive_name(selection: &batch_service::NormalizedSelection) -> String {
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
    use super::{archive_directory_entry_path, archive_relative_dir, normalize_archive_zip_name};

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
        let name = normalize_archive_zip_name(&"a".repeat(crate::utils::MAX_FILENAME_LEN)).unwrap();

        assert!(name.ends_with(".zip"));
        assert_eq!(name.len(), crate::utils::MAX_FILENAME_LEN);
        crate::utils::validate_name(&name).unwrap();
    }
}
