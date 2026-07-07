//! 集成测试：`webdav_lock_system`。

#[macro_use]
mod common;
use aster_drive::runtime::SharedRuntimeState;
use aster_drive::webdav::dav::DavLockError;

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
    use aster_drive::db::repository::{folder_repo, lock_repo};
    use aster_drive::services::{auth::local, files::file, files::folder};
    use aster_drive::webdav::dav::{DavLockSystem, DavPath};
    use aster_drive::webdav::db_lock_system::DbLockSystem;
    use xmltree::Element;

    let state = common::setup().await;
    let user = local::register(&state, "davlocks", "davlocks@example.com", "pass1234")
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

    let lock_system = DbLockSystem::new(state.writer_db().clone(), user.id, None);
    let folder_path = DavPath::new("/projects/").unwrap();
    let child_path = DavPath::new("/projects/docs/note.txt").unwrap();
    let owner = Element::parse(Cursor::new(
        br#"<D:owner xmlns:D="DAV:"><D:href>tester</D:href></D:owner>"#,
    ))
    .unwrap();

    let lock = lock_system
        .lock(
            &folder_path,
            Some("tester"),
            Some(&owner),
            Some(Duration::from_secs(120)),
            false,
            true,
        )
        .await
        .unwrap();
    assert!(lock.deep);
    assert_eq!(lock.principal.as_deref(), Some("tester"));
    assert!(!lock.token.is_empty());

    let locked_folder = folder_repo::find_by_id(state.writer_db(), projects.id)
        .await
        .unwrap();
    assert!(locked_folder.is_locked);

    let conflict = lock_system
        .check(&child_path, None, false, false, &[])
        .await
        .unwrap_err();
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

    let discovered = lock_system.discover(&child_path).await;
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
    let unlocked_folder = folder_repo::find_by_id(state.writer_db(), projects.id)
        .await
        .unwrap();
    assert!(!unlocked_folder.is_locked);
}

#[actix_web::test]
async fn test_db_lock_system_rejects_unrepresentable_timeout() {
    use aster_drive::db::repository::lock_repo;
    use aster_drive::services::{auth::local, files::file};
    use aster_drive::webdav::dav::{DavLockSystem, DavPath};
    use aster_drive::webdav::db_lock_system::DbLockSystem;

    let state = common::setup().await;
    let user = local::register(
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

    let lock_system = DbLockSystem::new(state.writer_db().clone(), user.id, None);
    let path = DavPath::new("/timeout.txt").unwrap();
    let result = lock_system
        .lock(
            &path,
            Some("tester"),
            None,
            Some(Duration::from_secs(u64::MAX)),
            false,
            false,
        )
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
async fn test_db_lock_system_replaces_expired_locks_and_rejects_active_conflicts() {
    use aster_drive::db::repository::{file_repo, lock_repo};
    use aster_drive::services::{auth::local, files::file, lock_service};
    use aster_drive::types::EntityType;
    use aster_drive::webdav::dav::{DavLockSystem, DavPath};
    use aster_drive::webdav::db_lock_system::DbLockSystem;
    use chrono::Duration as ChronoDuration;

    let state = common::setup().await;
    let user = local::register(&state, "davexpired", "davexpired@example.com", "pass1234")
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

    let expired_lock = lock_service::lock(
        &state,
        EntityType::File,
        file.id,
        Some(user.id),
        Some(
            aster_drive::services::lock_service::ResourceLockOwnerInfo::Text(
                aster_drive::services::lock_service::TextLockOwnerInfo {
                    value: "expired".to_string(),
                },
            ),
        ),
        Some(ChronoDuration::seconds(-1)),
    )
    .await
    .unwrap();

    let lock_system = DbLockSystem::new(state.writer_db().clone(), user.id, None);
    let file_path = DavPath::new("/expired.txt").unwrap();

    let replacement = lock_system
        .lock(
            &file_path,
            Some("tester"),
            None,
            Some(Duration::from_secs(60)),
            false,
            false,
        )
        .await
        .unwrap();
    assert_ne!(replacement.token, expired_lock.token);
    assert!(
        lock_repo::find_by_token(state.writer_db(), &expired_lock.token)
            .await
            .unwrap()
            .is_none()
    );

    let locked_file = file_repo::find_by_id(state.writer_db(), file.id)
        .await
        .unwrap();
    assert!(locked_file.is_locked);

    let conflict = lock_system
        .lock(
            &file_path,
            Some("tester"),
            None,
            Some(Duration::from_secs(60)),
            false,
            false,
        )
        .await
        .unwrap_err();
    let DavLockError::Conflict(conflict) = conflict else {
        panic!("active replacement lock should reject conflicting exclusive lock");
    };
    assert_eq!(conflict.token, replacement.token);

    assert!(
        lock_system
            .unlock(&file_path, "missing-token")
            .await
            .is_err()
    );
    let other_path = DavPath::new("/other-expired.txt").unwrap();
    assert!(
        lock_system
            .unlock(&other_path, &replacement.token)
            .await
            .is_err(),
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
    let unlocked_file = file_repo::find_by_id(state.writer_db(), file.id)
        .await
        .unwrap();
    assert!(!unlocked_file.is_locked);
}

#[actix_web::test]
async fn test_db_lock_system_allows_shared_locks_and_keeps_locked_until_last_unlock() {
    use aster_drive::db::repository::{file_repo, lock_repo};
    use aster_drive::services::{auth::local, files::file};
    use aster_drive::types::EntityType;
    use aster_drive::webdav::dav::{DavLockSystem, DavPath};
    use aster_drive::webdav::db_lock_system::DbLockSystem;

    let state = common::setup().await;
    let user = local::register(&state, "davshared", "davshared@example.com", "pass1234")
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

    let lock_system = DbLockSystem::new(state.writer_db().clone(), user.id, None);
    let file_path = DavPath::new("/shared.txt").unwrap();

    let first = lock_system
        .lock(
            &file_path,
            Some("tester-a"),
            None,
            Some(Duration::from_secs(60)),
            true,
            false,
        )
        .await
        .unwrap();
    let second = lock_system
        .lock(
            &file_path,
            Some("tester-b"),
            None,
            Some(Duration::from_secs(60)),
            true,
            false,
        )
        .await
        .unwrap();
    assert_ne!(first.token, second.token);

    let discovered = lock_system.discover(&file_path).await;
    assert_eq!(discovered.len(), 2);
    assert!(discovered.iter().any(|lock| lock.token == first.token));
    assert!(discovered.iter().any(|lock| lock.token == second.token));

    let exclusive_conflict = lock_system
        .lock(
            &file_path,
            Some("tester-c"),
            None,
            Some(Duration::from_secs(60)),
            false,
            false,
        )
        .await
        .unwrap_err();
    let DavLockError::Conflict(exclusive_conflict) = exclusive_conflict else {
        panic!("shared locks should reject conflicting exclusive lock");
    };
    assert!(
        [first.token.as_str(), second.token.as_str()].contains(&exclusive_conflict.token.as_str())
    );

    lock_system.unlock(&file_path, &first.token).await.unwrap();
    let still_locked = file_repo::find_by_id(state.writer_db(), file.id)
        .await
        .unwrap();
    assert!(still_locked.is_locked);
    assert_eq!(
        lock_repo::find_all_by_entity(state.writer_db(), EntityType::File, file.id)
            .await
            .unwrap()
            .len(),
        1
    );

    lock_system.unlock(&file_path, &second.token).await.unwrap();
    let unlocked = file_repo::find_by_id(state.writer_db(), file.id)
        .await
        .unwrap();
    assert!(!unlocked.is_locked);
}

#[actix_web::test]
async fn test_db_lock_system_exclusive_lock_blocks_shared_lock() {
    use aster_drive::services::{auth::local, files::file};
    use aster_drive::webdav::dav::{DavLockSystem, DavPath};
    use aster_drive::webdav::db_lock_system::DbLockSystem;

    let state = common::setup().await;
    let user = local::register(
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

    let lock_system = DbLockSystem::new(state.writer_db().clone(), user.id, None);
    let file_path = DavPath::new("/exclusive.txt").unwrap();

    let exclusive = lock_system
        .lock(
            &file_path,
            Some("tester-a"),
            None,
            Some(Duration::from_secs(60)),
            false,
            false,
        )
        .await
        .unwrap();
    let shared_conflict = lock_system
        .lock(
            &file_path,
            Some("tester-b"),
            None,
            Some(Duration::from_secs(60)),
            true,
            false,
        )
        .await
        .unwrap_err();
    let DavLockError::Conflict(shared_conflict) = shared_conflict else {
        panic!("exclusive lock should reject shared lock with a conflict");
    };
    assert_eq!(shared_conflict.token, exclusive.token);
}
