//! 远程节点接入测试。

#[macro_use]
mod common;

use aster_drive::api::api_error_code::ApiErrorCode;
use aster_drive::config::site_url::PUBLIC_SITE_URL_KEY;
use aster_drive::services::{config_service, remote::enrollment, remote::remote_node};

#[tokio::test]
async fn test_remote_node_enrollment_command_requires_public_site_url_code() {
    let state = common::setup().await;
    let node = remote_node::create(
        &state,
        remote_node::CreateRemoteNodeInput {
            name: "node-missing-url".to_string(),
            base_url: "https://node-missing-url.example.com".to_string(),
            transport_mode: aster_drive::types::RemoteNodeTransportMode::Direct,
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
    config_service::set(
        &state,
        PUBLIC_SITE_URL_KEY,
        vec!["https://master.example.com".to_string()],
        1,
    )
    .await
    .expect("public site URL should be configured");

    let node = remote_node::create(
        &state,
        remote_node::CreateRemoteNodeInput {
            name: "node-a".to_string(),
            base_url: "https://node-a.example.com".to_string(),
            transport_mode: aster_drive::types::RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("remote node should be created");
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
        aster_drive::api::pagination::SortOrder::Desc,
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
