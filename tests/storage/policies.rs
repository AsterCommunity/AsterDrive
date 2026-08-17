//! 存储策略管理测试

use crate::common;
use aster_drive::api::api_error_code::ApiErrorCode;
use aster_drive::config::site_url;

use actix_web::test;
use chrono::{Duration, Utc};
use serde_json::Value;

async fn list_storage_driver_descriptors_via_admin<S, B>(
    app: &S,
    token: &str,
    context: Option<&str>,
) -> Vec<Value>
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = actix_web::Error,
        >,
    B: actix_web::body::MessageBody + 'static,
{
    let uri = context.map_or_else(
        || "/api/v1/admin/policies/storage-drivers".to_string(),
        |context| format!("/api/v1/admin/policies/storage-drivers?context={context}"),
    );
    let req = test::TestRequest::get()
        .uri(&uri)
        .insert_header(("Cookie", common::access_cookie_header(token)))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    body["data"].as_array().expect("descriptor list").to_vec()
}

async fn create_local_policy_via_admin<S, B>(app: &S, token: &str, name: &str) -> i64
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = actix_web::Error,
        >,
    B: actix_web::body::MessageBody + 'static,
{
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(token)))
		.insert_header(common::csrf_header_for(token))
		.set_json(serde_json::json!({
			"name": name,
			"connection": {
				"connector_config": {
					"format_version": 1,
					"connector_id": "asterdrive.storage.local",
					"schema_version": 1,
					"values": {
						"base_path": format!("/tmp/asterdrive-{}-{}", name.to_ascii_lowercase().replace(' ', "-"), uuid::Uuid::new_v4())
					}
				},
				"behavior": {},
				"credential": { "mode": "none" }
			},
			"max_file_size": 0,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    body["data"]["id"].as_i64().unwrap()
}

async fn create_tencent_cos_policy_via_admin<S, B>(app: &S, token: &str, name: &str) -> i64
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = actix_web::Error,
        >,
    B: actix_web::body::MessageBody + 'static,
{
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(token)))
        .insert_header(common::csrf_header_for(token))
        .set_json(serde_json::json!({
            "name": name,
            "connection": {
                "connector_config": {
                    "format_version": 1,
                    "connector_id": "asterdrive.storage.tencent_cos",
                    "schema_version": 1,
                    "values": {
                        "endpoint": "https://cos.ap-guangzhou.myqcloud.com",
                        "bucket": "media-1250000000",
                        "base_path": ""
                    }
                },
                "behavior": {},
                "credential": {
                    "mode": "static",
                    "values": {
                        "tencent_cos_secret_id": "AKIDEXAMPLE",
                        "tencent_cos_secret_key": "SECRETEXAMPLE"
                    }
                }
            },
            "max_file_size": 0,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    body["data"]["id"].as_i64().unwrap()
}

async fn create_s3_policy_via_admin<S, B>(app: &S, token: &str, name: &str, endpoint: &str) -> i64
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = actix_web::Error,
        >,
    B: actix_web::body::MessageBody + 'static,
{
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(token)))
        .insert_header(common::csrf_header_for(token))
        .set_json(serde_json::json!({
            "name": name,
            "connection": s3_connection_json(
                endpoint,
                "media-1250000000",
                "tenant/files",
                "AKIDEXAMPLE",
                "SECRETEXAMPLE",
            ),
            "max_file_size": 0,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    body["data"]["id"].as_i64().unwrap()
}

async fn promote_policy_to_tencent_cos<S, B>(
    app: &S,
    token: &str,
    policy_id: i64,
) -> actix_web::dev::ServiceResponse<B>
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = actix_web::Error,
        >,
    B: actix_web::body::MessageBody + 'static,
{
    test::call_service(
        app,
        test::TestRequest::post()
            .uri(&format!(
                "/api/v1/admin/policies/{policy_id}/promote-connector"
            ))
            .insert_header(("Cookie", common::access_cookie_header(token)))
            .insert_header(common::csrf_header_for(token))
            .set_json(serde_json::json!({
                "target_connector_id": "asterdrive.storage.tencent_cos",
                "promotion_id": "promote_from_s3"
            }))
            .to_request(),
    )
    .await
}

fn local_action_connection() -> Value {
    serde_json::json!({
        "connector_config": {
            "format_version": 1,
            "connector_id": "asterdrive.storage.local",
            "schema_version": 2,
            "values": {
                "base_path": format!("/tmp/asterdrive-action-local-{}", uuid::Uuid::new_v4())
            }
        },
        "behavior": {},
        "credential": { "mode": "none" }
    })
}

fn tencent_cos_action_connection() -> Value {
    serde_json::json!({
        "connector_config": {
            "format_version": 1,
            "connector_id": "asterdrive.storage.tencent_cos",
            "schema_version": 1,
            "values": {
                "endpoint": "https://cos.ap-guangzhou.myqcloud.com",
                "bucket": "media-1250000000",
                "base_path": ""
            }
        },
        "behavior": {},
        "credential": {
            "mode": "static",
            "values": {
                "tencent_cos_secret_id": "AKIDEXAMPLE",
                "tencent_cos_secret_key": "SECRETEXAMPLE"
            }
        }
    })
}

fn connection_json(connection: aster_drive::storage::StorageConnectorConnectionInput) -> Value {
    serde_json::to_value(connection).expect("typed storage connector connection should serialize")
}

fn local_connection_json(base_path: impl Into<String>) -> Value {
    connection_json(common::local_connection(base_path))
}

fn s3_connection_json(
    endpoint: impl Into<String>,
    bucket: impl Into<String>,
    base_path: impl Into<String>,
    access_key: impl Into<String>,
    secret_key: impl Into<String>,
) -> Value {
    connection_json(common::s3_connection(
        endpoint, bucket, base_path, access_key, secret_key,
    ))
}

fn remote_connection_json(
    base_path: impl Into<String>,
    remote_node_id: Option<i64>,
    remote_storage_target_key: Option<String>,
    remote_download_strategy: aster_drive_model::types::RemoteDownloadStrategy,
    remote_upload_strategy: aster_drive_model::types::RemoteUploadStrategy,
) -> Value {
    connection_json(common::remote_connection(
        base_path,
        remote_node_id,
        remote_storage_target_key,
        remote_download_strategy,
        remote_upload_strategy,
    ))
}

fn onedrive_connection_json(base_path: impl Into<String>) -> Value {
    serde_json::json!({
        "connector_config": {
            "format_version": 1,
            "connector_id": "asterdrive.storage.onedrive",
            "schema_version": 1,
            "values": {
                "base_path": base_path.into(),
                "provider_resumable_upload_strategy": "server_relay",
                "provider_download_strategy": "server_relay",
                "provider_download_filename_mode": "provider_native"
            }
        },
        "behavior": {},
        "credential": {
            "mode": "authorization_application",
            "values": {
                "client_id": "test-client-id",
                "client_secret": "test-client-secret"
            }
        }
    })
}

fn azure_blob_connection_json(endpoint: impl Into<String>, container: impl Into<String>) -> Value {
    serde_json::json!({
        "connector_config": {
            "format_version": 1,
            "connector_id": "asterdrive.storage.azure_blob",
            "schema_version": 1,
            "values": {
                "endpoint": endpoint.into(),
                "bucket": container.into(),
                "base_path": "",
                "object_storage_upload_strategy": "relay_stream",
                "object_storage_download_strategy": "relay_stream"
            }
        },
        "behavior": {},
        "credential": {
            "mode": "static",
            "values": {
                "azure_blob_account_name": "test-account",
                "azure_blob_account_key": "test-account-key"
            }
        }
    })
}

#[actix_web::test]
async fn test_policy_input_rejects_invalid_and_unavailable_connectors_as_bad_requests() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for connector_id in ["com.example.missing", "INVALID ID"] {
        let connection = serde_json::json!({
            "connector_config": {
                "format_version": 1,
                "connector_id": connector_id,
                "schema_version": 1,
                "values": {}
            },
            "behavior": {},
            "credential": { "mode": "none" }
        });
        for (uri, payload) in [
            (
                "/api/v1/admin/policies",
                serde_json::json!({
                    "name": "Unknown connector",
                    "connection": connection.clone(),
                    "max_file_size": 0,
                    "is_default": false
                }),
            ),
            (
                "/api/v1/admin/policies/test",
                serde_json::json!({ "connection": connection }),
            ),
        ] {
            let req = test::TestRequest::post()
                .uri(uri)
                .insert_header(("Cookie", common::access_cookie_header(&token)))
                .insert_header(common::csrf_header_for(&token))
                .set_json(payload)
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 400, "{uri} should reject {connector_id}");
            let body: Value = test::read_body_json(resp).await;
            assert_eq!(body["code"], ApiErrorCode::BadRequest.as_str());
        }
    }
}

#[actix_web::test]
async fn test_admin_storage_driver_descriptors_expose_capability_matrix() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies/storage-drivers")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let descriptors = body["data"].as_array().expect("descriptor list");

    assert_eq!(descriptors.len(), 9);

    let descriptor = |connector_id: &str| {
        descriptors
            .iter()
            .find(|item| item["connector_id"] == connector_id)
            .unwrap_or_else(|| panic!("{connector_id} descriptor should exist"))
    };

    let onedrive = descriptor("asterdrive.storage.onedrive");
    assert_eq!(onedrive["credential_mode"], "oauth_delegated");
    assert_eq!(
        onedrive["deployment_scope"],
        "shared_across_primary_instances"
    );
    assert_eq!(onedrive["supports_initial_setup"], false);
    assert_eq!(onedrive["requires_authorization"], true);
    assert_eq!(onedrive["capabilities"]["presigned_download"], true);
    let qiniu = descriptor("asterdrive.storage.qiniu");
    assert_eq!(qiniu["credential_mode"], "static_secret");
    assert_eq!(qiniu["deployment_scope"], "shared_across_primary_instances");
    assert_eq!(qiniu["config_schema_version"], 1);
    assert_eq!(qiniu["capabilities"]["presigned_download"], true);
    assert_eq!(qiniu["upload_workflows"]["presigned_upload"], true);
    let qiniu_actions = qiniu["actions"].as_array().expect("Qiniu actions");
    assert!(qiniu_actions.iter().any(|action| {
        action["action_id"] == "test_draft_connection"
            && action["kind"] == "connection_test"
            && action["endpoints"].as_array().is_some_and(|endpoints| {
                endpoints.iter().any(|value| value == "test_policy_params")
            })
    }));
    assert!(qiniu_actions.iter().any(|action| {
        action["action_id"] == "test_saved_connection"
            && action["kind"] == "connection_test"
            && action["endpoints"].as_array().is_some_and(|endpoints| {
                endpoints
                    .iter()
                    .any(|value| value == "test_policy_connection")
            })
    }));
    assert_eq!(onedrive["authorization_provider"], "microsoft_graph");
    assert_eq!(
        onedrive["credential_management"]["title_key"],
        "onedrive_credential_title"
    );
    assert_eq!(
        onedrive["credential_management"]["status_presentations"]["authorized"]["label_key"],
        "onedrive_credential_status_authorized"
    );
    let onedrive_actions = onedrive["actions"].as_array().expect("onedrive actions");
    assert!(!onedrive_actions.iter().any(|action| {
        action["action_id"] == "test_draft_connection" && action["kind"] == "connection_test"
    }));
    let saved_onedrive_test = onedrive_actions
        .iter()
        .find(|action| {
            action["action_id"] == "test_saved_connection" && action["kind"] == "connection_test"
        })
        .expect("onedrive saved connection test action");
    assert_eq!(saved_onedrive_test["requires_saved_policy"], true);
    assert_eq!(saved_onedrive_test["requires_authorization"], true);
    assert_eq!(onedrive["upload_workflows"]["stream_upload"], true);
    assert_eq!(
        onedrive["upload_workflows"]["object_multipart_upload"],
        false
    );
    assert_eq!(
        onedrive["upload_workflows"]["provider_resumable_upload"],
        true
    );
    assert_eq!(
        onedrive["upload_workflows"]["frontend_direct_provider_resumable_upload"],
        true
    );
    assert!(onedrive["fields"].as_array().is_some_and(|fields| {
        fields.iter().any(|field| {
            field["name"] == "provider_download_strategy" && field["scope"] == "connector_config"
        })
    }));
    let onedrive_resumable =
        &onedrive["upload_workflows"]["provider_resumable_upload_capabilities"];
    assert_eq!(onedrive_resumable["provider"], "microsoft_graph");
    assert_eq!(
        onedrive_resumable["session_label"],
        "Microsoft Graph upload session"
    );
    assert_eq!(onedrive_resumable["min_fragment_size"], 320 * 1024);
    assert_eq!(onedrive_resumable["fragment_alignment"], 320 * 1024);
    assert_eq!(
        onedrive_resumable["default_fragment_size"],
        10 * 1024 * 1024
    );
    assert_eq!(onedrive_resumable["max_fragment_size"], 50 * 1024 * 1024);
    assert_eq!(onedrive_resumable["max_simple_upload_size"], 250_000_000);
    assert_eq!(onedrive_resumable["frontend_direct_upload"], true);
    assert_eq!(onedrive_resumable["implicit_completion"], true);
    assert_eq!(onedrive_resumable["abort_supported"], true);
    assert_eq!(onedrive_resumable["status_query_supported"], true);
    assert_eq!(
        onedrive["upload_workflows"]["simple_upload_capabilities"]["max_provider_single_request_size"],
        250_000_000
    );
    assert!(onedrive["upload_workflows"]["object_multipart_upload_capabilities"].is_null());

    let s3 = descriptor("asterdrive.storage.s3");
    assert!(
        s3["actions"]
            .as_array()
            .expect("s3 actions")
            .iter()
            .any(|action| action["action_id"] == "test_draft_connection"
                && action["kind"] == "connection_test")
    );
    assert_eq!(s3["upload_workflows"]["object_multipart_upload"], true);
    assert_eq!(
        s3["upload_workflows"]["object_multipart_upload_capabilities"]["min_part_size"],
        5 * 1024 * 1024
    );
    assert_eq!(
        s3["upload_workflows"]["object_multipart_upload_capabilities"]["presigned_part_upload"],
        true
    );
    assert_eq!(
        s3["upload_workflows"]["object_multipart_upload_capabilities"]["presigned_part_etag_required"],
        true
    );
    assert_eq!(
        s3["upload_workflows"]["object_multipart_upload_capabilities"]["explicit_complete_required"],
        true
    );
    assert!(
        s3["upload_workflows"]["provider_resumable_upload_capabilities"].is_null(),
        "S3 object multipart should not advertise provider-native resumable semantics"
    );
    assert_eq!(s3["capabilities"]["storage_native_thumbnail"], false);

    let alibaba_oss = descriptor("asterdrive.storage.alibaba_oss");
    assert_eq!(alibaba_oss["credential_mode"], "static_secret");
    assert_eq!(alibaba_oss["ui"]["label_key"], "driver_type_alibaba_oss");
    assert_eq!(
        alibaba_oss["ui"]["icon_src"],
        "/static/storage/aliyun-oss.svg"
    );
    assert_eq!(
        alibaba_oss["upload_workflows"]["object_multipart_upload"],
        true
    );
    assert_eq!(
        alibaba_oss["upload_workflows"]["object_multipart_upload_capabilities"]["presigned_part_upload"],
        true
    );
    assert_eq!(
        alibaba_oss["upload_workflows"]["object_multipart_upload_capabilities"]["presigned_part_etag_required"],
        true
    );
    for field in [
        "endpoint",
        "oss_server_side_endpoint",
        "oss_region",
        "bucket",
        "base_path",
        "oss_use_cname",
        "object_storage_upload_strategy",
        "object_storage_download_strategy",
        "aliyun_oss_access_key_id",
        "aliyun_oss_access_key_secret",
    ] {
        assert!(
            alibaba_oss["fields"]
                .as_array()
                .expect("OSS fields")
                .iter()
                .any(|candidate| candidate["name"] == field),
            "missing OSS descriptor field {field}"
        );
    }

    let azure_blob = descriptor("asterdrive.storage.azure_blob");
    assert_eq!(
        azure_blob["upload_workflows"]["object_multipart_upload"],
        true
    );
    assert_eq!(
        azure_blob["upload_workflows"]["object_multipart_upload_capabilities"]["presigned_part_etag_required"],
        false
    );

    let tencent_cos = descriptor("asterdrive.storage.tencent_cos");
    assert_eq!(tencent_cos["config_schema_version"], 1);
    assert_eq!(
        tencent_cos["capabilities"]["storage_native_thumbnail"],
        true
    );
    assert_eq!(
        tencent_cos["capabilities"]["storage_native_media_metadata"],
        true
    );
    let cos_promotion = tencent_cos["promotions"]
        .as_array()
        .expect("COS promotions")
        .iter()
        .find(|promotion| promotion["promotion_id"] == "promote_from_s3")
        .expect("COS should declare generic S3 promotion");
    assert_eq!(
        cos_promotion["source_connector_id"],
        "asterdrive.storage.s3"
    );
    assert_eq!(
        cos_promotion["requirements"][0]["matcher"],
        serde_json::json!({
            "kind": "url_host_suffix",
            "suffix": ".myqcloud.com"
        })
    );
    assert!(
        cos_promotion["config_mappings"]
            .as_array()
            .is_some_and(|mappings| mappings.iter().any(|mapping| {
                mapping["source_field"] == "bucket"
                    && mapping["target_field"] == "bucket"
                    && mapping["preserve_value"] == true
            }))
    );
    for descriptor in descriptors {
        let fields = descriptor["fields"].as_array().expect("connector fields");
        for core_behavior_field in [
            "storage_native_processing_enabled",
            "storage_native_thumbnail_enabled",
            "storage_native_thumbnail_extensions",
            "storage_native_media_metadata_enabled",
            "storage_native_media_metadata_extensions",
        ] {
            assert!(
                !fields
                    .iter()
                    .any(|field| field["name"] == core_behavior_field),
                "connector {} must expose native processing only as capability, not duplicate core behavior {core_behavior_field}",
                descriptor["connector_id"]
            );
        }
    }
    assert!(
        tencent_cos["actions"]
            .as_array()
            .expect("cos actions")
            .iter()
            .any(|action| {
                action["action_id"] == "configure_tencent_cos_cors"
                    && action["kind"] == "custom"
                    && action["fields"].is_null()
                    && action["endpoints"]
                        == serde_json::json!([
                            "execute_draft_storage_policy_action",
                            "execute_saved_storage_policy_action"
                        ])
                    && action["requires_saved_policy"] == false
                    && action["mutates_remote_state"] == true
                    && action["requires_confirmation"] == true
                    && action["output_fields"].as_array().is_some_and(|fields| {
                        fields.iter().any(|field| {
                            field["name"] == "request_id"
                                && field["label_key"] == "policy_cos_cors_output_request_id"
                                && field["value_kind"] == "text"
                        })
                    })
            })
    );
    let cos_endpoint = tencent_cos["fields"]
        .as_array()
        .expect("cos fields")
        .iter()
        .find(|field| field["name"] == "endpoint")
        .expect("cos endpoint field");
    assert_eq!(cos_endpoint["label_key"], "endpoint");
    assert_eq!(
        cos_endpoint["placeholder"],
        "https://<bucket-appid>.cos.<region>.myqcloud.com"
    );
    assert_eq!(cos_endpoint["help_key"], "cos_endpoint_hint");
    let s3_path_style = s3["fields"]
        .as_array()
        .expect("s3 fields")
        .iter()
        .find(|field| field["name"] == "s3_path_style")
        .expect("s3 path style field");
    assert_eq!(s3_path_style["label_key"], "s3_path_style");
    assert_eq!(s3_path_style["help_key"], "s3_path_style_desc");
    assert_eq!(s3_path_style["scope"], "connector_config");

    let local = descriptor("asterdrive.storage.local");
    assert_eq!(local["deployment_scope"], "instance_local");
    assert_eq!(local["supports_initial_setup"], true);
    assert_eq!(local["upload_workflows"]["object_multipart_upload"], false);
    assert_eq!(local["capabilities"]["remote_node_binding"], false);

    let sftp = descriptor("asterdrive.storage.sftp");
    assert_eq!(sftp["credential_mode"], "static_secret");
    assert_eq!(sftp["ui"]["label_key"], "driver_type_sftp");
    assert_eq!(sftp["upload_workflows"]["stream_upload"], true);
    assert_eq!(sftp["upload_workflows"]["object_multipart_upload"], false);
    assert_eq!(sftp["capabilities"]["efficient_range"], true);
    assert_eq!(sftp["capabilities"]["remote_node_binding"], false);

    let remote = descriptor("asterdrive.storage.remote");
    assert_eq!(remote["upload_workflows"]["object_multipart_upload"], true);
    assert_eq!(remote["capabilities"]["remote_node_binding"], true);
}

#[actix_web::test]
async fn test_policy_connector_promotion_preserves_namespace_and_rekeys_credential() {
    use aster_drive::db::repository::{policy_repo, storage_policy_connector_credential_repo};

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let policy_id = create_s3_policy_via_admin(
        &app,
        &token,
        "Promote COS",
        "https://media-1250000000.cos.ap-guangzhou.myqcloud.com",
    )
    .await;
    let credential_before =
        storage_policy_connector_credential_repo::find_by_policy(&db, policy_id)
            .await
            .unwrap()
            .expect("S3 credential should exist");

    let resp = promote_policy_to_tencent_cos(&app, &token, policy_id).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["connector_id"],
        "asterdrive.storage.tencent_cos"
    );
    assert_eq!(
        body["data"]["connector_config"]["values"]["bucket"],
        "media-1250000000"
    );
    assert_eq!(
        body["data"]["connector_config"]["values"]["base_path"],
        "tenant/files"
    );
    assert_eq!(
        body["data"]["connector_config"]["values"]["endpoint"],
        "https://media-1250000000.cos.ap-guangzhou.myqcloud.com"
    );
    assert!(
        body["data"]["connector_config"]["values"]
            .get("s3_path_style")
            .is_none()
    );

    let stored = policy_repo::find_by_id(&db, policy_id).await.unwrap();
    assert_eq!(stored.connector_id, "asterdrive.storage.tencent_cos");
    state
        .driver_registry()
        .get_driver(&stored)
        .expect("promoted runtime driver should use re-keyed COS credentials");
    let credential_after = storage_policy_connector_credential_repo::find_by_policy(&db, policy_id)
        .await
        .unwrap()
        .expect("promoted credential should exist");
    assert_eq!(
        credential_after.connector_id,
        "asterdrive.storage.tencent_cos"
    );
    assert_eq!(credential_after.schema_version, 1);
    assert_eq!(credential_after.revision, credential_before.revision + 1);
    assert_ne!(credential_after.ciphertext, credential_before.ciphertext);
}

#[actix_web::test]
async fn test_policy_connector_promotion_rejects_unknown_target_promotion_and_source() {
    use aster_drive::db::repository::policy_repo;

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let s3_policy_id = create_s3_policy_via_admin(
        &app,
        &token,
        "COS target validation",
        "https://media-1250000000.cos.ap-guangzhou.myqcloud.com",
    )
    .await;

    for payload in [
        serde_json::json!({
            "target_connector_id": "asterdrive.storage.tencent_cos",
            "promotion_id": "missing_promotion"
        }),
        serde_json::json!({
            "target_connector_id": "asterdrive.storage.local",
            "promotion_id": "promote_from_s3"
        }),
        serde_json::json!({
            "target_connector_id": "com.example.missing",
            "promotion_id": "promote_from_s3"
        }),
    ] {
        let req = test::TestRequest::post()
            .uri(&format!(
                "/api/v1/admin/policies/{s3_policy_id}/promote-connector"
            ))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(payload)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
        assert_eq!(
            policy_repo::find_by_id(&db, s3_policy_id)
                .await
                .unwrap()
                .connector_id,
            "asterdrive.storage.s3"
        );
    }

    let local_policy_id = create_local_policy_via_admin(&app, &token, "Local source").await;
    let resp = promote_policy_to_tencent_cos(&app, &token, local_policy_id).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyPromotionSourceUnsupported.as_str()
    );
    assert_eq!(
        policy_repo::find_by_id(&db, local_policy_id)
            .await
            .unwrap()
            .connector_id,
        "asterdrive.storage.local"
    );
}

#[actix_web::test]
async fn test_policy_connector_promotion_rejects_non_matching_source_config() {
    use aster_drive::db::repository::policy_repo;

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let policy_id =
        create_s3_policy_via_admin(&app, &token, "Generic S3", "https://s3.example.test").await;

    let resp = promote_policy_to_tencent_cos(&app, &token, policy_id).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyPromotionTargetUnsupported.as_str()
    );
    assert_eq!(
        policy_repo::find_by_id(&db, policy_id)
            .await
            .unwrap()
            .connector_id,
        "asterdrive.storage.s3"
    );
}

#[actix_web::test]
async fn test_policy_connector_promotion_rejects_active_upload_sessions() {
    use aster_drive::db::repository::policy_repo;

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let policy_id = create_s3_policy_via_admin(
        &app,
        &token,
        "COS with upload",
        "https://media-1250000000.cos.ap-guangzhou.myqcloud.com",
    )
    .await;
    let user = aster_drive::db::repository::user_repo::find_by_username(&db, "testuser")
        .await
        .unwrap()
        .expect("registered user should exist");
    create_policy_upload_session(
        &state,
        PolicyUploadSessionSpec {
            upload_id: "promotion-active-upload",
            policy_id,
            user_id: user.id,
            object_temp_key: None,
            status: None,
            expires_at: None,
        },
    )
    .await;

    let resp = promote_policy_to_tencent_cos(&app, &token, policy_id).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyUploadSessionsExist.as_str()
    );
    assert_eq!(
        policy_repo::find_by_id(&db, policy_id)
            .await
            .unwrap()
            .connector_id,
        "asterdrive.storage.s3"
    );
}

#[actix_web::test]
async fn test_policy_connector_promotion_ignores_expired_uploads_and_virtual_empty_blobs() {
    use aster_drive::db::repository::{file_repo, policy_repo};

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let policy_id = create_s3_policy_via_admin(
        &app,
        &token,
        "COS expired upload",
        "https://media-1250000000.cos.ap-guangzhou.myqcloud.com",
    )
    .await;
    let user = aster_drive::db::repository::user_repo::find_by_username(&db, "testuser")
        .await
        .unwrap()
        .expect("registered user should exist");
    create_policy_upload_session(
        &state,
        PolicyUploadSessionSpec {
            upload_id: "promotion-expired-upload",
            policy_id,
            user_id: user.id,
            object_temp_key: None,
            status: None,
            expires_at: Some(Utc::now() - Duration::minutes(1)),
        },
    )
    .await;
    file_repo::find_or_create_virtual_empty_blob(
        &db,
        aster_drive_model::entities::file_blob::Model::EMPTY_SHA256,
        policy_id,
    )
    .await
    .expect("virtual empty blob should be created");

    let resp = promote_policy_to_tencent_cos(&app, &token, policy_id).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        policy_repo::find_by_id(&db, policy_id)
            .await
            .unwrap()
            .connector_id,
        "asterdrive.storage.tencent_cos"
    );
}

#[actix_web::test]
async fn test_policy_connector_promotion_rejects_missing_credential_without_mutation() {
    use aster_drive::db::repository::{policy_repo, storage_policy_connector_credential_repo};

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let policy_id = create_s3_policy_via_admin(
        &app,
        &token,
        "COS missing credential",
        "https://media-1250000000.cos.ap-guangzhou.myqcloud.com",
    )
    .await;
    storage_policy_connector_credential_repo::delete_by_policy(&db, policy_id)
        .await
        .unwrap();

    let resp = promote_policy_to_tencent_cos(&app, &token, policy_id).await;
    assert_eq!(resp.status(), 500);
    assert_eq!(
        policy_repo::find_by_id(&db, policy_id)
            .await
            .unwrap()
            .connector_id,
        "asterdrive.storage.s3"
    );
    assert!(
        storage_policy_connector_credential_repo::find_by_policy(&db, policy_id)
            .await
            .unwrap()
            .is_none()
    );
}

#[actix_web::test]
async fn test_policy_connector_promotion_requires_admin() {
    use aster_drive::db::repository::policy_repo;

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);
    let policy_id = create_s3_policy_via_admin(
        &app,
        &admin_token,
        "COS admin only",
        "https://media-1250000000.cos.ap-guangzhou.myqcloud.com",
    )
    .await;
    admin_create_user!(
        app,
        admin_token,
        "promotion_user",
        "promotion-user@example.com",
        "password123"
    );
    let (user_token, _) = login_user!(app, "promotion_user", "password123");

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/admin/policies/{policy_id}/promote-connector"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&user_token)))
        .insert_header(common::csrf_header_for(&user_token))
        .set_json(serde_json::json!({
            "target_connector_id": "asterdrive.storage.tencent_cos",
            "promotion_id": "promote_from_s3"
        }))
        .to_request();
    assert_service_status!(app, req, 403);
    assert_eq!(
        policy_repo::find_by_id(&db, policy_id)
            .await
            .unwrap()
            .connector_id,
        "asterdrive.storage.s3"
    );
}

#[actix_web::test]
async fn test_connector_credential_promotion_cas_rejects_stale_revision() {
    use aster_drive::db::repository::storage_policy_connector_credential_repo;

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let policy_id = create_s3_policy_via_admin(
        &app,
        &token,
        "Credential CAS",
        "https://media-1250000000.cos.ap-guangzhou.myqcloud.com",
    )
    .await;
    let before = storage_policy_connector_credential_repo::find_by_policy(&db, policy_id)
        .await
        .unwrap()
        .expect("credential should exist");

    let updated = storage_policy_connector_credential_repo::promote_if_revision(
        &db,
        storage_policy_connector_credential_repo::ConnectorCredentialPromotion {
            policy_id,
            source_connector_id: &before.connector_id,
            source_schema_version: before.schema_version,
            expected_revision: before.revision + 1,
            target_connector_id: "asterdrive.storage.tencent_cos".to_string(),
            target_schema_version: 1,
            ciphertext: "replacement".to_string(),
        },
    )
    .await
    .unwrap();
    assert!(!updated);

    let after = storage_policy_connector_credential_repo::find_by_policy(&db, policy_id)
        .await
        .unwrap()
        .expect("credential should remain");
    assert_eq!(after.connector_id, before.connector_id);
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.ciphertext, before.ciphertext);
}

#[actix_web::test]
async fn test_storage_driver_catalog_contexts_are_backend_authoritative_in_single_profile() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let manage = list_storage_driver_descriptors_via_admin(&app, &token, None).await;
    let create = list_storage_driver_descriptors_via_admin(&app, &token, Some("create")).await;
    let setup = list_storage_driver_descriptors_via_admin(&app, &token, Some("setup")).await;

    assert_eq!(manage.len(), 9);
    assert_eq!(create.len(), 9);
    assert_eq!(setup.len(), 9);
    assert!(
        setup
            .iter()
            .any(|item| item["connector_id"] == "asterdrive.storage.local")
    );
    let onedrive = setup
        .iter()
        .find(|item| item["connector_id"] == "asterdrive.storage.onedrive")
        .expect("setup catalog should describe OneDrive");
    assert_eq!(onedrive["supports_initial_setup"], false);
}

#[actix_web::test]
async fn test_cluster_storage_driver_catalog_hides_local_only_from_new_policy_flows() {
    let mut state = common::setup().await;
    let mut config = (*state.config).clone();
    config.deployment.profile = aster_drive::config::DeploymentProfile::Cluster;
    state.config = std::sync::Arc::new(config);
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let manage = list_storage_driver_descriptors_via_admin(&app, &token, Some("manage")).await;
    let create = list_storage_driver_descriptors_via_admin(&app, &token, Some("create")).await;
    let setup = list_storage_driver_descriptors_via_admin(&app, &token, Some("setup")).await;

    assert_eq!(manage.len(), 9);
    assert!(
        manage
            .iter()
            .any(|item| item["connector_id"] == "asterdrive.storage.local")
    );
    assert_eq!(create.len(), 8);
    assert!(
        !create
            .iter()
            .any(|item| item["connector_id"] == "asterdrive.storage.local")
    );
    assert!(
        create
            .iter()
            .any(|item| item["connector_id"] == "asterdrive.storage.onedrive")
    );
    assert_eq!(setup.len(), 8);
    assert!(
        !setup
            .iter()
            .any(|item| item["connector_id"] == "asterdrive.storage.local")
    );
    let onedrive = setup
        .iter()
        .find(|item| item["connector_id"] == "asterdrive.storage.onedrive")
        .expect("cluster setup catalog should describe OneDrive");
    assert_eq!(onedrive["supports_initial_setup"], false);
}

#[actix_web::test]
async fn test_storage_driver_catalog_rejects_unknown_context() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies/storage-drivers?context=unknown")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_storage_driver_localizations_are_admin_only_and_cacheable() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let uri = "/api/v1/admin/policies/storage-drivers/localizations?context=create&locale=zh-CN";

    let req = test::TestRequest::get().uri(uri).to_request();
    assert_service_status!(app, req, 401);

    let (admin_token, _) = register_and_login!(app);
    let req = test::TestRequest::get()
        .uri(uri)
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("Cache-Control")
            .and_then(|value| value.to_str().ok()),
        Some("private, no-cache")
    );
    let etag = resp
        .headers()
        .get("ETag")
        .and_then(|value| value.to_str().ok())
        .expect("localization response ETag")
        .to_string();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["requested_locale"], "zh-CN");
    let resources = body["data"]["resources"]
        .as_array()
        .expect("connector localization resources");
    assert_eq!(resources.len(), 9);
    let local = resources
        .iter()
        .find(|resource| resource["connector_id"] == "asterdrive.storage.local")
        .expect("local connector localization");
    assert_eq!(local["namespace"], "asterdrive.storage.local");
    assert_eq!(local["resolved_locale"], "zh");
    assert_eq!(local["messages"]["driver_type_local"], "本机");
    let oss = resources
        .iter()
        .find(|resource| resource["connector_id"] == "asterdrive.storage.alibaba_oss")
        .expect("Alibaba OSS connector localization");
    assert_eq!(oss["resolved_locale"], "zh");
    assert_eq!(oss["messages"]["driver_type_alibaba_oss"], "阿里云 OSS");

    let req = test::TestRequest::get()
        .uri(uri)
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(("If-None-Match", format!("W/{etag}")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 304);
    assert_eq!(
        resp.headers()
            .get("ETag")
            .and_then(|value| value.to_str().ok()),
        Some(etag.as_str())
    );

    admin_create_user!(
        app,
        admin_token,
        "connector_reader",
        "connector-reader@example.com",
        "password123"
    );
    let (user_token, _) = login_user!(app, "connector_reader", "password123");
    let req = test::TestRequest::get()
        .uri(uri)
        .insert_header(("Cookie", common::access_cookie_header(&user_token)))
        .to_request();
    assert_service_status!(app, req, 403);

    let req = test::TestRequest::get()
        .uri("/api/v1/policies/storage-drivers/localizations?locale=zh-CN")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_storage_driver_localizations_reject_invalid_locale_and_context() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for uri in [
        "/api/v1/admin/policies/storage-drivers/localizations?locale=not_a_locale",
        "/api/v1/admin/policies/storage-drivers/localizations?context=unknown&locale=en",
    ] {
        let req = test::TestRequest::get()
            .uri(uri)
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400, "{uri} should reject invalid query");
    }
}

#[actix_web::test]
async fn test_cluster_rejects_direct_local_policy_creation_without_side_effects() {
    let mut state = common::setup().await;
    let db = state.writer_db().clone();
    let initial_policies = aster_drive::db::repository::policy_repo::find_all(&db)
        .await
        .expect("initial policy list");
    let mut config = (*state.config).clone();
    config.deployment.profile = aster_drive::config::DeploymentProfile::Cluster;
    state.config = std::sync::Arc::new(config);
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Unsafe local policy",
            "connection": local_connection_json("/tmp/unsafe-cluster-local"),
            "max_file_size": 0,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["msg"].as_str().is_some_and(|message| {
        message.contains("shared by every primary") && message.contains("instance_local")
    }));

    let final_policies = aster_drive::db::repository::policy_repo::find_all(&db)
        .await
        .expect("final policy list");
    assert_eq!(final_policies.len(), initial_policies.len());
    assert!(
        !final_policies
            .iter()
            .any(|policy| policy.name == "Unsafe local policy")
    );
}

#[actix_web::test]
async fn test_initial_storage_setup_rejects_connectors_requiring_post_setup_configuration() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let default_group = aster_drive::db::repository::policy_group_repo::find_default_group(&db)
        .await
        .expect("load default policy group")
        .expect("default policy group should exist");
    aster_drive::db::repository::policy_group_repo::delete_group_items_by_group(
        &db,
        default_group.id,
    )
    .await
    .expect("empty default policy group");
    assert_eq!(
        aster_drive::services::system_setup::state(&db)
            .await
            .expect("inspect setup state"),
        aster_drive::services::system_setup::SystemSetupState::NeedsStorage
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Premature OneDrive",
            "connection": {
                "connector_config": {
                    "format_version": 1,
                    "connector_id": "asterdrive.storage.onedrive",
                    "schema_version": 1,
                    "values": {
                        "base_path": "files",
                        "provider_resumable_upload_strategy": "server_relay",
                        "provider_download_strategy": "server_relay",
                        "provider_download_filename_mode": "provider_native"
                    }
                },
                "behavior": {},
                "credential": {
                    "mode": "authorization_application",
                    "values": {
                        "client_id": "setup-client",
                        "client_secret": "setup-secret"
                    }
                }
            },
            "max_file_size": 0,
            "is_default": true
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert!(
        body["msg"]
            .as_str()
            .is_some_and(|message| message.contains("post-setup configuration"))
    );

    assert_eq!(
        aster_drive::services::system_setup::state(&db)
            .await
            .expect("setup state should remain inspectable"),
        aster_drive::services::system_setup::SystemSetupState::NeedsStorage
    );
    assert!(
        !aster_drive::db::repository::policy_repo::find_all(&db)
            .await
            .expect("list policies after rejected setup connector")
            .iter()
            .any(|policy| policy.name == "Premature OneDrive")
    );
}

async fn create_personal_folder<S, B>(
    app: &S,
    token: &str,
    name: &str,
    parent_id: Option<i64>,
) -> i64
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = actix_web::Error,
        >,
    B: actix_web::body::MessageBody + 'static,
{
    let mut payload = serde_json::json!({ "name": name });
    if let Some(parent_id) = parent_id {
        payload["parent_id"] = serde_json::json!(parent_id);
    }
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(token)))
        .insert_header(common::csrf_header_for(token))
        .set_json(payload)
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    body["data"]["id"].as_i64().unwrap()
}

async fn create_nested_folders<S, B>(
    depth: usize,
    app: &S,
    token: &str,
    start_parent_id: i64,
) -> i64
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = actix_web::Error,
        >,
    B: actix_web::body::MessageBody + 'static,
{
    let mut current_parent_id = start_parent_id;
    for depth in 1..=depth {
        current_parent_id = create_personal_folder(
            app,
            token,
            &format!("deep-policy-child-{depth}"),
            Some(current_parent_id),
        )
        .await;
    }
    current_parent_id
}

async fn admin_set_folder_policy<S, B>(
    app: &S,
    token: &str,
    folder_id: i64,
    policy_id: Option<i64>,
) -> actix_web::dev::ServiceResponse<B>
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = actix_web::Error,
        >,
    B: actix_web::body::MessageBody + 'static,
{
    test::call_service(
        app,
        test::TestRequest::put()
            .uri(&format!("/api/v1/admin/folders/{folder_id}/policy"))
            .insert_header(("Cookie", common::access_cookie_header(token)))
            .insert_header(common::csrf_header_for(token))
            .set_json(serde_json::json!({ "policy_id": policy_id }))
            .to_request(),
    )
    .await
}

async fn uploaded_file_policy_id(
    state: &aster_drive::runtime::PrimaryAppState,
    file_id: i64,
) -> i64 {
    use aster_drive::db::repository::file_repo;

    let file = file_repo::find_by_id(state.writer_db(), file_id)
        .await
        .unwrap();
    let blob = file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
        .await
        .unwrap();
    blob.policy_id
}

struct PolicyUploadSessionSpec<'a> {
    upload_id: &'a str,
    policy_id: i64,
    user_id: i64,
    object_temp_key: Option<&'a str>,
    status: Option<aster_drive_model::types::UploadSessionStatus>,
    expires_at: Option<chrono::DateTime<Utc>>,
}

async fn create_policy_upload_session(
    state: &aster_drive::runtime::PrimaryAppState,
    spec: PolicyUploadSessionSpec<'_>,
) {
    use aster_drive::db::repository::upload_session_repo;
    use sea_orm::Set;

    let now = Utc::now();
    upload_session_repo::create(
        state.writer_db(),
        aster_drive_model::entities::upload_session::ActiveModel {
            id: Set(spec.upload_id.to_string()),
            user_id: Set(spec.user_id),
            team_id: Set(None),
            frontend_client_id: Set(None),
            filename: Set("pending-policy-upload.bin".to_string()),
            total_size: Set(10),
            chunk_size: Set(5),
            total_chunks: Set(2),
            received_count: Set(1),
            folder_id: Set(None),
            policy_id: Set(spec.policy_id),
            status: Set(spec
                .status
                .unwrap_or(aster_drive_model::types::UploadSessionStatus::Uploading)),
            session_kind: Set(if spec.object_temp_key.is_some() {
                aster_drive_model::types::UploadSessionKind::ProviderPresignedSingle
            } else {
                aster_drive_model::types::UploadSessionKind::OffsetStaging
            }),
            object_temp_key: Set(spec.object_temp_key.map(str::to_string)),
            object_multipart_id: Set(None),
            provider_session_ciphertext: Set(None),
            file_id: Set(None),
            created_at: Set(now),
            expires_at: Set(spec.expires_at.unwrap_or(now + Duration::hours(1))),
            updated_at: Set(now),
        },
    )
    .await
    .unwrap();
}

#[actix_web::test]
async fn test_user_default_policy_switch_updates_snapshot_immediately() {
    use aster_drive::services::{files::file, storage_policy::policy, user::account};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "policysnapsw",
        "policy-snapshot-switch@example.com",
        "password123",
    )
    .await
    .unwrap();

    let initial_policy = file::resolve_policy_for_size(&state, user.id, None, 0)
        .await
        .unwrap();

    let alternate_base_path = format!("/tmp/asterdrive-policy-switch-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&alternate_base_path).unwrap();
    let alternate_policy = policy::create(
        &state,
        policy::CreateStoragePolicyInput {
            name: "Alternate Local".to_string(),
            connection: common::local_connection(alternate_base_path.clone()),
            max_file_size: 0,
            chunk_size: None,
            is_default: false,
            allowed_types: None,
        },
    )
    .await
    .unwrap();

    assert_ne!(alternate_policy.id, initial_policy.id);

    let alternate_group = policy::create_group(
        &state,
        policy::CreateStoragePolicyGroupInput {
            name: "Alternate Group".to_string(),
            description: Some("Snapshot switch target".to_string()),
            is_enabled: true,
            is_default: false,
            items: vec![policy::StoragePolicyGroupItemInput {
                policy_id: alternate_policy.id,
                priority: 1,
                min_file_size: 0,
                max_file_size: 0,
            }],
        },
    )
    .await
    .unwrap();

    account::update(
        &state,
        account::UpdateUserInput {
            id: user.id,
            email_verified: None,
            role: None,
            status: None,
            must_change_password: None,
            storage_quota: None,
            policy_group_id: Some(alternate_group.id),
        },
    )
    .await
    .unwrap();

    assert_eq!(
        state.policy_snapshot.resolve_default_policy_id(user.id),
        Some(alternate_policy.id)
    );

    let resolved_after_switch = file::resolve_policy_for_size(&state, user.id, None, 0)
        .await
        .unwrap();
    assert_eq!(resolved_after_switch.id, alternate_policy.id);
}

#[actix_web::test]
async fn test_seed_policy_groups_backfills_missing_users_to_default_group() {
    use aster_drive::db::repository::{policy_group_repo, user_repo};
    use aster_drive::services::storage_policy::policy;
    use sea_orm::{ActiveModelTrait, Set};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "policybackfill",
        "policy-backfill@example.com",
        "password123",
    )
    .await
    .unwrap();
    let default_group = policy_group_repo::find_default_group(state.writer_db())
        .await
        .unwrap()
        .expect("default group should exist");
    let initial_group_count = policy_group_repo::find_all_groups(state.writer_db())
        .await
        .unwrap()
        .len();

    let mut user_active: aster_drive_model::entities::user::ActiveModel = user.into();
    user_active.policy_group_id = Set(None);
    user_active.update(state.writer_db()).await.unwrap();

    policy::ensure_policy_groups_seeded(state.writer_db())
        .await
        .unwrap();
    policy::ensure_policy_groups_seeded(state.writer_db())
        .await
        .unwrap();

    let updated = user_repo::find_by_email(state.writer_db(), "policy-backfill@example.com")
        .await
        .unwrap()
        .expect("user should exist");
    assert_eq!(updated.policy_group_id, Some(default_group.id));
    assert_eq!(
        policy_group_repo::find_all_groups(state.writer_db())
            .await
            .unwrap()
            .len(),
        initial_group_count,
        "repeated reconciliation must not create duplicate policy groups"
    );
}

#[actix_web::test]
async fn test_creating_first_default_policy_backfills_bootstrap_admin() {
    use aster_drive::db::repository::user_repo;
    use aster_drive::services::storage_policy::policy;
    use sea_orm::{ActiveModelTrait, ConnectionTrait, Set};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "storagebootstrap",
        "storage-bootstrap@example.com",
        "password123",
    )
    .await
    .unwrap();
    let mut active: aster_drive_model::entities::user::ActiveModel = user.into();
    active.policy_group_id = Set(None);
    active.update(state.writer_db()).await.unwrap();
    state
        .writer_db()
        .execute_unprepared("UPDATE storage_policies SET is_default = FALSE;")
        .await
        .unwrap();
    state
        .writer_db()
        .execute_unprepared("UPDATE storage_policy_groups SET is_default = FALSE;")
        .await
        .unwrap();
    state
        .driver_registry
        .reload_policy_snapshot(&state.policy_snapshot, state.writer_db())
        .await
        .unwrap();
    assert_eq!(
        aster_drive::services::system_setup::state(state.writer_db())
            .await
            .unwrap(),
        aster_drive::services::system_setup::SystemSetupState::NeedsStorage
    );

    let base_path = format!("/tmp/asterdrive-storage-bootstrap-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&base_path).unwrap();
    let created = policy::create(
        &state,
        policy::CreateStoragePolicyInput {
            name: "First Setup Default".to_string(),
            connection: common::local_connection(base_path),
            max_file_size: 0,
            chunk_size: None,
            is_default: true,
            allowed_types: None,
        },
    )
    .await
    .unwrap();

    let updated = user_repo::find_by_email(state.writer_db(), "storage-bootstrap@example.com")
        .await
        .unwrap()
        .expect("bootstrap admin should still exist");
    let assigned_group_id = updated
        .policy_group_id
        .expect("first default policy should backfill the bootstrap admin");
    assert_eq!(
        state.policy_snapshot.resolve_default_policy_id(updated.id),
        Some(created.id)
    );
    assert_eq!(
        state
            .policy_snapshot
            .system_default_policy_group()
            .expect("default policy group should exist")
            .id,
        assigned_group_id
    );
    assert_eq!(
        aster_drive::services::system_setup::state(state.writer_db())
            .await
            .unwrap(),
        aster_drive::services::system_setup::SystemSetupState::Ready
    );
}

#[actix_web::test]
async fn test_promoting_policy_backfills_unassigned_users() {
    use aster_drive::db::repository::user_repo;
    use aster_drive::services::storage_policy::policy;
    use sea_orm::{ActiveModelTrait, Set};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "promotebackfill",
        "promote-backfill@example.com",
        "password123",
    )
    .await
    .unwrap();
    let mut active: aster_drive_model::entities::user::ActiveModel = user.into();
    active.policy_group_id = Set(None);
    active.update(state.writer_db()).await.unwrap();

    let base_path = format!("/tmp/asterdrive-promote-backfill-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&base_path).unwrap();
    let policy = policy::create(
        &state,
        policy::CreateStoragePolicyInput {
            name: "Promoted Default".to_string(),
            connection: common::local_connection(base_path),
            max_file_size: 0,
            chunk_size: None,
            is_default: false,
            allowed_types: None,
        },
    )
    .await
    .unwrap();

    policy::update(
        &state,
        policy.id,
        policy::UpdateStoragePolicyInput {
            is_default: Some(true),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let updated = user_repo::find_by_email(state.writer_db(), "promote-backfill@example.com")
        .await
        .unwrap()
        .expect("unassigned user should still exist");
    assert!(updated.policy_group_id.is_some());
    assert_eq!(
        state.policy_snapshot.resolve_default_policy_id(updated.id),
        Some(policy.id)
    );
}

#[actix_web::test]
async fn test_policy_crud() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 列出策略（应有 1 个默认）
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["total"], 1);

    // 创建新策略
    let mut create_connection = s3_connection_json(
        "http://localhost:9000",
        "test-bucket",
        "",
        "minioadmin",
        "minioadmin",
    );
    create_connection["connector_config"]["values"]["object_storage_upload_strategy"] =
        serde_json::json!("presigned");
    create_connection["connector_config"]["values"]["s3_path_style"] = serde_json::json!(false);
    create_connection["connector_config"]["values"]["s3_region"] = serde_json::json!(" us-east-1 ");
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Test S3",
            "connection": create_connection,
            "max_file_size": 104857600,
            "chunk_size": 8388608
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "Test S3");
    assert_eq!(body["data"]["chunk_size"], 8_388_608);
    assert_eq!(
        body["data"]["connector_config"]["values"]["object_storage_upload_strategy"],
        "presigned"
    );
    assert_eq!(
        body["data"]["connector_config"]["values"]["s3_path_style"],
        false
    );
    assert_eq!(
        body["data"]["connector_config"]["values"]["s3_region"],
        "us-east-1"
    );
    let policy_id = body["data"]["id"].as_i64().unwrap();

    // 获取单个
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["connector_config"]["values"]["s3_path_style"],
        false
    );
    assert_eq!(
        body["data"]["connector_config"]["values"]["s3_region"],
        "us-east-1"
    );

    // 更新策略
    let mut update_connection = s3_connection_json(
        "http://localhost:9000",
        "test-bucket",
        "",
        "minioadmin",
        "minioadmin",
    );
    update_connection["connector_config"]["values"]["object_storage_upload_strategy"] =
        serde_json::json!("presigned");
    update_connection["connector_config"]["values"]["s3_path_style"] = serde_json::json!(false);
    update_connection["connector_config"]["values"]["s3_region"] =
        serde_json::json!("eu-central-1");
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Renamed S3",
            "connector_config": update_connection["connector_config"].clone()
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "Renamed S3");
    assert_eq!(
        body["data"]["connector_config"]["values"]["object_storage_upload_strategy"],
        "presigned"
    );
    assert_eq!(
        body["data"]["connector_config"]["values"]["s3_path_style"],
        false
    );
    assert_eq!(
        body["data"]["connector_config"]["values"]["s3_region"],
        "eu-central-1"
    );

    // Editing a non-secret credential field must retain the saved secret.
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "credential": {
                "mode": "static",
                "values": { "s3_access_key_id": "updated-access-key" }
            }
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 删除策略
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 只剩默认策略
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["total"], 1);
}

#[actix_web::test]
async fn test_policy_delete_rejects_upload_sessions_unless_forced() {
    use aster_drive::db::repository::{policy_repo, upload_session_repo};

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let base_path = format!(
        "/tmp/asterdrive-policy-upload-session-{}",
        uuid::Uuid::new_v4()
    );
    std::fs::create_dir_all(&base_path).unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Session Guard Policy",
            "connection": local_connection_json(&base_path),
            "chunk_size": 5_242_880,
            "max_file_size": 0
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["id"].as_i64().unwrap();

    let user = aster_drive::db::repository::user_repo::find_by_username(&db, "testuser")
        .await
        .unwrap()
        .expect("registered user should exist");
    let upload_id = uuid::Uuid::new_v4().to_string();
    let temp_dir = std::path::PathBuf::from(aster_forge_utils::paths::upload_temp_dir(
        &state.config.server.upload_temp_dir,
        &upload_id,
    ));
    tokio::fs::create_dir_all(&temp_dir).await.unwrap();
    tokio::fs::write(temp_dir.join("chunk-0"), b"partial")
        .await
        .unwrap();

    create_policy_upload_session(
        &state,
        PolicyUploadSessionSpec {
            upload_id: &upload_id,
            policy_id,
            user_id: user.id,
            object_temp_key: None,
            status: None,
            expires_at: None,
        },
    )
    .await;

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["msg"],
        "cannot delete policy: 1 upload session(s) still reference it"
    );

    assert!(
        policy_repo::find_by_id(&db, policy_id).await.is_ok(),
        "policy should remain after guarded delete"
    );
    assert!(
        upload_session_repo::find_by_id(&db, &upload_id)
            .await
            .is_ok(),
        "upload session should remain after guarded delete"
    );
    assert!(
        temp_dir.exists(),
        "local upload temp directory should remain after guarded delete"
    );

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/policies/{policy_id}?force=true"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    assert!(
        policy_repo::find_by_id(&db, policy_id).await.is_err(),
        "policy should be deleted by forced delete"
    );
    assert!(
        upload_session_repo::find_by_id(&db, &upload_id)
            .await
            .is_err(),
        "forced delete should remove upload sessions"
    );
    assert!(
        !temp_dir.exists(),
        "forced delete should remove local upload temp directory"
    );
}

#[actix_web::test]
async fn test_policy_force_delete_schedules_late_temp_object_cleanup() {
    use aster_drive::db::repository::{background_task_repo, policy_repo, upload_session_repo};
    use aster_drive::services::task;
    use aster_drive_model::entities::background_task;
    use aster_drive_model::types::{BackgroundTaskKind, BackgroundTaskStatus};
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let base_path = format!(
        "/tmp/asterdrive-policy-late-temp-cleanup-{}",
        uuid::Uuid::new_v4()
    );
    std::fs::create_dir_all(&base_path).unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Late Temp Cleanup Policy",
            "connection": local_connection_json(&base_path),
            "chunk_size": 5_242_880,
            "max_file_size": 0
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["id"].as_i64().unwrap();

    let user = aster_drive::db::repository::user_repo::find_by_username(&db, "testuser")
        .await
        .unwrap()
        .expect("registered user should exist");
    let upload_id = uuid::Uuid::new_v4().to_string();
    let temp_key = format!("files/late-orphan-{}.bin", uuid::Uuid::new_v4());
    create_policy_upload_session(
        &state,
        PolicyUploadSessionSpec {
            upload_id: &upload_id,
            policy_id,
            user_id: user.id,
            object_temp_key: Some(&temp_key),
            status: None,
            expires_at: None,
        },
    )
    .await;

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/policies/{policy_id}?force=true"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    assert!(
        policy_repo::find_by_id(&db, policy_id).await.is_err(),
        "policy should be deleted by forced delete"
    );
    assert!(
        upload_session_repo::find_by_id(&db, &upload_id)
            .await
            .is_err(),
        "forced delete should remove upload session"
    );

    let cleanup_task = background_task::Entity::find()
        .filter(background_task::Column::Kind.eq(BackgroundTaskKind::StoragePolicyTempCleanup))
        .one(&db)
        .await
        .unwrap()
        .expect("force delete should schedule delayed temp cleanup");
    assert_eq!(cleanup_task.status, BackgroundTaskStatus::Pending);
    let payload: Value = serde_json::from_str(cleanup_task.payload_json.as_ref()).unwrap();
    assert_eq!(payload["policy"]["id"], policy_id);
    assert_eq!(payload["temp_keys"][0], temp_key);

    let object_path = std::path::Path::new(&base_path).join(&temp_key);
    tokio::fs::create_dir_all(object_path.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&object_path, b"late presigned write")
        .await
        .unwrap();
    assert!(
        object_path.exists(),
        "test should create the late orphan object after policy deletion"
    );

    let mut active: background_task::ActiveModel = cleanup_task.clone().into();
    active.next_run_at = Set(Utc::now() - Duration::seconds(1));
    active.update(&db).await.unwrap();

    let stats = task::dispatch_due(&state).await.unwrap();
    assert_eq!(stats.claimed, 1);
    assert_eq!(stats.succeeded, 1);
    assert!(
        !object_path.exists(),
        "delayed cleanup should delete late temp object using policy snapshot"
    );

    let stored_task = background_task_repo::find_by_id(&db, cleanup_task.id)
        .await
        .unwrap();
    assert_eq!(stored_task.status, BackgroundTaskStatus::Succeeded);
    let result: Value =
        serde_json::from_str(stored_task.result_json.as_ref().unwrap().as_ref()).unwrap();
    assert_eq!(result["deleted_objects"], 1);
    assert_eq!(result["failed_objects"], 0);
}

#[actix_web::test]
async fn test_policy_force_delete_removes_corrupted_session_without_temp_object() {
    use aster_drive::db::repository::{policy_repo, upload_session_repo};
    use aster_drive_model::entities::upload_session;
    use aster_drive_model::types::UploadSessionKind;
    use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let base_path = format!(
        "/tmp/asterdrive-policy-corrupted-upload-{}",
        uuid::Uuid::new_v4()
    );
    std::fs::create_dir_all(&base_path).unwrap();
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Corrupted Upload Cleanup Policy",
            "connection": local_connection_json(&base_path),
            "chunk_size": 5_242_880,
            "max_file_size": 0
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["id"].as_i64().unwrap();

    let user = aster_drive::db::repository::user_repo::find_by_username(&db, "testuser")
        .await
        .unwrap()
        .expect("registered user should exist");
    let upload_id = uuid::Uuid::new_v4().to_string();
    create_policy_upload_session(
        &state,
        PolicyUploadSessionSpec {
            upload_id: &upload_id,
            policy_id,
            user_id: user.id,
            object_temp_key: None,
            status: None,
            expires_at: None,
        },
    )
    .await;
    let mut session: upload_session::ActiveModel = upload_session_repo::find_by_id(&db, &upload_id)
        .await
        .unwrap()
        .into_active_model();
    session.session_kind = Set(UploadSessionKind::ProviderPresignedSingle);
    session.update(&db).await.unwrap();

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/policies/{policy_id}?force=true"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert!(policy_repo::find_by_id(&db, policy_id).await.is_err());
    assert!(
        upload_session_repo::find_by_id(&db, &upload_id)
            .await
            .is_err()
    );
}

#[actix_web::test]
async fn test_policy_force_delete_still_rejects_blob_references() {
    use aster_drive::db::repository::{file_repo, policy_group_repo, policy_repo};
    use aster_drive::services::{files::file, storage_policy::policy, user::account};

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let user = aster_drive::db::repository::user_repo::find_by_username(&db, "testuser")
        .await
        .unwrap()
        .expect("registered user should exist");

    let base_path = format!("/tmp/asterdrive-policy-force-blob-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&base_path).unwrap();
    let policy = policy::create(
        &state,
        policy::CreateStoragePolicyInput {
            name: "Blob Guard Policy".to_string(),
            connection: common::local_connection(base_path),
            max_file_size: 0,
            chunk_size: None,
            is_default: false,
            allowed_types: None,
        },
    )
    .await
    .unwrap();

    let group = policy::create_group(
        &state,
        policy::CreateStoragePolicyGroupInput {
            name: "Blob Guard Group".to_string(),
            description: None,
            is_enabled: true,
            is_default: false,
            items: vec![policy::StoragePolicyGroupItemInput {
                policy_id: policy.id,
                priority: 1,
                min_file_size: 0,
                max_file_size: 0,
            }],
        },
    )
    .await
    .unwrap();

    account::update(
        &state,
        account::UpdateUserInput {
            id: user.id,
            email_verified: None,
            role: None,
            status: None,
            must_change_password: None,
            storage_quota: None,
            policy_group_id: Some(group.id),
        },
    )
    .await
    .unwrap();

    let temp_path = aster_forge_utils::paths::temp_file_path(
        &state.config.server.temp_dir,
        &uuid::Uuid::new_v4().to_string(),
    );
    tokio::fs::create_dir_all(&state.config.server.temp_dir)
        .await
        .unwrap();
    tokio::fs::write(&temp_path, b"blob reference")
        .await
        .unwrap();
    let file = file::store_from_temp(
        &state,
        user.id,
        file::StoreFromTempRequest::new(
            None,
            "blob-reference.txt",
            &temp_path,
            b"blob reference".len() as i64,
        ),
    )
    .await
    .unwrap();
    let blob = file_repo::find_blob_by_id(&db, file.blob_id).await.unwrap();
    assert_eq!(blob.policy_id, policy.id);

    let default_group = policy_group_repo::find_default_group(&db)
        .await
        .unwrap()
        .expect("default policy group should exist");
    account::update(
        &state,
        account::UpdateUserInput {
            id: user.id,
            email_verified: None,
            role: None,
            status: None,
            must_change_password: None,
            storage_quota: None,
            policy_group_id: Some(default_group.id),
        },
    )
    .await
    .unwrap();

    policy::delete_group(&state, group.id).await.unwrap();

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/policies/{}?force=true", policy.id))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["msg"],
        "cannot delete policy: 1 blob(s) still reference it"
    );
    assert!(
        policy_repo::find_by_id(&db, policy.id).await.is_ok(),
        "force must not delete a policy referenced by blobs"
    );
}

#[actix_web::test]
async fn test_policy_rejects_storage_native_thumbnail_for_unsupported_driver() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let mut connection = local_connection_json("/tmp/test-native-thumbnail-local");
    connection["behavior"] = serde_json::json!({
        "storage_native_thumbnail_enabled": true,
        "storage_native_thumbnail_extensions": ["png", ".jpg"]
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Native Thumbnail Local",
            "connection": connection,
            "max_file_size": 0,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyNativeThumbnailUnsupported.as_str()
    );
    assert!(
        body["msg"]
            .as_str()
            .unwrap_or_default()
            .contains("does not expose storage-native thumbnail processing")
    );

    let policy_id = create_local_policy_via_admin(&app, &token, "Native Thumbnail Patch").await;
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "behavior": {
                "storage_native_thumbnail_enabled": true,
                "storage_native_thumbnail_extensions": ["jpg"]
            }
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyNativeThumbnailUnsupported.as_str()
    );

    let mut draft_connection = local_connection_json("/tmp/test-native-thumbnail-draft");
    draft_connection["behavior"] = serde_json::json!({
        "storage_native_thumbnail_enabled": true,
        "storage_native_thumbnail_extensions": ["jpg"]
    });
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/test")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "connection": draft_connection }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyNativeThumbnailUnsupported.as_str()
    );
}

#[actix_web::test]
async fn test_policy_rejects_storage_native_media_metadata_for_unsupported_driver() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let mut connection = local_connection_json("/tmp/test-native-metadata-local");
    connection["behavior"] = serde_json::json!({
        "storage_native_media_metadata_enabled": true,
        "storage_native_media_metadata_extensions": ["mp4"]
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Native Metadata Local",
            "connection": connection,
            "max_file_size": 0,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyNativeMediaMetadataUnsupported.as_str()
    );
    assert!(
        body["msg"]
            .as_str()
            .unwrap_or_default()
            .contains("does not expose storage-native media metadata processing")
    );

    let policy_id = create_local_policy_via_admin(&app, &token, "Native Metadata Patch").await;
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "behavior": {
                "storage_native_media_metadata_enabled": true,
                "storage_native_media_metadata_extensions": ["mp4"]
            }
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyNativeMediaMetadataUnsupported.as_str()
    );

    let mut draft_connection = local_connection_json("/tmp/test-native-metadata-draft");
    draft_connection["behavior"] = serde_json::json!({
        "storage_native_media_metadata_enabled": true,
        "storage_native_media_metadata_extensions": ["mp4"]
    });
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/test")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "connection": draft_connection }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyNativeMediaMetadataUnsupported.as_str()
    );
}

#[actix_web::test]
async fn test_tencent_cos_uses_only_core_owned_storage_native_behavior_state() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let mut connection = connection_json(common::tencent_cos_connection(
        "https://bucket-1250000000.cos.ap-guangzhou.myqcloud.com",
        "bucket-1250000000",
        "",
        "AKIDEXAMPLE",
        "SECRETEXAMPLE",
    ));
    connection["behavior"] = serde_json::json!({
        "storage_native_thumbnail_enabled": true,
        "storage_native_thumbnail_extensions": [],
        "storage_native_media_metadata_enabled": true,
        "storage_native_media_metadata_extensions": []
    });

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "COS Unified Native Behavior",
            "connection": connection.clone(),
            "max_file_size": 0,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["id"].as_i64().expect("created policy id");
    assert_eq!(
        body["data"]["behavior"]["storage_native_thumbnail_enabled"],
        true
    );
    assert_eq!(
        body["data"]["behavior"]["storage_native_media_metadata_enabled"],
        true
    );
    assert!(body["data"]["behavior"]["storage_native_thumbnail_extensions"].is_null());
    assert!(body["data"]["behavior"]["storage_native_media_metadata_extensions"].is_null());
    assert_eq!(body["data"]["connector_config"]["schema_version"], 1);
    assert!(
        body["data"]["connector_config"]["values"]
            .get("storage_native_processing_enabled")
            .is_none()
    );
    assert!(
        body["data"]["connector_config"]["values"]
            .get("storage_native_media_metadata_enabled")
            .is_none()
    );

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["behavior"]["storage_native_thumbnail_enabled"],
        true
    );
    assert_eq!(
        body["data"]["behavior"]["storage_native_media_metadata_enabled"],
        true
    );

    for (behavior, extension_field, expected_extensions) in [
        (
            serde_json::json!({
            "storage_native_thumbnail_enabled": false,
            "storage_native_thumbnail_extensions": ["jpg"]
            }),
            "storage_native_thumbnail_extensions",
            serde_json::json!(["jpg"]),
        ),
        (
            serde_json::json!({
            "storage_native_media_metadata_enabled": false,
            "storage_native_media_metadata_extensions": ["mp4"]
            }),
            "storage_native_media_metadata_extensions",
            serde_json::json!(["mp4"]),
        ),
    ] {
        let req = test::TestRequest::patch()
            .uri(&format!("/api/v1/admin/policies/{policy_id}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({ "behavior": behavior }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(
            body["data"]["behavior"][extension_field], expected_extensions,
            "disabled behavior must retain its matching configuration"
        );
    }

    let mut dormant_local = local_connection_json(format!(
        "/tmp/asterdrive-dormant-native-config-{}",
        uuid::Uuid::new_v4()
    ));
    dormant_local["behavior"] = serde_json::json!({
        "storage_native_thumbnail_enabled": false,
        "storage_native_thumbnail_extensions": ["jpg"],
        "storage_native_media_metadata_enabled": false,
        "storage_native_media_metadata_extensions": ["mp4"]
    });
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Dormant Native Configuration",
            "connection": dormant_local,
            "max_file_size": 0,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["behavior"]["storage_native_thumbnail_enabled"],
        false
    );
    assert_eq!(
        body["data"]["behavior"]["storage_native_thumbnail_extensions"],
        serde_json::json!(["jpg"])
    );
    assert_eq!(
        body["data"]["behavior"]["storage_native_media_metadata_extensions"],
        serde_json::json!(["mp4"])
    );
    assert_eq!(
        body["data"]["behavior"]["storage_native_media_metadata_enabled"],
        false
    );

    let mut legacy_connector_state = connection.clone();
    legacy_connector_state["connector_config"]["values"]["storage_native_processing_enabled"] =
        serde_json::json!(true);
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "COS Legacy Duplicate State",
            "connection": legacy_connector_state,
            "max_file_size": 0,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::BadRequest.as_str());
    assert!(
        body["msg"]
            .as_str()
            .unwrap_or_default()
            .contains("storage_native_processing_enabled"),
        "legacy connector state should be named in the rejection: {body}"
    );

    let mut legacy_behavior_state = connection;
    legacy_behavior_state["behavior"]["thumbnail_processor"] = serde_json::json!("storage_native");
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "COS Legacy Behavior Key",
            "connection": legacy_behavior_state,
            "max_file_size": 0,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::BadRequest.as_str());
    assert!(
        body["msg"]
            .as_str()
            .unwrap_or_default()
            .contains("thumbnail_processor"),
        "the unpublished key rename must not leave a compatibility path: {body}"
    );
}

#[actix_web::test]
async fn test_user_policy_assignment() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 获取默认策略 ID
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policy-groups")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Dedicated User Group",
            "description": "Single binding target",
            "is_enabled": true,
            "is_default": false,
            "items": [
                {
                    "policy_id": policy_id,
                    "priority": 1,
                    "min_file_size": 0,
                    "max_file_size": 0
                }
            ]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let group_id = body["data"]["id"].as_i64().unwrap();

    // 获取用户 ID
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/users")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let user_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/users/{user_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "policy_group_id": group_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["policy_group_id"], group_id);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/users/{user_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["policy_group_id"], group_id);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/users/{user_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "policy_group_id": serde_json::Value::Null
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

// ── 系统策略 default 唯一性 ─────────────────────────────────

#[actix_web::test]
async fn test_system_policy_default_uniqueness() {
    use aster_drive::db::repository::policy_group_repo;

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 创建第二个策略并设为 default
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "New Default",
            "connection": local_connection_json("/tmp/test-new-default"),
            "max_file_size": 0,
            "is_default": true
        }))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let new_default_id = body["data"]["id"].as_i64().unwrap();

    // 列出所有策略，应只有一个 is_default=true
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let policies = body["data"]["items"].as_array().unwrap();
    let default_count = policies.iter().filter(|p| p["is_default"] == true).count();
    assert_eq!(
        default_count, 1,
        "should have exactly 1 default policy, got {default_count}"
    );

    let default_group = policy_group_repo::find_default_group(&db)
        .await
        .unwrap()
        .expect("default group should exist");
    let items = policy_group_repo::find_group_items(&db, default_group.id)
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].policy_id, new_default_id);
}

#[actix_web::test]
async fn test_patch_policy_promotes_existing_policy_to_default() {
    use aster_drive::db::repository::policy_group_repo;

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Patch To Default",
            "connection": local_connection_json("/tmp/test-patch-default"),
            "max_file_size": 0,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "is_default": true }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["is_default"], true);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let policies = body["data"]["items"].as_array().unwrap();
    let default_ids: Vec<i64> = policies
        .iter()
        .filter(|policy| policy["is_default"] == true)
        .map(|policy| policy["id"].as_i64().unwrap())
        .collect();

    assert_eq!(default_ids, vec![policy_id]);

    let default_group = policy_group_repo::find_default_group(&db)
        .await
        .unwrap()
        .expect("default group should exist");
    let items = policy_group_repo::find_group_items(&db, default_group.id)
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].policy_id, policy_id);
}

#[actix_web::test]
async fn test_set_only_default_rejects_missing_policy_without_clearing_default() {
    use aster_drive::db::repository::policy_repo;

    let state = common::setup().await;
    let original_default = policy_repo::find_default(state.writer_db())
        .await
        .unwrap()
        .expect("default policy should exist");

    let err = policy_repo::set_only_default(state.writer_db(), i64::MAX)
        .await
        .unwrap_err();
    assert!(err.message().contains("policy"));

    let current_default = policy_repo::find_default(state.writer_db())
        .await
        .unwrap()
        .expect("default policy should still exist");
    assert_eq!(current_default.id, original_default.id);
}

#[actix_web::test]
async fn test_cannot_disable_default_policy_group() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policy-groups")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let groups = body["data"]["items"]
        .as_array()
        .expect("policy group list should be an array");
    let group_id = groups
        .iter()
        .find(|item| item["is_default"].as_bool() == Some(true))
        .and_then(|item| item["id"].as_i64())
        .expect("default policy group should exist in list");

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/policy-groups/{group_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "is_enabled": false }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["msg"],
        "cannot disable the default storage policy group; set another group as default first"
    );
}

#[actix_web::test]
async fn test_policy_groups_are_sorted_by_created_at_desc() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    for group_name in ["First Group", "Second Group"] {
        let req = test::TestRequest::post()
            .uri("/api/v1/admin/policy-groups")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({
                "name": group_name,
                "description": format!("{group_name} description"),
                "is_enabled": true,
                "is_default": false,
                "items": [
                    {
                        "policy_id": policy_id,
                        "priority": 1,
                        "min_file_size": 0,
                        "max_file_size": 0
                    }
                ]
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policy-groups?limit=3&offset=0")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let groups = body["data"]["items"].as_array().unwrap();
    assert_eq!(body["data"]["total"], 3);
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0]["name"], "Second Group");
    assert_eq!(groups[1]["name"], "First Group");
}

#[actix_web::test]
async fn test_cannot_disable_assigned_policy_group() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policy-groups")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Assigned Group",
            "description": "Used by one user",
            "is_enabled": true,
            "is_default": false,
            "items": [
                {
                    "policy_id": policy_id,
                    "priority": 1,
                    "min_file_size": 0,
                    "max_file_size": 0
                }
            ]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let group_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/users")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let user_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/users/{user_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "policy_group_id": group_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/policy-groups/{group_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "is_enabled": false }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["msg"],
        "cannot disable policy group: 1 user assignment(s) still reference it"
    );
}

#[actix_web::test]
async fn test_cannot_assign_disabled_policy_group_to_user() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policy-groups")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Legacy Disabled Group",
            "description": "Disabled after assignment",
            "is_enabled": true,
            "is_default": false,
            "items": [
                {
                    "policy_id": policy_id,
                    "priority": 1,
                    "min_file_size": 0,
                    "max_file_size": 0
                }
            ]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let group_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/policy-groups/{group_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "is_enabled": false }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/users")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let user_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/users/{user_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "policy_group_id": group_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["msg"], "cannot assign a disabled storage policy group");
}

#[actix_web::test]
async fn test_cannot_disable_or_delete_policy_group_assigned_to_team() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policy-groups")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Team Bound Group",
            "description": "Referenced by a team",
            "is_enabled": true,
            "is_default": false,
            "items": [
                {
                    "policy_id": policy_id,
                    "priority": 1,
                    "min_file_size": 0,
                    "max_file_size": 0
                }
            ]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let group_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "teampolicyadmin",
            "email": "teampolicyadmin@example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/teams")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Policy Bound Team",
            "admin_identifier": "teampolicyadmin",
            "policy_group_id": group_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/policy-groups/{group_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "is_enabled": false }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["msg"],
        "cannot disable policy group: 1 team assignment(s) still reference it"
    );

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/policy-groups/{group_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["msg"],
        "cannot delete policy group: 1 team assignment(s) still reference it"
    );
}

#[actix_web::test]
async fn test_migrate_policy_group_assignments_moves_assignments_and_preserves_default() {
    let state = common::setup().await;
    let admin_user = common::create_test_account(
        &state,
        "pgmigrate-admin",
        "pgmigrate-admin@example.com",
        "password123",
    )
    .await
    .unwrap();
    let user_with_source_only = common::create_test_account(
        &state,
        "pgmigrate1",
        "pgmigrate1@example.com",
        "password123",
    )
    .await
    .unwrap();
    let user_with_existing_target = common::create_test_account(
        &state,
        "pgmigrate2",
        "pgmigrate2@example.com",
        "password123",
    )
    .await
    .unwrap();
    let app = create_test_app!(state);
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": admin_user.username,
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let token = common::extract_cookie(&resp, "aster_access").unwrap();

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policy-groups")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Source Group",
            "description": "Users will be migrated away",
            "is_enabled": true,
            "is_default": false,
            "items": [
                {
                    "policy_id": policy_id,
                    "priority": 1,
                    "min_file_size": 0,
                    "max_file_size": 0
                }
            ]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let source_group_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policy-groups")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Target Group",
            "description": "Users land here after migration",
            "is_enabled": true,
            "is_default": false,
            "items": [
                {
                    "policy_id": policy_id,
                    "priority": 1,
                    "min_file_size": 0,
                    "max_file_size": 0
                }
            ]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let target_group_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/users/{}", user_with_source_only.id))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "policy_group_id": source_group_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::patch()
        .uri(&format!(
            "/api/v1/admin/users/{}",
            user_with_existing_target.id
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "policy_group_id": target_group_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/admin/policy-groups/{source_group_id}/migrate-assignments"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target_group_id": target_group_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["source_group_id"], source_group_id);
    assert_eq!(body["data"]["target_group_id"], target_group_id);
    assert_eq!(body["data"]["affected_users"], 1);
    assert_eq!(body["data"]["affected_teams"], 0);
    assert_eq!(body["data"]["migrated_assignments"], 1);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/users/{}", user_with_source_only.id))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["policy_group_id"], target_group_id);

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/admin/users/{}",
            user_with_existing_target.id
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["policy_group_id"], target_group_id);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/policy-groups/{source_group_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["is_default"], false);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/policy-groups/{target_group_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["is_default"], false);
}

#[actix_web::test]
async fn test_cannot_migrate_policy_group_assignments_to_disabled_group() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policy-groups")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Migration Source",
            "description": "source",
            "is_enabled": true,
            "is_default": false,
            "items": [
                {
                    "policy_id": policy_id,
                    "priority": 1,
                    "min_file_size": 0,
                    "max_file_size": 0
                }
            ]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let source_group_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policy-groups")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Disabled Target",
            "description": "target",
            "is_enabled": false,
            "is_default": false,
            "items": [
                {
                    "policy_id": policy_id,
                    "priority": 1,
                    "min_file_size": 0,
                    "max_file_size": 0
                }
            ]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let target_group_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/admin/policy-groups/{source_group_id}/migrate-assignments"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target_group_id": target_group_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["msg"],
        "cannot migrate assignments to a disabled storage policy group"
    );
}

// ── 不能删除唯一的默认系统策略 ──────────────────────────────

#[actix_web::test]
async fn test_cannot_delete_only_default_policy() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 获取默认策略 ID
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    // 尝试删除唯一默认策略 → 应被拒绝
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "should reject deleting only default policy, got {}",
        resp.status()
    );
}

#[actix_web::test]
async fn test_cannot_delete_builtin_system_policy_even_after_switching_default() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let initial_policies = body["data"]["items"].as_array().unwrap();
    assert_eq!(
        initial_policies.len(),
        1,
        "fresh setup should contain exactly one built-in policy"
    );
    let built_in_policy_id = initial_policies[0]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Replacement Default",
            "connection": local_connection_json(format!("/tmp/test-replacement-default-{}", uuid::Uuid::new_v4())),
            "max_file_size": 0,
            "is_default": true
        }))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/policies/{built_in_policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "should reject deleting built-in policy #{built_in_policy_id}, got {}",
        resp.status()
    );

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let policies = body["data"]["items"].as_array().unwrap();
    assert!(
        policies
            .iter()
            .any(|policy| policy["id"].as_i64() == Some(built_in_policy_id)),
        "built-in policy #{built_in_policy_id} should still exist after failed delete"
    );
}

// ── 不能取消唯一的默认系统策略 ──────────────────────────────

#[actix_web::test]
async fn test_cannot_unset_only_default_policy() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 获取默认策略 ID
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    // 尝试取消 default → 应被拒绝
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({"is_default": false}))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "should reject unsetting only default, got {}",
        resp.status()
    );
}

// ── 用户绑定策略组的运行时校验 ─────────────────────────────

#[actix_web::test]
async fn test_resolve_policy_fails_without_user_policy_group() {
    use aster_drive::db::repository::user_repo;
    use aster_drive::services::files::file;
    use sea_orm::{ActiveModelTrait, Set};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "nogroup-user",
        "nogroup-user@example.com",
        "password123",
    )
    .await
    .unwrap();

    let model = user_repo::find_by_id(state.writer_db(), user.id)
        .await
        .unwrap();
    let mut active: aster_drive_model::entities::user::ActiveModel = model.into();
    active.policy_group_id = Set(None);
    active.updated_at = Set(chrono::Utc::now());
    active.update(state.writer_db()).await.unwrap();
    state
        .driver_registry
        .reload_policy_snapshot(&state.policy_snapshot, state.writer_db())
        .await
        .unwrap();

    let err = file::resolve_policy_for_size(&state, user.id, None, 0)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "E030");
    assert!(err.message().contains("no storage policy group assigned"));
}

#[actix_web::test]
async fn test_resolve_policy_fails_for_disabled_assigned_policy_group() {
    use aster_drive::db::repository::{policy_group_repo, user_repo};
    use aster_drive::services::files::file;
    use sea_orm::{ActiveModelTrait, Set};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "disabledgrpusr",
        "disabled-group-user@example.com",
        "password123",
    )
    .await
    .unwrap();

    let default_policy = aster_drive::db::repository::policy_repo::find_default(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    let now = chrono::Utc::now();
    let group = policy_group_repo::create_group(
        state.writer_db(),
        aster_drive_model::entities::storage_policy_group::ActiveModel {
            name: Set("Disabled Assigned Group".to_string()),
            description: Set(String::new()),
            is_enabled: Set(true),
            is_default: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    policy_group_repo::create_group_item(
        state.writer_db(),
        aster_drive_model::entities::storage_policy_group_item::ActiveModel {
            group_id: Set(group.id),
            policy_id: Set(default_policy.id),
            priority: Set(1),
            min_file_size: Set(0),
            max_file_size: Set(0),
            created_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let user_model = user_repo::find_by_id(state.writer_db(), user.id)
        .await
        .unwrap();
    let mut user_active: aster_drive_model::entities::user::ActiveModel = user_model.into();
    user_active.policy_group_id = Set(Some(group.id));
    user_active.updated_at = Set(chrono::Utc::now());
    user_active.update(state.writer_db()).await.unwrap();

    let group_model = policy_group_repo::find_group_by_id(state.writer_db(), group.id)
        .await
        .unwrap();
    let mut group_active: aster_drive_model::entities::storage_policy_group::ActiveModel =
        group_model.into();
    group_active.is_enabled = Set(false);
    group_active.updated_at = Set(chrono::Utc::now());
    group_active.update(state.writer_db()).await.unwrap();

    state
        .driver_registry
        .reload_policy_snapshot(&state.policy_snapshot, state.writer_db())
        .await
        .unwrap();

    let err = file::resolve_policy_for_size(&state, user.id, None, 0)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "E005");
    assert!(err.message().contains("is disabled"));
}

#[actix_web::test]
async fn test_resolve_policy_fails_when_policy_group_has_no_matching_rule() {
    use aster_drive::db::repository::{policy_group_repo, policy_repo, user_repo};
    use aster_drive::services::{files::file, storage_policy::policy};
    use sea_orm::{ActiveModelTrait, Set};

    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "gappolicyuser",
        "gap-policy-user@example.com",
        "password123",
    )
    .await
    .unwrap();

    let default_policy = policy_repo::find_default(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    let overflow_path = format!("/tmp/asterdrive-gap-policy-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&overflow_path).unwrap();
    let overflow_policy = policy::create(
        &state,
        policy::CreateStoragePolicyInput {
            name: "Gap Overflow Policy".to_string(),
            connection: common::local_connection(overflow_path.clone()),
            max_file_size: 0,
            chunk_size: None,
            is_default: false,
            allowed_types: None,
        },
    )
    .await
    .unwrap();

    let now = chrono::Utc::now();
    let group = policy_group_repo::create_group(
        state.writer_db(),
        aster_drive_model::entities::storage_policy_group::ActiveModel {
            name: Set("Gap Rule Group".to_string()),
            description: Set(String::new()),
            is_enabled: Set(true),
            is_default: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    for (priority, policy_id, min_file_size, max_file_size) in [
        (1, default_policy.id, 0, 10),
        (2, overflow_policy.id, 20, 0),
    ] {
        policy_group_repo::create_group_item(
            state.writer_db(),
            aster_drive_model::entities::storage_policy_group_item::ActiveModel {
                group_id: Set(group.id),
                policy_id: Set(policy_id),
                priority: Set(priority),
                min_file_size: Set(min_file_size),
                max_file_size: Set(max_file_size),
                created_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }

    let user_model = user_repo::find_by_id(state.writer_db(), user.id)
        .await
        .unwrap();
    let mut user_active: aster_drive_model::entities::user::ActiveModel = user_model.into();
    user_active.policy_group_id = Set(Some(group.id));
    user_active.updated_at = Set(now);
    user_active.update(state.writer_db()).await.unwrap();
    state
        .driver_registry
        .reload_policy_snapshot(&state.policy_snapshot, state.writer_db())
        .await
        .unwrap();

    let err = file::resolve_policy_for_size(&state, user.id, None, 15)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "E005");
    assert!(err.message().contains("no storage policy rule"));
}

#[actix_web::test]
async fn test_policy_delete_clears_folder_policy_reference() {
    use aster_drive::db::repository::folder_repo;

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let policy_id = create_local_policy_via_admin(&app, &token, "Folder Override Policy").await;
    let folder_id = create_personal_folder(&app, &token, "override-folder", None).await;

    let resp = admin_set_folder_policy(&app, &token, folder_id, Some(policy_id)).await;
    assert_eq!(resp.status(), 200);

    let folder = folder_repo::find_by_id(&db, folder_id).await.unwrap();
    assert_eq!(folder.policy_id, Some(policy_id));

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let folder = folder_repo::find_by_id(&db, folder_id).await.unwrap();
    assert_eq!(folder.policy_id, None);
}

#[actix_web::test]
async fn test_admin_folder_policy_can_be_cleared_with_null() {
    use aster_drive::db::repository::folder_repo;

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let policy_id =
        create_local_policy_via_admin(&app, &token, "Nullable Folder Override Policy").await;
    let folder_id = create_personal_folder(&app, &token, "nullable-override-folder", None).await;

    let resp = admin_set_folder_policy(&app, &token, folder_id, Some(policy_id)).await;
    assert_eq!(resp.status(), 200);

    let folder = folder_repo::find_by_id(&db, folder_id).await.unwrap();
    assert_eq!(folder.policy_id, Some(policy_id));

    let resp = admin_set_folder_policy(&app, &token, folder_id, None).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["policy_id"].is_null());

    let folder = folder_repo::find_by_id(&db, folder_id).await.unwrap();
    assert_eq!(folder.policy_id, None);
}

#[actix_web::test]
async fn test_non_admin_folder_patch_cannot_set_or_clear_policy() {
    use aster_drive::db::repository::folder_repo;

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let policy_id = create_local_policy_via_admin(&app, &token, "Admin Only Policy").await;
    let folder_id = create_personal_folder(&app, &token, "admin-only-folder-policy", None).await;

    let normal_user_id = admin_create_user!(
        app,
        token,
        "folderpolicyuser",
        "folderpolicyuser@example.com",
        "password123"
    );
    let normal_token = login_user!(app, "folderpolicyuser", "password123").0;
    let user_folder_id =
        create_personal_folder(&app, &normal_token, "normal-user-folder", None).await;

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{user_folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&normal_token)))
        .insert_header(common::csrf_header_for(&normal_token))
        .set_json(serde_json::json!({ "policy_id": policy_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.admin_required");
    assert_eq!(
        folder_repo::find_by_id(&db, user_folder_id)
            .await
            .unwrap()
            .policy_id,
        None
    );

    let resp = admin_set_folder_policy(&app, &token, folder_id, Some(policy_id)).await;
    assert_eq!(resp.status(), 200);
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "policy_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
    assert_eq!(
        folder_repo::find_by_id(&db, folder_id)
            .await
            .unwrap()
            .policy_id,
        Some(policy_id)
    );

    let user = aster_drive::db::repository::user_repo::find_by_id(&db, normal_user_id)
        .await
        .unwrap();
    assert_eq!(user.username, "folderpolicyuser");
}

#[actix_web::test]
async fn test_regular_folder_patch_omits_policy_id_and_preserves_binding() {
    use aster_drive::db::repository::folder_repo;

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let policy_id = create_local_policy_via_admin(&app, &token, "Patch Preserve Policy").await;
    let folder_id = create_personal_folder(&app, &token, "patch-preserve-policy", None).await;

    let resp = admin_set_folder_policy(&app, &token, folder_id, Some(policy_id)).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "patch-preserve-renamed" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "patch-preserve-renamed");
    assert_eq!(body["data"]["policy_id"], policy_id);

    let folder = folder_repo::find_by_id(&db, folder_id).await.unwrap();
    assert_eq!(folder.name, "patch-preserve-renamed");
    assert_eq!(folder.policy_id, Some(policy_id));
}

#[actix_web::test]
async fn test_team_owner_cannot_patch_team_folder_policy() {
    use aster_drive::db::repository::folder_repo;

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);
    let owner_id = admin_create_user!(
        app,
        admin_token,
        "tfpowner",
        "tfpowner@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "tfpowner", "password123").0;
    let policy_id =
        create_local_policy_via_admin(&app, &admin_token, "Team Owner Forbidden Policy").await;

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/teams")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "name": "Team Folder Policy Scope",
            "admin_user_id": owner_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Team Folder" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/teams/{team_id}/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "policy_id": policy_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
    assert_eq!(
        folder_repo::find_by_id(&db, folder_id)
            .await
            .unwrap()
            .policy_id,
        None
    );
}

#[actix_web::test]
async fn test_admin_folder_policy_rejects_unknown_and_deleted_folder() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let folder_id = create_personal_folder(&app, &token, "folder-policy-edge", None).await;

    let resp = admin_set_folder_policy(&app, &token, folder_id, Some(9_999_999)).await;
    assert_eq!(resp.status(), 404);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let policy_id = create_local_policy_via_admin(&app, &token, "Deleted Folder Policy").await;
    let resp = admin_set_folder_policy(&app, &token, folder_id, Some(policy_id)).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_folder_policy_inheritance_override_and_clear_affect_uploads() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let default_policy_id =
        aster_drive::db::repository::policy_repo::find_default(state.writer_db())
            .await
            .unwrap()
            .expect("default policy should exist")
            .id;
    let parent_policy_id =
        create_local_policy_via_admin(&app, &token, "Parent Folder Upload Policy").await;
    let child_policy_id =
        create_local_policy_via_admin(&app, &token, "Child Folder Upload Policy").await;
    let parent_id = create_personal_folder(&app, &token, "policy-parent", None).await;
    let child_id = create_personal_folder(&app, &token, "policy-child", Some(parent_id)).await;

    let file_id = upload_test_file_to_folder!(app, token, child_id);
    assert_eq!(
        uploaded_file_policy_id(&state, file_id).await,
        default_policy_id
    );

    let resp = admin_set_folder_policy(&app, &token, parent_id, Some(parent_policy_id)).await;
    assert_eq!(resp.status(), 200);
    let file_id = upload_test_file_to_folder!(app, token, child_id);
    assert_eq!(
        uploaded_file_policy_id(&state, file_id).await,
        parent_policy_id
    );

    let resp = admin_set_folder_policy(&app, &token, child_id, Some(child_policy_id)).await;
    assert_eq!(resp.status(), 200);
    let file_id = upload_test_file_to_folder!(app, token, child_id);
    assert_eq!(
        uploaded_file_policy_id(&state, file_id).await,
        child_policy_id
    );

    let resp = admin_set_folder_policy(&app, &token, child_id, None).await;
    assert_eq!(resp.status(), 200);
    let file_id = upload_test_file_to_folder!(app, token, child_id);
    assert_eq!(
        uploaded_file_policy_id(&state, file_id).await,
        parent_policy_id
    );

    let resp = admin_set_folder_policy(&app, &token, parent_id, None).await;
    assert_eq!(resp.status(), 200);
    let file_id = upload_test_file_to_folder!(app, token, child_id);
    assert_eq!(
        uploaded_file_policy_id(&state, file_id).await,
        default_policy_id
    );
}

#[actix_web::test]
async fn test_folder_policy_inheritance_deep_chain_affects_uploads() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let policy_id = create_local_policy_via_admin(&app, &token, "Deep Folder Upload Policy").await;

    let root_id = create_personal_folder(&app, &token, "deep-policy-root", None).await;
    let resp = admin_set_folder_policy(&app, &token, root_id, Some(policy_id)).await;
    assert_eq!(resp.status(), 200);

    let current_parent_id = create_nested_folders(12, &app, &token, root_id).await;

    let file_id = upload_test_file_to_folder!(app, token, current_parent_id);
    assert_eq!(uploaded_file_policy_id(&state, file_id).await, policy_id);
}

#[actix_web::test]
async fn test_policy_connection_endpoints_for_local_driver() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let stored_base_path = format!("/tmp/test-policy-connection-{}", uuid::Uuid::new_v4());
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Connection Test Policy",
            "connection": local_connection_json(&stored_base_path),
            "max_file_size": 0,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/admin/policies/{policy_id}/test"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::Success.as_str());
    assert_eq!(body["data"], serde_json::json!({}));
    assert!(!std::path::Path::new(&format!("{stored_base_path}/_aster_connection_test")).exists());

    let temp_base_path = format!("/tmp/test-policy-params-{}", uuid::Uuid::new_v4());
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/test")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "connection": local_connection_json(&temp_base_path)
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::Success.as_str());
    assert_eq!(body["data"], serde_json::json!({}));
    assert!(!std::path::Path::new(&format!("{temp_base_path}/_aster_connection_test")).exists());
}

#[actix_web::test]
async fn test_policy_connection_failures_return_admin_diagnostic_payload() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let blocked_base_path = format!("/tmp/test-policy-probe-file-{}", uuid::Uuid::new_v4());
    tokio::fs::write(&blocked_base_path, b"not a directory")
        .await
        .expect("probe fixture file should be written");

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/test")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "connection": local_connection_json(&blocked_base_path)
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 500);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::StorageMisconfigured.as_str());
    assert_eq!(body["msg"], "Storage Driver Error");
    assert!(body.get("data").is_none());
    assert!(body["error"]["retryable"].as_bool().is_some());
    assert!(body["error"]["diagnostic"]["kind"].as_str().is_some());
    assert!(body["error"]["diagnostic"].get("api_code").is_none());
    assert!(body["error"]["diagnostic"].get("retryable").is_none());
    let diagnostic_message = body["error"]["diagnostic"]["message"]
        .as_str()
        .expect("storage probe diagnostic should include the driver message");
    assert!(diagnostic_message.contains("connection test failed"));
    assert_ne!(diagnostic_message, "Storage Driver Error");

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Saved Probe Failure Policy",
            "connection": local_connection_json(&blocked_base_path),
            "max_file_size": 0,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/admin/policies/{policy_id}/test"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 500);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::StorageMisconfigured.as_str());
    assert_eq!(body["msg"], "Storage Driver Error");
    assert!(body.get("data").is_none());
    assert_eq!(body["error"]["diagnostic"]["kind"], "misconfigured");
    assert!(body["error"]["diagnostic"].get("api_code").is_none());
    assert!(body["error"]["diagnostic"].get("retryable").is_none());
    let diagnostic_message = body["error"]["diagnostic"]["message"]
        .as_str()
        .expect("saved storage probe diagnostic should include the driver message");
    assert!(diagnostic_message.contains("write test failed"));
    assert_ne!(diagnostic_message, "Storage Driver Error");
}

#[actix_web::test]
async fn test_policy_params_rejects_onedrive_draft_connection_test() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/test")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "connection": onedrive_connection_json("draft-root")
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::PolicyActionUnsupported.as_str());
    assert_eq!(
        body["msg"],
        "storage policy driver 'asterdrive.storage.onedrive' requires a saved storage policy with completed authorization; use the saved policy connection test after authorization"
    );
    assert!(body.get("data").is_none());
    assert_eq!(body["error"]["retryable"], false);
}

#[actix_web::test]
async fn test_connector_action_endpoints_reject_unknown_actions_with_stable_code() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/action")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action_id": "plugin.missing",
            "values": {},
            "connection": local_action_connection()
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::PolicyActionUnsupported.as_str());
    assert!(body["msg"].as_str().unwrap().contains("plugin.missing"));
    assert!(
        body["msg"]
            .as_str()
            .unwrap()
            .contains("asterdrive.storage.local")
    );

    let local_policy_id = create_local_policy_via_admin(&app, &token, "Missing Saved Action").await;
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/admin/policies/{local_policy_id}/action"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action_id": "plugin.missing",
            "values": {}
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::PolicyActionUnsupported.as_str());
}

#[actix_web::test]
async fn test_tencent_cos_action_validates_typed_values_before_draft_and_saved_execution() {
    let state = common::setup().await;
    assert!(
        site_url::public_site_urls(state.runtime_config()).is_empty(),
        "valid typed action input should stop at the missing public_site_url boundary"
    );
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/action")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action_id": "configure_tencent_cos_cors",
            "values": { "undeclared": true },
            "connection": tencent_cos_action_connection()
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyActionParameterInvalid.as_str()
    );
    assert!(body["msg"].as_str().unwrap().contains("undeclared"));

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/action")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action_id": "configure_tencent_cos_cors",
            "values": {},
            "connection": tencent_cos_action_connection()
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyActionParameterRequired.as_str()
    );

    let cos_policy_id = create_tencent_cos_policy_via_admin(&app, &token, "COS Saved Action").await;
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/admin/policies/{cos_policy_id}/action"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action_id": "configure_tencent_cos_cors",
            "values": {}
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyActionParameterRequired.as_str()
    );
}

#[actix_web::test]
async fn test_policy_params_reuses_saved_credentials_before_connector_config_validation() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let cos_policy_id =
        create_tencent_cos_policy_via_admin(&app, &token, "COS Draft Test Reuse").await;
    let mut connection = connection_json(common::tencent_cos_connection(
        "https://cos.ap-guangzhou.myqcloud.com",
        "media-draft-1250000000",
        "",
        "",
        "",
    ));
    connection["connector_config"]["values"]["onedrive_account_mode"] =
        serde_json::json!("work_or_school");

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/test")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "policy_id": cos_policy_id,
            "connection": connection
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::BadRequest.as_str(),
        "blank draft credentials should be filled from the saved policy before connector option validation"
    );
    assert!(
        body["msg"]
            .as_str()
            .is_some_and(|message| message.contains("onedrive_account_mode")),
        "connector config validation should run after saved credentials are restored: {body}"
    );
    assert_ne!(
        body["code"],
        ApiErrorCode::PolicyStorageAccessKeyRequired.as_str(),
        "blank draft access_key should be filled from the saved policy before connector option validation"
    );
    assert_ne!(
        body["code"],
        ApiErrorCode::PolicyStorageSecretKeyRequired.as_str(),
        "blank draft secret_key should be filled from the saved policy before connector option validation"
    );

    let local_policy_id =
        create_local_policy_via_admin(&app, &token, "Cross Connector Draft Reuse").await;
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/test")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "policy_id": local_policy_id,
            "connection": connection_json(common::tencent_cos_connection(
                "https://cos.ap-guangzhou.myqcloud.com",
                "media-draft-1250000000",
                "",
                "",
                "",
            ))
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::BadRequest.as_str());
    assert!(
        body["msg"]
            .as_str()
            .is_some_and(|message| message.contains("asterdrive.storage.local")
                && message.contains("asterdrive.storage.tencent_cos")),
        "draft credential reuse must reject cross-connector policy ids: {body}"
    );
}

#[actix_web::test]
async fn test_tencent_cos_cors_dedicated_routes_are_not_exposed() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let cos_policy_id = create_tencent_cos_policy_via_admin(&app, &token, "COS CORS Routes").await;

    for uri in [
        "/api/v1/admin/policies/tencent-cos/cors".to_string(),
        format!("/api/v1/admin/policies/{cos_policy_id}/tencent-cos/cors"),
    ] {
        let req = test::TestRequest::post()
            .uri(&uri)
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({
                "action_id": "configure_tencent_cos_cors",
                "values": {}
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            404,
            "old dedicated route should not exist: {uri}"
        );
    }
}

#[actix_web::test]
async fn test_policy_create_and_params_reject_incomplete_s3_credentials_with_stable_code() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let mut connection = s3_connection_json("https://s3.example.com", "archive", "", "", "");
    connection["credential"]["values"] = serde_json::json!({});

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Incomplete S3",
            "connection": connection.clone()
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::BadRequest.as_str());
    assert!(
        body["msg"]
            .as_str()
            .is_some_and(|message| message.contains("s3_access_key_id")),
        "unexpected missing S3 access key response: {body}"
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/test")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "connection": connection
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::BadRequest.as_str());
    assert!(
        body["msg"]
            .as_str()
            .is_some_and(|message| message.contains("s3_access_key_id")),
        "unexpected draft missing S3 access key response: {body}"
    );
}

#[actix_web::test]
async fn test_policy_create_rejects_invalid_s3_storage_fields_with_stable_codes() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Missing Bucket S3",
            "connection": s3_connection_json("https://s3.example.com", "", "", "AKIA", "SECRET")
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::BadRequest.as_str());
    assert!(
        body["msg"]
            .as_str()
            .is_some_and(|message| message.contains("bucket")),
        "unexpected missing S3 bucket response: {body}"
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Invalid Endpoint S3",
            "connection": s3_connection_json("s3.example.com", "archive", "", "AKIA", "SECRET")
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::BadRequest.as_str());
    assert!(
        body["msg"]
            .as_str()
            .is_some_and(|message| message.contains("endpoint")),
        "unexpected invalid S3 endpoint response: {body}"
    );
}

#[actix_web::test]
async fn test_policy_create_and_draft_test_reject_invalid_s3_region_before_storage_probe() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for (uri, include_name) in [
        ("/api/v1/admin/policies", true),
        ("/api/v1/admin/policies/test", false),
    ] {
        let mut connection =
            s3_connection_json("https://s3.example.com", "archive", "", "AKIA", "SECRET");
        connection["connector_config"]["values"]["s3_region"] =
            serde_json::json!("us-east-1/invalid");
        let mut payload = serde_json::json!({
            "connection": connection
        });
        if include_name {
            payload["name"] = serde_json::json!("Invalid Region S3");
        }

        let req = test::TestRequest::post()
            .uri(uri)
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(payload)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400, "{uri} should reject invalid region");
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["code"], ApiErrorCode::BadRequest.as_str());
        assert!(
            body["msg"]
                .as_str()
                .is_some_and(|message| message.contains("s3_region must be")),
            "unexpected {uri} error body: {body}"
        );
    }
}

#[actix_web::test]
async fn test_non_s3_object_storage_rejects_s3_region_for_create_update_and_draft() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for (connector_name, valid_connection) in [
        (
            "azure_blob",
            azure_blob_connection_json("https://acct.blob.core.windows.net", "archive"),
        ),
        (
            "tencent_cos",
            connection_json(common::tencent_cos_connection(
                "https://cos.ap-guangzhou.myqcloud.com",
                "archive-1250000000",
                "",
                "ACCESS",
                "SECRET",
            )),
        ),
    ] {
        let mut invalid_connection = valid_connection.clone();
        invalid_connection["connector_config"]["values"]["s3_region"] =
            serde_json::json!("us-east-1");
        let req = test::TestRequest::post()
            .uri("/api/v1/admin/policies")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({
                "name": format!("Invalid Region {connector_name}"),
                "connection": invalid_connection.clone()
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            400,
            "{connector_name} create should reject s3_region"
        );
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["code"], ApiErrorCode::BadRequest.as_str());
        assert!(
            body["msg"]
                .as_str()
                .is_some_and(|message| message.contains("s3_region")),
            "unexpected create validation body: {body}"
        );

        let req = test::TestRequest::post()
            .uri("/api/v1/admin/policies")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({
                "name": format!("Valid {connector_name}"),
                "connection": valid_connection.clone()
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            201,
            "{connector_name} setup create should succeed"
        );
        let body: Value = test::read_body_json(resp).await;
        let policy_id = body["data"]["id"].as_i64().expect("policy id");

        let req = test::TestRequest::patch()
            .uri(&format!("/api/v1/admin/policies/{policy_id}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({
                "connector_config": invalid_connection["connector_config"].clone()
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            400,
            "{connector_name} update should reject s3_region"
        );
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["code"], ApiErrorCode::BadRequest.as_str());
        assert!(
            body["msg"]
                .as_str()
                .is_some_and(|message| message.contains("s3_region")),
            "unexpected update validation body: {body}"
        );

        let req = test::TestRequest::post()
            .uri("/api/v1/admin/policies/test")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({
                "connection": invalid_connection
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            400,
            "{connector_name} draft should reject s3_region"
        );
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["code"], ApiErrorCode::BadRequest.as_str());
        assert!(
            body["msg"]
                .as_str()
                .is_some_and(|message| message.contains("s3_region")),
            "unexpected draft validation body: {body}"
        );
    }
}

#[actix_web::test]
async fn test_policy_create_rejects_remote_without_node_with_stable_code() {
    use aster_drive_model::types::{RemoteDownloadStrategy, RemoteUploadStrategy};

    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Remote Missing Node",
            "connection": remote_connection_json(
                "remote-missing-node",
                None,
                Some("test-target".to_string()),
                RemoteDownloadStrategy::RelayStream,
                RemoteUploadStrategy::RelayStream,
            )
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::BadRequest.as_str());
    assert!(
        body["msg"]
            .as_str()
            .is_some_and(|message| message.contains("remote_node_id"))
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/test")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "connection": remote_connection_json(
                "remote-missing-node",
                None,
                Some("test-target".to_string()),
                RemoteDownloadStrategy::RelayStream,
                RemoteUploadStrategy::RelayStream,
            )
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::BadRequest.as_str());
    assert!(
        body["msg"]
            .as_str()
            .is_some_and(|message| message.contains("remote_node_id"))
    );
}

#[actix_web::test]
async fn test_policy_create_rejects_remote_field_for_non_remote_connector() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let base_path = format!("/tmp/test-policy-unexpected-node-{}", uuid::Uuid::new_v4());
    let mut create_connection = local_connection_json(&base_path);
    create_connection["connector_config"]["values"]["remote_node_id"] = serde_json::json!(42);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Local Unexpected Node",
            "connection": create_connection
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::BadRequest.as_str());
    assert!(
        body["msg"]
            .as_str()
            .is_some_and(|message| message.contains("remote_node_id"))
    );

    let mut draft_connection = local_connection_json(format!(
        "/tmp/test-policy-unexpected-node-{}",
        uuid::Uuid::new_v4()
    ));
    draft_connection["connector_config"]["values"]["remote_node_id"] = serde_json::json!(42);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/test")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "connection": draft_connection
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::BadRequest.as_str());
    assert!(
        body["msg"]
            .as_str()
            .is_some_and(|message| message.contains("remote_node_id"))
    );
}

#[actix_web::test]
async fn test_policy_create_rejects_unusable_remote_nodes_with_stable_codes() {
    use aster_drive::services::remote::remote_node;
    use aster_drive_model::types::{
        RemoteDownloadStrategy, RemoteNodeTransportMode, RemoteUploadStrategy,
    };

    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let disabled_node = remote_node::create(
        &state,
        remote_node::CreateRemoteNodeInput {
            name: "disabled-policy-node".to_string(),
            base_url: "https://disabled-policy-node.example.com".to_string(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: false,
        },
    )
    .await
    .expect("disabled remote node should be created");
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Disabled Remote Policy",
            "connection": remote_connection_json(
                "disabled-remote",
                Some(disabled_node.id),
                Some("disabled-target".to_string()),
                RemoteDownloadStrategy::RelayStream,
                RemoteUploadStrategy::RelayStream,
            )
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyRemoteNodeDisabled.as_str()
    );

    let direct_node_without_url = remote_node::create(
        &state,
        remote_node::CreateRemoteNodeInput {
            name: "direct-empty-url-policy-node".to_string(),
            base_url: String::new(),
            transport_mode: RemoteNodeTransportMode::Direct,
            is_enabled: true,
        },
    )
    .await
    .expect("direct remote node without URL should be created");
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Direct Missing URL Policy",
            "connection": remote_connection_json(
                "direct-missing-url",
                Some(direct_node_without_url.id),
                Some("direct-target".to_string()),
                RemoteDownloadStrategy::RelayStream,
                RemoteUploadStrategy::RelayStream,
            )
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyRemoteNodeBaseUrlRequired.as_str()
    );

    let reverse_node = remote_node::create(
        &state,
        remote_node::CreateRemoteNodeInput {
            name: "reverse-presigned-policy-node".to_string(),
            base_url: String::new(),
            transport_mode: RemoteNodeTransportMode::ReverseTunnel,
            is_enabled: true,
        },
    )
    .await
    .expect("reverse remote node should be created");
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Reverse Presigned Policy",
            "connection": remote_connection_json(
                "reverse-presigned",
                Some(reverse_node.id),
                Some("reverse-target".to_string()),
                RemoteDownloadStrategy::RelayStream,
                RemoteUploadStrategy::Presigned,
            )
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyRemoteNodeTransferStrategyUnsupported.as_str()
    );
}

#[actix_web::test]
async fn test_policy_update_treats_blank_s3_secret_as_keep_existing() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Valid S3",
            "connection": s3_connection_json(
                "https://s3.example.com",
                "archive",
                "",
                "AKIA",
                "SECRET",
            )
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "credential": {
                "mode": "static",
                "values": {
                    "s3_access_key_id": "AKIA",
                    "s3_secret_access_key": ""
                }
            }
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Still Valid S3"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}
