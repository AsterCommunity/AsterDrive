use super::super::test_support;
use super::*;
use aster_drive_storage::{ConnectorId, StoragePolicyBehaviorConfig};

mod acceptance;
mod boundaries;

/// Reuse the real connector context and encrypted repository in management
/// tests, so direct trait calls do not accidentally bypass registry validation.
struct TestContext {
    db: sea_orm::DatabaseConnection,
    policy: storage_policy::Model,
    config: crate::config::Config,
    runtime: crate::config::RuntimeConfig,
    registry: crate::storage::DriverRegistry,
}
impl TestContext {
    async fn new(values: serde_json::Value, credential: Option<GoogleDriveCredentialV1>) -> Self {
        use sea_orm::ActiveModelTrait;
        let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
        test_support::migrate_current_storage_test_schema(&db).await;
        let policy = test_support::insertable_policy(test_support::policy(
            GoogleDriveConnector::ID,
            1,
            values,
            StoragePolicyBehaviorConfig::default(),
        ))
        .insert(&db)
        .await
        .unwrap();
        let mut config = crate::config::Config::default();
        config.auth.storage_credential_secret_key = KEY.into();
        if let Some(payload) = credential {
            super::super::persist_connector_credential_payload(
                &db,
                KEY,
                policy.id,
                &ConnectorId::declared(GoogleDriveConnector::ID),
                1,
                &payload,
            )
            .await
            .unwrap();
        }
        Self {
            db,
            policy,
            config,
            runtime: crate::config::RuntimeConfig::new(),
            registry: crate::storage::DriverRegistry::noop().unwrap(),
        }
    }
    fn context(&self) -> crate::storage::connectors::StorageConnectorContext<'_> {
        crate::storage::connectors::StorageConnectorContext::new(
            &self.db,
            &self.config,
            &self.runtime,
            &self.registry,
            None,
        )
    }
}

const KEY: &str = "google-drive-test-master-key-32bytes";
pub(super) fn payload() -> GoogleDriveCredentialV1 {
    GoogleDriveCredentialV1 {
        application: GoogleDriveAuthorizationApplicationV1 {
            client_id: "client".into(),
            client_secret: "private-secret".into(),
        },
        authorization: Some(GoogleDriveAuthorizationV1 {
            access_token: "private-access".into(),
            refresh_token: Some("private-refresh".into()),
            scopes: vec!["https://www.googleapis.com/auth/drive".into()],
            status: StorageCredentialStatus::Authorized,
            status_reason: None,
            expires_at: Some(Utc::now() + chrono::Duration::hours(1)),
            authorized_at: Some(Utc::now()),
            last_refreshed_at: None,
            last_validated_at: None,
        }),
    }
}
#[test]
fn descriptor_declares_server_side_oauth_and_optional_browser_uploads() {
    let connector = GoogleDriveConnector;
    let descriptor = connector.descriptor();
    descriptor.validate().unwrap();
    connector
        .localization()
        .unwrap()
        .validate_message_ids(descriptor.localization_message_ids())
        .unwrap();
    assert_eq!(
        descriptor.authorization_provider.as_deref(),
        Some("google_drive")
    );
    assert!(!descriptor.capabilities.presigned_download);
    assert!(
        descriptor
            .upload_workflows
            .frontend_direct_provider_resumable_upload
    );
    assert!(!connector.supports_remote_storage_target());
    for status in [
        "missing",
        "authorized",
        "reauth_required",
        "permission_denied",
        "invalid",
        "revoked",
    ] {
        assert!(
            descriptor
                .credential_management
                .as_ref()
                .unwrap()
                .status_presentations
                .contains_key(status)
        );
    }
}
#[test]
fn configuration_normalizes_ids_and_rejects_conflicting_pools() {
    let config = |v| test_support::connection_config(GoogleDriveConnector::ID, 1, v);
    let normalized = GoogleDriveConnector
        .validate_connector_config(&config(
            serde_json::json!({"root_folder_id":"  folder  ","shared_drive_id":" "}),
        ))
        .unwrap();
    assert_eq!(normalized.values["root_folder_id"], "folder");
    for v in [
        serde_json::json!({"app_data_folder":true,"shared_drive_id":"drive"}),
        serde_json::json!({"app_data_folder":true,"root_folder_id":"folder"}),
        serde_json::json!({"root_folder_id":"https://example.com/folder"}),
        serde_json::json!({"unknown":true}),
    ] {
        assert!(
            GoogleDriveConnector
                .validate_connector_config(&config(v))
                .is_err()
        );
    }
}
#[test]
fn oauth_authorization_uses_offline_consent_state_and_pkce() {
    let url = authorization_url(
        "client",
        "https://drive.test/callback",
        "scope",
        "state",
        "challenge",
    )
    .unwrap();
    let url = url::Url::parse(&url).unwrap();
    let values: std::collections::BTreeMap<_, _> = url.query_pairs().into_owned().collect();
    for (key, value) in [
        ("access_type", "offline"),
        ("prompt", "consent"),
        ("state", "state"),
        ("code_challenge_method", "S256"),
        ("code_challenge", "challenge"),
    ] {
        assert_eq!(values[key], value);
    }
    assert!(!url.as_str().contains("private-secret"));
}
#[tokio::test]
async fn encrypted_credential_info_failure_states_and_runtime_revocation() {
    use sea_orm::{ActiveModelTrait, TransactionTrait};
    let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
    test_support::migrate_current_storage_test_schema(&db).await;
    let policy = test_support::policy(
        GoogleDriveConnector::ID,
        1,
        serde_json::json!({}),
        StoragePolicyBehaviorConfig::default(),
    );
    let policy = test_support::insertable_policy(policy)
        .insert(&db)
        .await
        .unwrap();
    let mut config = crate::config::Config::default();
    config.auth.storage_credential_secret_key = KEY.into();
    let mut p = payload();
    p.authorization = None;
    super::super::persist_connector_credential_payload(
        &db,
        KEY,
        policy.id,
        &ConnectorId::declared(GoogleDriveConnector::ID),
        1,
        &p,
    )
    .await
    .unwrap();
    let row = repo::find_by_policy(&db, policy.id).await.unwrap().unwrap();
    assert!(
        GoogleDriveConnector
            .credential_info(&config, &row)
            .unwrap()
            .is_none()
    );
    assert!(
        GoogleDriveConnector
            .load_runtime_credential(&db, &config, &policy, &row)
            .await
            .unwrap()
            .is_none()
    );
    super::super::persist_connector_credential_payload(
        &db,
        KEY,
        policy.id,
        &ConnectorId::declared(GoogleDriveConnector::ID),
        1,
        &payload(),
    )
    .await
    .unwrap();
    let row = repo::find_by_policy(&db, policy.id).await.unwrap().unwrap();
    assert!(!row.ciphertext.contains("private-secret"));
    let info = GoogleDriveConnector
        .credential_info(&config, &row)
        .unwrap()
        .unwrap();
    assert_eq!(info.status, StorageCredentialStatus::Authorized);
    assert!(info.authorized_at.is_some());
    for (kind, status) in [
        (
            StorageErrorKind::Auth,
            StorageCredentialStatus::ReauthRequired,
        ),
        (
            StorageErrorKind::Permission,
            StorageCredentialStatus::PermissionDenied,
        ),
        (
            StorageErrorKind::Misconfigured,
            StorageCredentialStatus::Invalid,
        ),
    ] {
        let changed = GoogleDriveConnector
            .credential_validation_failure_payload(
                &config,
                &row,
                Some(kind),
                "unsafe-provider-secret",
            )
            .unwrap()
            .unwrap();
        let changed: GoogleDriveCredentialV1 = serde_json::from_value(changed).unwrap();
        assert_eq!(changed.authorization.as_ref().unwrap().status, status);
        assert!(
            !changed
                .authorization
                .unwrap()
                .status_reason
                .unwrap()
                .contains("unsafe-provider-secret")
        );
    }
    assert!(
        GoogleDriveConnector
            .credential_validation_failure_payload(
                &config,
                &row,
                Some(StorageErrorKind::Transient),
                "timeout"
            )
            .unwrap()
            .is_none()
    );
    let provider = GoogleTokenProvider::new(
        Some(db.clone()),
        KEY.into(),
        policy.id,
        payload(),
        CredentialUse::Runtime,
    )
    .unwrap();
    assert_eq!(provider.access_token().await.unwrap(), "private-access");
    assert!(!format!("{provider:?}").contains("private-"));
    repo::delete_by_policy(&db, policy.id).await.unwrap();
    assert_eq!(
        provider.access_token().await.unwrap_err().kind(),
        StorageErrorKind::Auth
    );
    // Saving the same application preserves authorization, changing its identity removes it.
    let input = |secret: &str| {
        StorageConnectorCredentialInput::AuthorizationApplication(
            serde_json::json!({"client_id":"client","client_secret":secret}),
        )
    };
    let config_envelope =
        test_support::connection_config(GoogleDriveConnector::ID, 1, serde_json::json!({}));
    super::super::persist_connector_credential_payload(
        &db,
        KEY,
        policy.id,
        &ConnectorId::declared(GoogleDriveConnector::ID),
        1,
        &payload(),
    )
    .await
    .unwrap();
    for (secret, authorized) in [("private-secret", true), ("replacement", false)] {
        let txn = db.begin().await.unwrap();
        GoogleDriveConnector
            .persist_credential(&txn, KEY, policy.id, &config_envelope, input(secret))
            .await
            .unwrap();
        txn.commit().await.unwrap();
        let row = repo::find_by_policy(&db, policy.id).await.unwrap().unwrap();
        assert_eq!(
            decode(KEY, &row).unwrap().authorization.is_some(),
            authorized
        );
    }
}
#[test]
fn cleanup_snapshot_contains_only_bound_ciphertext() {
    let plaintext = serde_json::to_string(&payload()).unwrap();
    let ciphertext =
        crypto::encrypt_connector_credential(KEY, 7, GoogleDriveConnector::ID, 1, &plaintext)
            .unwrap();
    let snapshot = super::super::StoragePolicyCleanupDriverSnapshot::encode(
        ConnectorId::declared(GoogleDriveConnector::ID),
        1,
        &GoogleDriveCleanupSnapshot {
            credential_ciphertext: ciphertext,
        },
    )
    .unwrap();
    let serialized = serde_json::to_string(&snapshot).unwrap();
    assert!(!serialized.contains("private-"));
    let restored: GoogleDriveCleanupSnapshot =
        snapshot.decode(GoogleDriveConnector::ID, 1).unwrap();
    assert!(
        crypto::decrypt_connector_credential(
            KEY,
            8,
            GoogleDriveConnector::ID,
            1,
            &restored.credential_ciphertext
        )
        .is_err()
    );
    assert!(
        snapshot
            .decode::<GoogleDriveCleanupSnapshot>(GoogleDriveConnector::ID, 2)
            .is_err()
    );
}
