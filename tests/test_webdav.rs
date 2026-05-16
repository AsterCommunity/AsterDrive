//! 集成测试：`webdav`。

#[macro_use]
mod common;

use actix_web::test;
use actix_web::{App, web};
use aster_drive::db::repository::{file_repo, property_repo};
use aster_drive::types::EntityType;
use base64::Engine;

fn basic_auth_header(username: &str, password: &str) -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"))
    )
}

macro_rules! create_webdav_basic_auth {
    ($app:expr, $token:expr) => {{
        let username = "testuser-webdav";
        let password = "webdav-pass-123";
        let req = test::TestRequest::post()
            .uri("/api/v1/webdav-accounts")
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .set_json(serde_json::json!({
                "username": username,
                "password": password
            }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201, "create WebDAV account should return 201");
        basic_auth_header(username, password)
    }};
}

fn snapshot_dir_tree(
    path: &std::path::Path,
) -> std::io::Result<std::collections::BTreeSet<String>> {
    fn walk(
        root: &std::path::Path,
        current: &std::path::Path,
        entries: &mut std::collections::BTreeSet<String>,
    ) -> std::io::Result<()> {
        for entry in std::fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                entries.insert(format!("{relative}/"));
                walk(root, &path, entries)?;
            } else {
                entries.insert(relative);
            }
        }
        Ok(())
    }

    let mut entries = std::collections::BTreeSet::new();
    if !path.exists() {
        return Ok(entries);
    }
    walk(path, path, &mut entries)?;
    Ok(entries)
}

#[actix_web::test]
async fn test_webdav_propfind_root() {
    let app = setup_with_webdav!();

    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    // PROPFIND 根目录 (Depth: 0)
    let req = test::TestRequest::with_uri("/webdav/")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth))
        .insert_header(("Depth", "0"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207, "PROPFIND root should return 207");
}

#[actix_web::test]
async fn test_webdav_mkcol_and_list() {
    let app = setup_with_webdav!();

    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    // MKCOL 创建目录
    let req = test::TestRequest::with_uri("/webdav/testdir/")
        .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201, "MKCOL should return 201");

    // PROPFIND 根目录 (Depth: 1) — 应包含 testdir
    let req = test::TestRequest::with_uri("/webdav/")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Depth", "1"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);
    assert!(
        xml.contains("testdir"),
        "PROPFIND should list testdir: {xml}"
    );
}

#[actix_web::test]
async fn test_webdav_put_get_delete() {
    let app = setup_with_webdav!();

    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    // PUT 上传文件
    let req = test::TestRequest::put()
        .uri("/webdav/hello.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "text/plain"))
        .set_payload("WebDAV test content")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "PUT should return 201 or 204, got {}",
        resp.status()
    );

    // GET 下载文件
    let req = test::TestRequest::get()
        .uri("/webdav/hello.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "GET should return 200");
    let body = test::read_body(resp).await;
    assert!(
        String::from_utf8_lossy(&body).contains("WebDAV test content"),
        "GET content mismatch"
    );

    // DELETE 删除文件
    let req = test::TestRequest::delete()
        .uri("/webdav/hello.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 200 || resp.status() == 204,
        "DELETE should return 200 or 204, got {}",
        resp.status()
    );

    // GET 应该 404
    let req = test::TestRequest::get()
        .uri("/webdav/hello.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_webdav_empty_put_creates_and_overwrites_file() {
    let app = setup_with_webdav!();

    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/empty.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Length", "0"))
        .set_payload(Vec::<u8>::new())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201, "empty PUT should create the file");

    let req = test::TestRequest::get()
        .uri("/webdav/empty.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "GET after empty PUT should find the file"
    );
    let body = test::read_body(resp).await;
    assert!(body.is_empty(), "empty PUT should store zero bytes");

    let req = test::TestRequest::put()
        .uri("/webdav/empty.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "text/plain"))
        .set_payload("non-empty")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 204, "PUT should overwrite the existing file");

    let req = test::TestRequest::put()
        .uri("/webdav/empty.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Length", "0"))
        .set_payload(Vec::<u8>::new())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        204,
        "empty PUT should overwrite the existing file"
    );

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::HEAD)
        .uri("/webdav/empty.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "HEAD should find the overwritten file");
    assert_eq!(
        resp.headers()
            .get("Content-Length")
            .and_then(|value| value.to_str().ok()),
        Some("0"),
        "empty overwrite should update the stored size"
    );

    let req = test::TestRequest::get()
        .uri("/webdav/empty.txt")
        .insert_header(("Authorization", auth))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "GET should still find exact path");
    let body = test::read_body(resp).await;
    assert!(body.is_empty(), "empty overwrite should store zero bytes");
}

#[actix_web::test]
async fn test_webdav_get_and_head_do_not_create_runtime_temp_files() {
    let state = common::setup().await;
    let runtime_temp_dir =
        aster_drive::utils::paths::runtime_temp_dir(&state.config.server.temp_dir);
    let db1 = state.db.clone();
    let db2 = state.db.clone();
    let webdav_config = aster_drive::config::WebDavConfig::default();
    let app = test::init_service(
        App::new()
            .app_data(web::PayloadConfig::new(10 * 1024 * 1024))
            .app_data(web::JsonConfig::default().limit(1024 * 1024))
            .app_data(web::Data::new(state))
            .configure(move |cfg| {
                aster_drive::webdav::configure(cfg, &webdav_config, &db2);
                aster_drive::api::configure_primary(cfg, &db1);
            }),
    )
    .await;

    let (token, _) = register_and_login!(app);
    upload_test_file_named!(app, token, "streamed-read.txt");
    let auth = create_webdav_basic_auth!(app, token);
    let runtime_path = std::path::Path::new(&runtime_temp_dir);

    let temp_snapshot_before_get = snapshot_dir_tree(runtime_path).unwrap();
    let req = test::TestRequest::get()
        .uri("/webdav/streamed-read.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "GET should return 200");
    let body = test::read_body(resp).await;
    assert!(
        String::from_utf8_lossy(&body).contains("test content"),
        "GET content mismatch"
    );
    let temp_snapshot_after_get = snapshot_dir_tree(runtime_path).unwrap();
    assert_eq!(
        temp_snapshot_after_get, temp_snapshot_before_get,
        "WebDAV GET should stream from storage without creating runtime temp files"
    );

    let temp_snapshot_before_head = snapshot_dir_tree(runtime_path).unwrap();
    let req = test::TestRequest::default()
        .method(actix_web::http::Method::HEAD)
        .uri("/webdav/streamed-read.txt")
        .insert_header(("Authorization", auth))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "HEAD should return 200");
    assert_eq!(
        resp.headers()
            .get("Content-Length")
            .and_then(|value| value.to_str().ok()),
        Some("12"),
        "HEAD should keep the file size header"
    );
    let temp_snapshot_after_head = snapshot_dir_tree(runtime_path).unwrap();
    assert_eq!(
        temp_snapshot_after_head, temp_snapshot_before_head,
        "WebDAV HEAD should not create runtime temp files"
    );
}

#[actix_web::test]
async fn test_webdav_put_local_fast_path_avoids_runtime_temp_files() {
    let state = common::setup().await;
    let runtime_temp_dir =
        aster_drive::utils::paths::runtime_temp_dir(&state.config.server.temp_dir);
    let db1 = state.db.clone();
    let db2 = state.db.clone();
    let webdav_config = aster_drive::config::WebDavConfig::default();
    let app = test::init_service(
        App::new()
            .app_data(web::PayloadConfig::new(10 * 1024 * 1024))
            .app_data(web::JsonConfig::default().limit(1024 * 1024))
            .app_data(web::Data::new(state))
            .configure(move |cfg| {
                aster_drive::webdav::configure(cfg, &webdav_config, &db2);
                aster_drive::api::configure_primary(cfg, &db1);
            }),
    )
    .await;

    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);
    let runtime_path = std::path::Path::new(&runtime_temp_dir);
    let temp_snapshot_before_put = snapshot_dir_tree(runtime_path).unwrap();

    let body = "WebDAV local fast path";
    let req = test::TestRequest::put()
        .uri("/webdav/local-fast-path.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "text/plain"))
        .insert_header(("Content-Length", body.len().to_string()))
        .set_payload(body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "PUT should return 201 or 204, got {}",
        resp.status()
    );

    let temp_snapshot_after_put = snapshot_dir_tree(runtime_path).unwrap();
    assert_eq!(
        temp_snapshot_after_put, temp_snapshot_before_put,
        "WebDAV PUT should use local staging instead of runtime temp when size is known"
    );

    let req = test::TestRequest::get()
        .uri("/webdav/local-fast-path.txt")
        .insert_header(("Authorization", auth))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body = test::read_body(resp).await;
    assert!(
        String::from_utf8_lossy(&body).contains("WebDAV local fast path"),
        "GET content mismatch after local fast-path PUT"
    );
}

#[actix_web::test]
async fn test_webdav_put_without_content_length_avoids_runtime_temp_files() {
    let state = common::setup().await;
    let runtime_temp_dir =
        aster_drive::utils::paths::runtime_temp_dir(&state.config.server.temp_dir);
    let upload_temp_dir = state.config.server.upload_temp_dir.clone();
    let db1 = state.db.clone();
    let db2 = state.db.clone();
    let webdav_config = aster_drive::config::WebDavConfig::default();
    let app = test::init_service(
        App::new()
            .app_data(web::PayloadConfig::new(10 * 1024 * 1024))
            .app_data(web::JsonConfig::default().limit(1024 * 1024))
            .app_data(web::Data::new(state))
            .configure(move |cfg| {
                aster_drive::webdav::configure(cfg, &webdav_config, &db2);
                aster_drive::api::configure_primary(cfg, &db1);
            }),
    )
    .await;

    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);
    let runtime_path = std::path::Path::new(&runtime_temp_dir);
    let upload_path = std::path::Path::new(&upload_temp_dir);
    let runtime_snapshot_before_put = snapshot_dir_tree(runtime_path).unwrap();
    let upload_snapshot_before_put = snapshot_dir_tree(upload_path).unwrap();

    let body = "WebDAV unknown size fallback";
    let req = test::TestRequest::put()
        .uri("/webdav/unknown-size.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "text/plain"))
        .set_payload(body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "PUT should return 201 or 204, got {}",
        resp.status()
    );

    let runtime_snapshot_after_put = snapshot_dir_tree(runtime_path).unwrap();
    assert_eq!(
        runtime_snapshot_after_put, runtime_snapshot_before_put,
        "WebDAV PUT without Content-Length should not create runtime temp files"
    );

    let upload_snapshot_after_put = snapshot_dir_tree(upload_path).unwrap();
    assert_eq!(
        upload_snapshot_after_put, upload_snapshot_before_put,
        "WebDAV fallback staging should be cleaned up from upload temp dir"
    );

    let req = test::TestRequest::get()
        .uri("/webdav/unknown-size.txt")
        .insert_header(("Authorization", auth))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body = test::read_body(resp).await;
    assert!(
        String::from_utf8_lossy(&body).contains("WebDAV unknown size fallback"),
        "GET content mismatch after unknown-size PUT"
    );
}

#[actix_web::test]
async fn test_webdav_runtime_toggle_takes_effect_immediately() {
    use actix_web::{App, web};
    use serde_json::Value;

    let state = common::setup().await;
    let db1 = state.db.clone();
    let db2 = state.db.clone();
    let webdav_config = aster_drive::config::WebDavConfig::default();
    let app = test::init_service(
        App::new()
            .app_data(web::PayloadConfig::new(10 * 1024 * 1024))
            .app_data(web::JsonConfig::default().limit(1024 * 1024))
            .app_data(web::Data::new(state))
            .configure(move |cfg| {
                aster_drive::webdav::configure(cfg, &webdav_config, &db2);
                aster_drive::api::configure_primary(cfg, &db1);
            }),
    )
    .await;

    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::with_uri("/webdav/")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Depth", "0"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/webdav_enabled")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "value": "false" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["key"], "webdav_enabled");
    assert_eq!(body["data"]["value"], "false");

    let req = test::TestRequest::with_uri("/webdav/")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Depth", "0"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 503);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/webdav_enabled")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "value": "true" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::with_uri("/webdav/")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth))
        .insert_header(("Depth", "0"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);
}

#[actix_web::test]
async fn test_webdav_copy_move() {
    let app = setup_with_webdav!();

    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    // PUT 创建源文件
    let req = test::TestRequest::put()
        .uri("/webdav/source.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("copy me")
        .to_request();
    test::call_service(&app, req).await;

    // COPY 复制文件
    let req = test::TestRequest::with_uri("/webdav/source.txt")
        .method(actix_web::http::Method::from_bytes(b"COPY").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/copied.txt"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "COPY should return 201/204, got {}",
        resp.status()
    );

    // 验证副本存在
    let req = test::TestRequest::get()
        .uri("/webdav/copied.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // MOVE 移动文件
    let req = test::TestRequest::with_uri("/webdav/source.txt")
        .method(actix_web::http::Method::from_bytes(b"MOVE").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/moved.txt"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "MOVE should return 201/204, got {}",
        resp.status()
    );

    // 原文件不存在
    let req = test::TestRequest::get()
        .uri("/webdav/source.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);

    // 新位置存在
    let req = test::TestRequest::get()
        .uri("/webdav/moved.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_webdav_copy_folder_recursively() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::with_uri("/webdav/srcdir/")
        .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::with_uri("/webdav/srcdir/sub/")
        .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::put()
        .uri("/webdav/srcdir/sub/nested.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("recursive copy content")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let req = test::TestRequest::with_uri("/webdav/srcdir/")
        .method(actix_web::http::Method::from_bytes(b"COPY").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/copied-dir/"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "COPY folder should return 201/204, got {}",
        resp.status()
    );

    let req = test::TestRequest::get()
        .uri("/webdav/copied-dir/sub/nested.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body = test::read_body(resp).await;
    assert_eq!(String::from_utf8_lossy(&body), "recursive copy content");
}

#[actix_web::test]
async fn test_webdav_move_overwrites_existing_destination() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    for (path, content) in [
        ("/webdav/source-overwrite.txt", "fresh content"),
        ("/webdav/existing-target.txt", "stale content"),
    ] {
        let req = test::TestRequest::put()
            .uri(path)
            .insert_header(("Authorization", auth.clone()))
            .set_payload(content)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status() == 201 || resp.status() == 204);
    }

    let req = test::TestRequest::with_uri("/webdav/source-overwrite.txt")
        .method(actix_web::http::Method::from_bytes(b"MOVE").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/existing-target.txt"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "MOVE overwrite should return 201/204, got {}",
        resp.status()
    );

    let req = test::TestRequest::get()
        .uri("/webdav/source-overwrite.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);

    let req = test::TestRequest::get()
        .uri("/webdav/existing-target.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body = test::read_body(resp).await;
    assert_eq!(String::from_utf8_lossy(&body), "fresh content");
}

#[actix_web::test]
async fn test_webdav_propfind_hides_hidden_artifacts() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    for path in [
        "/webdav/._hidden",
        "/webdav/.DS_Store",
        "/webdav/visible.txt",
    ] {
        let req = test::TestRequest::put()
            .uri(path)
            .insert_header(("Authorization", auth.clone()))
            .set_payload("artifact")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status() == 201 || resp.status() == 204);
    }

    let req = test::TestRequest::with_uri("/webdav/")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Depth", "1"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);

    assert!(
        xml.contains("visible.txt"),
        "visible file should be listed: {xml}"
    );
    assert!(
        !xml.contains("._hidden"),
        "._hidden should be filtered out: {xml}"
    );
    assert!(
        !xml.contains(".DS_Store"),
        ".DS_Store should be filtered out: {xml}"
    );
}

#[actix_web::test]
async fn test_webdav_copy_overwrites_existing_destination() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    for (path, content) in [
        ("/webdav/source-copy.txt", "copy fresh"),
        ("/webdav/existing-copy-target.txt", "copy stale"),
    ] {
        let req = test::TestRequest::put()
            .uri(path)
            .insert_header(("Authorization", auth.clone()))
            .set_payload(content)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status() == 201 || resp.status() == 204);
    }

    let req = test::TestRequest::with_uri("/webdav/source-copy.txt")
        .method(actix_web::http::Method::from_bytes(b"COPY").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/existing-copy-target.txt"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "COPY overwrite should return 201/204, got {}",
        resp.status()
    );

    let req = test::TestRequest::get()
        .uri("/webdav/source-copy.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/webdav/existing-copy-target.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body = test::read_body(resp).await;
    assert_eq!(String::from_utf8_lossy(&body), "copy fresh");
}

#[actix_web::test]
async fn test_webdav_custom_property_roundtrip() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/props.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("props")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let set_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propertyupdate xmlns:D="DAV:" xmlns:A="urn:aster:">
  <D:set>
    <D:prop>
      <A:color>blue</A:color>
    </D:prop>
  </D:set>
</D:propertyupdate>"#;
    let req = test::TestRequest::with_uri("/webdav/props.txt")
        .method(actix_web::http::Method::from_bytes(b"PROPPATCH").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(set_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);

    let propfind_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:" xmlns:A="urn:aster:">
  <D:prop>
    <A:color />
  </D:prop>
</D:propfind>"#;
    let req = test::TestRequest::with_uri("/webdav/props.txt")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Depth", "0"))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(propfind_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);
    assert!(
        xml.contains("blue"),
        "custom property value should roundtrip: {xml}"
    );

    let remove_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propertyupdate xmlns:D="DAV:" xmlns:A="urn:aster:">
  <D:remove>
    <D:prop>
      <A:color />
    </D:prop>
  </D:remove>
</D:propertyupdate>"#;
    let req = test::TestRequest::with_uri("/webdav/props.txt")
        .method(actix_web::http::Method::from_bytes(b"PROPPATCH").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(remove_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);

    let req = test::TestRequest::with_uri("/webdav/props.txt")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Depth", "0"))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(propfind_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);
    assert!(
        !xml.contains(">blue<"),
        "removed property should be absent: {xml}"
    );
}

#[actix_web::test]
async fn test_webdav_proppatch_rejects_dav_namespace_changes() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/dav-props.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("dav")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propertyupdate xmlns:D="DAV:">
  <D:set>
    <D:prop>
      <D:displayname>blocked</D:displayname>
    </D:prop>
  </D:set>
</D:propertyupdate>"#;
    let req = test::TestRequest::with_uri("/webdav/dav-props.txt")
        .method(actix_web::http::Method::from_bytes(b"PROPPATCH").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);
    assert!(
        xml.contains("403") || xml.contains("Forbidden"),
        "DAV namespace writes should be rejected: {xml}"
    );

    let propfind_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname />
  </D:prop>
</D:propfind>"#;
    let req = test::TestRequest::with_uri("/webdav/dav-props.txt")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Depth", "0"))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(propfind_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);
    assert!(
        !xml.contains("blocked"),
        "rejected DAV: property should not be persisted: {xml}"
    );
}

#[actix_web::test]
async fn test_webdav_hides_and_rejects_system_property_namespace() {
    use aster_drive::services::auth_service;

    let state = common::setup().await;
    let db1 = state.db.clone();
    let db2 = state.db.clone();
    let webdav_config = aster_drive::config::WebDavConfig::default();
    let app = test::init_service(
        App::new()
            .wrap(aster_drive::api::middleware::security_headers::default_headers())
            .app_data(web::PayloadConfig::new(10 * 1024 * 1024))
            .app_data(web::JsonConfig::default().limit(1024 * 1024))
            .app_data(web::Data::new(state.clone()))
            .configure(move |cfg| {
                aster_drive::webdav::configure(cfg, &webdav_config, &db2);
                aster_drive::api::configure_primary(cfg, &db1);
            }),
    )
    .await;

    let (token, _) = register_and_login!(app);
    let claims = auth_service::verify_token(&token, &state.config.auth.jwt_secret)
        .expect("access token should verify");
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/system-props.zip")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("zip")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let file =
        file_repo::find_by_name_in_folder(&state.db, claims.user_id, None, "system-props.zip")
            .await
            .expect("file lookup should succeed")
            .expect("uploaded file should exist");
    property_repo::upsert(
        &state.db,
        EntityType::File,
        file.id,
        "system.archive_preview",
        "zip_manifest.v1",
        Some("cached"),
    )
    .await
    .expect("internal system property should be writable through repo");

    let propfind_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:" xmlns:S="system.archive_preview">
  <D:prop>
    <S:zip_manifest.v1 />
  </D:prop>
</D:propfind>"#;
    let req = test::TestRequest::with_uri("/webdav/system-props.zip")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Depth", "0"))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(propfind_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);
    assert!(
        !xml.contains("cached") && xml.contains("zip_manifest.v1") && xml.contains("404"),
        "requested system properties must be reported as missing without exposing values: {xml}"
    );

    let proppatch_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propertyupdate xmlns:D="DAV:" xmlns:S="system.archive_preview">
  <D:set>
    <D:prop>
      <S:zip_manifest.v1>tampered</S:zip_manifest.v1>
    </D:prop>
  </D:set>
</D:propertyupdate>"#;
    let req = test::TestRequest::with_uri("/webdav/system-props.zip")
        .method(actix_web::http::Method::from_bytes(b"PROPPATCH").unwrap())
        .insert_header(("Authorization", auth))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(proppatch_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);
    assert!(
        xml.contains("403") || xml.contains("Forbidden"),
        "system namespace writes should be rejected: {xml}"
    );

    let cached = property_repo::find_by_key(
        &state.db,
        EntityType::File,
        file.id,
        "system.archive_preview",
        "zip_manifest.v1",
    )
    .await
    .expect("system property lookup should succeed")
    .expect("system property should still exist");
    assert_eq!(
        cached.value.as_deref(),
        Some("cached"),
        "rejected PROPPATCH must not overwrite system property"
    );
}

#[actix_web::test]
async fn test_webdav_basic_auth_root_scope() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "scope-root" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    let root_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "inside", "parent_id": root_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "outside" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/webdav-accounts")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "username": "basic-scope-user",
            "password": "basic-scope-pass",
            "root_folder_id": root_id,
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let basic = basic_auth_header("basic-scope-user", "basic-scope-pass");

    let req = test::TestRequest::with_uri("/webdav/")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", basic.clone()))
        .insert_header(("Depth", "1"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);
    assert!(xml.contains("inside"));
    assert!(!xml.contains("outside"));

    let req = test::TestRequest::get()
        .uri("/webdav/outside/")
        .insert_header(("Authorization", basic.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_client_error());
}

#[actix_web::test]
async fn test_webdav_options() {
    let app = setup_with_webdav!();

    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    // OPTIONS 应返回 DAV header
    let req = test::TestRequest::with_uri("/webdav/")
        .method(actix_web::http::Method::OPTIONS)
        .insert_header(("Authorization", auth))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let dav_header = resp
        .headers()
        .get("DAV")
        .map(|v| v.to_str().unwrap_or(""))
        .unwrap_or("");
    assert!(
        dav_header.contains("1"),
        "DAV header should contain '1', got: '{dav_header}'"
    );
}

#[actix_web::test]
async fn test_webdav_lock_unlock() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    // PUT 创建文件
    let req = test::TestRequest::put()
        .uri("/webdav/lockme.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("lock test")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    // LOCK 文件
    let lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
  <D:owner><D:href>testuser</D:href></D:owner>
</D:lockinfo>"#;

    let req = test::TestRequest::with_uri("/webdav/lockme.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Timeout", "Second-3600"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "LOCK should return 200, got {}",
        resp.status()
    );

    // 提取 Lock-Token header
    let lock_token = resp
        .headers()
        .get("Lock-Token")
        .map(|v| v.to_str().unwrap_or("").to_string())
        .unwrap_or_default();
    assert!(
        !lock_token.is_empty(),
        "Lock-Token header should be present"
    );

    // 删除应该失败（被锁了，没提交 token）
    let req = test::TestRequest::delete()
        .uri("/webdav/lockme.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 423 || resp.status() == 403,
        "DELETE locked file should fail, got {}",
        resp.status()
    );

    // UNLOCK
    let req = test::TestRequest::with_uri("/webdav/lockme.txt")
        .method(actix_web::http::Method::from_bytes(b"UNLOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Lock-Token", lock_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 200 || resp.status() == 204,
        "UNLOCK should return 200/204, got {}",
        resp.status()
    );

    // 解锁后删除应该成功
    let req = test::TestRequest::delete()
        .uri("/webdav/lockme.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 200 || resp.status() == 204,
        "DELETE after unlock should succeed, got {}",
        resp.status()
    );
}

#[actix_web::test]
async fn test_webdav_lock_missing_path_returns_not_found_instead_of_locked() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
  <D:owner><D:href>testuser</D:href></D:owner>
</D:lockinfo>"#;

    let req = test::TestRequest::with_uri("/webdav/missing-lock-target.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Timeout", "Second-3600"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        404,
        "LOCK on a missing path should return 404 instead of 423"
    );
}

#[actix_web::test]
async fn test_webdav_unauthorized() {
    let app = setup_with_webdav!();

    // 无认证访问 WebDAV
    let req = test::TestRequest::with_uri("/webdav/")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Depth", "0"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
    assert_eq!(
        resp.headers()
            .get("WWW-Authenticate")
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"AsterDrive WebDAV\"")
    );
}

#[actix_web::test]
async fn test_webdav_bearer_access_token_is_rejected_with_basic_challenge() {
    let app = setup_with_webdav!();
    let (access, _) = register_and_login!(app);

    let req = test::TestRequest::with_uri("/webdav/")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", format!("Bearer {access}")))
        .insert_header(("Depth", "0"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
    assert_eq!(
        resp.headers()
            .get("WWW-Authenticate")
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"AsterDrive WebDAV\"")
    );
}
