use super::audit::{
    OAUTH_AUDIT_ACTION_NAME, OAUTH_AUDIT_EVENT_AUTHORIZATION_FAILED,
    OAUTH_AUDIT_EVENT_CREDENTIAL_REFRESHED, OAUTH_AUDIT_PROVIDER, OAUTH_AUDIT_RESULT_FAILED,
    OAUTH_AUDIT_RESULT_SUCCESS, StorageCredentialOauthAuditDetails,
    storage_credential_oauth_audit_details, write_storage_credential_oauth_audit,
};
use super::microsoft::{
    MicrosoftTokenResponse, StorageCredentialMetadataInput, decrypt_application_client_secret,
    encrypt_application_client_secret, microsoft_authorization_url, storage_credential_metadata,
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
use crate::services::storage_policy::credential::{
    default_microsoft_graph_scopes_for_onedrive_options, normalize_scopes_with_default,
};
use crate::storage::StorageErrorKind;
use crate::storage::error::storage_driver_error;
use crate::types::{
    AuditAction, AuditEntityType, DriverType, MicrosoftGraphCloud, OneDriveAccountMode,
    StoragePolicyOptions, StoredStoragePolicyAllowedTypes, UserRole, UserStatus,
};
use migration::Migrator;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder};
use secrecy::ExposeSecret;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex as StdMutex};

fn request_client_secret(request: &MicrosoftGraphTokenRefreshRequest) -> Option<&str> {
    request
        .client_secret
        .as_ref()
        .map(secrecy::ExposeSecret::expose_secret)
}

#[test]
fn microsoft_graph_token_refresh_request_debug_redacts_secrets() {
    let request = MicrosoftGraphTokenRefreshRequest {
        cloud: MicrosoftGraphCloud::Global,
        tenant: "tenant-id".to_string(),
        client_id: "client-id".to_string(),
        client_secret: Some("plain-client-secret".into()),
        refresh_token: "plain-refresh-token".into(),
    };

    let debug = format!("{request:?}");
    assert!(debug.contains(r#"client_secret: Some("***REDACTED***")"#));
    assert!(debug.contains(r#"refresh_token: "***REDACTED***""#));
    assert!(!debug.contains("plain-client-secret"));
    assert!(!debug.contains("plain-refresh-token"));
}

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
        crate::metrics::NoopMetrics::arc(),
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
        crate::metrics::NoopMetrics::arc(),
    )
    .await
    .expect("storage credential test DB should connect");
    Migrator::up(&db, None)
        .await
        .expect("storage credential migrations should succeed");
    (db, db_path)
}

async fn build_oauth_test_state(
    db: sea_orm::DatabaseConnection,
    encryption_key: &str,
) -> crate::runtime::PrimaryAppState {
    let runtime_config = Arc::new(crate::config::RuntimeConfig::new());
    runtime_config.apply(test_config_model(
        crate::config::site_url::PUBLIC_SITE_URL_KEY,
        r#"["https://drive.example.test"]"#,
    ));
    let cache = aster_forge_cache::create_cache(&aster_forge_cache::CacheConfig {
        backend: "memory".to_string(),
        ..Default::default()
    })
    .await;
    let mut config = crate::config::Config::default();
    config.auth.storage_credential_secret_key = encryption_key.to_string();
    let (storage_change_tx, _) = tokio::sync::broadcast::channel(
        crate::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
    );
    let share_download_rollback =
        crate::services::share::spawn_detached_share_download_rollback_queue(
            db.clone(),
            crate::config::operations::share_download_rollback_queue_capacity(&runtime_config),
        );

    crate::runtime::PrimaryAppState {
        db_handles: aster_forge_db::DbHandles::single(db),
        driver_registry: Arc::new(crate::storage::DriverRegistry::noop()),
        runtime_config: runtime_config.clone(),
        policy_snapshot: Arc::new(crate::storage::PolicySnapshot::new()),
        config: Arc::new(config),
        cache,
        config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
        metrics: crate::metrics::NoopMetrics::arc(),
        mail_sender: crate::services::mail::sender::runtime_sender(runtime_config),
        storage_change_tx,
        share_download_rollback,
        background_task_dispatch_wakeup:
            crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
        remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
    }
}

fn test_config_model(key: &str, value: &str) -> aster_forge_db::system_config::Model {
    aster_forge_db::system_config::Model {
        id: 1,
        key: key.to_string(),
        value: value.to_string(),
        value_type: aster_forge_config::ConfigValueType::String,
        requires_restart: false,
        is_sensitive: false,
        source: aster_forge_config::ConfigSource::System,
        visibility: aster_forge_config::ConfigVisibility::Private,
        namespace: String::new(),
        category: crate::config::definitions::CONFIG_CATEGORY_SITE.to_string(),
        description: "test".to_string(),
        updated_at: Utc::now(),
        updated_by: None,
    }
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

async fn create_microsoft_graph_application_config(
    db: &sea_orm::DatabaseConnection,
    encryption_key: &str,
    policy_id: i64,
    client_id: &str,
    client_secret: &str,
) -> crate::entities::storage_connector_application_config::Model {
    upsert_microsoft_graph_application_config(
        db,
        encryption_key,
        policy_id,
        MicrosoftGraphApplicationConfigInput {
            cloud: Some(MicrosoftGraphCloud::Global),
            tenant: Some("common".to_string()),
            client_id: Some(client_id.to_string()),
            client_secret: Some(client_secret.to_string()),
            scopes: Some(vec!["offline_access".to_string()]),
        },
    )
    .await
    .expect("application config should save")
    .expect("application config should exist")
}

async fn create_microsoft_graph_credential_with_metadata(
    db: &sea_orm::DatabaseConnection,
    encryption_key: &str,
    policy_id: i64,
    access_token: &str,
    refresh_token: Option<&str>,
    expires_at: Option<chrono::DateTime<Utc>>,
    metadata: serde_json::Value,
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
            metadata: Set(metadata.to_string()),
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

async fn create_test_user(db: &sea_orm::DatabaseConnection, id: i64) {
    let now = Utc::now();
    crate::entities::user::Entity::insert(crate::entities::user::ActiveModel {
        id: Set(id),
        username: Set(format!("user-{id}")),
        email: Set(format!("user-{id}@example.test")),
        password_hash: Set("not-used".to_string()),
        role: Set(UserRole::Admin),
        status: Set(UserStatus::Active),
        must_change_password: Set(false),
        session_version: Set(0),
        email_verified_at: Set(Some(now)),
        pending_email: Set(None),
        storage_used: Set(0),
        storage_quota: Set(0),
        policy_group_id: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        config: Set(None),
    })
    .exec(db)
    .await
    .expect("test user should insert");
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

#[tokio::test]
async fn microsoft_graph_app_config_is_stored_in_connector_app_config_not_policy_or_credential() {
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "", "").await;

    let app_config = upsert_microsoft_graph_application_config(
        &db,
        encryption_key,
        policy.id,
        MicrosoftGraphApplicationConfigInput {
            cloud: Some(MicrosoftGraphCloud::China),
            tenant: Some(" contoso.partner.onmschina.cn ".to_string()),
            client_id: Some(" client-id ".to_string()),
            client_secret: Some(" client-secret ".to_string()),
            scopes: Some(vec![
                "Files.ReadWrite.All".to_string(),
                "offline_access".to_string(),
                "Files.ReadWrite.All".to_string(),
                " ".to_string(),
            ]),
        },
    )
    .await
    .expect("app config should save")
    .expect("application config row should be created");

    let stored_policy = policy_repo::find_by_id(&db, policy.id)
        .await
        .expect("policy should load");
    assert_eq!(stored_policy.access_key, "");
    assert_eq!(stored_policy.secret_key, "");
    assert_eq!(
        app_config.tenant_id.as_deref(),
        Some("contoso.partner.onmschina.cn")
    );
    assert_eq!(app_config.client_id.as_deref(), Some("client-id"));
    assert_eq!(
        serde_json::from_str::<Vec<String>>(&app_config.scopes).expect("scopes json"),
        vec!["Files.ReadWrite.All", "offline_access"]
    );

    let metadata = parse_metadata(&app_config.metadata).expect("metadata should parse");
    assert_eq!(metadata["cloud"], serde_json::json!("china"));
    let decrypted = decrypt_application_client_secret(
        encryption_key,
        policy.id,
        app_config
            .client_secret_ciphertext
            .as_deref()
            .expect("client secret ciphertext"),
    )
    .expect("client secret should decrypt");
    assert_eq!(decrypted.expose_secret(), "client-secret");

    let credential = storage_policy_credential_repo::find_by_policy_provider_kind(
        &db,
        policy.id,
        StorageCredentialProvider::MicrosoftGraph,
        StorageCredentialKind::OauthDelegated,
    )
    .await
    .expect("credential lookup should succeed");
    assert!(
        credential.is_none(),
        "saving application config must not create an OAuth credential row"
    );
}

#[tokio::test]
async fn microsoft_graph_app_config_update_preserves_authorization_tokens_and_saved_secret() {
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "", "").await;
    upsert_microsoft_graph_application_config(
        &db,
        encryption_key,
        policy.id,
        MicrosoftGraphApplicationConfigInput {
            cloud: Some(MicrosoftGraphCloud::Global),
            tenant: Some("common".to_string()),
            client_id: Some("old-client-id".to_string()),
            client_secret: Some("old-client-secret".to_string()),
            scopes: Some(vec!["offline_access".to_string()]),
        },
    )
    .await
    .expect("initial app config should save");
    create_microsoft_graph_credential_with_metadata(
        &db,
        encryption_key,
        policy.id,
        "old-access-token",
        Some("old-refresh-token"),
        Some(Utc::now() + Duration::minutes(10)),
        serde_json::json!({
            "cloud": MicrosoftGraphCloud::Global,
            "graph_base_url": MicrosoftGraphCloud::Global.graph_base_url(),
            "drive_id": "drive-id",
            "root_item_id": "root"
        }),
    )
    .await;

    let updated_config = upsert_microsoft_graph_application_config(
        &db,
        encryption_key,
        policy.id,
        MicrosoftGraphApplicationConfigInput {
            cloud: Some(MicrosoftGraphCloud::China),
            tenant: Some("organizations".to_string()),
            client_id: Some("new-client-id".to_string()),
            client_secret: Some("   ".to_string()),
            scopes: None,
        },
    )
    .await
    .expect("app config should update")
    .expect("application config row should exist");

    assert_eq!(updated_config.tenant_id.as_deref(), Some("organizations"));
    assert_eq!(updated_config.client_id.as_deref(), Some("new-client-id"));
    let decrypted = decrypt_application_client_secret(
        encryption_key,
        policy.id,
        updated_config
            .client_secret_ciphertext
            .as_deref()
            .expect("client secret ciphertext"),
    )
    .expect("client secret should decrypt");
    assert_eq!(decrypted.expose_secret(), "old-client-secret");

    let updated_credential = storage_policy_credential_repo::find_by_policy_provider_kind(
        &db,
        policy.id,
        StorageCredentialProvider::MicrosoftGraph,
        StorageCredentialKind::OauthDelegated,
    )
    .await
    .expect("credential should load")
    .expect("credential should still exist");
    assert_eq!(
        updated_credential.status,
        StorageCredentialStatus::Authorized
    );
    assert_eq!(
        decrypt_stored_oauth_token(
            encryption_key,
            policy.id,
            "access",
            updated_credential
                .access_token_ciphertext
                .as_deref()
                .unwrap(),
        ),
        "old-access-token"
    );
    assert_eq!(
        decrypt_stored_oauth_token(
            encryption_key,
            policy.id,
            "refresh",
            updated_credential
                .refresh_token_ciphertext
                .as_deref()
                .unwrap(),
        ),
        "old-refresh-token"
    );

    let metadata = parse_metadata(&updated_credential.metadata).expect("metadata should parse");
    assert_eq!(metadata["cloud"], serde_json::json!("global"));
    assert_eq!(metadata["drive_id"], "drive-id");
    assert!(metadata.get("client_id").is_none());
    assert!(metadata.get("client_secret_ciphertext").is_none());
}

#[tokio::test]
async fn microsoft_graph_authorization_uses_metadata_when_policy_legacy_keys_are_empty() {
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "", "").await;
    upsert_microsoft_graph_application_config(
        &db,
        encryption_key,
        policy.id,
        MicrosoftGraphApplicationConfigInput {
            cloud: Some(MicrosoftGraphCloud::Global),
            tenant: Some("organizations".to_string()),
            client_id: Some("metadata-client-id".to_string()),
            client_secret: Some("metadata-client-secret".to_string()),
            scopes: Some(vec![
                "offline_access".to_string(),
                "Files.ReadWrite".to_string(),
            ]),
        },
    )
    .await
    .expect("app config should save");
    create_test_user(&db, 1).await;
    let state = build_oauth_test_state(db, encryption_key).await;
    let req = actix_web::test::TestRequest::default()
        .uri("https://drive.example.test/admin")
        .to_http_request();

    let response = start_authorization(
        &state,
        &req,
        policy.id,
        1,
        StorageAuthorizationStartInput {
            provider: StorageCredentialProvider::MicrosoftGraph,
            microsoft_graph: None,
        },
    )
    .await
    .expect("authorization should start from metadata app config");

    let context = response
        .microsoft_graph
        .expect("Microsoft Graph context should be present");
    assert_eq!(context.client_id, "metadata-client-id");
    assert_eq!(context.tenant, "organizations");
    assert_eq!(context.scopes, vec!["offline_access", "Files.ReadWrite"]);
    assert!(
        response
            .authorization_url
            .contains("client_id=metadata-client-id")
    );
}

#[tokio::test]
async fn microsoft_graph_authorization_requires_secret_when_metadata_is_missing_secret() {
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "", "").await;
    upsert_microsoft_graph_application_config(
        &db,
        encryption_key,
        policy.id,
        MicrosoftGraphApplicationConfigInput {
            client_id: Some("metadata-client-id".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("partial app config should save");
    let state = build_oauth_test_state(db, encryption_key).await;
    let req = actix_web::test::TestRequest::default()
        .uri("https://drive.example.test/admin")
        .to_http_request();

    let error = start_authorization(
        &state,
        &req,
        policy.id,
        1,
        StorageAuthorizationStartInput {
            provider: StorageCredentialProvider::MicrosoftGraph,
            microsoft_graph: None,
        },
    )
    .await
    .expect_err("authorization without client secret should fail");

    assert!(error.to_string().contains("client_secret is required"));
}

#[tokio::test]
async fn microsoft_graph_authorization_rejects_unsaved_application_overrides() {
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "", "").await;
    upsert_microsoft_graph_application_config(
        &db,
        encryption_key,
        policy.id,
        MicrosoftGraphApplicationConfigInput {
            client_id: Some("saved-client-id".to_string()),
            client_secret: Some("saved-client-secret".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("app config should save");
    let state = build_oauth_test_state(db, encryption_key).await;
    let req = actix_web::test::TestRequest::default()
        .uri("https://drive.example.test/admin")
        .to_http_request();

    let error = start_authorization(
        &state,
        &req,
        policy.id,
        1,
        StorageAuthorizationStartInput {
            provider: StorageCredentialProvider::MicrosoftGraph,
            microsoft_graph: Some(MicrosoftGraphAuthorizationInput {
                client_id: Some("unsaved-client-id".to_string()),
                ..Default::default()
            }),
        },
    )
    .await
    .expect_err("authorization should reject unsaved app config overrides");

    assert!(error.to_string().contains("must be saved"));
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
fn storage_metadata_contains_authorization_result_without_application_secret() {
    let metadata = storage_credential_metadata(StorageCredentialMetadataInput {
        cloud: MicrosoftGraphCloud::Global,
        drive_id: "drive-id",
        root_item_id: "root",
        root_item_name: Some("Root"),
        id_token: Some("id-token"),
    })
    .unwrap();
    let parsed = serde_json::from_str::<serde_json::Value>(&metadata).unwrap();

    assert_eq!(parsed["cloud"], "global");
    assert_eq!(parsed["drive_id"], "drive-id");
    assert_eq!(parsed["root_item_id"], "root");
    assert_eq!(parsed["root_item_name"], "Root");
    assert_eq!(parsed["id_token"], "***REDACTED***");
    assert!(parsed.get("client_id").is_none());
    assert!(parsed.get("client_secret_ciphertext").is_none());
    assert!(parsed.get("client_secret_configured").is_none());
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
async fn credential_token_provider_requires_application_config_client_secret() {
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "legacy-client-id", "legacy-client-secret").await;
    let credential = create_microsoft_graph_credential_with_metadata(
        &db,
        encryption_key,
        policy.id,
        "cached-access-token",
        Some("refresh-token"),
        Some(Utc::now() + Duration::minutes(10)),
        serde_json::json!({
            "cloud": MicrosoftGraphCloud::Global,
            "client_id": "client-id",
            "drive_id": "drive-id",
            "root_item_id": "root"
        }),
    )
    .await;
    let application_config = upsert_microsoft_graph_application_config(
        &db,
        encryption_key,
        policy.id,
        MicrosoftGraphApplicationConfigInput {
            client_id: Some("client-id".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("application config should save")
    .expect("application config should exist");

    let error = match build_microsoft_graph_credential_token_provider(
        db,
        encryption_key.to_string(),
        &policy,
        &credential,
        &application_config,
        MicrosoftGraphCloud::Global,
    ) {
        Ok(_) => panic!("provider without app config client_secret should be rejected"),
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
    let application_config = create_microsoft_graph_application_config(
        &db,
        encryption_key,
        policy.id,
        "client-id",
        "client-secret",
    )
    .await;
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
        &application_config,
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
    let client_secret_ciphertext =
        encrypt_application_client_secret(encryption_key, policy.id, "client-secret")
            .expect("client secret should encrypt");
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
            client_id: Some("client-id".to_string()),
            client_secret_ciphertext: Some(client_secret_ciphertext),
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
    assert_eq!(request_client_secret(request), Some("client-secret"));
    assert_eq!(
        request.refresh_token.expose_secret(),
        "snapshot-refresh-token"
    );
}

#[tokio::test]
async fn cleanup_token_provider_uses_snapshot_client_credentials() {
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
        encrypt_application_client_secret(encryption_key, policy.id, "snapshot-client-secret")
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
        request_client_secret(request),
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
    let client_secret_ciphertext =
        encrypt_application_client_secret(encryption_key, policy.id, "client-secret")
            .expect("client secret should encrypt");
    let refresher = Arc::new(TestMicrosoftGraphTokenRefresher::new(Vec::new()));
    let provider = build_microsoft_graph_cleanup_token_provider_with_refresher(
        encryption_key.to_string(),
        &policy,
        MicrosoftGraphCleanupTokenSnapshot {
            cloud: MicrosoftGraphCloud::Global,
            tenant_id: None,
            client_id: Some("client-id".to_string()),
            client_secret_ciphertext: Some(client_secret_ciphertext),
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
    let application_config = create_microsoft_graph_application_config(
        &db,
        encryption_key,
        policy.id,
        "client-id",
        "client-secret",
    )
    .await;
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
        &application_config,
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
    let application_config = create_microsoft_graph_application_config(
        &db,
        encryption_key,
        policy.id,
        "client-id",
        "client-secret",
    )
    .await;
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
        &application_config,
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
    let application_config = create_microsoft_graph_application_config(
        &db,
        encryption_key,
        policy.id,
        "client-id",
        "client-secret",
    )
    .await;
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
        &application_config,
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
    assert_eq!(request_client_secret(&request), Some("client-secret"));
    assert_eq!(request.refresh_token.expose_secret(), "old-refresh-token");

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
    let application_config = create_microsoft_graph_application_config(
        &db,
        encryption_key,
        policy.id,
        "client-id",
        "client-secret",
    )
    .await;
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
        &application_config,
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
    let application_config = create_microsoft_graph_application_config(
        &db,
        encryption_key,
        policy.id,
        "client-id",
        "client-secret",
    )
    .await;
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
        &application_config,
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
    assert_eq!(request.refresh_token.expose_secret(), "old-refresh-token");
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
    let application_config = create_microsoft_graph_application_config(
        &db,
        encryption_key,
        policy.id,
        "client-id",
        "client-secret",
    )
    .await;
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
        &application_config,
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
    let application_config = create_microsoft_graph_application_config(
        &db,
        encryption_key,
        policy.id,
        "client-id",
        "client-secret",
    )
    .await;
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
        &application_config,
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
    let application_config = create_microsoft_graph_application_config(
        &db,
        encryption_key,
        policy.id,
        "client-id",
        "client-secret",
    )
    .await;
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
        &application_config,
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
    assert_eq!(request.refresh_token.expose_secret(), "old-refresh-token");
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
    let application_config = create_microsoft_graph_application_config(
        &db,
        encryption_key,
        policy.id,
        "client-id",
        "client-secret",
    )
    .await;
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
        &application_config,
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
    let application_config = create_microsoft_graph_application_config(
        &db,
        encryption_key,
        policy.id,
        "client-id",
        "client-secret",
    )
    .await;
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
        &application_config,
        MicrosoftGraphCloud::Global,
    )
    .unwrap_err();

    assert_eq!(error.storage_error_kind(), Some(StorageErrorKind::Auth));
    assert!(error.message().contains("missing access token"));
}

#[tokio::test]
async fn credential_token_provider_requires_client_id_from_application_config() {
    let db = setup_db().await;
    let encryption_key = "storage-token-test-master-key-32bytes";
    let policy = create_onedrive_policy(&db, "legacy-client-id", "legacy-client-secret").await;
    let credential = create_microsoft_graph_credential_with_metadata(
        &db,
        encryption_key,
        policy.id,
        "access-token",
        Some("refresh-token"),
        Some(Utc::now() + Duration::minutes(10)),
        serde_json::json!({
            "cloud": MicrosoftGraphCloud::Global,
            "client_secret_configured": true,
            "client_secret_ciphertext": encrypt_application_client_secret(
                encryption_key,
                policy.id,
                "client-secret"
            )
            .expect("client secret should encrypt"),
            "drive_id": "drive-id",
            "root_item_id": "root"
        }),
    )
    .await;
    let application_config = upsert_microsoft_graph_application_config(
        &db,
        encryption_key,
        policy.id,
        MicrosoftGraphApplicationConfigInput {
            client_secret: Some("client-secret".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("application config should save")
    .expect("application config should exist");

    let error = build_microsoft_graph_credential_token_provider(
        db,
        encryption_key.to_string(),
        &policy,
        &credential,
        &application_config,
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
