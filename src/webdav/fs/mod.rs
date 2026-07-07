//! WebDAV 子模块：`fs`。

use std::{collections::HashMap, pin::Pin, time::Instant};

use futures::stream;
use tokio::io::AsyncRead;

use crate::db::repository::{file_repo, folder_repo, property_repo, team_repo, user_repo};
use crate::entities::{file, file_blob};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{
    audit_service::{self, AuditContext},
    files::{file as file_ops, folder},
    content::property, events::storage_change, webdav::tree,
    workspace::storage::WorkspaceStorageScope,
};
use crate::types::{EntityType, NullablePatch};
use crate::utils::numbers::i64_to_u64;
use crate::webdav::dav::{
    DavDirEntry, DavFile, DavFileSystem, DavMetaData, DavPath, DavProp, FsError, FsFuture,
    FsStream, OpenOptions, ReadDirMeta,
};
use crate::webdav::dir_entry::AsterDavDirEntry;
use crate::webdav::download_audit::{
    WebdavDownloadAuditIdentity, WebdavDownloadRequestKind, record_download,
};
use crate::webdav::file::AsterDavFile;
use crate::webdav::metadata::AsterDavMeta;
use crate::webdav::path_resolver::{self, ResolvedNode};

/// AsterDrive WebDAV 文件系统，per-account workspace 实例。
#[derive(Clone)]
pub struct AsterDavFs {
    state: PrimaryAppState,
    webdav_account_id: Option<i64>,
    scope: WorkspaceStorageScope,
    /// 限制访问范围：None = 用户全部文件，Some(id) = 只能访问该文件夹及子目录
    root_folder_id: Option<i64>,
    audit_ctx: AuditContext,
}

pub(crate) struct AsterDavDownloadFile {
    pub(crate) file: file::Model,
    pub(crate) blob: file_blob::Model,
    pub(crate) meta: AsterDavMeta,
}

impl std::fmt::Debug for AsterDavFs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsterDavFs")
            .field("scope", &self.scope)
            .field("root_folder_id", &self.root_folder_id)
            .finish()
    }
}

impl AsterDavFs {
    pub fn new(state: PrimaryAppState, user_id: i64, root_folder_id: Option<i64>) -> Self {
        Self::new_with_audit(
            state,
            None,
            WorkspaceStorageScope::Personal { user_id },
            root_folder_id,
            AuditContext {
                user_id,
                ip_address: None,
                user_agent: None,
            },
        )
    }

    pub(crate) fn new_with_audit(
        state: PrimaryAppState,
        webdav_account_id: Option<i64>,
        scope: WorkspaceStorageScope,
        root_folder_id: Option<i64>,
        audit_ctx: AuditContext,
    ) -> Self {
        Self {
            state,
            webdav_account_id,
            scope,
            root_folder_id,
            audit_ctx,
        }
    }

    fn app_state(&self) -> PrimaryAppState {
        self.state.clone()
    }

    fn scope(&self) -> WorkspaceStorageScope {
        self.scope
    }

    pub(crate) async fn resolve_download_target(
        &self,
        path: &DavPath,
    ) -> Result<Option<AsterDavDownloadFile>, FsError> {
        let node = path_resolver::resolve_path_cached_for_read_in_scope(
            &self.state,
            self.scope,
            path,
            self.root_folder_id,
        )
        .await?;

        let file = match node {
            ResolvedNode::Root | ResolvedNode::Folder(_) => {
                return Ok(None);
            }
            ResolvedNode::File(file) => file,
        };

        let blob = file_repo::find_blob_by_id(self.state.reader_db(), file.blob_id)
            .await
            .map_err(|_| FsError::GeneralFailure)?;
        let meta = AsterDavMeta::from_file(&file, &blob);

        Ok(Some(AsterDavDownloadFile { file, blob, meta }))
    }

    pub(crate) async fn open_download_stream_for_file(
        &self,
        file: &file::Model,
        blob: &file_blob::Model,
        offset: Option<u64>,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>, FsError> {
        let policy = self
            .state
            .policy_snapshot
            .get_policy(blob.policy_id)
            .ok_or(FsError::GeneralFailure)?;
        let driver = self
            .state
            .driver_registry
            .get_driver(&policy)
            .map_err(|_| FsError::GeneralFailure)?;

        let stream = match offset {
            Some(offset) => driver
                .get_range(&blob.storage_path, offset, length)
                .await
                .map_err(|_| FsError::NotFound)?,
            None => driver
                .get_stream(&blob.storage_path)
                .await
                .map_err(|_| FsError::NotFound)?,
        };
        record_download(
            &self.state,
            &self.audit_ctx,
            WebdavDownloadAuditIdentity {
                account_id: self.webdav_account_id,
                scope: self.scope,
                root_folder_id: self.root_folder_id,
            },
            file,
            match offset {
                Some(_) => WebdavDownloadRequestKind::Ranged,
                None => WebdavDownloadRequestKind::Full,
            },
        )
        .await;
        Ok(stream)
    }

    pub(crate) async fn copy_dir_shallow(
        &self,
        from: &DavPath,
        to: &DavPath,
    ) -> Result<(), FsError> {
        let node = path_resolver::resolve_path_cached_in_scope(
            &self.state,
            self.scope,
            from,
            self.root_folder_id,
        )
        .await?;
        let src_folder = match node {
            ResolvedNode::Folder(folder) => folder,
            _ => return Err(FsError::Forbidden),
        };

        let (dest_parent_id, dest_name) = path_resolver::resolve_parent_cached_in_scope(
            &self.state,
            self.scope,
            to,
            self.root_folder_id,
        )
        .await?;

        let state = self.app_state();
        delete_existing_destination_for_overwrite(
            &state,
            self.scope(),
            dest_parent_id,
            &dest_name,
            &self.audit_ctx,
        )
        .await?;

        let created = folder::create_in_scope_with_audit(
            &state,
            self.scope(),
            &dest_name,
            dest_parent_id,
            &self.audit_ctx,
        )
        .await
        .map_err(to_fs_error)?;
        copy_visible_entity_properties(
            &state,
            EntityType::Folder,
            src_folder.id,
            EntityType::Folder,
            created.id,
        )
        .await?;
        let target_folder = folder_repo::find_by_id(state.reader_db(), created.id)
            .await
            .map_err(to_fs_error)?;
        let details = folder::audit_transfer_details_for_models(
            &state,
            self.scope(),
            &src_folder,
            &target_folder,
        )
        .await;
        audit_service::log_with_details(
            &state,
            &self.audit_ctx,
            audit_service::AuditAction::FolderCopy,
            crate::services::audit_service::AuditEntityType::Folder,
            Some(created.id),
            Some(&created.name),
            || details.clone(),
        )
        .await;

        Ok(())
    }
}

impl DavFileSystem for AsterDavFs {
    fn open<'a>(
        &'a self,
        path: &'a DavPath,
        options: OpenOptions,
    ) -> FsFuture<'a, Box<dyn DavFile>> {
        Box::pin(async move {
            if options.write {
                // 写模式
                let (parent_id, filename) = path_resolver::resolve_parent_cached_in_scope(
                    &self.state,
                    self.scope,
                    path,
                    self.root_folder_id,
                )
                .await?;

                let existing_file =
                    find_file_by_name_in_scope(&self.state, self.scope, parent_id, &filename)
                        .await?;

                // WebDAV handler 会在入口处做 lock token 校验，
                // 这里不要再用 is_locked 把合法持锁写入挡掉。

                let existing_file_id = existing_file.map(|f| f.id);

                if options.create_new && existing_file_id.is_some() {
                    return Err(FsError::Exists);
                }
                if !options.create && !options.create_new && existing_file_id.is_none() {
                    return Err(FsError::NotFound);
                }

                let dav_file = AsterDavFile::for_write_with_audit(
                    self.app_state(),
                    self.scope,
                    parent_id,
                    filename,
                    existing_file_id,
                    options.size,
                    self.audit_ctx.clone(),
                )
                .await?;

                Ok(Box::new(dav_file) as Box<dyn DavFile>)
            } else {
                let _ = path;
                // 读路径只允许 GET/HEAD 通过专用下载目标访问，避免回退到临时文件兜底。
                Err(FsError::Forbidden)
            }
        })
    }

    fn read_dir<'a>(
        &'a self,
        path: &'a DavPath,
        _meta: ReadDirMeta,
    ) -> FsFuture<'a, FsStream<Box<dyn DavDirEntry>>> {
        Box::pin(async move {
            let started_at = Instant::now();
            let resolve_started_at = Instant::now();
            let folder_id = match path_resolver::resolve_path_cached_for_read_in_scope(
                &self.state,
                self.scope,
                path,
                self.root_folder_id,
            )
            .await?
            {
                ResolvedNode::Root => self.root_folder_id,
                ResolvedNode::Folder(f) => Some(f.id),
                ResolvedNode::File(_) => return Err(FsError::Forbidden),
            };
            let resolve_elapsed_ms = resolve_started_at.elapsed().as_millis();

            let folders_started_at = Instant::now();
            let folders = crate::services::workspace::storage::list_folders_in_parent(
                &self.state,
                self.scope,
                folder_id,
            )
            .await
            .map_err(|_| FsError::GeneralFailure)?;
            let folders_elapsed_ms = folders_started_at.elapsed().as_millis();

            let files_started_at = Instant::now();
            let files = crate::services::workspace::storage::list_files_in_folder(
                &self.state,
                self.scope,
                folder_id,
            )
            .await
            .map_err(|_| FsError::GeneralFailure)?;
            let files_elapsed_ms = files_started_at.elapsed().as_millis();

            let mut entries: Vec<Box<dyn DavDirEntry>> = Vec::new();

            let entry_started_at = Instant::now();
            for folder in &folders {
                entries.push(Box::new(AsterDavDirEntry::from_folder(folder)));
            }

            for file in &files {
                entries.push(Box::new(AsterDavDirEntry::from_file_record(file)));
            }
            let entry_elapsed_ms = entry_started_at.elapsed().as_millis();

            tracing::debug!(
                folder_id,
                folder_count = folders.len(),
                file_count = files.len(),
                entry_count = entries.len(),
                resolve_elapsed_ms,
                folders_elapsed_ms,
                files_elapsed_ms,
                entry_elapsed_ms,
                total_elapsed_ms = started_at.elapsed().as_millis(),
                "WebDAV read_dir completed"
            );

            Ok(Box::pin(stream::iter(entries.into_iter().map(Ok)))
                as FsStream<Box<dyn DavDirEntry>>)
        })
    }

    fn metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, Box<dyn DavMetaData>> {
        Box::pin(async move {
            let node = path_resolver::resolve_path_cached_for_read_in_scope(
                &self.state,
                self.scope,
                path,
                self.root_folder_id,
            )
            .await?;

            let meta: Box<dyn DavMetaData> = match node {
                ResolvedNode::Root => Box::new(AsterDavMeta::root()),
                ResolvedNode::Folder(f) => Box::new(AsterDavMeta::from_folder(&f)),
                ResolvedNode::File(f) => {
                    let blob = file_repo::find_blob_by_id(self.state.reader_db(), f.blob_id)
                        .await
                        .map_err(|_| FsError::GeneralFailure)?;
                    Box::new(AsterDavMeta::from_file(&f, &blob))
                }
            };

            Ok(meta)
        })
    }

    fn create_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        Box::pin(async move {
            let (parent_id, name) = path_resolver::resolve_parent_cached_in_scope(
                &self.state,
                self.scope,
                path,
                self.root_folder_id,
            )
            .await?;

            let state = self.app_state();
            folder::create_in_scope_with_audit(
                &state,
                self.scope(),
                &name,
                parent_id,
                &self.audit_ctx,
            )
            .await
            .map_err(to_fs_error)?;

            Ok(())
        })
    }

    fn remove_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        Box::pin(async move {
            let node = path_resolver::resolve_path_cached_in_scope(
                &self.state,
                self.scope,
                path,
                self.root_folder_id,
            )
            .await?;
            let folder = match node {
                ResolvedNode::Folder(f) => f,
                _ => return Err(FsError::Forbidden),
            };

            let state = self.app_state();
            let details =
                folder::audit_location_details_for_model(&state, self.scope, &folder).await;
            tree::recursive_soft_delete_in_scope(&state, self.scope, folder.id)
                .await
                .map_err(to_fs_error)?;
            audit_service::log_with_details(
                &state,
                &self.audit_ctx,
                audit_service::AuditAction::FolderDelete,
                crate::services::audit_service::AuditEntityType::Folder,
                Some(folder.id),
                Some(&folder.name),
                || details.clone(),
            )
            .await;

            Ok(())
        })
    }

    fn remove_file<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        Box::pin(async move {
            let node = path_resolver::resolve_path_cached_in_scope(
                &self.state,
                self.scope,
                path,
                self.root_folder_id,
            )
            .await?;
            let file = match node {
                ResolvedNode::File(f) => f,
                _ => return Err(FsError::Forbidden),
            };

            let state = self.app_state();
            file_ops::delete_in_scope_with_audit(&state, self.scope(), file.id, &self.audit_ctx)
                .await
                .map_err(to_fs_error)?;

            Ok(())
        })
    }

    fn rename<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<'a, ()> {
        Box::pin(async move {
            let node = path_resolver::resolve_path_cached_in_scope(
                &self.state,
                self.scope,
                from,
                self.root_folder_id,
            )
            .await?;

            let (dest_parent_id, dest_name) = path_resolver::resolve_parent_cached_in_scope(
                &self.state,
                self.scope,
                to,
                self.root_folder_id,
            )
            .await?;

            let state = self.app_state();
            delete_existing_destination_for_overwrite(
                &state,
                self.scope(),
                dest_parent_id,
                &dest_name,
                &self.audit_ctx,
            )
            .await?;

            match node {
                ResolvedNode::File(f) => {
                    file_ops::update_in_scope_with_audit(
                        &state,
                        self.scope(),
                        f.id,
                        Some(dest_name),
                        dest_parent_id.into(),
                        &self.audit_ctx,
                    )
                    .await
                    .map_err(to_fs_error)?;
                }
                ResolvedNode::Folder(f) => {
                    folder::update_in_scope_with_audit(
                        &state,
                        self.scope(),
                        f.id,
                        Some(dest_name),
                        dest_parent_id.into(),
                        NullablePatch::Absent,
                        &self.audit_ctx,
                    )
                    .await
                    .map_err(to_fs_error)?;
                }
                ResolvedNode::Root => return Err(FsError::Forbidden),
            }

            Ok(())
        })
    }

    fn copy<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<'a, ()> {
        Box::pin(async move {
            let node = path_resolver::resolve_path_cached_in_scope(
                &self.state,
                self.scope,
                from,
                self.root_folder_id,
            )
            .await?;
            let (dest_parent_id, dest_name) = path_resolver::resolve_parent_cached_in_scope(
                &self.state,
                self.scope,
                to,
                self.root_folder_id,
            )
            .await?;

            let state = self.app_state();
            delete_existing_destination_for_overwrite(
                &state,
                self.scope(),
                dest_parent_id,
                &dest_name,
                &self.audit_ctx,
            )
            .await?;

            match node {
                ResolvedNode::File(f) => {
                    let copied = file_ops::duplicate_file_record_in_scope(
                        &state,
                        self.scope(),
                        &f,
                        dest_parent_id,
                        &dest_name,
                    )
                    .await
                    .map_err(to_fs_error)?;
                    copy_visible_entity_properties(
                        &state,
                        EntityType::File,
                        f.id,
                        EntityType::File,
                        copied.id,
                    )
                    .await?;
                    storage_change::publish(
                        &state,
                        storage_change::StorageChangeEvent::new(
                            storage_change::StorageChangeKind::FileCreated,
                            self.scope(),
                            vec![copied.id],
                            vec![],
                            vec![copied.folder_id],
                        )
                        .with_storage_delta(copied.size),
                    );
                    let details = file_ops::audit_transfer_details_for_models(
                        &state,
                        self.scope(),
                        &f,
                        &copied,
                    )
                    .await;
                    audit_service::log_with_details(
                        &state,
                        &self.audit_ctx,
                        audit_service::AuditAction::FileCopy,
                        crate::services::audit_service::AuditEntityType::File,
                        Some(copied.id),
                        Some(&copied.name),
                        || details.clone(),
                    )
                    .await;
                }
                ResolvedNode::Folder(f) => {
                    let copied = tree::copy_folder_tree_in_scope(
                        &state,
                        self.scope,
                        f.id,
                        dest_parent_id,
                        &dest_name,
                    )
                    .await
                    .map_err(to_fs_error)?;
                    copy_visible_properties_for_copied_tree(&state, self.scope(), f.id, copied.id)
                        .await?;
                    let details = folder::audit_transfer_details_for_models(
                        &state,
                        self.scope(),
                        &f,
                        &copied,
                    )
                    .await;
                    audit_service::log_with_details(
                        &state,
                        &self.audit_ctx,
                        audit_service::AuditAction::FolderCopy,
                        crate::services::audit_service::AuditEntityType::Folder,
                        Some(copied.id),
                        Some(&copied.name),
                        || details.clone(),
                    )
                    .await;
                }
                ResolvedNode::Root => return Err(FsError::Forbidden),
            }

            Ok(())
        })
    }

    fn get_quota(&self) -> FsFuture<'_, (u64, Option<u64>)> {
        Box::pin(async move {
            let (storage_used, storage_quota) = match self.scope {
                WorkspaceStorageScope::Personal { user_id } => {
                    let user = user_repo::find_by_id(self.state.reader_db(), user_id)
                        .await
                        .map_err(|_| FsError::GeneralFailure)?;
                    (user.storage_used, user.storage_quota)
                }
                WorkspaceStorageScope::Team { team_id, .. } => {
                    let team = team_repo::find_by_id(self.state.reader_db(), team_id)
                        .await
                        .map_err(|_| FsError::GeneralFailure)?;
                    (team.storage_used, team.storage_quota)
                }
            };

            let used = i64_to_u64(storage_used, "webdav storage_used")
                .map_err(|_| FsError::GeneralFailure)?;
            let total = if storage_quota > 0 {
                Some(
                    i64_to_u64(storage_quota, "webdav storage_quota")
                        .map_err(|_| FsError::GeneralFailure)?,
                )
            } else {
                None // 无限
            };

            Ok((used, total))
        })
    }

    fn have_props<'a>(
        &'a self,
        path: &'a DavPath,
    ) -> Pin<Box<dyn std::future::Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            let (entity_type, entity_id) =
                match resolve_entity_for_read(&self.state, self.scope, path, self.root_folder_id)
                    .await
                {
                    Some(v) => v,
                    None => return false,
                };
            property_repo::find_by_entity(self.state.reader_db(), entity_type, entity_id)
                .await
                .map(|props| {
                    props
                        .iter()
                        .any(|prop| !property::is_protected_namespace(&prop.namespace))
                })
                .unwrap_or(false)
        })
    }

    fn get_props<'a>(&'a self, path: &'a DavPath, do_content: bool) -> FsFuture<'a, Vec<DavProp>> {
        Box::pin(async move {
            let (entity_type, entity_id) =
                resolve_entity_for_read(&self.state, self.scope, path, self.root_folder_id)
                    .await
                    .ok_or(FsError::NotFound)?;

            let props =
                property_repo::find_by_entity(self.state.reader_db(), entity_type, entity_id)
                    .await
                    .map_err(|_| FsError::GeneralFailure)?;

            Ok(entity_props_to_dav_props(props, do_content))
        })
    }

    fn get_props_many<'a>(
        &'a self,
        paths: &'a [DavPath],
        do_content: bool,
    ) -> FsFuture<'a, HashMap<DavPath, Vec<DavProp>>> {
        Box::pin(async move {
            let mut target_paths: HashMap<(EntityType, i64), Vec<DavPath>> = HashMap::new();
            let mut targets = Vec::new();
            for path in paths {
                let Some(target) =
                    resolve_entity_for_read(&self.state, self.scope, path, self.root_folder_id)
                        .await
                else {
                    continue;
                };
                target_paths.entry(target).or_default().push(path.clone());
                targets.push(target);
            }

            let props = property_repo::find_by_entities(self.state.reader_db(), &targets)
                .await
                .map_err(|_| FsError::GeneralFailure)?;
            let mut props_by_target: HashMap<(EntityType, i64), Vec<DavProp>> = HashMap::new();
            for prop in props {
                if property::is_protected_namespace(&prop.namespace) {
                    continue;
                }
                props_by_target
                    .entry((prop.entity_type, prop.entity_id))
                    .or_default()
                    .push(entity_prop_to_dav_prop(prop, do_content));
            }

            let mut result = HashMap::with_capacity(paths.len());
            for (target, paths) in target_paths {
                let props = props_by_target.remove(&target).unwrap_or_default();
                for path in paths {
                    result.insert(path, props.clone());
                }
            }
            Ok(result)
        })
    }

    fn get_props_many_for_entities<'a>(
        &'a self,
        targets: &'a [(DavPath, EntityType, i64)],
        do_content: bool,
    ) -> FsFuture<'a, HashMap<DavPath, Vec<DavProp>>> {
        Box::pin(async move {
            let mut target_paths: HashMap<(EntityType, i64), Vec<DavPath>> = HashMap::new();
            let mut entity_targets = Vec::with_capacity(targets.len());
            for (path, entity_type, entity_id) in targets {
                let target = (*entity_type, *entity_id);
                target_paths.entry(target).or_default().push(path.clone());
                entity_targets.push(target);
            }

            let props = property_repo::find_by_entities(self.state.reader_db(), &entity_targets)
                .await
                .map_err(|_| FsError::GeneralFailure)?;
            let mut props_by_target: HashMap<(EntityType, i64), Vec<DavProp>> = HashMap::new();
            for prop in props {
                if property::is_protected_namespace(&prop.namespace) {
                    continue;
                }
                props_by_target
                    .entry((prop.entity_type, prop.entity_id))
                    .or_default()
                    .push(entity_prop_to_dav_prop(prop, do_content));
            }

            let mut result = HashMap::with_capacity(targets.len());
            for (target, paths) in target_paths {
                let props = props_by_target.remove(&target).unwrap_or_default();
                for path in paths {
                    result.insert(path, props.clone());
                }
            }
            Ok(result)
        })
    }

    fn patch_props<'a>(
        &'a self,
        path: &'a DavPath,
        patches: Vec<(bool, DavProp)>,
    ) -> FsFuture<'a, Vec<(http::StatusCode, DavProp)>> {
        Box::pin(async move {
            let (entity_type, entity_id) =
                resolve_entity(&self.state, self.scope, path, self.root_folder_id)
                    .await
                    .ok_or(FsError::NotFound)?;

            let mut protected_failure = false;
            for (_, prop) in &patches {
                let ns = prop.namespace.as_deref().unwrap_or("");
                if property::is_protected_namespace(ns) {
                    protected_failure = true;
                    break;
                }
            }

            if protected_failure {
                return Ok(patches
                    .into_iter()
                    .map(|(_, prop)| {
                        let ns = prop.namespace.as_deref().unwrap_or("");
                        let status = if property::is_protected_namespace(ns) {
                            http::StatusCode::FORBIDDEN
                        } else {
                            http::StatusCode::FAILED_DEPENDENCY
                        };
                        (status, prop)
                    })
                    .collect());
            }

            let txn = crate::db::transaction::begin(self.state.writer_db())
                .await
                .map_err(|_| FsError::GeneralFailure)?;

            for (set, prop) in &patches {
                let ns = prop.namespace.as_deref().unwrap_or("");
                if *set {
                    let value = prop.xml.as_ref().map(|x| String::from_utf8_lossy(x));
                    property_repo::upsert(
                        &txn,
                        entity_type,
                        entity_id,
                        ns,
                        &prop.name,
                        value.as_deref(),
                    )
                    .await
                    .map_err(|_| FsError::GeneralFailure)?;
                } else {
                    property_repo::delete_prop(&txn, entity_type, entity_id, ns, &prop.name)
                        .await
                        .map_err(|_| FsError::GeneralFailure)?;
                }
            }

            crate::db::transaction::commit(txn)
                .await
                .map_err(|_| FsError::GeneralFailure)?;

            for (set, prop) in &patches {
                let ns = prop.namespace.as_deref().unwrap_or("");
                let entity_type_label = entity_type.as_str();
                audit_service::log_with_details(
                    &self.state,
                    &self.audit_ctx,
                    if *set {
                        audit_service::AuditAction::PropertySet
                    } else {
                        audit_service::AuditAction::PropertyDelete
                    },
                    audit_service::AuditEntityType::from_entity_type(entity_type),
                    Some(entity_id),
                    None,
                    || {
                        audit_service::details(audit_service::PropertyAuditDetails {
                            entity_type: entity_type_label,
                            namespace: ns,
                            name: &prop.name,
                        })
                    },
                )
                .await;
            }

            Ok(patches
                .into_iter()
                .map(|(_, prop)| (http::StatusCode::OK, prop))
                .collect())
        })
    }
}

fn entity_props_to_dav_props(
    props: Vec<crate::entities::entity_property::Model>,
    do_content: bool,
) -> Vec<DavProp> {
    props
        .into_iter()
        .filter(|p| !property::is_protected_namespace(&p.namespace))
        .map(|p| entity_prop_to_dav_prop(p, do_content))
        .collect()
}

fn entity_prop_to_dav_prop(
    prop: crate::entities::entity_property::Model,
    do_content: bool,
) -> DavProp {
    DavProp {
        name: prop.name,
        prefix: None,
        namespace: if prop.namespace.is_empty() {
            None
        } else {
            Some(prop.namespace)
        },
        xml: if do_content {
            prop.value.map(|value| value.into_bytes())
        } else {
            None
        },
    }
}

/// 从 DavPath 解析出 (entity_type, entity_id)
async fn resolve_entity(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    path: &DavPath,
    root_folder_id: Option<i64>,
) -> Option<(EntityType, i64)> {
    match path_resolver::resolve_path_cached_in_scope(state, scope, path, root_folder_id).await {
        Ok(ResolvedNode::File(f)) => Some((EntityType::File, f.id)),
        Ok(ResolvedNode::Folder(f)) => Some((EntityType::Folder, f.id)),
        _ => None,
    }
}

async fn resolve_entity_for_read(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    path: &DavPath,
    root_folder_id: Option<i64>,
) -> Option<(EntityType, i64)> {
    match path_resolver::resolve_path_cached_for_read_in_scope(state, scope, path, root_folder_id)
        .await
    {
        Ok(ResolvedNode::File(f)) => Some((EntityType::File, f.id)),
        Ok(ResolvedNode::Folder(f)) => Some((EntityType::Folder, f.id)),
        _ => None,
    }
}

async fn copy_visible_entity_properties(
    state: &PrimaryAppState,
    src_entity_type: EntityType,
    src_entity_id: i64,
    dest_entity_type: EntityType,
    dest_entity_id: i64,
) -> Result<(), FsError> {
    let props = property_repo::find_by_entity(state.writer_db(), src_entity_type, src_entity_id)
        .await
        .map_err(|_| FsError::GeneralFailure)?;

    for prop in props {
        if property::is_protected_namespace(&prop.namespace) {
            continue;
        }
        property_repo::upsert(
            state.writer_db(),
            dest_entity_type,
            dest_entity_id,
            &prop.namespace,
            &prop.name,
            prop.value.as_deref(),
        )
        .await
        .map_err(|_| FsError::GeneralFailure)?;
    }

    Ok(())
}

async fn load_child_folders_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    parent_ids: &[i64],
) -> Result<Vec<crate::entities::folder::Model>, FsError> {
    match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            folder_repo::find_children_in_parents(state.writer_db(), user_id, parent_ids).await
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            folder_repo::find_team_children_in_parents(state.writer_db(), team_id, parent_ids).await
        }
    }
    .map_err(|_| FsError::GeneralFailure)
}

async fn load_files_in_folders_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_ids: &[i64],
) -> Result<Vec<crate::entities::file::Model>, FsError> {
    match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            file_repo::find_by_folders(state.writer_db(), user_id, folder_ids).await
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            file_repo::find_by_team_folders(state.writer_db(), team_id, folder_ids).await
        }
    }
    .map_err(|_| FsError::GeneralFailure)
}

async fn copy_visible_properties_for_copied_tree(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    src_root_id: i64,
    dest_root_id: i64,
) -> Result<(), FsError> {
    let mut frontier = vec![(src_root_id, dest_root_id)];

    while !frontier.is_empty() {
        for (src_folder_id, dest_folder_id) in &frontier {
            copy_visible_entity_properties(
                state,
                EntityType::Folder,
                *src_folder_id,
                EntityType::Folder,
                *dest_folder_id,
            )
            .await?;
        }

        let src_folder_ids: Vec<i64> = frontier.iter().map(|(src, _)| *src).collect();
        let dest_folder_ids: Vec<i64> = frontier.iter().map(|(_, dest)| *dest).collect();
        let dest_parent_by_src: HashMap<i64, i64> = frontier.iter().copied().collect();

        let (src_files, dest_files, src_children, dest_children) = tokio::try_join!(
            load_files_in_folders_in_scope(state, scope, &src_folder_ids),
            load_files_in_folders_in_scope(state, scope, &dest_folder_ids),
            load_child_folders_in_scope(state, scope, &src_folder_ids),
            load_child_folders_in_scope(state, scope, &dest_folder_ids),
        )?;

        let dest_file_by_parent_and_name: HashMap<(i64, String), i64> = dest_files
            .into_iter()
            .filter_map(|file| {
                file.folder_id
                    .map(|folder_id| ((folder_id, file.name), file.id))
            })
            .collect();

        for src_file in src_files {
            let Some(src_parent_id) = src_file.folder_id else {
                return Err(FsError::GeneralFailure);
            };
            let Some(dest_parent_id) = dest_parent_by_src.get(&src_parent_id).copied() else {
                return Err(FsError::GeneralFailure);
            };
            let Some(dest_file_id) = dest_file_by_parent_and_name
                .get(&(dest_parent_id, src_file.name.clone()))
                .copied()
            else {
                return Err(FsError::GeneralFailure);
            };
            copy_visible_entity_properties(
                state,
                EntityType::File,
                src_file.id,
                EntityType::File,
                dest_file_id,
            )
            .await?;
        }

        let dest_child_by_parent_and_name: HashMap<(i64, String), i64> = dest_children
            .into_iter()
            .filter_map(|folder| {
                folder
                    .parent_id
                    .map(|parent_id| ((parent_id, folder.name), folder.id))
            })
            .collect();

        let mut next_frontier = Vec::with_capacity(src_children.len());
        for src_child in src_children {
            let Some(src_parent_id) = src_child.parent_id else {
                return Err(FsError::GeneralFailure);
            };
            let Some(dest_parent_id) = dest_parent_by_src.get(&src_parent_id).copied() else {
                return Err(FsError::GeneralFailure);
            };
            let Some(dest_child_id) = dest_child_by_parent_and_name
                .get(&(dest_parent_id, src_child.name.clone()))
                .copied()
            else {
                return Err(FsError::GeneralFailure);
            };
            next_frontier.push((src_child.id, dest_child_id));
        }

        frontier = next_frontier;
    }

    Ok(())
}

async fn find_file_by_name_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_id: Option<i64>,
    name: &str,
) -> Result<Option<crate::entities::file::Model>, FsError> {
    match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            file_repo::find_by_name_in_folder(state.writer_db(), user_id, folder_id, name).await
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            file_repo::find_by_name_in_team_folder(state.writer_db(), team_id, folder_id, name)
                .await
        }
    }
    .map_err(|_| FsError::GeneralFailure)
}

async fn find_folder_by_name_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    parent_id: Option<i64>,
    name: &str,
) -> Result<Option<crate::entities::folder::Model>, FsError> {
    match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            folder_repo::find_by_name_in_parent(state.writer_db(), user_id, parent_id, name).await
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            folder_repo::find_by_name_in_team_parent(state.writer_db(), team_id, parent_id, name)
                .await
        }
    }
    .map_err(|_| FsError::GeneralFailure)
}

async fn delete_existing_destination_for_overwrite(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    parent_id: Option<i64>,
    name: &str,
    audit_ctx: &AuditContext,
) -> Result<(), FsError> {
    if let Some(existing) = find_file_by_name_in_scope(state, scope, parent_id, name).await? {
        let details = file_ops::audit_location_details_for_model(state, scope, &existing).await;
        file_repo::soft_delete(state.writer_db(), existing.id)
            .await
            .map_err(to_fs_error)?;
        storage_change::publish(
            state,
            storage_change::StorageChangeEvent::new(
                storage_change::StorageChangeKind::FileTrashed,
                scope,
                vec![existing.id],
                vec![],
                vec![existing.folder_id],
            ),
        );
        audit_service::log_with_details(
            state,
            audit_ctx,
            audit_service::AuditAction::FileDelete,
            crate::services::audit_service::AuditEntityType::File,
            Some(existing.id),
            Some(&existing.name),
            || details.clone(),
        )
        .await;
    }

    if let Some(existing) = find_folder_by_name_in_scope(state, scope, parent_id, name).await? {
        let details = folder::audit_location_details_for_model(state, scope, &existing).await;
        tree::recursive_soft_delete_in_scope(state, scope, existing.id)
            .await
            .map_err(to_fs_error)?;
        audit_service::log_with_details(
            state,
            audit_ctx,
            audit_service::AuditAction::FolderDelete,
            crate::services::audit_service::AuditEntityType::Folder,
            Some(existing.id),
            Some(&existing.name),
            || details.clone(),
        )
        .await;
    }

    Ok(())
}

/// AsterError → FsError 映射
fn to_fs_error(err: crate::errors::AsterError) -> FsError {
    match &err {
        crate::errors::AsterError::FileNotFound(_)
        | crate::errors::AsterError::FolderNotFound(_)
        | crate::errors::AsterError::RecordNotFound(_) => FsError::NotFound,

        crate::errors::AsterError::AuthForbidden(_) => FsError::Forbidden,

        crate::errors::AsterError::StorageQuotaExceeded(_) => FsError::InsufficientStorage,

        crate::errors::AsterError::FileTooLarge(_) => FsError::TooLarge,

        _ if file_repo::is_any_duplicate_name_error(&err)
            || folder_repo::is_any_duplicate_name_error(&err) =>
        {
            FsError::Exists
        }

        crate::errors::AsterError::ResourceLocked(_) => FsError::Forbidden,

        _ => FsError::GeneralFailure,
    }
}
