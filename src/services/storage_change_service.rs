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
use crate::runtime::StorageChangeRuntimeState;
use crate::services::workspace_storage_service::{WorkspaceResourceScope, WorkspaceStorageScope};

pub const STORAGE_CHANGE_CHANNEL_CAPACITY: usize = 1024;
const CACHE_INVALIDATION_COALESCE_DELAY: StdDuration = StdDuration::from_millis(25);
const CACHE_INVALIDATION_RESERVATION_TTL_SECS: u64 = 1;
const CACHE_INVALIDATION_RESERVATION_PREFIX: &str = "storage_change_cache_invalidation:";

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
    invalidate_storage_change_caches(state.cache().clone(), event.kind);
    if let Err(e) = state.storage_change_tx().send(event) {
        tracing::debug!("skip storage change broadcast without listeners: {e}");
    }
}

fn invalidate_storage_change_caches(cache: Arc<dyn CacheBackend>, kind: StorageChangeKind) {
    let prefixes = cache_invalidation_prefixes(kind);
    if prefixes.is_empty() {
        return;
    }

    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        tracing::debug!("skip async cache invalidation without tokio runtime");
        return;
    };
    drop(handle.spawn(async move {
        join_all(
            prefixes
                .into_iter()
                .map(|prefix| schedule_cache_prefix_invalidation(cache.clone(), prefix)),
        )
        .await;
    }));
}

fn cache_invalidation_prefixes(kind: StorageChangeKind) -> Vec<&'static str> {
    let mut prefixes = vec![
        crate::webdav::path_resolver::WEBDAV_PATH_CACHE_PREFIX,
        crate::webdav::path_resolver::WEBDAV_PARENT_CACHE_PREFIX,
    ];
    if kind.invalidates_folder_path_cache() {
        prefixes.push(crate::services::folder_service::FOLDER_PATH_CACHE_PREFIX);
    }
    prefixes
}

async fn schedule_cache_prefix_invalidation(cache: Arc<dyn CacheBackend>, prefix: &'static str) {
    let reservation_key = cache_invalidation_reservation_key(prefix);
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
    cache.invalidate_prefix(prefix).await;
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
    use crate::services::workspace_storage_service::WorkspaceStorageScope;

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
    fn file_changes_only_invalidate_webdav_path_prefixes() {
        let prefixes = super::cache_invalidation_prefixes(StorageChangeKind::FileCreated);

        assert_eq!(
            prefixes,
            vec![
                crate::webdav::path_resolver::WEBDAV_PATH_CACHE_PREFIX,
                crate::webdav::path_resolver::WEBDAV_PARENT_CACHE_PREFIX,
            ]
        );
    }

    #[test]
    fn folder_changes_also_invalidate_folder_path_prefix() {
        let prefixes = super::cache_invalidation_prefixes(StorageChangeKind::FolderUpdated);

        assert_eq!(
            prefixes,
            vec![
                crate::webdav::path_resolver::WEBDAV_PATH_CACHE_PREFIX,
                crate::webdav::path_resolver::WEBDAV_PARENT_CACHE_PREFIX,
                crate::services::folder_service::FOLDER_PATH_CACHE_PREFIX,
            ]
        );
    }

    #[test]
    fn sync_required_includes_folder_path_prefix() {
        let prefixes = super::cache_invalidation_prefixes(StorageChangeKind::SyncRequired);

        assert_eq!(
            prefixes,
            vec![
                crate::webdav::path_resolver::WEBDAV_PATH_CACHE_PREFIX,
                crate::webdav::path_resolver::WEBDAV_PARENT_CACHE_PREFIX,
                crate::services::folder_service::FOLDER_PATH_CACHE_PREFIX,
            ]
        );
    }
}
