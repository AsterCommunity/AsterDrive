---
description: AsterDrive Prometheus metrics, Grafana dashboard, Docker local observability stack, and production monitoring notes.
title: "Monitoring and Grafana"
---

:::tip[What this page covers]
This page explains how to enable AsterDrive's built-in Prometheus metrics, how Prometheus scrapes them, and how to import the official Grafana dashboard.  
If you are only doing a final pre-launch check, still focus on the "Production Security Boundary" section to confirm the metrics endpoint is not exposed directly to the public internet.
:::

AsterDrive's Prometheus metrics are optional and are not enabled in the default build. Once enabled, the service registers:

```text
GET /health/metrics
```

under the health check path. The output is Prometheus text exposition and covers HTTP, database, uploads and downloads, background tasks, storage drivers, authentication events, share download rollback, and process resources.

## Current capabilities and evidence

This page owns Drive's consumer capability and configuration contract. The [Forge consumer index](https://github.com/AsterCommunity/AsterForge/blob/master/docs/en/guide/reference-projects.md) owns public mechanisms and revision evidence. Checked on **2026-10-17** against merged source; planned support does not imply availability in released artifacts.

| Signal | Current support | Enablement and limits |
| --- | --- | --- |
| Local text / JSON logs | Integrated with Forge logging | `[logging]`; stdout or file; `RUST_LOG` takes priority; see [logging](/en/reference/config/logging/) |
| Prometheus metrics | Integrated with instance-owned Forge provider | `metrics` feature + `observability.prometheus.enabled`; primary/follower; disabled by default |
| OTLP metrics | Integrated; independent of or concurrent with Prometheus | `otlp-metrics` feature + `observability.otlp_metrics.enabled`; HTTP/protobuf; disabled by default |
| OTLP traces / cross-node traces / trace-log correlation | Unsupported | Forge #45 / Drive #638; existing `tracing` spans and request_id do not provide OTel export |
| OTLP logs | Unsupported | Local JSON logs are not an OTLP logs exporter |
| Jemalloc heap profiling / allocator statistics | Separate optional capabilities | See [heap profiling](https://drive.docs.astercosm.com/developer/en/testing/jemalloc-profiling/) for platform/allocator limits; not continuous profiling export |
| Continuous profiling platform export | Unsupported | No placeholder enablement switch |

### Pinned revisions and integration points

| Item | Evidence |
| --- | --- |
| Drive revision | [`4b857c0fc3a27941b36861657225f20786c3831b`](https://github.com/AsterCommunity/AsterDrive/commit/4b857c0fc3a27941b36861657225f20786c3831b) |
| Forge resolved revision | [`869756f0605c898420a28d3d45006cc33a87d17c`](https://github.com/AsterCommunity/AsterForge/commit/869756f0605c898420a28d3d45006cc33a87d17c); pinned Git dependencies in [Cargo.lock](https://github.com/AsterCommunity/AsterDrive/blob/4b857c0fc3a27941b36861657225f20786c3831b/Cargo.lock) and [Cargo.toml](https://github.com/AsterCommunity/AsterDrive/blob/4b857c0fc3a27941b36861657225f20786c3831b/Cargo.toml), without local path/patch |
| Compiled capabilities | [Product metrics crate](https://github.com/AsterCommunity/AsterDrive/blob/4b857c0fc3a27941b36861657225f20786c3831b/crates/aster_drive_metrics/Cargo.toml): `metrics` enables allocator/runtime-health; `prometheus` / `otlp` forward exporter features; root features are in Cargo.toml above |
| Configuration and validation | [ObservabilityConfig](https://github.com/AsterCommunity/AsterDrive/blob/4b857c0fc3a27941b36861657225f20786c3831b/src/config/metrics.rs), [example](https://github.com/AsterCommunity/AsterDrive/blob/4b857c0fc3a27941b36861657225f20786c3831b/config.example.toml); `ASTER__` overrides, restart required, unknown fields and uncompiled capabilities rejected |
| Product adapter / sampling | [Typed metric set and recorder](https://github.com/AsterCommunity/AsterDrive/blob/4b857c0fc3a27941b36861657225f20786c3831b/crates/aster_drive_metrics/src/lib.rs), [background tasks](https://github.com/AsterCommunity/AsterDrive/blob/4b857c0fc3a27941b36861657225f20786c3831b/src/runtime/tasks.rs); Drive owns business SQL, capacity sampling and label semantics |
| HTTP / role injection | [Health routes](https://github.com/AsterCommunity/AsterDrive/blob/4b857c0fc3a27941b36861657225f20786c3831b/src/api/routes/health.rs), [authentication](https://github.com/AsterCommunity/AsterDrive/blob/4b857c0fc3a27941b36861657225f20786c3831b/src/api/middleware/metrics_auth.rs), [primary/follower components](https://github.com/AsterCommunity/AsterDrive/blob/4b857c0fc3a27941b36861657225f20786c3831b/src/runtime/components.rs) |
| Lifecycle | [Runtime assembly](https://github.com/AsterCommunity/AsterDrive/blob/4b857c0fc3a27941b36861657225f20786c3831b/src/runtime/assembly.rs); metrics shutdown follows runtime producers and audit recovery, preserving the original runtime error; main retains the logging guard until exit |
| Forge public contracts | [Metrics](https://github.com/AsterCommunity/AsterForge/blob/869756f0605c898420a28d3d45006cc33a87d17c/docs/crates/aster_forge_metrics.md), [observability](https://github.com/AsterCommunity/AsterForge/blob/869756f0605c898420a28d3d45006cc33a87d17c/docs/crates/aster_forge_observability.md), [logging](https://github.com/AsterCommunity/AsterForge/blob/869756f0605c898420a28d3d45006cc33a87d17c/docs/crates/aster_forge_logging.md) |

### Metric schema and compatibility

The complete declarations belong to the [Forge shared schema](https://github.com/AsterCommunity/AsterForge/blob/869756f0605c898420a28d3d45006cc33a87d17c/crates/aster_forge_metrics/src/lib.rs), its [infrastructure recorder](https://github.com/AsterCommunity/AsterForge/blob/869756f0605c898420a28d3d45006cc33a87d17c/crates/aster_forge_metrics/src/shared.rs), and the [Drive product metric set](https://github.com/AsterCommunity/AsterDrive/blob/4b857c0fc3a27941b36861657225f20786c3831b/crates/aster_drive_metrics/src/lib.rs). They cover HTTP/DB/auth/task/config/system/health and product transfers, upload phases/backlog, capacity, cleanup, remote transport and avatars. Descriptors define kind, HELP, unit, labels and buckets; there is no second manually maintained inventory.

The [pre-instance product schema fixture](https://github.com/AsterCommunity/AsterDrive/blob/4b857c0fc3a27941b36861657225f20786c3831b/crates/aster_drive_metrics/tests/fixtures/pre_instance_schema.json) is a historical compatibility subset, not the complete current schema. `migration_preserves_the_entire_pre_instance_product_schema` compares names, kinds, HELP, labels and buckets; [Grafana contract tests](https://github.com/AsterCommunity/AsterDrive/blob/4b857c0fc3a27941b36861657225f20786c3831b/crates/aster_drive_metrics/tests/grafana_dashboard.rs) check the dashboard. Forge's metrics contract documents the API replacement map. The delivered migration has no planned metric rename; future name/unit/label/bucket changes require an explicit migration and tests.

`process_cpu_milliseconds_total` retains milliseconds. Product transfer bytes, logical file sizes and internal driver/remote I/O must not be added together. `blob_cleanup_retired_backing_bytes_total` accounts for acknowledged backing deletions, not guaranteed filesystem space recovery. Capacity minima are independent dimensions, not additive or suitable for cross-dimension ratios; use validity/last-success data for unknown or stale snapshots.

HTTP duration measures the handler/header stage, while file transfer duration follows the observed stream through EOF/error/drop. Presigned/redirect traffic outside AD's data plane is invisible. Sampled traces cannot replace unsampled throughput counters. A standalone worker role is not delivered; primary/follower evidence does not establish support for it.

### Validation, distribution and maintenance

- **Historical implementation evidence**: [PR #642](https://github.com/AsterCommunity/AsterDrive/pull/642), merged 2026-10-16, used Forge `2beaf986f1740629d3fd8913f865e4134d31e203`. Its local evidence includes four feature/config combinations, both roles' endpoint/authentication tests, schema/Grafana, upload and three-database migration checks. It explicitly did not wait for new CI; this is not an all-green CI claim.
- **Current dependency evidence**: the pinned revision above is now `869756f0605c898420a28d3d45006cc33a87d17c`. Static inspection does not rerun #642's full matrix at this revision. Each PR records its actual commands/results; historical success does not carry forward automatically.
- **Distribution evidence**: the [image workflow](https://github.com/AsterCommunity/AsterDrive/blob/4b857c0fc3a27941b36861657225f20786c3831b/.github/workflows/docker-image.yml) defines `-metrics` builds. OTLP currently uses source/custom-image builds described below. A workflow is not proof that a tag is downloadable; no particular release artifact, image digest or production rollout was verified here.
- **Checks**: `bun docs/scripts/check-observability-docs.mjs` checks bilingual revision links, pinned Drive paths and the current Forge lock revision offline. `bash scripts/test-metrics-features.sh` validates four combinations and runs configuration/authentication tests. Static/link checks do not establish runtime behavior or collector delivery.
- **Ownership**: PRs changing features, configuration, schema, protocols or lifecycle update both translations and the Forge index. Pin existing public commits and record tests, distribution and production separately. Documentation builds run the revision check without fetching consumers.

Metrics and scrape authentication were delivered by #642, closing #636/#637. Remaining traces work is [Drive #638](https://github.com/AsterCommunity/AsterDrive/issues/638) / [Forge #45](https://github.com/AsterCommunity/AsterForge/issues/45). [Drive #639](https://github.com/AsterCommunity/AsterDrive/issues/639) / [Forge #46](https://github.com/AsterCommunity/AsterForge/issues/46) track this source/index. Forge #44 still has library/template acceptance work; Drive does not need another recorder migration.

## Enable the Metrics Feature

When building from source, use:

```bash
cargo build --release --features metrics
```

Or use the build that includes all optional capabilities:

```bash
cargo build --release --features full
```

Set `[observability.prometheus].enabled = true` and restart; configure dedicated Basic Auth as shown below.

After startup, confirm the instance returns metrics:

```bash
curl http://<asterdrive-host>:3000/health/metrics
```

If it returns 404, check that both the `metrics` Cargo feature and static `[observability.prometheus].enabled` are enabled.
If it returns HTML or a proxy error, check the reverse proxy path and backend port first.

## Prometheus Scrape Configuration

When Prometheus and AsterDrive are on the same host or in the same network, scrape like this:

```yaml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: asterdrive
    metrics_path: /health/metrics
    static_configs:
      - targets:
          - 127.0.0.1:3000
```

If Prometheus runs in Docker and AsterDrive runs on the host, Docker Desktop usually uses:

```yaml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: asterdrive
    metrics_path: /health/metrics
    static_configs:
      - targets:
          - host.docker.internal:3000
```

On Linux Docker, if `host.docker.internal` is unavailable, add this to the Prometheus container:

```yaml
extra_hosts:
  - host.docker.internal:host-gateway
```

Then still use `host.docker.internal:3000` as the target.

## Docker Compose Example

Below is a minimal Prometheus + Grafana compose example for the scenario where Prometheus / Grafana run in Docker and AsterDrive listens on host port `3000`. For production deployment, adjust the network, ports, persistent directories, and access control to match your environment.

```yaml
services:
  prometheus:
    image: prom/prometheus:v3.7.3 # Example pinned to a verified version; validate config compatibility before upgrading in your deployment window.
    command:
      - --config.file=/etc/prometheus/prometheus.yml
      - --storage.tsdb.path=/prometheus
      - --web.enable-lifecycle
    ports:
      - "9090:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml:ro
      - prometheus-data:/prometheus
    extra_hosts:
      - host.docker.internal:host-gateway

  grafana:
    image: grafana/grafana:12.3.0 # Example pinned to a verified version; validate config compatibility before upgrading in your deployment window.
    ports:
      - "3300:3000"
    environment:
      - GF_SECURITY_ADMIN_USER=admin
      - GF_SECURITY_ADMIN_PASSWORD=admin
    volumes:
      - grafana-data:/var/lib/grafana
    depends_on:
      - prometheus

volumes:
  prometheus-data:
  grafana-data:
```

`prometheus.yml`:

```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: asterdrive
    metrics_path: /health/metrics
    static_configs:
      - targets:
          - host.docker.internal:3000
```

After starting the example, open:

- Prometheus: `http://localhost:9090`
- Grafana: `http://localhost:3300`

The sample Grafana account is `admin` / `admin`. In production, change it to an independent strong password and restrict access sources for the Grafana admin entry.

## Grafana Dashboard

The AsterDrive docs site provides a Grafana dashboard JSON that can be imported directly. Both the official docs site and self-hosted docs sites distribute this file from the site root:

```text
/grafana/asterdrive-overview.json
```

Full URL on the online docs site:

```text
https://drive.docs.astercosm.com/grafana/asterdrive-overview.json
```

Import steps:

1. Open Grafana.
2. Go to `Dashboards -> New -> Import`.
3. Paste the JSON URL above, or download the JSON and upload it.
4. Select your Prometheus datasource.
5. Import `AsterDrive Overview`.

This dashboard provides:

- Overview: target health, process uptime, RSS, CPU, HTTP RPS, and 5xx ratio.
- HTTP: request count by route/status, p95/p99 latency, error ratio, and slow routes.
- Database: SeaORM query count, error count, p95 latency, and average latency.
- Storage drivers: operation count, not_found, hard failures, p95 latency, and average latency.
- Transfer: uploads, downloads, upload session lifecycle, and transfer failures.
- Background tasks: backlog, status transitions, retry, and share download rollback queue.
- Authentication: login and refresh token events, authentication failure ratio.
- Process: RSS, CPU core usage, uptime.

Dashboard queries are based on AsterDrive's current metric labels:

- `job`
- `instance`
- `method`
- `route`
- `status`
- `backend`
- `kind`
- `driver`
- `operation`
- `mode`
- `source`
- `outcome`
- `range`
- `action`
- `reason`
- `event`

`job` and `instance` come from the Prometheus scrape target. They are not business labels written by AsterDrive itself.

## Explicit enablement and scrape authentication

Exporters are disabled by default. Cargo features compile capabilities; static configuration enables them. Restart after configuration changes. This replaces the previous compile-only automatic endpoint registration. Static configuration rejects unknown fields. Remove obsolete sections when upgrading and replace the unpublished `[metrics.basic_auth]` with `[observability.prometheus.basic_auth]` so misspelled authentication cannot silently disappear. Errors retain safe field names or parse byte positions, without configuration values.

```toml
[observability.prometheus]
enabled = true

[observability.prometheus.basic_auth]
enabled = true
username = "prometheus"
password_hash = '$argon2id$v=19$...' # replace with a complete verifier
```

Generate the verifier without putting a password in process arguments or initializing the database:

```bash
# Bash: input is not echoed or stored in shell history
read -r -s -p 'Metrics password: ' METRICS_PASSWORD
printf '%s' "$METRICS_PASSWORD" | ./aster_drive hash-password
unset METRICS_PASSWORD
```

The server accepts Argon2id v19 within 64 MiB / 3 passes / 4 lanes / 32 output bytes. Invalid enabled credentials fail startup, without a generated password or anonymous fallback. Usernames cannot contain colons or control characters. Environment overrides use names such as `ASTER__OBSERVABILITY__PROMETHEUS__BASIC_AUTH__PASSWORD_HASH`; protect PHC `$` characters with shell single quotes.

```yaml
scrape_configs:
  - job_name: asterdrive
    scheme: https
    metrics_path: /health/metrics
    basic_auth:
      username: prometheus
      password_file: /etc/prometheus/asterdrive-metrics.pass
    static_configs:
      - targets: [drive.example.com]
```

Prometheus supplies the original password; the server stores only its verifier. Use HTTPS or a trusted TLS termination proxy. Disabled Basic Auth leaves an explicitly enabled endpoint anonymous. JWT/cookies are not substitutes. Missing, malformed, duplicate or incorrect Authorization receives an empty 401 with `WWW-Authenticate: Basic realm="AsterDrive metrics"`. Authentication runs before export and availability checks. Authorized requests retain 200 text exposition / 503 unavailable; export and denial responses use `Cache-Control: no-store`. Health/readiness are unaffected.

Verification uses an independent blocking budget of two jobs, with immediate uniform 401 on saturation. Cancelling a request does not release a still-running job's permit. Syntactically valid Basic credentials run password verification regardless of username match, keeping the verification path the same. Wrong usernames also consume the two permits, so sustained scanning can temporarily cause legitimate scrapes to receive 401.

## Optional OTLP metrics

Existing `-metrics` images provide Prometheus pull. For OTLP, build from source or use `docker build --build-arg CARGO_FEATURES=server,cli,metrics,otlp-metrics -t asterdrive:observability .`; static exporter settings still default to disabled.

```bash
cargo build --release --features metrics,otlp-metrics
```

```toml
[observability]
environment = "production"

[observability.prometheus]
enabled = true # keep false for OTLP-only deployments

[observability.otlp_metrics]
enabled = true
endpoint = "https://collector.example.com/v1/metrics"
interval_seconds = 60
timeout_seconds = 5
# headers = { Authorization = "Bearer <collector credential>" }
```

One instance-owned metrics kernel feeds both readers from one business observation. OTLP supports HTTP/protobuf metrics only, not traces, Tempo or OTLP logs. Explicit configuration is not overridden by `OTEL_*`, proxy or SDK credential environment variables. HTTPS uses system trust; redirects are disabled. Collector outages do not prevent startup/readiness; failures remain locally visible and the next cycle retries a cumulative snapshot.

OTLP resource identity includes service/version/environment/role and the automatically generated config-sync runtime_id as `service.instance.id`, stable for one process lifetime and renewed on restart. No manual instance ID is needed. Prometheus `job/instance` comes from scrape targets, not business labels.

| Compiled capability | Configuration | HTTP endpoint |
| --- | --- | --- |
| Default / no metrics | exporters disabled | Absent |
| `metrics` | prometheus=true | Present, optional Basic Auth |
| `otlp-metrics` | otlp_metrics=true | Absent |
| Both features | independent enablement | Present only with prometheus=true |

Enabling an uncompiled capability fails startup. Compiled but disabled exporters use Noop and start no sampling/push workers. Persistent blob cleanup timestamps are maintained with reference-count writes in every build, so enabling metrics later retains useful ages; this adds no business DB round trips.


For a local smoke test, point the metrics endpoint at `http://127.0.0.1:4318/v1/metrics` and run an OpenTelemetry Collector with the following configuration. This accepts metrics only and prints diagnostics; it does not enable traces or persistent storage. No Collector instance was run for this documentation update.

```yaml
receivers:
  otlp:
    protocols:
      http:
        endpoint: 0.0.0.0:4318
exporters:
  debug:
    verbosity: basic
service:
  pipelines:
    metrics:
      receivers: [otlp]
      exporters: [debug]
```

## Production Security Boundary

Use the dedicated static Basic Auth above for application authentication, with network restrictions as appropriate.

In production, you must ensure:

- `/health/metrics` is not exposed directly to the public internet.
- Only scrapers such as Prometheus, Grafana Agent, and VictoriaMetrics Agent can access it.
- The reverse proxy applies source IP restrictions to `/health/metrics`, or it only listens on the internal network.
- Do not protect metrics with normal browser login state. Scrapers should not depend on browser sessions.

Handle normal user entry points and the metrics endpoint separately in the reverse proxy. The public site can expose `/health`, but `/health/metrics` should only be visible to the monitoring network.

## What to Watch

On the first day after launch, focus on:

- whether 5xx keeps increasing in `http_requests_total`
- whether p95/p99 in `http_request_duration_seconds` rises over time
- whether `db_queries_total{status="error"}` is non-zero
- whether `db_query_duration_seconds` shows abnormal slow query distributions
- whether `storage_driver_operations_total{status="failure",kind!="not_found"}` is non-zero
- whether `background_tasks_pending` keeps accumulating
- whether `background_task_retries_total` keeps increasing
- whether `share_download_rollback_pending` keeps accumulating
- whether `process_memory_rss_bytes` grows in only one direction and does not drop after idle periods

Observe `storage_driver_operations_total{status="failure",kind="not_found"}` separately. In thumbnail, cache, or object probing scenarios, `not_found` is often an expected miss. Alert rules should prioritize hard failures such as `storage_driver_operations_total{status="failure",kind!="not_found"}`.

## Metrics Calibration During Load Testing

When running the download, Range download, WebDAV GET, WebDAV Range GET, and WebDAV `PROPFIND Depth: 1` scripts under `tests/performance/k6`, scrape `/health/metrics` at the same time. k6 reports client-side p95/p99, throughput, and error rate; Prometheus metrics explain where the bottleneck actually sits.

Key comparisons:

- `http_request_duration_seconds`: confirm whether the slow path is REST download, WebDAV GET, or PROPFIND.
- `db_query_duration_seconds` / `db_queries_total{status="error"}`: confirm whether large directory enumeration and permission snapshots are overwhelming the database.
- `storage_driver_operations_total` and storage driver latency metrics: distinguish object storage throttling, range read latency, and ordinary not_found misses.
- `process_memory_rss_bytes`: confirm whether RSS keeps climbing under sustained concurrent downloads and large-directory PROPFIND.
- Remote follower scenarios: scrape both primary and follower. Watching only one side makes it easy to misjudge tunnel or upstream object storage bottlenecks.
