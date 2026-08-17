use std::collections::VecDeque;
use std::sync::{Arc, Mutex as StdMutex};

use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::{MicrosoftGraphCloud, StorageCredentialStatus};
use aster_drive_storage::{ConnectorId, StorageErrorKind, StoragePolicyBehaviorConfig};
use chrono::{Duration, Utc};
use sea_orm::IntoActiveModel;
use secrecy::ExposeSecret;

use super::oauth::{
    MicrosoftTokenResponse, microsoft_authorization_url, validate_microsoft_token_response,
};
use super::provider::{
    MicrosoftGraphTokenRefreshRequest, MicrosoftGraphTokenRefresher,
    build_microsoft_graph_credential_token_provider_with_refresher,
};
use super::{
    OneDriveAccountMode, OneDriveApplicationCredentialV1, OneDriveAuthorizationCredentialV1,
    OneDriveAuthorizationMetadataV1, OneDriveCredentialV1,
};
use crate::config::DatabaseConfig;
use crate::db;
use crate::db::repository::storage_policy_connector_credential_repo;
use crate::errors::{Result, storage_driver_error};

const KEY: &str = "oauth-connector-payload-test-key-32bytes";

#[derive(Debug)]
struct TestTokenRefresher {
    requests: StdMutex<Vec<MicrosoftGraphTokenRefreshRequest>>,
    responses: StdMutex<VecDeque<Result<MicrosoftTokenResponse>>>,
}

impl TestTokenRefresher {
    fn new(responses: Vec<Result<MicrosoftTokenResponse>>) -> Self {
        Self {
            requests: StdMutex::new(Vec::new()),
            responses: StdMutex::new(responses.into()),
        }
    }

    fn requests(&self) -> Vec<MicrosoftGraphTokenRefreshRequest> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl MicrosoftGraphTokenRefresher for TestTokenRefresher {
    async fn refresh_token(
        &self,
        request: MicrosoftGraphTokenRefreshRequest,
    ) -> Result<MicrosoftTokenResponse> {
        self.requests.lock().unwrap().push(request);
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("a refresh response must be queued")
    }
}

fn token_response(
    access_token: &str,
    refresh_token: Option<&str>,
    expires_in: i64,
) -> MicrosoftTokenResponse {
    MicrosoftTokenResponse {
        access_token: access_token.to_string(),
        refresh_token: refresh_token.map(ToOwned::to_owned),
        token_type: Some("Bearer".to_string()),
        expires_in: Some(expires_in),
        scope: Some("offline_access Files.ReadWrite.All".to_string()),
        id_token: None,
    }
}

fn credential_payload(
    access_token: &str,
    refresh_token: Option<&str>,
    expires_at: Option<chrono::DateTime<Utc>>,
) -> OneDriveCredentialV1 {
    OneDriveCredentialV1 {
        application: OneDriveApplicationCredentialV1 {
            cloud: MicrosoftGraphCloud::Global,
            tenant: "common".to_string(),
            client_id: "client-id".to_string(),
            client_secret: "client-secret".to_string(),
            scopes: vec![
                "offline_access".to_string(),
                "Files.ReadWrite.All".to_string(),
            ],
        },
        authorization: Some(OneDriveAuthorizationCredentialV1 {
            account_label: Some("Documents".to_string()),
            subject: Some("root".to_string()),
            tenant_id: Some("common".to_string()),
            scopes: vec![
                "offline_access".to_string(),
                "Files.ReadWrite.All".to_string(),
            ],
            access_token: access_token.to_string(),
            refresh_token: refresh_token.map(ToOwned::to_owned),
            metadata: OneDriveAuthorizationMetadataV1 {
                cloud: MicrosoftGraphCloud::Global,
                drive_id: "drive-id".to_string(),
                root_item_id: "root".to_string(),
                root_item_name: Some("Documents".to_string()),
                id_token_present: false,
            },
            status: StorageCredentialStatus::Authorized,
            status_reason: None,
            expires_at,
            authorized_at: Some(Utc::now()),
            last_refreshed_at: None,
            last_validated_at: None,
        }),
    }
}

async fn setup_credential(
    payload: &OneDriveCredentialV1,
) -> (
    sea_orm::DatabaseConnection,
    storage_policy::Model,
    aster_drive_model::entities::storage_policy_connector_credential::Model,
) {
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: "sqlite::memory:".into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    crate::storage::connectors::test_support::migrate_current_storage_test_schema(&db).await;
    let mut policy = crate::storage::connectors::test_support::onedrive_policy(
        OneDriveAccountMode::Personal,
        None,
        None,
        None,
        StoragePolicyBehaviorConfig::default(),
    );
    policy.name = "OneDrive".to_string();
    let policy = crate::db::repository::policy_repo::create(&db, policy.into_active_model())
        .await
        .unwrap();
    crate::storage::connectors::persist_connector_credential_payload(
        &db,
        KEY,
        policy.id,
        &ConnectorId::declared("asterdrive.storage.onedrive"),
        1,
        payload,
    )
    .await
    .unwrap();
    let credential = storage_policy_connector_credential_repo::find_by_policy(&db, policy.id)
        .await
        .unwrap()
        .unwrap();
    (db, policy, credential)
}

fn decode_payload(
    credential: &aster_drive_model::entities::storage_policy_connector_credential::Model,
) -> OneDriveCredentialV1 {
    crate::storage::connectors::decode_typed_connector_credential(
        KEY,
        credential,
        &ConnectorId::declared("asterdrive.storage.onedrive"),
        1,
    )
    .unwrap()
}

#[test]
fn token_refresh_request_debug_redacts_both_secrets() {
    let request = MicrosoftGraphTokenRefreshRequest {
        cloud: MicrosoftGraphCloud::Global,
        tenant: "common".to_string(),
        client_id: "client-id".to_string(),
        client_secret: Some("plain-client-secret".into()),
        refresh_token: "plain-refresh-token".into(),
    };
    let debug = format!("{request:?}");
    assert!(!debug.contains("plain-client-secret"));
    assert!(!debug.contains("plain-refresh-token"));
    assert!(debug.contains("***REDACTED***"));
}

#[test]
fn token_response_validation_covers_token_type_and_blank_access_token() {
    validate_microsoft_token_response(&token_response("token", None, 3600)).unwrap();
    let mut missing_type = token_response("token", None, 3600);
    missing_type.token_type = None;
    validate_microsoft_token_response(&missing_type).unwrap();

    let mut blank = token_response("  ", None, 3600);
    assert!(validate_microsoft_token_response(&blank).is_err());
    blank.access_token = "token".to_string();
    blank.token_type = Some("mac".to_string());
    assert!(validate_microsoft_token_response(&blank).is_err());
}

#[test]
fn authorization_url_contains_pkce_state_and_selected_cloud() {
    let url = microsoft_authorization_url(
        MicrosoftGraphCloud::China,
        "organizations",
        "client-id",
        "https://drive.example/callback",
        &[
            "offline_access".to_string(),
            "Files.ReadWrite.All".to_string(),
        ],
        "state-token",
        "pkce-challenge",
    )
    .unwrap();
    let url = url::Url::parse(&url).unwrap();
    assert_eq!(url.host_str(), Some("login.chinacloudapi.cn"));
    let query = url
        .query_pairs()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(query["state"], "state-token");
    assert_eq!(query["code_challenge_method"], "S256");
    assert_eq!(query["scope"], "offline_access Files.ReadWrite.All");
}

#[test]
fn authorization_url_rejects_tenants_that_change_endpoint_parsing() {
    for tenant in [
        "common/../../evil",
        "common?redirect_uri=https://evil.example/callback",
        "common#fragment",
        "//evil.example",
    ] {
        assert!(
            microsoft_authorization_url(
                MicrosoftGraphCloud::Global,
                tenant,
                "client-id",
                "https://drive.example/callback",
                &["offline_access".to_string()],
                "state-token",
                "pkce-challenge",
            )
            .is_err(),
            "{tenant}"
        );
    }
}

#[tokio::test]
async fn cached_access_token_does_not_refresh_before_expiry() {
    let payload = credential_payload(
        "cached-token",
        Some("refresh-token"),
        Some(Utc::now() + Duration::minutes(10)),
    );
    let (db, policy, credential) = setup_credential(&payload).await;
    let refresher = Arc::new(TestTokenRefresher::new(Vec::new()));
    let provider = build_microsoft_graph_credential_token_provider_with_refresher(
        db,
        KEY.to_string(),
        &policy,
        &credential,
        payload,
        refresher.clone(),
    )
    .unwrap();
    assert_eq!(provider.access_token().await.unwrap(), "cached-token");
    assert!(refresher.requests().is_empty());
}

#[tokio::test]
async fn refresh_success_rotates_tokens_and_revision() {
    let payload = credential_payload(
        "expired",
        Some("refresh-old"),
        Some(Utc::now() - Duration::minutes(1)),
    );
    let (db, policy, credential) = setup_credential(&payload).await;
    let refresher = Arc::new(TestTokenRefresher::new(vec![Ok(token_response(
        "access-new",
        Some("refresh-new"),
        3600,
    ))]));
    let provider = build_microsoft_graph_credential_token_provider_with_refresher(
        db.clone(),
        KEY.to_string(),
        &policy,
        &credential,
        payload,
        refresher.clone(),
    )
    .unwrap();
    assert_eq!(provider.access_token().await.unwrap(), "access-new");
    let requests = refresher.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].refresh_token.expose_secret(), "refresh-old");
    assert_eq!(
        requests[0].client_secret.as_ref().unwrap().expose_secret(),
        "client-secret"
    );
    let stored = storage_policy_connector_credential_repo::find_by_policy(&db, policy.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.revision, 2);
    let payload = decode_payload(&stored);
    let authorization = payload.authorization.unwrap();
    assert_eq!(authorization.access_token, "access-new");
    assert_eq!(authorization.refresh_token.as_deref(), Some("refresh-new"));
    assert_eq!(authorization.status, StorageCredentialStatus::Authorized);
}

#[tokio::test]
async fn missing_refresh_token_marks_reauthorization_required() {
    let payload = credential_payload("expired", None, Some(Utc::now() - Duration::minutes(1)));
    let (db, policy, credential) = setup_credential(&payload).await;
    let provider = build_microsoft_graph_credential_token_provider_with_refresher(
        db.clone(),
        KEY.to_string(),
        &policy,
        &credential,
        payload,
        Arc::new(TestTokenRefresher::new(Vec::new())),
    )
    .unwrap();
    let error = provider.access_token().await.unwrap_err();
    assert_eq!(error.storage_error_kind(), Some(StorageErrorKind::Auth));
    let stored = storage_policy_connector_credential_repo::find_by_policy(&db, policy.id)
        .await
        .unwrap()
        .unwrap();
    let authorization = decode_payload(&stored).authorization.unwrap();
    assert_eq!(
        authorization.status,
        StorageCredentialStatus::ReauthRequired
    );
    assert!(
        authorization
            .status_reason
            .unwrap()
            .contains("missing refresh token")
    );
}

#[tokio::test]
async fn transient_refresh_failure_preserves_authorized_status() {
    let payload = credential_payload(
        "expired",
        Some("refresh-token"),
        Some(Utc::now() - Duration::minutes(1)),
    );
    let (db, policy, credential) = setup_credential(&payload).await;
    let provider = build_microsoft_graph_credential_token_provider_with_refresher(
        db.clone(),
        KEY.to_string(),
        &policy,
        &credential,
        payload,
        Arc::new(TestTokenRefresher::new(vec![Err(storage_driver_error(
            StorageErrorKind::Transient,
            "provider temporarily unavailable",
        ))])),
    )
    .unwrap();
    let error = provider.access_token().await.unwrap_err();
    assert_eq!(
        error.storage_error_kind(),
        Some(StorageErrorKind::Transient)
    );
    let stored = storage_policy_connector_credential_repo::find_by_policy(&db, policy.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.revision, 1);
    assert_eq!(
        decode_payload(&stored).authorization.unwrap().status,
        StorageCredentialStatus::Authorized
    );
}
