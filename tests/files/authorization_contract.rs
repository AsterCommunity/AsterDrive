//! 授权契约回归：资源内容与集合成员变化是独立的权限/锁约束。
//! 特意使用 Resource-depth 父集合锁，避免 Infinity 祖先锁掩盖漏检。

use crate::common;
use aster_drive::services::files::{file, folder, lock, trash};
use aster_drive_model::types::{EntityType, LockDepth, LockMode, LockOrigin};
use aster_forge_api::NullablePatch;

#[actix_web::test]
async fn personal_root_listing_rechecks_actor_lifecycle_without_http_middleware() {
    use aster_drive_model::types::UserStatus;
    use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "listing-auth",
        "listing-auth@example.test",
        "password123",
    )
    .await
    .unwrap();
    let query = serde_json::from_value(serde_json::json!({})).unwrap();
    let params = folder::FolderListParams::from(&query);
    folder::list(&state, user.id, None, &params).await.unwrap();

    let user_id = user.id;
    let mut disabled =
        aster_drive::db::repository::user_repo::find_by_id(state.writer_db(), user_id)
            .await
            .unwrap()
            .into_active_model();
    disabled.status = Set(UserStatus::Disabled);
    disabled.update(state.writer_db()).await.unwrap();
    let error = match folder::list(&state, user_id, None, &params).await {
        Err(error) => error,
        Ok(_) => panic!("a disabled actor cannot read personal root through a direct service call"),
    };
    assert_eq!(
        error.api_error_code(),
        aster_drive::api::api_error_code::ApiErrorCode::AuthAccountDisabled
    );
}

async fn store_file(
    state: &aster_drive::runtime::PrimaryAppState,
    user_id: i64,
    folder_id: Option<i64>,
    name: &str,
) -> aster_drive::services::workspace::models::FileInfo {
    let temporary = std::path::Path::new(&state.config.server.temp_dir)
        .join(format!("authz-{}.txt", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(temporary.parent().unwrap()).unwrap();
    std::fs::write(&temporary, b"authorization fixture").unwrap();
    let stored = file::store_from_temp(
        state,
        user_id,
        file::StoreFromTempRequest::new(folder_id, name, temporary.to_str().unwrap(), 21),
    )
    .await
    .unwrap();
    match std::fs::remove_file(temporary) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("{error}"),
    }
    stored
}

#[actix_web::test]
async fn collection_resource_lock_blocks_copy_delete_restore_and_member_rename() {
    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "collection-auth",
        "collection-auth@example.test",
        "password123",
    )
    .await
    .unwrap();
    let parent = folder::create(&state, user.id, "protected", None)
        .await
        .unwrap();
    let child = folder::create(&state, user.id, "child", Some(parent.id))
        .await
        .unwrap();
    let source = store_file(&state, user.id, None, "source.txt").await;
    let live = store_file(&state, user.id, Some(parent.id), "live.txt").await;
    let deleted = store_file(&state, user.id, Some(parent.id), "deleted.txt").await;
    file::delete(&state, deleted.id, user.id).await.unwrap();
    let parent_lock = lock::acquire(
        &state,
        lock::LockTarget {
            workspace: lock::LockWorkspace::Personal { user_id: user.id },
            root: lock::LockRoot::Folder {
                folder_id: parent.id,
            },
            depth: LockDepth::Resource,
        },
        LockMode::Exclusive,
        LockOrigin::Product,
        Some(user.id),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    let before = aster_drive::db::repository::file_repo::find_by_id(state.writer_db(), live.id)
        .await
        .unwrap();
    let copy_error = file::copy_file(&state, source.id, user.id, Some(parent.id))
        .await
        .err()
        .unwrap();
    assert!(matches!(
        copy_error,
        aster_drive::errors::AsterError::ResourceLocked(_)
    ));
    let delete_error = file::delete(&state, live.id, user.id).await.err().unwrap();
    assert!(matches!(
        delete_error,
        aster_drive::errors::AsterError::ResourceLocked(_)
    ));
    let restore_error = trash::restore_file(&state, deleted.id, user.id)
        .await
        .err()
        .unwrap();
    assert!(matches!(
        restore_error,
        aster_drive::errors::AsterError::ResourceLocked(_)
    ));
    let rename_error = folder::update(
        &state,
        child.id,
        user.id,
        Some("renamed".into()),
        NullablePatch::Absent,
        NullablePatch::Absent,
    )
    .await
    .err()
    .unwrap();
    assert!(matches!(
        rename_error,
        aster_drive::errors::AsterError::ResourceLocked(_)
    ));
    assert_eq!(
        aster_drive::db::repository::file_repo::find_by_id(state.writer_db(), live.id)
            .await
            .unwrap(),
        before
    );
    assert!(
        aster_drive::db::repository::file_repo::find_by_id(state.writer_db(), deleted.id)
            .await
            .unwrap()
            .deleted_at
            .is_some()
    );
    assert_eq!(
        aster_drive::db::repository::folder_repo::find_by_id(state.writer_db(), child.id)
            .await
            .unwrap()
            .name,
        "child"
    );
    // 正例保证补齐集合约束没有把正常操作永久禁掉。
    lock::unlock_by_token(&state, &parent_lock.token)
        .await
        .unwrap();
    file::copy_file(&state, source.id, user.id, Some(parent.id))
        .await
        .unwrap();
    trash::restore_file(&state, deleted.id, user.id)
        .await
        .unwrap();
    file::delete(&state, live.id, user.id).await.unwrap();
}

#[actix_web::test]
async fn subtree_move_checks_descendant_canonical_locks_with_stale_presentation_path() {
    use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};
    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "subtree-auth",
        "subtree-auth@example.test",
        "password123",
    )
    .await
    .unwrap();
    let parent = folder::create(&state, user.id, "old-name", None)
        .await
        .unwrap();
    let nested = folder::create(&state, user.id, "nested", Some(parent.id))
        .await
        .unwrap();
    let destination = folder::create(&state, user.id, "destination", None)
        .await
        .unwrap();
    let child = store_file(&state, user.id, Some(nested.id), "locked.txt").await;
    let descendant_lock = lock::lock(
        &state,
        EntityType::File,
        child.id,
        Some(user.id),
        None,
        None,
    )
    .await
    .unwrap();
    // 模拟升级前遗留的 stale projection。权限必须读取当前 ancestry，不能从这段
    // 历史呈现路径推断根与后代关系。
    let current =
        aster_drive::db::repository::folder_repo::find_by_id(state.writer_db(), parent.id)
            .await
            .unwrap();
    let mut active = current.into_active_model();
    active.name = Set("current-name".into());
    active.update(state.writer_db()).await.unwrap();
    let error = folder::move_folder(&state, parent.id, user.id, Some(destination.id))
        .await
        .err()
        .unwrap();
    assert!(matches!(
        error,
        aster_drive::errors::AsterError::ResourceLocked(_)
    ));
    assert_eq!(
        aster_drive::db::repository::folder_repo::find_by_id(state.writer_db(), parent.id)
            .await
            .unwrap()
            .parent_id,
        None
    );
    lock::unlock_by_token(&state, &descendant_lock.token)
        .await
        .unwrap();
    folder::move_folder(&state, parent.id, user.id, Some(destination.id))
        .await
        .unwrap();
}
