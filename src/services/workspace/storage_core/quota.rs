use sea_orm::ConnectionTrait;

use crate::db::repository::{team_repo, user_repo};
use crate::errors::Result;
use crate::services::workspace::scope::{WorkspaceResourceScope, WorkspaceStorageScope};

/// Locks the authoritative quota row before child rows with owner foreign keys are written.
/// This avoids InnoDB shared-to-exclusive lock upgrades during concurrent file finalization.
pub(crate) async fn lock_storage_usage<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
) -> Result<()> {
    lock_storage_usage_for_resource_scope(db, scope.into()).await
}

pub(crate) async fn lock_storage_usage_for_resource_scope<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceResourceScope,
) -> Result<()> {
    match scope {
        WorkspaceResourceScope::Personal { user_id } => {
            user_repo::lock_by_id(db, user_id).await?;
        }
        WorkspaceResourceScope::Team { team_id } => {
            team_repo::lock_by_id(db, team_id).await?;
        }
    }
    Ok(())
}

pub(crate) async fn check_quota<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    size: i64,
) -> Result<()> {
    match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            user_repo::check_quota(db, user_id, size).await
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            team_repo::check_quota(db, team_id, size).await
        }
    }
}

pub(crate) async fn update_storage_used<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    delta: i64,
) -> Result<()> {
    update_storage_used_for_resource_scope(db, scope.into(), delta).await
}

pub(crate) async fn update_storage_used_for_resource_scope<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceResourceScope,
    delta: i64,
) -> Result<()> {
    match scope {
        WorkspaceResourceScope::Personal { user_id } => {
            user_repo::update_storage_used(db, user_id, delta).await
        }
        WorkspaceResourceScope::Team { team_id } => {
            team_repo::update_storage_used(db, team_id, delta).await
        }
    }
}
