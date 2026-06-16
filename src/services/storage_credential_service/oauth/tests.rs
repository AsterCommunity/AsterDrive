use super::audit::{
    OAUTH_AUDIT_ACTION_NAME, OAUTH_AUDIT_EVENT_AUTHORIZATION_FAILED,
    OAUTH_AUDIT_EVENT_CREDENTIAL_REFRESHED, OAUTH_AUDIT_PROVIDER, OAUTH_AUDIT_RESULT_FAILED,
    OAUTH_AUDIT_RESULT_SUCCESS, StorageCredentialOauthAuditDetails,
    storage_credential_oauth_audit_details, write_storage_credential_oauth_audit,
};
use super::microsoft::{
    MicrosoftTokenResponse, StorageCredentialMetadataInput, decrypt_stored_client_secret,
    encrypt_stored_client_secret, microsoft_authorization_url, storage_credential_metadata,
    validate_microsoft_token_response,
};
use super::provider::{
    MicrosoftGraphCleanupTokenSnapshot, MicrosoftGraphTokenRefreshRequest,
    MicrosoftGraphTokenRefresher, build_microsoft_graph_cleanup_token_provider_with_refresher,
    build_microsoft_graph_credential_token_provider_with_refresher,
};
use super::*;
use crate::config::DatabaseConfig;
use crate::db;
use crate::entities::{audit_log, storage_policy};
use crate::services::storage_credential_service::{
    default_microsoft_graph_scopes_for_onedrive_options, normalize_scopes_with_default,
};
use crate::storage::StorageErrorKind;
use crate::storage::error::storage_driver_error;
use crate::types::{
    AuditAction, AuditEntityType, MicrosoftGraphCloud, OneDriveAccountMode, StoragePolicyOptions,
    StoredStoragePolicyAllowedTypes,
};
use migration::Migrator;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex as StdMutex};

#[derive(Debug)]
struct TestMicrosoftGraphTokenRefresher {
    requests: StdMutex<Vec<MicrosoftGraphTokenRefreshRequest>>,
    responses: StdMutex<VecDeque<Result<MicrosoftTokenResponse>>>,
}

impl TestMicrosoftGraphTokenRefresher {
    fn new(responses: Vec<Result<MicrosoftTokenResponse>>) -> Self {
        Self {
            requests: StdMutex::new(Vec::new()),
            responses: StdMutex::new(responses.into()),
        }
    }

    fn requests(&self) -> Vec<MicrosoftGraphTokenRefreshRequest> {
        self.requests
            .lock()
            .expect("refresh request log lock")
            .clone()
    }
}

#[async_trait::async_trait]
impl MicrosoftGraphTokenRefresher for TestMicrosoftGraphTokenRefresher {
    async fn refresh_token(
        &self,
        request: MicrosoftGraphTokenRefreshRequest,
    ) -> Result<MicrosoftTokenResponse> {
        self.requests
            .lock()
            .expect("refresh request log lock")
            .push(request);
        self.responses
            .lock()
            .expect("refresh response queue lock")
            .pop_front()
            .expect("refresh response should be queued")
    }
}

#[derive(Debug)]
struct ConcurrentRotationBeforeSuccessRefresher {
    requests: StdMutex<Vec<MicrosoftGraphTokenRefreshRequest>>,
    responses: StdMutex<VecDeque<Result<MicrosoftTokenResponse>>>,
    db: sea_orm::DatabaseConnection,
    encryption_key: String,
    policy_id: i64,
}

impl ConcurrentRotationBeforeSuccessRefresher {
    fn new(
        db: sea_orm::DatabaseConnection,
        encryption_key: &str,
        policy_id: i64,
        responses: Vec<Result<MicrosoftTokenResponse>>,
    ) -> Self {
        Self {
            requests: StdMutex::new(Vec::new()),
            responses: StdMutex::new(responses.into()),
            db,
            encryption_key: encryption_key.to_string(),
            policy_id,
        }
    }

    fn requests(&self) -> Vec<MicrosoftGraphTokenRefreshRequest> {
        self.requests
            .lock()
            .expect("refresh request log lock")
            .clone()
    }
}

#[async_trait::async_trait]
impl MicrosoftGraphTokenRefresher for ConcurrentRotationBeforeSuccessRefresher {
    async fn refresh_token(
        &self,
        request: MicrosoftGraphTokenRefreshRequest,
    ) -> Result<MicrosoftTokenResponse> {
        self.requests
            .lock()
            .expect("refresh request log lock")
            .push(request);
        create_microsoft_graph_credential(
            &self.db,
            &self.encryption_key,
            self.policy_id,
            "newer-access-token",
            Some("newer-refresh-token"),
            Some(Utc::now() + Duration::minutes(10)),
        )
        .await;

        self.responses
            .lock()
            .expect("refresh response queue lock")
            .pop_front()
            .expect("refresh response should be queued")
    }
}

fn microsoft_token_response(
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

async fn setup_db() -> sea_orm::DatabaseConnection {
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            pool_size: 1,
            retry_count: 0,
        },
        crate::metrics_core::NoopMetrics::arc(),
    )
    .await
    .expect("storage credential test DB should connect");
    Migrator::up(&db, None)
        .await
        .expect("storage credential migrations should succeed");
    db
}

async fn setup_file_db(pool_size: u32) -> (sea_orm::DatabaseConnection, std::path::PathBuf) {
    let db_path = std::env::temp_dir().join(format!(
        "asterdrive-storage-credential-{}.db",
        uuid::Uuid::new_v4()
    ));
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: format!("sqlite://{}?mode=rwc", db_path.display()),
            pool_size,
            retry_count: 0,
        },
        crate::metrics_core::NoopMetrics::arc(),
    )
    .await
    .expect("storage credential test DB should connect");
    Migrator::up(&db, None)
        .await
        .expect("storage credential migrations should succeed");
    (db, db_path)
}

async fn create_onedrive_policy(
    db: &sea_orm::DatabaseConnection,
    client_id: &str,
    client_secret: &str,
) -> storage_policy::Model {
    create_onedrive_policy_with_options(
        db,
        client_id,
        client_secret,
        StoragePolicyOptions::default(),
    )
    .await
}

async fn create_onedrive_policy_with_options(
    db: &sea_orm::DatabaseConnection,
    client_id: &str,
    client_secret: &str,
    options: StoragePolicyOptions,
) -> storage_policy::Model {
    let now = Utc::now();
    policy_repo::create(
        db,
        storage_policy::ActiveModel {
            name: Set("onedrive".to_string()),
            driver_type: Set(DriverType::OneDrive),
            endpoint: Set(String::new()),
            bucket: Set(String::new()),
            access_key: Set(client_id.to_string()),
            secret_key: Set(client_secret.to_string()),
            base_path: Set(String::new()),
            remote_node_id: Set(None),
            max_file_size: Set(0),
            allowed_types: Set(StoredStoragePolicyAllowedTypes::empty()),
            options: Set(crate::types::serialize_storage_policy_options(&options).unwrap()),
            is_default: Set(false),
            chunk_size: Set(5_242_880),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .expect("policy should insert")
}

async fn create_microsoft_graph_credential(
    db: &sea_orm::DatabaseConnection,
    encryption_key: &str,
    policy_id: i64,
    access_token: &str,
    refresh_token: Option<&str>,
    expires_at: Option<chrono::DateTime<Utc>>,
) -> storage_policy_credential::Model {
    let now = Utc::now();
    let access_token_ciphertext = crypto::encrypt_token(
        encryption_key,
        crypto::token_aad(
            policy_id,
            StorageCredentialProvider::MicrosoftGraph.as_str(),
            "access",
        )
        .as_bytes(),
        access_token,
    )
    .expect("access token should encrypt");
    let refresh_token_ciphertext = refresh_token
        .map(|refresh_token| {
            crypto::encrypt_token(
                encryption_key,
                crypto::token_aad(
                    policy_id,
                    StorageCredentialProvider::MicrosoftGraph.as_str(),
                    "refresh",
                )
                .as_bytes(),
                refresh_token,
            )
        })
        .transpose()
        .expect("refresh token should encrypt");
    storage_policy_credential_repo::upsert_by_policy_provider_kind(
        db,
        storage_policy_credential::ActiveModel {
            policy_id: Set(policy_id),
            provider: Set(StorageCredentialProvider::MicrosoftGraph),
            credential_kind: Set(StorageCredentialKind::OauthDelegated),
            account_label: Set(Some("Drive".to_string())),
            subject: Set(Some("root".to_string())),
            tenant_id: Set(Some("common".to_string())),
            scopes: Set(r#"["offline_access","Files.ReadWrite.All"]"#.to_string()),
            access_token_ciphertext: Set(Some(access_token_ciphertext)),
            refresh_token_ciphertext: Set(refresh_token_ciphertext),
            metadata: Set(serde_json::json!({
                "cloud": MicrosoftGraphCloud::Global,
                "drive_id": "drive-id",
                "root_item_id": "root"
            })
            .to_string()),
            status: Set(StorageCredentialStatus::Authorized),
            status_reason: Set(None),
            expires_at: Set(expires_at),
            authorized_at: Set(Some(now)),
            last_refreshed_at: Set(None),
            last_validated_at: Set(None),
            ..Default::default()
        },
        now,
    )
    .await
    .expect("credential should insert")
}

async fn latest_oauth_audit_details(db: &sea_orm::DatabaseConnection) -> serde_json::Value {
    let entry = audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq(AuditAction::AdminTriggerStorageAction))
        .order_by_desc(audit_log::Column::Id)
        .one(db)
        .await
        .expect("audit lookup should succeed")
        .expect("audit entry should exist");
    serde_json::from_str(entry.details.as_deref().unwrap_or("{}"))
        .expect("audit details should be valid json")
}

#[tokio::test]
async fn credential_upsert_is_atomic_for_concurrent_same_key_inserts() {
    let (db, db_path) = setup_file_db(4).await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "client-id", "client-secret").await;

    let first = create_microsoft_graph_credential(
        &db,
        encryption_key,
        policy.id,
        "first-access-token",
        Some("first-refresh-token"),
        Some(Utc::now() + Duration::minutes(10)),
    );
    let second = create_microsoft_graph_credential(
        &db,
        encryption_key,
        policy.id,
        "second-access-token",
        Some("second-refresh-token"),
        Some(Utc::now() + Duration::minutes(10)),
    );

    let (first_result, second_result) = tokio::join!(first, second);

    assert_eq!(first_result.policy_id, policy.id);
    assert_eq!(second_result.policy_id, policy.id);
    let count = storage_policy_credential::Entity::find()
        .filter(storage_policy_credential::Column::PolicyId.eq(policy.id))
        .count(&db)
        .await
        .expect("credential count should load");
    assert_eq!(count, 1);
    let stored = storage_policy_credential_repo::find_by_policy_provider_kind(
        &db,
        policy.id,
        StorageCredentialProvider::MicrosoftGraph,
        StorageCredentialKind::OauthDelegated,
    )
    .await
    .expect("credential lookup should succeed")
    .expect("credential should exist");
    let stored_access = decrypt_stored_oauth_token(
        encryption_key,
        policy.id,
        "access",
        stored.access_token_ciphertext.as_deref().unwrap(),
    );
    assert!(["first-access-token", "second-access-token"].contains(&stored_access.as_str()));

    drop(db);
    let _ = std::fs::remove_file(db_path);
}

#[tokio::test]
async fn refresh_result_update_preserves_existing_refresh_token_when_omitted() {
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "client-id", "client-secret").await;
    let credential = create_microsoft_graph_credential(
        &db,
        encryption_key,
        policy.id,
        "old-access-token",
        Some("old-refresh-token"),
        Some(Utc::now() - Duration::minutes(10)),
    )
    .await;
    let old_refresh_ciphertext = credential
        .refresh_token_ciphertext
        .clone()
        .expect("refresh token should be stored");
    let new_access_ciphertext = crypto::encrypt_token(
        encryption_key,
        crypto::token_aad(
            policy.id,
            StorageCredentialProvider::MicrosoftGraph.as_str(),
            "access",
        )
        .as_bytes(),
        "new-access-token",
    )
    .expect("new access token should encrypt");

    let updated =
        storage_policy_credential_repo::update_oauth_refresh_result_if_refresh_token_matches(
            &db,
            storage_policy_credential_repo::OAuthRefreshUpdate {
                policy_id: policy.id,
                provider: StorageCredentialProvider::MicrosoftGraph,
                credential_kind: StorageCredentialKind::OauthDelegated,
                expected_refresh_token_ciphertext: &old_refresh_ciphertext,
                access_token_ciphertext: new_access_ciphertext,
                refresh_token_ciphertext: None,
                expires_at: Some(Utc::now() + Duration::minutes(30)),
                scopes: None,
                now: Utc::now(),
            },
        )
        .await
        .expect("refresh result should update");

    assert!(updated);
    let stored = storage_policy_credential_repo::find_by_policy_provider_kind(
        &db,
        policy.id,
        StorageCredentialProvider::MicrosoftGraph,
        StorageCredentialKind::OauthDelegated,
    )
    .await
    .expect("credential lookup should succeed")
    .expect("credential should exist");
    assert_eq!(
        stored.refresh_token_ciphertext.as_deref(),
        Some(old_refresh_ciphertext.as_str())
    );
    assert_eq!(
        decrypt_stored_oauth_token(
            encryption_key,
            policy.id,
            "access",
            stored.access_token_ciphertext.as_deref().unwrap(),
        ),
        "new-access-token"
    );
}

fn decrypt_stored_oauth_token(
    encryption_key: &str,
    policy_id: i64,
    kind: &str,
    ciphertext: &str,
) -> String {
    crypto::decrypt_token(
        encryption_key,
        crypto::token_aad(
            policy_id,
            StorageCredentialProvider::MicrosoftGraph.as_str(),
            kind,
        )
        .as_bytes(),
        ciphertext,
    )
    .expect("stored OAuth token should decrypt")
}

#[test]
fn microsoft_authorization_url_uses_selected_cloud_and_pkce() {
    let url = microsoft_authorization_url(
        MicrosoftGraphCloud::China,
        "organizations",
        "client-id",
        "https://drive.example.com/api/v1/admin/policies/storage-authorization/callback",
        &[
            "offline_access".to_string(),
            "Files.ReadWrite.All".to_string(),
        ],
        "state",
        "challenge",
    )
    .unwrap();

    assert!(url.starts_with("https://login.chinacloudapi.cn/organizations/oauth2/v2.0/authorize?"));
    assert!(url.contains("response_type=code"));
    assert!(url.contains("client_id=client-id"));
    assert!(url.contains("code_challenge=challenge"));
    assert!(url.contains("code_challenge_method=S256"));
}

#[test]
fn storage_authorization_failure_reason_values_are_stable() {
    assert_eq!(
        StorageAuthorizationFailureReason::InvalidState.as_str(),
        "invalid_state"
    );
    assert_eq!(
        StorageAuthorizationFailureReason::ProviderError.as_str(),
        "provider_error"
    );
    assert_eq!(
        StorageAuthorizationFailureReason::TokenExchangeFailed.as_str(),
        "token_exchange_failed"
    );
    assert_eq!(
        StorageAuthorizationFailureReason::DriveResolutionFailed.as_str(),
        "drive_resolution_failed"
    );
    assert_eq!(
        StorageAuthorizationFailureReason::InvalidRequest.as_str(),
        "invalid_request"
    );
    assert_eq!(
        StorageAuthorizationFailureReason::ServerError.as_str(),
        "server_error"
    );
    assert_eq!(
        StorageAuthorizationFailureReason::UnsupportedProvider.as_str(),
        "unsupported_provider"
    );
}

#[test]
fn microsoft_graph_scopes_default_to_user_drive_for_personal_and_work_or_school() {
    for mode in [
        OneDriveAccountMode::Personal,
        OneDriveAccountMode::WorkOrSchool,
    ] {
        let options = StoragePolicyOptions {
            onedrive_account_mode: Some(mode),
            ..Default::default()
        };

        assert_eq!(
            normalize_scopes_with_default(
                None,
                default_microsoft_graph_scopes_for_onedrive_options(&options),
            ),
            vec!["offline_access".to_string(), "Files.ReadWrite".to_string()]
        );
    }
}

#[test]
fn microsoft_graph_scopes_default_to_broad_drive_access_for_explicit_drive_id() {
    let options = StoragePolicyOptions {
        onedrive_account_mode: Some(OneDriveAccountMode::WorkOrSchool),
        onedrive_drive_id: Some("drive-id".to_string()),
        ..Default::default()
    };

    assert_eq!(
        normalize_scopes_with_default(
            None,
            default_microsoft_graph_scopes_for_onedrive_options(&options),
        ),
        vec![
            "offline_access".to_string(),
            "Files.ReadWrite.All".to_string(),
        ]
    );
}

#[test]
fn microsoft_graph_scopes_default_to_shared_drive_access_for_site_and_group_modes() {
    for mode in [
        OneDriveAccountMode::SharepointSite,
        OneDriveAccountMode::GroupDrive,
    ] {
        let options = StoragePolicyOptions {
            onedrive_account_mode: Some(mode),
            ..Default::default()
        };

        assert_eq!(
            normalize_scopes_with_default(
                None,
                default_microsoft_graph_scopes_for_onedrive_options(&options),
            ),
            vec![
                "offline_access".to_string(),
                "Files.ReadWrite.All".to_string(),
                "Sites.ReadWrite.All".to_string(),
            ]
        );
    }
}

#[test]
fn microsoft_graph_scopes_keep_existing_broad_default_when_account_mode_is_missing() {
    assert_eq!(
        normalize_scopes_with_default(
            None,
            default_microsoft_graph_scopes_for_onedrive_options(&StoragePolicyOptions::default()),
        ),
        vec![
            "offline_access".to_string(),
            "Files.ReadWrite.All".to_string(),
            "Sites.ReadWrite.All".to_string(),
        ]
    );
}

#[test]
fn microsoft_graph_scope_input_overrides_account_mode_default_and_deduplicates() {
    let options = StoragePolicyOptions {
        onedrive_account_mode: Some(OneDriveAccountMode::Personal),
        ..Default::default()
    };

    assert_eq!(
        normalize_scopes_with_default(
            Some(vec![
                " Files.ReadWrite.All ".to_string(),
                "offline_access".to_string(),
                "Files.ReadWrite.All".to_string(),
                " ".to_string(),
            ]),
            default_microsoft_graph_scopes_for_onedrive_options(&options),
        ),
        vec![
            "Files.ReadWrite.All".to_string(),
            "offline_access".to_string(),
        ]
    );
}

#[tokio::test]
async fn storage_credential_oauth_audit_uses_storage_policy_action_details() {
    let db = setup_db().await;

    write_storage_credential_oauth_audit(
        &db,
        0,
        StorageCredentialOauthAuditDetails {
            event: OAUTH_AUDIT_EVENT_CREDENTIAL_REFRESHED,
            result: OAUTH_AUDIT_RESULT_SUCCESS,
            policy_id: Some(42),
            cloud: Some(MicrosoftGraphCloud::Global),
            tenant: Some("common"),
            refresh_token_rotated: Some(true),
            ..Default::default()
        },
    )
    .await;

    let entry = audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq(AuditAction::AdminTriggerStorageAction))
        .one(&db)
        .await
        .expect("audit lookup should succeed")
        .expect("audit entry should exist");
    assert_eq!(entry.entity_type, AuditEntityType::StoragePolicy.as_str());
    assert_eq!(entry.entity_id, Some(42));
    let details =
        serde_json::from_str::<serde_json::Value>(entry.details.as_deref().unwrap()).unwrap();
    assert_eq!(details["action"], OAUTH_AUDIT_ACTION_NAME);
    assert_eq!(
        details["oauth_event"],
        OAUTH_AUDIT_EVENT_CREDENTIAL_REFRESHED
    );
    assert_eq!(details["provider"], OAUTH_AUDIT_PROVIDER.as_str());
    assert_eq!(details["cloud"], "global");
    assert_eq!(details["refresh_token_rotated"], true);
}

#[test]
fn storage_credential_oauth_audit_details_omit_absent_optional_fields() {
    let details = storage_credential_oauth_audit_details(StorageCredentialOauthAuditDetails {
        event: OAUTH_AUDIT_EVENT_AUTHORIZATION_FAILED,
        result: OAUTH_AUDIT_RESULT_FAILED,
        ..Default::default()
    });

    assert_eq!(details["action"], OAUTH_AUDIT_ACTION_NAME);
    assert_eq!(
        details["oauth_event"],
        OAUTH_AUDIT_EVENT_AUTHORIZATION_FAILED
    );
    assert_eq!(details["result"], OAUTH_AUDIT_RESULT_FAILED);
    assert!(details.get("policy_id").is_none());
    assert!(details.get("cloud").is_none());
    assert!(details.get("tenant").is_none());
    assert!(details.get("reason").is_none());
    assert!(details.get("client_secret_configured").is_none());
    assert!(details.get("refresh_token_rotated").is_none());
    assert!(details.get("recovered_from_token_rotation").is_none());
}

#[test]
fn storage_metadata_encrypts_client_secret_for_reuse() {
    let key = "storage-token-test-master-key-32bytes";
    let metadata = storage_credential_metadata(StorageCredentialMetadataInput {
        encryption_key: key,
        policy_id: 42,
        cloud: MicrosoftGraphCloud::Global,
        client_id: Some("client-id"),
        client_secret: Some("client-secret"),
        client_secret_ciphertext: None,
        drive_id: "drive-id",
        root_item_id: "root",
        root_item_name: Some("Root"),
        id_token: None,
    })
    .unwrap();
    let parsed = serde_json::from_str::<serde_json::Value>(&metadata).unwrap();

    assert_eq!(parsed["client_id"], "client-id");
    assert_eq!(parsed["client_secret_configured"], true);
    assert_ne!(parsed["client_secret_ciphertext"], "client-secret");
    assert_eq!(
        decrypt_stored_client_secret(
            key,
            42,
            parsed["client_secret_ciphertext"].as_str().unwrap(),
        )
        .unwrap(),
        "client-secret"
    );
}

#[test]
fn storage_metadata_preserves_existing_client_secret_ciphertext() {
    let key = "storage-token-test-master-key-32bytes";
    let ciphertext = encrypt_stored_client_secret(key, 42, "client-secret").unwrap();
    let metadata = storage_credential_metadata(StorageCredentialMetadataInput {
        encryption_key: key,
        policy_id: 42,
        cloud: MicrosoftGraphCloud::China,
        client_id: Some("client-id"),
        client_secret: None,
        client_secret_ciphertext: Some(&ciphertext),
        drive_id: "drive-id",
        root_item_id: "root",
        root_item_name: Some("Root"),
        id_token: None,
    })
    .unwrap();
    let parsed = serde_json::from_str::<serde_json::Value>(&metadata).unwrap();

    assert_eq!(parsed["client_secret_configured"], true);
    assert_eq!(parsed["client_secret_ciphertext"], ciphertext);
    assert_eq!(
        decrypt_stored_client_secret(
            key,
            42,
            parsed["client_secret_ciphertext"].as_str().unwrap(),
        )
        .unwrap(),
        "client-secret"
    );
}

#[test]
fn storage_metadata_ignores_blank_client_secret_values() {
    let key = "storage-token-test-master-key-32bytes";
    let metadata = storage_credential_metadata(StorageCredentialMetadataInput {
        encryption_key: key,
        policy_id: 42,
        cloud: MicrosoftGraphCloud::Global,
        client_id: Some(" client-id "),
        client_secret: Some("   "),
        client_secret_ciphertext: Some("   "),
        drive_id: "drive-id",
        root_item_id: "root",
        root_item_name: None,
        id_token: None,
    })
    .unwrap();
    let parsed = serde_json::from_str::<serde_json::Value>(&metadata).unwrap();

    assert_eq!(parsed["client_id"], "client-id");
    assert_eq!(parsed["client_secret_configured"], false);
    assert!(parsed.get("client_secret_ciphertext").is_none());
}

#[test]
fn microsoft_token_response_validation_accepts_bearer_or_missing_token_type() {
    validate_microsoft_token_response(&MicrosoftTokenResponse {
        access_token: "access-token".to_string(),
        refresh_token: None,
        token_type: Some("Bearer".to_string()),
        expires_in: Some(3600),
        scope: None,
        id_token: None,
    })
    .unwrap();

    validate_microsoft_token_response(&MicrosoftTokenResponse {
        access_token: "access-token".to_string(),
        refresh_token: None,
        token_type: None,
        expires_in: Some(3600),
        scope: None,
        id_token: None,
    })
    .unwrap();
}

#[test]
fn microsoft_token_response_validation_rejects_blank_access_token() {
    let error = validate_microsoft_token_response(&MicrosoftTokenResponse {
        access_token: " ".to_string(),
        refresh_token: None,
        token_type: Some("Bearer".to_string()),
        expires_in: Some(3600),
        scope: None,
        id_token: None,
    })
    .unwrap_err();

    assert!(error.message().contains("missing access_token"));
}

#[test]
fn microsoft_token_response_validation_rejects_unsupported_token_type() {
    let error = validate_microsoft_token_response(&MicrosoftTokenResponse {
        access_token: "access-token".to_string(),
        refresh_token: None,
        token_type: Some("mac".to_string()),
        expires_in: Some(3600),
        scope: None,
        id_token: None,
    })
    .unwrap_err();

    assert!(error.message().contains("unsupported token_type"));
}

#[tokio::test]
async fn credential_token_provider_requires_client_secret() {
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "client-id", "").await;
    let credential = create_microsoft_graph_credential(
        &db,
        encryption_key,
        policy.id,
        "cached-access-token",
        Some("refresh-token"),
        Some(Utc::now() + Duration::minutes(10)),
    )
    .await;

    let error = match build_microsoft_graph_credential_token_provider(
        db,
        encryption_key.to_string(),
        &policy,
        &credential,
        MicrosoftGraphCloud::Global,
    ) {
        Ok(_) => panic!("provider without client_secret should be rejected"),
        Err(error) => error,
    };

    assert_eq!(error.storage_error_kind(), Some(StorageErrorKind::Auth));
    assert!(error.to_string().contains("client_secret"));
}

#[tokio::test]
async fn credential_token_provider_refreshes_when_access_token_expiry_is_missing() {
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "client-id", "client-secret").await;
    let credential = create_microsoft_graph_credential(
        &db,
        encryption_key,
        policy.id,
        "unknown-expiry-access-token",
        Some("refresh-token"),
        None,
    )
    .await;
    let refresher = Arc::new(TestMicrosoftGraphTokenRefresher::new(vec![Ok(
        microsoft_token_response("refreshed-access-token", None, 3600),
    )]));
    let provider = build_microsoft_graph_credential_token_provider_with_refresher(
        db,
        encryption_key.to_string(),
        &policy,
        &credential,
        MicrosoftGraphCloud::Global,
        refresher.clone(),
    )
    .expect("provider should build");

    let access_token = provider.access_token().await.expect("token should refresh");

    assert_eq!(access_token, "refreshed-access-token");
    assert_eq!(refresher.requests().len(), 1);
}

#[tokio::test]
async fn cleanup_token_provider_refreshes_from_snapshot_without_database_writes() {
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "client-id", "client-secret").await;
    let access_token_ciphertext = crypto::encrypt_token(
        encryption_key,
        crypto::token_aad(
            policy.id,
            StorageCredentialProvider::MicrosoftGraph.as_str(),
            "access",
        )
        .as_bytes(),
        "expired-access-token",
    )
    .expect("access token should encrypt");
    let refresh_token_ciphertext = crypto::encrypt_token(
        encryption_key,
        crypto::token_aad(
            policy.id,
            StorageCredentialProvider::MicrosoftGraph.as_str(),
            "refresh",
        )
        .as_bytes(),
        "snapshot-refresh-token",
    )
    .expect("refresh token should encrypt");
    let refresher = Arc::new(TestMicrosoftGraphTokenRefresher::new(vec![Ok(
        microsoft_token_response(
            "cleanup-access-token",
            Some("rotated-cleanup-refresh"),
            3600,
        ),
    )]));
    let provider = build_microsoft_graph_cleanup_token_provider_with_refresher(
        encryption_key.to_string(),
        &policy,
        MicrosoftGraphCleanupTokenSnapshot {
            cloud: MicrosoftGraphCloud::Global,
            tenant_id: Some("tenant-id".to_string()),
            client_id: None,
            client_secret_ciphertext: None,
            access_token_ciphertext,
            refresh_token_ciphertext: Some(refresh_token_ciphertext),
            expires_at: Some(Utc::now() - Duration::minutes(5)),
        },
        refresher.clone(),
    )
    .expect("cleanup provider should build");

    let access_token = provider.access_token().await.expect("token should refresh");
    let cached = provider
        .access_token()
        .await
        .expect("fresh token should be cached");

    assert_eq!(access_token, "cleanup-access-token");
    assert_eq!(cached, "cleanup-access-token");
    let requests = refresher.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.cloud, MicrosoftGraphCloud::Global);
    assert_eq!(request.tenant, "tenant-id");
    assert_eq!(request.client_id, "client-id");
    assert_eq!(request.client_secret.as_deref(), Some("client-secret"));
    assert_eq!(request.refresh_token, "snapshot-refresh-token");
}

#[tokio::test]
async fn cleanup_token_provider_uses_snapshot_client_credentials_when_policy_secret_is_empty() {
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "   ", "   ").await;
    let access_token_ciphertext = crypto::encrypt_token(
        encryption_key,
        crypto::token_aad(
            policy.id,
            StorageCredentialProvider::MicrosoftGraph.as_str(),
            "access",
        )
        .as_bytes(),
        "expired-access-token",
    )
    .expect("access token should encrypt");
    let refresh_token_ciphertext = crypto::encrypt_token(
        encryption_key,
        crypto::token_aad(
            policy.id,
            StorageCredentialProvider::MicrosoftGraph.as_str(),
            "refresh",
        )
        .as_bytes(),
        "snapshot-refresh-token",
    )
    .expect("refresh token should encrypt");
    let client_secret_ciphertext =
        encrypt_stored_client_secret(encryption_key, policy.id, "snapshot-client-secret")
            .expect("client secret should encrypt");
    let refresher = Arc::new(TestMicrosoftGraphTokenRefresher::new(vec![Ok(
        microsoft_token_response("cleanup-access-token", None, 3600),
    )]));
    let provider = build_microsoft_graph_cleanup_token_provider_with_refresher(
        encryption_key.to_string(),
        &policy,
        MicrosoftGraphCleanupTokenSnapshot {
            cloud: MicrosoftGraphCloud::Global,
            tenant_id: Some("tenant-id".to_string()),
            client_id: Some(" snapshot-client-id ".to_string()),
            client_secret_ciphertext: Some(format!(" {client_secret_ciphertext} ")),
            access_token_ciphertext,
            refresh_token_ciphertext: Some(refresh_token_ciphertext),
            expires_at: Some(Utc::now() - Duration::minutes(5)),
        },
        refresher.clone(),
    )
    .expect("cleanup provider should build with snapshot client credentials");

    let access_token = provider.access_token().await.expect("token should refresh");

    assert_eq!(access_token, "cleanup-access-token");
    let requests = refresher.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.client_id, "snapshot-client-id");
    assert_eq!(
        request.client_secret.as_deref(),
        Some("snapshot-client-secret")
    );
}

#[tokio::test]
async fn cleanup_token_provider_rejects_missing_refresh_token_after_expiry() {
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "client-id", "client-secret").await;
    let access_token_ciphertext = crypto::encrypt_token(
        encryption_key,
        crypto::token_aad(
            policy.id,
            StorageCredentialProvider::MicrosoftGraph.as_str(),
            "access",
        )
        .as_bytes(),
        "expired-access-token",
    )
    .expect("access token should encrypt");
    let refresher = Arc::new(TestMicrosoftGraphTokenRefresher::new(Vec::new()));
    let provider = build_microsoft_graph_cleanup_token_provider_with_refresher(
        encryption_key.to_string(),
        &policy,
        MicrosoftGraphCleanupTokenSnapshot {
            cloud: MicrosoftGraphCloud::Global,
            tenant_id: None,
            client_id: None,
            client_secret_ciphertext: None,
            access_token_ciphertext,
            refresh_token_ciphertext: None,
            expires_at: Some(Utc::now() - Duration::minutes(5)),
        },
        refresher,
    )
    .expect("cleanup provider should build");

    let error = provider.access_token().await.unwrap_err();

    assert_eq!(error.storage_error_kind(), Some(StorageErrorKind::Auth));
    assert!(error.message().contains("missing refresh token"));
}

#[tokio::test]
async fn credential_token_provider_returns_cached_access_token_before_expiry() {
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "client-id", "client-secret").await;
    let credential = create_microsoft_graph_credential(
        &db,
        encryption_key,
        policy.id,
        "cached-access-token",
        None,
        Some(Utc::now() + Duration::minutes(10)),
    )
    .await;
    let provider = build_microsoft_graph_credential_token_provider(
        db.clone(),
        encryption_key.to_string(),
        &policy,
        &credential,
        MicrosoftGraphCloud::Global,
    )
    .expect("provider should build");

    let access_token = provider.access_token().await.expect("token should load");

    assert_eq!(access_token, "cached-access-token");
    let stored = storage_policy_credential_repo::find_by_policy_provider_kind(
        &db,
        policy.id,
        StorageCredentialProvider::MicrosoftGraph,
        StorageCredentialKind::OauthDelegated,
    )
    .await
    .expect("credential lookup should succeed")
    .expect("credential should exist");
    assert_eq!(stored.status, StorageCredentialStatus::Authorized);
    assert_eq!(stored.status_reason, None);
}

#[tokio::test]
async fn credential_token_provider_marks_reauth_required_when_refresh_token_is_missing() {
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "client-id", "client-secret").await;
    let credential = create_microsoft_graph_credential(
        &db,
        encryption_key,
        policy.id,
        "expired-access-token",
        None,
        Some(Utc::now() - Duration::minutes(10)),
    )
    .await;
    let provider = build_microsoft_graph_credential_token_provider(
        db.clone(),
        encryption_key.to_string(),
        &policy,
        &credential,
        MicrosoftGraphCloud::Global,
    )
    .expect("provider should build");

    let error = provider.access_token().await.unwrap_err();

    assert_eq!(error.storage_error_kind(), Some(StorageErrorKind::Auth));
    let stored = storage_policy_credential_repo::find_by_policy_provider_kind(
        &db,
        policy.id,
        StorageCredentialProvider::MicrosoftGraph,
        StorageCredentialKind::OauthDelegated,
    )
    .await
    .expect("credential lookup should succeed")
    .expect("credential should exist");
    assert_eq!(stored.status, StorageCredentialStatus::ReauthRequired);
    assert!(
        stored
            .status_reason
            .as_deref()
            .unwrap_or_default()
            .contains("missing refresh token")
    );
}

#[tokio::test]
async fn credential_token_provider_refresh_success_writes_new_access_and_refresh_tokens() {
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "client-id", "client-secret").await;
    let credential = create_microsoft_graph_credential(
        &db,
        encryption_key,
        policy.id,
        "expired-access-token",
        Some("old-refresh-token"),
        Some(Utc::now() - Duration::minutes(10)),
    )
    .await;
    let refresher = Arc::new(TestMicrosoftGraphTokenRefresher::new(vec![Ok(
        microsoft_token_response("new-access-token", Some("new-refresh-token"), 3600),
    )]));
    let provider = build_microsoft_graph_credential_token_provider_with_refresher(
        db.clone(),
        encryption_key.to_string(),
        &policy,
        &credential,
        MicrosoftGraphCloud::Global,
        refresher.clone(),
    )
    .expect("provider should build");

    let access_token = provider.access_token().await.expect("token should refresh");

    assert_eq!(access_token, "new-access-token");
    assert_eq!(refresher.requests().len(), 1);
    let request = refresher
        .requests()
        .into_iter()
        .next()
        .expect("request should be logged");
    assert_eq!(request.cloud, MicrosoftGraphCloud::Global);
    assert_eq!(request.tenant, "common");
    assert_eq!(request.client_id, "client-id");
    assert_eq!(request.client_secret.as_deref(), Some("client-secret"));
    assert_eq!(request.refresh_token, "old-refresh-token");

    let stored = storage_policy_credential_repo::find_by_policy_provider_kind(
        &db,
        policy.id,
        StorageCredentialProvider::MicrosoftGraph,
        StorageCredentialKind::OauthDelegated,
    )
    .await
    .expect("credential lookup should succeed")
    .expect("credential should exist");
    assert_eq!(stored.status, StorageCredentialStatus::Authorized);
    assert_eq!(stored.status_reason, None);
    assert!(stored.last_refreshed_at.is_some());
    assert!(
        stored
            .expires_at
            .is_some_and(|expires_at| expires_at > Utc::now())
    );
    assert_eq!(
        decrypt_stored_oauth_token(
            encryption_key,
            policy.id,
            "access",
            stored.access_token_ciphertext.as_deref().unwrap(),
        ),
        "new-access-token"
    );
    assert_eq!(
        decrypt_stored_oauth_token(
            encryption_key,
            policy.id,
            "refresh",
            stored.refresh_token_ciphertext.as_deref().unwrap(),
        ),
        "new-refresh-token"
    );
    assert_eq!(
        serde_json::from_str::<Vec<String>>(&stored.scopes).unwrap(),
        vec![
            "offline_access".to_string(),
            "Files.ReadWrite.All".to_string()
        ]
    );
}

#[tokio::test]
async fn credential_token_provider_refresh_success_preserves_refresh_token_when_response_omits_or_blanks_it()
 {
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "client-id", "client-secret").await;
    let credential = create_microsoft_graph_credential(
        &db,
        encryption_key,
        policy.id,
        "expired-access-token",
        Some("old-refresh-token"),
        Some(Utc::now() - Duration::minutes(10)),
    )
    .await;
    let refresher = Arc::new(TestMicrosoftGraphTokenRefresher::new(vec![Ok(
        microsoft_token_response("new-access-token", Some("   "), 3600),
    )]));
    let provider = build_microsoft_graph_credential_token_provider_with_refresher(
        db.clone(),
        encryption_key.to_string(),
        &policy,
        &credential,
        MicrosoftGraphCloud::Global,
        refresher,
    )
    .expect("provider should build");

    let access_token = provider.access_token().await.expect("token should refresh");

    assert_eq!(access_token, "new-access-token");
    let stored = storage_policy_credential_repo::find_by_policy_provider_kind(
        &db,
        policy.id,
        StorageCredentialProvider::MicrosoftGraph,
        StorageCredentialKind::OauthDelegated,
    )
    .await
    .expect("credential lookup should succeed")
    .expect("credential should exist");
    assert_eq!(
        decrypt_stored_oauth_token(
            encryption_key,
            policy.id,
            "refresh",
            stored.refresh_token_ciphertext.as_deref().unwrap(),
        ),
        "old-refresh-token"
    );
    assert_eq!(
        decrypt_stored_oauth_token(
            encryption_key,
            policy.id,
            "access",
            stored.access_token_ciphertext.as_deref().unwrap(),
        ),
        "new-access-token"
    );
    let audit = latest_oauth_audit_details(&db).await;
    assert_eq!(audit["refresh_token_rotated"], false);
}

#[tokio::test]
async fn credential_token_provider_refresh_success_cas_recovers_newer_rotated_db_token() {
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "client-id", "client-secret").await;
    let credential = create_microsoft_graph_credential(
        &db,
        encryption_key,
        policy.id,
        "expired-access-token",
        Some("old-refresh-token"),
        Some(Utc::now() - Duration::minutes(10)),
    )
    .await;
    let refresher = Arc::new(ConcurrentRotationBeforeSuccessRefresher::new(
        db.clone(),
        encryption_key,
        policy.id,
        vec![Ok(microsoft_token_response(
            "ignored-access-token",
            Some("ignored-refresh-token"),
            3600,
        ))],
    ));
    let provider = build_microsoft_graph_credential_token_provider_with_refresher(
        db.clone(),
        encryption_key.to_string(),
        &policy,
        &credential,
        MicrosoftGraphCloud::Global,
        refresher.clone(),
    )
    .expect("provider should build");

    let access_token = provider
        .access_token()
        .await
        .expect("newer DB token should win CAS race");

    assert_eq!(access_token, "newer-access-token");
    let request = refresher
        .requests()
        .into_iter()
        .next()
        .expect("refresh request should be logged");
    assert_eq!(request.refresh_token, "old-refresh-token");
    let stored = storage_policy_credential_repo::find_by_policy_provider_kind(
        &db,
        policy.id,
        StorageCredentialProvider::MicrosoftGraph,
        StorageCredentialKind::OauthDelegated,
    )
    .await
    .expect("credential lookup should succeed")
    .expect("credential should exist");
    assert_eq!(stored.status, StorageCredentialStatus::Authorized);
    assert_eq!(stored.status_reason, None);
    assert_eq!(
        decrypt_stored_oauth_token(
            encryption_key,
            policy.id,
            "access",
            stored.access_token_ciphertext.as_deref().unwrap(),
        ),
        "newer-access-token"
    );
    assert_eq!(
        decrypt_stored_oauth_token(
            encryption_key,
            policy.id,
            "refresh",
            stored.refresh_token_ciphertext.as_deref().unwrap(),
        ),
        "newer-refresh-token"
    );
    assert_eq!(
        serde_json::from_str::<Vec<String>>(&stored.scopes).unwrap(),
        vec![
            "offline_access".to_string(),
            "Files.ReadWrite.All".to_string()
        ]
    );
}

#[tokio::test]
async fn credential_token_provider_refresh_failure_marks_reauth_required() {
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "client-id", "client-secret").await;
    let credential = create_microsoft_graph_credential(
        &db,
        encryption_key,
        policy.id,
        "expired-access-token",
        Some("old-refresh-token"),
        Some(Utc::now() - Duration::minutes(10)),
    )
    .await;
    let refresher = Arc::new(TestMicrosoftGraphTokenRefresher::new(vec![Err(
        AsterError::auth_invalid_credentials("invalid_grant"),
    )]));
    let provider = build_microsoft_graph_credential_token_provider_with_refresher(
        db.clone(),
        encryption_key.to_string(),
        &policy,
        &credential,
        MicrosoftGraphCloud::Global,
        refresher,
    )
    .expect("provider should build");

    let error = provider.access_token().await.unwrap_err();

    assert_eq!(error.storage_error_kind(), Some(StorageErrorKind::Auth));
    let stored = storage_policy_credential_repo::find_by_policy_provider_kind(
        &db,
        policy.id,
        StorageCredentialProvider::MicrosoftGraph,
        StorageCredentialKind::OauthDelegated,
    )
    .await
    .expect("credential lookup should succeed")
    .expect("credential should exist");
    assert_eq!(stored.status, StorageCredentialStatus::ReauthRequired);
    assert!(
        stored
            .status_reason
            .as_deref()
            .unwrap_or_default()
            .contains("invalid_grant")
    );
    assert_eq!(
        decrypt_stored_oauth_token(
            encryption_key,
            policy.id,
            "access",
            stored.access_token_ciphertext.as_deref().unwrap(),
        ),
        "expired-access-token"
    );
}

#[tokio::test]
async fn credential_token_provider_transient_refresh_failure_does_not_mark_reauth_required() {
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "client-id", "client-secret").await;
    let credential = create_microsoft_graph_credential(
        &db,
        encryption_key,
        policy.id,
        "expired-access-token",
        Some("old-refresh-token"),
        Some(Utc::now() - Duration::minutes(10)),
    )
    .await;
    let refresher = Arc::new(TestMicrosoftGraphTokenRefresher::new(vec![Err(
        storage_driver_error(
            StorageErrorKind::Transient,
            "temporary Microsoft Graph outage",
        ),
    )]));
    let provider = build_microsoft_graph_credential_token_provider_with_refresher(
        db.clone(),
        encryption_key.to_string(),
        &policy,
        &credential,
        MicrosoftGraphCloud::Global,
        refresher,
    )
    .expect("provider should build");

    let error = provider.access_token().await.unwrap_err();

    assert_eq!(
        error.storage_error_kind(),
        Some(StorageErrorKind::Transient)
    );
    let stored = storage_policy_credential_repo::find_by_policy_provider_kind(
        &db,
        policy.id,
        StorageCredentialProvider::MicrosoftGraph,
        StorageCredentialKind::OauthDelegated,
    )
    .await
    .expect("credential lookup should succeed")
    .expect("credential should exist");
    assert_eq!(stored.status, StorageCredentialStatus::Authorized);
    assert_eq!(stored.status_reason, None);
}

#[tokio::test]
async fn credential_token_provider_refresh_failure_uses_newer_rotated_db_token() {
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "client-id", "client-secret").await;
    let credential = create_microsoft_graph_credential(
        &db,
        encryption_key,
        policy.id,
        "expired-access-token",
        Some("old-refresh-token"),
        Some(Utc::now() - Duration::minutes(10)),
    )
    .await;
    let refresher = Arc::new(TestMicrosoftGraphTokenRefresher::new(vec![Err(
        AsterError::auth_invalid_credentials("invalid_grant"),
    )]));
    let provider = build_microsoft_graph_credential_token_provider_with_refresher(
        db.clone(),
        encryption_key.to_string(),
        &policy,
        &credential,
        MicrosoftGraphCloud::Global,
        refresher.clone(),
    )
    .expect("provider should build");
    create_microsoft_graph_credential(
        &db,
        encryption_key,
        policy.id,
        "newer-access-token",
        Some("newer-refresh-token"),
        Some(Utc::now() + Duration::minutes(10)),
    )
    .await;

    let access_token = provider
        .access_token()
        .await
        .expect("newer DB token should recover refresh race");

    assert_eq!(access_token, "newer-access-token");
    let request = refresher
        .requests()
        .into_iter()
        .next()
        .expect("refresh request should be logged");
    assert_eq!(request.refresh_token, "old-refresh-token");
    let stored = storage_policy_credential_repo::find_by_policy_provider_kind(
        &db,
        policy.id,
        StorageCredentialProvider::MicrosoftGraph,
        StorageCredentialKind::OauthDelegated,
    )
    .await
    .expect("credential lookup should succeed")
    .expect("credential should exist");
    assert_eq!(stored.status, StorageCredentialStatus::Authorized);
    assert_eq!(stored.status_reason, None);
    assert_eq!(
        decrypt_stored_oauth_token(
            encryption_key,
            policy.id,
            "access",
            stored.access_token_ciphertext.as_deref().unwrap(),
        ),
        "newer-access-token"
    );
    assert_eq!(
        decrypt_stored_oauth_token(
            encryption_key,
            policy.id,
            "refresh",
            stored.refresh_token_ciphertext.as_deref().unwrap(),
        ),
        "newer-refresh-token"
    );
}

#[tokio::test]
async fn credential_token_provider_refresh_failure_rejects_expired_rotated_db_token_without_reauth()
{
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "client-id", "client-secret").await;
    let credential = create_microsoft_graph_credential(
        &db,
        encryption_key,
        policy.id,
        "expired-access-token",
        Some("old-refresh-token"),
        Some(Utc::now() - Duration::minutes(10)),
    )
    .await;
    let refresher = Arc::new(TestMicrosoftGraphTokenRefresher::new(vec![Err(
        AsterError::auth_invalid_credentials("invalid_grant"),
    )]));
    let provider = build_microsoft_graph_credential_token_provider_with_refresher(
        db.clone(),
        encryption_key.to_string(),
        &policy,
        &credential,
        MicrosoftGraphCloud::Global,
        refresher,
    )
    .expect("provider should build");
    create_microsoft_graph_credential(
        &db,
        encryption_key,
        policy.id,
        "also-expired-access-token",
        Some("newer-refresh-token"),
        Some(Utc::now() - Duration::minutes(5)),
    )
    .await;

    let error = provider.access_token().await.unwrap_err();

    assert_eq!(error.storage_error_kind(), Some(StorageErrorKind::Auth));
    assert!(error.message().contains("already rotated"));
    let stored = storage_policy_credential_repo::find_by_policy_provider_kind(
        &db,
        policy.id,
        StorageCredentialProvider::MicrosoftGraph,
        StorageCredentialKind::OauthDelegated,
    )
    .await
    .expect("credential lookup should succeed")
    .expect("credential should exist");
    assert_eq!(stored.status, StorageCredentialStatus::Authorized);
    assert_eq!(stored.status_reason, None);
    assert_eq!(
        decrypt_stored_oauth_token(
            encryption_key,
            policy.id,
            "access",
            stored.access_token_ciphertext.as_deref().unwrap(),
        ),
        "also-expired-access-token"
    );
    assert_eq!(
        decrypt_stored_oauth_token(
            encryption_key,
            policy.id,
            "refresh",
            stored.refresh_token_ciphertext.as_deref().unwrap(),
        ),
        "newer-refresh-token"
    );
}

#[tokio::test]
async fn credential_token_provider_requires_access_token_ciphertext() {
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "client-id", "client-secret").await;
    let mut credential = create_microsoft_graph_credential(
        &db,
        encryption_key,
        policy.id,
        "access-token",
        Some("refresh-token"),
        Some(Utc::now() + Duration::minutes(10)),
    )
    .await;
    credential.access_token_ciphertext = None;

    let error = build_microsoft_graph_credential_token_provider(
        db,
        encryption_key.to_string(),
        &policy,
        &credential,
        MicrosoftGraphCloud::Global,
    )
    .unwrap_err();

    assert_eq!(error.storage_error_kind(), Some(StorageErrorKind::Auth));
    assert!(error.message().contains("missing access token"));
}

#[tokio::test]
async fn credential_token_provider_requires_client_id_from_policy_or_metadata() {
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, " ", "client-secret").await;
    let credential = create_microsoft_graph_credential(
        &db,
        encryption_key,
        policy.id,
        "access-token",
        Some("refresh-token"),
        Some(Utc::now() + Duration::minutes(10)),
    )
    .await;

    let error = build_microsoft_graph_credential_token_provider(
        db,
        encryption_key.to_string(),
        &policy,
        &credential,
        MicrosoftGraphCloud::Global,
    )
    .unwrap_err();

    assert_eq!(error.storage_error_kind(), Some(StorageErrorKind::Auth));
    assert!(
        error
            .message()
            .contains("missing Microsoft Graph client_id")
    );
}
