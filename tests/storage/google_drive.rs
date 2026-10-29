//! Exercise Google Drive through REST guards, policy transactions and the real
//! repository. The common fixture selects SQLite/PostgreSQL/MySQL via the normal
//! ASTER_TEST_DATABASE_BACKEND matrix; these tests never call Google.
use crate::common;
use actix_web::test;
use aster_drive::db::repository::storage_policy_connector_credential_repo as credentials;
use serde_json::{Value, json};

fn connection(secret: Option<&str>) -> Value {
    let mut credential = json!({"client_id":"google-test-client"});
    if let Some(secret) = secret {
        credential["client_secret"] = json!(secret);
    }
    json!({
        "connector_config":{"format_version":1,"connector_id":"asterdrive.storage.google_drive","schema_version":1,"values":{"root_folder_id":"test-pool"}},
        "behavior":{},"credential":{"mode":"authorization_application","values":credential}
    })
}

#[actix_web::test]
async fn google_drive_admin_api_preserves_omitted_secret_replaces_it_and_rolls_back_invalid_edits()
{
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let req=test::TestRequest::post().uri("/api/v1/admin/policies")
        .insert_header(("Cookie",common::access_cookie_header(&token))).insert_header(common::csrf_header_for(&token))
        .set_json(json!({"name":"Google pool","connection":connection(Some("private-initial-secret")),"max_file_size":0,"is_default":false})).to_request();
    let response = test::call_service(&app, req).await;
    assert_eq!(response.status(), 201);
    let body: Value = test::read_body_json(response).await;
    assert!(!body.to_string().contains("private-initial-secret"));
    let id = body["data"]["id"].as_i64().unwrap();
    let first = credentials::find_by_policy(&db, id).await.unwrap().unwrap();
    assert!(!first.ciphertext.contains("private-initial-secret"));
    for (secret, name) in [
        (None, "Preserved"),
        (Some(""), "Blank preserved"),
        (Some("private-replacement-secret"), "Replaced"),
    ] {
        let req = test::TestRequest::patch()
            .uri(&format!("/api/v1/admin/policies/{id}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(json!({"name":name,"credential":connection(secret)["credential"]}))
            .to_request();
        let response = test::call_service(&app, req).await;
        assert_eq!(response.status(), 200);
        let body: Value = test::read_body_json(response).await;
        assert!(!body.to_string().contains("private-"));
    }
    let before = credentials::find_by_policy(&db, id).await.unwrap().unwrap();
    assert_eq!(before.revision, first.revision + 3);
    let req=test::TestRequest::patch().uri(&format!("/api/v1/admin/policies/{id}"))
        .insert_header(("Cookie",common::access_cookie_header(&token))).insert_header(common::csrf_header_for(&token))
        .set_json(json!({"name":"Must roll back","credential":{"mode":"authorization_application","values":{"client_id":"other-client"}}})).to_request();
    let response = test::call_service(&app, req).await;
    assert_eq!(response.status(), 400);
    let after = credentials::find_by_policy(&db, id).await.unwrap().unwrap();
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.ciphertext, before.ciphertext);
    let policy = aster_drive::db::repository::policy_repo::find_by_id(&db, id)
        .await
        .unwrap();
    assert_eq!(policy.name, "Replaced");
}

#[actix_web::test]
async fn google_drive_api_descriptor_and_rejected_creation_keep_management_boundaries() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies/storage-drivers")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .to_request();
    let body: Value = test::call_and_read_body_json(&app, req).await;
    let google = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["connector_id"] == "asterdrive.storage.google_drive")
        .unwrap();
    assert_eq!(google["authorization_provider"], "google_drive");
    assert_eq!(google["credential_mode"], "oauth_delegated");
    assert_eq!(google["supports_initial_setup"], false);
    assert_eq!(
        google["upload_workflows"]["frontend_direct_provider_resumable_upload"],
        true
    );
    assert_eq!(google["capabilities"]["presigned_download"], false);
    assert_eq!(google["capabilities"]["remote_node_binding"], false);
    let before = aster_drive::db::repository::policy_repo::find_all(&db)
        .await
        .unwrap()
        .len();
    for (values, secret) in [
        (json!({"root_folder_id":"test-pool"}), None),
        (
            json!({"root_folder_id":"pool","app_data_folder":true}),
            Some("private-secret"),
        ),
    ] {
        let mut input = connection(secret);
        input["connector_config"]["values"] = values;
        let req=test::TestRequest::post().uri("/api/v1/admin/policies")
            .insert_header(("Cookie",common::access_cookie_header(&token))).insert_header(common::csrf_header_for(&token))
            .set_json(json!({"name":"Rejected Google pool","connection":input,"max_file_size":0,"is_default":false})).to_request();
        let response = test::call_service(&app, req).await;
        assert_eq!(response.status(), 400);
        let body: Value = test::read_body_json(response).await;
        assert!(!body.to_string().contains("private-secret"));
        assert_eq!(
            aster_drive::db::repository::policy_repo::find_all(&db)
                .await
                .unwrap()
                .len(),
            before
        );
    }
}

#[derive(Default)]
struct GoogleSessionProbe {
    protocol_version: std::sync::atomic::AtomicU32,
    pause: std::sync::atomic::AtomicBool,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
    object_size: std::sync::Mutex<Option<u64>>,
    offset: std::sync::atomic::AtomicU64,
    total: u64,
    aborts: std::sync::atomic::AtomicUsize,
    deletes: std::sync::atomic::AtomicUsize,
}
#[async_trait::async_trait]
impl aster_drive_storage::StorageDriver for GoogleSessionProbe {
    async fn put(&self, _: &str, _: &[u8]) -> aster_drive_storage::Result<String> {
        unreachable!("test simulates browser bytes")
    }
    async fn get(&self, _: &str) -> aster_drive_storage::Result<Vec<u8>> {
        unreachable!("finalization only inspects metadata")
    }
    async fn get_stream(
        &self,
        _: &str,
    ) -> aster_drive_storage::Result<Box<dyn tokio::io::AsyncRead + Unpin + Send>> {
        unreachable!("finalization only inspects metadata")
    }
    async fn delete(&self, _: &str) -> aster_drive_storage::Result<()> {
        self.deletes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        *self.object_size.lock().unwrap() = None;
        Ok(())
    }
    async fn exists(&self, _: &str) -> aster_drive_storage::Result<bool> {
        Ok(self.object_size.lock().unwrap().is_some())
    }
    async fn metadata(
        &self,
        _: &str,
    ) -> aster_drive_storage::Result<aster_drive_storage::BlobMetadata> {
        self.object_size
            .lock()
            .unwrap()
            .map(|size| aster_drive_storage::BlobMetadata {
                size,
                content_type: None,
            })
            .ok_or_else(|| {
                aster_drive_storage::storage_driver_error(
                    aster_drive_storage::StorageErrorKind::NotFound,
                    "not yet committed",
                )
            })
    }
    fn max_single_put_size(&self) -> Option<u64> {
        Some(5 * 1024 * 1024)
    }
    fn extensions(&self) -> aster_drive_storage::StorageDriverExtensions<'_> {
        aster_drive_storage::StorageDriverExtensions {
            provider_resumable: Some(self),
            ..Default::default()
        }
    }
}
#[async_trait::async_trait]
impl aster_drive_storage::ProviderResumableUploadDriver for GoogleSessionProbe {
    fn provider_resumable_upload_capabilities(
        &self,
    ) -> aster_drive_storage::ProviderResumableUploadCapabilities {
        aster_drive_storage::ProviderResumableUploadCapabilities {
            browser_upload_protocol: Some(aster_drive_storage::BrowserUploadDriver::protocol(self)),
            provider: "google_drive",
            session_label: "Google Drive resumable upload",
            min_fragment_size: 256 * 1024,
            default_fragment_size: 8 * 1024 * 1024,
            max_fragment_size: 8 * 1024 * 1024,
            fragment_alignment: 256 * 1024,
            max_simple_upload_size: Some(5 * 1024 * 1024),
            frontend_direct_upload: true,
            implicit_completion: true,
            abort_supported: true,
            status_query_supported: true,
        }
    }
    fn browser_upload(&self) -> Option<&dyn aster_drive_storage::BrowserUploadDriver> {
        Some(self)
    }
    async fn create_upload_session(
        &self,
        _: &str,
    ) -> aster_drive_storage::Result<aster_drive_storage::ProviderResumableUploadSession> {
        Ok(aster_drive_storage::ProviderResumableUploadSession {
            upload_url:
                "https://www.googleapis.com/upload/drive/v3/files?upload_id=private-session".into(),
            expires_at: None,
            next_expected_ranges: vec!["0-".into()],
        })
    }
    async fn query_upload_session(
        &self,
        _: &str,
    ) -> aster_drive_storage::Result<aster_drive_storage::ProviderResumableUploadStatus> {
        let offset = self.offset.load(std::sync::atomic::Ordering::SeqCst);
        Ok(aster_drive_storage::ProviderResumableUploadStatus {
            expires_at: None,
            next_expected_ranges: if offset == self.total {
                vec![]
            } else {
                vec![format!("{offset}-")]
            },
        })
    }
    async fn abort_upload_session(&self, _: &str) -> aster_drive_storage::Result<()> {
        self.aborts
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
    async fn upload_session_fragment_reader(
        &self,
        _: &str,
        _: u64,
        _: u64,
        _: Box<dyn tokio::io::AsyncRead + Unpin + Send + Sync>,
        _: i64,
    ) -> aster_drive_storage::Result<aster_drive_storage::ProviderResumableUploadFragmentOutcome>
    {
        unreachable!("browser writes directly to provider")
    }
}

async fn google_upload_fixture(
    strategy: &str,
    total: u64,
) -> (
    aster_drive::runtime::PrimaryAppState,
    i64,
    std::sync::Arc<GoogleSessionProbe>,
) {
    use sea_orm::{ActiveModelTrait, Set};
    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "google-upload",
        "google-upload@test.com",
        "password123",
    )
    .await
    .unwrap();
    let policy = aster_drive::db::repository::policy_repo::find_default(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    let id = policy.id;
    let mut active: aster_drive_model::entities::storage_policy::ActiveModel = policy.into();
    active.connector_id = Set("asterdrive.storage.google_drive".into());
    active.storage_config = Set(common::encoded_policy_config(
        "asterdrive.storage.google_drive",
        json!({"root_folder_id":"pool","provider_resumable_upload_strategy":strategy}),
        aster_drive_storage::StoragePolicyBehaviorConfig::default(),
    ));
    active.update(state.writer_db()).await.unwrap();
    state
        .driver_registry
        .reload_policy_snapshot(&state.policy_snapshot, state.writer_db())
        .await
        .unwrap();
    let driver = std::sync::Arc::new(GoogleSessionProbe {
        total,
        ..Default::default()
    });
    state.driver_registry.insert_for_test(id, driver.clone());
    (state, user.id, driver)
}

#[actix_web::test]
async fn google_drive_dual_upload_strategies_encrypt_sessions_and_only_direct_returns_capability_url()
 {
    use aster_drive::services::files::upload;
    for strategy in ["server_relay", "frontend_direct"] {
        let total = 8 * 1024 * 1024 + 1;
        let (state, user, driver) = google_upload_fixture(strategy, total).await;
        let response = upload::init_upload(
            &state,
            user,
            "file.bin",
            i64::try_from(total).unwrap(),
            None,
            None,
        )
        .await
        .unwrap();
        let direct = strategy == "frontend_direct";
        assert_eq!(
            response.mode,
            if direct {
                aster_drive_model::types::UploadTransport::ProviderResumable
            } else {
                aster_drive_model::types::UploadTransport::Chunked
            }
        );
        assert_eq!(response.provider_resumable.is_some(), direct);
        let id = response.upload_id.unwrap();
        let row =
            aster_drive::db::repository::upload_session_repo::find_by_id(state.writer_db(), &id)
                .await
                .unwrap();
        assert!(
            row.provider_session_ciphertext
                .as_deref()
                .is_some_and(|value| !value.contains("private-session"))
        );
        driver
            .offset
            .store(256 * 1024, std::sync::atomic::Ordering::SeqCst);
        let progress = upload::get_progress(&state, &id, user).await.unwrap();
        assert_eq!(progress.provider_resumable.is_some(), direct);
        if direct {
            assert_eq!(
                progress
                    .provider_resumable
                    .unwrap()
                    .browser_upload
                    .unwrap()
                    .confirmed_offset,
                256 * 1024
            );
        }
        upload::cancel_upload(&state, &id, user).await.unwrap();
        assert!(driver.aborts.load(std::sync::atomic::Ordering::SeqCst) > 0);
        assert!(driver.deletes.load(std::sync::atomic::Ordering::SeqCst) > 0);
    }
}

#[actix_web::test]
async fn google_drive_browser_completion_cannot_publish_missing_or_wrong_sized_objects() {
    use aster_drive::services::files::upload;
    for size in [None, Some(3), Some(4)] {
        let (state, user, driver) = google_upload_fixture("frontend_direct", 4).await;
        let response = upload::init_upload(&state, user, "file.bin", 4, None, None)
            .await
            .unwrap();
        let id = response.upload_id.unwrap();
        *driver.object_size.lock().unwrap() = size;
        driver.offset.store(4, std::sync::atomic::Ordering::SeqCst);
        let result = upload::complete_upload(&state, &id, user, None).await;
        let saved = aster_drive::db::repository::user_repo::find_by_id(state.writer_db(), user)
            .await
            .unwrap();
        if size == Some(4) {
            let file = result.unwrap();
            assert_eq!(file.size, 4);
            assert_eq!(saved.storage_used, 4);
        } else {
            assert!(result.is_err());
            assert_eq!(saved.storage_used, 0);
            assert!(
                aster_drive::db::repository::file_repo::find_by_name_in_folder(
                    state.writer_db(),
                    user,
                    None,
                    "file.bin"
                )
                .await
                .unwrap()
                .is_none()
            );
        }
    }
}

#[actix_web::test]
async fn google_drive_location_changes_cannot_hide_blobs_or_unfinished_cleanup() {
    use aster_drive::services::{files::upload, storage_policy::policy};
    use sea_orm::{ActiveModelTrait, Set};
    for kind in ["blob", "upload", "expired", "failed"] {
        let (state, user, driver) = google_upload_fixture("frontend_direct", 4).await;
        let init = upload::init_upload(&state, user, "retained.bin", 4, None, None)
            .await
            .unwrap();
        let id = init.upload_id.unwrap();
        let session =
            aster_drive::db::repository::upload_session_repo::find_by_id(state.writer_db(), &id)
                .await
                .unwrap();
        let policy_id = session.policy_id;
        if kind == "blob" {
            *driver.object_size.lock().unwrap() = Some(4);
            driver.offset.store(4, std::sync::atomic::Ordering::SeqCst);
            upload::complete_upload(&state, &id, user, None)
                .await
                .unwrap();
        } else if kind == "expired" || kind == "failed" {
            let mut active: aster_drive_model::entities::upload_session::ActiveModel =
                session.into();
            if kind == "failed" {
                active.status = Set(aster_drive_model::types::UploadSessionStatus::Failed);
            }
            active.expires_at = Set(chrono::Utc::now() - chrono::Duration::minutes(1));
            active.update(state.writer_db()).await.unwrap();
        }
        let original =
            aster_drive::db::repository::policy_repo::find_by_id(state.writer_db(), policy_id)
                .await
                .unwrap();
        for values in [
            json!({"root_folder_id":"other"}),
            json!({"shared_drive_id":"other"}),
            json!({"app_data_folder":true}),
            json!({"root_folder_id":"pool","base_path":"different"}),
        ] {
            let candidate=serde_json::from_value(json!({"format_version":1,"connector_id":"asterdrive.storage.google_drive","schema_version":1,"values":values})).unwrap();
            let error = policy::update(
                &state,
                policy_id,
                policy::UpdateStoragePolicyInput {
                    name: Some("must roll back".into()),
                    connector_config: Some(candidate),
                    ..Default::default()
                },
            )
            .await
            .unwrap_err();
            assert!(
                error.to_string().contains("storage location cannot change"),
                "{kind}: {error}"
            );
            let after =
                aster_drive::db::repository::policy_repo::find_by_id(state.writer_db(), policy_id)
                    .await
                    .unwrap();
            assert_eq!(after.storage_config, original.storage_config);
            assert_eq!(after.name, original.name);
        }
        // Transport changes do not relocate existing objects and remain allowed.
        let candidate=serde_json::from_value(json!({"format_version":1,"connector_id":"asterdrive.storage.google_drive","schema_version":1,"values":{"root_folder_id":"pool","provider_resumable_upload_strategy":"server_relay"}})).unwrap();
        policy::update(
            &state,
            policy_id,
            policy::UpdateStoragePolicyInput {
                connector_config: Some(candidate),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }
}

#[actix_web::test]
async fn google_drive_empty_policy_can_select_a_base_path_before_use() {
    use aster_drive::services::storage_policy::policy;
    let (state, _, _) = google_upload_fixture("frontend_direct", 4).await;
    let original = aster_drive::db::repository::policy_repo::find_default(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    let candidate=serde_json::from_value(json!({"format_version":1,"connector_id":"asterdrive.storage.google_drive","schema_version":1,"values":{"root_folder_id":"pool","base_path":" /AsterDrive//blobs/ "}})).unwrap();
    let updated = policy::update(
        &state,
        original.id,
        policy::UpdateStoragePolicyInput {
            connector_config: Some(candidate),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        updated.connector_config.values["base_path"],
        "AsterDrive/blobs"
    );
}

#[actix_web::test]
async fn google_drive_permanent_purge_deletes_provider_object_before_discarding_blob_record() {
    use aster_drive::services::files::{file, upload};
    let (state, user, driver) = google_upload_fixture("frontend_direct", 4).await;
    let init = upload::init_upload(&state, user, "purge.bin", 4, None, None)
        .await
        .unwrap();
    *driver.object_size.lock().unwrap() = Some(4);
    driver.offset.store(4, std::sync::atomic::Ordering::SeqCst);
    let file = upload::complete_upload(&state, &init.upload_id.unwrap(), user, None)
        .await
        .unwrap();
    let record = aster_drive::db::repository::file_repo::find_by_id(state.writer_db(), file.id)
        .await
        .unwrap();
    file::purge(&state, file.id, user).await.unwrap();
    assert!(driver.object_size.lock().unwrap().is_none());
    assert!(driver.deletes.load(std::sync::atomic::Ordering::SeqCst) > 0);
    assert!(
        aster_drive::db::repository::file_repo::find_blob_by_id(state.writer_db(), record.blob_id)
            .await
            .is_err()
    );
}

// A third, deliberately different wire protocol proves the coordination layer is extensible.
#[async_trait::async_trait]
impl aster_drive_storage::BrowserUploadDriver for GoogleSessionProbe {
    fn protocol(&self) -> aster_drive_storage::BrowserUploadProtocol {
        aster_drive_storage::BrowserUploadProtocol {
            id: "test.plugin.upload".into(),
            version: self
                .protocol_version
                .load(std::sync::atomic::Ordering::SeqCst)
                .max(7),
        }
    }
    async fn advance(
        &self,
        context: aster_drive_storage::BrowserUploadContext<'_>,
        private_state: Value,
        receipt: Option<aster_drive_storage::BrowserUploadReceipt<'_>>,
    ) -> aster_drive_storage::Result<aster_drive_storage::BrowserUploadTransition> {
        use aster_drive_storage::*;
        if self.pause.swap(false, std::sync::atomic::Ordering::SeqCst) {
            self.entered.notify_one();
            self.release.notified().await;
        }
        let offset = self.offset.load(std::sync::atomic::Ordering::SeqCst);
        if let Some(receipt) = receipt {
            // This plugin uses a custom response header with PATCH, not either built-in format.
            if receipt.observation.status == 299 {
                assert_eq!(
                    receipt
                        .observation
                        .headers
                        .get("x-test-accepted")
                        .map(String::as_str),
                    Some("plugin-receipt")
                );
                assert!(private_state.get("round").is_some());
            }
        }
        let request = (offset < context.total_size).then(|| BrowserUploadRequest {
            url: context.upload_url.into(),
            method: BrowserUploadMethod::Patch,
            headers: std::collections::BTreeMap::from([("X-Test-Protocol".into(), "seven".into())]),
            body: Some(BrowserUploadRange {
                start: offset,
                end: (offset + context.fragment_size).min(context.total_size),
            }),
            response_headers: vec!["x-test-accepted".into()],
        });
        Ok(BrowserUploadTransition {
            confirmed_offset: offset,
            provider_completed: offset == context.total_size,
            request,
            private_state: json!({"round":private_state["round"].as_u64().unwrap_or(0)+1}),
        })
    }
}

#[actix_web::test]
async fn browser_coordination_supports_a_third_protocol_partial_progress_restore_and_replayed_reports()
 {
    use aster_drive::services::files::upload::{self, BrowserUploadReport};
    use aster_drive_storage::BrowserUploadObservation;
    use std::{collections::BTreeMap, sync::atomic::Ordering};
    let (state, user, driver) = google_upload_fixture("frontend_direct", 8).await;
    let init = upload::init_upload(&state, user, "plugin.bin", 8, None, None)
        .await
        .unwrap();
    let init_step = init.provider_resumable.unwrap().browser_upload.unwrap();
    assert_eq!(init_step.protocol.id, "test.plugin.upload");
    assert_eq!(init_step.protocol.version, 7);
    let id = init.upload_id.unwrap();
    let step = upload::coordinate_browser_upload(
        &state,
        &id,
        user,
        BrowserUploadReport {
            request_id: None,
            observation: None,
        },
    )
    .await
    .unwrap();
    let instruction = step.instruction.unwrap();
    assert_eq!(
        instruction.request.method,
        aster_drive_storage::BrowserUploadMethod::Patch
    );
    assert_eq!(instruction.request.body.unwrap().start, 0);
    driver.offset.store(2, Ordering::SeqCst);
    let report = BrowserUploadReport {
        request_id: Some(instruction.request_id),
        observation: Some(BrowserUploadObservation {
            status: 299,
            headers: BTreeMap::from([
                ("X-Test-Accepted".into(), "plugin-receipt".into()),
                ("X-Unrequested".into(), "ignored".into()),
            ]),
            body: "opaque plugin body".into(),
            body_truncated: false,
        }),
    };
    let next = upload::coordinate_browser_upload(&state, &id, user, report.clone())
        .await
        .unwrap();
    assert_eq!(next.confirmed_offset, 2);
    assert!(!next.provider_completed);
    // A response-loss retry of an old ID reconciles, rather than applying its observation twice.
    let replay = upload::coordinate_browser_upload(&state, &id, user, report)
        .await
        .unwrap();
    assert_eq!(replay.confirmed_offset, 2);
    let restored = upload::list_recoverable_sessions(&state, user, None)
        .await
        .unwrap();
    let restored = restored.iter().find(|s| s.upload_id == id).unwrap();
    assert!(
        restored.chunks_on_disk.is_empty(),
        "partially received chunk cannot be skipped"
    );
    assert_eq!(
        restored
            .provider_resumable
            .as_ref()
            .unwrap()
            .browser_upload
            .as_ref()
            .unwrap()
            .confirmed_offset,
        2
    );
    driver.offset.store(8, Ordering::SeqCst);
    let done = upload::coordinate_browser_upload(
        &state,
        &id,
        user,
        BrowserUploadReport {
            request_id: None,
            observation: None,
        },
    )
    .await
    .unwrap();
    assert!(done.provider_completed);
    assert!(done.instruction.is_none());
    // Browser/provider completion is insufficient to publish a missing object.
    assert!(
        upload::complete_upload(&state, &id, user, None)
            .await
            .is_err()
    );
}

#[actix_web::test]
async fn browser_coordination_checks_owner_scope_expiry_terminal_status_and_report_bounds() {
    use aster_drive::services::files::upload::{self, BrowserUploadReport};
    use sea_orm::{ActiveModelTrait, Set};
    let (state, user, _) = google_upload_fixture("frontend_direct", 8).await;
    let id = upload::init_upload(&state, user, "file.bin", 8, None, None)
        .await
        .unwrap()
        .upload_id
        .unwrap();
    let report = || BrowserUploadReport {
        request_id: None,
        observation: None,
    };
    assert!(
        upload::coordinate_browser_upload(&state, &id, user + 1, report())
            .await
            .is_err()
    );
    assert!(
        upload::coordinate_browser_upload_for_team(&state, 999, &id, user, report())
            .await
            .is_err()
    );
    let mut row: aster_drive_model::entities::upload_session::ActiveModel =
        aster_drive::db::repository::upload_session_repo::find_by_id(state.writer_db(), &id)
            .await
            .unwrap()
            .into();
    row.expires_at = Set(chrono::Utc::now() - chrono::Duration::seconds(1));
    row.update(state.writer_db()).await.unwrap();
    let error = upload::coordinate_browser_upload(&state, &id, user, report())
        .await
        .err()
        .unwrap();
    assert_eq!(
        error.api_error_code(),
        aster_drive::api::api_error_code::ApiErrorCode::UploadSessionExpired
    );
}

#[actix_web::test]
async fn browser_protocol_snapshots_reject_unknown_versions_and_cancellation_wins_inflight_coordination()
 {
    use aster_drive::services::files::upload::{self, BrowserUploadReport};
    use std::sync::atomic::Ordering;
    let (state, user, driver) = google_upload_fixture("frontend_direct", 8).await;
    let id = upload::init_upload(&state, user, "file.bin", 8, None, None)
        .await
        .unwrap()
        .upload_id
        .unwrap();
    let report = || BrowserUploadReport {
        request_id: None,
        observation: None,
    };
    driver.protocol_version.store(8, Ordering::SeqCst);
    let error = upload::coordinate_browser_upload(&state, &id, user, report())
        .await
        .err()
        .unwrap();
    assert!(error.message().contains("protocol is unsupported"));
    driver.protocol_version.store(7, Ordering::SeqCst);
    driver.pause.store(true, Ordering::SeqCst);
    let (result, ()) = tokio::join!(
        upload::coordinate_browser_upload(&state, &id, user, report()),
        async {
            driver.entered.notified().await;
            upload::cancel_upload(&state, &id, user).await.unwrap();
            driver.release.notify_one();
        },
    );
    assert!(
        result.is_err(),
        "no instruction can be issued after cancellation deleted the session"
    );
    assert!(
        aster_drive::db::repository::upload_session_repo::find_by_id(state.writer_db(), &id)
            .await
            .is_err()
    );
}
