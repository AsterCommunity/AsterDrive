# Health Checks

Health checks are mounted at the repository root, not under `/api/v1`.

Both `primary` and `follower` nodes register this group.

## Endpoints

| Method | Path | Description |
| --- | --- | --- |
| `GET` / `HEAD` | `/health` | Liveness check |
| `GET` / `HEAD` | `/health/ready` | Readiness check, including database and storage availability |
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

`build_time` comes from the compile-time `ASTER_BUILD_TIME` value.

`HEAD /health` has the same semantics, but returns no body.

## `GET /health/ready`

This endpoint does more than a database ping. The current logic checks the database first, then performs a light readiness probe for the active node mode:

- `primary`: checks that the default storage policy exists, that the driver can be instantiated, and that low-cost prerequisites such as the local storage directory are present
- `follower`: checks the follower's active storage driver and the state required for binding

This endpoint is meant to stay cheap. It does not perform remote S3 or remote-storage read/write/delete probes. Use the admin policy "test connection" action when you need to verify object storage credentials or permissions.

Return semantics:

- all ready: `200`
- database unavailable: `503` with `Database unavailable`
- storage unavailable: `503` with `Storage unavailable`

Recommended deployment usage:

- `/health` for liveness / basic probing
- `/health/ready` for readiness / pre-rollout probing

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
