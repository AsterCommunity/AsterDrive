use aster_forge_db::transaction;
use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, DatabaseConnection, IntoActiveModel, Set};

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{
    file_repo, folder_repo, lock_namespace_repo, lock_repo, team_repo, user_repo,
};
use crate::errors::{AsterError, Result, auth_forbidden_with_code};
use crate::runtime::SharedRuntimeState;
use crate::services::ops::audit::{self, AuditContext};
use aster_drive_model::entities::{resource_lock, resource_lock_namespace};
use aster_drive_model::types::{EntityType, LockDepth, LockMode, LockOrigin, LockWorkspaceType};

use super::domain::{LockRoot, LockTarget, LockWorkspace};
use super::models::ResourceLockOwnerInfo;
use super::owner_info::serialize_resource_lock_owner_info;
use super::ownership::check_entity_ownership;
use super::path::resolve_entity_path;

#[derive(Debug, Clone)]
pub(crate) struct LockAcquireCommand {
    pub target: LockTarget,
    pub mode: LockMode,
    pub origin: LockOrigin,
    pub holder_user_id: Option<i64>,
    pub owner_info: Option<ResourceLockOwnerInfo>,
    pub timeout: Option<Duration>,
    pub presentation_path: Option<String>,
}

/// Acquire an exclusive, resource-depth product lock for a file or folder.
pub async fn lock(
    state: &impl SharedRuntimeState,
    entity_type: EntityType,
    entity_id: i64,
    owner_id: Option<i64>,
    owner_info: Option<ResourceLockOwnerInfo>,
    timeout: Option<Duration>,
) -> Result<resource_lock::Model> {
    let target = resolve_entity_target(state.writer_db(), entity_type, entity_id).await?;
    let path = resolve_entity_path(state.writer_db(), entity_type, entity_id).await?;
    acquire(
        state,
        target,
        LockMode::Exclusive,
        origin_for_owner_info(owner_info.as_ref()),
        owner_id,
        owner_info,
        timeout,
        Some(path),
    )
    .await
}

#[expect(
    clippy::too_many_arguments,
    reason = "lock acquisition keeps each persisted domain dimension explicit"
)]
pub async fn acquire(
    state: &impl SharedRuntimeState,
    target: LockTarget,
    mode: LockMode,
    origin: LockOrigin,
    holder_user_id: Option<i64>,
    owner_info: Option<ResourceLockOwnerInfo>,
    timeout: Option<Duration>,
    presentation_path: Option<String>,
) -> Result<resource_lock::Model> {
    acquire_on(
        state.writer_db(),
        target,
        mode,
        origin,
        holder_user_id,
        owner_info,
        timeout,
        presentation_path,
    )
    .await
}

#[expect(
    clippy::too_many_arguments,
    reason = "transactional acquisition mirrors the complete lock command without a duplicate DTO"
)]
pub async fn acquire_on(
    db: &DatabaseConnection,
    target: LockTarget,
    mode: LockMode,
    origin: LockOrigin,
    holder_user_id: Option<i64>,
    owner_info: Option<ResourceLockOwnerInfo>,
    timeout: Option<Duration>,
    presentation_path: Option<String>,
) -> Result<resource_lock::Model> {
    let txn = transaction::begin(db).await?;
    let namespace = lock_target_namespace(&txn, target.workspace).await?;
    let result = acquire_after_namespace_lock_on(
        &txn,
        namespace,
        LockAcquireCommand {
            target,
            mode,
            origin,
            holder_user_id,
            owner_info,
            timeout,
            presentation_path,
        },
    )
    .await;

    match result {
        Ok(lock) => {
            transaction::commit(txn).await?;
            tracing::debug!(
                lock_id = lock.id,
                namespace_id = lock.namespace_id,
                root_kind = ?lock.root_kind,
                holder_user_id = lock.holder_user_id,
                timeout_at = ?lock.timeout_at,
                "locked resource"
            );
            Ok(lock)
        }
        Err(error) => Err(error),
    }
}

pub(crate) async fn acquire_after_namespace_lock_on<C: sea_orm::ConnectionTrait>(
    db: &C,
    namespace: resource_lock_namespace::Model,
    command: LockAcquireCommand,
) -> Result<resource_lock::Model> {
    let LockAcquireCommand {
        target,
        mode,
        origin,
        holder_user_id,
        owner_info,
        timeout,
        presentation_path,
    } = command;
    let now = Utc::now();
    let timeout_at = timeout.map(|duration| now + duration);
    let serialized_owner_info = serialize_resource_lock_owner_info(owner_info.as_ref())?;
    let token = format!("urn:uuid:{}", uuid::Uuid::new_v4());

    let expected_namespace_key = target.workspace.persistence_key();
    if (namespace.workspace_type, namespace.workspace_id) != expected_namespace_key {
        return Err(AsterError::resource_locked(
            "lock namespace does not match the requested workspace",
        ));
    }
    lock_and_revalidate_target(db, target).await?;

    let mut projection_changed = false;
    for existing in lock_repo::find_all_by_namespace_for_update(db, namespace.id).await? {
        if existing
            .timeout_at
            .is_some_and(|expires_at| expires_at < now)
        {
            lock_repo::delete_by_id(db, existing.id).await?;
            projection_changed = true;
            continue;
        }

        if locks_overlap(&existing, target, presentation_path.as_deref())
            && (mode == LockMode::Exclusive || existing.mode == LockMode::Exclusive)
        {
            return Err(AsterError::resource_locked("resource is already locked"));
        }
    }

    let (root_kind, root_folder_id, root_file_id) = target.root.persistence_parts();
    let lock = lock_repo::create(
        db,
        resource_lock::ActiveModel {
            token: Set(token),
            namespace_id: Set(namespace.id),
            root_kind: Set(root_kind),
            root_folder_id: Set(root_folder_id),
            root_file_id: Set(root_file_id),
            depth: Set(target.depth),
            mode: Set(mode),
            origin: Set(origin),
            holder_user_id: Set(holder_user_id),
            owner_info: Set(serialized_owner_info),
            timeout_at: Set(timeout_at),
            lockroot_path: Set(presentation_path),
            created_at: Set(now),
            ..Default::default()
        },
    )
    .await?;
    increment_generation(db, namespace).await?;
    if projection_changed {
        tracing::debug!(
            namespace_id = lock.namespace_id,
            "expired lock replacement updated the namespace projection"
        );
    }
    Ok(lock)
}

/// Unlock all direct locks the actor may release from one file or folder.
pub async fn unlock(
    state: &impl SharedRuntimeState,
    entity_type: EntityType,
    entity_id: i64,
    user_id: i64,
) -> Result<()> {
    let target = resolve_entity_target(state.writer_db(), entity_type, entity_id).await?;
    transaction::with_transaction(state.writer_db(), async |txn| {
        let namespace = lock_target_namespace(txn, target.workspace).await?;
        lock_and_revalidate_target(txn, target).await?;

        let now = Utc::now();
        let expired =
            lock_repo::delete_expired_by_entity_before(txn, entity_type, entity_id, now).await?;
        let locks = lock_repo::find_all_by_entity_for_update(txn, entity_type, entity_id).await?;
        let mut removed = expired;
        if !locks.is_empty() {
            if check_entity_ownership(txn, entity_type, entity_id, user_id).await? {
                removed = removed
                    .saturating_add(usize_to_u64(locks.len(), "resource lock delete count")?);
                lock_repo::delete_by_entity(txn, entity_type, entity_id).await?;
            } else {
                if locks
                    .iter()
                    .any(|lock| lock.holder_user_id != Some(user_id))
                {
                    return Err(auth_forbidden_with_code(
                        ApiErrorCode::LockNotOwner,
                        "not the lock owner",
                    ));
                }
                removed = removed
                    .saturating_add(usize_to_u64(locks.len(), "resource lock delete count")?);
                lock_repo::delete_by_entity_and_owner(txn, entity_type, entity_id, user_id).await?;
            }
        }
        if removed != 0 {
            increment_generation(txn, namespace).await?;
        }
        Ok(())
    })
    .await?;

    tracing::debug!(entity_type = ?entity_type, entity_id, user_id, "unlocked resource");
    Ok(())
}

pub async fn unlock_by_token(state: &impl SharedRuntimeState, token: &str) -> Result<()> {
    unlock_by_token_on(state.writer_db(), token).await?;
    Ok(())
}

pub async fn unlock_by_token_on(
    db: &DatabaseConnection,
    token: &str,
) -> Result<resource_lock::Model> {
    let snapshot = lock_repo::find_by_token(db, token)
        .await?
        .ok_or_else(|| AsterError::record_not_found("lock not found"))?;
    let lock = release_snapshot_on(db, snapshot, LockSelector::Token(token)).await?;
    tracing::debug!(
        lock_id = lock.id,
        namespace_id = lock.namespace_id,
        root_kind = ?lock.root_kind,
        "unlocked resource by token"
    );
    Ok(lock)
}

pub async fn force_unlock(state: &impl SharedRuntimeState, lock_id: i64) -> Result<()> {
    let snapshot = lock_repo::find_by_id(state.writer_db(), lock_id)
        .await?
        .ok_or_else(|| AsterError::record_not_found("lock not found"))?;
    let lock = release_snapshot_on(state.writer_db(), snapshot, LockSelector::Id(lock_id)).await?;
    tracing::debug!(
        lock_id,
        namespace_id = lock.namespace_id,
        root_kind = ?lock.root_kind,
        "force unlocked resource"
    );
    Ok(())
}

pub async fn force_unlock_with_audit(
    state: &impl SharedRuntimeState,
    lock_id: i64,
    audit_ctx: &AuditContext,
) -> Result<()> {
    let lock = lock_repo::find_by_id(state.writer_db(), lock_id)
        .await?
        .ok_or_else(|| AsterError::record_not_found("lock not found"))?;
    force_unlock(state, lock_id).await?;
    audit::log_with_details(
        state,
        audit_ctx,
        audit::AuditAction::AdminForceUnlock,
        crate::services::ops::audit::AuditEntityType::ResourceLock,
        Some(lock_id),
        lock.lockroot_path.as_deref(),
        || {
            audit::details(audit::LockAuditDetails {
                entity_type: lock.root_kind,
                entity_id: lock.entity_id(),
            })
        },
    )
    .await;
    Ok(())
}

#[derive(Clone, Copy)]
enum LockSelector<'a> {
    Token(&'a str),
    Id(i64),
}

async fn release_snapshot_on(
    db: &DatabaseConnection,
    snapshot: resource_lock::Model,
    selector: LockSelector<'_>,
) -> Result<resource_lock::Model> {
    transaction::with_transaction(db, async |txn| {
        let namespace = lock_namespace_repo::lock_by_id(txn, snapshot.namespace_id).await?;
        lock_snapshot_target(txn, &snapshot, &namespace).await?;
        let current = match selector {
            LockSelector::Token(token) => lock_repo::find_by_token_for_update(txn, token).await?,
            LockSelector::Id(id) => lock_repo::find_by_id_for_update(txn, id).await?,
        }
        .ok_or_else(|| AsterError::record_not_found("lock not found"))?;
        if current.namespace_id != snapshot.namespace_id
            || current.root_kind != snapshot.root_kind
            || current.root_folder_id != snapshot.root_folder_id
            || current.root_file_id != snapshot.root_file_id
        {
            return Err(AsterError::resource_locked(
                "resource lock target changed while releasing it",
            ));
        }
        lock_repo::delete_by_id(txn, current.id).await?;
        increment_generation(txn, namespace).await?;
        Ok(current)
    })
    .await
}

pub async fn refresh_by_token_on(
    db: &DatabaseConnection,
    token: &str,
    new_timeout_at: Option<chrono::DateTime<Utc>>,
) -> Result<resource_lock::Model> {
    let snapshot = lock_repo::find_by_token(db, token)
        .await?
        .ok_or_else(|| AsterError::record_not_found("lock not found"))?;
    transaction::with_transaction(db, async |txn| {
        let namespace = lock_namespace_repo::lock_by_id(txn, snapshot.namespace_id).await?;
        lock_snapshot_target(txn, &snapshot, &namespace).await?;
        let current = lock_repo::find_by_token_for_update(txn, token)
            .await?
            .ok_or_else(|| AsterError::record_not_found("lock not found"))?;
        if current.namespace_id != snapshot.namespace_id
            || current.root_kind != snapshot.root_kind
            || current.root_folder_id != snapshot.root_folder_id
            || current.root_file_id != snapshot.root_file_id
        {
            return Err(AsterError::resource_locked(
                "resource lock target changed while refreshing it",
            ));
        }
        let mut active = current.into_active_model();
        active.timeout_at = Set(new_timeout_at);
        let updated = active.update(txn).await.map_err(AsterError::from)?;
        increment_generation(txn, namespace).await?;
        Ok(updated)
    })
    .await
}

pub async fn replace_owner_info_and_timeout_by_token_on(
    db: &DatabaseConnection,
    token: &str,
    owner_info: ResourceLockOwnerInfo,
    new_timeout_at: Option<chrono::DateTime<Utc>>,
) -> Result<resource_lock::Model> {
    let serialized_owner_info = serialize_resource_lock_owner_info(Some(&owner_info))?;
    let snapshot = lock_repo::find_by_token(db, token)
        .await?
        .ok_or_else(|| AsterError::record_not_found("lock not found"))?;
    transaction::with_transaction(db, async |txn| {
        let namespace = lock_namespace_repo::lock_by_id(txn, snapshot.namespace_id).await?;
        lock_snapshot_target(txn, &snapshot, &namespace).await?;
        let current = lock_repo::find_by_token_for_update(txn, token)
            .await?
            .ok_or_else(|| AsterError::record_not_found("lock not found"))?;
        if current.namespace_id != snapshot.namespace_id
            || current.root_kind != snapshot.root_kind
            || current.root_folder_id != snapshot.root_folder_id
            || current.root_file_id != snapshot.root_file_id
        {
            return Err(AsterError::resource_locked(
                "resource lock target changed while replacing it",
            ));
        }
        let mut active = current.into_active_model();
        active.owner_info = Set(serialized_owner_info);
        active.timeout_at = Set(new_timeout_at);
        let updated = active.update(txn).await.map_err(AsterError::from)?;
        increment_generation(txn, namespace).await?;
        Ok(updated)
    })
    .await
}

async fn resolve_entity_target<C: sea_orm::ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<LockTarget> {
    let (workspace, root) = match entity_type {
        EntityType::File => {
            let file = file_repo::find_by_id(db, entity_id).await?;
            (
                LockWorkspace::from_file(&file)?,
                LockRoot::File { file_id: entity_id },
            )
        }
        EntityType::Folder => {
            let folder = folder_repo::find_by_id(db, entity_id).await?;
            (
                LockWorkspace::from_folder(&folder)?,
                LockRoot::Folder {
                    folder_id: entity_id,
                },
            )
        }
    };
    Ok(LockTarget {
        workspace,
        root,
        depth: LockDepth::Resource,
    })
}

async fn lock_target_namespace<C: sea_orm::ConnectionTrait>(
    db: &C,
    workspace: LockWorkspace,
) -> Result<resource_lock_namespace::Model> {
    let (workspace_type, workspace_id) = workspace.persistence_key();
    match workspace_type {
        LockWorkspaceType::Personal => {
            user_repo::find_by_id(db, workspace_id).await?;
        }
        LockWorkspaceType::Team => {
            team_repo::find_by_id(db, workspace_id).await?;
        }
    }
    lock_namespace_repo::ensure_and_lock(db, workspace_type, workspace_id).await
}

async fn lock_and_revalidate_target<C: sea_orm::ConnectionTrait>(
    db: &C,
    target: LockTarget,
) -> Result<()> {
    match target.root {
        LockRoot::WorkspaceRoot => {}
        LockRoot::File { file_id } => {
            let file = file_repo::lock_by_id(db, file_id).await?;
            if LockWorkspace::from_file(&file)? != target.workspace {
                return Err(AsterError::resource_locked(
                    "file workspace changed during lock mutation",
                ));
            }
        }
        LockRoot::Folder { folder_id } => {
            let folder = folder_repo::lock_by_id(db, folder_id).await?;
            if LockWorkspace::from_folder(&folder)? != target.workspace {
                return Err(AsterError::resource_locked(
                    "folder workspace changed during lock mutation",
                ));
            }
        }
    }
    Ok(())
}

async fn lock_snapshot_target<C: sea_orm::ConnectionTrait>(
    db: &C,
    lock: &resource_lock::Model,
    namespace: &resource_lock_namespace::Model,
) -> Result<()> {
    let workspace = match namespace.workspace_type {
        LockWorkspaceType::Personal => LockWorkspace::Personal {
            user_id: namespace.workspace_id,
        },
        LockWorkspaceType::Team => LockWorkspace::Team {
            team_id: namespace.workspace_id,
        },
    };
    lock_and_revalidate_target(
        db,
        LockTarget {
            workspace,
            root: LockRoot::from_model(lock)?,
            depth: lock.depth,
        },
    )
    .await
}

async fn increment_generation<C: sea_orm::ConnectionTrait>(
    db: &C,
    namespace: resource_lock_namespace::Model,
) -> Result<()> {
    lock_namespace_repo::increment_generation(db, namespace).await?;
    Ok(())
}

fn origin_for_owner_info(owner_info: Option<&ResourceLockOwnerInfo>) -> LockOrigin {
    match owner_info {
        Some(ResourceLockOwnerInfo::Webdav(_)) => LockOrigin::WebDav,
        Some(ResourceLockOwnerInfo::Wopi(_)) => LockOrigin::Wopi,
        Some(ResourceLockOwnerInfo::Text(_)) | None => LockOrigin::Product,
    }
}

fn locks_overlap(
    existing: &resource_lock::Model,
    requested: LockTarget,
    requested_path: Option<&str>,
) -> bool {
    if existing.root_kind == requested.root.persistence_parts().0
        && existing.root_folder_id == requested.root.persistence_parts().1
        && existing.root_file_id == requested.root.persistence_parts().2
    {
        return true;
    }
    if matches!(
        existing.root_kind,
        aster_drive_model::types::LockRootKind::WorkspaceRoot
    ) && existing.depth == LockDepth::Infinity
    {
        return true;
    }
    if matches!(requested.root, LockRoot::WorkspaceRoot) && requested.depth == LockDepth::Infinity {
        return true;
    }

    let (Some(existing_path), Some(requested_path)) =
        (existing.lockroot_path.as_deref(), requested_path)
    else {
        return false;
    };
    path_is_ancestor(existing_path, requested_path) && existing.depth == LockDepth::Infinity
        || path_is_ancestor(requested_path, existing_path) && requested.depth == LockDepth::Infinity
}

fn path_is_ancestor(ancestor: &str, descendant: &str) -> bool {
    if ancestor == descendant {
        return true;
    }
    let prefix = if ancestor.ends_with('/') {
        ancestor.to_string()
    } else {
        format!("{ancestor}/")
    };
    descendant.starts_with(&prefix)
}

fn usize_to_u64(value: usize, label: &str) -> Result<u64> {
    u64::try_from(value).map_err(|_| AsterError::internal_error(format!("{label} overflow")))
}
