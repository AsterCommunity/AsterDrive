//! AsterDrive storage, database, property, lock, and audit adapters for WebDAV.

mod deltav;
mod dir_entry;
mod download_audit;
pub mod file;
pub mod lock;
mod metadata;
mod mutation;
pub mod path_resolver;

use aster_forge_db::transaction;
use chrono::Utc;
use std::{collections::HashMap, pin::Pin};

use sea_orm::ConnectionTrait;
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::db::repository::{file_repo, folder_repo, property_repo, team_repo, user_repo};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{
    events::storage_change,
    files::{file as file_ops, folder},
    ops::audit::{self, AuditContext},
    workspace::storage::WorkspaceStorageScope,
};
use crate::webdav::backend::dir_entry::AsterDavDirEntry;
use crate::webdav::backend::download_audit::{
    WebdavDownloadAuditIdentity, WebdavDownloadRequestKind, record_download,
};
use crate::webdav::backend::file::{AsterDavWriteHandle, DavWriteOpenContext};
use crate::webdav::backend::path_resolver::ResolvedNode;
use crate::webdav::handlers::resources::MUTATION_FOLDER_TREE_LIMITS;
use aster_drive_model::entities::{file as file_entity, file_blob};
use aster_drive_model::types::EntityType;
use aster_forge_api::NullablePatch;
use aster_forge_utils::http_range::HttpByteRange;
use aster_forge_utils::numbers::{i64_to_u64, usize_to_u64};
use aster_forge_webdav::plan_atomic_proppatch;
use aster_forge_webdav::{
    DavBackendError, DavBackendErrorKind, DavConditionalOutcome, DavConditionalResource,
    DavContentStream, DavDirectoryEnumerator, DavDirectoryPage, DavDirectoryPageRequest,
    DavDownloadSource, DavFileSystem, DavLockError, DavMetaData, DavMutationCredentials,
    DavOpenedDownload, DavPath, DavProp, DavWriteOptions, DavWriteSystem, FsError, FsFuture,
    IfHeader, plan_http_conditionals,
};

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
    pub(crate) file: file_entity::Model,
    pub(crate) blob: file_blob::Model,
    pub(crate) meta: AsterDavMeta,
}

pub(crate) use deltav::{AuthorizedDeltavRevision, DeltavCapabilityTarget};
pub(crate) use metadata::AsterDavMeta;

pub(crate) struct DavMutationConditions<'a> {
    pub(crate) prefix: &'a str,
    pub(crate) if_header: Option<&'a IfHeader>,
    pub(crate) request_scheme: &'a str,
    pub(crate) request_host: &'a str,
    pub(crate) http_headers: &'a http::HeaderMap,
    pub(crate) http_method: aster_forge_webdav::DavMethod,
    pub(crate) http_target: &'a aster_forge_webdav::DavPath,
}

fn deltav_backend_to_fs_error(error: DavBackendError) -> FsError {
    match error.kind {
        DavBackendErrorKind::NotFound => FsError::NotFound,
        DavBackendErrorKind::Forbidden => FsError::Forbidden,
        DavBackendErrorKind::AlreadyExists => FsError::Exists,
        DavBackendErrorKind::InsufficientStorage => FsError::InsufficientStorage,
        DavBackendErrorKind::PayloadTooLarge => FsError::TooLarge,
        DavBackendErrorKind::InvalidInput => FsError::BadRequest,
        DavBackendErrorKind::Conflict
        | DavBackendErrorKind::Locked
        | DavBackendErrorKind::Unsupported
        | DavBackendErrorKind::Internal => {
            tracing::warn!(kind = ?error.kind, "DeltaV backend failure crossed the filesystem adapter");
            FsError::GeneralFailure
        }
    }
}

#[derive(Debug)]
pub(crate) enum AsterDavMutationError {
    FileSystem(FsError),
    Locked(DavPath),
    Conflict,
    PreconditionFailed,
    Backend,
}

enum DeletedResource {
    File(file_entity::Model),
    Folder(aster_drive_model::entities::folder::Model),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AsterDavDirectoryCursor {
    Folders(i64),
    Files(i64),
}

pub(crate) struct AsterDavWriteDirectoryEnumerator<'a> {
    dav_fs: &'a AsterDavFs,
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

        let authorized = match node {
            ResolvedNode::Root | ResolvedNode::Folder(_) => {
                return Ok(None);
            }
            ResolvedNode::File(file) => file,
        };

        let (file, blob, revision) =
            file_ops::load_current_download_snapshot(&self.state, authorized.id)
                .await
                .map_err(to_fs_error)?;
        crate::services::workspace::storage::ensure_active_file_scope(&file, self.scope)
            .map_err(to_fs_error)?;
        if file.folder_id != authorized.folder_id || file.name != authorized.name {
            return Err(FsError::NotFound);
        }
        let meta = AsterDavMeta::from_file(&file, revision.etag);

        Ok(Some(AsterDavDownloadFile { file, blob, meta }))
    }

    pub(crate) async fn metadata_for_write(&self, path: &DavPath) -> Result<AsterDavMeta, FsError> {
        let node = path_resolver::resolve_path_cached_in_scope(
            &self.state,
            self.scope,
            path,
            self.root_folder_id,
        )
        .await?;
        match node {
            ResolvedNode::Root => Ok(AsterDavMeta::root()),
            ResolvedNode::Folder(folder) => Ok(AsterDavMeta::from_folder(&folder)),
            ResolvedNode::File(authorized) => {
                let (file, _, revision) =
                    file_ops::load_current_download_snapshot(&self.state, authorized.id)
                        .await
                        .map_err(to_fs_error)?;
                crate::services::workspace::storage::ensure_active_file_scope(&file, self.scope)
                    .map_err(to_fs_error)?;
                if file.folder_id != authorized.folder_id || file.name != authorized.name {
                    return Err(FsError::NotFound);
                }
                Ok(AsterDavMeta::from_file(&file, revision.etag))
            }
        }
    }

    pub(crate) async fn open_download_stream_for_file(
        &self,
        file: &file_entity::Model,
        blob: &file_blob::Model,
        offset: Option<u64>,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>, FsError> {
        if blob.is_virtual_empty() {
            return Ok(Box::new(tokio::io::empty()));
        }
        let storage_path = blob
            .storage_path_for_connector()
            .ok_or(FsError::GeneralFailure)?;
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
                .get_range(storage_path, offset, length)
                .await
                .map_err(|_| FsError::NotFound)?,
            None => driver
                .get_stream(storage_path)
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

    pub(crate) fn write_directory_enumerator(&self) -> AsterDavWriteDirectoryEnumerator<'_> {
        AsterDavWriteDirectoryEnumerator { dav_fs: self }
    }

    async fn resolve_directory_id(
        &self,
        path: &DavPath,
        writer_authoritative: bool,
    ) -> Result<Option<i64>, DavBackendError> {
        let resolved = if writer_authoritative {
            path_resolver::resolve_path_cached_in_scope(
                &self.state,
                self.scope,
                path,
                self.root_folder_id,
            )
            .await
        } else {
            path_resolver::resolve_path_cached_for_read_in_scope(
                &self.state,
                self.scope,
                path,
                self.root_folder_id,
            )
            .await
        }
        .map_err(DavBackendError::from)?;
        match resolved {
            ResolvedNode::Root => Ok(self.root_folder_id),
            ResolvedNode::Folder(folder) => Ok(Some(folder.id)),
            ResolvedNode::File(_) => Err(DavBackendError::new(DavBackendErrorKind::Forbidden)),
        }
    }

    async fn folder_page(
        &self,
        parent_id: Option<i64>,
        after_id: Option<i64>,
        limit: u64,
        writer_authoritative: bool,
    ) -> Result<Vec<aster_drive_model::entities::folder::Model>, DavBackendError> {
        let db = if writer_authoritative {
            self.state.writer_db()
        } else {
            self.state.reader_db()
        };
        match self.scope {
            WorkspaceStorageScope::Personal { user_id } => {
                folder_repo::find_children_after_id(db, user_id, parent_id, after_id, limit).await
            }
            WorkspaceStorageScope::Team { team_id, .. } => {
                folder_repo::find_team_children_after_id(db, team_id, parent_id, after_id, limit)
                    .await
            }
        }
        .map_err(|error| {
            tracing::warn!(error = %error, "WebDAV directory folder page query failed");
            DavBackendError::new(DavBackendErrorKind::Internal)
        })
    }

    async fn file_page(
        &self,
        folder_id: Option<i64>,
        after_id: Option<i64>,
        limit: u64,
        writer_authoritative: bool,
    ) -> Result<Vec<file_entity::Model>, DavBackendError> {
        let db = if writer_authoritative {
            self.state.writer_db()
        } else {
            self.state.reader_db()
        };
        match self.scope {
            WorkspaceStorageScope::Personal { user_id } => {
                file_repo::find_by_folder_after_id(db, user_id, folder_id, after_id, limit).await
            }
            WorkspaceStorageScope::Team { team_id, .. } => {
                file_repo::find_by_team_folder_after_id(db, team_id, folder_id, after_id, limit)
                    .await
            }
        }
        .map_err(|error| {
            tracing::warn!(error = %error, "WebDAV directory file page query failed");
            DavBackendError::new(DavBackendErrorKind::Internal)
        })
    }

    async fn read_directory_page_with_consistency(
        &self,
        request: DavDirectoryPageRequest<'_, AsterDavDirectoryCursor>,
        writer_authoritative: bool,
    ) -> Result<DavDirectoryPage<AsterDavDirEntry, AsterDavDirectoryCursor>, DavBackendError> {
        let folder_id = self
            .resolve_directory_id(request.path, writer_authoritative)
            .await?;
        let fetch_size = request
            .maximum_entries
            .checked_add(1)
            .ok_or_else(|| DavBackendError::new(DavBackendErrorKind::InvalidInput))?;
        let fetch_size = usize_to_u64(fetch_size, "WebDAV directory page fetch size")
            .map_err(|_| DavBackendError::new(DavBackendErrorKind::InvalidInput))?;

        let (folder_after, file_after) = match request.cursor {
            None => (None, None),
            Some(AsterDavDirectoryCursor::Folders(id)) => (Some(*id), None),
            Some(AsterDavDirectoryCursor::Files(id)) => {
                let files = self
                    .file_page(folder_id, Some(*id), fetch_size, writer_authoritative)
                    .await?;
                return self
                    .file_only_page(files, request.maximum_entries, writer_authoritative)
                    .await;
            }
        };

        let folders = self
            .folder_page(folder_id, folder_after, fetch_size, writer_authoritative)
            .await?;
        if folders.len() > request.maximum_entries {
            let returned = folders
                .into_iter()
                .take(request.maximum_entries)
                .collect::<Vec<_>>();
            let next_cursor = returned
                .last()
                .map(|folder| AsterDavDirectoryCursor::Folders(folder.id));
            let entries = returned
                .iter()
                .map(AsterDavDirEntry::from_folder)
                .collect::<Vec<_>>();
            return Ok(DavDirectoryPage {
                entries,
                next_cursor,
            });
        }

        let mut entries = folders
            .iter()
            .map(AsterDavDirEntry::from_folder)
            .collect::<Vec<_>>();
        let remaining = request.maximum_entries.saturating_sub(entries.len());
        if remaining == 0 {
            let last_id = folders.last().map(|folder| folder.id);
            let has_more_files = !self
                .file_page(folder_id, None, 1, writer_authoritative)
                .await?
                .is_empty();
            return Ok(DavDirectoryPage {
                entries,
                next_cursor: has_more_files
                    .then(|| AsterDavDirectoryCursor::Folders(last_id.unwrap_or_default())),
            });
        }

        let file_fetch_size = remaining
            .checked_add(1)
            .and_then(|value| usize_to_u64(value, "WebDAV file page fetch size").ok())
            .ok_or_else(|| DavBackendError::new(DavBackendErrorKind::InvalidInput))?;
        let files = self
            .file_page(folder_id, file_after, file_fetch_size, writer_authoritative)
            .await?;
        let has_more_files = files.len() > remaining;
        let returned_files = files.into_iter().take(remaining).collect::<Vec<_>>();
        let last_file_id = returned_files.last().map(|file| file.id);
        entries.extend(
            self.file_entries(&returned_files, writer_authoritative)
                .await?,
        );
        Ok(DavDirectoryPage {
            entries,
            next_cursor: has_more_files
                .then(|| AsterDavDirectoryCursor::Files(last_file_id.unwrap_or_default())),
        })
    }

    async fn file_entries(
        &self,
        files: &[file_entity::Model],
        writer_authoritative: bool,
    ) -> Result<Vec<AsterDavDirEntry>, DavBackendError> {
        let db = if writer_authoritative {
            self.state.writer_db()
        } else {
            self.state.reader_db()
        };
        let file_ids = files.iter().map(|file| file.id).collect::<Vec<_>>();
        let revisions =
            crate::db::repository::revision_repo::current_revision_snapshots_by_file_ids(
                db, &file_ids,
            )
            .await
            .map_err(|error| {
                tracing::warn!(%error, "WebDAV current revision query failed");
                DavBackendError::new(DavBackendErrorKind::Internal)
            })?;
        files
            .iter()
            .map(|file| {
                let snapshot = revisions.get(&file.id).cloned().ok_or_else(|| {
                    tracing::warn!(
                        file_id = file.id,
                        "WebDAV directory entry has no current revision"
                    );
                    DavBackendError::new(DavBackendErrorKind::Internal)
                })?;
                Ok(AsterDavDirEntry::from_file_record(
                    file,
                    snapshot.revision,
                    snapshot.deltav_controlled,
                ))
            })
            .collect()
    }

    async fn file_only_page(
        &self,
        files: Vec<file_entity::Model>,
        maximum_entries: usize,
        writer_authoritative: bool,
    ) -> Result<DavDirectoryPage<AsterDavDirEntry, AsterDavDirectoryCursor>, DavBackendError> {
        let has_more = files.len() > maximum_entries;
        let returned = files.into_iter().take(maximum_entries).collect::<Vec<_>>();
        let next_cursor = has_more
            .then(|| AsterDavDirectoryCursor::Files(returned.last().map_or(0, |file| file.id)));
        Ok(DavDirectoryPage {
            entries: self.file_entries(&returned, writer_authoritative).await?,
            next_cursor,
        })
    }
}

impl DavDirectoryEnumerator for AsterDavFs {
    type Cursor = AsterDavDirectoryCursor;
    type Entry = AsterDavDirEntry;

    async fn read_directory_page<'a>(
        &'a self,
        request: DavDirectoryPageRequest<'a, Self::Cursor>,
    ) -> Result<DavDirectoryPage<Self::Entry, Self::Cursor>, DavBackendError> {
        self.read_directory_page_with_consistency(request, false)
            .await
    }
}

impl DavDirectoryEnumerator for AsterDavWriteDirectoryEnumerator<'_> {
    type Cursor = AsterDavDirectoryCursor;
    type Entry = AsterDavDirEntry;

    async fn read_directory_page<'a>(
        &'a self,
        request: DavDirectoryPageRequest<'a, Self::Cursor>,
    ) -> Result<DavDirectoryPage<Self::Entry, Self::Cursor>, DavBackendError> {
        self.dav_fs
            .read_directory_page_with_consistency(request, true)
            .await
    }
}

impl DavDownloadSource for AsterDavFs {
    type Metadata = AsterDavMeta;

    async fn metadata<'a>(&'a self, path: &'a DavPath) -> Result<Self::Metadata, DavBackendError> {
        self.resolve_download_target(path)
            .await
            .map_err(DavBackendError::from)?
            .map(|target| target.meta)
            .ok_or_else(|| DavBackendError::new(DavBackendErrorKind::Unsupported))
    }

    async fn open_full<'a>(
        &'a self,
        path: &'a DavPath,
    ) -> Result<DavOpenedDownload, DavBackendError> {
        crate::webdav::observation::add_backend_open();
        self.open_download(path, None).await
    }

    async fn open_range<'a>(
        &'a self,
        path: &'a DavPath,
        range: HttpByteRange,
    ) -> Result<DavOpenedDownload, DavBackendError> {
        crate::webdav::observation::add_backend_open();
        self.open_download(path, Some(range)).await
    }
}

impl AsterDavFs {
    async fn open_download(
        &self,
        path: &DavPath,
        range: Option<HttpByteRange>,
    ) -> Result<DavOpenedDownload, DavBackendError> {
        let target = self
            .resolve_download_target(path)
            .await
            .map_err(DavBackendError::from)?
            .ok_or_else(|| DavBackendError::new(DavBackendErrorKind::Unsupported))?;
        let expected_length = range.map_or_else(|| target.meta.len(), |range| range.length());
        let (offset, length) = range.map_or((None, None), |range| {
            (Some(range.start()), Some(range.length()))
        });
        let reader = self
            .open_download_stream_for_file(&target.file, &target.blob, offset, length)
            .await
            .map_err(DavBackendError::from)?;
        let stream = exact_length_stream(reader, expected_length);
        Ok(DavOpenedDownload::new(stream, expected_length))
    }
}

pub(crate) fn exact_length_stream(
    mut reader: Box<dyn AsyncRead + Unpin + Send>,
    expected_length: u64,
) -> DavContentStream {
    Box::pin(async_stream::stream! {
        let mut remaining = expected_length;
        let mut buffer = vec![0_u8; crate::storage::io_limits::DOWNLOAD_READER_BUFFER_BYTES];
        while remaining != 0 {
            let maximum = usize::try_from(remaining)
                .unwrap_or(usize::MAX)
                .min(buffer.len());
            match reader.read(&mut buffer[..maximum]).await {
                Ok(0) => {
                    tracing::warn!(expected_length, remaining, "WebDAV download stream ended early");
                    yield Err(DavBackendError::new(DavBackendErrorKind::Internal));
                    return;
                }
                Ok(read) => {
                    let Ok(read_u64) = usize_to_u64(read, "WebDAV download chunk length") else {
                        yield Err(DavBackendError::new(DavBackendErrorKind::Internal));
                        return;
                    };
                    remaining -= read_u64;
                    yield Ok(bytes::Bytes::copy_from_slice(&buffer[..read]));
                }
                Err(error) => {
                    tracing::warn!(error = %error, expected_length, remaining, "WebDAV download stream failed");
                    yield Err(DavBackendError::new(DavBackendErrorKind::Internal));
                    return;
                }
            }
        }
    })
}

impl AsterDavFs {
    pub(crate) async fn open_write_with_precondition(
        &self,
        path: &DavPath,
        options: DavWriteOptions,
        file_precondition: Option<crate::services::workspace::storage::FileWritePrecondition>,
    ) -> Result<AsterDavWriteHandle, DavBackendError> {
        let (parent_id, filename) = path_resolver::resolve_parent_cached_in_scope(
            &self.state,
            self.scope,
            path,
            self.root_folder_id,
        )
        .await
        .map_err(|error| match error {
            FsError::NotFound => DavBackendError::new(DavBackendErrorKind::Conflict),
            error => DavBackendError::from(error),
        })?;
        let existing_file =
            find_file_by_name_in_scope(&self.state, self.scope, parent_id, &filename)
                .await
                .map_err(DavBackendError::from)?;
        let existing_file_id = existing_file.map(|file| file.id);
        if options.create_new && existing_file_id.is_some() {
            return Err(DavBackendError::new(DavBackendErrorKind::AlreadyExists));
        }
        if !options.create && !options.create_new && existing_file_id.is_none() {
            return Err(DavBackendError::new(DavBackendErrorKind::NotFound));
        }
        AsterDavWriteHandle::for_write_with_audit(
            self.app_state(),
            DavWriteOpenContext {
                scope: self.scope,
                folder_id: parent_id,
                filename,
                existing_file_id,
                declared_size: options.expected_length,
                submitted_lock_tokens: options.credentials.submitted_lock_tokens,
                audit_ctx: self.audit_ctx.clone(),
                file_precondition,
            },
        )
        .await
        .map_err(DavBackendError::from)
    }
}

impl DavWriteSystem for AsterDavFs {
    type Handle = AsterDavWriteHandle;

    async fn open_write<'a>(
        &'a self,
        path: &'a DavPath,
        options: DavWriteOptions,
    ) -> Result<Self::Handle, DavBackendError> {
        self.open_write_with_precondition(path, options, None).await
    }
}

impl DavFileSystem for AsterDavFs {
    fn metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, Box<dyn DavMetaData>> {
        Box::pin(async move {
            if matches!(
                crate::webdav::deltav::classify_reserved_path(path),
                crate::webdav::deltav::ReservedDeltavPath::Version(_)
            ) {
                return self
                    .deltav_metadata(path)
                    .await
                    .map(|metadata| Box::new(metadata) as Box<dyn DavMetaData>)
                    .map_err(deltav_backend_to_fs_error);
            }
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
                ResolvedNode::File(authorized) => {
                    let (file, _, revision) =
                        file_ops::load_current_download_snapshot(&self.state, authorized.id)
                            .await
                            .map_err(to_fs_error)?;
                    crate::services::workspace::storage::ensure_active_file_scope(
                        &file, self.scope,
                    )
                    .map_err(to_fs_error)?;
                    if file.folder_id != authorized.folder_id || file.name != authorized.name {
                        return Err(FsError::NotFound);
                    }
                    Box::new(AsterDavMeta::from_file(&file, revision.etag))
                }
            };

            Ok(meta)
        })
    }

    fn create_dir<'a>(
        &'a self,
        path: &'a DavPath,
        credentials: DavMutationCredentials,
    ) -> FsFuture<'a, ()> {
        Box::pin(async move {
            match path_resolver::resolve_path_cached_in_scope(
                &self.state,
                self.scope,
                path,
                self.root_folder_id,
            )
            .await
            {
                Ok(_) => return Err(FsError::Exists),
                Err(FsError::NotFound) => {}
                Err(err) => return Err(err),
            }

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
                crate::services::files::lock::LockMutationCredentials::SubmittedTokens(
                    credentials.submitted_lock_tokens,
                ),
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
            folder::delete_in_scope(
                &state,
                self.scope,
                folder.id,
                Some(MUTATION_FOLDER_TREE_LIMITS),
            )
            .await
            .map_err(to_fs_error)?;
            audit::log_with_details(
                &state,
                &self.audit_ctx,
                audit::AuditAction::FolderDelete,
                crate::services::ops::audit::AuditEntityType::Folder,
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
                Some(MUTATION_FOLDER_TREE_LIMITS),
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
                Some(MUTATION_FOLDER_TREE_LIMITS),
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
                    audit::log_with_details(
                        &state,
                        &self.audit_ctx,
                        audit::AuditAction::FileCopy,
                        crate::services::ops::audit::AuditEntityType::File,
                        Some(copied.id),
                        Some(&copied.name),
                        || details.clone(),
                    )
                    .await;
                }
                ResolvedNode::Folder(f) => {
                    let (copied, storage_delta) =
                        folder::copy_folder_tree_in_scope_with_user_properties(
                            &state,
                            self.scope,
                            f.id,
                            dest_parent_id,
                            &dest_name,
                            Some(MUTATION_FOLDER_TREE_LIMITS),
                        )
                        .await
                        .map_err(to_fs_error)?;
                    storage_change::publish(
                        &state,
                        storage_change::StorageChangeEvent::new(
                            storage_change::StorageChangeKind::FolderCreated,
                            self.scope(),
                            vec![],
                            vec![copied.id],
                            vec![copied.parent_id],
                        )
                        .with_storage_delta(storage_delta),
                    );
                    copy_visible_folder_properties_for_copied_tree(
                        &state,
                        self.scope(),
                        f.id,
                        copied.id,
                    )
                    .await?;
                    let details = folder::audit_transfer_details_for_models(
                        &state,
                        self.scope(),
                        &f,
                        &copied,
                    )
                    .await;
                    audit::log_with_details(
                        &state,
                        &self.audit_ctx,
                        audit::AuditAction::FolderCopy,
                        crate::services::ops::audit::AuditEntityType::Folder,
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
            if matches!(
                crate::webdav::deltav::classify_reserved_path(path),
                crate::webdav::deltav::ReservedDeltavPath::Version(_)
            ) {
                return match self.deltav_dead_properties(path).await {
                    Ok(properties) => !properties.is_empty(),
                    Err(error) => {
                        tracing::warn!(kind = ?error.kind, "DeltaV dead-property existence lookup failed");
                        false
                    }
                };
            }
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
                        .any(|prop| !property_repo::is_protected_namespace(&prop.namespace))
                })
                .unwrap_or(false)
        })
    }

    fn get_props<'a>(&'a self, path: &'a DavPath, do_content: bool) -> FsFuture<'a, Vec<DavProp>> {
        Box::pin(async move {
            if matches!(
                crate::webdav::deltav::classify_reserved_path(path),
                crate::webdav::deltav::ReservedDeltavPath::Version(_)
            ) {
                return self
                    .deltav_dead_properties(path)
                    .await
                    .map_err(deltav_backend_to_fs_error);
            }
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
            let mut result = HashMap::with_capacity(paths.len());
            for path in paths {
                if matches!(
                    crate::webdav::deltav::classify_reserved_path(path),
                    crate::webdav::deltav::ReservedDeltavPath::Version(_)
                ) {
                    result.insert(
                        path.clone(),
                        self.deltav_dead_properties(path)
                            .await
                            .map_err(deltav_backend_to_fs_error)?,
                    );
                    continue;
                }
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
                if property_repo::is_protected_namespace(&prop.namespace) {
                    continue;
                }
                props_by_target
                    .entry((prop.entity_type, prop.entity_id))
                    .or_default()
                    .push(entity_prop_to_dav_prop(prop, do_content));
            }

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

            let protocol_plan = plan_atomic_proppatch(patches.iter().map(|(_, prop)| {
                property_repo::is_protected_namespace(prop.namespace.as_deref().unwrap_or(""))
            }));
            if !protocol_plan.apply {
                return Ok(patches
                    .into_iter()
                    .zip(protocol_plan.statuses)
                    .map(|((_, prop), status)| (status, prop))
                    .collect());
            }

            let txn = transaction::begin(self.state.writer_db())
                .await
                .map_err(|_| FsError::GeneralFailure)?;

            let mut applied = Vec::with_capacity(patches.len());

            for (set, prop) in &patches {
                let ns = prop.namespace.as_deref().unwrap_or("");
                let current =
                    property_repo::find_by_key(&txn, entity_type, entity_id, ns, &prop.name)
                        .await
                        .map_err(|_| FsError::GeneralFailure)?;
                if *set {
                    let value = prop.xml.as_ref().map(|x| String::from_utf8_lossy(x));
                    if current
                        .as_ref()
                        .is_some_and(|current| current.value.as_deref() == value.as_deref())
                    {
                        applied.push(false);
                        continue;
                    }
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
                    applied.push(true);
                } else {
                    if current.is_none() {
                        applied.push(false);
                        continue;
                    }
                    property_repo::delete_prop(&txn, entity_type, entity_id, ns, &prop.name)
                        .await
                        .map_err(|_| FsError::GeneralFailure)?;
                    applied.push(true);
                }
            }

            let changed = applied.iter().any(|applied| *applied);
            let mut controlled_revision_file_id = None;
            if changed && matches!(entity_type, EntityType::File) {
                let file = file_repo::find_by_id(&txn, entity_id)
                    .await
                    .map_err(|_| FsError::GeneralFailure)?;
                let history =
                    crate::db::repository::revision_repo::find_history_by_file_id(&txn, file.id)
                        .await
                        .map_err(|_| FsError::GeneralFailure)?;
                if history.deltav_controlled_at.is_some() {
                    crate::services::workspace::storage::lock_storage_usage(&txn, self.scope)
                        .await
                        .map_err(to_fs_error)?;
                    if file.size > 0 {
                        crate::services::workspace::storage::check_quota(
                            &txn, self.scope, file.size,
                        )
                        .await
                        .map_err(to_fs_error)?;
                    }
                    let actor_username =
                        crate::services::workspace::storage::load_scope_actor_username(
                            &txn, self.scope,
                        )
                        .await
                        .map_err(|_| FsError::GeneralFailure)?;
                    file_repo::increment_blob_ref_count(&txn, file.blob_id)
                        .await
                        .map_err(to_fs_error)?;
                    crate::db::repository::revision_repo::append(
                        &txn,
                        file.id,
                        None,
                        crate::db::repository::revision_repo::NewRevision {
                            blob_id: file.blob_id,
                            logical_size: file.size,
                            mime_type: &file.mime_type,
                            content_sha256: None,
                            creator_user_id: Some(self.scope.actor_user_id()),
                            creator_display_name: &actor_username,
                            comment: None,
                            reason:
                                crate::db::repository::revision_repo::RevisionReason::PropertyChange,
                            created_at: Utc::now(),
                            etag: None,
                        },
                    )
                    .await
                    .map_err(|_| FsError::GeneralFailure)?;
                    if file.size != 0 {
                        crate::services::workspace::storage::update_storage_used(
                            &txn, self.scope, file.size,
                        )
                        .await
                        .map_err(to_fs_error)?;
                    }
                    controlled_revision_file_id = Some(file.id);
                }
            }

            transaction::commit(txn)
                .await
                .map_err(|_| FsError::GeneralFailure)?;

            if let Some(file_id) = controlled_revision_file_id {
                crate::services::content::version::cleanup_excess(&self.state, file_id)
                    .await
                    .map_err(to_fs_error)?;
            }

            for ((set, prop), applied) in patches.iter().zip(applied.iter()) {
                if !*applied {
                    continue;
                }
                let ns = prop.namespace.as_deref().unwrap_or("");
                let entity_type_label = entity_type.as_str();
                audit::log_with_details(
                    &self.state,
                    &self.audit_ctx,
                    if *set {
                        audit::AuditAction::PropertySet
                    } else {
                        audit::AuditAction::PropertyDelete
                    },
                    audit::AuditEntityType::from_entity_type(entity_type),
                    Some(entity_id),
                    None,
                    || {
                        audit::details(audit::PropertyAuditDetails {
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
                .zip(protocol_plan.statuses)
                .map(|((_, prop), status)| (status, prop))
                .collect())
        })
    }
}

fn entity_props_to_dav_props(
    props: Vec<aster_drive_model::entities::entity_property::Model>,
    do_content: bool,
) -> Vec<DavProp> {
    props
        .into_iter()
        .filter(|p| !property_repo::is_protected_namespace(&p.namespace))
        .map(|p| entity_prop_to_dav_prop(p, do_content))
        .collect()
}

fn entity_prop_to_dav_prop(
    prop: aster_drive_model::entities::entity_property::Model,
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
    copy_visible_entity_properties_on(
        state.writer_db(),
        src_entity_type,
        src_entity_id,
        dest_entity_type,
        dest_entity_id,
    )
    .await
}

async fn copy_visible_entity_properties_on<C: ConnectionTrait>(
    db: &C,
    src_entity_type: EntityType,
    src_entity_id: i64,
    dest_entity_type: EntityType,
    dest_entity_id: i64,
) -> Result<(), FsError> {
    let props = property_repo::find_by_entity(db, src_entity_type, src_entity_id)
        .await
        .map_err(|_| FsError::GeneralFailure)?;

    for prop in props {
        if property_repo::is_protected_namespace(&prop.namespace) {
            continue;
        }
        property_repo::upsert(
            db,
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
) -> Result<Vec<aster_drive_model::entities::folder::Model>, FsError> {
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

async fn copy_visible_folder_properties_for_copied_tree(
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

        let (src_children, dest_children) = tokio::try_join!(
            load_child_folders_in_scope(state, scope, &src_folder_ids),
            load_child_folders_in_scope(state, scope, &dest_folder_ids),
        )?;

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
) -> Result<Option<aster_drive_model::entities::file::Model>, FsError> {
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
) -> Result<Option<aster_drive_model::entities::folder::Model>, FsError> {
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
    traversal_limits: Option<folder::FolderTreeTraversalLimits>,
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
        audit::log_with_details(
            state,
            audit_ctx,
            audit::AuditAction::FileDelete,
            crate::services::ops::audit::AuditEntityType::File,
            Some(existing.id),
            Some(&existing.name),
            || details.clone(),
        )
        .await;
    }

    if let Some(existing) = find_folder_by_name_in_scope(state, scope, parent_id, name).await? {
        let details = folder::audit_location_details_for_model(state, scope, &existing).await;
        folder::delete_in_scope(state, scope, existing.id, traversal_limits)
            .await
            .map_err(to_fs_error)?;
        audit::log_with_details(
            state,
            audit_ctx,
            audit::AuditAction::FolderDelete,
            crate::services::ops::audit::AuditEntityType::Folder,
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

        crate::errors::AsterError::StorageQuotaExceeded(_)
        | crate::errors::AsterError::OperationResourceLimitExceeded(_) => {
            FsError::InsufficientStorage
        }

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

struct AtomicTargetRevalidation<'a> {
    path: &'a DavPath,
    check_locks: bool,
    deep: bool,
}

async fn revalidate_atomic_target<C: ConnectionTrait>(
    db: &C,
    namespace_id: i64,
    scope: WorkspaceStorageScope,
    root_folder_id: Option<i64>,
    target: AtomicTargetRevalidation<'_>,
    conditions: &DavMutationConditions<'_>,
) -> Result<(), AsterDavMutationError> {
    if target.check_locks {
        lock::revalidate_mutation_locks(db, namespace_id, target.path, target.deep, conditions)
            .await
            .map_err(map_atomic_lock_error)?;
    }
    if target.path != conditions.http_target {
        return Ok(());
    }

    let node = path_resolver::resolve_path_in_scope(db, scope, target.path, root_folder_id)
        .await
        .map_err(AsterDavMutationError::FileSystem)?;
    let (etag, last_modified) = match node {
        ResolvedNode::Root => (None, None),
        ResolvedNode::Folder(folder) => (
            Some(format!("dir-{}", folder.updated_at.timestamp())),
            Some(metadata::to_system_time(folder.updated_at)),
        ),
        ResolvedNode::File(file) => (
            Some(
                crate::db::repository::revision_repo::current_etag(db, file.id)
                    .await
                    .map_err(|_| AsterDavMutationError::Backend)?,
            ),
            Some(metadata::to_system_time(file.updated_at)),
        ),
    };
    let plan = plan_http_conditionals(
        conditions.http_method,
        conditions.http_headers,
        DavConditionalResource {
            exists: true,
            etag: etag.as_deref(),
            last_modified,
        },
    )
    .map_err(|_| AsterDavMutationError::Backend)?;
    if plan.outcome == DavConditionalOutcome::Proceed {
        Ok(())
    } else {
        Err(AsterDavMutationError::PreconditionFailed)
    }
}

fn map_atomic_lock_error(error: DavLockError) -> AsterDavMutationError {
    match error {
        DavLockError::Conflict(lock) => AsterDavMutationError::Locked((*lock.path).clone()),
        DavLockError::TokenMismatch
        | DavLockError::ParentMissing
        | DavLockError::LimitExceeded
        | DavLockError::NotFound
        | DavLockError::Backend => AsterDavMutationError::Backend,
    }
}

fn map_ancestor_lock_error(error: lock::LockMutationAncestorError) -> AsterDavMutationError {
    match error {
        lock::LockMutationAncestorError::Conflict => AsterDavMutationError::Conflict,
        lock::LockMutationAncestorError::Backend => AsterDavMutationError::Backend,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DatabaseConfig;

    async fn test_db() -> sea_orm::DatabaseConnection {
        crate::db::connect_with_metrics(
            &DatabaseConfig {
                url: "sqlite::memory:".into(),
                pool_size: 1,
                retry_count: 0,
            },
            aster_drive_metrics::NoopMetrics::arc(),
        )
        .await
        .expect("WebDAV backend test database should connect")
    }

    async fn revalidate_root_with_if_match(
        value: &'static str,
    ) -> Result<(), AsterDavMutationError> {
        let db = test_db().await;
        let path = DavPath::new("/").unwrap();
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::IF_MATCH,
            http::HeaderValue::from_static(value),
        );
        let conditions = DavMutationConditions {
            prefix: "/dav",
            if_header: None,
            request_scheme: "http",
            request_host: "localhost",
            http_headers: &headers,
            http_method: aster_forge_webdav::DavMethod::Delete,
            http_target: &path,
        };

        revalidate_atomic_target(
            &db,
            0,
            WorkspaceStorageScope::Personal { user_id: 1 },
            None,
            AtomicTargetRevalidation {
                path: &path,
                check_locks: false,
                deep: false,
            },
            &conditions,
        )
        .await
    }

    #[tokio::test]
    async fn atomic_target_revalidation_rejects_mismatching_condition_on_root() {
        assert!(matches!(
            revalidate_root_with_if_match("\"missing-root-etag\"").await,
            Err(AsterDavMutationError::PreconditionFailed)
        ));
    }

    #[tokio::test]
    async fn atomic_target_revalidation_accepts_if_match_star_on_root() {
        revalidate_root_with_if_match("*")
            .await
            .expect("If-Match star should match the existing WebDAV root");
    }
}
