//! Database authority shared by trash admission and fenced purge mutations.

use crate::db::repository::background_task_repo;
use aster_drive_model::types::{BackgroundTaskKind, BackgroundTaskStatus};
use aster_forge_tasks::TaskLease;
use sea_orm::ConnectionTrait;

use crate::{
    api::api_error_code::ApiErrorCode,
    errors::{AsterError, Result},
    services::{
        files::lock::{self, LockWorkspace},
        workspace::storage::WorkspaceResourceScope,
    },
};

/// Hold this lock until the enclosing product/admission transaction commits.
pub(crate) async fn lock_scope_on<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceResourceScope,
) -> Result<()> {
    let workspace = match scope {
        WorkspaceResourceScope::Personal { user_id } => LockWorkspace::Personal { user_id },
        WorkspaceResourceScope::Team { team_id } => LockWorkspace::Team { team_id },
    };
    lock::lock_workspace_for_mutation_on(db, workspace).await?;
    Ok(())
}

pub(crate) async fn admit_mutation_on<C: ConnectionTrait>(
    db: &C,
    scope: impl Into<WorkspaceResourceScope>,
) -> Result<()> {
    let scope = scope.into();
    lock_scope_on(db, scope).await?;
    let (workspace_type, workspace_id) = match scope {
        WorkspaceResourceScope::Personal { user_id } => LockWorkspace::Personal { user_id },
        WorkspaceResourceScope::Team { team_id } => LockWorkspace::Team { team_id },
    }
    .persistence_key();
    if background_task_repo::find_active_by_kind_in_workspace(
        db,
        BackgroundTaskKind::TrashPurgeAll,
        workspace_type,
        workspace_id,
    )
    .await?
    .is_some()
    {
        return Err(AsterError::conflict(
            "Trash is being emptied; wait until the task finishes before restoring or deleting items",
        )
        .with_api_error_code(ApiErrorCode::TrashPurgeInProgress));
    }
    Ok(())
}

/// Fence every destructive transaction, including an old worker after lease takeover.
pub(crate) async fn fence_purge_on<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceResourceScope,
    lease: TaskLease,
) -> Result<()> {
    lock_scope_on(db, scope).await?;
    let task = background_task_repo::find_by_id_for_update(db, lease.task_id).await?;
    if task.as_ref().is_some_and(|task| {
        task.processing_token == lease.processing_token
            && task.status == BackgroundTaskStatus::Canceling
    }) {
        return Err(AsterError::conflict("Trash purge cancellation requested")
            .with_api_error_code(ApiErrorCode::TaskCancellationRequested));
    }
    let Some(task) = task.filter(|task| {
        task.processing_token == lease.processing_token
            && task.status == BackgroundTaskStatus::Processing
            && task
                .lease_expires_at
                .is_some_and(|until| until > chrono::Utc::now())
    }) else {
        return Err(
            AsterError::conflict("Trash purge task lease is no longer active")
                .with_api_error_code(ApiErrorCode::TaskLeaseLost),
        );
    };
    let matches_scope = match scope {
        WorkspaceResourceScope::Personal { user_id } => {
            task.team_id.is_none() && task.creator_user_id == Some(user_id)
        }
        WorkspaceResourceScope::Team { team_id } => task.team_id == Some(team_id),
    };
    if task.kind != BackgroundTaskKind::TrashPurgeAll || !matches_scope {
        return Err(AsterError::internal_error(
            "Trash purge fence belongs to another scope or task kind",
        ));
    }
    Ok(())
}

/// Destructive purge callers must state whether they are ordinary mutations, a
/// fenced empty-trash worker, or internal maintenance outside this workflow.
#[derive(Clone, Copy)]
pub(crate) enum PurgeGuard {
    Mutation,
    Task(TaskLease),
    Maintenance,
}

impl PurgeGuard {
    pub(crate) async fn enforce_on<C: ConnectionTrait>(
        self,
        db: &C,
        scope: WorkspaceResourceScope,
    ) -> Result<()> {
        match self {
            Self::Mutation => admit_mutation_on(db, scope).await,
            Self::Task(lease) => fence_purge_on(db, scope, lease).await,
            Self::Maintenance => lock_scope_on(db, scope).await,
        }
    }
}
