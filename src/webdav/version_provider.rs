//! AsterDrive persistence adapter for Forge's DeltaV protocol implementation.

use sea_orm::DatabaseConnection;

use crate::db::repository::{file_repo, user_repo, version_repo};
use crate::services::workspace::storage::WorkspaceStorageScope;
use crate::webdav::auth::WebdavAuthResult;
use crate::webdav::path_resolver::{self, ResolvedNode};
use aster_forge_utils::numbers::i64_to_u64;
use aster_forge_webdav::dav::{DavPath, LsFuture};
use aster_forge_webdav::deltav::{
    DavVersionEntry, DavVersionError, DavVersionHistory, DavVersionProvider,
};

pub(crate) struct AsterDavVersionProvider<'a> {
    db: &'a DatabaseConnection,
    scope: WorkspaceStorageScope,
    root_folder_id: Option<i64>,
}

impl<'a> AsterDavVersionProvider<'a> {
    pub(crate) fn new(db: &'a DatabaseConnection, auth: &WebdavAuthResult) -> Self {
        Self {
            db,
            scope: auth.scope,
            root_folder_id: auth.root_folder_id,
        }
    }
}

impl DavVersionProvider for AsterDavVersionProvider<'_> {
    fn version_history<'a>(
        &'a self,
        path: &'a DavPath,
    ) -> LsFuture<'a, Result<DavVersionHistory, DavVersionError>> {
        Box::pin(async move {
            let file = match path_resolver::resolve_path_in_scope(
                self.db,
                self.scope,
                path,
                self.root_folder_id,
            )
            .await
            .map_err(|_| DavVersionError::NotFound)?
            {
                ResolvedNode::File(file) => file,
                ResolvedNode::Root | ResolvedNode::Folder(_) => {
                    return Err(DavVersionError::NotFile);
                }
            };

            let versions = version_repo::find_by_file_id(self.db, file.id)
                .await
                .map_err(|_| DavVersionError::Backend)?;
            let creator = match file.created_by_user_id {
                Some(user_id) => user_repo::find_by_id(self.db, user_id)
                    .await
                    .map(|user| user.username)
                    .unwrap_or_else(|_| file.created_by_username.clone()),
                None => file.created_by_username.clone(),
            };
            let creator = if creator.is_empty() {
                "unknown".to_string()
            } else {
                creator
            };

            let current = match file_repo::find_blob_by_id(self.db, file.blob_id).await {
                Ok(blob) => Some(DavVersionEntry {
                    id: None,
                    size: i64_to_u64(blob.size, "WebDAV current version size")
                        .map_err(|_| DavVersionError::Backend)?,
                    modified: file.updated_at.into(),
                    creator: creator.clone(),
                }),
                Err(_) => None,
            };

            let blob_ids = versions
                .iter()
                .map(|version| version.blob_id)
                .collect::<Vec<_>>();
            let blobs = file_repo::find_blobs_by_ids(self.db, &blob_ids)
                .await
                .unwrap_or_default();
            let mut previous = Vec::with_capacity(versions.len());
            for version in versions {
                let size = blobs
                    .get(&version.blob_id)
                    .map_or(version.size, |blob| blob.size);
                previous.push(DavVersionEntry {
                    id: Some(version.version.to_string()),
                    size: i64_to_u64(size, "WebDAV historical version size")
                        .map_err(|_| DavVersionError::Backend)?,
                    modified: version.created_at.into(),
                    creator: creator.clone(),
                });
            }

            Ok(DavVersionHistory { current, previous })
        })
    }

    fn validate_version_control<'a>(
        &'a self,
        path: &'a DavPath,
    ) -> LsFuture<'a, Result<(), DavVersionError>> {
        Box::pin(async move {
            match path_resolver::resolve_path_in_scope(
                self.db,
                self.scope,
                path,
                self.root_folder_id,
            )
            .await
            {
                Ok(ResolvedNode::File(_)) => Ok(()),
                Ok(ResolvedNode::Root | ResolvedNode::Folder(_)) => Err(DavVersionError::NotFile),
                Err(_) => Err(DavVersionError::NotFound),
            }
        })
    }
}
