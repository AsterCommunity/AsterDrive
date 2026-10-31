//! Product output receipts. Receipt, file, blob reference, revision and quota commit together.
use crate::errors::{AsterError, Result};
use aster_drive_model::{
    entities::{background_task, file, folder, task_resource_publication},
    types::BackgroundTaskStatus,
};
use aster_forge_tasks::TaskLease;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect, Set,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone)]
pub(crate) struct TaskResourcePublication {
    pub lease: TaskLease,
    key: String,
    metadata: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    version: u8,
    resource: Resource,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum Resource {
    File(file::Model),
    Folder(folder::Model),
}

impl TaskResourcePublication {
    pub fn new(lease: TaskLease, entry: &str) -> Self {
        Self {
            lease,
            key: hex::encode(Sha256::digest(entry.as_bytes())),
            metadata: None,
        }
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    async fn load_snapshot<C: ConnectionTrait>(&self, db: &C) -> Result<Option<Snapshot>> {
        let row =
            task_resource_publication::Entity::find_by_id((self.lease.task_id, self.key.clone()))
                .one(db)
                .await?;
        row.map(|row| decode_snapshot(&row.resource_snapshot))
            .transpose()
    }

    pub fn entry_key(&self) -> &str {
        &self.key
    }

    pub async fn published_file_keys<C: ConnectionTrait>(
        db: &C,
        task_id: i64,
    ) -> Result<std::collections::HashSet<String>> {
        let rows = task_resource_publication::Entity::find()
            .filter(task_resource_publication::Column::TaskId.eq(task_id))
            .all(db)
            .await?;
        rows.into_iter()
            .filter_map(|row| {
                decode_snapshot(&row.resource_snapshot)
                    .map(|snapshot| {
                        matches!(snapshot.resource, Resource::File(_)).then_some(row.entry_key)
                    })
                    .transpose()
            })
            .collect()
    }

    pub async fn metadata<C: ConnectionTrait>(&self, db: &C) -> Result<Option<serde_json::Value>> {
        Ok(self
            .load_snapshot(db)
            .await?
            .and_then(|snapshot| snapshot.metadata))
    }

    async fn find<C: ConnectionTrait>(&self, db: &C) -> Result<Option<Resource>> {
        Ok(self
            .load_snapshot(db)
            .await?
            .map(|snapshot| snapshot.resource))
    }

    pub async fn find_file<C: ConnectionTrait>(&self, db: &C) -> Result<Option<file::Model>> {
        match self.find(db).await? {
            None => Ok(None),
            Some(Resource::File(file)) => {
                if file::Entity::find_by_id(file.id)
                    .one(db)
                    .await?
                    .is_some_and(|current| current.deleted_at.is_some())
                {
                    return Err(AsterError::file_not_found(format!("file #{}", file.id)));
                }
                Ok(Some(file))
            }
            Some(Resource::Folder(_)) => Err(AsterError::internal_error(
                "task publication resource kind mismatch",
            )),
        }
    }
    pub async fn find_folder<C: ConnectionTrait>(&self, db: &C) -> Result<Option<folder::Model>> {
        match self.find(db).await? {
            None => Ok(None),
            Some(Resource::Folder(folder)) => {
                if folder::Entity::find_by_id(folder.id)
                    .one(db)
                    .await?
                    .is_some_and(|current| current.deleted_at.is_some())
                {
                    return Err(AsterError::folder_not_found(format!(
                        "folder #{}",
                        folder.id
                    )));
                }
                Ok(Some(folder))
            }
            Some(Resource::File(_)) => Err(AsterError::internal_error(
                "task publication resource kind mismatch",
            )),
        }
    }

    /// Serializes publication with task takeover. A stale producer must roll its product transaction back.
    pub async fn lock_owner<C: ConnectionTrait>(&self, db: &C) -> Result<()> {
        let mut query = background_task::Entity::find_by_id(self.lease.task_id)
            .filter(background_task::Column::ProcessingToken.eq(self.lease.processing_token))
            .filter(background_task::Column::Status.eq(BackgroundTaskStatus::Processing));
        if db.get_database_backend() != sea_orm::DbBackend::Sqlite {
            query = query.lock_exclusive();
        }
        if query.one(db).await?.is_none() {
            return Err(aster_forge_tasks::TaskCoreError::LeaseLost {
                task_id: self.lease.task_id,
                processing_token: self.lease.processing_token,
            }
            .into());
        }
        Ok(())
    }

    async fn insert<C: ConnectionTrait>(&self, db: &C, resource: Resource) -> Result<()> {
        let snapshot = serde_json::to_string(&Snapshot {
            version: 1,
            resource,
            metadata: self.metadata.clone(),
        })
        .map_err(|_| AsterError::internal_error("serialize task file receipt"))?;
        task_resource_publication::ActiveModel {
            task_id: Set(self.lease.task_id),
            entry_key: Set(self.key.clone()),
            resource_snapshot: Set(snapshot),
        }
        .insert(db)
        .await?;
        Ok(())
    }
    pub async fn insert_file<C: ConnectionTrait>(&self, db: &C, file: &file::Model) -> Result<()> {
        self.insert(db, Resource::File(file.clone())).await
    }
    pub async fn insert_folder<C: ConnectionTrait>(
        &self,
        db: &C,
        folder: &folder::Model,
    ) -> Result<()> {
        self.insert(db, Resource::Folder(folder.clone())).await
    }
}

fn decode_snapshot(raw: &str) -> Result<Snapshot> {
    let snapshot: Snapshot = serde_json::from_str(raw)
        .map_err(|_| AsterError::internal_error("invalid persisted task publication snapshot"))?;
    if snapshot.version != 1 {
        return Err(AsterError::internal_error(
            "unsupported task publication snapshot version",
        ));
    }
    Ok(snapshot)
}
