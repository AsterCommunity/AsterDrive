use super::owner_info::{deserialize_resource_lock_owner_info, serialize_resource_lock_owner_info};
use super::*;

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use aster_drive_migration::Migrator;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, Set};

use crate::config::{Config, DatabaseConfig, RuntimeConfig};
use crate::db::repository::{lock_namespace_repo, lock_repo};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::mail::sender;
use crate::services::workspace::storage::WorkspaceResourceScope;
use crate::storage::{DriverRegistry, PolicySnapshot};
use aster_drive_model::entities::{
    file, file_blob, folder, resource_lock, storage_policy, team, user,
};
use aster_drive_model::types::{
    DriverType, EntityType, LockDepth, LockMode, LockOrigin, LockRootKind, LockWorkspaceType,
    StoredLockOwnerInfo, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions, UserRole,
    UserStatus,
};
use aster_forge_cache as cache;
use aster_forge_cache::{CacheBackend, CacheConfig, CacheExt, MemoryCache};
use aster_forge_webdav::{
    DavLockAcquireRequest, DavLockError, DavLockSystem, DavMutationCredentials, DavPath,
};

struct ProjectionCacheSpy {
    inner: MemoryCache,
    get_calls: AtomicUsize,
    set_calls: AtomicUsize,
    fail_reads: bool,
}

impl ProjectionCacheSpy {
    fn new(fail_reads: bool) -> Self {
        Self {
            inner: MemoryCache::new(300),
            get_calls: AtomicUsize::new(0),
            set_calls: AtomicUsize::new(0),
            fail_reads,
        }
    }
}

#[async_trait]
impl CacheBackend for ProjectionCacheSpy {
    fn backend_name(&self) -> &'static str {
        "resource-lock-test"
    }

    async fn health_check(&self) -> aster_forge_cache::Result<()> {
        Ok(())
    }

    async fn get_bytes(&self, key: &str) -> Option<Vec<u8>> {
        self.get_calls.fetch_add(1, Ordering::Relaxed);
        if self.fail_reads {
            None
        } else {
            self.inner.get_bytes(key).await
        }
    }

    async fn take_bytes(&self, key: &str) -> Option<Vec<u8>> {
        self.inner.take_bytes(key).await
    }

    async fn set_bytes(&self, key: &str, value: Vec<u8>, ttl_secs: Option<u64>) {
        self.set_calls.fetch_add(1, Ordering::Relaxed);
        self.inner.set_bytes(key, value, ttl_secs).await;
    }

    async fn set_bytes_if_absent(&self, key: &str, value: Vec<u8>, ttl_secs: Option<u64>) -> bool {
        self.inner.set_bytes_if_absent(key, value, ttl_secs).await
    }

    async fn delete(&self, key: &str) {
        self.inner.delete(key).await;
    }

    async fn delete_many(&self, keys: &[String]) {
        self.inner.delete_many(keys).await;
    }

    async fn invalidate_prefix(&self, prefix: &str) {
        self.inner.invalidate_prefix(prefix).await;
    }
}

struct LockTestFixture {
    state: PrimaryAppState,
    user: user::Model,
    team: team::Model,
    folder: folder::Model,
    file: file::Model,
}

fn sample_lock(owner_info: Option<StoredLockOwnerInfo>) -> resource_lock::Model {
    resource_lock::Model {
        id: 42,
        token: "urn:uuid:test".to_string(),
        namespace_id: 3,
        root_kind: LockRootKind::File,
        root_folder_id: None,
        root_file_id: Some(7),
        depth: LockDepth::Resource,
        mode: LockMode::Exclusive,
        origin: LockOrigin::Product,
        holder_user_id: Some(9),
        owner_info,
        timeout_at: None,
        lockroot_path: Some("/docs/report.txt".to_string()),
        created_at: Utc::now(),
    }
}

async fn build_lock_test_fixture() -> LockTestFixture {
    let temp_root =
        std::env::temp_dir().join(format!("asterdrive-lock-service-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_root).expect("lock service temp root should exist");

    let db = crate::db::connect_with_metrics(
        &DatabaseConfig {
            url: "sqlite::memory:".into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .expect("lock service test DB should connect");
    Migrator::up(&db, None)
        .await
        .expect("lock service migrations should succeed");

    let now = Utc::now();
    let policy = storage_policy::ActiveModel {
        name: Set("Lock Test Policy".to_string()),
        driver_type: Set(DriverType::Local),
        endpoint: Set(String::new()),
        bucket: Set(String::new()),
        access_key: Set(String::new()),
        secret_key: Set(String::new()),
        base_path: Set(temp_root.join("uploads").to_string_lossy().into_owned()),
        max_file_size: Set(0),
        allowed_types: Set(StoredStoragePolicyAllowedTypes::empty()),
        options: Set(StoredStoragePolicyOptions::empty()),
        is_default: Set(true),
        chunk_size: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("lock test policy should insert");

    let user = user::ActiveModel {
        username: Set(format!("lock-user-{}", uuid::Uuid::new_v4())),
        email: Set(format!("lock-{}@example.com", uuid::Uuid::new_v4())),
        password_hash: Set("not-used".to_string()),
        role: Set(UserRole::User),
        status: Set(UserStatus::Active),
        session_version: Set(0),
        email_verified_at: Set(Some(now)),
        pending_email: Set(None),
        storage_used: Set(0),
        storage_quota: Set(0),
        policy_group_id: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        config: Set(None),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("lock test user should insert");

    let team = team::ActiveModel {
        name: Set("Lock Test Team".to_string()),
        description: Set(String::new()),
        created_by: Set(user.id),
        storage_used: Set(0),
        storage_quota: Set(0),
        policy_group_id: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        archived_at: Set(None),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("lock test team should insert");

    let folder = folder::ActiveModel {
        name: Set("docs".to_string()),
        parent_id: Set(None),
        team_id: Set(None),
        owner_user_id: Set(Some(user.id)),
        created_by_user_id: Set(Some(user.id)),
        created_by_username: Set(user.username.clone()),
        policy_id: Set(Some(policy.id)),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("lock test folder should insert");

    let blob = file_blob::ActiveModel {
        hash: Set(format!("lock-blob-{}", uuid::Uuid::new_v4())),
        size: Set(1),
        policy_id: Set(policy.id),
        storage_path: Set(format!("files/{}", uuid::Uuid::new_v4())),
        thumbnail_path: Set(None),
        thumbnail_processor: Set(None),
        thumbnail_version: Set(None),
        ref_count: Set(1),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("lock test blob should insert");

    let file = file::ActiveModel {
        name: Set("lock-target.txt".to_string()),
        folder_id: Set(Some(folder.id)),
        team_id: Set(None),
        blob_id: Set(blob.id),
        size: Set(1),
        owner_user_id: Set(Some(user.id)),
        created_by_user_id: Set(Some(user.id)),
        created_by_username: Set(user.username.clone()),
        mime_type: Set("text/plain".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("lock test file should insert");

    let runtime_config = Arc::new(RuntimeConfig::new());
    let cache = cache::create_cache(&CacheConfig::default()).await;
    let mut config = Config::default();
    config.server.temp_dir = temp_root.join(".tmp").to_string_lossy().into_owned();
    config.server.upload_temp_dir = temp_root.join(".uploads").to_string_lossy().into_owned();
    let storage_change_bus = crate::services::events::storage_change::StorageChangeBus::new(
        crate::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
    );
    let share_download_rollback =
        crate::services::share::spawn_detached_share_download_rollback_queue(
            db.clone(),
            crate::config::operations::share_download_rollback_queue_capacity(&runtime_config),
        );

    let state = PrimaryAppState {
        db_handles: aster_forge_db::DbHandles::single(db),
        driver_registry: Arc::new(DriverRegistry::noop()),
        runtime_config: runtime_config.clone(),
        policy_snapshot: Arc::new(PolicySnapshot::new()),
        config: Arc::new(config),
        cache,
        config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
        metrics: aster_drive_metrics::NoopMetrics::arc(),
        mail_sender: sender::runtime_sender(runtime_config),
        storage_change_bus,
        share_download_rollback,
        background_task_dispatch_wakeup:
            crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
        remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
    };

    LockTestFixture {
        state,
        user,
        team,
        folder,
        file,
    }
}

async fn namespace_generation(
    fixture: &LockTestFixture,
    workspace_type: LockWorkspaceType,
    workspace_id: i64,
) -> i64 {
    lock_namespace_repo::find_by_workspace(fixture.state.writer_db(), workspace_type, workspace_id)
        .await
        .expect("namespace should query")
        .expect("namespace should exist")
        .generation
}

async fn projected_file_state(fixture: &LockTestFixture) -> ResourceLockState {
    let states = load_for_scope(
        &fixture.state,
        WorkspaceResourceScope::Personal {
            user_id: fixture.user.id,
        },
        std::slice::from_ref(&fixture.file),
        &[],
    )
    .await
    .expect("lock projection should load");
    state_for(&states, EntityType::File, fixture.file.id)
}

fn apply_webdav_lock_limit(fixture: &LockTestFixture, value: &str) {
    fixture
        .state
        .runtime_config
        .apply(aster_forge_db::system_config::Model {
            id: 1,
            key: crate::config::definitions::WEBDAV_MAX_ACTIVE_LOCKS_PER_USER_KEY.to_string(),
            value: value.to_string(),
            value_type: aster_forge_config::ConfigValueType::Number,
            requires_restart: false,
            is_sensitive: false,
            source: aster_forge_config::ConfigSource::System,
            visibility: aster_forge_config::ConfigVisibility::Private,
            namespace: String::new(),
            category: crate::config::definitions::CONFIG_CATEGORY_WEBDAV.to_string(),
            description: "test".to_string(),
            updated_at: Utc::now(),
            updated_by: None,
        });
}

#[test]
fn serializes_and_deserializes_owner_payloads() {
    for owner_info in [
        ResourceLockOwnerInfo::Wopi(WopiLockOwnerInfo {
            app_key: "collabora".to_string(),
            lock: "lock-123".to_string(),
        }),
        ResourceLockOwnerInfo::Webdav(WebdavLockOwnerInfo {
            xml: "<D:owner xmlns:D=\"DAV:\"><D:href>mailto:test@example.com</D:href></D:owner>"
                .to_string(),
        }),
        ResourceLockOwnerInfo::Text(TextLockOwnerInfo {
            value: "user@example.com".to_string(),
        }),
    ] {
        let stored = serialize_resource_lock_owner_info(Some(&owner_info))
            .expect("owner payload should serialize")
            .expect("stored owner info should exist");
        let parsed = deserialize_resource_lock_owner_info(&sample_lock(Some(stored)))
            .expect("owner payload should deserialize");
        assert_eq!(parsed, Some(owner_info));
    }
}

#[test]
fn rejects_legacy_or_unknown_owner_payloads() {
    for raw in [
        "<D:owner xmlns:D=\"DAV:\"><D:href>mailto:test@example.com</D:href></D:owner>",
        "user@example.com",
        r#"{"kind":"legacy","value":"user@example.com"}"#,
    ] {
        let error = deserialize_resource_lock_owner_info(&sample_lock(Some(StoredLockOwnerInfo(
            raw.to_string(),
        ))))
        .expect_err("invalid owner payload should be rejected");
        assert!(
            error
                .to_string()
                .contains("deserialize resource lock owner payload")
        );
    }
}

#[tokio::test]
async fn expired_lock_is_replaced_and_generation_advances_once_per_commit() {
    let fixture = build_lock_test_fixture().await;
    acquire(
        &fixture.state,
        LockTarget {
            workspace: LockWorkspace::Personal {
                user_id: fixture.user.id,
            },
            root: LockRoot::File {
                file_id: fixture.file.id,
            },
            depth: LockDepth::Resource,
        },
        LockMode::Exclusive,
        LockOrigin::Product,
        Some(fixture.user.id),
        None,
        Some(Duration::seconds(-5)),
        Some("/docs/lock-target.txt".to_string()),
    )
    .await
    .expect("expired lock fixture should acquire");

    let replacement = lock(
        &fixture.state,
        aster_drive_model::types::EntityType::File,
        fixture.file.id,
        Some(fixture.user.id),
        None,
        Some(Duration::seconds(30)),
    )
    .await
    .expect("expired lock should be replaced");

    let locks = lock_repo::find_all(fixture.state.writer_db())
        .await
        .expect("locks should load");
    assert_eq!(locks.len(), 1);
    assert_eq!(locks[0].id, replacement.id);
    assert_eq!(
        namespace_generation(&fixture, LockWorkspaceType::Personal, fixture.user.id).await,
        2
    );
}

#[tokio::test]
async fn expired_cleanup_is_idempotent_and_empty_database_needs_no_namespace() {
    let fixture = build_lock_test_fixture().await;
    assert_eq!(cleanup_expired(&fixture.state).await.unwrap(), 0);
    assert!(
        lock_namespace_repo::find_by_workspace(
            fixture.state.writer_db(),
            LockWorkspaceType::Personal,
            fixture.user.id,
        )
        .await
        .unwrap()
        .is_none()
    );
    let orphan_namespace = lock_namespace_repo::ensure_and_lock(
        fixture.state.writer_db(),
        LockWorkspaceType::Personal,
        fixture.user.id + 500_000,
    )
    .await
    .unwrap();
    assert_eq!(orphan_namespace.generation, 0);
    assert_eq!(cleanup_expired(&fixture.state).await.unwrap(), 0);
    assert_eq!(
        lock_namespace_repo::find_by_id(fixture.state.writer_db(), orphan_namespace.id)
            .await
            .unwrap()
            .unwrap()
            .generation,
        0
    );

    acquire(
        &fixture.state,
        LockTarget {
            workspace: LockWorkspace::Personal {
                user_id: fixture.user.id,
            },
            root: LockRoot::File {
                file_id: fixture.file.id,
            },
            depth: LockDepth::Resource,
        },
        LockMode::Exclusive,
        LockOrigin::Product,
        Some(fixture.user.id),
        None,
        Some(Duration::seconds(-1)),
        Some("/docs/lock-target.txt".to_string()),
    )
    .await
    .unwrap();

    assert_eq!(cleanup_expired(&fixture.state).await.unwrap(), 1);
    assert_eq!(
        namespace_generation(&fixture, LockWorkspaceType::Personal, fixture.user.id).await,
        2
    );
    assert!(
        lock_repo::find_all(fixture.state.writer_db())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(cleanup_expired(&fixture.state).await.unwrap(), 0);
    assert_eq!(
        namespace_generation(&fixture, LockWorkspaceType::Personal, fixture.user.id).await,
        2
    );
}

#[tokio::test]
async fn holder_cleanup_advances_generation_for_surviving_namespace() {
    let fixture = build_lock_test_fixture().await;
    let target = LockTarget {
        workspace: LockWorkspace::Team {
            team_id: fixture.team.id,
        },
        root: LockRoot::WorkspaceRoot,
        depth: LockDepth::Infinity,
    };
    acquire(
        &fixture.state,
        target,
        LockMode::Shared,
        LockOrigin::WebDav,
        Some(fixture.user.id),
        None,
        None,
        Some("/".to_string()),
    )
    .await
    .unwrap();
    let surviving_holder_id = fixture.user.id + 100_000;
    acquire(
        &fixture.state,
        target,
        LockMode::Shared,
        LockOrigin::WebDav,
        Some(surviving_holder_id),
        None,
        None,
        Some("/".to_string()),
    )
    .await
    .unwrap();

    let txn = aster_forge_db::transaction::begin(fixture.state.writer_db())
        .await
        .unwrap();
    assert_eq!(
        delete_all_held_by_on(&txn, fixture.user.id).await.unwrap(),
        1
    );
    aster_forge_db::transaction::commit(txn).await.unwrap();

    let remaining = lock_repo::find_by_owner(fixture.state.writer_db(), surviving_holder_id)
        .await
        .unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(
        namespace_generation(&fixture, LockWorkspaceType::Team, fixture.team.id).await,
        3
    );

    let txn = aster_forge_db::transaction::begin(fixture.state.writer_db())
        .await
        .unwrap();
    assert_eq!(
        delete_all_held_by_on(&txn, fixture.user.id + 200_000)
            .await
            .unwrap(),
        0
    );
    aster_forge_db::transaction::commit(txn).await.unwrap();
    assert_eq!(
        namespace_generation(&fixture, LockWorkspaceType::Team, fixture.team.id).await,
        3
    );
}

#[tokio::test]
async fn deleting_team_workspace_namespace_removes_workspace_root_lock() {
    let fixture = build_lock_test_fixture().await;
    let root_lock = acquire(
        &fixture.state,
        LockTarget {
            workspace: LockWorkspace::Team {
                team_id: fixture.team.id,
            },
            root: LockRoot::WorkspaceRoot,
            depth: LockDepth::Infinity,
        },
        LockMode::Exclusive,
        LockOrigin::WebDav,
        Some(fixture.user.id),
        None,
        None,
        Some("/".to_string()),
    )
    .await
    .unwrap();

    let txn = aster_forge_db::transaction::begin(fixture.state.writer_db())
        .await
        .unwrap();
    assert_eq!(
        delete_workspace_namespace_on(&txn, LockWorkspaceType::Team, fixture.team.id)
            .await
            .unwrap(),
        1
    );
    aster_forge_db::transaction::commit(txn).await.unwrap();

    assert!(
        lock_namespace_repo::find_by_workspace(
            fixture.state.writer_db(),
            LockWorkspaceType::Team,
            fixture.team.id,
        )
        .await
        .unwrap()
        .is_none()
    );
    assert!(
        lock_repo::find_by_token(fixture.state.writer_db(), &root_lock.token)
            .await
            .unwrap()
            .is_none()
    );

    let txn = aster_forge_db::transaction::begin(fixture.state.writer_db())
        .await
        .unwrap();
    assert_eq!(
        delete_workspace_namespace_on(&txn, LockWorkspaceType::Team, fixture.team.id)
            .await
            .unwrap(),
        0
    );
    aster_forge_db::transaction::commit(txn).await.unwrap();
}

#[tokio::test]
async fn concurrent_webdav_locks_share_one_owner_quota_across_workspaces() {
    let fixture = build_lock_test_fixture().await;
    apply_webdav_lock_limit(&fixture, "1");
    let personal = crate::webdav::backend::lock::DbLockSystem::new(
        fixture.state.clone(),
        fixture.user.id,
        None,
    );
    let team = crate::webdav::backend::lock::DbLockSystem::new_with_audit(
        fixture.state.clone(),
        crate::services::workspace::storage::WorkspaceStorageScope::Team {
            team_id: fixture.team.id,
            actor_user_id: fixture.user.id,
        },
        None,
        crate::services::ops::audit::AuditContext {
            user_id: fixture.user.id,
            ip_address: None,
            user_agent: None,
        },
    );
    let personal_path = DavPath::new("/").unwrap();
    let team_path = DavPath::new("/").unwrap();

    let (personal_result, team_result) = tokio::join!(
        personal.lock(DavLockAcquireRequest {
            path: &personal_path,
            principal: None,
            owner: None,
            timeout: Some(std::time::Duration::from_secs(60)),
            shared: false,
            deep: true,
            credentials: DavMutationCredentials::default(),
        }),
        team.lock(DavLockAcquireRequest {
            path: &team_path,
            principal: None,
            owner: None,
            timeout: Some(std::time::Duration::from_secs(60)),
            shared: false,
            deep: true,
            credentials: DavMutationCredentials::default(),
        }),
    );
    let results = [personal_result, team_result];
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(DavLockError::LimitExceeded)))
            .count(),
        1
    );
    assert_eq!(
        lock_repo::count_active_by_owner(fixture.state.writer_db(), fixture.user.id, Utc::now())
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn exclusive_and_shared_compatibility_is_enforced() {
    let fixture = build_lock_test_fixture().await;
    let target = LockTarget {
        workspace: LockWorkspace::Personal {
            user_id: fixture.user.id,
        },
        root: LockRoot::File {
            file_id: fixture.file.id,
        },
        depth: LockDepth::Resource,
    };
    acquire(
        &fixture.state,
        target,
        LockMode::Shared,
        LockOrigin::WebDav,
        Some(fixture.user.id),
        None,
        None,
        Some("/docs/lock-target.txt".to_string()),
    )
    .await
    .expect("first shared lock should acquire");
    acquire(
        &fixture.state,
        target,
        LockMode::Shared,
        LockOrigin::WebDav,
        Some(fixture.user.id + 1),
        None,
        None,
        Some("/docs/lock-target.txt".to_string()),
    )
    .await
    .expect("second shared lock should acquire");

    let error = acquire(
        &fixture.state,
        target,
        LockMode::Exclusive,
        LockOrigin::Product,
        Some(fixture.user.id),
        None,
        None,
        Some("/docs/lock-target.txt".to_string()),
    )
    .await
    .expect_err("exclusive lock should conflict with active shared locks");
    assert!(matches!(
        error,
        crate::errors::AsterError::ResourceLocked(_)
    ));
}

#[tokio::test]
async fn protocol_locks_require_their_internal_token_not_a_matching_holder() {
    let fixture = build_lock_test_fixture().await;
    let target = LockTarget {
        workspace: LockWorkspace::Personal {
            user_id: fixture.user.id,
        },
        root: LockRoot::File {
            file_id: fixture.file.id,
        },
        depth: LockDepth::Resource,
    };
    let webdav_lock = acquire(
        &fixture.state,
        target,
        LockMode::Exclusive,
        LockOrigin::WebDav,
        Some(fixture.user.id),
        None,
        None,
        Some("/docs/lock-target.txt".to_string()),
    )
    .await
    .expect("WebDAV lock should acquire");

    let error = enforce_file_mutation(
        fixture.state.writer_db(),
        &fixture.file,
        &LockMutationCredentials::HolderUser(fixture.user.id).submitted(),
    )
    .await
    .expect_err("matching holder must not authorize a WebDAV lock");
    assert!(matches!(
        error,
        crate::errors::AsterError::ResourceLocked(_)
    ));

    enforce_file_mutation(
        fixture.state.writer_db(),
        &fixture.file,
        &LockMutationCredentials::SubmittedTokens(vec![webdav_lock.token]).submitted(),
    )
    .await
    .expect("the internal WebDAV lock token should authorize the mutation");
}

#[tokio::test]
async fn folder_infinity_lock_conflicts_with_descendant_file_lock() {
    let fixture = build_lock_test_fixture().await;
    acquire(
        &fixture.state,
        LockTarget {
            workspace: LockWorkspace::Personal {
                user_id: fixture.user.id,
            },
            root: LockRoot::Folder {
                folder_id: fixture.folder.id,
            },
            depth: LockDepth::Infinity,
        },
        LockMode::Exclusive,
        LockOrigin::WebDav,
        Some(fixture.user.id),
        None,
        None,
        Some("/docs".to_string()),
    )
    .await
    .expect("folder infinity lock should acquire");

    let error = lock(
        &fixture.state,
        aster_drive_model::types::EntityType::File,
        fixture.file.id,
        Some(fixture.user.id),
        None,
        None,
    )
    .await
    .expect_err("descendant file lock should conflict");
    assert!(matches!(
        error,
        crate::errors::AsterError::ResourceLocked(_)
    ));
}

#[tokio::test]
async fn token_unlock_force_unlock_refresh_and_replace_update_generation() {
    let fixture = build_lock_test_fixture().await;
    let first = lock(
        &fixture.state,
        aster_drive_model::types::EntityType::File,
        fixture.file.id,
        Some(fixture.user.id),
        None,
        None,
    )
    .await
    .expect("file lock should acquire");
    let refreshed_at = Utc::now() + Duration::minutes(5);
    let refreshed =
        refresh_by_token_on(fixture.state.writer_db(), &first.token, Some(refreshed_at))
            .await
            .expect("lock should refresh");
    assert_eq!(refreshed.timeout_at, Some(refreshed_at));
    let replaced_at = Utc::now() + Duration::minutes(10);
    let replacement_owner = ResourceLockOwnerInfo::Wopi(WopiLockOwnerInfo {
        app_key: "onlyoffice".to_string(),
        lock: "replacement-lock".to_string(),
    });
    let replaced = replace_owner_info_and_timeout_by_token_on(
        fixture.state.writer_db(),
        &first.token,
        replacement_owner.clone(),
        Some(replaced_at),
    )
    .await
    .expect("lock owner payload should replace");
    assert_eq!(replaced.timeout_at, Some(replaced_at));
    assert_eq!(
        deserialize_resource_lock_owner_info(&replaced).expect("owner payload should deserialize"),
        Some(replacement_owner)
    );
    unlock_by_token(&fixture.state, &first.token)
        .await
        .expect("token unlock should succeed");

    let second = lock(
        &fixture.state,
        aster_drive_model::types::EntityType::File,
        fixture.file.id,
        Some(fixture.user.id),
        None,
        None,
    )
    .await
    .expect("second file lock should acquire");
    force_unlock(&fixture.state, second.id)
        .await
        .expect("force unlock should succeed");

    assert!(
        lock_repo::find_all(fixture.state.writer_db())
            .await
            .expect("locks should load")
            .is_empty()
    );
    assert_eq!(
        namespace_generation(&fixture, LockWorkspaceType::Personal, fixture.user.id).await,
        6
    );
}

#[tokio::test]
async fn lock_projection_cache_miss_then_hit_uses_one_fill() {
    let mut fixture = build_lock_test_fixture().await;
    let cache = Arc::new(ProjectionCacheSpy::new(false));
    fixture.state.cache = cache.clone();
    lock(
        &fixture.state,
        EntityType::File,
        fixture.file.id,
        Some(fixture.user.id),
        None,
        None,
    )
    .await
    .expect("file lock should acquire");

    assert!(matches!(
        projected_file_state(&fixture).await,
        ResourceLockState::Direct { .. }
    ));
    assert!(matches!(
        projected_file_state(&fixture).await,
        ResourceLockState::Direct { .. }
    ));
    assert_eq!(cache.get_calls.load(Ordering::Relaxed), 2);
    assert_eq!(cache.set_calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn malformed_or_unavailable_lock_projection_cache_falls_back_to_database() {
    let mut fixture = build_lock_test_fixture().await;
    let lock = lock(
        &fixture.state,
        EntityType::File,
        fixture.file.id,
        Some(fixture.user.id),
        None,
        None,
    )
    .await
    .expect("file lock should acquire");
    let generation =
        namespace_generation(&fixture, LockWorkspaceType::Personal, fixture.user.id).await;
    let key = format!(
        "resource_lock_projection:{}:{generation}",
        lock.namespace_id
    );
    fixture
        .state
        .cache()
        .set_bytes(&key, b"not-json".to_vec(), Some(300))
        .await;

    assert!(matches!(
        projected_file_state(&fixture).await,
        ResourceLockState::Direct { .. }
    ));
    let repaired = fixture
        .state
        .cache()
        .get::<serde_json::Value>(&key)
        .await
        .expect("malformed cache entry should be replaced");
    assert!(repaired.is_array());

    let unavailable = Arc::new(ProjectionCacheSpy::new(true));
    fixture.state.cache = unavailable.clone();
    assert!(matches!(
        projected_file_state(&fixture).await,
        ResourceLockState::Direct { .. }
    ));
    assert_eq!(unavailable.get_calls.load(Ordering::Relaxed), 1);
    assert_eq!(unavailable.set_calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn lock_projection_generation_change_ignores_stale_cached_timeout() {
    let fixture = build_lock_test_fixture().await;
    let lock = lock(
        &fixture.state,
        EntityType::File,
        fixture.file.id,
        Some(fixture.user.id),
        None,
        Some(Duration::minutes(1)),
    )
    .await
    .expect("file lock should acquire");
    let first_state = projected_file_state(&fixture).await;
    let refreshed_at = Utc::now() + Duration::minutes(10);
    refresh_by_token_on(fixture.state.writer_db(), &lock.token, Some(refreshed_at))
        .await
        .expect("lock should refresh");

    let refreshed_state = projected_file_state(&fixture).await;
    assert_ne!(first_state, refreshed_state);
    assert_eq!(
        refreshed_state,
        ResourceLockState::Direct {
            mode: LockMode::Exclusive,
            expires_at: Some(refreshed_at),
        }
    );
}

#[tokio::test]
async fn team_workspace_root_lifecycle_and_missing_team_are_distinguished() {
    let fixture = build_lock_test_fixture().await;
    let target = LockTarget {
        workspace: LockWorkspace::Team {
            team_id: fixture.team.id,
        },
        root: LockRoot::WorkspaceRoot,
        depth: LockDepth::Infinity,
    };
    let root_lock = acquire(
        &fixture.state,
        target,
        LockMode::Exclusive,
        LockOrigin::Product,
        Some(fixture.user.id),
        None,
        None,
        Some("/".to_string()),
    )
    .await
    .expect("team workspace root should lock");
    assert_eq!(root_lock.root_kind, LockRootKind::WorkspaceRoot);
    unlock_by_token(&fixture.state, &root_lock.token)
        .await
        .expect("team workspace root should unlock by token");

    let error = acquire(
        &fixture.state,
        LockTarget {
            workspace: LockWorkspace::Team {
                team_id: fixture.team.id + 100_000,
            },
            root: LockRoot::WorkspaceRoot,
            depth: LockDepth::Infinity,
        },
        LockMode::Exclusive,
        LockOrigin::Product,
        Some(fixture.user.id),
        None,
        None,
        Some("/".to_string()),
    )
    .await
    .expect_err("missing team target should be reported");
    assert!(matches!(
        error,
        crate::errors::AsterError::RecordNotFound(_)
    ));
}

#[tokio::test]
async fn workspace_root_lock_blocks_root_membership_creation() {
    let fixture = build_lock_test_fixture().await;
    acquire(
        &fixture.state,
        LockTarget {
            workspace: LockWorkspace::Personal {
                user_id: fixture.user.id,
            },
            root: LockRoot::WorkspaceRoot,
            depth: LockDepth::Resource,
        },
        LockMode::Exclusive,
        LockOrigin::Product,
        Some(fixture.user.id),
        None,
        None,
        Some("/".to_string()),
    )
    .await
    .expect("workspace root lock should acquire");

    let error = crate::services::files::folder::create_in_scope(
        &fixture.state,
        crate::services::workspace::storage::WorkspaceStorageScope::Personal {
            user_id: fixture.user.id,
        },
        "blocked-at-root",
        None,
        crate::services::files::lock::LockMutationCredentials::None,
    )
    .await
    .expect_err("workspace root lock should block root membership changes");
    assert!(matches!(
        error,
        crate::errors::AsterError::ResourceLocked(_)
    ));
}
