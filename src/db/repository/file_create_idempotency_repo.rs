//! 仓储模块：`file_create_idempotency_repo`。

use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, IntoActiveModel, QueryFilter,
    QueryOrder, QuerySelect, Set,
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::file_create_idempotency::{self, Entity as FileCreateIdempotency};

const EXPIRED_DELETE_BATCH_SIZE: u64 = 1_000;

/// Partition of an idempotency key: claims are unique per actor, workspace and key hash.
#[derive(Clone, Copy, Debug)]
pub struct FileCreateIdempotencyScope {
    /// User performing the create; the same key from a different actor is a different claim.
    pub actor_user_id: i64,
    /// Workspace partition identifier (e.g. personal or team), never a free-form string.
    pub workspace_kind: &'static str,
    /// Id of the workspace within `workspace_kind`.
    pub workspace_id: i64,
}

/// Loads an existing idempotency claim for the scope and hashed key. Returns
/// `None` when no claim exists; claims are matched on the hashed key, never the
/// raw client value.
pub async fn find<C: ConnectionTrait>(
    db: &C,
    scope: FileCreateIdempotencyScope,
    key_hash: &str,
) -> Result<Option<file_create_idempotency::Model>> {
    FileCreateIdempotency::find()
        .filter(file_create_idempotency::Column::ActorUserId.eq(scope.actor_user_id))
        .filter(file_create_idempotency::Column::WorkspaceKind.eq(scope.workspace_kind))
        .filter(file_create_idempotency::Column::WorkspaceId.eq(scope.workspace_id))
        .filter(file_create_idempotency::Column::KeyHash.eq(key_hash))
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// Deletes one claim row by id; succeeds even when the id is absent.
pub async fn delete<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    FileCreateIdempotency::delete_by_id(id)
        .exec(db)
        .await
        .map(|_| ())
        .map_err(AsterError::from)
}

/// Removes expired claims in bounded batches (1000 ids each) ordered by expiry,
/// so a large backlog cannot turn into one long-running delete, and returns the
/// total number of removed rows.
pub async fn delete_expired<C: ConnectionTrait>(db: &C, now: DateTime<Utc>) -> Result<u64> {
    let mut total_deleted = 0_u64;
    loop {
        let ids = FileCreateIdempotency::find()
            .select_only()
            .column(file_create_idempotency::Column::Id)
            .filter(file_create_idempotency::Column::ExpiresAt.lte(now))
            .order_by_asc(file_create_idempotency::Column::ExpiresAt)
            .order_by_asc(file_create_idempotency::Column::Id)
            .limit(EXPIRED_DELETE_BATCH_SIZE)
            .into_tuple::<i64>()
            .all(db)
            .await
            .map_err(AsterError::from)?;
        if ids.is_empty() {
            return Ok(total_deleted);
        }

        let deleted = FileCreateIdempotency::delete_many()
            .filter(file_create_idempotency::Column::Id.is_in(ids))
            .exec(db)
            .await
            .map_err(AsterError::from)?
            .rows_affected;
        total_deleted = total_deleted.checked_add(deleted).ok_or_else(|| {
            AsterError::internal_error("expired file-create idempotency delete count overflow")
        })?;
    }
}

/// Records a new idempotency claim with no result file yet. The scope plus
/// hashed key must be unique; uniqueness violations surface as database errors.
pub async fn create_claim<C: ConnectionTrait>(
    db: &C,
    scope: FileCreateIdempotencyScope,
    key_hash: &str,
    request_fingerprint: &str,
    now: DateTime<Utc>,
    expires_at: DateTime<Utc>,
) -> Result<file_create_idempotency::Model> {
    file_create_idempotency::ActiveModel {
        actor_user_id: Set(scope.actor_user_id),
        workspace_kind: Set(scope.workspace_kind.to_string()),
        workspace_id: Set(scope.workspace_id),
        key_hash: Set(key_hash.to_string()),
        request_fingerprint: Set(request_fingerprint.to_string()),
        result_file_id: Set(None),
        created_at: Set(now),
        expires_at: Set(expires_at),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(AsterError::from)
}

/// Fills in the created file id on a claim once the create succeeded; replays
/// then resolve to this id instead of re-running the create.
pub async fn complete<C: ConnectionTrait>(
    db: &C,
    claim: file_create_idempotency::Model,
    result_file_id: i64,
) -> Result<file_create_idempotency::Model> {
    let mut active = claim.into_active_model();
    active.result_file_id = Set(Some(result_file_id));
    active.update(db).await.map_err(AsterError::from)
}
