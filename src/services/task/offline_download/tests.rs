use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;

use reqwest::header::{CONTENT_LENGTH, HeaderValue};
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use url::Url;

use super::aria2::{
    Aria2AddUriOptions, Aria2DownloadFailureClass, Aria2DownloadStatus, Aria2OfflineDownloadEngine,
    Aria2RpcCallError, Aria2RpcClient, Aria2RpcMethod, Aria2RpcParam, Aria2TellStatus,
    Aria2TellStatusKey, aria2_rpc_error_is_missing_download, aria2_rpc_error_is_unauthorized,
    classify_aria2_download_failure, parse_aria2_length, parse_aria2_rpc_error_response,
    prepare_aria2_output_dir,
};
use super::naming::{
    filename_from_content_disposition, offline_download_task_display_name_with_engine,
};
use super::runtime::{
    Aria2TaskRuntime, OfflineDownloadRuntimeState, decode_offline_download_runtime_state,
};
use super::source::validate_public_download_ip;
use super::*;
use crate::services::task::{
    TaskExecutionContext, TaskLease, is_task_worker_shutdown_requested,
};

fn request(url: &str) -> CreateOfflineDownloadTaskParams {
    CreateOfflineDownloadTaskParams {
        url: url.to_string(),
        filename: None,
        target_folder_id: None,
        expected_sha256: None,
    }
}

#[test]
fn redact_url_for_display_removes_sensitive_parts() {
    let url =
        Url::parse("https://user:secret@example.com:8443/files/archive.zip?token=secret#download")
            .unwrap();

    assert_eq!(
        redact_url_for_display(&url),
        "https://example.com:8443/files/archive.zip"
    );
}

#[test]
fn parse_source_url_only_accepts_http_and_https() {
    assert!(parse_and_validate_source_url(" https://example.com/file ").is_ok());
    assert!(parse_and_validate_source_url("http://example.com/file").is_ok());
    assert!(parse_and_validate_source_url("ftp://example.com/file").is_err());
    assert!(parse_and_validate_source_url("file:///etc/passwd").is_err());
    assert!(parse_and_validate_source_url("https://").is_err());
    assert!(parse_and_validate_source_url("https://user@example.com/file").is_err());
    assert!(parse_and_validate_source_url("https://:secret@example.com/file").is_err());
}

#[test]
fn effective_timeout_accounts_for_local_rate_limit() {
    let configured = StdDuration::from_secs(600);

    assert_eq!(
        effective_offline_download_request_timeout(configured, 1024 * 1024 * 1024, None).unwrap(),
        configured
    );
    assert_eq!(
        effective_offline_download_request_timeout(
            configured,
            1024 * 1024 * 1024,
            Some(1024 * 1024)
        )
        .unwrap(),
        StdDuration::from_secs(1024 + THROTTLED_DOWNLOAD_TIMEOUT_SLACK_SECS)
    );
    assert_eq!(
        effective_offline_download_request_timeout(
            StdDuration::from_secs(1200),
            1024 * 1024 * 1024,
            Some(1024 * 1024)
        )
        .unwrap(),
        StdDuration::from_secs(1200)
    );
}

#[test]
fn validate_public_download_ip_rejects_sensitive_ipv4_ranges() {
    for ip in [
        Ipv4Addr::new(0, 0, 0, 0),
        Ipv4Addr::new(100, 64, 0, 1),
        Ipv4Addr::new(100, 127, 255, 254),
        Ipv4Addr::new(10, 0, 0, 1),
        Ipv4Addr::new(127, 0, 0, 1),
        Ipv4Addr::new(198, 18, 0, 1),
        Ipv4Addr::new(198, 19, 255, 254),
        Ipv4Addr::new(169, 254, 1, 1),
        Ipv4Addr::new(169, 254, 169, 254),
        Ipv4Addr::new(172, 16, 0, 1),
        Ipv4Addr::new(192, 168, 1, 1),
        Ipv4Addr::new(224, 0, 0, 1),
        Ipv4Addr::new(240, 0, 0, 1),
    ] {
        assert!(
            validate_public_download_ip(IpAddr::V4(ip)).is_err(),
            "{ip} should be blocked"
        );
    }

    assert!(validate_public_download_ip(IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34))).is_ok());
}

#[test]
fn validate_public_download_ip_rejects_sensitive_ipv6_ranges() {
    for ip in [
        Ipv6Addr::UNSPECIFIED,
        Ipv6Addr::LOCALHOST,
        "fc00::1".parse().unwrap(),
        "fd12:3456::1".parse().unwrap(),
        "fe80::1".parse().unwrap(),
        "ff02::1".parse().unwrap(),
        "2001:db8::1".parse().unwrap(),
        "::ffff:127.0.0.1".parse().unwrap(),
        "::ffff:10.0.0.1".parse().unwrap(),
        "::ffff:169.254.169.254".parse().unwrap(),
    ] {
        assert!(
            validate_public_download_ip(IpAddr::V6(ip)).is_err(),
            "{ip} should be blocked"
        );
    }

    assert!(
        validate_public_download_ip(IpAddr::V6(
            "2606:2800:220:1:248:1893:25c8:1946".parse().unwrap()
        ))
        .is_ok()
    );
}

#[test]
fn offline_download_rate_limiter_uses_one_second_batch_cap() {
    assert!(OfflineDownloadRateLimiter::new(None).is_none());
    assert!(OfflineDownloadRateLimiter::new(Some(0)).is_none());

    let limiter = OfflineDownloadRateLimiter::new(Some(5)).unwrap();
    assert_eq!(limiter.max_batch_bytes, 5);

    let large = OfflineDownloadRateLimiter::new(Some(u64::MAX)).unwrap();
    assert_eq!(large.max_batch_bytes, u32::MAX);
}

#[test]
fn declared_content_length_ignores_invalid_values() {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(CONTENT_LENGTH, HeaderValue::from_static("not-a-number"));

    assert_eq!(declared_content_length(&headers).unwrap(), None);
}

#[test]
fn declared_content_length_handles_missing_valid_and_oversized_values() {
    let mut headers = reqwest::header::HeaderMap::new();
    assert_eq!(declared_content_length(&headers).unwrap(), None);

    headers.insert(CONTENT_LENGTH, HeaderValue::from_static("42"));
    assert_eq!(declared_content_length(&headers).unwrap(), Some(42));

    headers.insert(
        CONTENT_LENGTH,
        HeaderValue::from_static("9223372036854775808"),
    );
    assert_eq!(declared_content_length(&headers).unwrap(), None);
}

#[test]
fn verify_expected_sha256_accepts_absent_or_matching_hash_only() {
    let hash = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

    verify_expected_sha256(None, hash).unwrap();
    verify_expected_sha256(Some(hash), hash).unwrap();

    let error = verify_expected_sha256(
        Some("1111111111111111111111111111111111111111111111111111111111111111"),
        hash,
    )
    .expect_err("mismatched expected hash should fail verification");
    assert!(error.message().contains("offline download sha256 mismatch"));
}

#[test]
fn ensure_download_size_allowed_rejects_values_above_limit() {
    ensure_download_size_allowed(1024, 1024).unwrap();

    let error = ensure_download_size_allowed(1025, 1024)
        .expect_err("download larger than configured limit should fail");
    assert!(error.message().contains("exceeds server limit"));
}

#[test]
fn transient_storage_error_marks_error_as_retryable_text() {
    let error = transient_storage_error("remote timeout");

    assert!(
        matches!(error, AsterError::StorageDriverError(message) if message == "transient: remote timeout")
    );
}

#[tokio::test]
async fn offline_download_throttle_returns_immediately_when_unconfigured() {
    let context = TaskExecutionContext::new(TaskLease::new(42, 7), CancellationToken::new());

    OfflineDownloadRateLimiter::throttle(None, 1024, &context)
        .await
        .unwrap();
}

#[tokio::test]
async fn offline_download_throttle_stops_on_shutdown_before_reserving_capacity() {
    let shutdown_token = CancellationToken::new();
    let context = TaskExecutionContext::new(TaskLease::new(42, 7), shutdown_token.clone());
    let limiter = OfflineDownloadRateLimiter::new(Some(1)).unwrap();

    shutdown_token.cancel();

    let error = OfflineDownloadRateLimiter::throttle(Some(&limiter), 1, &context)
        .await
        .expect_err("cancelled task should not wait for throttle capacity");
    assert!(is_task_worker_shutdown_requested(&error));
}

#[test]
fn filename_from_content_disposition_prefers_rfc5987_filename_star() {
    let raw = "attachment; filename=\"fallback.txt\"; filename*=UTF-8''from%20star.txt";

    assert_eq!(
        filename_from_content_disposition(raw).as_deref(),
        Some("from star.txt")
    );
}

#[test]
fn normalize_request_trims_filename_and_sha256() {
    let mut params = request(" https://example.com/file.bin ");
    params.filename = Some(" file.bin ".to_string());
    params.target_folder_id = Some(42);
    params.expected_sha256 =
        Some(" ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789 ".to_string());

    let normalized = normalize_offline_download_request(params).unwrap();

    assert_eq!(normalized.url.as_str(), "https://example.com/file.bin");
    assert_eq!(normalized.filename.as_deref(), Some("file.bin"));
    assert_eq!(normalized.target_folder_id, Some(42));
    assert_eq!(
        normalized.expected_sha256.as_deref(),
        Some("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789")
    );
}

#[test]
fn normalize_request_rejects_invalid_sha256() {
    let mut params = request("https://example.com/file.bin");
    params.expected_sha256 = Some("not-a-sha".to_string());

    assert!(normalize_offline_download_request(params).is_err());
}

#[test]
fn resolve_filename_prefers_requested_then_response_then_url_path() {
    let url = Url::parse("https://example.com/downloads/from-url%20name.txt?token=1").unwrap();

    assert_eq!(
        resolve_offline_download_filename(Some("requested.txt"), Some("response.txt"), &url)
            .unwrap(),
        "requested.txt"
    );
    assert_eq!(
        resolve_offline_download_filename(None, Some("response.txt"), &url).unwrap(),
        "response.txt"
    );
    assert_eq!(
        resolve_offline_download_filename(None, None, &url).unwrap(),
        "from-url name.txt"
    );
}

#[test]
fn resolve_filename_falls_back_when_url_segment_is_not_valid_name() {
    let url = Url::parse("https://example.com/downloads/CON").unwrap();

    assert_eq!(
        resolve_offline_download_filename(None, None, &url).unwrap(),
        "download"
    );
}

#[test]
fn offline_download_task_display_name_includes_selected_engine() {
    let payload = OfflineDownloadTaskPayload {
        url: "https://example.com/file.bin".to_string(),
        filename: Some("file.bin".to_string()),
        target_folder_id: None,
        expected_sha256: None,
        source_display_url: Some("https://example.com/file.bin".to_string()),
    };
    let base = offline_download_task_base_display_name(&payload);

    assert_eq!(base, "Import file.bin from link");
    assert_eq!(
        offline_download_task_display_name_with_engine(
            &base,
            operations::OfflineDownloadEngine::Builtin
        ),
        "Import file.bin from link via AsterDrive built-in"
    );
    assert_eq!(
        offline_download_task_display_name_with_engine(
            &base,
            operations::OfflineDownloadEngine::Aria2
        ),
        "Import file.bin from link via aria2"
    );
}

#[test]
fn aria2_options_use_safe_whitelist_and_per_download_limit() {
    let engine = Aria2OfflineDownloadEngine {
        max_bytes: 1024,
        download_timeout: StdDuration::from_secs(60),
        client: Aria2RpcClient::new(
            "http://127.0.0.1:6800/jsonrpc",
            Some("secret".to_string()),
            StdDuration::from_secs(1),
        )
        .unwrap(),
        split: 4,
        max_connection_per_server: 2,
        lowest_speed_limit_bytes_per_sec: Some(128),
    };
    let request = OfflineDownloadStartRequest {
        url: Url::parse("https://example.com/file.bin").unwrap(),
        temp_path: PathBuf::from("/tmp/asterdrive-task/source"),
        expected_sha256: None,
        max_bytes_per_sec: Some(1024),
        runtime_json: None,
    };

    let options = serde_json::to_value(engine.options(&request)).unwrap();

    assert_eq!(
        options.get("dir"),
        Some(&Value::String("/tmp/asterdrive-task".to_string()))
    );
    assert_eq!(
        options.get("out"),
        Some(&Value::String("source".to_string()))
    );
    assert_eq!(options.get("split"), Some(&Value::String("4".to_string())));
    assert_eq!(
        options.get("max-connection-per-server"),
        Some(&Value::String("2".to_string()))
    );
    assert_eq!(
        options.get("lowest-speed-limit"),
        Some(&Value::String("128".to_string()))
    );
    assert_eq!(
        options.get("max-download-limit"),
        Some(&Value::String("1024".to_string()))
    );
    assert!(
        options.get("max-overall-download-limit").is_none(),
        "task-level settings must not mutate aria2 global bandwidth limits"
    );
}

#[test]
fn aria2_rpc_params_put_secret_token_first() {
    let client = Aria2RpcClient::new(
        "http://127.0.0.1:6800/jsonrpc",
        Some("rpc-secret".to_string()),
        StdDuration::from_secs(1),
    )
    .unwrap();

    assert_eq!(
        serde_json::to_value(client.params(Aria2RpcParam::String("gid-1"), None)).unwrap(),
        json!(["token:rpc-secret", "gid-1"])
    );
}

#[test]
fn aria2_rpc_empty_params_still_put_secret_token_first() {
    let client = Aria2RpcClient::new(
        "http://127.0.0.1:6800/jsonrpc",
        Some("rpc-secret".to_string()),
        StdDuration::from_secs(1),
    )
    .unwrap();

    assert_eq!(
        serde_json::to_value(client.params_empty()).unwrap(),
        json!(["token:rpc-secret"])
    );
}

#[test]
fn aria2_add_uri_params_match_json_rpc_shape() {
    let client = Aria2RpcClient::new(
        "http://127.0.0.1:6800/jsonrpc",
        Some("rpc-secret".to_string()),
        StdDuration::from_secs(1),
    )
    .unwrap();
    let options = Aria2AddUriOptions {
        dir: "/tmp/task".to_string(),
        out: "source".to_string(),
        allow_overwrite: "true".to_string(),
        auto_file_renaming: "false".to_string(),
        follow_torrent: "false".to_string(),
        follow_metalink: "false".to_string(),
        max_redirect: "0".to_string(),
        user_agent: "AsterDrive-test".to_string(),
        split: "1".to_string(),
        max_connection_per_server: "1".to_string(),
        lowest_speed_limit: None,
        max_download_limit: Some("1024".to_string()),
    };

    let uris = ["https://example.com/"];

    assert_eq!(
        serde_json::to_value(client.params(
            Aria2RpcParam::Uris(&uris),
            Some(Aria2RpcParam::AddUriOptions(&options))
        ))
        .unwrap(),
        json!([
            "token:rpc-secret",
            ["https://example.com/"],
            {
                "dir": "/tmp/task",
                "out": "source",
                "allow-overwrite": "true",
                "auto-file-renaming": "false",
                "follow-torrent": "false",
                "follow-metalink": "false",
                "max-redirect": "0",
                "user-agent": "AsterDrive-test",
                "split": "1",
                "max-connection-per-server": "1",
                "max-download-limit": "1024"
            }
        ])
    );
}

#[test]
fn aria2_tell_status_params_request_only_required_keys() {
    let client = Aria2RpcClient::new(
        "http://127.0.0.1:6800/jsonrpc",
        None,
        StdDuration::from_secs(1),
    )
    .unwrap();
    let keys = [
        Aria2TellStatusKey::Gid,
        Aria2TellStatusKey::Status,
        Aria2TellStatusKey::TotalLength,
        Aria2TellStatusKey::CompletedLength,
        Aria2TellStatusKey::DownloadSpeed,
        Aria2TellStatusKey::ErrorCode,
        Aria2TellStatusKey::ErrorMessage,
    ];

    assert_eq!(
        serde_json::to_value(client.params(
            Aria2RpcParam::String("gid-1"),
            Some(Aria2RpcParam::TellStatusKeys(&keys))
        ))
        .unwrap(),
        json!([
            "gid-1",
            [
                "gid",
                "status",
                "totalLength",
                "completedLength",
                "downloadSpeed",
                "errorCode",
                "errorMessage"
            ]
        ])
    );
}

#[test]
fn aria2_rpc_params_omit_secret_when_absent() {
    let client = Aria2RpcClient::new(
        "http://127.0.0.1:6800/jsonrpc",
        None,
        StdDuration::from_secs(1),
    )
    .unwrap();

    assert_eq!(
        serde_json::to_value(client.params(Aria2RpcParam::String("gid-1"), None)).unwrap(),
        json!(["gid-1"])
    );
}

#[test]
fn aria2_options_omit_optional_limits_when_unset() {
    let engine = Aria2OfflineDownloadEngine {
        max_bytes: 1024,
        download_timeout: StdDuration::from_secs(60),
        client: Aria2RpcClient::new(
            "http://127.0.0.1:6800/jsonrpc",
            None,
            StdDuration::from_secs(1),
        )
        .unwrap(),
        split: 1,
        max_connection_per_server: 1,
        lowest_speed_limit_bytes_per_sec: None,
    };
    let request = OfflineDownloadStartRequest {
        url: Url::parse("https://example.com/file.bin").unwrap(),
        temp_path: PathBuf::from("/tmp/asterdrive-task/source"),
        expected_sha256: None,
        max_bytes_per_sec: None,
        runtime_json: None,
    };

    let options = engine.options(&request);

    let options = serde_json::to_value(options).unwrap();
    assert!(options.get("lowest-speed-limit").is_none());
    assert!(options.get("max-download-limit").is_none());
    assert!(options.get("max-overall-download-limit").is_none());
}

#[test]
fn aria2_rpc_call_error_preserves_structured_code_until_conversion() {
    let missing = Aria2RpcCallError::Rpc {
        method: Aria2RpcMethod::ForceRemove,
        code: 1,
        message: "GID not found".to_string(),
        http_status: None,
    };
    assert!(matches!(missing, Aria2RpcCallError::Rpc { code: 1, .. }));

    let other = Aria2RpcCallError::Rpc {
        method: Aria2RpcMethod::ForceRemove,
        code: 10,
        message: "other failure".to_string(),
        http_status: None,
    };
    assert!(!matches!(other, Aria2RpcCallError::Rpc { code: 1, .. }));

    let converted = other.into_aster_error();
    assert!(matches!(
        converted,
        AsterError::StorageDriverError(message)
            if message.to_string().contains("aria2.forceRemove failed with code 10: other failure")
    ));
}

#[test]
fn aria2_rpc_http_error_body_preserves_json_rpc_unauthorized() {
    let error = parse_aria2_rpc_error_response(
        r#"{"jsonrpc":"2.0","id":"asterdrive-test","error":{"code":1,"message":"Unauthorized"}}"#,
    )
    .expect("aria2 JSON-RPC error body should parse");

    assert_eq!(error.code, 1);
    assert_eq!(error.message, "Unauthorized");
    assert!(aria2_rpc_error_is_unauthorized(
        Aria2RpcMethod::GetVersion,
        error.code,
        &error.message,
        Some(reqwest::StatusCode::BAD_REQUEST),
    ));
    assert!(aria2_rpc_error_is_unauthorized(
        Aria2RpcMethod::GetVersion,
        error.code,
        &error.message,
        None,
    ));
    assert!(!aria2_rpc_error_is_unauthorized(
        Aria2RpcMethod::ForceRemove,
        error.code,
        &error.message,
        Some(reqwest::StatusCode::BAD_REQUEST),
    ));
}

#[test]
fn aria2_rpc_missing_download_detection_is_method_and_status_specific() {
    assert!(aria2_rpc_error_is_missing_download(
        Aria2RpcMethod::ForceRemove,
        1,
        "GID 1234567890abcdef not found",
        None,
    ));
    assert!(aria2_rpc_error_is_missing_download(
        Aria2RpcMethod::RemoveDownloadResult,
        1,
        "GID 1234567890abcdef not found",
        None,
    ));
    assert!(aria2_rpc_error_is_missing_download(
        Aria2RpcMethod::ForceRemove,
        1,
        "GID 1234567890abcdef not found",
        Some(reqwest::StatusCode::BAD_REQUEST),
    ));
    assert!(aria2_rpc_error_is_missing_download(
        Aria2RpcMethod::ForceRemove,
        1,
        "GID 1234567890abcdef does not exist",
        Some(reqwest::StatusCode::BAD_REQUEST),
    ));
    assert!(!aria2_rpc_error_is_missing_download(
        Aria2RpcMethod::GetVersion,
        1,
        "GID 1234567890abcdef not found",
        None,
    ));
    assert!(!aria2_rpc_error_is_missing_download(
        Aria2RpcMethod::ForceRemove,
        2,
        "GID 1234567890abcdef not found",
        Some(reqwest::StatusCode::BAD_REQUEST),
    ));
    assert!(!aria2_rpc_error_is_missing_download(
        Aria2RpcMethod::ForceRemove,
        1,
        "wrong type",
        Some(reqwest::StatusCode::BAD_REQUEST),
    ));
}

#[cfg(unix)]
#[tokio::test]
async fn aria2_output_dir_is_writable_by_external_process_user() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "aster-drive-aria2-output-dir-{}",
        crate::utils::id::new_uuid()
    ));
    let temp_path = root.join("tasks/42/7/source");

    prepare_aria2_output_dir(&temp_path).await.unwrap();

    let token_dir = temp_path.parent().unwrap();
    let task_dir = token_dir.parent().unwrap();
    let tasks_dir = task_dir.parent().unwrap();
    let modes = [tasks_dir, task_dir, token_dir]
        .map(|dir| std::fs::metadata(dir).unwrap().permissions().mode() & 0o777);
    let _ = std::fs::remove_dir_all(root);

    assert_eq!(modes, [0o711, 0o777, 0o777]);
}

#[cfg(unix)]
#[tokio::test]
async fn aria2_output_dir_repairs_existing_parent_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "aster-drive-aria2-existing-parent-{}",
        crate::utils::id::new_uuid()
    ));
    let tasks_dir = root.join("tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();
    std::fs::set_permissions(&tasks_dir, std::fs::Permissions::from_mode(0o755)).unwrap();

    let temp_path = tasks_dir.join("42/7/source");
    prepare_aria2_output_dir(&temp_path).await.unwrap();

    let token_dir = temp_path.parent().unwrap();
    let task_dir = token_dir.parent().unwrap();
    let modes = [&tasks_dir, task_dir, token_dir]
        .map(|dir| std::fs::metadata(dir).unwrap().permissions().mode() & 0o777);
    let _ = std::fs::remove_dir_all(root);

    assert_eq!(modes, [0o711, 0o777, 0o777]);
}

#[test]
fn aria2_rpc_unauthorized_maps_to_auth_api_code() {
    let error = Aria2RpcCallError::Rpc {
        method: Aria2RpcMethod::GetVersion,
        code: 1,
        message: "Unauthorized".to_string(),
        http_status: Some(reqwest::StatusCode::BAD_REQUEST),
    }
    .into_aster_error();

    assert_eq!(
        error.api_error_code_override(),
        Some(crate::api::api_error_code::ApiErrorCode::OfflineDownloadAria2RpcAuthFailed)
    );
    assert!(error.message().contains("authentication failed"));
}

#[test]
fn aria2_download_status_deserializes_known_and_unknown_values() {
    #[derive(Deserialize)]
    struct Wrapper {
        status: Aria2DownloadStatus,
    }

    assert!(matches!(
        serde_json::from_str::<Wrapper>(r#"{"status":"active"}"#)
            .unwrap()
            .status,
        Aria2DownloadStatus::Active
    ));
    assert!(matches!(
        serde_json::from_str::<Wrapper>(r#"{"status":"future"}"#)
            .unwrap()
            .status,
        Aria2DownloadStatus::Unknown(value) if value == "future"
    ));
}

fn aria2_error_status(message: &str) -> Aria2TellStatus {
    Aria2TellStatus {
        status: Aria2DownloadStatus::Error,
        total_length: "0".to_string(),
        completed_length: "0".to_string(),
        error_code: Some("1".to_string()),
        error_message: Some(message.to_string()),
    }
}

#[test]
fn aria2_download_failure_classifier_isolates_client_error_heuristics() {
    assert_eq!(
        classify_aria2_download_failure(&aria2_error_status("HTTP response status code 404")),
        Aria2DownloadFailureClass::PermanentClientError
    );
    assert_eq!(
        classify_aria2_download_failure(&aria2_error_status("Resource not found")),
        Aria2DownloadFailureClass::PermanentClientError
    );
    assert_eq!(
        classify_aria2_download_failure(&aria2_error_status("connection timed out")),
        Aria2DownloadFailureClass::TransientOrUnknown
    );
}

#[test]
fn parse_aria2_length_handles_empty_and_rejects_invalid_or_oversized_values() {
    assert_eq!(parse_aria2_length("", "aria2 totalLength").unwrap(), 0);
    assert_eq!(parse_aria2_length("0", "aria2 totalLength").unwrap(), 0);
    assert_eq!(parse_aria2_length("42", "aria2 totalLength").unwrap(), 42);
    assert!(parse_aria2_length("-1", "aria2 totalLength").is_err());
    assert!(parse_aria2_length("not-a-number", "aria2 totalLength").is_err());
    assert!(parse_aria2_length(&u64::MAX.to_string(), "aria2 totalLength").is_err());
}

#[test]
fn offline_download_runtime_state_round_trips_aria2_gid() {
    let runtime = OfflineDownloadRuntimeState {
        engine: Some(operations::OfflineDownloadEngine::Aria2),
        aria2: Some(Aria2TaskRuntime {
            gid: "abc123".to_string(),
            processing_token: 7,
        }),
    };
    let raw = serde_json::to_string(&runtime).unwrap();
    let decoded = decode_offline_download_runtime_state(Some(&raw));

    assert_eq!(
        decoded.engine,
        Some(operations::OfflineDownloadEngine::Aria2)
    );
    let aria2 = decoded.aria2.unwrap();
    assert_eq!(aria2.gid, "abc123");
    assert_eq!(aria2.processing_token, 7);
}

#[test]
fn offline_download_runtime_state_ignores_missing_blank_and_invalid_json() {
    assert!(decode_offline_download_runtime_state(None).aria2.is_none());
    assert!(
        decode_offline_download_runtime_state(Some("  "))
            .aria2
            .is_none()
    );
    assert!(
        decode_offline_download_runtime_state(Some("{not-json"))
            .aria2
            .is_none()
    );
}
