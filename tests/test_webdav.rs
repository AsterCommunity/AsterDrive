//! 集成测试：`webdav`。

#[macro_use]
mod common;

use actix_web::test;
use actix_web::{App, HttpServer, web};
use aster_drive::config::{RateLimitConfig, WebDavConfig};
use aster_drive::db::repository::{file_repo, property_repo};
use aster_drive::entities::{team, team_member, user, webdav_account};
use aster_drive::runtime::{PrimaryAppState, SharedRuntimeState};
use aster_drive::types::{EntityType, TeamMemberRole, UserRole, UserStatus};
use base64::Engine;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};
use std::io::Cursor;
use tokio::task::JoinHandle;
use xmltree::Element;

fn basic_auth_header(username: &str, password: &str) -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"))
    )
}

fn webdav_test_username(label: &str) -> String {
    format!("webdav-{label}-{}", uuid::Uuid::new_v4().simple())
}

fn webdav_test_password(label: &str) -> String {
    format!("TEST_PASSWORD_{label}_{}", uuid::Uuid::new_v4().simple())
}

struct RunningWebdavServer {
    base_url: String,
    handle: actix_web::dev::ServerHandle,
    task: JoinHandle<std::io::Result<()>>,
}

impl RunningWebdavServer {
    async fn stop(self) {
        self.handle.stop(true).await;
        let _ = self.task.await;
    }
}

async fn start_real_webdav_server(state: PrimaryAppState) -> RunningWebdavServer {
    let db = state.writer_db().clone();
    let webdav_config = WebDavConfig::default();
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("real WebDAV test server should bind to a random local port");
    let addr = listener
        .local_addr()
        .expect("real WebDAV test server local addr should be available");
    let server = HttpServer::new(move || {
        let db = db.clone();
        let webdav_config = webdav_config.clone();
        App::new()
            .wrap(actix_web::middleware::Compress::default())
            .wrap(aster_drive::api::middleware::security_headers::default_headers())
            .app_data(web::PayloadConfig::new(10 * 1024 * 1024))
            .app_data(web::JsonConfig::default().limit(1024 * 1024))
            .app_data(web::Data::new(state.clone()))
            .configure(move |cfg| aster_drive::webdav::configure(cfg, &webdav_config, &db))
    })
    .listen(listener)
    .expect("real WebDAV test server should listen")
    .run();
    let handle = server.handle();
    let task = tokio::spawn(server);
    RunningWebdavServer {
        base_url: format!("http://{addr}"),
        handle,
        task,
    }
}

async fn seed_real_webdav_account(state: &PrimaryAppState) -> (String, String) {
    let now = Utc::now();
    let default_policy_group =
        aster_drive::db::repository::policy_group_repo::find_default_group(state.writer_db())
            .await
            .expect("default policy group lookup should succeed")
            .expect("default policy group should exist");
    let user = user::ActiveModel {
        username: Set("real-webdav-user".to_string()),
        email: Set("real-webdav-user@example.com".to_string()),
        password_hash: Set("unused".to_string()),
        role: Set(UserRole::User),
        status: Set(UserStatus::Active),
        session_version: Set(0),
        email_verified_at: Set(Some(now)),
        pending_email: Set(None),
        storage_used: Set(0),
        storage_quota: Set(0),
        policy_group_id: Set(Some(default_policy_group.id)),
        created_at: Set(now),
        updated_at: Set(now),
        config: Set(None),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("real WebDAV test user should be inserted");
    state
        .policy_snapshot
        .set_user_policy_group(user.id, default_policy_group.id);

    let username = webdav_test_username("real-account");
    let password = webdav_test_password("REAL_ACCOUNT");
    webdav_account::ActiveModel {
        user_id: Set(user.id),
        username: Set(username.clone()),
        password_hash: Set(aster_drive::utils::hash::hash_password(&password)
            .expect("real WebDAV test password should hash")),
        root_folder_id: Set(None),
        is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("real WebDAV account should be inserted");

    (username, password)
}

async fn seed_real_team_webdav_accounts(
    state: &PrimaryAppState,
) -> (String, String, String, String, i64) {
    let now = Utc::now();
    let default_policy_group =
        aster_drive::db::repository::policy_group_repo::find_default_group(state.writer_db())
            .await
            .expect("default policy group lookup should succeed")
            .expect("default policy group should exist");
    let user = user::ActiveModel {
        username: Set("real-team-webdav-user".to_string()),
        email: Set("real-team-webdav-user@example.com".to_string()),
        password_hash: Set("unused".to_string()),
        role: Set(UserRole::User),
        status: Set(UserStatus::Active),
        session_version: Set(0),
        email_verified_at: Set(Some(now)),
        pending_email: Set(None),
        storage_used: Set(0),
        storage_quota: Set(0),
        policy_group_id: Set(Some(default_policy_group.id)),
        created_at: Set(now),
        updated_at: Set(now),
        config: Set(None),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("real team WebDAV test user should be inserted");
    state
        .policy_snapshot
        .set_user_policy_group(user.id, default_policy_group.id);

    let team = team::ActiveModel {
        name: Set("Real Team WebDAV".to_string()),
        description: Set("WebDAV team workspace".to_string()),
        created_by: Set(user.id),
        storage_used: Set(0),
        storage_quota: Set(0),
        policy_group_id: Set(Some(default_policy_group.id)),
        created_at: Set(now),
        updated_at: Set(now),
        archived_at: Set(None),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("real team WebDAV team should be inserted");

    team_member::ActiveModel {
        team_id: Set(team.id),
        user_id: Set(user.id),
        role: Set(TeamMemberRole::Owner),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("real team WebDAV owner membership should be inserted");

    let personal_username = webdav_test_username("real-team-personal");
    let personal_password = webdav_test_password("REAL_TEAM_PERSONAL");
    webdav_account::ActiveModel {
        user_id: Set(user.id),
        team_id: Set(None),
        username: Set(personal_username.clone()),
        password_hash: Set(aster_drive::utils::hash::hash_password(&personal_password)
            .expect("personal WebDAV password should hash")),
        root_folder_id: Set(None),
        is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("personal WebDAV account should be inserted");

    let team_username = webdav_test_username("real-team-account");
    let team_password = webdav_test_password("REAL_TEAM_ACCOUNT");
    webdav_account::ActiveModel {
        user_id: Set(user.id),
        team_id: Set(Some(team.id)),
        username: Set(team_username.clone()),
        password_hash: Set(aster_drive::utils::hash::hash_password(&team_password)
            .expect("team WebDAV password should hash")),
        root_folder_id: Set(None),
        is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("team WebDAV account should be inserted");

    (
        team_username,
        team_password,
        personal_username,
        personal_password,
        team.id,
    )
}

macro_rules! create_webdav_basic_auth {
    ($app:expr, $token:expr) => {{
        let username = webdav_test_username("basic");
        let password = webdav_test_password("BASIC");
        let req = test::TestRequest::post()
            .uri("/api/v1/webdav-accounts")
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .set_json(serde_json::json!({
                "username": &username,
                "password": &password
            }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201, "create WebDAV account should return 201");
        basic_auth_header(&username, &password)
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

async fn setup_with_custom_webdav_config(
    webdav_config: WebDavConfig,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let rl = RateLimitConfig::default();
    let network_trust = state.config.network_trust.clone();

    test::init_service(
        App::new()
            .wrap(aster_drive::api::middleware::security_headers::default_headers())
            .app_data(web::PayloadConfig::new(1024 * 1024))
            .app_data(web::JsonConfig::default().limit(1024 * 1024))
            .app_data(web::Data::new(state))
            .configure(move |cfg| {
                aster_drive::webdav::configure(cfg, &webdav_config, &db);
                cfg.service(
                    web::scope("/api/v1")
                        .service(aster_drive::api::routes::auth::routes(&rl, &network_trust))
                        .service(aster_drive::api::routes::webdav_accounts::routes(
                            &rl,
                            &network_trust,
                        )),
                );
            }),
    )
    .await
}

async fn setup_webdav_with_runtime_config(
    key: &str,
    value: &str,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    let state = common::setup().await;
    state
        .runtime_config
        .apply(common::system_config_model(key, value));
    let db1 = state.writer_db().clone();
    let db2 = state.writer_db().clone();
    let webdav_config = WebDavConfig::default();

    test::init_service(
        App::new()
            .wrap(aster_drive::api::middleware::security_headers::default_headers())
            .app_data(web::PayloadConfig::new(10 * 1024 * 1024))
            .app_data(web::JsonConfig::default().limit(1024 * 1024))
            .app_data(web::Data::new(state))
            .configure(move |cfg| {
                aster_drive::webdav::configure(cfg, &webdav_config, &db2);
                aster_drive::api::configure_primary(cfg, &db1);
            }),
    )
    .await
}

#[actix_web::test]
async fn test_webdav_propfind_root() {
    let app = setup_with_webdav!();

    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    for uri in ["/webdav", "/webdav/"] {
        // PROPFIND 根目录 (Depth: 0)
        let req = test::TestRequest::with_uri(uri)
            .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
            .insert_header(("Authorization", auth.clone()))
            .insert_header(("Depth", "0"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            207,
            "PROPFIND root should return 207 for {uri}"
        );
    }
}

#[actix_web::test]
async fn test_webdav_proppatch_root_is_explicitly_unsupported() {
    let app = setup_with_webdav!();

    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propertyupdate xmlns:D="DAV:" xmlns:A="urn:aster:">
  <D:set>
    <D:prop>
      <A:color>blue</A:color>
    </D:prop>
  </D:set>
</D:propertyupdate>"#;
    for uri in ["/webdav", "/webdav/"] {
        let req = test::TestRequest::with_uri(uri)
            .method(actix_web::http::Method::from_bytes(b"PROPPATCH").unwrap())
            .insert_header(("Authorization", auth.clone()))
            .insert_header(("Content-Type", "application/xml"))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::FORBIDDEN,
            "PROPPATCH on the WebDAV mount root is intentionally unsupported for {uri}"
        );
        let body = test::read_body(resp).await;
        let text = String::from_utf8_lossy(&body);
        assert!(
            text.contains("mount root"),
            "root PROPPATCH rejection should explain the unsupported target for {uri}: {text}"
        );
    }
}

#[actix_web::test]
async fn test_webdav_propfind_root_custom_dead_property_is_missing() {
    let app = setup_with_webdav!();

    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:" xmlns:A="urn:aster:">
  <D:prop>
    <A:color />
  </D:prop>
</D:propfind>"#;
    let req = test::TestRequest::with_uri("/webdav/")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth))
        .insert_header(("Depth", "0"))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);
    assert!(
        xml.contains("color") && xml.contains("HTTP/1.1 404 Not Found"),
        "root custom dead properties should be reported missing, not persisted: {xml}"
    );
    Element::parse(Cursor::new(xml.as_bytes()))
        .expect("PROPFIND root custom prop XML should parse");
}

#[actix_web::test]
async fn test_webdav_propfind_missing_depth_uses_infinity_semantics() {
    let app = setup_with_webdav!();

    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::with_uri("/webdav/")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        403,
        "missing PROPFIND Depth defaults to infinity; collection infinity is explicitly rejected"
    );
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);
    assert!(
        xml.contains("propfind-finite-depth"),
        "collection Depth: infinity rejection should include RFC 4918 propfind-finite-depth precondition: {xml}"
    );
    Element::parse(Cursor::new(xml.as_bytes())).expect("PROPFIND error XML should parse");
}

#[actix_web::test]
async fn test_webdav_propfind_infinity_file_behaves_as_single_resource() {
    let app = setup_with_webdav!();

    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/propfind-infinity-file.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("file")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let req = test::TestRequest::with_uri("/webdav/propfind-infinity-file.txt")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth))
        .insert_header(("Depth", "infinity"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        207,
        "Depth: infinity on a non-collection resource should not be rejected as a collection traversal"
    );
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
async fn test_webdav_mkcol_body_boundaries() {
    let app = setup_with_webdav!();

    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::with_uri("/webdav/body-empty/")
        .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .set_payload(Vec::<u8>::new())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201, "empty MKCOL body should be accepted");

    let req = test::TestRequest::with_uri("/webdav/body-non-empty/")
        .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .set_payload("x")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        415,
        "non-empty MKCOL body should be rejected"
    );

    let req = test::TestRequest::with_uri("/webdav/body-large/")
        .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .set_payload(vec![b'x'; 2 * 1024 * 1024])
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        415,
        "large non-empty MKCOL body should be rejected without requiring full body collection"
    );
}

#[actix_web::test]
async fn test_webdav_bodyless_methods_reject_non_empty_request_bodies() {
    let app = setup_with_webdav!();

    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/bodyless-source.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("source")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
</D:lockinfo>"#;
    let req = test::TestRequest::with_uri("/webdav/bodyless-source.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Depth", "0"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let lock_token = resp
        .headers()
        .get("Lock-Token")
        .and_then(|value| value.to_str().ok())
        .expect("LOCK response should include Lock-Token")
        .to_string();

    let req = test::TestRequest::with_uri("/webdav/")
        .method(actix_web::http::Method::OPTIONS)
        .insert_header(("Authorization", auth.clone()))
        .set_payload("ignored")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 415, "OPTIONS body must not be ignored");

    let req = test::TestRequest::delete()
        .uri("/webdav/bodyless-source.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("ignored")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 415, "DELETE body must not be ignored");

    let req = test::TestRequest::with_uri("/webdav/bodyless-source.txt")
        .method(actix_web::http::Method::from_bytes(b"COPY").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/bodyless-copy.txt"))
        .set_payload("ignored")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 415, "COPY body must not be ignored");

    let req = test::TestRequest::with_uri("/webdav/bodyless-source.txt")
        .method(actix_web::http::Method::from_bytes(b"MOVE").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/bodyless-move.txt"))
        .set_payload("ignored")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 415, "MOVE body must not be ignored");

    let req = test::TestRequest::with_uri("/webdav/bodyless-source.txt")
        .method(actix_web::http::Method::from_bytes(b"UNLOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Lock-Token", lock_token))
        .set_payload("ignored")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 415, "UNLOCK body must not be ignored");
}

#[actix_web::test]
async fn test_webdav_propfind_rejects_xml_body_over_limit() {
    let webdav_config = WebDavConfig {
        xml_payload_limit: 8,
        ..WebDavConfig::default()
    };
    let app = setup_with_custom_webdav_config(webdav_config).await;

    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);
    let req = test::TestRequest::with_uri("/webdav/")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth))
        .insert_header(("Depth", "0"))
        .set_payload("<propfind />")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::PAYLOAD_TOO_LARGE
    );
}

#[actix_web::test]
async fn test_webdav_xml_methods_reject_body_over_limit() {
    let webdav_config = WebDavConfig {
        xml_payload_limit: 8,
        ..WebDavConfig::default()
    };
    let app = setup_with_custom_webdav_config(webdav_config).await;

    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);
    let over_limit_xml = "<?xml version=\"1.0\"?><D:x xmlns:D=\"DAV:\">too-large</D:x>";

    for method in ["REPORT", "PROPFIND", "PROPPATCH", "LOCK"] {
        let req = test::TestRequest::with_uri("/webdav/")
            .method(actix_web::http::Method::from_bytes(method.as_bytes()).unwrap())
            .insert_header(("Authorization", auth.clone()))
            .insert_header(("Depth", "0"))
            .insert_header(("Content-Type", "application/xml"))
            .set_payload(over_limit_xml)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::PAYLOAD_TOO_LARGE,
            "{method} should reject XML bodies over webdav.xml_payload_limit"
        );
    }
}

#[actix_web::test]
async fn test_webdav_small_xml_methods_still_reach_handlers() {
    let webdav_config = WebDavConfig {
        xml_payload_limit: 128,
        ..WebDavConfig::default()
    };
    let app = setup_with_custom_webdav_config(webdav_config).await;

    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/xml-limit-small.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "text/plain"))
        .insert_header(("Content-Length", "5"))
        .set_payload("hello")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::CREATED);

    let cases = [
        (
            "PROPFIND",
            "/webdav/",
            "<D:propfind xmlns:D=\"DAV:\"/>",
            actix_web::http::StatusCode::MULTI_STATUS,
        ),
        (
            "PROPPATCH",
            "/webdav/xml-limit-small.txt",
            "<D:propertyupdate xmlns:D=\"DAV:\" xmlns:A=\"urn:a\"><D:set><D:prop><A:x>y</A:x></D:prop></D:set></D:propertyupdate>",
            actix_web::http::StatusCode::MULTI_STATUS,
        ),
        (
            "REPORT",
            "/webdav/xml-limit-small.txt",
            "<D:version-tree xmlns:D=\"DAV:\"/>",
            actix_web::http::StatusCode::MULTI_STATUS,
        ),
    ];

    for (method, uri, payload, expected_status) in cases {
        let req = test::TestRequest::with_uri(uri)
            .method(actix_web::http::Method::from_bytes(method.as_bytes()).unwrap())
            .insert_header(("Authorization", auth.clone()))
            .insert_header(("Depth", "0"))
            .insert_header(("Content-Type", "application/xml"))
            .set_payload(payload)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            expected_status,
            "{method} should accept XML bodies within webdav.xml_payload_limit"
        );
    }

    let lock_body = r#"<D:lockinfo xmlns:D="DAV:"><D:lockscope><D:exclusive/></D:lockscope><D:locktype><D:write/></D:locktype></D:lockinfo>"#;
    let req = test::TestRequest::with_uri("/webdav/xml-limit-small.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Timeout", "Second-3600"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::OK,
        "LOCK should accept XML bodies within webdav.xml_payload_limit"
    );
}

#[actix_web::test]
async fn test_webdav_put_is_not_limited_by_xml_body_limit() {
    let webdav_config = WebDavConfig {
        xml_payload_limit: 8,
        ..WebDavConfig::default()
    };
    let app = setup_with_custom_webdav_config(webdav_config).await;

    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);
    let data = vec![b'p'; 32];
    let req = test::TestRequest::put()
        .uri("/webdav/xml-limit-put.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/octet-stream"))
        .insert_header(("Content-Length", data.len().to_string()))
        .set_payload(data.clone())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::CREATED);

    let req = test::TestRequest::get()
        .uri("/webdav/xml-limit-put.txt")
        .insert_header(("Authorization", auth))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let body = test::read_body(resp).await;
    assert_eq!(body.as_ref(), data.as_slice());
}

#[actix_web::test]
async fn test_webdav_get_supports_binary_range_requests() {
    let app = setup_with_webdav!();

    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);
    let data: Vec<u8> = (0..=255).cycle().take(4099).collect();
    let req = test::TestRequest::put()
        .uri("/webdav/range-image.bin")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/octet-stream"))
        .insert_header(("Content-Length", data.len().to_string()))
        .set_payload(data.clone())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::CREATED);

    let req = test::TestRequest::get()
        .uri("/webdav/range-image.bin")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Range", "bytes=1024-2047"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::PARTIAL_CONTENT,
        "WebDAV GET should honor byte ranges"
    );
    assert_eq!(
        resp.headers()
            .get("Content-Range")
            .and_then(|value| value.to_str().ok()),
        Some("bytes 1024-2047/4099")
    );
    assert_eq!(
        resp.headers()
            .get("Accept-Ranges")
            .and_then(|value| value.to_str().ok()),
        Some("bytes")
    );
    assert_eq!(
        resp.headers()
            .get("Content-Encoding")
            .and_then(|value| value.to_str().ok()),
        Some("identity")
    );
    let body = test::read_body(resp).await;
    assert_eq!(body.as_ref(), &data[1024..=2047]);

    let req = test::TestRequest::get()
        .uri("/webdav/range-image.bin")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Range", "bytes=-9"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::PARTIAL_CONTENT);
    let body = test::read_body(resp).await;
    assert_eq!(body.as_ref(), &data[data.len() - 9..]);

    let req = test::TestRequest::get()
        .uri("/webdav/range-image.bin")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Range", "bytes=4099-5000"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::RANGE_NOT_SATISFIABLE,
        "GET with an unsatisfiable byte range should return 416"
    );
    assert_eq!(
        resp.headers()
            .get("Content-Range")
            .and_then(|value| value.to_str().ok()),
        Some("bytes */4099"),
        "GET 416 must report the current representation length"
    );

    let req = test::TestRequest::with_uri("/webdav/range-image.bin")
        .method(actix_web::http::Method::HEAD)
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Range", "bytes=4099-5000"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::OK,
        "HEAD should ignore Range instead of returning 416"
    );
    assert_eq!(
        resp.headers()
            .get("Content-Length")
            .and_then(|value| value.to_str().ok()),
        Some("4099")
    );

    let req = test::TestRequest::get()
        .uri("/webdav/range-image.bin")
        .insert_header(("Authorization", auth))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let body = test::read_body(resp).await;
    assert_eq!(body.as_ref(), data.as_slice());
}

#[actix_web::test]
async fn test_webdav_real_http_put_with_content_length_persists_bytes() {
    let state = common::setup().await;
    let (username, password) = seed_real_webdav_account(&state).await;
    let server = start_real_webdav_server(state).await;
    let client = reqwest::Client::new();
    let data: Vec<u8> = (0..=255).cycle().take(8193).collect();

    let put = client
        .put(format!("{}/webdav/finder-length.bin", server.base_url))
        .basic_auth(&username, Some(&password))
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .header(reqwest::header::CONTENT_LENGTH, data.len().to_string())
        .body(data.clone())
        .send()
        .await
        .expect("real WebDAV PUT with Content-Length should receive a response");
    let put_status = put.status();
    let put_body = put
        .text()
        .await
        .expect("real WebDAV PUT error body should be readable");
    assert!(
        put_status == reqwest::StatusCode::CREATED || put_status == reqwest::StatusCode::NO_CONTENT,
        "real WebDAV PUT should create or overwrite the file, got {put_status}: {put_body}"
    );

    let get = client
        .get(format!("{}/webdav/finder-length.bin", server.base_url))
        .basic_auth(&username, Some(&password))
        .send()
        .await
        .expect("real WebDAV GET should receive a response");
    assert_eq!(get.status(), reqwest::StatusCode::OK);
    let bytes = get.bytes().await.expect("real WebDAV GET body should read");
    assert_eq!(bytes.as_ref(), data.as_slice());

    server.stop().await;
}

#[actix_web::test]
async fn test_webdav_real_http_rclone_propfind_reports_uploaded_size_with_declared_prefix() {
    let state = common::setup().await;
    let (username, password) = seed_real_webdav_account(&state).await;
    let server = start_real_webdav_server(state).await;
    let client = reqwest::Client::new();
    let data: Vec<u8> = (0..=255).cycle().take(129106).collect();
    let url = format!("{}/webdav/rclone-size-check.bin", server.base_url);

    let put = client
        .put(&url)
        .basic_auth(&username, Some(&password))
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .header(reqwest::header::CONTENT_LENGTH, data.len().to_string())
        .body(data)
        .send()
        .await
        .expect("real WebDAV PUT for rclone PROPFIND check should receive a response");
    assert!(
        put.status() == reqwest::StatusCode::CREATED
            || put.status() == reqwest::StatusCode::NO_CONTENT,
        "real WebDAV PUT should create or overwrite the file, got {}",
        put.status()
    );

    let propfind_body = r#"<?xml version="1.0"?>
<d:propfind xmlns:d="DAV:">
 <d:prop>
  <d:displayname/>
  <d:getlastmodified/>
  <d:getcontentlength/>
  <d:resourcetype/>
 </d:prop>
</d:propfind>"#;
    let propfind = client
        .request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), &url)
        .basic_auth(&username, Some(&password))
        .header("Depth", "0")
        .header(reqwest::header::CONTENT_TYPE, "application/xml")
        .body(propfind_body)
        .send()
        .await
        .expect("rclone-style PROPFIND should receive a response");
    assert_eq!(propfind.status(), reqwest::StatusCode::MULTI_STATUS);
    let body = propfind
        .text()
        .await
        .expect("rclone-style PROPFIND response body should be readable");
    assert!(
        body.contains("xmlns:d=\"DAV:\""),
        "PROPFIND response must declare the lowercase DAV prefix used in echoed props: {body}"
    );
    assert!(
        body.contains("<d:getcontentlength xmlns:d=\"DAV:\">129106</d:getcontentlength>"),
        "PROPFIND response should report uploaded size under the requested DAV prefix: {body}"
    );
    Element::parse(Cursor::new(body.as_bytes())).expect("PROPFIND response XML should parse");

    server.stop().await;
}

#[actix_web::test]
async fn test_webdav_real_http_chunked_put_persists_bytes() {
    let state = common::setup().await;
    let (username, password) = seed_real_webdav_account(&state).await;
    let server = start_real_webdav_server(state).await;
    let client = reqwest::Client::new();
    let data: Vec<u8> = (0..=251).cycle().take(16 * 1024 + 17).collect();
    let chunks = futures::stream::iter(
        data.chunks(1024)
            .map(|chunk| Ok::<_, std::io::Error>(bytes::Bytes::copy_from_slice(chunk)))
            .collect::<Vec<_>>(),
    );

    let put = client
        .put(format!("{}/webdav/finder-chunked.bin", server.base_url))
        .basic_auth(&username, Some(&password))
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .body(reqwest::Body::wrap_stream(chunks))
        .send()
        .await
        .expect("real WebDAV chunked PUT should receive a response");
    let put_status = put.status();
    let put_body = put
        .text()
        .await
        .expect("real WebDAV chunked PUT error body should be readable");
    assert!(
        put_status == reqwest::StatusCode::CREATED || put_status == reqwest::StatusCode::NO_CONTENT,
        "real WebDAV chunked PUT should create or overwrite the file, got {put_status}: {put_body}"
    );

    let get = client
        .get(format!("{}/webdav/finder-chunked.bin", server.base_url))
        .basic_auth(&username, Some(&password))
        .send()
        .await
        .expect("real WebDAV GET after chunked PUT should receive a response");
    assert_eq!(get.status(), reqwest::StatusCode::OK);
    let bytes = get
        .bytes()
        .await
        .expect("real WebDAV GET after chunked PUT body should read");
    assert_eq!(bytes.as_ref(), data.as_slice());

    server.stop().await;
}

#[actix_web::test]
async fn test_webdav_real_http_finder_expected_length_put_persists_bytes() {
    let state = common::setup().await;
    let (username, password) = seed_real_webdav_account(&state).await;
    let server = start_real_webdav_server(state).await;
    let client = reqwest::Client::new();
    let data: Vec<u8> = (0..=253).cycle().take(32 * 1024 + 29).collect();
    let chunks = futures::stream::iter(
        data.chunks(2048)
            .map(|chunk| Ok::<_, std::io::Error>(bytes::Bytes::copy_from_slice(chunk)))
            .collect::<Vec<_>>(),
    );

    let put = client
        .put(format!(
            "{}/webdav/finder-expected-length.bin",
            server.base_url
        ))
        .basic_auth(&username, Some(&password))
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .header("X-Expected-Entity-Length", data.len().to_string())
        .body(reqwest::Body::wrap_stream(chunks))
        .send()
        .await
        .expect("Finder-style expected-length PUT should receive a response");
    assert!(
        put.status() == reqwest::StatusCode::CREATED
            || put.status() == reqwest::StatusCode::NO_CONTENT,
        "Finder-style expected-length PUT should create or overwrite the file, got {}",
        put.status()
    );

    let get = client
        .get(format!(
            "{}/webdav/finder-expected-length.bin",
            server.base_url
        ))
        .basic_auth(&username, Some(&password))
        .send()
        .await
        .expect("Finder-style expected-length GET should receive a response");
    assert_eq!(get.status(), reqwest::StatusCode::OK);
    let bytes = get
        .bytes()
        .await
        .expect("Finder-style expected-length GET body should read");
    assert_eq!(bytes.as_ref(), data.as_slice());

    server.stop().await;
}

#[actix_web::test]
async fn test_webdav_real_http_expected_length_mismatch_does_not_create_empty_file() {
    let state = common::setup().await;
    let (username, password) = seed_real_webdav_account(&state).await;
    let server = start_real_webdav_server(state).await;
    let client = reqwest::Client::new();

    let put = client
        .put(format!("{}/webdav/finder-empty-shell.bin", server.base_url))
        .basic_auth(&username, Some(&password))
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .header("X-Expected-Entity-Length", "4096")
        .body(Vec::<u8>::new())
        .send()
        .await
        .expect("mismatched expected-length PUT should receive a response");
    assert_eq!(
        put.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "mismatched expected length must be rejected instead of creating a zero-byte file"
    );

    let get = client
        .get(format!("{}/webdav/finder-empty-shell.bin", server.base_url))
        .basic_auth(&username, Some(&password))
        .send()
        .await
        .expect("GET after mismatched expected-length PUT should receive a response");
    assert_eq!(get.status(), reqwest::StatusCode::NOT_FOUND);

    server.stop().await;
}

#[actix_web::test]
async fn test_webdav_real_http_put_with_own_lock_overwrites_placeholder() {
    let state = common::setup().await;
    let (username, password) = seed_real_webdav_account(&state).await;
    let server = start_real_webdav_server(state).await;
    let client = reqwest::Client::new();
    let url = format!("{}/webdav/finder-locked-overwrite.jar", server.base_url);

    let placeholder = client
        .put(&url)
        .basic_auth(&username, Some(&password))
        .header(reqwest::header::CONTENT_LENGTH, "0")
        .body(Vec::<u8>::new())
        .send()
        .await
        .expect("Finder-style placeholder PUT should receive a response");
    assert!(
        placeholder.status() == reqwest::StatusCode::CREATED
            || placeholder.status() == reqwest::StatusCode::NO_CONTENT,
        "placeholder PUT should create or overwrite the file, got {}",
        placeholder.status()
    );

    let lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
  <D:owner><D:href>finder</D:href></D:owner>
</D:lockinfo>"#;
    let lock = client
        .request(reqwest::Method::from_bytes(b"LOCK").unwrap(), &url)
        .basic_auth(&username, Some(&password))
        .header(reqwest::header::CONTENT_TYPE, "application/xml")
        .header("Timeout", "Second-3600")
        .body(lock_body)
        .send()
        .await
        .expect("LOCK before Finder overwrite should receive a response");
    assert_eq!(lock.status(), reqwest::StatusCode::OK);
    let lock_token = lock
        .headers()
        .get("Lock-Token")
        .and_then(|value| value.to_str().ok())
        .expect("LOCK response should include Lock-Token")
        .to_string();
    let submitted_lock_token = lock_token.trim_matches(|c| c == '<' || c == '>');

    let data: Vec<u8> = (0..=251).cycle().take(128 * 1024 + 31).collect();
    let overwrite = client
        .put(&url)
        .basic_auth(&username, Some(&password))
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .header(reqwest::header::CONTENT_LENGTH, data.len().to_string())
        .header("If", format!("(<{submitted_lock_token}>)"))
        .body(data.clone())
        .send()
        .await
        .expect("PUT with own WebDAV lock token should receive a response");
    let overwrite_status = overwrite.status();
    let overwrite_body = overwrite
        .text()
        .await
        .expect("locked overwrite response body should be readable");
    assert!(
        overwrite_status == reqwest::StatusCode::CREATED
            || overwrite_status == reqwest::StatusCode::NO_CONTENT,
        "PUT with own WebDAV lock token should overwrite the placeholder, got {overwrite_status}: {overwrite_body}"
    );

    let get = client
        .get(&url)
        .basic_auth(&username, Some(&password))
        .send()
        .await
        .expect("GET after locked overwrite should receive a response");
    assert_eq!(get.status(), reqwest::StatusCode::OK);
    let bytes = get
        .bytes()
        .await
        .expect("GET after locked overwrite body should read");
    assert_eq!(bytes.as_ref(), data.as_slice());

    let unlock = client
        .request(reqwest::Method::from_bytes(b"UNLOCK").unwrap(), &url)
        .basic_auth(&username, Some(&password))
        .header("Lock-Token", lock_token)
        .send()
        .await
        .expect("UNLOCK after locked overwrite should receive a response");
    assert!(
        unlock.status() == reqwest::StatusCode::NO_CONTENT
            || unlock.status() == reqwest::StatusCode::OK,
        "UNLOCK should succeed after locked overwrite, got {}",
        unlock.status()
    );

    server.stop().await;
}

#[actix_web::test]
async fn test_webdav_real_http_team_account_uses_team_workspace() {
    let state = common::setup().await;
    let (team_username, team_password, personal_username, personal_password, _team_id) =
        seed_real_team_webdav_accounts(&state).await;
    let server = start_real_webdav_server(state).await;
    let client = reqwest::Client::new();
    let url = format!("{}/webdav/team-scope.bin", server.base_url);
    let data: Vec<u8> = (0..=247).cycle().take(64 * 1024 + 13).collect();

    let put = client
        .put(&url)
        .basic_auth(&team_username, Some(&team_password))
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .header(reqwest::header::CONTENT_LENGTH, data.len().to_string())
        .body(data.clone())
        .send()
        .await
        .expect("team WebDAV PUT should receive a response");
    let put_status = put.status();
    let put_body = put
        .text()
        .await
        .expect("team WebDAV PUT response body should be readable");
    assert!(
        put_status == reqwest::StatusCode::CREATED || put_status == reqwest::StatusCode::NO_CONTENT,
        "team WebDAV PUT should create the file, got {put_status}: {put_body}"
    );

    let team_get = client
        .get(&url)
        .basic_auth(&team_username, Some(&team_password))
        .send()
        .await
        .expect("team WebDAV GET should receive a response");
    assert_eq!(team_get.status(), reqwest::StatusCode::OK);
    let bytes = team_get
        .bytes()
        .await
        .expect("team WebDAV GET body should read");
    assert_eq!(bytes.as_ref(), data.as_slice());

    let personal_get = client
        .get(&url)
        .basic_auth(&personal_username, Some(&personal_password))
        .send()
        .await
        .expect("personal WebDAV GET should receive a response");
    assert_eq!(
        personal_get.status(),
        reqwest::StatusCode::NOT_FOUND,
        "personal account must not see files written through a team WebDAV account"
    );

    server.stop().await;
}

#[actix_web::test]
async fn test_webdav_real_http_team_account_is_rejected_after_team_archive() {
    let state = common::setup().await;
    let (team_username, team_password, _, _, team_id) =
        seed_real_team_webdav_accounts(&state).await;
    let server = start_real_webdav_server(state.clone()).await;
    let client = reqwest::Client::new();
    let url = format!("{}/webdav/team-archive-check.bin", server.base_url);

    let before_archive = client
        .put(&url)
        .basic_auth(&team_username, Some(&team_password))
        .header(reqwest::header::CONTENT_LENGTH, "5")
        .body("ready".to_string())
        .send()
        .await
        .expect("team WebDAV PUT before archive should receive a response");
    assert!(
        before_archive.status() == reqwest::StatusCode::CREATED
            || before_archive.status() == reqwest::StatusCode::NO_CONTENT,
        "team WebDAV account should work before archive, got {}",
        before_archive.status()
    );

    let team = aster_drive::db::repository::team_repo::find_by_id(state.writer_db(), team_id)
        .await
        .expect("team should exist");
    let mut active_team = team.into_active_model();
    active_team.archived_at = Set(Some(Utc::now()));
    active_team
        .update(state.writer_db())
        .await
        .expect("team should archive");
    let after_archive = client
        .get(&url)
        .basic_auth(&team_username, Some(&team_password))
        .send()
        .await
        .expect("team WebDAV GET after archive should receive a response");
    assert_eq!(
        after_archive.status(),
        reqwest::StatusCode::UNAUTHORIZED,
        "archived team WebDAV account must be rejected at auth boundary"
    );

    server.stop().await;
}

#[actix_web::test]
async fn test_webdav_real_http_large_chunked_put_uses_webdav_payload_limit() {
    let state = common::setup().await;
    let (username, password) = seed_real_webdav_account(&state).await;
    let server = start_real_webdav_server(state).await;
    let client = reqwest::Client::new();
    let data: Vec<u8> = (0..=250).cycle().take(11 * 1024 * 1024).collect();
    let chunks = futures::stream::iter(
        data.chunks(64 * 1024)
            .map(|chunk| Ok::<_, std::io::Error>(bytes::Bytes::copy_from_slice(chunk)))
            .collect::<Vec<_>>(),
    );

    let put = client
        .put(format!(
            "{}/webdav/finder-large-chunked.bin",
            server.base_url
        ))
        .basic_auth(&username, Some(&password))
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .body(reqwest::Body::wrap_stream(chunks))
        .send()
        .await
        .expect("large real WebDAV chunked PUT should receive a response");
    let put_status = put.status();
    let put_body = put
        .text()
        .await
        .expect("large real WebDAV chunked PUT error body should be readable");
    assert!(
        put_status == reqwest::StatusCode::CREATED || put_status == reqwest::StatusCode::NO_CONTENT,
        "large real WebDAV chunked PUT should create or overwrite the file, got {put_status}: {put_body}"
    );

    let get = client
        .get(format!(
            "{}/webdav/finder-large-chunked.bin",
            server.base_url
        ))
        .basic_auth(&username, Some(&password))
        .header(reqwest::header::RANGE, "bytes=10485760-10485887")
        .send()
        .await
        .expect("large real WebDAV range GET should receive a response");
    assert_eq!(get.status(), reqwest::StatusCode::PARTIAL_CONTENT);
    let bytes = get
        .bytes()
        .await
        .expect("large real WebDAV range GET body should read");
    assert_eq!(
        bytes.as_ref(),
        &data[10 * 1024 * 1024..10 * 1024 * 1024 + 128]
    );

    server.stop().await;
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
async fn test_webdav_put_existing_collection_returns_method_not_allowed() {
    let app = setup_with_webdav!();

    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::with_uri("/webdav/put-collection/")
        .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::put()
        .uri("/webdav/put-collection/")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("not a collection")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        405,
        "PUT to an existing collection should be rejected explicitly"
    );

    let req = test::TestRequest::with_uri("/webdav/put-collection/")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Depth", "0"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        207,
        "failed PUT must leave the collection addressable as a collection"
    );
}

#[actix_web::test]
async fn test_webdav_get_and_head_do_not_create_runtime_temp_files() {
    let state = common::setup().await;
    let runtime_temp_dir =
        aster_drive::utils::paths::runtime_temp_dir(&state.config.server.temp_dir);
    let db1 = state.writer_db().clone();
    let db2 = state.writer_db().clone();
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
    let db1 = state.writer_db().clone();
    let db2 = state.writer_db().clone();
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
    let db1 = state.writer_db().clone();
    let db2 = state.writer_db().clone();
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
    let db1 = state.writer_db().clone();
    let db2 = state.writer_db().clone();
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
    assert_eq!(
        resp.headers()
            .get("Cache-Control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store"),
        "COPY responses must not be cacheable"
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
    assert_eq!(
        resp.headers()
            .get("Cache-Control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store"),
        "MOVE responses must not be cacheable"
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
async fn test_webdav_copy_move_rejects_similar_destination_prefix() {
    let app = setup_with_webdav!();

    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    for (method, source, destination) in [
        (
            "COPY",
            "/webdav/copy-prefix-source.txt",
            "/webdav-team/copied.txt",
        ),
        (
            "MOVE",
            "/webdav/move-prefix-source.txt",
            "/webdav-team/moved.txt",
        ),
    ] {
        let req = test::TestRequest::put()
            .uri(source)
            .insert_header(("Authorization", auth.clone()))
            .set_payload("prefix boundary")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status() == 201 || resp.status() == 204);

        let req = test::TestRequest::with_uri(source)
            .method(actix_web::http::Method::from_bytes(method.as_bytes()).unwrap())
            .insert_header(("Authorization", auth.clone()))
            .insert_header(("Destination", destination))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            400,
            "{method} should reject destination outside the WebDAV prefix"
        );

        let req = test::TestRequest::get()
            .uri(source)
            .insert_header(("Authorization", auth.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }
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
async fn test_webdav_copy_folder_depth_zero_copies_collection_without_children() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    for uri in ["/webdav/shallow-src/", "/webdav/shallow-src/sub/"] {
        let req = test::TestRequest::with_uri(uri)
            .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
            .insert_header(("Authorization", auth.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    let req = test::TestRequest::put()
        .uri("/webdav/shallow-src/sub/nested.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("depth zero should not copy this")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let req = test::TestRequest::with_uri("/webdav/shallow-src/")
        .method(actix_web::http::Method::from_bytes(b"COPY").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/shallow-dst/"))
        .insert_header(("Depth", "0"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "COPY Depth: 0 folder should create the destination collection, got {}",
        resp.status()
    );

    let req = test::TestRequest::with_uri("/webdav/shallow-dst/")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Depth", "0"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);

    let req = test::TestRequest::get()
        .uri("/webdav/shallow-dst/sub/nested.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        404,
        "COPY Depth: 0 must not copy collection descendants"
    );
}

#[actix_web::test]
async fn test_webdav_copy_folder_rejects_destination_inside_source_tree() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    for uri in ["/webdav/self-copy/", "/webdav/self-copy/sub/"] {
        let req = test::TestRequest::with_uri(uri)
            .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
            .insert_header(("Authorization", auth.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    let req = test::TestRequest::with_uri("/webdav/self-copy/")
        .method(actix_web::http::Method::from_bytes(b"COPY").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/self-copy/sub/copy/"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        403,
        "recursive collection COPY into its own tree must be rejected"
    );

    let req = test::TestRequest::with_uri("/webdav/self-copy/sub/copy/")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Depth", "0"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        404,
        "rejected self-COPY must not create the destination subtree"
    );
}

#[actix_web::test]
async fn test_webdav_move_and_delete_reject_non_infinity_depth_headers() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::with_uri("/webdav/depth-dir/")
        .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::with_uri("/webdav/depth-dir/")
        .method(actix_web::http::Method::from_bytes(b"MOVE").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/depth-moved-dir/"))
        .insert_header(("Depth", "0"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let req = test::TestRequest::delete()
        .uri("/webdav/depth-dir/")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Depth", "0"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let req = test::TestRequest::get()
        .uri("/webdav/depth-dir/")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 405);
}

#[actix_web::test]
async fn test_webdav_file_operations_ignore_depth_header() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/depth-delete-file.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("delete")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let req = test::TestRequest::delete()
        .uri("/webdav/depth-delete-file.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Depth", "0"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        204,
        "Depth header must be ignored for DELETE on non-collection resources"
    );

    let req = test::TestRequest::put()
        .uri("/webdav/depth-move-file.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("move")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let req = test::TestRequest::with_uri("/webdav/depth-move-file.txt")
        .method(actix_web::http::Method::from_bytes(b"MOVE").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/depth-moved-file.txt"))
        .insert_header(("Depth", "0"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "Depth header must be ignored for MOVE on non-collection resources, got {}",
        resp.status()
    );

    let req = test::TestRequest::put()
        .uri("/webdav/depth-copy-file.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("copy")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let req = test::TestRequest::with_uri("/webdav/depth-copy-file.txt")
        .method(actix_web::http::Method::from_bytes(b"COPY").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/depth-copied-file.txt"))
        .insert_header(("Depth", "1"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "COPY must reject Depth: 1 per RFC 4918, got {}",
        resp.status()
    );

    let req = test::TestRequest::get()
        .uri("/webdav/depth-copied-file.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404, "rejected COPY must not create target");
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
async fn test_webdav_blocks_system_file_write_targets_by_default() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    for path in [
        "/webdav/.DS_Store",
        "/webdav/Thumbs.db",
        "/webdav/desktop.ini",
    ] {
        let req = test::TestRequest::put()
            .uri(path)
            .insert_header(("Authorization", auth.clone()))
            .set_payload("artifact")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 403, "PUT {path} should be blocked");

        let req = test::TestRequest::get()
            .uri(path)
            .insert_header(("Authorization", auth.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            404,
            "blocked PUT {path} should not create a file"
        );
    }

    for path in ["/webdav/.Spotlight-V100/", "/webdav/$RECYCLE.BIN/"] {
        let req = test::TestRequest::with_uri(path)
            .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
            .insert_header(("Authorization", auth.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 403, "MKCOL {path} should be blocked");
    }

    let req = test::TestRequest::put()
        .uri("/webdav/source-system-block.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("source")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    for (method, destination) in [
        ("COPY", "/webdav/Thumbs.db"),
        ("MOVE", "/webdav/desktop.ini"),
    ] {
        let req = test::TestRequest::with_uri("/webdav/source-system-block.txt")
            .method(actix_web::http::Method::from_bytes(method.as_bytes()).unwrap())
            .insert_header(("Authorization", auth.clone()))
            .insert_header(("Destination", destination))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            403,
            "{method} to {destination} should be blocked"
        );

        let req = test::TestRequest::get()
            .uri(destination)
            .insert_header(("Authorization", auth.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            404,
            "blocked {method} should not create destination {destination}"
        );
    }

    let req = test::TestRequest::get()
        .uri("/webdav/source-system-block.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "blocked MOVE should leave source intact"
    );
}

#[actix_web::test]
async fn test_webdav_system_file_names_are_visible_when_blocking_is_disabled() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/webdav_block_system_files_enabled")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "value": "false" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

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
        assert!(
            resp.status() == 201 || resp.status() == 204,
            "PUT {path} should be allowed after disabling blocking"
        );
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
        xml.contains("._hidden"),
        "._hidden should remain visible when blocking is disabled: {xml}"
    );
    assert!(
        xml.contains(".DS_Store"),
        ".DS_Store should remain visible when blocking is disabled: {xml}"
    );
}

#[actix_web::test]
async fn test_webdav_system_file_patterns_use_json_list_runtime_config() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/webdav_block_system_file_patterns")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "value": ["blocked-*"] }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::put()
        .uri("/webdav/blocked-file.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("blocked")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::put()
        .uri("/webdav/.DS_Store")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("custom patterns replaced defaults")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "custom JSON list should replace default system-file patterns"
    );
}

#[actix_web::test]
async fn test_webdav_empty_system_file_pattern_list_blocks_nothing() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/webdav_block_system_file_patterns")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "value": [] }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    for path in ["/webdav/.DS_Store", "/webdav/Thumbs.db"] {
        let req = test::TestRequest::put()
            .uri(path)
            .insert_header(("Authorization", auth.clone()))
            .set_payload("empty pattern list")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(
            resp.status() == 201 || resp.status() == 204,
            "empty pattern list should allow {path}"
        );
    }
}

#[actix_web::test]
async fn test_webdav_system_file_matching_uses_decoded_basename_only() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::with_uri("/webdav/metadata/")
        .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::put()
        .uri("/webdav/metadata/%2eDS_Store")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("encoded system file")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        403,
        "percent-encoded .DS_Store basename should be blocked"
    );

    let req = test::TestRequest::put()
        .uri("/webdav/metadata/report.DS_Store")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("normal file with similar suffix")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "system-file matching should only compare the basename, not suffix substrings"
    );

    let req = test::TestRequest::put()
        .uri("/webdav/System%20Volume%20Information")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("encoded spaces")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        403,
        "decoded basename with spaces should be blocked"
    );

    let req = test::TestRequest::put()
        .uri("/webdav/metadata/report.docx")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("business file")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "normal nested business file should not be blocked"
    );
}

#[actix_web::test]
async fn test_webdav_existing_system_files_remain_readable_and_deletable() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/webdav_block_system_files_enabled")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "value": "false" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::put()
        .uri("/webdav/.DS_Store")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("historical metadata")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/webdav_block_system_files_enabled")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "value": "true" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/webdav/.DS_Store")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body = test::read_body(resp).await;
    assert_eq!(String::from_utf8_lossy(&body), "historical metadata");

    let req = test::TestRequest::delete()
        .uri("/webdav/.DS_Store")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        204,
        "blocking should not prevent users from deleting historical system files"
    );

    let req = test::TestRequest::get()
        .uri("/webdav/.DS_Store")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
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
async fn test_webdav_copy_file_overwrites_existing_collection_destination() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/file-over-collection-source.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("file replaces collection")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let req = test::TestRequest::with_uri("/webdav/file-over-collection-target.txt/")
        .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::put()
        .uri("/webdav/file-over-collection-target.txt/old-child.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("old child")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let req = test::TestRequest::with_uri("/webdav/file-over-collection-source.txt")
        .method(actix_web::http::Method::from_bytes(b"COPY").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/file-over-collection-target.txt"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "COPY file over existing collection should overwrite it, got {}",
        resp.status()
    );

    let req = test::TestRequest::get()
        .uri("/webdav/file-over-collection-target.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body = test::read_body(resp).await;
    assert_eq!(String::from_utf8_lossy(&body), "file replaces collection");

    let req = test::TestRequest::get()
        .uri("/webdav/file-over-collection-target.txt/old-child.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        404,
        "overwritten collection descendants must be removed"
    );
}

#[actix_web::test]
async fn test_webdav_copy_file_over_collection_checks_locked_descendants() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/locked-overwrite-source.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("must not overwrite locked collection")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let req = test::TestRequest::with_uri("/webdav/locked-overwrite-target.txt/")
        .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::put()
        .uri("/webdav/locked-overwrite-target.txt/locked-child.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("locked child")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
  <D:owner><D:href>testuser</D:href></D:owner>
</D:lockinfo>"#;
    let req = test::TestRequest::with_uri("/webdav/locked-overwrite-target.txt/locked-child.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Depth", "0"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::with_uri("/webdav/locked-overwrite-source.txt")
        .method(actix_web::http::Method::from_bytes(b"COPY").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/locked-overwrite-target.txt"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        207,
        "COPY over a collection with locked descendants should return Multi-Status"
    );
    assert_eq!(
        resp.headers()
            .get("Cache-Control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store"),
        "COPY Multi-Status responses must not be cacheable"
    );
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);
    assert!(
        xml.contains("/webdav/locked-overwrite-target.txt/locked-child.txt")
            && xml.contains("423 Locked"),
        "{xml}"
    );

    let req = test::TestRequest::get()
        .uri("/webdav/locked-overwrite-target.txt/locked-child.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "failed overwrite must leave the locked collection subtree intact"
    );
}

#[actix_web::test]
async fn test_webdav_copy_rejects_invalid_overwrite_header() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/overwrite-invalid-source.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("source")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let req = test::TestRequest::with_uri("/webdav/overwrite-invalid-source.txt")
        .method(actix_web::http::Method::from_bytes(b"COPY").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/overwrite-invalid-target.txt"))
        .insert_header(("Overwrite", "maybe"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "invalid Overwrite values must be rejected instead of treated as T"
    );

    let req = test::TestRequest::get()
        .uri("/webdav/overwrite-invalid-target.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        404,
        "COPY with invalid Overwrite must not create the destination"
    );
}

#[actix_web::test]
async fn test_webdav_copy_rejects_absolute_destination_on_other_host() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/other-host-source.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("source")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let req = test::TestRequest::with_uri("/webdav/other-host-source.txt")
        .method(actix_web::http::Method::from_bytes(b"COPY").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Host", "local.example"))
        .insert_header((
            "Destination",
            "http://remote.example/webdav/other-host-target.txt",
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "absolute Destination URIs for another host must not be treated as local paths"
    );
    let body = test::read_body(resp).await;
    let body_text = String::from_utf8_lossy(&body);
    assert!(
        body_text.contains("Destination must stay on this WebDAV server"),
        "Destination host mismatch should keep the existing error text: {body_text}"
    );

    let req = test::TestRequest::get()
        .uri("/webdav/other-host-target.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
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
async fn test_webdav_custom_property_preserves_xml_subtree() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/props-xml.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("props")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let set_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propertyupdate xmlns:D="DAV:" xmlns:A="urn:aster:" xmlns:B="urn:aster:child">
  <D:set>
    <D:prop>
      <A:complex>
        <B:item key="one">alpha</B:item>
        <B:item key="two"><B:nested>beta</B:nested></B:item>
      </A:complex>
    </D:prop>
  </D:set>
</D:propertyupdate>"#;
    let req = test::TestRequest::with_uri("/webdav/props-xml.txt")
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
    <A:complex />
  </D:prop>
</D:propfind>"#;
    let req = test::TestRequest::with_uri("/webdav/props-xml.txt")
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
        xml.contains("urn:aster:child")
            && xml.contains("key=\"one\"")
            && xml.contains("alpha")
            && xml.contains("nested")
            && xml.contains("beta"),
        "complex dead property XML subtree should roundtrip: {xml}"
    );
    Element::parse(Cursor::new(xml.as_bytes())).expect("PROPFIND response XML should parse");
}

#[actix_web::test]
async fn test_webdav_dead_property_preserves_xml_lang_on_property_name() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/props-lang.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("props")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let set_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propertyupdate xmlns:D="DAV:" xmlns:A="urn:aster:">
  <D:set>
    <D:prop>
      <A:title xml:lang="zh-CN">标题</A:title>
    </D:prop>
  </D:set>
</D:propertyupdate>"#;
    let req = test::TestRequest::with_uri("/webdav/props-lang.txt")
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
    <A:title />
  </D:prop>
</D:propfind>"#;
    let req = test::TestRequest::with_uri("/webdav/props-lang.txt")
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
    assert!(xml.contains("xml:lang=\"zh-CN\""), "{xml}");
    assert!(xml.contains("标题"), "{xml}");
    Element::parse(Cursor::new(xml.as_bytes())).expect("PROPFIND response XML should parse");
}

#[actix_web::test]
async fn test_webdav_dead_property_inherits_xml_lang_from_prop_container() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/props-lang-inherited.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("props")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let set_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propertyupdate xmlns:D="DAV:" xmlns:A="urn:aster:">
  <D:set>
    <D:prop xml:lang="fr">
      <A:title>Bonjour</A:title>
    </D:prop>
  </D:set>
</D:propertyupdate>"#;
    let req = test::TestRequest::with_uri("/webdav/props-lang-inherited.txt")
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
    <A:title />
  </D:prop>
</D:propfind>"#;
    let req = test::TestRequest::with_uri("/webdav/props-lang-inherited.txt")
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
    assert!(xml.contains("xml:lang=\"fr\""), "{xml}");
    assert!(xml.contains("Bonjour"), "{xml}");
    Element::parse(Cursor::new(xml.as_bytes())).expect("PROPFIND response XML should parse");
}

#[actix_web::test]
async fn test_webdav_copy_file_preserves_dead_properties() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/props-copy-source.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("copy props")
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
    let req = test::TestRequest::with_uri("/webdav/props-copy-source.txt")
        .method(actix_web::http::Method::from_bytes(b"PROPPATCH").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(set_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);

    let req = test::TestRequest::with_uri("/webdav/props-copy-source.txt")
        .method(actix_web::http::Method::from_bytes(b"COPY").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/props-copy-target.txt"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "COPY file should return 201/204, got {}",
        resp.status()
    );

    let propfind_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:" xmlns:A="urn:aster:">
  <D:prop>
    <A:color />
  </D:prop>
</D:propfind>"#;
    let req = test::TestRequest::with_uri("/webdav/props-copy-target.txt")
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
        xml.contains(">blue<"),
        "COPY must preserve file dead properties: {xml}"
    );
}

#[actix_web::test]
async fn test_webdav_copy_folder_depth_zero_preserves_collection_dead_properties() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::with_uri("/webdav/props-shallow-source/")
        .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::with_uri("/webdav/props-shallow-source/child/")
        .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let set_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propertyupdate xmlns:D="DAV:" xmlns:A="urn:aster:">
  <D:set>
    <D:prop>
      <A:label>shallow-root</A:label>
    </D:prop>
  </D:set>
</D:propertyupdate>"#;
    let req = test::TestRequest::with_uri("/webdav/props-shallow-source/")
        .method(actix_web::http::Method::from_bytes(b"PROPPATCH").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(set_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);

    let req = test::TestRequest::with_uri("/webdav/props-shallow-source/")
        .method(actix_web::http::Method::from_bytes(b"COPY").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/props-shallow-target/"))
        .insert_header(("Depth", "0"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "COPY Depth: 0 collection should return 201/204, got {}",
        resp.status()
    );

    let propfind_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:" xmlns:A="urn:aster:">
  <D:prop>
    <A:label />
  </D:prop>
</D:propfind>"#;
    let req = test::TestRequest::with_uri("/webdav/props-shallow-target/")
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
        xml.contains(">shallow-root<"),
        "COPY Depth: 0 must preserve collection dead properties: {xml}"
    );

    let req = test::TestRequest::with_uri("/webdav/props-shallow-target/child/")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Depth", "0"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        404,
        "COPY Depth: 0 still must not copy collection members"
    );
}

#[actix_web::test]
async fn test_webdav_copy_folder_recursively_preserves_descendant_dead_properties() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    for uri in [
        "/webdav/props-tree-source/",
        "/webdav/props-tree-source/sub/",
    ] {
        let req = test::TestRequest::with_uri(uri)
            .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
            .insert_header(("Authorization", auth.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    let req = test::TestRequest::put()
        .uri("/webdav/props-tree-source/sub/file.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("tree props")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    for (uri, value) in [
        ("/webdav/props-tree-source/", "root-prop"),
        ("/webdav/props-tree-source/sub/", "sub-prop"),
        ("/webdav/props-tree-source/sub/file.txt", "file-prop"),
    ] {
        let set_body = format!(
            r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propertyupdate xmlns:D="DAV:" xmlns:A="urn:aster:">
  <D:set>
    <D:prop>
      <A:marker>{value}</A:marker>
    </D:prop>
  </D:set>
</D:propertyupdate>"#
        );
        let req = test::TestRequest::with_uri(uri)
            .method(actix_web::http::Method::from_bytes(b"PROPPATCH").unwrap())
            .insert_header(("Authorization", auth.clone()))
            .insert_header(("Content-Type", "application/xml"))
            .set_payload(set_body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 207);
    }

    let req = test::TestRequest::with_uri("/webdav/props-tree-source/")
        .method(actix_web::http::Method::from_bytes(b"COPY").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/props-tree-target/"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "recursive COPY collection should return 201/204, got {}",
        resp.status()
    );

    let propfind_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:" xmlns:A="urn:aster:">
  <D:prop>
    <A:marker />
  </D:prop>
</D:propfind>"#;
    for (uri, expected) in [
        ("/webdav/props-tree-target/", "root-prop"),
        ("/webdav/props-tree-target/sub/", "sub-prop"),
        ("/webdav/props-tree-target/sub/file.txt", "file-prop"),
    ] {
        let req = test::TestRequest::with_uri(uri)
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
            xml.contains(expected),
            "recursive COPY must preserve dead property '{expected}' for {uri}: {xml}"
        );
    }
}

#[actix_web::test]
async fn test_webdav_propfind_allprop_honors_include() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/allprop-include.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("allprop include")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
  <D:allprop />
  <D:include>
    <D:quota-used-bytes />
    <D:getcontentlength />
  </D:include>
</D:propfind>"#;
    let req = test::TestRequest::with_uri("/webdav/allprop-include.txt")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Depth", "0"))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);
    assert!(
        xml.contains("HTTP/1.1 200 OK") && xml.contains("HTTP/1.1 404 Not Found"),
        "allprop/include should return default allprop values plus missing included props: {xml}"
    );
    assert!(
        xml.contains("quota-used-bytes"),
        "included DAV properties outside allprop must be reported instead of ignored: {xml}"
    );
    assert_eq!(
        xml.matches("getcontentlength").count(),
        2,
        "include should not duplicate properties already returned by allprop: {xml}"
    );
    Element::parse(Cursor::new(xml.as_bytes())).expect("PROPFIND response XML should parse");
}

#[actix_web::test]
async fn test_webdav_propfind_collection_does_not_report_getcontentlength() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::with_uri("/webdav/collection-length/")
        .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let allprop_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
  <D:allprop />
</D:propfind>"#;
    let req = test::TestRequest::with_uri("/webdav/collection-length/")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Depth", "0"))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(allprop_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);
    assert!(
        !xml.contains("getcontentlength"),
        "allprop on a collection must not report getcontentlength: {xml}"
    );

    let propname_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
  <D:propname />
</D:propfind>"#;
    let req = test::TestRequest::with_uri("/webdav/collection-length/")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Depth", "0"))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(propname_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);
    assert!(
        !xml.contains("getcontentlength"),
        "propname on a collection must not list getcontentlength: {xml}"
    );

    let prop_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:getcontentlength />
  </D:prop>
</D:propfind>"#;
    let req = test::TestRequest::with_uri("/webdav/collection-length/")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth))
        .insert_header(("Depth", "0"))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(prop_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);
    assert!(
        xml.contains("getcontentlength") && xml.contains("HTTP/1.1 404 Not Found"),
        "named getcontentlength on a collection should be reported missing: {xml}"
    );
    Element::parse(Cursor::new(xml.as_bytes())).expect("PROPFIND response XML should parse");
}

#[actix_web::test]
async fn test_webdav_propfind_depth_one_large_directory_live_props() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::with_uri("/webdav/live-large/")
        .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    for index in 0..24 {
        let req = test::TestRequest::put()
            .uri(&format!("/webdav/live-large/file-{index}.txt"))
            .insert_header(("Authorization", auth.clone()))
            .set_payload(format!("live prop fixture {index}"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(
            resp.status() == 201 || resp.status() == 204,
            "PUT fixture file {index} should succeed, got {}",
            resp.status()
        );
    }

    let propfind_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:displayname />
    <D:resourcetype />
    <D:getcontentlength />
    <D:getlastmodified />
    <D:creationdate />
    <D:getetag />
  </D:prop>
</D:propfind>"#;
    let req = test::TestRequest::with_uri("/webdav/live-large/")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth))
        .insert_header(("Depth", "1"))
        .insert_header(("Content-Type", "application/xml"))
        .set_payload(propfind_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);

    assert_eq!(
        xml.matches("<D:response>").count(),
        25,
        "Depth: 1 should include the directory plus all children: {xml}"
    );
    for index in 0..24 {
        assert!(
            xml.contains(&format!("file-{index}.txt")),
            "Depth: 1 live prop response should include file-{index}.txt: {xml}"
        );
    }
    assert!(
        xml.contains("getlastmodified") && xml.contains("getetag"),
        "live prop response should include requested live metadata: {xml}"
    );
    Element::parse(Cursor::new(xml.as_bytes())).expect("PROPFIND response XML should parse");
}

#[actix_web::test]
async fn test_webdav_propfind_rejects_invalid_request_grammar() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let cases = [
        (
            "include without allprop",
            r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
  <D:include><D:getcontentlength /></D:include>
</D:propfind>"#,
        ),
        (
            "mixed propname and prop",
            r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
  <D:propname />
  <D:prop><D:displayname /></D:prop>
</D:propfind>"#,
        ),
        (
            "allprop with prop",
            r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
  <D:allprop />
  <D:prop><D:displayname /></D:prop>
</D:propfind>"#,
        ),
        (
            "unknown child",
            r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
  <D:unknown />
</D:propfind>"#,
        ),
    ];

    for (label, body) in cases {
        let req = test::TestRequest::with_uri("/webdav/")
            .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
            .insert_header(("Authorization", auth.clone()))
            .insert_header(("Depth", "0"))
            .insert_header(("Content-Type", "application/xml"))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            400,
            "invalid PROPFIND grammar should be rejected for case: {label}"
        );
    }
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
async fn test_webdav_proppatch_rejects_invalid_request_grammar() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/proppatch-invalid.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("invalid")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let cases = [
        (
            "empty propertyupdate",
            r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propertyupdate xmlns:D="DAV:" />"#,
        ),
        (
            "unknown instruction",
            r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propertyupdate xmlns:D="DAV:">
  <D:foo />
</D:propertyupdate>"#,
        ),
        (
            "set without prop",
            r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propertyupdate xmlns:D="DAV:">
  <D:set />
</D:propertyupdate>"#,
        ),
        (
            "set with unknown child",
            r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propertyupdate xmlns:D="DAV:" xmlns:A="urn:aster:">
  <D:set>
    <A:color>blue</A:color>
  </D:set>
</D:propertyupdate>"#,
        ),
        (
            "set with multiple prop containers",
            r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propertyupdate xmlns:D="DAV:" xmlns:A="urn:aster:">
  <D:set>
    <D:prop><A:color>blue</A:color></D:prop>
    <D:prop><A:size>large</A:size></D:prop>
  </D:set>
</D:propertyupdate>"#,
        ),
    ];

    for (label, body) in cases {
        let req = test::TestRequest::with_uri("/webdav/proppatch-invalid.txt")
            .method(actix_web::http::Method::from_bytes(b"PROPPATCH").unwrap())
            .insert_header(("Authorization", auth.clone()))
            .insert_header(("Content-Type", "application/xml"))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            400,
            "invalid PROPPATCH grammar should be rejected for case: {label}"
        );
    }
}

#[actix_web::test]
async fn test_webdav_proppatch_is_atomic_when_one_property_fails() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/proppatch-atomic.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("atomic")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propertyupdate xmlns:D="DAV:" xmlns:A="urn:aster:">
  <D:set>
    <D:prop>
      <A:color>green</A:color>
    </D:prop>
  </D:set>
  <D:set>
    <D:prop>
      <D:displayname>blocked</D:displayname>
    </D:prop>
  </D:set>
</D:propertyupdate>"#;
    let req = test::TestRequest::with_uri("/webdav/proppatch-atomic.txt")
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
        xml.contains("403") && xml.contains("424"),
        "mixed PROPPATCH failure should mark the protected property and dependent properties: {xml}"
    );

    let propfind_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:" xmlns:A="urn:aster:">
  <D:prop>
    <A:color />
  </D:prop>
</D:propfind>"#;
    let req = test::TestRequest::with_uri("/webdav/proppatch-atomic.txt")
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
        xml.contains("404") && !xml.contains(">green<"),
        "failed PROPPATCH must not persist earlier successful-looking properties: {xml}"
    );
}

#[actix_web::test]
async fn test_webdav_hides_and_rejects_system_property_namespace() {
    use aster_drive::services::auth_service;

    let state = common::setup().await;
    let db1 = state.writer_db().clone();
    let db2 = state.writer_db().clone();
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

    let file = file_repo::find_by_name_in_folder(
        state.writer_db(),
        claims.user_id,
        None,
        "system-props.zip",
    )
    .await
    .expect("file lookup should succeed")
    .expect("uploaded file should exist");
    property_repo::upsert(
        state.writer_db(),
        EntityType::File,
        file.id,
        "system.archive_preview",
        "zip_manifest.v2",
        Some("cached"),
    )
    .await
    .expect("internal system property should be writable through repo");

    let propfind_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:" xmlns:S="system.archive_preview">
  <D:prop>
    <S:zip_manifest.v2 />
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
        !xml.contains("cached") && xml.contains("zip_manifest.v2") && xml.contains("404"),
        "requested system properties must be reported as missing without exposing values: {xml}"
    );

    let proppatch_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propertyupdate xmlns:D="DAV:" xmlns:S="system.archive_preview">
  <D:set>
    <D:prop>
      <S:zip_manifest.v2>tampered</S:zip_manifest.v2>
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
        state.writer_db(),
        EntityType::File,
        file.id,
        "system.archive_preview",
        "zip_manifest.v2",
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

    let basic_scope_username = webdav_test_username("basic-scope");
    let basic_scope_password = webdav_test_password("BASIC_SCOPE");
    let req = test::TestRequest::post()
        .uri("/api/v1/webdav-accounts")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "username": &basic_scope_username,
            "password": &basic_scope_password,
            "root_folder_id": root_id,
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let basic = basic_auth_header(&basic_scope_username, &basic_scope_password);

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
    assert!(
        dav_header.contains("2"),
        "DAV header should advertise class 2 locking support, got: '{dav_header}'"
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
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);
    assert!(
        xml.contains("lockroot") && xml.contains("/webdav/lockme.txt"),
        "LOCK response should include RFC 4918 lockroot for the locked resource: {xml}"
    );
    Element::parse(Cursor::new(xml.as_bytes())).expect("LOCK response XML should parse");

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

    let req = test::TestRequest::with_uri("/webdav/lockme.txt")
        .method(actix_web::http::Method::from_bytes(b"UNLOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header((
            "Lock-Token",
            lock_token.trim_matches(|c| c == '<' || c == '>'),
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "UNLOCK must reject malformed Lock-Token headers before token lookup"
    );

    let req = test::TestRequest::with_uri("/webdav/not-lockme.txt")
        .method(actix_web::http::Method::from_bytes(b"UNLOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Lock-Token", lock_token.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        409,
        "UNLOCK request URI must identify the locked resource or its deep lock scope"
    );

    let req = test::TestRequest::delete()
        .uri("/webdav/lockme.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 423 || resp.status() == 403,
        "failed UNLOCK on another URI must leave the original lock active, got {}",
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
async fn test_webdav_lock_resource_abuse_limit_rejects_new_placeholder_without_creating_file() {
    let app = setup_webdav_with_runtime_config(
        aster_drive::config::definitions::WEBDAV_MAX_ACTIVE_LOCKS_PER_USER_KEY,
        "1",
    )
    .await;
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);
    let lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
  <D:owner><D:href>testuser</D:href></D:owner>
</D:lockinfo>"#;

    let req = test::TestRequest::with_uri("/webdav/locked-empty-1.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Timeout", "Second-3600"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        201,
        "first LOCK on an unmapped file should still create an RFC 4918 locked empty resource"
    );

    let req = test::TestRequest::with_uri("/webdav/locked-empty-2.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Timeout", "Second-3600"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        507,
        "second active LOCK should hit the per-user resource-abuse guard"
    );

    let req = test::TestRequest::get()
        .uri("/webdav/locked-empty-2.txt")
        .insert_header(("Authorization", auth))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        404,
        "failed LOCK must not create the placeholder resource"
    );
}

#[actix_web::test]
async fn test_webdav_copy_locked_source_does_not_require_source_lock_token() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/copy-locked-source.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("copy locked source")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
</D:lockinfo>"#;
    let req = test::TestRequest::with_uri("/webdav/copy-locked-source.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Depth", "0"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::with_uri("/webdav/copy-locked-source.txt")
        .method(actix_web::http::Method::from_bytes(b"COPY").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/copy-locked-source-target.txt"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "COPY must not require source lock token because source is not modified, got {}",
        resp.status()
    );

    let req = test::TestRequest::get()
        .uri("/webdav/copy-locked-source-target.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body = test::read_body(resp).await;
    assert_eq!(String::from_utf8_lossy(&body), "copy locked source");
}

#[actix_web::test]
async fn test_webdav_copy_locked_destination_without_token_is_locked() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    for (uri, body) in [
        ("/webdav/copy-locked-dest-source.txt", "source replacement"),
        ("/webdav/copy-locked-dest-target.txt", "locked destination"),
    ] {
        let req = test::TestRequest::put()
            .uri(uri)
            .insert_header(("Authorization", auth.clone()))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status() == 201 || resp.status() == 204);
    }

    let lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
</D:lockinfo>"#;
    let req = test::TestRequest::with_uri("/webdav/copy-locked-dest-target.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Depth", "0"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let req = test::TestRequest::with_uri("/webdav/copy-locked-dest-source.txt")
        .method(actix_web::http::Method::from_bytes(b"COPY").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/copy-locked-dest-target.txt"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::LOCKED,
        "COPY over a locked destination must require the destination lock token"
    );
}

#[actix_web::test]
async fn test_webdav_copy_locked_destination_accepts_tagged_destination_lock_token() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    for (uri, body) in [
        (
            "/webdav/copy-locked-dest-token-source.txt",
            "source replacement",
        ),
        (
            "/webdav/copy-locked-dest-token-target.txt",
            "locked destination",
        ),
    ] {
        let req = test::TestRequest::put()
            .uri(uri)
            .insert_header(("Authorization", auth.clone()))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status() == 201 || resp.status() == 204);
    }

    let lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
</D:lockinfo>"#;
    let req = test::TestRequest::with_uri("/webdav/copy-locked-dest-token-target.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Depth", "0"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let lock_token = resp
        .headers()
        .get("Lock-Token")
        .and_then(|value| value.to_str().ok())
        .expect("LOCK response should include Lock-Token")
        .to_string();

    let req = test::TestRequest::with_uri("/webdav/copy-locked-dest-token-source.txt")
        .method(actix_web::http::Method::from_bytes(b"COPY").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/copy-locked-dest-token-target.txt"))
        .insert_header((
            "If",
            format!(r#"</webdav/copy-locked-dest-token-target.txt> ({lock_token})"#),
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "COPY should proceed when the tagged destination token is submitted, got {}",
        resp.status()
    );

    let req = test::TestRequest::get()
        .uri("/webdav/copy-locked-dest-token-target.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body = test::read_body(resp).await;
    assert_eq!(String::from_utf8_lossy(&body), "source replacement");
}

#[actix_web::test]
async fn test_webdav_lock_missing_file_creates_locked_empty_resource() {
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
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Timeout", "Second-3600"))
        .insert_header(("Depth", "0"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        201,
        "LOCK on a missing file URL should create a locked empty resource"
    );
    let lock_token = resp
        .headers()
        .get("Lock-Token")
        .and_then(|value| value.to_str().ok())
        .expect("LOCK response should include Lock-Token")
        .to_string();

    let req = test::TestRequest::get()
        .uri("/webdav/missing-lock-target.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body = test::read_body(resp).await;
    assert!(
        body.is_empty(),
        "locked empty resource should be zero bytes"
    );

    let req = test::TestRequest::put()
        .uri("/webdav/missing-lock-target.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("blocked")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        423,
        "PUT without the lock token should not overwrite locked empty resource"
    );

    let submitted_lock_token = lock_token.trim_matches(|c| c == '<' || c == '>');
    let req = test::TestRequest::put()
        .uri("/webdav/missing-lock-target.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("If", format!("(<{submitted_lock_token}>)")))
        .set_payload("written after lock")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "PUT with own lock token should overwrite locked empty resource, got {}",
        resp.status()
    );

    let req = test::TestRequest::get()
        .uri("/webdav/missing-lock-target.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body = test::read_body(resp).await;
    assert_eq!(String::from_utf8_lossy(&body), "written after lock");
}

#[actix_web::test]
async fn test_webdav_depth_zero_collection_lock_protects_member_urls() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::with_uri("/webdav/parent-lock/")
        .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    for (path, body) in [
        ("/webdav/parent-lock/existing.txt", "existing"),
        ("/webdav/copy-into-parent-lock.txt", "copy"),
    ] {
        let req = test::TestRequest::put()
            .uri(path)
            .insert_header(("Authorization", auth.clone()))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status() == 201 || resp.status() == 204);
    }

    let lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
</D:lockinfo>"#;
    let req = test::TestRequest::with_uri("/webdav/parent-lock/")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Depth", "0"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let lock_token = resp
        .headers()
        .get("Lock-Token")
        .and_then(|value| value.to_str().ok())
        .expect("LOCK response should include Lock-Token")
        .trim_matches(|c| c == '<' || c == '>')
        .to_string();

    let req = test::TestRequest::put()
        .uri("/webdav/parent-lock/new.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("new")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        423,
        "PUT creating a member URL must submit the depth-0 parent collection lock token"
    );

    let req = test::TestRequest::delete()
        .uri("/webdav/parent-lock/existing.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        423,
        "DELETE of a member URL must submit the depth-0 parent collection lock token"
    );

    let req = test::TestRequest::with_uri("/webdav/parent-lock/existing.txt")
        .method(actix_web::http::Method::from_bytes(b"MOVE").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/moved-out-of-parent-lock.txt"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        423,
        "MOVE out of a depth-0 locked collection must submit the parent lock token"
    );

    let req = test::TestRequest::with_uri("/webdav/copy-into-parent-lock.txt")
        .method(actix_web::http::Method::from_bytes(b"COPY").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/parent-lock/copied.txt"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        423,
        "COPY into a depth-0 locked collection must submit the destination parent lock token"
    );

    let req = test::TestRequest::put()
        .uri("/webdav/parent-lock/new.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("If", format!("(<{lock_token}>)")))
        .set_payload("new")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "submitting the parent collection lock token should allow member creation, got {}",
        resp.status()
    );
}

#[actix_web::test]
async fn test_webdav_tagged_if_token_only_unlocks_matching_resource() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/if-scope.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("initial")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
  <D:owner><D:href>testuser</D:href></D:owner>
</D:lockinfo>"#;
    let req = test::TestRequest::with_uri("/webdav/if-scope.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Depth", "0"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let lock_token = resp
        .headers()
        .get("Lock-Token")
        .and_then(|value| value.to_str().ok())
        .expect("LOCK response should include Lock-Token")
        .trim_matches(|c| c == '<' || c == '>')
        .to_string();

    let req = test::TestRequest::put()
        .uri("/webdav/if-scope.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("If", format!("</webdav/other.txt> (<{lock_token}>)")))
        .set_payload("wrong resource")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        412,
        "tagged If token for a different resource must fail the If precondition"
    );

    let req = test::TestRequest::put()
        .uri("/webdav/if-scope.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("If", format!("</webdav/if-scope.txt> (<{lock_token}>)")))
        .set_payload("matching resource")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "tagged If token for the request resource should unlock it, got {}",
        resp.status()
    );
}

#[actix_web::test]
async fn test_webdav_lock_rejects_invalid_lockinfo_and_timeout_headers() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    for (label, body) in [
        (
            "non-DAV lockinfo",
            r#"<?xml version="1.0" encoding="utf-8" ?>
<X:lockinfo xmlns:X="urn:not-dav">
  <X:lockscope><X:exclusive/></X:lockscope>
  <X:locktype><X:write/></X:locktype>
</X:lockinfo>"#,
        ),
        (
            "ambiguous lockscope",
            r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/><D:shared/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
</D:lockinfo>"#,
        ),
        (
            "ambiguous locktype",
            r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/><D:other/></D:locktype>
</D:lockinfo>"#,
        ),
    ] {
        let req = test::TestRequest::with_uri("/webdav/invalid-lockinfo.txt")
            .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
            .insert_header(("Authorization", auth.clone()))
            .insert_header(("Content-Type", "application/xml"))
            .insert_header(("Depth", "0"))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            400,
            "LOCK should reject invalid lockinfo body for case: {label}"
        );
    }

    let valid_lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
</D:lockinfo>"#;
    let req = test::TestRequest::with_uri("/webdav/invalid-timeout.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Depth", "0"))
        .insert_header(("Timeout", "Second-x"))
        .set_payload(valid_lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400, "invalid Timeout must be rejected");

    let req = test::TestRequest::with_uri("/webdav/overflow-timeout.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Depth", "0"))
        .insert_header(("Timeout", "Second-9223372036854775808"))
        .set_payload(valid_lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "oversized Timeout values must be rejected instead of becoming infinite locks"
    );

    let req = test::TestRequest::with_uri("/webdav/infinite-timeout.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Depth", "0"))
        .insert_header(("Timeout", "Infinite"))
        .set_payload(valid_lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        201,
        "Infinite Timeout should be clamped to the server maximum"
    );

    let req = test::TestRequest::with_uri("/webdav/later-timeout.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Depth", "0"))
        .insert_header(("Timeout", "nonsense, Second-60"))
        .set_payload(valid_lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        201,
        "Timeout parser should skip unsupported candidates and accept a later valid one"
    );
}

#[actix_web::test]
async fn test_webdav_lock_token_header_does_not_submit_write_lock_token() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/lock-token-header.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("initial")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
</D:lockinfo>"#;
    let req = test::TestRequest::with_uri("/webdav/lock-token-header.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Depth", "0"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let lock_token = resp
        .headers()
        .get("Lock-Token")
        .and_then(|value| value.to_str().ok())
        .expect("LOCK response should include Lock-Token")
        .to_string();

    let req = test::TestRequest::put()
        .uri("/webdav/lock-token-header.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Lock-Token", lock_token.clone()))
        .set_payload("must stay locked")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        423,
        "Lock-Token request header must not replace If token submission for PUT"
    );

    let submitted_lock_token = lock_token.trim_matches(|c| c == '<' || c == '>');
    let req = test::TestRequest::put()
        .uri("/webdav/lock-token-header.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("If", format!("(<{submitted_lock_token}>)")))
        .set_payload("if unlocks")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "If token submission should still satisfy the lock, got {}",
        resp.status()
    );
}

#[actix_web::test]
async fn test_webdav_absolute_tagged_if_token_uses_request_origin_for_lock_submission() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/if-lock-origin.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Host", "local.example"))
        .set_payload("initial")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
</D:lockinfo>"#;
    let req = test::TestRequest::with_uri("/webdav/if-lock-origin.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Host", "local.example"))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Depth", "0"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let lock_token = resp
        .headers()
        .get("Lock-Token")
        .and_then(|value| value.to_str().ok())
        .expect("LOCK response should include Lock-Token")
        .trim_matches(|c| c == '<' || c == '>')
        .to_string();

    let req = test::TestRequest::put()
        .uri("/webdav/if-lock-origin.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Host", "local.example"))
        .insert_header((
            "If",
            format!("<http://remote.example/webdav/if-lock-origin.txt> (<{lock_token}>)"),
        ))
        .set_payload("wrong origin")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        412,
        "absolute tagged If token for another origin must fail precondition evaluation"
    );

    let req = test::TestRequest::put()
        .uri("/webdav/if-lock-origin.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Host", "local.example"))
        .insert_header((
            "If",
            format!("<http://local.example/webdav/if-lock-origin.txt> (<{lock_token}>)"),
        ))
        .set_payload("same origin")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "absolute tagged If token for the request origin should satisfy the lock, got {}",
        resp.status()
    );
}

#[actix_web::test]
async fn test_webdav_lock_refresh_requires_if_header_token_submission() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/lock-refresh-if.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("initial")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
</D:lockinfo>"#;
    let req = test::TestRequest::with_uri("/webdav/lock-refresh-if.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Depth", "0"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let lock_token = resp
        .headers()
        .get("Lock-Token")
        .and_then(|value| value.to_str().ok())
        .expect("LOCK response should include Lock-Token")
        .to_string();

    let req = test::TestRequest::with_uri("/webdav/lock-refresh-if.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Lock-Token", lock_token.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "empty-body LOCK refresh must not accept Lock-Token without If"
    );

    let submitted_lock_token = lock_token.trim_matches(|c| c == '<' || c == '>');
    let req = test::TestRequest::with_uri("/webdav/other-lock-refresh-if.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("If", format!("(<{submitted_lock_token}>)")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        412,
        "empty-body LOCK refresh must target the locked resource or its deep lock scope"
    );

    let req = test::TestRequest::with_uri("/webdav/lock-refresh-if.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("If", format!("(<{submitted_lock_token}>")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "malformed If headers must be rejected instead of scanned for lock tokens"
    );

    let req = test::TestRequest::with_uri("/webdav/lock-refresh-if.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("If", format!("(<{submitted_lock_token}>)")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "empty-body LOCK refresh should accept token submitted through If"
    );
    assert!(
        resp.headers().get("Lock-Token").is_none(),
        "LOCK refresh response must not include a Lock-Token response header"
    );
}

#[actix_web::test]
async fn test_webdav_lock_refresh_counts_negated_tokens_as_submission() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/lock-refresh-not-token.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("initial")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
</D:lockinfo>"#;
    let req = test::TestRequest::with_uri("/webdav/lock-refresh-not-token.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Depth", "0"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let lock_token = resp
        .headers()
        .get("Lock-Token")
        .and_then(|value| value.to_str().ok())
        .expect("LOCK response should include Lock-Token")
        .trim_matches(|c| c == '<' || c == '>')
        .to_string();

    let req = test::TestRequest::with_uri("/webdav/lock-refresh-not-token.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header((
            "If",
            format!("(<{lock_token}>) (Not <urn:uuid:not-submitted>)"),
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "LOCK refresh requires exactly one submitted lock token; RFC 4918 still counts a negated state-token as submitted"
    );
}

#[actix_web::test]
async fn test_webdav_if_not_matching_current_lock_token_fails_precondition() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/if-not-lock.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("initial")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
</D:lockinfo>"#;
    let req = test::TestRequest::with_uri("/webdav/if-not-lock.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Depth", "0"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let lock_token = resp
        .headers()
        .get("Lock-Token")
        .and_then(|value| value.to_str().ok())
        .expect("LOCK response should include Lock-Token")
        .trim_matches(|c| c == '<' || c == '>')
        .to_string();

    let req = test::TestRequest::put()
        .uri("/webdav/if-not-lock.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("If", format!("(Not <{lock_token}>)")))
        .set_payload("should fail")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        412,
        "If: Not <current-lock-token> must fail as a precondition"
    );
}

#[actix_web::test]
async fn test_webdav_if_header_evaluates_etag_conditions() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/if-etag.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("initial etag")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::HEAD)
        .uri("/webdav/if-etag.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let etag = resp
        .headers()
        .get("ETag")
        .and_then(|value| value.to_str().ok())
        .expect("HEAD should return ETag")
        .to_string();

    let req = test::TestRequest::put()
        .uri("/webdav/if-etag.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("If", r#"(["definitely-wrong"])"#))
        .set_payload("wrong etag")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        412,
        "PUT with non-matching If ETag must fail"
    );

    let req = test::TestRequest::put()
        .uri("/webdav/if-etag.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("If", format!("([{etag}])")))
        .set_payload("matching etag")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "PUT with matching If ETag should proceed, got {}",
        resp.status()
    );

    let req = test::TestRequest::get()
        .uri("/webdav/if-etag.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body = test::read_body(resp).await;
    assert_eq!(String::from_utf8_lossy(&body), "matching etag");
}

#[actix_web::test]
async fn test_webdav_get_head_if_none_match_matching_etag_returns_not_modified() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/http-if-none-match.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("etag body")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::HEAD)
        .uri("/webdav/http-if-none-match.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let etag = resp
        .headers()
        .get("ETag")
        .and_then(|value| value.to_str().ok())
        .expect("HEAD should return ETag")
        .to_string();

    for method in [actix_web::http::Method::GET, actix_web::http::Method::HEAD] {
        let req = test::TestRequest::with_uri("/webdav/http-if-none-match.txt")
            .method(method.clone())
            .insert_header(("Authorization", auth.clone()))
            .insert_header(("If-None-Match", etag.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::NOT_MODIFIED,
            "{method} with matching If-None-Match should return 304"
        );
        assert_eq!(
            resp.headers()
                .get("ETag")
                .and_then(|value| value.to_str().ok()),
            Some(etag.as_str())
        );
    }
}

#[actix_web::test]
async fn test_webdav_get_head_return_last_modified() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/http-last-modified.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("last modified body")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let mut observed = Vec::new();
    for method in [actix_web::http::Method::GET, actix_web::http::Method::HEAD] {
        let req = test::TestRequest::with_uri("/webdav/http-last-modified.txt")
            .method(method.clone())
            .insert_header(("Authorization", auth.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::OK,
            "{method} should succeed"
        );
        let last_modified = resp
            .headers()
            .get("Last-Modified")
            .and_then(|value| value.to_str().ok())
            .expect("GET/HEAD should return Last-Modified")
            .to_string();
        assert!(
            last_modified
                .parse::<actix_web::http::header::HttpDate>()
                .is_ok(),
            "Last-Modified should be a valid HTTP date: {last_modified}"
        );
        observed.push(last_modified);
    }

    assert_eq!(
        observed[0], observed[1],
        "GET and HEAD should expose the same Last-Modified for the same resource"
    );
}

#[actix_web::test]
async fn test_webdav_get_head_http_date_preconditions() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/http-date-preconditions.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("date validator body")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::HEAD)
        .uri("/webdav/http-date-preconditions.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
    let last_modified = resp
        .headers()
        .get("Last-Modified")
        .and_then(|value| value.to_str().ok())
        .expect("HEAD should return Last-Modified")
        .to_string();
    let etag = resp
        .headers()
        .get("ETag")
        .and_then(|value| value.to_str().ok())
        .expect("HEAD should return ETag")
        .to_string();
    let old_http_date = "Sun, 06 Nov 1994 08:49:37 GMT";

    for method in [actix_web::http::Method::GET, actix_web::http::Method::HEAD] {
        let req = test::TestRequest::with_uri("/webdav/http-date-preconditions.txt")
            .method(method.clone())
            .insert_header(("Authorization", auth.clone()))
            .insert_header(("If-Modified-Since", last_modified.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::NOT_MODIFIED,
            "{method} with matching If-Modified-Since should return 304"
        );
        assert_eq!(
            resp.headers()
                .get("Last-Modified")
                .and_then(|value| value.to_str().ok()),
            Some(last_modified.as_str())
        );
    }

    let req = test::TestRequest::get()
        .uri("/webdav/http-date-preconditions.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("If-Modified-Since", old_http_date))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::OK,
        "stale If-Modified-Since should not suppress the response body"
    );

    let req = test::TestRequest::get()
        .uri("/webdav/http-date-preconditions.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("If-Unmodified-Since", old_http_date))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::PRECONDITION_FAILED,
        "If-Unmodified-Since should fail when the resource changed after the supplied timestamp"
    );

    let req = test::TestRequest::get()
        .uri("/webdav/http-date-preconditions.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("If-None-Match", "\"definitely-wrong\""))
        .insert_header(("If-Modified-Since", last_modified.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::OK,
        "If-None-Match should take precedence over If-Modified-Since when it is present"
    );

    let req = test::TestRequest::get()
        .uri("/webdav/http-date-preconditions.txt")
        .insert_header(("Authorization", auth))
        .insert_header(("If-None-Match", etag))
        .insert_header(("If-Modified-Since", old_http_date))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::NOT_MODIFIED,
        "matching If-None-Match should still return 304 regardless of If-Modified-Since"
    );
}

#[actix_web::test]
async fn test_webdav_put_http_if_match_preconditions() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/http-if-match.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("initial")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::HEAD)
        .uri("/webdav/http-if-match.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let etag = resp
        .headers()
        .get("ETag")
        .and_then(|value| value.to_str().ok())
        .expect("HEAD should return ETag")
        .to_string();

    let req = test::TestRequest::put()
        .uri("/webdav/http-if-match.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("If-Match", "\"definitely-wrong\""))
        .set_payload("must not write")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::PRECONDITION_FAILED,
        "PUT with non-matching HTTP If-Match must fail"
    );

    let req = test::TestRequest::get()
        .uri("/webdav/http-if-match.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body = test::read_body(resp).await;
    assert_eq!(String::from_utf8_lossy(&body), "initial");

    let req = test::TestRequest::put()
        .uri("/webdav/http-if-match.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("If-Match", etag))
        .set_payload("updated")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "PUT with matching HTTP If-Match should proceed, got {}",
        resp.status()
    );

    let req = test::TestRequest::put()
        .uri("/webdav/http-if-match-missing.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("If-Match", "*"))
        .set_payload("must not create")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::PRECONDITION_FAILED,
        "PUT with If-Match: * must not create a missing resource"
    );

    let req = test::TestRequest::get()
        .uri("/webdav/http-if-match-missing.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_webdav_mutations_http_etag_preconditions() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    for (path, body) in [
        ("/webdav/http-if-delete.txt", "delete source"),
        ("/webdav/http-if-copy.txt", "copy source"),
        ("/webdav/http-if-move.txt", "move source"),
    ] {
        let req = test::TestRequest::put()
            .uri(path)
            .insert_header(("Authorization", auth.clone()))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status() == 201 || resp.status() == 204);
    }

    let req = test::TestRequest::delete()
        .uri("/webdav/http-if-delete.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("If-Match", "\"wrong-delete-etag\""))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::PRECONDITION_FAILED
    );

    let req = test::TestRequest::with_uri("/webdav/http-if-copy.txt")
        .method(actix_web::http::Method::from_bytes(b"COPY").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/http-if-copy-target.txt"))
        .insert_header(("If-Match", "\"wrong-copy-etag\""))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::PRECONDITION_FAILED
    );

    let req = test::TestRequest::with_uri("/webdav/http-if-move.txt")
        .method(actix_web::http::Method::from_bytes(b"MOVE").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/http-if-move-target.txt"))
        .insert_header(("If-Match", "\"wrong-move-etag\""))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::PRECONDITION_FAILED
    );

    for (path, expected) in [
        ("/webdav/http-if-delete.txt", 200),
        ("/webdav/http-if-copy-target.txt", 404),
        ("/webdav/http-if-move-target.txt", 404),
    ] {
        let req = test::TestRequest::get()
            .uri(path)
            .insert_header(("Authorization", auth.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            expected,
            "{path} precondition result mismatch"
        );
    }

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::HEAD)
        .uri("/webdav/http-if-delete.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let delete_etag = resp
        .headers()
        .get("ETag")
        .and_then(|value| value.to_str().ok())
        .expect("HEAD should return ETag")
        .to_string();

    let req = test::TestRequest::delete()
        .uri("/webdav/http-if-delete.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("If-None-Match", delete_etag))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::PRECONDITION_FAILED,
        "DELETE with matching If-None-Match must fail with 412"
    );

    let req = test::TestRequest::with_uri("/webdav/http-if-copy.txt")
        .method(actix_web::http::Method::from_bytes(b"COPY").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/http-if-copy-target.txt"))
        .insert_header(("If-Match", "*"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let req = test::TestRequest::with_uri("/webdav/http-if-move.txt")
        .method(actix_web::http::Method::from_bytes(b"MOVE").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/http-if-move-target.txt"))
        .insert_header(("If-None-Match", "\"definitely-not-current\""))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);
}

#[actix_web::test]
async fn test_webdav_read_methods_apply_if_header_preconditions() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/if-read.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("read precondition")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let req = test::TestRequest::get()
        .uri("/webdav/if-read.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("If", r#"(["wrong-etag"])"#))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 412, "GET must enforce failed If headers");

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::HEAD)
        .uri("/webdav/if-read.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("If", r#"(["wrong-etag"])"#))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 412, "HEAD must enforce failed If headers");

    let req = test::TestRequest::with_uri("/webdav/if-read.txt")
        .method(actix_web::http::Method::from_bytes(b"PROPFIND").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Depth", "0"))
        .insert_header(("If", r#"(["wrong-etag"])"#))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        412,
        "PROPFIND must enforce failed If headers"
    );
}

#[actix_web::test]
async fn test_webdav_absolute_tagged_if_uri_uses_request_origin() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/if-origin.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Host", "local.example"))
        .set_payload("initial")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let req = test::TestRequest::default()
        .method(actix_web::http::Method::HEAD)
        .uri("/webdav/if-origin.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Host", "local.example"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let etag = resp
        .headers()
        .get("ETag")
        .and_then(|value| value.to_str().ok())
        .expect("HEAD should return ETag")
        .to_string();

    let req = test::TestRequest::put()
        .uri("/webdav/if-origin.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Host", "local.example"))
        .insert_header((
            "If",
            format!(r#"<http://local.example/webdav/if-origin.txt> ([{etag}])"#),
        ))
        .set_payload("same origin")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "absolute tagged If URI for the request origin should match, got {}",
        resp.status()
    );

    let req = test::TestRequest::put()
        .uri("/webdav/if-origin.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Host", "local.example"))
        .insert_header((
            "If",
            format!(r#"<http://remote.example/webdav/if-origin.txt> ([{etag}])"#),
        ))
        .set_payload("other origin")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        412,
        "absolute tagged If URI for another origin must not match this resource"
    );
}

#[actix_web::test]
async fn test_webdav_if_header_allows_any_matching_state_list() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/if-or.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("initial")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let req = test::TestRequest::put()
        .uri("/webdav/if-or.txt")
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("If", r#"(["wrong-etag"]) (Not <DAV:no-lock>)"#))
        .set_payload("or branch")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == 201 || resp.status() == 204,
        "second If state-list should allow the request, got {}",
        resp.status()
    );

    let req = test::TestRequest::get()
        .uri("/webdav/if-or.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body = test::read_body(resp).await;
    assert_eq!(String::from_utf8_lossy(&body), "or branch");
}

#[actix_web::test]
async fn test_webdav_recursive_delete_reports_locked_children_as_multistatus() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::with_uri("/webdav/partial-delete/")
        .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::put()
        .uri("/webdav/partial-delete/locked.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("locked child")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
  <D:owner><D:href>testuser</D:href></D:owner>
</D:lockinfo>"#;
    let req = test::TestRequest::with_uri("/webdav/partial-delete/locked.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Depth", "0"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::delete()
        .uri("/webdav/partial-delete/")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        207,
        "recursive DELETE with locked descendants should return Multi-Status"
    );
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);
    assert!(xml.contains("/webdav/partial-delete/locked.txt"), "{xml}");
    assert!(xml.contains("423 Locked"), "{xml}");

    let req = test::TestRequest::get()
        .uri("/webdav/partial-delete/locked.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "locked descendant should remain after Multi-Status preflight failure"
    );
}

#[actix_web::test]
async fn test_webdav_recursive_move_reports_locked_children_as_multistatus() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    for uri in ["/webdav/partial-move/", "/webdav/move-target-parent/"] {
        let req = test::TestRequest::with_uri(uri)
            .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
            .insert_header(("Authorization", auth.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    let req = test::TestRequest::put()
        .uri("/webdav/partial-move/locked.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("locked child")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
  <D:owner><D:href>testuser</D:href></D:owner>
</D:lockinfo>"#;
    let req = test::TestRequest::with_uri("/webdav/partial-move/locked.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Depth", "0"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::with_uri("/webdav/partial-move/")
        .method(actix_web::http::Method::from_bytes(b"MOVE").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/move-target-parent/partial-move/"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        207,
        "recursive MOVE with locked descendants should return Multi-Status"
    );
    assert_eq!(
        resp.headers()
            .get("Cache-Control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store"),
        "MOVE Multi-Status responses must not be cacheable"
    );
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);
    assert!(xml.contains("/webdav/partial-move/locked.txt"), "{xml}");
    assert!(xml.contains("423 Locked"), "{xml}");

    let req = test::TestRequest::get()
        .uri("/webdav/partial-move/locked.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "locked descendant should remain at the source after Multi-Status preflight failure"
    );

    let req = test::TestRequest::get()
        .uri("/webdav/move-target-parent/partial-move/locked.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        404,
        "failed recursive MOVE should not create the destination subtree"
    );
}

#[actix_web::test]
async fn test_webdav_recursive_copy_continues_unlocked_siblings_after_locked_destination_child() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    for uri in [
        "/webdav/partial-copy-source/",
        "/webdav/partial-copy-source/open/",
        "/webdav/partial-copy-dest/",
        "/webdav/partial-copy-dest/locked/",
    ] {
        let req = test::TestRequest::with_uri(uri)
            .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
            .insert_header(("Authorization", auth.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    for (uri, body) in [
        (
            "/webdav/partial-copy-source/open/file.txt",
            "unlocked sibling",
        ),
        (
            "/webdav/partial-copy-source/locked/file.txt",
            "locked replacement",
        ),
        (
            "/webdav/partial-copy-dest/locked/file.txt",
            "locked existing",
        ),
    ] {
        if uri.contains("/partial-copy-source/locked/") {
            let req = test::TestRequest::with_uri("/webdav/partial-copy-source/locked/")
                .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
                .insert_header(("Authorization", auth.clone()))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 201);
        }
        let req = test::TestRequest::put()
            .uri(uri)
            .insert_header(("Authorization", auth.clone()))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status() == 201 || resp.status() == 204);
    }

    let lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
</D:lockinfo>"#;
    let req = test::TestRequest::with_uri("/webdav/partial-copy-dest/locked/file.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Depth", "0"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::with_uri("/webdav/partial-copy-source/")
        .method(actix_web::http::Method::from_bytes(b"COPY").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/partial-copy-dest/"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);
    assert!(
        xml.contains("/webdav/partial-copy-dest/locked/file.txt"),
        "{xml}"
    );
    assert!(xml.contains("423 Locked"), "{xml}");

    let req = test::TestRequest::get()
        .uri("/webdav/partial-copy-dest/open/file.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "deep COPY should still copy unlocked siblings when another member fails"
    );
    let body = test::read_body(resp).await;
    assert_eq!(String::from_utf8_lossy(&body), "unlocked sibling");
}

#[actix_web::test]
async fn test_webdav_recursive_copy_move_stops_when_existing_destination_collection_is_locked() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
</D:lockinfo>"#;

    for method in ["COPY", "MOVE"] {
        let label = method.to_ascii_lowercase();
        let source_root = format!("/webdav/partial-dest-root-lock-{label}-source/");
        let source_child = format!("{source_root}open/");
        let source_file = format!("{source_child}file.txt");
        let destination_root = format!("/webdav/partial-dest-root-lock-{label}-dest/");
        let destination_child_file = format!("{destination_root}open/file.txt");

        for uri in [&source_root, &source_child, &destination_root] {
            let req = test::TestRequest::with_uri(uri)
                .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
                .insert_header(("Authorization", auth.clone()))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 201, "MKCOL {uri} should succeed");
        }

        let req = test::TestRequest::put()
            .uri(&source_file)
            .insert_header(("Authorization", auth.clone()))
            .set_payload(format!("{method} must not write into locked destination"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status() == 201 || resp.status() == 204);

        let req = test::TestRequest::with_uri(&destination_root)
            .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
            .insert_header(("Authorization", auth.clone()))
            .insert_header(("Content-Type", "application/xml"))
            .insert_header(("Depth", "0"))
            .set_payload(lock_body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200, "LOCK {destination_root} should succeed");

        let req = test::TestRequest::with_uri(&source_root)
            .method(actix_web::http::Method::from_bytes(method.as_bytes()).unwrap())
            .insert_header(("Authorization", auth.clone()))
            .insert_header(("Destination", destination_root.as_str()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            207,
            "{method} into a locked existing collection should return Multi-Status"
        );
        let body = test::read_body(resp).await;
        let xml = String::from_utf8_lossy(&body);
        assert!(xml.contains(&destination_root), "{xml}");
        assert!(xml.contains("423 Locked"), "{xml}");

        let req = test::TestRequest::get()
            .uri(&destination_child_file)
            .insert_header(("Authorization", auth.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            404,
            "{method} must not create descendants inside the locked destination collection"
        );

        let req = test::TestRequest::get()
            .uri(&source_file)
            .insert_header(("Authorization", auth.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            200,
            "{method} failure at the destination root must leave the source tree intact"
        );
    }
}

#[actix_web::test]
async fn test_webdav_recursive_move_continues_unlocked_siblings_after_locked_child() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    for uri in [
        "/webdav/partial-move-continue/",
        "/webdav/partial-move-continue/open/",
        "/webdav/partial-move-continue/locked/",
        "/webdav/partial-move-continue-target/",
    ] {
        let req = test::TestRequest::with_uri(uri)
            .method(actix_web::http::Method::from_bytes(b"MKCOL").unwrap())
            .insert_header(("Authorization", auth.clone()))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    for (uri, body) in [
        (
            "/webdav/partial-move-continue/open/file.txt",
            "unlocked sibling",
        ),
        (
            "/webdav/partial-move-continue/locked/file.txt",
            "locked child",
        ),
    ] {
        let req = test::TestRequest::put()
            .uri(uri)
            .insert_header(("Authorization", auth.clone()))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status() == 201 || resp.status() == 204);
    }

    let lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
</D:lockinfo>"#;
    let req = test::TestRequest::with_uri("/webdav/partial-move-continue/locked/file.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Depth", "0"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::with_uri("/webdav/partial-move-continue/")
        .method(actix_web::http::Method::from_bytes(b"MOVE").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Destination", "/webdav/partial-move-continue-target/moved/"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 207);
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);
    assert!(
        xml.contains("/webdav/partial-move-continue/locked/file.txt"),
        "{xml}"
    );
    assert!(xml.contains("423 Locked"), "{xml}");

    let req = test::TestRequest::get()
        .uri("/webdav/partial-move-continue-target/moved/open/file.txt")
        .insert_header(("Authorization", auth.clone()))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "deep MOVE should still move unlocked siblings when another member fails"
    );
    let body = test::read_body(resp).await;
    assert_eq!(String::from_utf8_lossy(&body), "unlocked sibling");
}

#[actix_web::test]
async fn test_webdav_locked_response_includes_lock_token_submitted_error() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/locked-error-body.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("locked")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
</D:lockinfo>"#;
    let req = test::TestRequest::with_uri("/webdav/locked-error-body.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Depth", "0"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::put()
        .uri("/webdav/locked-error-body.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("blocked")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::LOCKED);
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);
    assert!(xml.contains("lock-token-submitted"), "{xml}");
    assert!(xml.contains("/webdav/locked-error-body.txt"), "{xml}");
    Element::parse(Cursor::new(xml.as_bytes())).expect("locked error XML should parse");
}

#[actix_web::test]
async fn test_webdav_unlock_wrong_uri_includes_lock_token_matches_request_uri_error() {
    let app = setup_with_webdav!();
    let (token, _) = register_and_login!(app);
    let auth = create_webdav_basic_auth!(app, token);

    let req = test::TestRequest::put()
        .uri("/webdav/unlock-error-source.txt")
        .insert_header(("Authorization", auth.clone()))
        .set_payload("locked")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 201 || resp.status() == 204);

    let lock_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
</D:lockinfo>"#;
    let req = test::TestRequest::with_uri("/webdav/unlock-error-source.txt")
        .method(actix_web::http::Method::from_bytes(b"LOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Content-Type", "application/xml"))
        .insert_header(("Depth", "0"))
        .set_payload(lock_body)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let lock_token = resp
        .headers()
        .get("Lock-Token")
        .and_then(|value| value.to_str().ok())
        .expect("LOCK response should include Lock-Token")
        .to_string();

    let req = test::TestRequest::with_uri("/webdav/unlock-error-other.txt")
        .method(actix_web::http::Method::from_bytes(b"UNLOCK").unwrap())
        .insert_header(("Authorization", auth.clone()))
        .insert_header(("Lock-Token", lock_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), actix_web::http::StatusCode::CONFLICT);
    let body = test::read_body(resp).await;
    let xml = String::from_utf8_lossy(&body);
    assert!(xml.contains("lock-token-matches-request-uri"), "{xml}");
    Element::parse(Cursor::new(xml.as_bytes())).expect("unlock error XML should parse");
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
