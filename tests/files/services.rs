//! 集成测试：`services`。

use crate::common;

use aster_drive::services::files::file::StoreFromTempRequest;
use std::collections::BTreeSet;
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

fn assert_share_target_check_violation(db: &sea_orm::DatabaseConnection, err: &sea_orm::DbErr) {
    let message = err.to_string();
    let lower_message = message.to_ascii_lowercase();
    let matches_named_constraint = message.contains("chk_shares_exactly_one_target");
    let matches_generic_check_failure = lower_message.contains("check constraint failed");

    match db.get_database_backend() {
        sea_orm::DbBackend::Sqlite => assert!(
            matches_named_constraint || matches_generic_check_failure,
            "unexpected sqlite share target violation: {message}"
        ),
        _ => assert!(
            matches_named_constraint,
            "expected named share target constraint violation, got: {message}"
        ),
    }
}

fn assert_share_token_length_violation(db: &sea_orm::DatabaseConnection, err: &sea_orm::DbErr) {
    let message = err.to_string();
    let lower_message = message.to_ascii_lowercase();
    let matches_named_constraint = message.contains("chk_shares_token_length_32");
    let matches_generic_check_failure = lower_message.contains("check constraint failed");
    let matches_postgres_varchar_error =
        lower_message.contains("value too long for type character varying(32)");
    let matches_mysql_length_error =
        lower_message.contains("data too long for column") && lower_message.contains("token");

    match db.get_database_backend() {
        sea_orm::DbBackend::Sqlite => assert!(
            matches_named_constraint || matches_generic_check_failure,
            "unexpected sqlite share token length violation: {message}"
        ),
        sea_orm::DbBackend::Postgres => assert!(
            matches_named_constraint || matches_postgres_varchar_error,
            "unexpected postgres share token length violation: {message}"
        ),
        sea_orm::DbBackend::MySql => assert!(
            matches_named_constraint || matches_mysql_length_error,
            "unexpected mysql share token length violation: {message}"
        ),
        _ => panic!("unsupported database backend for share token length assertion: {message}"),
    }
}

fn write_service_fixture(name: &str, contents: &str) -> String {
    let dir = format!("/tmp/asterdrive-services-test-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&dir).unwrap();
    let path = format!("{dir}/{name}");
    std::fs::write(&path, contents).unwrap();
    path
}

async fn store_service_file(
    state: &aster_drive::runtime::PrimaryAppState,
    user_id: i64,
    folder_id: Option<i64>,
    name: &str,
    contents: &str,
) -> i64 {
    let path = write_service_fixture(name, contents);
    aster_drive::services::files::file::store_from_temp(
        state,
        user_id,
        StoreFromTempRequest::new(folder_id, name, &path, contents.len() as i64),
    )
    .await
    .unwrap()
    .id
}

async fn user_storage_used(state: &aster_drive::runtime::PrimaryAppState, user_id: i64) -> i64 {
    aster_drive::db::repository::user_repo::find_by_id(state.writer_db(), user_id)
        .await
        .unwrap()
        .storage_used
}

async fn wait_for_share_download_count(
    state: &aster_drive::runtime::PrimaryAppState,
    share_id: i64,
    expected: i64,
) {
    for _ in 0..40 {
        let reloaded =
            aster_drive::db::repository::share_repo::find_by_id(state.writer_db(), share_id)
                .await
                .unwrap();
        if reloaded.download_count == expected {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    let reloaded = aster_drive::db::repository::share_repo::find_by_id(state.writer_db(), share_id)
        .await
        .unwrap();
    panic!(
        "share #{share_id} download_count did not reach {expected}, got {}",
        reloaded.download_count
    );
}

// ─── Auth Service ─────────────────────────────────────────────────

#[actix_web::test]
async fn test_auth_local_register_login() {
    let state = common::setup().await;

    // 注册
    let user = common::create_test_account(&state, "alice", "alice@example.com", "password123")
        .await
        .unwrap();
    assert_eq!(user.username, "alice");

    // 第一个用户是 admin
    assert!(user.role.is_admin());

    // 登录 → LoginResult { access_token, refresh_token, user_id }
    let result =
        aster_drive::services::auth::local::login(&state, "alice", "password123", None, None)
            .await
            .unwrap();
    let result = common::expect_authenticated_login(result);
    assert!(!result.access_token.is_empty());
    assert!(!result.refresh_token.is_empty());
    assert_eq!(result.user_id, user.id);

    // 错误密码
    let err =
        aster_drive::services::auth::local::login(&state, "alice", "wrongpass", None, None).await;
    assert!(err.is_err());

    // 重复注册
    let err =
        common::create_test_account(&state, "alice", "alice2@example.com", "password123").await;
    assert!(err.is_err());
}

#[actix_web::test]
async fn test_auth_local_rejects_password_shorter_than_eight_chars() {
    let state = common::setup().await;

    let err = common::create_test_account(&state, "alice", "alice@example.com", "pass123")
        .await
        .unwrap_err();

    assert_eq!(err.message(), "password must be at least 8 characters");
}

#[actix_web::test]
async fn test_auth_local_change_password() {
    let state = common::setup().await;

    let user = common::create_test_account(&state, "alice", "alice@example.com", "password123")
        .await
        .unwrap();

    aster_drive::services::auth::local::change_password(
        &state,
        user.id,
        "password123",
        "newpass456",
    )
    .await
    .unwrap();

    let old_login =
        aster_drive::services::auth::local::login(&state, "alice", "password123", None, None).await;
    assert!(old_login.is_err());

    let new_login =
        aster_drive::services::auth::local::login(&state, "alice", "newpass456", None, None)
            .await
            .unwrap();
    let new_login = common::expect_authenticated_login(new_login);
    assert_eq!(new_login.user_id, user.id);
}

#[actix_web::test]
async fn test_auth_local_set_password() {
    let state = common::setup().await;

    let user = common::create_test_account(&state, "alice", "alice@example.com", "password123")
        .await
        .unwrap();

    aster_drive::services::auth::local::set_password(&state, user.id, "resetpass789")
        .await
        .unwrap();

    let old_login =
        aster_drive::services::auth::local::login(&state, "alice", "password123", None, None).await;
    assert!(old_login.is_err());

    let new_login =
        aster_drive::services::auth::local::login(&state, "alice", "resetpass789", None, None)
            .await
            .unwrap();
    let new_login = common::expect_authenticated_login(new_login);
    assert_eq!(new_login.user_id, user.id);
}

#[actix_web::test]
async fn test_auth_local_verify_token() {
    let state = common::setup().await;

    common::create_test_account(&state, "bobb", "bob@example.com", "pass1234")
        .await
        .unwrap();

    let login_result =
        aster_drive::services::auth::local::login(&state, "bobb", "pass1234", None, None)
            .await
            .unwrap();
    let login_result = common::expect_authenticated_login(login_result);

    // 验证 access token
    let claims = aster_drive::services::auth::local::verify_token(
        &login_result.access_token,
        &state.config.auth.jwt_secret,
    )
    .unwrap();
    assert_eq!(claims.sub, claims.user_id.to_string());

    // 假 token
    let err = aster_drive::services::auth::local::verify_token(
        "fake.token.here",
        &state.config.auth.jwt_secret,
    );
    assert!(err.is_err());
}

// ─── File Service ─────────────────────────────────────────────────

#[actix_web::test]
async fn test_file_service_get_info() {
    let state = common::setup().await;

    let user = common::create_test_account(&state, "user1", "u1@example.com", "pass1234")
        .await
        .unwrap();

    // 上传临时文件
    let temp_dir = format!("/tmp/asterdrive-svc-test-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&temp_dir).unwrap();
    let temp_path = format!("{}/test.txt", temp_dir);
    std::fs::write(&temp_path, "hello service test").unwrap();

    let file = aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "service_test.txt", &temp_path, 18),
    )
    .await
    .unwrap();

    // get_info
    let info = aster_drive::services::files::file::get_info(&state, file.id, user.id)
        .await
        .unwrap();
    assert_eq!(info.name, "service_test.txt");
    assert_eq!(info.owner_user_id, Some(user.id));
    assert_eq!(info.created_by_user_id, Some(user.id));
    assert_eq!(info.created_by_username, user.username);

    // 别人的文件
    let user2 = common::create_test_account(&state, "user2", "u2@example.com", "pass1234")
        .await
        .unwrap();
    let err = aster_drive::services::files::file::get_info(&state, file.id, user2.id).await;
    assert!(err.is_err());
}

#[actix_web::test]
async fn test_file_active_model_partial_name_update_refreshes_classification() {
    use aster_drive_model::entities::file;
    use aster_forge_file_classification::FileCategory;
    use sea_orm::{ActiveModelTrait, EntityTrait, Set};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "filepartialname",
        "filepartialname@example.com",
        "password123",
    )
    .await
    .unwrap();
    let file_id = store_service_file(&state, user.id, None, "notes.txt", "hello").await;

    file::ActiveModel {
        id: Set(file_id),
        name: Set("backup.tar.gz".to_string()),
        ..Default::default()
    }
    .update(state.writer_db())
    .await
    .unwrap();

    let updated = file::Entity::find_by_id(file_id)
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.extension, "gz");
    assert_eq!(updated.compound_extension.as_deref(), Some("tar.gz"));
    assert_eq!(updated.file_category, FileCategory::Archive);
}

#[actix_web::test]
async fn test_file_active_model_partial_mime_update_refreshes_classification() {
    use aster_drive_model::entities::file;
    use aster_forge_file_classification::FileCategory;
    use sea_orm::{ActiveModelTrait, EntityTrait, Set};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "filepartialmime",
        "filepartialmime@example.com",
        "password123",
    )
    .await
    .unwrap();
    let file_id = store_service_file(&state, user.id, None, "extensionless", "hello").await;

    file::ActiveModel {
        id: Set(file_id),
        mime_type: Set("image/png".to_string()),
        ..Default::default()
    }
    .update(state.writer_db())
    .await
    .unwrap();

    let updated = file::Entity::find_by_id(file_id)
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.extension, "");
    assert_eq!(updated.compound_extension, None);
    assert_eq!(updated.file_category, FileCategory::Image);
}

#[actix_web::test]
async fn test_collect_folder_tree_respects_deleted_visibility() {
    use aster_drive::services::{files::folder, webdav::tree};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "treewalker",
        "treewalker@example.com",
        "password123",
    )
    .await
    .unwrap();

    let root = folder::create(&state, user.id, "root", None).await.unwrap();
    let active_child = folder::create(&state, user.id, "active", Some(root.id))
        .await
        .unwrap();
    let deleted_child = folder::create(&state, user.id, "deleted", Some(root.id))
        .await
        .unwrap();
    let deleted_grandchild =
        folder::create(&state, user.id, "deleted-leaf", Some(deleted_child.id))
            .await
            .unwrap();

    let root_file = store_service_file(&state, user.id, Some(root.id), "root.txt", "root").await;
    let active_file = store_service_file(
        &state,
        user.id,
        Some(active_child.id),
        "active.txt",
        "active",
    )
    .await;
    let deleted_file = store_service_file(
        &state,
        user.id,
        Some(deleted_child.id),
        "deleted.txt",
        "deleted",
    )
    .await;
    let deleted_grandchild_file = store_service_file(
        &state,
        user.id,
        Some(deleted_grandchild.id),
        "deleted-leaf.txt",
        "deleted leaf",
    )
    .await;

    folder::delete(&state, deleted_child.id, user.id)
        .await
        .unwrap();

    let (visible_files, visible_folder_ids) =
        tree::collect_folder_tree(&state, user.id, root.id, false)
            .await
            .unwrap();
    let visible_file_ids = visible_files
        .into_iter()
        .map(|file| file.id)
        .collect::<BTreeSet<_>>();
    let visible_folder_ids = visible_folder_ids.into_iter().collect::<BTreeSet<_>>();

    assert_eq!(
        visible_folder_ids,
        [root.id, active_child.id].into_iter().collect()
    );
    assert_eq!(
        visible_file_ids,
        [root_file, active_file].into_iter().collect()
    );

    let (all_files, all_folder_ids) = tree::collect_folder_tree(&state, user.id, root.id, true)
        .await
        .unwrap();
    let all_file_ids = all_files
        .into_iter()
        .map(|file| file.id)
        .collect::<BTreeSet<_>>();
    let all_folder_ids = all_folder_ids.into_iter().collect::<BTreeSet<_>>();

    assert_eq!(
        all_folder_ids,
        [
            root.id,
            active_child.id,
            deleted_child.id,
            deleted_grandchild.id
        ]
        .into_iter()
        .collect()
    );
    assert_eq!(
        all_file_ids,
        [
            root_file,
            active_file,
            deleted_file,
            deleted_grandchild_file
        ]
        .into_iter()
        .collect()
    );
}

#[actix_web::test]
async fn test_collect_folder_tree_handles_empty_leaf_folder() {
    use aster_drive::services::{files::folder, webdav::tree};

    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "treeleaf", "treeleaf@example.com", "password123")
            .await
            .unwrap();

    let leaf = folder::create(&state, user.id, "leaf", None).await.unwrap();

    let (visible_files, visible_folder_ids) =
        tree::collect_folder_tree(&state, user.id, leaf.id, false)
            .await
            .unwrap();
    assert!(visible_files.is_empty());
    assert_eq!(visible_folder_ids, vec![leaf.id]);

    let (all_files, all_folder_ids) = tree::collect_folder_tree(&state, user.id, leaf.id, true)
        .await
        .unwrap();
    assert!(all_files.is_empty());
    assert_eq!(all_folder_ids, vec![leaf.id]);
}

#[actix_web::test]
async fn test_list_trash_keeps_original_paths_for_files_and_folders() {
    use aster_drive::services::{files::file, files::folder, files::trash};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "trashpaths",
        "trashpaths@example.com",
        "password123",
    )
    .await
    .unwrap();

    let projects = folder::create(&state, user.id, "Projects", None)
        .await
        .unwrap();
    let reports = folder::create(&state, user.id, "Reports", Some(projects.id))
        .await
        .unwrap();
    let archive = folder::create(&state, user.id, "Archive", Some(projects.id))
        .await
        .unwrap();

    let file_id =
        store_service_file(&state, user.id, Some(reports.id), "report.txt", "report").await;

    file::delete(&state, file_id, user.id).await.unwrap();
    folder::delete(&state, archive.id, user.id).await.unwrap();

    let trash = trash::list_trash(&state, user.id, 10, 0, 10, None)
        .await
        .unwrap();

    assert_eq!(trash.folders_total, 1);
    assert_eq!(trash.files_total, 1);
    assert_eq!(trash.folders.len(), 1);
    assert_eq!(trash.files.len(), 1);
    assert_eq!(trash.folders[0].id, archive.id);
    assert_eq!(trash.folders[0].original_path, "/Projects");
    assert_eq!(trash.files[0].id, file_id);
    assert_eq!(trash.files[0].original_path, "/Projects/Reports");
}

#[actix_web::test]
async fn test_list_trash_handles_root_and_shared_parent_paths() {
    use aster_drive::services::{files::file, files::folder, files::trash};

    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "trashmix", "trashmix@example.com", "password123")
            .await
            .unwrap();

    let shared = folder::create(&state, user.id, "Shared", None)
        .await
        .unwrap();
    let docs = folder::create(&state, user.id, "Docs", Some(shared.id))
        .await
        .unwrap();
    let nested_folder_a = folder::create(&state, user.id, "Archive-A", Some(shared.id))
        .await
        .unwrap();
    let nested_folder_b = folder::create(&state, user.id, "Archive-B", Some(shared.id))
        .await
        .unwrap();
    let root_folder = folder::create(&state, user.id, "RootTrash", None)
        .await
        .unwrap();

    let nested_file_a =
        store_service_file(&state, user.id, Some(docs.id), "nested-a.txt", "nested a").await;
    let nested_file_b =
        store_service_file(&state, user.id, Some(docs.id), "nested-b.txt", "nested b").await;
    let root_file = store_service_file(&state, user.id, None, "root.txt", "root").await;

    for file_id in [nested_file_a, nested_file_b, root_file] {
        file::delete(&state, file_id, user.id).await.unwrap();
    }
    for folder_id in [nested_folder_a.id, nested_folder_b.id, root_folder.id] {
        folder::delete(&state, folder_id, user.id).await.unwrap();
    }

    let trash = trash::list_trash(&state, user.id, 10, 0, 10, None)
        .await
        .unwrap();

    assert_eq!(trash.folders_total, 3);
    assert_eq!(trash.files_total, 3);

    let nested_folder_paths = trash
        .folders
        .iter()
        .filter(|item| item.id == nested_folder_a.id || item.id == nested_folder_b.id)
        .map(|item| item.original_path.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(nested_folder_paths, BTreeSet::from(["/Shared"]));

    let root_folder_item = trash
        .folders
        .iter()
        .find(|item| item.id == root_folder.id)
        .unwrap();
    assert_eq!(root_folder_item.original_path, "/");

    let nested_file_paths = trash
        .files
        .iter()
        .filter(|item| item.id == nested_file_a || item.id == nested_file_b)
        .map(|item| item.original_path.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(nested_file_paths, BTreeSet::from(["/Shared/Docs"]));

    let root_file_item = trash
        .files
        .iter()
        .find(|item| item.id == root_file)
        .unwrap();
    assert_eq!(root_file_item.original_path, "/");
}

#[actix_web::test]
async fn test_list_trash_zero_limits_keep_totals_and_empty_items() {
    use aster_drive::services::{files::file, files::folder, files::trash};

    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "trashzero", "trashzero@example.com", "password123")
            .await
            .unwrap();

    let root_folder = folder::create(&state, user.id, "ZeroFolder", None)
        .await
        .unwrap();
    let root_file = store_service_file(&state, user.id, None, "zero.txt", "zero").await;

    folder::delete(&state, root_folder.id, user.id)
        .await
        .unwrap();
    file::delete(&state, root_file, user.id).await.unwrap();

    let trash = trash::list_trash(&state, user.id, 0, 0, 0, None)
        .await
        .unwrap();

    assert_eq!(trash.folders_total, 1);
    assert_eq!(trash.files_total, 1);
    assert!(trash.folders.is_empty());
    assert!(trash.files.is_empty());
    assert!(trash.next_file_cursor.is_none());
}

// ─── File Lock ─────────────────────────────────────────────────

#[actix_web::test]
async fn test_file_lock_lock_unlock() {
    let state = common::setup().await;

    let user = common::create_test_account(&state, "locker", "locker@example.com", "pass1234")
        .await
        .unwrap();

    // 创建文件夹来锁
    let folder = aster_drive::services::files::folder::create(&state, user.id, "LockTest", None)
        .await
        .unwrap();
    assert!(matches!(
        folder.lock_state,
        aster_drive::services::files::lock::ResourceLockState::Unlocked
    ));

    // 锁定
    let lock = aster_drive::services::files::lock::lock(
        &state,
        aster_drive_model::types::EntityType::Folder,
        folder.id,
        Some(user.id),
        None,
        None,
    )
    .await
    .unwrap();
    assert!(!lock.token.is_empty());

    assert!(
        aster_drive::db::repository::lock_repo::find_by_token(state.writer_db(), &lock.token)
            .await
            .unwrap()
            .is_some()
    );

    // 重复锁定应失败
    let err = aster_drive::services::files::lock::lock(
        &state,
        aster_drive_model::types::EntityType::Folder,
        folder.id,
        Some(user.id),
        None,
        None,
    )
    .await;
    assert!(err.is_err());

    // 删除应失败（存在权威锁记录）
    let err = aster_drive::services::files::folder::delete(&state, folder.id, user.id).await;
    assert!(err.is_err());

    // 解锁
    aster_drive::services::files::lock::unlock(
        &state,
        aster_drive_model::types::EntityType::Folder,
        folder.id,
        user.id,
    )
    .await
    .unwrap();

    assert!(
        aster_drive::db::repository::lock_repo::find_by_token(state.writer_db(), &lock.token)
            .await
            .unwrap()
            .is_none()
    );

    // 删除成功
    aster_drive::services::files::folder::delete(&state, folder.id, user.id)
        .await
        .unwrap();
}

#[actix_web::test]
async fn test_file_lock_force_unlock() {
    let state = common::setup().await;

    let user = common::create_test_account(&state, "admin1", "admin1@example.com", "pass1234")
        .await
        .unwrap();

    let folder = aster_drive::services::files::folder::create(&state, user.id, "ForceTest", None)
        .await
        .unwrap();

    let lock = aster_drive::services::files::lock::lock(
        &state,
        aster_drive_model::types::EntityType::Folder,
        folder.id,
        Some(user.id),
        None,
        None,
    )
    .await
    .unwrap();

    // 强制解锁（admin 操作）
    aster_drive::services::files::lock::force_unlock(&state, lock.id)
        .await
        .unwrap();

    assert!(
        aster_drive::db::repository::lock_repo::find_by_token(state.writer_db(), &lock.token)
            .await
            .unwrap()
            .is_none()
    );
}

#[actix_web::test]
async fn test_file_lock_unlock_by_token_clears_file_lock_state() {
    use aster_drive::db::repository::lock_repo;
    use aster_drive::services::{files::file, files::lock};

    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "tokunlock", "tokunlock@example.com", "pass1234")
            .await
            .unwrap();

    let temp_dir = format!("/tmp/asterdrive-lock-token-test-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&temp_dir).unwrap();
    let temp_path = format!("{temp_dir}/locked.txt");
    std::fs::write(&temp_path, "lock by token").unwrap();

    let file = file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "locked.txt", &temp_path, "lock by token".len() as i64),
    )
    .await
    .unwrap();

    let lock = lock::lock(
        &state,
        aster_drive_model::types::EntityType::File,
        file.id,
        Some(user.id),
        None,
        None,
    )
    .await
    .unwrap();

    lock::unlock_by_token(&state, &lock.token).await.unwrap();
    assert!(
        lock_repo::find_by_token(state.writer_db(), &lock.token)
            .await
            .unwrap()
            .is_none()
    );
}

#[actix_web::test]
async fn test_file_lock_cleanup_expired_unlocks_only_expired_resources() {
    use aster_drive::db::repository::lock_repo;
    use aster_drive::services::{files::folder, files::lock};
    use chrono::Duration;

    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "lockcleanup", "lockcleanup@example.com", "pass1234")
            .await
            .unwrap();

    let expired_folder = folder::create(&state, user.id, "ExpiredLock", None)
        .await
        .unwrap();
    let active_folder = folder::create(&state, user.id, "ActiveLock", None)
        .await
        .unwrap();

    let active_lock = lock::lock(
        &state,
        aster_drive_model::types::EntityType::Folder,
        active_folder.id,
        Some(user.id),
        None,
        Some(Duration::minutes(10)),
    )
    .await
    .unwrap();
    let expired_lock = lock::lock(
        &state,
        aster_drive_model::types::EntityType::Folder,
        expired_folder.id,
        Some(user.id),
        None,
        Some(Duration::seconds(-1)),
    )
    .await
    .unwrap();

    assert!(
        lock_repo::find_by_token(state.writer_db(), &expired_lock.token)
            .await
            .unwrap()
            .is_some(),
        "cleanup fixture should retain an expired lock before cleanup starts"
    );

    let cleaned = lock::cleanup_expired(&state).await.unwrap();
    assert_eq!(cleaned, 1);

    assert!(
        lock_repo::find_by_token(state.writer_db(), &expired_lock.token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        lock_repo::find_by_token(state.writer_db(), &active_lock.token)
            .await
            .unwrap()
            .is_some()
    );
}

// ─── Version Service ──────────────────────────────────────────────

#[actix_web::test]
async fn test_version_service_list_delete() {
    let state = common::setup().await;

    let user = common::create_test_account(&state, "veruser", "ver@example.com", "pass1234")
        .await
        .unwrap();

    // 上传 v1
    let temp_dir = format!("/tmp/asterdrive-ver-test-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&temp_dir).unwrap();
    let temp1 = format!("{}/v1.txt", temp_dir);
    std::fs::write(&temp1, "version 1").unwrap();

    let file = aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "versioned.txt", &temp1, 9),
    )
    .await
    .unwrap();

    // Every file starts with an immutable initial revision.
    let versions = aster_drive::services::content::version::list_versions(
        &state,
        file.id,
        user.id,
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(versions.len(), 1);
    assert!(versions[0].current);
    assert_eq!(versions[0].version, 1);

    // 覆盖 → v2（产生 v1 版本记录）
    let temp2 = format!("{}/v2.txt", temp_dir);
    std::fs::write(&temp2, "version 2 content").unwrap();

    let _ = aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "versioned.txt", &temp2, 17).overwrite(file.id),
    )
    .await
    .unwrap();

    let versions = aster_drive::services::content::version::list_versions(
        &state,
        file.id,
        user.id,
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].version, 2);
    assert!(versions[0].current);
    assert_eq!(versions[1].version, 1);

    // 删除版本
    aster_drive::services::content::version::delete_version(
        &state,
        file.id,
        versions[1].id,
        user.id,
    )
    .await
    .unwrap();

    let versions = aster_drive::services::content::version::list_versions(
        &state,
        file.id,
        user.id,
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].version, 2);
    assert!(versions[0].current);
}

#[actix_web::test]
async fn test_delete_version_preserves_sequence_gaps() {
    let state = common::setup().await;

    let user = common::create_test_account(&state, "densever", "densever@example.com", "pass1234")
        .await
        .unwrap();

    let temp1 = write_service_fixture("dense-v1.txt", "1111");
    let file = aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "dense-versioned.txt", &temp1, 4),
    )
    .await
    .unwrap();

    let temp2 = write_service_fixture("dense-v2.txt", "2222");
    aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "dense-versioned.txt", &temp2, 4).overwrite(file.id),
    )
    .await
    .unwrap();

    let temp3 = write_service_fixture("dense-v3.txt", "3333");
    aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "dense-versioned.txt", &temp3, 4).overwrite(file.id),
    )
    .await
    .unwrap();

    let versions = aster_drive::services::content::version::list_versions(
        &state,
        file.id,
        user.id,
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        versions
            .iter()
            .map(|version| version.version)
            .collect::<Vec<_>>(),
        vec![3, 2, 1]
    );

    aster_drive::services::content::version::delete_version(
        &state,
        file.id,
        versions[2].id,
        user.id,
    )
    .await
    .unwrap();

    let versions = aster_drive::services::content::version::list_versions(
        &state,
        file.id,
        user.id,
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].version, 3);
    assert_eq!(versions[1].version, 2);

    let temp4 = write_service_fixture("dense-v4.txt", "4444");
    aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "dense-versioned.txt", &temp4, 4).overwrite(file.id),
    )
    .await
    .unwrap();

    let versions = aster_drive::services::content::version::list_versions(
        &state,
        file.id,
        user.id,
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        versions
            .iter()
            .map(|version| version.version)
            .collect::<Vec<_>>(),
        vec![4, 3, 2]
    );
}

#[actix_web::test]
async fn test_version_storage_used_tracks_overwrite_delete_and_restore() {
    let state = common::setup().await;

    let user = common::create_test_account(
        &state,
        "versionquota",
        "versionquota@example.com",
        "pass1234",
    )
    .await
    .unwrap();

    let temp1 = write_service_fixture("quota-v1.txt", "1111");
    let file = aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "quota-versioned.txt", &temp1, 4),
    )
    .await
    .unwrap();
    assert_eq!(user_storage_used(&state, user.id).await, 4);

    let temp2 = write_service_fixture("quota-v2.txt", "222222");
    aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "quota-versioned.txt", &temp2, 6).overwrite(file.id),
    )
    .await
    .unwrap();
    assert_eq!(user_storage_used(&state, user.id).await, 10);

    let temp3 = write_service_fixture("quota-v3.txt", "33333333");
    aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "quota-versioned.txt", &temp3, 8).overwrite(file.id),
    )
    .await
    .unwrap();
    assert_eq!(user_storage_used(&state, user.id).await, 18);

    let versions = aster_drive::services::content::version::list_versions(
        &state,
        file.id,
        user.id,
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        versions
            .iter()
            .map(|version| version.size)
            .collect::<Vec<_>>(),
        vec![8, 6, 4]
    );

    aster_drive::services::content::version::delete_version(
        &state,
        file.id,
        versions[2].id,
        user.id,
    )
    .await
    .unwrap();
    assert_eq!(user_storage_used(&state, user.id).await, 14);

    let versions = aster_drive::services::content::version::list_versions(
        &state,
        file.id,
        user.id,
        Default::default(),
    )
    .await
    .unwrap();
    let restored = aster_drive::services::content::version::restore_version(
        &state,
        file.id,
        versions[1].id,
        user.id,
    )
    .await
    .unwrap();
    assert_eq!(restored.size, 6);
    assert_eq!(user_storage_used(&state, user.id).await, 20);

    let versions = aster_drive::services::content::version::list_versions(
        &state,
        file.id,
        user.id,
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(versions.len(), 3);
    assert_eq!(versions[0].version, 4);
    assert!(versions[0].current);
}

#[actix_web::test]
async fn test_version_cleanup_excess_reclaims_storage_used() {
    let state = common::setup().await;

    let user = common::create_test_account(
        &state,
        "versionlimit",
        "versionlimit@example.com",
        "pass1234",
    )
    .await
    .unwrap();

    let mut max_versions = aster_drive::db::repository::config_repo::find_by_key(
        state.writer_db(),
        "max_versions_per_file",
    )
    .await
    .unwrap()
    .unwrap();
    max_versions.value = "1".to_string();
    state.runtime_config.apply(max_versions);

    let temp1 = write_service_fixture("limit-v1.txt", "1111");
    let file = aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "limit-versioned.txt", &temp1, 4),
    )
    .await
    .unwrap();
    assert_eq!(user_storage_used(&state, user.id).await, 4);

    let temp2 = write_service_fixture("limit-v2.txt", "222222");
    aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "limit-versioned.txt", &temp2, 6).overwrite(file.id),
    )
    .await
    .unwrap();
    assert_eq!(user_storage_used(&state, user.id).await, 10);

    let temp3 = write_service_fixture("limit-v3.txt", "33333333");
    aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "limit-versioned.txt", &temp3, 8).overwrite(file.id),
    )
    .await
    .unwrap();

    let versions = aster_drive::services::content::version::list_versions(
        &state,
        file.id,
        user.id,
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].version, 3);
    assert!(versions[0].current);
    assert_eq!(versions[1].version, 2);
    assert_eq!(versions[1].size, 6);
    assert_eq!(user_storage_used(&state, user.id).await, 14);

    let restored = aster_drive::services::content::version::restore_version(
        &state,
        file.id,
        versions[1].id,
        user.id,
    )
    .await
    .unwrap();
    assert_eq!(restored.size, 6);

    let versions = aster_drive::services::content::version::list_versions(
        &state,
        file.id,
        user.id,
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(versions.len(), 2, "restore must enforce the history limit");
    assert_eq!(versions[0].version, 4);
    assert!(versions[0].current);
    assert_eq!(versions[0].size, 6);
    assert_eq!(versions[1].version, 3);
    assert_eq!(versions[1].size, 8);
    assert_eq!(user_storage_used(&state, user.id).await, 14);
}

#[actix_web::test]
async fn test_version_restore_honors_zero_history_limit() {
    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "versionlimit0",
        "versionlimit0@example.com",
        "pass1234",
    )
    .await
    .unwrap();

    let temp1 = write_service_fixture("limit-zero-v1.txt", "one");
    let file = aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "limit-zero.txt", &temp1, 3),
    )
    .await
    .unwrap();
    let initial_revision =
        aster_drive::db::repository::revision_repo::find_by_file_id(state.writer_db(), file.id)
            .await
            .unwrap()
            .into_iter()
            .find(|revision| revision.sequence == 1)
            .unwrap();

    let temp2 = write_service_fixture("limit-zero-v2.txt", "second");
    aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "limit-zero.txt", &temp2, 6).overwrite(file.id),
    )
    .await
    .unwrap();

    let mut max_versions = aster_drive::db::repository::config_repo::find_by_key(
        state.writer_db(),
        "max_versions_per_file",
    )
    .await
    .unwrap()
    .unwrap();
    max_versions.value = "0".to_string();
    state.runtime_config.apply(max_versions);

    aster_drive::services::content::version::restore_version(
        &state,
        file.id,
        initial_revision.id,
        user.id,
    )
    .await
    .unwrap();

    let versions = aster_drive::services::content::version::list_versions(
        &state,
        file.id,
        user.id,
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(versions.len(), 1);
    assert!(versions[0].current);
    assert_eq!(versions[0].version, 3);
    assert_eq!(versions[0].size, 3);
    assert_eq!(user_storage_used(&state, user.id).await, 3);
}

#[actix_web::test]
async fn test_version_restore_appends_head_and_preserves_history_blobs() {
    let state = common::setup().await;

    let user =
        common::create_test_account(&state, "restoreuser", "restore@example.com", "pass1234")
            .await
            .unwrap();

    let temp_dir = format!("/tmp/asterdrive-restore-test-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&temp_dir).unwrap();

    let temp1 = format!("{}/v1.txt", temp_dir);
    std::fs::write(&temp1, "version 1").unwrap();
    let file = aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "restore.txt", &temp1, 9),
    )
    .await
    .unwrap();

    let temp2 = format!("{}/v2.txt", temp_dir);
    std::fs::write(&temp2, "version 2").unwrap();
    aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "restore.txt", &temp2, 9).overwrite(file.id),
    )
    .await
    .unwrap();

    let temp3 = format!("{}/v3.txt", temp_dir);
    std::fs::write(&temp3, "version 3").unwrap();
    aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "restore.txt", &temp3, 9).overwrite(file.id),
    )
    .await
    .unwrap();

    let temp4 = format!("{}/v4.txt", temp_dir);
    std::fs::write(&temp4, "version 4").unwrap();
    let latest = aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "restore.txt", &temp4, 9).overwrite(file.id),
    )
    .await
    .unwrap();

    let versions = aster_drive::services::content::version::list_versions(
        &state,
        file.id,
        user.id,
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        versions.iter().map(|v| v.version).collect::<Vec<_>>(),
        vec![4, 3, 2, 1]
    );

    let v3 = versions.iter().find(|v| v.version == 3).unwrap().clone();
    let v2 = versions.iter().find(|v| v.version == 2).unwrap().clone();
    let v1 = versions.iter().find(|v| v.version == 1).unwrap().clone();
    let old_current_blob_id = latest.blob_id;

    let restored =
        aster_drive::services::content::version::restore_version(&state, file.id, v2.id, user.id)
            .await
            .unwrap();

    assert_eq!(Some(restored.blob_id), v2.blob_id);

    let versions = aster_drive::services::content::version::list_versions(
        &state,
        file.id,
        user.id,
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(versions.len(), 5);
    assert_eq!(versions[0].version, 5);
    assert!(versions[0].current);
    assert_eq!(versions[0].blob_id, v2.blob_id);
    assert_eq!(versions[4].blob_id, v1.blob_id);

    assert!(
        aster_drive::db::repository::file_repo::find_blob_by_id(
            state.writer_db(),
            v1.blob_id.unwrap()
        )
        .await
        .is_ok()
    );
    assert!(
        aster_drive::db::repository::file_repo::find_blob_by_id(
            state.writer_db(),
            v2.blob_id.unwrap()
        )
        .await
        .is_ok()
    );
    assert!(
        aster_drive::db::repository::file_repo::find_blob_by_id(
            state.writer_db(),
            v3.blob_id.unwrap()
        )
        .await
        .is_ok()
    );
    assert!(
        aster_drive::db::repository::file_repo::find_blob_by_id(
            state.writer_db(),
            old_current_blob_id
        )
        .await
        .is_ok()
    );

    let temp5 = format!("{}/v5.txt", temp_dir);
    std::fs::write(&temp5, "version 5").unwrap();
    aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "restore.txt", &temp5, 9).overwrite(file.id),
    )
    .await
    .unwrap();

    let versions = aster_drive::services::content::version::list_versions(
        &state,
        file.id,
        user.id,
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        versions.iter().map(|v| v.version).collect::<Vec<_>>(),
        vec![6, 5, 4, 3, 2, 1]
    );
}

#[actix_web::test]
async fn test_version_restore_failure_keeps_thumbnail_and_rolls_back_revision() {
    use sea_orm::{ActiveModelTrait, ConnectionTrait, IntoActiveModel, Set};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "restorefailure",
        "restorefailure@example.com",
        "pass1234",
    )
    .await
    .unwrap();
    let initial = write_service_fixture("restore-failure-v1.txt", "version 1");
    let file = aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "restore-failure.txt", &initial, 9),
    )
    .await
    .unwrap();
    let overwrite = write_service_fixture("restore-failure-v2.txt", "version 2");
    let current = aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "restore-failure.txt", &overwrite, 9).overwrite(file.id),
    )
    .await
    .unwrap();
    let target = aster_drive::services::content::version::list_versions(
        &state,
        file.id,
        user.id,
        Default::default(),
    )
    .await
    .unwrap()
    .into_iter()
    .find(|version| version.version == 1)
    .unwrap();
    let revision_count =
        aster_drive::db::repository::revision_repo::count_by_file_id(state.writer_db(), file.id)
            .await
            .unwrap();

    let blob =
        aster_drive::db::repository::file_repo::find_blob_by_id(state.writer_db(), current.blob_id)
            .await
            .unwrap();
    let thumbnail_path = format!("thumbnails/restore-failure-{}.webp", blob.id);
    let policy = state
        .policy_snapshot
        .get_policy_or_err(blob.policy_id)
        .unwrap();
    let driver = state.driver_registry.get_driver(&policy).unwrap();
    driver.put(&thumbnail_path, b"thumbnail").await.unwrap();
    let mut active_blob = blob.into_active_model();
    active_blob.thumbnail_path = Set(Some(thumbnail_path.clone()));
    active_blob.thumbnail_processor = Set(Some("images".to_string()));
    active_blob.thumbnail_version = Set(Some("test".to_string()));
    active_blob.update(state.writer_db()).await.unwrap();

    let db = state.writer_db();
    match db.get_database_backend() {
        sea_orm::DbBackend::Sqlite => {
            db.execute_unprepared(&format!(
                "CREATE TRIGGER fail_restore_file_update BEFORE UPDATE ON files \
                 WHEN OLD.id = {} BEGIN SELECT RAISE(ABORT, 'forced restore failure'); END",
                file.id
            ))
            .await
            .unwrap();
        }
        sea_orm::DbBackend::Postgres => {
            db.execute_unprepared(&format!(
                "CREATE FUNCTION fail_restore_file_update_fn() RETURNS trigger AS $$ BEGIN \
                 IF OLD.id = {} THEN RAISE EXCEPTION 'forced restore failure'; END IF; \
                 RETURN NEW; END; $$ LANGUAGE plpgsql",
                file.id
            ))
            .await
            .unwrap();
            db.execute_unprepared(
                "CREATE TRIGGER fail_restore_file_update BEFORE UPDATE ON files \
                 FOR EACH ROW EXECUTE FUNCTION fail_restore_file_update_fn()",
            )
            .await
            .unwrap();
        }
        sea_orm::DbBackend::MySql => {
            db.execute_unprepared(&format!(
                "CREATE TRIGGER fail_restore_file_update BEFORE UPDATE ON files FOR EACH ROW \
                 BEGIN IF OLD.id = {} THEN SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'forced restore failure'; END IF; END",
                file.id
            ))
            .await
            .unwrap();
        }
        backend => panic!("unsupported test database backend: {backend:?}"),
    }

    aster_drive::services::content::version::restore_version(&state, file.id, target.id, user.id)
        .await
        .expect_err("the injected file update failure must abort restore");

    assert!(driver.exists(&thumbnail_path).await.unwrap());
    assert_eq!(
        aster_drive::db::repository::revision_repo::count_by_file_id(state.writer_db(), file.id)
            .await
            .unwrap(),
        revision_count
    );
}

#[actix_web::test]
async fn test_version_restore_same_blob_keeps_current_thumbnail() {
    use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "restoresameblob",
        "restoresameblob@example.com",
        "pass1234",
    )
    .await
    .unwrap();
    let policy = aster_drive::db::repository::policy_repo::find_default(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    let mut active_policy: aster_drive_model::entities::storage_policy::ActiveModel =
        policy.clone().into();
    active_policy.storage_config = Set(common::with_local_content_dedup(&policy, true));
    active_policy.update(state.writer_db()).await.unwrap();
    state
        .driver_registry
        .reload_policy_snapshot(&state.policy_snapshot, state.writer_db())
        .await
        .unwrap();

    let initial = write_service_fixture("restore-same-blob-v1.txt", "same bytes");
    let file = aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "restore-same-blob.txt", &initial, 10),
    )
    .await
    .unwrap();
    let overwrite = write_service_fixture("restore-same-blob-v2.txt", "same bytes");
    let current = aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "restore-same-blob.txt", &overwrite, 10).overwrite(file.id),
    )
    .await
    .unwrap();
    let target = aster_drive::services::content::version::list_versions(
        &state,
        file.id,
        user.id,
        Default::default(),
    )
    .await
    .unwrap()
    .into_iter()
    .find(|version| version.version == 1)
    .unwrap();
    assert_eq!(target.blob_id, Some(current.blob_id));

    let blob =
        aster_drive::db::repository::file_repo::find_blob_by_id(state.writer_db(), current.blob_id)
            .await
            .unwrap();
    let thumbnail_path = format!("thumbnails/restore-same-blob-{}.webp", blob.id);
    let policy = state
        .policy_snapshot
        .get_policy_or_err(blob.policy_id)
        .unwrap();
    let driver = state.driver_registry.get_driver(&policy).unwrap();
    driver.put(&thumbnail_path, b"thumbnail").await.unwrap();
    let mut active_blob = blob.into_active_model();
    active_blob.thumbnail_path = Set(Some(thumbnail_path.clone()));
    active_blob.thumbnail_processor = Set(Some("images".to_string()));
    active_blob.thumbnail_version = Set(Some("test".to_string()));
    active_blob.update(state.writer_db()).await.unwrap();

    let restored = aster_drive::services::content::version::restore_version(
        &state, file.id, target.id, user.id,
    )
    .await
    .unwrap();

    assert_eq!(restored.blob_id, current.blob_id);
    assert!(driver.exists(&thumbnail_path).await.unwrap());
}

#[actix_web::test]
async fn test_identical_overwrite_still_appends_revision() {
    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "samebytes", "samebytes@example.com", "pass1234")
            .await
            .unwrap();
    let initial = write_service_fixture("same-initial.txt", "identical bytes");
    let file = aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "same.txt", &initial, 15),
    )
    .await
    .unwrap();
    let overwrite = write_service_fixture("same-overwrite.txt", "identical bytes");
    aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "same.txt", &overwrite, 15).overwrite(file.id),
    )
    .await
    .unwrap();

    let revisions =
        aster_drive::db::repository::revision_repo::find_by_file_id(state.writer_db(), file.id)
            .await
            .unwrap();
    assert_eq!(revisions.len(), 2);
    let initial = revisions
        .iter()
        .find(|revision| revision.sequence == 1)
        .unwrap();
    let overwrite = revisions
        .iter()
        .find(|revision| revision.sequence == 2)
        .unwrap();
    assert_eq!(overwrite.predecessor_revision_id, Some(initial.id));
    assert_ne!(overwrite.public_id, initial.public_id);
    assert_ne!(overwrite.etag, initial.etag);
    assert_eq!(overwrite.logical_size, initial.logical_size);
}

#[actix_web::test]
async fn test_content_overwrite_revision_uses_actual_actor_username() {
    use actix_web::web::Bytes;
    use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};

    let state = common::setup().await;
    let creator = common::create_test_account(
        &state,
        "revisioncreator",
        "revisioncreator@example.com",
        "pass1234",
    )
    .await
    .unwrap();
    let actor = common::create_test_account(
        &state,
        "revisionactor",
        "revisionactor@example.com",
        "pass1234",
    )
    .await
    .unwrap();
    let initial = write_service_fixture("revision-actor.txt", "before");
    let file = aster_drive::services::files::file::store_from_temp(
        &state,
        actor.id,
        StoreFromTempRequest::new(None, "revision-actor.txt", &initial, 6),
    )
    .await
    .unwrap();
    let mut file_record =
        aster_drive::db::repository::file_repo::find_by_id(state.writer_db(), file.id)
            .await
            .unwrap()
            .into_active_model();
    file_record.created_by_user_id = Set(Some(creator.id));
    file_record.created_by_username = Set(creator.username.clone());
    file_record.update(state.writer_db()).await.unwrap();

    aster_drive::services::files::file::update_content(
        &state,
        file.id,
        actor.id,
        Bytes::from_static(b"after"),
        None,
    )
    .await
    .unwrap();

    let revisions =
        aster_drive::db::repository::revision_repo::find_by_file_id(state.writer_db(), file.id)
            .await
            .unwrap();
    let current = revisions
        .iter()
        .find(|revision| revision.sequence == 2)
        .unwrap();
    assert_eq!(current.creator_user_id, Some(actor.id));
    assert_eq!(
        current.creator_display_name.as_deref(),
        Some(actor.username.as_str())
    );
}

#[actix_web::test]
async fn test_revision_append_rejects_stale_expected_head_without_side_effects() {
    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "stalehead", "stalehead@example.com", "pass1234")
            .await
            .unwrap();
    let temp = write_service_fixture("stale.txt", "head");
    let file = aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "stale.txt", &temp, 4),
    )
    .await
    .unwrap();
    let history = aster_drive::db::repository::revision_repo::find_history_by_file_id(
        state.writer_db(),
        file.id,
    )
    .await
    .unwrap();
    let count_before =
        aster_drive::db::repository::revision_repo::count_by_file_id(state.writer_db(), file.id)
            .await
            .unwrap();

    let error = aster_drive::db::repository::revision_repo::append(
        state.writer_db(),
        file.id,
        Some(history.current_revision_id.unwrap() + 999),
        aster_drive::db::repository::revision_repo::NewRevision {
            blob_id: file.blob_id,
            logical_size: file.size,
            mime_type: &file.mime_type,
            content_sha256: None,
            creator_user_id: Some(user.id),
            creator_display_name: &user.username,
            comment: None,
            reason: aster_drive::db::repository::revision_repo::RevisionReason::Overwrite,
            created_at: chrono::Utc::now(),
            etag: None,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        aster_drive::db::repository::revision_repo::RevisionAppendError::HeadChanged
    ));

    let after = aster_drive::db::repository::revision_repo::find_history_by_file_id(
        state.writer_db(),
        file.id,
    )
    .await
    .unwrap();
    assert_eq!(after.current_revision_id, history.current_revision_id);
    assert_eq!(after.next_sequence, history.next_sequence);
    assert_eq!(
        aster_drive::db::repository::revision_repo::count_by_file_id(state.writer_db(), file.id)
            .await
            .unwrap(),
        count_before
    );
}

#[actix_web::test]
async fn test_revision_cursor_pagination_handles_limits_and_sequence_gaps() {
    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "revisionpage",
        "revisionpage@example.com",
        "pass1234",
    )
    .await
    .unwrap();
    let first = write_service_fixture("page-1.txt", "one");
    let file = aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "page.txt", &first, 3),
    )
    .await
    .unwrap();
    for (name, contents) in [
        ("page-2.txt", "two"),
        ("page-3.txt", "three"),
        ("page-4.txt", "four"),
    ] {
        let path = write_service_fixture(name, contents);
        aster_drive::services::files::file::store_from_temp(
            &state,
            user.id,
            StoreFromTempRequest::new(None, "page.txt", &path, contents.len() as i64)
                .overwrite(file.id),
        )
        .await
        .unwrap();
    }
    let all =
        aster_drive::db::repository::revision_repo::find_by_file_id(state.writer_db(), file.id)
            .await
            .unwrap();
    aster_drive::services::content::version::delete_version(
        &state,
        file.id,
        all.iter()
            .find(|revision| revision.sequence == 2)
            .unwrap()
            .id,
        user.id,
    )
    .await
    .unwrap();

    let first_page = aster_drive::services::content::version::list_versions(
        &state,
        file.id,
        user.id,
        aster_drive::services::workspace::models::FileVersionListQuery {
            limit: Some(2),
            after_sequence: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        first_page
            .iter()
            .map(|item| item.version)
            .collect::<Vec<_>>(),
        vec![4, 3]
    );
    let second_page = aster_drive::services::content::version::list_versions(
        &state,
        file.id,
        user.id,
        aster_drive::services::workspace::models::FileVersionListQuery {
            limit: Some(2),
            after_sequence: Some(3),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        second_page
            .iter()
            .map(|item| item.version)
            .collect::<Vec<_>>(),
        vec![1]
    );
    let clamped = aster_drive::services::content::version::list_versions(
        &state,
        file.id,
        user.id,
        aster_drive::services::workspace::models::FileVersionListQuery {
            limit: Some(0),
            after_sequence: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        clamped.iter().map(|item| item.version).collect::<Vec<_>>(),
        vec![4],
        "limit zero must clamp to one instead of disabling the page bound"
    );
}

#[actix_web::test]
async fn test_current_revision_delete_is_rejected_without_side_effects() {
    use sea_orm::EntityTrait;

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "deletecurrent",
        "deletecurrent@example.com",
        "pass1234",
    )
    .await
    .unwrap();
    let temp = write_service_fixture("current.txt", "current");
    let file = aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "current.txt", &temp, 7),
    )
    .await
    .unwrap();
    let revision =
        aster_drive::db::repository::revision_repo::find_by_file_id(state.writer_db(), file.id)
            .await
            .unwrap()
            .remove(0);
    let error = aster_drive::services::content::version::delete_version(
        &state,
        file.id,
        revision.id,
        user.id,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code(), "E005");
    let after = aster_drive_model::entities::file_revision::Entity::find_by_id(revision.id)
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.blob_id, revision.blob_id);
    assert!(after.retired_at.is_none());
    assert!(after.purged_at.is_none());
}

#[actix_web::test]
async fn test_historical_delete_keeps_tombstone_and_repairs_predecessor() {
    use sea_orm::EntityTrait;

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "deletetombstone",
        "deletetombstone@example.com",
        "pass1234",
    )
    .await
    .unwrap();
    let first = write_service_fixture("tombstone-1.txt", "one");
    let file = aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "tombstone.txt", &first, 3),
    )
    .await
    .unwrap();
    for (name, contents) in [("tombstone-2.txt", "two"), ("tombstone-3.txt", "three")] {
        let path = write_service_fixture(name, contents);
        aster_drive::services::files::file::store_from_temp(
            &state,
            user.id,
            StoreFromTempRequest::new(None, "tombstone.txt", &path, contents.len() as i64)
                .overwrite(file.id),
        )
        .await
        .unwrap();
    }
    let revisions =
        aster_drive::db::repository::revision_repo::find_by_file_id(state.writer_db(), file.id)
            .await
            .unwrap();
    let first = revisions
        .iter()
        .find(|revision| revision.sequence == 1)
        .unwrap();
    let middle = revisions
        .iter()
        .find(|revision| revision.sequence == 2)
        .unwrap()
        .clone();
    let head = revisions
        .iter()
        .find(|revision| revision.sequence == 3)
        .unwrap();
    assert_eq!(head.predecessor_revision_id, Some(middle.id));

    aster_drive::services::content::version::delete_version(&state, file.id, middle.id, user.id)
        .await
        .unwrap();
    let tombstone = aster_drive_model::entities::file_revision::Entity::find_by_id(middle.id)
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(tombstone.public_id, middle.public_id);
    assert_eq!(tombstone.sequence, 2);
    assert!(tombstone.blob_id.is_none());
    assert!(tombstone.retired_at.is_some());
    assert!(tombstone.purged_at.is_some());
    let repaired_head = aster_drive_model::entities::file_revision::Entity::find_by_id(head.id)
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(repaired_head.predecessor_revision_id, Some(first.id));
}

#[actix_web::test]
async fn test_revision_restore_restores_user_properties_but_preserves_protected_properties() {
    use aster_drive::db::repository::{property_repo, revision_repo};
    use aster_drive_model::types::EntityType;

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "revisionprops",
        "revisionprops@example.com",
        "pass1234",
    )
    .await
    .unwrap();
    let initial = write_service_fixture("property-1.txt", "one");
    let file = aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "properties.txt", &initial, 3),
    )
    .await
    .unwrap();

    property_repo::upsert(
        state.writer_db(),
        EntityType::File,
        file.id,
        "urn:test",
        "color",
        Some("blue"),
    )
    .await
    .unwrap();
    property_repo::upsert(
        state.writer_db(),
        EntityType::File,
        file.id,
        "DAV:",
        "displayname",
        Some("dav-old"),
    )
    .await
    .unwrap();
    property_repo::upsert(
        state.writer_db(),
        EntityType::File,
        file.id,
        "system.preview",
        "cache",
        Some("system-old"),
    )
    .await
    .unwrap();
    property_repo::upsert(
        state.writer_db(),
        EntityType::File,
        file.id,
        "system.",
        "root",
        Some("system-root-old"),
    )
    .await
    .unwrap();
    property_repo::upsert(
        state.writer_db(),
        EntityType::File,
        file.id,
        "systemx.preview",
        "cache",
        Some("boundary-old"),
    )
    .await
    .unwrap();
    property_repo::upsert(
        state.writer_db(),
        EntityType::File,
        file.id,
        "System.preview",
        "cache",
        Some("case-sensitive-old"),
    )
    .await
    .unwrap();

    let second = write_service_fixture("property-2.txt", "two");
    aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "properties.txt", &second, 3).overwrite(file.id),
    )
    .await
    .unwrap();
    let second_revision = revision_repo::find_by_file_id(state.writer_db(), file.id)
        .await
        .unwrap()
        .into_iter()
        .find(|revision| revision.sequence == 2)
        .unwrap();
    let second_properties = revision_repo::find_properties(state.writer_db(), second_revision.id)
        .await
        .unwrap();
    assert_eq!(second_properties.len(), 3);
    let second_value = |namespace: &str, name: &str| {
        second_properties
            .iter()
            .find(|property| property.namespace == namespace && property.name == name)
            .and_then(|property| property.xml_value.as_deref())
    };
    assert_eq!(second_value("urn:test", "color"), Some("blue"));
    assert_eq!(
        second_value("systemx.preview", "cache"),
        Some("boundary-old")
    );
    assert_eq!(
        second_value("System.preview", "cache"),
        Some("case-sensitive-old")
    );

    property_repo::upsert(
        state.writer_db(),
        EntityType::File,
        file.id,
        "urn:test",
        "color",
        Some("red"),
    )
    .await
    .unwrap();
    property_repo::upsert(
        state.writer_db(),
        EntityType::File,
        file.id,
        "DAV:",
        "displayname",
        Some("dav-current"),
    )
    .await
    .unwrap();
    property_repo::upsert(
        state.writer_db(),
        EntityType::File,
        file.id,
        "system.preview",
        "cache",
        Some("system-current"),
    )
    .await
    .unwrap();
    property_repo::upsert(
        state.writer_db(),
        EntityType::File,
        file.id,
        "system.",
        "root",
        Some("system-root-current"),
    )
    .await
    .unwrap();
    property_repo::upsert(
        state.writer_db(),
        EntityType::File,
        file.id,
        "systemx.preview",
        "cache",
        Some("boundary-current"),
    )
    .await
    .unwrap();
    property_repo::upsert(
        state.writer_db(),
        EntityType::File,
        file.id,
        "System.preview",
        "cache",
        Some("case-sensitive-current"),
    )
    .await
    .unwrap();
    let third = write_service_fixture("property-3.txt", "three");
    aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "properties.txt", &third, 5).overwrite(file.id),
    )
    .await
    .unwrap();

    aster_drive::services::content::version::restore_version(
        &state,
        file.id,
        second_revision.id,
        user.id,
    )
    .await
    .unwrap();
    let properties = property_repo::find_by_entity(state.writer_db(), EntityType::File, file.id)
        .await
        .unwrap();
    let value = |namespace: &str, name: &str| {
        properties
            .iter()
            .find(|property| property.namespace == namespace && property.name == name)
            .and_then(|property| property.value.as_deref())
    };
    assert_eq!(value("urn:test", "color"), Some("blue"));
    assert_eq!(value("DAV:", "displayname"), Some("dav-current"));
    assert_eq!(value("system.preview", "cache"), Some("system-current"));
    assert_eq!(value("system.", "root"), Some("system-root-current"));
    assert_eq!(value("systemx.preview", "cache"), Some("boundary-old"));
    assert_eq!(value("System.preview", "cache"), Some("case-sensitive-old"));

    let revisions = revision_repo::find_by_file_id(state.writer_db(), file.id)
        .await
        .unwrap();
    let restored = revisions
        .iter()
        .find(|revision| revision.sequence == 4)
        .unwrap();
    assert_eq!(restored.reason, "restore");
    let restored_properties = revision_repo::find_properties(state.writer_db(), restored.id)
        .await
        .unwrap();
    assert_eq!(restored_properties.len(), 3);
    let restored_value = |namespace: &str, name: &str| {
        restored_properties
            .iter()
            .find(|property| property.namespace == namespace && property.name == name)
            .and_then(|property| property.xml_value.as_deref())
    };
    assert_eq!(restored_value("urn:test", "color"), Some("blue"));
    assert_eq!(
        restored_value("systemx.preview", "cache"),
        Some("boundary-old")
    );
    assert_eq!(
        restored_value("System.preview", "cache"),
        Some("case-sensitive-old")
    );
}

#[actix_web::test]
async fn test_copy_move_rename_and_trash_preserve_revision_history_identity() {
    use aster_drive::db::repository::revision_repo;
    use aster_forge_api::NullablePatch;

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "revisionidentity",
        "revisionidentity@example.com",
        "pass1234",
    )
    .await
    .unwrap();
    let initial = write_service_fixture("identity-1.txt", "one");
    let file = aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "identity.txt", &initial, 3),
    )
    .await
    .unwrap();
    let overwrite = write_service_fixture("identity-2.txt", "two");
    aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "identity.txt", &overwrite, 3).overwrite(file.id),
    )
    .await
    .unwrap();

    let source_history = revision_repo::find_history_by_file_id(state.writer_db(), file.id)
        .await
        .unwrap();
    let source_revision_ids = revision_repo::find_by_file_id(state.writer_db(), file.id)
        .await
        .unwrap()
        .into_iter()
        .map(|revision| (revision.sequence, revision.public_id, revision.etag))
        .collect::<Vec<_>>();

    let copied = aster_drive::services::files::file::copy_file(&state, file.id, user.id, None)
        .await
        .unwrap();
    let copied_history = revision_repo::find_history_by_file_id(state.writer_db(), copied.id)
        .await
        .unwrap();
    assert_ne!(copied_history.id, source_history.id);
    assert_ne!(copied_history.public_id, source_history.public_id);
    let copied_revisions = revision_repo::find_by_file_id(state.writer_db(), copied.id)
        .await
        .unwrap();
    assert_eq!(copied_revisions.len(), 1);
    assert_eq!(copied_revisions[0].sequence, 1);
    assert_eq!(copied_revisions[0].reason, "copy");

    aster_drive::services::files::file::update(
        &state,
        file.id,
        user.id,
        Some("renamed.txt".to_string()),
        NullablePatch::Absent,
    )
    .await
    .unwrap();
    let folder =
        aster_drive::services::files::folder::create(&state, user.id, "Revision Identity", None)
            .await
            .unwrap();
    aster_drive::services::files::file::move_file(&state, file.id, user.id, Some(folder.id))
        .await
        .unwrap();
    aster_drive::services::files::file::delete(&state, file.id, user.id)
        .await
        .unwrap();
    aster_drive::services::files::trash::restore_file(&state, file.id, user.id)
        .await
        .unwrap();

    let final_history = revision_repo::find_history_by_file_id(state.writer_db(), file.id)
        .await
        .unwrap();
    assert_eq!(final_history.id, source_history.id);
    assert_eq!(final_history.public_id, source_history.public_id);
    assert_eq!(
        revision_repo::find_by_file_id(state.writer_db(), file.id)
            .await
            .unwrap()
            .into_iter()
            .map(|revision| (revision.sequence, revision.public_id, revision.etag))
            .collect::<Vec<_>>(),
        source_revision_ids
    );
}

// ─── Copy Naming ──────────────────────────────────────────────────

#[actix_web::test]
async fn test_copy_file_naming() {
    let state = common::setup().await;

    let user = common::create_test_account(&state, "copier", "copier@example.com", "pass1234")
        .await
        .unwrap();

    let temp_dir = format!("/tmp/asterdrive-copy-test-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&temp_dir).unwrap();
    let temp = format!("{}/f.txt", temp_dir);
    std::fs::write(&temp, "copy me").unwrap();

    let file = aster_drive::services::files::file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "doc.txt", &temp, 7),
    )
    .await
    .unwrap();

    // 复制 1 → "doc (1).txt"
    let copy1 = aster_drive::services::files::file::copy_file(&state, file.id, user.id, None)
        .await
        .unwrap();
    assert_eq!(copy1.name, "doc (1).txt");

    // 复制 2 → "doc (2).txt"
    let copy2 = aster_drive::services::files::file::copy_file(&state, file.id, user.id, None)
        .await
        .unwrap();
    assert_eq!(copy2.name, "doc (2).txt");
}

// ─── Folder Service ───────────────────────────────────────────────

#[actix_web::test]
async fn test_folder_service_cycle_detection() {
    let state = common::setup().await;

    let user = common::create_test_account(&state, "cycl", "cyc@example.com", "pass1234")
        .await
        .unwrap();

    let a = aster_drive::services::files::folder::create(&state, user.id, "A", None)
        .await
        .unwrap();
    let b = aster_drive::services::files::folder::create(&state, user.id, "B", Some(a.id))
        .await
        .unwrap();

    // 把 A 移到 B 下面 → 循环，应失败
    let err = aster_drive::services::files::folder::update(
        &state,
        a.id,
        user.id,
        None,
        aster_forge_api::NullablePatch::Value(b.id),
        aster_forge_api::NullablePatch::Absent,
    )
    .await;
    assert!(err.is_err());

    // 正常移动应该 OK
    let c = aster_drive::services::files::folder::create(&state, user.id, "C", None)
        .await
        .unwrap();
    let result = aster_drive::services::files::folder::update(
        &state,
        b.id,
        user.id,
        None,
        aster_forge_api::NullablePatch::Value(c.id),
        aster_forge_api::NullablePatch::Absent,
    )
    .await;
    assert!(result.is_ok());
}

#[actix_web::test]
async fn test_folder_copy_preserves_multi_level_tree_and_storage_used() {
    let state = common::setup().await;

    let user = common::create_test_account(&state, "copytree", "copytree@example.com", "pass1234")
        .await
        .unwrap();

    let root = aster_drive::services::files::folder::create(&state, user.id, "Tree", None)
        .await
        .unwrap();
    let child_a =
        aster_drive::services::files::folder::create(&state, user.id, "ChildA", Some(root.id))
            .await
            .unwrap();
    let child_b =
        aster_drive::services::files::folder::create(&state, user.id, "ChildB", Some(root.id))
            .await
            .unwrap();
    let grandchild = aster_drive::services::files::folder::create(
        &state,
        user.id,
        "Grandchild",
        Some(child_a.id),
    )
    .await
    .unwrap();

    let root_file_id = store_service_file(&state, user.id, Some(root.id), "root.txt", "root").await;
    let child_a_file_id =
        store_service_file(&state, user.id, Some(child_a.id), "child-a.txt", "alpha").await;
    let child_b_file_id =
        store_service_file(&state, user.id, Some(child_b.id), "child-b.txt", "bravo").await;
    let grandchild_file_id = store_service_file(
        &state,
        user.id,
        Some(grandchild.id),
        "grandchild.txt",
        "charlie",
    )
    .await;

    let storage_before_copy = user_storage_used(&state, user.id).await;
    let copied = aster_drive::services::files::folder::copy_folder(&state, root.id, user.id, None)
        .await
        .unwrap();
    assert_eq!(copied.name, "Tree (1)");
    assert_eq!(
        user_storage_used(&state, user.id).await,
        storage_before_copy * 2
    );

    let copied_root_files = aster_drive::db::repository::file_repo::find_by_folder(
        state.writer_db(),
        user.id,
        Some(copied.id),
    )
    .await
    .unwrap();
    assert_eq!(copied_root_files.len(), 1);
    assert_eq!(copied_root_files[0].name, "root.txt");
    assert_ne!(copied_root_files[0].id, root_file_id);
    assert_eq!(
        copied_root_files[0].blob_id,
        aster_drive::db::repository::file_repo::find_by_id(state.writer_db(), root_file_id)
            .await
            .unwrap()
            .blob_id
    );

    let copied_children = aster_drive::db::repository::folder_repo::find_children(
        state.writer_db(),
        user.id,
        Some(copied.id),
    )
    .await
    .unwrap();
    let copied_child_names: BTreeSet<String> = copied_children
        .iter()
        .map(|folder| folder.name.clone())
        .collect();
    assert_eq!(
        copied_child_names,
        BTreeSet::from(["ChildA".to_string(), "ChildB".to_string()])
    );

    let copied_child_a = copied_children
        .iter()
        .find(|folder| folder.name == "ChildA")
        .unwrap();
    let copied_child_b = copied_children
        .iter()
        .find(|folder| folder.name == "ChildB")
        .unwrap();

    let copied_child_a_files = aster_drive::db::repository::file_repo::find_by_folder(
        state.writer_db(),
        user.id,
        Some(copied_child_a.id),
    )
    .await
    .unwrap();
    assert_eq!(copied_child_a_files.len(), 1);
    assert_eq!(copied_child_a_files[0].name, "child-a.txt");
    assert_ne!(copied_child_a_files[0].id, child_a_file_id);

    let copied_child_b_files = aster_drive::db::repository::file_repo::find_by_folder(
        state.writer_db(),
        user.id,
        Some(copied_child_b.id),
    )
    .await
    .unwrap();
    assert_eq!(copied_child_b_files.len(), 1);
    assert_eq!(copied_child_b_files[0].name, "child-b.txt");
    assert_ne!(copied_child_b_files[0].id, child_b_file_id);

    let copied_grandchildren = aster_drive::db::repository::folder_repo::find_children(
        state.writer_db(),
        user.id,
        Some(copied_child_a.id),
    )
    .await
    .unwrap();
    assert_eq!(copied_grandchildren.len(), 1);
    assert_eq!(copied_grandchildren[0].name, "Grandchild");

    let copied_grandchild_files = aster_drive::db::repository::file_repo::find_by_folder(
        state.writer_db(),
        user.id,
        Some(copied_grandchildren[0].id),
    )
    .await
    .unwrap();
    assert_eq!(copied_grandchild_files.len(), 1);
    assert_eq!(copied_grandchild_files[0].name, "grandchild.txt");
    assert_ne!(copied_grandchild_files[0].id, grandchild_file_id);
    assert_eq!(
        copied_grandchild_files[0].blob_id,
        aster_drive::db::repository::file_repo::find_by_id(state.writer_db(), grandchild_file_id)
            .await
            .unwrap()
            .blob_id
    );

    for copied_file in [
        &copied_root_files[0],
        &copied_child_a_files[0],
        &copied_child_b_files[0],
        &copied_grandchild_files[0],
    ] {
        let history = aster_drive::db::repository::revision_repo::find_history_by_file_id(
            state.writer_db(),
            copied_file.id,
        )
        .await
        .expect("every recursively copied file must have a revision history");
        let revisions = aster_drive::db::repository::revision_repo::find_by_file_id(
            state.writer_db(),
            copied_file.id,
        )
        .await
        .unwrap();
        assert_eq!(revisions.len(), 1);
        assert_eq!(revisions[0].sequence, 1);
        assert_eq!(revisions[0].reason, "copy");
        assert_eq!(history.current_revision_id, Some(revisions[0].id));
        assert_eq!(revisions[0].blob_id, Some(copied_file.blob_id));
    }
}

#[actix_web::test]
async fn test_empty_folder_copy_creates_no_revision_rows() {
    use sea_orm::{EntityTrait, PaginatorTrait};

    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "copyempty", "copyempty@example.com", "pass1234")
            .await
            .unwrap();
    let source = aster_drive::services::files::folder::create(&state, user.id, "Empty", None)
        .await
        .unwrap();

    let copied =
        aster_drive::services::files::folder::copy_folder(&state, source.id, user.id, None)
            .await
            .unwrap();
    let copied_files = aster_drive::db::repository::file_repo::find_by_folder(
        state.writer_db(),
        user.id,
        Some(copied.id),
    )
    .await
    .unwrap();
    assert!(copied_files.is_empty());
    assert_eq!(
        aster_drive_model::entities::file_revision::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        aster_drive_model::entities::file_revision_history::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        0
    );
}

#[actix_web::test]
async fn test_folder_copy_creates_initial_revisions_across_batch_boundary() {
    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "copybatch", "copybatch@example.com", "pass1234")
            .await
            .unwrap();
    let app = create_test_app!(state.clone());
    let (token, _) = login_user!(app, "copybatch", "pass1234");
    let source = aster_drive::services::files::folder::create(&state, user.id, "Batch", None)
        .await
        .unwrap();

    for index in 0..51 {
        common::create_empty_file_via_api(
            &app,
            &token,
            &format!("file-{index:02}.txt"),
            Some(source.id),
        )
        .await;
    }

    let copied =
        aster_drive::services::files::folder::copy_folder(&state, source.id, user.id, None)
            .await
            .unwrap();
    let copied_files = aster_drive::db::repository::file_repo::find_by_folder(
        state.writer_db(),
        user.id,
        Some(copied.id),
    )
    .await
    .unwrap();
    assert_eq!(copied_files.len(), 51);

    for copied_file in copied_files {
        let history = aster_drive::db::repository::revision_repo::find_history_by_file_id(
            state.writer_db(),
            copied_file.id,
        )
        .await
        .unwrap();
        let revisions = aster_drive::db::repository::revision_repo::find_by_file_id(
            state.writer_db(),
            copied_file.id,
        )
        .await
        .unwrap();
        assert_eq!(revisions.len(), 1);
        assert_eq!(revisions[0].reason, "copy");
        assert_eq!(revisions[0].sequence, 1);
        assert_eq!(revisions[0].predecessor_revision_id, None);
        assert_eq!(revisions[0].blob_id, Some(copied_file.blob_id));
        assert_eq!(history.current_revision_id, Some(revisions[0].id));
        assert_eq!(history.next_sequence, 2);
    }
}

#[actix_web::test]
async fn test_property_batch_insert_empty_and_chunk_boundaries() {
    use aster_drive::db::repository::property_repo::NewEntityProperty;
    use aster_drive_model::entities::entity_property;
    use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};

    let state = common::setup().await;
    aster_drive::db::repository::property_repo::insert_many(state.writer_db(), Vec::new())
        .await
        .unwrap();
    assert_eq!(
        entity_property::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        0
    );

    for count in [499usize, 500, 501] {
        let namespace = format!("urn:batch:{count}");
        let properties = (0..count)
            .map(|index| NewEntityProperty {
                entity_type: aster_drive_model::types::EntityType::File,
                entity_id: index as i64 + 1,
                namespace: namespace.clone(),
                name: "marker".to_string(),
                value: Some(index.to_string()),
            })
            .collect();
        aster_drive::db::repository::property_repo::insert_many(state.writer_db(), properties)
            .await
            .unwrap();
        assert_eq!(
            entity_property::Entity::find()
                .filter(entity_property::Column::Namespace.eq(&namespace))
                .count(state.writer_db())
                .await
                .unwrap(),
            count as u64
        );
    }
}

#[actix_web::test]
async fn test_folder_copy_quota_failure_does_not_create_descendants() {
    use sea_orm::{ActiveModelTrait, Set};

    let state = common::setup().await;
    let db = state.writer_db().clone();

    let user =
        common::create_test_account(&state, "copyquota", "copyquota@example.com", "pass1234")
            .await
            .unwrap();

    let root = aster_drive::services::files::folder::create(&state, user.id, "QuotaTree", None)
        .await
        .unwrap();
    let nested =
        aster_drive::services::files::folder::create(&state, user.id, "Nested", Some(root.id))
            .await
            .unwrap();

    let root_file_id =
        store_service_file(&state, user.id, Some(root.id), "root.txt", "root payload").await;
    let nested_file_id = store_service_file(
        &state,
        user.id,
        Some(nested.id),
        "nested.txt",
        "nested payload",
    )
    .await;

    let storage_before_copy = user_storage_used(&state, user.id).await;
    let mut user_active: aster_drive_model::entities::user::ActiveModel =
        aster_drive::db::repository::user_repo::find_by_id(&db, user.id)
            .await
            .unwrap()
            .into();
    user_active.storage_quota = Set(storage_before_copy);
    user_active.update(&db).await.unwrap();

    let err = aster_drive::services::files::folder::copy_folder(&state, root.id, user.id, None)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "E032");
    assert_eq!(
        user_storage_used(&state, user.id).await,
        storage_before_copy
    );

    let copied_root = aster_drive::db::repository::folder_repo::find_by_name_in_parent(
        &db,
        user.id,
        None,
        "QuotaTree (1)",
    )
    .await
    .unwrap();
    if let Some(copied_root) = copied_root {
        let copied_root_files = aster_drive::db::repository::file_repo::find_by_folder(
            &db,
            user.id,
            Some(copied_root.id),
        )
        .await
        .unwrap();
        assert!(
            copied_root_files.is_empty(),
            "quota failure should not leave copied files in the new root shell"
        );
        assert!(
            aster_drive::db::repository::folder_repo::find_by_name_in_parent(
                &db,
                user.id,
                Some(copied_root.id),
                "Nested",
            )
            .await
            .unwrap()
            .is_none(),
            "quota failure should not materialize descendant folders under the copy shell"
        );
    }

    let source_root_files =
        aster_drive::db::repository::file_repo::find_by_folder(&db, user.id, Some(root.id))
            .await
            .unwrap();
    assert_eq!(source_root_files.len(), 1);
    assert_eq!(source_root_files[0].id, root_file_id);

    let source_nested = aster_drive::db::repository::folder_repo::find_by_name_in_parent(
        &db,
        user.id,
        Some(root.id),
        "Nested",
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(source_nested.id, nested.id);

    let source_nested_files =
        aster_drive::db::repository::file_repo::find_by_folder(&db, user.id, Some(nested.id))
            .await
            .unwrap();
    assert_eq!(source_nested_files.len(), 1);
    assert_eq!(source_nested_files[0].id, nested_file_id);
}

// ─── Property Service ─────────────────────────────────────────────

#[actix_web::test]
async fn test_property_service_dav_readonly() {
    let state = common::setup().await;

    let user = common::create_test_account(&state, "prop", "prop@example.com", "pass1234")
        .await
        .unwrap();

    let folder = aster_drive::services::files::folder::create(&state, user.id, "PropTest", None)
        .await
        .unwrap();

    // 普通命名空间 OK
    let prop = aster_drive::services::content::property::set(
        &state,
        aster_drive_model::types::EntityType::Folder,
        folder.id,
        user.id,
        "aster:",
        "color",
        Some("blue"),
    )
    .await
    .unwrap();
    assert_eq!(prop.name, "color");

    // DAV: 命名空间被拒绝
    let err = aster_drive::services::content::property::set(
        &state,
        aster_drive_model::types::EntityType::Folder,
        folder.id,
        user.id,
        "DAV:",
        "getcontenttype",
        Some("text/plain"),
    )
    .await;
    assert!(err.is_err());
}

// ─── Driver Registry Invalidation ────────────────────────────────

#[actix_web::test]
async fn test_driver_registry_invalidate_on_policy_update() {
    let state = common::setup().await;

    // 获取默认策略
    let policies = aster_drive::db::repository::policy_repo::find_all(state.writer_db())
        .await
        .unwrap();
    let policy = &policies[0];

    // 首次 get_driver → 缓存创建
    let driver1 = state.driver_registry.get_driver(policy).unwrap();

    // 再次获取 → 应返回同一个缓存实例（Arc 指针相同）
    let driver2 = state.driver_registry.get_driver(policy).unwrap();
    assert!(
        std::sync::Arc::ptr_eq(&driver1, &driver2),
        "cached driver should be the same Arc instance"
    );

    // 通过 service 更新策略（会触发 invalidate）
    aster_drive::services::storage_policy::policy::update(
        &state,
        policy.id,
        aster_drive::services::storage_policy::policy::UpdateStoragePolicyInput {
            name: Some("Updated Name".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // 更新后获取 → 应是新的实例（缓存已失效，重新创建）
    let updated_policy =
        aster_drive::db::repository::policy_repo::find_by_id(state.writer_db(), policy.id)
            .await
            .unwrap();
    let driver3 = state.driver_registry.get_driver(&updated_policy).unwrap();
    assert!(
        !std::sync::Arc::ptr_eq(&driver1, &driver3),
        "driver should be recreated after policy update"
    );
}

#[actix_web::test]
async fn test_share_service_batch_delete_validates_ids_before_scope_work() {
    let state = common::setup().await;
    let oversized = vec![1_i64; aster_drive::services::files::batch::MAX_BATCH_ITEMS + 1];

    let err = match aster_drive::services::share::batch_delete_shares(&state, 999, &[]).await {
        Ok(_) => panic!("empty personal batch delete should fail validation"),
        Err(err) => err,
    };
    assert_eq!(err.code(), "E005");
    assert!(
        err.to_string()
            .contains("at least one share ID is required")
    );

    let err = match aster_drive::services::share::batch_delete_shares(&state, 999, &oversized).await
    {
        Ok(_) => panic!("oversized personal batch delete should fail validation"),
        Err(err) => err,
    };
    assert_eq!(err.code(), "E005");
    assert!(err.to_string().contains("batch size cannot exceed"));

    let err =
        match aster_drive::services::share::batch_delete_team_shares(&state, 999, 999, &[]).await {
            Ok(_) => panic!("empty team batch delete should fail validation"),
            Err(err) => err,
        };
    assert_eq!(err.code(), "E005");
    assert!(
        err.to_string()
            .contains("at least one share ID is required")
    );

    let err =
        match aster_drive::services::share::batch_delete_team_shares(&state, 999, 999, &oversized)
            .await
        {
            Ok(_) => panic!("oversized team batch delete should fail validation"),
            Err(err) => err,
        };
    assert_eq!(err.code(), "E005");
    assert!(err.to_string().contains("batch size cannot exceed"));
}

#[actix_web::test]
async fn test_share_download_failure_rolls_back_download_quota() {
    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "sharedownload",
        "sharedownload@example.com",
        "password123",
    )
    .await
    .unwrap();

    let file_id = store_service_file(&state, user.id, None, "download.txt", "download").await;
    let share = aster_drive::services::share::create_share(
        &state,
        user.id,
        aster_drive::services::share::ShareTarget::file(file_id),
        None,
        None,
        1,
    )
    .await
    .unwrap();

    let file = aster_drive::db::repository::file_repo::find_by_id(state.writer_db(), file_id)
        .await
        .unwrap();
    let blob =
        aster_drive::db::repository::file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
            .await
            .unwrap();
    let policy = state
        .policy_snapshot
        .get_policy_or_err(blob.policy_id)
        .unwrap();
    let stored_path = std::path::Path::new(&common::local_policy_base_path(&policy))
        .join(blob.storage_path_for_connector().expect("stored blob path"));
    std::fs::remove_file(&stored_path).unwrap();

    let err = aster_drive::services::share::download_shared_file_with_range_header(
        &state,
        &share.token,
        None,
        None,
    )
    .await
    .unwrap_err();
    assert_ne!(err.code(), "E053");

    let reloaded = aster_drive::db::repository::share_repo::find_by_id(state.writer_db(), share.id)
        .await
        .unwrap();
    assert_eq!(reloaded.download_count, 0);

    let err = aster_drive::services::share::download_shared_file_with_range_header(
        &state,
        &share.token,
        None,
        None,
    )
    .await
    .unwrap_err();
    assert_ne!(err.code(), "E053");

    let reloaded = aster_drive::db::repository::share_repo::find_by_id(state.writer_db(), share.id)
        .await
        .unwrap();
    assert_eq!(reloaded.download_count, 0);
}

#[actix_web::test]
async fn test_share_download_abort_rolls_back_download_quota() {
    use actix_web::test;

    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let user =
        common::create_test_account(&state, "sdlabort", "sdlabort@example.com", "password123")
            .await
            .unwrap();

    let file_id = store_service_file(
        &state,
        user.id,
        None,
        "abort-download.txt",
        "abort-download",
    )
    .await;
    let share = aster_drive::services::share::create_share(
        &state,
        user.id,
        aster_drive::services::share::ShareTarget::file(file_id),
        None,
        None,
        1,
    )
    .await
    .unwrap();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{}/download", share.token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    drop(resp);

    wait_for_share_download_count(&state, share.id, 0).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{}/download", share.token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    drop(resp);

    wait_for_share_download_count(&state, share.id, 0).await;
}

#[actix_web::test]
async fn test_share_target_check_constraint_rejects_zero_or_multiple_targets() {
    use sea_orm::{ActiveModelTrait, Set};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "shareconstraint",
        "shareconstraint@example.com",
        "password123",
    )
    .await
    .unwrap();

    let file_id = store_service_file(&state, user.id, None, "check.txt", "check-body").await;
    let folder = aster_drive::services::files::folder::create(&state, user.id, "CheckFolder", None)
        .await
        .unwrap();
    let now = chrono::Utc::now();

    let err = aster_drive_model::entities::share::ActiveModel {
        token: Set(uuid::Uuid::new_v4().simple().to_string()),
        user_id: Set(user.id),
        team_id: Set(None),
        file_id: Set(None),
        folder_id: Set(None),
        password: Set(None),
        expires_at: Set(None),
        max_downloads: Set(0),
        download_count: Set(0),
        view_count: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .unwrap_err();
    assert_share_target_check_violation(state.writer_db(), &err);

    let err = aster_drive_model::entities::share::ActiveModel {
        token: Set(uuid::Uuid::new_v4().simple().to_string()),
        user_id: Set(user.id),
        team_id: Set(None),
        file_id: Set(Some(file_id)),
        folder_id: Set(Some(folder.id)),
        password: Set(None),
        expires_at: Set(None),
        max_downloads: Set(0),
        download_count: Set(0),
        view_count: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .unwrap_err();
    assert_share_target_check_violation(state.writer_db(), &err);
}

#[actix_web::test]
async fn test_share_token_length_constraint_rejects_tokens_longer_than_32_chars() {
    use sea_orm::{ActiveModelTrait, Set};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "sharetokenlen",
        "sharetokenlen@example.com",
        "password123",
    )
    .await
    .unwrap();

    let file_id = store_service_file(&state, user.id, None, "token-length.txt", "body").await;
    let now = chrono::Utc::now();

    let err = aster_drive_model::entities::share::ActiveModel {
        token: Set("x".repeat(33)),
        user_id: Set(user.id),
        team_id: Set(None),
        file_id: Set(Some(file_id)),
        folder_id: Set(None),
        password: Set(None),
        expires_at: Set(None),
        max_downloads: Set(0),
        download_count: Set(0),
        view_count: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .unwrap_err();

    assert_share_token_length_violation(state.writer_db(), &err);
}

#[actix_web::test]
async fn test_team_service_accepts_128_multibyte_characters_in_name() {
    let state = common::setup().await;
    let owner = common::create_test_account(
        &state,
        "teamunicode",
        "teamunicode@example.com",
        "password123",
    )
    .await
    .unwrap();

    let valid_name = "你".repeat(128);
    let team = aster_drive::services::workspace::team::create_team(
        &state,
        owner.id,
        aster_drive::services::workspace::team::CreateTeamInput {
            name: valid_name.clone(),
            description: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(team.name, valid_name);

    let err = aster_drive::services::workspace::team::create_team(
        &state,
        owner.id,
        aster_drive::services::workspace::team::CreateTeamInput {
            name: "你".repeat(129),
            description: None,
        },
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), "E005");
    assert!(
        err.to_string()
            .contains("team name must be at most 128 characters")
    );
}

#[actix_web::test]
async fn test_team_service_clamps_negative_default_storage_quota() {
    let state = common::setup().await;
    let owner =
        common::create_test_account(&state, "teamquota", "teamquota@example.com", "password123")
            .await
            .unwrap();

    let mut updated = aster_drive::db::repository::config_repo::find_by_key(
        state.writer_db(),
        "default_storage_quota",
    )
    .await
    .unwrap()
    .unwrap();
    updated.value = "-1".to_string();
    state.runtime_config.apply(updated);

    let team = aster_drive::services::workspace::team::create_team(
        &state,
        owner.id,
        aster_drive::services::workspace::team::CreateTeamInput {
            name: "Quota Clamp".to_string(),
            description: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(team.storage_quota, 0);
}

#[actix_web::test]
async fn test_team_service_rejects_create_without_default_policy_group() {
    use sea_orm::ConnectionTrait;

    let state = common::setup().await;
    let owner = common::create_test_account(
        &state,
        "teamnodefault",
        "teamnodefault@example.com",
        "password123",
    )
    .await
    .unwrap();

    state
        .writer_db()
        .execute_unprepared("UPDATE storage_policy_groups SET is_default = FALSE;")
        .await
        .unwrap();
    state
        .driver_registry
        .reload_policy_snapshot(&state.policy_snapshot, state.writer_db())
        .await
        .unwrap();

    let err = aster_drive::services::workspace::team::create_team(
        &state,
        owner.id,
        aster_drive::services::workspace::team::CreateTeamInput {
            name: "No Default Policy Group".to_string(),
            description: None,
        },
    )
    .await
    .unwrap_err();

    assert_eq!(err.code(), "E005");
    assert!(
        err.message()
            .contains("no system default storage policy group configured")
    );
}

#[actix_web::test]
async fn test_team_service_degrades_missing_creator_rows() {
    use sea_orm::{IntoActiveModel, Set};

    let state = common::setup().await;
    let owner =
        common::create_test_account(&state, "missowner", "missowner@example.com", "password123")
            .await
            .unwrap();
    let member = common::create_test_account(
        &state,
        "missmember",
        "missmember@example.com",
        "password123",
    )
    .await
    .unwrap();

    let team = aster_drive::services::workspace::team::create_team(
        &state,
        owner.id,
        aster_drive::services::workspace::team::CreateTeamInput {
            name: "Missing Creator".to_string(),
            description: None,
        },
    )
    .await
    .unwrap();

    aster_drive::services::workspace::team::add_member(
        &state,
        team.id,
        owner.id,
        aster_drive::services::workspace::team::AddTeamMemberInput {
            user_id: Some(member.id),
            identifier: None,
            role: aster_drive_model::types::TeamMemberRole::Member,
        },
    )
    .await
    .unwrap();

    common::set_foreign_key_checks(state.writer_db(), false)
        .await
        .unwrap();
    let mut broken_team =
        aster_drive::db::repository::team_repo::find_by_id(state.writer_db(), team.id)
            .await
            .unwrap()
            .into_active_model();
    broken_team.created_by = Set(i64::MAX);
    aster_drive::db::repository::team_repo::update(state.writer_db(), broken_team)
        .await
        .unwrap();
    common::set_foreign_key_checks(state.writer_db(), true)
        .await
        .unwrap();

    let loaded = aster_drive::services::workspace::team::get_team(&state, team.id, owner.id)
        .await
        .unwrap();
    assert!(loaded.created_by.is_none());

    let teams = aster_drive::services::workspace::team::list_teams(&state, member.id, false)
        .await
        .unwrap();
    assert_eq!(teams.len(), 1);
    assert!(teams[0].created_by.is_none());
}

#[actix_web::test]
async fn test_folder_repo_find_expired_deleted_includes_team_folders() {
    use chrono::{Duration, Utc};
    use sea_orm::Set;

    let state = common::setup().await;
    let owner = common::create_test_account(
        &state,
        "trashteamowner",
        "trashteamowner@example.com",
        "password123",
    )
    .await
    .unwrap();
    let team = aster_drive::services::workspace::team::create_team(
        &state,
        owner.id,
        aster_drive::services::workspace::team::CreateTeamInput {
            name: "Trash Team".to_string(),
            description: None,
        },
    )
    .await
    .unwrap();

    let deleted_at = Utc::now() - Duration::days(10);
    let created = aster_drive::db::repository::folder_repo::create(
        state.writer_db(),
        aster_drive_model::entities::folder::ActiveModel {
            name: Set("Team Trash".to_string()),
            parent_id: Set(None),
            team_id: Set(Some(team.id)),
            owner_user_id: Set(None),
            created_by_user_id: Set(Some(owner.id)),
            created_by_username: Set(owner.username.clone()),
            policy_id: Set(None),
            created_at: Set(deleted_at),
            updated_at: Set(deleted_at),
            deleted_at: Set(Some(deleted_at)),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let expired = aster_drive::db::repository::folder_repo::find_expired_deleted(
        state.writer_db(),
        Utc::now(),
    )
    .await
    .unwrap();

    assert!(expired.iter().any(|folder| folder.id == created.id));
}

#[actix_web::test]
async fn test_folder_repo_find_all_by_user_excludes_team_folders() {
    use chrono::Utc;
    use sea_orm::Set;

    let state = common::setup().await;
    let owner =
        common::create_test_account(&state, "foldown", "foldown@example.com", "password123")
            .await
            .unwrap();
    let member =
        common::create_test_account(&state, "foldmem", "foldmem@example.com", "password123")
            .await
            .unwrap();

    let team = aster_drive::services::workspace::team::create_team(
        &state,
        owner.id,
        aster_drive::services::workspace::team::CreateTeamInput {
            name: "Folder Scope".to_string(),
            description: None,
        },
    )
    .await
    .unwrap();

    aster_drive::services::workspace::team::add_member(
        &state,
        team.id,
        owner.id,
        aster_drive::services::workspace::team::AddTeamMemberInput {
            user_id: Some(member.id),
            identifier: None,
            role: aster_drive_model::types::TeamMemberRole::Member,
        },
    )
    .await
    .unwrap();

    let now = Utc::now();
    let personal = aster_drive::db::repository::folder_repo::create(
        state.writer_db(),
        aster_drive_model::entities::folder::ActiveModel {
            name: Set("Personal".to_string()),
            parent_id: Set(None),
            team_id: Set(None),
            owner_user_id: Set(Some(member.id)),
            created_by_user_id: Set(Some(member.id)),
            created_by_username: Set(member.username.clone()),
            policy_id: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            deleted_at: Set(None),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let team_folder = aster_drive::db::repository::folder_repo::create(
        state.writer_db(),
        aster_drive_model::entities::folder::ActiveModel {
            name: Set("Team".to_string()),
            parent_id: Set(None),
            team_id: Set(Some(team.id)),
            owner_user_id: Set(None),
            created_by_user_id: Set(Some(member.id)),
            created_by_username: Set(member.username.clone()),
            policy_id: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            deleted_at: Set(None),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let folders =
        aster_drive::db::repository::folder_repo::find_all_by_user(state.writer_db(), member.id)
            .await
            .unwrap();
    let folder_ids: BTreeSet<i64> = folders.into_iter().map(|folder| folder.id).collect();

    assert!(folder_ids.contains(&personal.id));
    assert!(!folder_ids.contains(&team_folder.id));
}

#[actix_web::test]
async fn test_folder_repo_top_level_deleted_pagination_is_stable_for_equal_timestamps() {
    use chrono::Utc;
    use sea_orm::Set;

    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "trashord", "trashord@example.com", "password123")
            .await
            .unwrap();

    let deleted_at = Utc::now();
    let first = aster_drive::db::repository::folder_repo::create(
        state.writer_db(),
        aster_drive_model::entities::folder::ActiveModel {
            name: Set("first".to_string()),
            parent_id: Set(None),
            team_id: Set(None),
            owner_user_id: Set(Some(user.id)),
            created_by_user_id: Set(Some(user.id)),
            created_by_username: Set(user.username.clone()),
            policy_id: Set(None),
            created_at: Set(deleted_at),
            updated_at: Set(deleted_at),
            deleted_at: Set(Some(deleted_at)),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let second = aster_drive::db::repository::folder_repo::create(
        state.writer_db(),
        aster_drive_model::entities::folder::ActiveModel {
            name: Set("second".to_string()),
            parent_id: Set(None),
            team_id: Set(None),
            owner_user_id: Set(Some(user.id)),
            created_by_user_id: Set(Some(user.id)),
            created_by_username: Set(user.username.clone()),
            policy_id: Set(None),
            created_at: Set(deleted_at),
            updated_at: Set(deleted_at),
            deleted_at: Set(Some(deleted_at)),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let (page_one, total) =
        aster_drive::db::repository::folder_repo::find_top_level_deleted_paginated(
            state.writer_db(),
            user.id,
            1,
            0,
        )
        .await
        .unwrap();
    let (page_two, _) = aster_drive::db::repository::folder_repo::find_top_level_deleted_paginated(
        state.writer_db(),
        user.id,
        1,
        1,
    )
    .await
    .unwrap();

    assert_eq!(total, 2);
    assert_eq!(page_one.len(), 1);
    assert_eq!(page_two.len(), 1);
    assert_eq!(page_one[0].id, second.id);
    assert_eq!(page_two[0].id, first.id);
}

#[actix_web::test]
async fn test_team_service_list_teams_for_member() {
    let state = common::setup().await;
    let owner = common::create_test_account(
        &state,
        "listteams-owner",
        "listteams-owner@example.com",
        "password123",
    )
    .await
    .unwrap();
    let member = common::create_test_account(
        &state,
        "listteams-member",
        "listteams-member@example.com",
        "password123",
    )
    .await
    .unwrap();

    let team = aster_drive::services::workspace::team::create_team(
        &state,
        owner.id,
        aster_drive::services::workspace::team::CreateTeamInput {
            name: "List Teams".to_string(),
            description: None,
        },
    )
    .await
    .unwrap();

    aster_drive::services::workspace::team::add_member(
        &state,
        team.id,
        owner.id,
        aster_drive::services::workspace::team::AddTeamMemberInput {
            user_id: Some(member.id),
            identifier: None,
            role: aster_drive_model::types::TeamMemberRole::Member,
        },
    )
    .await
    .unwrap();

    let teams = aster_drive::services::workspace::team::list_teams(&state, member.id, false)
        .await
        .unwrap();
    assert_eq!(teams.len(), 1);
    assert_eq!(teams[0].id, team.id);
    assert_eq!(
        teams[0].my_role,
        aster_drive_model::types::TeamMemberRole::Member
    );
}

#[actix_web::test]
async fn test_team_service_list_user_team_ids_filters_archived_teams() {
    let state = common::setup().await;
    let owner = common::create_test_account(
        &state,
        "teamids-owner",
        "listteamids-owner@example.com",
        "password123",
    )
    .await
    .unwrap();
    let member = common::create_test_account(
        &state,
        "teamids-member",
        "listteamids-member@example.com",
        "password123",
    )
    .await
    .unwrap();

    let active_team = aster_drive::services::workspace::team::create_team(
        &state,
        owner.id,
        aster_drive::services::workspace::team::CreateTeamInput {
            name: "Active Team".to_string(),
            description: None,
        },
    )
    .await
    .unwrap();
    let archived_team = aster_drive::services::workspace::team::create_team(
        &state,
        owner.id,
        aster_drive::services::workspace::team::CreateTeamInput {
            name: "Archived Team".to_string(),
            description: None,
        },
    )
    .await
    .unwrap();

    for team_id in [active_team.id, archived_team.id] {
        aster_drive::services::workspace::team::add_member(
            &state,
            team_id,
            owner.id,
            aster_drive::services::workspace::team::AddTeamMemberInput {
                user_id: Some(member.id),
                identifier: None,
                role: aster_drive_model::types::TeamMemberRole::Member,
            },
        )
        .await
        .unwrap();
    }

    aster_drive::services::workspace::team::archive_team(&state, archived_team.id, owner.id)
        .await
        .unwrap();

    let active_team_ids =
        aster_drive::services::workspace::team::list_user_team_ids(&state, member.id, false)
            .await
            .unwrap();
    assert_eq!(active_team_ids.len(), 1);
    assert!(active_team_ids.contains(&active_team.id));
    assert!(!active_team_ids.contains(&archived_team.id));

    let archived_team_ids =
        aster_drive::services::workspace::team::list_user_team_ids(&state, member.id, true)
            .await
            .unwrap();
    assert_eq!(archived_team_ids.len(), 1);
    assert!(archived_team_ids.contains(&archived_team.id));
    assert!(!archived_team_ids.contains(&active_team.id));
}

#[actix_web::test]
async fn test_team_archive_cleanup_deletes_expired_team_data() {
    use chrono::{Duration, Utc};
    use sea_orm::{EntityTrait, IntoActiveModel, Set};

    let state = common::setup().await;
    let owner = common::create_test_account(
        &state,
        "cleanup-owner",
        "cleanup-owner@example.com",
        "password123",
    )
    .await
    .unwrap();
    let team = aster_drive::services::workspace::team::create_team(
        &state,
        owner.id,
        aster_drive::services::workspace::team::CreateTeamInput {
            name: "Cleanup Team".to_string(),
            description: None,
        },
    )
    .await
    .unwrap();

    let default_policy_id =
        aster_drive::db::repository::policy_repo::find_default(state.writer_db())
            .await
            .unwrap()
            .expect("default policy should exist")
            .id;
    let now = Utc::now();
    let folder = aster_drive::db::repository::folder_repo::create(
        state.writer_db(),
        aster_drive_model::entities::folder::ActiveModel {
            name: Set("cleanup-folder".to_string()),
            parent_id: Set(None),
            team_id: Set(Some(team.id)),
            owner_user_id: Set(None),
            created_by_user_id: Set(Some(owner.id)),
            created_by_username: Set(owner.username.clone()),
            policy_id: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            deleted_at: Set(None),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let blob = aster_drive::db::repository::file_repo::create_blob(
        state.writer_db(),
        aster_drive_model::entities::file_blob::ActiveModel {
            hash: Set(format!("cleanup-blob-{}", uuid::Uuid::new_v4())),
            size: Set(12),
            policy_id: Set(default_policy_id),
            storage_path: Set(Some(format!("files/{}", uuid::Uuid::new_v4()))),
            ref_count: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let file = aster_drive::db::repository::file_repo::create(
        state.writer_db(),
        aster_drive_model::entities::file::ActiveModel {
            name: Set("cleanup.txt".to_string()),
            folder_id: Set(Some(folder.id)),
            team_id: Set(Some(team.id)),
            blob_id: Set(blob.id),
            size: Set(12),
            owner_user_id: Set(None),
            created_by_user_id: Set(Some(owner.id)),
            created_by_username: Set(owner.username.clone()),
            mime_type: Set("text/plain".to_string()),
            created_at: Set(now),
            updated_at: Set(now),
            deleted_at: Set(None),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let history = aster_drive::db::repository::revision_repo::create_initial(
        state.writer_db(),
        &file,
        aster_drive::db::repository::revision_repo::RevisionReason::Create,
    )
    .await
    .unwrap();

    aster_drive::db::repository::property_repo::upsert(
        state.writer_db(),
        aster_drive_model::types::EntityType::Folder,
        folder.id,
        "test",
        "label",
        Some("cleanup"),
    )
    .await
    .unwrap();

    aster_drive::services::files::lock::lock(
        &state,
        aster_drive_model::types::EntityType::Folder,
        folder.id,
        Some(owner.id),
        None,
        None,
    )
    .await
    .unwrap();

    aster_drive::db::repository::share_repo::create(
        state.writer_db(),
        aster_drive_model::entities::share::ActiveModel {
            token: Set(uuid::Uuid::new_v4().simple().to_string()),
            user_id: Set(owner.id),
            team_id: Set(Some(team.id)),
            file_id: Set(Some(file.id)),
            folder_id: Set(None),
            password: Set(None),
            expires_at: Set(None),
            max_downloads: Set(0),
            download_count: Set(0),
            view_count: Set(0),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let upload_id = uuid::Uuid::new_v4().to_string();
    aster_drive::db::repository::upload_session_repo::create(
        state.writer_db(),
        aster_drive_model::entities::upload_session::ActiveModel {
            id: Set(upload_id.clone()),
            user_id: Set(owner.id),
            team_id: Set(Some(team.id)),
            frontend_client_id: Set(None),
            filename: Set("pending.bin".to_string()),
            total_size: Set(10),
            chunk_size: Set(10),
            total_chunks: Set(1),
            received_count: Set(0),
            folder_id: Set(Some(folder.id)),
            policy_id: Set(default_policy_id),
            placement_profile_id: Set(None),
            placement_rule_id: Set(None),
            placement_revision: Set(None),
            placement_execution_preference: Set("automatic".to_string()),
            status: Set(aster_drive_model::types::UploadSessionStatus::Uploading),
            session_kind: Set(aster_drive_model::types::UploadSessionKind::ProviderPresignedSingle),
            object_temp_key: Set(Some(format!("files/{upload_id}"))),
            object_multipart_id: Set(None),
            provider_session_ciphertext: Set(None),
            file_id: Set(None),
            created_at: Set(now),
            expires_at: Set(now + Duration::hours(1)),
            updated_at: Set(now),
        },
    )
    .await
    .unwrap();

    aster_drive::services::workspace::team::archive_team(&state, team.id, owner.id)
        .await
        .unwrap();

    let mut archived_team =
        aster_drive::db::repository::team_repo::find_by_id(state.writer_db(), team.id)
            .await
            .unwrap()
            .into_active_model();
    archived_team.archived_at = Set(Some(Utc::now() - Duration::days(8)));
    archived_team.updated_at = Set(Utc::now() - Duration::days(8));
    aster_drive::db::repository::team_repo::update(state.writer_db(), archived_team)
        .await
        .unwrap();

    let deleted = aster_drive::services::workspace::team::cleanup_expired_archived_teams(&state)
        .await
        .unwrap();
    assert_eq!(deleted, 1);
    assert!(
        aster_drive::db::repository::team_repo::find_by_id(state.writer_db(), team.id)
            .await
            .is_err()
    );
    assert!(
        aster_drive::db::repository::file_repo::find_by_id(state.writer_db(), file.id)
            .await
            .is_err()
    );
    assert!(
        aster_drive_model::entities::file_revision_history::Entity::find_by_id(history.history_id)
            .one(state.writer_db())
            .await
            .unwrap()
            .is_some(),
        "purged team ledger identities must remain as retired tombstones"
    );
    assert!(
        aster_drive::db::repository::folder_repo::find_by_id(state.writer_db(), folder.id)
            .await
            .is_err()
    );
    assert!(
        aster_drive::db::repository::team_member_repo::find_by_team_and_user(
            state.writer_db(),
            team.id,
            owner.id
        )
        .await
        .unwrap()
        .is_none()
    );
    assert!(
        aster_drive::db::repository::share_repo::find_by_team(state.writer_db(), team.id)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        aster_drive::db::repository::upload_session_repo::find_by_team(state.writer_db(), team.id)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        aster_drive::db::repository::lock_repo::find_by_path_prefix(
            state.writer_db(),
            &format!("/teams/{}/", team.id),
        )
        .await
        .unwrap()
        .is_empty()
    );
    assert!(
        aster_drive::db::repository::property_repo::find_by_entity(
            state.writer_db(),
            aster_drive_model::types::EntityType::Folder,
            folder.id,
        )
        .await
        .unwrap()
        .is_empty()
    );
}

#[cfg(unix)]
#[actix_web::test]
async fn test_team_archive_cleanup_keeps_team_when_upload_temp_delete_fails() {
    use chrono::{Duration, Utc};
    use sea_orm::{IntoActiveModel, Set};

    let state = common::setup().await;
    let owner = common::create_test_account(
        &state,
        "cleanupfail",
        "cleanup-fail-owner@example.com",
        "password123",
    )
    .await
    .unwrap();
    let team = aster_drive::services::workspace::team::create_team(
        &state,
        owner.id,
        aster_drive::services::workspace::team::CreateTeamInput {
            name: "Cleanup Failure Team".to_string(),
            description: None,
        },
    )
    .await
    .unwrap();

    let policy = aster_drive::db::repository::policy_repo::find_default(state.writer_db())
        .await
        .unwrap()
        .expect("default policy should exist");
    let driver = state.driver_registry.get_driver(&policy).unwrap();
    let upload_id = uuid::Uuid::new_v4().to_string();
    let temp_key = format!("tmp/team-cleanup-fails/{upload_id}.bin");
    driver.put(&temp_key, b"stale team upload").await.unwrap();
    let now = Utc::now();
    aster_drive::db::repository::upload_session_repo::create(
        state.writer_db(),
        aster_drive_model::entities::upload_session::ActiveModel {
            id: Set(upload_id.clone()),
            user_id: Set(owner.id),
            team_id: Set(Some(team.id)),
            frontend_client_id: Set(None),
            filename: Set("pending.bin".to_string()),
            total_size: Set(10),
            chunk_size: Set(10),
            total_chunks: Set(1),
            received_count: Set(0),
            folder_id: Set(None),
            policy_id: Set(policy.id),
            placement_profile_id: Set(None),
            placement_rule_id: Set(None),
            placement_revision: Set(None),
            placement_execution_preference: Set("automatic".to_string()),
            status: Set(aster_drive_model::types::UploadSessionStatus::Uploading),
            session_kind: Set(aster_drive_model::types::UploadSessionKind::ProviderPresignedSingle),
            object_temp_key: Set(Some(temp_key.clone())),
            object_multipart_id: Set(None),
            provider_session_ciphertext: Set(None),
            file_id: Set(None),
            created_at: Set(now),
            expires_at: Set(now + Duration::hours(1)),
            updated_at: Set(now),
        },
    )
    .await
    .unwrap();

    aster_drive::services::workspace::team::archive_team(&state, team.id, owner.id)
        .await
        .unwrap();
    let mut archived_team =
        aster_drive::db::repository::team_repo::find_by_id(state.writer_db(), team.id)
            .await
            .unwrap()
            .into_active_model();
    archived_team.archived_at = Set(Some(Utc::now() - Duration::days(8)));
    archived_team.updated_at = Set(Utc::now() - Duration::days(8));
    aster_drive::db::repository::team_repo::update(state.writer_db(), archived_team)
        .await
        .unwrap();

    let object_parent = std::path::Path::new(&common::local_policy_base_path(&policy))
        .join(&temp_key)
        .parent()
        .unwrap()
        .to_path_buf();
    let guard = DirModeGuard::set_read_only(object_parent);

    let deleted = aster_drive::services::workspace::team::cleanup_expired_archived_teams(&state)
        .await
        .unwrap();

    assert_eq!(deleted, 0);
    assert!(
        aster_drive::db::repository::team_repo::find_by_id(state.writer_db(), team.id)
            .await
            .is_ok(),
        "team row must remain until upload temp cleanup can be retried"
    );
    assert!(
        aster_drive::db::repository::upload_session_repo::find_by_id(state.writer_db(), &upload_id)
            .await
            .is_ok(),
        "upload session must remain as the retry handle for the stale temp object"
    );
    assert!(driver.exists(&temp_key).await.unwrap());

    drop(guard);

    let deleted = aster_drive::services::workspace::team::cleanup_expired_archived_teams(&state)
        .await
        .unwrap();

    assert_eq!(deleted, 1);
    assert!(
        aster_drive::db::repository::team_repo::find_by_id(state.writer_db(), team.id)
            .await
            .is_err()
    );
    assert!(
        aster_drive::db::repository::upload_session_repo::find_by_id(state.writer_db(), &upload_id)
            .await
            .is_err()
    );
    assert!(!driver.exists(&temp_key).await.unwrap());
}

#[actix_web::test]
async fn test_team_archive_cleanup_processes_multiple_file_and_folder_batches() {
    use chrono::{Duration, Utc};
    use sea_orm::{ColumnTrait, EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, Set};

    let state = common::setup().await;
    let owner = common::create_test_account(
        &state,
        "batchowner",
        "batchcleanup-owner@example.com",
        "password123",
    )
    .await
    .unwrap();
    let team = aster_drive::services::workspace::team::create_team(
        &state,
        owner.id,
        aster_drive::services::workspace::team::CreateTeamInput {
            name: "Batch Cleanup Team".to_string(),
            description: None,
        },
    )
    .await
    .unwrap();

    let default_policy_id =
        aster_drive::db::repository::policy_repo::find_default(state.writer_db())
            .await
            .unwrap()
            .expect("default policy should exist")
            .id;
    let now = Utc::now();
    let blob = aster_drive::db::repository::file_repo::create_blob(
        state.writer_db(),
        aster_drive_model::entities::file_blob::ActiveModel {
            hash: Set(format!("batch-cleanup-blob-{}", uuid::Uuid::new_v4())),
            size: Set(1),
            policy_id: Set(default_policy_id),
            storage_path: Set(Some(format!("files/{}", uuid::Uuid::new_v4()))),
            ref_count: Set(1001),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let mut sample_file_ids = Vec::new();
    let mut sample_history_ids = Vec::new();
    for idx in 0..1001 {
        let file = aster_drive::db::repository::file_repo::create(
            state.writer_db(),
            aster_drive_model::entities::file::ActiveModel {
                name: Set(format!("batched-file-{idx:04}.txt")),
                folder_id: Set(None),
                team_id: Set(Some(team.id)),
                blob_id: Set(blob.id),
                size: Set(1),
                owner_user_id: Set(None),
                created_by_user_id: Set(Some(owner.id)),
                created_by_username: Set(owner.username.clone()),
                mime_type: Set("text/plain".to_string()),
                created_at: Set(now),
                updated_at: Set(now),
                deleted_at: Set(None),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let history = aster_drive::db::repository::revision_repo::create_initial(
            state.writer_db(),
            &file,
            aster_drive::db::repository::revision_repo::RevisionReason::Create,
        )
        .await
        .unwrap();
        if idx == 0 || idx == 1000 {
            sample_file_ids.push(file.id);
            sample_history_ids.push(history.history_id);
        }
    }

    assert!(
        aster_drive::services::ops::integrity::audit_revision_ledger(state.writer_db())
            .await
            .unwrap()
            .is_empty(),
        "revision ledger audit must cross its 1000-row pagination boundary"
    );

    let mut sample_folder_ids = Vec::new();
    for idx in 0..1001 {
        let folder = aster_drive::db::repository::folder_repo::create(
            state.writer_db(),
            aster_drive_model::entities::folder::ActiveModel {
                name: Set(format!("batched-folder-{idx:04}")),
                parent_id: Set(None),
                team_id: Set(Some(team.id)),
                owner_user_id: Set(None),
                created_by_user_id: Set(Some(owner.id)),
                created_by_username: Set(owner.username.clone()),
                policy_id: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
                deleted_at: Set(None),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        if idx == 0 || idx == 1000 {
            aster_drive::db::repository::property_repo::upsert(
                state.writer_db(),
                aster_drive_model::types::EntityType::Folder,
                folder.id,
                "aster:",
                "batch",
                Some("cleanup"),
            )
            .await
            .unwrap();
            sample_folder_ids.push(folder.id);
        }
    }

    aster_drive::services::workspace::team::archive_team(&state, team.id, owner.id)
        .await
        .unwrap();

    let mut archived_team =
        aster_drive::db::repository::team_repo::find_by_id(state.writer_db(), team.id)
            .await
            .unwrap()
            .into_active_model();
    archived_team.archived_at = Set(Some(Utc::now() - Duration::days(8)));
    archived_team.updated_at = Set(Utc::now() - Duration::days(8));
    aster_drive::db::repository::team_repo::update(state.writer_db(), archived_team)
        .await
        .unwrap();

    let deleted = aster_drive::services::workspace::team::cleanup_expired_archived_teams(&state)
        .await
        .unwrap();
    assert_eq!(deleted, 1);
    assert!(
        aster_drive::db::repository::team_repo::find_by_id(state.writer_db(), team.id)
            .await
            .is_err()
    );
    for file_id in sample_file_ids {
        assert!(
            aster_drive::db::repository::file_repo::find_by_id(state.writer_db(), file_id)
                .await
                .is_err()
        );
    }
    for history_id in sample_history_ids {
        let history =
            aster_drive_model::entities::file_revision_history::Entity::find_by_id(history_id)
                .one(state.writer_db())
                .await
                .unwrap()
                .expect("archived team cleanup must preserve the revision history tombstone");
        assert_eq!(history.file_id, None);
        assert_eq!(history.current_revision_id, None);
        assert!(history.retired_at.is_some());

        let revisions = aster_drive_model::entities::file_revision::Entity::find()
            .filter(aster_drive_model::entities::file_revision::Column::HistoryId.eq(history_id))
            .all(state.writer_db())
            .await
            .unwrap();
        assert_eq!(revisions.len(), 1);
        assert_eq!(revisions[0].blob_id, None);
        assert!(revisions[0].retired_at.is_some());
        assert!(revisions[0].purged_at.is_some());
    }
    assert_eq!(
        aster_drive_model::entities::file_revision_history::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        1_001,
        "all batched histories must remain as retired identity tombstones"
    );
    assert_eq!(
        aster_drive_model::entities::file_revision::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        1_001,
        "all batched revisions must remain as purged sequence tombstones"
    );
    for folder_id in sample_folder_ids {
        assert!(
            aster_drive::db::repository::folder_repo::find_by_id(state.writer_db(), folder_id)
                .await
                .is_err()
        );
        assert!(
            aster_drive::db::repository::property_repo::find_by_entity(
                state.writer_db(),
                aster_drive_model::types::EntityType::Folder,
                folder_id,
            )
            .await
            .unwrap()
            .is_empty()
        );
    }
    assert!(
        aster_drive::db::repository::file_repo::find_blob_by_id(state.writer_db(), blob.id)
            .await
            .is_err()
    );
}

#[actix_web::test]
async fn test_team_archive_cleanup_respects_configured_retention() {
    use chrono::{Duration, Utc};
    use sea_orm::{IntoActiveModel, Set};

    let state = common::setup().await;
    let owner = common::create_test_account(
        &state,
        "clnretainown",
        "cleanup-retention-owner@example.com",
        "password123",
    )
    .await
    .unwrap();
    let team = aster_drive::services::workspace::team::create_team(
        &state,
        owner.id,
        aster_drive::services::workspace::team::CreateTeamInput {
            name: "Retention Team".to_string(),
            description: None,
        },
    )
    .await
    .unwrap();

    let mut config = aster_drive::db::repository::config_repo::find_by_key(
        state.writer_db(),
        "team_archive_retention_days",
    )
    .await
    .unwrap()
    .unwrap();
    config.value = "30".to_string();
    state.runtime_config.apply(config);

    aster_drive::services::workspace::team::archive_team(&state, team.id, owner.id)
        .await
        .unwrap();

    let mut archived_team =
        aster_drive::db::repository::team_repo::find_by_id(state.writer_db(), team.id)
            .await
            .unwrap()
            .into_active_model();
    archived_team.archived_at = Set(Some(Utc::now() - Duration::days(8)));
    archived_team.updated_at = Set(Utc::now() - Duration::days(8));
    aster_drive::db::repository::team_repo::update(state.writer_db(), archived_team)
        .await
        .unwrap();

    let deleted = aster_drive::services::workspace::team::cleanup_expired_archived_teams(&state)
        .await
        .unwrap();
    assert_eq!(deleted, 0);

    let archived =
        aster_drive::db::repository::team_repo::find_archived_by_id(state.writer_db(), team.id)
            .await
            .unwrap();
    assert_eq!(archived.id, team.id);
}
