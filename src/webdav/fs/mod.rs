//! WebDAV 子模块：`fs`。

use std::pin::Pin;

use futures::stream;
use tokio::io::AsyncRead;

use crate::db::repository::{file_repo, folder_repo, property_repo, team_repo, user_repo};
use crate::runtime::PrimaryAppState;
use crate::services::{
    audit_service::{self, AuditContext},
    file_service, folder_service, property_service, webdav_service,
    workspace_storage_service::WorkspaceStorageScope,
};
use crate::types::{EntityType, NullablePatch};
use crate::utils::numbers::i64_to_u64;
use crate::webdav::dav::{
    DavDirEntry, DavFile, DavFileSystem, DavMetaData, DavPath, DavProp, FsError, FsFuture,
    FsStream, OpenOptions, ReadDirMeta,
};
use crate::webdav::dir_entry::AsterDavDirEntry;
use crate::webdav::file::AsterDavFile;
use crate::webdav::metadata::AsterDavMeta;
use crate::webdav::path_resolver::{self, ResolvedNode};

/// AsterDrive WebDAV 文件系统，per-account workspace 实例。
#[derive(Clone)]
pub struct AsterDavFs {
    state: PrimaryAppState,
    scope: WorkspaceStorageScope,
    /// 限制访问范围：None = 用户全部文件，Some(id) = 只能访问该文件夹及子目录
    root_folder_id: Option<i64>,
    audit_ctx: AuditContext,
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
        scope: WorkspaceStorageScope,
        root_folder_id: Option<i64>,
        audit_ctx: AuditContext,
    ) -> Self {
        Self {
            state,
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

    pub(crate) async fn open_read_stream(
        &self,
        path: &DavPath,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>, FsError> {
        self.open_read_stream_with_range(path, None, None).await
    }

    pub(crate) async fn open_read_stream_with_range(
        &self,
        path: &DavPath,
        offset: Option<u64>,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>, FsError> {
        let node = path_resolver::resolve_path_cached_in_scope(
            &self.state,
            self.scope,
            path,
            self.root_folder_id,
        )
        .await?;

        let file = match node {
            ResolvedNode::File(file) => file,
            _ => return Err(FsError::Forbidden),
        };

        let blob = file_repo::find_blob_by_id(self.state.writer_db(), file.blob_id)
            .await
            .map_err(|_| FsError::GeneralFailure)?;
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
        audit_service::log(
            &self.state,
            &self.audit_ctx,
            audit_service::AuditAction::FileDownload,
            crate::services::audit_service::AuditEntityType::File,
            Some(file.id),
            Some(&file.name),
            Some(serde_json::json!({ "source": "webdav" })),
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
        let created = folder_service::create_in_scope_with_audit(
            &state,
            self.scope(),
            &dest_name,
            dest_parent_id,
            &self.audit_ctx,
        )
        .await
        .map_err(to_fs_error)?;
        audit_service::log(
            &state,
            &self.audit_ctx,
            audit_service::AuditAction::FolderCopy,
            crate::services::audit_service::AuditEntityType::Folder,
            Some(created.id),
            Some(&created.name),
            Some(serde_json::json!({
                "source": "webdav",
                "depth": "0",
                "source_folder_id": src_folder.id,
            })),
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
                // 读路径只允许 GET/HEAD 通过 open_read_stream 访问，避免回退到临时文件兜底。
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
            let folder_id = match path_resolver::resolve_path_cached_in_scope(
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

            let folders = crate::services::workspace_storage_service::list_folders_in_parent(
                &self.state,
                self.scope,
                folder_id,
            )
            .await
            .map_err(|_| FsError::GeneralFailure)?;
            let files = crate::services::workspace_storage_service::list_files_in_folder(
                &self.state,
                self.scope,
                folder_id,
            )
            .await
            .map_err(|_| FsError::GeneralFailure)?;

            let mut entries: Vec<Box<dyn DavDirEntry>> = Vec::new();

            for folder in &folders {
                entries.push(Box::new(AsterDavDirEntry::from_folder(folder)));
            }

            // 批量查询所有 blob（1 次查询替代 N 次）
            let blob_ids: Vec<i64> = files.iter().map(|f| f.blob_id).collect();
            let blobs = file_repo::find_blobs_by_ids(self.state.writer_db(), &blob_ids)
                .await
                .map_err(|_| FsError::GeneralFailure)?;

            for file in &files {
                if let Some(blob) = blobs.get(&file.blob_id) {
                    entries.push(Box::new(AsterDavDirEntry::from_file(file, blob)));
                }
            }

            Ok(Box::pin(stream::iter(entries.into_iter().map(Ok)))
                as FsStream<Box<dyn DavDirEntry>>)
        })
    }

    fn metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, Box<dyn DavMetaData>> {
        Box::pin(async move {
            let node = path_resolver::resolve_path_cached_in_scope(
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
                    let blob = file_repo::find_blob_by_id(self.state.writer_db(), f.blob_id)
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
            folder_service::create_in_scope_with_audit(
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
            webdav_service::recursive_soft_delete_in_scope(&state, self.scope, folder.id)
                .await
                .map_err(to_fs_error)?;
            audit_service::log(
                &state,
                &self.audit_ctx,
                audit_service::AuditAction::FolderDelete,
                crate::services::audit_service::AuditEntityType::Folder,
                Some(folder.id),
                Some(&folder.name),
                Some(serde_json::json!({ "source": "webdav" })),
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
            file_service::delete_in_scope_with_audit(
                &state,
                self.scope(),
                file.id,
                &self.audit_ctx,
            )
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

            match node {
                ResolvedNode::File(f) => {
                    // 如果目标已有同名文件，先删除（WebDAV MOVE 覆盖语义）
                    if let Some(existing) = find_file_by_name_in_scope(
                        &self.state,
                        self.scope,
                        dest_parent_id,
                        &dest_name,
                    )
                    .await?
                    {
                        file_service::delete_in_scope_with_audit(
                            &state,
                            self.scope(),
                            existing.id,
                            &self.audit_ctx,
                        )
                        .await
                        .map_err(to_fs_error)?;
                    }

                    file_service::update_in_scope_with_audit(
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
                    folder_service::update_in_scope_with_audit(
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

            match node {
                ResolvedNode::File(f) => {
                    // WebDAV COPY 覆盖语义：目标已存在先删除
                    if let Some(existing) = find_file_by_name_in_scope(
                        &self.state,
                        self.scope,
                        dest_parent_id,
                        &dest_name,
                    )
                    .await?
                    {
                        file_service::delete_in_scope_with_audit(
                            &state,
                            self.scope(),
                            existing.id,
                            &self.audit_ctx,
                        )
                        .await
                        .map_err(to_fs_error)?;
                    }

                    let copied =
                        file_service::duplicate_file_record(&state, &f, dest_parent_id, &dest_name)
                            .await
                            .map_err(to_fs_error)?;
                    audit_service::log(
                        &state,
                        &self.audit_ctx,
                        audit_service::AuditAction::FileCopy,
                        crate::services::audit_service::AuditEntityType::File,
                        Some(copied.id),
                        Some(&copied.name),
                        Some(serde_json::json!({ "source": "webdav" })),
                    )
                    .await;
                }
                ResolvedNode::Folder(f) => {
                    let copied = webdav_service::copy_folder_tree_in_scope(
                        &state,
                        self.scope,
                        f.id,
                        dest_parent_id,
                        &dest_name,
                    )
                    .await
                    .map_err(to_fs_error)?;
                    audit_service::log(
                        &state,
                        &self.audit_ctx,
                        audit_service::AuditAction::FolderCopy,
                        crate::services::audit_service::AuditEntityType::Folder,
                        Some(copied.id),
                        Some(&copied.name),
                        Some(serde_json::json!({ "source": "webdav" })),
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
                    let user = user_repo::find_by_id(self.state.writer_db(), user_id)
                        .await
                        .map_err(|_| FsError::GeneralFailure)?;
                    (user.storage_used, user.storage_quota)
                }
                WorkspaceStorageScope::Team { team_id, .. } => {
                    let team = team_repo::find_by_id(self.state.writer_db(), team_id)
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
                match resolve_entity(&self.state, self.scope, path, self.root_folder_id).await {
                    Some(v) => v,
                    None => return false,
                };
            property_repo::find_by_entity(self.state.writer_db(), entity_type, entity_id)
                .await
                .map(|props| {
                    props
                        .iter()
                        .any(|prop| !property_service::is_protected_namespace(&prop.namespace))
                })
                .unwrap_or(false)
        })
    }

    fn get_props<'a>(&'a self, path: &'a DavPath, do_content: bool) -> FsFuture<'a, Vec<DavProp>> {
        Box::pin(async move {
            let (entity_type, entity_id) =
                resolve_entity(&self.state, self.scope, path, self.root_folder_id)
                    .await
                    .ok_or(FsError::NotFound)?;

            let props =
                property_repo::find_by_entity(self.state.writer_db(), entity_type, entity_id)
                    .await
                    .map_err(|_| FsError::GeneralFailure)?;

            Ok(props
                .into_iter()
                .filter(|p| !property_service::is_protected_namespace(&p.namespace))
                .map(|p| DavProp {
                    name: p.name,
                    prefix: None,
                    namespace: if p.namespace.is_empty() {
                        None
                    } else {
                        Some(p.namespace)
                    },
                    xml: if do_content {
                        p.value.map(|v| v.into_bytes())
                    } else {
                        None
                    },
                })
                .collect())
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

            let mut results = Vec::new();

            for (set, prop) in patches {
                let ns = prop.namespace.as_deref().unwrap_or("");

                // DAV: 与 system.* 命名空间只读，后者用于内部缓存/派生属性。
                if property_service::is_protected_namespace(ns) {
                    results.push((http::StatusCode::FORBIDDEN, prop));
                    continue;
                }

                let status = if set {
                    let value = prop.xml.as_ref().map(|x| String::from_utf8_lossy(x));
                    match property_repo::upsert(
                        self.state.writer_db(),
                        entity_type,
                        entity_id,
                        ns,
                        &prop.name,
                        value.as_deref(),
                    )
                    .await
                    {
                        Ok(_) => http::StatusCode::OK,
                        Err(_) => http::StatusCode::INTERNAL_SERVER_ERROR,
                    }
                } else {
                    match property_repo::delete_prop(
                        self.state.writer_db(),
                        entity_type,
                        entity_id,
                        ns,
                        &prop.name,
                    )
                    .await
                    {
                        Ok(_) => http::StatusCode::OK,
                        Err(_) => http::StatusCode::INTERNAL_SERVER_ERROR,
                    }
                };

                if status.is_success() {
                    let entity_type_label = entity_type.as_str();
                    audit_service::log_with_details(
                        &self.state,
                        &self.audit_ctx,
                        if set {
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

                results.push((status, prop));
            }

            Ok(results)
        })
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
