//! Exercise management entry points through encrypted rows and the real registry.
use super::*;
use crate::storage::connectors::{
    StoragePolicyCleanupDriverSnapshot, StoragePolicyCleanupSnapshots,
};
use serde_json::json;

#[tokio::test]
async fn runtime_factory_requires_authorization_and_reloads_revocation() {
    let context =
        TestContext::new(json!({"capacity_probe_timeout_secs": 7}), Some(payload())).await;
    assert_eq!(
        context
            .registry
            .get_driver(&context.policy)
            .err()
            .unwrap()
            .storage_error_kind(),
        Some(StorageErrorKind::Auth)
    );
    context
        .registry
        .reload_storage_policy_credentials(&context.db, &context.config)
        .await
        .unwrap();
    let driver = context.registry.get_driver(&context.policy).unwrap();
    assert!(driver.supports_efficient_range());
    assert_eq!(driver.max_single_put_size(), Some(5 * 1024 * 1024));
    let resumable = driver
        .extensions()
        .provider_resumable
        .unwrap()
        .provider_resumable_upload_capabilities();
    assert_eq!(resumable.provider, "google_drive");
    assert!(
        resumable.frontend_direct_upload
            && resumable.abort_supported
            && resumable.status_query_supported
    );
    assert!(driver.extensions().stream_upload.is_some());
    assert!(driver.extensions().multipart.is_none());
    assert_eq!(
        context
            .registry
            .connectors()
            .require_policy(&context.policy)
            .unwrap()
            .upload_transport(&context.policy)
            .unwrap(),
        StorageConnectorUploadTransport::ProviderResumable(
            ProviderResumableUploadStrategy::ServerRelay
        )
    );

    // Rebuilding with the wrong opaque credential type must fail before I/O.
    assert!(
        GoogleDriveConnector
            .build_authorized_driver(
                &context.policy,
                StorageConnectorRuntimeCredential::new(
                    ConnectorId::declared(GoogleDriveConnector::ID),
                    ()
                )
            )
            .is_err()
    );
    let draft = test_support::connection_config(GoogleDriveConnector::ID, 1, json!({}));
    assert_eq!(
        GoogleDriveConnector
            .build_driver_from_connection(
                &context.context(),
                &draft,
                &StorageConnectorCredentialInput::AuthorizationApplication(json!({}))
            )
            .await
            .err()
            .unwrap()
            .storage_error_kind(),
        Some(StorageErrorKind::Unsupported)
    );

    repo::delete_by_policy(&context.db, context.policy.id)
        .await
        .unwrap();
    context
        .registry
        .reload_storage_policy_credentials(&context.db, &context.config)
        .await
        .unwrap();
    context.registry.invalidate(context.policy.id);
    assert_eq!(
        context
            .registry
            .get_driver(&context.policy)
            .err()
            .unwrap()
            .storage_error_kind(),
        Some(StorageErrorKind::Auth)
    );
}

#[tokio::test]
async fn missing_authorization_cannot_start_a_flow_or_change_failure_state() {
    let context = TestContext::new(json!({}), None).await;
    assert!(matches!(
        GoogleDriveConnector
            .start_authorization(
                &context.context(),
                &context.policy,
                "https://drive.test/callback"
            )
            .await
            .err()
            .unwrap(),
        AsterError::ValidationError(_)
    ));
    let mut value = payload();
    value.authorization = None;
    crate::storage::connectors::persist_connector_credential_payload(
        &context.db,
        KEY,
        context.policy.id,
        &ConnectorId::declared(GoogleDriveConnector::ID),
        1,
        &value,
    )
    .await
    .unwrap();
    let row = repo::find_by_policy(&context.db, context.policy.id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        GoogleDriveConnector
            .credential_validation_failure_payload(
                &context.config,
                &row,
                Some(StorageErrorKind::Auth),
                "revoked"
            )
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn cleanup_factory_requires_a_bound_valid_snapshot_and_survives_row_deletion() {
    let context = TestContext::new(json!({}), Some(payload())).await;
    assert!(GoogleDriveConnector.cleanup_snapshot_required());
    let snapshot = GoogleDriveConnector
        .cleanup_snapshot_for_policy(&context.context(), &context.policy)
        .await
        .unwrap()
        .unwrap();
    repo::delete_by_policy(&context.db, context.policy.id)
        .await
        .unwrap();
    let driver = GoogleDriveConnector
        .build_cleanup_driver(
            &context.context(),
            &context.policy,
            StoragePolicyCleanupSnapshots {
                driver_snapshot: Some(&snapshot),
            },
        )
        .await
        .unwrap();
    assert!(driver.extensions().stream_upload.is_some());
    assert!(
        GoogleDriveConnector
            .build_cleanup_driver(
                &context.context(),
                &context.policy,
                StoragePolicyCleanupSnapshots {
                    driver_snapshot: None
                }
            )
            .await
            .is_err()
    );
    // Authenticated encryption alone does not prove that the decrypted schema
    // is valid. This also rejects ciphertext copied from another policy.
    for (policy_id, plaintext) in [
        (context.policy.id, "{not-json"),
        (context.policy.id + 1, "{}"),
    ] {
        let invalid = StoragePolicyCleanupDriverSnapshot::encode(
            ConnectorId::declared(GoogleDriveConnector::ID),
            1,
            &GoogleDriveCleanupSnapshot {
                credential_ciphertext: crypto::encrypt_connector_credential(
                    KEY,
                    policy_id,
                    GoogleDriveConnector::ID,
                    1,
                    plaintext,
                )
                .unwrap(),
            },
        )
        .unwrap();
        assert!(
            GoogleDriveConnector
                .build_cleanup_driver(
                    &context.context(),
                    &context.policy,
                    StoragePolicyCleanupSnapshots {
                        driver_snapshot: Some(&invalid)
                    }
                )
                .await
                .is_err()
        );
    }
    assert!(
        repo::find_by_policy(&context.db, context.policy.id)
            .await
            .unwrap()
            .is_none()
    );
}
