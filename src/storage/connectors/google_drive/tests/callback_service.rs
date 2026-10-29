//! Run the actual OAuth orchestration with only the provider endpoint replaced.
use super::*;
use crate::storage::connectors::{
    StorageConnectorAuthorizationError, StorageConnectorContext, StorageConnectorDriver,
    StorageConnectorRegistry,
};
use crate::storage::test_support::{HttpFixture, HttpReply, ResponseGate};
use sea_orm::{ActiveModelTrait, Set};
use serde_json::json;

// The real connector still owns flow validation, PKCE, token parsing, encrypted
// payloads and runtime construction. Injection changes only the HTTP endpoint.
struct LocalOAuthEndpoint(String);
#[async_trait]
impl StorageConnector for LocalOAuthEndpoint {
    fn descriptor(&self) -> StorageConnectorDescriptor {
        GoogleDriveConnector.descriptor()
    }
    fn localization(&self) -> Result<aster_drive_storage::StorageConnectorLocalization> {
        GoogleDriveConnector.localization()
    }
    async fn build_driver_from_connection(
        &self,
        context: &StorageConnectorContext<'_>,
        config: &aster_drive_storage::ConnectorConfigEnvelope,
        credential: &StorageConnectorCredentialInput,
    ) -> Result<Box<dyn StorageDriver>> {
        GoogleDriveConnector
            .build_driver_from_connection(context, config, credential)
            .await
    }
    fn build_runtime_driver(
        &self,
        registry: &crate::storage::DriverRegistry,
        policy: &storage_policy::Model,
    ) -> Result<StorageConnectorDriver> {
        GoogleDriveConnector.build_runtime_driver(registry, policy)
    }
    fn upload_transport(
        &self,
        policy: &storage_policy::Model,
    ) -> Result<StorageConnectorUploadTransport> {
        GoogleDriveConnector.upload_transport(policy)
    }
    async fn start_authorization(
        &self,
        context: &StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        redirect_uri: &str,
    ) -> Result<StorageConnectorAuthorizationStart> {
        GoogleDriveConnector
            .start_authorization(context, policy, redirect_uri)
            .await
    }
    async fn finish_authorization(
        &self,
        context: &StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        flow: &aster_drive_model::entities::storage_policy_authorization_flow::Model,
        code: &str,
        now: chrono::DateTime<Utc>,
    ) -> std::result::Result<
        StorageConnectorAuthorizationCallback,
        StorageConnectorAuthorizationError,
    > {
        GoogleDriveConnector
            .complete_authorization(context, policy, flow, code, now, &self.0)
            .await
            .map_err(oauth::authorization_error)
    }
    async fn load_runtime_credential(
        &self,
        db: &sea_orm::DatabaseConnection,
        config: &crate::config::Config,
        policy: &storage_policy::Model,
        credential: &storage_policy_connector_credential::Model,
    ) -> Result<Option<StorageConnectorRuntimeCredential>> {
        GoogleDriveConnector
            .load_runtime_credential(db, config, policy, credential)
            .await
    }
    fn credential_info(
        &self,
        config: &crate::config::Config,
        credential: &storage_policy_connector_credential::Model,
    ) -> Result<Option<crate::storage::connectors::StorageConnectorCredentialInfo>> {
        GoogleDriveConnector.credential_info(config, credential)
    }
}

#[tokio::test]
async fn service_callback_commits_credentials_once_and_reloads_the_runtime() {
    use crate::services::storage_policy::credential::{
        StorageAuthorizationCallbackQuery, finish_authorization_callback, start_authorization,
    };
    use aster_drive_model::types::StorageAuthorizationFlowStatus;

    // Pause the provider so a duplicate callback can race while the first one
    // is outside its consume transaction. Only the first may exchange a code.
    let context = TestContext::new(json!({}), Some(payload())).await;
    let now = Utc::now();
    let user = aster_drive_model::entities::user::ActiveModel {
        username: Set("oauth-admin".into()),
        email: Set("oauth@example.test".into()),
        password_hash: Set("unused".into()),
        role: Set(aster_drive_model::types::UserRole::Admin),
        status: Set(aster_drive_model::types::UserStatus::Active),
        session_version: Set(0),
        email_verified_at: Set(Some(now)),
        pending_email: Set(None),
        storage_used: Set(0),
        storage_quota: Set(0),
        policy_group_id: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        config: Set(None),
        ..Default::default()
    }
    .insert(&context.db)
    .await
    .unwrap();
    let gate = Arc::new(ResponseGate::default());
    let peer = HttpFixture::new([HttpReply::json(200, json!({"access_token":"new-access","refresh_token":"new-refresh","expires_in":3600,"token_type":"Bearer","scope":"https://www.googleapis.com/auth/drive"})).gated(gate.clone())]).await;
    let mut state = crate::runtime::tasks::test_support::setup_primary_state()
        .await
        .get_ref()
        .clone();
    state.db_handles = aster_forge_db::DbHandles::single(context.db.clone());
    state.audit_writer = crate::runtime::test_support::audit_writer(&context.db).await;
    state.config = Arc::new(context.config.clone());
    crate::db::repository::config_repo::upsert(
        &context.db,
        crate::config::site_url::PUBLIC_SITE_URL_KEY,
        r#"["https://drive.test"]"#,
        user.id,
    )
    .await
    .unwrap();
    state.runtime_config.reload(&context.db).await.unwrap();
    state.driver_registry = Arc::new(crate::storage::DriverRegistry::with_connectors(
        aster_drive_metrics::NoopMetrics::arc(),
        Arc::new(
            StorageConnectorRegistry::new(vec![Arc::new(LocalOAuthEndpoint(format!(
                "{}/token",
                peer.base
            )))])
            .unwrap(),
        ),
    ));
    let before = repo::find_by_policy(&context.db, context.policy.id)
        .await
        .unwrap()
        .unwrap();
    let request = actix_web::test::TestRequest::with_uri("https://drive.test/").to_http_request();
    let start = start_authorization(&state, &request, context.policy.id, user.id)
        .await
        .unwrap();
    let state_value = reqwest::Url::parse(&start.authorization_url)
        .unwrap()
        .query_pairs()
        .find(|(key, _)| key == "state")
        .unwrap()
        .1
        .into_owned();
    let query = StorageAuthorizationCallbackQuery {
        code: Some("code".into()),
        state: Some(state_value.clone()),
        error: None,
        error_description: None,
    };
    let first = finish_authorization_callback(&state, &query);
    tokio::pin!(first);
    tokio::select! {
        result = &mut first => panic!("callback finished before token response: {result:?}"),
        _ = gate.entered.notified() => {}
    }
    let duplicate = finish_authorization_callback(&state, &query)
        .await
        .unwrap_err();
    assert_eq!(
        duplicate.reason(),
        crate::storage::connectors::StorageAuthorizationFailureReason::InvalidState
    );
    gate.release.notify_one();
    let outcome = first.await.unwrap();
    assert_eq!(outcome.credential.policy_id, context.policy.id);
    assert_eq!(
        outcome.credential.status,
        StorageCredentialStatus::Authorized
    );
    let row = repo::find_by_policy(&context.db, context.policy.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.revision, before.revision + 1);
    assert_eq!(
        decode(KEY, &row)
            .unwrap()
            .authorization
            .unwrap()
            .access_token,
        "new-access"
    );
    assert!(!row.ciphertext.contains("new-refresh"));
    assert!(state.driver_registry.get_driver(&context.policy).is_ok());
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    let stored_flow =
        aster_drive_model::entities::storage_policy_authorization_flow::Entity::find()
            .filter(
                aster_drive_model::entities::storage_policy_authorization_flow::Column::StateHash
                    .eq(crypto::token_hash(&state_value)),
            )
            .one(&context.db)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(stored_flow.status, StorageAuthorizationFlowStatus::Consumed);
    assert!(stored_flow.consumed_at.is_some());
    assert_eq!(peer.requests().len(), 1);
    peer.stop().await;
}
