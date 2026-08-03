use aster_forge_db::transaction;
use chrono::Utc;
use sea_orm::{ConnectionTrait, DatabaseConnection};

use crate::db::repository::{folder_repo, lock_namespace_repo, lock_repo, team_repo, user_repo};
use crate::errors::{AsterError, Result};
use aster_drive_model::entities::{file, folder, resource_lock};
use aster_drive_model::types::{LockDepth, LockOrigin, LockRootKind};

use super::domain::{LockRoot, LockTarget, LockWorkspace};

#[derive(Debug, Default)]
pub struct SubmittedLockCredentials<'a> {
    pub holder_user_id: Option<i64>,
    pub tokens: &'a [String],
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum LockMutationCredentials {
    #[default]
    None,
    HolderUser(i64),
    SubmittedTokens(Vec<String>),
}

impl LockMutationCredentials {
    #[must_use]
    pub fn submitted(&self) -> SubmittedLockCredentials<'_> {
        match self {
            Self::None => SubmittedLockCredentials::none(),
            Self::HolderUser(user_id) => SubmittedLockCredentials {
                holder_user_id: Some(*user_id),
                tokens: &[],
            },
            Self::SubmittedTokens(tokens) => SubmittedLockCredentials {
                holder_user_id: None,
                tokens,
            },
        }
    }
}

impl SubmittedLockCredentials<'_> {
    #[must_use]
    pub const fn none() -> Self {
        Self {
            holder_user_id: None,
            tokens: &[],
        }
    }
}

pub async fn enforce_file_mutation_on<C: ConnectionTrait>(
    txn: &C,
    file: &file::Model,
    credentials: &SubmittedLockCredentials<'_>,
) -> Result<file::Model> {
    let target = LockTarget {
        workspace: LockWorkspace::from_file(file)?,
        root: LockRoot::File { file_id: file.id },
        depth: LockDepth::Resource,
    };
    let namespace = lock_workspace_for_mutation_on(txn, target.workspace).await?;
    let current = crate::db::repository::file_repo::lock_by_id(txn, file.id).await?;
    if LockWorkspace::from_file(&current)? != target.workspace {
        return Err(AsterError::resource_locked(
            "file workspace changed during mutation lock validation",
        ));
    }
    enforce_mutation_locks_in_namespace_on(
        txn,
        namespace.id,
        target,
        current.folder_id,
        credentials,
    )
    .await?;
    Ok(current)
}

pub async fn enforce_folder_mutation_on<C: ConnectionTrait>(
    txn: &C,
    folder: &folder::Model,
    depth: LockDepth,
    credentials: &SubmittedLockCredentials<'_>,
) -> Result<folder::Model> {
    let target = LockTarget {
        workspace: LockWorkspace::from_folder(folder)?,
        root: LockRoot::Folder {
            folder_id: folder.id,
        },
        depth,
    };
    let namespace = lock_workspace_for_mutation_on(txn, target.workspace).await?;
    let current = folder_repo::lock_by_id(txn, folder.id).await?;
    if LockWorkspace::from_folder(&current)? != target.workspace {
        return Err(AsterError::resource_locked(
            "folder workspace changed during mutation lock validation",
        ));
    }
    enforce_mutation_locks_in_namespace_on(
        txn,
        namespace.id,
        target,
        current.parent_id,
        credentials,
    )
    .await?;
    Ok(current)
}

pub async fn enforce_collection_membership_mutation_on<C: ConnectionTrait>(
    txn: &C,
    workspace: LockWorkspace,
    parent_folder_id: Option<i64>,
    credentials: &SubmittedLockCredentials<'_>,
) -> Result<()> {
    let namespace = lock_workspace_for_mutation_on(txn, workspace).await?;
    let (root, ancestor_parent_id) = match parent_folder_id {
        Some(folder_id) => {
            let parent = folder_repo::lock_by_id(txn, folder_id).await?;
            if LockWorkspace::from_folder(&parent)? != workspace {
                return Err(AsterError::resource_locked(
                    "parent folder workspace changed during membership mutation",
                ));
            }
            (LockRoot::Folder { folder_id }, parent.parent_id)
        }
        None => {
            match workspace {
                LockWorkspace::Personal { user_id } => {
                    user_repo::lock_by_id(txn, user_id).await?;
                }
                LockWorkspace::Team { team_id } => {
                    team_repo::lock_by_id(txn, team_id).await?;
                }
            }
            (LockRoot::WorkspaceRoot, None)
        }
    };
    enforce_mutation_locks_in_namespace_on(
        txn,
        namespace.id,
        LockTarget {
            workspace,
            root,
            depth: LockDepth::Resource,
        },
        ancestor_parent_id,
        credentials,
    )
    .await
}

pub async fn enforce_file_mutation(
    db: &DatabaseConnection,
    file: &file::Model,
    credentials: &SubmittedLockCredentials<'_>,
) -> Result<()> {
    transaction::with_transaction(db, async |txn| {
        enforce_file_mutation_on(txn, file, credentials).await?;
        Ok(())
    })
    .await
}

pub async fn enforce_folder_mutation(
    db: &DatabaseConnection,
    folder: &folder::Model,
    depth: LockDepth,
    credentials: &SubmittedLockCredentials<'_>,
) -> Result<()> {
    transaction::with_transaction(db, async |txn| {
        enforce_folder_mutation_on(txn, folder, depth, credentials).await?;
        Ok(())
    })
    .await
}

pub async fn lock_workspace_for_mutation_on<C: ConnectionTrait>(
    txn: &C,
    workspace: LockWorkspace,
) -> Result<aster_drive_model::entities::resource_lock_namespace::Model> {
    let (workspace_type, workspace_id) = workspace.persistence_key();
    lock_namespace_repo::ensure_and_lock(txn, workspace_type, workspace_id).await
}

async fn enforce_mutation_locks_in_namespace_on<C: ConnectionTrait>(
    txn: &C,
    namespace_id: i64,
    target: LockTarget,
    parent_folder_id: Option<i64>,
    credentials: &SubmittedLockCredentials<'_>,
) -> Result<()> {
    let ancestor_ids = load_ancestor_ids(txn, target.workspace, parent_folder_id).await?;
    let now = Utc::now();

    for lock in lock_repo::find_all_by_namespace_for_update(txn, namespace_id).await? {
        if lock.timeout_at.is_some_and(|expires_at| expires_at < now)
            || !lock_covers_target(&lock, target, &ancestor_ids)
            || lock_is_satisfied(&lock, credentials)
        {
            continue;
        }
        return Err(AsterError::resource_locked("resource is locked"));
    }
    Ok(())
}

async fn load_ancestor_ids<C: ConnectionTrait>(
    db: &C,
    workspace: LockWorkspace,
    parent_folder_id: Option<i64>,
) -> Result<Vec<i64>> {
    let Some(parent_folder_id) = parent_folder_id else {
        return Ok(Vec::new());
    };
    let models = match workspace {
        LockWorkspace::Personal { user_id } => {
            folder_repo::find_ancestor_models(db, user_id, parent_folder_id).await?
        }
        LockWorkspace::Team { team_id } => {
            folder_repo::find_team_ancestor_models(db, team_id, parent_folder_id).await?
        }
    };
    Ok(models.into_iter().map(|folder| folder.id).collect())
}

fn lock_covers_target(
    lock: &resource_lock::Model,
    target: LockTarget,
    ancestor_ids: &[i64],
) -> bool {
    match lock.root_kind {
        LockRootKind::WorkspaceRoot => {
            matches!(target.root, LockRoot::WorkspaceRoot) || lock.depth == LockDepth::Infinity
        }
        LockRootKind::File => {
            matches!(target.root, LockRoot::File { file_id } if lock.root_file_id == Some(file_id))
        }
        LockRootKind::Folder => {
            let direct = matches!(target.root, LockRoot::Folder { folder_id } if lock.root_folder_id == Some(folder_id));
            direct
                || lock.depth == LockDepth::Infinity
                    && lock
                        .root_folder_id
                        .is_some_and(|folder_id| ancestor_ids.contains(&folder_id))
        }
    }
}

fn lock_is_satisfied(
    lock: &resource_lock::Model,
    credentials: &SubmittedLockCredentials<'_>,
) -> bool {
    credentials.tokens.iter().any(|token| token == &lock.token)
        || lock.origin == LockOrigin::Product
            && credentials
                .holder_user_id
                .is_some_and(|user_id| lock.holder_user_id == Some(user_id))
}
