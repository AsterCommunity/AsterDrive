//! Real multi-primary acceptance tests backed by PostgreSQL and Redis.

use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, Generate, KeyInit},
};
use aster_drive_migration::Migrator;
use aster_forge_test::postgres::{PostgresTestContainer, PostgresTestDatabase};
use aster_forge_test::process::{TestProcess, available_loopback_port};
use aster_forge_test::redis::RedisTestContainer;
use aster_forge_test::smtp::SmtpTestContainer;
use aster_forge_test::suite::TestContainerSuite;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use futures::{SinkExt, StreamExt};
use hkdf::Hkdf;
use reqwest::header::SET_COOKIE;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, Database, DatabaseConnection, EntityTrait,
    QueryFilter, Set,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;
use tokio_util::sync::CancellationToken;

use aster_drive::storage::remote_protocol::tunnel::server::{
    REMOTE_TUNNEL_CONNECT_PATH, REMOTE_TUNNEL_PROXY_PATH_PREFIX, RemoteTunnelStreamFrame,
    RemoteTunnelStreamFrameKind, decode_stream_frame, encode_stream_frame,
};
use aster_drive::storage::remote_protocol::{
    INTERNAL_AUTH_ACCESS_KEY_HEADER, INTERNAL_AUTH_NONCE_HEADER, INTERNAL_AUTH_SIGNATURE_HEADER,
    INTERNAL_AUTH_TIMESTAMP_HEADER, INTERNAL_STORAGE_BASE_PATH, RemoteStorageCapabilities,
    sign_internal_request,
};

const RUNTIME_LEASE_ID: &str = "aster_drive.background_tasks";
const ADMIN_PASSWORD: &str = "AsterDrive-E2E-Password-399!";
const POLICY_GROUP_USER_PASSWORD: &str = "AsterDrive-Policy-User-399!";
const SHARED_SECRET: &str = "asterdrive399abcdef0123456789abcdef0123456789abcdef0123456789abcd";
const INTERNAL_PROXY_SECRET: &str =
    "asterdrive399proxyabcdef0123456789abcdef0123456789abcdef012345";
const DATABASE_FAULT_ROLE_PASSWORD: &str = "AsterDriveDatabaseFault399";
const S3_CONNECTOR_ID: &str = "asterdrive.storage.s3";
const SFTP_CONNECTOR_ID: &str = "asterdrive.storage.sftp";
const CONNECTOR_SCHEMA_VERSION: u32 = 1;
const CONNECTOR_CREDENTIAL_FORMAT_VERSION: u32 = 1;
const STORAGE_CREDENTIAL_INFO: &[u8] = b"asterdrive:storage-credential-token:v1";
const CONNECTOR_CREDENTIAL_AAD_PREFIX: &str = "storage_policy_connector_credential";

// These test-owned mirrors keep process-level fixtures aligned with the private
// built-in connector schemas without widening production module visibility.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MultiPrimaryS3ConnectorConfigV1 {
    endpoint: String,
    bucket: String,
    base_path: String,
    object_storage_upload_strategy: aster_drive_model::types::ObjectStorageUploadStrategy,
    object_storage_download_strategy: aster_drive_model::types::ObjectStorageDownloadStrategy,
    s3_path_style: bool,
    s3_region: String,
    s3_connect_timeout_secs: u64,
    s3_read_timeout_secs: u64,
    s3_operation_timeout_secs: u64,
}

impl Default for MultiPrimaryS3ConnectorConfigV1 {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:9000".to_string(),
            bucket: "asterdrive-e2e".to_string(),
            base_path: String::new(),
            object_storage_upload_strategy:
                aster_drive_model::types::ObjectStorageUploadStrategy::RelayStream,
            object_storage_download_strategy:
                aster_drive_model::types::ObjectStorageDownloadStrategy::RelayStream,
            s3_path_style: true,
            s3_region: "auto".to_string(),
            s3_connect_timeout_secs: 5,
            s3_read_timeout_secs: 30,
            s3_operation_timeout_secs: 3_600,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MultiPrimaryS3StaticCredentialsV1 {
    s3_access_key_id: String,
    s3_secret_access_key: String,
}

impl Default for MultiPrimaryS3StaticCredentialsV1 {
    fn default() -> Self {
        Self {
            s3_access_key_id: "e2e-access".to_string(),
            s3_secret_access_key: "e2e-secret".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MultiPrimarySftpConnectorConfigV1 {
    endpoint: String,
    base_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sftp_host_key_fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MultiPrimarySftpStaticCredentialsV1 {
    sftp_username: String,
    sftp_password: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MultiPrimaryConnectorCredentialCiphertextEnvelope {
    format_version: u32,
    connector_id: String,
    schema_version: u32,
    ciphertext: String,
}

// Multi-primary processes decrypt this stable persistence format at startup.
// Encoding it outside the application crate makes the E2E fixture exercise the
// same boundary as an already-populated database instead of calling internals.
fn encode_multi_primary_policy_config<T: Serialize>(
    connector_id: &str,
    config: T,
) -> aster_drive_model::types::StoredStoragePolicyConfig {
    let values = serde_json::to_value(config).expect("serialize multi-primary connector config");
    let encoded = aster_drive_storage::encode_storage_policy_config(
        aster_drive_storage::ConnectorConfigEnvelope::new(
            aster_drive_storage::ConnectorId::declared(connector_id),
            CONNECTOR_SCHEMA_VERSION,
            values,
        ),
        aster_drive_storage::StoragePolicyBehaviorConfig::default(),
    )
    .expect("encode multi-primary storage policy config");
    aster_drive_model::types::StoredStoragePolicyConfig::from(encoded)
}

fn connector_credential_aad(policy_id: i64, connector_id: &str, schema_version: u32) -> String {
    format!("{CONNECTOR_CREDENTIAL_AAD_PREFIX}:{policy_id}:{connector_id}:{schema_version}")
}

fn multi_primary_credential_cipher() -> Aes256Gcm {
    let hk = Hkdf::<Sha256>::new(None, SHARED_SECRET.as_bytes());
    let mut key = [0_u8; 32];
    hk.expand(STORAGE_CREDENTIAL_INFO, &mut key)
        .expect("derive multi-primary storage credential encryption key");
    Aes256Gcm::new_from_slice(&key).expect("construct multi-primary storage credential cipher")
}

fn encrypt_multi_primary_connector_credential<T: Serialize>(
    policy_id: i64,
    connector_id: &str,
    payload: &T,
) -> String {
    let plaintext = serde_json::to_string(payload)
        .expect("serialize multi-primary connector credential payload");
    let nonce = Nonce::generate();
    let aad = connector_credential_aad(policy_id, connector_id, CONNECTOR_SCHEMA_VERSION);
    let ciphertext = multi_primary_credential_cipher()
        .encrypt(
            &nonce,
            aes_gcm::aead::Payload {
                msg: plaintext.as_bytes(),
                aad: aad.as_bytes(),
            },
        )
        .expect("encrypt multi-primary connector credential payload");
    serde_json::to_string(&MultiPrimaryConnectorCredentialCiphertextEnvelope {
        format_version: CONNECTOR_CREDENTIAL_FORMAT_VERSION,
        connector_id: connector_id.to_string(),
        schema_version: CONNECTOR_SCHEMA_VERSION,
        ciphertext: format!(
            "v1:{}:{}",
            URL_SAFE_NO_PAD.encode(nonce),
            URL_SAFE_NO_PAD.encode(ciphertext)
        ),
    })
    .expect("serialize multi-primary connector credential ciphertext envelope")
}

fn decrypt_multi_primary_connector_credential<T: for<'de> Deserialize<'de>>(
    policy_id: i64,
    connector_id: &str,
    schema_version: u32,
    raw: &str,
) -> Result<T, String> {
    let envelope: MultiPrimaryConnectorCredentialCiphertextEnvelope =
        serde_json::from_str(raw).map_err(|error| error.to_string())?;
    if envelope.format_version != CONNECTOR_CREDENTIAL_FORMAT_VERSION
        || envelope.connector_id != connector_id
        || envelope.schema_version != schema_version
    {
        return Err("connector credential ciphertext envelope identity mismatch".to_string());
    }
    let mut parts = envelope.ciphertext.split(':');
    if parts.next() != Some("v1") {
        return Err("connector credential ciphertext version mismatch".to_string());
    }
    let nonce = parts
        .next()
        .ok_or_else(|| "connector credential ciphertext nonce is missing".to_string())?;
    let ciphertext = parts
        .next()
        .ok_or_else(|| "connector credential ciphertext body is missing".to_string())?;
    if parts.next().is_some() {
        return Err("connector credential ciphertext has trailing fields".to_string());
    }
    let nonce = URL_SAFE_NO_PAD
        .decode(nonce)
        .map_err(|error| error.to_string())?;
    let nonce: [u8; 12] = nonce
        .try_into()
        .map_err(|_| "connector credential nonce length mismatch".to_string())?;
    let nonce = Nonce::try_from(nonce.as_slice())
        .map_err(|_| "connector credential nonce length mismatch".to_string())?;
    let ciphertext = URL_SAFE_NO_PAD
        .decode(ciphertext)
        .map_err(|error| error.to_string())?;
    let aad = connector_credential_aad(policy_id, connector_id, schema_version);
    let plaintext = multi_primary_credential_cipher()
        .decrypt(
            &nonce,
            aes_gcm::aead::Payload {
                msg: &ciphertext,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&plaintext).map_err(|error| error.to_string())
}

async fn persist_multi_primary_connector_credential<T: Serialize>(
    database: &DatabaseConnection,
    policy_id: i64,
    connector_id: &str,
    payload: &T,
) -> aster_drive_model::entities::storage_policy_connector_credential::Model {
    let ciphertext = encrypt_multi_primary_connector_credential(policy_id, connector_id, payload);
    aster_drive::db::repository::storage_policy_connector_credential_repo::upsert(
        database,
        policy_id,
        connector_id.to_string(),
        i32::try_from(CONNECTOR_SCHEMA_VERSION)
            .expect("connector schema version should fit database column"),
        ciphertext,
    )
    .await
    .expect("persist multi-primary connector credential")
}

async fn create_multi_primary_s3_policy(
    database: &DatabaseConnection,
    name: &str,
    max_file_size: i64,
    is_default: bool,
) -> aster_drive_model::entities::storage_policy::Model {
    let now = Utc::now();
    let policy = aster_drive::db::repository::policy_repo::create(
        database,
        aster_drive_model::entities::storage_policy::ActiveModel {
            name: Set(name.to_string()),
            connector_id: Set(S3_CONNECTOR_ID.to_string()),
            storage_config: Set(encode_multi_primary_policy_config(
                S3_CONNECTOR_ID,
                MultiPrimaryS3ConnectorConfigV1::default(),
            )),
            max_file_size: Set(max_file_size),
            allowed_types: Set(aster_drive_model::types::StoredStoragePolicyAllowedTypes::empty()),
            is_default: Set(is_default),
            chunk_size: Set(5_242_880),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .expect("create multi-primary S3 storage policy");
    let credential = persist_multi_primary_connector_credential(
        database,
        policy.id,
        S3_CONNECTOR_ID,
        &MultiPrimaryS3StaticCredentialsV1::default(),
    )
    .await;
    assert_eq!(credential.revision, 1);
    policy
}

#[test]
fn multi_primary_policy_fixtures_encode_current_connector_contracts() {
    let s3_config = MultiPrimaryS3ConnectorConfigV1::default();
    let encoded_s3 = encode_multi_primary_policy_config(S3_CONNECTOR_ID, s3_config.clone());
    let (decoded_s3, behavior) =
        aster_drive_storage::decode_storage_policy_config::<MultiPrimaryS3ConnectorConfigV1>(
            encoded_s3.as_ref(),
            &aster_drive_storage::ConnectorId::declared(S3_CONNECTOR_ID),
            CONNECTOR_SCHEMA_VERSION,
        )
        .expect("decode multi-primary S3 policy fixture");
    assert_eq!(decoded_s3, s3_config);
    assert_eq!(
        behavior,
        aster_drive_storage::StoragePolicyBehaviorConfig::default()
    );
    assert!(!encoded_s3.as_ref().contains("s3_access_key_id"));
    assert!(!encoded_s3.as_ref().contains("s3_secret_access_key"));

    let sftp_config = MultiPrimarySftpConnectorConfigV1 {
        endpoint: "sftp://127.0.0.1:22".to_string(),
        base_path: String::new(),
        sftp_host_key_fingerprint: None,
    };
    let encoded_sftp = encode_multi_primary_policy_config(SFTP_CONNECTOR_ID, sftp_config.clone());
    let (decoded_sftp, _) =
        aster_drive_storage::decode_storage_policy_config::<MultiPrimarySftpConnectorConfigV1>(
            encoded_sftp.as_ref(),
            &aster_drive_storage::ConnectorId::declared(SFTP_CONNECTOR_ID),
            CONNECTOR_SCHEMA_VERSION,
        )
        .expect("decode multi-primary SFTP policy fixture");
    assert_eq!(decoded_sftp, sftp_config);
    assert!(!encoded_sftp.as_ref().contains("sftp_username"));
    assert!(!encoded_sftp.as_ref().contains("sftp_password"));
}

#[test]
fn multi_primary_connector_credential_fixture_encrypts_and_binds_identity() {
    let credentials = MultiPrimaryS3StaticCredentialsV1::default();
    let encrypted = encrypt_multi_primary_connector_credential(7, S3_CONNECTOR_ID, &credentials);

    assert!(!encrypted.contains(&credentials.s3_access_key_id));
    assert!(!encrypted.contains(&credentials.s3_secret_access_key));
    assert_eq!(
        decrypt_multi_primary_connector_credential::<MultiPrimaryS3StaticCredentialsV1>(
            7,
            S3_CONNECTOR_ID,
            CONNECTOR_SCHEMA_VERSION,
            &encrypted,
        )
        .expect("decrypt matching multi-primary connector credential fixture"),
        credentials
    );
    assert!(
        decrypt_multi_primary_connector_credential::<MultiPrimaryS3StaticCredentialsV1>(
            8,
            S3_CONNECTOR_ID,
            CONNECTOR_SCHEMA_VERSION,
            &encrypted,
        )
        .is_err(),
        "credential ciphertext must be bound to its policy"
    );
    assert!(
        decrypt_multi_primary_connector_credential::<MultiPrimaryS3StaticCredentialsV1>(
            7,
            SFTP_CONNECTOR_ID,
            CONNECTOR_SCHEMA_VERSION,
            &encrypted,
        )
        .is_err(),
        "credential envelope must reject a different connector"
    );
    assert!(
        decrypt_multi_primary_connector_credential::<MultiPrimaryS3StaticCredentialsV1>(
            7,
            S3_CONNECTOR_ID,
            CONNECTOR_SCHEMA_VERSION + 1,
            &encrypted,
        )
        .is_err(),
        "credential envelope must reject a different schema version"
    );

    let mut tampered: MultiPrimaryConnectorCredentialCiphertextEnvelope =
        serde_json::from_str(&encrypted).expect("decode encrypted fixture envelope");
    tampered.ciphertext.push('A');
    let tampered = serde_json::to_string(&tampered).expect("encode tampered fixture envelope");
    assert!(
        decrypt_multi_primary_connector_credential::<MultiPrimaryS3StaticCredentialsV1>(
            7,
            S3_CONNECTOR_ID,
            CONNECTOR_SCHEMA_VERSION,
            &tampered,
        )
        .is_err(),
        "credential ciphertext must reject tampering"
    );
}

struct DatabaseFaultRole {
    name: String,
    url: String,
}

struct SharedServices {
    postgres: PostgresTestContainer,
    database: PostgresTestDatabase,
    redis: RedisTestContainer,
    smtp: SmtpTestContainer,
    database_url: String,
    redis_url: String,
    config_topic: String,
}

impl SharedServices {
    async fn start() -> Self {
        Self::start_with_seeded_database_in_suite(true, test_suite()).await
    }

    async fn start_empty() -> Self {
        Self::start_with_seeded_database_in_suite(false, test_suite()).await
    }

    async fn start_for_redis_readiness_outage() -> Self {
        Self::start_with_seeded_database_in_suite(true, redis_readiness_test_suite()).await
    }

    async fn start_with_seeded_database_in_suite(
        seed_database: bool,
        suite: &TestContainerSuite,
    ) -> Self {
        let postgres = PostgresTestContainer::start(suite).await;
        let smtp = SmtpTestContainer::start(suite).await;
        smtp.clear_messages().await;
        let database_name = format!("asterdrive_multi_primary_{}", uuid::Uuid::new_v4().simple());
        let test_database = postgres.create_database(&database_name).await;
        let database_url = test_database.url().to_string();
        let database = test_database.connect().await;
        if seed_database {
            Migrator::up(&database, None)
                .await
                .expect("apply migrations to isolated multi-primary database");
            aster_drive_migration::with_database_migration_lock(&database, |transaction| {
                Box::pin(aster_drive_migration::finalize_storage_policy_upgrade(
                    transaction,
                ))
            })
            .await
            .expect("finalize isolated multi-primary storage policy schema");
            create_multi_primary_s3_policy(&database, "E2E Shared Object Storage", 0, true).await;
            aster_drive::services::storage_policy::policy::ensure_policy_groups_seeded(&database)
                .await
                .expect("seed default E2E storage policy group");
            seed_runtime_config(&database, smtp.smtp_address().port()).await;
        }
        database
            .close()
            .await
            .expect("close isolated database seed connection");

        let redis = RedisTestContainer::start(suite).await;

        Self {
            database_url,
            database: test_database,
            redis_url: redis.url().to_string(),
            postgres,
            redis,
            smtp,
            config_topic: format!(
                "aster_drive.multi_primary_e2e.{}",
                uuid::Uuid::new_v4().simple()
            ),
        }
    }

    async fn connect_database(&self) -> DatabaseConnection {
        self.database.connect().await
    }

    async fn cleanup_database(&self) {
        self.database.cleanup().await;
    }

    async fn create_database_fault_role(&self) -> DatabaseFaultRole {
        let name = format!("asterdrive_fault_{}", uuid::Uuid::new_v4().simple());
        let database_name = self.database.name();
        let admin = Database::connect(self.postgres.admin_url())
            .await
            .expect("connect to PostgreSQL admin database for fault role");
        admin
            .execute_unprepared(&format!(
                "CREATE ROLE {name} LOGIN PASSWORD '{DATABASE_FAULT_ROLE_PASSWORD}'; \
                 GRANT CONNECT ON DATABASE {database_name} TO {name};"
            ))
            .await
            .expect("create isolated PostgreSQL fault role");
        admin
            .close()
            .await
            .expect("close PostgreSQL admin connection after creating fault role");

        let database = self.connect_database().await;
        database
            .execute_unprepared(&format!(
                "GRANT USAGE, CREATE ON SCHEMA public TO {name}; \
                 GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO {name}; \
                 GRANT USAGE, SELECT, UPDATE ON ALL SEQUENCES IN SCHEMA public TO {name}; \
                 GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA public TO {name};"
            ))
            .await
            .expect("grant database access to isolated PostgreSQL fault role");
        database
            .close()
            .await
            .expect("close fault role grant connection");

        let mut url = url::Url::parse(&self.database_url)
            .expect("multi-primary PostgreSQL URL should be valid");
        url.set_username(&name)
            .expect("fault role username should be URL-safe");
        url.set_password(Some(DATABASE_FAULT_ROLE_PASSWORD))
            .expect("fault role password should be URL-safe");
        DatabaseFaultRole {
            name,
            url: url.into(),
        }
    }

    async fn set_database_fault_role_login(&self, role: &DatabaseFaultRole, enabled: bool) {
        let admin = Database::connect(self.postgres.admin_url())
            .await
            .expect("connect to PostgreSQL admin database for fault injection");
        let login = if enabled { "LOGIN" } else { "NOLOGIN" };
        admin
            .execute_unprepared(&format!("ALTER ROLE {} {login}", role.name))
            .await
            .expect("update PostgreSQL fault role login state");
        if !enabled {
            admin
                .execute_unprepared(&format!(
                    "SELECT pg_terminate_backend(pid) FROM pg_stat_activity \
                     WHERE usename = '{}' AND pid <> pg_backend_pid()",
                    role.name
                ))
                .await
                .expect("terminate isolated PostgreSQL fault role sessions");
        }
        admin
            .close()
            .await
            .expect("close PostgreSQL fault injection connection");
    }

    async fn drop_database_fault_role(&self, role: &DatabaseFaultRole) {
        let admin = Database::connect(self.postgres.admin_url())
            .await
            .expect("connect to PostgreSQL admin database to drop fault role");
        admin
            .execute_unprepared(&format!("DROP ROLE IF EXISTS {}", role.name))
            .await
            .expect("drop isolated PostgreSQL fault role");
        admin
            .close()
            .await
            .expect("close PostgreSQL admin connection after dropping fault role");
    }
}

async fn seed_runtime_config(database: &DatabaseConnection, smtp_port: u16) {
    aster_drive::db::repository::config_repo::ensure_defaults_with_env(
        database,
        &|_| None::<String>,
    )
    .await
    .expect("seed runtime config defaults");

    let values = [
        (
            aster_drive::config::definitions::MAIL_OUTBOX_DISPATCH_INTERVAL_SECS_KEY,
            "1".to_string(),
        ),
        (
            aster_drive::config::definitions::BACKGROUND_TASK_DISPATCH_INTERVAL_SECS_KEY,
            "1".to_string(),
        ),
        (
            aster_drive::config::definitions::BACKGROUND_TASK_DISPATCH_IDLE_MAX_INTERVAL_SECS_KEY,
            "1".to_string(),
        ),
        (
            aster_drive::config::definitions::REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS_KEY,
            "1".to_string(),
        ),
        (
            aster_drive::config::definitions::MAIL_SMTP_HOST_KEY,
            "127.0.0.1".to_string(),
        ),
        (
            aster_drive::config::definitions::MAIL_SMTP_PORT_KEY,
            smtp_port.to_string(),
        ),
        (
            aster_drive::config::definitions::MAIL_FROM_ADDRESS_KEY,
            "asterdrive-e2e@example.com".to_string(),
        ),
        (
            aster_drive::config::definitions::MAIL_FROM_NAME_KEY,
            "AsterDrive E2E".to_string(),
        ),
        (
            aster_drive::config::definitions::MAIL_SECURITY_KEY,
            "false".to_string(),
        ),
    ];
    for (key, value) in values {
        aster_drive::db::repository::config_repo::upsert_with_actor(database, key, &value, None)
            .await
            .unwrap_or_else(|error| panic!("seed runtime config {key}: {error}"));
    }
}

async fn configure_default_sftp_policy(database: &DatabaseConnection) -> i64 {
    let policy = aster_drive::db::repository::policy_repo::find_default(database)
        .await
        .expect("load default E2E storage policy")
        .expect("default E2E storage policy should exist");
    let policy_id = policy.id;
    let mut active: aster_drive_model::entities::storage_policy::ActiveModel = policy.into();
    active.connector_id = Set(SFTP_CONNECTOR_ID.to_string());
    active.storage_config = Set(encode_multi_primary_policy_config(
        SFTP_CONNECTOR_ID,
        MultiPrimarySftpConnectorConfigV1 {
            endpoint: "sftp://127.0.0.1:22".to_string(),
            base_path: String::new(),
            sftp_host_key_fingerprint: None,
        },
    ));
    active
        .update(database)
        .await
        .expect("configure default SFTP policy for cluster staging E2E");
    let credential = persist_multi_primary_connector_credential(
        database,
        policy_id,
        SFTP_CONNECTOR_ID,
        &MultiPrimarySftpStaticCredentialsV1 {
            sftp_username: "asterdrive-e2e".to_string(),
            sftp_password: "unused-before-staging-validation".to_string(),
        },
    )
    .await;
    assert_eq!(credential.connector_id, SFTP_CONNECTOR_ID);
    assert_eq!(credential.revision, 2);
    policy_id
}

fn test_suite() -> &'static TestContainerSuite {
    static SUITE: OnceLock<TestContainerSuite> = OnceLock::new();
    SUITE.get_or_init(|| TestContainerSuite::new("asterdrive-multi-primary"))
}

fn redis_readiness_test_suite() -> &'static TestContainerSuite {
    static SUITE: OnceLock<TestContainerSuite> = OnceLock::new();
    SUITE.get_or_init(|| TestContainerSuite::new("asterdrive-redis-readiness"))
}

fn e2e_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

struct ServerProcess {
    port: u16,
    process: TestProcess,
}

impl ServerProcess {
    fn spawn(name: &str, services: &SharedServices) -> Self {
        Self::spawn_with_database_pool_size(name, services, 5)
    }

    fn spawn_with_database_pool_size(
        name: &str,
        services: &SharedServices,
        database_pool_size: u32,
    ) -> Self {
        Self::spawn_with_database_url(name, services, database_pool_size, &services.database_url)
    }

    fn spawn_with_database_url(
        name: &str,
        services: &SharedServices,
        database_pool_size: u32,
        database_url: &str,
    ) -> Self {
        let port = available_loopback_port();
        let mut command = Command::new(env!("CARGO_BIN_EXE_aster_drive"));
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("ASTER__") {
                command.env_remove(key);
            }
        }
        command
            .env("ASTER__DEPLOYMENT__PROFILE", "cluster")
            .env(
                "ASTER__DEPLOYMENT__INTERNAL_ENDPOINT",
                format!("http://127.0.0.1:{port}"),
            )
            .env(
                "ASTER__DEPLOYMENT__INTERNAL_PROXY_SECRET",
                INTERNAL_PROXY_SECRET,
            )
            .env("ASTER__SERVER__HOST", "127.0.0.1")
            .env("ASTER__SERVER__PORT", port.to_string())
            .env("ASTER__SERVER__WORKERS", "1")
            .env("ASTER__DATABASE__URL", database_url)
            .env("ASTER__DATABASE__POOL_SIZE", database_pool_size.to_string())
            .env("ASTER__CACHE__BACKEND", "redis")
            .env("ASTER__CACHE__ENDPOINT", &services.redis_url)
            .env("ASTER__CONFIG_SYNC__BACKEND", "redis")
            .env("ASTER__CONFIG_SYNC__ENDPOINT", &services.redis_url)
            .env("ASTER__CONFIG_SYNC__TOPIC", &services.config_topic)
            .env("ASTER__AUTH__JWT_SECRET", SHARED_SECRET)
            .env("ASTER__AUTH__SHARE_COOKIE_SECRET", SHARED_SECRET)
            .env("ASTER__AUTH__DIRECT_LINK_SECRET", SHARED_SECRET)
            .env("ASTER__AUTH__MFA_SECRET_KEY", SHARED_SECRET)
            .env("ASTER__AUTH__STORAGE_CREDENTIAL_SECRET_KEY", SHARED_SECRET)
            .env("ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES", "true")
            .env("ASTER__LOGGING__LEVEL", "warn");
        let process = TestProcess::spawn(name, &mut command);

        Self { port, process }
    }

    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn name(&self) -> &str {
        self.process.name()
    }

    fn terminate(&mut self) {
        self.process.terminate();
    }

    #[cfg(unix)]
    fn terminate_gracefully(&mut self) {
        assert!(
            self.process.terminate_gracefully(Duration::from_secs(20)),
            "primary {} did not stop after SIGTERM\n{}",
            self.name(),
            self.diagnostics()
        );
    }

    fn assert_running(&mut self) {
        self.process.assert_running();
    }

    fn diagnostics(&self) -> String {
        self.process.diagnostics()
    }
}

async fn wait_for_health(client: &reqwest::Client, server: &mut ServerProcess) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
    loop {
        server.assert_running();
        if let Ok(response) = client
            .get(format!("{}/health", server.base_url()))
            .send()
            .await
            && response.status().is_success()
        {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "primary {} did not become healthy\n{}",
                server.name(),
                server.diagnostics()
            );
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn wait_for_ready_code(
    client: &reqwest::Client,
    server: &mut ServerProcess,
    expected_code: &str,
    timeout: Duration,
) -> Value {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        server.assert_running();
        let last_response = match client
            .get(format!("{}/health/ready", server.base_url()))
            .send()
            .await
        {
            Ok(response) => {
                let status = response.status();
                match response.json::<Value>().await {
                    Ok(body) if body["code"] == expected_code => return body,
                    Ok(body) => format!("{status}: {body}"),
                    Err(error) => format!("{status}: {error}"),
                }
            }
            Err(error) => error.to_string(),
        };

        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for readiness code {expected_code} from {}: {}",
            server.name(),
            last_response
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn wait_for_ready_status(
    client: &reqwest::Client,
    server: &mut ServerProcess,
    expected_status: reqwest::StatusCode,
    timeout: Duration,
) -> Value {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        server.assert_running();
        let response = client
            .get(format!("{}/health/ready", server.base_url()))
            .send()
            .await;
        let last_response = match response {
            Ok(response) => {
                let status = response.status();
                match response.json::<Value>().await {
                    Ok(body) if status == expected_status => return body,
                    Ok(body) => format!("{status}: {body}"),
                    Err(error) => format!("{status}: {error}"),
                }
            }
            Err(error) => error.to_string(),
        };

        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for readiness status {expected_status} from {}: {}",
            server.name(),
            last_response
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn setup_and_login(client: &reqwest::Client, server: &ServerProcess) -> String {
    let setup_response = client
        .post(format!("{}/api/v1/auth/setup", server.base_url()))
        .json(&json!({
            "username": "admin",
            "email": "admin@example.com",
            "password": ADMIN_PASSWORD,
        }))
        .send()
        .await
        .expect("send initial admin setup request");
    let setup_status = setup_response.status();
    let setup_body = setup_response.text().await.expect("read setup response");
    assert_eq!(
        setup_status.as_u16(),
        201,
        "admin setup failed: {setup_body}"
    );

    login(client, server, "admin", ADMIN_PASSWORD).await
}

async fn login(
    client: &reqwest::Client,
    server: &ServerProcess,
    identifier: &str,
    password: &str,
) -> String {
    let login_response = client
        .post(format!("{}/api/v1/auth/login", server.base_url()))
        .json(&json!({
            "identifier": identifier,
            "password": password,
        }))
        .send()
        .await
        .expect("send login request");
    let login_status = login_response.status();
    let access_token = cookie_value(&login_response, "aster_access");
    let login_body = login_response.text().await.expect("read login response");
    assert!(
        login_status.is_success(),
        "login for {identifier} failed with {login_status}: {login_body}"
    );
    access_token.unwrap_or_else(|| {
        panic!("login response for {identifier} omitted aster_access: {login_body}")
    })
}

fn cookie_value(response: &reqwest::Response, name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    response
        .headers()
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|value| {
            value
                .strip_prefix(&prefix)
                .and_then(|value| value.split(';').next())
                .map(str::to_string)
        })
}

async fn set_registration_enabled(
    client: &reqwest::Client,
    server: &ServerProcess,
    access_token: &str,
    enabled: bool,
) {
    let response = client
        .put(format!(
            "{}/api/v1/admin/config/auth_allow_user_registration",
            server.base_url()
        ))
        .bearer_auth(access_token)
        .json(&json!({ "value": enabled.to_string() }))
        .send()
        .await
        .expect("send runtime config mutation");
    let status = response.status();
    let body = response
        .text()
        .await
        .expect("read config mutation response");
    assert!(
        status.is_success(),
        "config mutation failed with {status}: {body}"
    );
}

async fn registration_enabled(client: &reqwest::Client, server: &ServerProcess) -> bool {
    let response = client
        .post(format!("{}/api/v1/auth/check", server.base_url()))
        .send()
        .await
        .expect("send public runtime config probe");
    let status = response.status();
    let body: Value = response.json().await.expect("decode public config probe");
    assert!(status.is_success(), "public config probe failed: {body}");
    body["data"]["allow_user_registration"]
        .as_bool()
        .expect("allow_user_registration should be boolean")
}

async fn wait_for_registration_enabled(
    client: &reqwest::Client,
    server: &mut ServerProcess,
    expected: bool,
    timeout: Duration,
) {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        server.assert_running();
        if registration_enabled(client, server).await == expected {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "primary {} did not reconcile allow_user_registration={expected}\n{}",
                server.name(),
                server.diagnostics()
            );
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn wait_for_runtime_lease(
    database: &DatabaseConnection,
    server: &mut ServerProcess,
) -> aster_forge_db::runtime_lease::Model {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        server.assert_running();
        if let Some(lease) = aster_forge_db::runtime_lease::Entity::find_by_id(RUNTIME_LEASE_ID)
            .one(database)
            .await
            .expect("query runtime lease")
        {
            return lease;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "primary {} did not acquire runtime lease\n{}",
                server.name(),
                server.diagnostics()
            );
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn load_single_runtime_lease(
    database: &DatabaseConnection,
) -> aster_forge_db::runtime_lease::Model {
    let leases = aster_forge_db::runtime_lease::Entity::find()
        .all(database)
        .await
        .expect("list runtime leases");
    assert_eq!(leases.len(), 1, "only one runtime lease row may exist");
    let lease = leases
        .into_iter()
        .next()
        .expect("runtime lease should exist");
    assert_eq!(lease.lease_id, RUNTIME_LEASE_ID);
    lease
}

async fn assert_single_live_runtime_lease(
    database: &DatabaseConnection,
) -> aster_forge_db::runtime_lease::Model {
    let lease = load_single_runtime_lease(database).await;
    assert!(
        lease.expires_at > Utc::now(),
        "active runtime lease must not be expired"
    );
    lease
}

async fn wait_for_new_runtime_owner(
    database: &DatabaseConnection,
    server: &mut ServerProcess,
    previous_owner: &str,
    timeout: Duration,
) -> aster_forge_db::runtime_lease::Model {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        server.assert_running();
        if let Some(lease) = aster_forge_db::runtime_lease::Entity::find_by_id(RUNTIME_LEASE_ID)
            .one(database)
            .await
            .expect("query runtime lease during takeover")
            && lease.owner_id != previous_owner
        {
            assert!(
                lease.expires_at > Utc::now(),
                "new runtime lease owner must publish a live expiration"
            );
            return lease;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "primary {} did not take over the runtime lease\n{}",
                server.name(),
                server.diagnostics()
            );
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn create_trash_purge_task(
    client: &reqwest::Client,
    server: &ServerProcess,
    access_token: &str,
) -> i64 {
    let response = client
        .delete(format!("{}/api/v1/trash", server.base_url()))
        .bearer_auth(access_token)
        .send()
        .await
        .expect("create trash purge task");
    let status = response.status();
    let body: Value = response.json().await.expect("decode trash purge response");
    assert!(status.is_success(), "trash purge request failed: {body}");
    body["data"]["id"]
        .as_i64()
        .expect("trash purge response should include task id")
}

async fn wait_for_background_task(
    database: &DatabaseConnection,
    task_id: i64,
    server_a: &mut ServerProcess,
    server_b: &mut ServerProcess,
    timeout: Duration,
) -> aster_drive_model::entities::background_task::Model {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        server_a.assert_running();
        server_b.assert_running();
        let task = aster_drive_model::entities::background_task::Entity::find_by_id(task_id)
            .one(database)
            .await
            .expect("query background task")
            .expect("background task should exist");
        if task.status.is_terminal() {
            return task;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "background task {task_id} did not finish: {:?}\n{}\n{}",
                task.status,
                server_a.diagnostics(),
                server_b.diagnostics()
            );
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn create_due_invitation_mail(
    database: &DatabaseConnection,
) -> aster_forge_db::mail_outbox::Model {
    let now = Utc::now();
    let payload = aster_drive::services::mail::template::MailTemplatePayload::user_invitation(
        "e2e@example.com",
        "https://drive.example.com/invite/e2e",
        "AsterDrive E2E",
        "1 hour",
    )
    .to_stored()
    .expect("serialize E2E mail payload");
    aster_forge_db::create_mail_outbox_row(
        database,
        aster_forge_db::MailOutboxCreate {
            template_code: aster_forge_mail::MailTemplateCode::UserInvitation,
            to_address: "e2e@example.com".to_string(),
            to_name: Some("E2E Recipient".to_string()),
            payload_json: payload,
            next_attempt_at: now,
            now,
        },
    )
    .await
    .expect("create E2E mail outbox row")
}

async fn wait_for_mail_outbox_sent(
    database: &DatabaseConnection,
    outbox_id: i64,
    services: &SharedServices,
    server_a: &mut ServerProcess,
    server_b: &mut ServerProcess,
) -> aster_forge_db::mail_outbox::Model {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        server_a.assert_running();
        server_b.assert_running();
        let row = aster_forge_db::mail_outbox::Entity::find_by_id(outbox_id)
            .one(database)
            .await
            .expect("query mail outbox row")
            .expect("mail outbox row should exist");
        let messages = services.smtp.message_count().await;
        if row.status == aster_forge_mail::MailOutboxStatus::Sent && messages >= 1 {
            return row;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "mail outbox row {outbox_id} did not send: status={:?}, accepted={}\n{}\n{}",
                row.status,
                messages,
                server_a.diagnostics(),
                server_b.diagnostics()
            );
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn scheduled_runtime_records(
    database: &DatabaseConnection,
    task_name: &str,
) -> Vec<aster_drive_model::entities::background_task::Model> {
    aster_drive_model::entities::background_task::Entity::find()
        .filter(
            aster_drive_model::entities::background_task::Column::Kind
                .eq(aster_drive_model::types::BackgroundTaskKind::SystemRuntime),
        )
        .all(database)
        .await
        .expect("query scheduled runtime records")
        .into_iter()
        .filter(|task| {
            task.dedupe_key.is_some()
                && serde_json::from_str::<Value>(task.payload_json.as_ref())
                    .ok()
                    .and_then(|payload| payload["task_name"].as_str().map(str::to_string))
                    .as_deref()
                    == Some(task_name)
        })
        .collect()
}

struct SyntheticTunnelFollower {
    shutdown: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl SyntheticTunnelFollower {
    async fn stop(self) {
        self.shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(5), self.task)
            .await
            .expect("synthetic tunnel follower should stop before timeout")
            .expect("synthetic tunnel follower task should join");
    }
}

async fn seed_reverse_tunnel_node(
    database: &DatabaseConnection,
) -> aster_drive_model::entities::managed_follower::Model {
    let now = Utc::now();
    let node = aster_drive_model::entities::managed_follower::ActiveModel {
        name: Set("multi-primary reverse tunnel follower".to_string()),
        base_url: Set(String::new()),
        access_key: Set(format!("e2e-access-{}", uuid::Uuid::new_v4().simple())),
        secret_key: Set(format!("e2e-secret-{}", uuid::Uuid::new_v4().simple())),
        is_enabled: Set(true),
        transport_mode: Set(aster_drive_model::types::RemoteNodeTransportMode::ReverseTunnel),
        last_capabilities: Set("{}".to_string()),
        last_error: Set(String::new()),
        last_checked_at: Set(None),
        tunnel_last_error: Set(String::new()),
        tunnel_last_seen_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(database)
    .await
    .expect("insert reverse tunnel E2E remote node");

    aster_drive_model::entities::follower_enrollment_session::ActiveModel {
        managed_follower_id: Set(node.id),
        token_hash: Set(format!("e2e-token-{}", uuid::Uuid::new_v4().simple())),
        ack_token_hash: Set(format!("e2e-ack-{}", uuid::Uuid::new_v4().simple())),
        expires_at: Set(now + chrono::Duration::minutes(30)),
        redeemed_at: Set(Some(now)),
        acked_at: Set(Some(now)),
        invalidated_at: Set(None),
        created_at: Set(now),
        ..Default::default()
    }
    .insert(database)
    .await
    .expect("mark reverse tunnel E2E enrollment complete");
    node
}

async fn defer_remote_node_health_tests(database: &DatabaseConnection) {
    aster_drive::db::repository::config_repo::upsert_with_actor(
        database,
        aster_drive::config::definitions::REMOTE_NODE_HEALTH_TEST_INTERVAL_SECS_KEY,
        "3600",
        None,
    )
    .await
    .expect("defer automatic remote-node health tests during tunnel routing E2E");
}

fn spawn_synthetic_tunnel_follower(
    primary_url: String,
    remote_node: aster_drive_model::entities::managed_follower::Model,
) -> SyntheticTunnelFollower {
    let shutdown = CancellationToken::new();
    let worker_shutdown = shutdown.clone();
    let task = tokio::spawn(async move {
        while !worker_shutdown.is_cancelled() {
            let result = run_synthetic_tunnel_connection(
                &primary_url,
                &remote_node,
                worker_shutdown.clone(),
            )
            .await;
            if worker_shutdown.is_cancelled() {
                break;
            }
            if let Err(error) = result {
                tracing::debug!("synthetic reverse tunnel reconnecting after: {error}");
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    });
    SyntheticTunnelFollower { shutdown, task }
}

async fn run_synthetic_tunnel_connection(
    primary_url: &str,
    remote_node: &aster_drive_model::entities::managed_follower::Model,
    shutdown: CancellationToken,
) -> Result<(), String> {
    let ws_url = format!(
        "{}{}",
        primary_url.replacen("http://", "ws://", 1),
        REMOTE_TUNNEL_CONNECT_PATH
    );
    let timestamp = Utc::now().timestamp();
    let nonce = uuid::Uuid::new_v4().to_string();
    let signature = sign_internal_request(
        &remote_node.secret_key,
        "GET",
        REMOTE_TUNNEL_CONNECT_PATH,
        timestamp,
        &nonce,
        None,
    );
    let mut request = ws_url
        .into_client_request()
        .map_err(|error| format!("build synthetic tunnel websocket request: {error}"))?;
    let headers = request.headers_mut();
    headers.insert(
        INTERNAL_AUTH_ACCESS_KEY_HEADER,
        HeaderValue::from_str(&remote_node.access_key)
            .map_err(|error| format!("set synthetic tunnel access key: {error}"))?,
    );
    headers.insert(
        INTERNAL_AUTH_TIMESTAMP_HEADER,
        HeaderValue::from_str(&timestamp.to_string())
            .map_err(|error| format!("set synthetic tunnel timestamp: {error}"))?,
    );
    headers.insert(
        INTERNAL_AUTH_NONCE_HEADER,
        HeaderValue::from_str(&nonce)
            .map_err(|error| format!("set synthetic tunnel nonce: {error}"))?,
    );
    headers.insert(
        INTERNAL_AUTH_SIGNATURE_HEADER,
        HeaderValue::from_str(&signature)
            .map_err(|error| format!("set synthetic tunnel signature: {error}"))?,
    );

    let (socket, _) = connect_async(request)
        .await
        .map_err(|error| format!("connect synthetic tunnel websocket: {error}"))?;
    let (mut writer, mut reader) = socket.split();
    loop {
        let message = tokio::select! {
            _ = shutdown.cancelled() => return Ok(()),
            message = reader.next() => message,
        };
        let Some(message) = message else {
            return Ok(());
        };
        match message.map_err(|error| format!("read synthetic tunnel websocket: {error}"))? {
            WsMessage::Binary(bytes) => {
                let start = decode_stream_frame(bytes)
                    .map_err(|error| format!("decode synthetic tunnel frame: {error}"))?;
                if start.kind != RemoteTunnelStreamFrameKind::RequestStart {
                    return Err(format!(
                        "synthetic tunnel expected request_start, got {:?}",
                        start.kind
                    ));
                }
                drain_synthetic_request_body(&start.request_id, &mut reader, &mut writer).await?;
                send_synthetic_capabilities_response(&start.request_id, &mut writer).await?;
            }
            WsMessage::Ping(bytes) => writer
                .send(WsMessage::Pong(bytes))
                .await
                .map_err(|error| format!("send synthetic tunnel pong: {error}"))?,
            WsMessage::Close(_) => return Ok(()),
            _ => {}
        }
    }
}

async fn drain_synthetic_request_body<R, W>(
    request_id: &str,
    reader: &mut R,
    writer: &mut W,
) -> Result<(), String>
where
    R: futures::Stream<Item = Result<WsMessage, tokio_tungstenite::tungstenite::Error>> + Unpin,
    W: futures::Sink<WsMessage, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    loop {
        let message = reader
            .next()
            .await
            .ok_or_else(|| "synthetic tunnel closed before request_end".to_string())?
            .map_err(|error| format!("read synthetic tunnel request body: {error}"))?;
        match message {
            WsMessage::Binary(bytes) => {
                let frame = decode_stream_frame(bytes)
                    .map_err(|error| format!("decode synthetic request body frame: {error}"))?;
                if frame.request_id != request_id {
                    return Err("synthetic tunnel received interleaved request".to_string());
                }
                match frame.kind {
                    RemoteTunnelStreamFrameKind::RequestBody => {}
                    RemoteTunnelStreamFrameKind::RequestEnd => return Ok(()),
                    RemoteTunnelStreamFrameKind::Error => return Ok(()),
                    other => {
                        return Err(format!(
                            "synthetic tunnel received unexpected request frame {other:?}"
                        ));
                    }
                }
            }
            WsMessage::Ping(bytes) => writer
                .send(WsMessage::Pong(bytes))
                .await
                .map_err(|error| format!("send synthetic tunnel body pong: {error}"))?,
            WsMessage::Close(_) => {
                return Err("synthetic tunnel closed before request_end".to_string());
            }
            _ => {}
        }
    }
}

async fn send_synthetic_capabilities_response<W>(
    request_id: &str,
    writer: &mut W,
) -> Result<(), String>
where
    W: futures::Sink<WsMessage, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let body = serde_json::to_vec(&json!({
        "code": "success",
        "msg": "",
        "data": RemoteStorageCapabilities::current(),
    }))
    .map_err(|error| format!("encode synthetic tunnel capabilities: {error}"))?;
    let frames = [
        RemoteTunnelStreamFrame {
            kind: RemoteTunnelStreamFrameKind::ResponseStart,
            request_id: request_id.to_string(),
            method: None,
            path_and_query: None,
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            content_length: Some(body.len() as u64),
            status: Some(200),
            message: None,
            body: bytes::Bytes::new(),
        },
        RemoteTunnelStreamFrame {
            kind: RemoteTunnelStreamFrameKind::ResponseBody,
            request_id: request_id.to_string(),
            method: None,
            path_and_query: None,
            headers: Vec::new(),
            content_length: None,
            status: None,
            message: None,
            body: bytes::Bytes::copy_from_slice(&body[..body.len() / 2]),
        },
        RemoteTunnelStreamFrame {
            kind: RemoteTunnelStreamFrameKind::ResponseBody,
            request_id: request_id.to_string(),
            method: None,
            path_and_query: None,
            headers: Vec::new(),
            content_length: None,
            status: None,
            message: None,
            body: bytes::Bytes::copy_from_slice(&body[body.len() / 2..]),
        },
        RemoteTunnelStreamFrame {
            kind: RemoteTunnelStreamFrameKind::ResponseEnd,
            request_id: request_id.to_string(),
            method: None,
            path_and_query: None,
            headers: Vec::new(),
            content_length: None,
            status: None,
            message: None,
            body: bytes::Bytes::new(),
        },
    ];
    for frame in frames {
        writer
            .send(WsMessage::Binary(encode_stream_frame(&frame).map_err(
                |error| format!("encode synthetic response frame: {error}"),
            )?))
            .await
            .map_err(|error| format!("send synthetic response frame: {error}"))?;
    }
    Ok(())
}

async fn wait_for_tunnel_owner(
    database: &DatabaseConnection,
    remote_node_id: i64,
    expected_endpoint: &str,
    server: &mut ServerProcess,
    timeout: Duration,
) -> aster_drive_model::entities::remote_tunnel_owner::Model {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        server.assert_running();
        if let Some(owner) =
            aster_drive_model::entities::remote_tunnel_owner::Entity::find_by_id(remote_node_id)
                .one(database)
                .await
                .expect("query reverse tunnel owner directory")
            && owner.internal_endpoint == expected_endpoint
            && owner.lease_expires_at > Utc::now()
        {
            return owner;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "reverse tunnel owner did not become {expected_endpoint}\n{}",
                server.diagnostics()
            );
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn test_remote_node_through(
    client: &reqwest::Client,
    server: &ServerProcess,
    access_token: &str,
    remote_node_id: i64,
) -> Value {
    let response = client
        .post(format!(
            "{}/api/v1/admin/remote-nodes/{remote_node_id}/test",
            server.base_url()
        ))
        .bearer_auth(access_token)
        .send()
        .await
        .expect("send reverse tunnel remote-node probe");
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .expect("decode reverse tunnel remote-node probe");
    assert!(
        status.is_success(),
        "reverse tunnel probe through {} failed with {status}: {body}\n{}",
        server.name(),
        server.diagnostics()
    );
    body
}

async fn stale_fencing_proxy_response(
    client: &reqwest::Client,
    server: &ServerProcess,
    remote_node_id: i64,
    stale_fencing_token: &str,
) -> (reqwest::StatusCode, String) {
    let mut url = reqwest::Url::parse(&format!(
        "{}{REMOTE_TUNNEL_PROXY_PATH_PREFIX}/{remote_node_id}",
        server.base_url()
    ))
    .expect("build stale fencing proxy URL");
    url.query_pairs_mut()
        .append_pair("method", "GET")
        .append_pair(
            "path_and_query",
            &format!("{INTERNAL_STORAGE_BASE_PATH}/capabilities"),
        )
        .append_pair("fencing_token", stale_fencing_token)
        .append_pair("runtime_id", "stale-e2e-runtime")
        .append_pair("headers", "W10");
    let request_target = format!("{}?{}", url.path(), url.query().unwrap_or_default());
    let timestamp = Utc::now().timestamp();
    let nonce = uuid::Uuid::new_v4().to_string();
    let signature = sign_internal_request(
        INTERNAL_PROXY_SECRET,
        "POST",
        &request_target,
        timestamp,
        &nonce,
        Some(0),
    );
    let response = client
        .post(url)
        .header(INTERNAL_AUTH_ACCESS_KEY_HEADER, "stale-e2e-runtime")
        .header(INTERNAL_AUTH_TIMESTAMP_HEADER, timestamp.to_string())
        .header(INTERNAL_AUTH_NONCE_HEADER, nonce)
        .header(INTERNAL_AUTH_SIGNATURE_HEADER, signature)
        .header(reqwest::header::CONTENT_LENGTH, "0")
        .body(Vec::new())
        .send()
        .await
        .expect("send stale fencing proxy request");
    let status = response.status();
    let body = response
        .text()
        .await
        .expect("read stale fencing proxy response");
    (status, body)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker and two real AsterDrive primary processes"]
async fn cluster_upload_init_on_second_primary_rejects_pod_local_stream_staging() {
    let _guard = e2e_lock().lock().await;
    let services = SharedServices::start().await;
    let database = services.connect_database().await;
    let policy_id = configure_default_sftp_policy(&database).await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build cluster upload E2E HTTP client");

    let mut primary_a = ServerProcess::spawn("primary-a", &services);
    wait_for_health(&client, &mut primary_a).await;
    let access_token = setup_and_login(&client, &primary_a).await;
    let mut primary_b = ServerProcess::spawn("primary-b", &services);
    wait_for_health(&client, &mut primary_b).await;

    let response = client
        .post(format!("{}/api/v1/files/upload/init", primary_b.base_url()))
        .bearer_auth(&access_token)
        .json(&json!({
            "filename": "cluster-stream-staging.bin",
            "total_size": 10 * 1024 * 1024,
        }))
        .send()
        .await
        .expect("send upload init to second primary");
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .expect("decode cluster staging rejection response");
    assert_eq!(status, reqwest::StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "bad_request");
    assert!(
        body["msg"]
            .as_str()
            .unwrap_or_default()
            .contains("stream_staging")
    );
    assert!(
        body["msg"]
            .as_str()
            .unwrap_or_default()
            .contains("Pod-local staging")
    );
    assert_eq!(
        aster_drive::db::repository::upload_session_repo::count_by_policy(&database, policy_id)
            .await
            .expect("count cluster staging sessions"),
        0,
        "rejected cluster staging init must not persist a session"
    );

    primary_a.terminate();
    primary_b.terminate();
    database
        .close()
        .await
        .expect("close cluster upload E2E database connection");
    services.cleanup_database().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker and two real AsterDrive primary processes"]
async fn fresh_postgres_concurrent_primaries_share_startup_and_setup_state_machine() {
    let _guard = e2e_lock().lock().await;
    let services = SharedServices::start_empty().await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build concurrent startup E2E HTTP client");

    let mut primary_a = ServerProcess::spawn_with_database_pool_size("primary-a", &services, 1);
    let mut primary_b = ServerProcess::spawn_with_database_pool_size("primary-b", &services, 1);
    let ((), ()) = tokio::join!(
        wait_for_health(&client, &mut primary_a),
        wait_for_health(&client, &mut primary_b),
    );
    let (ready_a, ready_b) = tokio::join!(
        wait_for_ready_status(
            &client,
            &mut primary_a,
            reqwest::StatusCode::OK,
            Duration::from_secs(30),
        ),
        wait_for_ready_status(
            &client,
            &mut primary_b,
            reqwest::StatusCode::OK,
            Duration::from_secs(30),
        ),
    );
    assert_eq!(ready_a["data"]["status"], "needs_admin");
    assert_eq!(ready_b["data"]["status"], "needs_admin");

    let access_token = setup_and_login(&client, &primary_a).await;
    let (needs_storage_a, needs_storage_b) = tokio::join!(
        wait_for_ready_status(
            &client,
            &mut primary_a,
            reqwest::StatusCode::OK,
            Duration::from_secs(30),
        ),
        wait_for_ready_status(
            &client,
            &mut primary_b,
            reqwest::StatusCode::OK,
            Duration::from_secs(30),
        ),
    );
    assert_eq!(needs_storage_a["data"]["status"], "needs_storage");
    assert_eq!(needs_storage_b["data"]["status"], "needs_storage");

    let setup_catalog: Value = client
        .get(format!(
            "{}/api/v1/admin/policies/storage-drivers?context=setup",
            primary_a.base_url()
        ))
        .bearer_auth(&access_token)
        .send()
        .await
        .expect("list cluster setup storage connector catalog")
        .error_for_status()
        .expect("cluster setup storage connector catalog should succeed")
        .json()
        .await
        .expect("decode cluster setup storage connector catalog");
    let setup_connectors = setup_catalog["data"]
        .as_array()
        .expect("cluster setup connector list");
    assert!(
        setup_connectors
            .iter()
            .any(|connector| connector["driver_type"] == "s3")
    );
    assert!(
        !setup_connectors
            .iter()
            .any(|connector| connector["driver_type"] == "local"),
        "cluster setup catalog must not advertise instance-local storage"
    );
    let setup_onedrive = setup_connectors
        .iter()
        .find(|connector| connector["driver_type"] == "one_drive")
        .expect("initial setup catalog should describe OneDrive as unavailable");
    assert_eq!(setup_onedrive["supports_initial_setup"], false);

    let manage_catalog: Value = client
        .get(format!(
            "{}/api/v1/admin/policies/storage-drivers?context=manage",
            primary_b.base_url()
        ))
        .bearer_auth(&access_token)
        .send()
        .await
        .expect("list cluster management storage connector catalog")
        .error_for_status()
        .expect("cluster management storage connector catalog should succeed")
        .json()
        .await
        .expect("decode cluster management storage connector catalog");
    assert!(
        manage_catalog["data"]
            .as_array()
            .expect("cluster management connector list")
            .iter()
            .any(|connector| connector["driver_type"] == "local"),
        "management catalog must retain Local metadata for existing-policy inspection"
    );

    let rejected_local_response = client
        .post(format!("{}/api/v1/admin/policies", primary_b.base_url()))
        .bearer_auth(&access_token)
        .json(&json!({
            "name": "Rejected Pod Local",
            "driver_type": "local",
            "base_path": "./data/uploads",
            "max_file_size": 0,
            "chunk_size": 5_242_880,
            "is_default": true
        }))
        .send()
        .await
        .expect("reject first instance-local policy through primary B");
    assert_eq!(
        rejected_local_response.status(),
        reqwest::StatusCode::BAD_REQUEST
    );
    let rejected_local_body: Value = rejected_local_response
        .json()
        .await
        .expect("decode rejected instance-local policy response");
    assert!(
        rejected_local_body["msg"]
            .as_str()
            .unwrap_or_default()
            .contains("instance_local")
    );

    let create_initial_policy = |primary: &ServerProcess, name: &str| {
        client
            .post(format!("{}/api/v1/admin/policies", primary.base_url()))
            .bearer_auth(&access_token)
            .json(&json!({
                "name": name,
                "driver_type": "s3",
                "endpoint": "http://127.0.0.1:9000",
                "bucket": "asterdrive-fresh-e2e",
                "access_key": "e2e-access",
                "secret_key": "e2e-secret",
                "base_path": "",
                "max_file_size": 0,
                "chunk_size": 5_242_880,
                "is_default": true,
                "options": {
                    "object_storage_upload_strategy": "presigned",
                    "s3_path_style": true
                }
            }))
            .send()
    };
    let (create_policy_a, create_policy_b) = tokio::join!(
        create_initial_policy(&primary_a, "Fresh Shared Default A"),
        create_initial_policy(&primary_b, "Fresh Shared Default B"),
    );
    let initial_policy_responses = [
        create_policy_a.expect("create initial shared policy through primary A"),
        create_policy_b.expect("create initial shared policy through primary B"),
    ];
    let mut created_count = 0;
    let mut rejected_response = None;
    for response in initial_policy_responses {
        if response.status() == reqwest::StatusCode::CREATED {
            created_count += 1;
        } else {
            assert!(
                rejected_response.is_none(),
                "only one concurrent initial policy request may be rejected"
            );
            rejected_response = Some(response);
        }
    }
    assert_eq!(
        created_count, 1,
        "exactly one concurrent initial policy request must commit"
    );
    let rejected_response =
        rejected_response.expect("one concurrent initial policy request must be rejected");
    assert_eq!(
        rejected_response.status(),
        reqwest::StatusCode::BAD_REQUEST,
        "the losing initial policy request must return a stable validation error"
    );
    let rejected_body: Value = rejected_response
        .json()
        .await
        .expect("decode rejected concurrent initial policy response");
    assert_eq!(
        rejected_body["code"], "validation.system_already_initialized",
        "the loser must observe the completed initial setup transition"
    );

    let (ready_a, ready_b) = tokio::join!(
        wait_for_ready_status(
            &client,
            &mut primary_a,
            reqwest::StatusCode::OK,
            Duration::from_secs(30),
        ),
        wait_for_ready_status(
            &client,
            &mut primary_b,
            reqwest::StatusCode::OK,
            Duration::from_secs(30),
        ),
    );
    assert_eq!(ready_a["data"]["status"], "ready");
    assert_eq!(ready_b["data"]["status"], "ready");

    let database = services.connect_database().await;
    let history = aster_drive_migration::inspect_migration_history(&database)
        .await
        .expect("inspect migration history after concurrent startup");
    assert_eq!(
        history.track,
        aster_drive_migration::MigrationTrack::Current
    );
    assert!(history.pending_current.is_empty());
    assert!(history.unknown_applied.is_empty());
    assert_eq!(
        history.applied,
        aster_drive_migration::current_migration_names()
    );
    assert_eq!(
        aster_drive::db::repository::policy_repo::find_all(&database)
            .await
            .expect("list policies after setup")
            .len(),
        1,
        "concurrent Primary startup and setup must produce one storage policy"
    );
    let default_group =
        aster_drive::db::repository::policy_group_repo::find_default_group(&database)
            .await
            .expect("load default policy group after setup")
            .expect("default policy group should exist after setup");
    let admin = aster_drive::db::repository::user_repo::find_by_username(&database, "admin")
        .await
        .expect("load setup administrator")
        .expect("setup administrator should exist");
    assert_eq!(admin.policy_group_id, Some(default_group.id));

    primary_a.terminate();
    primary_b.terminate();
    database
        .close()
        .await
        .expect("close concurrent startup E2E database connection");
    services.cleanup_database().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker and two real AsterDrive primary processes"]
async fn concurrent_primary_startup_reconciles_one_default_policy_group() {
    let _guard = e2e_lock().lock().await;
    let services = SharedServices::start().await;
    let database = services.connect_database().await;
    aster_drive_model::entities::storage_policy_group_item::Entity::delete_many()
        .exec(&database)
        .await
        .expect("remove seeded policy group items");
    aster_drive_model::entities::storage_policy_group::Entity::delete_many()
        .exec(&database)
        .await
        .expect("remove seeded policy groups");
    database
        .close()
        .await
        .expect("close policy group setup database connection");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build policy group reconciliation E2E HTTP client");
    let mut primary_a = ServerProcess::spawn_with_database_pool_size("primary-a", &services, 1);
    let mut primary_b = ServerProcess::spawn_with_database_pool_size("primary-b", &services, 1);
    let ((), ()) = tokio::join!(
        wait_for_health(&client, &mut primary_a),
        wait_for_health(&client, &mut primary_b),
    );

    let database = services.connect_database().await;
    let groups = aster_drive_model::entities::storage_policy_group::Entity::find()
        .all(&database)
        .await
        .expect("list reconciled policy groups");
    assert_eq!(groups.len(), 1);
    assert!(groups[0].is_default);
    let items = aster_drive_model::entities::storage_policy_group_item::Entity::find()
        .all(&database)
        .await
        .expect("list reconciled policy group items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].group_id, groups[0].id);

    primary_a.terminate();
    primary_b.terminate();
    database
        .close()
        .await
        .expect("close reconciled policy group database connection");
    services.cleanup_database().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker and a real AsterDrive primary process"]
async fn redis_outage_only_marks_cluster_readiness_unavailable_and_recovers() {
    let _guard = e2e_lock().lock().await;
    let services = SharedServices::start_for_redis_readiness_outage().await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build Redis readiness E2E HTTP client");
    let mut primary = ServerProcess::spawn("primary-redis-readiness", &services);
    wait_for_health(&client, &mut primary).await;
    let _access_token = setup_and_login(&client, &primary).await;

    let initial_ready = wait_for_ready_status(
        &client,
        &mut primary,
        reqwest::StatusCode::OK,
        Duration::from_secs(30),
    )
    .await;
    assert_eq!(initial_ready["data"]["status"], "ready");

    services.redis.stop().await;
    let unavailable = wait_for_ready_code(
        &client,
        &mut primary,
        "config.error",
        Duration::from_secs(30),
    )
    .await;
    assert_eq!(unavailable["msg"], "Cache unavailable");
    assert!(
        client
            .get(format!("{}/health", primary.base_url()))
            .send()
            .await
            .expect("request liveness during Redis outage")
            .status()
            .is_success(),
        "Redis outage must not make the liveness endpoint fail"
    );

    services.redis.restart().await;
    let recovered = wait_for_ready_status(
        &client,
        &mut primary,
        reqwest::StatusCode::OK,
        Duration::from_secs(60),
    )
    .await;
    assert_eq!(recovered["data"]["status"], "ready");
    assert!(
        client
            .get(format!("{}/health", primary.base_url()))
            .send()
            .await
            .expect("request liveness after Redis recovery")
            .status()
            .is_success(),
        "liveness endpoint must remain healthy after Redis recovery"
    );

    primary.terminate();
    services.cleanup_database().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker and two real AsterDrive primary processes"]
async fn storage_policy_update_propagates_to_second_primary_without_restart() {
    let _guard = e2e_lock().lock().await;
    let services = SharedServices::start().await;
    let database = services.connect_database().await;
    let policy_id = aster_drive::db::repository::policy_repo::find_default(&database)
        .await
        .expect("load default E2E storage policy")
        .expect("default E2E storage policy should exist")
        .id;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build storage topology E2E HTTP client");

    let mut primary_a = ServerProcess::spawn("primary-a", &services);
    wait_for_health(&client, &mut primary_a).await;
    let access_token = setup_and_login(&client, &primary_a).await;
    let mut primary_b = ServerProcess::spawn("primary-b", &services);
    wait_for_health(&client, &mut primary_b).await;

    let response = client
        .patch(format!(
            "{}/api/v1/admin/policies/{policy_id}",
            primary_a.base_url()
        ))
        .bearer_auth(&access_token)
        .json(&json!({ "max_file_size": 1 }))
        .send()
        .await
        .expect("update storage policy through primary A");
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .expect("decode storage policy update response");
    assert!(
        status.is_success(),
        "policy update failed with {status}: {body}"
    );

    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        primary_b.assert_running();
        let response = client
            .post(format!("{}/api/v1/files/upload/init", primary_b.base_url()))
            .bearer_auth(&access_token)
            .json(&json!({
                "filename": "cross-primary-policy-limit.bin",
                "total_size": 2,
            }))
            .send()
            .await
            .expect("send upload init through primary B");
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .expect("decode upload init response from primary B");
        if status == reqwest::StatusCode::BAD_REQUEST && body["code"] == "file.too_large" {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "primary B did not reload the updated policy, last response {status}: {body}\n{}",
                primary_b.diagnostics()
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    primary_a.terminate();
    primary_b.terminate();
    database
        .close()
        .await
        .expect("close storage topology E2E database connection");
    services.cleanup_database().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker and two real AsterDrive primary processes"]
async fn user_policy_group_assignment_propagates_to_second_primary_without_restart() {
    let _guard = e2e_lock().lock().await;
    let services = SharedServices::start().await;
    let database = services.connect_database().await;
    let now = Utc::now();
    let constrained_policy =
        create_multi_primary_s3_policy(&database, "E2E User Policy Group Limit", 1, false).await;
    let constrained_group = aster_drive::db::repository::policy_group_repo::create_group(
        &database,
        aster_drive_model::entities::storage_policy_group::ActiveModel {
            name: Set("E2E User Policy Group".to_string()),
            description: Set("Targeted config-sync E2E fixture".to_string()),
            is_enabled: Set(true),
            is_default: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .expect("create constrained E2E policy group");
    aster_drive::db::repository::policy_group_repo::create_group_item(
        &database,
        aster_drive_model::entities::storage_policy_group_item::ActiveModel {
            group_id: Set(constrained_group.id),
            policy_id: Set(constrained_policy.id),
            priority: Set(1),
            min_file_size: Set(0),
            max_file_size: Set(0),
            created_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .expect("add constrained E2E policy group item");
    database
        .close()
        .await
        .expect("close user policy group E2E fixture database connection");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build user policy group E2E HTTP client");
    let mut primary_a = ServerProcess::spawn("primary-a", &services);
    wait_for_health(&client, &mut primary_a).await;
    let admin_access_token = setup_and_login(&client, &primary_a).await;
    let mut primary_b = ServerProcess::spawn("primary-b", &services);
    wait_for_health(&client, &mut primary_b).await;

    let response = client
        .post(format!("{}/api/v1/admin/users", primary_a.base_url()))
        .bearer_auth(&admin_access_token)
        .json(&json!({
            "username": "policy-user",
            "email": "policy-user@example.com",
            "password": POLICY_GROUP_USER_PASSWORD,
            "must_change_password": false,
        }))
        .send()
        .await
        .expect("create user on primary A");
    let status = response.status();
    let body: Value = response.json().await.expect("decode created user response");
    assert!(
        status.is_success(),
        "user creation failed with {status}: {body}"
    );
    let user_id = body["data"]["user"]["id"]
        .as_i64()
        .expect("created user response should contain an id");

    let response = client
        .patch(format!(
            "{}/api/v1/admin/users/{user_id}",
            primary_a.base_url()
        ))
        .bearer_auth(&admin_access_token)
        .json(&json!({ "policy_group_id": constrained_group.id }))
        .send()
        .await
        .expect("assign constrained policy group on primary A");
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .expect("decode updated user policy group response");
    assert!(
        status.is_success(),
        "policy group assignment failed with {status}: {body}"
    );
    assert_eq!(body["data"]["policy_group_id"], constrained_group.id);

    let user_access_token = login(
        &client,
        &primary_b,
        "policy-user",
        POLICY_GROUP_USER_PASSWORD,
    )
    .await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        primary_b.assert_running();
        let response = client
            .post(format!("{}/api/v1/files/upload/init", primary_b.base_url()))
            .bearer_auth(&user_access_token)
            .json(&json!({
                "filename": "targeted-user-policy-group-limit.bin",
                "total_size": 2,
            }))
            .send()
            .await
            .expect("send user upload init through primary B");
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .expect("decode user upload init response from primary B");
        if status == reqwest::StatusCode::BAD_REQUEST && body["code"] == "file.too_large" {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "primary B did not apply the targeted user policy group update, last response \
                 {status}: {body}\n{}",
                primary_b.diagnostics()
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    primary_a.terminate();
    primary_b.terminate();
    services.cleanup_database().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker and two real AsterDrive primary processes"]
async fn config_sync_propagates_and_reconciles_after_redis_outage() {
    let _guard = e2e_lock().lock().await;
    let services = SharedServices::start().await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build E2E HTTP client");
    let mut primary_a = ServerProcess::spawn("primary-a", &services);
    wait_for_health(&client, &mut primary_a).await;
    let access_token = setup_and_login(&client, &primary_a).await;

    let mut primary_b = ServerProcess::spawn("primary-b", &services);
    wait_for_health(&client, &mut primary_b).await;
    assert!(registration_enabled(&client, &primary_b).await);
    tokio::time::sleep(Duration::from_secs(1)).await;

    set_registration_enabled(&client, &primary_a, &access_token, false).await;
    wait_for_registration_enabled(&client, &mut primary_b, false, Duration::from_secs(15)).await;

    services.redis.stop().await;
    let database = services.connect_database().await;
    aster_drive::db::repository::config_repo::upsert(
        &database,
        "auth_allow_user_registration",
        "true",
        1,
    )
    .await
    .expect("update authoritative config while notification transport is offline");
    tokio::time::sleep(Duration::from_millis(750)).await;
    assert!(
        !registration_enabled(&client, &primary_b).await,
        "primary B must keep its old snapshot until reconnect reconciliation"
    );

    services.redis.restart().await;
    wait_for_registration_enabled(&client, &mut primary_a, true, Duration::from_secs(60)).await;
    wait_for_registration_enabled(&client, &mut primary_b, true, Duration::from_secs(60)).await;

    primary_a.terminate();
    primary_b.terminate();
    database
        .close()
        .await
        .expect("close config E2E database connection");
    services.cleanup_database().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker and two real AsterDrive primary processes"]
async fn scheduler_has_one_owner_and_standby_takes_over_after_owner_crash() {
    let _guard = e2e_lock().lock().await;
    let services = SharedServices::start().await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build E2E HTTP client");
    let database = services.connect_database().await;

    let mut primary_a = ServerProcess::spawn("primary-a", &services);
    wait_for_health(&client, &mut primary_a).await;
    let initial_lease = wait_for_runtime_lease(&database, &mut primary_a).await;

    let mut primary_b = ServerProcess::spawn("primary-b", &services);
    wait_for_health(&client, &mut primary_b).await;
    tokio::time::sleep(Duration::from_secs(12)).await;
    let renewed_lease = assert_single_live_runtime_lease(&database).await;
    assert_eq!(
        renewed_lease.owner_id, initial_lease.owner_id,
        "standby primary must not steal a live lease"
    );
    assert!(
        renewed_lease.last_renewed_at > initial_lease.last_renewed_at,
        "active owner should renew while both primaries are healthy"
    );

    primary_a.terminate();
    let lease_after_crash = load_single_runtime_lease(&database).await;
    assert_eq!(lease_after_crash.owner_id, initial_lease.owner_id);

    let deadline = tokio::time::Instant::now() + Duration::from_secs(50);
    loop {
        primary_b.assert_running();
        let lease = load_single_runtime_lease(&database).await;
        if lease.owner_id != initial_lease.owner_id {
            assert!(
                lease.expires_at > Utc::now(),
                "new runtime lease owner must publish a live expiration"
            );
            assert!(
                lease.last_renewed_at >= lease_after_crash.expires_at,
                "standby must acquire only after the crashed owner's lease expires"
            );
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "primary B did not take over the expired scheduler lease\n{}",
                primary_b.diagnostics()
            );
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    primary_b.terminate();
    database
        .close()
        .await
        .expect("close scheduler E2E database connection");
    services.cleanup_database().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker, PostgreSQL fault injection, and two real AsterDrive primary processes"]
async fn partitioned_runtime_owner_is_fenced_after_database_access_recovers() {
    let _guard = e2e_lock().lock().await;
    let services = SharedServices::start().await;
    let fault_role = services.create_database_fault_role().await;
    let database = services.connect_database().await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build database partition E2E HTTP client");

    let mut primary_a =
        ServerProcess::spawn_with_database_url("primary-a", &services, 5, &fault_role.url);
    wait_for_health(&client, &mut primary_a).await;
    let initial_lease = wait_for_runtime_lease(&database, &mut primary_a).await;
    let mut primary_b = ServerProcess::spawn("primary-b", &services);
    wait_for_health(&client, &mut primary_b).await;

    tokio::time::sleep(Duration::from_secs(12)).await;
    let lease_before_partition = assert_single_live_runtime_lease(&database).await;
    assert_eq!(lease_before_partition.owner_id, initial_lease.owner_id);
    assert!(lease_before_partition.last_renewed_at > initial_lease.last_renewed_at);

    services
        .set_database_fault_role_login(&fault_role, false)
        .await;
    let unavailable = wait_for_ready_code(
        &client,
        &mut primary_a,
        "database.error",
        Duration::from_secs(20),
    )
    .await;
    assert_eq!(unavailable["msg"], "Database unavailable");
    let takeover = wait_for_new_runtime_owner(
        &database,
        &mut primary_b,
        &initial_lease.owner_id,
        Duration::from_secs(50),
    )
    .await;
    assert!(
        takeover.last_renewed_at >= lease_before_partition.expires_at,
        "standby must acquire only after the partitioned owner's lease expires"
    );

    services
        .set_database_fault_role_login(&fault_role, true)
        .await;
    let recovered = wait_for_ready_status(
        &client,
        &mut primary_a,
        reqwest::StatusCode::OK,
        Duration::from_secs(30),
    )
    .await;
    assert_eq!(recovered["data"]["status"], "needs_admin");

    tokio::time::sleep(Duration::from_secs(12)).await;
    let lease_after_recovery = assert_single_live_runtime_lease(&database).await;
    assert_eq!(
        lease_after_recovery.owner_id, takeover.owner_id,
        "recovered stale owner must remain standby"
    );
    assert!(
        lease_after_recovery.last_renewed_at > takeover.last_renewed_at,
        "new owner must keep renewing after the stale owner reconnects"
    );

    let outbox = create_due_invitation_mail(&database).await;
    wait_for_mail_outbox_sent(
        &database,
        outbox.id,
        &services,
        &mut primary_a,
        &mut primary_b,
    )
    .await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        services.smtp.message_count().await,
        1,
        "partition recovery must not create duplicate mail side effects"
    );
    let records = scheduled_runtime_records(&database, "mail-outbox-dispatch").await;
    assert_eq!(
        records.len(),
        1,
        "partition recovery must keep one scheduled firing"
    );

    primary_a.terminate();
    primary_b.terminate();
    database
        .close()
        .await
        .expect("close database partition E2E connection");
    services.cleanup_database().await;
    services.drop_database_fault_role(&fault_role).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker and two real AsterDrive primary processes"]
async fn scheduled_mail_dispatch_has_one_firing_and_one_delivery_across_primaries() {
    let _guard = e2e_lock().lock().await;
    let services = SharedServices::start().await;
    let database = services.connect_database().await;
    let outbox = create_due_invitation_mail(&database).await;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build E2E HTTP client");
    let mut primary_a = ServerProcess::spawn("primary-a", &services);
    wait_for_health(&client, &mut primary_a).await;
    let mut primary_b = ServerProcess::spawn("primary-b", &services);
    wait_for_health(&client, &mut primary_b).await;

    let sent = wait_for_mail_outbox_sent(
        &database,
        outbox.id,
        &services,
        &mut primary_a,
        &mut primary_b,
    )
    .await;
    assert_eq!(
        sent.payload_json.as_ref(),
        aster_forge_mail::StoredMailPayload::CLEARED_JSON
    );
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(services.smtp.message_count().await, 1);

    let records = scheduled_runtime_records(&database, "mail-outbox-dispatch").await;
    assert_eq!(
        records.len(),
        1,
        "one scheduled firing must create one history row"
    );
    assert!(records[0].dedupe_key.is_some());

    primary_a.terminate();
    primary_b.terminate();
    database
        .close()
        .await
        .expect("close mail E2E database connection");
    services.cleanup_database().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker and two real AsterDrive primary processes"]
async fn background_task_claim_is_fenced_across_primaries() {
    let _guard = e2e_lock().lock().await;
    let services = SharedServices::start().await;
    let database = services.connect_database().await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build E2E HTTP client");

    let mut primary_a = ServerProcess::spawn("primary-a", &services);
    wait_for_health(&client, &mut primary_a).await;
    let access_token = setup_and_login(&client, &primary_a).await;
    let mut primary_b = ServerProcess::spawn("primary-b", &services);
    wait_for_health(&client, &mut primary_b).await;

    let task_id = create_trash_purge_task(&client, &primary_a, &access_token).await;
    let task = wait_for_background_task(
        &database,
        task_id,
        &mut primary_a,
        &mut primary_b,
        Duration::from_secs(30),
    )
    .await;
    assert_eq!(
        task.status,
        aster_drive_model::types::BackgroundTaskStatus::Succeeded
    );
    assert_eq!(task.processing_token, 1);
    assert_eq!(task.attempt_count, 0);
    assert!(task.processing_started_at.is_none());
    assert!(task.lease_expires_at.is_none());
    assert!(
        !aster_drive::db::repository::background_task_repo::mark_succeeded(
            &database,
            aster_drive::db::repository::background_task_repo::TaskSuccessUpdate {
                id: task.id,
                processing_token: 0,
                result_json: None,
                steps_json: None,
                current: 1,
                total: 1,
                status_text: Some("stale worker completion"),
                finished_at: Utc::now(),
                expires_at: Utc::now() + chrono::Duration::hours(1),
            },
        )
        .await
        .expect("stale processing token write should execute without database error"),
        "a stale worker token must not overwrite the completed task"
    );

    primary_a.terminate();
    primary_b.terminate();
    database
        .close()
        .await
        .expect("close background task E2E database connection");
    services.cleanup_database().await;
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker and two real AsterDrive primary processes"]
async fn graceful_primary_shutdown_releases_lease_for_standby() {
    let _guard = e2e_lock().lock().await;
    let services = SharedServices::start().await;
    let database = services.connect_database().await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build E2E HTTP client");

    let mut primary_a = ServerProcess::spawn("primary-a", &services);
    wait_for_health(&client, &mut primary_a).await;
    let initial_lease = wait_for_runtime_lease(&database, &mut primary_a).await;
    let mut primary_b = ServerProcess::spawn("primary-b", &services);
    wait_for_health(&client, &mut primary_b).await;

    primary_a.terminate_gracefully();
    let takeover = wait_for_new_runtime_owner(
        &database,
        &mut primary_b,
        &initial_lease.owner_id,
        Duration::from_secs(20),
    )
    .await;
    assert_ne!(takeover.owner_id, initial_lease.owner_id);

    primary_b.terminate();
    database
        .close()
        .await
        .expect("close graceful shutdown E2E database connection");
    services.cleanup_database().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker, two real AsterDrive primaries, and a synthetic tunnel follower"]
async fn reverse_tunnel_request_hitting_non_owner_primary_streams_through_owner() {
    let _guard = e2e_lock().lock().await;
    let services = SharedServices::start().await;
    let database = services.connect_database().await;
    defer_remote_node_health_tests(&database).await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("build reverse tunnel E2E HTTP client");

    let mut primary_a = ServerProcess::spawn("primary-a", &services);
    wait_for_health(&client, &mut primary_a).await;
    let access_token = setup_and_login(&client, &primary_a).await;
    let mut primary_b = ServerProcess::spawn("primary-b", &services);
    wait_for_health(&client, &mut primary_b).await;
    let remote_node = seed_reverse_tunnel_node(&database).await;

    let follower = spawn_synthetic_tunnel_follower(primary_a.base_url(), remote_node.clone());
    let owner = wait_for_tunnel_owner(
        &database,
        remote_node.id,
        &primary_a.base_url(),
        &mut primary_a,
        Duration::from_secs(15),
    )
    .await;
    assert_eq!(owner.remote_node_id, remote_node.id);

    let probe = test_remote_node_through(&client, &primary_b, &access_token, remote_node.id).await;
    assert_eq!(
        probe["data"]["capabilities"]["protocol_version"],
        RemoteStorageCapabilities::current().protocol_version
    );
    assert_eq!(probe["data"]["tunnel"]["status"], "online");

    follower.stop().await;
    let follower_b = spawn_synthetic_tunnel_follower(primary_b.base_url(), remote_node.clone());
    let owner_b = wait_for_tunnel_owner(
        &database,
        remote_node.id,
        &primary_b.base_url(),
        &mut primary_b,
        Duration::from_secs(10),
    )
    .await;
    assert_ne!(
        owner_b.fencing_token, owner.fencing_token,
        "a clean tunnel disconnect should release ownership for immediate takeover"
    );
    let direct_probe =
        test_remote_node_through(&client, &primary_b, &access_token, remote_node.id).await;
    assert_eq!(
        direct_probe["data"]["capabilities"]["protocol_version"],
        RemoteStorageCapabilities::current().protocol_version
    );

    follower_b.stop().await;
    primary_a.terminate();
    primary_b.terminate();
    database
        .close()
        .await
        .expect("close reverse tunnel routing E2E database connection");
    services.cleanup_database().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker, two real AsterDrive primaries, and a synthetic tunnel follower"]
async fn reverse_tunnel_owner_failover_fences_stale_primary() {
    let _guard = e2e_lock().lock().await;
    let services = SharedServices::start().await;
    let database = services.connect_database().await;
    defer_remote_node_health_tests(&database).await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("build reverse tunnel failover E2E HTTP client");

    let mut primary_a = ServerProcess::spawn("primary-a", &services);
    wait_for_health(&client, &mut primary_a).await;
    let access_token = setup_and_login(&client, &primary_a).await;
    let mut primary_b = ServerProcess::spawn("primary-b", &services);
    wait_for_health(&client, &mut primary_b).await;
    let remote_node = seed_reverse_tunnel_node(&database).await;

    let follower_a = spawn_synthetic_tunnel_follower(primary_a.base_url(), remote_node.clone());
    let owner_a = wait_for_tunnel_owner(
        &database,
        remote_node.id,
        &primary_a.base_url(),
        &mut primary_a,
        Duration::from_secs(15),
    )
    .await;
    primary_a.terminate();
    follower_a.stop().await;
    let follower_b = spawn_synthetic_tunnel_follower(primary_b.base_url(), remote_node.clone());
    let owner_b = wait_for_tunnel_owner(
        &database,
        remote_node.id,
        &primary_b.base_url(),
        &mut primary_b,
        Duration::from_secs(65),
    )
    .await;
    assert_ne!(owner_b.fencing_token, owner_a.fencing_token);

    let probe = test_remote_node_through(&client, &primary_b, &access_token, remote_node.id).await;
    assert_eq!(probe["data"]["tunnel"]["status"], "online");
    let (stale_status, stale_body) =
        stale_fencing_proxy_response(&client, &primary_b, remote_node.id, &owner_a.fencing_token)
            .await;
    assert!(
        !stale_status.is_success() && stale_body.contains("fencing token is stale"),
        "stale owner token should be rejected, got {stale_status}: {stale_body}"
    );

    follower_b.stop().await;
    primary_b.terminate();
    database
        .close()
        .await
        .expect("close reverse tunnel failover E2E database connection");
    services.cleanup_database().await;
}
