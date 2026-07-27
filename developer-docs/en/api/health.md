# Health Checks

Health checks are mounted at the repository root, not under `/api/v1`.

Both `primary` and `follower` nodes register this group.

## Endpoints

| Method | Path | Description |
| --- | --- | --- |
| `GET` / `HEAD` | `/health` | Liveness check |
| `GET` / `HEAD` | `/health/ready` | Readiness check covering the database, setup state, and lightweight prerequisites for the active node mode |
| `GET` | `/health/memory` | Heap statistics, registered only in `debug_assertions + openapi` builds |
| `GET` | `/health/metrics` | Prometheus metrics, present only when the `metrics` feature is enabled |

## `GET /health`

Typical response:

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "status": "ok",
    "version": "0.0.0",
    "build_time": "2026-03-22T00:00:00Z"
  }
}
```

`build_time` comes from the compile-time `ASTER_BUILD_TIME` environment variable. When the
variable is unset, the build script falls back to the current UTC time.

`HEAD /health` has the same semantics, but returns no body.

## `GET /health/ready`

This endpoint does more than a database ping. It checks the database and the active cache backend for every deployment profile. The `single` profile remains ready when an unavailable configured remote cache falls back to a healthy memory cache, while diagnostics report that state as degraded. Shared-runtime profiles require both the configured and active backend to be Redis. It then continues according to node mode:

- `primary`: validates dynamic cluster topology and the authoritative setup state. `needs_admin` and `needs_storage` return `200` immediately; only `ready` requires a default storage policy, a constructible driver, and that driver's lightweight readiness check
- `follower`: checks the follower's active storage driver and the state required for binding

This endpoint is meant to stay cheap. It does not perform remote S3 or remote-storage read/write/delete probes. Use the admin policy "test connection" action when you need to verify object storage credentials or permissions.

Successful `primary` responses use one of three `data.status` values:

- `needs_admin`: `200`; base dependencies are healthy, but the first administrator does not exist
- `needs_storage`: `200`; an administrator exists, but the default storage policy group or administrator assignment is incomplete
- `ready`: `200`; product setup is complete and the default driver's lightweight readiness check passed

Error responses:

- database unavailable: `503` with `Database unavailable`
- an unhealthy active cache: `503` with `Cache unavailable`; in cluster deployments, an active backend that differs from configuration also returns `503`, and both the configured and active backend must be Redis
- invalid cluster topology, missing default policy, driver-construction failure, or lightweight readiness failure: `503` with `Storage unavailable`

Recommended deployment usage:

- `/health` for liveness / basic probing
- `/health/ready` for Kubernetes readiness; final launch validation must also require `data.status = "ready"`

## `GET /health/memory`

Only `debug_assertions + openapi` builds register this endpoint.

It reports current heap allocation and peak usage as MB strings.

## `GET /health/metrics`

Only compiled when the `metrics` feature is enabled. Output is Prometheus text exposition.

If you need metrics, build with:

```bash
cargo build --release --features metrics
```

or:

```bash
cargo build --release --features full
```

The application layer does not add authentication here. In production, access must be restricted by reverse proxy, firewall, security group, or internal-only binding.

### Current metrics

HTTP and database:

| Metric | Labels | Notes |
| --- | --- | --- |
| `http_requests_total` | `method`, `route`, `status` | Request count |
| `http_request_duration_seconds` | `method`, `route`, `status` | Request latency histogram |
| `db_queries_total` | `backend`, `kind`, `status` | SeaORM query count |
| `db_query_duration_seconds` | `backend`, `kind`, `status` | SeaORM query latency histogram |

Auth, upload, download, and tasks:

| Metric | Labels | Notes |
| --- | --- | --- |
| `auth_events_total` | `action`, `status`, `reason` | Login and refresh-token events |
| `file_uploads_total` | `mode`, `status` | Upload outcomes across direct / chunked / presigned modes |
| `file_downloads_total` | `source`, `outcome`, `range` | Download outcomes |
| `upload_sessions_total` | `mode` | Created upload sessions |
| `upload_session_events_total` | `mode`, `event`, `status` | Session lifecycle events |
| `background_tasks_total` | `kind`, `status` | Task state transitions |
| `background_task_retries_total` | `kind` | Retry count |
| `background_tasks_pending` | none | Current `Pending` / `Retry` backlog |

Storage drivers and share rollback:

| Metric | Labels | Notes |
| --- | --- | --- |
| `storage_driver_operations_total` | `driver`, `operation`, `status`, `kind` | Driver operations |
| `storage_driver_operation_duration_seconds` | `driver`, `operation`, `status`, `kind` | Driver latency histogram |
| `share_download_rollback_events_total` | `event` | Rollback queue events after interrupted public-share downloads |
| `share_download_rollback_pending` | none | Pending rollback work |

Process metrics:

| Metric | Labels | Notes |
| --- | --- | --- |
| `process_memory_rss_bytes` | none | Resident set size |
| `process_cpu_milliseconds_total` | none | Total CPU time in milliseconds |
| `process_uptime_seconds` | none | Uptime since metrics initialization |

`process_cpu_milliseconds_total` is already exposed in milliseconds. `process_uptime_seconds` is monotonic rather than epoch-based.
