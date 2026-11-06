//! Root writes must authorize the actor independently of storage-placement lookup.
use crate::common;
use actix_web::test;
use aster_drive::services::workspace::team;
use aster_drive_model::entities::{file, file_blob, folder, upload_session};
use sea_orm::{EntityTrait, PaginatorTrait};
use serde_json::{Value, json};

/// Count files, blobs, folders and sessions to detect side effects from rejected team writes.
async fn stored_resources(db: &sea_orm::DatabaseConnection) -> [u64; 4] {
    [
        file::Entity::find().count(db).await.unwrap(),
        file_blob::Entity::find().count(db).await.unwrap(),
        folder::Entity::find().count(db).await.unwrap(),
        upload_session::Entity::find().count(db).await.unwrap(),
    ]
}

/// Resource ownership alone must never authorize another team's metadata mutation.
#[actix_web::test]
async fn team_folder_icon_rejects_nonmembers_and_removed_members_without_mutation() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (owner_token, _) = register_and_login!(app);
    let owner =
        aster_drive::db::repository::user_repo::find_by_username(state.writer_db(), "testuser")
            .await
            .unwrap()
            .unwrap();
    let outside_id = admin_create_user!(
        app,
        owner_token,
        "iconoutside",
        "iconoutside@example.test",
        "password123"
    );
    let (outside_token, _) = login_user!(app, "iconoutside", "password123");
    let target = team::create_team(
        &state,
        owner.id,
        team::CreateTeamInput {
            name: "Icon authorization".into(),
            description: None,
        },
    )
    .await
    .unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/teams/{}/folders", target.id))
            .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
            .insert_header(common::csrf_header_for(&owner_token))
            .set_json(json!({"name":"private-folder"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 201);
    let body: Value = test::read_body_json(response).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();
    for removed in [false, true] {
        if removed {
            team::add_member(
                &state,
                target.id,
                owner.id,
                team::AddTeamMemberInput {
                    user_id: Some(outside_id),
                    identifier: None,
                    role: aster_drive_model::types::TeamMemberRole::Member,
                },
            )
            .await
            .unwrap();
            let response = test::call_service(
                &app,
                test::TestRequest::put()
                    .uri(&format!(
                        "/api/v1/teams/{}/folders/{folder_id}/icon",
                        target.id
                    ))
                    .insert_header(("Cookie", common::access_cookie_header(&outside_token)))
                    .insert_header(common::csrf_header_for(&outside_token))
                    .set_json(json!({"kind":"builtin","key":"documents"}))
                    .to_request(),
            )
            .await;
            assert_eq!(
                response.status(),
                200,
                "authorized members retain metadata access"
            );
            team::remove_member(&state, target.id, owner.id, outside_id)
                .await
                .unwrap();
        }
        let before =
            aster_drive::db::repository::folder_repo::find_by_id(state.writer_db(), folder_id)
                .await
                .unwrap();
        let response = test::call_service(
            &app,
            test::TestRequest::put()
                .uri(&format!(
                    "/api/v1/teams/{}/folders/{folder_id}/icon",
                    target.id
                ))
                .insert_header(("Cookie", common::access_cookie_header(&outside_token)))
                .insert_header(common::csrf_header_for(&outside_token))
                .set_json(json!({"kind":"emoji","value":"📚"}))
                .to_request(),
        )
        .await;
        assert_eq!(
            response.status(),
            403,
            "nonmember/removed member must be denied"
        );
        let body: Value = test::read_body_json(response).await;
        assert_eq!(body["code"], "team.not_member");
        let after =
            aster_drive::db::repository::folder_repo::find_by_id(state.writer_db(), folder_id)
                .await
                .unwrap();
        assert_eq!(
            after, before,
            "rejection must not mutate metadata or activity"
        );
    }
}

/// Verify team root writes reject nonmembers and removed members before side effects.
#[actix_web::test]
async fn team_root_writes_reject_nonmembers_and_removed_members_before_side_effects() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (owner_token, _) = register_and_login!(app);
    let owner =
        aster_drive::db::repository::user_repo::find_by_username(state.writer_db(), "testuser")
            .await
            .unwrap()
            .unwrap();
    let other_id = admin_create_user!(
        app,
        owner_token,
        "outside",
        "outside@example.test",
        "password123"
    );
    let (token, _) = login_user!(app, "outside", "password123");
    let target = team::create_team(
        &state,
        owner.id,
        team::CreateTeamInput {
            name: "Authorization boundary".into(),
            description: None,
        },
    )
    .await
    .unwrap();
    for phase in ["never-member", "removed-member"] {
        if phase == "removed-member" {
            team::add_member(
                &state,
                target.id,
                owner.id,
                team::AddTeamMemberInput {
                    user_id: Some(other_id),
                    identifier: None,
                    role: aster_drive_model::types::TeamMemberRole::Member,
                },
            )
            .await
            .unwrap();
            // Exercise the positive path and warm authorization caches before removal.
            for (path, payload) in [
                (
                    "upload/init",
                    json!({"filename":"permitted.bin","total_size":1}),
                ),
                ("new", json!({"name":"permitted.txt"})),
            ] {
                let response = test::call_service(
                    &app,
                    test::TestRequest::post()
                        .uri(&format!("/api/v1/teams/{}/files/{path}", target.id))
                        .insert_header(("Cookie", common::access_cookie_header(&token)))
                        .insert_header(common::csrf_header_for(&token))
                        .set_json(payload)
                        .to_request(),
                )
                .await;
                assert_eq!(response.status(), 201, "member must retain {path} access");
            }
            team::remove_member(&state, target.id, owner.id, other_id)
                .await
                .unwrap();
        }
        for (path, payload) in [
            (
                "upload/init",
                json!({"filename":"forbidden.bin","total_size":1}),
            ),
            (
                "upload/init",
                json!({"filename":"forbidden.bin","total_size":1,"relative_path":"forbidden/upload.bin"}),
            ),
            ("new", json!({"name":"forbidden.txt"})),
            (
                "new",
                json!({"name":"forbidden.txt","relative_path":"forbidden/empty.txt"}),
            ),
        ] {
            let before = stored_resources(state.writer_db()).await;
            let response = test::call_service(
                &app,
                test::TestRequest::post()
                    .uri(&format!("/api/v1/teams/{}/files/{path}", target.id))
                    .insert_header(("Cookie", common::access_cookie_header(&token)))
                    .insert_header(common::csrf_header_for(&token))
                    .set_json(payload)
                    .to_request(),
            )
            .await;
            assert_eq!(response.status(), 403, "{phase} must not write {path}");
            let body: Value = test::read_body_json(response).await;
            assert_eq!(
                body["code"],
                aster_drive::api::api_error_code::ApiErrorCode::TeamNotMember.as_str()
            );
            assert_eq!(
                stored_resources(state.writer_db()).await,
                before,
                "rejected {phase} {path} must leave files/blobs/folders/sessions untouched"
            );
        }
    }
}

/// A valid member with a missing routing assignment fails without writing any resources.
#[actix_web::test]
async fn team_write_missing_snapshot_assignment_fails_without_database_policy_fallback() {
    let mut state = common::setup().await;
    let owner = common::create_test_account(
        &state,
        "missingowner",
        "missing-snapshot@example.test",
        "password123",
    )
    .await
    .unwrap();
    let target = team::create_team(
        &state,
        owner.id,
        team::CreateTeamInput {
            name: "Missing snapshot".into(),
            description: None,
        },
    )
    .await
    .unwrap();
    assert!(
        state
            .policy_snapshot
            .resolve_team_policy_group_id(target.id)
            .is_some()
    );
    state.policy_snapshot = std::sync::Arc::new(aster_drive::storage::PolicySnapshot::new());
    let before = stored_resources(state.writer_db()).await;
    let error = aster_drive::services::files::upload::init_upload_for_team(
        &state,
        target.id,
        owner.id,
        "blocked.bin",
        1,
        None,
        None,
    )
    .await
    .err()
    .expect("missing snapshot assignment must reject initialization");
    assert_eq!(
        error.api_error_code(),
        aster_drive::api::api_error_code::ApiErrorCode::StoragePolicyNotFound
    );
    assert!(
        error
            .message()
            .contains("no storage placement profile in snapshot")
    );
    assert_eq!(stored_resources(state.writer_db()).await, before);
}
