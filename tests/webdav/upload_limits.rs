//! Real HTTP acceptance for bounded PUT reception, including a sender that never sends EOF.

use super::*;
use aster_drive_model::entities::{file_blob, storage_policy};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

enum Limit {
    Policy,
    Quota,
    Global,
}

#[actix_web::test]
async fn chunked_policy_limit_returns_413_before_eof_and_removes_temp() {
    assert_open_chunked_put_is_bounded(Limit::Policy).await;
}

#[actix_web::test]
async fn chunked_quota_limit_returns_507_before_eof_and_removes_temp() {
    assert_open_chunked_put_is_bounded(Limit::Quota).await;
}

#[actix_web::test]
async fn chunked_global_limit_returns_413_before_eof_and_removes_temp() {
    assert_open_chunked_put_is_bounded(Limit::Global).await;
}

async fn assert_open_chunked_put_is_bounded(limit: Limit) {
    let mut state = common::setup().await;
    let (username, password) = seed_real_webdav_account(&state).await;
    let owner = user::Entity::find()
        .filter(user::Column::Username.eq("real-webdav-user"))
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    match limit {
        Limit::Policy => {
            let policy = state.policy_snapshot.system_default_policy().unwrap();
            let mut active: storage_policy::ActiveModel = policy.into();
            active.max_file_size = Set(8);
            active.update(state.writer_db()).await.unwrap();
            state
                .driver_registry
                .reload_policy_snapshot(&state.policy_snapshot, state.writer_db())
                .await
                .unwrap();
        }
        Limit::Quota => {
            let mut active: user::ActiveModel = owner.clone().into();
            active.storage_quota = Set(8);
            active.update(state.writer_db()).await.unwrap();
        }
        Limit::Global => Arc::make_mut(&mut state.config).webdav.payload_limit = 8,
    }
    let blobs_before = file_blob::Entity::find()
        .count(state.writer_db())
        .await
        .unwrap();
    let server = start_real_webdav_server(state.clone()).await;
    let address = server.base_url.trim_start_matches("http://");
    let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
    let auth = basic_auth_header(&username, &password);
    let request = format!(
        "PUT /webdav/too-large.bin HTTP/1.1\r\nHost: {address}\r\nAuthorization: {auth}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n8\r\n12345678\r\n1\r\n9\r\n"
    );
    socket.write_all(request.as_bytes()).await.unwrap();
    // No terminating chunk, no socket shutdown: the response must not depend on finish().
    let response = tokio::time::timeout(Duration::from_secs(3), async {
        let mut response = Vec::new();
        let mut buf = [0; 1024];
        while !response.windows(4).any(|w| w == b"\r\n\r\n") {
            let count = socket.read(&mut buf).await.unwrap();
            assert!(count > 0, "server must return an HTTP error before closing");
            response.extend_from_slice(&buf[..count]);
        }
        String::from_utf8(response).unwrap()
    })
    .await
    .expect("server must reject an over-limit stream before EOF");
    let status = if matches!(limit, Limit::Quota) {
        507
    } else {
        413
    };
    assert!(
        response.starts_with(&format!("HTTP/1.1 {status} ")),
        "{response}"
    );
    assert_eq!(
        std::fs::read_dir(&state.config.server.upload_temp_dir)
            .unwrap()
            .count(),
        0
    );
    assert!(
        file_repo::find_by_name_in_folder(state.writer_db(), owner.id, None, "too-large.bin")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        file_blob::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        blobs_before
    );
    assert_eq!(
        user_repo::find_by_id(state.writer_db(), owner.id)
            .await
            .unwrap()
            .storage_used,
        0
    );
    drop(socket);
    server.stop().await;
}

#[actix_web::test]
async fn exact_global_limit_accepts_known_chunked_and_empty_puts() {
    let mut state = common::setup().await;
    Arc::make_mut(&mut state.config).webdav.payload_limit = 8;
    let (username, password) = seed_real_webdav_account(&state).await;
    let server = start_real_webdav_server(state).await;
    let client = reqwest::Client::new();
    for (name, data, chunked) in [
        ("known", "12345678", false),
        ("chunked", "12345678", true),
        ("empty", "", true),
    ] {
        let body = if chunked {
            reqwest::Body::wrap_stream(futures::stream::iter(vec![Ok::<_, std::io::Error>(
                bytes::Bytes::copy_from_slice(data.as_bytes()),
            )]))
        } else {
            reqwest::Body::from(data.to_owned())
        };
        let url = format!("{}/webdav/{name}.txt", server.base_url);
        let response = client
            .put(&url)
            .basic_auth(&username, Some(&password))
            .body(body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::CREATED);
        let received = client
            .get(&url)
            .basic_auth(&username, Some(&password))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!(received, data);
    }
    server.stop().await;
}

#[actix_web::test]
async fn configured_route_limit_prechecks_known_local_put_and_leaves_no_file() {
    let state = common::setup().await;
    let (username, password) = seed_real_webdav_account(&state).await;
    // Intentionally differs from AppState's default: configure() owns the route's limit.
    let config = WebDavConfig {
        payload_limit: 8,
        ..Default::default()
    };
    let db = state.writer_db().clone();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .configure(move |cfg| aster_drive::webdav::configure(cfg, &config, &db)),
    )
    .await;
    let req = test::TestRequest::put()
        .uri("/webdav/local.txt")
        .insert_header(("Authorization", basic_auth_header(&username, &password)))
        .insert_header(("Content-Length", "9"))
        .set_payload("123456789")
        .to_request();
    let response = test::call_service(&app, req).await;
    assert_eq!(
        response.status(),
        actix_web::http::StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(
        std::fs::read_dir(&state.config.server.upload_temp_dir)
            .unwrap()
            .count(),
        0
    );
}

#[actix_web::test]
async fn known_local_put_prechecks_policy_and_quota_without_charging() {
    for quota in [false, true] {
        let state = common::setup().await;
        let (username, password) = seed_real_webdav_account(&state).await;
        let owner = user::Entity::find()
            .filter(user::Column::Username.eq("real-webdav-user"))
            .one(state.writer_db())
            .await
            .unwrap()
            .unwrap();
        if quota {
            let mut active: user::ActiveModel = owner.clone().into();
            active.storage_quota = Set(8);
            active.update(state.writer_db()).await.unwrap();
        } else {
            let mut active: storage_policy::ActiveModel = state
                .policy_snapshot
                .system_default_policy()
                .unwrap()
                .into();
            active.max_file_size = Set(8);
            active.update(state.writer_db()).await.unwrap();
            state
                .driver_registry
                .reload_policy_snapshot(&state.policy_snapshot, state.writer_db())
                .await
                .unwrap();
        }
        let server = start_real_webdav_server(state.clone()).await;
        let response = reqwest::Client::new()
            .put(format!("{}/webdav/precheck.txt", server.base_url))
            .basic_auth(&username, Some(&password))
            .body("123456789")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), if quota { 507 } else { 413 });
        assert!(
            file_repo::find_by_name_in_folder(state.writer_db(), owner.id, None, "precheck.txt")
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            user_repo::find_by_id(state.writer_db(), owner.id)
                .await
                .unwrap()
                .storage_used,
            0
        );
        server.stop().await;
    }
}
