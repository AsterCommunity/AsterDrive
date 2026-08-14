use chrono::Utc;
use sea_orm::ConnectionTrait;

use crate::db::repository::file_repo;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::events::storage_change;
use crate::services::workspace::storage::{
    WorkspaceStorageScope, create_exact_file_from_blob, create_new_file_from_blob,
    resolve_policy_for_size,
};
use aster_drive_model::entities::{file, file_blob};

const EMPTY_SIZE: i64 = 0;

#[derive(Debug, Clone, Copy)]
pub(crate) enum EmptyFileNameMode {
    ResolveUnique,
    Exact,
}

#[derive(Clone)]
pub(crate) struct PreparedEmptyFile {
    scope: WorkspaceStorageScope,
    folder_id: Option<i64>,
    filename: String,
    name_mode: EmptyFileNameMode,
    policy_id: i64,
}

impl PreparedEmptyFile {
    pub(crate) async fn prepare(
        state: &PrimaryAppState,
        scope: WorkspaceStorageScope,
        folder_id: Option<i64>,
        filename: &str,
        name_mode: EmptyFileNameMode,
    ) -> Result<Self> {
        let policy = resolve_policy_for_size(state, scope, folder_id, EMPTY_SIZE).await?;
        Self::with_resolved_policy(scope, folder_id, filename, name_mode, policy.id)
    }

    pub(crate) fn with_resolved_policy(
        scope: WorkspaceStorageScope,
        folder_id: Option<i64>,
        filename: &str,
        name_mode: EmptyFileNameMode,
        policy_id: i64,
    ) -> Result<Self> {
        let filename = aster_forge_validation::filename::normalize_validate_name(filename)?;
        Ok(Self {
            scope,
            folder_id,
            filename,
            name_mode,
            policy_id,
        })
    }

    pub(crate) fn folder_id(&self) -> Option<i64> {
        self.folder_id
    }

    pub(crate) async fn persist_blob_on<C: ConnectionTrait>(
        &self,
        txn: &C,
    ) -> Result<file_blob::Model> {
        Ok(file_repo::find_or_create_virtual_empty_blob(
            txn,
            file_blob::Model::EMPTY_SHA256,
            self.policy_id,
        )
        .await?
        .model)
    }

    pub(crate) async fn create_file_on<C: ConnectionTrait>(
        &self,
        txn: &C,
        blob: &file_blob::Model,
    ) -> Result<file::Model> {
        match self.name_mode {
            EmptyFileNameMode::ResolveUnique => {
                create_new_file_from_blob(
                    txn,
                    self.scope,
                    self.folder_id,
                    &self.filename,
                    blob,
                    Utc::now(),
                )
                .await
            }
            EmptyFileNameMode::Exact => {
                create_exact_file_from_blob(
                    txn,
                    self.scope,
                    self.folder_id,
                    &self.filename,
                    blob,
                    Utc::now(),
                )
                .await
            }
        }
    }

    pub(crate) fn publish_created(&self, state: &PrimaryAppState, created: &file::Model) {
        publish_empty_file_created(state, self.scope, created);
    }
}

pub(crate) fn publish_empty_file_created(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    created: &file::Model,
) {
    storage_change::publish(
        state,
        storage_change::StorageChangeEvent::new(
            storage_change::StorageChangeKind::FileCreated,
            scope,
            vec![created.id],
            vec![],
            vec![created.folder_id],
        )
        .with_storage_delta(EMPTY_SIZE),
    );
}
