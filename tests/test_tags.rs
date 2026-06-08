//! 集成测试：`tags`。

#[macro_use]
mod common;

use actix_web::{http::StatusCode, test};
use aster_drive::db::repository::property_repo;
use aster_drive::runtime::SharedRuntimeState;
use aster_drive::services::tag_service::TAG_PROPERTY_NAMESPACE;
use aster_drive::types::EntityType;
use serde_json::Value;

async fn create_tag_response(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: &str,
    uri: &str,
    name: &str,
    color: &str,
) -> (StatusCode, Value) {
    let req = test::TestRequest::post()
        .uri(uri)
        .insert_header(("Cookie", common::access_cookie_header(token)))
        .insert_header(common::csrf_header_for(token))
        .set_json(serde_json::json!({ "name": name, "color": color }))
        .to_request();
    let resp = test::call_service(app, req).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    (status, body)
}

async fn create_tag_at(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: &str,
    uri: &str,
    name: &str,
    color: &str,
) -> i64 {
    let (status, body) = create_tag_response(app, token, uri, name, color).await;
    assert_eq!(status, 201, "create tag failed: {body:?}");
    body["data"]["id"].as_i64().unwrap()
}

async fn create_tag(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: &str,
    name: &str,
    color: &str,
) -> i64 {
    create_tag_at(app, token, "/api/v1/tags", name, color).await
}

async fn create_folder_at(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: &str,
    uri: &str,
    name: &str,
) -> i64 {
    let req = test::TestRequest::post()
        .uri(uri)
        .insert_header(("Cookie", common::access_cookie_header(token)))
        .insert_header(common::csrf_header_for(token))
        .set_json(serde_json::json!({ "name": name }))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 201, "create folder should return 201");
    let body: Value = test::read_body_json(resp).await;
    body["data"]["id"].as_i64().unwrap()
}

async fn upload_file_at(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: &str,
    uri: &str,
    filename: &str,
    content: &str,
) -> i64 {
    let boundary = "----TagBoundary123";
    let payload = format!(
        "------TagBoundary123\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n\
         Content-Type: text/plain\r\n\r\n\
         {content}\r\n\
         ------TagBoundary123--\r\n"
    );
    let req = test::TestRequest::post()
        .uri(uri)
        .insert_header(("Cookie", common::access_cookie_header(token)))
        .insert_header(common::csrf_header_for(token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 201, "upload should return 201");
    let body: Value = test::read_body_json(resp).await;
    body["data"]["id"].as_i64().unwrap()
}

#[actix_web::test]
async fn test_personal_tags_attach_list_search_and_delete_cleanup() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_named!(app, token, "tagged-report.txt");
    let other_file_id = upload_test_file_named!(app, token, "untagged-report.txt");
    let tag_id = create_tag(&app, &token, "Important", "#3b82f6").await;

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/tags/{tag_id}/file/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, 200, "attach tag failed: {body:?}");
    assert_eq!(body["data"]["tags"][0]["name"], "Important");

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, 200, "list folder failed: {body:?}");
    let files = body["data"]["files"].as_array().unwrap();
    let tagged = files
        .iter()
        .find(|file| file["id"].as_i64() == Some(file_id))
        .unwrap();
    assert_eq!(tagged["tags"][0]["id"].as_i64(), Some(tag_id));

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/search?tag_ids={tag_id}&type=file"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let result_ids = body["data"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["id"].as_i64().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(result_ids, vec![file_id]);
    assert!(!result_ids.contains(&other_file_id));

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/tags/{tag_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let prop = property_repo::find_by_key(
        state.writer_db(),
        EntityType::File,
        file_id,
        TAG_PROPERTY_NAMESPACE,
        &tag_id.to_string(),
    )
    .await
    .unwrap();
    assert!(
        prop.is_none(),
        "deleting a tag must clean system.tags binding"
    );
}

#[actix_web::test]
async fn test_personal_tags_replace_and_match_all_search() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_named!(app, token, "both-tags.txt");
    let first_tag = create_tag(&app, &token, "One", "#16a34a").await;
    let second_tag = create_tag(&app, &token, "Two", "#dc2626").await;

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/tags/file/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "tag_ids": [first_tag, second_tag] }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, 200, "replace tags failed: {body:?}");

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/search?tag_ids={first_tag},{second_tag}&tag_match=all&type=file"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let files = body["data"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["id"].as_i64(), Some(file_id));
}

#[actix_web::test]
async fn test_personal_tags_validate_normalize_filter_and_update() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);

    let (status, body) =
        create_tag_response(&app, &token, "/api/v1/tags", "  Alpha  ", " #ABCDEF ").await;
    assert_eq!(
        status, 201,
        "trimmed mixed-case tag should create: {body:?}"
    );
    let alpha_id = body["data"]["id"].as_i64().unwrap();
    assert_eq!(body["data"]["name"], "Alpha");
    assert_eq!(body["data"]["normalized_name"], "alpha");
    assert_eq!(body["data"]["color"], "#abcdef");

    let (status, _) = create_tag_response(&app, &token, "/api/v1/tags", " alpha ", "#123456").await;
    assert_eq!(
        status, 400,
        "duplicate names should be normalized by trim/case"
    );

    for (name, color) in [
        ("   ", "#123456"),
        ("x", "123456"),
        ("x", "#12345"),
        ("x", "#12345g"),
    ] {
        let (status, _) = create_tag_response(&app, &token, "/api/v1/tags", name, color).await;
        assert_eq!(status, 400, "invalid tag input should be rejected");
    }

    let unicode_name = "标".repeat(64);
    let (status, body) =
        create_tag_response(&app, &token, "/api/v1/tags", &unicode_name, "#654321").await;
    assert_eq!(
        status, 201,
        "64 unicode scalar characters should create: {body:?}"
    );
    assert_eq!(body["data"]["name"], unicode_name);

    let long_name = "a".repeat(65);
    let (status, _) =
        create_tag_response(&app, &token, "/api/v1/tags", &long_name, "#123456").await;
    assert_eq!(
        status, 400,
        "names longer than 64 characters should be rejected"
    );

    let long_unicode_name = "标".repeat(65);
    let (status, _) =
        create_tag_response(&app, &token, "/api/v1/tags", &long_unicode_name, "#123456").await;
    assert_eq!(
        status, 400,
        "unicode names longer than 64 characters should be rejected"
    );

    let beta_id = create_tag(&app, &token, "Beta", "#16a34a").await;

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/tags/{beta_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": " ALPHA " }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "patch should reject duplicate normalized names"
    );

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/tags/{alpha_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "color": " #00AAFF " }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "Alpha");
    assert_eq!(body["data"]["color"], "#00aaff");

    let req = test::TestRequest::get()
        .uri("/api/v1/tags?q=alp")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let items = body["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"].as_i64(), Some(alpha_id));
}

#[actix_web::test]
async fn test_tags_replace_deduplicates_clears_and_enforces_limits() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_named!(app, token, "replace-edge.txt");
    let first_tag = create_tag(&app, &token, "First", "#2563eb").await;
    let second_tag = create_tag(&app, &token, "Second", "#f97316").await;

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/tags/file/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "tag_ids": [first_tag, first_tag, second_tag] }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let tags = body["data"]["tags"].as_array().unwrap();
    assert_eq!(tags.len(), 2, "replace should deduplicate tag ids");

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/tags/file/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "tag_ids": [] }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["tags"].as_array().unwrap().len(), 0);

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/tags/file/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "tag_ids": [0] }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let too_many = (1..=65).collect::<Vec<i64>>();
    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/tags/file/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "tag_ids": too_many }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_tags_batch_attach_detach_file_and_folder_usage_counts() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_named!(app, token, "batch-tagged.txt");
    let folder_id = create_folder_at(&app, &token, "/api/v1/folders", "Tagged Folder").await;
    let tag_id = create_tag(&app, &token, "Batch", "#0891b2").await;

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/tags/{tag_id}/batch"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "file_ids": [file_id], "folder_ids": [folder_id] }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/tags")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["items"][0]["usage_count"].as_u64(), Some(2));

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/tags/folder/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["tags"][0]["id"].as_i64(), Some(tag_id));

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/search?tag_ids={tag_id}&type=folder"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let folders = body["data"]["folders"].as_array().unwrap();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0]["id"].as_i64(), Some(folder_id));
    assert_eq!(folders[0]["tags"][0]["id"].as_i64(), Some(tag_id));

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/tags/{tag_id}/batch"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "file_ids": [file_id], "folder_ids": [folder_id] }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/tags")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["items"][0]["usage_count"].as_u64(), Some(0));
}

#[actix_web::test]
async fn test_tag_search_any_all_and_invalid_filters() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);
    let both_id = upload_test_file_named!(app, token, "both-tag-search.txt");
    let first_only_id = upload_test_file_named!(app, token, "first-only-search.txt");
    let first_tag = create_tag(&app, &token, "Search One", "#2563eb").await;
    let second_tag = create_tag(&app, &token, "Search Two", "#db2777").await;

    for (file_id, tag_ids) in [
        (both_id, vec![first_tag, second_tag]),
        (first_only_id, vec![first_tag]),
    ] {
        let req = test::TestRequest::put()
            .uri(&format!("/api/v1/tags/file/{file_id}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({ "tag_ids": tag_ids }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/search?tag_ids={first_tag},{second_tag}&tag_match=any&type=file"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let mut result_ids = body["data"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["id"].as_i64().unwrap())
        .collect::<Vec<_>>();
    result_ids.sort_unstable();
    assert_eq!(result_ids, vec![both_id, first_only_id]);

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/search?tag_ids={first_tag},{second_tag}&tag_match=all&type=file"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let files = body["data"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["id"].as_i64(), Some(both_id));
    assert_eq!(files[0]["tags"].as_array().unwrap().len(), 2);

    for query in [
        "tag_ids=",
        "tag_ids=1,,2",
        "tag_ids=abc",
        "tag_ids=0",
        "tag_ids=1&tag_match=some",
    ] {
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/search?{query}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            400,
            "invalid search query should fail: {query}"
        );
    }

    let too_many = (1..=65)
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/search?tag_ids={too_many}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_tags_are_isolated_between_users_and_batch_is_atomic() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    let (first_token, _) = register_and_login!(app);
    let second_user_id = admin_create_user!(
        app,
        first_token,
        "tagiso",
        "tagiso@example.com",
        "password123"
    );
    assert!(second_user_id > 0);
    let (second_token, _) = login_user!(app, "tagiso", "password123");

    let first_file_id = upload_test_file_named!(app, first_token, "owner-file.txt");
    let second_file_id = upload_test_file_named!(app, second_token, "other-file.txt");
    let first_tag = create_tag(&app, &first_token, "Private", "#475569").await;

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/tags/{first_tag}/file/{second_file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&first_token)))
        .insert_header(common::csrf_header_for(&first_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        matches!(resp.status(), StatusCode::FORBIDDEN | StatusCode::NOT_FOUND),
        "attaching a tag to another user's file must fail"
    );

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/tags/{first_tag}/batch"))
        .insert_header(("Cookie", common::access_cookie_header(&first_token)))
        .insert_header(common::csrf_header_for(&first_token))
        .set_json(
            serde_json::json!({ "file_ids": [first_file_id, second_file_id], "folder_ids": [] }),
        )
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        matches!(resp.status(), StatusCode::FORBIDDEN | StatusCode::NOT_FOUND),
        "batch with an inaccessible entity must fail"
    );

    let prop = property_repo::find_by_key(
        state.writer_db(),
        EntityType::File,
        first_file_id,
        TAG_PROPERTY_NAMESPACE,
        &first_tag.to_string(),
    )
    .await
    .unwrap();
    assert!(
        prop.is_none(),
        "failed batch attach must not partially write"
    );

    let req = test::TestRequest::get()
        .uri("/api/v1/tags")
        .insert_header(("Cookie", common::access_cookie_header(&second_token)))
        .insert_header(common::csrf_header_for(&second_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["items"].as_array().unwrap().len(),
        0,
        "tags must not leak between personal workspaces"
    );

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/search?tag_ids={first_tag}&type=file"))
        .insert_header(("Cookie", common::access_cookie_header(&second_token)))
        .insert_header(common::csrf_header_for(&second_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        404,
        "search should reject tag ids outside the active scope"
    );
}

#[actix_web::test]
async fn test_team_tags_require_manager_for_write_and_member_can_read() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let owner_id = admin_create_user!(
        app,
        admin_token,
        "tagowner",
        "tagowner@example.com",
        "password123"
    );
    let member_id = admin_create_user!(
        app,
        admin_token,
        "tagmember",
        "tagmember@example.com",
        "password123"
    );
    let (owner_token, _) = login_user!(app, "tagowner", "password123");
    let (member_token, _) = login_user!(app, "tagmember", "password123");
    assert!(owner_id > 0);

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({ "name": "Tag Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, 201, "create team failed: {body:?}");
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/members"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({ "user_id": owner_id, "role": "owner" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/members"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({ "user_id": member_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/tags"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .set_json(serde_json::json!({ "name": "Nope", "color": "#64748b" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/tags"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Team Tag", "color": "#0f766e" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/tags"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["items"][0]["name"], "Team Tag");
    assert_eq!(body["data"]["total"].as_u64(), Some(1));
}

#[actix_web::test]
async fn test_team_tags_attach_to_team_entities_and_reject_cross_scope_tags() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let owner_id = admin_create_user!(
        app,
        admin_token,
        "teamscopeowner",
        "teamscopeowner@example.com",
        "password123"
    );
    assert!(owner_id > 0);
    let (owner_token, _) = login_user!(app, "teamscopeowner", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({ "name": "Tag Scope Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/members"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({ "user_id": owner_id, "role": "owner" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let team_folder_id = create_folder_at(
        &app,
        &owner_token,
        &format!("/api/v1/teams/{team_id}/folders"),
        "Team Tagged Folder",
    )
    .await;
    let team_file_id = upload_file_at(
        &app,
        &owner_token,
        &format!("/api/v1/teams/{team_id}/files/upload?folder_id={team_folder_id}"),
        "team-tagged.txt",
        "team content",
    )
    .await;

    let personal_tag = create_tag(&app, &owner_token, "Personal Only", "#334155").await;
    let team_tag = create_tag_at(
        &app,
        &owner_token,
        &format!("/api/v1/teams/{team_id}/tags"),
        "Team Only",
        "#0d9488",
    )
    .await;

    let req = test::TestRequest::put()
        .uri(&format!(
            "/api/v1/teams/{team_id}/tags/{personal_tag}/file/{team_file_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        404,
        "team endpoints must reject personal tag ids"
    );

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/tags/{team_tag}/folder/{team_folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        404,
        "personal endpoints must reject team tag ids"
    );

    let req = test::TestRequest::put()
        .uri(&format!(
            "/api/v1/teams/{team_id}/tags/{team_tag}/file/{team_file_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["tags"][0]["id"].as_i64(), Some(team_tag));

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/teams/{team_id}/search?tag_ids={team_tag}&type=file"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let files = body["data"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["id"].as_i64(), Some(team_file_id));
}
