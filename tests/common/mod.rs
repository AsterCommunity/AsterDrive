//! 集成测试公共 helper。

use aster_drive::runtime::{PrimaryAppState, SharedRuntimeState};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::HashMap,
    fs::{File, OpenOptions},
    hash::{Hash, Hasher},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::Command,
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
const SHARED_TEST_CONTAINER_STATE_DIR: &str = "/tmp/asterdrive-testcontainers";
// Keep the year within MySQL TIMESTAMP's supported range.
#[allow(dead_code)]
pub const TEST_FUTURE_SHARE_EXPIRY_RFC3339: &str = "2099-12-31T23:59:59Z";

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

#[allow(dead_code)]
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

struct SharedTestDatabaseContainer {
    _container: testcontainers::ContainerAsync<testcontainers::GenericImage>,
    _lease: SharedTestContainerLease,
    admin_database_url: String,
    database_url: String,
}

struct MySqlSchemaTemplate {
    database_name: String,
    create_table_sql: Vec<String>,
}

struct PostgresDatabaseTemplate {
    database_name: String,
}

static POSTGRES_TEST_CONTAINER: tokio::sync::OnceCell<SharedTestDatabaseContainer> =
    tokio::sync::OnceCell::const_new();
static MYSQL_TEST_CONTAINER: tokio::sync::OnceCell<SharedTestDatabaseContainer> =
    tokio::sync::OnceCell::const_new();
static POSTGRES_DATABASE_TEMPLATE: tokio::sync::OnceCell<PostgresDatabaseTemplate> =
    tokio::sync::OnceCell::const_new();
static MYSQL_SCHEMA_TEMPLATE: tokio::sync::OnceCell<MySqlSchemaTemplate> =
    tokio::sync::OnceCell::const_new();

#[derive(Default, Deserialize, Serialize)]
struct SharedTestContainerState {
    #[serde(default)]
    databases_by_pid: HashMap<u32, Vec<String>>,
    pids: Vec<u32>,
}

struct SharedTestContainerLease {
    backend: TestDatabaseBackend,
}

impl Drop for SharedTestContainerLease {
    fn drop(&mut self) {
        release_shared_test_container(self.backend);
    }
}

impl SharedTestContainerLease {
    fn new(backend: TestDatabaseBackend) -> Self {
        Self { backend }
    }
}

impl SharedTestContainerState {
    fn normalize(&mut self) {
        self.pids.sort_unstable();
        self.pids.dedup();

        self.databases_by_pid
            .retain(|pid, _| self.pids.binary_search(pid).is_ok());
        for pid in &self.pids {
            let databases = self.databases_by_pid.entry(*pid).or_default();
            databases.sort_unstable();
            databases.dedup();
        }
    }

    fn register_pid(&mut self, pid: u32) {
        if !self.pids.contains(&pid) {
            self.pids.push(pid);
        }
        self.normalize();
    }

    fn remember_database(&mut self, pid: u32, database_name: &str) {
        self.register_pid(pid);
        let databases = self.databases_by_pid.entry(pid).or_default();
        if !databases.iter().any(|name| name == database_name) {
            databases.push(database_name.to_string());
        }
        databases.sort_unstable();
    }
}

impl TestDatabaseBackend {
    fn as_str(self) -> &'static str {
        match self {
            Self::Sqlite => "sqlite",
            Self::Postgres => "postgres",
            Self::MySql => "mysql",
        }
    }

    fn container_port(self) -> u16 {
        match self {
            Self::Sqlite => 0,
            Self::Postgres => 5432,
            Self::MySql => 3306,
        }
    }

    fn shared_container_name(self) -> String {
        format!("asterdrive-test-{}-{}", test_workspace_id(), self.as_str())
    }

    fn shared_state_path(self) -> PathBuf {
        shared_test_container_state_dir().join(format!(
            "{}-{}.json",
            test_workspace_id(),
            self.as_str()
        ))
    }

    fn shared_lock_path(self) -> PathBuf {
        shared_test_container_state_dir().join(format!(
            "{}-{}.lock",
            test_workspace_id(),
            self.as_str()
        ))
    }

    fn database_url(self, port: u16) -> String {
        match self {
            Self::Sqlite => "sqlite::memory:".to_string(),
            Self::Postgres => format!("postgres://postgres:postgres@127.0.0.1:{port}/asterdrive"),
            Self::MySql => format!("mysql://aster:asterpass@127.0.0.1:{port}/asterdrive"),
        }
    }

    fn admin_database_url(self, port: u16) -> String {
        match self {
            Self::Sqlite => "sqlite::memory:".to_string(),
            Self::Postgres => self.database_url(port),
            Self::MySql => format!("mysql://root:rootpass@127.0.0.1:{port}/asterdrive"),
        }
    }
}

fn test_workspace_id() -> &'static str {
    static WORKSPACE_ID: OnceLock<String> = OnceLock::new();
    WORKSPACE_ID.get_or_init(|| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        env!("CARGO_MANIFEST_DIR").hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    })
}

fn shared_test_container_state_dir() -> &'static Path {
    static STATE_DIR: OnceLock<PathBuf> = OnceLock::new();
    STATE_DIR
        .get_or_init(|| {
            let path = PathBuf::from(SHARED_TEST_CONTAINER_STATE_DIR);
            std::fs::create_dir_all(&path).expect("shared test container state dir should exist");
            path
        })
        .as_path()
}

fn lock_shared_test_container_state(backend: TestDatabaseBackend) -> File {
    let lock_path = backend.shared_lock_path();
    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(lock_path)
        .expect("shared test container lock file should open");
    file.lock_exclusive()
        .expect("shared test container lock should be acquired");
    file
}

fn load_shared_test_container_state(
    file: &mut File,
    backend: TestDatabaseBackend,
) -> SharedTestContainerState {
    let state_path = backend.shared_state_path();
    if !state_path.exists() {
        return SharedTestContainerState::default();
    }

    file.seek(SeekFrom::Start(0))
        .expect("state lock file should seek");
    let mut raw = String::new();
    File::open(state_path)
        .and_then(|mut state_file| state_file.read_to_string(&mut raw))
        .expect("shared test container state should be readable");

    let mut state = if raw.trim().is_empty() {
        SharedTestContainerState::default()
    } else {
        serde_json::from_str(&raw).expect("shared test container state should be valid json")
    };
    state.normalize();
    state
}

fn save_shared_test_container_state(
    file: &mut File,
    backend: TestDatabaseBackend,
    state: &SharedTestContainerState,
) {
    let state_path = backend.shared_state_path();
    file.seek(SeekFrom::Start(0))
        .expect("state lock file should seek");

    let json = serde_json::to_vec(state).expect("shared test container state should serialize");
    let mut state_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(state_path)
        .expect("shared test container state file should open");
    state_file
        .write_all(&json)
        .expect("shared test container state should write");
    state_file
        .write_all(b"\n")
        .expect("shared test container state should end with newline");
    state_file
        .flush()
        .expect("shared test container state should flush");
    let _ = file.flush();
}

fn process_is_running(pid: u32) -> bool {
    if pid == std::process::id() {
        return true;
    }

    Command::new("/bin/kill")
        .arg("-0")
        .arg(pid.to_string())
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn prune_shared_test_container_state(state: &mut SharedTestContainerState) -> Vec<String> {
    let stale_pids = state
        .pids
        .iter()
        .copied()
        .filter(|pid| !process_is_running(*pid))
        .collect::<Vec<_>>();
    let stale_databases = stale_pids
        .iter()
        .flat_map(|pid| {
            state
                .databases_by_pid
                .remove(pid)
                .unwrap_or_default()
                .into_iter()
        })
        .collect::<Vec<_>>();

    state.pids.retain(|pid| !stale_pids.contains(pid));
    state.normalize();

    stale_databases
}

fn remember_shared_test_database(backend: TestDatabaseBackend, database_name: &str) {
    let mut lock_file = lock_shared_test_container_state(backend);
    let mut state = load_shared_test_container_state(&mut lock_file, backend);
    state.remember_database(std::process::id(), database_name);
    save_shared_test_container_state(&mut lock_file, backend, &state);
}

fn test_backend_from_database_backend(backend: sea_orm::DbBackend) -> Option<TestDatabaseBackend> {
    match backend {
        sea_orm::DbBackend::Postgres => Some(TestDatabaseBackend::Postgres),
        sea_orm::DbBackend::MySql => Some(TestDatabaseBackend::MySql),
        _ => None,
    }
}

fn release_shared_test_container(backend: TestDatabaseBackend) {
    let mut lock_file = lock_shared_test_container_state(backend);
    let mut state = load_shared_test_container_state(&mut lock_file, backend);
    let _ = prune_shared_test_container_state(&mut state);
    save_shared_test_container_state(&mut lock_file, backend, &state);
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
        url: admin_database_url.to_string(),
        pool_size: 1,
        retry_count: 0,
    };
    let admin_db =
        aster_drive::db::connect_with_metrics(&admin_cfg, aster_drive::metrics::NoopMetrics::arc())
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
}

async fn ensure_mysql_test_user_access(admin_database_url: &str, username: &str) {
    use sea_orm::ConnectionTrait;

    let admin_cfg = aster_drive::config::DatabaseConfig {
        url: admin_database_url.to_string(),
        pool_size: 1,
        retry_count: 0,
    };
    let admin_db =
        aster_drive::db::connect_with_metrics(&admin_cfg, aster_drive::metrics::NoopMetrics::arc())
            .await
            .expect("mysql test admin connection should succeed");
    let grant_sql = format!(
        "GRANT ALL PRIVILEGES ON *.* TO {}@'%'",
        quote_mysql_string(username)
    );
    admin_db
        .execute_unprepared(&grant_sql)
        .await
        .expect("mysql test user grant should succeed");
}

#[allow(dead_code)]
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

#[allow(dead_code)]
pub fn seed_csrf_token(session_token: &str) -> String {
    let csrf_token = aster_forge_actix_middleware::csrf::build_csrf_token();
    remember_csrf_token(session_token, &csrf_token);
    csrf_token
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
pub fn access_cookie_header(access_token: impl AsRef<str>) -> String {
    let access_token = access_token.as_ref();
    format!(
        "aster_access={access_token}; aster_csrf={}",
        csrf_token_for(access_token)
    )
}

#[allow(dead_code)]
pub fn refresh_cookie_header(refresh_token: impl AsRef<str>) -> String {
    let refresh_token = refresh_token.as_ref();
    format!(
        "aster_refresh={refresh_token}; aster_csrf={}",
        csrf_token_for(refresh_token)
    )
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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

async fn wait_for_database(database_url: &str) {
    let mut last_err: Option<String> = None;
    let ready = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            let cfg = aster_drive::config::DatabaseConfig {
                url: database_url.to_string(),
                pool_size: 1,
                retry_count: 0,
            };
            match aster_drive::db::connect_with_metrics(
                &cfg,
                aster_drive::metrics::NoopMetrics::arc(),
            )
            .await
            {
                Ok(_) => break,
                Err(err) => {
                    last_err = Some(err.to_string());
                    // 这里只是数据库 readiness probe 的退避间隔；外层 timeout 才是最终边界。
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
        }
    })
    .await;

    if ready.is_err() {
        panic!(
            "timed out waiting for database {database_url}: {}",
            last_err.unwrap_or_else(|| "unknown error".to_string())
        );
    }
}

async fn start_postgres_test_container() -> SharedTestDatabaseContainer {
    use testcontainers::{GenericImage, ImageExt, ReuseDirective, runners::AsyncRunner};

    let backend = TestDatabaseBackend::Postgres;
    let mut lock_file = lock_shared_test_container_state(backend);
    let mut state = load_shared_test_container_state(&mut lock_file, backend);
    let stale_databases = prune_shared_test_container_state(&mut state);
    let current_pid = std::process::id();
    state.register_pid(current_pid);

    let container = GenericImage::new("postgres", "16")
        .with_exposed_port(testcontainers::core::IntoContainerPort::tcp(
            backend.container_port(),
        ))
        .with_container_name(backend.shared_container_name())
        .with_reuse(ReuseDirective::Always)
        .with_env_var("POSTGRES_USER", "postgres")
        .with_env_var("POSTGRES_PASSWORD", "postgres")
        .with_env_var("POSTGRES_DB", "asterdrive")
        .start()
        .await
        .expect("failed to start postgres test container");
    let port = container
        .get_host_port_ipv4(testcontainers::core::IntoContainerPort::tcp(
            backend.container_port(),
        ))
        .await
        .expect("postgres test port should be exposed");
    let database_url = backend.database_url(port);
    let admin_database_url = backend.admin_database_url(port);

    wait_for_database(&database_url).await;
    drop_stale_test_databases(
        sea_orm::DbBackend::Postgres,
        &admin_database_url,
        &stale_databases,
    )
    .await;
    save_shared_test_container_state(&mut lock_file, backend, &state);

    SharedTestDatabaseContainer {
        _container: container,
        _lease: SharedTestContainerLease::new(backend),
        admin_database_url,
        database_url,
    }
}

async fn start_mysql_test_container() -> SharedTestDatabaseContainer {
    use testcontainers::{GenericImage, ImageExt, ReuseDirective, runners::AsyncRunner};

    let backend = TestDatabaseBackend::MySql;
    let mut lock_file = lock_shared_test_container_state(backend);
    let mut state = load_shared_test_container_state(&mut lock_file, backend);
    let stale_databases = prune_shared_test_container_state(&mut state);
    let current_pid = std::process::id();
    state.register_pid(current_pid);

    let container = GenericImage::new("mysql", "8.4")
        .with_exposed_port(testcontainers::core::IntoContainerPort::tcp(
            backend.container_port(),
        ))
        .with_container_name(backend.shared_container_name())
        .with_reuse(ReuseDirective::Always)
        .with_env_var("MYSQL_DATABASE", "asterdrive")
        .with_env_var("MYSQL_USER", "aster")
        .with_env_var("MYSQL_PASSWORD", "asterpass")
        .with_env_var("MYSQL_ROOT_PASSWORD", "rootpass")
        .start()
        .await
        .expect("failed to start mysql test container");
    let port = container
        .get_host_port_ipv4(testcontainers::core::IntoContainerPort::tcp(
            backend.container_port(),
        ))
        .await
        .expect("mysql test port should be exposed");
    let database_url = backend.database_url(port);
    let admin_database_url = backend.admin_database_url(port);

    wait_for_database(&database_url).await;
    ensure_mysql_test_user_access(&admin_database_url, "aster").await;
    drop_stale_test_databases(
        sea_orm::DbBackend::MySql,
        &admin_database_url,
        &stale_databases,
    )
    .await;
    save_shared_test_container_state(&mut lock_file, backend, &state);

    SharedTestDatabaseContainer {
        _container: container,
        _lease: SharedTestContainerLease::new(backend),
        admin_database_url,
        database_url,
    }
}

async fn shared_test_database_urls(backend: TestDatabaseBackend) -> (String, String) {
    match backend {
        TestDatabaseBackend::Sqlite => {
            ("sqlite::memory:".to_string(), "sqlite::memory:".to_string())
        }
        TestDatabaseBackend::Postgres => {
            let container = POSTGRES_TEST_CONTAINER
                .get_or_init(start_postgres_test_container)
                .await;
            (
                container.admin_database_url.clone(),
                container.database_url.clone(),
            )
        }
        TestDatabaseBackend::MySql => {
            let container = MYSQL_TEST_CONTAINER
                .get_or_init(start_mysql_test_container)
                .await;
            (
                container.admin_database_url.clone(),
                container.database_url.clone(),
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

    use sea_orm::ConnectionTrait;

    let admin_cfg = aster_drive::config::DatabaseConfig {
        url: admin_database_url.to_string(),
        pool_size: 1,
        retry_count: 0,
    };
    let admin_db =
        aster_drive::db::connect_with_metrics(&admin_cfg, aster_drive::metrics::NoopMetrics::arc())
            .await
            .unwrap();
    let backend = admin_db.get_database_backend();
    let parsed_url = reqwest::Url::parse(database_url).unwrap();
    let base_name = database_name_from_url(&parsed_url).unwrap_or_else(|| "asterdrive".to_string());

    let isolated_name = match backend {
        sea_orm::DbBackend::Postgres => isolated_database_name(&base_name, 63),
        sea_orm::DbBackend::MySql => isolated_database_name(&base_name, 64),
        _ => return database_url.to_string(),
    };
    let test_backend = test_backend_from_database_backend(backend)
        .expect("isolated database provisioning only supports postgres/mysql");
    remember_shared_test_database(test_backend, &isolated_name);

    let create_sql = match (backend, template_database_name) {
        (sea_orm::DbBackend::Postgres, Some(template_database_name)) => format!(
            "CREATE DATABASE {} TEMPLATE {}",
            quote_database_identifier(backend, &isolated_name),
            quote_database_identifier(backend, template_database_name)
        ),
        _ => format!(
            "CREATE DATABASE {}",
            quote_database_identifier(backend, &isolated_name)
        ),
    };
    admin_db.execute_unprepared(&create_sql).await.unwrap();

    replace_database_name(parsed_url, &isolated_name)
}

async fn provision_isolated_test_database_url(
    admin_database_url: &str,
    database_url: &str,
) -> String {
    provision_isolated_test_database_url_with_template(admin_database_url, database_url, None).await
}

async fn build_postgres_database_template() -> PostgresDatabaseTemplate {
    let (admin_database_url, database_url) =
        shared_test_database_urls(TestDatabaseBackend::Postgres).await;
    let template_database_url =
        provision_isolated_test_database_url(&admin_database_url, &database_url).await;

    let db_cfg = aster_drive::config::DatabaseConfig {
        url: template_database_url.clone(),
        pool_size: 1,
        retry_count: 0,
    };
    let db =
        aster_drive::db::connect_with_metrics(&db_cfg, aster_drive::metrics::NoopMetrics::arc())
            .await
            .expect("postgres template database connection should succeed");

    use migration::Migrator;
    Migrator::up(&db, None)
        .await
        .expect("postgres template database migrations should succeed");
    db.close()
        .await
        .expect("postgres template database should close cleanly");

    let template_database_name = reqwest::Url::parse(&template_database_url)
        .ok()
        .and_then(|url| database_name_from_url(&url))
        .expect("postgres template database name should exist");

    PostgresDatabaseTemplate {
        database_name: template_database_name,
    }
}

async fn resolve_test_database_url_for(backend: TestDatabaseBackend) -> String {
    let (admin_database_url, database_url) = shared_test_database_urls(backend).await;
    match backend {
        TestDatabaseBackend::Postgres => {
            let template = POSTGRES_DATABASE_TEMPLATE
                .get_or_init(build_postgres_database_template)
                .await;
            provision_isolated_test_database_url_with_template(
                &admin_database_url,
                &database_url,
                Some(&template.database_name),
            )
            .await
        }
        _ => provision_isolated_test_database_url(&admin_database_url, &database_url).await,
    }
}

async fn resolve_test_database_url() -> String {
    resolve_test_database_url_for(configured_test_database_backend()).await
}

#[allow(dead_code)]
pub async fn postgres_test_database_url() -> String {
    resolve_test_database_url_for(TestDatabaseBackend::Postgres).await
}

#[allow(dead_code)]
pub async fn mysql_test_database_url() -> String {
    resolve_test_database_url_for(TestDatabaseBackend::MySql).await
}

/// 构建一个干净的测试 PrimaryAppState。
///
/// 默认使用内存 SQLite。若设置 `ASTER_TEST_DATABASE_BACKEND=postgres|mysql`，
/// 会自动启动一个共享 testcontainers 容器，并为当前测试实例分配独立数据库。
#[allow(dead_code)]
pub async fn setup() -> PrimaryAppState {
    init_test_process_state();
    let database_url = resolve_test_database_url().await;
    setup_with_database_url(&database_url).await
}

/// 构建使用内存缓存的测试 PrimaryAppState。
#[allow(dead_code)]
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
        metrics: aster_drive::metrics::NoopMetrics::arc(),
        mail_sender: base.mail_sender,
        storage_change_tx: base.storage_change_tx,
        share_download_rollback: base.share_download_rollback,
        background_task_dispatch_wakeup: base.background_task_dispatch_wakeup,
        remote_protocol: base.remote_protocol,
        upload_runtime: base.upload_runtime,
    }
}

/// Creates a test account while respecting the production initialization lifecycle.
///
/// The first fixture account goes through `setup`; later fixture accounts use ordinary
/// registration. Tests that exercise setup/register behavior directly should call those service
/// functions themselves instead of this convenience helper.
#[allow(dead_code)]
pub async fn create_test_account(
    state: &PrimaryAppState,
    username: &str,
    email: &str,
    password: &str,
) -> aster_drive::errors::Result<aster_drive::services::auth::local::AuthUserInfo> {
    if aster_drive::services::auth::local::check_auth_state(state).await? {
        aster_drive::services::auth::local::register(state, username, email, password).await
    } else {
        aster_drive::services::auth::local::setup(state, username, email, password).await
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
#[allow(dead_code)]
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

/// Creates an account through the production setup/register lifecycle and confirms registration
/// email when that policy is enabled.
#[allow(dead_code)]
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
    let initialized = aster_drive::db::repository::system_initialization_repo::is_initialized(db)
        .await
        .expect("test initialization state should load");
    if !initialized {
        return setup_test_account_via_api(app, username, email, password).await;
    }

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
        && configured_test_database_backend() == TestDatabaseBackend::MySql
}

async fn load_mysql_schema_template(
    db: &sea_orm::DatabaseConnection,
    database_name: String,
) -> MySqlSchemaTemplate {
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

    MySqlSchemaTemplate {
        database_name,
        create_table_sql,
    }
}

async fn build_mysql_schema_template() -> MySqlSchemaTemplate {
    let (admin_database_url, database_url) =
        shared_test_database_urls(TestDatabaseBackend::MySql).await;
    let template_database_url =
        provision_isolated_test_database_url(&admin_database_url, &database_url).await;

    let db_cfg = aster_drive::config::DatabaseConfig {
        url: template_database_url.clone(),
        pool_size: 1,
        retry_count: 0,
    };
    let db =
        aster_drive::db::connect_with_metrics(&db_cfg, aster_drive::metrics::NoopMetrics::arc())
            .await
            .expect("mysql schema template connection should succeed");

    use migration::Migrator;
    Migrator::up(&db, None)
        .await
        .expect("mysql schema template migrations should succeed");

    let template_database_name = reqwest::Url::parse(&template_database_url)
        .ok()
        .and_then(|url| database_name_from_url(&url))
        .expect("mysql schema template database name should exist");

    load_mysql_schema_template(&db, template_database_name).await
}

async fn clone_mysql_schema_from_template(db: &sea_orm::DatabaseConnection) {
    use sea_orm::ConnectionTrait;

    let template = MYSQL_SCHEMA_TEMPLATE
        .get_or_init(build_mysql_schema_template)
        .await;

    set_foreign_key_checks(db, false)
        .await
        .expect("mysql schema clone should disable foreign key checks");

    for ddl in &template.create_table_sql {
        db.execute_unprepared(ddl)
            .await
            .expect("mysql schema clone should create table");
    }

    db.execute_unprepared(&format!(
        "INSERT INTO seaql_migrations SELECT * FROM {}.seaql_migrations",
        quote_database_identifier(sea_orm::DbBackend::MySql, &template.database_name)
    ))
    .await
    .expect("mysql schema clone should copy seaql_migrations rows");

    set_foreign_key_checks(db, true)
        .await
        .expect("mysql schema clone should restore foreign key checks");
}

/// 构建一个干净的测试 PrimaryAppState（指定数据库 URL）
pub async fn setup_with_database_url(database_url: &str) -> PrimaryAppState {
    init_test_process_state();
    let db_cfg = aster_drive::config::DatabaseConfig {
        url: database_url.to_string(),
        pool_size: 1,
        retry_count: 0,
    };
    let db =
        aster_drive::db::connect_with_metrics(&db_cfg, aster_drive::metrics::NoopMetrics::arc())
            .await
            .unwrap();

    // 跑迁移
    use migration::Migrator;
    if should_use_mysql_schema_template(database_url) {
        clone_mysql_schema_from_template(&db).await;
    } else {
        Migrator::up(&db, None).await.unwrap();
    }

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
            bootstrap_insecure_cookies: true,
        },
        ..Default::default()
    });

    // 创建默认本地存储策略
    use chrono::Utc;
    use sea_orm::Set;
    let now = Utc::now();
    let _ = aster_drive::db::repository::policy_repo::create(
        &db,
        aster_drive::entities::storage_policy::ActiveModel {
            name: Set("Test Local".to_string()),
            driver_type: Set(aster_drive::types::DriverType::Local),
            endpoint: Set(String::new()),
            bucket: Set(String::new()),
            access_key: Set(String::new()),
            secret_key: Set(String::new()),
            base_path: Set(test_dir),
            max_file_size: Set(0),
            allowed_types: Set(aster_drive::types::StoredStoragePolicyAllowedTypes::empty()),
            options: Set(aster_drive::types::StoredStoragePolicyOptions::empty()),
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

    let runtime_config = std::sync::Arc::new(aster_drive::config::RuntimeConfig::new());
    runtime_config.reload(&db).await.unwrap();

    let policy_snapshot = std::sync::Arc::new(aster_drive::storage::PolicySnapshot::new());
    policy_snapshot.reload(&db).await.unwrap();
    let mail_sender = aster_forge_mail::memory_sender();

    let (storage_change_tx, _) = tokio::sync::broadcast::channel(
        aster_drive::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
    );
    let share_download_rollback =
        aster_drive::services::share::spawn_detached_share_download_rollback_queue(
            db.clone(),
            aster_drive::config::operations::share_download_rollback_queue_capacity(
                &runtime_config,
            ),
        );

    let driver_registry = std::sync::Arc::new(aster_drive::storage::DriverRegistry::noop());
    let remote_protocol = aster_drive::runtime::PrimaryAppState::new_remote_protocol();
    remote_protocol.set_persistence_db(db.clone());
    driver_registry.set_remote_protocol(remote_protocol.clone());

    PrimaryAppState {
        db_handles: aster_drive::db::connect_reader_for_writer_with_metrics(
            &db_cfg,
            db.clone(),
            aster_drive::metrics::NoopMetrics::arc(),
        )
        .await
        .unwrap(),
        driver_registry,
        runtime_config,
        policy_snapshot,
        config,
        cache,
        config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
        metrics: aster_drive::metrics::NoopMetrics::arc(),
        mail_sender,
        storage_change_tx,
        share_download_rollback,
        background_task_dispatch_wakeup:
            aster_drive::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
        remote_protocol,
        upload_runtime: aster_drive::runtime::PrimaryAppState::new_upload_runtime(),
    }
}

#[allow(dead_code)]
pub async fn flush_mail_outbox(state: &PrimaryAppState) {
    flush_mail_outbox_with(state.writer_db(), &state.runtime_config, &state.mail_sender).await;
}

#[allow(dead_code)]
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
#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
pub fn extract_token_from_mail_message(
    message: &aster_forge_mail::MailMessage,
    marker: &str,
) -> Option<String> {
    extract_token_from_content(&message.text_body, marker)
        .or_else(|| extract_token_from_content(&message.html_body, marker))
}

#[allow(dead_code)]
pub fn extract_verification_token_from_mail_sender(
    sender: &Arc<dyn aster_forge_mail::MailSender>,
) -> Option<String> {
    let memory_sender = aster_forge_mail::memory_sender_ref(sender)
        .expect("memory mail sender should be available in tests");
    let message = memory_sender.last_message()?;
    extract_token_from_mail_message(&message, "/api/v1/auth/contact-verification/confirm?token=")
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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
        use aster_drive::runtime::SharedRuntimeState;

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
        use actix_web::{App, test, web};
        use aster_drive::runtime::SharedRuntimeState;

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
                .app_data(web::Data::new(state))
                .configure(move |cfg| {
                    aster_drive::webdav::configure(cfg, &webdav_config, &db2);
                    aster_drive::api::configure_primary(cfg, &db1);
                }),
        )
        .await;
        app
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
