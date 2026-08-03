use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, IntoActiveModel, QueryFilter,
    QuerySelect, Set,
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::resource_lock_namespace;
use aster_drive_model::types::LockWorkspaceType;

pub async fn ensure_and_lock<C: ConnectionTrait>(
    db: &C,
    workspace_type: LockWorkspaceType,
    workspace_id: i64,
) -> Result<resource_lock_namespace::Model> {
    let now = Utc::now();
    resource_lock_namespace::Entity::insert(resource_lock_namespace::ActiveModel {
        workspace_type: Set(workspace_type),
        workspace_id: Set(workspace_id),
        generation: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    })
    .on_conflict_do_nothing_on([
        resource_lock_namespace::Column::WorkspaceType,
        resource_lock_namespace::Column::WorkspaceId,
    ])
    .exec_without_returning(db)
    .await
    .map_err(AsterError::from)?;

    resource_lock_namespace::Entity::find()
        .filter(resource_lock_namespace::Column::WorkspaceType.eq(workspace_type))
        .filter(resource_lock_namespace::Column::WorkspaceId.eq(workspace_id))
        .lock_exclusive()
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::internal_error("resource lock namespace disappeared"))
}

pub async fn find_by_id<C: ConnectionTrait>(
    db: &C,
    namespace_id: i64,
) -> Result<Option<resource_lock_namespace::Model>> {
    resource_lock_namespace::Entity::find_by_id(namespace_id)
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_workspace<C: ConnectionTrait>(
    db: &C,
    workspace_type: LockWorkspaceType,
    workspace_id: i64,
) -> Result<Option<resource_lock_namespace::Model>> {
    resource_lock_namespace::Entity::find()
        .filter(resource_lock_namespace::Column::WorkspaceType.eq(workspace_type))
        .filter(resource_lock_namespace::Column::WorkspaceId.eq(workspace_id))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn lock_by_id<C: ConnectionTrait>(
    db: &C,
    namespace_id: i64,
) -> Result<resource_lock_namespace::Model> {
    resource_lock_namespace::Entity::find_by_id(namespace_id)
        .lock_exclusive()
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::record_not_found("resource lock namespace"))
}

pub async fn increment_generation<C: ConnectionTrait>(
    db: &C,
    namespace: resource_lock_namespace::Model,
) -> Result<resource_lock_namespace::Model> {
    let generation = namespace
        .generation
        .checked_add(1)
        .ok_or_else(|| AsterError::internal_error("resource lock generation overflow"))?;
    let mut active = namespace.into_active_model();
    active.generation = Set(generation);
    active.updated_at = Set(Utc::now());
    active.update(db).await.map_err(AsterError::from)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use sea_orm::{DbBackend, EntityTrait, QueryTrait, Set};

    use super::*;

    #[test]
    fn ensure_namespace_insert_uses_valid_mysql_conflict_syntax() {
        let now = Utc::now();
        let sql = resource_lock_namespace::Entity::insert(resource_lock_namespace::ActiveModel {
            workspace_type: Set(LockWorkspaceType::Personal),
            workspace_id: Set(42),
            generation: Set(0),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        })
        .on_conflict_do_nothing_on([
            resource_lock_namespace::Column::WorkspaceType,
            resource_lock_namespace::Column::WorkspaceId,
        ])
        .build(DbBackend::MySql)
        .to_string();

        assert!(sql.contains("ON DUPLICATE KEY UPDATE `id` = `id`"), "{sql}");
        assert!(!sql.contains("ON DUPLICATE KEY IGNORE"), "{sql}");
    }
}
