use aster_drive_model::entities::{file, file_blob, file_revision, file_revision_history};
use aster_forge_webdav::{
    DavBackendError, DavBackendErrorKind, DavResourceState, DavVersioningState, FsError,
};
use sea_orm::ConnectionTrait;

use crate::db::repository::{file_repo, folder_repo, revision_repo};
use crate::services::workspace::storage::WorkspaceStorageScope;

use super::path_resolver::ResolvedNode;
use super::{AsterDavFs, AsterDavMeta, path_resolver};
use aster_forge_webdav::DavProp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeltavCapabilityTarget {
    pub(crate) resource: DavResourceState,
    pub(crate) versioning: DavVersioningState,
    pub(crate) reserved_unmapped: bool,
}

impl DeltavCapabilityTarget {
    const fn regular(resource: DavResourceState, versioning: DavVersioningState) -> Self {
        Self {
            resource,
            versioning,
            reserved_unmapped: false,
        }
    }

    const fn reserved_unmapped() -> Self {
        Self {
            resource: DavResourceState::Unmapped,
            versioning: DavVersioningState::Unsupported,
            reserved_unmapped: true,
        }
    }
}

impl From<DavResourceState> for DeltavCapabilityTarget {
    fn from(resource: DavResourceState) -> Self {
        Self::regular(resource, DavVersioningState::Unsupported)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AuthorizedDeltavRevision {
    pub(crate) file: file::Model,
    pub(crate) blob: file_blob::Model,
    pub(crate) history: file_revision_history::Model,
    pub(crate) revision: file_revision::Model,
}

#[derive(Debug, Clone)]
pub(crate) struct DeltavHistoryTarget {
    pub(crate) file: file::Model,
    pub(crate) history: file_revision_history::Model,
    pub(crate) selected_revision: Option<file_revision::Model>,
}

fn backend_error(error: impl std::fmt::Display) -> DavBackendError {
    tracing::warn!(error = %error, "DeltaV backend operation failed");
    DavBackendError::new(DavBackendErrorKind::Internal)
}

async fn ensure_deltav_file_visible_for_scope_on<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    root_folder_id: Option<i64>,
    file: &file::Model,
) -> crate::errors::Result<()> {
    crate::services::workspace::storage::ensure_active_file_scope(file, scope).map_err(|_| {
        crate::errors::AsterError::record_not_found("DeltaV file is outside the active scope")
    })?;
    let Some(root_folder_id) = root_folder_id else {
        return Ok(());
    };
    let Some(folder_id) = file.folder_id else {
        return Err(crate::errors::AsterError::record_not_found(
            "DeltaV file is outside the mounted root",
        ));
    };
    let ancestors = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            folder_repo::find_ancestor_models(db, user_id, folder_id).await
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            folder_repo::find_team_ancestor_models(db, team_id, folder_id).await
        }
    }?;
    if ancestors.iter().any(|folder| folder.deleted_at.is_some())
        || !ancestors.iter().any(|folder| folder.id == root_folder_id)
    {
        return Err(crate::errors::AsterError::record_not_found(
            "DeltaV file is outside the mounted root",
        ));
    }
    Ok(())
}

impl AsterDavFs {
    async fn ensure_deltav_file_visible_on<C: ConnectionTrait>(
        &self,
        db: &C,
        file: &file::Model,
    ) -> Result<(), DavBackendError> {
        ensure_deltav_file_visible_for_scope_on(db, self.scope, self.root_folder_id, file)
            .await
            .map_err(|error| {
                if matches!(error, crate::errors::AsterError::RecordNotFound(_)) {
                    DavBackendError::new(DavBackendErrorKind::NotFound)
                } else {
                    backend_error(error)
                }
            })
    }

    pub(crate) async fn deltav_capability_target(
        &self,
        path: &aster_forge_webdav::DavPath,
    ) -> Result<DeltavCapabilityTarget, DavBackendError> {
        match crate::webdav::deltav::classify_reserved_path(path) {
            crate::webdav::deltav::ReservedDeltavPath::Version(public_id) => {
                self.load_deltav_revision(&public_id).await?;
                return Ok(DeltavCapabilityTarget::regular(
                    DavResourceState::File,
                    DavVersioningState::Version,
                ));
            }
            crate::webdav::deltav::ReservedDeltavPath::Reserved => {
                return Ok(DeltavCapabilityTarget::reserved_unmapped());
            }
            crate::webdav::deltav::ReservedDeltavPath::Ordinary => {}
        }

        match path_resolver::resolve_path_cached_in_scope(
            &self.state,
            self.scope,
            path,
            self.root_folder_id,
        )
        .await
        {
            Ok(ResolvedNode::Root) => Ok(DeltavCapabilityTarget::regular(
                DavResourceState::MountRoot,
                DavVersioningState::Unsupported,
            )),
            Ok(ResolvedNode::Folder(_)) => Ok(DeltavCapabilityTarget::regular(
                DavResourceState::Collection,
                DavVersioningState::Unsupported,
            )),
            Ok(ResolvedNode::File(file)) => {
                let history =
                    revision_repo::find_history_by_file_id(self.state.writer_db(), file.id)
                        .await
                        .map_err(backend_error)?;
                Ok(DeltavCapabilityTarget::regular(
                    DavResourceState::File,
                    if history.deltav_controlled_at.is_some() {
                        DavVersioningState::CheckedIn
                    } else {
                        DavVersioningState::Versionable
                    },
                ))
            }
            Err(FsError::NotFound) => Ok(DeltavCapabilityTarget::regular(
                DavResourceState::Unmapped,
                DavVersioningState::Unsupported,
            )),
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) async fn load_deltav_revision(
        &self,
        public_id: &str,
    ) -> Result<AuthorizedDeltavRevision, DavBackendError> {
        self.load_deltav_revision_on(self.state.writer_db(), public_id)
            .await
    }

    pub(crate) async fn load_deltav_revision_on<C: ConnectionTrait>(
        &self,
        db: &C,
        public_id: &str,
    ) -> Result<AuthorizedDeltavRevision, DavBackendError> {
        let target = revision_repo::find_deltav_revision_by_public_id(db, public_id)
            .await
            .map_err(backend_error)?
            .ok_or_else(|| DavBackendError::new(DavBackendErrorKind::NotFound))?;
        self.ensure_deltav_file_visible_on(db, &target.file).await?;
        let blob_id = target
            .revision
            .blob_id
            .ok_or_else(|| DavBackendError::new(DavBackendErrorKind::NotFound))?;
        let blob = file_repo::find_blob_by_id(db, blob_id)
            .await
            .map_err(|error| {
                if matches!(error, crate::errors::AsterError::RecordNotFound(_)) {
                    DavBackendError::new(DavBackendErrorKind::NotFound)
                } else {
                    backend_error(error)
                }
            })?;
        Ok(AuthorizedDeltavRevision {
            file: target.file,
            blob,
            history: target.history,
            revision: target.revision,
        })
    }

    pub(crate) async fn deltav_history_target(
        &self,
        path: &aster_forge_webdav::DavPath,
    ) -> Result<DeltavHistoryTarget, DavBackendError> {
        if let crate::webdav::deltav::ReservedDeltavPath::Version(public_id) =
            crate::webdav::deltav::classify_reserved_path(path)
        {
            let target = self.load_deltav_revision(&public_id).await?;
            return Ok(DeltavHistoryTarget {
                file: target.file,
                history: target.history,
                selected_revision: Some(target.revision),
            });
        }
        let node = path_resolver::resolve_path_cached_in_scope(
            &self.state,
            self.scope,
            path,
            self.root_folder_id,
        )
        .await
        .map_err(DavBackendError::from)?;
        let ResolvedNode::File(file) = node else {
            return Err(DavBackendError::new(DavBackendErrorKind::NotFound));
        };
        let history = revision_repo::find_history_by_file_id(self.state.writer_db(), file.id)
            .await
            .map_err(backend_error)?;
        if history.deltav_controlled_at.is_none() {
            return Err(DavBackendError::new(DavBackendErrorKind::Conflict));
        }
        Ok(DeltavHistoryTarget {
            file,
            history,
            selected_revision: None,
        })
    }

    pub(crate) async fn deltav_current_revision(
        &self,
        file_id: i64,
    ) -> Result<file_revision::Model, DavBackendError> {
        revision_repo::find_current_by_file_id(self.state.writer_db(), file_id)
            .await
            .map_err(backend_error)
    }

    pub(crate) async fn deltav_revisions(
        &self,
        history: &file_revision_history::Model,
        limit: u64,
    ) -> Result<Vec<file_revision::Model>, DavBackendError> {
        revision_repo::find_deltav_revisions(self.state.writer_db(), history, limit)
            .await
            .map_err(backend_error)
    }

    pub(crate) async fn deltav_revision_properties(
        &self,
        revision_ids: &[i64],
    ) -> Result<
        std::collections::HashMap<
            i64,
            Vec<aster_drive_model::entities::file_revision_property::Model>,
        >,
        DavBackendError,
    > {
        revision_repo::find_properties_by_revision_ids(self.state.writer_db(), revision_ids)
            .await
            .map_err(backend_error)
    }

    pub(crate) async fn deltav_metadata(
        &self,
        path: &aster_forge_webdav::DavPath,
    ) -> Result<AsterDavMeta, DavBackendError> {
        let crate::webdav::deltav::ReservedDeltavPath::Version(public_id) =
            crate::webdav::deltav::classify_reserved_path(path)
        else {
            return Err(DavBackendError::new(DavBackendErrorKind::NotFound));
        };
        let target = self.load_deltav_revision(&public_id).await?;
        Ok(AsterDavMeta::from_revision(&target.file, &target.revision))
    }

    pub(crate) async fn deltav_dead_properties(
        &self,
        path: &aster_forge_webdav::DavPath,
    ) -> Result<Vec<DavProp>, DavBackendError> {
        let crate::webdav::deltav::ReservedDeltavPath::Version(public_id) =
            crate::webdav::deltav::classify_reserved_path(path)
        else {
            return Err(DavBackendError::new(DavBackendErrorKind::NotFound));
        };
        let target = self.load_deltav_revision(&public_id).await?;
        let properties = self
            .deltav_revision_properties(&[target.revision.id])
            .await?;
        Ok(properties
            .get(&target.revision.id)
            .into_iter()
            .flatten()
            .map(|property| DavProp {
                name: property.name.clone(),
                prefix: None,
                namespace: (!property.namespace.is_empty()).then(|| property.namespace.clone()),
                xml: property
                    .xml_value
                    .as_ref()
                    .map(|value| value.as_bytes().to_vec()),
            })
            .collect())
    }

    pub(crate) async fn activate_deltav(
        &self,
        path: &aster_forge_webdav::DavPath,
    ) -> Result<file_revision_history::Model, DavBackendError> {
        let node = path_resolver::resolve_path_in_scope(
            self.state.writer_db(),
            self.scope,
            path,
            self.root_folder_id,
        )
        .await
        .map_err(DavBackendError::from)?;
        let ResolvedNode::File(file) = node else {
            return Err(DavBackendError::new(DavBackendErrorKind::NotFound));
        };
        let file_id = file.id;
        let scope = self.scope;
        let root_folder_id = self.root_folder_id;
        aster_forge_db::transaction::with_transaction_retry(
            self.state.writer_db(),
            &aster_forge_db::retry::RetryConfig::deadlock(),
            move |txn| {
                Box::pin(async move {
                    let file = file_repo::find_by_id(txn, file_id).await?;
                    ensure_deltav_file_visible_for_scope_on(txn, scope, root_folder_id, &file)
                        .await?;
                    revision_repo::activate_deltav(txn, file_id).await
                })
            },
            |error: &crate::errors::AsterError| {
                error
                    .database_error_kind()
                    .is_some_and(aster_forge_db::DatabaseErrorKind::is_transient_locking)
            },
        )
        .await
        .map_err(backend_error)
    }
}
