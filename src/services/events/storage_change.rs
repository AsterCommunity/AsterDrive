//! 存储变更事件广播和缓存失效服务。

use std::{
    collections::{BTreeSet, HashSet},
    sync::Arc,
    time::Duration as StdDuration,
};

use chrono::{DateTime, Utc};
use futures::future::join_all;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::db::repository::{file_repo, folder_repo};
use crate::errors::Result;
use crate::runtime::StorageChangeRuntimeState;
use crate::services::workspace::storage::{WorkspaceResourceScope, WorkspaceStorageScope};

/// Capacity of the local storage-change broadcast channel, in events.
pub const STORAGE_CHANGE_CHANNEL_CAPACITY: usize = 1024;
const CACHE_INVALIDATION_COALESCE_DELAY: StdDuration = StdDuration::from_millis(25);
const CACHE_INVALIDATION_RESERVATION_TTL_SECS: u64 = 1;
const CACHE_INVALIDATION_RESERVATION_PREFIX: &str = "storage_change_cache_invalidation:";
const AFFECTED_PARENT_LOOKUP_CHUNK_SIZE: usize = 500;

mod transport;
pub use transport::{build_storage_change_bus, run_cross_instance_subscription};

/// Event bus carrying storage-change events, with an optional cross-instance Redis transport.
pub type StorageChangeBus =
    aster_forge_events::TransientEventBus<StorageChangeEvent, aster_forge_events::RedisEventBus>;

#[derive(Debug, Default, PartialEq, Eq)]
struct CacheInvalidationTargets {
    prefixes: Vec<String>,
    keys: Vec<String>,
}

/// Intended recipients of a storage-change event, derived from the originating storage scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorageChangeAudience {
    /// Only the named user (personal scope events).
    User(i64),
    /// Only members of the named team (team scope events).
    Team(i64),
    /// Everyone; used for instance-wide signals such as sync-required.
    Any,
}

fn default_storage_change_audience() -> StorageChangeAudience {
    StorageChangeAudience::Any
}

/// Kind of storage mutation that produced the event; the serde wire values use dotted
/// lowercase names and are part of the client-facing event protocol.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageChangeKind {
    /// A new file was created.
    #[serde(rename = "file.created")]
    FileCreated,
    /// File metadata or content changed without a scope transition.
    #[serde(rename = "file.updated")]
    FileUpdated,
    /// The file was moved to trash.
    #[serde(rename = "file.trashed")]
    FileTrashed,
    /// The file was restored from trash.
    #[serde(rename = "file.restored_from_trash")]
    FileRestoredFromTrash,
    /// The file was permanently deleted.
    #[serde(rename = "file.purged")]
    FilePurged,
    /// A historical version was restored as the file's current head.
    #[serde(rename = "file.version_restored")]
    FileVersionRestored,
    /// A historical version was deleted from the file's history.
    #[serde(rename = "file.version_deleted")]
    FileVersionDeleted,
    /// A new folder was created.
    #[serde(rename = "folder.created")]
    FolderCreated,
    /// Folder metadata or hierarchy changed.
    #[serde(rename = "folder.updated")]
    FolderUpdated,
    /// The folder (with its contents) was moved to trash.
    #[serde(rename = "folder.trashed")]
    FolderTrashed,
    /// The folder was restored from trash.
    #[serde(rename = "folder.restored_from_trash")]
    FolderRestoredFromTrash,
    /// The folder was permanently deleted.
    #[serde(rename = "folder.purged")]
    FolderPurged,
    /// A tag was created.
    #[serde(rename = "tag.created")]
    TagCreated,
    /// A tag definition (name, color, order) was updated.
    #[serde(rename = "tag.updated")]
    TagUpdated,
    /// A tag definition was deleted.
    #[serde(rename = "tag.deleted")]
    TagDeleted,
    /// Tags were attached to or detached from entities.
    #[serde(rename = "tag.assignment_changed")]
    TagAssignmentChanged,
    /// A lock was created on a file.
    #[serde(rename = "lock.created")]
    LockCreated,
    /// A lock was released from a file.
    #[serde(rename = "lock.deleted")]
    LockDeleted,
    /// All trashed items in a scope were purged at once.
    #[serde(rename = "trash.purged_all")]
    TrashPurgedAll,
    /// Clients should re-synchronize against authoritative state; no entity ids are listed.
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
            Self::LockCreated | Self::LockDeleted | Self::TrashPurgedAll => false,
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
            | Self::LockDeleted
            | Self::TrashPurgedAll => false,
        }
    }
}

/// Workspace the change belongs to, serialized into the client-facing event payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageChangeWorkspace {
    /// The change happened in a user's personal storage.
    Personal,
    /// The change happened in the storage of the given team.
    Team {
        /// Id of the team whose storage changed.
        team_id: i64,
    },
}

/// Broadcast event describing a change in storage contents. Distributed over the local
/// bus and (when configured) a cross-instance Redis transport, and consumed by SSE
/// clients plus cache invalidation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageChangeEvent {
    #[serde(skip, default = "default_storage_change_audience")]
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(ignore))]
    pub(crate) audience: StorageChangeAudience,
    /// Type of mutation that produced this event.
    pub kind: StorageChangeKind,
    /// Workspace the change happened in; `None` on instance-wide events without a scope.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(required = true))]
    pub workspace: Option<StorageChangeWorkspace>,
    /// Deduplicated, sorted ids of files affected by the change.
    pub file_ids: Vec<i64>,
    /// Deduplicated, sorted ids of folders affected by the change.
    pub folder_ids: Vec<i64>,
    /// Deduplicated, sorted ids of the parents of affected entities; used to refresh listings.
    pub affected_parent_ids: Vec<i64>,
    /// True when the storage root itself is an affected parent.
    pub root_affected: bool,
    /// True when this event must be applied to stored usage accounting.
    pub affects_quota: bool,
    /// Signed byte delta to apply to usage; `None` when only a refresh is required.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(required = true))]
    pub storage_delta: Option<i64>,
    /// Server timestamp (UTC) at event creation.
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

    /// Refresh authoritative usage after replaying a publication whose original event may
    /// have been lost. No arithmetic delta is replayed for already committed mutations.
    pub(crate) fn with_quota_refresh(mut self) -> Self {
        self.affects_quota = true;
        self.storage_delta = None;
        self
    }

    /// Instance-wide signal telling clients to re-fetch authoritative state, e.g. after
    /// a cross-instance subscription gap. Carries no entity ids or storage delta.
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

    /// Whether the event should be delivered to the given user, based on its audience
    /// and the user's team memberships.
    pub fn is_visible_to(&self, user_id: i64, team_ids: &HashSet<i64>) -> bool {
        match self.audience {
            StorageChangeAudience::Any => true,
            StorageChangeAudience::User(target_user_id) => target_user_id == user_id,
            StorageChangeAudience::Team(team_id) => team_ids.contains(&team_id),
        }
    }
}

/// Publish a storage-change event: invalidates related caches locally, broadcasts to
/// local subscribers, and forwards the event to other instances when a transport is
/// configured. Failures degrade to logged warnings, never to caller errors.
pub fn publish<S: StorageChangeRuntimeState>(state: &S, event: StorageChangeEvent) {
    transport::publish_cross_instance(state, &event);
    publish_local(state, event);
}

pub(super) fn publish_local<S: StorageChangeRuntimeState>(state: &S, event: StorageChangeEvent) {
    invalidate_storage_change_caches(state.cache().clone(), &event);
    if let Err(e) = state.storage_change_bus().publish_local(event) {
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

fn invalidate_storage_change_caches(
    cache: Arc<dyn aster_forge_cache::CacheBackend>,
    event: &StorageChangeEvent,
) {
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
            crate::webdav::backend::path_resolver::path_cache_personal_prefix(user_id),
            crate::webdav::backend::path_resolver::parent_cache_personal_prefix(user_id),
        ],
        StorageChangeAudience::Team(team_id) => vec![
            crate::webdav::backend::path_resolver::path_cache_team_prefix(team_id),
            crate::webdav::backend::path_resolver::parent_cache_team_prefix(team_id),
        ],
        StorageChangeAudience::Any => vec![
            crate::webdav::backend::path_resolver::WEBDAV_PATH_CACHE_PREFIX.to_string(),
            crate::webdav::backend::path_resolver::WEBDAV_PARENT_CACHE_PREFIX.to_string(),
        ],
    }
}

async fn schedule_cache_prefix_invalidation(
    cache: Arc<dyn aster_forge_cache::CacheBackend>,
    prefix: String,
) {
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
                crate::webdav::backend::path_resolver::path_cache_personal_prefix(11),
                crate::webdav::backend::path_resolver::parent_cache_personal_prefix(11),
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
                crate::webdav::backend::path_resolver::path_cache_team_prefix(42),
                crate::webdav::backend::path_resolver::parent_cache_team_prefix(42),
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
                crate::webdav::backend::path_resolver::path_cache_personal_prefix(11),
                crate::webdav::backend::path_resolver::parent_cache_personal_prefix(11),
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
                crate::webdav::backend::path_resolver::WEBDAV_PATH_CACHE_PREFIX.to_string(),
                crate::webdav::backend::path_resolver::WEBDAV_PARENT_CACHE_PREFIX.to_string(),
                crate::services::files::folder::FOLDER_PATH_CACHE_PREFIX.to_string(),
            ]
        );
        assert!(targets.keys.is_empty());
    }

    #[test]
    fn quota_refresh_preserves_scope_and_clears_any_previous_delta() {
        for scope in [
            WorkspaceStorageScope::Personal { user_id: 11 },
            WorkspaceStorageScope::Team {
                team_id: 42,
                actor_user_id: 11,
            },
        ] {
            for delta in [-512, 0, 512] {
                let event = StorageChangeEvent::new(
                    StorageChangeKind::FileCreated,
                    scope,
                    vec![7],
                    vec![],
                    vec![None],
                )
                .with_storage_delta(delta)
                .with_quota_refresh();
                assert!(event.affects_quota);
                assert!(event.storage_delta.is_none());
                assert_eq!(event.file_ids, vec![7]);
                assert!(event.root_affected);
                let payload = serde_json::to_value(&event).unwrap();
                assert_eq!(payload["affects_quota"], true);
                assert!(payload["storage_delta"].is_null());
                match scope {
                    WorkspaceStorageScope::Personal { user_id } => {
                        assert_eq!(event.audience, super::StorageChangeAudience::User(user_id));
                        assert_eq!(payload["workspace"]["kind"], "personal");
                    }
                    WorkspaceStorageScope::Team { team_id, .. } => {
                        assert_eq!(event.audience, super::StorageChangeAudience::Team(team_id));
                        assert_eq!(payload["workspace"]["team_id"], team_id);
                    }
                }
            }
        }
    }

    #[test]
    fn trash_purged_all_keeps_scoped_wire_kind_without_path_cache_invalidation() {
        let event = StorageChangeEvent::new(
            StorageChangeKind::TrashPurgedAll,
            WorkspaceStorageScope::Personal { user_id: 11 },
            vec![],
            vec![],
            vec![],
        )
        .with_storage_delta(-512);

        let serialized = serde_json::to_value(&event).unwrap();
        assert_eq!(serialized["kind"], "trash.purged_all");
        assert_eq!(serialized["workspace"]["kind"], "personal");
        assert_eq!(serialized["storage_delta"], -512);
        assert_eq!(serialized["affects_quota"], true);
        assert!(!event.kind.invalidates_folder_path_cache());
        assert!(!event.kind.invalidates_webdav_path_cache());

        let targets = super::cache_invalidation_targets(&event);
        assert!(targets.prefixes.is_empty());
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
