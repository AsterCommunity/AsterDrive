//! WebDAV Litmus 0.18 conformance baseline.
//!
//! The external suites are ignored in the ordinary Rust test run. The
//! WebDAV compatibility workflow installs the pinned Litmus build and runs
//! them explicitly while preserving per-suite artifacts.

use crate::common;

use actix_web::dev::Service;
use actix_web::{App, HttpServer, web};
use aster_drive::config::WebDavConfig;
use aster_drive::entities::{user, webdav_account};
use aster_drive::runtime::{PrimaryAppState, SharedRuntimeState};
use aster_drive::types::{UserRole, UserStatus};
use base64::Engine;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, Set};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;

const LITMUS_VERSION: &str = "0.18";
const LITMUS_BIN_ENV: &str = "LITMUS_BIN";
const ARTIFACT_DIR_ENV: &str = "ASTER_WEBDAV_COMPAT_ARTIFACT_DIR";
const DEFAULT_SUITE_TIMEOUT: Duration = Duration::from_secs(120);
const BASELINE: &str = include_str!("fixtures/litmus-baseline.txt");

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum LitmusEvaluationMode {
    Baseline,
    Probe,
}

#[derive(Clone, Copy)]
struct LitmusGroup {
    name: &'static str,
    expected_test_count: usize,
    timeout: Duration,
    environment: &'static [(&'static str, &'static str)],
    evaluation_mode: LitmusEvaluationMode,
}

const TEST_GROUPS: &[LitmusGroup] = &[
    LitmusGroup {
        name: "basic",
        expected_test_count: 16,
        timeout: DEFAULT_SUITE_TIMEOUT,
        environment: &[],
        evaluation_mode: LitmusEvaluationMode::Baseline,
    },
    LitmusGroup {
        name: "copymove",
        expected_test_count: 13,
        timeout: DEFAULT_SUITE_TIMEOUT,
        environment: &[],
        evaluation_mode: LitmusEvaluationMode::Baseline,
    },
    LitmusGroup {
        name: "props",
        expected_test_count: 33,
        timeout: DEFAULT_SUITE_TIMEOUT,
        environment: &[],
        evaluation_mode: LitmusEvaluationMode::Baseline,
    },
    LitmusGroup {
        name: "locks",
        expected_test_count: 40,
        timeout: DEFAULT_SUITE_TIMEOUT,
        environment: &[],
        evaluation_mode: LitmusEvaluationMode::Baseline,
    },
    LitmusGroup {
        name: "http",
        expected_test_count: 4,
        timeout: DEFAULT_SUITE_TIMEOUT,
        environment: &[],
        evaluation_mode: LitmusEvaluationMode::Baseline,
    },
];

#[path = "litmus/resource.rs"]
mod resource_litmus;

#[path = "litmus/security_policy.rs"]
mod security_policy_litmus;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
enum LitmusCaseStatus {
    Passed,
    Failed,
    Skipped,
    ExpectedFailure,
    Warning,
}

impl LitmusCaseStatus {
    fn baseline_name(self) -> Option<&'static str> {
        match self {
            Self::Failed => Some("FAIL"),
            Self::Skipped => Some("SKIPPED"),
            Self::Warning => Some("WARNING"),
            Self::Passed | Self::ExpectedFailure => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct LitmusCaseResult {
    number: usize,
    name: String,
    status: LitmusCaseStatus,
    detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct LitmusWarning {
    test: String,
    message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedLitmusOutput {
    cases: Vec<LitmusCaseResult>,
    warnings: Vec<LitmusWarning>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct BaselineKey {
    group: String,
    status: LitmusCaseStatus,
    test: String,
}

#[derive(Clone, Debug)]
struct BaselineEntry {
    key: BaselineKey,
    tracking_issue: String,
    rationale: String,
}

#[derive(Serialize)]
struct LitmusReport {
    litmus_version: &'static str,
    group: String,
    evaluation_mode: LitmusEvaluationMode,
    environment: BTreeMap<&'static str, &'static str>,
    timeout_seconds: u64,
    expected_test_count: usize,
    observed_test_count: usize,
    exit_code: Option<i32>,
    timed_out: bool,
    cases: Vec<LitmusCaseResult>,
    warnings: Vec<LitmusWarning>,
    observed_differences: Vec<String>,
    accepted_differences: Vec<String>,
    errors: Vec<String>,
}

struct LitmusProcessResult {
    exit_status: ExitStatus,
    timed_out: bool,
    stdout: String,
    stderr: String,
}

struct TestWorkspace {
    path: PathBuf,
    _guard: Option<aster_forge_utils::raii::TempDirGuard>,
}

impl TestWorkspace {
    fn create(group: &str) -> Result<Self, String> {
        if let Some(root) = std::env::var_os(ARTIFACT_DIR_ENV) {
            let path = PathBuf::from(root).join("litmus").join(group);
            fs::create_dir_all(&path)
                .map_err(|error| format!("failed to create Litmus artifact directory: {error}"))?;
            return Ok(Self { path, _guard: None });
        }

        let path = std::env::temp_dir().join(format!(
            "asterdrive-litmus-{group}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&path)
            .map_err(|error| format!("failed to create Litmus temporary directory: {error}"))?;
        let guard = aster_forge_utils::raii::TempDirGuard::new(
            path.clone(),
            "WebDAV Litmus compatibility test",
        );
        Ok(Self {
            path,
            _guard: Some(guard),
        })
    }
}

struct RunningWebdavServer {
    base_url: String,
    handle: actix_web::dev::ServerHandle,
    task: JoinHandle<std::io::Result<()>>,
    server_log_path: PathBuf,
}

impl RunningWebdavServer {
    async fn stop(self) -> Result<(), String> {
        self.handle.stop(true).await;
        let result = match self.task.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(format!("WebDAV server stopped with an I/O error: {error}")),
            Err(error) => Err(format!("WebDAV server task join failed: {error}")),
        };

        let message = match &result {
            Ok(()) => "server stopped cleanly\n".to_string(),
            Err(error) => format!("{error}\n"),
        };
        if let Err(error) = fs::write(&self.server_log_path, message) {
            return Err(format!(
                "failed to write WebDAV server result to {}: {error}",
                self.server_log_path.display()
            ));
        }

        result
    }
}

async fn start_real_webdav_server(
    state: PrimaryAppState,
    workspace: &Path,
) -> Result<RunningWebdavServer, String> {
    let request_log_path = workspace.join("requests.log");
    let request_log = File::create(&request_log_path).map_err(|error| {
        format!(
            "failed to create WebDAV request log {}: {error}",
            request_log_path.display()
        )
    })?;
    let request_log = Arc::new(Mutex::new(request_log));
    let server_log_path = workspace.join("server.log");
    let db = state.writer_db().clone();
    let webdav_config = WebDavConfig::default();
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("Litmus test server failed to bind: {error}"))?;
    let addr = listener
        .local_addr()
        .map_err(|error| format!("Litmus test server address lookup failed: {error}"))?;

    let server = HttpServer::new(move || {
        let db = db.clone();
        let webdav_config = webdav_config.clone();
        let request_log = Arc::clone(&request_log);
        App::new()
            .wrap_fn(move |request, service| {
                let started_at = Instant::now();
                let method = request.method().clone();
                let uri = request.uri().clone();
                let litmus_case = request
                    .headers()
                    .get("X-Litmus")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("-")
                    .to_string();
                let request_log = Arc::clone(&request_log);
                let response = service.call(request);

                async move {
                    let result = response.await;
                    let status = result
                        .as_ref()
                        .map(|response| response.status().as_u16().to_string())
                        .unwrap_or_else(|_| "service-error".to_string());
                    if let Ok(mut log) = request_log.lock() {
                        let _ = writeln!(
                            log,
                            "method={} uri={} status={} litmus={} duration_ms={}",
                            method,
                            uri,
                            status,
                            litmus_case,
                            started_at.elapsed().as_millis()
                        );
                        let _ = log.flush();
                    }
                    result
                }
            })
            .wrap(actix_web::middleware::Compress::default())
            .wrap(aster_forge_actix_middleware::security_headers::default_headers())
            .app_data(web::PayloadConfig::new(10 * 1024 * 1024))
            .app_data(web::JsonConfig::default().limit(1024 * 1024))
            .app_data(web::Data::new(state.clone()))
            .configure(move |cfg| aster_drive::webdav::configure(cfg, &webdav_config, &db))
    })
    .listen(listener)
    .map_err(|error| format!("Litmus test server failed to listen: {error}"))?
    .run();
    let handle = server.handle();
    let task = tokio::spawn(server);

    Ok(RunningWebdavServer {
        base_url: format!("http://{addr}"),
        handle,
        task,
        server_log_path,
    })
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
    .expect("Litmus test user should be inserted");
    state
        .policy_snapshot
        .set_user_policy_group(user.id, default_policy_group.id);

    let username = format!("litmus-dav-{}", uuid::Uuid::new_v4().simple());
    let password = format!("LITMUS_DAV_{}", uuid::Uuid::new_v4().simple());
    webdav_account::ActiveModel {
        user_id: Set(user.id),
        username: Set(username.clone()),
        password_hash: Set(aster_forge_crypto::hash_password(&password)
            .expect("Litmus WebDAV password should hash")),
        root_folder_id: Set(None),
        is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("Litmus WebDAV account should be inserted");

    (username, password)
}

fn resolve_litmus_wrapper() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os(LITMUS_BIN_ENV) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!(
            "{LITMUS_BIN_ENV} points to a missing file: {}",
            path.display()
        ));
    }

    if let Some(path) = find_in_path("litmus") {
        return Ok(path);
    }

    Err(format!(
        "Litmus wrapper not found. Install pinned Litmus {LITMUS_VERSION} or set {LITMUS_BIN_ENV}"
    ))
}

fn find_in_path(program: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|directory| directory.join(program))
            .find(|candidate| candidate.is_file())
    })
}

fn run_litmus_group(
    litmus_wrapper: &Path,
    workspace: &Path,
    url: &str,
    username: &str,
    password: &str,
    group: LitmusGroup,
) -> Result<LitmusProcessResult, String> {
    let stdout_path = workspace.join("stdout.log");
    let stderr_path = workspace.join("stderr.log");
    let stdout = File::create(&stdout_path)
        .map_err(|error| format!("failed to create {}: {error}", stdout_path.display()))?;
    let stderr = File::create(&stderr_path)
        .map_err(|error| format!("failed to create {}: {error}", stderr_path.display()))?;

    let mut command = Command::new(litmus_wrapper);
    command
        .args([url, username, password])
        .env("TESTS", group.name)
        .envs(group.environment.iter().copied())
        .current_dir(workspace)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .stdin(Stdio::null());
    configure_process_group(&mut command);

    let mut child = command.spawn().map_err(|error| {
        format!(
            "failed to spawn Litmus {} group with {}: {error}",
            group.name,
            litmus_wrapper.display()
        )
    })?;
    let started_at = Instant::now();
    let mut timed_out = false;

    let exit_status = loop {
        if started_at.elapsed() > group.timeout {
            timed_out = true;
            terminate_process_group(&mut child);
            break child.wait().map_err(|error| {
                format!("failed to wait for timed-out Litmus process: {error}")
            })?;
        }

        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(error) => {
                terminate_process_group(&mut child);
                return Err(format!("failed to poll Litmus process: {error}"));
            }
        }
    };

    redact_litmus_artifacts(workspace, username, password)?;
    let stdout = read_litmus_log(&stdout_path)?;
    let stderr = read_litmus_log(&stderr_path)?;

    Ok(LitmusProcessResult {
        exit_status,
        timed_out,
        stdout,
        stderr,
    })
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn terminate_process_group(child: &mut std::process::Child) {
    let process_group = format!("-{}", child.id());
    let _ = Command::new("kill")
        .args(["-TERM", &process_group])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    std::thread::sleep(Duration::from_millis(250));
    if matches!(child.try_wait(), Ok(None)) {
        let _ = Command::new("kill")
            .args(["-KILL", &process_group])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[cfg(not(unix))]
fn terminate_process_group(child: &mut std::process::Child) {
    let _ = child.kill();
}

fn redact_litmus_artifacts(workspace: &Path, username: &str, password: &str) -> Result<(), String> {
    let basic_credentials =
        base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
    let replacements = [
        (
            basic_credentials.into_bytes(),
            b"[redacted-basic-auth]".as_slice(),
        ),
        (
            password.as_bytes().to_vec(),
            b"[redacted-password]".as_slice(),
        ),
        (
            username.as_bytes().to_vec(),
            b"[redacted-username]".as_slice(),
        ),
    ];

    for filename in ["stdout.log", "stderr.log", "debug.log", "child.log"] {
        let path = workspace.join(filename);
        if !path.exists() {
            continue;
        }
        let contents = fs::read(&path)
            .map_err(|error| format!("failed to read {} for redaction: {error}", path.display()))?;
        let contents = redact_bytes(contents, &replacements);
        fs::write(&path, contents)
            .map_err(|error| format!("failed to redact {}: {error}", path.display()))?;
    }

    Ok(())
}

fn read_litmus_log(path: &Path) -> Result<String, String> {
    let contents =
        fs::read(path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    Ok(String::from_utf8_lossy(&contents).into_owned())
}

fn redact_bytes(contents: Vec<u8>, replacements: &[(Vec<u8>, &[u8])]) -> Vec<u8> {
    let mut result = contents;
    for (secret, replacement) in replacements {
        if secret.is_empty() {
            continue;
        }
        let mut redacted = Vec::with_capacity(result.len());
        let mut cursor = 0;
        while let Some(offset) = result[cursor..]
            .windows(secret.len())
            .position(|window| window == secret)
        {
            let position = cursor + offset;
            redacted.extend_from_slice(&result[cursor..position]);
            redacted.extend_from_slice(replacement);
            cursor = position + secret.len();
        }
        redacted.extend_from_slice(&result[cursor..]);
        result = redacted;
    }
    result
}

fn parse_litmus_output(stdout: &str) -> ParsedLitmusOutput {
    let mut cases = BTreeMap::new();
    let mut warnings = Vec::new();
    let mut pending_case: Option<(usize, String)> = None;
    for raw_line in stdout.lines() {
        let line = raw_line.rsplit('\r').next().unwrap_or(raw_line).trim();
        if let Some((number, remainder)) = line.split_once(". ")
            && let Ok(number) = number.trim().parse::<usize>()
        {
            let name_source = remainder
                .split_once(" WARNING:")
                .map(|(name, _)| name)
                .unwrap_or(remainder);
            let name = parse_litmus_case_name(name_source);
            if !name.is_empty() {
                pending_case = Some((number, name));
            }
            if let Some((_, message)) = remainder.split_once(" WARNING:")
                && let Some((_, name)) = &pending_case
            {
                warnings.push(LitmusWarning {
                    test: name.clone(),
                    message: message.trim().to_string(),
                });
            }

            if let Some((status_position, marker, status)) = parse_litmus_status(remainder) {
                let name = parse_litmus_case_name(&remainder[..status_position]);
                if !name.is_empty() {
                    cases.insert(
                        number,
                        LitmusCaseResult {
                            number,
                            name,
                            status,
                            detail: parse_litmus_detail(remainder, status_position, marker),
                        },
                    );
                    pending_case = None;
                }
            }
            continue;
        }

        if let Some(message) = line.strip_prefix("WARNING:")
            && let Some((_, name)) = &pending_case
        {
            warnings.push(LitmusWarning {
                test: name.clone(),
                message: message.trim().to_string(),
            });
            continue;
        }

        if let Some((status_position, marker, status)) = parse_litmus_status(line)
            && let Some((number, name)) = pending_case.take()
        {
            cases.insert(
                number,
                LitmusCaseResult {
                    number,
                    name,
                    status,
                    detail: parse_litmus_detail(line, status_position, marker),
                },
            );
        }
    }

    ParsedLitmusOutput {
        cases: cases.into_values().collect(),
        warnings,
    }
}

fn parse_litmus_status(line: &str) -> Option<(usize, &'static str, LitmusCaseStatus)> {
    [
        (" SKIPPED", LitmusCaseStatus::Skipped),
        (" XFAIL", LitmusCaseStatus::ExpectedFailure),
        (" FAIL", LitmusCaseStatus::Failed),
        (" pass", LitmusCaseStatus::Passed),
    ]
    .into_iter()
    .filter_map(|(marker, status)| line.find(marker).map(|position| (position, marker, status)))
    .min_by_key(|(position, _, _)| *position)
}

fn parse_litmus_case_name(value: &str) -> String {
    value.trim().trim_end_matches('.').trim().to_string()
}

fn parse_litmus_detail(line: &str, status_position: usize, marker: &str) -> String {
    line[status_position + marker.len()..]
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .to_string()
}

fn parse_baseline(contents: &str) -> Result<Vec<BaselineEntry>, String> {
    let known_groups: BTreeSet<&str> = TEST_GROUPS
        .iter()
        .chain(resource_litmus::TEST_GROUPS)
        .chain(security_policy_litmus::TEST_GROUPS)
        .map(|group| group.name)
        .collect();
    let mut entries = Vec::new();
    let mut keys = BTreeSet::new();

    for (line_index, raw_line) in contents.lines().enumerate() {
        let line_number = line_index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let fields: Vec<&str> = line.split('|').map(str::trim).collect();
        if fields.len() != 5 {
            return Err(format!(
                "Litmus baseline line {line_number} must have 5 pipe-separated fields"
            ));
        }
        let [group, status, test, tracking_issue, rationale] = fields.as_slice() else {
            unreachable!();
        };
        if !known_groups.contains(group) {
            return Err(format!(
                "Litmus baseline line {line_number} has unknown group `{group}`"
            ));
        }
        let status = match *status {
            "FAIL" => LitmusCaseStatus::Failed,
            "SKIPPED" => LitmusCaseStatus::Skipped,
            "WARNING" => LitmusCaseStatus::Warning,
            value => {
                return Err(format!(
                    "Litmus baseline line {line_number} has unsupported status `{value}`"
                ));
            }
        };
        if test.is_empty() || rationale.is_empty() {
            return Err(format!(
                "Litmus baseline line {line_number} requires a test name and rationale"
            ));
        }
        const ISSUE_PREFIX: &str = "https://github.com/AsterCommunity/AsterDrive/issues/";
        if !tracking_issue.starts_with(ISSUE_PREFIX) || tracking_issue.ends_with("/421") {
            return Err(format!(
                "Litmus baseline line {line_number} must reference an independent AsterDrive issue"
            ));
        }

        let key = BaselineKey {
            group: (*group).to_string(),
            status,
            test: (*test).to_string(),
        };
        if !keys.insert(key.clone()) {
            return Err(format!(
                "Litmus baseline line {line_number} duplicates {} {} {}",
                key.group,
                key.status.baseline_name().unwrap_or("unknown"),
                key.test
            ));
        }
        entries.push(BaselineEntry {
            key,
            tracking_issue: (*tracking_issue).to_string(),
            rationale: (*rationale).to_string(),
        });
    }

    Ok(entries)
}

fn evaluate_litmus_result(
    group: LitmusGroup,
    process: &LitmusProcessResult,
    output: &ParsedLitmusOutput,
    baseline: &[BaselineEntry],
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut observed_differences = Vec::new();
    let mut accepted_differences = Vec::new();
    let mut errors = Vec::new();
    if process.timed_out {
        errors.push(format!(
            "Litmus {} timed out after {:?}",
            group.name, group.timeout
        ));
    }
    if output.cases.len() != group.expected_test_count {
        errors.push(format!(
            "Litmus {} executed {} tests, expected {} for version {}",
            group.name,
            output.cases.len(),
            group.expected_test_count,
            LITMUS_VERSION
        ));
    }

    let expected: BTreeMap<BaselineKey, &BaselineEntry> = baseline
        .iter()
        .filter(|entry| entry.key.group == group.name)
        .map(|entry| (entry.key.clone(), entry))
        .collect();
    let mut observed: BTreeSet<BaselineKey> = output
        .cases
        .iter()
        .filter_map(|case| {
            case.status.baseline_name().map(|_| BaselineKey {
                group: group.name.to_string(),
                status: case.status,
                test: case.name.clone(),
            })
        })
        .collect();
    observed.extend(output.warnings.iter().map(|warning| BaselineKey {
        group: group.name.to_string(),
        status: LitmusCaseStatus::Warning,
        test: warning.test.clone(),
    }));

    for difference in &observed {
        observed_differences.push(format!(
            "{} {}",
            difference.status.baseline_name().unwrap_or("difference"),
            difference.test
        ));
        if matches!(group.evaluation_mode, LitmusEvaluationMode::Probe) {
            continue;
        }
        if let Some(entry) = expected.get(difference) {
            accepted_differences.push(format!(
                "{} {} tracked by {}: {}",
                difference.status.baseline_name().unwrap_or("unknown"),
                difference.test,
                entry.tracking_issue,
                entry.rationale
            ));
        } else {
            errors.push(format!(
                "unexpected {} in Litmus {}: {}",
                difference.status.baseline_name().unwrap_or("difference"),
                group.name,
                difference.test
            ));
        }
    }
    if matches!(group.evaluation_mode, LitmusEvaluationMode::Baseline) {
        for entry in expected.values() {
            if !observed.contains(&entry.key) {
                errors.push(format!(
                    "stale Litmus baseline entry: {} {} no longer occurs; remove {}",
                    entry.key.status.baseline_name().unwrap_or("difference"),
                    entry.key.test,
                    entry.tracking_issue
                ));
            }
        }
    } else if !expected.is_empty() {
        errors.push(format!(
            "Litmus probe group {} must not use the conformance baseline",
            group.name
        ));
    }

    let has_failures = output
        .cases
        .iter()
        .any(|case| case.status == LitmusCaseStatus::Failed);
    if !process.exit_status.success() && !has_failures && !process.timed_out {
        errors.push(format!(
            "Litmus {} exited with {:?} without reporting a failed test\nstderr:\n{}",
            group.name,
            process.exit_status.code(),
            process.stderr
        ));
    }
    if process.exit_status.success() && has_failures {
        errors.push(format!(
            "Litmus {} reported failed tests but returned a successful exit status",
            group.name
        ));
    }

    (observed_differences, accepted_differences, errors)
}

fn write_report(workspace: &Path, report: &LitmusReport) -> Result<(), String> {
    let path = workspace.join("result.json");
    let contents = serde_json::to_string_pretty(report)
        .map_err(|error| format!("failed to serialize Litmus report: {error}"))?;
    fs::write(&path, format!("{contents}\n"))
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn format_failure(
    group: LitmusGroup,
    process: &LitmusProcessResult,
    errors: &[String],
    workspace: &Path,
) -> String {
    let evaluation = match group.evaluation_mode {
        LitmusEvaluationMode::Baseline => "baseline",
        LitmusEvaluationMode::Probe => "probe",
    };
    let mut message = format!(
        "Litmus `{}` {evaluation} failed; artifacts: {}\n",
        group.name,
        workspace.display()
    );
    for error in errors {
        message.push_str(&format!("- {error}\n"));
    }
    message.push_str("\n--- stdout ---\n");
    message.push_str(&process.stdout);
    if !process.stderr.is_empty() {
        message.push_str("\n--- stderr ---\n");
        message.push_str(&process.stderr);
    }
    message
}

async fn run_single_litmus_test(state: PrimaryAppState, group: LitmusGroup) -> Result<(), String> {
    let litmus_wrapper = resolve_litmus_wrapper()?;
    let baseline = parse_baseline(BASELINE)?;
    let workspace = TestWorkspace::create(group.name)?;
    let (username, password) = seed_real_webdav_account(&state).await;
    let server = start_real_webdav_server(state, &workspace.path).await?;
    let webdav_url = format!("{}/webdav/", server.base_url);

    let litmus_join_result = tokio::task::spawn_blocking({
        let litmus_wrapper = litmus_wrapper.clone();
        let workspace = workspace.path.clone();
        let username = username.clone();
        let password = password.clone();
        move || {
            run_litmus_group(
                &litmus_wrapper,
                &workspace,
                &webdav_url,
                &username,
                &password,
                group,
            )
        }
    })
    .await;
    let server_result = server.stop().await;
    let litmus_result =
        litmus_join_result.map_err(|error| format!("Litmus blocking task failed to join: {error}"));

    let process = match litmus_result {
        Ok(Ok(result)) => result,
        Ok(Err(error)) | Err(error) => {
            return match server_result {
                Ok(()) => Err(error),
                Err(server_error) => Err(format!("{error}\n{server_error}")),
            };
        }
    };
    server_result?;

    let output = parse_litmus_output(&process.stdout);
    let (observed_differences, accepted_differences, errors) =
        evaluate_litmus_result(group, &process, &output, &baseline);
    let report = LitmusReport {
        litmus_version: LITMUS_VERSION,
        group: group.name.to_string(),
        evaluation_mode: group.evaluation_mode,
        environment: group.environment.iter().copied().collect(),
        timeout_seconds: group.timeout.as_secs(),
        expected_test_count: group.expected_test_count,
        observed_test_count: output.cases.len(),
        exit_code: process.exit_status.code(),
        timed_out: process.timed_out,
        cases: output.cases,
        warnings: output.warnings,
        observed_differences,
        accepted_differences,
        errors: errors.clone(),
    };
    write_report(&workspace.path, &report)?;

    if errors.is_empty() {
        match group.evaluation_mode {
            LitmusEvaluationMode::Baseline => println!(
                "[litmus/{}] {} tests completed; {} known differences",
                group.name,
                report.observed_test_count,
                report.accepted_differences.len()
            ),
            LitmusEvaluationMode::Probe => println!(
                "[litmus/{}] {} probe cases completed; {} policy differences observed",
                group.name,
                report.observed_test_count,
                report.observed_differences.len()
            ),
        }
        Ok(())
    } else {
        Err(format_failure(group, &process, &errors, &workspace.path))
    }
}

async fn run_group(group: LitmusGroup) {
    let state = common::setup().await;
    if let Err(error) = run_single_litmus_test(state, group).await {
        panic!("{error}");
    }
}

#[test]
fn litmus_output_parser_distinguishes_all_statuses_and_carriage_returns() {
    let output = concat!(
        "-> running `locks':\n",
        "\r 0. init.................... \r 0. init.................... pass\n",
        "\r 1. begin................... \r 1. begin................... FAIL (expected 200)\n",
        "\r 2. conditional............. \r 2. conditional............. SKIPPED (missing lock token)\n",
        "\r 3. known-bug............... \r 3. known-bug............... XFAIL\n",
        "\r 4. warning-case............ WARNING: legacy response\n",
        "    ...................... pass (with 1 warning)\n",
        "\r 5. detail-word............. FAIL (server reported pass after the failure)\n",
    );

    assert_eq!(
        parse_litmus_output(output),
        ParsedLitmusOutput {
            cases: vec![
                LitmusCaseResult {
                    number: 0,
                    name: "init".to_string(),
                    status: LitmusCaseStatus::Passed,
                    detail: String::new(),
                },
                LitmusCaseResult {
                    number: 1,
                    name: "begin".to_string(),
                    status: LitmusCaseStatus::Failed,
                    detail: "expected 200".to_string(),
                },
                LitmusCaseResult {
                    number: 2,
                    name: "conditional".to_string(),
                    status: LitmusCaseStatus::Skipped,
                    detail: "missing lock token".to_string(),
                },
                LitmusCaseResult {
                    number: 3,
                    name: "known-bug".to_string(),
                    status: LitmusCaseStatus::ExpectedFailure,
                    detail: String::new(),
                },
                LitmusCaseResult {
                    number: 4,
                    name: "warning-case".to_string(),
                    status: LitmusCaseStatus::Passed,
                    detail: "with 1 warning".to_string(),
                },
                LitmusCaseResult {
                    number: 5,
                    name: "detail-word".to_string(),
                    status: LitmusCaseStatus::Failed,
                    detail: "server reported pass after the failure".to_string(),
                },
            ],
            warnings: vec![LitmusWarning {
                test: "warning-case".to_string(),
                message: "legacy response".to_string(),
            }],
        }
    );
}

#[cfg(unix)]
#[test]
fn security_policy_probe_records_failures_without_creating_conformance_errors() {
    use std::os::unix::process::ExitStatusExt;

    let group = LitmusGroup {
        name: "protected",
        expected_test_count: 1,
        timeout: DEFAULT_SUITE_TIMEOUT,
        environment: &[("TEST_PROTECTED", ".DAV")],
        evaluation_mode: LitmusEvaluationMode::Probe,
    };
    let process = LitmusProcessResult {
        exit_status: ExitStatus::from_raw(1 << 8),
        timed_out: false,
        stdout: String::new(),
        stderr: String::new(),
    };
    let output = ParsedLitmusOutput {
        cases: vec![LitmusCaseResult {
            number: 0,
            name: "put".to_string(),
            status: LitmusCaseStatus::Failed,
            detail: "protected path accepted the write".to_string(),
        }],
        warnings: Vec::new(),
    };

    let (observed, accepted, errors) = evaluate_litmus_result(group, &process, &output, &[]);

    assert_eq!(observed, vec!["FAIL put"]);
    assert!(accepted.is_empty());
    assert!(errors.is_empty());
}

#[test]
fn litmus_baseline_requires_independent_tracking_issues() {
    let umbrella =
        "locks|FAIL|lockdiscovery|https://github.com/AsterCommunity/AsterDrive/issues/421|umbrella";
    let error = parse_baseline(umbrella).expect_err("umbrella issue should not satisfy baseline");
    assert!(error.contains("independent AsterDrive issue"));

    let child = "locks|FAIL|lockdiscovery|https://github.com/AsterCommunity/AsterDrive/issues/423|tracked gap";
    let parsed = parse_baseline(child).expect("child issue baseline should parse");
    assert_eq!(parsed.len(), 1);
}

#[test]
fn litmus_artifact_redaction_preserves_non_utf8_bytes() {
    let redacted = redact_bytes(
        b"prefix\xffsecret\x00suffix".to_vec(),
        &[(b"secret".to_vec(), b"[redacted]".as_slice())],
    );
    assert_eq!(redacted, b"prefix\xff[redacted]\x00suffix");
}

#[test]
fn committed_litmus_baseline_is_well_formed() {
    parse_baseline(BASELINE).expect("committed Litmus baseline should be valid");
}

#[actix_web::test]
#[ignore = "requires pinned Litmus 0.18; run via the WebDAV compatibility workflow"]
async fn test_litmus_basic() {
    run_group(TEST_GROUPS[0]).await;
}

#[actix_web::test]
#[ignore = "requires pinned Litmus 0.18; run via the WebDAV compatibility workflow"]
async fn test_litmus_copymove() {
    run_group(TEST_GROUPS[1]).await;
}

#[actix_web::test]
#[ignore = "requires pinned Litmus 0.18; run via the WebDAV compatibility workflow"]
async fn test_litmus_props() {
    run_group(TEST_GROUPS[2]).await;
}

#[actix_web::test]
#[ignore = "requires pinned Litmus 0.18; run via the WebDAV compatibility workflow"]
async fn test_litmus_locks() {
    run_group(TEST_GROUPS[3]).await;
}

#[actix_web::test]
#[ignore = "requires pinned Litmus 0.18; run via the WebDAV compatibility workflow"]
async fn test_litmus_http() {
    run_group(TEST_GROUPS[4]).await;
}
