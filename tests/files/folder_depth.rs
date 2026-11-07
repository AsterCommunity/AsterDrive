//! The supported hierarchy depth uses the virtual workspace root as depth zero.

use crate::common;
use actix_web::test;
use aster_drive::db::repository::{folder_repo, user_repo};
use aster_drive::runtime::PrimaryAppState;
use aster_drive::services::files::folder as folders;
use aster_drive_model::entities::folder;
use aster_drive_model::types::folder::MAX_FOLDER_DEPTH;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};
use serde_json::Value;

async fn insert_chain(
    state: &PrimaryAppState,
    user_id: i64,
    team_id: Option<i64>,
    start_id: i64,
    depth: usize,
) -> Vec<i64> {
    let ids = (0..depth)
        .map(|index| start_id + index as i64)
        .collect::<Vec<_>>();
    let now = Utc::now();
    for chunk in ids.chunks(250) {
        let models = chunk
            .iter()
            .map(|&id| folder::ActiveModel {
                id: Set(id),
                name: Set(format!("depth-{id}")),
                parent_id: Set((id != start_id).then_some(id - 1)),
                owner_user_id: Set(team_id.is_none().then_some(user_id)),
                team_id: Set(team_id),
                created_by_user_id: Set(Some(user_id)),
                created_by_username: Set("testuser".to_string()),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            })
            .collect();
        folder_repo::create_many(state.writer_db(), models)
            .await
            .expect("insert hierarchy fixture");
    }
    ids
}

#[actix_web::test]
async fn folder_depth_4096_allows_exact_boundary_and_rejects_new_levels_atomically() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .unwrap();
    let ids = insert_chain(&state, user.id, None, 50_000, MAX_FOLDER_DEPTH - 1).await;
    let parent_id = *ids.last().unwrap();
    let leaf = folders::create(&state, user.id, "boundary", Some(parent_id))
        .await
        .expect("depth 4096 must remain usable");
    let ancestors = folder_repo::find_ancestors(state.writer_db(), user.id, leaf.id)
        .await
        .unwrap();
    assert_eq!(ancestors.len(), MAX_FOLDER_DEPTH);
    assert_eq!(ancestors.first().unwrap().0, ids[0]);
    assert_eq!(ancestors.last().unwrap().0, leaf.id);
    let error = folders::create(&state, user.id, "too-deep", Some(leaf.id))
        .await
        .unwrap_err();
    assert!(error.message().contains("4096"));

    let before = folder::Entity::find()
        .count(state.writer_db())
        .await
        .unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/files/new")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(
                serde_json::json!({"name":"ignored.txt", "folder_id":leaf.id,
            "relative_path":"must-not-exist/file.txt"}),
            )
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 400);
    assert_eq!(
        folder::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        before
    );

    // Existing corrupt/old data must be rejected rather than silently truncated.
    let over = folder::ActiveModel {
        name: Set("existing-over-limit".to_string()),
        parent_id: Set(Some(leaf.id)),
        owner_user_id: Set(Some(user.id)),
        created_by_user_id: Set(Some(user.id)),
        created_by_username: Set(user.username.clone()),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .unwrap();
    assert!(
        folder_repo::find_ancestors(state.writer_db(), user.id, over.id)
            .await
            .is_err()
    );
}

#[actix_web::test]
async fn folder_depth_4096_checks_subtree_height_for_copy_move_and_batch_move() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .unwrap();
    let ids = insert_chain(&state, user.id, None, 50_000, MAX_FOLDER_DEPTH - 1).await;
    let too_deep_parent = *ids.last().unwrap();
    let exact_parent = ids[ids.len() - 2];
    let source = folders::create(&state, user.id, "source", None)
        .await
        .unwrap();
    let child = folders::create(&state, user.id, "child", Some(source.id))
        .await
        .unwrap();
    let other = folders::create(&state, user.id, "other", None)
        .await
        .unwrap();
    let before = folder::Entity::find()
        .count(state.writer_db())
        .await
        .unwrap();

    assert!(
        folders::copy_folder(&state, source.id, user.id, Some(too_deep_parent))
            .await
            .is_err()
    );
    assert_eq!(
        folder::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        before,
        "reject the entire copy before creating its root"
    );
    assert!(
        folders::move_folder(&state, source.id, user.id, Some(too_deep_parent))
            .await
            .is_err()
    );
    assert_eq!(
        folder_repo::find_by_id(state.writer_db(), source.id)
            .await
            .unwrap()
            .parent_id,
        None
    );

    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/batch/move")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(
                serde_json::json!({"file_ids":[],"folder_ids":[source.id,other.id],
            "target_folder_id":too_deep_parent}),
            )
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["data"]["succeeded"], 1);
    assert_eq!(body["data"]["failed"], 1);
    assert_eq!(body["data"]["errors"][0]["entity_id"], source.id);
    assert_eq!(
        folder_repo::find_ancestors(state.writer_db(), user.id, other.id)
            .await
            .unwrap()
            .len(),
        MAX_FOLDER_DEPTH
    );

    folders::move_folder(&state, source.id, user.id, Some(exact_parent))
        .await
        .unwrap();
    assert_eq!(
        folder_repo::find_ancestors(state.writer_db(), user.id, child.id)
            .await
            .unwrap()
            .len(),
        MAX_FOLDER_DEPTH
    );
    let copied = folders::copy_folder(&state, source.id, user.id, Some(exact_parent))
        .await
        .unwrap();
    let copied_child =
        folder_repo::find_by_name_in_parent(state.writer_db(), user.id, Some(copied.id), "child")
            .await
            .unwrap()
            .unwrap();
    assert_eq!(
        folder_repo::find_ancestors(state.writer_db(), user.id, copied_child.id)
            .await
            .unwrap()
            .len(),
        MAX_FOLDER_DEPTH
    );
}

#[actix_web::test]
async fn folder_depth_4096_team_boundary_and_cycle_reads_are_bounded() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/teams")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({"name":"Deep team"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 201);
    let body: Value = test::read_body_json(response).await;
    let team_id = body["data"]["id"].as_i64().unwrap();
    let ids = insert_chain(&state, user.id, Some(team_id), 50_000, MAX_FOLDER_DEPTH).await;
    let leaf_id = *ids.last().unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/v1/teams/{team_id}/folders/{leaf_id}/ancestors"
            ))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["data"].as_array().unwrap().len(), MAX_FOLDER_DEPTH);
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/teams/{team_id}/folders"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({"name":"too-deep", "parent_id":leaf_id}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 400);

    let cycle = folders::create(&state, user.id, "cycle", None)
        .await
        .unwrap();
    folder::Entity::update_many()
        .col_expr(
            folder::Column::ParentId,
            sea_orm::sea_query::Expr::value(cycle.id),
        )
        .filter(folder::Column::Id.eq(cycle.id))
        .exec(state.writer_db())
        .await
        .unwrap();
    assert!(
        folder_repo::find_ancestors(state.writer_db(), user.id, cycle.id)
            .await
            .is_err()
    );
    assert!(
        folders::build_folder_paths(state.writer_db(), &[cycle.id])
            .await
            .is_err()
    );
}

#[actix_web::test]
async fn folder_depth_4096_restore_checks_the_final_parent_before_writing() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .unwrap();
    let ids = insert_chain(&state, user.id, None, 50_000, MAX_FOLDER_DEPTH - 1).await;
    let parent = *ids.last().unwrap();
    let root = folders::create(&state, user.id, "trashed", None)
        .await
        .unwrap();
    let child = folders::create(&state, user.id, "child", Some(root.id))
        .await
        .unwrap();
    folder::Entity::update_many()
        .col_expr(
            folder::Column::DeletedAt,
            sea_orm::sea_query::Expr::value(Some(Utc::now())),
        )
        .filter(folder::Column::Id.is_in([root.id, child.id]))
        .exec(state.writer_db())
        .await
        .unwrap();
    let mut active: folder::ActiveModel = folder_repo::find_by_id(state.writer_db(), root.id)
        .await
        .unwrap()
        .into();
    active.parent_id = Set(Some(parent));
    active.update(state.writer_db()).await.unwrap();

    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/trash/folder/{}/restore", root.id))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request(),
    )
    .await;
    assert_eq!(
        response.status(),
        400,
        "restoring two levels under depth 4095 must fail"
    );
    for id in [root.id, child.id] {
        assert!(
            folder_repo::find_by_id(state.writer_db(), id)
                .await
                .unwrap()
                .deleted_at
                .is_some()
        );
    }

    let mut active: folder::ActiveModel = folder_repo::find_by_id(state.writer_db(), root.id)
        .await
        .unwrap()
        .into();
    active.parent_id = Set(Some(ids[ids.len() - 2]));
    active.update(state.writer_db()).await.unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/trash/folder/{}/restore", root.id))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    assert_eq!(
        folder_repo::find_ancestors(state.writer_db(), user.id, child.id)
            .await
            .unwrap()
            .len(),
        MAX_FOLDER_DEPTH
    );
}

#[actix_web::test]
async fn folder_depth_4096_existing_deeper_tree_can_still_be_deleted() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .unwrap();
    let ids = insert_chain(&state, user.id, None, 50_000, MAX_FOLDER_DEPTH + 2).await;
    // A sibling encountered after returning from the deep branch proves that
    // switching to the bounded parent-link cursor does not lose cached siblings.
    let sibling = folder::ActiveModel {
        name: Set("cleanup-sibling".to_string()),
        parent_id: Set(Some(ids[0])),
        owner_user_id: Set(Some(user.id)),
        created_by_user_id: Set(Some(user.id)),
        created_by_username: Set(user.username.clone()),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::delete()
            .uri(&format!("/api/v1/folders/{}", ids[0]))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 202);
    let stats = aster_drive::services::task::drain(&state).await.unwrap();
    assert_eq!(stats.failed, 0);
    assert_eq!(stats.succeeded, 1);
    for id in [ids[0], *ids.last().unwrap(), sibling.id] {
        assert!(
            folder_repo::find_by_id(state.writer_db(), id)
                .await
                .unwrap()
                .deleted_at
                .is_some()
        );
    }
    let remaining = folder::Entity::find()
        .filter(folder::Column::DeletedAt.is_null())
        .count(state.writer_db())
        .await
        .unwrap();
    assert_eq!(remaining, 0);
    aster_drive::services::files::trash::purge_folder(&state, ids[0], user.id)
        .await
        .expect("legacy over-depth tree must remain permanently removable");
    assert_eq!(
        folder::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        0
    );
}
