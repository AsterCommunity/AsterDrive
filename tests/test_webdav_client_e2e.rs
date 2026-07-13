//! Real WebDAV client compatibility tests.
//!
//! These tests require external binaries and are intentionally ignored by
//! default. Run with:
//!
//! `cargo test --test test_webdav_client_e2e -- --ignored --nocapture`

mod common;

use actix_web::{App, HttpServer, web};
use aster_drive::config::WebDavConfig;
use aster_drive::entities::{user, webdav_account};
use aster_drive::runtime::{PrimaryAppState, SharedRuntimeState};
use aster_drive::types::{UserRole, UserStatus};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, Set};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;

const CLIENT_COMMAND_TIMEOUT: Duration = Duration::from_secs(45);

struct RunningWebdavServer {
    base_url: String,
    handle: actix_web::dev::ServerHandle,
    task: JoinHandle<std::io::Result<()>>,
}

impl RunningWebdavServer {
    async fn stop(self) {
        self.handle.stop(true).await;
        let _ = self.task.await;
    }
}

struct ClientCommandOutput {
    stdout: String,
    stderr: String,
}

struct RcloneWebdavClient {
    server: RunningWebdavServer,
    work_dir: PathBuf,
    _work_dir_guard: aster_forge_utils::raii::TempDirGuard,
    config_path: PathBuf,
}

impl RcloneWebdavClient {
    async fn stop(self) {
        self.server.stop().await;
    }
}

fn webdav_test_username(label: &str) -> String {
    format!("client-dav-{label}-{}", uuid::Uuid::new_v4().simple())
}

fn webdav_test_password(label: &str) -> String {
    format!("CLIENT_DAV_{label}_{}", uuid::Uuid::new_v4().simple())
}

fn unique_name(label: &str) -> String {
    format!("{label}-{}", uuid::Uuid::new_v4().simple())
}

fn temp_dir(label: &str) -> (PathBuf, aster_forge_utils::raii::TempDirGuard) {
    let path = std::env::temp_dir().join(unique_name(label));
    std::fs::create_dir_all(&path).expect("client e2e temp dir should be created");
    let guard = aster_forge_utils::raii::TempDirGuard::new(path.clone(), "webdav client e2e");
    (path, guard)
}

fn path_arg(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn display_command(program: &str, args: &[String]) -> String {
    std::iter::once(program.to_string())
        .chain(args.iter().map(|arg| format!("{arg:?}")))
        .collect::<Vec<_>>()
        .join(" ")
}

async fn start_real_webdav_server(state: PrimaryAppState) -> RunningWebdavServer {
    let db = state.writer_db().clone();
    let webdav_config = WebDavConfig::default();
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("real WebDAV client test server should bind to a random local port");
    let addr = listener
        .local_addr()
        .expect("real WebDAV client test server local addr should be available");
    let server = HttpServer::new(move || {
        let db = db.clone();
        let webdav_config = webdav_config.clone();
        App::new()
            .wrap(actix_web::middleware::Compress::default())
            .wrap(aster_forge_actix_middleware::security_headers::default_headers())
            .app_data(web::PayloadConfig::new(10 * 1024 * 1024))
            .app_data(web::JsonConfig::default().limit(1024 * 1024))
            .app_data(web::Data::new(state.clone()))
            .configure(move |cfg| aster_drive::webdav::configure(cfg, &webdav_config, &db))
    })
    .listen(listener)
    .expect("real WebDAV client test server should listen")
    .run();
    let handle = server.handle();
    let task = tokio::spawn(server);

    RunningWebdavServer {
        base_url: format!("http://{addr}"),
        handle,
        task,
    }
}

async fn seed_real_webdav_account(state: &PrimaryAppState) -> (String, String) {
    let now = Utc::now();
    let default_policy_group =
        aster_drive::db::repository::policy_group_repo::find_default_group(state.writer_db())
            .await
            .expect("default policy group lookup should succeed")
            .expect("default policy group should exist");
    let user_suffix = uuid::Uuid::new_v4().simple().to_string();
    let user = user::ActiveModel {
        username: Set(format!("webdav-client-user-{user_suffix}")),
        email: Set(format!("webdav-client-user-{user_suffix}@example.com")),
        password_hash: Set("unused".to_string()),
        role: Set(UserRole::User),
        status: Set(UserStatus::Active),
        session_version: Set(0),
        email_verified_at: Set(Some(now)),
        pending_email: Set(None),
        storage_used: Set(0),
        storage_quota: Set(0),
        policy_group_id: Set(Some(default_policy_group.id)),
        created_at: Set(now),
        updated_at: Set(now),
        config: Set(None),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("real WebDAV client test user should be inserted");
    state
        .policy_snapshot
        .set_user_policy_group(user.id, default_policy_group.id);

    let username = webdav_test_username("account");
    let password = webdav_test_password("ACCOUNT");
    webdav_account::ActiveModel {
        user_id: Set(user.id),
        username: Set(username.clone()),
        password_hash: Set(aster_forge_crypto::hash_password(&password)
            .expect("real WebDAV client test password should hash")),
        root_folder_id: Set(None),
        is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("real WebDAV client account should be inserted");

    (username, password)
}

async fn run_client_command(
    program: &str,
    args: Vec<String>,
    stdin: Option<String>,
) -> ClientCommandOutput {
    run_client_command_with_env(program, args, stdin, Vec::new(), None).await
}

async fn run_client_command_with_env(
    program: &str,
    args: Vec<String>,
    stdin: Option<String>,
    envs: Vec<(String, String)>,
    current_dir: Option<PathBuf>,
) -> ClientCommandOutput {
    let program = program.to_string();
    let command_display = display_command(&program, &args);
    tokio::task::spawn_blocking(move || {
        let mut command = Command::new(&program);
        command.args(&args);
        command.envs(envs);
        if let Some(current_dir) = current_dir {
            command.current_dir(current_dir);
        }
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        if stdin.is_some() {
            command.stdin(Stdio::piped());
        } else {
            command.stdin(Stdio::null());
        }

        let mut child = command
            .spawn()
            .unwrap_or_else(|error| panic!("failed to spawn client command `{command_display}`: {error}"));

        if let Some(input) = stdin
            && let Some(mut child_stdin) = child.stdin.take()
            && let Err(error) = child_stdin.write_all(input.as_bytes())
            && error.kind() != std::io::ErrorKind::BrokenPipe
        {
            panic!("failed to write stdin for client command `{command_display}`: {error}");
        }

        let started_at = Instant::now();
        loop {
            if started_at.elapsed() > CLIENT_COMMAND_TIMEOUT {
                let _ = child.kill();
                let output = child.wait_with_output().unwrap_or_else(|error| {
                    panic!("failed to collect timed-out client command `{command_display}`: {error}")
                });
                panic!(
                    "client command timed out after {:?}: {command_display}\nstdout:\n{}\nstderr:\n{}",
                    CLIENT_COMMAND_TIMEOUT,
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }

            match child.try_wait() {
                Ok(Some(_)) => {
                    let output = child.wait_with_output().unwrap_or_else(|error| {
                        panic!("failed to collect client command `{command_display}`: {error}")
                    });
                    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                    assert!(
                        output.status.success(),
                        "client command failed: {command_display}\nstatus: {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
                        output.status
                    );
                    return ClientCommandOutput { stdout, stderr };
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(25)),
                Err(error) => panic!("failed to poll client command `{command_display}`: {error}"),
            }
        }
    })
    .await
    .expect("client command blocking task should complete")
}

fn rclone_base_args(config_path: &Path) -> Vec<String> {
    vec![
        "--config".to_string(),
        path_arg(config_path),
        "--retries".to_string(),
        "1".to_string(),
        "--low-level-retries".to_string(),
        "1".to_string(),
        "--contimeout".to_string(),
        "5s".to_string(),
        "--timeout".to_string(),
        "15s".to_string(),
        "--stats".to_string(),
        "0".to_string(),
    ]
}

async fn run_rclone(config_path: &Path, args: &[&str]) -> ClientCommandOutput {
    run_rclone_args(
        config_path,
        args.iter().map(|arg| (*arg).to_string()).collect(),
    )
    .await
}

async fn run_rclone_args(config_path: &Path, args: Vec<String>) -> ClientCommandOutput {
    let mut full_args = rclone_base_args(config_path);
    full_args.extend(args);
    run_client_command("rclone", full_args, None).await
}

async fn obscure_rclone_password(password: &str) -> String {
    let output = run_client_command(
        "rclone",
        vec!["obscure".to_string(), password.to_string()],
        None,
    )
    .await;
    output.stdout.trim().to_string()
}

async fn write_rclone_config(config_path: &Path, base_url: &str, username: &str, password: &str) {
    let obscured_password = obscure_rclone_password(password).await;
    std::fs::write(
        config_path,
        format!(
            "[asterdav]\ntype = webdav\nurl = {base_url}/webdav\nvendor = other\nuser = {username}\npass = {obscured_password}\n"
        ),
    )
    .expect("rclone config should be written");
}

async fn setup_rclone_webdav_client(label: &str) -> RcloneWebdavClient {
    let state = common::setup().await;
    let (username, password) = seed_real_webdav_account(&state).await;
    let server = start_real_webdav_server(state).await;
    let (work_dir, work_dir_guard) = temp_dir(label);
    let config_path = work_dir.join("rclone.conf");

    write_rclone_config(&config_path, &server.base_url, &username, &password).await;

    RcloneWebdavClient {
        server,
        work_dir,
        _work_dir_guard: work_dir_guard,
        config_path,
    }
}

fn write_netrc_credentials(work_dir: &Path, username: &str, password: &str) -> PathBuf {
    let netrc_path = work_dir.join(".netrc");
    std::fs::write(
        &netrc_path,
        format!("machine 127.0.0.1\nlogin {username}\npassword {password}\n"),
    )
    .expect("WebDAV client netrc file should be written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        std::fs::set_permissions(&netrc_path, std::fs::Permissions::from_mode(0o600))
            .expect("WebDAV client netrc permissions should be restricted");
    }

    netrc_path
}

fn remote_path(path: &str) -> String {
    format!("asterdav:{path}")
}

fn ascii_pattern_bytes(len: usize) -> Vec<u8> {
    const PATTERN: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz-_.";
    (0..len)
        .map(|index| PATTERN[index % PATTERN.len()])
        .collect()
}

fn assert_listing_contains(listing: &str, expected: &str) {
    assert!(
        listing.lines().any(|line| line == expected),
        "listing should contain {expected:?}\nlisting:\n{listing}"
    );
}

fn assert_listing_not_contains(listing: &str, unexpected: &str) {
    assert!(
        listing.lines().all(|line| line != unexpected),
        "listing should not contain {unexpected:?}\nlisting:\n{listing}"
    );
}

fn curl_base_args(netrc_path: &Path) -> Vec<String> {
    vec![
        "--silent".to_string(),
        "--show-error".to_string(),
        "--fail-with-body".to_string(),
        "--netrc-file".to_string(),
        path_arg(netrc_path),
    ]
}

async fn run_curl(netrc_path: &Path, args: Vec<String>) -> ClientCommandOutput {
    let mut full_args = curl_base_args(netrc_path);
    full_args.extend(args);
    run_client_command("curl", full_args, None).await
}

fn header_value(headers: &str, name: &str) -> Option<String> {
    headers.lines().find_map(|line| {
        let (header_name, value) = line.split_once(':')?;
        header_name
            .eq_ignore_ascii_case(name)
            .then(|| value.trim().to_string())
    })
}

#[actix_web::test]
#[ignore = "requires the rclone binary, add -- --ignored to run"]
async fn test_webdav_rclone_client_roundtrip() {
    let client = setup_rclone_webdav_client("asterdrive-rclone-webdav-e2e").await;
    let source_path = client.work_dir.join("source.txt");
    let downloaded_path = client.work_dir.join("downloaded.txt");
    let dir_name = unique_name("rclone-dir");
    let original_remote = format!("asterdav:{dir_name}/hello world.txt");
    let copied_remote = format!("asterdav:{dir_name}/copied.txt");
    let moved_remote = format!("asterdav:{dir_name}/moved.txt");
    let content = "AsterDrive rclone WebDAV compatibility\nline two\n";

    std::fs::write(&source_path, content).expect("rclone source file should be written");

    let root_listing = run_rclone(&client.config_path, &["lsf", "asterdav:"]).await;
    assert!(
        !root_listing.stdout.contains(&dir_name),
        "fresh rclone test directory should not already exist: {}",
        root_listing.stdout
    );

    run_rclone(
        &client.config_path,
        &["mkdir", &format!("asterdav:{dir_name}")],
    )
    .await;
    run_rclone(
        &client.config_path,
        &["copyto", &path_arg(&source_path), &original_remote],
    )
    .await;

    let listing = run_rclone(
        &client.config_path,
        &["lsf", &format!("asterdav:{dir_name}")],
    )
    .await;
    assert!(
        listing.stdout.contains("hello world.txt"),
        "rclone listing should include uploaded file: {}",
        listing.stdout
    );

    let cat = run_rclone(&client.config_path, &["cat", &original_remote]).await;
    assert_eq!(cat.stdout, content, "rclone should read uploaded bytes");

    run_rclone(
        &client.config_path,
        &["copyto", &original_remote, &copied_remote],
    )
    .await;
    run_rclone(
        &client.config_path,
        &["moveto", &copied_remote, &moved_remote],
    )
    .await;
    run_rclone(
        &client.config_path,
        &["copyto", &moved_remote, &path_arg(&downloaded_path)],
    )
    .await;
    let downloaded =
        std::fs::read_to_string(&downloaded_path).expect("rclone downloaded file should read");
    assert_eq!(downloaded, content);

    run_rclone(&client.config_path, &["deletefile", &moved_remote]).await;
    run_rclone(&client.config_path, &["deletefile", &original_remote]).await;
    run_rclone(
        &client.config_path,
        &["rmdir", &format!("asterdav:{dir_name}")],
    )
    .await;

    let root_listing = run_rclone(&client.config_path, &["lsf", "asterdav:"]).await;
    assert!(
        !root_listing.stdout.contains(&dir_name),
        "rclone cleanup should remove test directory: {}",
        root_listing.stdout
    );

    client.stop().await;
}

#[actix_web::test]
#[ignore = "requires the rclone binary, add -- --ignored to run"]
async fn test_webdav_rclone_sync_tree_updates_deletes_and_downloads() {
    let client = setup_rclone_webdav_client("asterdrive-rclone-sync-webdav-e2e").await;
    let source_dir = client.work_dir.join("sync-source");
    let download_dir = client.work_dir.join("sync-download");
    let remote_dir = unique_name("rclone-sync-dir");
    let remote = remote_path(&remote_dir);

    std::fs::create_dir_all(source_dir.join("nested/empty-child"))
        .expect("rclone sync source directories should be created");
    std::fs::write(source_dir.join("root.txt"), "root v1\n")
        .expect("rclone root source file should be written");
    std::fs::write(source_dir.join("nested/alpha.txt"), "alpha v1\n")
        .expect("rclone nested source file should be written");
    std::fs::write(
        source_dir.join("nested/name with spaces #1.txt"),
        "special v1\n",
    )
    .expect("rclone special-name source file should be written");

    let source_arg = path_arg(&source_dir);
    run_rclone(
        &client.config_path,
        &["sync", "--create-empty-src-dirs", &source_arg, &remote],
    )
    .await;

    let listing = run_rclone(&client.config_path, &["lsf", "-R", &remote]).await;
    assert_listing_contains(&listing.stdout, "root.txt");
    assert_listing_contains(&listing.stdout, "nested/");
    assert_listing_contains(&listing.stdout, "nested/alpha.txt");
    assert_listing_contains(&listing.stdout, "nested/empty-child/");
    assert_listing_contains(&listing.stdout, "nested/name with spaces #1.txt");

    run_rclone(
        &client.config_path,
        &["check", "--size-only", &source_arg, &remote],
    )
    .await;

    std::fs::write(source_dir.join("root.txt"), "root v2 with more bytes\n")
        .expect("rclone updated root source file should be written");
    std::fs::remove_file(source_dir.join("nested/alpha.txt"))
        .expect("rclone removed source file should be deleted locally");
    std::fs::write(source_dir.join("nested/beta.txt"), "beta v2\n")
        .expect("rclone added nested source file should be written");
    std::fs::create_dir_all(source_dir.join("new-empty"))
        .expect("rclone new empty source directory should be created");

    run_rclone(
        &client.config_path,
        &["sync", "--create-empty-src-dirs", &source_arg, &remote],
    )
    .await;

    let updated_listing = run_rclone(&client.config_path, &["lsf", "-R", &remote]).await;
    assert_listing_contains(&updated_listing.stdout, "root.txt");
    assert_listing_contains(&updated_listing.stdout, "nested/beta.txt");
    assert_listing_contains(&updated_listing.stdout, "nested/name with spaces #1.txt");
    assert_listing_contains(&updated_listing.stdout, "new-empty/");
    assert_listing_not_contains(&updated_listing.stdout, "nested/alpha.txt");

    let cat = run_rclone(&client.config_path, &["cat", &format!("{remote}/root.txt")]).await;
    assert_eq!(cat.stdout, "root v2 with more bytes\n");

    let download_arg = path_arg(&download_dir);
    run_rclone(&client.config_path, &["sync", &remote, &download_arg]).await;
    let downloaded_root = std::fs::read_to_string(download_dir.join("root.txt"))
        .expect("rclone downloaded root file should be readable");
    assert_eq!(downloaded_root, "root v2 with more bytes\n");
    let downloaded_special =
        std::fs::read_to_string(download_dir.join("nested/name with spaces #1.txt"))
            .expect("rclone downloaded special-name file should be readable");
    assert_eq!(downloaded_special, "special v1\n");
    assert!(
        !download_dir.join("nested/alpha.txt").exists(),
        "rclone download should not resurrect a remote file deleted by sync"
    );

    run_rclone(&client.config_path, &["purge", &remote]).await;
    client.stop().await;
}

#[actix_web::test]
#[ignore = "requires the rclone binary, add -- --ignored to run"]
async fn test_webdav_rclone_server_side_recursive_copy_and_move() {
    let client =
        setup_rclone_webdav_client("asterdrive-rclone-recursive-copy-move-webdav-e2e").await;
    let source_dir = client.work_dir.join("tree-source");
    let root_dir = unique_name("rclone-tree-dir");
    let source_remote = remote_path(&format!("{root_dir}/source"));
    let copied_remote = remote_path(&format!("{root_dir}/copied"));
    let moved_remote = remote_path(&format!("{root_dir}/moved"));

    std::fs::create_dir_all(source_dir.join("sub/deep"))
        .expect("rclone recursive source directories should be created");
    std::fs::write(source_dir.join("top.txt"), "top file\n")
        .expect("rclone top source file should be written");
    std::fs::write(source_dir.join("sub/deep/nested.txt"), "nested file\n")
        .expect("rclone nested source file should be written");

    let source_arg = path_arg(&source_dir);
    run_rclone(&client.config_path, &["copy", &source_arg, &source_remote]).await;

    run_rclone(
        &client.config_path,
        &["copyto", &source_remote, &copied_remote],
    )
    .await;
    let copied_listing = run_rclone(&client.config_path, &["lsf", "-R", &copied_remote]).await;
    assert_listing_contains(&copied_listing.stdout, "top.txt");
    assert_listing_contains(&copied_listing.stdout, "sub/deep/nested.txt");
    let copied_nested = run_rclone(
        &client.config_path,
        &["cat", &format!("{copied_remote}/sub/deep/nested.txt")],
    )
    .await;
    assert_eq!(copied_nested.stdout, "nested file\n");

    run_rclone(
        &client.config_path,
        &["moveto", &copied_remote, &moved_remote],
    )
    .await;
    let root_listing = run_rclone(&client.config_path, &["lsf", &remote_path(&root_dir)]).await;
    assert_listing_contains(&root_listing.stdout, "source/");
    assert_listing_contains(&root_listing.stdout, "moved/");
    assert_listing_not_contains(&root_listing.stdout, "copied/");

    let moved_top = run_rclone(
        &client.config_path,
        &["cat", &format!("{moved_remote}/top.txt")],
    )
    .await;
    assert_eq!(moved_top.stdout, "top file\n");
    let original_top = run_rclone(
        &client.config_path,
        &["cat", &format!("{source_remote}/top.txt")],
    )
    .await;
    assert_eq!(
        original_top.stdout, "top file\n",
        "server-side COPY must leave the original tree intact"
    );

    run_rclone(&client.config_path, &["purge", &remote_path(&root_dir)]).await;
    client.stop().await;
}

#[actix_web::test]
#[ignore = "requires the rclone binary, add -- --ignored to run"]
async fn test_webdav_rclone_special_names_stat_and_range_reads() {
    let client = setup_rclone_webdav_client("asterdrive-rclone-special-webdav-e2e").await;
    let source_path = client.work_dir.join("range-source.txt");
    let remote_dir = unique_name("rclone-special-dir");
    let nested_remote = remote_path(&format!("{remote_dir}/space dir"));
    let file_remote = format!("{nested_remote}/range file #1 +plus.txt");
    let data = ascii_pattern_bytes(8193);

    std::fs::write(&source_path, &data).expect("rclone range source file should be written");
    run_rclone(&client.config_path, &["mkdir", &remote_path(&remote_dir)]).await;
    run_rclone(&client.config_path, &["mkdir", &nested_remote]).await;
    run_rclone(
        &client.config_path,
        &["copyto", &path_arg(&source_path), &file_remote],
    )
    .await;

    let listing = run_rclone(
        &client.config_path,
        &["lsf", "--format", "sp", "--separator", "|", &nested_remote],
    )
    .await;
    assert_listing_contains(&listing.stdout, "8193|range file #1 +plus.txt");

    let head = run_rclone(&client.config_path, &["cat", "--head", "32", &file_remote]).await;
    assert_eq!(head.stdout.as_bytes(), &data[..32]);

    let middle = run_rclone(
        &client.config_path,
        &["cat", "--offset", "1024", "--count", "257", &file_remote],
    )
    .await;
    assert_eq!(middle.stdout.as_bytes(), &data[1024..1281]);

    let tail = run_rclone(&client.config_path, &["cat", "--tail", "41", &file_remote]).await;
    assert_eq!(tail.stdout.as_bytes(), &data[data.len() - 41..]);

    run_rclone(&client.config_path, &["deletefile", &file_remote]).await;
    run_rclone(&client.config_path, &["rmdir", &nested_remote]).await;
    run_rclone(&client.config_path, &["rmdir", &remote_path(&remote_dir)]).await;
    client.stop().await;
}

#[actix_web::test]
#[ignore = "requires the curl binary, add -- --ignored to run"]
async fn test_webdav_curl_client_methods_ranges_and_locks() {
    let state = common::setup().await;
    let (username, password) = seed_real_webdav_account(&state).await;
    let server = start_real_webdav_server(state).await;
    let (work_dir, _work_dir_guard) = temp_dir("asterdrive-curl-webdav-e2e");
    let netrc_path = write_netrc_credentials(&work_dir, &username, &password);
    let upload_path = work_dir.join("curl-upload.txt");
    let range_headers_path = work_dir.join("range.headers");
    let range_body_path = work_dir.join("range.body");
    let lock_body_path = work_dir.join("lock.xml");
    let lock_headers_path = work_dir.join("lock.headers");
    let dir_name = unique_name("curl-dir");
    let dir_url = format!("{}/webdav/{dir_name}", server.base_url);
    let source_url = format!("{dir_url}/source.txt");
    let copied_url = format!("{dir_url}/copied.txt");
    let moved_url = format!("{dir_url}/moved.txt");
    let content = "curl WebDAV compatibility payload\nsecond line\nthird line\n";

    std::fs::write(&upload_path, content).expect("curl upload file should be written");
    std::fs::write(
        &lock_body_path,
        r#"<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D="DAV:">
  <D:lockscope><D:exclusive/></D:lockscope>
  <D:locktype><D:write/></D:locktype>
  <D:owner><D:href>curl-client</D:href></D:owner>
</D:lockinfo>"#,
    )
    .expect("curl LOCK body should be written");

    run_curl(
        &netrc_path,
        vec!["-X".to_string(), "MKCOL".to_string(), format!("{dir_url}/")],
    )
    .await;
    run_curl(
        &netrc_path,
        vec![
            "--upload-file".to_string(),
            path_arg(&upload_path),
            source_url.clone(),
        ],
    )
    .await;

    run_curl(
        &netrc_path,
        vec![
            "-H".to_string(),
            "Range: bytes=5-18".to_string(),
            "-D".to_string(),
            path_arg(&range_headers_path),
            "-o".to_string(),
            path_arg(&range_body_path),
            source_url.clone(),
        ],
    )
    .await;
    let range_headers =
        std::fs::read_to_string(&range_headers_path).expect("curl range headers should read");
    assert!(
        range_headers.contains("206"),
        "curl range GET should receive 206 Partial Content headers:\n{range_headers}"
    );
    let expected_content_range = format!("bytes 5-18/{}", content.len());
    assert_eq!(
        header_value(&range_headers, "Content-Range").as_deref(),
        Some(expected_content_range.as_str())
    );
    let range_body =
        std::fs::read_to_string(&range_body_path).expect("curl range response body should read");
    assert_eq!(range_body, &content[5..=18]);

    run_curl(
        &netrc_path,
        vec![
            "-X".to_string(),
            "COPY".to_string(),
            "-H".to_string(),
            format!("Destination: {copied_url}"),
            source_url.clone(),
        ],
    )
    .await;
    run_curl(
        &netrc_path,
        vec![
            "-X".to_string(),
            "MOVE".to_string(),
            "-H".to_string(),
            format!("Destination: {moved_url}"),
            copied_url.clone(),
        ],
    )
    .await;
    let moved = run_curl(&netrc_path, vec![moved_url.clone()]).await;
    assert_eq!(
        moved.stdout, content,
        "curl should read the resource after COPY then MOVE"
    );

    run_curl(
        &netrc_path,
        vec![
            "-X".to_string(),
            "LOCK".to_string(),
            "-H".to_string(),
            "Content-Type: application/xml".to_string(),
            "-H".to_string(),
            "Timeout: Second-3600".to_string(),
            "--data-binary".to_string(),
            format!("@{}", path_arg(&lock_body_path)),
            "-D".to_string(),
            path_arg(&lock_headers_path),
            moved_url.clone(),
        ],
    )
    .await;
    let lock_headers =
        std::fs::read_to_string(&lock_headers_path).expect("curl LOCK headers should read");
    let lock_token = header_value(&lock_headers, "Lock-Token")
        .unwrap_or_else(|| panic!("LOCK response should include Lock-Token:\n{lock_headers}"));
    assert!(
        lock_token.starts_with('<') && lock_token.ends_with('>'),
        "LOCK token should be enclosed for direct use in UNLOCK: {lock_token}"
    );

    run_curl(
        &netrc_path,
        vec![
            "-X".to_string(),
            "UNLOCK".to_string(),
            "-H".to_string(),
            format!("Lock-Token: {lock_token}"),
            moved_url.clone(),
        ],
    )
    .await;
    run_curl(
        &netrc_path,
        vec!["-X".to_string(), "DELETE".to_string(), moved_url],
    )
    .await;
    run_curl(
        &netrc_path,
        vec!["-X".to_string(), "DELETE".to_string(), source_url],
    )
    .await;
    run_curl(
        &netrc_path,
        vec![
            "-X".to_string(),
            "DELETE".to_string(),
            format!("{dir_url}/"),
        ],
    )
    .await;

    server.stop().await;
}

#[actix_web::test]
#[ignore = "requires the cadaver binary, add -- --ignored to run"]
async fn test_webdav_cadaver_client_roundtrip() {
    let state = common::setup().await;
    let (username, password) = seed_real_webdav_account(&state).await;
    let server = start_real_webdav_server(state).await;
    let (work_dir, _work_dir_guard) = temp_dir("asterdrive-cadaver-webdav-e2e");
    let rc_path = work_dir.join("cadaverrc");
    let source_path = work_dir.join("source.txt");
    let downloaded_path = work_dir.join("downloaded.txt");
    let moved_downloaded_path = work_dir.join("moved-downloaded.txt");
    let dir_name = unique_name("cadaver-dir");
    let content = "AsterDrive cadaver WebDAV compatibility\nline two\n";

    std::fs::write(&rc_path, "").expect("cadaver rc file should be written");
    std::fs::write(&source_path, content).expect("cadaver source file should be written");

    write_netrc_credentials(&work_dir, &username, &password);

    let endpoint =
        reqwest::Url::parse(&format!("{}/webdav/", server.base_url)).expect("valid WebDAV URL");

    let script = format!(
        "ls\nmkcol {dir_name}\ncd {dir_name}\nput {} original.txt\nls\nget original.txt {}\nmove original.txt moved.txt\nget moved.txt {}\ndelete moved.txt\ncd ..\nrmcol {dir_name}\nls\nquit\n",
        path_arg(&source_path),
        path_arg(&downloaded_path),
        path_arg(&moved_downloaded_path),
    );
    let output = run_client_command_with_env(
        "cadaver",
        vec!["-r".to_string(), path_arg(&rc_path), endpoint.to_string()],
        Some(script),
        vec![("HOME".to_string(), path_arg(&work_dir))],
        Some(work_dir.clone()),
    )
    .await;

    assert!(
        output.stdout.contains("original.txt") || output.stderr.contains("original.txt"),
        "cadaver listing should include uploaded file\nstdout:\n{}\nstderr:\n{}",
        output.stdout,
        output.stderr
    );
    assert!(
        downloaded_path.exists(),
        "cadaver should download original file to {}\nstdout:\n{}\nstderr:\n{}",
        downloaded_path.display(),
        output.stdout,
        output.stderr
    );
    let downloaded =
        std::fs::read_to_string(&downloaded_path).expect("cadaver downloaded file should read");
    assert_eq!(downloaded, content);
    assert!(
        moved_downloaded_path.exists(),
        "cadaver should download moved file to {}\nstdout:\n{}\nstderr:\n{}",
        moved_downloaded_path.display(),
        output.stdout,
        output.stderr
    );
    let moved_downloaded = std::fs::read_to_string(&moved_downloaded_path)
        .expect("cadaver moved downloaded file should read");
    assert_eq!(moved_downloaded, content);

    let client = reqwest::Client::new();
    let deleted = client
        .get(format!("{}/webdav/{dir_name}/moved.txt", server.base_url))
        .basic_auth(&username, Some(&password))
        .send()
        .await
        .expect("GET after cadaver cleanup should receive a response");
    assert_eq!(
        deleted.status(),
        reqwest::StatusCode::NOT_FOUND,
        "cadaver delete should remove moved file"
    );

    server.stop().await;
}
