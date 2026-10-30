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

This version uses **Aster Metrics v1**. Upgrade the application and Grafana JSON together; migrate custom rules using the [generated catalog](/metrics/asterdrive-v1.json), and never sum old and new schemas. CPU is a seconds-based Counter; do not divide its rate by 1000. HTTP volume comes from duration count with http_response_status_code; DB errors have a nonempty error_type. HTTP duration includes response-body completion, changing long-download comparisons with the former header-only boundary.

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
- `service_name`
- `deployment_environment_name`
- `aster_role`
- `service_instance_id`
- `aster_metrics_schema_version`
- `http_request_method`
- `http_route`
- `http_response_status_code`
- `error_type`
- `db_system_name`
- `db_operation_name`
- `db_client_connection_pool_name`
- `db_client_connection_state`
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

- whether 5xx keeps increasing in `http_server_request_duration_seconds_count`
- whether p95/p99 in `http_server_request_duration_seconds` rises over time
- whether `db_client_operation_duration_seconds_count{error_type!=""}` is non-zero
- whether `db_client_operation_duration_seconds` shows abnormal slow query distributions
- whether `aster_drive_storage_driver_operations_total{status="failure",kind!="not_found"}` is non-zero
- whether `aster_task_pending` keeps accumulating
- whether `aster_task_retries_total` keeps increasing
- whether `aster_drive_share_download_rollback_pending` keeps accumulating
- whether `process_memory_usage_bytes` grows in only one direction and does not drop after idle periods

Observe `aster_drive_storage_driver_operations_total{status="failure",kind="not_found"}` separately. In thumbnail, cache, or object probing scenarios, `not_found` is often an expected miss. Alert rules should prioritize hard failures such as `aster_drive_storage_driver_operations_total{status="failure",kind!="not_found"}`.

## Metrics Calibration During Load Testing

When running the download, Range download, WebDAV GET, WebDAV Range GET, and WebDAV `PROPFIND Depth: 1` scripts under `tests/performance/k6`, scrape `/health/metrics` at the same time. k6 reports client-side p95/p99, throughput, and error rate; Prometheus metrics explain where the bottleneck actually sits.

Key comparisons:

- `http_server_request_duration_seconds`: confirm whether the slow path is REST download, WebDAV GET, or PROPFIND.
- `db_client_operation_duration_seconds` / `db_client_operation_duration_seconds_count{error_type!=""}`: confirm whether large directory enumeration and permission snapshots are overwhelming the database.
- `aster_drive_storage_driver_operations_total` and storage driver latency metrics: distinguish object storage throttling, range read latency, and ordinary not_found misses.
- `process_memory_usage_bytes`: confirm whether RSS keeps climbing under sustained concurrent downloads and large-directory PROPFIND.
- Remote follower scenarios: scrape both primary and follower. Watching only one side makes it easy to misjudge tunnel or upstream object storage bottlenecks.
