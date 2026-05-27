---
description: AsterDrive Prometheus metrics, Grafana dashboard, Docker local observability stack, and production monitoring notes.
---

# Monitoring and Grafana

::: tip What this page covers
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

After startup, confirm the instance returns metrics:

```bash
curl http://<asterdrive-host>:3000/health/metrics
```

If it returns 404, the current binary usually was not built with the `metrics` feature.  
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

The AsterDrive docs site provides a Grafana dashboard JSON that can be imported directly. Both the official docs site and self-hosted VitePress docs site distribute this file from the site root:

```text
/grafana/asterdrive-overview.json
```

Full URL on the online docs site:

```text
https://drive.astercosm.com/grafana/asterdrive-overview.json
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

## Production Security Boundary

AsterDrive currently does not apply application-layer authentication to `/health/metrics`. This keeps Prometheus scraping simple and stable. Access control should be handled at the network boundary.

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
