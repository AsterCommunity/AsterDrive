# AsterDrive Architecture Overview

This document describes what is already implemented in the repository today, not an early design sketch.

If you are new to the repository, read this page first and then [`module-designs.md`](./module-designs.md).

## 60-second version for new contributors

- AsterDrive is no longer a single-mode monolith. The same codebase now supports two node modes:
  - `primary`: serves the main REST API, public sharing, WebDAV, frontend pages, runtime configuration, and background tasks
  - `follower`: exposes only health checks and the internal object storage protocol, acting as a managed storage node for a remote primary
- Metadata mainly lives in the database, while file content mainly lives in storage drivers. They are linked through tables such as `files`, `file_blobs`, `file_versions`, and `upload_sessions`
- Personal spaces and team spaces share the same file pipeline; route and service layers switch scope through `WorkspaceStorageScope`
- The backend main path is still:
  `src/api/routes/*` -> `src/services/*` -> `src/db/repository/*` / `src/storage/*`
- WebDAV is not just another REST branch. It is mounted separately under `src/webdav/`
- The binary starts HTTP service by default. With the default `cli` feature enabled, the same entry point also exposes operational subcommands such as `doctor`, `config`, `database-migrate`, and `node enroll`
- Frontend code lives in `frontend-panel/`, and production assets are served directly by the primary node
- Configuration is split into two layers:
  - Static config: `data/config.toml` + `ASTER__...` environment variables
  - Runtime config: database `system_config`, with `src/config/definitions.rs` as the single source of truth

## Where to look first

| Question | Start here | Why |
| --- | --- | --- |
| How the service starts and how primary / follower are distinguished | `src/main.rs`, `src/config/node_mode.rs`, `src/runtime/startup/` | This defines the startup mode, runtime state, and node responsibilities |
| How operational CLI commands run | `src/main.rs`, `src/cli/**` | Subcommands under the `cli` feature are dispatched before HTTP startup |
| Which routes the primary node registers | `src/api/primary.rs`, `src/api/routes/` | This decides the registration order for `/api/v1`, `/health`, `/d`, `/pv`, WebDAV, and the frontend fallback |
| What the follower exposes | `src/api/follower.rs`, `src/api/routes/internal_storage.rs` | The follower only serves the internal storage protocol and health checks |
| How the remote-node reverse tunnel works | `src/api/routes/remote_tunnel.rs`, `src/storage/remote_protocol/tunnel/` | The primary exposes the tunnel control plane, and the follower connects back to it |
| How a REST endpoint is implemented | The corresponding `src/api/routes/**` file | Route handlers parse parameters, wrap auth, and adapt responses |
| Where file, team, share, and upload rules live | `src/services/**` | Business semantics are centralized in the service layer |
| How data is queried and written | `src/db/repository/**` | Repository code encapsulates database access and cross-database compatibility details |
| How file content reaches disk, object storage, OneDrive, or remote nodes | `src/storage/**` | Connector descriptors, driver abstractions, concrete drivers, and remote protocol code live here |
| Why WebDAV is different from REST | `src/webdav/**` | This is a separate protocol entry layer |
| Why team spaces reuse personal-space semantics | `src/services/workspace_scope_service.rs`, `src/services/workspace_storage_service/`, `src/services/workspace_storage_core.rs`, `src/services/files/folder/`, `src/services/files/file/` | Scope switching, upload orchestration, and the unified storage core all live here |
| How the schema evolves | `migration/`, `src/entities/**` | Migration files and entities need to be read together |

When you are chasing a specific feature, the fastest path is usually:

1. Find the entry point in `src/api/routes/**`
2. Jump to `src/services/**`
3. Then inspect `src/db/repository/**`, `src/storage/**`, or `src/webdav/**`

## Runtime modes and system boundaries

### Primary node

The primary node registers:

- REST API: `/api/v1/*`
- Remote-node reverse tunnel internal API: `/api/v1/internal/remote-tunnel/*`
- Health checks: `/health*`
- Public sharing and direct links:
  - `/api/v1/s/{token}*`
  - `/d/{token}/{filename}`
  - `/pv/{token}/{filename}`
- WebDAV: default `/webdav`
- Frontend pages and static assets: fall back through `src/api/routes/frontend.rs`
- Development OpenAPI:
  - `/swagger-ui`
  - `/api-docs/openapi.json`

### Follower node

The follower does not serve normal user APIs, WebDAV, or frontend pages. It only registers:

- Health checks: `/health*`
- Internal object storage protocol: `/api/v1/internal/storage/*`

This internal protocol is currently used for object writes, object assembly, object listing, binding synchronization, and managed ingress profile control between the primary node and managed remote nodes.

If a remote node uses `reverse_tunnel` or `auto` and has no directly reachable `base_url`, the follower does not expose an additional direct entry for the primary. Instead, the tunnel worker inside the follower process actively connects to the primary's `/api/v1/internal/remote-tunnel/*`.

## How a request flows

### Ordinary REST requests on the primary

1. `src/main.rs` enters `run_primary_http_server()`
2. `src/api/primary.rs` registers the route modules under `/api/v1`
3. The request passes through global middleware:
   - compression
   - request ID
   - runtime CORS
   - security response headers
4. The corresponding handler in `src/api/routes/**` is hit
5. Protected endpoints then go through route-level JWT auth and rate limiting
6. `src/services/**` applies business rules
7. `src/db/repository/**` handles database reads and writes; binary content flows into `src/storage/**`
8. The route layer returns unified JSON, or directly returns file streams / SSE / WebDAV / Prometheus text responses

Things to remember:

- `/d/...`, `/pv/...`, file downloads, thumbnails, and share downloads do not use the unified JSON wrapper
- `GET /api/v1/auth/events/storage` is SSE
- `GET /health/metrics` is Prometheus text exposition
- The frontend fallback route is registered last, so API / WebDAV routes must be registered before it

### Internal storage requests on the follower

1. `src/api/follower.rs` only registers `/api/v1/internal/storage/*`
2. `src/api/routes/internal_storage.rs` validates internal signatures or presigned access
3. `remote::master_binding` resolves primary-node bindings and ingress policies
4. `driver_registry` returns the actual storage driver
5. The request is handled by the local / object-storage / remote driver capability interface

If you are debugging remote-node writes, do not start in the normal `files` / `upload` routes.

Remote nodes have two transport modes:

- `direct`: the primary sends HTTP requests straight to the follower's `/api/v1/internal/storage/*`
- `reverse_tunnel`: the primary registers the internal storage request in the tunnel registry; the follower pulls the request through `/api/v1/internal/remote-tunnel/poll` / `/complete` or via the `/connect` WebSocket and returns the result

`auto` selects `direct` or `reverse_tunnel` based on whether the remote node has a non-empty `base_url`.

### WebDAV requests

WebDAV does not go through `src/api/routes/**`. Instead:

1. `crate::webdav::configure()` mounts it on the configured prefix on the primary
2. It checks the runtime `webdav_enabled` switch
3. It performs WebDAV-specific Basic Auth
4. It builds a user-scoped `AsterDavFs`
5. It uses the database lock system and version capability support
6. It enters the custom WebDAV / DeltaV handler

## Layered structure

```text
┌─────────────────────────────────────────────┐
│ Entry layer                                 │
│  - React frontend / public sharing pages    │
│  - REST API (primary)                       │
│  - Internal Storage API (follower)          │
│  - WebDAV / DeltaV                          │
├─────────────────────────────────────────────┤
│ Application layer                           │
│  - Routes, DTOs, unified responses, error codes
│  - Middleware for JWT / Admin / Rate Limit / CORS
├─────────────────────────────────────────────┤
│ Business layer                              │
│  - auth / profile / team / file / folder    │
│  - upload / batch / share / trash / task    │
│  - policy / config / audit / webdav / wopi  │
│  - workspace scope / storage core           │
├─────────────────────────────────────────────┤
│ Infrastructure layer                        │
│  - SeaORM + migration                       │
│  - StorageConnector descriptor / action     │
│  - StorageDriver(Local/S3/SFTP/Azure)       │
│  - StorageDriver(Tencent COS/OneDrive)      │
│  - StorageDriver(Remote)                    │
│  - CacheBackend(Memory / Redis)             │
├─────────────────────────────────────────────┤
│ Data layer                                  │
│  - users / teams / team_members             │
│  - folders / files / file_blobs / versions  │
│  - shares / upload_sessions / tasks         │
│  - webdav_accounts / system_config / locks  │
└─────────────────────────────────────────────┘
```

The practical rule of thumb in this repository remains:

- route layer handles HTTP / protocol adaptation
- service layer handles business semantics
- repo layer handles database reads and writes
- storage layer handles object content

## Key modules

| Module | Current responsibility |
| --- | --- |
| `src/main.rs` | Process entry, node-mode selection, HTTP server startup, graceful shutdown |
| `src/runtime/startup/common.rs` | Connect database, run migrations, prepare default policies and runtime config, load policy snapshot / driver registry / cache |
| `src/runtime/startup/primary.rs` | Build the primary runtime: `RuntimeConfig`, mail sender, SSE broadcaster, share-download rollback queue, and remote protocol runtime |
| `src/runtime/startup/follower.rs` | Build the follower runtime: keep only the shared state needed by the follower |
| `src/runtime/tasks.rs` | Register and shut down primary periodic tasks; metrics system tasks are injected through `MetricsRecorder` |
| `src/metrics_core.rs` | Always-compiled metrics recording trait and `NoopMetrics`; business code only depends on this layer |
| `src/metrics.rs` | Concrete Prometheus implementation, compiled only when the `metrics` feature is enabled |
| `src/api/primary.rs` | Primary route registration |
| `src/api/follower.rs` | Follower route registration |
| `src/api/routes/auth/mod.rs` | Authentication, sessions, preferences, avatars, SSE |
| `src/api/routes/files/` | File read/write, uploads, thumbnails, versions, WOPI startup |
| `src/api/routes/folders.rs` | Folder endpoints and the team-space aggregation entry; team `files` routes are mounted here |
| `src/api/routes/tags.rs` | Personal and team workspace tag libraries, entity tag bindings, and batch tag operations |
| `src/api/routes/admin/` | Admin backend endpoints, including policies, remote nodes, users, teams, share audit, background tasks, storage migration, file / blob observability, config, locks, and audit |
| `src/api/routes/share_public.rs` | Public sharing API, `/d` direct links, and `/pv` preview links |
| `src/api/routes/internal_storage.rs` | Follower internal object storage protocol |
| `src/api/routes/remote_tunnel.rs` | Primary-side remote-node reverse tunnel internal entry |
| `src/services/` | Central business rule layer |
| `src/storage/connectors/` | Storage connectors: descriptors, fields, actions, connection tests, upload workflows, and credential requirements |
| `src/storage/drivers/` | Local, S3-compatible, SFTP, Azure Blob, Tencent COS, OneDrive, and remote drivers |
| `src/storage/remote_protocol/tunnel/` | Reverse tunnel transport runtime, auth, registry, and streaming responses |
| `src/webdav/` | WebDAV filesystem, auth, locks, and DeltaV support |
| `frontend-panel/` | React 19 + Vite frontend; build artifacts are served by the backend |

## Startup flow

### Common startup steps

The rough order in `src/main.rs` is currently:

1. Install the panic hook
2. Load `.env`
3. If the `cli` feature is enabled and a CLI subcommand is present, run it and exit immediately
4. Initialize static config
5. Initialize logging
6. Clean runtime temp directories
7. Choose `primary` or `follower` according to `config.server.start_mode`

Prometheus metrics are not initialized directly in `main.rs`. They are created inside `prepare_common()` as `MetricsRecorder`:

- when the `metrics` feature is enabled, Prometheus registry initialization is performed and a Prometheus recorder is injected
- when it is disabled, `NoopMetrics` is injected
- business code, HTTP middleware, storage-driver wrappers, and background tasks only depend on the trait in `src/metrics_core.rs`, not on Prometheus directly

### `prepare_common()`

`src/runtime/startup/common.rs` performs the shared preparation for all nodes:

1. Create `MetricsRecorder`, so database connections and later runtime state can share the same recorder
2. Connect to the database
3. Run all migrations
4. Prepare SQLite search acceleration if the current backend supports it
5. Ensure at least one default local storage policy exists
6. Seed the default policy group only in primary mode
7. Initialize the `auth_cookie_secure` bootstrap value
8. Write default values into `system_config`
9. Clean deprecated `node_runtime_mode` and old thumbnail runtime config keys
10. Reload `PolicySnapshot`
11. Reload `DriverRegistry` according to the node mode
12. Initialize the cache backend

### Database handles

Runtime state keeps both writer and reader handles through `DbHandles`:

- `state.db` / `state.writer_db()` is the writer. All transactions, writes, read-after-write paths, quota authority checks, login session issuance, refresh token rotation, upload init/chunk/complete/cancel, and repository helpers that rely on SQLite single-connection lock emulation must continue to use the writer.
- `state.reader_db()` is the pure read entry point. Under SQLite file databases it opens an independent reader pool after writer-side migration and bootstrap are complete, with WAL, `mode=ro`, and `PRAGMA query_only=ON`. Under PostgreSQL / MySQL or in-memory SQLite it points to the same pool as the writer.
- Reader queries are allowed to lag briefly at WAL snapshot level. They should only be used for lists, details, search, upload progress, recoverable sessions, presign query stages, auth snapshot cache misses, public runtime snapshots, and admin overview statistics that do not immediately feed into authoritative write decisions.
- Do not silently convert shared validation helpers to reader access unless every caller is confirmed to be read-only. Prefer choosing `reader_db()` or `writer_db()` explicitly at the service entry so the semantic choice is visible in code.

### Primary-specific startup

`src/runtime/startup/primary.rs` additionally prepares:

- `RuntimeConfig`
- runtime mail sender
- storage-change broadcast channel
- share-download rollback queue
- `RemoteProtocolRuntime`, including the reverse-tunnel registry, injected into `DriverRegistry`

After that, `src/api/primary.rs` registers the main routes and `src/runtime/tasks.rs` starts primary periodic tasks.

### Follower-specific startup

`src/runtime/startup/follower.rs` keeps only the shared state needed by the follower.

Then `src/api/follower.rs` registers only:

- `/api/v1/internal/storage/*`
- `/health*`

`spawn_follower_background_tasks(state)` currently starts follower-safe shared metrics background work and the reverse-tunnel follower worker. It does not start primary business cleanup tasks or `background-task-dispatch`.

## Background Tasks

Primary background work is registered by `src/runtime/tasks.rs` and is split into one resident worker plus periodic tasks:

- resident worker: `share-download-rollback`
- periodic tasks:
  - `mail-outbox-dispatch`
  - `background-task-dispatch`
  - `upload-cleanup`
  - `completed-upload-cleanup`
  - `blob-reconcile`
  - `system-health-check`, including database, cache, and remote-node health checks
  - `trash-cleanup`
  - `team-archive-cleanup`
  - `lock-cleanup`
  - `auth-session-cleanup`
  - `external-auth-flow-cleanup`
  - `mfa-flow-cleanup`, covering MFA login flows, TOTP setup flows, and email codes
  - `audit-cleanup`
  - `task-cleanup`
  - `wopi-session-cleanup`

Periodic tasks run according to runtime config intervals. They write `SystemRuntime` records only when there is an actual result or failure; empty polls use `RuntimeTaskRunOutcome::quiet()`. Consecutive healthy `system-health-check` successes refresh the latest success record instead of creating noisy rows every round.

User-visible `background_tasks` records are dispatched by `background-task-dispatch`. The dispatcher currently uses four lanes:

- `Archive`: `archive_compress`, `archive_extract`, `archive_preview_generate`
- `Thumbnail`: `thumbnail_generate`, `image_preview_generate`, `media_metadata_extract`
- `StorageMigration`: `storage_policy_migration`
- `Fallback`: `storage_policy_temp_cleanup`, `trash_purge_all`, `blob_maintenance`, `system_runtime`

The first three lanes have their own runtime concurrency settings. Fallback uses the generic `background_task_max_concurrency`.

After claiming a task, the dispatcher creates a `TaskExecutionContext` for business execution. The context carries both the processing-token lease and the graceful-shutdown token; `main.rs` injects the same token into the HTTP server, SSE, and background-task stack, so SIGINT / SIGTERM starts all of them winding down together. Task code, download polling, and blocking archive compression / extraction workers should use this context for activity checks. Only lower-level helpers that write progress, runtime metadata, or final state should receive `TaskLeaseGuard` directly. During service shutdown, the context makes execution exit cooperatively; the dispatcher then releases a still-matching processing token back to `Retry` without spending retry budget.

## CLI and Offline Operations

The default features in `Cargo.toml` include `cli`, so the default `aster_drive` binary can either start the service or run offline operational subcommands. `src/main.rs` parses these before HTTP startup:

| Subcommand | Code entry | Current responsibility |
| --- | --- | --- |
| `serve` or no subcommand | `src/main.rs` | Start primary / follower HTTP service |
| `doctor` | `src/cli/doctor.rs`, `src/cli/doctor/**` | Database, migration, runtime config, storage policy, and deep consistency audits |
| `config` | `src/cli/config.rs` | Offline read, set, import, export, and validation for `system_config` |
| `database-migrate` | `src/cli/database_migration.rs`, `src/cli/database_migration/**` | Cross-database backend migration with dry-run, verify-only, and resume support |
| `node enroll` | `src/cli/node.rs` | Follower writes local master binding using an enrollment token issued by the primary |

These CLI commands usually connect directly to the database and do not go through HTTP route handlers. When changing them, start in `src/cli/**` and the corresponding service, not in `src/api/routes/**`.

## Configuration Layers

### Static config

Static config comes from:

- `data/config.toml`
- `ASTER__...` environment variables

It mainly controls:

- listen address, port, and worker count
- node startup mode
- database connection
- WebDAV prefix
- cache and logging
- follower remote storage target local root: `server.follower.remote_storage_target_local_root`, default `remote-storage-targets`

The first startup creates `data/config.toml` automatically. Relative paths in the config file are resolved relative to `data/` by default. For compatibility, old values already written as `data/...` avoid being expanded into `data/data/...`. The old root-level `config.toml` is no longer the default read location.

### Runtime config

Runtime config is stored in the database `system_config` table and hot-updated through admin APIs.

The single source of truth is `src/config/definitions.rs`. Common keys include:

- `webdav_enabled`
- `webdav_block_system_files_enabled`
- `webdav_block_system_file_patterns`
- `default_storage_quota`
- `trash_retention_days`
- `team_archive_retention_days`
- `max_versions_per_file`
- `auth_cookie_secure`
- `auth_*_ttl_secs`
- `auth_email_code_login_*`
- `public_site_url`
- `cors_*`
- `mail_outbox_dispatch_interval_secs`
- `background_task_dispatch_interval_secs`
- `background_task_dispatch_idle_max_interval_secs`
- `background_task_max_concurrency`
- `background_task_archive_max_concurrency`
- `background_task_thumbnail_max_concurrency`
- `background_task_storage_migration_max_concurrency`
- `background_task_max_attempts`
- `share_download_rollback_queue_capacity`
- `share_stream_session_ttl_secs`
- `maintenance_cleanup_interval_secs`
- `blob_reconcile_interval_secs`
- `remote_node_health_test_interval_secs`
- `task_retention_hours`
- `archive_extract_*`
- `archive_build_*`
- `archive_preview_*`
- `archive_extract_max_staging_bytes`
- `thumbnail_max_source_bytes`
- `thumbnail_max_dimension`
- `image_preview_max_dimension`
- `media_metadata_enabled`
- `media_metadata_max_source_bytes`
- `media_processing_registry_json`
- `wopi_*`

`system_config.category` only uses category constants registered in `src/config/definitions.rs`. Current categories include:

- `site` / `site.preview`: public site entry, branding, and preview apps
- `user.registration_and_login` / `user.avatar`: registration, login, and avatars
- `auth`: auth cookies and token TTLs
- `mail.config` / `mail.template`: mail sending and templates
- `network`: CORS and network-access rules
- `runtime.mail` / `runtime.background_task` / `runtime.maintenance` / `runtime.limits` / `runtime.share_stream`: runtime dispatch, maintenance, and limits
- `storage`: versioning, trash, team archive retention, and default quota
- `file_processing.archive_extract` / `file_processing.archive_preview` / `file_processing.archive_build` / `file_processing.media`: archive and media processing
- `webdav` / `audit`: WebDAV and audit logging

Adding a category requires updating the allowed list and frontend zh/en i18n. Unit tests around `ALL_CONFIGS` reject unregistered categories and verify that frontend titles and descriptions exist for second-level categories.

`public_site_url` is historically singular but stores a list. Its config type is `string_array`, the admin API exposes it as a string array, and the database stores a normalized JSON string array. When generating absolute URLs, request-aware paths first try to exactly match the current request scheme/Host against the configured list; without request context or without a match, the first item is used as fallback. This config also participates in same-site CSRF origin checks for cookie-authenticated writes, but it does not grant CORS access.

## Where Changes Should Go

| Change | Preferred layer |
| --- | --- |
| New primary REST endpoint | `src/api/routes/**` |
| New follower internal protocol capability | `src/api/routes/internal_storage.rs`, `src/storage/remote_protocol.rs` |
| Permissions, quota, locks, versions, share scope, team semantics | `src/services/**` |
| New query, pagination, or filter condition | `src/db/repository/**` |
| Storage connector descriptors, connection tests, driver actions, upload strategies, and object read/write rules | `src/storage/**` |
| WebDAV protocol behavior | `src/webdav/**` |
| Table fields, indexes, defaults | `migration/` + `src/entities/**` |
| Frontend page, state management, SDK call | `frontend-panel/src/**` |

Complex business logic in the route layer is usually a code smell.

## Further Reading

- [`module-designs.md`](./module-designs.md)
- [`api/index.md`](./api/index.md)
- [`testing.md`](./testing.md)
