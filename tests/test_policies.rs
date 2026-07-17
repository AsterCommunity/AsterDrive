//! 存储策略管理测试

#[macro_use]
mod common;
use aster_drive::api::api_error_code::ApiErrorCode;
use aster_drive::config::site_url;
use aster_drive::runtime::SharedRuntimeState;

use actix_web::test;
use chrono::{Duration, Utc};
use serde_json::Value;

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
            "driver_type": "local",
            "base_path": format!("/tmp/asterdrive-{}-{}", name.to_ascii_lowercase().replace(' ', "-"), uuid::Uuid::new_v4()),
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
            "driver_type": "tencent_cos",
            "endpoint": "https://cos.ap-guangzhou.myqcloud.com",
            "bucket": "media-1250000000",
            "access_key": "AKIDEXAMPLE",
            "secret_key": "SECRETEXAMPLE",
            "max_file_size": 0,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    body["data"]["id"].as_i64().unwrap()
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

    assert_eq!(descriptors.len(), 7);

    let descriptor = |driver_type: &str| {
        descriptors
            .iter()
            .find(|item| item["driver_type"] == driver_type)
            .unwrap_or_else(|| panic!("{driver_type} descriptor should exist"))
    };

    let onedrive = descriptor("one_drive");
    assert_eq!(onedrive["credential_mode"], "oauth_delegated");
    assert_eq!(onedrive["requires_authorization"], true);
    assert_eq!(onedrive["authorization_provider"], "microsoft_graph");
    let onedrive_actions = onedrive["actions"].as_array().expect("onedrive actions");
    assert!(!onedrive_actions.iter().any(|action| {
        action["affordance_action"] == "test_draft_connection"
            && action["kind"] == "connection_test"
    }));
    let saved_onedrive_test = onedrive_actions
        .iter()
        .find(|action| {
            action["affordance_action"] == "test_saved_connection"
                && action["kind"] == "connection_test"
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
        false
    );
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
    assert_eq!(onedrive_resumable["frontend_direct_upload"], false);
    assert_eq!(onedrive_resumable["implicit_completion"], true);
    assert_eq!(onedrive_resumable["abort_supported"], false);
    assert_eq!(onedrive_resumable["status_query_supported"], false);
    assert_eq!(
        onedrive["upload_workflows"]["simple_upload_capabilities"]["max_provider_single_request_size"],
        250_000_000
    );
    assert!(onedrive["upload_workflows"]["object_multipart_upload_capabilities"].is_null());

    let s3 = descriptor("s3");
    assert!(s3["actions"].as_array().expect("s3 actions").iter().any(
        |action| action["affordance_action"] == "test_draft_connection"
            && action["kind"] == "connection_test"
    ));
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

    let azure_blob = descriptor("azure_blob");
    assert_eq!(
        azure_blob["upload_workflows"]["object_multipart_upload"],
        true
    );
    assert_eq!(
        azure_blob["upload_workflows"]["object_multipart_upload_capabilities"]["presigned_part_etag_required"],
        false
    );

    let tencent_cos = descriptor("tencent_cos");
    assert_eq!(
        tencent_cos["capabilities"]["storage_native_thumbnail"],
        true
    );
    assert_eq!(
        tencent_cos["capabilities"]["storage_native_media_metadata"],
        true
    );
    assert!(
        tencent_cos["actions"]
            .as_array()
            .expect("cos actions")
            .iter()
            .any(
                |action| action["policy_action"] == "configure_tencent_cos_cors"
                    && action["kind"] == "policy_action"
                    && action["mutates_remote_state"] == true
            )
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
    assert_eq!(s3_path_style["visible_when_driver_types"][0], "s3");

    let local = descriptor("local");
    assert_eq!(local["upload_workflows"]["object_multipart_upload"], false);
    assert_eq!(local["capabilities"]["remote_node_binding"], false);

    let sftp = descriptor("sftp");
    assert_eq!(sftp["credential_mode"], "static_secret");
    assert_eq!(sftp["ui"]["label_key"], "driver_type_sftp");
    assert_eq!(sftp["upload_workflows"]["stream_upload"], true);
    assert_eq!(sftp["upload_workflows"]["object_multipart_upload"], false);
    assert_eq!(sftp["capabilities"]["efficient_range"], true);
    assert_eq!(sftp["capabilities"]["remote_node_binding"], false);

    let remote = descriptor("remote");
    assert_eq!(remote["upload_workflows"]["object_multipart_upload"], true);
    assert_eq!(remote["capabilities"]["remote_node_binding"], true);
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
    status: Option<aster_drive::types::UploadSessionStatus>,
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
        aster_drive::entities::upload_session::ActiveModel {
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
                .unwrap_or(aster_drive::types::UploadSessionStatus::Uploading)),
            session_kind: Set(None),
            object_temp_key: Set(spec.object_temp_key.map(str::to_string)),
            object_multipart_id: Set(None),
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
    use aster_drive::types::DriverType;

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
            connection: policy::StoragePolicyConnectionInput {
                driver_type: DriverType::Local,
                endpoint: String::new(),
                bucket: String::new(),
                access_key: String::new(),
                secret_key: String::new(),
                base_path: alternate_base_path.clone(),
                remote_node_id: None,
                remote_storage_target_key: None,
                options: Default::default(),
            },
            max_file_size: 0,
            chunk_size: None,
            is_default: false,
            allowed_types: None,
            options: None,
            remote_storage_target_key: None,
            application_config: Default::default(),
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

    let mut user_active: aster_drive::entities::user::ActiveModel = user.into();
    user_active.policy_group_id = Set(None);
    user_active.update(state.writer_db()).await.unwrap();

    policy::ensure_policy_groups_seeded(state.writer_db())
        .await
        .unwrap();

    let updated = user_repo::find_by_email(state.writer_db(), "policy-backfill@example.com")
        .await
        .unwrap()
        .expect("user should exist");
    assert_eq!(updated.policy_group_id, Some(default_group.id));
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
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Test S3",
            "driver_type": "s3",
            "endpoint": "http://localhost:9000",
            "bucket": "test-bucket",
            "access_key": "minioadmin",
            "secret_key": "minioadmin",
            "base_path": "",
            "max_file_size": 104857600,
            "chunk_size": 8388608,
            "options": serde_json::json!({
                "object_storage_upload_strategy": "presigned",
                "s3_path_style": false
            })
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "Test S3");
    assert_eq!(body["data"]["chunk_size"], 8_388_608);
    assert_eq!(
        body["data"]["options"]["object_storage_upload_strategy"],
        "presigned"
    );
    assert_eq!(body["data"]["options"]["s3_path_style"], false);
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
    assert_eq!(body["data"]["options"]["s3_path_style"], false);

    // 更新策略
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Renamed S3" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "Renamed S3");

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
async fn test_policy_promotes_generic_s3_policy_to_tencent_cos() {
    use aster_drive::services::storage_policy::policy;
    use aster_drive::types::{
        DriverType, ObjectStorageDownloadStrategy, ObjectStorageUploadStrategy,
        StoragePolicyOptions, parse_storage_policy_options,
    };

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let policy = policy::create(
        &state,
        policy::CreateStoragePolicyInput {
            name: "COS via S3".to_string(),
            connection: policy::StoragePolicyConnectionInput {
                driver_type: DriverType::S3,
                endpoint: "https://cos.ap-guangzhou.myqcloud.com".to_string(),
                bucket: "bucket-1250000000".to_string(),
                access_key: "ak".to_string(),
                secret_key: "sk".to_string(),
                base_path: "tenant/prefix".to_string(),
                remote_node_id: None,
                remote_storage_target_key: None,
                options: Default::default(),
            },
            max_file_size: 0,
            chunk_size: None,
            is_default: false,
            allowed_types: None,
            options: Some(StoragePolicyOptions {
                object_storage_upload_strategy: Some(ObjectStorageUploadStrategy::Presigned),
                object_storage_download_strategy: Some(ObjectStorageDownloadStrategy::Presigned),
                s3_path_style: Some(false),
                ..Default::default()
            }),
            remote_storage_target_key: None,
            application_config: Default::default(),
        },
    )
    .await
    .unwrap();

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/admin/policies/{}/promote-s3-driver",
            policy.id
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target_driver_type": "tencent_cos",
            "endpoint": "https://cos.ap-guangzhou.myqcloud.com",
            "bucket": "bucket-1250000000"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["driver_type"], "tencent_cos");
    assert_eq!(body["data"]["bucket"], "bucket-1250000000");
    assert_eq!(body["data"]["base_path"], "tenant/prefix");
    assert_eq!(
        body["data"]["options"]["object_storage_upload_strategy"],
        "presigned"
    );
    assert_eq!(
        body["data"]["options"]["object_storage_download_strategy"],
        "presigned"
    );
    assert_eq!(body["data"]["options"]["s3_path_style"], false);

    let stored = aster_drive::db::repository::policy_repo::find_by_id(&db, policy.id)
        .await
        .unwrap();
    assert_eq!(stored.driver_type, DriverType::TencentCos);
    assert_eq!(stored.bucket, "bucket-1250000000");
    assert_eq!(stored.base_path, "tenant/prefix");
    let stored_options = parse_storage_policy_options(stored.options.as_ref());
    assert_eq!(
        stored_options.effective_object_storage_upload_strategy(),
        ObjectStorageUploadStrategy::Presigned
    );
    assert_eq!(
        stored_options.effective_object_storage_download_strategy(),
        ObjectStorageDownloadStrategy::Presigned
    );
    assert!(!stored_options.effective_s3_path_style());
}

#[actix_web::test]
async fn test_policy_promote_s3_driver_rejects_bucket_change() {
    use aster_drive::services::storage_policy::policy;
    use aster_drive::types::DriverType;

    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let policy = policy::create(
        &state,
        policy::CreateStoragePolicyInput {
            name: "Bucket Guard S3".to_string(),
            connection: policy::StoragePolicyConnectionInput {
                driver_type: DriverType::S3,
                endpoint: "https://cos.ap-guangzhou.myqcloud.com".to_string(),
                bucket: "bucket-1250000000".to_string(),
                access_key: "ak".to_string(),
                secret_key: "sk".to_string(),
                base_path: "tenant/prefix".to_string(),
                remote_node_id: None,
                remote_storage_target_key: None,
                options: Default::default(),
            },
            max_file_size: 0,
            chunk_size: None,
            is_default: false,
            allowed_types: None,
            options: None,
            remote_storage_target_key: None,
            application_config: Default::default(),
        },
    )
    .await
    .unwrap();

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/admin/policies/{}/promote-s3-driver",
            policy.id
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target_driver_type": "tencent_cos",
            "endpoint": "https://cos.ap-guangzhou.myqcloud.com",
            "bucket": "other-bucket-1250000000"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyPromotionBucketChangeDenied.as_str()
    );
    assert_eq!(
        body["msg"],
        "bucket cannot be changed by S3-compatible driver promotion"
    );
}

#[actix_web::test]
async fn test_policy_promote_s3_driver_rejects_non_generic_s3_source() {
    use aster_drive::services::storage_policy::policy;
    use aster_drive::types::DriverType;

    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let base_path = format!(
        "/tmp/asterdrive-policy-promote-non-s3-{}",
        uuid::Uuid::new_v4()
    );
    std::fs::create_dir_all(&base_path).unwrap();
    let policy = policy::create(
        &state,
        policy::CreateStoragePolicyInput {
            name: "Local Source".to_string(),
            connection: policy::StoragePolicyConnectionInput {
                driver_type: DriverType::Local,
                endpoint: String::new(),
                bucket: String::new(),
                access_key: String::new(),
                secret_key: String::new(),
                base_path,
                remote_node_id: None,
                remote_storage_target_key: None,
                options: Default::default(),
            },
            max_file_size: 0,
            chunk_size: None,
            is_default: false,
            allowed_types: None,
            options: None,
            remote_storage_target_key: None,
            application_config: Default::default(),
        },
    )
    .await
    .unwrap();

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/admin/policies/{}/promote-s3-driver",
            policy.id
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target_driver_type": "tencent_cos",
            "endpoint": "https://cos.ap-guangzhou.myqcloud.com",
            "bucket": "bucket-1250000000"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyPromotionSourceUnsupported.as_str()
    );
    assert_eq!(
        body["msg"],
        "only generic S3-compatible policies can be promoted"
    );
}

#[actix_web::test]
async fn test_policy_promote_s3_driver_rejects_unsupported_target() {
    use aster_drive::services::storage_policy::policy;
    use aster_drive::types::DriverType;

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let policy = policy::create(
        &state,
        policy::CreateStoragePolicyInput {
            name: "Unsupported Target S3".to_string(),
            connection: policy::StoragePolicyConnectionInput {
                driver_type: DriverType::S3,
                endpoint: "https://s3.amazonaws.com".to_string(),
                bucket: "bucket-a".to_string(),
                access_key: "ak".to_string(),
                secret_key: "sk".to_string(),
                base_path: "tenant/prefix".to_string(),
                remote_node_id: None,
                remote_storage_target_key: None,
                options: Default::default(),
            },
            max_file_size: 0,
            chunk_size: None,
            is_default: false,
            allowed_types: None,
            options: None,
            remote_storage_target_key: None,
            application_config: Default::default(),
        },
    )
    .await
    .unwrap();

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/admin/policies/{}/promote-s3-driver",
            policy.id
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target_driver_type": "s3",
            "endpoint": "https://s3.amazonaws.com",
            "bucket": "bucket-a"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyPromotionTargetUnsupported.as_str()
    );
    assert_eq!(
        body["msg"],
        "promoting S3-compatible policy to 's3' is not supported"
    );

    let stored = aster_drive::db::repository::policy_repo::find_by_id(&db, policy.id)
        .await
        .unwrap();
    assert_eq!(stored.driver_type, DriverType::S3);
}

#[actix_web::test]
async fn test_policy_promote_s3_driver_rejects_active_upload_sessions() {
    use aster_drive::services::storage_policy::policy;
    use aster_drive::types::DriverType;

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let policy = policy::create(
        &state,
        policy::CreateStoragePolicyInput {
            name: "Active Session S3".to_string(),
            connection: policy::StoragePolicyConnectionInput {
                driver_type: DriverType::S3,
                endpoint: "https://cos.ap-guangzhou.myqcloud.com".to_string(),
                bucket: "bucket-1250000000".to_string(),
                access_key: "ak".to_string(),
                secret_key: "sk".to_string(),
                base_path: "tenant/prefix".to_string(),
                remote_node_id: None,
                remote_storage_target_key: None,
                options: Default::default(),
            },
            max_file_size: 0,
            chunk_size: None,
            is_default: false,
            allowed_types: None,
            options: None,
            remote_storage_target_key: None,
            application_config: Default::default(),
        },
    )
    .await
    .unwrap();

    let user = aster_drive::db::repository::user_repo::find_by_username(&db, "testuser")
        .await
        .unwrap()
        .expect("registered user should exist");
    let upload_id = uuid::Uuid::new_v4().to_string();
    create_policy_upload_session(
        &state,
        PolicyUploadSessionSpec {
            upload_id: &upload_id,
            policy_id: policy.id,
            user_id: user.id,
            object_temp_key: None,
            status: None,
            expires_at: None,
        },
    )
    .await;

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/admin/policies/{}/promote-s3-driver",
            policy.id
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target_driver_type": "tencent_cos",
            "endpoint": "https://cos.ap-guangzhou.myqcloud.com",
            "bucket": "bucket-1250000000"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["msg"],
        "cannot promote policy: 1 active upload session(s) still reference it"
    );

    let stored = aster_drive::db::repository::policy_repo::find_by_id(&db, policy.id)
        .await
        .unwrap();
    assert_eq!(stored.driver_type, DriverType::S3);
}

#[actix_web::test]
async fn test_policy_promote_s3_driver_ignores_expired_upload_sessions() {
    use aster_drive::services::storage_policy::policy;
    use aster_drive::types::DriverType;

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let policy = policy::create(
        &state,
        policy::CreateStoragePolicyInput {
            name: "Expired Session S3".to_string(),
            connection: policy::StoragePolicyConnectionInput {
                driver_type: DriverType::S3,
                endpoint: "https://cos.ap-guangzhou.myqcloud.com".to_string(),
                bucket: "bucket-1250000000".to_string(),
                access_key: "ak".to_string(),
                secret_key: "sk".to_string(),
                base_path: "tenant/prefix".to_string(),
                remote_node_id: None,
                remote_storage_target_key: None,
                options: Default::default(),
            },
            max_file_size: 0,
            chunk_size: None,
            is_default: false,
            allowed_types: None,
            options: None,
            remote_storage_target_key: None,
            application_config: Default::default(),
        },
    )
    .await
    .unwrap();

    let user = aster_drive::db::repository::user_repo::find_by_username(&db, "testuser")
        .await
        .unwrap()
        .expect("registered user should exist");
    let upload_id = uuid::Uuid::new_v4().to_string();
    create_policy_upload_session(
        &state,
        PolicyUploadSessionSpec {
            upload_id: &upload_id,
            policy_id: policy.id,
            user_id: user.id,
            object_temp_key: None,
            status: None,
            expires_at: Some(Utc::now() - Duration::hours(1)),
        },
    )
    .await;

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/admin/policies/{}/promote-s3-driver",
            policy.id
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target_driver_type": "tencent_cos",
            "endpoint": "https://cos.ap-guangzhou.myqcloud.com",
            "bucket": "bucket-1250000000"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let stored = aster_drive::db::repository::policy_repo::find_by_id(&db, policy.id)
        .await
        .unwrap();
    assert_eq!(stored.driver_type, DriverType::TencentCos);
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
            "driver_type": "local",
            "base_path": base_path,
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
    use aster_drive::entities::background_task;
    use aster_drive::services::task;
    use aster_drive::types::{BackgroundTaskKind, BackgroundTaskStatus};
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
            "driver_type": "local",
            "base_path": base_path,
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
    use aster_drive::entities::upload_session;
    use aster_drive::types::UploadSessionKind;
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
            "driver_type": "local",
            "base_path": base_path,
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
    session.session_kind = Set(Some(UploadSessionKind::ProviderPresignedSingle));
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
    use aster_drive::types::DriverType;

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
            connection: policy::StoragePolicyConnectionInput {
                driver_type: DriverType::Local,
                endpoint: String::new(),
                bucket: String::new(),
                access_key: String::new(),
                secret_key: String::new(),
                base_path,
                remote_node_id: None,
                remote_storage_target_key: None,
                options: Default::default(),
            },
            max_file_size: 0,
            chunk_size: None,
            is_default: false,
            allowed_types: None,
            options: None,
            remote_storage_target_key: None,
            application_config: Default::default(),
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

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Native Thumbnail Local",
            "driver_type": "local",
            "base_path": "/tmp/test-native-thumbnail-local",
            "max_file_size": 0,
            "is_default": false,
            "options": {
                "thumbnail_processor": "storage_native",
                "thumbnail_extensions": ["png", ".jpg"]
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
    assert!(
        body["msg"]
            .as_str()
            .unwrap_or_default()
            .contains("does not expose storage-native thumbnail processing")
    );
}

#[actix_web::test]
async fn test_policy_rejects_storage_native_media_metadata_for_unsupported_driver() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Native Metadata Local",
            "driver_type": "local",
            "base_path": "/tmp/test-native-metadata-local",
            "max_file_size": 0,
            "is_default": false,
            "options": {
                "storage_native_processing_enabled": true,
                "storage_native_media_metadata_enabled": true,
                "media_metadata_extensions": ["mp4"]
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
    assert!(
        body["msg"]
            .as_str()
            .unwrap_or_default()
            .contains("does not expose storage-native media metadata processing")
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
            "driver_type": "local",
            "base_path": "/tmp/test-new-default",
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
            "driver_type": "local",
            "base_path": "/tmp/test-patch-default",
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
            "driver_type": "local",
            "base_path": format!("/tmp/test-replacement-default-{}", uuid::Uuid::new_v4()),
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
    let mut active: aster_drive::entities::user::ActiveModel = model.into();
    active.policy_group_id = Set(None);
    active.updated_at = Set(chrono::Utc::now());
    active.update(state.writer_db()).await.unwrap();
    state
        .policy_snapshot
        .reload(state.writer_db())
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
        aster_drive::entities::storage_policy_group::ActiveModel {
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
        aster_drive::entities::storage_policy_group_item::ActiveModel {
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
    let mut user_active: aster_drive::entities::user::ActiveModel = user_model.into();
    user_active.policy_group_id = Set(Some(group.id));
    user_active.updated_at = Set(chrono::Utc::now());
    user_active.update(state.writer_db()).await.unwrap();

    let group_model = policy_group_repo::find_group_by_id(state.writer_db(), group.id)
        .await
        .unwrap();
    let mut group_active: aster_drive::entities::storage_policy_group::ActiveModel =
        group_model.into();
    group_active.is_enabled = Set(false);
    group_active.updated_at = Set(chrono::Utc::now());
    group_active.update(state.writer_db()).await.unwrap();

    state
        .policy_snapshot
        .reload(state.writer_db())
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
    use aster_drive::types::DriverType;
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
            connection: policy::StoragePolicyConnectionInput {
                driver_type: DriverType::Local,
                endpoint: String::new(),
                bucket: String::new(),
                access_key: String::new(),
                secret_key: String::new(),
                base_path: overflow_path.clone(),
                remote_node_id: None,
                remote_storage_target_key: None,
                options: Default::default(),
            },
            max_file_size: 0,
            chunk_size: None,
            is_default: false,
            allowed_types: None,
            options: None,
            remote_storage_target_key: None,
            application_config: Default::default(),
        },
    )
    .await
    .unwrap();

    let now = chrono::Utc::now();
    let group = policy_group_repo::create_group(
        state.writer_db(),
        aster_drive::entities::storage_policy_group::ActiveModel {
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
            aster_drive::entities::storage_policy_group_item::ActiveModel {
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
    let mut user_active: aster_drive::entities::user::ActiveModel = user_model.into();
    user_active.policy_group_id = Set(Some(group.id));
    user_active.updated_at = Set(now);
    user_active.update(state.writer_db()).await.unwrap();
    state
        .policy_snapshot
        .reload(state.writer_db())
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
            "driver_type": "local",
            "base_path": stored_base_path,
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
            "driver_type": "local",
            "base_path": temp_base_path
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
            "driver_type": "local",
            "base_path": blocked_base_path
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
            "driver_type": "local",
            "base_path": blocked_base_path,
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
            "driver_type": "one_drive",
            "base_path": "draft-root",
            "options": {
                "onedrive_cloud": "global",
                "onedrive_account_mode": "work_or_school",
                "onedrive_root_item_id": "root"
            }
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::PolicyActionUnsupported.as_str());
    assert_eq!(
        body["msg"],
        "storage policy driver 'onedrive' requires a saved storage policy with completed authorization; use the saved policy connection test after authorization"
    );
    assert!(body.get("data").is_none());
    assert_eq!(body["error"]["retryable"], false);
}

#[actix_web::test]
async fn test_tencent_cos_cors_config_rejects_invalid_inputs_with_stable_codes() {
    let state = common::setup().await;
    assert!(
        site_url::public_site_urls(state.runtime_config()).is_empty(),
        "this test expects missing public_site_url to drive the COS CORS parameter-required branch"
    );
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/action")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "configure_tencent_cos_cors",
            "driver_type": "local",
            "base_path": format!("/tmp/test-policy-cos-cors-local-{}", uuid::Uuid::new_v4())
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::PolicyActionUnsupported.as_str());

    let local_policy_id = create_local_policy_via_admin(&app, &token, "COS CORS Local").await;
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/admin/policies/{local_policy_id}/action"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "configure_tencent_cos_cors"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::PolicyActionUnsupported.as_str());

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/action")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "configure_tencent_cos_cors",
            "driver_type": "one_drive",
            "base_path": "draft-root"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], ApiErrorCode::PolicyActionUnsupported.as_str());
    assert_eq!(
        body["msg"],
        "storage policy action 'configure_tencent_cos_cors' is not supported for onedrive storage policies"
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/action")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "configure_tencent_cos_cors",
            "driver_type": "tencent_cos",
            "endpoint": "https://cos.ap-guangzhou.myqcloud.com",
            "bucket": "media-1250000000"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyStorageAccessKeyRequired.as_str()
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/action")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "configure_tencent_cos_cors",
            "driver_type": "tencent_cos",
            "endpoint": "https://cos.ap-guangzhou.myqcloud.com",
            "bucket": "media-1250000000",
            "access_key": "AKIDEXAMPLE",
            "secret_key": "SECRETEXAMPLE"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyActionParameterRequired.as_str()
    );

    let cos_policy_id = create_tencent_cos_policy_via_admin(&app, &token, "COS CORS Saved").await;
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/admin/policies/{cos_policy_id}/action"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "configure_tencent_cos_cors"
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
async fn test_tencent_cos_cors_draft_action_reuses_saved_credentials_when_blank() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let cos_policy_id =
        create_tencent_cos_policy_via_admin(&app, &token, "COS CORS Draft Reuse").await;

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/action")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "configure_tencent_cos_cors",
            "policy_id": cos_policy_id,
            "driver_type": "tencent_cos",
            "endpoint": "https://cos.ap-guangzhou.myqcloud.com",
            "bucket": "media-draft-1250000000",
            "access_key": "",
            "secret_key": ""
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyActionParameterRequired.as_str(),
        "blank draft credentials should be filled from saved policy before action-specific validation"
    );
}

#[actix_web::test]
async fn test_policy_params_reuses_saved_credentials_when_blank() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let cos_policy_id =
        create_tencent_cos_policy_via_admin(&app, &token, "COS Draft Test Reuse").await;

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/test")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "policy_id": cos_policy_id,
            "driver_type": "tencent_cos",
            "endpoint": "https://cos.ap-guangzhou.myqcloud.com",
            "bucket": "media-draft-1250000000",
            "access_key": "",
            "secret_key": "",
            "options": {
                "onedrive_account_mode": "work_or_school"
            }
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyOneDriveOptionsUnsupported.as_str(),
        "blank draft credentials should be filled from the saved policy before connector option validation"
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
}

#[actix_web::test]
async fn test_tencent_cos_cors_draft_action_rejects_saved_credential_driver_mismatch() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let local_policy_id =
        create_local_policy_via_admin(&app, &token, "COS CORS Credential Mismatch").await;

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/action")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "configure_tencent_cos_cors",
            "policy_id": local_policy_id,
            "driver_type": "tencent_cos",
            "endpoint": "https://cos.ap-guangzhou.myqcloud.com",
            "bucket": "media-draft-1250000000",
            "access_key": "",
            "secret_key": ""
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyActionParameterInvalid.as_str()
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
                "action": "configure_tencent_cos_cors"
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

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Incomplete S3",
            "driver_type": "s3",
            "endpoint": "https://s3.example.com",
            "bucket": "archive"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyStorageAccessKeyRequired.as_str()
    );
    assert_eq!(
        body["msg"],
        "access_key is required for S3-compatible storage policies"
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/test")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "driver_type": "s3",
            "endpoint": "https://s3.example.com",
            "bucket": "archive"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyStorageAccessKeyRequired.as_str()
    );
    assert_eq!(
        body["msg"],
        "access_key is required for S3-compatible storage policies"
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
            "driver_type": "s3",
            "endpoint": "https://s3.example.com",
            "access_key": "AKIA",
            "secret_key": "SECRET"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyStorageBucketRequired.as_str()
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Invalid Endpoint S3",
            "driver_type": "s3",
            "endpoint": "s3.example.com",
            "bucket": "archive",
            "access_key": "AKIA",
            "secret_key": "SECRET"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyStorageEndpointInvalid.as_str()
    );
}

#[actix_web::test]
async fn test_policy_create_rejects_remote_without_node_with_stable_code() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Remote Missing Node",
            "driver_type": "remote",
            "base_path": "remote-missing-node"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyRemoteNodeRequired.as_str()
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/test")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "driver_type": "remote",
            "base_path": "remote-missing-node"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyRemoteNodeRequired.as_str()
    );
}

#[actix_web::test]
async fn test_policy_create_rejects_remote_node_for_non_remote_policy_with_stable_code() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let base_path = format!("/tmp/test-policy-unexpected-node-{}", uuid::Uuid::new_v4());

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Local Unexpected Node",
            "driver_type": "local",
            "base_path": base_path,
            "remote_node_id": 42
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyRemoteNodeUnexpected.as_str()
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/test")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "driver_type": "local",
            "base_path": format!("/tmp/test-policy-unexpected-node-{}", uuid::Uuid::new_v4()),
            "remote_node_id": 42
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyRemoteNodeUnexpected.as_str()
    );
}

#[actix_web::test]
async fn test_policy_create_rejects_unusable_remote_nodes_with_stable_codes() {
    use aster_drive::services::remote::remote_node;
    use aster_drive::types::RemoteNodeTransportMode;

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
            "driver_type": "remote",
            "base_path": "disabled-remote",
            "remote_node_id": disabled_node.id
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
            "driver_type": "remote",
            "base_path": "direct-missing-url",
            "remote_node_id": direct_node_without_url.id
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
            "driver_type": "remote",
            "base_path": "reverse-presigned",
            "remote_node_id": reverse_node.id,
            "options": {
                "remote_upload_strategy": "presigned"
            }
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
async fn test_policy_update_rejects_clearing_existing_s3_secret_with_stable_code() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Valid S3",
            "driver_type": "s3",
            "endpoint": "https://s3.example.com",
            "bucket": "archive",
            "access_key": "AKIA",
            "secret_key": "SECRET"
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
            "secret_key": ""
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::PolicyStorageSecretKeyRequired.as_str()
    );
    assert_eq!(
        body["msg"],
        "secret_key is required for S3-compatible storage policies"
    );

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
