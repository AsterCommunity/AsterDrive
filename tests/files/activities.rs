//! Workspace attribution and durable activity through real authenticated routes.
use crate::common;
use actix_web::{http::StatusCode, test};
use aster_drive::db::repository::{file_repo, folder_repo, workspace_activity_repo};
use aster_drive_model::entities::{file, workspace_activity};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, Set,
};
use serde_json::{Value, json};

macro_rules! request {
    ($app:expr,$token:expr,$method:ident,$uri:expr,$body:expr) => {{
        let response = test::call_service(
            &$app,
            test::TestRequest::$method()
                .uri(&$uri)
                .insert_header(("Cookie", common::access_cookie_header(&$token)))
                .insert_header(common::csrf_header_for(&$token))
                .set_json($body)
                .to_request(),
        )
        .await;
        let status = response.status();
        let body: Value = test::read_body_json(response).await;
        (status, body)
    }};
}

async fn events(
    state: &aster_drive::runtime::PrimaryAppState,
    kind: &str,
    id: i64,
) -> Vec<workspace_activity::Model> {
    workspace_activity::Entity::find()
        .filter(workspace_activity::Column::EntityType.eq(kind))
        .filter(workspace_activity::Column::EntityId.eq(id))
        .order_by_asc(workspace_activity::Column::Id)
        .all(state.writer_db())
        .await
        .unwrap()
}

#[actix_web::test]
async fn attribution_and_activity_follow_real_mutations_and_noops() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let id = common::create_empty_file_via_api(&app, &token, "first.txt", None).await;
    let original = file_repo::find_by_id(state.writer_db(), id).await.unwrap();
    assert_eq!(original.modified_by_user_id, original.created_by_user_id);
    assert_eq!(events(&state, "file", id).await.len(), 1);
    let (status, body) = request!(
        app,
        token,
        patch,
        format!("/api/v1/files/{id}"),
        json!({"name":"renamed.txt"})
    );
    assert_eq!(status, 200, "{body}");
    let renamed = file_repo::find_by_id(state.writer_db(), id).await.unwrap();
    assert_eq!(renamed.created_by_user_id, original.created_by_user_id);
    assert_eq!(renamed.modified_by_username.as_deref(), Some("testuser"));
    let rows = events(&state, "file", id).await;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].action, "file_rename");
    let details: Value = serde_json::from_str(&rows[1].details).unwrap();
    assert_eq!(details["previous_name"], "first.txt");
    assert_eq!(details["next_name"], "renamed.txt");
    let (status, _) = request!(
        app,
        token,
        patch,
        format!("/api/v1/files/{id}"),
        json!({"name":"renamed.txt"})
    );
    assert_eq!(status, 200);
    assert_eq!(events(&state, "file", id).await.len(), 2);
    assert_eq!(
        file_repo::find_by_id(state.writer_db(), id)
            .await
            .unwrap()
            .modified_at,
        renamed.modified_at
    );
    let (status, page) = request!(
        app,
        token,
        get,
        format!("/api/v1/files/{id}/activities?limit=1"),
        json!({})
    );
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["data"]["items"][0]["action"], "file_rename");
    let cursor = page["data"]["next_before_id"].as_i64().unwrap();
    let (_, page) = request!(
        app,
        token,
        get,
        format!("/api/v1/files/{id}/activities?limit=1&before_id={cursor}"),
        json!({})
    );
    assert_eq!(page["data"]["items"][0]["action"], "file_create");
    assert!(page["data"]["next_before_id"].is_null());
    let (_, listing) = request!(app, token, get, "/api/v1/folders", json!({}));
    assert_eq!(
        listing["data"]["files"][0]["created_by"]["name"],
        "testuser"
    );
    assert_eq!(
        listing["data"]["files"][0]["modified_by"]["name"],
        "testuser"
    );
}

#[actix_web::test]
async fn activity_is_mandatory_when_optional_audit_is_disabled() {
    let state = common::setup().await;
    state
        .runtime_config
        .apply(common::system_config_model("audit_log_enabled", "false"));
    state.runtime_config.apply(common::system_config_model(
        "audit_log_recorded_actions",
        "[]",
    ));
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let id = common::create_empty_file_via_api(&app, &token, "audit-off.txt", None).await;
    let (status, body) = request!(
        app,
        token,
        patch,
        format!("/api/v1/files/{id}"),
        json!({"name":"still-recorded.txt"})
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(events(&state, "file", id).await.len(), 2);
    assert!(
        file_repo::find_by_id(state.writer_db(), id)
            .await
            .unwrap()
            .modified_at
            .is_some()
    );
}

#[actix_web::test]
async fn failed_activity_insert_rolls_back_file_folder_and_notifications() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let id = common::create_empty_file_via_api(&app, &token, "before.txt", None).await;
    let destination =
        common::create_empty_file_via_api(&app, &token, "overwrite-target.txt", None).await;
    let destination_before = file_repo::find_by_id(state.writer_db(), destination)
        .await
        .unwrap();
    let (_, body) = request!(
        app,
        token,
        post,
        "/api/v1/folders",
        json!({"name":"before-folder"})
    );
    let folder_id = body["data"]["id"].as_i64().unwrap();
    let before = file_repo::find_by_id(state.writer_db(), id).await.unwrap();
    let folder_before = folder_repo::find_by_id(state.writer_db(), folder_id)
        .await
        .unwrap();
    let mut changes = state.storage_change_bus.subscribe();
    state
        .writer_db()
        .execute_unprepared("DROP TABLE workspace_activities")
        .await
        .unwrap();
    let (status, _) = request!(
        app,
        token,
        patch,
        format!("/api/v1/files/{id}"),
        json!({"name":"must-rollback.txt"})
    );
    assert_eq!(status, 500);
    assert_eq!(
        file_repo::find_by_id(state.writer_db(), id).await.unwrap(),
        before
    );
    let (status, _) = request!(
        app,
        token,
        put,
        format!("/api/v1/folders/{folder_id}/icon"),
        json!({"kind":"emoji","value":"📚"})
    );
    assert_eq!(status, 500);
    assert_eq!(
        folder_repo::find_by_id(state.writer_db(), folder_id)
            .await
            .unwrap(),
        folder_before
    );
    assert!(
        changes.try_recv().is_err(),
        "failed mutations must not emit successful storage events"
    );
    let (status, _) = request!(
        app,
        token,
        post,
        "/api/v1/files/new",
        json!({"name":"must-not-exist.txt"})
    );
    assert_eq!(status, 500);
    assert!(
        file_repo::find_by_name_in_folder(
            state.writer_db(),
            before.owner_user_id.unwrap(),
            None,
            "must-not-exist.txt"
        )
        .await
        .unwrap()
        .is_none()
    );
    use aster_forge_webdav::{DavFileSystem, DavPath};
    let fs = aster_drive::webdav::backend::AsterDavFs::new(
        state.clone(),
        before.owner_user_id.unwrap(),
        None,
    );
    assert!(
        fs.copy(
            &DavPath::new("/before.txt").unwrap(),
            &DavPath::new("/overwrite-target.txt").unwrap()
        )
        .await
        .is_err()
    );
    assert_eq!(
        file_repo::find_by_id(state.writer_db(), destination)
            .await
            .unwrap(),
        destination_before,
        "a failed overwrite-delete activity must preserve the destination"
    );
    assert!(
        changes.try_recv().is_err(),
        "failed overwrite deletion must not emit a success notification"
    );
}

#[actix_web::test]
async fn activity_pages_validate_bounds_and_do_not_cross_personal_workspaces() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (owner, _) = register_and_login!(app);
    let id = common::create_empty_file_via_api(&app, &owner, "private.txt", None).await;
    admin_create_user!(
        app,
        owner,
        "outsider",
        "outsider@example.com",
        "password123"
    );
    let (outsider, _) = login_user!(app, "outsider", "password123");
    let (status, _) = request!(
        app,
        outsider,
        get,
        format!("/api/v1/files/{id}/activities"),
        json!({})
    );
    assert!(matches!(
        status,
        StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
    ));
    for query in ["limit=0", "limit=201", "before_id=0", "before_id=-1"] {
        let (status, _) = request!(
            app,
            owner,
            get,
            format!("/api/v1/files/{id}/activities?{query}"),
            json!({})
        );
        assert_eq!(status, 400, "{query}");
    }
    let request = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{id}/activities"))
        .to_request();
    assert_service_status!(app, request, 401);
}

#[actix_web::test]
async fn historical_unknown_attribution_and_retention_do_not_fabricate_an_author() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let id = common::create_empty_file_via_api(&app, &token, "historical.txt", None).await;
    let mut old: file::ActiveModel = file_repo::find_by_id(state.writer_db(), id)
        .await
        .unwrap()
        .into();
    old.modified_at = Set(None);
    old.modified_by_user_id = Set(None);
    old.modified_by_username = Set(None);
    old.update(state.writer_db()).await.unwrap();
    let (_, info) = request!(app, token, get, format!("/api/v1/files/{id}"), json!({}));
    assert!(info["data"]["modified_by"].is_null());
    assert_eq!(info["data"]["created_by"]["name"], "testuser");
    request!(
        app,
        token,
        patch,
        format!("/api/v1/files/{id}"),
        json!({"name":"known-now.txt"})
    );
    let before = file_repo::find_by_id(state.writer_db(), id).await.unwrap();
    workspace_activity_repo::delete_before(
        state.writer_db(),
        chrono::Utc::now() + chrono::Duration::days(1),
    )
    .await
    .unwrap();
    assert!(events(&state, "file", id).await.is_empty());
    assert_eq!(
        file_repo::find_by_id(state.writer_db(), id).await.unwrap(),
        before
    );
}

#[actix_web::test]
async fn concurrent_renames_record_locked_before_state_in_commit_order() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let id = common::create_empty_file_via_api(&app, &token, "original.txt", None).await;
    let make = |name: &str| {
        test::TestRequest::patch()
            .uri(&format!("/api/v1/files/{id}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(json!({"name":name}))
            .to_request()
    };
    let (one, two) = tokio::join!(
        test::call_service(&app, make("one.txt")),
        test::call_service(&app, make("two.txt"))
    );
    assert_eq!(one.status(), 200);
    assert_eq!(two.status(), 200);
    let rows = events(&state, "file", id).await;
    assert_eq!(rows.len(), 3);
    let first: Value = serde_json::from_str(&rows[1].details).unwrap();
    let last: Value = serde_json::from_str(&rows[2].details).unwrap();
    assert_eq!(first["previous_name"], "original.txt");
    assert_eq!(first["next_name"], last["previous_name"]);
    assert_eq!(
        last["next_name"],
        file_repo::find_by_id(state.writer_db(), id)
            .await
            .unwrap()
            .name
    );
}

#[actix_web::test]
async fn creation_idempotency_replay_does_not_duplicate_activity() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let mut id = None;
    for _ in 0..2 {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/files/new")
                .insert_header(("Cookie", common::access_cookie_header(&token)))
                .insert_header(common::csrf_header_for(&token))
                .insert_header(("Idempotency-Key", "activity-replay-657"))
                .set_json(json!({"name":"once.txt"}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), 201);
        let body: Value = test::read_body_json(response).await;
        let current = body["data"]["id"].as_i64().unwrap();
        assert!(id.is_none() || id == Some(current));
        id = Some(current);
    }
    assert_eq!(events(&state, "file", id.unwrap()).await.len(), 1);
}

#[actix_web::test]
async fn team_attribution_resource_permissions_and_deleted_actor_snapshots() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (owner, _) = register_and_login!(app);
    let member_id = admin_create_user!(
        app,
        owner,
        "member657",
        "member657@example.com",
        "password123"
    );
    let (member, _) = login_user!(app, "member657", "password123");
    let (status, body) = request!(
        app,
        owner,
        post,
        "/api/v1/teams",
        json!({"name":"Attribution team"})
    );
    assert_eq!(status, 201, "{body}");
    let team = body["data"]["id"].as_i64().unwrap();
    let (status, body) = request!(
        app,
        owner,
        post,
        format!("/api/v1/teams/{team}/members"),
        json!({"user_id":member_id,"role":"member"})
    );
    assert_eq!(status, 201, "{body}");
    let (status, body) = request!(
        app,
        member,
        post,
        format!("/api/v1/teams/{team}/files/new"),
        json!({"name":"team-created.txt"})
    );
    assert_eq!(status, 201, "{body}");
    let id = body["data"]["id"].as_i64().unwrap();
    let (status, body) = request!(
        app,
        owner,
        patch,
        format!("/api/v1/teams/{team}/files/{id}"),
        json!({"name":"team-modified.txt"})
    );
    assert_eq!(status, 200, "{body}");
    let (_, info) = request!(
        app,
        member,
        get,
        format!("/api/v1/teams/{team}/files/{id}"),
        json!({})
    );
    assert_eq!(info["data"]["created_by"]["name"], "member657");
    assert_eq!(info["data"]["modified_by"]["name"], "testuser");
    let (_, list) = request!(
        app,
        member,
        get,
        format!("/api/v1/teams/{team}/folders"),
        json!({})
    );
    assert_eq!(list["data"]["files"][0]["created_by"]["name"], "member657");
    assert_eq!(list["data"]["files"][0]["modified_by"]["name"], "testuser");
    let (status, page) = request!(
        app,
        member,
        get,
        format!("/api/v1/teams/{team}/files/{id}/activities"),
        json!({})
    );
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["data"]["items"].as_array().unwrap().len(), 2);
    let (status, _) = request!(
        app,
        member,
        get,
        format!("/api/v1/teams/{team}/activities"),
        json!({})
    );
    assert_eq!(status, 403);
    let (status, page) = request!(
        app,
        owner,
        get,
        format!("/api/v1/teams/{team}/activities"),
        json!({})
    );
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["data"]["items"][0]["entity_id"], id);
    let (_, body) = request!(
        app,
        owner,
        post,
        "/api/v1/teams",
        json!({"name":"Another team"})
    );
    let other = body["data"]["id"].as_i64().unwrap();
    let (status, _) = request!(
        app,
        owner,
        get,
        format!("/api/v1/teams/{other}/files/{id}/activities"),
        json!({})
    );
    assert!(matches!(
        status,
        StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
    ));
    let (status, _) = request!(
        app,
        owner,
        delete,
        format!("/api/v1/admin/users/{member_id}"),
        json!({})
    );
    assert_eq!(status, 200);
    let (_, info) = request!(
        app,
        owner,
        get,
        format!("/api/v1/teams/{team}/files/{id}"),
        json!({})
    );
    assert_eq!(info["data"]["created_by"]["name"], "member657");
    let history = events(&state, "file", id).await;
    assert_eq!(history[0].actor_name, "member657");
    assert_eq!(history[0].actor_user_id, Some(member_id));
}

#[actix_web::test]
async fn activity_export_is_authorized_formula_safe_and_snapshot_bounded() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (owner, _) = register_and_login!(app);
    let (_, body) = request!(
        app,
        owner,
        post,
        "/api/v1/teams",
        json!({"name":"Export team"})
    );
    let team = body["data"]["id"].as_i64().unwrap();
    let (status, body) = request!(
        app,
        owner,
        post,
        format!("/api/v1/teams/{team}/files/new"),
        json!({"name":"=SUM(1).txt"})
    );
    assert_eq!(status, 201, "{body}");
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/v1/teams/{team}/activities/export"))
            .insert_header(("Cookie", common::access_cookie_header(&owner)))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let expected = response
        .headers()
        .get("X-Activity-Export-Rows")
        .unwrap()
        .to_str()
        .unwrap()
        .parse::<usize>()
        .unwrap();
    let (status, _) = request!(
        app,
        owner,
        post,
        format!("/api/v1/teams/{team}/files/new"),
        json!({"name":"after-snapshot.txt"})
    );
    assert_eq!(status, 201);
    let bytes = test::read_body(response).await;
    let mut csv = csv::Reader::from_reader(bytes.as_ref());
    let headers = csv.headers().unwrap().clone();
    assert!(
        !headers
            .iter()
            .any(|h| h == "ip_address" || h == "user_agent")
    );
    let name_index = headers.iter().position(|h| h == "entity_name").unwrap();
    let records = csv
        .records()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(records.len(), expected);
    assert!(records.iter().any(|r| r[name_index].starts_with("'=SUM")));
    assert!(
        !records
            .iter()
            .any(|r| r[name_index] == *"after-snapshot.txt")
    );
}

#[actix_web::test]
async fn tags_are_atomic_idempotent_metadata_changes() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let id = common::create_empty_file_via_api(&app, &token, "tagged.txt", None).await;
    let (_, body) = request!(
        app,
        token,
        post,
        "/api/v1/tags",
        json!({"name":"Important","color":"#123456"})
    );
    let tag = body["data"]["id"].as_i64().unwrap();
    for _ in 0..2 {
        let (status, body) = request!(
            app,
            token,
            put,
            format!("/api/v1/tags/{tag}/file/{id}"),
            json!({})
        );
        assert_eq!(status, 200, "{body}");
    }
    let history = events(&state, "file", id).await;
    assert_eq!(history.len(), 2);
    assert_eq!(history[1].action, "tag_attach");
    let details: Value = serde_json::from_str(&history[1].details).unwrap();
    assert_eq!(details["previous_tag_ids"], json!([]));
    assert_eq!(details["tag_ids"], json!([tag]));
    let before = file_repo::find_by_id(state.writer_db(), id).await.unwrap();
    state
        .writer_db()
        .execute_unprepared("DROP TABLE workspace_activities")
        .await
        .unwrap();
    let (status, _) = request!(
        app,
        token,
        delete,
        format!("/api/v1/tags/{tag}/file/{id}"),
        json!({})
    );
    assert_eq!(status, 500);
    let (_, tags) = request!(
        app,
        token,
        get,
        format!("/api/v1/tags/file/{id}"),
        json!({})
    );
    assert_eq!(tags["data"]["tags"].as_array().unwrap().len(), 1);
    assert_eq!(
        file_repo::find_by_id(state.writer_db(), id).await.unwrap(),
        before
    );
}

#[actix_web::test]
async fn display_names_match_details_listing_search_and_fall_back_when_cleared() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let id = common::create_empty_file_via_api(&app, &token, "display-657.txt", None).await;
    let (status, body) = request!(
        app,
        token,
        post,
        "/api/v1/folders",
        json!({"name":"display-folder-657"})
    );
    assert_eq!(status, 201, "{body}");
    let folder = body["data"]["id"].as_i64().unwrap();
    let (status, body) = request!(
        app,
        token,
        patch,
        "/api/v1/auth/profile",
        json!({"display_name":"Display 657"})
    );
    assert_eq!(status, 200, "{body}");
    for uri in [
        format!("/api/v1/files/{id}"),
        format!("/api/v1/folders/{folder}/info"),
        "/api/v1/folders".into(),
        "/api/v1/search?q=display".into(),
    ] {
        let (status, body) = request!(app, token, get, uri, json!({}));
        assert_eq!(status, 200, "{body}");
        let data = &body["data"];
        let resources = if data.get("files").is_some() {
            data["files"]
                .as_array()
                .unwrap()
                .iter()
                .chain(data["folders"].as_array().unwrap())
                .collect::<Vec<_>>()
        } else {
            vec![data]
        };
        assert!(!resources.is_empty(), "{uri}: {body}");
        for resource in resources {
            assert_eq!(
                resource["created_by"]["name"], "Display 657",
                "{uri}: {body}"
            );
            assert_eq!(
                resource["modified_by"]["name"], "Display 657",
                "{uri}: {body}"
            );
        }
    }
    let (status, body) = request!(
        app,
        token,
        patch,
        "/api/v1/auth/profile",
        json!({"display_name":"   "})
    );
    assert_eq!(status, 200, "{body}");
    let (_, body) = request!(app, token, get, format!("/api/v1/files/{id}"), json!({}));
    assert_eq!(body["data"]["created_by"]["name"], "testuser");
    file::Entity::update_many()
        .col_expr(
            file::Column::CreatedByUsername,
            sea_orm::sea_query::Expr::value(""),
        )
        .filter(file::Column::Id.eq(id))
        .exec(state.writer_db())
        .await
        .unwrap();
    let (_, body) = request!(app, token, get, format!("/api/v1/files/{id}"), json!({}));
    assert_eq!(
        body["data"]["created_by"]["name"], "testuser",
        "a live identity must resolve even with an empty historical snapshot"
    );
    // Immutable history keeps its original identity snapshot when display names change.
    assert_eq!(events(&state, "file", id).await[0].actor_name, "testuser");
}

#[actix_web::test]
async fn properties_record_only_real_changes_and_activity_failure_rolls_back() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let id = common::create_empty_file_via_api(&app, &token, "property-657.txt", None).await;
    let uri = format!("/api/v1/properties/file/{id}");
    for value in ["one", "one", "two"] {
        let (status, body) = request!(
            app,
            token,
            put,
            uri,
            json!({"namespace":"urn:657","name":"note","value":value})
        );
        assert_eq!(status, 200, "{body}");
    }
    let records = events(&state, "file", id).await;
    assert_eq!(records.len(), 3);
    assert_eq!(records[1].action, "property_set");
    assert!(
        !records[1].details.contains("one"),
        "arbitrary property values must not enter history"
    );
    workspace_activity::Entity::delete_many()
        .exec(state.writer_db())
        .await
        .unwrap();
    // Removing the required table injects a real persistence failure across every backend.
    state
        .writer_db()
        .execute_unprepared("DROP TABLE workspace_activities")
        .await
        .unwrap();
    let before = file_repo::find_by_id(state.writer_db(), id).await.unwrap();
    let (status, _) = request!(
        app,
        token,
        put,
        uri,
        json!({"namespace":"urn:657","name":"note","value":"failed"})
    );
    assert_eq!(status, 500);
    let prop = aster_drive::db::repository::property_repo::find_by_key(
        state.writer_db(),
        aster_drive_model::types::EntityType::File,
        id,
        "urn:657",
        "note",
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(prop.value.as_deref(), Some("two"));
    let after = file_repo::find_by_id(state.writer_db(), id).await.unwrap();
    assert_eq!(after.modified_at, before.modified_at);
}

#[actix_web::test]
async fn migration_preserves_existing_resources_and_does_not_invent_modifiers() {
    use aster_drive_migration::{CurrentMigrator, MigratorTrait};
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let id = common::create_empty_file_via_api(&app, &token, "historical-657.txt", None).await;
    let before = file_repo::find_by_id(state.writer_db(), id).await.unwrap();
    CurrentMigrator::down(state.writer_db(), Some(1))
        .await
        .unwrap();
    CurrentMigrator::up(state.writer_db(), Some(1))
        .await
        .unwrap();
    let after = file_repo::find_by_id(state.writer_db(), id).await.unwrap();
    assert_eq!(after.name, before.name);
    assert_eq!(after.created_by_user_id, before.created_by_user_id);
    assert_eq!(after.created_by_username, before.created_by_username);
    assert_eq!(after.updated_at, before.updated_at);
    assert!(after.modified_by_user_id.is_none());
    assert!(after.modified_by_username.is_none());
    assert!(after.modified_at.is_none());
    assert!(events(&state, "file", id).await.is_empty());
}

#[actix_web::test]
async fn cross_workspace_copy_correlates_both_sides_without_leaking_foreign_resources() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let source =
        common::create_empty_file_via_api(&app, &token, "private-source-657.txt", None).await;
    let (_, body) = request!(
        app,
        token,
        post,
        "/api/v1/teams",
        json!({"name":"Destination 657"})
    );
    let team = body["data"]["id"].as_i64().unwrap();
    let (status, body) = request!(
        app,
        token,
        post,
        "/api/v1/workspace-transfer/copy",
        json!({"source_workspace":{"kind":"personal"},"destination_workspace":{"kind":"team","team_id":team},"file_ids":[source],"folder_ids":[],"target_folder_id":null})
    );
    assert_eq!(status, 200, "{body}");
    let outgoing = events(&state, "file", source)
        .await
        .into_iter()
        .find(|r| r.action == "file_copy")
        .unwrap();
    let (_, page) = request!(
        app,
        token,
        get,
        format!("/api/v1/teams/{team}/activities"),
        json!({})
    );
    let incoming = &page["data"]["items"][0];
    assert_eq!(incoming["operation_id"], outgoing.operation_id);
    assert_eq!(incoming["details"]["source_workspace"], "external");
    assert!(incoming["details"].get("source_file_id").is_none());
    assert!(incoming["details"].get("source_name").is_none());
    let destination = incoming["entity_id"].as_i64().unwrap();
    assert_ne!(destination, source);
    assert_eq!(events(&state, "file", destination).await.len(), 1);
    let original = file_repo::find_by_id(state.writer_db(), source)
        .await
        .unwrap();
    assert_eq!(
        original.modified_at,
        Some(original.created_at),
        "copying must not modify source attribution"
    );
    let (status, body) = request!(
        app,
        token,
        post,
        "/api/v1/workspace-transfer/move",
        json!({"source_workspace":{"kind":"personal"},"destination_workspace":{"kind":"team","team_id":team},"file_ids":[source],"folder_ids":[],"target_folder_id":null})
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["data"]["failed"], 0);
    let history = events(&state, "file", source).await;
    let deleted = history.last().unwrap();
    let copied = &history[history.len() - 2];
    assert_eq!(deleted.action, "file_delete");
    assert_eq!(copied.action, "file_copy");
    assert_eq!(
        copied.operation_id, deleted.operation_id,
        "copy and source deletion belong to one cross-workspace move"
    );
    let (_, page) = request!(
        app,
        token,
        get,
        format!("/api/v1/teams/{team}/activities"),
        json!({})
    );
    assert_eq!(
        page["data"]["items"][0]["operation_id"],
        deleted.operation_id
    );
}

#[actix_web::test]
async fn retention_config_is_independent_bounded_and_keeps_current_attribution() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let id = common::create_empty_file_via_api(&app, &token, "retention-657.txt", None).await;
    let first = events(&state, "file", id).await.remove(0);
    let mut old: workspace_activity::ActiveModel = first.into();
    old.created_at = Set(chrono::Utc::now() - chrono::Duration::days(2));
    old.update(state.writer_db()).await.unwrap();
    request!(
        app,
        token,
        patch,
        format!("/api/v1/files/{id}"),
        json!({"name":"retained-summary.txt"})
    );
    let before = file_repo::find_by_id(state.writer_db(), id).await.unwrap();
    state
        .runtime_config
        .apply(common::system_config_model("audit_log_retention_days", "1"));
    assert_eq!(
        aster_drive::services::workspace::activity::cleanup_expired(&state)
            .await
            .unwrap(),
        0
    );
    state.runtime_config.apply(common::system_config_model(
        "workspace_activity_retention_days",
        "1",
    ));
    assert_eq!(
        aster_drive::services::workspace::activity::cleanup_expired(&state)
            .await
            .unwrap(),
        1
    );
    let remaining = events(&state, "file", id).await;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].action, "file_rename");
    state.runtime_config.apply(common::system_config_model(
        "workspace_activity_retention_days",
        "9223372036854775807",
    ));
    assert!(
        aster_drive::services::workspace::activity::cleanup_expired(&state)
            .await
            .is_err()
    );
    assert_eq!(events(&state, "file", id).await.len(), 1);
    assert_eq!(
        file_repo::find_by_id(state.writer_db(), id).await.unwrap(),
        before
    );
}

#[actix_web::test]
async fn buffered_content_update_records_edit_instead_of_upload() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let id = common::create_empty_file_via_api(&app, &token, "buffered-edit.txt", None).await;
    let file = file_repo::find_by_id(state.writer_db(), id).await.unwrap();
    aster_drive::services::files::file::update_content(
        &state,
        id,
        file.created_by_user_id.unwrap(),
        bytes::Bytes::from_static(b"edited"),
        None,
    )
    .await
    .unwrap();
    let rows = events(&state, "file", id).await;
    assert_eq!(rows.last().unwrap().action, "file_edit");
    assert_eq!(rows.last().unwrap().source, "rest");
}

#[actix_web::test]
async fn generic_webdav_mutations_keep_source_and_overwrite_delete_activity() {
    use aster_forge_webdav::{DavFileSystem, DavMutationCredentials, DavPath};
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let id = common::create_empty_file_via_api(&app, &token, "dav-source.txt", None).await;
    let overwritten = common::create_empty_file_via_api(&app, &token, "dav-copy.txt", None).await;
    let file = file_repo::find_by_id(state.writer_db(), id).await.unwrap();
    let fs = aster_drive::webdav::backend::AsterDavFs::new(
        state.clone(),
        file.created_by_user_id.unwrap(),
        None,
    );
    fs.copy(
        &DavPath::new("/dav-source.txt").unwrap(),
        &DavPath::new("/dav-copy.txt").unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
        events(&state, "file", overwritten)
            .await
            .last()
            .unwrap()
            .action,
        "file_delete"
    );
    assert_eq!(
        events(&state, "file", overwritten)
            .await
            .last()
            .unwrap()
            .source,
        "webdav"
    );
    fs.rename(
        &DavPath::new("/dav-source.txt").unwrap(),
        &DavPath::new("/dav-renamed.txt").unwrap(),
    )
    .await
    .unwrap();
    fs.remove_file(&DavPath::new("/dav-renamed.txt").unwrap())
        .await
        .unwrap();
    fs.create_dir(
        &DavPath::new("/dav-folder").unwrap(),
        DavMutationCredentials::default(),
    )
    .await
    .unwrap();
    fs.rename(
        &DavPath::new("/dav-folder").unwrap(),
        &DavPath::new("/dav-folder-renamed").unwrap(),
    )
    .await
    .unwrap();
    fs.remove_dir(&DavPath::new("/dav-folder-renamed").unwrap())
        .await
        .unwrap();
    let rows = workspace_activity::Entity::find()
        .filter(workspace_activity::Column::Source.eq("webdav"))
        .all(state.writer_db())
        .await
        .unwrap();
    for action in [
        "file_copy",
        "file_rename",
        "file_delete",
        "folder_create",
        "folder_rename",
        "folder_delete",
    ] {
        assert!(
            rows.iter().any(|row| row.action == action),
            "missing {action}"
        );
    }
    assert_eq!(events(&state, "file", id).await.len(), 3);
}

#[actix_web::test]
async fn overview_counts_one_deletion_operation_for_a_batch_tree() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let (_, folder) = request!(
        app,
        token,
        post,
        "/api/v1/folders",
        json!({"name":"delete-count-tree"})
    );
    let folder_id = folder["data"]["id"].as_i64().unwrap();
    for index in 0..3 {
        common::create_empty_file_via_api(
            &app,
            &token,
            &format!("descendant-{index}.txt"),
            Some(folder_id),
        )
        .await;
    }
    let (status, body) = request!(
        app,
        token,
        post,
        "/api/v1/batch/delete",
        json!({"file_ids":[],"folder_ids":[folder_id]})
    );
    assert_eq!(status, 200, "{body}");
    let rows = workspace_activity::Entity::find()
        .filter(workspace_activity::Column::Action.is_in(["file_delete", "folder_delete"]))
        .all(state.writer_db())
        .await
        .unwrap();
    assert_eq!(rows.len(), 4);
    assert!(
        rows.iter()
            .all(|row| row.operation_id == rows[0].operation_id)
    );
    let (_, overview) = request!(
        app,
        token,
        get,
        "/api/v1/admin/overview?days=1&timezone=UTC",
        json!({})
    );
    assert_eq!(
        overview["data"]["daily_reports"][0]["deletions"], 1,
        "{overview}"
    );
    let now = chrono::Utc::now();
    let counts = workspace_activity_repo::action_counts_in_windows(
        state.writer_db(),
        &[(
            now - chrono::Duration::days(1),
            now + chrono::Duration::days(1),
        )],
    )
    .await
    .unwrap();
    assert_eq!(
        counts
            .iter()
            .find(|(_, action, _)| action == "file_delete")
            .unwrap()
            .2,
        1
    );
    assert!(
        workspace_activity_repo::action_counts_in_windows(state.writer_db(), &[])
            .await
            .unwrap()
            .is_empty()
    );
}

#[actix_web::test]
async fn retention_config_rejects_negative_and_non_integer_writes() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    for value in ["-3", "abc", "1.5"] {
        let (status, _) = request!(
            app,
            token,
            put,
            "/api/v1/admin/config/workspace_activity_retention_days",
            json!({"value":value})
        );
        assert_eq!(status, 400, "{value}");
    }
    for value in ["0", "30"] {
        let (status, body) = request!(
            app,
            token,
            put,
            "/api/v1/admin/config/workspace_activity_retention_days",
            json!({"value":value})
        );
        assert_eq!(status, 200, "{body}");
    }
}

#[actix_web::test]
async fn audit_configuration_schema_and_legacy_values_exclude_workspace_mutations() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let (_, schema) = request!(app, token, get, "/api/v1/admin/config/schema", json!({}));
    let options = schema["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key"] == "audit_log_recorded_actions")
        .unwrap()["options"]
        .as_array()
        .unwrap();
    assert!(
        options
            .iter()
            .any(|option| option["value"] == "file_download")
    );
    for option in options {
        let action =
            aster_drive_model::types::AuditAction::from_str_name(option["value"].as_str().unwrap())
                .unwrap();
        assert!(!action.is_workspace_mutation());
    }
    let (status, _) = request!(
        app,
        token,
        put,
        "/api/v1/admin/config/audit_log_recorded_actions",
        json!({"value":["file_upload"]})
    );
    assert_eq!(status, 400);
    aster_drive::db::repository::config_repo::upsert_with_actor(
        state.writer_db(),
        "audit_log_recorded_actions",
        r#"["file_upload","user_login"]"#,
        None,
    )
    .await
    .unwrap();
    let (_, config) = request!(
        app,
        token,
        get,
        "/api/v1/admin/config/audit_log_recorded_actions",
        json!({})
    );
    assert_eq!(config["data"]["value"], json!(["user_login"]));
}
