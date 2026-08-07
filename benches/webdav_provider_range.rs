use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use aster_drive::storage::drivers::local::LocalDriver;
use aster_drive::storage::drivers::onedrive::{
    MicrosoftGraphClient, MicrosoftGraphClientConfig, OneDriveDriver,
};
use aster_drive::storage::drivers::remote::{RemoteDriver, RemoteDriverConfig};
use aster_drive::storage::drivers::s3::{
    S3Driver, S3DriverConfig, S3DriverOptions, S3StaticCredentials,
};
use aster_drive::storage::drivers::sftp::{SftpDriver, SftpDriverConfig, SftpStaticCredentials};
use aster_drive::storage::remote_protocol::RemoteStorageCapabilities;
use aster_drive_model::entities::managed_follower;
use aster_drive_model::types::RemoteNodeTransportMode;
use aster_drive_storage::{BlobMetadata, StorageDriver};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};

type AnyError = Box<dyn Error + Send + Sync>;
type BenchResult<T> = Result<T, AnyError>;

const ARTIFACT_SCHEMA_VERSION: u32 = 1;
const DEFAULT_PAYLOAD_BYTES: u64 = 5 * 1024 * 1024;
const DEFAULT_RANGE_BYTES: u64 = 256 * 1024;
const MAX_PAYLOAD_BYTES: u64 = 1024 * 1024 * 1024;
const DEFAULT_WARMUPS: usize = 3;
const DEFAULT_SAMPLES: usize = 20;
const DEFAULT_OBJECT_PATH: &str = "webdav-provider-range-v1.bin";
const READ_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone)]
struct BenchConfig {
    provider: String,
    provider_required: bool,
    payload_bytes: u64,
    range_bytes: u64,
    warmups: usize,
    samples: usize,
    object_path: String,
    output_path: PathBuf,
    cleanup_fixture: bool,
    baseline_path: Option<PathBuf>,
    baseline_profile: Option<String>,
    fail_on_regression: bool,
}

impl BenchConfig {
    fn from_env() -> BenchResult<Self> {
        let payload_bytes = env_u64("ASTER_BENCH_RANGE_PAYLOAD_BYTES", DEFAULT_PAYLOAD_BYTES)?;
        let range_bytes = env_u64("ASTER_BENCH_RANGE_BYTES", DEFAULT_RANGE_BYTES)?;
        if range_bytes < 2 {
            return Err(
                "ASTER_BENCH_RANGE_BYTES must be at least 2 so multi-range windows stay non-empty"
                    .into(),
            );
        }
        if payload_bytes > MAX_PAYLOAD_BYTES {
            return Err(format!(
                "ASTER_BENCH_RANGE_PAYLOAD_BYTES exceeds the bounded benchmark fixture limit of {MAX_PAYLOAD_BYTES} bytes"
            )
            .into());
        }
        if payload_bytes < range_bytes.saturating_mul(6) {
            return Err(format!(
                "ASTER_BENCH_RANGE_PAYLOAD_BYTES must be at least six times ASTER_BENCH_RANGE_BYTES (payload={payload_bytes}, range={range_bytes})"
            )
            .into());
        }

        Ok(Self {
            provider: env_string("ASTER_BENCH_RANGE_PROVIDER", "local").to_ascii_lowercase(),
            provider_required: env_bool("ASTER_BENCH_RANGE_PROVIDER_REQUIRED", false)?,
            payload_bytes,
            range_bytes,
            warmups: env_usize("ASTER_BENCH_RANGE_WARMUPS", DEFAULT_WARMUPS)?,
            samples: env_usize("ASTER_BENCH_RANGE_SAMPLES", DEFAULT_SAMPLES)?.max(1),
            object_path: env_string("ASTER_BENCH_RANGE_OBJECT_PATH", DEFAULT_OBJECT_PATH),
            output_path: PathBuf::from(env_string(
                "ASTER_BENCH_RANGE_OUTPUT",
                "tests/performance/results/webdav-provider-range/artifact.json",
            )),
            cleanup_fixture: env_bool("ASTER_BENCH_RANGE_CLEANUP", true)?,
            baseline_path: env_optional("ASTER_BENCH_RANGE_BASELINE").map(PathBuf::from),
            baseline_profile: env_optional("ASTER_BENCH_RANGE_BASELINE_PROFILE"),
            fail_on_regression: env_bool("ASTER_BENCH_RANGE_FAIL_ON_REGRESSION", false)?,
        })
    }

    fn scenario_specs(&self) -> BTreeMap<String, ScenarioSpec> {
        let half = self.range_bytes / 2;
        let mut scenarios = BTreeMap::new();
        scenarios.insert(
            "full_get".to_string(),
            ScenarioSpec::Full {
                selected_bytes: self.payload_bytes,
            },
        );
        scenarios.insert(
            "single_range_early".to_string(),
            ScenarioSpec::Single {
                offset: 0,
                length: self.range_bytes,
            },
        );
        scenarios.insert(
            "single_range_late".to_string(),
            ScenarioSpec::Single {
                offset: self.payload_bytes - self.range_bytes,
                length: self.range_bytes,
            },
        );
        scenarios.insert(
            "multi_range_disjoint".to_string(),
            ScenarioSpec::Multi {
                ranges: [
                    ByteWindow {
                        offset: self.range_bytes.saturating_mul(2),
                        length: half,
                    },
                    ByteWindow {
                        offset: self.payload_bytes - self.range_bytes.saturating_mul(3),
                        length: self.range_bytes - half,
                    },
                ],
            },
        );
        scenarios
    }
}

#[derive(Debug, Clone, Copy)]
struct ByteWindow {
    offset: u64,
    length: u64,
}

#[derive(Debug, Clone)]
enum ScenarioSpec {
    Full { selected_bytes: u64 },
    Single { offset: u64, length: u64 },
    Multi { ranges: [ByteWindow; 2] },
}

enum ProviderBuild {
    Ready(ProviderFixture),
    Skipped {
        provider: String,
        reason: String,
        prerequisites: Vec<String>,
        config_summary: Value,
    },
}

struct ProviderFixture {
    provider: String,
    driver: Box<dyn StorageDriver>,
    requests_per_backend_call: u64,
    config_summary: Value,
    cleanup_root: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct BenchmarkArtifact {
    schema_version: u32,
    generated_at: String,
    git_revision: Option<String>,
    git_dirty: Option<bool>,
    status: &'static str,
    provider: String,
    skip_reason: Option<String>,
    prerequisites: Vec<String>,
    fixture: FixtureSummary,
    sampling: SamplingSummary,
    machine: MachineSummary,
    provider_config: Value,
    scenarios: BTreeMap<String, ScenarioReport>,
    fallback: Option<ScenarioReport>,
    baseline: BaselineComparison,
}

#[derive(Debug, Serialize)]
struct FixtureSummary {
    object_path: String,
    payload_bytes: u64,
    range_bytes: u64,
    content_pattern: &'static str,
    cleanup_requested: bool,
}

#[derive(Debug, Serialize)]
struct SamplingSummary {
    warmups: usize,
    samples: usize,
    read_buffer_bytes: usize,
}

#[derive(Debug, Serialize, Clone)]
struct MachineSummary {
    profile: Option<String>,
    build_profile: &'static str,
    os: String,
    architecture: String,
    cpu_model: Option<String>,
    logical_cpus: usize,
    rustc: Option<String>,
    kernel: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScenarioReport {
    selected_bytes: u64,
    expected_backend_calls: u64,
    expected_backend_requests: u64,
    expected_prefix_skip_bytes: u64,
    samples: Vec<Sample>,
    summary: ScenarioStatistics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Sample {
    open_ms: f64,
    ttfb_ms: f64,
    read_ms: f64,
    total_ms: f64,
    throughput_bytes_per_second: f64,
    backend_call_count: u64,
    backend_request_count: u64,
    actual_read_bytes: u64,
    prefix_skip_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScenarioStatistics {
    open_ms: Distribution,
    ttfb_ms: Distribution,
    read_ms: Distribution,
    total_ms: Distribution,
    throughput_bytes_per_second: Distribution,
    backend_call_count: Distribution,
    backend_request_count: Distribution,
    actual_read_bytes: Distribution,
    prefix_skip_bytes: Distribution,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Distribution {
    min: f64,
    p50: f64,
    p95: f64,
    p99: f64,
    max: f64,
}

#[derive(Debug, Serialize)]
struct BaselineComparison {
    baseline_path: Option<String>,
    profile: Option<String>,
    status: &'static str,
    reason: Option<String>,
    policy: Option<RegressionPolicy>,
    scenarios: BTreeMap<String, ScenarioComparison>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegressionPolicy {
    ttfb_p95_max_ratio: f64,
    throughput_p50_min_ratio: f64,
}

#[derive(Debug, Serialize)]
struct ScenarioComparison {
    ttfb_p95_ratio: f64,
    throughput_p50_ratio: f64,
    regressed: bool,
}

#[derive(Debug, Deserialize)]
struct BaselineFile {
    schema_version: u32,
    regression_policy: RegressionPolicy,
    profiles: Vec<BaselineProfile>,
}

#[derive(Debug, Deserialize)]
struct BaselineProfile {
    profile: String,
    provider: String,
    payload_bytes: u64,
    range_bytes: u64,
    sampling: BaselineSampling,
    machine: BaselineMachine,
    scenarios: BTreeMap<String, BaselineScenario>,
}

#[derive(Debug, Deserialize)]
struct BaselineSampling {
    warmups: usize,
    samples: usize,
    read_buffer_bytes: usize,
}

#[derive(Debug, Deserialize)]
struct BaselineMachine {
    build_profile: String,
}

#[derive(Debug, Deserialize)]
struct BaselineScenario {
    ttfb_p95_ms: f64,
    throughput_p50_bytes_per_second: f64,
}

#[tokio::main]
async fn main() -> BenchResult<()> {
    let config = BenchConfig::from_env()?;
    let machine = machine_summary(config.baseline_profile.clone());
    let fixture_summary = FixtureSummary {
        object_path: config.object_path.clone(),
        payload_bytes: config.payload_bytes,
        range_bytes: config.range_bytes,
        content_pattern: "byte[index % 251]",
        cleanup_requested: config.cleanup_fixture,
    };
    let sampling = SamplingSummary {
        warmups: config.warmups,
        samples: config.samples,
        read_buffer_bytes: READ_BUFFER_BYTES,
    };

    let provider_fixture = match build_provider(&config).await? {
        ProviderBuild::Ready(provider_fixture) => provider_fixture,
        ProviderBuild::Skipped {
            provider,
            reason,
            prerequisites,
            config_summary,
        } => {
            let artifact = BenchmarkArtifact {
                schema_version: ARTIFACT_SCHEMA_VERSION,
                generated_at: chrono::Utc::now().to_rfc3339(),
                git_revision: command_output("git", &["rev-parse", "HEAD"]),
                git_dirty: git_dirty(),
                status: "skipped",
                provider,
                skip_reason: Some(reason.clone()),
                prerequisites,
                fixture: fixture_summary,
                sampling,
                machine,
                provider_config: config_summary,
                scenarios: BTreeMap::new(),
                fallback: None,
                baseline: baseline_not_compared(&config, "provider benchmark was skipped"),
            };
            write_artifact(&config.output_path, &artifact).await?;
            if config.provider_required {
                return Err(reason.into());
            }
            return Ok(());
        }
    };

    let benchmark_result = async {
        let payload = deterministic_payload(config.payload_bytes)?;
        provider_fixture
            .driver
            .put(&config.object_path, &payload)
            .await?;

        let scenario_specs = config.scenario_specs();
        let mut scenarios = BTreeMap::new();
        for (name, scenario) in &scenario_specs {
            for _ in 0..config.warmups {
                let _ = run_provider_scenario(
                    provider_fixture.driver.as_ref(),
                    provider_fixture.requests_per_backend_call,
                    &config.object_path,
                    scenario,
                )
                .await?;
            }
            let mut samples = Vec::with_capacity(config.samples);
            for _ in 0..config.samples {
                samples.push(
                    run_provider_scenario(
                        provider_fixture.driver.as_ref(),
                        provider_fixture.requests_per_backend_call,
                        &config.object_path,
                        scenario,
                    )
                    .await?,
                );
            }
            scenarios.insert(name.clone(), report_for_scenario(scenario, samples));
        }

        let fallback = run_fallback_benchmark(&config, Arc::<[u8]>::from(payload)).await?;
        let baseline = compare_baseline(&config, &scenarios).await?;
        let baseline_regressed = baseline
            .scenarios
            .values()
            .any(|scenario| scenario.regressed);

        let artifact = BenchmarkArtifact {
            schema_version: ARTIFACT_SCHEMA_VERSION,
            generated_at: chrono::Utc::now().to_rfc3339(),
            git_revision: command_output("git", &["rev-parse", "HEAD"]),
            git_dirty: git_dirty(),
            status: "completed",
            provider: provider_fixture.provider.clone(),
            skip_reason: None,
            prerequisites: Vec::new(),
            fixture: fixture_summary,
            sampling,
            machine,
            provider_config: provider_fixture.config_summary.clone(),
            scenarios,
            fallback: Some(fallback),
            baseline,
        };
        write_artifact(&config.output_path, &artifact).await?;

        if config.fail_on_regression && baseline_regressed {
            return Err(
                "provider Range benchmark exceeded the selected versioned baseline policy".into(),
            );
        }
        Ok(())
    }
    .await;

    let cleanup_result = if config.cleanup_fixture {
        cleanup_provider_fixture(&provider_fixture, &config.object_path).await
    } else {
        Ok(())
    };
    finish_benchmark(benchmark_result, cleanup_result)
}

async fn cleanup_provider_fixture(
    provider_fixture: &ProviderFixture,
    object_path: &str,
) -> BenchResult<()> {
    let mut cleanup_errors = Vec::new();
    if let Err(error) = provider_fixture.driver.delete(object_path).await {
        cleanup_errors.push(format!(
            "failed to delete provider fixture '{object_path}': {error}"
        ));
    }
    if let Some(root) = &provider_fixture.cleanup_root
        && let Err(error) = tokio::fs::remove_dir_all(root).await
    {
        cleanup_errors.push(format!(
            "failed to remove local provider fixture root '{}': {error}",
            root.display()
        ));
    }

    if cleanup_errors.is_empty() {
        Ok(())
    } else {
        Err(cleanup_errors.join("; ").into())
    }
}

fn finish_benchmark<T>(
    benchmark_result: BenchResult<T>,
    cleanup_result: BenchResult<()>,
) -> BenchResult<T> {
    match (benchmark_result, cleanup_result) {
        (Ok(value), Ok(())) => Ok(value),
        (Ok(_), Err(cleanup_error)) => Err(cleanup_error),
        (Err(benchmark_error), Ok(())) => Err(benchmark_error),
        (Err(benchmark_error), Err(cleanup_error)) => {
            eprintln!("WebDAV provider Range fixture cleanup failed: {cleanup_error}");
            Err(benchmark_error)
        }
    }
}

async fn build_provider(config: &BenchConfig) -> BenchResult<ProviderBuild> {
    match config.provider.as_str() {
        "local" => build_local_provider().await,
        "s3" | "s3-compatible" | "s3_compatible" => build_s3_provider(),
        "onedrive" | "one-drive" => build_onedrive_provider(),
        "sftp" => build_sftp_provider(),
        "remote" => build_remote_provider(),
        provider => Ok(ProviderBuild::Skipped {
            provider: provider.to_string(),
            reason: format!(
                "unknown provider '{provider}'; expected local, s3, onedrive, sftp, or remote"
            ),
            prerequisites: vec![
                "ASTER_BENCH_RANGE_PROVIDER=local|s3|onedrive|sftp|remote".to_string(),
            ],
            config_summary: json!({}),
        }),
    }
}

async fn build_local_provider() -> BenchResult<ProviderBuild> {
    let root = std::env::temp_dir().join(format!(
        "asterdrive-webdav-provider-range-{}",
        uuid::Uuid::new_v4()
    ));
    tokio::fs::create_dir_all(&root).await?;
    let root_string = root.to_string_lossy().into_owned();
    let driver = LocalDriver::new(&root_string)?;
    Ok(ProviderBuild::Ready(ProviderFixture {
        provider: "local".to_string(),
        driver: Box::new(driver),
        requests_per_backend_call: 1,
        config_summary: json!({
            "storage_root_kind": "temporary_directory",
            "request_count_contract": "one local file open per backend call",
        }),
        cleanup_root: Some(root),
    }))
}

fn build_s3_provider() -> BenchResult<ProviderBuild> {
    let required = [
        "ASTER_BENCH_S3_ENDPOINT",
        "ASTER_BENCH_S3_BUCKET",
        "ASTER_BENCH_S3_ACCESS_KEY",
        "ASTER_BENCH_S3_SECRET_KEY",
    ];
    if let Some(skipped) = missing_provider_env("s3", &required) {
        return Ok(skipped);
    }
    let endpoint = env_required("ASTER_BENCH_S3_ENDPOINT")?;
    let bucket = env_required("ASTER_BENCH_S3_BUCKET")?;
    let region = env_string("ASTER_BENCH_S3_REGION", "us-east-1");
    let base_path = env_string("ASTER_BENCH_S3_BASE_PATH", "asterdrive-range-benchmark");
    let path_style = env_bool("ASTER_BENCH_S3_PATH_STYLE", true)?;
    let driver = S3Driver::new(
        S3DriverConfig {
            endpoint: endpoint.clone(),
            bucket: bucket.clone(),
            base_path: base_path.clone(),
            region: region.clone(),
            path_style,
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(120),
            operation_timeout: Duration::from_secs(180),
        },
        S3StaticCredentials {
            access_key: env_required("ASTER_BENCH_S3_ACCESS_KEY")?,
            secret_key: env_required("ASTER_BENCH_S3_SECRET_KEY")?,
        },
        S3DriverOptions::default(),
        |builder| builder,
    )?;
    Ok(ProviderBuild::Ready(ProviderFixture {
        provider: "s3".to_string(),
        driver: Box::new(driver),
        requests_per_backend_call: 1,
        config_summary: json!({
            "endpoint_kind": "configured_s3_compatible",
            "bucket_configured": !bucket.is_empty(),
            "region": region,
            "base_path_kind": "benchmark_fixture",
            "path_style": path_style,
            "fixture_identifiers_redacted": true,
            "request_count_contract": "one successful no-retry GetObject request per backend call",
        }),
        cleanup_root: None,
    }))
}

fn build_onedrive_provider() -> BenchResult<ProviderBuild> {
    let required = [
        "ASTER_BENCH_ONEDRIVE_ACCESS_TOKEN",
        "ASTER_BENCH_ONEDRIVE_DRIVE_ID",
        "ASTER_BENCH_ONEDRIVE_ROOT_ITEM_ID",
    ];
    if let Some(skipped) = missing_provider_env("onedrive", &required) {
        return Ok(skipped);
    }
    let graph_base_url = env_string(
        "ASTER_BENCH_ONEDRIVE_GRAPH_BASE_URL",
        "https://graph.microsoft.com",
    );
    let drive_id = env_required("ASTER_BENCH_ONEDRIVE_DRIVE_ID")?;
    let root_item_id = env_required("ASTER_BENCH_ONEDRIVE_ROOT_ITEM_ID")?;
    let base_path = env_string(
        "ASTER_BENCH_ONEDRIVE_BASE_PATH",
        "asterdrive-range-benchmark",
    );
    let client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(
        graph_base_url.clone(),
        env_required("ASTER_BENCH_ONEDRIVE_ACCESS_TOKEN")?,
    ))?;
    let driver = OneDriveDriver::new(
        client,
        drive_id.clone(),
        root_item_id.clone(),
        base_path.clone(),
        10 * 1024 * 1024,
    );
    Ok(ProviderBuild::Ready(ProviderFixture {
        provider: "onedrive".to_string(),
        driver: Box::new(driver),
        requests_per_backend_call: 2,
        config_summary: json!({
            "graph_endpoint_kind": if graph_base_url == "https://graph.microsoft.com" { "microsoft_graph" } else { "custom_graph_compatible" },
            "drive_id_configured": !drive_id.is_empty(),
            "root_item_id_configured": !root_item_id.is_empty(),
            "base_path_kind": "benchmark_fixture",
            "fixture_identifiers_redacted": true,
            "request_count_contract": "one Graph /content request plus one download redirect request per backend call",
        }),
        cleanup_root: None,
    }))
}

fn build_sftp_provider() -> BenchResult<ProviderBuild> {
    let required = [
        "ASTER_BENCH_SFTP_ENDPOINT",
        "ASTER_BENCH_SFTP_USERNAME",
        "ASTER_BENCH_SFTP_PASSWORD",
        "ASTER_BENCH_SFTP_HOST_KEY_FINGERPRINT",
    ];
    if let Some(skipped) = missing_provider_env("sftp", &required) {
        return Ok(skipped);
    }
    let endpoint = env_required("ASTER_BENCH_SFTP_ENDPOINT")?;
    let base_path = env_string("ASTER_BENCH_SFTP_BASE_PATH", "asterdrive-range-benchmark");
    let host_key_fingerprint = env_required("ASTER_BENCH_SFTP_HOST_KEY_FINGERPRINT")?;
    let driver = SftpDriver::new(
        SftpDriverConfig {
            endpoint: endpoint.clone(),
            base_path: base_path.clone(),
            host_key_fingerprint: Some(host_key_fingerprint.clone()),
        },
        SftpStaticCredentials {
            username: env_required("ASTER_BENCH_SFTP_USERNAME")?,
            password: env_required("ASTER_BENCH_SFTP_PASSWORD")?,
        },
    )?;
    Ok(ProviderBuild::Ready(ProviderFixture {
        provider: "sftp".to_string(),
        driver: Box::new(driver),
        requests_per_backend_call: 1,
        config_summary: json!({
            "endpoint_configured": !endpoint.is_empty(),
            "base_path_kind": "benchmark_fixture",
            "host_key_pinning": !host_key_fingerprint.is_empty(),
            "fixture_identifiers_redacted": true,
            "request_count_contract": "one SFTP file open per backend call after connection warmup",
        }),
        cleanup_root: None,
    }))
}

fn build_remote_provider() -> BenchResult<ProviderBuild> {
    let required = [
        "ASTER_BENCH_REMOTE_BASE_URL",
        "ASTER_BENCH_REMOTE_ACCESS_KEY",
        "ASTER_BENCH_REMOTE_SECRET_KEY",
    ];
    if let Some(skipped) = missing_provider_env("remote", &required) {
        return Ok(skipped);
    }
    let base_url = env_required("ASTER_BENCH_REMOTE_BASE_URL")?;
    let base_path = env_string("ASTER_BENCH_REMOTE_BASE_PATH", "asterdrive-range-benchmark");
    let target_key = env_optional("ASTER_BENCH_REMOTE_STORAGE_TARGET_KEY");
    let capabilities = match env_optional("ASTER_BENCH_REMOTE_CAPABILITIES_JSON") {
        Some(raw) => serde_json::from_str::<RemoteStorageCapabilities>(&raw)?,
        None => RemoteStorageCapabilities::current(),
    };
    let stored_capabilities = serde_json::to_string(&capabilities)?;
    let now = chrono::Utc::now();
    let follower = managed_follower::Model {
        id: 1,
        name: "provider-range-benchmark".to_string(),
        base_url: base_url.clone(),
        access_key: env_required("ASTER_BENCH_REMOTE_ACCESS_KEY")?,
        secret_key: env_required("ASTER_BENCH_REMOTE_SECRET_KEY")?,
        is_enabled: true,
        transport_mode: RemoteNodeTransportMode::Direct,
        last_capabilities: stored_capabilities,
        last_error: String::new(),
        last_checked_at: None,
        tunnel_last_error: String::new(),
        tunnel_last_seen_at: None,
        created_at: now,
        updated_at: now,
    };
    let driver = RemoteDriver::new(
        &RemoteDriverConfig {
            base_path: base_path.clone(),
            remote_storage_target_key: target_key.clone(),
            max_file_size: 0,
        },
        &follower,
    )?;
    Ok(ProviderBuild::Ready(ProviderFixture {
        provider: "remote".to_string(),
        driver: Box::new(driver),
        requests_per_backend_call: 1,
        config_summary: json!({
            "base_url_configured": !base_url.is_empty(),
            "base_path_kind": "benchmark_fixture",
            "storage_target_selection": if target_key.is_some() { "explicit" } else { "default" },
            "protocol_version": capabilities.protocol_version,
            "min_supported_protocol_version": capabilities.min_supported_protocol_version,
            "server_version": capabilities.server_version,
            "fixture_identifiers_redacted": true,
            "request_count_contract": "one signed internal-storage GET per backend call",
        }),
        cleanup_root: None,
    }))
}

fn missing_provider_env(provider: &str, required: &[&str]) -> Option<ProviderBuild> {
    let missing = required
        .iter()
        .filter(|name| env_optional(name).is_none())
        .map(|name| (*name).to_string())
        .collect::<Vec<_>>();
    (!missing.is_empty()).then(|| ProviderBuild::Skipped {
        provider: provider.to_string(),
        reason: format!(
            "provider fixture is not configured; missing {}",
            missing.join(", ")
        ),
        prerequisites: required.iter().map(|name| (*name).to_string()).collect(),
        config_summary: json!({ "missing_environment": missing }),
    })
}

async fn run_provider_scenario(
    driver: &dyn StorageDriver,
    requests_per_backend_call: u64,
    object_path: &str,
    scenario: &ScenarioSpec,
) -> BenchResult<Sample> {
    match scenario {
        ScenarioSpec::Full { selected_bytes } => {
            let expected = ByteWindow {
                offset: 0,
                length: *selected_bytes,
            };
            let sample = read_one(driver, object_path, None, expected).await?;
            let actual_read_bytes = sample.bytes_read;
            Ok(sample.into_sample(1, requests_per_backend_call, actual_read_bytes, 0))
        }
        ScenarioSpec::Single { offset, length } => {
            let expected = ByteWindow {
                offset: *offset,
                length: *length,
            };
            let sample = read_one(driver, object_path, Some(expected), expected).await?;
            let actual_read_bytes = sample.bytes_read;
            Ok(sample.into_sample(1, requests_per_backend_call, actual_read_bytes, 0))
        }
        ScenarioSpec::Multi { ranges } => {
            read_multi(driver, requests_per_backend_call, object_path, *ranges).await
        }
    }
}

struct ReadSample {
    open_duration: Duration,
    first_byte_duration: Duration,
    read_duration: Duration,
    total_duration: Duration,
    bytes_read: u64,
}

impl ReadSample {
    fn into_sample(
        self,
        backend_call_count: u64,
        backend_request_count: u64,
        actual_read_bytes: u64,
        prefix_skip_bytes: u64,
    ) -> Sample {
        let read_seconds = self.read_duration.as_secs_f64().max(f64::EPSILON);
        Sample {
            open_ms: duration_ms(self.open_duration),
            ttfb_ms: duration_ms(self.first_byte_duration),
            read_ms: duration_ms(self.read_duration),
            total_ms: duration_ms(self.total_duration),
            throughput_bytes_per_second: self.bytes_read as f64 / read_seconds,
            backend_call_count,
            backend_request_count,
            actual_read_bytes,
            prefix_skip_bytes,
        }
    }
}

async fn read_one(
    driver: &dyn StorageDriver,
    object_path: &str,
    range: Option<ByteWindow>,
    expected: ByteWindow,
) -> BenchResult<ReadSample> {
    let started = Instant::now();
    let mut reader = match range {
        Some(range) => {
            driver
                .get_range(object_path, range.offset, Some(range.length))
                .await?
        }
        None => driver.get_stream(object_path).await?,
    };
    let opened = Instant::now();
    let mut buffer = vec![0_u8; READ_BUFFER_BYTES];
    let mut first_byte_at = None;
    let mut bytes_read = 0_u64;
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        if first_byte_at.is_none() {
            first_byte_at = Some(Instant::now());
        }
        let read = u64::try_from(read)?;
        let remaining = expected.length.saturating_sub(bytes_read);
        if read > remaining {
            return Err(format!(
                "provider returned more bytes than the selected window: expected={}, received_at_least={}",
                expected.length,
                bytes_read.saturating_add(read)
            )
            .into());
        }
        validate_pattern(
            &buffer[..usize::try_from(read)?],
            expected.offset.saturating_add(bytes_read),
        )?;
        bytes_read = bytes_read.saturating_add(read);
    }
    let finished = Instant::now();
    let first_byte_at = first_byte_at.ok_or("provider returned an empty stream")?;
    if bytes_read != expected.length {
        return Err(format!(
            "provider returned a short byte window: expected={}, actual={bytes_read}",
            expected.length
        )
        .into());
    }
    Ok(ReadSample {
        open_duration: opened.duration_since(started),
        first_byte_duration: first_byte_at.duration_since(started),
        read_duration: finished.duration_since(opened),
        total_duration: finished.duration_since(started),
        bytes_read,
    })
}

async fn read_multi(
    driver: &dyn StorageDriver,
    requests_per_backend_call: u64,
    object_path: &str,
    ranges: [ByteWindow; 2],
) -> BenchResult<Sample> {
    let started = Instant::now();
    let first = read_one(driver, object_path, Some(ranges[0]), ranges[0]).await?;
    let second = read_one(driver, object_path, Some(ranges[1]), ranges[1]).await?;
    let total_duration = started.elapsed();
    let read_duration = first.read_duration.saturating_add(second.read_duration);
    let selected = first.bytes_read.saturating_add(second.bytes_read);
    Ok(Sample {
        open_ms: duration_ms(first.open_duration.saturating_add(second.open_duration)),
        ttfb_ms: duration_ms(first.first_byte_duration),
        read_ms: duration_ms(read_duration),
        total_ms: duration_ms(total_duration),
        throughput_bytes_per_second: selected as f64
            / read_duration.as_secs_f64().max(f64::EPSILON),
        backend_call_count: 2,
        backend_request_count: requests_per_backend_call.saturating_mul(2),
        actual_read_bytes: selected,
        prefix_skip_bytes: 0,
    })
}

fn validate_pattern(bytes: &[u8], absolute_offset: u64) -> BenchResult<()> {
    for (index, actual) in bytes.iter().copied().enumerate() {
        let index = u64::try_from(index)?;
        let expected = u8::try_from(absolute_offset.saturating_add(index) % 251)?;
        if actual != expected {
            return Err(format!(
                "provider returned unexpected content at byte {}: expected={expected}, actual={actual}",
                absolute_offset.saturating_add(index)
            )
            .into());
        }
    }
    Ok(())
}

fn report_for_scenario(scenario: &ScenarioSpec, samples: Vec<Sample>) -> ScenarioReport {
    let (selected_bytes, expected_backend_calls) = match scenario {
        ScenarioSpec::Full { selected_bytes } => (*selected_bytes, 1),
        ScenarioSpec::Single { length, .. } => (*length, 1),
        ScenarioSpec::Multi { ranges } => (ranges[0].length + ranges[1].length, 2),
    };
    ScenarioReport {
        selected_bytes,
        expected_backend_calls,
        expected_backend_requests: samples
            .first()
            .map_or(0, |sample| sample.backend_request_count),
        expected_prefix_skip_bytes: 0,
        summary: summarize_samples(&samples),
        samples,
    }
}

async fn run_fallback_benchmark(
    config: &BenchConfig,
    payload: Arc<[u8]>,
) -> BenchResult<ScenarioReport> {
    let offset = config.payload_bytes - config.range_bytes;
    let driver = FallbackDriver::new(payload);
    for _ in 0..config.warmups {
        let _ = run_fallback_sample(&driver, offset, config.range_bytes).await?;
    }
    let mut samples = Vec::with_capacity(config.samples);
    for _ in 0..config.samples {
        samples.push(run_fallback_sample(&driver, offset, config.range_bytes).await?);
    }
    Ok(ScenarioReport {
        selected_bytes: config.range_bytes,
        expected_backend_calls: 1,
        expected_backend_requests: 1,
        expected_prefix_skip_bytes: offset,
        summary: summarize_samples(&samples),
        samples,
    })
}

async fn run_fallback_sample(
    driver: &FallbackDriver,
    offset: u64,
    length: u64,
) -> BenchResult<Sample> {
    let before_bytes = driver.bytes_read.load(Ordering::SeqCst);
    let before_opens = driver.stream_opens.load(Ordering::SeqCst);
    let window = ByteWindow { offset, length };
    let sample = read_one(driver, "fallback.bin", Some(window), window).await?;
    let actual_read_bytes = driver
        .bytes_read
        .load(Ordering::SeqCst)
        .saturating_sub(before_bytes);
    let backend_calls = driver
        .stream_opens
        .load(Ordering::SeqCst)
        .saturating_sub(before_opens);
    Ok(sample.into_sample(
        u64::try_from(backend_calls)?,
        u64::try_from(backend_calls)?,
        actual_read_bytes,
        offset,
    ))
}

struct CountingReader {
    inner: std::io::Cursor<Arc<[u8]>>,
    bytes_read: Arc<AtomicU64>,
}

impl AsyncRead for CountingReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let result = std::io::Read::read(&mut self.inner, buffer.initialize_unfilled());
        if let Ok(read) = result {
            buffer.advance(read);
            self.bytes_read
                .fetch_add(u64::try_from(read).unwrap_or(u64::MAX), Ordering::SeqCst);
        }
        Poll::Ready(result.map(|_| ()))
    }
}

struct FallbackDriver {
    data: Arc<[u8]>,
    stream_opens: AtomicUsize,
    bytes_read: Arc<AtomicU64>,
}

impl FallbackDriver {
    fn new(data: Arc<[u8]>) -> Self {
        Self {
            data,
            stream_opens: AtomicUsize::new(0),
            bytes_read: Arc::new(AtomicU64::new(0)),
        }
    }
}

#[async_trait]
impl StorageDriver for FallbackDriver {
    async fn put(&self, path: &str, _data: &[u8]) -> aster_drive_storage::Result<String> {
        Ok(path.to_string())
    }

    async fn get(&self, _path: &str) -> aster_drive_storage::Result<Vec<u8>> {
        Ok(self.data.as_ref().to_vec())
    }

    async fn get_stream(
        &self,
        _path: &str,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.stream_opens.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(CountingReader {
            inner: std::io::Cursor::new(Arc::clone(&self.data)),
            bytes_read: Arc::clone(&self.bytes_read),
        }))
    }

    async fn delete(&self, _path: &str) -> aster_drive_storage::Result<()> {
        Ok(())
    }

    async fn exists(&self, _path: &str) -> aster_drive_storage::Result<bool> {
        Ok(true)
    }

    async fn metadata(&self, _path: &str) -> aster_drive_storage::Result<BlobMetadata> {
        Ok(BlobMetadata {
            size: u64::try_from(self.data.len()).unwrap_or(u64::MAX),
            content_type: Some("application/octet-stream".to_string()),
        })
    }
}

#[cfg(test)]
pub async fn contract_multi_range_accounting() {
    let root = std::env::temp_dir().join(format!(
        "asterdrive-webdav-provider-range-contract-{}",
        uuid::Uuid::new_v4()
    ));
    tokio::fs::create_dir_all(&root).await.unwrap();
    let root_string = root.to_string_lossy().into_owned();
    let driver = LocalDriver::new(&root_string).unwrap();
    let payload = deterministic_payload(1024).unwrap();
    driver.put("contract.bin", &payload).await.unwrap();
    let ranges = [
        ByteWindow {
            offset: 10,
            length: 8,
        },
        ByteWindow {
            offset: 900,
            length: 8,
        },
    ];

    let sample = run_provider_scenario(&driver, 3, "contract.bin", &ScenarioSpec::Multi { ranges })
        .await
        .unwrap();

    assert_eq!(sample.backend_call_count, 2);
    assert_eq!(sample.backend_request_count, 6);
    assert_eq!(sample.actual_read_bytes, 16);
    driver.delete("contract.bin").await.unwrap();
    tokio::fs::remove_dir_all(root).await.unwrap();
}

#[cfg(test)]
pub fn contract_odd_multi_range_accounting() {
    let config = BenchConfig {
        provider: "local".to_string(),
        provider_required: false,
        payload_bytes: 18,
        range_bytes: 3,
        warmups: 0,
        samples: 1,
        object_path: "contract.bin".to_string(),
        output_path: PathBuf::from("artifact.json"),
        cleanup_fixture: true,
        baseline_path: None,
        baseline_profile: None,
        fail_on_regression: false,
    };
    let ScenarioSpec::Multi { ranges } = config
        .scenario_specs()
        .remove("multi_range_disjoint")
        .unwrap()
    else {
        panic!("multi_range_disjoint must remain a multi-range scenario");
    };

    assert!(ranges.iter().all(|range| range.length > 0));
    assert_eq!(ranges[0].length + ranges[1].length, config.range_bytes);
}

#[cfg(test)]
pub async fn contract_failed_benchmark_cleans_fixture() {
    let root = std::env::temp_dir().join(format!(
        "asterdrive-webdav-provider-range-cleanup-contract-{}",
        uuid::Uuid::new_v4()
    ));
    tokio::fs::create_dir_all(&root).await.unwrap();
    let root_string = root.to_string_lossy().into_owned();
    let driver = LocalDriver::new(&root_string).unwrap();
    let payload = deterministic_payload(32).unwrap();
    driver.put("contract.bin", &payload).await.unwrap();
    let provider_fixture = ProviderFixture {
        provider: "local".to_string(),
        driver: Box::new(driver),
        requests_per_backend_call: 1,
        config_summary: json!({}),
        cleanup_root: Some(root.clone()),
    };
    let benchmark_result: BenchResult<()> = Err("synthetic benchmark failure".into());
    let cleanup_result = cleanup_provider_fixture(&provider_fixture, "contract.bin").await;

    let error = finish_benchmark(benchmark_result, cleanup_result).unwrap_err();

    assert_eq!(error.to_string(), "synthetic benchmark failure");
    assert!(!root.exists());
}

#[cfg(test)]
pub async fn contract_fallback_read_accounting() {
    let payload = Arc::<[u8]>::from(deterministic_payload(1024).unwrap());
    let driver = FallbackDriver::new(payload);
    let offset = 900;
    let length = 16;

    let sample = run_fallback_sample(&driver, offset, length).await.unwrap();

    assert_eq!(sample.backend_call_count, 1);
    assert_eq!(sample.backend_request_count, 1);
    assert_eq!(sample.actual_read_bytes, offset + length);
    assert_eq!(sample.prefix_skip_bytes, offset);
}

fn summarize_samples(samples: &[Sample]) -> ScenarioStatistics {
    ScenarioStatistics {
        open_ms: distribution(samples.iter().map(|sample| sample.open_ms)),
        ttfb_ms: distribution(samples.iter().map(|sample| sample.ttfb_ms)),
        read_ms: distribution(samples.iter().map(|sample| sample.read_ms)),
        total_ms: distribution(samples.iter().map(|sample| sample.total_ms)),
        throughput_bytes_per_second: distribution(
            samples
                .iter()
                .map(|sample| sample.throughput_bytes_per_second),
        ),
        backend_call_count: distribution(
            samples
                .iter()
                .map(|sample| sample.backend_call_count as f64),
        ),
        backend_request_count: distribution(
            samples
                .iter()
                .map(|sample| sample.backend_request_count as f64),
        ),
        actual_read_bytes: distribution(
            samples.iter().map(|sample| sample.actual_read_bytes as f64),
        ),
        prefix_skip_bytes: distribution(
            samples.iter().map(|sample| sample.prefix_skip_bytes as f64),
        ),
    }
}

fn distribution(values: impl Iterator<Item = f64>) -> Distribution {
    let mut values = values.collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    Distribution {
        min: values[0],
        p50: percentile(&values, 0.50),
        p95: percentile(&values, 0.95),
        p99: percentile(&values, 0.99),
        max: values[values.len() - 1],
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "The interpolated percentile index is bounded by the non-empty slice length."
)]
fn percentile(values: &[f64], percentile: f64) -> f64 {
    if values.len() == 1 {
        return values[0];
    }
    let position = percentile * (values.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        return values[lower];
    }
    let weight = position - lower as f64;
    values[lower] + (values[upper] - values[lower]) * weight
}

async fn compare_baseline(
    config: &BenchConfig,
    scenarios: &BTreeMap<String, ScenarioReport>,
) -> BenchResult<BaselineComparison> {
    let Some(path) = config.baseline_path.as_deref() else {
        return Ok(baseline_not_compared(config, "no baseline path configured"));
    };
    let Some(profile) = config.baseline_profile.as_deref() else {
        return Ok(baseline_not_compared(
            config,
            "no baseline profile configured",
        ));
    };
    let bytes = tokio::fs::read(path).await?;
    let baseline: BaselineFile = serde_json::from_slice(&bytes)?;
    if baseline.schema_version != ARTIFACT_SCHEMA_VERSION {
        return Ok(baseline_not_compared(
            config,
            "baseline schema version does not match artifact schema",
        ));
    }
    let Some(selected) = baseline.profiles.iter().find(|candidate| {
        candidate.profile == profile
            && candidate.provider == config.provider
            && candidate.payload_bytes == config.payload_bytes
            && candidate.range_bytes == config.range_bytes
            && candidate.sampling.warmups == config.warmups
            && candidate.sampling.samples == config.samples
            && candidate.sampling.read_buffer_bytes == READ_BUFFER_BYTES
            && candidate.machine.build_profile == current_build_profile()
    }) else {
        return Ok(BaselineComparison {
            baseline_path: Some(path.display().to_string()),
            profile: Some(profile.to_string()),
            status: "not_compared",
            reason: Some("no matching provider, fixture, and machine profile".to_string()),
            policy: Some(baseline.regression_policy),
            scenarios: BTreeMap::new(),
        });
    };

    let mut comparisons = BTreeMap::new();
    for (name, current) in scenarios {
        let Some(reference) = selected.scenarios.get(name) else {
            continue;
        };
        let ttfb_ratio = ratio(current.summary.ttfb_ms.p95, reference.ttfb_p95_ms);
        let throughput_ratio = ratio(
            current.summary.throughput_bytes_per_second.p50,
            reference.throughput_p50_bytes_per_second,
        );
        comparisons.insert(
            name.clone(),
            ScenarioComparison {
                ttfb_p95_ratio: ttfb_ratio,
                throughput_p50_ratio: throughput_ratio,
                regressed: ttfb_ratio > baseline.regression_policy.ttfb_p95_max_ratio
                    || throughput_ratio < baseline.regression_policy.throughput_p50_min_ratio,
            },
        );
    }
    Ok(BaselineComparison {
        baseline_path: Some(path.display().to_string()),
        profile: Some(profile.to_string()),
        status: "compared",
        reason: None,
        policy: Some(baseline.regression_policy),
        scenarios: comparisons,
    })
}

fn baseline_not_compared(config: &BenchConfig, reason: &str) -> BaselineComparison {
    BaselineComparison {
        baseline_path: config
            .baseline_path
            .as_ref()
            .map(|path| path.display().to_string()),
        profile: config.baseline_profile.clone(),
        status: "not_compared",
        reason: Some(reason.to_string()),
        policy: None,
        scenarios: BTreeMap::new(),
    }
}

fn ratio(current: f64, baseline: f64) -> f64 {
    if baseline <= f64::EPSILON {
        f64::INFINITY
    } else {
        current / baseline
    }
}

async fn write_artifact(path: &Path, artifact: &BenchmarkArtifact) -> BenchResult<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let body = serde_json::to_vec_pretty(artifact)?;
    tokio::fs::write(path, body).await?;
    println!("WebDAV provider Range artifact: {}", path.display());
    Ok(())
}

fn deterministic_payload(size: u64) -> BenchResult<Vec<u8>> {
    let size = usize::try_from(size)?;
    (0..size)
        .map(|index| u8::try_from(index % 251).map_err(Into::into))
        .collect()
}

fn machine_summary(profile: Option<String>) -> MachineSummary {
    MachineSummary {
        profile,
        build_profile: current_build_profile(),
        os: std::env::consts::OS.to_string(),
        architecture: std::env::consts::ARCH.to_string(),
        cpu_model: cpu_model(),
        logical_cpus: std::thread::available_parallelism().map_or(1, usize::from),
        rustc: command_output("rustc", &["--version"]),
        kernel: command_output("uname", &["-srmo"]),
    }
}

const fn current_build_profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "optimized"
    }
}

fn cpu_model() -> Option<String> {
    let contents = std::fs::read_to_string("/proc/cpuinfo").ok()?;
    contents.lines().find_map(|line| {
        line.split_once(':')
            .and_then(|(key, value)| (key.trim() == "model name").then(|| value.trim().to_string()))
    })
}

fn command_output(command: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(command).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_dirty() -> Option<bool> {
    command_output("git", &["status", "--porcelain"]).map(|output| !output.is_empty())
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn env_optional(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_required(name: &str) -> BenchResult<String> {
    env_optional(name).ok_or_else(|| format!("missing required environment variable {name}").into())
}

fn env_string(name: &str, fallback: &str) -> String {
    env_optional(name).unwrap_or_else(|| fallback.to_string())
}

fn env_bool(name: &str, fallback: bool) -> BenchResult<bool> {
    let Some(value) = env_optional(name) else {
        return Ok(fallback);
    };
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(format!("invalid boolean environment variable {name}={value}").into()),
    }
}

fn env_u64(name: &str, fallback: u64) -> BenchResult<u64> {
    match env_optional(name) {
        Some(value) => Ok(value.parse()?),
        None => Ok(fallback),
    }
}

fn env_usize(name: &str, fallback: usize) -> BenchResult<usize> {
    match env_optional(name) {
        Some(value) => Ok(value.parse()?),
        None => Ok(fallback),
    }
}
