//! 集成测试公共 helper。
#![expect(
    dead_code,
    reason = "shared integration-test support exposes helpers used by different test binaries"
)]

use aster_drive::runtime::PrimaryAppState;
use aster_forge_test::{
    fixture::{SuiteFixtureLock, SuiteFixtureState},
    mysql::MysqlTestContainer,
    postgres::PostgresTestContainer,
    suite::TestContainerSuite,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

thread_local! {
    static CSRF_LOOKUP_CACHE: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

fn csrf_registry() -> &'static Mutex<HashMap<String, String>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_csrf_registry() -> std::sync::MutexGuard<'static, HashMap<String, String>> {
    csrf_registry()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

const TEST_DATABASE_BACKEND_ENV: &str = "ASTER_TEST_DATABASE_BACKEND";
// Keep the year within MySQL TIMESTAMP's supported range.
pub const TEST_FUTURE_SHARE_EXPIRY_RFC3339: &str = "2099-12-31T23:59:59Z";
pub const DELTAV_VERSION_HREF_PREFIX: &str = "/webdav/.asterdrive-deltav/versions/";

pub fn deltav_version_entries(xml: &str) -> Vec<(String, String)> {
    use aster_forge_webdav::DavXmlElement as Element;

    let multistatus = Element::parse_reader(std::io::Cursor::new(xml.as_bytes()))
        .expect("DeltaV Multi-Status XML should parse");
    multistatus
        .child_elements()
        .filter(|element| element.name == "response")
        .filter_map(|response| {
            let href = response
                .child_elements()
                .find(|element| element.name == "href")
                .and_then(Element::text)?;
            if !href.starts_with(DELTAV_VERSION_HREF_PREFIX) {
                return None;
            }
            let version_name = response
                .child_elements()
                .filter(|element| element.name == "propstat")
                .filter_map(|propstat| {
                    propstat
                        .child_elements()
                        .find(|element| element.name == "prop")
                })
                .flat_map(Element::child_elements)
                .filter(|property| property.name == "version-name")
                .filter_map(Element::text)
                .find(|value| !value.is_empty())?;
            Some((href, version_name))
        })
        .collect()
}

pub fn deltav_version_hrefs(xml: &str) -> Vec<String> {
    let mut hrefs = Vec::new();
    for (href, _) in deltav_version_entries(xml) {
        if !hrefs.contains(&href) {
            hrefs.push(href);
        }
    }
    hrefs
}

pub fn deltav_version_href_by_name(xml: &str, version_name: &str) -> Option<String> {
    deltav_version_entries(xml)
        .into_iter()
        .find_map(|(href, name)| (name == version_name).then_some(href))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestLocalConnectorConfigV1 {
    pub base_path: String,
    pub content_dedup: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestRemoteConnectorConfigV1 {
    pub base_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_node_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_storage_target_key: Option<String>,
    pub remote_download_strategy: aster_drive_model::types::RemoteDownloadStrategy,
    pub remote_upload_strategy: aster_drive_model::types::RemoteUploadStrategy,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestS3ConnectorConfigV1 {
    pub endpoint: String,
    pub bucket: String,
    pub base_path: String,
    pub object_storage_upload_strategy: aster_drive_model::types::ObjectStorageUploadStrategy,
    pub object_storage_download_strategy: aster_drive_model::types::ObjectStorageDownloadStrategy,
    pub s3_path_style: bool,
    pub s3_region: String,
    pub s3_connect_timeout_secs: u64,
    pub s3_read_timeout_secs: u64,
    pub s3_operation_timeout_secs: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestTencentCosConnectorConfigV1 {
    pub endpoint: String,
    pub bucket: String,
    pub base_path: String,
    pub object_storage_upload_strategy: aster_drive_model::types::ObjectStorageUploadStrategy,
    pub object_storage_download_strategy: aster_drive_model::types::ObjectStorageDownloadStrategy,
}

pub fn connector_envelope<T: Serialize>(
    connector_id: &'static str,
    values: T,
) -> aster_drive_storage::ConnectorConfigEnvelope {
    let values = serde_json::to_value(values)
        .and_then(serde_json::from_value)
        .expect("typed integration connector config should serialize as a field map");
    aster_drive_storage::ConnectorConfigEnvelope::new(
        aster_drive_storage::ConnectorId::declared(connector_id),
        1,
        values,
    )
}

pub fn connector_envelope_with_schema<T: Serialize>(
    connector_id: &'static str,
    schema_version: u32,
    values: T,
) -> aster_drive_storage::ConnectorConfigEnvelope {
    let values = serde_json::to_value(values)
        .and_then(serde_json::from_value)
        .expect("typed integration connector config should serialize as a field map");
    aster_drive_storage::ConnectorConfigEnvelope::new(
        aster_drive_storage::ConnectorId::declared(connector_id),
        schema_version,
        values,
    )
}

pub fn encoded_policy_config<T: Serialize>(
    connector_id: &'static str,
    values: T,
    behavior: aster_drive_storage::StoragePolicyBehaviorConfig,
) -> aster_drive_model::types::StoredStoragePolicyConfig {
    aster_drive_model::types::StoredStoragePolicyConfig(
        aster_drive_storage::encode_storage_policy_config(
            aster_drive_storage::ConnectorConfigEnvelope::new(
                aster_drive_storage::ConnectorId::declared(connector_id),
                1,
                serde_json::to_value(values)
                    .expect("typed integration connector config should serialize"),
            ),
            behavior,
        )
        .expect("typed integration storage policy config should encode"),
    )
}

pub fn local_connection(
    base_path: impl Into<String>,
) -> aster_drive::storage::StorageConnectorConnectionInput {
    aster_drive::storage::StorageConnectorConnectionInput {
        connector_config: connector_envelope(
            "asterdrive.storage.local",
            TestLocalConnectorConfigV1 {
                base_path: base_path.into(),
                content_dedup: false,
            },
        ),
        behavior: aster_drive_storage::StoragePolicyBehaviorConfig::default(),
        credential: aster_drive::storage::StorageConnectorCredentialInput::None,
    }
}

pub fn local_connection_json(base_path: impl Into<String>) -> serde_json::Value {
    serde_json::to_value(local_connection(base_path))
        .expect("local integration connection should serialize")
}

pub fn s3_connection(
    endpoint: impl Into<String>,
    bucket: impl Into<String>,
    base_path: impl Into<String>,
    access_key: impl Into<String>,
    secret_key: impl Into<String>,
) -> aster_drive::storage::StorageConnectorConnectionInput {
    s3_connection_with_strategies(
        endpoint,
        bucket,
        base_path,
        access_key,
        secret_key,
        aster_drive_model::types::ObjectStorageUploadStrategy::RelayStream,
        aster_drive_model::types::ObjectStorageDownloadStrategy::RelayStream,
    )
}

pub fn s3_connection_json(
    endpoint: impl Into<String>,
    bucket: impl Into<String>,
    base_path: impl Into<String>,
    access_key: impl Into<String>,
    secret_key: impl Into<String>,
) -> serde_json::Value {
    serde_json::to_value(s3_connection(
        endpoint, bucket, base_path, access_key, secret_key,
    ))
    .expect("S3 integration connection should serialize")
}

pub fn s3_connection_with_strategies(
    endpoint: impl Into<String>,
    bucket: impl Into<String>,
    base_path: impl Into<String>,
    access_key: impl Into<String>,
    secret_key: impl Into<String>,
    upload_strategy: aster_drive_model::types::ObjectStorageUploadStrategy,
    download_strategy: aster_drive_model::types::ObjectStorageDownloadStrategy,
) -> aster_drive::storage::StorageConnectorConnectionInput {
    aster_drive::storage::StorageConnectorConnectionInput {
        connector_config: connector_envelope(
            "asterdrive.storage.s3",
            TestS3ConnectorConfigV1 {
                endpoint: endpoint.into(),
                bucket: bucket.into(),
                base_path: base_path.into(),
                object_storage_upload_strategy: upload_strategy,
                object_storage_download_strategy: download_strategy,
                s3_path_style: true,
                s3_region: "us-east-1".to_string(),
                s3_connect_timeout_secs: 5,
                s3_read_timeout_secs: 30,
                s3_operation_timeout_secs: 3_600,
            },
        ),
        behavior: aster_drive_storage::StoragePolicyBehaviorConfig::default(),
        credential: aster_drive::storage::StorageConnectorCredentialInput::Static(
            serde_json::to_value(TestS3StaticCredentialsV1 {
                s3_access_key_id: access_key.into(),
                s3_secret_access_key: secret_key.into(),
            })
            .expect("typed S3 integration credentials should serialize"),
        ),
    }
}

pub fn tencent_cos_connection(
    endpoint: impl Into<String>,
    bucket: impl Into<String>,
    base_path: impl Into<String>,
    secret_id: impl Into<String>,
    secret_key: impl Into<String>,
) -> aster_drive::storage::StorageConnectorConnectionInput {
    aster_drive::storage::StorageConnectorConnectionInput {
        connector_config: connector_envelope_with_schema(
            "asterdrive.storage.tencent_cos",
            1,
            TestTencentCosConnectorConfigV1 {
                endpoint: endpoint.into(),
                bucket: bucket.into(),
                base_path: base_path.into(),
                object_storage_upload_strategy:
                    aster_drive_model::types::ObjectStorageUploadStrategy::RelayStream,
                object_storage_download_strategy:
                    aster_drive_model::types::ObjectStorageDownloadStrategy::RelayStream,
            },
        ),
        behavior: aster_drive_storage::StoragePolicyBehaviorConfig::default(),
        credential: aster_drive::storage::StorageConnectorCredentialInput::Static(
            serde_json::to_value(TestTencentCosStaticCredentialsV1 {
                tencent_cos_secret_id: secret_id.into(),
                tencent_cos_secret_key: secret_key.into(),
            })
            .expect("typed Tencent COS integration credentials should serialize"),
        ),
    }
}

pub fn remote_connection(
    base_path: impl Into<String>,
    remote_node_id: Option<i64>,
    remote_storage_target_key: Option<String>,
    remote_download_strategy: aster_drive_model::types::RemoteDownloadStrategy,
    remote_upload_strategy: aster_drive_model::types::RemoteUploadStrategy,
) -> aster_drive::storage::StorageConnectorConnectionInput {
    aster_drive::storage::StorageConnectorConnectionInput {
        connector_config: connector_envelope(
            "asterdrive.storage.remote",
            TestRemoteConnectorConfigV1 {
                base_path: base_path.into(),
                remote_node_id,
                remote_storage_target_key,
                remote_download_strategy,
                remote_upload_strategy,
            },
        ),
        behavior: aster_drive_storage::StoragePolicyBehaviorConfig::default(),
        credential: aster_drive::storage::StorageConnectorCredentialInput::None,
    }
}

pub fn remote_connector_config(
    base_path: impl Into<String>,
    remote_node_id: Option<i64>,
    remote_storage_target_key: Option<String>,
    remote_download_strategy: aster_drive_model::types::RemoteDownloadStrategy,
    remote_upload_strategy: aster_drive_model::types::RemoteUploadStrategy,
) -> aster_drive_storage::ConnectorConfigEnvelope {
    connector_envelope(
        "asterdrive.storage.remote",
        TestRemoteConnectorConfigV1 {
            base_path: base_path.into(),
            remote_node_id,
            remote_storage_target_key,
            remote_download_strategy,
            remote_upload_strategy,
        },
    )
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestS3StaticCredentialsV1 {
    pub s3_access_key_id: String,
    pub s3_secret_access_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestTencentCosStaticCredentialsV1 {
    pub tencent_cos_secret_id: String,
    pub tencent_cos_secret_key: String,
}

pub fn local_policy_base_path(
    policy: &aster_drive_model::entities::storage_policy::Model,
) -> String {
    local_policy_config(policy).base_path
}

pub fn local_policy_config(
    policy: &aster_drive_model::entities::storage_policy::Model,
) -> TestLocalConnectorConfigV1 {
    aster_drive_storage::decode_storage_policy_config::<TestLocalConnectorConfigV1>(
        policy.storage_config.as_ref(),
        &aster_drive_storage::ConnectorId::declared("asterdrive.storage.local"),
        1,
    )
    .expect("typed local integration policy should decode")
    .0
}

pub fn with_local_policy_base_path(
    policy: &aster_drive_model::entities::storage_policy::Model,
    base_path: impl Into<String>,
) -> aster_drive_model::types::StoredStoragePolicyConfig {
    let (mut config, behavior) =
        aster_drive_storage::decode_storage_policy_config::<TestLocalConnectorConfigV1>(
            policy.storage_config.as_ref(),
            &aster_drive_storage::ConnectorId::declared("asterdrive.storage.local"),
            1,
        )
        .expect("typed local integration policy should decode");
    config.base_path = base_path.into();
    encoded_policy_config("asterdrive.storage.local", config, behavior)
}

pub fn with_local_content_dedup(
    policy: &aster_drive_model::entities::storage_policy::Model,
    enabled: bool,
) -> aster_drive_model::types::StoredStoragePolicyConfig {
    let (mut config, behavior) =
        aster_drive_storage::decode_storage_policy_config::<TestLocalConnectorConfigV1>(
            policy.storage_config.as_ref(),
            &aster_drive_storage::ConnectorId::declared("asterdrive.storage.local"),
            1,
        )
        .expect("typed local integration policy should decode");
    config.content_dedup = enabled;
    encoded_policy_config("asterdrive.storage.local", config, behavior)
}

pub fn with_storage_policy_behavior(
    policy: &aster_drive_model::entities::storage_policy::Model,
    behavior: aster_drive_storage::StoragePolicyBehaviorConfig,
) -> aster_drive_model::types::StoredStoragePolicyConfig {
    let envelope: aster_drive_storage::StoragePolicyConfigEnvelope =
        serde_json::from_str(policy.storage_config.as_ref())
            .expect("typed integration storage policy envelope should decode");
    aster_drive_model::types::StoredStoragePolicyConfig(
        aster_drive_storage::encode_storage_policy_config(envelope.connector, behavior)
            .expect("typed integration storage policy config should encode"),
    )
}

pub fn s3_policy_base_path(policy: &aster_drive_model::entities::storage_policy::Model) -> String {
    aster_drive_storage::decode_storage_policy_config::<TestS3ConnectorConfigV1>(
        policy.storage_config.as_ref(),
        &aster_drive_storage::ConnectorId::declared("asterdrive.storage.s3"),
        1,
    )
    .expect("typed S3 integration policy should decode")
    .0
    .base_path
}

pub fn remote_policy_config(
    policy: &aster_drive_model::entities::storage_policy::Model,
) -> TestRemoteConnectorConfigV1 {
    aster_drive_storage::decode_storage_policy_config::<TestRemoteConnectorConfigV1>(
        policy.storage_config.as_ref(),
        &aster_drive_storage::ConnectorId::declared("asterdrive.storage.remote"),
        1,
    )
    .expect("typed remote integration policy should decode")
    .0
}

fn init_test_process_state() {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {});
}

pub async fn set_foreign_key_checks(
    db: &sea_orm::DatabaseConnection,
    enabled: bool,
) -> Result<(), sea_orm::DbErr> {
    use sea_orm::ConnectionTrait;

    let sql = match (db.get_database_backend(), enabled) {
        (sea_orm::DbBackend::Sqlite, true) => "PRAGMA foreign_keys=ON;",
        (sea_orm::DbBackend::Sqlite, false) => "PRAGMA foreign_keys=OFF;",
        (sea_orm::DbBackend::Postgres, true) => "SET session_replication_role = origin;",
        (sea_orm::DbBackend::Postgres, false) => "SET session_replication_role = replica;",
        (sea_orm::DbBackend::MySql, true) => "SET FOREIGN_KEY_CHECKS = 1;",
        (sea_orm::DbBackend::MySql, false) => "SET FOREIGN_KEY_CHECKS = 0;",
        _ => return Ok(()),
    };

    db.execute_unprepared(sql).await.map(|_| ())
}

pub async fn bind_policy_to_folder(
    state: &aster_drive::runtime::PrimaryAppState,
    folder_id: i64,
    policy_id: i64,
) {
    aster_drive::services::files::folder::admin_set_policy_with_audit(
        state,
        folder_id,
        Some(policy_id),
        &aster_drive::services::ops::audit::AuditContext::system(),
    )
    .await
    .expect("policy should bind to folder");
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TestDatabaseBackend {
    Sqlite,
    Postgres,
    MySql,
}

#[derive(Clone)]
struct MySqlSchemaTemplate {
    create_table_sql: Vec<String>,
    migration_rows: Vec<(String, i64)>,
}

static POSTGRES_TEST_CONTAINER: tokio::sync::OnceCell<PostgresTestContainer> =
    tokio::sync::OnceCell::const_new();
static MYSQL_TEST_CONTAINER: tokio::sync::OnceCell<MysqlTestContainer> =
    tokio::sync::OnceCell::const_new();
const POSTGRES_TEMPLATE_FIXTURE: &str = "postgres-template";
const MYSQL_TEMPLATE_FIXTURE: &str = "mysql-schema-template";
const TEST_FIXTURE_PRODUCER_VERSION: &str = env!("CARGO_PKG_VERSION");
static MYSQL_SCHEMA_TEMPLATE_CACHE: tokio::sync::OnceCell<MySqlSchemaTemplate> =
    tokio::sync::OnceCell::const_new();

fn test_container_suite() -> &'static TestContainerSuite {
    static SUITE: OnceLock<TestContainerSuite> = OnceLock::new();
    SUITE.get_or_init(|| TestContainerSuite::new("asterdrive"))
}

async fn drop_stale_test_databases(
    backend: sea_orm::DbBackend,
    admin_database_url: &str,
    database_names: &[String],
) {
    if database_names.is_empty() {
        return;
    }

    use sea_orm::ConnectionTrait;

    let admin_cfg = aster_drive::config::DatabaseConfig {
        url: admin_database_url.into(),
        pool_size: 1,
        retry_count: 0,
    };
    let admin_db =
        aster_drive::db::connect_with_metrics(&admin_cfg, aster_drive_metrics::NoopMetrics::arc())
            .await
            .expect("stale test database cleanup should connect");

    for database_name in database_names {
        let drop_sql = format!(
            "DROP DATABASE IF EXISTS {}",
            quote_database_identifier(backend, database_name)
        );
        admin_db
            .execute_unprepared(&drop_sql)
            .await
            .expect("stale test database should drop");
    }
    admin_db
        .close()
        .await
        .expect("stale test database cleanup connection should close");
}

async fn configure_mysql_test_user(admin_database_url: &str, username: &str, password: &str) {
    use sea_orm::ConnectionTrait;

    let admin_cfg = aster_drive::config::DatabaseConfig {
        url: admin_database_url.into(),
        pool_size: 1,
        retry_count: 0,
    };
    let admin_db =
        aster_drive::db::connect_with_metrics(&admin_cfg, aster_drive_metrics::NoopMetrics::arc())
            .await
            .expect("mysql test admin connection should succeed");

    let username = quote_mysql_string(username);
    let password = quote_mysql_string(password);
    admin_db
        .execute_unprepared(&format!(
            "CREATE USER IF NOT EXISTS {username}@'%' IDENTIFIED BY {password}"
        ))
        .await
        .expect("mysql test user should exist");
    admin_db
        .execute_unprepared(&format!(
            "ALTER USER {username}@'%' IDENTIFIED BY {password}"
        ))
        .await
        .expect("mysql test user password should be current");

    let grant_sql = format!("GRANT ALL PRIVILEGES ON *.* TO {username}@'%'");
    admin_db
        .execute_unprepared(&grant_sql)
        .await
        .expect("mysql test user grant should succeed");
    admin_db
        .close()
        .await
        .expect("mysql test user setup connection should close");
}

pub fn remember_csrf_token(session_token: &str, csrf_token: &str) {
    if session_token.is_empty() || csrf_token.is_empty() {
        return;
    }

    CSRF_LOOKUP_CACHE.with(|cache| {
        cache
            .borrow_mut()
            .insert(session_token.to_string(), csrf_token.to_string());
    });

    lock_csrf_registry().insert(session_token.to_string(), csrf_token.to_string());
}

pub fn seed_csrf_token(session_token: &str) -> String {
    let csrf_token = aster_forge_actix_middleware::csrf::build_csrf_token();
    remember_csrf_token(session_token, &csrf_token);
    csrf_token
}

#[track_caller]
pub fn expect_authenticated_login(
    completion: aster_drive::services::auth::mfa::PrimaryLoginCompletion,
) -> aster_drive::services::auth::local::LoginResult {
    match completion {
        aster_drive::services::auth::mfa::PrimaryLoginCompletion::Authenticated(login) => login,
        aster_drive::services::auth::mfa::PrimaryLoginCompletion::MfaRequired(_) => {
            panic!("expected login to complete without MFA challenge")
        }
    }
}

pub fn csrf_token_for(session_token: impl AsRef<str>) -> String {
    let session_token = session_token.as_ref();
    if let Some(token) = CSRF_LOOKUP_CACHE.with(|cache| cache.borrow().get(session_token).cloned())
    {
        return token;
    }

    lock_csrf_registry()
        .get(session_token)
        .cloned()
        .inspect(|csrf_token| {
            CSRF_LOOKUP_CACHE.with(|cache| {
                cache
                    .borrow_mut()
                    .insert(session_token.to_string(), csrf_token.clone());
            });
        })
        .unwrap_or_else(|| panic!("missing csrf token for session token: {session_token}"))
}

pub fn access_cookie_header(access_token: impl AsRef<str>) -> String {
    let access_token = access_token.as_ref();
    format!(
        "aster_access={access_token}; aster_csrf={}",
        csrf_token_for(access_token)
    )
}

pub fn refresh_cookie_header(refresh_token: impl AsRef<str>) -> String {
    let refresh_token = refresh_token.as_ref();
    format!(
        "aster_refresh={refresh_token}; aster_csrf={}",
        csrf_token_for(refresh_token)
    )
}

pub fn access_and_refresh_cookie_header(
    access_token: impl AsRef<str>,
    refresh_token: impl AsRef<str>,
) -> String {
    let access_token = access_token.as_ref();
    let refresh_token = refresh_token.as_ref();
    format!(
        "aster_access={access_token}; aster_refresh={refresh_token}; aster_csrf={}",
        csrf_token_for(access_token)
    )
}

pub fn csrf_header_for(session_token: impl AsRef<str>) -> (&'static str, String) {
    ("X-CSRF-Token", csrf_token_for(session_token))
}

fn configured_test_database_backend() -> TestDatabaseBackend {
    match std::env::var(TEST_DATABASE_BACKEND_ENV)
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        None | Some("") | Some("sqlite") => TestDatabaseBackend::Sqlite,
        Some("postgres") | Some("postgresql") => TestDatabaseBackend::Postgres,
        Some("mysql") => TestDatabaseBackend::MySql,
        Some(other) => panic!(
            "unsupported {TEST_DATABASE_BACKEND_ENV} value '{other}', expected sqlite/postgres/mysql"
        ),
    }
}

async fn start_mysql_test_container() -> MysqlTestContainer {
    let container = MysqlTestContainer::start(test_container_suite()).await;
    configure_mysql_test_user(container.root_url(), "aster", "asterpass").await;
    drop_stale_test_databases(
        sea_orm::DbBackend::MySql,
        container.root_url(),
        container.stale_resources(),
    )
    .await;
    container.forget_resources(container.stale_resources());
    container
}

fn product_database_url(database_url: &str, backend: TestDatabaseBackend) -> String {
    let mut url = reqwest::Url::parse(database_url).expect("test database URL should parse");
    url.set_path("/asterdrive");
    if backend == TestDatabaseBackend::MySql {
        url.set_username("aster")
            .expect("MySQL test URL should accept a username");
        url.set_password(Some("asterpass"))
            .expect("MySQL test URL should accept a password");
    }
    url.to_string()
}

async fn shared_test_database_urls(backend: TestDatabaseBackend) -> (String, String) {
    match backend {
        TestDatabaseBackend::Sqlite => {
            ("sqlite::memory:".to_string(), "sqlite::memory:".to_string())
        }
        TestDatabaseBackend::Postgres => {
            let container = POSTGRES_TEST_CONTAINER
                .get_or_init(|| PostgresTestContainer::start(test_container_suite()))
                .await;
            (
                container.admin_url().to_string(),
                product_database_url(container.admin_url(), backend),
            )
        }
        TestDatabaseBackend::MySql => {
            let container = MYSQL_TEST_CONTAINER
                .get_or_init(start_mysql_test_container)
                .await;
            (
                container.root_url().to_string(),
                product_database_url(container.root_url(), backend),
            )
        }
    }
}

fn sanitized_database_name_prefix(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();

    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        "asterdrive".to_string()
    } else {
        trimmed.to_string()
    }
}

fn isolated_database_name(base_name: &str, max_len: usize) -> String {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let reserved = "_it_".len() + suffix.len();
    let max_prefix_len = max_len.saturating_sub(reserved).max(1);
    let prefix: String = sanitized_database_name_prefix(base_name)
        .chars()
        .take(max_prefix_len)
        .collect();

    format!("{prefix}_it_{suffix}")
}

fn database_name_from_url(url: &reqwest::Url) -> Option<String> {
    url.path_segments()
        .and_then(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .rfind(|segment| !segment.is_empty())
                .map(str::to_string)
        })
        .filter(|value| !value.is_empty())
}

fn replace_database_name(mut url: reqwest::Url, database_name: &str) -> String {
    url.set_path(&format!("/{database_name}"));
    url.to_string()
}

fn quote_database_identifier(backend: sea_orm::DbBackend, database_name: &str) -> String {
    match backend {
        sea_orm::DbBackend::Postgres => format!("\"{}\"", database_name.replace('"', "\"\"")),
        sea_orm::DbBackend::MySql => format!("`{}`", database_name.replace('`', "``")),
        _ => database_name.to_string(),
    }
}

fn quote_mysql_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

async fn provision_isolated_test_database_url_with_template(
    admin_database_url: &str,
    database_url: &str,
    template_database_name: Option<&str>,
) -> String {
    if database_url == "sqlite::memory:" || database_url.starts_with("sqlite://") {
        return database_url.to_string();
    }

    let parsed_url = reqwest::Url::parse(database_url).unwrap();
    let base_name = database_name_from_url(&parsed_url).unwrap_or_else(|| "asterdrive".to_string());
    let backend = match parsed_url.scheme() {
        "postgres" | "postgresql" => sea_orm::DbBackend::Postgres,
        "mysql" => sea_orm::DbBackend::MySql,
        scheme => panic!("unsupported isolated test database URL scheme: {scheme}"),
    };

    let isolated_name = match backend {
        sea_orm::DbBackend::Postgres => isolated_database_name(&base_name, 63),
        sea_orm::DbBackend::MySql => isolated_database_name(&base_name, 64),
        _ => unreachable!("isolated database provisioning only supports postgres/mysql"),
    };

    match backend {
        sea_orm::DbBackend::Postgres => {
            let container = POSTGRES_TEST_CONTAINER
                .get_or_init(|| PostgresTestContainer::start(test_container_suite()))
                .await;
            let database = match template_database_name {
                Some(template) => {
                    container
                        .create_database_from_template(&isolated_name, template)
                        .await
                }
                None => container.create_database(&isolated_name).await,
            };
            database.url().to_string()
        }
        sea_orm::DbBackend::MySql => {
            use sea_orm::ConnectionTrait;

            let container = MYSQL_TEST_CONTAINER
                .get_or_init(start_mysql_test_container)
                .await;
            container.remember_resource(&isolated_name);

            let admin_cfg = aster_drive::config::DatabaseConfig {
                url: admin_database_url.into(),
                pool_size: 1,
                retry_count: 0,
            };
            let admin_db = aster_drive::db::connect_with_metrics(
                &admin_cfg,
                aster_drive_metrics::NoopMetrics::arc(),
            )
            .await
            .expect("MySQL test admin connection should succeed");
            admin_db
                .execute_unprepared(&format!(
                    "CREATE DATABASE {}",
                    quote_database_identifier(backend, &isolated_name)
                ))
                .await
                .expect("isolated MySQL test database should be created");
            admin_db
                .close()
                .await
                .expect("MySQL test admin connection should close");

            replace_database_name(parsed_url, &isolated_name)
        }
        _ => unreachable!("isolated database provisioning only supports postgres/mysql"),
    }
}

async fn provision_isolated_test_database_url(
    admin_database_url: &str,
    database_url: &str,
) -> String {
    provision_isolated_test_database_url_with_template(admin_database_url, database_url, None).await
}

fn database_fixture_fingerprint() -> String {
    env!("ASTER_TEST_SCHEMA_FINGERPRINT").to_string()
}

fn fixture_database_name(prefix: &str, fingerprint: &str, max_len: usize) -> String {
    let suffix: String = fingerprint
        .strip_prefix("migration-src-")
        .unwrap_or(fingerprint)
        .bytes()
        .filter(u8::is_ascii_alphanumeric)
        .take(16)
        .map(char::from)
        .collect();
    let reserved = "_template_".len() + suffix.len();
    let prefix_len = max_len.saturating_sub(reserved).max(1);
    format!(
        "{}_template_{suffix}",
        sanitized_database_name_prefix(prefix)
            .chars()
            .take(prefix_len)
            .collect::<String>()
    )
}

async fn fixture_database_is_usable(database_url: &str) -> bool {
    use sea_orm::{ConnectionTrait, Statement};

    let db_cfg = aster_drive::config::DatabaseConfig {
        url: database_url.into(),
        pool_size: 1,
        retry_count: 0,
    };
    let Ok(db) =
        aster_drive::db::connect_with_metrics(&db_cfg, aster_drive_metrics::NoopMetrics::arc())
            .await
    else {
        return false;
    };
    let backend = db.get_database_backend();
    let usable = db
        .query_one_raw(Statement::from_string(
            backend,
            "SELECT 1 FROM seaql_migrations LIMIT 1",
        ))
        .await
        .is_ok();
    let closed = db.close().await.is_ok();
    usable && closed
}

async fn ensure_postgres_template_locked(
    container: &PostgresTestContainer,
    fixture_lock: &SuiteFixtureLock,
    fingerprint: &str,
) -> String {
    let container_identity = container.container_identity();
    if let Some(state) = fixture_lock.load() {
        if state.matches(
            POSTGRES_TEMPLATE_FIXTURE,
            container_identity,
            fingerprint,
            TEST_FIXTURE_PRODUCER_VERSION,
        ) && fixture_database_is_usable(&replace_database_name(
            reqwest::Url::parse(container.admin_url())
                .expect("PostgreSQL test admin URL should parse"),
            state.resource(),
        ))
        .await
        {
            return state.resource().to_string();
        }
        container.drop_shared_database(state.resource()).await;
        fixture_lock.clear();
    }

    let template_name = fixture_database_name("asterdrive_pg", fingerprint, 63);
    container.drop_shared_database(&template_name).await;
    let template = container.create_shared_database(&template_name).await;
    let db_cfg = aster_drive::config::DatabaseConfig {
        url: template.url().into(),
        pool_size: 1,
        retry_count: 0,
    };
    let db =
        aster_drive::db::connect_with_metrics(&db_cfg, aster_drive_metrics::NoopMetrics::arc())
            .await
            .expect("postgres template database connection should succeed");
    use aster_drive_migration::Migrator;
    Migrator::up(&db, None)
        .await
        .expect("postgres template database migrations should succeed");
    db.close()
        .await
        .expect("postgres template database should close cleanly");

    fixture_lock.publish(&SuiteFixtureState::new(
        POSTGRES_TEMPLATE_FIXTURE,
        container_identity,
        fingerprint,
        &template_name,
        TEST_FIXTURE_PRODUCER_VERSION,
    ));
    template_name
}

async fn resolve_test_database_url_for(backend: TestDatabaseBackend) -> String {
    let (admin_database_url, database_url) = shared_test_database_urls(backend).await;
    match backend {
        TestDatabaseBackend::Postgres => {
            let container = POSTGRES_TEST_CONTAINER
                .get_or_init(|| PostgresTestContainer::start(test_container_suite()))
                .await;
            let fixture_lock =
                SuiteFixtureLock::acquire(test_container_suite(), POSTGRES_TEMPLATE_FIXTURE);
            let fingerprint = database_fixture_fingerprint();
            let template_name =
                ensure_postgres_template_locked(container, &fixture_lock, &fingerprint).await;
            provision_isolated_test_database_url_with_template(
                &admin_database_url,
                &database_url,
                Some(&template_name),
            )
            .await
        }
        _ => provision_isolated_test_database_url(&admin_database_url, &database_url).await,
    }
}

async fn resolve_test_database_url() -> String {
    resolve_test_database_url_for(configured_test_database_backend()).await
}

pub async fn postgres_test_database_url() -> String {
    resolve_test_database_url_for(TestDatabaseBackend::Postgres).await
}

pub async fn mysql_test_database_url() -> String {
    resolve_test_database_url_for(TestDatabaseBackend::MySql).await
}

pub async fn postgres_empty_test_database_url() -> String {
    let (admin_database_url, database_url) =
        shared_test_database_urls(TestDatabaseBackend::Postgres).await;
    provision_isolated_test_database_url(&admin_database_url, &database_url).await
}

pub async fn mysql_empty_test_database_url() -> String {
    let (admin_database_url, database_url) =
        shared_test_database_urls(TestDatabaseBackend::MySql).await;
    provision_isolated_test_database_url(&admin_database_url, &database_url).await
}

/// 构建一个干净的测试 PrimaryAppState。
///
/// 默认使用内存 SQLite。若设置 `ASTER_TEST_DATABASE_BACKEND=postgres|mysql`，
/// 会自动启动一个共享 testcontainers 容器，并为当前测试实例分配独立数据库。
pub async fn setup() -> PrimaryAppState {
    setup_with_pool_size(1).await
}

/// 构建一个干净的测试 PrimaryAppState，并为支持并发连接的数据库配置 writer pool。
///
/// SQLite 的生产连接适配器会固定为单 writer connection；PostgreSQL/MySQL 使用调用方
/// 指定的连接数，以便集成测试覆盖真实并发事务。
pub async fn setup_with_pool_size(pool_size: u32) -> PrimaryAppState {
    init_test_process_state();
    let database_url = resolve_test_database_url().await;
    setup_with_database_url_and_pool_size(&database_url, pool_size).await
}

/// 构建使用内存缓存的测试 PrimaryAppState。
pub async fn setup_with_memory_cache() -> PrimaryAppState {
    let base = setup().await;
    let cache_config = aster_forge_cache::CacheConfig {
        backend: "memory".to_string(),
        default_ttl: 60,
        ..Default::default()
    };
    let cache = aster_forge_cache::create_cache(&cache_config).await;

    PrimaryAppState {
        db_handles: base.db_handles,
        driver_registry: base.driver_registry,
        runtime_config: base.runtime_config,
        policy_snapshot: base.policy_snapshot,
        config: base.config,
        cache,
        config_sync: base.config_sync,
        metrics: aster_drive_metrics::NoopMetrics::arc(),
        mail_sender: base.mail_sender,
        storage_change_bus: base.storage_change_bus,
        share_download_rollback: base.share_download_rollback,
        background_task_dispatch_wakeup: base.background_task_dispatch_wakeup,
        remote_protocol: base.remote_protocol,
    }
}

pub fn test_password_hash_policy(memory_kib: u32) -> aster_forge_crypto::PasswordHashPolicy {
    aster_forge_crypto::PasswordHashPolicy::new(
        aster_forge_crypto::PasswordHashWorkFactor::new(memory_kib, 1, 1, 32).unwrap(),
        aster_forge_crypto::PasswordHashVerificationLimits::new(64 * 1024, 3, 4, 32).unwrap(),
    )
    .unwrap()
}

pub async fn configure_test_password_hash_policy(state: &mut PrimaryAppState, memory_kib: u32) {
    configure_test_password_hash_runtime(state, memory_kib, 1).await;
}

pub async fn configure_test_password_hash_runtime(
    state: &mut PrimaryAppState,
    memory_kib: u32,
    max_concurrency: usize,
) {
    let runtime_config = std::sync::Arc::new(
        aster_drive::config::RuntimeConfig::with_password_hash_policy(
            max_concurrency,
            test_password_hash_policy(memory_kib),
        )
        .unwrap(),
    );
    runtime_config.reload(state.writer_db()).await.unwrap();
    state.runtime_config = runtime_config;
}

/// Creates a test account while respecting the production initialization lifecycle.
///
/// The first fixture account goes through `setup`; later fixture accounts use ordinary
/// registration. Tests that exercise setup/register behavior directly should call those service
/// functions themselves instead of this convenience helper.
pub async fn create_test_account(
    state: &PrimaryAppState,
    username: &str,
    email: &str,
    password: &str,
) -> aster_drive::errors::Result<aster_drive::services::auth::local::AuthUserInfo> {
    match aster_drive::services::system_setup::state(state.writer_db()).await? {
        aster_drive::services::system_setup::SystemSetupState::NeedsAdmin => {
            aster_drive::services::auth::local::setup(state, username, email, password).await
        }
        aster_drive::services::system_setup::SystemSetupState::Ready => {
            aster_drive::services::auth::local::register(state, username, email, password).await
        }
        aster_drive::services::system_setup::SystemSetupState::NeedsStorage => {
            Err(aster_drive::errors::AsterError::internal_error(
                "test account helper requires storage setup to be complete",
            ))
        }
    }
}

async fn create_test_account_at_api_endpoint<S, B, E>(
    app: &S,
    endpoint: &str,
    username: &str,
    email: &str,
    password: &str,
) -> i64
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = E,
        >,
    B: actix_web::body::MessageBody,
    B::Error: std::fmt::Debug,
    E: std::fmt::Debug,
{
    let mut request = actix_web::test::TestRequest::post()
        .uri(endpoint)
        .peer_addr("127.0.0.1:12345".parse().unwrap());
    if endpoint == "/api/v1/auth/setup" {
        // Generic fixtures must not derive public_site_url from a synthetic request host.
        request = request.insert_header(("Host", "@"));
    }
    let request = request
        .set_json(serde_json::json!({
            "username": username,
            "email": email,
            "password": password,
        }))
        .to_request();
    let response = actix_web::test::call_service(app, request).await;
    assert_eq!(response.status(), 201, "account creation should return 201");
    let body: serde_json::Value = actix_web::test::read_body_json(response).await;
    body["data"]["id"]
        .as_i64()
        .expect("account creation response should contain user id")
}

/// Creates the initial administrator through the public setup endpoint.
pub async fn setup_test_account_via_api<S, B, E>(
    app: &S,
    username: &str,
    email: &str,
    password: &str,
) -> i64
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = E,
        >,
    B: actix_web::body::MessageBody,
    B::Error: std::fmt::Debug,
    E: std::fmt::Debug,
{
    create_test_account_at_api_endpoint(app, "/api/v1/auth/setup", username, email, password).await
}

/// Creates an empty personal-workspace file through the canonical HTTP lifecycle.
pub async fn create_empty_file_via_api<S, B, E>(
    app: &S,
    access_token: &str,
    name: &str,
    folder_id: Option<i64>,
) -> i64
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = E,
        >,
    B: actix_web::body::MessageBody,
    B::Error: std::fmt::Debug,
    E: std::fmt::Debug,
{
    let request = actix_web::test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", access_cookie_header(access_token)))
        .insert_header(csrf_header_for(access_token))
        .set_json(serde_json::json!({
            "name": name,
            "folder_id": folder_id,
        }))
        .to_request();
    let response = actix_web::test::call_service(app, request).await;
    assert_eq!(
        response.status(),
        201,
        "empty file creation should return 201"
    );
    let body: serde_json::Value = actix_web::test::read_body_json(response).await;
    body["data"]["id"]
        .as_i64()
        .expect("empty file response should contain file id")
}

/// Creates an account through the production setup/register lifecycle and confirms registration
/// email when that policy is enabled.
pub async fn create_test_account_via_api<S, B, E>(
    app: &S,
    db: &sea_orm::DatabaseConnection,
    mail_sender: &std::sync::Arc<dyn aster_forge_mail::MailSender>,
    username: &str,
    email: &str,
    password: &str,
) -> i64
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = E,
        >,
    B: actix_web::body::MessageBody,
    B::Error: std::fmt::Debug,
    E: std::fmt::Debug,
{
    let setup_state = aster_drive::services::system_setup::state(db)
        .await
        .expect("test setup state should load");
    if setup_state == aster_drive::services::system_setup::SystemSetupState::NeedsAdmin {
        return setup_test_account_via_api(app, username, email, password).await;
    }
    assert_eq!(
        setup_state,
        aster_drive::services::system_setup::SystemSetupState::Ready,
        "test account helper requires storage setup to be complete"
    );

    let user_id = create_test_account_at_api_endpoint(
        app,
        "/api/v1/auth/register",
        username,
        email,
        password,
    )
    .await;
    if let Some(token) =
        extract_verification_token_from_mail_sender_or_outbox(db, mail_sender).await
    {
        let request = actix_web::test::TestRequest::get()
            .uri(&format!(
                "/api/v1/auth/contact-verification/confirm?token={}",
                urlencoding::encode(&token)
            ))
            .to_request();
        let response = actix_web::test::call_service(app, request).await;
        assert_eq!(
            response.status(),
            302,
            "contact verification should return 302"
        );
    }
    user_id
}

fn should_use_mysql_schema_template(database_url: &str) -> bool {
    database_url.starts_with("mysql://")
        && std::env::var("ASTER_TEST_DISABLE_MYSQL_SCHEMA_TEMPLATE").as_deref() != Ok("1")
}

async fn load_mysql_schema_template(db: &sea_orm::DatabaseConnection) -> MySqlSchemaTemplate {
    use sea_orm::{ConnectionTrait, Statement};

    let tables = db
        .query_all_raw(Statement::from_string(
            sea_orm::DbBackend::MySql,
            "SHOW FULL TABLES WHERE Table_type = 'BASE TABLE'",
        ))
        .await
        .expect("mysql schema template should list tables");

    let mut table_names: Vec<String> = tables
        .into_iter()
        .map(|row| {
            row.try_get_by_index(0)
                .expect("mysql schema template table name should exist")
        })
        .collect();
    table_names.sort();

    let mut create_table_sql = Vec::with_capacity(table_names.len());
    for table_name in &table_names {
        let ddl_row = db
            .query_one_raw(Statement::from_string(
                sea_orm::DbBackend::MySql,
                format!(
                    "SHOW CREATE TABLE {}",
                    quote_database_identifier(sea_orm::DbBackend::MySql, table_name)
                ),
            ))
            .await
            .expect("mysql schema template should load table ddl")
            .expect("mysql schema template show create table should return one row");

        let ddl: String = ddl_row
            .try_get_by_index(1)
            .expect("mysql schema template ddl should exist");
        create_table_sql.push(ddl);
    }

    let migration_rows = db
        .query_all_raw(Statement::from_string(
            sea_orm::DbBackend::MySql,
            "SELECT version, applied_at FROM seaql_migrations ORDER BY version",
        ))
        .await
        .expect("mysql schema template should load migration history")
        .into_iter()
        .map(|row| {
            let version = row
                .try_get_by_index(0)
                .expect("mysql schema template migration version should exist");
            let applied_at = row
                .try_get_by_index(1)
                .expect("mysql schema template migration timestamp should exist");
            (version, applied_at)
        })
        .collect();

    MySqlSchemaTemplate {
        create_table_sql,
        migration_rows,
    }
}

async fn ensure_mysql_schema_template_locked(
    container: &MysqlTestContainer,
    fixture_lock: &SuiteFixtureLock,
    fingerprint: &str,
) -> MySqlSchemaTemplate {
    let container_identity = container.container_identity();
    if let Some(state) = fixture_lock.load() {
        if state.matches(
            MYSQL_TEMPLATE_FIXTURE,
            container_identity,
            fingerprint,
            TEST_FIXTURE_PRODUCER_VERSION,
        ) && fixture_database_is_usable(&container.database_url(state.resource())).await
        {
            if let Some(template) = MYSQL_SCHEMA_TEMPLATE_CACHE.get() {
                return template.clone();
            }
            let db_cfg = aster_drive::config::DatabaseConfig {
                url: container.database_url(state.resource()).into(),
                pool_size: 1,
                retry_count: 0,
            };
            let db = aster_drive::db::connect_with_metrics(
                &db_cfg,
                aster_drive_metrics::NoopMetrics::arc(),
            )
            .await
            .expect("mysql schema template connection should succeed");
            let template = load_mysql_schema_template(&db).await;
            db.close()
                .await
                .expect("mysql schema template connection should close");
            let _ = MYSQL_SCHEMA_TEMPLATE_CACHE.set(template.clone());
            return template;
        }
        container.drop_shared_database(state.resource()).await;
        fixture_lock.clear();
    }

    let template_name = fixture_database_name("asterdrive_mysql", fingerprint, 64);
    container.drop_shared_database(&template_name).await;
    container.create_shared_database(&template_name).await;
    let db_cfg = aster_drive::config::DatabaseConfig {
        url: container.database_url(&template_name).into(),
        pool_size: 1,
        retry_count: 0,
    };
    let db =
        aster_drive::db::connect_with_metrics(&db_cfg, aster_drive_metrics::NoopMetrics::arc())
            .await
            .expect("mysql schema template connection should succeed");
    aster_drive_migration::Migrator::up(&db, None)
        .await
        .expect("mysql schema template migrations should succeed");
    let template = load_mysql_schema_template(&db).await;
    db.close()
        .await
        .expect("mysql schema template connection should close");
    fixture_lock.publish(&SuiteFixtureState::new(
        MYSQL_TEMPLATE_FIXTURE,
        container_identity,
        fingerprint,
        &template_name,
        TEST_FIXTURE_PRODUCER_VERSION,
    ));
    let _ = MYSQL_SCHEMA_TEMPLATE_CACHE.set(template.clone());
    template
}

async fn clone_mysql_schema_from_template(db: &sea_orm::DatabaseConnection) {
    use sea_orm::ConnectionTrait;

    let container = MYSQL_TEST_CONTAINER
        .get_or_init(start_mysql_test_container)
        .await;
    let fixture_lock = SuiteFixtureLock::acquire(test_container_suite(), MYSQL_TEMPLATE_FIXTURE);
    let fingerprint = database_fixture_fingerprint();
    let template =
        ensure_mysql_schema_template_locked(container, &fixture_lock, &fingerprint).await;
    drop(fixture_lock);

    set_foreign_key_checks(db, false)
        .await
        .expect("mysql schema clone should disable foreign key checks");

    for ddl in &template.create_table_sql {
        db.execute_unprepared(ddl)
            .await
            .expect("mysql schema clone should create table");
    }

    if !template.migration_rows.is_empty() {
        let placeholders = std::iter::repeat_n("(?, ?)", template.migration_rows.len())
            .collect::<Vec<_>>()
            .join(", ");
        let values = template
            .migration_rows
            .iter()
            .flat_map(|(version, applied_at)| [version.clone().into(), (*applied_at).into()])
            .collect::<Vec<sea_orm::Value>>();
        db.execute_raw(sea_orm::Statement::from_sql_and_values(
            sea_orm::DbBackend::MySql,
            format!("INSERT INTO seaql_migrations (version, applied_at) VALUES {placeholders}"),
            values,
        ))
        .await
        .expect("mysql schema clone should restore seaql_migrations rows");
    }

    set_foreign_key_checks(db, true)
        .await
        .expect("mysql schema clone should restore foreign key checks");
}

/// 构建一个干净的测试 PrimaryAppState（指定数据库 URL）
pub async fn setup_with_database_url(database_url: &str) -> PrimaryAppState {
    setup_with_database_url_and_pool_size(database_url, 1).await
}

async fn setup_with_database_url_and_pool_size(
    database_url: &str,
    pool_size: u32,
) -> PrimaryAppState {
    init_test_process_state();
    let pool_size = if database_url.starts_with("sqlite:") {
        1
    } else {
        pool_size
    };
    let db_cfg = aster_drive::config::DatabaseConfig {
        url: database_url.into(),
        pool_size,
        retry_count: 0,
    };
    let schema_db_cfg = aster_drive::config::DatabaseConfig {
        url: database_url.into(),
        pool_size: 1,
        retry_count: 0,
    };
    let schema_db = aster_drive::db::connect_with_metrics(
        &schema_db_cfg,
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();

    // 跑迁移
    let used_mysql_schema_template = should_use_mysql_schema_template(database_url);
    if used_mysql_schema_template {
        clone_mysql_schema_from_template(&schema_db).await;
    } else {
        aster_drive_migration::Migrator::up(&schema_db, None)
            .await
            .unwrap();
    }
    let db = if pool_size == 1 {
        schema_db
    } else {
        schema_db
            .close()
            .await
            .expect("schema setup connection should close before opening concurrent writer pool");
        aster_drive::db::connect_with_metrics(&db_cfg, aster_drive_metrics::NoopMetrics::arc())
            .await
            .expect("concurrent test writer pool should connect")
    };
    // 每个测试用独立临时目录避免并行竞争
    let test_dir = format!("/tmp/asterdrive-test-{}", uuid::Uuid::new_v4());
    let temp_dir = format!("{test_dir}/temp");
    let upload_temp_dir = format!("{test_dir}/uploads");
    let avatar_dir = format!("{test_dir}/avatar");
    std::fs::create_dir_all(&test_dir).unwrap();
    std::fs::create_dir_all(&temp_dir).unwrap();
    std::fs::create_dir_all(&upload_temp_dir).unwrap();
    std::fs::create_dir_all(&avatar_dir).unwrap();

    let config = std::sync::Arc::new(aster_drive::config::Config {
        server: aster_drive::config::ServerConfig {
            temp_dir,
            upload_temp_dir,
            ..Default::default()
        },
        auth: aster_drive::config::AuthConfig {
            jwt_secret: "test-secret-key-for-integration-tests".to_string(),
            share_cookie_secret: "test-share-cookie-secret-for-integration-tests".to_string(),
            direct_link_secret: "test-direct-link-secret-for-integration-tests".to_string(),
            mfa_secret_key: "test-mfa-secret-key-for-integration-tests".to_string(),
            storage_credential_secret_key:
                "test-storage-credential-secret-key-for-integration-tests".to_string(),
            webdav_auth_cache_secret: "test-webdav-auth-cache-secret-for-integration-tests"
                .to_string(),
            password_hash_max_concurrency: 1,
            bootstrap_insecure_cookies: true,
        },
        ..Default::default()
    });

    // 测试夹具显式创建默认本地存储策略；生产启动流程不会自动创建策略。
    use aster_drive_model::types::StoredStoragePolicyConfig;
    use aster_drive_storage::{
        ConnectorConfigEnvelope, ConnectorId, StoragePolicyBehaviorConfig,
        encode_storage_policy_config,
    };
    use chrono::Utc;
    use sea_orm::Set;
    let now = Utc::now();
    let storage_config = encode_storage_policy_config(
        ConnectorConfigEnvelope::new(
            ConnectorId::declared("asterdrive.storage.local"),
            1,
            serde_json::json!({
                "base_path": test_dir.clone(),
                "content_dedup": false
            }),
        ),
        StoragePolicyBehaviorConfig::default(),
    )
    .expect("default local policy config should serialize");
    let _ = aster_drive::db::repository::policy_repo::create(
        &db,
        aster_drive_model::entities::storage_policy::ActiveModel {
            name: Set("Test Local".to_string()),
            connector_id: Set("asterdrive.storage.local".to_string()),
            storage_config: Set(StoredStoragePolicyConfig(storage_config)),
            max_file_size: Set(0),
            allowed_types: Set(aster_drive_model::types::StoredStoragePolicyAllowedTypes::empty()),
            is_default: Set(true),
            chunk_size: Set(5_242_880),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    aster_drive::services::storage_policy::policy::ensure_policy_groups_seeded(&db)
        .await
        .unwrap();

    aster_drive::db::repository::config_repo::ensure_system_value_if_missing(
        &db,
        aster_drive::config::auth_runtime::AUTH_COOKIE_SECURE_KEY,
        "false",
    )
    .await
    .unwrap();

    aster_drive::db::repository::config_repo::ensure_defaults_with_env(&db, &|_| None)
        .await
        .unwrap();
    aster_drive::db::repository::config_repo::upsert(
        &db,
        aster_drive::config::avatar::AVATAR_DIR_KEY,
        &avatar_dir,
        0,
    )
    .await
    .unwrap();

    // 测试用内存缓存。
    let cache_config = aster_forge_cache::CacheConfig::default();
    let cache = aster_forge_cache::create_cache(&cache_config).await;

    // 初始化全局 config（WebDAV file.rs 内部调 get_config() 需要）
    // OnceLock 只设置一次，后续调用忽略
    let _ = aster_drive::config::set_config_for_test(config.clone());

    let password_hash_policy = test_password_hash_policy(8);
    let runtime_config = std::sync::Arc::new(
        aster_drive::config::RuntimeConfig::with_password_hash_policy(1, password_hash_policy)
            .unwrap(),
    );
    runtime_config.reload(&db).await.unwrap();

    let driver_registry = std::sync::Arc::new(
        aster_drive::storage::DriverRegistry::noop().expect("built-in storage connector registry"),
    );
    let policy_snapshot = std::sync::Arc::new(aster_drive::storage::PolicySnapshot::new());
    driver_registry
        .reload_policy_snapshot(&policy_snapshot, &db)
        .await
        .unwrap();
    let mail_sender = aster_forge_mail::memory_sender();

    let storage_change_bus = aster_drive::services::events::storage_change::StorageChangeBus::new(
        aster_drive::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
    );
    let share_download_rollback =
        aster_drive::services::share::spawn_detached_share_download_rollback_queue(
            db.clone(),
            aster_drive::config::operations::share_download_rollback_queue_capacity(
                &runtime_config,
            ),
        );

    let remote_protocol = aster_drive::runtime::PrimaryAppState::new_remote_protocol();
    remote_protocol.set_persistence_db(db.clone());
    driver_registry.set_remote_protocol(remote_protocol.clone());

    PrimaryAppState {
        db_handles: aster_drive::db::connect_reader_for_writer_with_metrics(
            &db_cfg,
            db.clone(),
            aster_drive_metrics::NoopMetrics::arc(),
        )
        .await
        .unwrap(),
        driver_registry,
        runtime_config,
        policy_snapshot,
        config,
        cache,
        config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
        metrics: aster_drive_metrics::NoopMetrics::arc(),
        mail_sender,
        storage_change_bus,
        share_download_rollback,
        background_task_dispatch_wakeup:
            aster_drive::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
        remote_protocol,
    }
}

pub async fn flush_mail_outbox(state: &PrimaryAppState) {
    flush_mail_outbox_with(state.writer_db(), &state.runtime_config, &state.mail_sender).await;
}

pub async fn flush_mail_outbox_with(
    db: &sea_orm::DatabaseConnection,
    runtime_config: &std::sync::Arc<aster_drive::config::RuntimeConfig>,
    mail_sender: &std::sync::Arc<dyn aster_forge_mail::MailSender>,
) {
    const MAX_ATTEMPTS: usize = 8;

    for attempt in 0..MAX_ATTEMPTS {
        aster_drive::services::mail::outbox::drain_with(db, runtime_config, mail_sender)
            .await
            .expect("mail outbox drain should succeed");

        let active = aster_forge_db::MailOutboxDbStore::new(db.clone())
            .count_active()
            .await
            .expect("mail outbox active count should succeed");
        if active == 0 {
            return;
        }

        if attempt + 1 < MAX_ATTEMPTS {
            // drain 可能刚触发了同进程内的异步发送/落库链路，先让出一次调度，不再硬睡固定时长。
            tokio::task::yield_now().await;
        }
    }

    panic!("mail outbox should drain in tests");
}

/// 从 Set-Cookie header 提取指定 cookie 的值
pub fn extract_cookie<B>(resp: &actix_web::dev::ServiceResponse<B>, name: &str) -> Option<String> {
    let value = resp
        .response()
        .cookies()
        .find(|c| c.name() == name)
        .map(|c| c.value().to_string())?;

    if matches!(name, "aster_access" | "aster_refresh")
        && let Some(csrf_token) = resp
            .response()
            .cookies()
            .find(|cookie| cookie.name() == "aster_csrf")
            .map(|cookie| cookie.value().to_string())
    {
        remember_csrf_token(&value, &csrf_token);
    }

    Some(value)
}

fn extract_token_from_content(content: &str, marker: &str) -> Option<String> {
    let (_, suffix) = content.split_once(marker)?;
    let encoded: String = suffix
        .chars()
        .take_while(|ch| !matches!(ch, '"' | '\'' | '<' | '>' | '&' | ' ' | '\r' | '\n'))
        .collect();
    if encoded.is_empty() {
        return None;
    }

    urlencoding::decode(&encoded)
        .ok()
        .map(|value| value.into_owned())
}

pub fn extract_token_from_mail_message(
    message: &aster_forge_mail::MailMessage,
    marker: &str,
) -> Option<String> {
    extract_token_from_content(&message.text_body, marker)
        .or_else(|| extract_token_from_content(&message.html_body, marker))
}

pub fn extract_verification_token_from_mail_sender(
    sender: &Arc<dyn aster_forge_mail::MailSender>,
) -> Option<String> {
    let memory_sender = aster_forge_mail::memory_sender_ref(sender)
        .expect("memory mail sender should be available in tests");
    let message = memory_sender.last_message()?;
    extract_token_from_mail_message(&message, "/api/v1/auth/contact-verification/confirm?token=")
}

pub async fn extract_verification_token_from_mail_sender_or_outbox(
    db: &sea_orm::DatabaseConnection,
    sender: &Arc<dyn aster_forge_mail::MailSender>,
) -> Option<String> {
    if let Some(token) = extract_verification_token_from_mail_sender(sender) {
        return Some(token);
    }

    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};

    let row = aster_forge_db::mail_outbox::Entity::find()
        .filter(aster_forge_db::mail_outbox::Column::TemplateCode.is_in([
            aster_forge_mail::MailTemplateCode::RegisterActivation,
            aster_forge_mail::MailTemplateCode::ContactChangeConfirmation,
        ]))
        .order_by_desc(aster_forge_db::mail_outbox::Column::Id)
        .one(db)
        .await
        .expect("mail outbox lookup should succeed")?;

    serde_json::from_str::<serde_json::Value>(row.payload_json.as_ref())
        .expect("mail outbox payload should be valid json")
        .get("token")
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

pub fn system_config_model(key: &str, value: &str) -> aster_forge_db::system_config::Model {
    aster_forge_db::system_config::Model {
        id: 0,
        key: key.to_string(),
        value: value.to_string(),
        value_type: aster_forge_config::ConfigValueType::String,
        requires_restart: false,
        is_sensitive: false,
        source: aster_forge_config::ConfigSource::System,
        visibility: aster_forge_config::ConfigVisibility::Private,
        namespace: String::new(),
        category: aster_drive::config::definitions::CONFIG_CATEGORY_SITE.to_string(),
        description: "test".to_string(),
        updated_at: chrono::Utc::now(),
        updated_by: None,
    }
}

/// 创建标准测试 App
#[macro_export]
macro_rules! create_test_app {
    ($state:expr) => {{
        use actix_web::{App, test, web};
        let state = $state;
        let db = state.writer_db().clone();
        test::init_service(
            App::new()
                .wrap(aster_forge_actix_middleware::security_headers::default_headers())
                .app_data(web::PayloadConfig::new(
                    aster_drive::api::extractors::DEFAULT_PAYLOAD_LIMIT,
                ))
                .app_data(aster_drive::api::extractors::json_config(
                    aster_drive::api::extractors::DEFAULT_JSON_LIMIT,
                ))
                .app_data(web::Data::new(state))
                .configure(move |cfg| aster_drive::api::configure_primary(cfg, &db)),
        )
        .await
    }};
}

/// 兼容 `call_service` / `try_call_service` 两种返回路径的状态断言
#[macro_export]
macro_rules! assert_service_status {
    ($app:expr, $req:expr, $status:expr) => {{
        use actix_web::test;

        let result = test::try_call_service(&$app, $req).await;
        match result {
            Ok(resp) => assert_eq!(resp.status(), $status),
            Err(err) => {
                let resp = err.error_response();
                assert_eq!(resp.status(), $status);
            }
        }
    }};
    ($app:expr, $req:expr, $status:expr, $msg:expr) => {{
        use actix_web::test;

        let result = test::try_call_service(&$app, $req).await;
        match result {
            Ok(resp) => assert_eq!(resp.status(), $status, $msg),
            Err(err) => {
                let resp = err.error_response();
                assert_eq!(resp.status(), $status, $msg);
            }
        }
    }};
}

/// 注册 + 登录，返回 (access_cookie, refresh_cookie)
#[macro_export]
macro_rules! register_and_login {
    ($app:expr) => {{
        use actix_web::test;

        common::setup_test_account_via_api(
            &$app,
            "testuser",
            "test@example.com",
            "password123",
        )
        .await;

        // 登录
        let req = test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .set_json(serde_json::json!({
                "identifier": "testuser",
                "password": "password123"
            }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 200, "login should return 200");
        let access =
            common::extract_cookie(&resp, "aster_access").expect("access cookie missing");
        let refresh =
            common::extract_cookie(&resp, "aster_refresh").expect("refresh cookie missing");
        (access, refresh)
    }};
}

/// 管理员创建普通用户，返回 user_id
#[macro_export]
macro_rules! admin_create_user {
    ($app:expr, $admin_token:expr, $username:expr, $email:expr, $password:expr) => {{
        use actix_web::test;
        use serde_json::Value;

        let req = test::TestRequest::post()
            .uri("/api/v1/admin/users")
            .insert_header(("Cookie", common::access_cookie_header(&$admin_token)))
            .insert_header(common::csrf_header_for(&$admin_token))
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .set_json(serde_json::json!({
                "username": $username,
                "email": $email,
                "password": $password
            }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201, "admin create user should return 201");
        let body: Value = test::read_body_json(resp).await;
        body["data"]["user"]["id"].as_i64().unwrap()
    }};
}

/// 使用用户名/邮箱登录，返回 (access_cookie, refresh_cookie)
#[macro_export]
macro_rules! login_user {
    ($app:expr, $identifier:expr, $password:expr) => {{
        use actix_web::test;

        let req = test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .set_json(serde_json::json!({
                "identifier": $identifier,
                "password": $password
            }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 200, "login should return 200");
        let access =
            common::extract_cookie(&resp, "aster_access").expect("access cookie missing");
        let refresh =
            common::extract_cookie(&resp, "aster_refresh").expect("refresh cookie missing");
        (access, refresh)
    }};
}

#[macro_export]
macro_rules! confirm_latest_contact_verification {
    ($app:expr, $db:expr, $mail_sender:expr) => {{
        use actix_web::test;

        if let Some(token) =
            common::extract_verification_token_from_mail_sender_or_outbox(&$db, &$mail_sender).await
        {
            let req = test::TestRequest::get()
                .uri(&format!(
                    "/api/v1/auth/contact-verification/confirm?token={}",
                    urlencoding::encode(&token)
                ))
                .to_request();
            let resp = test::call_service(&$app, req).await;
            assert_eq!(resp.status(), 302, "contact verification should return 302");
            let location = resp
                .headers()
                .get("Location")
                .and_then(|value| value.to_str().ok())
                .expect("contact verification redirect location missing")
                .to_string();
            Some(location)
        } else {
            None
        }
    }};
}

/// 上传测试文件，返回 file_id
#[macro_export]
macro_rules! upload_test_file {
    ($app:expr, $token:expr) => {{
        use actix_web::test;
        use serde_json::Value;

        let boundary = "----TestBoundary123";
        let payload = format!(
            "------TestBoundary123\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\n\
             Content-Type: text/plain\r\n\r\n\
             test content\r\n\
             ------TestBoundary123--\r\n"
        );
        let req = test::TestRequest::post()
            .uri("/api/v1/files/upload")
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            ))
            .set_payload(payload)
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201, "upload should return 201");
        let body: Value = test::read_body_json(resp).await;
        body["data"]["id"].as_i64().unwrap()
    }};
}

/// 上传指定名称测试文件，返回 file_id
#[macro_export]
macro_rules! upload_test_file_named {
    ($app:expr, $token:expr, $name:expr) => {{
        use actix_web::test;
        use serde_json::Value;

        let boundary = "----TestBoundary123";
        let payload = format!(
            "------TestBoundary123\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\n\
             Content-Type: text/plain\r\n\r\n\
             test content\r\n\
             ------TestBoundary123--\r\n",
            name = $name
        );
        let req = test::TestRequest::post()
            .uri("/api/v1/files/upload")
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            ))
            .set_payload(payload)
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201, "upload should return 201");
        let body: Value = test::read_body_json(resp).await;
        body["data"]["id"].as_i64().unwrap()
    }};
}

/// 上传测试文件到指定文件夹，返回 file_id
#[macro_export]
macro_rules! upload_test_file_to_folder {
    ($app:expr, $token:expr, $folder_id:expr) => {{
        use actix_web::test;
        use serde_json::Value;

        let boundary = "----TestBoundary123";
        let payload = format!(
            "------TestBoundary123\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"test-in-folder.txt\"\r\n\
             Content-Type: text/plain\r\n\r\n\
             test content in folder\r\n\
             ------TestBoundary123--\r\n"
        );
        let req = test::TestRequest::post()
            .uri(&format!("/api/v1/files/upload?folder_id={}", $folder_id))
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            ))
            .set_payload(payload)
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201, "upload to folder should return 201");
        let body: Value = test::read_body_json(resp).await;
        body["data"]["id"].as_i64().unwrap()
    }};
}

/// 构建带 WebDAV 路由的测试 App
#[macro_export]
macro_rules! setup_with_webdav {
    () => {{
        let (app, _state) = setup_with_webdav!(with_state);
        app
    }};
    (with_state) => {{
        use actix_web::{App, test, web};
        let state = common::setup().await;
        let db1 = state.writer_db().clone();
        let db2 = state.writer_db().clone();
        let webdav_config = aster_drive::config::WebDavConfig::default();
        let app = test::init_service(
            App::new()
                .wrap(aster_forge_actix_middleware::security_headers::default_headers())
                .app_data(web::PayloadConfig::new(
                    aster_drive::api::extractors::DEFAULT_PAYLOAD_LIMIT,
                ))
                .app_data(web::JsonConfig::default().limit(1024 * 1024))
                .app_data(web::Data::new(state.clone()))
                .configure(move |cfg| {
                    aster_drive::webdav::configure(cfg, &webdav_config, &db2);
                    aster_drive::api::configure_primary(cfg, &db1);
                }),
        )
        .await;
        (app, state)
    }};
}

#[macro_export]
macro_rules! setup_with_webdav_and_mail {
    () => {{
        use actix_web::{App, test, web};

        let state = common::setup().await;
        let db = state.writer_db().clone();
        let mail_sender = state.mail_sender.clone();
        let db1 = state.writer_db().clone();
        let db2 = state.writer_db().clone();
        let webdav_config = aster_drive::config::WebDavConfig::default();
        let app = test::init_service(
            App::new()
                .wrap(aster_forge_actix_middleware::security_headers::default_headers())
                .app_data(web::PayloadConfig::new(
                    aster_drive::api::extractors::DEFAULT_PAYLOAD_LIMIT,
                ))
                .app_data(web::JsonConfig::default().limit(1024 * 1024))
                .app_data(web::Data::new(state))
                .configure(move |cfg| {
                    aster_drive::webdav::configure(cfg, &webdav_config, &db2);
                    aster_drive::api::configure_primary(cfg, &db1);
                }),
        )
        .await;
        (app, db, mail_sender)
    }};
}
