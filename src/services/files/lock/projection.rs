use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::db::repository::{folder_repo, lock_namespace_repo, lock_repo};
use crate::errors::Result;
use crate::runtime::SharedRuntimeState;
use crate::services::workspace::storage::WorkspaceResourceScope;
use aster_drive_model::entities::{file, folder, resource_lock};
use aster_drive_model::types::{EntityType, LockDepth, LockMode, LockRootKind, LockWorkspaceType};
use aster_forge_cache::CacheExt;

const LOCK_PROJECTION_CACHE_TTL: u64 = 300;
const LOCK_PROJECTION_CACHE_PREFIX: &str = "resource_lock_projection:";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedLockProjection {
    root_kind: LockRootKind,
    root_folder_id: Option<i64>,
    root_file_id: Option<i64>,
    depth: LockDepth,
    mode: LockMode,
    expires_at: Option<DateTime<Utc>>,
}

pub type LockStateMap = HashMap<(EntityType, i64), super::ResourceLockState>;

pub async fn load_for_resources(
    state: &impl SharedRuntimeState,
    files: &[file::Model],
    folders: &[folder::Model],
) -> Result<LockStateMap> {
    let mut groups = HashMap::<super::LockWorkspace, (Vec<file::Model>, Vec<folder::Model>)>::new();
    for file in files {
        groups
            .entry(super::LockWorkspace::from_file(file)?)
            .or_default()
            .0
            .push(file.clone());
    }
    for folder in folders {
        groups
            .entry(super::LockWorkspace::from_folder(folder)?)
            .or_default()
            .1
            .push(folder.clone());
    }

    let mut states = LockStateMap::new();
    for (workspace, (files, folders)) in groups {
        let scope = match workspace {
            super::LockWorkspace::Personal { user_id } => {
                WorkspaceResourceScope::Personal { user_id }
            }
            super::LockWorkspace::Team { team_id } => WorkspaceResourceScope::Team { team_id },
        };
        states.extend(load_for_scope(state, scope, &files, &folders).await?);
    }
    Ok(states)
}

pub fn state_for(
    states: &LockStateMap,
    entity_type: EntityType,
    entity_id: i64,
) -> super::ResourceLockState {
    states
        .get(&(entity_type, entity_id))
        .cloned()
        .unwrap_or(super::ResourceLockState::Unlocked)
}

pub async fn load_for_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceResourceScope,
    files: &[file::Model],
    folders: &[folder::Model],
) -> Result<LockStateMap> {
    let (workspace_type, workspace_id) = match scope {
        WorkspaceResourceScope::Personal { user_id } => (LockWorkspaceType::Personal, user_id),
        WorkspaceResourceScope::Team { team_id } => (LockWorkspaceType::Team, team_id),
    };
    let namespace =
        lock_namespace_repo::find_by_workspace(state.reader_db(), workspace_type, workspace_id)
            .await?;
    let Some(namespace) = namespace else {
        return Ok(unlocked_states(files, folders));
    };

    let locks = load_cached_locks(state, namespace.id, namespace.generation).await?;
    let ancestor_ids = load_ancestor_ids(state, scope, files, folders).await?;
    let now = Utc::now();
    let mut states = unlocked_states(files, folders);
    for file in files {
        states.insert(
            (EntityType::File, file.id),
            project_state(
                &locks,
                LockTargetRef::File(file.id),
                file.folder_id,
                &ancestor_ids,
                now,
            ),
        );
    }
    for folder in folders {
        states.insert(
            (EntityType::Folder, folder.id),
            project_state(
                &locks,
                LockTargetRef::Folder(folder.id),
                folder.parent_id,
                &ancestor_ids,
                now,
            ),
        );
    }
    Ok(states)
}

async fn load_cached_locks(
    state: &impl SharedRuntimeState,
    namespace_id: i64,
    mut generation: i64,
) -> Result<Vec<CachedLockProjection>> {
    for _attempt in 0..2 {
        let key = format!("{LOCK_PROJECTION_CACHE_PREFIX}{namespace_id}:{generation}");
        let locks = if let Some(cached) = state.cache().get::<Vec<CachedLockProjection>>(&key).await
        {
            cached
        } else {
            let loaded = lock_repo::find_all_by_namespace(state.reader_db(), namespace_id)
                .await?
                .into_iter()
                .map(CachedLockProjection::from)
                .collect::<Vec<_>>();
            state
                .cache()
                .set(&key, &loaded, Some(LOCK_PROJECTION_CACHE_TTL))
                .await;
            loaded
        };
        let current_generation = lock_namespace_repo::find_by_id(state.reader_db(), namespace_id)
            .await?
            .map(|namespace| namespace.generation);
        if current_generation == Some(generation) {
            return Ok(locks);
        }
        if let Some(current_generation) = current_generation {
            generation = current_generation;
        } else {
            return Ok(Vec::new());
        }
    }

    let namespace = lock_namespace_repo::find_by_id(state.reader_db(), namespace_id).await?;
    let Some(namespace) = namespace else {
        return Ok(Vec::new());
    };
    Ok(
        lock_repo::find_all_by_namespace(state.reader_db(), namespace.id)
            .await?
            .into_iter()
            .map(CachedLockProjection::from)
            .collect(),
    )
}

async fn load_ancestor_ids(
    state: &impl SharedRuntimeState,
    scope: WorkspaceResourceScope,
    files: &[file::Model],
    folders: &[folder::Model],
) -> Result<HashMap<i64, Vec<i64>>> {
    let mut ids = files
        .iter()
        .filter_map(|file| file.folder_id)
        .chain(folders.iter().filter_map(|folder| folder.parent_id))
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();

    let mut result = HashMap::with_capacity(ids.len());
    for folder_id in ids {
        let ancestors = match scope {
            WorkspaceResourceScope::Personal { user_id } => {
                folder_repo::find_ancestor_models(state.reader_db(), user_id, folder_id).await?
            }
            WorkspaceResourceScope::Team { team_id } => {
                folder_repo::find_team_ancestor_models(state.reader_db(), team_id, folder_id)
                    .await?
            }
        };
        result.insert(
            folder_id,
            ancestors.into_iter().map(|folder| folder.id).collect(),
        );
    }
    Ok(result)
}

fn unlocked_states(files: &[file::Model], folders: &[folder::Model]) -> LockStateMap {
    files
        .iter()
        .map(|file| {
            (
                (EntityType::File, file.id),
                super::ResourceLockState::Unlocked,
            )
        })
        .chain(folders.iter().map(|folder| {
            (
                (EntityType::Folder, folder.id),
                super::ResourceLockState::Unlocked,
            )
        }))
        .collect()
}

#[derive(Clone, Copy)]
enum LockTargetRef {
    File(i64),
    Folder(i64),
}

fn project_state(
    locks: &[CachedLockProjection],
    target: LockTargetRef,
    parent_folder_id: Option<i64>,
    ancestor_ids: &HashMap<i64, Vec<i64>>,
    now: DateTime<Utc>,
) -> super::ResourceLockState {
    let direct = locks.iter().find(|lock| {
        !is_expired(lock, now)
            && match target {
                LockTargetRef::File(id) => {
                    lock.root_kind == LockRootKind::File && lock.root_file_id == Some(id)
                }
                LockTargetRef::Folder(id) => {
                    lock.root_kind == LockRootKind::Folder && lock.root_folder_id == Some(id)
                }
            }
    });
    if let Some(lock) = direct {
        return super::ResourceLockState::Direct {
            mode: lock.mode,
            expires_at: lock.expires_at,
        };
    }

    let ancestors = parent_folder_id
        .and_then(|id| ancestor_ids.get(&id))
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let inherited = locks.iter().find(|lock| {
        if is_expired(lock, now) || lock.depth != LockDepth::Infinity {
            return false;
        }
        match lock.root_kind {
            LockRootKind::WorkspaceRoot => true,
            LockRootKind::Folder => lock
                .root_folder_id
                .is_some_and(|folder_id| ancestors.contains(&folder_id)),
            LockRootKind::File => false,
        }
    });
    inherited.map_or(super::ResourceLockState::Unlocked, |lock| {
        let root = match lock.root_kind {
            LockRootKind::WorkspaceRoot => super::LockRootSummary::WorkspaceRoot,
            LockRootKind::Folder => {
                let Some(folder_id) = lock.root_folder_id else {
                    return super::ResourceLockState::Unlocked;
                };
                super::LockRootSummary::Folder { folder_id }
            }
            LockRootKind::File => return super::ResourceLockState::Unlocked,
        };
        super::ResourceLockState::Inherited {
            root,
            mode: lock.mode,
            expires_at: lock.expires_at,
        }
    })
}

fn is_expired(lock: &CachedLockProjection, now: DateTime<Utc>) -> bool {
    lock.expires_at.is_some_and(|expires_at| expires_at <= now)
}

impl From<resource_lock::Model> for CachedLockProjection {
    fn from(lock: resource_lock::Model) -> Self {
        Self {
            root_kind: lock.root_kind,
            root_folder_id: lock.root_folder_id,
            root_file_id: lock.root_file_id,
            depth: lock.depth,
            mode: lock.mode,
            expires_at: lock.timeout_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::files::lock::{LockRootSummary, ResourceLockState};

    fn projected_lock(
        root_kind: LockRootKind,
        root_folder_id: Option<i64>,
        root_file_id: Option<i64>,
        depth: LockDepth,
        mode: LockMode,
        expires_at: Option<DateTime<Utc>>,
    ) -> CachedLockProjection {
        CachedLockProjection {
            root_kind,
            root_folder_id,
            root_file_id,
            depth,
            mode,
            expires_at,
        }
    }

    #[test]
    fn direct_lock_takes_precedence_over_inherited_lock() {
        let now = Utc::now();
        let locks = vec![
            projected_lock(
                LockRootKind::WorkspaceRoot,
                None,
                None,
                LockDepth::Infinity,
                LockMode::Exclusive,
                None,
            ),
            projected_lock(
                LockRootKind::File,
                None,
                Some(7),
                LockDepth::Resource,
                LockMode::Shared,
                None,
            ),
        ];

        assert_eq!(
            project_state(&locks, LockTargetRef::File(7), None, &HashMap::new(), now),
            ResourceLockState::Direct {
                mode: LockMode::Shared,
                expires_at: None,
            }
        );
    }

    #[test]
    fn folder_infinity_lock_projects_to_descendant() {
        let now = Utc::now();
        let locks = vec![projected_lock(
            LockRootKind::Folder,
            Some(3),
            None,
            LockDepth::Infinity,
            LockMode::Exclusive,
            None,
        )];
        let ancestors = HashMap::from([(5, vec![3, 5])]);

        assert_eq!(
            project_state(&locks, LockTargetRef::File(9), Some(5), &ancestors, now),
            ResourceLockState::Inherited {
                root: LockRootSummary::Folder { folder_id: 3 },
                mode: LockMode::Exclusive,
                expires_at: None,
            }
        );
    }

    #[test]
    fn expired_locks_are_not_projected() {
        let now = Utc::now();
        let locks = vec![projected_lock(
            LockRootKind::File,
            None,
            Some(7),
            LockDepth::Resource,
            LockMode::Exclusive,
            Some(now),
        )];

        assert_eq!(
            project_state(&locks, LockTargetRef::File(7), None, &HashMap::new(), now),
            ResourceLockState::Unlocked
        );
    }

    #[test]
    fn cached_projection_contains_no_lock_credentials_or_owner_payload() {
        let cached = projected_lock(
            LockRootKind::File,
            None,
            Some(7),
            LockDepth::Resource,
            LockMode::Exclusive,
            None,
        );
        let json = serde_json::to_string(&cached).expect("projection should serialize");

        for forbidden in ["token", "owner", "holder", "wopi", "lockroot_path"] {
            assert!(
                !json.contains(forbidden),
                "unexpected {forbidden} in {json}"
            );
        }
    }
}
