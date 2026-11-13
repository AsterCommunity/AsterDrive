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
    PendingBlob { blob_id: i64, processing_token: i64 },
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
            Some(Resource::Folder(_) | Resource::PendingBlob { .. }) => Err(
                AsterError::internal_error("task publication resource kind mismatch"),
            ),
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
            Some(Resource::File(_) | Resource::PendingBlob { .. }) => Err(
                AsterError::internal_error("task publication resource kind mismatch"),
            ),
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

    /// Called in the same transaction that creates the zero-reference cleanup row.
    pub async fn insert_blob_attempt<C: ConnectionTrait>(
        &self,
        db: &C,
        blob_id: i64,
    ) -> Result<()> {
        Self::new(self.lease, &format!("blob-attempt:{blob_id}"))
            .insert(
                db,
                Resource::PendingBlob {
                    blob_id,
                    processing_token: self.lease.processing_token,
                },
            )
            .await
    }

    /// Retire the execution receipt in the transaction that publishes its product result.
    pub async fn finish_blob_attempt<C: ConnectionTrait>(
        &self,
        db: &C,
        blob_id: i64,
    ) -> Result<()> {
        let attempt = Self::new(self.lease, &format!("blob-attempt:{blob_id}"));
        task_resource_publication::Entity::delete_by_id((self.lease.task_id, attempt.key))
            .exec(db)
            .await?;
        Ok(())
    }
}

/// GC must not remove an external write still owned by a valid task execution.
pub(crate) async fn blob_has_active_task_owner<C: ConnectionTrait>(
    db: &C,
    blob_id: i64,
) -> Result<bool> {
    let key = hex::encode(Sha256::digest(format!("blob-attempt:{blob_id}").as_bytes()));
    let Some(row) = task_resource_publication::Entity::find()
        .filter(task_resource_publication::Column::EntryKey.eq(key))
        .one(db)
        .await?
    else {
        return Ok(false);
    };
    let Resource::PendingBlob {
        blob_id: recorded,
        processing_token,
    } = decode_snapshot(&row.resource_snapshot)?.resource
    else {
        return Err(AsterError::internal_error(
            "invalid blob attempt receipt kind",
        ));
    };
    if recorded != blob_id {
        return Err(AsterError::internal_error(
            "invalid blob attempt receipt identity",
        ));
    }
    Ok(background_task::Entity::find_by_id(row.task_id)
        .one(db)
        .await?
        .is_some_and(|task| {
            task.status == BackgroundTaskStatus::Processing
                && task.processing_token == processing_token
                && task
                    .lease_expires_at
                    .is_some_and(|expiry| expiry > chrono::Utc::now())
        }))
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

#[cfg(test)]
mod tests {
    use super::*;
    use aster_drive_model::types::{BackgroundTaskKind, StoredTaskPayload};
    use chrono::{Duration, Utc};
    use sea_orm::IntoActiveModel;

    #[tokio::test]
    async fn pending_blob_is_protected_only_by_its_live_processing_lease() {
        let state = crate::runtime::tasks::test_support::setup_primary_state().await;
        let db = state.writer_db();
        let now = Utc::now();
        let task = background_task::ActiveModel {
            kind: Set(BackgroundTaskKind::ArchiveCompress),
            status: Set(BackgroundTaskStatus::Processing),
            display_name: Set("pending blob owner".into()),
            payload_json: Set(StoredTaskPayload("{}".into())),
            processing_token: Set(7),
            lease_expires_at: Set(Some(now + Duration::seconds(60))),
            next_run_at: Set(now),
            expires_at: Set(now + Duration::hours(1)),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .unwrap();
        let publication = TaskResourcePublication::new(
            TaskLease {
                task_id: task.id,
                processing_token: 7,
            },
            "archive",
        );
        publication.insert_blob_attempt(db, 42).await.unwrap();
        assert!(blob_has_active_task_owner(db, 42).await.unwrap());
        assert!(!blob_has_active_task_owner(db, 43).await.unwrap());
        for (token, status, expiry) in [
            (
                8,
                BackgroundTaskStatus::Processing,
                now + Duration::seconds(60),
            ),
            (
                7,
                BackgroundTaskStatus::Succeeded,
                now + Duration::seconds(60),
            ),
            (
                7,
                BackgroundTaskStatus::Processing,
                now - Duration::seconds(1),
            ),
        ] {
            let mut update = task.clone().into_active_model();
            update.processing_token = Set(token);
            update.status = Set(status);
            update.lease_expires_at = Set(Some(expiry));
            update.update(db).await.unwrap();
            assert!(!blob_has_active_task_owner(db, 42).await.unwrap());
        }
    }
}
