//! 服务模块：`storage_change_service`。

use std::{
    collections::{BTreeSet, HashSet},
    sync::Arc,
    time::Duration as StdDuration,
};

use chrono::{DateTime, Utc};
use futures::future::join_all;
use serde::Serialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::cache::CacheBackend;
use crate::db::repository::{file_repo, folder_repo};
use crate::errors::Result;
use crate::runtime::StorageChangeRuntimeState;
use crate::services::workspace::storage::{WorkspaceResourceScope, WorkspaceStorageScope};

pub const STORAGE_CHANGE_CHANNEL_CAPACITY: usize = 1024;
const CACHE_INVALIDATION_COALESCE_DELAY: StdDuration = StdDuration::from_millis(25);
const CACHE_INVALIDATION_RESERVATION_TTL_SECS: u64 = 1;
const CACHE_INVALIDATION_RESERVATION_PREFIX: &str = "storage_change_cache_invalidation:";
const AFFECTED_PARENT_LOOKUP_CHUNK_SIZE: usize = 500;

#[derive(Debug, Default, PartialEq, Eq)]
struct CacheInvalidationTargets {
    prefixes: Vec<String>,
    keys: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageChangeAudience {
    User(i64),
    Team(i64),
    Any,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageChangeKind {
    #[serde(rename = "file.created")]
    FileCreated,
    #[serde(rename = "file.updated")]
    FileUpdated,
    #[serde(rename = "file.trashed")]
    FileTrashed,
    #[serde(rename = "file.restored_from_trash")]
    FileRestoredFromTrash,
    #[serde(rename = "file.purged")]
    FilePurged,
    #[serde(rename = "file.version_restored")]
    FileVersionRestored,
    #[serde(rename = "file.version_deleted")]
    FileVersionDeleted,
    #[serde(rename = "folder.created")]
    FolderCreated,
    #[serde(rename = "folder.updated")]
    FolderUpdated,
    #[serde(rename = "folder.trashed")]
    FolderTrashed,
    #[serde(rename = "folder.restored_from_trash")]
    FolderRestoredFromTrash,
    #[serde(rename = "folder.purged")]
    FolderPurged,
    #[serde(rename = "tag.created")]
    TagCreated,
    #[serde(rename = "tag.updated")]
    TagUpdated,
    #[serde(rename = "tag.deleted")]
    TagDeleted,
    #[serde(rename = "tag.assignment_changed")]
    TagAssignmentChanged,
    #[serde(rename = "lock.created")]
    LockCreated,
    #[serde(rename = "lock.deleted")]
    LockDeleted,
    #[serde(rename = "sync.required")]
    SyncRequired,
}

impl StorageChangeKind {
    fn invalidates_folder_path_cache(self) -> bool {
        match self {
            Self::FileCreated
            | Self::FileUpdated
            | Self::FileTrashed
            | Self::FileRestoredFromTrash
            | Self::FilePurged
            | Self::FileVersionRestored
            | Self::FileVersionDeleted => false,
            Self::FolderCreated
            | Self::FolderUpdated
            | Self::FolderTrashed
            | Self::FolderRestoredFromTrash
            | Self::FolderPurged
            | Self::SyncRequired => true,
            Self::TagCreated | Self::TagUpdated | Self::TagDeleted | Self::TagAssignmentChanged => {
                false
            }
            Self::LockCreated | Self::LockDeleted => false,
        }
    }

    fn invalidates_webdav_path_cache(self) -> bool {
        match self {
            Self::FileCreated
            | Self::FileUpdated
            | Self::FileTrashed
            | Self::FileRestoredFromTrash
            | Self::FilePurged
            | Self::FileVersionRestored
            | Self::FileVersionDeleted
            | Self::FolderCreated
            | Self::FolderUpdated
            | Self::FolderTrashed
            | Self::FolderRestoredFromTrash
            | Self::FolderPurged
            | Self::SyncRequired => true,
            Self::TagCreated
            | Self::TagUpdated
            | Self::TagDeleted
            | Self::TagAssignmentChanged
            | Self::LockCreated
            | Self::LockDeleted => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageChangeWorkspace {
    Personal,
    Team { team_id: i64 },
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageChangeEvent {
    #[serde(skip_serializing)]
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(ignore))]
    audience: StorageChangeAudience,
    pub kind: StorageChangeKind,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(required = true))]
    pub workspace: Option<StorageChangeWorkspace>,
    pub file_ids: Vec<i64>,
    pub folder_ids: Vec<i64>,
    pub affected_parent_ids: Vec<i64>,
    pub root_affected: bool,
    pub affects_quota: bool,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(required = true))]
    pub storage_delta: Option<i64>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub at: DateTime<Utc>,
}

impl StorageChangeEvent {
    pub(crate) fn new(
        kind: StorageChangeKind,
        scope: WorkspaceStorageScope,
        file_ids: Vec<i64>,
        folder_ids: Vec<i64>,
        affected_parent_ids: Vec<Option<i64>>,
    ) -> Self {
        let (audience, workspace) = match scope {
            WorkspaceStorageScope::Personal { user_id } => (
                StorageChangeAudience::User(user_id),
                StorageChangeWorkspace::Personal,
            ),
            WorkspaceStorageScope::Team { team_id, .. } => (
                StorageChangeAudience::Team(team_id),
                StorageChangeWorkspace::Team { team_id },
            ),
        };
        let (affected_parent_ids, root_affected) =
            normalize_parent_ids(affected_parent_ids.into_iter());

        Self {
            audience,
            kind,
            workspace: Some(workspace),
            file_ids: normalize_ids(file_ids.into_iter()),
            folder_ids: normalize_ids(folder_ids.into_iter()),
            affected_parent_ids,
            root_affected,
            affects_quota: false,
            storage_delta: None,
            at: Utc::now(),
        }
    }

    pub(crate) fn new_for_resource_scope(
        kind: StorageChangeKind,
        scope: WorkspaceResourceScope,
        file_ids: Vec<i64>,
        folder_ids: Vec<i64>,
        affected_parent_ids: Vec<Option<i64>>,
    ) -> Self {
        let (audience, workspace) = match scope {
            WorkspaceResourceScope::Personal { user_id } => (
                StorageChangeAudience::User(user_id),
                StorageChangeWorkspace::Personal,
            ),
            WorkspaceResourceScope::Team { team_id } => (
                StorageChangeAudience::Team(team_id),
                StorageChangeWorkspace::Team { team_id },
            ),
        };
        let (affected_parent_ids, root_affected) =
            normalize_parent_ids(affected_parent_ids.into_iter());

        Self {
            audience,
            kind,
            workspace: Some(workspace),
            file_ids: normalize_ids(file_ids.into_iter()),
            folder_ids: normalize_ids(folder_ids.into_iter()),
            affected_parent_ids,
            root_affected,
            affects_quota: false,
            storage_delta: None,
            at: Utc::now(),
        }
    }

    pub(crate) fn with_storage_delta(mut self, delta: i64) -> Self {
        self.affects_quota = delta != 0;
        self.storage_delta = Some(delta);
        self
    }

    pub fn sync_required() -> Self {
        Self {
            audience: StorageChangeAudience::Any,
            kind: StorageChangeKind::SyncRequired,
            workspace: None,
            file_ids: Vec::new(),
            folder_ids: Vec::new(),
            affected_parent_ids: Vec::new(),
            root_affected: false,
            affects_quota: true,
            storage_delta: None,
            at: Utc::now(),
        }
    }

    pub fn is_visible_to(&self, user_id: i64, team_ids: &HashSet<i64>) -> bool {
        match self.audience {
            StorageChangeAudience::Any => true,
            StorageChangeAudience::User(target_user_id) => target_user_id == user_id,
            StorageChangeAudience::Team(team_id) => team_ids.contains(&team_id),
        }
    }
}

pub fn publish<S: StorageChangeRuntimeState>(state: &S, event: StorageChangeEvent) {
    invalidate_storage_change_caches(state.cache().clone(), &event);
    if let Err(e) = state.storage_change_tx().send(event) {
        tracing::debug!("skip storage change broadcast without listeners: {e}");
    }
}

pub(crate) async fn affected_parent_ids_for_entities(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    file_ids: &[i64],
    folder_ids: &[i64],
) -> Result<Vec<Option<i64>>> {
    let mut parent_ids = Vec::new();

    for chunk in file_ids.chunks(AFFECTED_PARENT_LOOKUP_CHUNK_SIZE) {
        let files = match scope {
            WorkspaceStorageScope::Personal { user_id } => {
                file_repo::find_by_ids_in_personal_scope(state.reader_db(), user_id, chunk).await?
            }
            WorkspaceStorageScope::Team { team_id, .. } => {
                file_repo::find_by_ids_in_team_scope(state.reader_db(), team_id, chunk).await?
            }
        };
        parent_ids.extend(files.into_iter().map(|file| file.folder_id));
    }

    for chunk in folder_ids.chunks(AFFECTED_PARENT_LOOKUP_CHUNK_SIZE) {
        let folders = match scope {
            WorkspaceStorageScope::Personal { user_id } => {
                folder_repo::find_by_ids_in_personal_scope(state.reader_db(), user_id, chunk)
                    .await?
            }
            WorkspaceStorageScope::Team { team_id, .. } => {
                folder_repo::find_by_ids_in_team_scope(state.reader_db(), team_id, chunk).await?
            }
        };
        parent_ids.extend(folders.into_iter().map(|folder| folder.parent_id));
    }

    parent_ids.sort();
    parent_ids.dedup();

    Ok(parent_ids)
}

fn invalidate_storage_change_caches(cache: Arc<dyn CacheBackend>, event: &StorageChangeEvent) {
    let targets = cache_invalidation_targets(event);
    if targets.prefixes.is_empty() && targets.keys.is_empty() {
        return;
    }

    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        tracing::debug!("skip async cache invalidation without tokio runtime");
        return;
    };
    drop(handle.spawn(async move {
        let CacheInvalidationTargets { prefixes, keys } = targets;
        let prefix_invalidations = prefixes
            .into_iter()
            .map(|prefix| schedule_cache_prefix_invalidation(cache.clone(), prefix));
        join_all(prefix_invalidations).await;
        if !keys.is_empty() {
            cache.delete_many(&keys).await;
        }
    }));
}

fn cache_invalidation_targets(event: &StorageChangeEvent) -> CacheInvalidationTargets {
    if !event.kind.invalidates_webdav_path_cache() {
        return CacheInvalidationTargets::default();
    }

    let mut targets = CacheInvalidationTargets {
        prefixes: webdav_path_cache_invalidation_prefixes(event.audience),
        keys: Vec::new(),
    };
    if event.kind.invalidates_folder_path_cache() {
        if event.kind == StorageChangeKind::SyncRequired {
            targets
                .prefixes
                .push(crate::services::files::folder::FOLDER_PATH_CACHE_PREFIX.to_string());
        } else {
            targets.keys.extend(
                event
                    .folder_ids
                    .iter()
                    .copied()
                    .map(crate::services::files::folder::folder_path_cache_key),
            );
        }
    }
    targets
}

fn webdav_path_cache_invalidation_prefixes(audience: StorageChangeAudience) -> Vec<String> {
    match audience {
        StorageChangeAudience::User(user_id) => vec![
            crate::webdav::path_resolver::path_cache_personal_prefix(user_id),
            crate::webdav::path_resolver::parent_cache_personal_prefix(user_id),
        ],
        StorageChangeAudience::Team(team_id) => vec![
            crate::webdav::path_resolver::path_cache_team_prefix(team_id),
            crate::webdav::path_resolver::parent_cache_team_prefix(team_id),
        ],
        StorageChangeAudience::Any => vec![
            crate::webdav::path_resolver::WEBDAV_PATH_CACHE_PREFIX.to_string(),
            crate::webdav::path_resolver::WEBDAV_PARENT_CACHE_PREFIX.to_string(),
        ],
    }
}

async fn schedule_cache_prefix_invalidation(cache: Arc<dyn CacheBackend>, prefix: String) {
    let reservation_key = cache_invalidation_reservation_key(&prefix);
    if !cache
        .set_bytes_if_absent(
            &reservation_key,
            Vec::new(),
            Some(CACHE_INVALIDATION_RESERVATION_TTL_SECS),
        )
        .await
    {
        return;
    }

    tokio::time::sleep(CACHE_INVALIDATION_COALESCE_DELAY).await;
    cache.invalidate_prefix(&prefix).await;
    cache.delete(&reservation_key).await;
}

fn cache_invalidation_reservation_key(prefix: &str) -> String {
    format!("{CACHE_INVALIDATION_RESERVATION_PREFIX}{prefix}")
}

fn normalize_ids(ids: impl Iterator<Item = i64>) -> Vec<i64> {
    BTreeSet::from_iter(ids).into_iter().collect()
}

fn normalize_parent_ids(parent_ids: impl Iterator<Item = Option<i64>>) -> (Vec<i64>, bool) {
    let mut normalized = BTreeSet::new();
    let mut root_affected = false;

    for parent_id in parent_ids {
        match parent_id {
            Some(parent_id) => {
                normalized.insert(parent_id);
            }
            None => root_affected = true,
        }
    }

    (normalized.into_iter().collect(), root_affected)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{StorageChangeEvent, StorageChangeKind, StorageChangeWorkspace};
    use crate::services::workspace::storage::WorkspaceStorageScope;

    #[test]
    fn storage_change_event_normalizes_ids_and_root_flag() {
        let event = StorageChangeEvent::new(
            StorageChangeKind::FileUpdated,
            WorkspaceStorageScope::Personal { user_id: 7 },
            vec![5, 3, 5],
            vec![9, 8, 9],
            vec![Some(2), None, Some(2)],
        );

        assert_eq!(event.file_ids, vec![3, 5]);
        assert_eq!(event.folder_ids, vec![8, 9]);
        assert_eq!(event.affected_parent_ids, vec![2]);
        assert!(event.root_affected);
        assert!(matches!(
            event.workspace,
            Some(StorageChangeWorkspace::Personal)
        ));
    }

    #[test]
    fn storage_change_event_filters_personal_and_team_visibility() {
        let personal = StorageChangeEvent::new(
            StorageChangeKind::FileTrashed,
            WorkspaceStorageScope::Personal { user_id: 11 },
            vec![1],
            vec![],
            vec![None],
        );
        let team = StorageChangeEvent::new(
            StorageChangeKind::FolderUpdated,
            WorkspaceStorageScope::Team {
                team_id: 42,
                actor_user_id: 11,
            },
            vec![],
            vec![7],
            vec![Some(3)],
        );

        assert!(personal.is_visible_to(11, &HashSet::new()));
        assert!(!personal.is_visible_to(12, &HashSet::new()));

        let visible_teams = HashSet::from([42]);
        assert!(team.is_visible_to(11, &visible_teams));
        assert!(!team.is_visible_to(11, &HashSet::new()));
    }

    #[test]
    fn personal_file_changes_only_invalidate_personal_webdav_path_prefixes() {
        let event = StorageChangeEvent::new(
            StorageChangeKind::FileCreated,
            WorkspaceStorageScope::Personal { user_id: 11 },
            vec![1],
            vec![],
            vec![None],
        );
        let targets = super::cache_invalidation_targets(&event);

        assert_eq!(
            targets.prefixes,
            vec![
                crate::webdav::path_resolver::path_cache_personal_prefix(11),
                crate::webdav::path_resolver::parent_cache_personal_prefix(11),
            ]
        );
        assert!(targets.keys.is_empty());
    }

    #[test]
    fn team_file_changes_invalidate_team_webdav_path_prefixes() {
        let event = StorageChangeEvent::new(
            StorageChangeKind::FileUpdated,
            WorkspaceStorageScope::Team {
                team_id: 42,
                actor_user_id: 11,
            },
            vec![1],
            vec![],
            vec![Some(3)],
        );
        let targets = super::cache_invalidation_targets(&event);

        assert_eq!(
            targets.prefixes,
            vec![
                crate::webdav::path_resolver::path_cache_team_prefix(42),
                crate::webdav::path_resolver::parent_cache_team_prefix(42),
            ]
        );
        assert!(targets.keys.is_empty());
    }

    #[test]
    fn folder_changes_invalidate_changed_folder_path_keys() {
        let event = StorageChangeEvent::new(
            StorageChangeKind::FolderUpdated,
            WorkspaceStorageScope::Personal { user_id: 11 },
            vec![],
            vec![7, 9],
            vec![Some(3)],
        );
        let targets = super::cache_invalidation_targets(&event);

        assert_eq!(
            targets.prefixes,
            vec![
                crate::webdav::path_resolver::path_cache_personal_prefix(11),
                crate::webdav::path_resolver::parent_cache_personal_prefix(11),
            ]
        );
        assert_eq!(
            targets.keys,
            vec![
                crate::services::files::folder::folder_path_cache_key(7),
                crate::services::files::folder::folder_path_cache_key(9),
            ]
        );
    }

    #[test]
    fn sync_required_includes_folder_path_prefix() {
        let event = StorageChangeEvent::sync_required();
        let targets = super::cache_invalidation_targets(&event);

        assert_eq!(
            targets.prefixes,
            vec![
                crate::webdav::path_resolver::WEBDAV_PATH_CACHE_PREFIX.to_string(),
                crate::webdav::path_resolver::WEBDAV_PARENT_CACHE_PREFIX.to_string(),
                crate::services::files::folder::FOLDER_PATH_CACHE_PREFIX.to_string(),
            ]
        );
        assert!(targets.keys.is_empty());
    }

    #[test]
    fn tag_changes_do_not_invalidate_webdav_path_caches() {
        let event = StorageChangeEvent::new(
            StorageChangeKind::TagAssignmentChanged,
            WorkspaceStorageScope::Personal { user_id: 11 },
            vec![1],
            vec![],
            vec![None],
        );
        let targets = super::cache_invalidation_targets(&event);

        assert!(targets.prefixes.is_empty());
        assert!(targets.keys.is_empty());
    }

    #[test]
    fn lock_changes_do_not_invalidate_webdav_path_caches() {
        for kind in [
            StorageChangeKind::LockCreated,
            StorageChangeKind::LockDeleted,
        ] {
            let event = StorageChangeEvent::new(
                kind,
                WorkspaceStorageScope::Team {
                    team_id: 42,
                    actor_user_id: 11,
                },
                vec![1],
                vec![],
                vec![Some(3)],
            );
            let targets = super::cache_invalidation_targets(&event);

            assert!(targets.prefixes.is_empty());
            assert!(targets.keys.is_empty());
        }
    }
}
