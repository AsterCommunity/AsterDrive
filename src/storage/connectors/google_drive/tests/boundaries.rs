use super::*;
use crate::storage::test_support::{HttpFixture, HttpReply, ResponseGate};
use sea_orm::TransactionTrait;
use serde_json::json;

fn input(value: serde_json::Value) -> StorageConnectorCredentialInput {
    StorageConnectorCredentialInput::AuthorizationApplication(value)
}

#[tokio::test]
async fn registry_entry_preserves_only_the_same_app_secret_and_requires_it_on_creation() {
    let context = TestContext::new(json!({}), Some(payload())).await;
    let config = test_support::connection_config(GoogleDriveConnector::ID, 1, json!({}));
    for value in [
        json!({"client_id":"client"}),
        json!({"client_id":"client","client_secret":""}),
        json!({"client_id":" client ","client_secret":"   "}),
    ] {
        let txn = context.db.begin().await.unwrap();
        crate::storage::connectors::persist_credential(
            context.registry.connectors(),
            &txn,
            KEY,
            context.policy.id,
            &config,
            input(value),
        )
        .await
        .unwrap();
        txn.commit().await.unwrap();
        let stored = decode(
            KEY,
            &repo::find_by_policy(&context.db, context.policy.id)
                .await
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(stored.application.client_secret.as_str(), "private-secret");
        assert!(stored.authorization.is_some());
        assert!(!format!("{:?}", stored.application).contains("private-secret"));
    }
    for value in [
        json!({"client_id":"different"}),
        json!({"client_id":"different","client_secret":""}),
        json!({"client_id":"","client_secret":"x"}),
    ] {
        let before = repo::find_by_policy(&context.db, context.policy.id)
            .await
            .unwrap()
            .unwrap();
        let txn = context.db.begin().await.unwrap();
        assert!(
            crate::storage::connectors::persist_credential(
                context.registry.connectors(),
                &txn,
                KEY,
                context.policy.id,
                &config,
                input(value)
            )
            .await
            .is_err()
        );
        txn.rollback().await.unwrap();
        assert_eq!(
            repo::find_by_policy(&context.db, context.policy.id)
                .await
                .unwrap()
                .unwrap()
                .ciphertext,
            before.ciphertext
        );
    }
    let txn = context.db.begin().await.unwrap();
    crate::storage::connectors::persist_credential(
        context.registry.connectors(),
        &txn,
        KEY,
        context.policy.id,
        &config,
        input(json!({"client_id":"different","client_secret":"new-secret"})),
    )
    .await
    .unwrap();
    txn.commit().await.unwrap();
    let stored = decode(
        KEY,
        &repo::find_by_policy(&context.db, context.policy.id)
            .await
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert!(stored.authorization.is_none());
    assert_eq!(stored.application.client_secret.as_str(), "new-secret");

    let context = TestContext::new(json!({}), None).await;
    let txn = context.db.begin().await.unwrap();
    assert!(
        crate::storage::connectors::persist_credential(
            context.registry.connectors(),
            &txn,
            KEY,
            context.policy.id,
            &config,
            input(json!({"client_id":"client"}))
        )
        .await
        .is_err()
    );
    txn.rollback().await.unwrap();
    assert!(
        repo::find_by_policy(&context.db, context.policy.id)
            .await
            .unwrap()
            .is_none()
    );
}

async fn flow(
    context: &TestContext,
) -> aster_drive_model::entities::storage_policy_authorization_flow::Model {
    let start = GoogleDriveConnector
        .start_authorization(
            &context.context(),
            &context.policy,
            "https://drive.test/callback",
        )
        .await
        .unwrap();
    let verifier = start.pkce_verifier.as_deref().unwrap();
    assert_eq!(verifier.len(), 43);
    let url = reqwest::Url::parse(&start.authorization_url).unwrap();
    let query: std::collections::BTreeMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(
        query["code_challenge"],
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(Sha256::digest(verifier.as_bytes()))
    );
    assert_eq!(
        query["scope"],
        scope(&GoogleDriveConnector::decode_config(&context.policy).unwrap())
    );
    assert!(!start.context.contains("private-secret"));
    assert!(!start.authorization_url.contains("private-secret"));
    let now = Utc::now();
    aster_drive_model::entities::storage_policy_authorization_flow::Model {
        id: 1,
        provider: start.provider,
        policy_id: Some(context.policy.id),
        created_by_user_id: 1,
        state_hash: crypto::token_hash(&start.state),
        pkce_verifier: start.pkce_verifier,
        redirect_uri: "https://drive.test/callback".into(),
        scopes: serde_json::to_string(&start.scopes).unwrap(),
        context: start.context,
        status: aster_drive_model::types::StorageAuthorizationFlowStatus::Consumed,
        created_at: now,
        expires_at: now + chrono::Duration::minutes(10),
        consumed_at: Some(now),
    }
}
fn token() -> serde_json::Value {
    json!({"access_token":"fresh-access","refresh_token":"fresh-refresh","expires_in":3600,"token_type":"Bearer","scope":"https://www.googleapis.com/auth/drive"})
}

#[tokio::test]
async fn exchange_rejects_authenticated_but_malformed_application_snapshot_and_disconnects() {
    let context = TestContext::new(json!({}), Some(payload())).await;
    let mut flow = flow(&context).await;
    let mut saved: GoogleDriveFlowContext = serde_json::from_str(&flow.context).unwrap();
    saved.application_ciphertext = crypto::encrypt_token(
        KEY,
        flow_aad(context.policy.id, &flow.state_hash).as_bytes(),
        "{not-json",
    )
    .unwrap();
    flow.context = serde_json::to_string(&saved).unwrap();
    let peer = HttpFixture::new([]).await;
    let endpoint = format!("{}/token", peer.base);
    let error = GoogleDriveConnector
        .complete_authorization(
            &context.context(),
            &context.policy,
            &flow,
            "code",
            Utc::now(),
            &endpoint,
        )
        .await
        .err()
        .unwrap();
    assert!(matches!(error, AsterError::DatabaseOperation(_)));
    assert!(peer.requests().is_empty());
    peer.stop().await;
    // An unreachable token endpoint must not invalidate a previously saved grant.
    let valid_flow = self::flow(&context).await;
    let before = repo::find_by_policy(&context.db, context.policy.id)
        .await
        .unwrap()
        .unwrap();
    let error = GoogleDriveConnector
        .complete_authorization(
            &context.context(),
            &context.policy,
            &valid_flow,
            "code",
            Utc::now(),
            &endpoint,
        )
        .await
        .err()
        .unwrap();
    assert_eq!(
        error.storage_error_kind(),
        Some(StorageErrorKind::Transient)
    );
    assert_eq!(
        repo::find_by_policy(&context.db, context.policy.id)
            .await
            .unwrap()
            .unwrap()
            .ciphertext,
        before.ciphertext
    );
}

#[tokio::test]
async fn real_flow_exchange_binds_pkce_scope_and_application_before_final_cas_persistence() {
    for appdata in [false, true] {
        let context = TestContext::new(json!({"app_data_folder":appdata}), Some(payload())).await;
        let flow = flow(&context).await;
        let mut response = token();
        response["scope"] = json!(if appdata {
            "https://www.googleapis.com/auth/drive.appdata"
        } else {
            "https://www.googleapis.com/auth/drive"
        });
        let peer = HttpFixture::new([HttpReply::json(200, response)]).await;
        let callback = GoogleDriveConnector
            .complete_authorization(
                &context.context(),
                &context.policy,
                &flow,
                "private-code",
                Utc::now(),
                &format!("{}/token", peer.base),
            )
            .await
            .unwrap();
        let row = repo::find_by_policy(&context.db, context.policy.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(callback.expected_credential_revision, row.revision);
        let req = &peer.requests()[0];
        let form: std::collections::BTreeMap<_, _> = url::form_urlencoded::parse(&req.body)
            .into_owned()
            .collect();
        for (key, value) in [
            ("code", "private-code"),
            ("client_id", "client"),
            ("client_secret", "private-secret"),
            ("redirect_uri", "https://drive.test/callback"),
            ("code_verifier", flow.pkce_verifier.as_deref().unwrap()),
            ("grant_type", "authorization_code"),
        ] {
            assert_eq!(form[key], value);
        }
        let txn = context.db.begin().await.unwrap();
        let saved = crate::storage::connectors::persist_authorization_callback(
            &txn,
            KEY,
            &context.policy,
            &GoogleDriveConnector.descriptor(),
            callback.credential_payload,
            callback.expected_credential_revision,
        )
        .await
        .unwrap();
        txn.commit().await.unwrap();
        assert!(!saved.ciphertext.contains("fresh-"));
        let auth = decode(KEY, &saved).unwrap().authorization.unwrap();
        assert_eq!(auth.access_token, "fresh-access");
        assert!(auth.authorized_at.is_some());
        assert_eq!(auth.status, StorageCredentialStatus::Authorized);
        peer.stop().await;
    }
}

#[tokio::test]
async fn invalid_flow_binding_and_changed_application_are_rejected_without_network_calls() {
    for variant in 0..6 {
        let context = TestContext::new(json!({}), Some(payload())).await;
        let mut flow = flow(&context).await;
        match variant {
            0 => flow.policy_id = Some(context.policy.id + 1),
            1 => flow.provider = StorageCredentialProvider::MicrosoftGraph,
            2 => flow.state_hash = "tampered".into(),
            3 => flow.pkce_verifier = None,
            4 => flow.pkce_verifier = Some("".into()),
            _ => {
                let mut changed = payload();
                changed.application.client_id = "new-client".into();
                crate::storage::connectors::persist_connector_credential_payload(
                    &context.db,
                    KEY,
                    context.policy.id,
                    &ConnectorId::declared(GoogleDriveConnector::ID),
                    1,
                    &changed,
                )
                .await
                .unwrap();
            }
        }
        let peer = HttpFixture::new([]).await;
        assert!(
            GoogleDriveConnector
                .complete_authorization(
                    &context.context(),
                    &context.policy,
                    &flow,
                    "code",
                    Utc::now(),
                    &format!("{}/token", peer.base)
                )
                .await
                .is_err()
        );
        assert!(peer.requests().is_empty());
        peer.stop().await;
    }
}

#[tokio::test]
async fn finish_authorization_distinguishes_invalid_state_from_server_failures_before_exchange() {
    use crate::storage::connectors::StorageAuthorizationFailureReason;

    // Every case must fail locally, before the real Google endpoint is used.
    for variant in 0..6 {
        let context = TestContext::new(json!({}), Some(payload())).await;
        let mut flow = flow(&context).await;
        match variant {
            0 => flow.provider = StorageCredentialProvider::MicrosoftGraph,
            1 => flow.pkce_verifier = None,
            2 => flow.pkce_verifier = Some("".into()),
            3 => flow.state_hash = "tampered".into(),
            4 => flow.context = "{".into(),
            _ => repo::delete_by_policy(&context.db, context.policy.id)
                .await
                .unwrap(),
        }
        let error = GoogleDriveConnector
            .finish_authorization(
                &context.context(),
                &context.policy,
                &flow,
                "code",
                Utc::now(),
            )
            .await
            .err()
            .unwrap();
        assert_eq!(
            error.reason(),
            if variant < 3 {
                StorageAuthorizationFailureReason::InvalidState
            } else {
                StorageAuthorizationFailureReason::ServerError
            },
            "variant {variant}: {error}"
        );
    }
}

#[test]
fn callback_error_stage_is_not_inferred_from_generic_storage_error_kinds() {
    for error in [
        AsterError::precondition_failed("precondition"),
        AsterError::unsupported_driver("unsupported"),
    ] {
        assert!(error.storage_error_kind().is_some());
        let code = error.code();
        let mapped = oauth::authorization_error(error);
        assert_eq!(
            mapped.reason(),
            crate::storage::connectors::StorageAuthorizationFailureReason::ServerError
        );
        assert_eq!(mapped.into_source().code(), code);
    }
}

#[tokio::test]
async fn exchange_rejects_missing_offline_grant_missing_scope_and_malformed_tokens() {
    for (response, kind) in [
        (
            HttpReply::json(
                200,
                json!({"access_token":"x","expires_in":3600,"token_type":"Bearer"}),
            ),
            StorageErrorKind::Auth,
        ),
        (
            HttpReply::json(
                200,
                json!({"access_token":"x","refresh_token":" ","expires_in":3600,"token_type":"Bearer"}),
            ),
            StorageErrorKind::Auth,
        ),
        (
            HttpReply::json(
                200,
                json!({"access_token":"x","refresh_token":"r","expires_in":3600,"token_type":"Bearer","scope":"unrelated"}),
            ),
            StorageErrorKind::Permission,
        ),
        (
            HttpReply::text(200, "not JSON private-secret"),
            StorageErrorKind::Transient,
        ),
        (
            HttpReply::json(
                400,
                json!({"error":"invalid_client","error_description":"private-secret"}),
            ),
            StorageErrorKind::Misconfigured,
        ),
        (
            HttpReply::text(403, "proxy private-secret"),
            StorageErrorKind::Transient,
        ),
        (HttpReply::text(503, "outage"), StorageErrorKind::Transient),
    ] {
        let context = TestContext::new(json!({}), Some(payload())).await;
        let flow = flow(&context).await;
        let before = repo::find_by_policy(&context.db, context.policy.id)
            .await
            .unwrap()
            .unwrap();
        let peer = HttpFixture::new([response]).await;
        let error = GoogleDriveConnector
            .complete_authorization(
                &context.context(),
                &context.policy,
                &flow,
                "code",
                Utc::now(),
                &format!("{}/token", peer.base),
            )
            .await
            .err()
            .unwrap();
        assert_eq!(error.storage_error_kind(), Some(kind));
        assert_eq!(
            oauth::authorization_error(error.clone()).reason(),
            crate::storage::connectors::StorageAuthorizationFailureReason::TokenExchangeFailed
        );
        assert!(!error.to_string().contains("private-secret"));
        assert_eq!(
            repo::find_by_policy(&context.db, context.policy.id)
                .await
                .unwrap()
                .unwrap()
                .ciphertext,
            before.ciphertext
        );
        peer.stop().await;
    }
}

#[tokio::test]
async fn application_changes_during_exchange_and_before_final_commit_are_both_protected() {
    for during_exchange in [true, false] {
        let context = TestContext::new(json!({}), Some(payload())).await;
        let flow = flow(&context).await;
        let gate = Arc::new(ResponseGate::default());
        let reply = HttpReply::json(200, token());
        let peer = HttpFixture::new([if during_exchange {
            reply.gated(gate.clone())
        } else {
            reply
        }])
        .await;
        let edit = async {
            if during_exchange {
                gate.entered.notified().await;
            }
            let mut changed = payload();
            changed.application.client_id = "winner-client".into();
            crate::storage::connectors::persist_connector_credential_payload(
                &context.db,
                KEY,
                context.policy.id,
                &ConnectorId::declared(GoogleDriveConnector::ID),
                1,
                &changed,
            )
            .await
            .unwrap();
            if during_exchange {
                gate.release.notify_one();
            }
        };
        let connector_context = context.context();
        let url = format!("{}/token", peer.base);
        if during_exchange {
            let (result, ()) = tokio::join!(
                GoogleDriveConnector.complete_authorization(
                    &connector_context,
                    &context.policy,
                    &flow,
                    "code",
                    Utc::now(),
                    &url
                ),
                edit
            );
            assert!(result.is_err());
        } else {
            let callback = GoogleDriveConnector
                .complete_authorization(
                    &connector_context,
                    &context.policy,
                    &flow,
                    "code",
                    Utc::now(),
                    &url,
                )
                .await
                .unwrap();
            edit.await;
            let txn = context.db.begin().await.unwrap();
            assert!(
                crate::storage::connectors::persist_authorization_callback(
                    &txn,
                    KEY,
                    &context.policy,
                    &GoogleDriveConnector.descriptor(),
                    callback.credential_payload,
                    callback.expected_credential_revision
                )
                .await
                .is_err()
            );
            txn.rollback().await.unwrap();
        }
        let row = repo::find_by_policy(&context.db, context.policy.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            decode(KEY, &row).unwrap().application.client_id,
            "winner-client"
        );
        assert_eq!(row.revision, 2);
        peer.stop().await;
    }
}

#[tokio::test]
async fn cleanup_snapshot_rebuilds_the_driver_after_the_credential_row_is_deleted() {
    let context = TestContext::new(json!({"root_folder_id":"pool"}), Some(payload())).await;
    let snapshot = GoogleDriveConnector
        .cleanup_snapshot_for_policy(&context.context(), &context.policy)
        .await
        .unwrap()
        .unwrap();
    assert!(
        !serde_json::to_string(&snapshot)
            .unwrap()
            .contains("private-")
    );
    repo::delete_by_policy(&context.db, context.policy.id)
        .await
        .unwrap();
    let snapshots = crate::storage::connectors::StoragePolicyCleanupSnapshots {
        driver_snapshot: Some(&snapshot),
    };
    let driver = GoogleDriveConnector
        .build_cleanup_driver(&context.context(), &context.policy, snapshots)
        .await
        .unwrap();
    assert!(driver.extensions().provider_resumable.is_some());
    assert!(
        GoogleDriveConnector
            .cleanup_snapshot_for_policy(&context.context(), &context.policy)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn final_authorization_persistence_rejects_pool_changes_and_deleted_credentials() {
    use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};
    for pool_changed in [true, false] {
        let context = TestContext::new(json!({}), Some(payload())).await;
        let flow = flow(&context).await;
        let peer = HttpFixture::new([HttpReply::json(200, token())]).await;
        let callback = GoogleDriveConnector
            .complete_authorization(
                &context.context(),
                &context.policy,
                &flow,
                "code",
                Utc::now(),
                &format!("{}/token", peer.base),
            )
            .await
            .unwrap();
        if pool_changed {
            let mut policy = context.policy.clone().into_active_model();
            policy.storage_config = Set(test_support::policy_config(
                GoogleDriveConnector::ID,
                1,
                json!({"app_data_folder":true}),
                StoragePolicyBehaviorConfig::default(),
            ));
            policy.update(&context.db).await.unwrap();
        } else {
            repo::delete_by_policy(&context.db, context.policy.id)
                .await
                .unwrap();
        }
        let txn = context.db.begin().await.unwrap();
        assert!(
            crate::storage::connectors::persist_authorization_callback(
                &txn,
                KEY,
                &context.policy,
                &GoogleDriveConnector.descriptor(),
                callback.credential_payload,
                callback.expected_credential_revision
            )
            .await
            .is_err()
        );
        txn.rollback().await.unwrap();
        let current = repo::find_by_policy(&context.db, context.policy.id)
            .await
            .unwrap();
        if pool_changed {
            assert_eq!(current.unwrap().revision, 1);
        } else {
            assert!(current.is_none());
        }
        peer.stop().await;
    }
}

#[tokio::test]
async fn authorization_result_rolls_back_with_the_callers_transaction() {
    let context = TestContext::new(json!({}), Some(payload())).await;
    let flow = flow(&context).await;
    let peer = HttpFixture::new([HttpReply::json(200, token())]).await;
    let callback = GoogleDriveConnector
        .complete_authorization(
            &context.context(),
            &context.policy,
            &flow,
            "code",
            Utc::now(),
            &format!("{}/token", peer.base),
        )
        .await
        .unwrap();
    let before = repo::find_by_policy(&context.db, context.policy.id)
        .await
        .unwrap()
        .unwrap();
    let txn = context.db.begin().await.unwrap();
    crate::storage::connectors::persist_authorization_callback(
        &txn,
        KEY,
        &context.policy,
        &GoogleDriveConnector.descriptor(),
        callback.credential_payload,
        callback.expected_credential_revision,
    )
    .await
    .unwrap();
    txn.rollback().await.unwrap();
    let after = repo::find_by_policy(&context.db, context.policy.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.ciphertext, before.ciphertext);
    peer.stop().await;
}

#[test]
fn upload_transfer_and_capacity_fields_are_descriptor_owned_and_backward_compatible() {
    let normalize = |values| {
        GoogleDriveConnector.validate_connector_config(&test_support::connection_config(
            GoogleDriveConnector::ID,
            1,
            values,
        ))
    };
    let defaults = normalize(json!({})).unwrap();
    assert_eq!(
        defaults.values["provider_resumable_upload_strategy"],
        "server_relay"
    );
    assert_eq!(defaults.values["capacity_probe_timeout_secs"], 10);
    for strategy in ["server_relay", "frontend_direct"] {
        let normalized = normalize(
            json!({"provider_resumable_upload_strategy":strategy,"capacity_probe_timeout_secs":17}),
        )
        .unwrap();
        let policy = test_support::policy(
            GoogleDriveConnector::ID,
            1,
            serde_json::to_value(normalized.values).unwrap(),
            StoragePolicyBehaviorConfig::default(),
        );
        let actual = GoogleDriveConnector.upload_transport(&policy).unwrap();
        assert!(
            matches!(actual,StorageConnectorUploadTransport::ProviderResumable(selected) if selected==if strategy=="server_relay" {ProviderResumableUploadStrategy::ServerRelay}else{ProviderResumableUploadStrategy::FrontendDirect})
        );
    }
    for values in [
        json!({"provider_resumable_upload_strategy":"unknown"}),
        json!({"capacity_probe_timeout_secs":1}),
        json!({"capacity_probe_timeout_secs":31}),
        json!({"capacity_probe_timeout_secs":-1}),
    ] {
        assert!(normalize(values).is_err());
    }
    let descriptor = GoogleDriveConnector.descriptor();
    assert!(descriptor.capabilities.capacity);
    assert!(
        descriptor
            .upload_workflows
            .frontend_direct_provider_resumable_upload
    );
    assert!(!descriptor.capabilities.presigned_download);
    assert!(!descriptor.capabilities.custom_download_base_url);
    assert!(
        descriptor
            .fields
            .iter()
            .all(|field| field.name != "provider_download_filename_mode"
                && field.name != "provider_download_strategy")
    );
}

#[test]
fn base_path_is_normalized_and_appdata_field_explains_reauthorization() {
    let config = GoogleDriveConnector
        .validate_connector_config(&test_support::connection_config(
            GoogleDriveConnector::ID,
            1,
            json!({"base_path":" /AsterDrive//图片/ "}),
        ))
        .unwrap();
    assert_eq!(config.values["base_path"], "AsterDrive/图片");
    for path in ["a/../b", "a\\b", "x\ny", "a/./b", "a/ /b"] {
        assert!(
            GoogleDriveConnector
                .validate_connector_config(&test_support::connection_config(
                    GoogleDriveConnector::ID,
                    1,
                    json!({"base_path":path})
                ))
                .is_err()
        );
    }
    let descriptor = GoogleDriveConnector.descriptor();
    assert!(
        descriptor
            .fields
            .iter()
            .any(|field| field.name == "base_path"
                && field.help_key.as_deref() == Some("google_drive_base_path_desc"))
    );
    assert!(
        descriptor
            .fields
            .iter()
            .any(|field| field.name == "app_data_folder"
                && field.help_key.as_deref() == Some("google_drive_app_data_folder_desc"))
    );
    GoogleDriveConnector
        .localization()
        .unwrap()
        .validate_message_ids(descriptor.localization_message_ids())
        .unwrap();
}
