//! 集成测试：`webdav_lock_system`。

use crate::common;
use aster_drive::runtime::SharedRuntimeState;
use aster_forge_webdav::{DavLockAcquireRequest, DavLockError, DavMutationCredentials};

use std::io::Cursor;
use std::time::Duration;

fn write_temp_fixture(name: &str, contents: &str) -> String {
    let dir = format!("/tmp/asterdrive-webdav-lock-test-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&dir).unwrap();
    let path = format!("{dir}/{name}");
    std::fs::write(&path, contents).unwrap();
    path
}

#[actix_web::test]
async fn test_db_lock_system_deep_lock_supports_check_refresh_discover_and_delete() {
    use aster_drive::db::repository::lock_repo;
    use aster_drive::services::{files::file, files::folder};
    use aster_drive::webdav::backend::lock::DbLockSystem;
    use aster_forge_webdav::DavXmlElement;
    use aster_forge_webdav::{DavLockSystem, DavPath};

    let state = common::setup().await;
    let user = common::create_test_account(&state, "davlocks", "davlocks@example.com", "pass1234")
        .await
        .unwrap();

    let projects = folder::create(&state, user.id, "projects", None)
        .await
        .unwrap();
    let docs = folder::create(&state, user.id, "docs", Some(projects.id))
        .await
        .unwrap();
    let temp_path = write_temp_fixture("note.txt", "deep lock content");
    file::store_from_temp(
        &state,
        user.id,
        file::StoreFromTempRequest::new(
            Some(docs.id),
            "note.txt",
            &temp_path,
            "deep lock content".len() as i64,
        ),
    )
    .await
    .unwrap();

    let lock_system = DbLockSystem::new(state.clone(), user.id, None);
    let folder_path = DavPath::new("/projects/").unwrap();
    let child_path = DavPath::new("/projects/docs/note.txt").unwrap();
    let owner = DavXmlElement::parse_reader(Cursor::new(
        br#"<D:owner xmlns:D="DAV:"><D:href>tester</D:href></D:owner>"#,
    ))
    .unwrap();

    let lock = lock_system
        .lock(DavLockAcquireRequest {
            path: &folder_path,
            principal: Some("tester"),
            owner: Some(&owner),
            timeout: Some(Duration::from_secs(120)),
            shared: false,
            deep: true,
            credentials: DavMutationCredentials::default(),
        })
        .await
        .unwrap()
        .lock;
    assert!(lock.deep);
    assert_eq!(lock.principal.as_deref(), Some("tester"));
    assert!(!lock.token.is_empty());

    assert_eq!(
        lock_repo::find_all_by_entity(
            state.writer_db(),
            aster_drive_model::types::EntityType::Folder,
            projects.id,
        )
        .await
        .unwrap()
        .len(),
        1
    );

    let conflict = lock_system
        .check(&child_path, None, false, false, &[])
        .await
        .unwrap_err();
    let DavLockError::Conflict(conflict) = conflict else {
        panic!("missing deep-lock token should return the conflicting lock");
    };
    assert_eq!(conflict.token, lock.token);

    lock_system
        .check(
            &child_path,
            None,
            false,
            false,
            std::slice::from_ref(&lock.token),
        )
        .await
        .unwrap();

    let discovered = lock_system.discover(&child_path).await.unwrap();
    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0].token, lock.token);
    assert_eq!(discovered[0].principal, None);
    assert!(discovered[0].owner.is_some());

    let refreshed = lock_system
        .refresh(&folder_path, &lock.token, Some(Duration::from_secs(30)))
        .await
        .unwrap();
    assert_eq!(refreshed.token, lock.token);
    assert_eq!(refreshed.principal, None);
    assert!(refreshed.owner.is_some());
    assert_eq!(refreshed.timeout, Some(Duration::from_secs(30)));

    let unrelated_path = DavPath::new("/unrelated.txt").unwrap();
    assert!(
        lock_system
            .refresh(&unrelated_path, &lock.token, Some(Duration::from_secs(45)))
            .await
            .is_err(),
        "LOCK refresh must target the locked resource or a resource covered by a deep lock"
    );

    let persisted = lock_repo::find_by_token(state.writer_db(), &lock.token)
        .await
        .unwrap()
        .expect("refreshed lock should still exist");
    assert!(persisted.timeout_at.is_some());

    lock_system.delete(&folder_path).await.unwrap();
    assert!(
        lock_repo::find_by_token(state.writer_db(), &lock.token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        lock_repo::find_all_by_entity(
            state.writer_db(),
            aster_drive_model::types::EntityType::Folder,
            projects.id,
        )
        .await
        .unwrap()
        .is_empty()
    );
}

#[actix_web::test]
async fn test_db_lock_system_checks_parent_and_member_lock_roots_separately() {
    use aster_drive::services::{files::file, files::folder};
    use aster_drive::webdav::backend::lock::DbLockSystem;
    use aster_forge_webdav::{DavLockSystem, DavPath};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "dav-lock-roots",
        "dav-lock-roots@example.com",
        "pass1234",
    )
    .await
    .unwrap();
    let parent = folder::create(&state, user.id, "locked-parent", None)
        .await
        .unwrap();
    let temp_path = write_temp_fixture("member.txt", "member lock content");
    file::store_from_temp(
        &state,
        user.id,
        file::StoreFromTempRequest::new(
            Some(parent.id),
            "member.txt",
            &temp_path,
            "member lock content".len() as i64,
        ),
    )
    .await
    .unwrap();

    let lock_system = DbLockSystem::new(state.clone(), user.id, None);
    let parent_path = DavPath::new("/locked-parent/").unwrap();
    let member_path = DavPath::new("/locked-parent/member.txt").unwrap();
    let parent_lock = lock_system
        .lock(DavLockAcquireRequest {
            path: &parent_path,
            principal: None,
            owner: None,
            timeout: Some(Duration::from_secs(60)),
            shared: false,
            deep: false,
            credentials: DavMutationCredentials::default(),
        })
        .await
        .unwrap()
        .lock;
    let member_lock = lock_system
        .lock(DavLockAcquireRequest {
            path: &member_path,
            principal: None,
            owner: None,
            timeout: Some(Duration::from_secs(60)),
            shared: false,
            deep: false,
            credentials: DavMutationCredentials::default(),
        })
        .await
        .unwrap()
        .lock;

    lock_system
        .check(
            &member_path,
            None,
            false,
            false,
            std::slice::from_ref(&member_lock.token),
        )
        .await
        .unwrap();
    let error = lock_system
        .check(
            &parent_path,
            None,
            false,
            false,
            std::slice::from_ref(&member_lock.token),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        DavLockError::Conflict(lock) if lock.token == parent_lock.token
    ));

    let submitted = [parent_lock.token, member_lock.token];
    for path in [&parent_path, &member_path] {
        lock_system
            .check(path, None, false, false, &submitted)
            .await
            .unwrap();
    }
}

#[actix_web::test]
async fn test_db_lock_system_rejects_unrepresentable_timeout() {
    use aster_drive::db::repository::lock_repo;
    use aster_drive::services::files::file;
    use aster_drive::webdav::backend::lock::DbLockSystem;
    use aster_forge_webdav::{DavLockSystem, DavPath};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "davlocks-timeout",
        "davlocks-timeout@example.com",
        "pass1234",
    )
    .await
    .unwrap();
    let temp_path = write_temp_fixture("timeout.txt", "timeout content");
    file::store_from_temp(
        &state,
        user.id,
        file::StoreFromTempRequest::new(
            None,
            "timeout.txt",
            &temp_path,
            "timeout content".len() as i64,
        ),
    )
    .await
    .unwrap();

    let lock_system = DbLockSystem::new(state.clone(), user.id, None);
    let path = DavPath::new("/timeout.txt").unwrap();
    let result = lock_system
        .lock(DavLockAcquireRequest {
            path: &path,
            principal: Some("tester"),
            owner: None,
            timeout: Some(Duration::from_secs(u64::MAX)),
            shared: false,
            deep: false,
            credentials: DavMutationCredentials::default(),
        })
        .await;

    assert!(
        result.is_err(),
        "unrepresentable lock timeout must be rejected instead of persisted as infinite"
    );
    assert!(
        lock_repo::find_by_path_prefix(state.writer_db(), "/timeout.txt")
            .await
            .unwrap()
            .is_empty(),
        "rejected timeout must not create a persisted lock"
    );
}

#[actix_web::test]
async fn test_db_lock_system_uses_one_canonical_path_for_encoded_special_names() {
    use aster_drive::db::repository::lock_repo;
    use aster_drive::services::{files::file, files::folder};
    use aster_drive::webdav::backend::lock::DbLockSystem;
    use aster_forge_webdav::{DavLockSystem, DavPath};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "dav-special-path",
        "dav-special-path@example.com",
        "pass1234",
    )
    .await
    .unwrap();
    let folder = folder::create(&state, user.id, "目录 空间", None)
        .await
        .unwrap();
    let temp_path = write_temp_fixture("special-name.txt", "special lock content");
    file::store_from_temp(
        &state,
        user.id,
        file::StoreFromTempRequest::new(
            Some(folder.id),
            "# 文件.txt",
            &temp_path,
            "special lock content".len() as i64,
        ),
    )
    .await
    .unwrap();

    let folder_raw = DavPath::new("/目录 空间/").unwrap();
    let folder_encoded = DavPath::new("/%E7%9B%AE%E5%BD%95%20%E7%A9%BA%E9%97%B4/").unwrap();
    let file_raw = DavPath::new("/目录 空间/# 文件.txt").unwrap();
    let file_encoded =
        DavPath::new("/%E7%9B%AE%E5%BD%95%20%E7%A9%BA%E9%97%B4/%23%20%E6%96%87%E4%BB%B6.txt")
            .unwrap();
    assert_eq!(folder_raw, folder_encoded);
    assert_eq!(file_raw, file_encoded);
    assert_eq!(file_raw.as_bytes(), file_raw.as_str().as_bytes());

    let lock_system = DbLockSystem::new(state.clone(), user.id, None);
    let deep_lock = lock_system
        .lock(DavLockAcquireRequest {
            path: &folder_raw,
            principal: Some("tester"),
            owner: None,
            timeout: Some(Duration::from_secs(60)),
            shared: false,
            deep: true,
            credentials: DavMutationCredentials::default(),
        })
        .await
        .unwrap()
        .lock;
    let persisted = lock_repo::find_by_token(state.writer_db(), &deep_lock.token)
        .await
        .unwrap()
        .expect("deep special-name lock should be persisted");
    assert_eq!(persisted.path(), folder_raw.as_str());

    let conflict = lock_system
        .check(&file_encoded, None, false, false, &[])
        .await
        .unwrap_err();
    let DavLockError::Conflict(conflict) = conflict else {
        panic!("missing encoded-path lock token should return the conflicting lock");
    };
    assert_eq!(conflict.token, deep_lock.token);
    lock_system
        .check(
            &file_encoded,
            None,
            false,
            false,
            std::slice::from_ref(&deep_lock.token),
        )
        .await
        .unwrap();
    assert_eq!(lock_system.discover(&file_encoded).await.unwrap().len(), 1);
    lock_system
        .refresh(
            &folder_encoded,
            &deep_lock.token,
            Some(Duration::from_secs(30)),
        )
        .await
        .unwrap();
    lock_system.delete(&folder_encoded).await.unwrap();
    assert!(
        lock_repo::find_by_token(state.writer_db(), &deep_lock.token)
            .await
            .unwrap()
            .is_none(),
        "deleting an encoded collection path must delete its canonical deep lock"
    );

    let file_lock = lock_system
        .lock(DavLockAcquireRequest {
            path: &file_raw,
            principal: Some("tester"),
            owner: None,
            timeout: Some(Duration::from_secs(60)),
            shared: false,
            deep: false,
            credentials: DavMutationCredentials::default(),
        })
        .await
        .unwrap()
        .lock;
    lock_system
        .unlock(&file_encoded, &file_lock.token)
        .await
        .unwrap();
    assert!(
        lock_repo::find_by_token(state.writer_db(), &file_lock.token)
            .await
            .unwrap()
            .is_none(),
        "UNLOCK must match the same canonical lock after URI percent-decoding"
    );
}

#[actix_web::test]
async fn test_db_lock_system_replaces_expired_locks_and_rejects_active_conflicts() {
    use aster_drive::db::repository::lock_repo;
    use aster_drive::services::{files::file, files::lock};
    use aster_drive::webdav::backend::lock::DbLockSystem;
    use aster_drive_model::types::EntityType;
    use aster_forge_webdav::{DavLockSystem, DavPath};
    use chrono::Duration as ChronoDuration;

    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "davexpired", "davexpired@example.com", "pass1234")
            .await
            .unwrap();

    let temp_path = write_temp_fixture("expired.txt", "expired lock content");
    let file = file::store_from_temp(
        &state,
        user.id,
        file::StoreFromTempRequest::new(
            None,
            "expired.txt",
            &temp_path,
            "expired lock content".len() as i64,
        ),
    )
    .await
    .unwrap();

    let expired_lock = lock::lock(
        &state,
        EntityType::File,
        file.id,
        Some(user.id),
        Some(
            aster_drive::services::files::lock::ResourceLockOwnerInfo::Text(
                aster_drive::services::files::lock::TextLockOwnerInfo {
                    value: "expired".to_string(),
                },
            ),
        ),
        Some(ChronoDuration::seconds(-1)),
    )
    .await
    .unwrap();

    let lock_system = DbLockSystem::new(state.clone(), user.id, None);
    let file_path = DavPath::new("/expired.txt").unwrap();

    let replacement = lock_system
        .lock(DavLockAcquireRequest {
            path: &file_path,
            principal: Some("tester"),
            owner: None,
            timeout: Some(Duration::from_secs(60)),
            shared: false,
            deep: false,
            credentials: DavMutationCredentials::default(),
        })
        .await
        .unwrap()
        .lock;
    assert_ne!(replacement.token, expired_lock.token);
    assert!(
        lock_repo::find_by_token(state.writer_db(), &expired_lock.token)
            .await
            .unwrap()
            .is_none()
    );

    assert!(
        lock_repo::find_by_token(state.writer_db(), &replacement.token)
            .await
            .unwrap()
            .is_some()
    );

    let conflict = lock_system
        .lock(DavLockAcquireRequest {
            path: &file_path,
            principal: Some("tester"),
            owner: None,
            timeout: Some(Duration::from_secs(60)),
            shared: false,
            deep: false,
            credentials: DavMutationCredentials::default(),
        })
        .await
        .unwrap_err();
    let DavLockError::Conflict(conflict) = conflict else {
        panic!("active replacement lock should reject conflicting exclusive lock");
    };
    assert_eq!(conflict.token, replacement.token);

    assert!(matches!(
        lock_system
            .unlock(&file_path, "missing-token")
            .await
            .unwrap_err(),
        DavLockError::TokenMismatch
    ));
    let other_path = DavPath::new("/other-expired.txt").unwrap();
    assert!(
        matches!(
            lock_system
                .unlock(&other_path, &replacement.token)
                .await
                .unwrap_err(),
            DavLockError::TokenMismatch
        ),
        "UNLOCK must target the locked resource or a resource covered by a deep lock"
    );
    assert!(
        lock_repo::find_by_token(state.writer_db(), &replacement.token)
            .await
            .unwrap()
            .is_some(),
        "failed UNLOCK on an unrelated path must not delete the lock"
    );

    lock_system
        .unlock(&file_path, &replacement.token)
        .await
        .unwrap();
    assert!(
        lock_repo::find_by_token(state.writer_db(), &replacement.token)
            .await
            .unwrap()
            .is_none()
    );
}

#[actix_web::test]
async fn test_db_lock_system_allows_shared_locks_and_keeps_locked_until_last_unlock() {
    use aster_drive::db::repository::lock_repo;
    use aster_drive::services::files::file;
    use aster_drive::webdav::backend::lock::DbLockSystem;
    use aster_drive_model::types::EntityType;
    use aster_forge_webdav::{DavLockSystem, DavPath};

    let state = common::setup().await;
    let user =
        common::create_test_account(&state, "davshared", "davshared@example.com", "pass1234")
            .await
            .unwrap();

    let temp_path = write_temp_fixture("shared.txt", "shared lock content");
    let file = file::store_from_temp(
        &state,
        user.id,
        file::StoreFromTempRequest::new(
            None,
            "shared.txt",
            &temp_path,
            "shared lock content".len() as i64,
        ),
    )
    .await
    .unwrap();

    let lock_system = DbLockSystem::new(state.clone(), user.id, None);
    let file_path = DavPath::new("/shared.txt").unwrap();

    let first = lock_system
        .lock(DavLockAcquireRequest {
            path: &file_path,
            principal: Some("tester-a"),
            owner: None,
            timeout: Some(Duration::from_secs(60)),
            shared: true,
            deep: false,
            credentials: DavMutationCredentials::default(),
        })
        .await
        .unwrap()
        .lock;
    let second = lock_system
        .lock(DavLockAcquireRequest {
            path: &file_path,
            principal: Some("tester-b"),
            owner: None,
            timeout: Some(Duration::from_secs(60)),
            shared: true,
            deep: false,
            credentials: DavMutationCredentials::default(),
        })
        .await
        .unwrap()
        .lock;
    assert_ne!(first.token, second.token);

    let discovered = lock_system.discover(&file_path).await.unwrap();
    assert_eq!(discovered.len(), 2);
    assert!(discovered.iter().any(|lock| lock.token == first.token));
    assert!(discovered.iter().any(|lock| lock.token == second.token));
    lock_system
        .check(
            &file_path,
            None,
            false,
            false,
            std::slice::from_ref(&first.token),
        )
        .await
        .expect("one valid shared-lock token should satisfy the resource lock root");

    let exclusive_conflict = lock_system
        .lock(DavLockAcquireRequest {
            path: &file_path,
            principal: Some("tester-c"),
            owner: None,
            timeout: Some(Duration::from_secs(60)),
            shared: false,
            deep: false,
            credentials: DavMutationCredentials::default(),
        })
        .await
        .unwrap_err();
    let DavLockError::Conflict(exclusive_conflict) = exclusive_conflict else {
        panic!("shared locks should reject conflicting exclusive lock");
    };
    assert!(
        [first.token.as_str(), second.token.as_str()].contains(&exclusive_conflict.token.as_str())
    );

    lock_system.unlock(&file_path, &first.token).await.unwrap();
    assert_eq!(
        lock_repo::find_all_by_entity(state.writer_db(), EntityType::File, file.id)
            .await
            .unwrap()
            .len(),
        1
    );

    lock_system.unlock(&file_path, &second.token).await.unwrap();
}

#[actix_web::test]
async fn test_db_lock_system_exclusive_lock_blocks_shared_lock() {
    use aster_drive::services::files::file;
    use aster_drive::webdav::backend::lock::DbLockSystem;
    use aster_forge_webdav::{DavLockSystem, DavPath};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "davexclusive",
        "davexclusive@example.com",
        "pass1234",
    )
    .await
    .unwrap();

    let temp_path = write_temp_fixture("exclusive.txt", "exclusive lock content");
    file::store_from_temp(
        &state,
        user.id,
        file::StoreFromTempRequest::new(
            None,
            "exclusive.txt",
            &temp_path,
            "exclusive lock content".len() as i64,
        ),
    )
    .await
    .unwrap();

    let lock_system = DbLockSystem::new(state.clone(), user.id, None);
    let file_path = DavPath::new("/exclusive.txt").unwrap();

    let exclusive = lock_system
        .lock(DavLockAcquireRequest {
            path: &file_path,
            principal: Some("tester-a"),
            owner: None,
            timeout: Some(Duration::from_secs(60)),
            shared: false,
            deep: false,
            credentials: DavMutationCredentials::default(),
        })
        .await
        .unwrap()
        .lock;
    let shared_conflict = lock_system
        .lock(DavLockAcquireRequest {
            path: &file_path,
            principal: Some("tester-b"),
            owner: None,
            timeout: Some(Duration::from_secs(60)),
            shared: true,
            deep: false,
            credentials: DavMutationCredentials::default(),
        })
        .await
        .unwrap_err();
    let DavLockError::Conflict(shared_conflict) = shared_conflict else {
        panic!("exclusive lock should reject shared lock with a conflict");
    };
    assert_eq!(shared_conflict.token, exclusive.token);
}

#[actix_web::test]
async fn test_db_lock_system_propagates_backend_failures_from_every_query_port() {
    use aster_drive::webdav::backend::lock::DbLockSystem;
    use aster_forge_webdav::{DavBackendErrorKind, DavLockSystem, DavPath};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "dav-lock-fail",
        "dav-lock-backend-failure@example.com",
        "pass1234",
    )
    .await
    .unwrap();
    let lock_system = DbLockSystem::new(state.clone(), user.id, None);
    state.writer_db().clone().close().await.unwrap();

    let path = DavPath::new("/backend-failure.txt").unwrap();
    assert!(matches!(
        lock_system.check(&path, None, false, false, &[]).await,
        Err(DavLockError::Backend)
    ));
    assert!(matches!(
        lock_system.discover(&path).await,
        Err(error) if error.kind == DavBackendErrorKind::Internal
    ));
    assert!(matches!(
        lock_system.discover_many(std::slice::from_ref(&path)).await,
        Err(error) if error.kind == DavBackendErrorKind::Internal
    ));
    assert!(matches!(
        lock_system.conflicting_locks(&path, false).await,
        Err(error) if error.kind == DavBackendErrorKind::Internal
    ));
}

#[actix_web::test]
async fn test_db_lock_system_distinguishes_mount_root_and_missing_targets() {
    use aster_drive::db::repository::{file_repo, lock_repo};
    use aster_drive::webdav::backend::lock::DbLockSystem;
    use aster_drive_model::types::LockRootKind;
    use aster_forge_webdav::{DavLockSystem, DavPath};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "dav-lock-targets",
        "dav-lock-targets@example.com",
        "pass1234",
    )
    .await
    .unwrap();
    let lock_system = DbLockSystem::new(state.clone(), user.id, None);

    let missing = lock_system
        .lock(DavLockAcquireRequest {
            path: &DavPath::new("/missing.txt").unwrap(),
            principal: None,
            owner: None,
            timeout: Some(Duration::from_secs(60)),
            shared: false,
            deep: false,
            credentials: DavMutationCredentials::default(),
        })
        .await
        .expect("a missing non-collection target should become a lock-null resource");
    assert!(!missing.resource_existed);
    let missing_file =
        file_repo::find_by_name_in_folder(state.writer_db(), user.id, None, "missing.txt")
            .await
            .unwrap()
            .expect("LOCK should create the missing empty file");
    assert_eq!(missing_file.size, 0);
    let missing_stored = lock_repo::find_by_token(state.writer_db(), &missing.lock.token)
        .await
        .unwrap()
        .expect("lock-null lock should be persisted with the file");
    assert_eq!(missing_stored.root_file_id, Some(missing_file.id));
    lock_system
        .unlock(&DavPath::new("/missing.txt").unwrap(), &missing.lock.token)
        .await
        .expect("lock-null resource should support UNLOCK");

    let root_path = DavPath::new("/").unwrap();
    let root_lock = lock_system
        .lock(DavLockAcquireRequest {
            path: &root_path,
            principal: None,
            owner: None,
            timeout: Some(Duration::from_secs(60)),
            shared: false,
            deep: true,
            credentials: DavMutationCredentials::default(),
        })
        .await
        .expect("virtual mount root should support LOCK")
        .lock;
    let stored = lock_repo::find_by_token(state.writer_db(), &root_lock.token)
        .await
        .unwrap()
        .expect("root lock should be persisted");
    assert_eq!(stored.root_kind, LockRootKind::WorkspaceRoot);
    assert!(stored.root_folder_id.is_none());
    assert!(stored.root_file_id.is_none());

    lock_system
        .unlock(&root_path, &root_lock.token)
        .await
        .expect("virtual mount root should support UNLOCK");
}

#[actix_web::test]
async fn test_db_lock_system_scopes_same_path_to_workspace_namespace() {
    use aster_drive::services::files::file;
    use aster_drive::webdav::backend::lock::DbLockSystem;
    use aster_forge_webdav::{DavLockSystem, DavPath};

    let state = common::setup().await;
    let first_user = common::create_test_account(
        &state,
        "dav-lock-scope-a",
        "dav-lock-scope-a@example.com",
        "pass1234",
    )
    .await
    .unwrap();
    let second_user = common::create_test_account(
        &state,
        "dav-lock-scope-b",
        "dav-lock-scope-b@example.com",
        "pass1234",
    )
    .await
    .unwrap();
    for (user, fixture_name) in [(&first_user, "scope-a.txt"), (&second_user, "scope-b.txt")] {
        let temp_path = write_temp_fixture(fixture_name, "workspace scoped lock");
        file::store_from_temp(
            &state,
            user.id,
            file::StoreFromTempRequest::new(
                None,
                "same-path.txt",
                &temp_path,
                "workspace scoped lock".len() as i64,
            ),
        )
        .await
        .unwrap();
    }

    let path = DavPath::new("/same-path.txt").unwrap();
    let first_system = DbLockSystem::new(state.clone(), first_user.id, None);
    let second_system = DbLockSystem::new(state.clone(), second_user.id, None);
    let first_lock = first_system
        .lock(DavLockAcquireRequest {
            path: &path,
            principal: Some("first-user"),
            owner: None,
            timeout: Some(Duration::from_secs(60)),
            shared: false,
            deep: false,
            credentials: DavMutationCredentials::default(),
        })
        .await
        .unwrap()
        .lock;

    second_system
        .check(&path, None, false, false, &[])
        .await
        .expect("a lock in another workspace must not conflict");
    assert!(second_system.discover(&path).await.unwrap().is_empty());
    let second_lock = second_system
        .lock(DavLockAcquireRequest {
            path: &path,
            principal: Some("second-user"),
            owner: None,
            timeout: Some(Duration::from_secs(60)),
            shared: false,
            deep: false,
            credentials: DavMutationCredentials::default(),
        })
        .await
        .expect("same URI in another workspace should be independently lockable")
        .lock;
    assert_ne!(first_lock.token, second_lock.token);
}
