//! OneDrive's real flow snapshot and shared final-write concurrency boundary.
use super::*;
use sea_orm::TransactionTrait;

struct Fixture {
    db: sea_orm::DatabaseConnection,
    policy: storage_policy::Model,
    config: crate::config::Config,
    runtime: crate::config::RuntimeConfig,
    registry: crate::storage::DriverRegistry,
}
impl Fixture {
    async fn new() -> Self {
        let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
        crate::storage::connectors::test_support::migrate_current_storage_test_schema(&db).await;
        let connector = connector_config(
            OneDriveAccountMode::Personal,
            Some("common"),
            None,
            None,
            None,
        );
        let policy = policy(connector.clone())
            .into_active_model()
            .insert(&db)
            .await
            .unwrap();
        OneDriveConnector::upsert_application_config(
            &db,
            KEY,
            policy.id,
            &connector,
            OneDriveAuthorizationApplicationV1 {
                client_id: "client-a".into(),
                client_secret: "secret-a".into(),
                scopes: None,
            },
        )
        .await
        .unwrap();
        let mut config = crate::config::Config::default();
        config.auth.storage_credential_secret_key = KEY.into();
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
    async fn flow(&self) -> aster_drive_model::entities::storage_policy_authorization_flow::Model {
        let start = OneDriveConnector
            .start_authorization(&self.context(), &self.policy, "https://drive.test/callback")
            .await
            .unwrap();
        let now = chrono::Utc::now();
        aster_drive_model::entities::storage_policy_authorization_flow::Model {
            id: 1,
            provider: start.provider,
            policy_id: Some(self.policy.id),
            created_by_user_id: 1,
            state_hash: crate::services::storage_policy::credential::crypto::token_hash(
                &start.state,
            ),
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
    async fn replace_application(&self) {
        let config = OneDriveConnector::decode_config(&self.policy).unwrap();
        OneDriveConnector::upsert_application_config(
            &self.db,
            KEY,
            self.policy.id,
            &config,
            OneDriveAuthorizationApplicationV1 {
                client_id: "client-b".into(),
                client_secret: "secret-b".into(),
                scopes: None,
            },
        )
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn callback_reports_writer_failure_without_attempting_token_exchange() {
    let fixture = Fixture::new().await;
    let flow = fixture.flow().await;
    // Drop the authoritative table after the flow snapshot is captured. A DB
    // failure must stay a server error, not become an invalid-state response.
    sea_orm::ConnectionTrait::execute_unprepared(
        &fixture.db,
        "DROP TABLE storage_policy_connector_credentials",
    )
    .await
    .unwrap();
    let error = OneDriveConnector
        .finish_authorization(
            &fixture.context(),
            &fixture.policy,
            &flow,
            "unused-code",
            chrono::Utc::now(),
        )
        .await
        .err()
        .unwrap();
    assert_eq!(
        error.reason(),
        crate::storage::connectors::StorageAuthorizationFailureReason::ServerError
    );
    assert!(matches!(
        error.into_source(),
        crate::errors::AsterError::DatabaseOperation(_)
    ));
}

#[tokio::test]
async fn authorization_start_captures_the_saved_revision_without_secret_leaks() {
    let fixture = Fixture::new().await;
    let flow = fixture.flow().await;
    let context: oauth::MicrosoftGraphFlowContext = serde_json::from_str(&flow.context).unwrap();
    let row =
        storage_policy_connector_credential_repo::find_by_policy(&fixture.db, fixture.policy.id)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(context.credential_revision, Some(row.revision));
    assert_eq!(context.client_id, "client-a");
    assert!(!flow.context.contains("secret-a"));
}

#[tokio::test]
async fn changed_deleted_and_legacy_flows_fail_before_any_token_exchange() {
    for variant in 0..4 {
        let fixture = Fixture::new().await;
        let mut flow = fixture.flow().await;
        match variant {
            0 => fixture.replace_application().await,
            1 => storage_policy_connector_credential_repo::delete_by_policy(
                &fixture.db,
                fixture.policy.id,
            )
            .await
            .unwrap(),
            2 => {
                let mut context: serde_json::Value = serde_json::from_str(&flow.context).unwrap();
                context
                    .as_object_mut()
                    .unwrap()
                    .remove("credential_revision");
                flow.context = context.to_string();
            }
            _ => flow.policy_id = Some(fixture.policy.id + 1),
        }
        // No valid authorization code or network fixture is provided: a stale
        // flow must reject before attempting provider I/O.
        let error = OneDriveConnector
            .finish_authorization(
                &fixture.context(),
                &fixture.policy,
                &flow,
                "never-send-this-code",
                chrono::Utc::now(),
            )
            .await
            .err()
            .unwrap();
        assert!(matches!(
            error.reason(),
            StorageAuthorizationFailureReason::InvalidState
                | StorageAuthorizationFailureReason::UnsupportedProvider
        ));
        if variant == 0 {
            let row = storage_policy_connector_credential_repo::find_by_policy(
                &fixture.db,
                fixture.policy.id,
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(row.revision, 2);
        }
    }
}

#[tokio::test]
async fn final_one_drive_callback_cannot_overwrite_a_newer_application_or_revive_deleted_credentials()
 {
    for deleted in [false, true] {
        let fixture = Fixture::new().await;
        let flow = fixture.flow().await;
        let context: oauth::MicrosoftGraphFlowContext =
            serde_json::from_str(&flow.context).unwrap();
        let old = storage_policy_connector_credential_repo::find_by_policy(
            &fixture.db,
            fixture.policy.id,
        )
        .await
        .unwrap()
        .unwrap();
        let payload: OneDriveCredentialV1 =
            crate::storage::connectors::decode_typed_connector_credential(
                KEY,
                &old,
                &aster_drive_storage::ConnectorId::declared(OneDriveConnector::ID),
                1,
            )
            .unwrap();
        if deleted {
            storage_policy_connector_credential_repo::delete_by_policy(
                &fixture.db,
                fixture.policy.id,
            )
            .await
            .unwrap();
        } else {
            fixture.replace_application().await;
        }
        let txn = fixture.db.begin().await.unwrap();
        assert!(
            crate::storage::connectors::persist_authorization_callback(
                &txn,
                KEY,
                &fixture.policy,
                &OneDriveConnector.descriptor(),
                serde_json::to_value(payload).unwrap(),
                context.credential_revision.unwrap()
            )
            .await
            .is_err()
        );
        txn.rollback().await.unwrap();
        let current = storage_policy_connector_credential_repo::find_by_policy(
            &fixture.db,
            fixture.policy.id,
        )
        .await
        .unwrap();
        if deleted {
            assert!(current.is_none());
        } else {
            let row = current.unwrap();
            assert_eq!(row.revision, 2);
            let payload: OneDriveCredentialV1 =
                crate::storage::connectors::decode_typed_connector_credential(
                    KEY,
                    &row,
                    &aster_drive_storage::ConnectorId::declared(OneDriveConnector::ID),
                    1,
                )
                .unwrap();
            assert_eq!(payload.application.client_id, "client-b");
        }
    }
}

#[tokio::test]
async fn current_revision_callback_participates_in_the_callers_transaction() {
    let fixture = Fixture::new().await;
    let flow = fixture.flow().await;
    let context: oauth::MicrosoftGraphFlowContext = serde_json::from_str(&flow.context).unwrap();
    let old =
        storage_policy_connector_credential_repo::find_by_policy(&fixture.db, fixture.policy.id)
            .await
            .unwrap()
            .unwrap();
    let payload: OneDriveCredentialV1 =
        crate::storage::connectors::decode_typed_connector_credential(
            KEY,
            &old,
            &aster_drive_storage::ConnectorId::declared(OneDriveConnector::ID),
            1,
        )
        .unwrap();
    let txn = fixture.db.begin().await.unwrap();
    crate::storage::connectors::persist_authorization_callback(
        &txn,
        KEY,
        &fixture.policy,
        &OneDriveConnector.descriptor(),
        serde_json::to_value(payload).unwrap(),
        context.credential_revision.unwrap(),
    )
    .await
    .unwrap();
    txn.rollback().await.unwrap();
    let current =
        storage_policy_connector_credential_repo::find_by_policy(&fixture.db, fixture.policy.id)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(current.revision, old.revision);
    assert_eq!(current.ciphertext, old.ciphertext);
}
