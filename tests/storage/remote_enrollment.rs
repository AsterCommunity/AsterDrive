//! 远程节点接入测试。

use crate::common;

use actix_web::{body::to_bytes, test};
use aster_drive::api::api_error_code::ApiErrorCode;
use aster_drive::config::site_url::PUBLIC_SITE_URL_KEY;
use aster_drive::db::repository::follower_enrollment_session_repo;
use aster_drive::runtime::{PrimaryAppState, SharedRuntimeState};
use aster_drive::services::{ops::config, remote::enrollment, remote::remote_node};
use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};
use std::sync::Arc;

async fn configure_public_site_url(state: &PrimaryAppState) {
    config::set(
        state,
        PUBLIC_SITE_URL_KEY,
        vec!["https://master.example.com".to_string()],
        1,
    )
    .await
    .expect("public site URL should be configured");
}

async fn create_remote_node(state: &PrimaryAppState, name: &str) -> remote_node::RemoteNodeInfo {
    remote_node::create(
        state,
        remote_node::CreateRemoteNodeInput {
            name: name.to_string(),
            base_url: format!("https://{name}.example.com"),
            transport_mode: aster_drive_model::types::RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("remote node should be created")
}

#[tokio::test]
async fn test_remote_node_enrollment_command_requires_public_site_url_code() {
    let state = common::setup().await;
    let node = remote_node::create(
        &state,
        remote_node::CreateRemoteNodeInput {
            name: "node-missing-url".to_string(),
            base_url: "https://node-missing-url.example.com".to_string(),
            transport_mode: aster_drive_model::types::RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("remote node should be created");

    let error = enrollment::create_enrollment_command(&state, node.id)
        .await
        .expect_err("missing public_site_url should reject enrollment command");

    assert_eq!(
        error.api_error_code(),
        ApiErrorCode::ConfigPublicSiteUrlRequired
    );
    assert!(error.message().contains("public_site_url"));
}

#[tokio::test]
async fn test_completed_remote_node_enrollment_rejects_new_token() {
    let state = common::setup().await;
    configure_public_site_url(&state).await;

    let node = create_remote_node(&state, "node-a").await;
    assert_eq!(
        node.enrollment_status,
        remote_node::RemoteNodeEnrollmentStatus::NotStarted
    );

    let command = enrollment::create_enrollment_command(&state, node.id)
        .await
        .expect("initial enrollment command should be created");
    assert_eq!(
        remote_node::get(&state, node.id)
            .await
            .expect("remote node should be loaded")
            .enrollment_status,
        remote_node::RemoteNodeEnrollmentStatus::Pending
    );

    let bootstrap = enrollment::redeem_enrollment_token(&state, &command.token)
        .await
        .expect("enrollment token should be redeemable");
    assert_eq!(
        remote_node::get(&state, node.id)
            .await
            .expect("remote node should be loaded")
            .enrollment_status,
        remote_node::RemoteNodeEnrollmentStatus::Redeemed
    );

    enrollment::ack_enrollment_token(&state, &bootstrap.ack_token)
        .await
        .expect("enrollment should be acknowledged");
    let replay_error = enrollment::redeem_enrollment_token(&state, &command.token)
        .await
        .expect_err("acked enrollment token should reject replay");
    assert_eq!(
        replay_error.message(),
        enrollment::ENROLLMENT_TOKEN_COMPLETED_MESSAGE
    );
    assert_eq!(
        remote_node::get(&state, node.id)
            .await
            .expect("remote node should be loaded")
            .enrollment_status,
        remote_node::RemoteNodeEnrollmentStatus::Completed
    );
    let page = remote_node::list_paginated(
        &state,
        10,
        0,
        aster_drive::api::pagination::AdminRemoteNodeSortBy::CreatedAt,
        aster_forge_api::SortOrder::Desc,
    )
    .await
    .expect("remote node list should load");
    let listed = page
        .items
        .iter()
        .find(|item| item.id == node.id)
        .expect("created remote node should be listed");
    assert_eq!(
        listed.enrollment_status,
        remote_node::RemoteNodeEnrollmentStatus::Completed
    );

    let error = enrollment::create_enrollment_command(&state, node.id)
        .await
        .expect_err("completed enrollment should reject a new command");
    assert_eq!(
        error.message(),
        enrollment::REMOTE_NODE_ENROLLMENT_ALREADY_COMPLETED_MESSAGE
    );
}

#[tokio::test]
async fn test_remote_enrollment_rejects_blank_and_unknown_tokens() {
    let state = common::setup().await;

    let blank = enrollment::redeem_enrollment_token(&state, " \n\t ")
        .await
        .expect_err("blank token should be rejected");
    assert_eq!(blank.message(), "token cannot be blank");

    let unknown = enrollment::redeem_enrollment_token(&state, "enr_unknown")
        .await
        .expect_err("unknown token should be rejected");
    assert_eq!(unknown.message(), "invalid enrollment token");
}

#[tokio::test]
async fn test_remote_enrollment_preserves_replaced_and_expired_errors() {
    let state = common::setup().await;
    configure_public_site_url(&state).await;
    let node = create_remote_node(&state, "node-state-boundaries").await;

    let replaced = enrollment::create_enrollment_command(&state, node.id)
        .await
        .expect("first enrollment command should be created");
    let expired = enrollment::create_enrollment_command(&state, node.id)
        .await
        .expect("replacement enrollment command should be created");

    let replaced_error = enrollment::redeem_enrollment_token(&state, &replaced.token)
        .await
        .expect_err("replaced token should be rejected");
    assert_eq!(
        replaced_error.message(),
        enrollment::ENROLLMENT_TOKEN_REPLACED_MESSAGE
    );

    let token_hash = aster_forge_crypto::sha256_hex(expired.token.as_bytes());
    let session =
        follower_enrollment_session_repo::find_by_token_hash(state.writer_db(), &token_hash)
            .await
            .expect("enrollment session lookup should succeed")
            .expect("enrollment session should exist");
    let mut active = session.into_active_model();
    active.expires_at = Set(chrono::Utc::now() - chrono::Duration::seconds(1));
    active
        .update(state.writer_db())
        .await
        .expect("enrollment session expiry should update");

    let expired_error = enrollment::redeem_enrollment_token(&state, &expired.token)
        .await
        .expect_err("expired token should be rejected");
    assert_eq!(
        expired_error.message(),
        enrollment::ENROLLMENT_TOKEN_EXPIRED_MESSAGE
    );
}

#[actix_web::test]
async fn test_remote_enrollment_replay_response_does_not_expose_bootstrap_credentials() {
    let state = common::setup().await;
    configure_public_site_url(&state).await;
    let node = create_remote_node(&state, "node-replay-response").await;
    let command = enrollment::create_enrollment_command(&state, node.id)
        .await
        .expect("enrollment command should be created");
    let bootstrap = enrollment::redeem_enrollment_token(&state, &command.token)
        .await
        .expect("first redemption should succeed");
    let app = create_test_app!(state.clone());

    let req = test::TestRequest::post()
        .uri("/api/v1/public/remote-enrollment/redeem")
        .set_json(serde_json::json!({ "token": command.token }))
        .to_request();
    let response = test::call_service(&app, req).await;
    assert_eq!(response.status(), actix_web::http::StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body())
        .await
        .expect("error response body should be readable");
    let body_text = std::str::from_utf8(&body).expect("error response should be UTF-8 JSON");
    let body_json: serde_json::Value =
        serde_json::from_slice(&body).expect("error response should be valid JSON");

    assert_eq!(body_json["code"], ApiErrorCode::BadRequest.as_str());
    assert_eq!(
        body_json["msg"],
        enrollment::ENROLLMENT_TOKEN_REDEEMED_MESSAGE
    );
    assert!(body_json["data"].is_null());
    for forbidden in [
        "access_key",
        "secret_key",
        "ack_token",
        bootstrap.access_key.as_str(),
        bootstrap.secret_key.as_str(),
        bootstrap.ack_token.as_str(),
    ] {
        assert!(
            !body_text.contains(forbidden),
            "replay error response must not expose {forbidden}"
        );
    }

    enrollment::ack_enrollment_token(&state, &bootstrap.ack_token)
        .await
        .expect("failed replay must not prevent the winner from acknowledging enrollment");
}

#[tokio::test]
async fn test_remote_enrollment_claim_rolls_back_when_bootstrap_assembly_fails() {
    let state = common::setup().await;
    configure_public_site_url(&state).await;
    let node = create_remote_node(&state, "node-claim-rollback").await;
    let command = enrollment::create_enrollment_command(&state, node.id)
        .await
        .expect("enrollment command should be created");

    state.runtime_config().remove(PUBLIC_SITE_URL_KEY);
    let error = enrollment::redeem_enrollment_token(&state, &command.token)
        .await
        .expect_err("bootstrap assembly without public site URL should fail");
    assert_eq!(
        error.api_error_code(),
        ApiErrorCode::ConfigPublicSiteUrlRequired
    );

    state
        .runtime_config()
        .reload(state.writer_db())
        .await
        .expect("public site URL runtime snapshot should reload");
    let bootstrap = enrollment::redeem_enrollment_token(&state, &command.token)
        .await
        .expect("rolled back claim should leave the token redeemable");
    assert_eq!(bootstrap.remote_node_id, node.id);
}

async fn assert_concurrent_remote_enrollment_single_winner(database_url: String) {
    let state_a = common::setup_with_database_url(&database_url).await;
    configure_public_site_url(&state_a).await;
    let node = create_remote_node(&state_a, "node-concurrent-redeem").await;
    let command = enrollment::create_enrollment_command(&state_a, node.id)
        .await
        .expect("enrollment command should be created");

    let second_db = aster_drive::db::connect_with_metrics(
        &aster_drive::config::DatabaseConfig {
            url: database_url.into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .expect("second writer connection should open");
    let state_b = PrimaryAppState {
        db_handles: aster_forge_db::DbHandles::single(second_db),
        ..state_a.clone()
    };

    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let state_a_task = state_a.clone();
    let token_a = command.token.clone();
    let barrier_a = barrier.clone();
    let first = tokio::spawn(async move {
        barrier_a.wait().await;
        enrollment::redeem_enrollment_token(&state_a_task, &token_a).await
    });
    let token_b = command.token.clone();
    let second = tokio::spawn(async move {
        barrier.wait().await;
        enrollment::redeem_enrollment_token(&state_b, &token_b).await
    });

    let (first, second) = tokio::join!(first, second);
    let first = first.expect("first redemption task should not panic");
    let second = second.expect("second redemption task should not panic");
    assert_eq!(
        usize::from(first.is_ok()) + usize::from(second.is_ok()),
        1,
        "exactly one concurrent redemption must receive bootstrap credentials"
    );
    assert_eq!(
        usize::from(first.is_err()) + usize::from(second.is_err()),
        1,
        "exactly one concurrent redemption must lose the claim"
    );

    let winner = first.as_ref().ok().or(second.as_ref().ok()).unwrap();
    assert_eq!(winner.remote_node_id, node.id);
    assert!(!winner.access_key.is_empty());
    assert!(!winner.secret_key.is_empty());
    assert!(!winner.ack_token.is_empty());
    let loser = first.as_ref().err().or(second.as_ref().err()).unwrap();
    assert_eq!(
        loser.message(),
        enrollment::ENROLLMENT_TOKEN_REDEEMED_MESSAGE
    );

    enrollment::ack_enrollment_token(&state_a, &winner.ack_token)
        .await
        .expect("the winning redemption should remain acknowledgeable");
}

#[tokio::test]
async fn test_concurrent_remote_enrollment_single_winner_on_sqlite() {
    let database_path = format!(
        "/tmp/asterdrive-enrollment-race-{}.db",
        uuid::Uuid::new_v4()
    );
    assert_concurrent_remote_enrollment_single_winner(format!("sqlite://{database_path}?mode=rwc"))
        .await;
}

#[tokio::test]
async fn test_concurrent_remote_enrollment_single_winner_on_postgres() {
    if std::env::var("ASTER_TEST_DATABASE_BACKEND").as_deref() != Ok("postgres") {
        return;
    }
    assert_concurrent_remote_enrollment_single_winner(common::postgres_test_database_url().await)
        .await;
}

#[tokio::test]
async fn test_concurrent_remote_enrollment_single_winner_on_mysql() {
    if std::env::var("ASTER_TEST_DATABASE_BACKEND").as_deref() != Ok("mysql") {
        return;
    }
    assert_concurrent_remote_enrollment_single_winner(common::mysql_test_database_url().await)
        .await;
}
