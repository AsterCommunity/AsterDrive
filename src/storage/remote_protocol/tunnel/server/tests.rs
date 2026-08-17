use super::frame::{REMOTE_TUNNEL_STREAM_FRAME_VERSION, REMOTE_TUNNEL_STREAM_META_LIMIT};
use super::*;
use crate::api::response::ApiResponse;
use crate::storage::remote_protocol::{
    INTERNAL_AUTH_ACCESS_KEY_HEADER, REMOTE_CONTROL_PLANE_BODY_LIMIT,
};
use bytes::Bytes;
use http::{Method, StatusCode};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncReadExt as _;

fn build_remote_node(id: i64, access_key: &str) -> managed_follower::Model {
    let now = chrono::Utc::now();
    managed_follower::Model {
        id,
        name: format!("node-{id}"),
        base_url: String::new(),
        access_key: access_key.to_string(),
        secret_key: format!("secret-{id}"),
        is_enabled: true,
        transport_mode: aster_drive_model::types::RemoteNodeTransportMode::ReverseTunnel,
        last_capabilities: "{}".to_string(),
        last_error: String::new(),
        last_checked_at: None,
        tunnel_last_error: String::new(),
        tunnel_last_seen_at: None,
        created_at: now,
        updated_at: now,
    }
}

#[test]
fn reverse_tunnel_transport_gate_uses_effective_mode() {
    let mut node = build_remote_node(1, "transport-gate");
    assert!(ensure_reverse_tunnel_transport(&node).is_ok());

    node.transport_mode = aster_drive_model::types::RemoteNodeTransportMode::Auto;
    assert!(ensure_reverse_tunnel_transport(&node).is_ok());

    node.base_url = "http://follower.example.com".to_string();
    let auto_direct = ensure_reverse_tunnel_transport(&node)
        .expect_err("auto with a base URL should resolve to direct transport");
    assert!(
        auto_direct
            .message()
            .contains("does not resolve to reverse tunnel")
    );

    node.transport_mode = aster_drive_model::types::RemoteNodeTransportMode::Direct;
    let direct = ensure_reverse_tunnel_transport(&node)
        .expect_err("direct nodes should be rejected by tunnel endpoints");
    assert!(
        direct
            .message()
            .contains("does not resolve to reverse tunnel")
    );
}

fn stream_response_start_frame(request_id: &str, status: StatusCode) -> RemoteTunnelStreamFrame {
    RemoteTunnelStreamFrame {
        kind: RemoteTunnelStreamFrameKind::ResponseStart,
        request_id: request_id.to_string(),
        method: None,
        path_and_query: None,
        headers: vec![("x-stream".to_string(), "ok".to_string())],
        content_length: Some(12),
        status: Some(status.as_u16()),
        message: None,
        body: Bytes::new(),
    }
}

fn stream_response_body_frame(request_id: &str, body: &'static [u8]) -> RemoteTunnelStreamFrame {
    RemoteTunnelStreamFrame {
        kind: RemoteTunnelStreamFrameKind::ResponseBody,
        request_id: request_id.to_string(),
        method: None,
        path_and_query: None,
        headers: Vec::new(),
        content_length: None,
        status: None,
        message: None,
        body: Bytes::from_static(body),
    }
}

fn stream_response_end_frame(request_id: &str) -> RemoteTunnelStreamFrame {
    RemoteTunnelStreamFrame {
        kind: RemoteTunnelStreamFrameKind::ResponseEnd,
        request_id: request_id.to_string(),
        method: None,
        path_and_query: None,
        headers: Vec::new(),
        content_length: None,
        status: None,
        message: None,
        body: Bytes::new(),
    }
}

#[test]
fn tunnel_payloads_serialize_body_as_base64() {
    let request = RemoteTunnelRequest {
        request_id: "req-1".to_string(),
        method: "PUT".to_string(),
        path_and_query: "/api/v1/internal/storage/objects/a".to_string(),
        headers: Vec::new(),
        body: Bytes::from_static(b"hello tunnel"),
    };

    let value = serde_json::to_value(&request).expect("request should serialize");
    assert_eq!(value["body"], "aGVsbG8gdHVubmVs");

    let decoded: RemoteTunnelRequest =
        serde_json::from_value(value).expect("request should deserialize");
    assert_eq!(decoded.body, &b"hello tunnel"[..]);
}

#[test]
fn tunnel_payloads_still_accept_legacy_byte_arrays() {
    let value = serde_json::json!({
        "request_id": "req-1",
        "status": 200,
        "headers": [],
        "body": [104, 101, 108, 108, 111]
    });

    let decoded: RemoteTunnelResponse =
        serde_json::from_value(value).expect("legacy response should deserialize");
    assert_eq!(decoded.body, b"hello");
}

#[test]
fn tunnel_request_still_accepts_legacy_byte_arrays() {
    let value = serde_json::json!({
        "request_id": "req-legacy",
        "method": "PUT",
        "path_and_query": "/api/v1/internal/storage/objects/legacy.bin",
        "headers": [],
        "body": [0, 1, 2, 255]
    });

    let decoded: RemoteTunnelRequest =
        serde_json::from_value(value).expect("legacy request body should deserialize");
    assert_eq!(decoded.body, &b"\x00\x01\x02\xff"[..]);
}

#[test]
fn tunnel_request_rejects_invalid_base64_body() {
    let value = serde_json::json!({
        "request_id": "req-invalid-base64",
        "method": "PUT",
        "path_and_query": "/api/v1/internal/storage/objects/invalid.bin",
        "headers": [],
        "body": "***"
    });

    let error = serde_json::from_value::<RemoteTunnelRequest>(value)
        .expect_err("invalid base64 request body should fail");
    assert!(error.to_string().contains("invalid base64 body"));
}

#[test]
fn poll_response_serialized_length_matches_json_at_base64_boundaries() {
    for body_len in [0, 1, 2, 3, 4, 5, 6, 7, 1023, 1024, 1025] {
        let request = RemoteTunnelRequest {
            request_id: "req-budget".to_string(),
            method: "PUT".to_string(),
            path_and_query: "/api/v1/internal/storage/objects/budget.bin".to_string(),
            headers: vec![(
                "content-type".to_string(),
                "application/octet-stream".to_string(),
            )],
            body: Bytes::from(vec![0x5a; body_len]),
        };
        let expected = serde_json::to_vec(&ApiResponse::ok(RemoteTunnelPollResponse {
            request: Some(request.clone()),
        }))
        .expect("poll response should serialize");
        let measured = super::payload::serialized_poll_response_len(&request)
            .expect("poll response length should be measurable");
        assert_eq!(measured, expected.len(), "body length {body_len}");
    }

    assert_eq!(
        REMOTE_TUNNEL_POLL_BODY_LIMIT,
        ((crate::storage::remote_protocol::REMOTE_CONTROL_PLANE_BODY_LIMIT
            - REMOTE_TUNNEL_POLL_METADATA_BUDGET)
            / 4)
            * 3
    );
}

#[test]
fn stream_frame_roundtrips_metadata_and_body() {
    let frame = RemoteTunnelStreamFrame {
        kind: RemoteTunnelStreamFrameKind::RequestStart,
        request_id: "req-stream-1".to_string(),
        method: Some("PUT".to_string()),
        path_and_query: Some("/api/v1/internal/storage/objects/a.bin?offset=7".to_string()),
        headers: vec![
            (
                "content-type".to_string(),
                "application/octet-stream".to_string(),
            ),
            ("x-extra".to_string(), "yes".to_string()),
        ],
        content_length: Some(11),
        status: None,
        message: None,
        body: Bytes::from_static(b"hello frame"),
    };

    let encoded = encode_stream_frame(&frame).expect("stream frame should encode");
    let decoded = decode_stream_frame(encoded).expect("stream frame should decode");

    assert_eq!(decoded.kind, frame.kind);
    assert_eq!(decoded.request_id, frame.request_id);
    assert_eq!(decoded.method, frame.method);
    assert_eq!(decoded.path_and_query, frame.path_and_query);
    assert_eq!(decoded.headers, frame.headers);
    assert_eq!(decoded.content_length, frame.content_length);
    assert_eq!(decoded.status, frame.status);
    assert_eq!(decoded.message, frame.message);
    assert_eq!(decoded.body, frame.body);
}

#[test]
fn stream_frame_decode_rejects_too_short_input() {
    let error = decode_stream_frame(Bytes::from_static(&[REMOTE_TUNNEL_STREAM_FRAME_VERSION]))
        .expect_err("short stream frame should fail");

    assert!(error.message().contains("frame is too short"));
}

#[test]
fn stream_frame_decode_rejects_unsupported_version() {
    let mut bytes = vec![REMOTE_TUNNEL_STREAM_FRAME_VERSION + 1];
    bytes.extend_from_slice(&0u64.to_be_bytes());

    let error = decode_stream_frame(Bytes::from(bytes))
        .expect_err("unsupported stream frame version should fail");

    assert!(
        error
            .message()
            .contains("unsupported reverse tunnel streaming frame version")
    );
}

#[test]
fn stream_frame_decode_rejects_truncated_metadata() {
    let mut bytes = vec![REMOTE_TUNNEL_STREAM_FRAME_VERSION];
    bytes.extend_from_slice(&4u64.to_be_bytes());
    bytes.extend_from_slice(b"{}");

    let error = decode_stream_frame(Bytes::from(bytes))
        .expect_err("truncated stream frame metadata should fail");

    assert!(error.message().contains("metadata is truncated"));
}

#[test]
fn stream_frame_decode_rejects_metadata_above_limit() {
    let mut bytes = vec![REMOTE_TUNNEL_STREAM_FRAME_VERSION];
    bytes.extend_from_slice(
        &u64::try_from(REMOTE_TUNNEL_STREAM_META_LIMIT + 1)
            .expect("test metadata length should fit u64")
            .to_be_bytes(),
    );

    let error = decode_stream_frame(Bytes::from(bytes))
        .expect_err("oversized stream frame metadata should fail");

    assert!(error.message().contains("metadata is too large"));
}

#[test]
fn stream_frame_encode_rejects_metadata_above_limit() {
    let frame = RemoteTunnelStreamFrame {
        kind: RemoteTunnelStreamFrameKind::Error,
        request_id: "req-large-meta".to_string(),
        method: None,
        path_and_query: None,
        headers: Vec::new(),
        content_length: None,
        status: None,
        message: Some("x".repeat(REMOTE_TUNNEL_STREAM_META_LIMIT)),
        body: Bytes::new(),
    };

    let error =
        encode_stream_frame(&frame).expect_err("oversized stream frame metadata should fail");

    assert!(error.message().contains("metadata is too large"));
}

#[tokio::test]
async fn registry_send_dispatches_to_poll_connection_and_completes_response() {
    let registry = Arc::new(RemoteTunnelRegistry::new());
    let node = build_remote_node(42, "poll-access");
    let (request_rx, _registration) = registry.register_poll(&node);
    let send_handle = tokio::spawn({
        let registry = registry.clone();
        let node = node.clone();
        async move {
            registry
                .send(
                    &node,
                    Method::POST,
                    "/api/v1/internal/storage/objects/file.txt".to_string(),
                    Some(4),
                    vec![("x-extra".to_string(), "yes".to_string())],
                    Bytes::from_static(b"body"),
                )
                .await
        }
    });

    let queued = request_rx
        .await
        .expect("poll connection should receive dispatched request");
    let request = queued.request;
    assert_eq!(request.method, "POST");
    assert_eq!(
        request.path_and_query,
        "/api/v1/internal/storage/objects/file.txt"
    );
    assert_eq!(request.body, &b"body"[..]);
    assert!(
        request.headers.iter().any(|(name, value)| {
            name == INTERNAL_AUTH_ACCESS_KEY_HEADER && value == "poll-access"
        }),
        "signed access key header should be attached"
    );
    assert!(
        request
            .headers
            .iter()
            .any(|(name, value)| name == "content-length" && value == "4"),
        "content length header should be attached"
    );
    assert!(
        request
            .headers
            .iter()
            .any(|(name, value)| name == "x-extra" && value == "yes"),
        "caller-supplied headers should be preserved"
    );

    registry
        .complete(
            &node,
            RemoteTunnelResponse {
                request_id: request.request_id,
                status: 201,
                headers: vec![("x-response".to_string(), "ok".to_string())],
                body: b"created".to_vec(),
            },
        )
        .expect("poll response should complete pending request");
    let response = send_handle
        .await
        .expect("send task should join")
        .expect("poll fallback send should complete");
    assert_eq!(response.status, StatusCode::CREATED);
    assert_eq!(
        response
            .headers
            .get("x-response")
            .and_then(|value| value.to_str().ok()),
        Some("ok")
    );
    assert_eq!(response.body, Bytes::from_static(b"created"));
}

#[tokio::test]
async fn registry_poll_accepts_body_at_exact_policy_limit() {
    let registry = Arc::new(RemoteTunnelRegistry::new());
    let node = build_remote_node(61, "poll-exact-limit");
    let (request_rx, _registration) = registry.register_poll(&node);
    let exact_limit =
        u64::try_from(REMOTE_TUNNEL_POLL_BODY_LIMIT).expect("poll body limit should fit u64");
    let send_handle = tokio::spawn({
        let registry = registry.clone();
        let node = node.clone();
        async move {
            registry
                .send(
                    &node,
                    Method::PUT,
                    "/api/v1/internal/storage/objects/exact-limit.bin".to_string(),
                    Some(exact_limit),
                    Vec::new(),
                    Bytes::from(vec![0x5a; REMOTE_TUNNEL_POLL_BODY_LIMIT]),
                )
                .await
        }
    });

    let request = request_rx
        .await
        .expect("poll connection should receive exact-limit request")
        .request;
    assert_eq!(request.body.len(), REMOTE_TUNNEL_POLL_BODY_LIMIT);
    let measured_len = super::payload::serialized_poll_response_len(&request)
        .expect("exact-limit response length should be measurable");
    assert!(measured_len <= REMOTE_CONTROL_PLANE_BODY_LIMIT);

    let encoded = serde_json::to_vec(&ApiResponse::ok(RemoteTunnelPollResponse {
        request: Some(request.clone()),
    }))
    .expect("exact-limit poll response should serialize");
    assert_eq!(encoded.len(), measured_len);
    assert!(encoded.len() <= REMOTE_CONTROL_PLANE_BODY_LIMIT);
    #[derive(serde::Deserialize)]
    struct FollowerPollEnvelope {
        data: Option<RemoteTunnelPollResponse>,
    }
    let decoded: FollowerPollEnvelope = serde_json::from_slice(&encoded)
        .expect("follower should deserialize exact-limit poll response");
    assert_eq!(
        decoded
            .data
            .and_then(|poll| poll.request)
            .map(|request| request.body.len()),
        Some(REMOTE_TUNNEL_POLL_BODY_LIMIT)
    );

    registry
        .complete(
            &node,
            RemoteTunnelResponse {
                request_id: request.request_id,
                status: 200,
                headers: Vec::new(),
                body: Vec::new(),
            },
        )
        .expect("exact-limit poll response should complete");
    send_handle
        .await
        .expect("exact-limit send task should join")
        .expect("exact-limit poll request should succeed");
}

#[tokio::test]
async fn registry_poll_rejects_one_byte_over_policy_limit_before_dispatch() {
    let registry = Arc::new(RemoteTunnelRegistry::new());
    let node = build_remote_node(62, "poll-over-limit");
    let (request_rx, _registration) = registry.register_poll(&node);

    let error = registry
        .send(
            &node,
            Method::PUT,
            "/api/v1/internal/storage/objects/over-limit.bin".to_string(),
            None,
            Vec::new(),
            Bytes::from(vec![0; REMOTE_TUNNEL_POLL_BODY_LIMIT + 1]),
        )
        .await
        .expect_err("poll body one byte over the policy limit should fail");

    assert_eq!(
        error.storage_error_kind(),
        Some(StorageErrorKind::Unsupported)
    );
    assert!(error.message().contains("polling request body exceeds"));
    assert_eq!(registry.pending_poll_request_count(), 0);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), request_rx)
            .await
            .is_err(),
        "oversized body must not consume or dispatch the poll connection"
    );
}

#[tokio::test]
async fn registry_poll_rejects_oversized_metadata_before_dispatch() {
    let registry = Arc::new(RemoteTunnelRegistry::new());
    let node = build_remote_node(63, "poll-metadata-over-limit");
    let (request_rx, _registration) = registry.register_poll(&node);
    let oversized_path = format!(
        "/api/v1/internal/storage/objects/{}",
        "x".repeat(REMOTE_CONTROL_PLANE_BODY_LIMIT)
    );

    let error = registry
        .send(
            &node,
            Method::GET,
            oversized_path,
            None,
            Vec::new(),
            Bytes::new(),
        )
        .await
        .expect_err("poll metadata beyond the follower budget should fail");

    assert_eq!(
        error.storage_error_kind(),
        Some(StorageErrorKind::Unsupported)
    );
    assert!(error.message().contains("envelope exceeds"));
    assert_eq!(registry.pending_poll_request_count(), 0);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), request_rx)
            .await
            .is_err(),
        "oversized metadata must not consume or dispatch the poll connection"
    );
}

#[tokio::test]
async fn poll_last_seen_keeps_tunnel_online_between_poll_cycles() {
    let registry = RemoteTunnelRegistry::new();
    let node = build_remote_node(43, "poll-online-gap");
    let (_request_rx, registration) = registry.register_poll(&node);
    drop(registration);

    assert!(
        registry.is_online(&node),
        "single-dispatch poll workers have a brief reconnect gap after each request"
    );
}

#[tokio::test]
async fn stale_poll_registration_guard_does_not_remove_newer_connection() {
    let registry = Arc::new(RemoteTunnelRegistry::new());
    let node = build_remote_node(50, "poll-reconnect");
    let (_stale_rx, stale_guard) = registry.register_poll(&node);
    let (current_rx, _current_guard) = registry.register_poll(&node);

    drop(stale_guard);

    let send_handle = tokio::spawn({
        let registry = registry.clone();
        let node = node.clone();
        async move {
            registry
                .send(
                    &node,
                    Method::GET,
                    "/api/v1/internal/storage/objects/reconnect.txt".to_string(),
                    None,
                    Vec::new(),
                    Bytes::new(),
                )
                .await
        }
    });

    let queued = match tokio::time::timeout(Duration::from_millis(250), current_rx).await {
        Ok(Ok(queued)) => queued,
        Ok(Err(error)) => {
            send_handle.abort();
            panic!("newer poll receiver should stay open: {error}");
        }
        Err(_) => {
            send_handle.abort();
            panic!("send should dispatch through the newer poll connection");
        }
    };
    let request = queued.request;
    assert_eq!(
        request.path_and_query,
        "/api/v1/internal/storage/objects/reconnect.txt"
    );

    registry
        .complete(
            &node,
            RemoteTunnelResponse {
                request_id: request.request_id,
                status: 200,
                headers: Vec::new(),
                body: b"ok".to_vec(),
            },
        )
        .expect("newer poll connection should complete request");
    let response = send_handle
        .await
        .expect("send task should join")
        .expect("send should complete through newer connection");
    assert_eq!(response.status, StatusCode::OK);
}

#[tokio::test]
async fn poll_request_sender_does_not_remove_connection_for_same_key_wrong_node() {
    let registry = Arc::new(RemoteTunnelRegistry::new());
    let node = build_remote_node(56, "poll-shared-key");
    let wrong_node = build_remote_node(57, "poll-shared-key");
    let (request_rx, _registration) = registry.register_poll(&node);

    let wrong_send_handle = tokio::spawn({
        let registry = registry.clone();
        let wrong_node = wrong_node.clone();
        async move {
            registry
                .send(
                    &wrong_node,
                    Method::GET,
                    "/api/v1/internal/storage/objects/wrong-node.txt".to_string(),
                    None,
                    Vec::new(),
                    Bytes::new(),
                )
                .await
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let send_handle = tokio::spawn({
        let registry = registry.clone();
        let node = node.clone();
        async move {
            registry
                .send(
                    &node,
                    Method::GET,
                    "/api/v1/internal/storage/objects/right-node.txt".to_string(),
                    None,
                    Vec::new(),
                    Bytes::new(),
                )
                .await
        }
    });

    let queued = tokio::time::timeout(Duration::from_millis(500), request_rx)
        .await
        .expect("matching node should still receive the registered poll connection")
        .expect("registered poll receiver should stay open");
    let request = queued.request;
    assert_eq!(
        request.path_and_query,
        "/api/v1/internal/storage/objects/right-node.txt"
    );

    registry
        .complete(
            &node,
            RemoteTunnelResponse {
                request_id: request.request_id,
                status: 200,
                headers: Vec::new(),
                body: b"ok".to_vec(),
            },
        )
        .expect("matching poll request should complete");
    let response = send_handle
        .await
        .expect("matching send task should join")
        .expect("matching send should complete");
    assert_eq!(response.status, StatusCode::OK);

    let wrong_error = wrong_send_handle
        .await
        .expect("wrong-node send task should join")
        .expect_err("wrong-node send should time out instead of consuming another node connection");
    assert!(wrong_error.message().contains("reverse tunnel is offline"));
}

#[tokio::test]
async fn poll_completion_rejects_wrong_node_without_consuming_pending() {
    let registry = Arc::new(RemoteTunnelRegistry::new());
    let node = build_remote_node(51, "poll-owner");
    let wrong_node = build_remote_node(52, "poll-wrong");
    let (request_rx, _registration) = registry.register_poll(&node);
    let send_handle = tokio::spawn({
        let registry = registry.clone();
        let node = node.clone();
        async move {
            registry
                .send(
                    &node,
                    Method::DELETE,
                    "/api/v1/internal/storage/objects/owned.txt".to_string(),
                    None,
                    Vec::new(),
                    Bytes::new(),
                )
                .await
        }
    });

    let request = request_rx
        .await
        .expect("poll connection should receive request")
        .request;
    let wrong_node_error = registry
        .complete(
            &wrong_node,
            RemoteTunnelResponse {
                request_id: request.request_id.clone(),
                status: 200,
                headers: Vec::new(),
                body: Vec::new(),
            },
        )
        .expect_err("wrong node should not complete poll response");
    assert!(
        wrong_node_error
            .message()
            .contains("does not belong to this remote node")
    );

    registry
        .complete(
            &node,
            RemoteTunnelResponse {
                request_id: request.request_id,
                status: 202,
                headers: Vec::new(),
                body: Vec::new(),
            },
        )
        .expect("owner node should still complete pending response");
    let response = send_handle
        .await
        .expect("send task should join")
        .expect("send should complete after owner response");
    assert_eq!(response.status, StatusCode::ACCEPTED);
}

#[test]
fn registry_tracks_stream_lane_until_registration_guard_drops() {
    let registry = Arc::new(RemoteTunnelRegistry::new());
    let node = build_remote_node(43, "stream-access");

    let (lane_id, _request_rx, guard) = registry.register_stream_lane(&node);
    assert!(!lane_id.is_empty());
    assert!(registry.has_stream_lane(&node));

    drop(guard);
    assert!(!registry.has_stream_lane(&node));
}

#[tokio::test]
async fn registry_stream_invalid_start_does_not_consume_pending_response() {
    let registry = Arc::new(RemoteTunnelRegistry::new());
    let node = build_remote_node(53, "stream-invalid-start");
    let (lane_id, mut request_rx, _guard) = registry.register_stream_lane(&node);
    let send_handle = tokio::spawn({
        let registry = registry.clone();
        let node = node.clone();
        async move {
            registry
                .send_stream(
                    &node,
                    Method::GET,
                    "/api/v1/internal/storage/objects/invalid-start.bin".to_string(),
                    None,
                    Vec::new(),
                    Box::new(std::io::Cursor::new(Bytes::new())),
                )
                .await
        }
    });

    let start = request_rx
        .recv()
        .await
        .expect("stream lane should receive request_start");
    let invalid_start = RemoteTunnelStreamFrame {
        kind: RemoteTunnelStreamFrameKind::ResponseStart,
        request_id: start.request_id.clone(),
        method: None,
        path_and_query: None,
        headers: Vec::new(),
        content_length: None,
        status: None,
        message: None,
        body: Bytes::new(),
    };
    let error = registry
        .complete_stream_frame(&node, &lane_id, invalid_start)
        .await
        .expect_err("response_start without status should fail");
    assert!(error.message().contains("missing status"));

    registry
        .complete_stream_frame(
            &node,
            &lane_id,
            stream_response_start_frame(&start.request_id, StatusCode::OK),
        )
        .await
        .expect("valid response_start should still complete pending stream");
    let response = send_handle
        .await
        .expect("stream send task should join")
        .expect("stream send should complete after valid response_start");
    assert_eq!(response.status, StatusCode::OK);
}

#[tokio::test]
async fn registry_stream_request_roundtrips_start_body_and_end() {
    let registry = Arc::new(RemoteTunnelRegistry::new());
    let node = build_remote_node(44, "stream-roundtrip");
    let (lane_id, mut request_rx, _guard) = registry.register_stream_lane(&node);

    let send_handle = tokio::spawn({
        let registry = registry.clone();
        let node = node.clone();
        async move {
            registry
                .send_stream(
                    &node,
                    Method::PUT,
                    "/api/v1/internal/storage/objects/stream.bin".to_string(),
                    Some(12),
                    vec![(
                        "content-type".to_string(),
                        "application/octet-stream".to_string(),
                    )],
                    Box::new(std::io::Cursor::new(Bytes::from_static(b"request-body"))),
                )
                .await
        }
    });

    let start = request_rx
        .recv()
        .await
        .expect("stream lane should receive request_start");
    assert_eq!(start.kind, RemoteTunnelStreamFrameKind::RequestStart);
    assert_eq!(start.method.as_deref(), Some("PUT"));
    assert_eq!(
        start.path_and_query.as_deref(),
        Some("/api/v1/internal/storage/objects/stream.bin")
    );
    assert_eq!(start.content_length, Some(12));
    assert!(
        start
            .headers
            .iter()
            .any(|(name, value)| name == "content-type" && value == "application/octet-stream")
    );

    let body = request_rx
        .recv()
        .await
        .expect("stream lane should receive request body");
    assert_eq!(body.kind, RemoteTunnelStreamFrameKind::RequestBody);
    assert_eq!(body.request_id, start.request_id);
    assert_eq!(body.body, Bytes::from_static(b"request-body"));

    let end = request_rx
        .recv()
        .await
        .expect("stream lane should receive request end");
    assert_eq!(end.kind, RemoteTunnelStreamFrameKind::RequestEnd);
    assert_eq!(end.request_id, start.request_id);

    registry
        .complete_stream_frame(
            &node,
            &lane_id,
            stream_response_start_frame(&start.request_id, StatusCode::OK),
        )
        .await
        .expect("response_start should complete stream start");
    let mut response = send_handle
        .await
        .expect("stream send task should join")
        .expect("stream send should receive response_start");
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(
        response
            .headers
            .get("x-stream")
            .and_then(|value| value.to_str().ok()),
        Some("ok")
    );

    registry
        .complete_stream_frame(
            &node,
            &lane_id,
            stream_response_body_frame(&start.request_id, b"hello "),
        )
        .await
        .expect("response_body should be accepted");
    registry
        .complete_stream_frame(
            &node,
            &lane_id,
            stream_response_body_frame(&start.request_id, b"stream"),
        )
        .await
        .expect("second response_body should be accepted");
    registry
        .complete_stream_frame(
            &node,
            &lane_id,
            stream_response_end_frame(&start.request_id),
        )
        .await
        .expect("response_end should be accepted");

    let mut response_body = Vec::new();
    response
        .body
        .read_to_end(&mut response_body)
        .await
        .expect("stream response body should read");
    assert_eq!(response_body, b"hello stream");
    drop(response);
    assert!(!registry.has_pending_stream_response(&start.request_id));
}

#[tokio::test]
async fn registry_serializes_stream_requests_on_single_lane_until_body_drops() {
    let registry = Arc::new(RemoteTunnelRegistry::new());
    let node = build_remote_node(54, "stream-single-lane");
    let (lane_id, mut request_rx, _guard) = registry.register_stream_lane(&node);

    let first_handle = tokio::spawn({
        let registry = registry.clone();
        let node = node.clone();
        async move {
            registry
                .send_stream(
                    &node,
                    Method::GET,
                    "/api/v1/internal/storage/objects/first.bin".to_string(),
                    None,
                    Vec::new(),
                    Box::new(std::io::Cursor::new(Bytes::new())),
                )
                .await
        }
    });
    let first_start = request_rx
        .recv()
        .await
        .expect("stream lane should receive first request_start");
    let first_end = request_rx
        .recv()
        .await
        .expect("stream lane should receive first request_end");
    assert_eq!(first_end.kind, RemoteTunnelStreamFrameKind::RequestEnd);
    assert_eq!(first_end.request_id, first_start.request_id);
    registry
        .complete_stream_frame(
            &node,
            &lane_id,
            stream_response_start_frame(&first_start.request_id, StatusCode::OK),
        )
        .await
        .expect("first response_start should complete stream start");
    let first_response = first_handle
        .await
        .expect("first stream send task should join")
        .expect("first stream send should complete after response_start");

    let second_handle = tokio::spawn({
        let registry = registry.clone();
        let node = node.clone();
        async move {
            registry
                .send_stream(
                    &node,
                    Method::GET,
                    "/api/v1/internal/storage/objects/second.bin".to_string(),
                    None,
                    Vec::new(),
                    Box::new(std::io::Cursor::new(Bytes::new())),
                )
                .await
        }
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(100), request_rx.recv())
            .await
            .is_err(),
        "single busy stream lane should not accept a second request yet"
    );

    drop(first_response);

    let first_abort = tokio::time::timeout(Duration::from_millis(500), request_rx.recv())
        .await
        .expect("first stream drop should notify follower before lane reuse")
        .expect("stream lane should stay open for first abort frame");
    assert_eq!(first_abort.kind, RemoteTunnelStreamFrameKind::Error);
    assert_eq!(first_abort.request_id, first_start.request_id);
    assert!(
        first_abort
            .message
            .as_deref()
            .unwrap_or_default()
            .contains("local response reader closed")
    );

    let second_start = tokio::time::timeout(Duration::from_millis(500), request_rx.recv())
        .await
        .expect("second request should dispatch after first response body drops")
        .expect("stream lane should receive second request_start");
    assert_eq!(second_start.kind, RemoteTunnelStreamFrameKind::RequestStart);
    registry
        .complete_stream_frame(
            &node,
            &lane_id,
            stream_response_start_frame(&second_start.request_id, StatusCode::CREATED),
        )
        .await
        .expect("second response_start should complete stream start");
    let second_response = second_handle
        .await
        .expect("second stream send task should join")
        .expect("second stream send should complete after lane release");
    assert_eq!(second_response.status, StatusCode::CREATED);
}

#[tokio::test]
async fn registry_allows_concurrent_stream_requests_on_multiple_lanes() {
    let registry = Arc::new(RemoteTunnelRegistry::new());
    let node = build_remote_node(55, "stream-multi-lane");
    let (lane_a, mut request_rx_a, _guard_a) = registry.register_stream_lane(&node);
    let (lane_b, mut request_rx_b, _guard_b) = registry.register_stream_lane(&node);

    let first_handle = tokio::spawn({
        let registry = registry.clone();
        let node = node.clone();
        async move {
            registry
                .send_stream(
                    &node,
                    Method::GET,
                    "/api/v1/internal/storage/objects/a.bin".to_string(),
                    None,
                    Vec::new(),
                    Box::new(std::io::Cursor::new(Bytes::new())),
                )
                .await
        }
    });
    let second_handle = tokio::spawn({
        let registry = registry.clone();
        let node = node.clone();
        async move {
            registry
                .send_stream(
                    &node,
                    Method::GET,
                    "/api/v1/internal/storage/objects/b.bin".to_string(),
                    None,
                    Vec::new(),
                    Box::new(std::io::Cursor::new(Bytes::new())),
                )
                .await
        }
    });

    let start_a = tokio::time::timeout(Duration::from_millis(500), request_rx_a.recv())
        .await
        .expect("first lane should receive a concurrent request")
        .expect("first lane should stay open");
    let start_b = tokio::time::timeout(Duration::from_millis(500), request_rx_b.recv())
        .await
        .expect("second lane should receive a concurrent request")
        .expect("second lane should stay open");
    assert_eq!(start_a.kind, RemoteTunnelStreamFrameKind::RequestStart);
    assert_eq!(start_b.kind, RemoteTunnelStreamFrameKind::RequestStart);
    assert_ne!(start_a.request_id, start_b.request_id);

    registry
        .complete_stream_frame(
            &node,
            &lane_a,
            stream_response_start_frame(&start_a.request_id, StatusCode::OK),
        )
        .await
        .expect("first lane should complete first response");
    registry
        .complete_stream_frame(
            &node,
            &lane_b,
            stream_response_start_frame(&start_b.request_id, StatusCode::ACCEPTED),
        )
        .await
        .expect("second lane should complete second response");

    let first_response = first_handle
        .await
        .expect("first stream send task should join")
        .expect("first stream send should complete");
    let second_response = second_handle
        .await
        .expect("second stream send task should join")
        .expect("second stream send should complete");
    let statuses = [first_response.status, second_response.status];
    assert!(statuses.contains(&StatusCode::OK));
    assert!(statuses.contains(&StatusCode::ACCEPTED));
}

#[tokio::test]
async fn registry_stream_completion_rejects_wrong_node_and_lane_without_consuming_pending() {
    let registry = Arc::new(RemoteTunnelRegistry::new());
    let node = build_remote_node(45, "stream-owner");
    let wrong_node = build_remote_node(46, "stream-wrong");
    let (lane_id, mut request_rx, _guard) = registry.register_stream_lane(&node);

    let send_handle = tokio::spawn({
        let registry = registry.clone();
        let node = node.clone();
        async move {
            registry
                .send_stream(
                    &node,
                    Method::GET,
                    "/api/v1/internal/storage/objects/owned.bin".to_string(),
                    None,
                    Vec::new(),
                    Box::new(std::io::Cursor::new(Bytes::new())),
                )
                .await
        }
    });
    let start = request_rx
        .recv()
        .await
        .expect("stream lane should receive request_start");

    let wrong_node_error = registry
        .complete_stream_frame(
            &wrong_node,
            &lane_id,
            stream_response_start_frame(&start.request_id, StatusCode::OK),
        )
        .await
        .expect_err("wrong node should not complete stream response");
    assert!(
        wrong_node_error
            .message()
            .contains("does not belong to this remote node")
    );

    let wrong_lane_error = registry
        .complete_stream_frame(
            &node,
            "wrong-lane",
            stream_response_start_frame(&start.request_id, StatusCode::OK),
        )
        .await
        .expect_err("wrong lane should not complete stream response");
    assert!(
        wrong_lane_error
            .message()
            .contains("does not belong to this lane")
    );

    registry
        .complete_stream_frame(
            &node,
            &lane_id,
            stream_response_start_frame(&start.request_id, StatusCode::ACCEPTED),
        )
        .await
        .expect("owner lane should still complete pending stream");
    let response = send_handle
        .await
        .expect("stream send task should join")
        .expect("stream send should complete after valid owner response");
    assert_eq!(response.status, StatusCode::ACCEPTED);
}

#[tokio::test]
async fn registry_stream_error_frame_fails_waiting_start() {
    let registry = Arc::new(RemoteTunnelRegistry::new());
    let node = build_remote_node(47, "stream-error");
    let (lane_id, mut request_rx, _guard) = registry.register_stream_lane(&node);

    let send_handle = tokio::spawn({
        let registry = registry.clone();
        let node = node.clone();
        async move {
            registry
                .send_stream(
                    &node,
                    Method::GET,
                    "/api/v1/internal/storage/objects/error.bin".to_string(),
                    None,
                    Vec::new(),
                    Box::new(std::io::Cursor::new(Bytes::new())),
                )
                .await
        }
    });
    let start = request_rx
        .recv()
        .await
        .expect("stream lane should receive request_start");
    let error = registry
        .complete_stream_frame(
            &node,
            &lane_id,
            RemoteTunnelStreamFrame::error(
                start.request_id,
                "follower stream exploded".to_string(),
            ),
        )
        .await
        .expect_err("error frame should return error to lane handler");
    assert!(error.message().contains("follower stream exploded"));

    let send_result = send_handle.await.expect("stream send task should join");
    let send_error = match send_result {
        Ok(_) => panic!("waiting stream start should fail on error frame"),
        Err(error) => error,
    };
    assert!(send_error.message().contains("follower stream exploded"));
}

#[tokio::test]
async fn registry_stream_error_frame_after_start_surfaces_as_body_read_error() {
    let registry = Arc::new(RemoteTunnelRegistry::new());
    let node = build_remote_node(49, "stream-body-error");
    let (lane_id, mut request_rx, _guard) = registry.register_stream_lane(&node);

    let send_handle = tokio::spawn({
        let registry = registry.clone();
        let node = node.clone();
        async move {
            registry
                .send_stream(
                    &node,
                    Method::GET,
                    "/api/v1/internal/storage/objects/body-error.bin".to_string(),
                    None,
                    Vec::new(),
                    Box::new(std::io::Cursor::new(Bytes::new())),
                )
                .await
        }
    });
    let start = request_rx
        .recv()
        .await
        .expect("stream lane should receive request_start");
    let request_end = request_rx
        .recv()
        .await
        .expect("stream lane should receive request_end");
    assert_eq!(request_end.kind, RemoteTunnelStreamFrameKind::RequestEnd);
    assert_eq!(request_end.request_id, start.request_id);
    registry
        .complete_stream_frame(
            &node,
            &lane_id,
            stream_response_start_frame(&start.request_id, StatusCode::OK),
        )
        .await
        .expect("response_start should be accepted");
    let mut response = send_handle
        .await
        .expect("stream send task should join")
        .expect("stream send should complete after response_start");

    let error = registry
        .complete_stream_frame(
            &node,
            &lane_id,
            RemoteTunnelStreamFrame::error(start.request_id, "body stream failed".to_string()),
        )
        .await
        .expect_err("error frame should return error to lane handler");
    assert!(error.message().contains("body stream failed"));

    let mut body = Vec::new();
    let read_error = response
        .body
        .read_to_end(&mut body)
        .await
        .expect_err("response body read should surface stream error");
    assert!(read_error.to_string().contains("body stream failed"));
}

#[tokio::test]
async fn registry_stream_body_send_failure_notifies_follower_and_releases_pending() {
    let registry = Arc::new(RemoteTunnelRegistry::new());
    let node = build_remote_node(58, "stream-reader-drop");
    let (lane_id, mut request_rx, _guard) = registry.register_stream_lane(&node);

    let send_handle = tokio::spawn({
        let registry = registry.clone();
        let node = node.clone();
        async move {
            registry
                .send_stream(
                    &node,
                    Method::GET,
                    "/api/v1/internal/storage/objects/drop-reader.bin".to_string(),
                    None,
                    Vec::new(),
                    Box::new(std::io::Cursor::new(Bytes::new())),
                )
                .await
        }
    });
    let start = request_rx
        .recv()
        .await
        .expect("stream lane should receive request_start");
    let request_end = request_rx
        .recv()
        .await
        .expect("stream lane should receive request_end");
    assert_eq!(request_end.kind, RemoteTunnelStreamFrameKind::RequestEnd);
    assert_eq!(request_end.request_id, start.request_id);
    registry
        .complete_stream_frame(
            &node,
            &lane_id,
            stream_response_start_frame(&start.request_id, StatusCode::OK),
        )
        .await
        .expect("response_start should be accepted");
    let response = send_handle
        .await
        .expect("stream send task should join")
        .expect("response_start should produce a response reader");
    drop(response);

    let abort = tokio::time::timeout(Duration::from_millis(500), request_rx.recv())
        .await
        .expect("follower should be notified to abort the stream")
        .expect("stream lane should stay open for abort frame");
    assert_eq!(abort.kind, RemoteTunnelStreamFrameKind::Error);
    assert_eq!(abort.request_id, start.request_id);
    assert!(
        abort
            .message
            .as_deref()
            .unwrap_or_default()
            .contains("local response reader closed")
    );

    tokio::time::timeout(Duration::from_millis(500), async {
        while registry.has_pending_stream_response(&start.request_id) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("pending stream response should be released after abort notification");

    let error = registry
        .complete_stream_frame(
            &node,
            &lane_id,
            stream_response_body_frame(&start.request_id, b"late body"),
        )
        .await
        .expect_err("late response_body should fail after local reader drops");
    assert!(error.message().contains("no longer pending"));
}

#[tokio::test]
async fn dropping_stream_registration_fails_pending_stream_request_and_releases_lane() {
    let registry = Arc::new(RemoteTunnelRegistry::new());
    let node = build_remote_node(48, "stream-drop");
    let (lane_id, mut request_rx, guard) = registry.register_stream_lane(&node);

    let send_handle = tokio::spawn({
        let registry = registry.clone();
        let node = node.clone();
        async move {
            registry
                .send_stream(
                    &node,
                    Method::GET,
                    "/api/v1/internal/storage/objects/drop.bin".to_string(),
                    None,
                    Vec::new(),
                    Box::new(std::io::Cursor::new(Bytes::new())),
                )
                .await
        }
    });
    let start = request_rx
        .recv()
        .await
        .expect("stream lane should receive request_start");
    assert!(registry.has_pending_stream_response(&start.request_id));

    drop(guard);
    assert!(!registry.has_stream_lane(&node));
    assert!(!registry.has_pending_stream_response(&start.request_id));

    let send_result = send_handle.await.expect("stream send task should join");
    let error = match send_result {
        Ok(_) => panic!("pending stream request should fail when lane registration drops"),
        Err(error) => error,
    };
    assert!(error.message().contains("streaming lane closed"));
    assert!(!lane_id.is_empty());
}
