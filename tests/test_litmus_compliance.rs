//! 集成测试：`test_litmus_compliance`。

#[macro_use]
mod common;

use actix_web::{App, HttpServer, web};
use aster_drive::config::WebDavConfig;
use aster_drive::entities::{user, webdav_account};
use aster_drive::runtime::{PrimaryAppState, SharedRuntimeState};
use aster_drive::types::{UserRole, UserStatus};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, Set};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;

// 搜索 litmus 测试可执行文件的目录，按优先级顺序
const LITMUS_BIN_DIRS: &[&str] = &[
    "/usr/libexec/litmus",
    "/usr/lib/litmus",
    "/usr/local/libexec/litmus",
    "/usr/local/lib/litmus",
];

// 单个 litmus 测试组的最大挂钟时间
const CLIENT_COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

struct LitmusGroup {
    name: &'static str,
    executable: &'static str,
}

// 五个核心 RFC 4918 litmus 测试组
const TEST_GROUPS: &[LitmusGroup] = &[
    LitmusGroup {
        name: "basic",
        executable: "basic",
    },
    LitmusGroup {
        name: "copymove",
        executable: "copymove",
    },
    LitmusGroup {
        name: "props",
        executable: "props",
    },
    LitmusGroup {
        name: "locks",
        executable: "locks",
    },
    LitmusGroup {
        name: "http",
        executable: "http",
    },
];

// 运行单个 litmus 测试组可执行文件的结果
struct LitmusResult {
    group: String,
    exit_status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
}

// 一个正在运行的 Actix Web 服务器，仅公开 WebDAV 路由
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

// 在随机本地端口上启动真实的 WebDAV 服务器
async fn start_real_webdav_server(state: PrimaryAppState) -> RunningWebdavServer {
    let db = state.writer_db().clone();
    let webdav_config = WebDavConfig::default();
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("litmus test server should bind to a random local port");
    let addr = listener
        .local_addr()
        .expect("litmus test server local addr should be available");
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
    .expect("litmus test server should listen")
    .run();
    let handle = server.handle();
    let task = tokio::spawn(server);
    RunningWebdavServer {
        base_url: format!("http://{addr}"),
        handle,
        task,
    }
}

// 在数据库中创建真实的用户和 WebDAV 账户
async fn seed_real_webdav_account(state: &PrimaryAppState) -> (String, String) {
    let now = Utc::now();
    let default_policy_group =
        aster_drive::db::repository::policy_group_repo::find_default_group(state.writer_db())
            .await
            .expect("default policy group lookup should succeed")
            .expect("default policy group should exist");
    let user_suffix = uuid::Uuid::new_v4().simple().to_string();
    let user = user::ActiveModel {
        username: Set(format!("litmus-user-{user_suffix}")),
        email: Set(format!("litmus-user-{user_suffix}@example.com")),
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
    .expect("litmus test user should be inserted");
    state
        .policy_snapshot
        .set_user_policy_group(user.id, default_policy_group.id);

    let username = format!("litmus-dav-{}", uuid::Uuid::new_v4().simple());
    let password = format!("LITMUS_DAV_{}", uuid::Uuid::new_v4().simple());
    webdav_account::ActiveModel {
        user_id: Set(user.id),
        username: Set(username.clone()),
        password_hash: Set(aster_forge_crypto::hash_password(&password)
            .expect("litmus WebDAV password should hash")),
        root_folder_id: Set(None),
        is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("litmus WebDAV account should be inserted");

    (username, password)
}

// 通过检查已知目录来解析 litmus 测试可执行文件的路径
fn resolve_litmus_executable(exec_name: &str) -> Option<PathBuf> {
    for dir in LITMUS_BIN_DIRS {
        let path = Path::new(dir).join(exec_name);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

// 针对正在运行的 WebDAV 服务器运行一个 litmus 测试组可执行文件
async fn run_litmus_group(
    executable: &Path,
    url: &str,
    username: &str,
    password: &str,
    group_name: &str,
) -> LitmusResult {
    let program = executable.to_string_lossy().into_owned();
    let args = vec![url.to_string(), username.to_string(), password.to_string()];

    let display = format!(
        "{} {} {} {}",
        program, url, group_name, "[credentials hidden]"
    );
    let group_owned = group_name.to_string();
    let output = tokio::task::spawn_blocking(move || {
        let mut command = Command::new(&program);
        command.args(&args);
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.stdin(Stdio::null());

        let mut child = command
            .spawn()
            .unwrap_or_else(|e| panic!("failed to spawn litmus `{display}`: {e}"));

        let started_at = Instant::now();
        loop {
            if started_at.elapsed() > CLIENT_COMMAND_TIMEOUT {
                let _ = child.kill();
                let output = child.wait_with_output().unwrap_or_else(|e| {
                    panic!("failed to collect timed-out litmus `{display}`: {e}")
                });
                panic!(
                    "litmus `{display}` timed out after {:?}\nstdout:\n{}\nstderr:\n{}",
                    CLIENT_COMMAND_TIMEOUT,
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }

            match child.try_wait() {
                Ok(Some(_)) => {
                    let output = child
                        .wait_with_output()
                        .unwrap_or_else(|e| panic!("failed to collect litmus `{display}`: {e}"));
                    return LitmusResult {
                        group: group_owned,
                        exit_status: output.status,
                        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                    };
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(25)),
                Err(e) => panic!("failed to poll litmus `{display}`: {e}"),
            }
        }
    })
    .await
    .expect("litmus blocking task should complete");

    output
}

// 解析 litmus 测试输出并提取通过/失败计数
fn parse_litmus_output(stdout: &str) -> (usize, usize, Vec<String>) {
    let mut total: usize = 0;
    let mut failed: usize = 0;
    let mut failed_tests: Vec<String> = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();

        if let Some(dot_pos) = trimmed.find(". ") {
            let prefix = &trimmed[..dot_pos];
            if prefix.parse::<usize>().is_ok() {
                let after_number = trimmed[dot_pos + 2..].trim();
                total += 1;

                let lower = after_number.to_ascii_lowercase();
                if lower.contains("fail") {
                    failed += 1;
                    let test_name = after_number
                        .split("...")
                        .next()
                        .unwrap_or(after_number)
                        .split("FAIL")
                        .next()
                        .unwrap_or(after_number)
                        .trim()
                        .to_string();
                    if !test_name.is_empty() {
                        failed_tests.push(test_name);
                    } else {
                        failed_tests.push(after_number.to_string());
                    }
                }
            }
        }
    }

    // 回退
    if total == 0 {
        for line in stdout.lines() {
            let trimmed = line.trim();
            let lower = trimmed.to_ascii_lowercase();
            if lower.contains("fail") && !lower.contains("pass") {
                failed += 1;
                failed_tests.push(trimmed.to_string());
            }
            if lower.contains("fail") || lower.contains("pass") {
                total += 1;
            }
        }
    }

    (total, failed, failed_tests)
}

fn format_litmus_failure(result: &LitmusResult, failed_tests: &[String]) -> String {
    let mut msg = format!(
        "litmus `{}` group FAILED (exit code: {:?})\n",
        result.group,
        result.exit_status.code(),
    );

    if !failed_tests.is_empty() {
        msg.push_str(&format!("{} test(s) failed:\n", failed_tests.len()));
        for test in failed_tests {
            msg.push_str(&format!("  - {test}\n"));
        }
    }

    msg.push_str("\n--- stdout ---\n");
    msg.push_str(&result.stdout);
    if !result.stderr.is_empty() {
        msg.push_str("\n--- stderr ---\n");
        msg.push_str(&result.stderr);
    }

    msg
}

// 针对新配置的 WebDAV 服务器运行单个 litmus 测试组
async fn run_single_litmus_test(state: PrimaryAppState, group: &LitmusGroup) {
    let (username, password) = seed_real_webdav_account(&state).await;
    let server = start_real_webdav_server(state).await;

    let webdav_url = format!("{}/webdav/", server.base_url);

    let executable = resolve_litmus_executable(group.executable).unwrap_or_else(|| {
        panic!(
            "litmus executable `{}` not found in any of {:?}. \
             Install litmus with: sudo apt install litmus",
            group.executable, LITMUS_BIN_DIRS
        )
    });

    let result = run_litmus_group(&executable, &webdav_url, &username, &password, group.name).await;

    let (total, failed, failed_tests) = parse_litmus_output(&result.stdout);

    println!(
        "[litmus/{}]: {}/{} passed",
        group.name,
        total - failed,
        total,
    );

    if !result.exit_status.success() {
        panic!("{}", format_litmus_failure(&result, &failed_tests));
    }

    server.stop().await;
}

#[actix_web::test]
#[ignore = "需要 litmus 二进制文件，使用 -- --ignored 来运行"]
// 测试 OPTIONS、PUT/GET 字节比对、MKCOL、DELETE
async fn test_litmus_basic() {
    let state = common::setup().await;
    run_single_litmus_test(state, &TEST_GROUPS[0]).await;
}

#[actix_web::test]
#[ignore = "需要 litmus 二进制文件，使用 -- --ignored 来运行"]
// 测试 COPY 和 MOVE，包含各种覆盖/目标/集合组合
async fn test_litmus_copymove() {
    let state = common::setup().await;
    run_single_litmus_test(state, &TEST_GROUPS[1]).await;
}

#[actix_web::test]
#[ignore = "需要 litmus 二进制文件，使用 -- --ignored 来运行"]
// 测试 PROPFIND/PROPPATCH：设置、删除、替换、跨 COPY 的死属性、命名空间
async fn test_litmus_props() {
    let state = common::setup().await;
    run_single_litmus_test(state, &TEST_GROUPS[2]).await;
}

#[actix_web::test]
#[ignore = "需要 litmus 二进制文件，使用 -- --ignored 来运行"]
// 测试 LOCK/UNLOCK：共享/排他锁、锁发现、集合锁定、刷新
async fn test_litmus_locks() {
    let state = common::setup().await;
    run_single_litmus_test(state, &TEST_GROUPS[3]).await;
}

#[actix_web::test]
#[ignore = "需要 litmus 二进制文件，使用 -- --ignored 来运行"]
// 测试 HTTP 前提条件：If-Match、If-None-Match、Range、Expect 等
async fn test_litmus_http() {
    let state = common::setup().await;
    run_single_litmus_test(state, &TEST_GROUPS[4]).await;
}
