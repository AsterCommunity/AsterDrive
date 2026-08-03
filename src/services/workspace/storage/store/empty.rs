use std::sync::Arc;

use chrono::Utc;
use sea_orm::ConnectionTrait;

use crate::db::repository::file_repo;
use crate::errors::Result;
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::events::storage_change;
use crate::services::workspace::storage::{
    PreparedNonDedupBlobUpload, WorkspaceStorageScope, cleanup_preuploaded_blob_upload,
    create_exact_file_from_blob, create_new_file_from_blob, local_content_dedup_enabled,
    persist_preuploaded_blob, prepare_non_dedup_blob_upload, resolve_policy_for_size,
};
use aster_drive_model::entities::{file, file_blob};
use aster_drive_storage::StorageDriver;

const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const EMPTY_SIZE: i64 = 0;

#[derive(Debug, Clone, Copy)]
pub(crate) enum EmptyFileNameMode {
    ResolveUnique,
    Exact,
}

#[derive(Clone)]
enum PreparedEmptyBlob {
    SharedDedup { storage_path: String },
    OwnedNonDedup(PreparedNonDedupBlobUpload),
}

#[derive(Clone)]
pub(crate) struct PreparedEmptyFile {
    scope: WorkspaceStorageScope,
    folder_id: Option<i64>,
    filename: String,
    name_mode: EmptyFileNameMode,
    policy_id: i64,
    driver: Arc<dyn StorageDriver>,
    blob: PreparedEmptyBlob,
}

impl PreparedEmptyFile {
    pub(crate) async fn prepare(
        state: &PrimaryAppState,
        scope: WorkspaceStorageScope,
        folder_id: Option<i64>,
        filename: &str,
        name_mode: EmptyFileNameMode,
    ) -> Result<Self> {
        let filename = aster_forge_validation::filename::normalize_validate_name(filename)?;
        let policy = resolve_policy_for_size(state, scope, folder_id, EMPTY_SIZE).await?;
        let driver = state.driver_registry().get_driver(&policy)?;
        let blob = if local_content_dedup_enabled(&policy) {
            let storage_path =
                aster_forge_validation::filename::storage_path_from_blob_key(EMPTY_SHA256)?;
            if !driver.exists(&storage_path).await? {
                driver.put(&storage_path, &[]).await?;
            }
            PreparedEmptyBlob::SharedDedup { storage_path }
        } else {
            let prepared = prepare_non_dedup_blob_upload(&policy, EMPTY_SIZE, Some(&filename))?;
            if let Err(error) = driver.put(prepared.storage_path(), &[]).await {
                cleanup_preuploaded_blob_upload(
                    driver.as_ref(),
                    &prepared,
                    "empty object upload failure",
                )
                .await;
                return Err(error.into());
            }
            PreparedEmptyBlob::OwnedNonDedup(prepared)
        };

        Ok(Self {
            scope,
            folder_id,
            filename,
            name_mode,
            policy_id: policy.id,
            driver,
            blob,
        })
    }

    pub(crate) fn folder_id(&self) -> Option<i64> {
        self.folder_id
    }

    pub(crate) async fn persist_blob_on<C: ConnectionTrait>(
        &self,
        txn: &C,
    ) -> Result<file_blob::Model> {
        match &self.blob {
            PreparedEmptyBlob::SharedDedup { storage_path } => Ok(file_repo::find_or_create_blob(
                txn,
                EMPTY_SHA256,
                EMPTY_SIZE,
                self.policy_id,
                storage_path,
            )
            .await?
            .model),
            PreparedEmptyBlob::OwnedNonDedup(prepared) => {
                persist_preuploaded_blob(txn, prepared).await
            }
        }
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

    pub(crate) async fn cleanup_after_db_failure(&self, reason: &str) {
        if let PreparedEmptyBlob::OwnedNonDedup(prepared) = &self.blob {
            cleanup_preuploaded_blob_upload(self.driver.as_ref(), prepared, reason).await;
        }
    }

    pub(crate) fn publish_created(&self, state: &PrimaryAppState, created: &file::Model) {
        storage_change::publish(
            state,
            storage_change::StorageChangeEvent::new(
                storage_change::StorageChangeKind::FileCreated,
                self.scope,
                vec![created.id],
                vec![],
                vec![created.folder_id],
            )
            .with_storage_delta(EMPTY_SIZE),
        );
    }
}
