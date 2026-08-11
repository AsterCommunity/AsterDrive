use std::sync::Arc;

use aster_forge_db::transaction;
use aster_forge_tasks::{
    TaskClaimCandidate, TaskExecutionContext, TaskLease, TaskLeaseGuard, available_lane_capacity,
    spawn_task_heartbeat_with_interval,
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use tokio::time::{Duration, sleep};

use crate::config::DatabaseConfig;
use crate::db::repository::background_task_repo;
use crate::db::{self, repository::config_repo};
use crate::errors::AsterError;
use crate::services::files::batch as batch_service;
use crate::services::files::file as file_service;
use crate::services::files::folder as folder_service;
use crate::services::task::types::FolderTreeMutationOperation;
use crate::services::task::{
    SystemRuntimeTaskKind, is_task_lease_lost, is_task_worker_shutdown_requested,
};
use crate::services::workspace::storage::WorkspaceStorageScope;
use aster_drive_migration::Migrator;
use aster_drive_model::entities::{background_task, file, file_blob, folder, user};
use aster_drive_model::types::{
    BackgroundTaskKind, BackgroundTaskStatus, EntityType, LockDepth, LockMode, LockOrigin,
    StoredTaskPayload, UserRole, UserStatus,
};
use aster_drive_storage::error::{StorageErrorKind, storage_driver_error};
use aster_forge_file_classification::FileCategory;
use tokio_util::sync::CancellationToken;

use super::claim::{claim_candidates_for_lane, claim_due_for_lane};
use super::execute::{BackgroundTaskExecutionStore, run_claimed_tasks};
use super::lane::{TaskLane, TaskLaneConfig, task_lane};

async fn build_dispatch_test_db() -> sea_orm::DatabaseConnection {
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: "sqlite::memory:".into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .expect("dispatch test DB should connect");
    Migrator::up(&db, None)
        .await
        .expect("dispatch test migrations should succeed");
    config_repo::ensure_defaults_with_env(&db, &|_| None)
        .await
        .expect("dispatch test config defaults should exist");
    db
}

async fn build_dispatch_test_state() -> crate::runtime::PrimaryAppState {
    let db = build_dispatch_test_db().await;
    let cache = aster_forge_cache::create_cache(&aster_forge_cache::CacheConfig {
        ..Default::default()
    })
    .await;
    let runtime_config = Arc::new(crate::config::RuntimeConfig::new());
    runtime_config
        .reload(&db)
        .await
        .expect("dispatch test runtime config should reload");
    let storage_change_bus = crate::services::events::storage_change::StorageChangeBus::new(
        crate::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
    );
    let (share_download_rollback, _worker) =
        crate::services::share::build_share_download_rollback_queue(
            db.clone(),
            1,
            aster_drive_metrics::NoopMetrics::arc(),
        );

    crate::runtime::PrimaryAppState {
        db_handles: aster_forge_db::DbHandles::single(db),
        driver_registry: Arc::new(
            crate::storage::DriverRegistry::noop().expect("built-in storage connector registry"),
        ),
        runtime_config,
        policy_snapshot: Arc::new(crate::storage::PolicySnapshot::new()),
        config: Arc::new(crate::config::Config::default()),
        cache,
        config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
        metrics: aster_drive_metrics::NoopMetrics::arc(),
        mail_sender: aster_forge_mail::memory_sender(),
        storage_change_bus,
        share_download_rollback,
        background_task_dispatch_wakeup:
            crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
        remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
    }
}

async fn insert_dispatch_test_task(
    db: &sea_orm::DatabaseConnection,
    kind: BackgroundTaskKind,
    status: BackgroundTaskStatus,
    created_offset_secs: i64,
    lease_expires_at: Option<chrono::DateTime<Utc>>,
) -> background_task::Model {
    let now = Utc::now();
    background_task::ActiveModel {
        kind: Set(kind),
        status: Set(status),
        creator_user_id: Set(None),
        team_id: Set(None),
        share_id: Set(None),
        display_name: Set(format!("dispatch-claim-{created_offset_secs}")),
        payload_json: Set(StoredTaskPayload("{}".to_string())),
        result_json: Set(None),
        runtime_json: Set(None),
        steps_json: Set(None),
        progress_current: Set(0),
        progress_total: Set(0),
        status_text: Set(None),
        attempt_count: Set(0),
        max_attempts: Set(1),
        next_run_at: Set(now - chrono::Duration::seconds(1)),
        processing_token: Set(0),
        processing_started_at: Set(match status {
            BackgroundTaskStatus::Processing => Some(now - chrono::Duration::seconds(30)),
            _ => None,
        }),
        last_heartbeat_at: Set(match status {
            BackgroundTaskStatus::Processing => Some(now - chrono::Duration::seconds(30)),
            _ => None,
        }),
        lease_expires_at: Set(lease_expires_at),
        started_at: Set(match status {
            BackgroundTaskStatus::Processing => Some(now - chrono::Duration::seconds(30)),
            _ => None,
        }),
        finished_at: Set(None),
        last_error: Set(None),
        failure_can_retry: Set(None),
        expires_at: Set(now + chrono::Duration::hours(1)),
        created_at: Set(now + chrono::Duration::seconds(created_offset_secs)),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("dispatch test task should insert")
}

async fn insert_processing_system_runtime_task(
    db: &sea_orm::DatabaseConnection,
) -> background_task::Model {
    let now = Utc::now();
    background_task::ActiveModel {
        kind: Set(BackgroundTaskKind::SystemRuntime),
        status: Set(BackgroundTaskStatus::Processing),
        creator_user_id: Set(None),
        team_id: Set(None),
        share_id: Set(None),
        display_name: Set("dispatch system runtime".to_string()),
        payload_json: Set(crate::services::task::runtime::system_runtime_payload_json(
            SystemRuntimeTaskKind::BackgroundTaskDispatch,
        )
        .expect("system runtime payload should serialize")),
        result_json: Set(None),
        runtime_json: Set(None),
        steps_json: Set(None),
        progress_current: Set(0),
        progress_total: Set(1),
        status_text: Set(Some("Processing".to_string())),
        attempt_count: Set(0),
        max_attempts: Set(1),
        next_run_at: Set(now),
        processing_token: Set(7),
        processing_started_at: Set(Some(now)),
        last_heartbeat_at: Set(Some(now)),
        lease_expires_at: Set(Some(now + chrono::Duration::seconds(60))),
        started_at: Set(Some(now)),
        finished_at: Set(None),
        last_error: Set(None),
        failure_can_retry: Set(None),
        expires_at: Set(now + chrono::Duration::hours(1)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("processing system runtime task should insert")
}

fn claim_candidate(index: usize, task: &background_task::Model) -> TaskClaimCandidate {
    TaskClaimCandidate {
        index,
        task_id: task.id,
        expected_processing_token: task.processing_token,
        next_processing_token: task.processing_token + 1,
    }
}

fn test_lane_config(lane: TaskLane, limit: usize, fast_continue: bool) -> TaskLaneConfig {
    let lock_key = match lane {
        TaskLane::Archive => crate::config::operations::BACKGROUND_TASK_ARCHIVE_MAX_CONCURRENCY_KEY,
        TaskLane::Thumbnail => {
            crate::config::operations::BACKGROUND_TASK_THUMBNAIL_MAX_CONCURRENCY_KEY
        }
        TaskLane::OfflineDownload => {
            crate::config::operations::OFFLINE_DOWNLOAD_MAX_CONCURRENCY_KEY
        }
        TaskLane::StorageMigration => {
            crate::config::operations::BACKGROUND_TASK_STORAGE_MIGRATION_MAX_CONCURRENCY_KEY
        }
        TaskLane::Fallback => crate::config::operations::BACKGROUND_TASK_MAX_CONCURRENCY_KEY,
    };
    TaskLaneConfig {
        lane,
        kinds: super::super::registry::task_lane_kinds(lane),
        limit,
        fast_continue,
        lock_key,
    }
}

#[tokio::test]
async fn run_claimed_tasks_marks_non_retryable_task_failure() {
    let state = build_dispatch_test_state().await;
    let task = insert_processing_system_runtime_task(state.writer_db()).await;
    let lease = TaskLease::new(task.id, task.processing_token);

    let stats = run_claimed_tasks(
        &state,
        vec![(task.clone(), lease)],
        CancellationToken::new(),
    )
    .await
    .expect("non-retryable task failure should be recorded, not returned as dispatch error");

    assert_eq!(stats.claimed, 0);
    assert_eq!(stats.succeeded, 0);
    assert_eq!(stats.retried, 0);
    assert_eq!(stats.failed, 1);

    let stored = background_task_repo::find_by_id(state.writer_db(), task.id)
        .await
        .expect("failed task should still exist");
    assert_eq!(stored.status, BackgroundTaskStatus::Failed);
    assert_eq!(stored.attempt_count, 1);
    assert_eq!(stored.processing_started_at, None);
    assert_eq!(stored.last_heartbeat_at, None);
    assert_eq!(stored.lease_expires_at, None);
    assert_eq!(stored.failure_can_retry, Some(false));
    assert!(
        stored
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("should not be dispatched"))
    );
    assert!(stored.finished_at.is_some());
}

#[tokio::test]
async fn run_claimed_tasks_releases_pre_cancelled_task_without_running_handler() {
    let state = build_dispatch_test_state().await;
    let task = insert_processing_system_runtime_task(state.writer_db()).await;
    let lease = TaskLease::new(task.id, task.processing_token);
    let shutdown_token = CancellationToken::new();
    shutdown_token.cancel();

    let stats = run_claimed_tasks(&state, vec![(task.clone(), lease)], shutdown_token)
        .await
        .expect("shutdown release should be handled as a cooperative worker stop");

    assert_eq!(stats, aster_forge_tasks::DispatchStats::default());

    let stored = background_task_repo::find_by_id(state.writer_db(), task.id)
        .await
        .expect("released task should still exist");
    assert_eq!(stored.status, BackgroundTaskStatus::Retry);
    assert_eq!(stored.attempt_count, 0);
    assert_eq!(stored.processing_started_at, None);
    assert_eq!(stored.last_heartbeat_at, None);
    assert_eq!(stored.lease_expires_at, None);
    assert_eq!(stored.status_text, None);
    assert_eq!(stored.last_error, None);
    assert_eq!(stored.failure_can_retry, None);
    assert_eq!(stored.finished_at, None);
}

#[tokio::test]
async fn folder_tree_task_blocks_membership_changes_and_cleans_up_after_shutdown() {
    const CHILD_COUNT: usize = 5_000;
    const INSERT_BATCH: usize = 400;

    let state = build_dispatch_test_state().await;
    let now = Utc::now();
    let user = user::ActiveModel {
        username: Set("folder-tree-dispatch-user".to_string()),
        email: Set("folder-tree-dispatch@example.com".to_string()),
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
    .insert(state.writer_db())
    .await
    .expect("folder-tree dispatch user should insert");
    let root = folder::ActiveModel {
        name: Set("folder-tree-root".to_string()),
        parent_id: Set(None),
        team_id: Set(None),
        owner_user_id: Set(Some(user.id)),
        created_by_user_id: Set(Some(user.id)),
        created_by_username: Set(user.username.clone()),
        policy_id: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("folder-tree root should insert");
    let movable = folder::ActiveModel {
        name: Set("movable-folder".to_string()),
        parent_id: Set(None),
        team_id: Set(None),
        owner_user_id: Set(Some(user.id)),
        created_by_user_id: Set(Some(user.id)),
        created_by_username: Set(user.username.clone()),
        policy_id: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("movable folder should insert");
    let mut policy = crate::storage::connectors::test_support::local_policy(
        std::env::temp_dir()
            .join("folder-tree-dispatch-uploads")
            .to_string_lossy()
            .into_owned(),
    );
    policy.name = "Folder Tree Dispatch Policy".to_string();
    policy.is_default = true;
    let policy = crate::storage::connectors::test_support::insertable_policy(policy)
        .insert(state.writer_db())
        .await
        .expect("folder-tree dispatch policy should insert");
    let blob = file_blob::ActiveModel {
        hash: Set("folder-tree-dispatch-blob".to_string()),
        size: Set(0),
        policy_id: Set(policy.id),
        storage_path: Set("folder-tree-dispatch-blob".to_string()),
        ref_count: Set(2),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("folder-tree dispatch blob should insert");
    let movable_files = ["movable-file-a.txt", "movable-file-b.txt"]
        .into_iter()
        .map(|name| file::ActiveModel {
            name: Set(name.to_string()),
            folder_id: Set(None),
            team_id: Set(None),
            blob_id: Set(blob.id),
            size: Set(0),
            owner_user_id: Set(Some(user.id)),
            created_by_user_id: Set(Some(user.id)),
            created_by_username: Set(user.username.clone()),
            mime_type: Set("text/plain".to_string()),
            extension: Set("txt".to_string()),
            compound_extension: Set(None),
            file_category: Set(FileCategory::Document),
            created_at: Set(now),
            updated_at: Set(now),
            deleted_at: Set(None),
            ..Default::default()
        })
        .collect::<Vec<_>>();
    crate::db::repository::file_repo::create_many(state.writer_db(), movable_files)
        .await
        .expect("movable files should insert");
    let movable_file_a = crate::db::repository::file_repo::find_by_name_in_folder(
        state.writer_db(),
        user.id,
        None,
        "movable-file-a.txt",
    )
    .await
    .expect("movable file lookup should succeed")
    .expect("movable file should exist");
    let movable_file_b = crate::db::repository::file_repo::find_by_name_in_folder(
        state.writer_db(),
        user.id,
        None,
        "movable-file-b.txt",
    )
    .await
    .expect("movable file lookup should succeed")
    .expect("movable file should exist");

    for batch_start in (0..CHILD_COUNT).step_by(INSERT_BATCH) {
        let batch_end = (batch_start + INSERT_BATCH).min(CHILD_COUNT);
        let models = (batch_start..batch_end)
            .map(|index| folder::ActiveModel {
                name: Set(format!("child-{index:05}")),
                parent_id: Set(Some(root.id)),
                team_id: Set(None),
                owner_user_id: Set(Some(user.id)),
                created_by_user_id: Set(Some(user.id)),
                created_by_username: Set(user.username.clone()),
                policy_id: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
                deleted_at: Set(None),
                ..Default::default()
            })
            .collect();
        crate::db::repository::folder_repo::create_many(state.writer_db(), models)
            .await
            .expect("folder-tree children should insert");
    }

    for file_id in [movable_file_a.id, movable_file_b.id] {
        file_service::move_file(&state, file_id, user.id, Some(root.id))
            .await
            .expect("membership-lock fixture file should move into the source folder");
    }
    let source_child = crate::db::repository::folder_repo::find_by_name_in_parent(
        state.writer_db(),
        user.id,
        Some(root.id),
        "child-00000",
    )
    .await
    .expect("membership-lock source child lookup should succeed")
    .expect("membership-lock source child should exist");
    let membership_lock = crate::services::files::lock::acquire(
        &state,
        crate::services::files::lock::LockTarget {
            workspace: crate::services::files::lock::LockWorkspace::Personal { user_id: user.id },
            root: crate::services::files::lock::LockRoot::Folder { folder_id: root.id },
            depth: LockDepth::Resource,
        },
        LockMode::Exclusive,
        LockOrigin::Product,
        None,
        Some(crate::services::files::lock::ResourceLockOwnerInfo::Text(
            crate::services::files::lock::TextLockOwnerInfo {
                value: "source-membership-test".to_string(),
            },
        )),
        None,
        crate::services::files::lock::resolve_entity_path(
            state.writer_db(),
            EntityType::Folder,
            root.id,
        )
        .await
        .map(Some)
        .expect("membership-lock source path should resolve"),
    )
    .await
    .expect("source collection resource lock should acquire");
    let source_file_error = file_service::move_file(&state, movable_file_a.id, user.id, None)
        .await
        .expect_err("source collection lock should block moving a file out");
    assert!(matches!(source_file_error, AsterError::ResourceLocked(_)));
    let source_folder_error = folder_service::move_folder(&state, source_child.id, user.id, None)
        .await
        .expect_err("source collection lock should block moving a folder out");
    assert!(matches!(source_folder_error, AsterError::ResourceLocked(_)));
    let source_batch_result = batch_service::batch_move(
        &state,
        user.id,
        &[movable_file_a.id, movable_file_b.id],
        &[],
        None,
    )
    .await;
    let Err(source_batch_error) = source_batch_result else {
        panic!("source collection lock should abort the atomic batch move");
    };
    assert!(matches!(source_batch_error, AsterError::ResourceLocked(_)));
    crate::services::files::lock::unlock_by_token(&state, &membership_lock.token)
        .await
        .expect("source collection resource lock should release");
    for file_id in [movable_file_a.id, movable_file_b.id] {
        file_service::move_file(&state, file_id, user.id, None)
            .await
            .expect("membership-lock fixture file should move back to root");
    }

    let invalid_restore =
        crate::services::task::folder_tree::create_folder_tree_mutation_task_in_scope(
            &state,
            WorkspaceStorageScope::Personal { user_id: user.id },
            root.id,
            FolderTreeMutationOperation::Restore,
        )
        .await
        .expect_err("an active folder must not create a restore task");
    assert!(matches!(invalid_restore, AsterError::RecordNotFound(_)));

    let task = crate::services::task::folder_tree::create_folder_tree_mutation_task_in_scope(
        &state,
        WorkspaceStorageScope::Personal { user_id: user.id },
        root.id,
        FolderTreeMutationOperation::Delete,
    )
    .await
    .expect("folder-tree delete task should be created");
    crate::db::repository::folder_tree_operation_repo::stage_ids(
        state.writer_db(),
        task.id,
        EntityType::Folder,
        &[root.id, root.id],
    )
    .await
    .expect("duplicate staging IDs should insert idempotently");
    crate::db::repository::folder_tree_operation_repo::stage_ids(
        state.writer_db(),
        task.id,
        EntityType::Folder,
        &[root.id],
    )
    .await
    .expect("repeated staging should remain idempotent");
    assert_eq!(
        crate::db::repository::folder_tree_operation_repo::count(state.writer_db(), task.id)
            .await
            .expect("deduplicated staging count should load"),
        1
    );
    crate::db::repository::folder_tree_operation_repo::clear(state.writer_db(), task.id)
        .await
        .expect("staging dedupe fixture should clear before task execution");
    let claimed = claim_due_for_lane(&state, test_lane_config(TaskLane::Fallback, 1, false))
        .await
        .expect("folder-tree delete task should be claimed");
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].0.id, task.id);

    let shutdown_token = CancellationToken::new();
    let runner_state = state.clone();
    let runner_shutdown = shutdown_token.clone();
    let runner =
        tokio::spawn(
            async move { run_claimed_tasks(&runner_state, claimed, runner_shutdown).await },
        );

    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let staged = crate::db::repository::folder_tree_operation_repo::count(
                state.writer_db(),
                task.id,
            )
            .await
            .expect("staged member count should load");
            let locks = crate::db::repository::lock_repo::find_all_by_entity(
                state.writer_db(),
                EntityType::Folder,
                root.id,
            )
            .await
            .expect("folder-tree operation lock should load");
            if staged > 0 && !locks.is_empty() {
                break;
            }
            sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("folder-tree task should begin staging while holding its operation lock");

    let create_error = folder_service::create(&state, user.id, "concurrent-child", Some(root.id))
        .await
        .expect_err("operation lock should block inserting a child into the tree");
    assert!(matches!(create_error, AsterError::ResourceLocked(_)));

    let move_error = folder_service::move_folder(&state, movable.id, user.id, Some(root.id))
        .await
        .expect_err("operation lock should block moving a folder into the tree");
    assert!(matches!(move_error, AsterError::ResourceLocked(_)));

    let file_move_error =
        file_service::move_file(&state, movable_file_a.id, user.id, Some(root.id))
            .await
            .expect_err("operation lock should block moving a file into the tree");
    assert!(matches!(file_move_error, AsterError::ResourceLocked(_)));

    let batch_move_result = batch_service::batch_move(
        &state,
        user.id,
        &[movable_file_a.id, movable_file_b.id],
        &[],
        Some(root.id),
    )
    .await;
    let Err(batch_move_error) = batch_move_result else {
        panic!("operation lock should block a batch move into the tree");
    };
    assert!(matches!(batch_move_error, AsterError::ResourceLocked(_)));

    shutdown_token.cancel();
    runner
        .await
        .expect("folder-tree task runner should not panic")
        .expect("folder-tree task shutdown should be handled cooperatively");

    let stored = background_task_repo::find_by_id(state.writer_db(), task.id)
        .await
        .expect("cancelled folder-tree task should still exist");
    assert_eq!(stored.status, BackgroundTaskStatus::Retry);
    assert_eq!(stored.attempt_count, 0);
    assert_eq!(
        crate::db::repository::folder_tree_operation_repo::count(state.writer_db(), task.id)
            .await
            .expect("cancelled task staging should be queryable"),
        0
    );
    assert!(
        crate::db::repository::lock_repo::find_all_by_entity(
            state.writer_db(),
            EntityType::Folder,
            root.id,
        )
        .await
        .expect("cancelled task locks should be queryable")
        .is_empty()
    );
    assert!(
        crate::db::repository::folder_repo::find_by_id(state.writer_db(), root.id)
            .await
            .expect("folder-tree root should still exist")
            .deleted_at
            .is_none()
    );
    assert_eq!(
        crate::db::repository::folder_repo::find_by_id(state.writer_db(), movable.id)
            .await
            .expect("movable folder should still exist")
            .parent_id,
        None
    );
    for file_id in [movable_file_a.id, movable_file_b.id] {
        assert_eq!(
            crate::db::repository::file_repo::find_by_id(state.writer_db(), file_id)
                .await
                .expect("movable file should still exist")
                .folder_id,
            None
        );
    }
    assert!(
        crate::db::repository::folder_repo::find_by_name_in_parent(
            state.writer_db(),
            user.id,
            Some(root.id),
            "concurrent-child",
        )
        .await
        .expect("concurrent child lookup should succeed")
        .is_none()
    );

    let mut reclaimed = claim_due_for_lane(&state, test_lane_config(TaskLane::Fallback, 1, false))
        .await
        .expect("cancelled folder-tree task should be reclaimed");
    assert_eq!(reclaimed.len(), 1);
    let (lost_task, lost_lease) = reclaimed.pop().expect("reclaimed task should exist");
    let context = TaskExecutionContext::new(
        lost_lease,
        Duration::from_secs(60),
        CancellationToken::new(),
    );
    let lost_guard = context.lease_guard().clone();
    let lost_state = state.clone();
    let lost_runner = tokio::spawn(async move {
        crate::services::task::folder_tree::process_folder_tree_mutation_task(
            &lost_state,
            &lost_task,
            context,
        )
        .await
    });

    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let staged = crate::db::repository::folder_tree_operation_repo::count(
                state.writer_db(),
                task.id,
            )
            .await
            .expect("takeover staging count should load");
            let locks = crate::db::repository::lock_repo::find_all_by_entity(
                state.writer_db(),
                EntityType::Folder,
                root.id,
            )
            .await
            .expect("takeover operation lock should load");
            if staged > 0 && !locks.is_empty() {
                break;
            }
            sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("reclaimed folder-tree task should begin staging");

    let lost_mark = AsterError::from(lost_guard.mark_lost());
    assert!(is_task_lease_lost(&lost_mark));
    let lost_error = lost_runner
        .await
        .expect("lease-lost folder-tree runner should not panic")
        .expect_err("lease loss should stop the old folder-tree worker");
    assert!(is_task_lease_lost(&lost_error));
    assert!(
        crate::db::repository::folder_tree_operation_repo::count(state.writer_db(), task.id)
            .await
            .expect("lease-lost staging count should load")
            > 0
    );
    assert!(
        !crate::db::repository::lock_repo::find_all_by_entity(
            state.writer_db(),
            EntityType::Folder,
            root.id,
        )
        .await
        .expect("lease-lost operation lock should load")
        .is_empty()
    );
    assert!(
        background_task_repo::release_processing(
            state.writer_db(),
            task.id,
            lost_lease.processing_token,
            Utc::now(),
            BackgroundTaskStatus::Retry,
        )
        .await
        .expect("lease-lost task should return to retry")
    );

    let takeover = claim_due_for_lane(&state, test_lane_config(TaskLane::Fallback, 1, false))
        .await
        .expect("new worker should claim the lease-lost task");
    assert_eq!(takeover.len(), 1);
    let stats = run_claimed_tasks(&state, takeover, CancellationToken::new())
        .await
        .expect("new worker should finish the retained folder-tree operation");
    assert_eq!(stats.failed, 0);
    assert_eq!(stats.succeeded, 1);

    let stored = background_task_repo::find_by_id(state.writer_db(), task.id)
        .await
        .expect("taken-over folder-tree task should still exist");
    assert_eq!(stored.status, BackgroundTaskStatus::Succeeded);
    assert!(stored.result_json.is_some());
    assert_eq!(
        crate::db::repository::folder_tree_operation_repo::count(state.writer_db(), task.id)
            .await
            .expect("successful takeover staging should be queryable"),
        0
    );
    assert!(
        crate::db::repository::lock_repo::find_all_by_entity(
            state.writer_db(),
            EntityType::Folder,
            root.id,
        )
        .await
        .expect("successful takeover locks should be queryable")
        .is_empty()
    );
    assert!(
        crate::db::repository::folder_repo::find_by_id(state.writer_db(), root.id)
            .await
            .expect("taken-over folder-tree root should still exist")
            .deleted_at
            .is_some()
    );
}

#[tokio::test]
async fn task_heartbeat_can_stop_while_sqlite_writer_pool_is_busy() {
    let state = build_dispatch_test_state().await;
    let task = insert_processing_system_runtime_task(state.writer_db()).await;
    let lease = TaskLease::new(task.id, task.processing_token);
    let lease_guard = TaskLeaseGuard::new(lease, Duration::from_secs(60));
    let stop_token = CancellationToken::new();
    let writer_txn = transaction::begin(state.writer_db())
        .await
        .expect("test should acquire the only SQLite writer connection");

    // Regression guard for SQLite single-writer deployments: heartbeat may be
    // waiting in pool acquire while task code holds the only writer connection,
    // but cancelling the heartbeat must still let task completion proceed.
    let heartbeat = spawn_task_heartbeat_with_interval(
        BackgroundTaskExecutionStore::new(state.clone()),
        lease_guard,
        stop_token.clone(),
        Duration::from_millis(10),
        |now| aster_forge_tasks::task_lease_expires_at(now, super::TASK_PROCESSING_STALE_SECS),
    );
    sleep(Duration::from_millis(30)).await;

    stop_token.cancel();
    tokio::time::timeout(Duration::from_millis(200), heartbeat)
        .await
        .expect("heartbeat should stop without waiting for the busy SQLite writer pool")
        .expect("heartbeat task should not panic");

    transaction::rollback(writer_txn)
        .await
        .expect("test writer transaction should roll back");
}

#[test]
fn task_lane_keeps_archive_and_thumbnail_separate() {
    assert_eq!(
        task_lane(BackgroundTaskKind::ArchiveCompress),
        TaskLane::Archive
    );
    assert_eq!(
        task_lane(BackgroundTaskKind::ArchiveExtract),
        TaskLane::Archive
    );
    assert_eq!(
        task_lane(BackgroundTaskKind::ArchivePreviewGenerate),
        TaskLane::Archive
    );
    assert_eq!(
        task_lane(BackgroundTaskKind::ThumbnailGenerate),
        TaskLane::Thumbnail
    );
    assert_eq!(
        task_lane(BackgroundTaskKind::ImagePreviewGenerate),
        TaskLane::Thumbnail
    );
    assert_eq!(
        task_lane(BackgroundTaskKind::MediaMetadataExtract),
        TaskLane::Thumbnail
    );
    assert_eq!(
        task_lane(BackgroundTaskKind::OfflineDownload),
        TaskLane::OfflineDownload
    );
    assert_eq!(
        task_lane(BackgroundTaskKind::TrashPurgeAll),
        TaskLane::Fallback
    );
    assert_eq!(
        task_lane(BackgroundTaskKind::SystemRuntime),
        TaskLane::Fallback
    );
    assert_eq!(
        task_lane(BackgroundTaskKind::StoragePolicyTempCleanup),
        TaskLane::Fallback
    );
    assert_eq!(
        task_lane(BackgroundTaskKind::StoragePolicyMigration),
        TaskLane::StorageMigration
    );
}

#[test]
fn available_lane_capacity_saturates_when_active_exceeds_limit() {
    assert_eq!(available_lane_capacity(3, 1), 2);
    assert_eq!(available_lane_capacity(3, 3), 0);
    assert_eq!(available_lane_capacity(3, 4), 0);
    assert_eq!(available_lane_capacity(3, u64::MAX), 0);
}

#[tokio::test]
async fn claim_candidates_for_lane_claims_batch_up_to_rechecked_capacity() {
    let db = build_dispatch_test_db().await;
    let tasks = [
        insert_dispatch_test_task(
            &db,
            BackgroundTaskKind::ArchiveCompress,
            BackgroundTaskStatus::Pending,
            -3,
            None,
        )
        .await,
        insert_dispatch_test_task(
            &db,
            BackgroundTaskKind::ArchiveExtract,
            BackgroundTaskStatus::Pending,
            -2,
            None,
        )
        .await,
        insert_dispatch_test_task(
            &db,
            BackgroundTaskKind::ArchiveCompress,
            BackgroundTaskStatus::Pending,
            -1,
            None,
        )
        .await,
    ];
    let candidates = tasks
        .iter()
        .enumerate()
        .map(|(index, task)| claim_candidate(index, task))
        .collect::<Vec<_>>();

    let claimed_at = Utc::now();
    let claimed = claim_candidates_for_lane(
        &db,
        test_lane_config(TaskLane::Archive, 2, true),
        &candidates,
        claimed_at - chrono::Duration::seconds(60),
        claimed_at,
        aster_forge_tasks::task_lease_expires_at(claimed_at, super::TASK_PROCESSING_STALE_SECS),
    )
    .await
    .expect("batch claim should succeed");

    assert_eq!(claimed.len(), 2);
    assert_eq!(claimed[0].task_id, tasks[0].id);
    assert_eq!(claimed[1].task_id, tasks[1].id);
    assert_eq!(claimed[0].processing_token, 1);
    assert_eq!(claimed[1].processing_token, 1);

    let stored = background_task::Entity::find()
        .all(&db)
        .await
        .expect("stored tasks should load");
    let processing = stored
        .iter()
        .filter(|task| task.status == BackgroundTaskStatus::Processing)
        .map(|task| task.id)
        .collect::<Vec<_>>();
    assert!(processing.contains(&tasks[0].id));
    assert!(processing.contains(&tasks[1].id));
    assert!(!processing.contains(&tasks[2].id));
}

#[tokio::test]
async fn claim_candidates_for_lane_skips_claim_when_rechecked_capacity_is_full() {
    let db = build_dispatch_test_db().await;
    let now = Utc::now();
    insert_dispatch_test_task(
        &db,
        BackgroundTaskKind::ThumbnailGenerate,
        BackgroundTaskStatus::Processing,
        -3,
        Some(now + chrono::Duration::seconds(60)),
    )
    .await;
    let pending = insert_dispatch_test_task(
        &db,
        BackgroundTaskKind::ThumbnailGenerate,
        BackgroundTaskStatus::Pending,
        -1,
        None,
    )
    .await;
    let candidates = vec![claim_candidate(0, &pending)];

    let claimed_at = Utc::now();
    let claimed = claim_candidates_for_lane(
        &db,
        test_lane_config(TaskLane::Thumbnail, 1, true),
        &candidates,
        claimed_at - chrono::Duration::seconds(60),
        claimed_at,
        aster_forge_tasks::task_lease_expires_at(claimed_at, super::TASK_PROCESSING_STALE_SECS),
    )
    .await
    .expect("full lane batch claim should succeed without claiming");

    assert!(claimed.is_empty());
    let stored = background_task_repo::find_by_id(&db, pending.id)
        .await
        .expect("pending task should still exist");
    assert_eq!(stored.status, BackgroundTaskStatus::Pending);
    assert_eq!(stored.processing_token, 0);
}

#[tokio::test]
async fn claim_candidates_for_lane_continues_after_stale_candidate_loses_cas() {
    let db = build_dispatch_test_db().await;
    let stale = insert_dispatch_test_task(
        &db,
        BackgroundTaskKind::ArchiveCompress,
        BackgroundTaskStatus::Pending,
        -2,
        None,
    )
    .await;
    let next = insert_dispatch_test_task(
        &db,
        BackgroundTaskKind::ArchiveCompress,
        BackgroundTaskStatus::Pending,
        -1,
        None,
    )
    .await;
    let candidates = vec![
        TaskClaimCandidate {
            index: 0,
            task_id: stale.id,
            expected_processing_token: stale.processing_token + 1,
            next_processing_token: stale.processing_token + 2,
        },
        claim_candidate(1, &next),
    ];

    let claimed_at = Utc::now();
    let claimed = claim_candidates_for_lane(
        &db,
        test_lane_config(TaskLane::Archive, 1, true),
        &candidates,
        claimed_at - chrono::Duration::seconds(60),
        claimed_at,
        aster_forge_tasks::task_lease_expires_at(claimed_at, super::TASK_PROCESSING_STALE_SECS),
    )
    .await
    .expect("batch claim should skip stale CAS misses");

    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].task_id, next.id);
    let stale = background_task_repo::find_by_id(&db, stale.id)
        .await
        .expect("stale candidate should still exist");
    assert_eq!(stale.status, BackgroundTaskStatus::Pending);
    assert_eq!(stale.processing_token, 0);
}

#[tokio::test]
async fn forge_task_context_preserves_drive_shutdown_error_code() {
    let lease = TaskLease::new(42, 7);
    let shutdown_token = CancellationToken::new();
    let context = aster_forge_tasks::TaskExecutionContext::new(
        lease,
        Duration::from_secs(60),
        shutdown_token.clone(),
    );

    shutdown_token.cancel();

    let error = context
        .ensure_active()
        .map_err(AsterError::from)
        .expect_err("cancelled shutdown token should stop the worker");
    assert!(is_task_worker_shutdown_requested(&error));
    assert_eq!(
        error.api_error_code_override(),
        Some(crate::api::api_error_code::ApiErrorCode::TaskWorkerShutdownRequested)
    );
}

#[test]
fn thumbnail_retry_only_keeps_transient_storage_errors() {
    let transient = AsterError::from(storage_driver_error(
        StorageErrorKind::Transient,
        "remote timeout",
    ));
    let misconfigured = AsterError::from(storage_driver_error(
        StorageErrorKind::Misconfigured,
        "missing bucket",
    ));

    assert!(
        super::super::registry::task_retry_class(BackgroundTaskKind::ThumbnailGenerate, &transient)
            .should_auto_retry()
    );
    assert!(
        !super::super::registry::task_retry_class(
            BackgroundTaskKind::ThumbnailGenerate,
            &misconfigured,
        )
        .can_manual_retry()
    );
    assert!(
        super::super::registry::task_retry_class(
            BackgroundTaskKind::ImagePreviewGenerate,
            &transient,
        )
        .should_auto_retry()
    );
    assert!(
        !super::super::registry::task_retry_class(
            BackgroundTaskKind::ImagePreviewGenerate,
            &misconfigured,
        )
        .can_manual_retry()
    );
    assert!(
        super::super::registry::task_retry_class(
            BackgroundTaskKind::MediaMetadataExtract,
            &transient,
        )
        .should_auto_retry()
    );
    assert!(
        !super::super::registry::task_retry_class(
            BackgroundTaskKind::MediaMetadataExtract,
            &misconfigured,
        )
        .can_manual_retry()
    );
}

#[test]
fn archive_validation_errors_are_not_retryable() {
    let error = AsterError::validation_error("archive entry compression ratio exceeds limit");
    let retry_class =
        super::super::registry::task_retry_class(BackgroundTaskKind::ArchiveExtract, &error);

    assert!(!retry_class.should_auto_retry());
    assert!(!retry_class.can_manual_retry());
}

#[test]
fn archive_transient_storage_errors_are_auto_retryable() {
    let error = AsterError::from(storage_driver_error(
        StorageErrorKind::Transient,
        "remote timeout",
    ));
    let retry_class =
        super::super::registry::task_retry_class(BackgroundTaskKind::ArchiveCompress, &error);

    assert!(retry_class.should_auto_retry());
    assert!(retry_class.can_manual_retry());
}
