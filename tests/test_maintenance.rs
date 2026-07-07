//! 集成测试：`maintenance`。

mod common;
use aster_drive::runtime::SharedRuntimeState;

use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, EntityTrait, PaginatorTrait, Set};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
struct DirModeGuard {
    path: std::path::PathBuf,
    mode: u32,
}

#[cfg(unix)]
impl DirModeGuard {
    fn set_read_only(path: impl Into<std::path::PathBuf>) -> Self {
        let path = path.into();
        let metadata = std::fs::metadata(&path).unwrap();
        let mode = metadata.permissions().mode();
        let mut perms = metadata.permissions();
        perms.set_mode(0o555);
        std::fs::set_permissions(&path, perms).unwrap();
        Self { path, mode }
    }
}

#[cfg(unix)]
impl Drop for DirModeGuard {
    fn drop(&mut self) {
        if let Ok(metadata) = std::fs::metadata(&self.path) {
            let mut perms = metadata.permissions();
            perms.set_mode(self.mode);
            let _ = std::fs::set_permissions(&self.path, perms);
        }
    }
}

async fn default_policy(
    state: &aster_drive::runtime::PrimaryAppState,
) -> aster_drive::entities::storage_policy::Model {
    aster_drive::db::repository::policy_repo::find_default(state.writer_db())
        .await
        .unwrap()
        .expect("default policy should exist in test setup")
}

struct UploadSessionSpec<'a> {
    team_id: Option<i64>,
    upload_id: &'a str,
    status: aster_drive::types::UploadSessionStatus,
    expires_at: chrono::DateTime<chrono::Utc>,
    object_temp_key: Option<&'a str>,
    object_multipart_id: Option<&'a str>,
    file_id: Option<i64>,
}

impl<'a> UploadSessionSpec<'a> {
    fn new(
        upload_id: &'a str,
        status: aster_drive::types::UploadSessionStatus,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        Self {
            team_id: None,
            upload_id,
            status,
            expires_at,
            object_temp_key: None,
            object_multipart_id: None,
            file_id: None,
        }
    }

    fn team(mut self, team_id: i64) -> Self {
        self.team_id = Some(team_id);
        self
    }

    fn object_upload(
        mut self,
        object_temp_key: Option<&'a str>,
        object_multipart_id: Option<&'a str>,
    ) -> Self {
        self.object_temp_key = object_temp_key;
        self.object_multipart_id = object_multipart_id;
        self
    }

    fn file_id(mut self, file_id: i64) -> Self {
        self.file_id = Some(file_id);
        self
    }
}

async fn create_upload_session(
    state: &aster_drive::runtime::PrimaryAppState,
    user_id: i64,
    spec: UploadSessionSpec<'_>,
) {
    use aster_drive::db::repository::upload_session_repo;

    let policy = default_policy(state).await;
    let now = chrono::Utc::now();
    upload_session_repo::create(
        state.writer_db(),
        aster_drive::entities::upload_session::ActiveModel {
            id: Set(spec.upload_id.to_string()),
            user_id: Set(user_id),
            team_id: Set(spec.team_id),
            frontend_client_id: Set(None),
            filename: Set("manual-upload.bin".to_string()),
            total_size: Set(10),
            chunk_size: Set(5),
            total_chunks: Set(2),
            received_count: Set(2),
            folder_id: Set(None),
            policy_id: Set(policy.id),
            status: Set(spec.status),
            object_temp_key: Set(spec.object_temp_key.map(str::to_string)),
            object_multipart_id: Set(spec.object_multipart_id.map(str::to_string)),
            file_id: Set(spec.file_id),
            created_at: Set(now),
            expires_at: Set(spec.expires_at),
            updated_at: Set(now),
        },
    )
    .await
    .unwrap();
}

async fn store_test_file(
    state: &aster_drive::runtime::PrimaryAppState,
    user_id: i64,
    filename: &str,
    bytes: &[u8],
) -> aster_drive::services::workspace::models::FileInfo {
    let temp_path = aster_drive::utils::paths::temp_file_path(
        &state.config.server.temp_dir,
        &uuid::Uuid::new_v4().to_string(),
    );
    tokio::fs::create_dir_all(&state.config.server.temp_dir)
        .await
        .unwrap();
    tokio::fs::write(&temp_path, bytes).await.unwrap();

    aster_drive::services::files::file::store_from_temp(
        state,
        user_id,
        aster_drive::services::files::file::StoreFromTempRequest::new(
            None,
            filename,
            &temp_path,
            bytes.len() as i64,
        ),
    )
    .await
    .unwrap()
}

fn thumb_path(blob_hash: &str) -> String {
    format!(
        "_thumb/images/1/{}/{}/{}.webp",
        &blob_hash[..2],
        &blob_hash[2..4],
        blob_hash
    )
}

fn current_thumb_path(blob_hash: &str) -> String {
    format!(
        "_thumb/images/1/{}/{}/{}.webp",
        &blob_hash[..2],
        &blob_hash[2..4],
        blob_hash
    )
}

async fn create_blob(
    state: &aster_drive::runtime::PrimaryAppState,
    hash: &str,
    storage_path: &str,
    bytes: &[u8],
    ref_count: i32,
) -> aster_drive::entities::file_blob::Model {
    use aster_drive::db::repository::file_repo;

    let policy = default_policy(state).await;
    let driver = state.driver_registry.get_driver(&policy).unwrap();
    driver.put(storage_path, bytes).await.unwrap();

    let now = Utc::now();
    file_repo::create_blob(
        state.writer_db(),
        aster_drive::entities::file_blob::ActiveModel {
            hash: Set(hash.to_string()),
            size: Set(bytes.len() as i64),
            policy_id: Set(policy.id),
            storage_path: Set(storage_path.to_string()),
            ref_count: Set(ref_count),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap()
}

async fn create_wopi_session(
    state: &aster_drive::runtime::PrimaryAppState,
    actor_user_id: i64,
    file_id: i64,
    token: &str,
    expires_at: chrono::DateTime<chrono::Utc>,
) {
    use aster_drive::db::repository::wopi_session_repo;

    wopi_session_repo::create(
        state.writer_db(),
        aster_drive::entities::wopi_session::ActiveModel {
            token_hash: Set(aster_drive::utils::hash::sha256_hex(token.as_bytes())),
            actor_user_id: Set(actor_user_id),
            session_version: Set(1),
            team_id: Set(None),
            file_id: Set(file_id),
            app_key: Set("onlyoffice".to_string()),
            expires_at: Set(expires_at),
            created_at: Set(Utc::now()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
}

#[actix_web::test]
async fn test_cleanup_expired_completed_upload_sessions_removes_broken_temp_object() {
    use aster_drive::db::repository::upload_session_repo;
    use aster_drive::services::{auth::local, maintenance_service};

    let state = common::setup().await;
    let user = local::register(&state, "maintuser1", "maint1@test.com", "password123")
        .await
        .unwrap();
    let policy = default_policy(&state).await;
    let driver = state.driver_registry.get_driver(&policy).unwrap();
    let temp_key = "tmp/broken-completed.bin";
    driver.put(temp_key, b"stale upload").await.unwrap();

    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            "broken-completed",
            aster_drive::types::UploadSessionStatus::Completed,
            Utc::now() - Duration::hours(1),
        )
        .object_upload(Some(temp_key), None),
    )
    .await;

    let stats = maintenance_service::cleanup_expired_completed_upload_sessions(&state)
        .await
        .unwrap();

    assert_eq!(stats.completed_sessions_deleted, 1);
    assert_eq!(stats.broken_completed_sessions_deleted, 1);
    assert!(
        upload_session_repo::find_by_id(state.writer_db(), "broken-completed")
            .await
            .is_err()
    );
    assert!(!driver.exists(temp_key).await.unwrap());
}

#[actix_web::test]
async fn test_cleanup_expired_completed_upload_sessions_removes_broken_completed_multipart_object()
{
    use aster_drive::db::repository::upload_session_repo;
    use aster_drive::services::{auth::local, maintenance_service};

    let state = common::setup().await;
    let user = local::register(&state, "maintuser1b", "maint1b@test.com", "password123")
        .await
        .unwrap();
    let policy = default_policy(&state).await;
    let driver = state.driver_registry.get_driver(&policy).unwrap();
    let temp_key = "tmp/broken-completed-multipart.bin";
    driver
        .put(temp_key, b"stale completed multipart")
        .await
        .unwrap();

    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            "broken-completed-multipart",
            aster_drive::types::UploadSessionStatus::Completed,
            Utc::now() - Duration::hours(1),
        )
        .object_upload(Some(temp_key), Some("already-completed-upload")),
    )
    .await;

    let stats = maintenance_service::cleanup_expired_completed_upload_sessions(&state)
        .await
        .unwrap();

    assert_eq!(stats.completed_sessions_deleted, 1);
    assert_eq!(stats.broken_completed_sessions_deleted, 1);
    assert!(
        upload_session_repo::find_by_id(state.writer_db(), "broken-completed-multipart")
            .await
            .is_err()
    );
    assert!(!driver.exists(temp_key).await.unwrap());
}

#[cfg(unix)]
#[actix_web::test]
async fn test_cleanup_expired_completed_upload_sessions_keeps_session_when_temp_delete_fails() {
    use aster_drive::db::repository::upload_session_repo;
    use aster_drive::services::{auth::local, maintenance_service};

    let state = common::setup().await;
    let user = local::register(&state, "maintfail1", "maintfail1@test.com", "password123")
        .await
        .unwrap();
    let policy = default_policy(&state).await;
    let driver = state.driver_registry.get_driver(&policy).unwrap();
    let temp_key = "tmp/delete-fails/broken-completed.bin";
    driver.put(temp_key, b"stale upload").await.unwrap();

    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            "broken-completed-delete-fails",
            aster_drive::types::UploadSessionStatus::Completed,
            Utc::now() - Duration::hours(1),
        )
        .object_upload(Some(temp_key), None),
    )
    .await;

    let object_parent = std::path::Path::new(&policy.base_path)
        .join(temp_key)
        .parent()
        .unwrap()
        .to_path_buf();
    let guard = DirModeGuard::set_read_only(object_parent);

    let stats = maintenance_service::cleanup_expired_completed_upload_sessions(&state)
        .await
        .unwrap();

    assert_eq!(stats.completed_sessions_deleted, 0);
    assert_eq!(stats.broken_completed_sessions_deleted, 0);
    assert!(
        upload_session_repo::find_by_id(state.writer_db(), "broken-completed-delete-fails")
            .await
            .is_ok(),
        "session row must remain so the stale remote object has a retry handle"
    );
    assert!(driver.exists(temp_key).await.unwrap());

    drop(guard);

    let stats = maintenance_service::cleanup_expired_completed_upload_sessions(&state)
        .await
        .unwrap();

    assert_eq!(stats.completed_sessions_deleted, 1);
    assert_eq!(stats.broken_completed_sessions_deleted, 1);
    assert!(
        upload_session_repo::find_by_id(state.writer_db(), "broken-completed-delete-fails")
            .await
            .is_err()
    );
    assert!(!driver.exists(temp_key).await.unwrap());
}

#[actix_web::test]
async fn test_cleanup_expired_completed_upload_sessions_keeps_live_blob() {
    use aster_drive::db::repository::{file_repo, upload_session_repo};
    use aster_drive::services::{auth::local, maintenance_service};

    let state = common::setup().await;
    let user = local::register(&state, "maintuser2", "maint2@test.com", "password123")
        .await
        .unwrap();
    let file = store_test_file(&state, user.id, "kept.txt", b"kept blob").await;
    let blob = file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
        .await
        .unwrap();
    let policy = default_policy(&state).await;
    let driver = state.driver_registry.get_driver(&policy).unwrap();

    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            "completed-with-file",
            aster_drive::types::UploadSessionStatus::Completed,
            Utc::now() - Duration::hours(1),
        )
        .object_upload(Some(&blob.storage_path), None)
        .file_id(file.id),
    )
    .await;

    let stats = maintenance_service::cleanup_expired_completed_upload_sessions(&state)
        .await
        .unwrap();

    assert_eq!(stats.completed_sessions_deleted, 1);
    assert_eq!(stats.broken_completed_sessions_deleted, 0);
    assert!(
        upload_session_repo::find_by_id(state.writer_db(), "completed-with-file")
            .await
            .is_err()
    );
    assert!(
        file_repo::find_by_id(state.writer_db(), file.id)
            .await
            .is_ok()
    );
    assert!(
        file_repo::find_blob_by_id(state.writer_db(), blob.id)
            .await
            .is_ok()
    );
    assert!(driver.exists(&blob.storage_path).await.unwrap());
}

#[actix_web::test]
async fn test_cleanup_expired_completed_upload_sessions_removes_stale_temp_for_completed_file() {
    use aster_drive::db::repository::{file_repo, upload_session_repo};
    use aster_drive::services::{auth::local, maintenance_service};

    let state = common::setup().await;
    let user = local::register(&state, "maintuser2b", "maint2b@test.com", "password123")
        .await
        .unwrap();
    let file = store_test_file(&state, user.id, "presigned-kept.txt", b"kept blob").await;
    let blob = file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
        .await
        .unwrap();
    let policy = default_policy(&state).await;
    let driver = state.driver_registry.get_driver(&policy).unwrap();
    let temp_key = "tmp/completed-file-stale-temp.bin";
    driver.put(temp_key, b"stale temp").await.unwrap();

    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            "completed-file-stale-temp",
            aster_drive::types::UploadSessionStatus::Completed,
            Utc::now() - Duration::hours(1),
        )
        .object_upload(Some(temp_key), None)
        .file_id(file.id),
    )
    .await;

    let stats = maintenance_service::cleanup_expired_completed_upload_sessions(&state)
        .await
        .unwrap();

    assert_eq!(stats.completed_sessions_deleted, 1);
    assert_eq!(stats.broken_completed_sessions_deleted, 0);
    assert!(
        upload_session_repo::find_by_id(state.writer_db(), "completed-file-stale-temp")
            .await
            .is_err()
    );
    assert!(
        file_repo::find_blob_by_id(state.writer_db(), blob.id)
            .await
            .is_ok()
    );
    assert!(driver.exists(&blob.storage_path).await.unwrap());
    assert!(!driver.exists(temp_key).await.unwrap());
}

#[actix_web::test]
async fn test_cleanup_expired_wopi_sessions_removes_only_expired_rows() {
    use aster_drive::db::repository::wopi_session_repo;
    use aster_drive::entities::wopi_session;
    use aster_drive::services::{auth::local, wopi_service};

    let state = common::setup().await;
    let user = local::register(&state, "wopimaint1", "wopimaint1@test.com", "password123")
        .await
        .unwrap();
    let file = store_test_file(&state, user.id, "wopi-cleanup.txt", b"cleanup").await;

    create_wopi_session(
        &state,
        user.id,
        file.id,
        "expired-wopi-token",
        Utc::now() - Duration::minutes(10),
    )
    .await;
    create_wopi_session(
        &state,
        user.id,
        file.id,
        "live-wopi-token",
        Utc::now() + Duration::minutes(10),
    )
    .await;

    let count = wopi_service::cleanup_expired(&state).await.unwrap();
    assert_eq!(count, 1);

    assert!(
        wopi_session_repo::find_by_token_hash(
            state.writer_db(),
            &aster_drive::utils::hash::sha256_hex(b"expired-wopi-token"),
        )
        .await
        .unwrap()
        .is_none()
    );
    assert!(
        wopi_session_repo::find_by_token_hash(
            state.writer_db(),
            &aster_drive::utils::hash::sha256_hex(b"live-wopi-token"),
        )
        .await
        .unwrap()
        .is_some()
    );

    assert_eq!(
        wopi_session::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        1
    );
}

#[actix_web::test]
async fn test_cleanup_expired_completed_upload_sessions_processes_all_batches() {
    use aster_drive::entities::upload_session::Entity as UploadSession;
    use aster_drive::services::{auth::local, maintenance_service};

    let state = common::setup().await;
    let user = local::register(&state, "maintbatch", "maintbatch@test.com", "password123")
        .await
        .unwrap();

    for i in 0..1001 {
        let upload_id = format!("batch-session-{i:04}");
        let file_id = if i % 2 == 0 { None } else { Some(i as i64 + 1) };
        create_upload_session(
            &state,
            user.id,
            file_id.map_or_else(
                || {
                    UploadSessionSpec::new(
                        &upload_id,
                        aster_drive::types::UploadSessionStatus::Completed,
                        Utc::now() - Duration::hours(1),
                    )
                },
                |file_id| {
                    UploadSessionSpec::new(
                        &upload_id,
                        aster_drive::types::UploadSessionStatus::Completed,
                        Utc::now() - Duration::hours(1),
                    )
                    .file_id(file_id)
                },
            ),
        )
        .await;
    }

    let stats = maintenance_service::cleanup_expired_completed_upload_sessions(&state)
        .await
        .unwrap();

    assert_eq!(stats.completed_sessions_deleted, 1001);
    assert_eq!(stats.broken_completed_sessions_deleted, 501);
    assert_eq!(
        UploadSession::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        0
    );
}

#[actix_web::test]
async fn test_cleanup_expired_completed_upload_sessions_cleans_team_sessions() {
    use aster_drive::db::repository::upload_session_repo;
    use aster_drive::services::{auth::local, maintenance_service, team_service};

    let state = common::setup().await;
    let user = local::register(&state, "maintteam1", "maintteam1@test.com", "password123")
        .await
        .unwrap();
    let team = team_service::create_team(
        &state,
        user.id,
        team_service::CreateTeamInput {
            name: "Maintenance Team".to_string(),
            description: None,
        },
    )
    .await
    .unwrap();
    let policy = default_policy(&state).await;
    let driver = state.driver_registry.get_driver(&policy).unwrap();
    let temp_key = "tmp/team-broken-completed.bin";
    driver.put(temp_key, b"stale upload").await.unwrap();

    create_upload_session(
        &state,
        user.id,
        UploadSessionSpec::new(
            "team-broken-completed",
            aster_drive::types::UploadSessionStatus::Completed,
            Utc::now() - Duration::hours(1),
        )
        .team(team.id)
        .object_upload(Some(temp_key), None),
    )
    .await;

    let stats = maintenance_service::cleanup_expired_completed_upload_sessions(&state)
        .await
        .unwrap();

    assert_eq!(stats.completed_sessions_deleted, 1);
    assert_eq!(stats.broken_completed_sessions_deleted, 1);
    assert!(
        upload_session_repo::find_by_id(state.writer_db(), "team-broken-completed")
            .await
            .is_err()
    );
    assert!(!driver.exists(temp_key).await.unwrap());
}

#[actix_web::test]
async fn test_reconcile_blob_state_deletes_orphans_and_fixes_ref_counts() {
    use aster_drive::db::repository::{file_repo, version_repo};
    use aster_drive::services::{auth::local, maintenance_service};

    let state = common::setup().await;
    let user = local::register(&state, "maintuser3", "maint3@test.com", "password123")
        .await
        .unwrap();
    let policy = default_policy(&state).await;
    let driver = state.driver_registry.get_driver(&policy).unwrap();

    let live_file = store_test_file(&state, user.id, "live.txt", b"live blob").await;
    let live_blob = file_repo::find_blob_by_id(state.writer_db(), live_file.blob_id)
        .await
        .unwrap();
    let mut live_blob_active: aster_drive::entities::file_blob::ActiveModel =
        live_blob.clone().into();
    live_blob_active.ref_count = Set(7);
    live_blob_active.updated_at = Set(Utc::now());
    live_blob_active.update(state.writer_db()).await.unwrap();

    let version_hash = "b".repeat(64);
    let version_blob = create_blob(
        &state,
        &version_hash,
        "versions/version-only.bin",
        b"version blob",
        9,
    )
    .await;
    version_repo::create(
        state.writer_db(),
        aster_drive::entities::file_version::ActiveModel {
            file_id: Set(live_file.id),
            blob_id: Set(version_blob.id),
            version: Set(1),
            size: Set(version_blob.size),
            created_at: Set(Utc::now()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let orphan_hash = "c".repeat(64);
    let orphan_path = "orphans/orphan.bin";
    let orphan_blob = create_blob(&state, &orphan_hash, orphan_path, b"orphan blob", 1).await;
    let orphan_thumb = thumb_path(&orphan_hash);
    driver.put(&orphan_thumb, b"thumb").await.unwrap();

    let stats = maintenance_service::reconcile_blob_state(&state)
        .await
        .unwrap();

    assert_eq!(stats.ref_count_fixed, 3);
    assert_eq!(stats.orphan_blobs_deleted, 1);

    let live_blob_after = file_repo::find_blob_by_id(state.writer_db(), live_blob.id)
        .await
        .unwrap();
    assert_eq!(live_blob_after.ref_count, 1);

    let version_blob_after = file_repo::find_blob_by_id(state.writer_db(), version_blob.id)
        .await
        .unwrap();
    assert_eq!(version_blob_after.ref_count, 1);

    assert!(
        file_repo::find_blob_by_id(state.writer_db(), orphan_blob.id)
            .await
            .is_err()
    );
    assert!(!driver.exists(orphan_path).await.unwrap());
    assert!(!driver.exists(&orphan_thumb).await.unwrap());
}

#[actix_web::test]
async fn test_reconcile_blob_state_processes_all_batches_without_skipping() {
    use aster_drive::entities::file_blob::Entity as FileBlob;
    use aster_drive::services::maintenance_service;

    let state = common::setup().await;
    let policy = default_policy(&state).await;
    let driver = state.driver_registry.get_driver(&policy).unwrap();

    for i in 0..1001u64 {
        let hash = format!("{i:064x}");
        let storage_path = format!("paging/blob-{i:04}.bin");
        create_blob(&state, &hash, &storage_path, b"x", 1).await;
    }

    let stats = maintenance_service::reconcile_blob_state(&state)
        .await
        .unwrap();

    assert_eq!(stats.ref_count_fixed, 1001);
    assert_eq!(stats.orphan_blobs_deleted, 1001);
    assert_eq!(FileBlob::find().count(state.writer_db()).await.unwrap(), 0);
    assert!(!driver.exists("paging/blob-0000.bin").await.unwrap());
    assert!(!driver.exists("paging/blob-1000.bin").await.unwrap());
}

#[actix_web::test]
async fn test_reconcile_blob_state_skips_fresh_cleanup_claim() {
    use aster_drive::db::repository::file_repo;
    use aster_drive::services::maintenance_service;

    let state = common::setup().await;
    let policy = default_policy(&state).await;
    let driver = state.driver_registry.get_driver(&policy).unwrap();
    let storage_path = "claims/fresh.bin";
    let blob = create_blob(
        &state,
        &"d".repeat(64),
        storage_path,
        b"fresh cleanup claim",
        file_repo::BLOB_CLEANUP_CLAIMED_REF_COUNT,
    )
    .await;

    let stats = maintenance_service::reconcile_blob_state(&state)
        .await
        .unwrap();

    assert_eq!(stats.ref_count_fixed, 0);
    assert_eq!(stats.orphan_blobs_deleted, 0);
    let reloaded_blob = file_repo::find_blob_by_id(state.writer_db(), blob.id)
        .await
        .unwrap();
    assert_eq!(
        reloaded_blob.ref_count,
        file_repo::BLOB_CLEANUP_CLAIMED_REF_COUNT
    );
    assert!(driver.exists(storage_path).await.unwrap());
}

#[actix_web::test]
async fn test_reconcile_blob_state_recovers_stale_cleanup_claim() {
    use aster_drive::db::repository::file_repo;
    use aster_drive::services::maintenance_service;

    let state = common::setup().await;
    let policy = default_policy(&state).await;
    let driver = state.driver_registry.get_driver(&policy).unwrap();
    let storage_path = "claims/stale.bin";
    let blob = create_blob(
        &state,
        &"e".repeat(64),
        storage_path,
        b"stale cleanup claim",
        file_repo::BLOB_CLEANUP_CLAIMED_REF_COUNT,
    )
    .await;
    let mut active: aster_drive::entities::file_blob::ActiveModel = blob.clone().into();
    active.updated_at = Set(Utc::now() - Duration::minutes(11));
    active.update(state.writer_db()).await.unwrap();

    let stats = maintenance_service::reconcile_blob_state(&state)
        .await
        .unwrap();

    assert_eq!(stats.ref_count_fixed, 1);
    assert_eq!(stats.orphan_blobs_deleted, 1);
    assert!(
        file_repo::find_blob_by_id(state.writer_db(), blob.id)
            .await
            .is_err()
    );
    assert!(!driver.exists(storage_path).await.unwrap());
}

#[cfg(unix)]
#[actix_web::test]
async fn test_purge_keeps_blob_row_when_storage_delete_fails_then_maintenance_retries() {
    use aster_drive::db::repository::file_repo;
    use aster_drive::services::{auth::local, files::file, maintenance_service};

    let state = common::setup().await;
    let user = local::register(&state, "maintuser4", "maint4@test.com", "password123")
        .await
        .unwrap();
    let policy = default_policy(&state).await;
    let driver = state.driver_registry.get_driver(&policy).unwrap();

    let file = store_test_file(&state, user.id, "retry.txt", b"retry blob").await;
    let blob = file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
        .await
        .unwrap();

    let object_parent = std::path::Path::new(&policy.base_path)
        .join(&blob.storage_path)
        .parent()
        .unwrap()
        .to_path_buf();
    let _guard = DirModeGuard::set_read_only(object_parent);

    file::purge(&state, file.id, user.id).await.unwrap();

    assert!(
        file_repo::find_by_id(state.writer_db(), file.id)
            .await
            .is_err()
    );
    let blob_after_purge = file_repo::find_blob_by_id(state.writer_db(), blob.id)
        .await
        .unwrap();
    assert_eq!(blob_after_purge.ref_count, 0);
    assert!(driver.exists(&blob.storage_path).await.unwrap());

    drop(_guard);

    let stats = maintenance_service::reconcile_blob_state(&state)
        .await
        .unwrap();

    assert_eq!(stats.orphan_blobs_deleted, 1);
    assert!(
        file_repo::find_blob_by_id(state.writer_db(), blob.id)
            .await
            .is_err()
    );
    assert!(!driver.exists(&blob.storage_path).await.unwrap());
}

#[actix_web::test]
async fn test_purge_releases_all_versioned_storage_used() {
    use aster_drive::db::repository::user_repo;
    use aster_drive::services::{auth::local, files::file};

    let state = common::setup().await;
    let user = local::register(&state, "maintquota1", "maintquota1@test.com", "password123")
        .await
        .unwrap();

    let initial_bytes = b"first-version";
    let updated_bytes = b"second-version-kept";
    let file = store_test_file(&state, user.id, "quota-purge.txt", initial_bytes).await;

    file::update_content(
        &state,
        file.id,
        user.id,
        actix_web::web::Bytes::from_static(updated_bytes),
        None,
    )
    .await
    .unwrap();

    let before_purge = user_repo::find_by_id(state.writer_db(), user.id)
        .await
        .unwrap();
    assert_eq!(
        before_purge.storage_used,
        initial_bytes.len() as i64 + updated_bytes.len() as i64
    );

    file::purge(&state, file.id, user.id).await.unwrap();

    let after_purge = user_repo::find_by_id(state.writer_db(), user.id)
        .await
        .unwrap();
    assert_eq!(after_purge.storage_used, 0);
}

#[actix_web::test]
async fn test_batch_purge_releases_all_versioned_storage_used() {
    use aster_drive::db::repository::{file_repo, user_repo};
    use aster_drive::services::{auth::local, files::file};

    let state = common::setup().await;
    let user = local::register(&state, "maintquota2", "maintquota2@test.com", "password123")
        .await
        .unwrap();

    let file_a_v1 = b"alpha-version-one";
    let file_a_v2 = b"alpha-version-two-long";
    let file_b_v1 = b"beta-version-one";
    let file_b_v2 = b"beta-version-two-even-longer";

    let file_a = store_test_file(&state, user.id, "quota-a.txt", file_a_v1).await;
    let file_b = store_test_file(&state, user.id, "quota-b.txt", file_b_v1).await;

    file::update_content(
        &state,
        file_a.id,
        user.id,
        actix_web::web::Bytes::from_static(file_a_v2),
        None,
    )
    .await
    .unwrap();
    file::update_content(
        &state,
        file_b.id,
        user.id,
        actix_web::web::Bytes::from_static(file_b_v2),
        None,
    )
    .await
    .unwrap();

    let before_purge = user_repo::find_by_id(state.writer_db(), user.id)
        .await
        .unwrap();
    assert_eq!(
        before_purge.storage_used,
        (file_a_v1.len() + file_a_v2.len() + file_b_v1.len() + file_b_v2.len()) as i64
    );

    let files = file_repo::find_by_ids(state.writer_db(), &[file_a.id, file_b.id])
        .await
        .unwrap();
    let purged = file::batch_purge(&state, files, user.id).await.unwrap();
    assert_eq!(purged, 2);

    let after_purge = user_repo::find_by_id(state.writer_db(), user.id)
        .await
        .unwrap();
    assert_eq!(after_purge.storage_used, 0);
}

#[actix_web::test]
async fn test_integrity_audit_detects_storage_and_tree_inconsistencies() {
    use aster_drive::db::repository::{file_repo, folder_repo, user_repo};
    use aster_drive::entities::{file_blob, folder};
    use aster_drive::services::{auth::local, integrity_service};

    let state = common::setup().await;
    let user = local::register(&state, "audituser1", "audit1@test.com", "password123")
        .await
        .unwrap();
    let policy = default_policy(&state).await;
    let driver = state.driver_registry.get_driver(&policy).unwrap();

    let live_file = store_test_file(&state, user.id, "audit.txt", b"audit blob").await;
    let live_blob = file_repo::find_blob_by_id(state.writer_db(), live_file.blob_id)
        .await
        .unwrap();

    let mut user_active: aster_drive::entities::user::ActiveModel =
        user_repo::find_by_id(state.writer_db(), user.id)
            .await
            .unwrap()
            .into();
    user_active.storage_used = Set(999);
    user_active.update(state.writer_db()).await.unwrap();

    let mut blob_active: file_blob::ActiveModel = live_blob.clone().into();
    blob_active.ref_count = Set(7);
    blob_active.updated_at = Set(Utc::now());
    blob_active.update(state.writer_db()).await.unwrap();

    driver.delete(&live_blob.storage_path).await.unwrap();
    driver.put("stray/untracked.bin", b"stray").await.unwrap();

    let orphan_thumb_hash = "d".repeat(64);
    driver
        .put(&current_thumb_path(&orphan_thumb_hash), b"thumb")
        .await
        .unwrap();

    let now = Utc::now();
    common::set_foreign_key_checks(state.writer_db(), false)
        .await
        .unwrap();
    let dangling_folder = folder_repo::create(
        state.writer_db(),
        folder::ActiveModel {
            name: Set("dangling".to_string()),
            parent_id: Set(Some(999_999)),
            team_id: Set(None),
            owner_user_id: Set(Some(user.id)),
            created_by_user_id: Set(Some(user.id)),
            created_by_username: Set(user.username.clone()),
            policy_id: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    common::set_foreign_key_checks(state.writer_db(), true)
        .await
        .unwrap();

    let cycle_a = folder_repo::create(
        state.writer_db(),
        folder::ActiveModel {
            name: Set("cycle-a".to_string()),
            parent_id: Set(None),
            team_id: Set(None),
            owner_user_id: Set(Some(user.id)),
            created_by_user_id: Set(Some(user.id)),
            created_by_username: Set(user.username.clone()),
            policy_id: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let cycle_b = folder_repo::create(
        state.writer_db(),
        folder::ActiveModel {
            name: Set("cycle-b".to_string()),
            parent_id: Set(Some(cycle_a.id)),
            team_id: Set(None),
            owner_user_id: Set(Some(user.id)),
            created_by_user_id: Set(Some(user.id)),
            created_by_username: Set(user.username.clone()),
            policy_id: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let mut cycle_a_active: folder::ActiveModel = cycle_a.clone().into();
    cycle_a_active.parent_id = Set(Some(cycle_b.id));
    cycle_a_active.updated_at = Set(Utc::now());
    cycle_a_active.update(state.writer_db()).await.unwrap();

    let usage_drifts = integrity_service::audit_storage_usage(state.writer_db())
        .await
        .unwrap();
    assert!(usage_drifts.iter().any(|drift| {
        drift.owner_kind == integrity_service::StorageOwnerKind::User
            && drift.owner_id == user.id
            && drift.recorded_bytes == 999
    }));

    let blob_drifts = integrity_service::audit_blob_ref_counts(state.writer_db(), None)
        .await
        .unwrap();
    assert!(blob_drifts.iter().any(|drift| {
        drift.blob_id == live_blob.id
            && drift.recorded_ref_count == 7
            && drift.actual_ref_count == 1
    }));

    let storage_report = integrity_service::audit_storage_objects(
        state.writer_db(),
        state.driver_registry.as_ref(),
        None,
        aster_drive::config::operations::thumbnail_max_dimension(state.runtime_config()),
        aster_drive::config::operations::image_preview_max_dimension(state.runtime_config()),
    )
    .await
    .unwrap();
    assert!(
        storage_report
            .missing_blob_objects
            .iter()
            .any(|issue| issue.blob_id == Some(live_blob.id))
    );
    assert!(
        storage_report
            .untracked_objects
            .iter()
            .any(|issue| issue.path == "stray/untracked.bin")
    );
    assert!(
        storage_report
            .orphan_thumbnails
            .iter()
            .any(|issue| issue.path == current_thumb_path(&orphan_thumb_hash))
    );

    let folder_issues = integrity_service::audit_folder_tree(state.writer_db())
        .await
        .unwrap();
    assert!(folder_issues.iter().any(|issue| {
        issue.kind == integrity_service::FolderTreeIssueKind::MissingParent
            && issue.folder_id == dangling_folder.id
    }));
    assert!(
        folder_issues
            .iter()
            .any(|issue| issue.kind == integrity_service::FolderTreeIssueKind::Cycle)
    );
}

#[actix_web::test]
async fn test_integrity_fix_repairs_storage_usage_and_blob_ref_counts() {
    use aster_drive::db::repository::{file_repo, user_repo};
    use aster_drive::entities::file_blob;
    use aster_drive::services::{auth::local, integrity_service};

    let state = common::setup().await;
    let user = local::register(&state, "audituser2", "audit2@test.com", "password123")
        .await
        .unwrap();
    let file = store_test_file(&state, user.id, "repair.txt", b"repair blob").await;
    let blob = file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
        .await
        .unwrap();

    let mut user_active: aster_drive::entities::user::ActiveModel =
        user_repo::find_by_id(state.writer_db(), user.id)
            .await
            .unwrap()
            .into();
    user_active.storage_used = Set(0);
    user_active.update(state.writer_db()).await.unwrap();

    let mut blob_active: file_blob::ActiveModel = blob.clone().into();
    blob_active.ref_count = Set(0);
    blob_active.updated_at = Set(Utc::now());
    blob_active.update(state.writer_db()).await.unwrap();

    let usage_drifts = integrity_service::audit_storage_usage(state.writer_db())
        .await
        .unwrap();
    assert_eq!(usage_drifts.len(), 1);
    integrity_service::fix_storage_usage_drifts(state.writer_db(), &usage_drifts)
        .await
        .unwrap();
    assert!(
        integrity_service::audit_storage_usage(state.writer_db())
            .await
            .unwrap()
            .is_empty()
    );

    let blob_drifts = integrity_service::audit_blob_ref_counts(state.writer_db(), None)
        .await
        .unwrap();
    assert_eq!(blob_drifts.len(), 1);
    integrity_service::fix_blob_ref_count_drifts(state.writer_db(), &blob_drifts)
        .await
        .unwrap();
    assert!(
        integrity_service::audit_blob_ref_counts(state.writer_db(), None)
            .await
            .unwrap()
            .is_empty()
    );
}
