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
use aster_drive::runtime::PrimaryAppState;
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

fn webdav_test_username(label: &str) -> String {
    format!("client-dav-{label}-{}", uuid::Uuid::new_v4().simple())
}

fn webdav_test_password(label: &str) -> String {
    format!("CLIENT_DAV_{label}_{}", uuid::Uuid::new_v4().simple())
}

fn unique_name(label: &str) -> String {
    format!("{label}-{}", uuid::Uuid::new_v4().simple())
}

fn temp_dir(label: &str) -> (PathBuf, aster_drive::utils::raii::TempDirGuard) {
    let path = std::env::temp_dir().join(unique_name(label));
    std::fs::create_dir_all(&path).expect("client e2e temp dir should be created");
    let guard = aster_drive::utils::raii::TempDirGuard::new(path.clone(), "webdav client e2e");
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
            .wrap(aster_drive::api::middleware::security_headers::default_headers())
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
        password_hash: Set(aster_drive::utils::hash::hash_password(&password)
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
    let mut full_args = rclone_base_args(config_path);
    full_args.extend(args.iter().map(|arg| (*arg).to_string()));
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

#[actix_web::test]
#[ignore = "requires the rclone binary, add -- --ignored to run"]
async fn test_webdav_rclone_client_roundtrip() {
    let state = common::setup().await;
    let (username, password) = seed_real_webdav_account(&state).await;
    let server = start_real_webdav_server(state).await;
    let (work_dir, _work_dir_guard) = temp_dir("asterdrive-rclone-webdav-e2e");
    let config_path = work_dir.join("rclone.conf");
    let source_path = work_dir.join("source.txt");
    let downloaded_path = work_dir.join("downloaded.txt");
    let dir_name = unique_name("rclone-dir");
    let original_remote = format!("asterdav:{dir_name}/hello world.txt");
    let copied_remote = format!("asterdav:{dir_name}/copied.txt");
    let moved_remote = format!("asterdav:{dir_name}/moved.txt");
    let content = "AsterDrive rclone WebDAV compatibility\nline two\n";

    std::fs::write(&source_path, content).expect("rclone source file should be written");
    let obscured_password = obscure_rclone_password(&password).await;
    std::fs::write(
        &config_path,
        format!(
            "[asterdav]\ntype = webdav\nurl = {}/webdav\nvendor = other\nuser = {}\npass = {}\n",
            server.base_url, username, obscured_password
        ),
    )
    .expect("rclone config should be written");

    let root_listing = run_rclone(&config_path, &["lsf", "asterdav:"]).await;
    assert!(
        !root_listing.stdout.contains(&dir_name),
        "fresh rclone test directory should not already exist: {}",
        root_listing.stdout
    );

    run_rclone(&config_path, &["mkdir", &format!("asterdav:{dir_name}")]).await;
    run_rclone(
        &config_path,
        &["copyto", &path_arg(&source_path), &original_remote],
    )
    .await;

    let listing = run_rclone(&config_path, &["lsf", &format!("asterdav:{dir_name}")]).await;
    assert!(
        listing.stdout.contains("hello world.txt"),
        "rclone listing should include uploaded file: {}",
        listing.stdout
    );

    let cat = run_rclone(&config_path, &["cat", &original_remote]).await;
    assert_eq!(cat.stdout, content, "rclone should read uploaded bytes");

    run_rclone(&config_path, &["copyto", &original_remote, &copied_remote]).await;
    run_rclone(&config_path, &["moveto", &copied_remote, &moved_remote]).await;
    run_rclone(
        &config_path,
        &["copyto", &moved_remote, &path_arg(&downloaded_path)],
    )
    .await;
    let downloaded =
        std::fs::read_to_string(&downloaded_path).expect("rclone downloaded file should read");
    assert_eq!(downloaded, content);

    run_rclone(&config_path, &["deletefile", &moved_remote]).await;
    run_rclone(&config_path, &["deletefile", &original_remote]).await;
    run_rclone(&config_path, &["rmdir", &format!("asterdav:{dir_name}")]).await;

    let root_listing = run_rclone(&config_path, &["lsf", "asterdav:"]).await;
    assert!(
        !root_listing.stdout.contains(&dir_name),
        "rclone cleanup should remove test directory: {}",
        root_listing.stdout
    );

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

    let netrc_path = work_dir.join(".netrc");
    std::fs::write(
        &netrc_path,
        format!("machine 127.0.0.1\nlogin {username}\npassword {password}\n"),
    )
    .expect("cadaver netrc file should be written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        std::fs::set_permissions(&netrc_path, std::fs::Permissions::from_mode(0o600))
            .expect("cadaver netrc permissions should be restricted");
    }

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
