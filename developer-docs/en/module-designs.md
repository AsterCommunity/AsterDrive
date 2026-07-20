# Core Module Design Notes

This document supplements the repository-wide view from [`architecture.md`](./architecture.md) and explains a few core modules that are already implemented but hard to understand just from directory names and endpoint names.

This is the current implementation design, not a future plan and not an idealized rewrite proposal.

Current coverage:

- unified workspace storage pipeline
- sharing service
- background-task system
- storage-policy migration tasks
- admin file / blob observability
- `doctor` / consistency audits
- cross-database migration CLI

If you only want to know where requests enter and which layer to modify, start with [`architecture.md`](./architecture.md). If you already know the entry point but do not yet understand why a module is split the way it is, keep reading.

## 1. Unified workspace storage pipeline

Main code paths:

- `src/services/workspace/scope/`
- `src/services/workspace/storage/`
- `src/services/workspace/models.rs`
- `src/services/workspace/storage_core/`
- `src/services/files/file/*`
- `src/services/files/folder/*`

### Design goal

This pipeline solves a very specific problem: personal spaces and team spaces have highly similar file semantics, but permissions, quota ownership, and default policy-group ownership are not the same.

If we keep two complete service stacks for the two space types, we quickly get these problems:

- file, folder, upload, share, trash, and task rules must be implemented twice
- new features are easy to add to one path and forget in the other
- behavior diverges over time when regressions are fixed in only one space type

The core idea is:

1. The route layer only maps a request into a `WorkspaceStorageScope`
2. The service layer reuses the unified file pipeline as much as possible
3. Only the places that truly depend on workspace identity differences branch on scope

### Core abstraction: `WorkspaceStorageScope`

`WorkspaceStorageScope` has only two shapes:

- `Personal { user_id }`
- `Team { team_id, actor_user_id }`

This type is not about saving parameters. It is about freezing the two dimensions that matter: who owns the resource and who is acting on it.

In personal space the two are usually the same user. In team space they are not:

- `team_id` means the workspace that owns the resource
- `actor_user_id` means the member who initiated the operation

So a function that accepts only `scope` already receives the minimum complete context needed for later permission checks.

### Layer split

The unified storage pipeline is currently split into three layers:

1. `workspace::scope`
   Handles scope access checks and whether a file/folder belongs to that workspace
2. `workspace::storage`
   Assembles upload, persistence, pre-upload blob, and multipart entry points into one workflow
3. `workspace::storage_core`
   Handles more stable core actions such as policy resolution, quota read/write, and blob / file-record creation

The point of this split is not file size. It is to separate logic with different change rates:

- scope rules change as team features evolve
- upload modes and entry paths will keep growing
- core persistence and billing rules are relatively stable and should become the base layer

### Main workflow

No matter whether the entry point is REST upload, directory upload, WebDAV flush, or background archive import, the chain tries to converge on the same semantics:

1. Confirm that the scope can be accessed
2. Verify that the target folder belongs to the current workspace
3. Resolve which storage policy the final file should use
4. Choose an upload mode according to driver type and policy
5. Create or reuse a blob
6. Create or overwrite the `files` record
7. Update personal or team `storage_used`
8. After the transaction, clean up storage side effects, broadcast changes, and do other follow-up work

A few easy-to-misread points:

- Personal and team spaces share the same file pipeline, but quota ownership is different
- A folder may override the storage policy; otherwise the system falls back to the user's default policy or the team's policy group
- Local storage may use content deduplication; object-storage, OneDrive, and Remote paths do not deduplicate by default
- “Upload succeeded” does not mean only that an object was written. It also includes database state transitions and quota accounting

### Why policy resolution belongs in the unified pipeline

Policy selection is not just config reading. It is part of business semantics. It affects:

- which storage driver ultimately receives the content
- whether the current file size is allowed
- whether local content deduplication is enabled
- whose quota gets charged

If policy resolution lived in the route layer, every caller would need to reimplement the rule that folder overrides win over default policy groups. That would quickly produce multiple inconsistent policy decisions in one repository.

The current rule is:

1. Check whether the target folder explicitly binds a policy
2. Personal space uses the user's bound policy or policy group
3. Team space uses the team's bound policy group

This keeps policy decisions in the same layer as workspace semantics instead of scattering them across handlers, repositories, or drivers.

### Why quota checking is two-phase

File writes involve external storage side effects. If we only checked once at the end of the transaction, users would still go through the full upload path even when they are obviously over quota, which is bad for both experience and resource use.

So the current design intentionally uses two phases:

- a fast-fail check outside the transaction
- an authoritative check again right before the final write inside the transaction

The outer check only reduces wasted work. It does not provide the final consistency guarantee. The final guarantee still belongs to the transaction-side check and atomic update.

### Key constraints

This pipeline currently depends on several constraints that should not be broken casually:

- scope checks must happen before resource access
- `files` / `folders` workspace ownership checks must not bypass `ensure_*_scope`
- quota writes must stay consistent with file-record creation
- local deduplication is a storage-policy capability, not a global default
- database accounting finishes inside the transaction, and irreversible side effects happen after the transaction

If you add a new file entry point later, the first question is not “where should this endpoint live?” but “can it reuse this unified pipeline?” Only when the business semantics are truly different should you open a new branch.

## 2. Sharing service

Main code paths:

- `src/services/share/mod.rs`
- `src/services/share/management.rs`
- `src/services/share/content.rs`
- `src/services/share/shared.rs`

### Design goal

The sharing service exposes internal resources to anonymous or semi-anonymous visitors without letting the public sharing page directly inherit the internal permission model.

So the current design intentionally splits sharing into two paths:

- management path: create, update, delete, and list shares using authenticated user / team-member permissions
- public access path: read share content through a token, trusting only share state, not the original login state

Both paths use the same `shares` data, but they do not share the authentication precondition.

### Share object model

REST share creation uses `target: { type, id }` to describe the target. The service maps that to a single-target persistent model. A share can only point to one resource:

- file share: `file_id` is set, `folder_id` is empty
- folder share: `folder_id` is set, `file_id` is empty

This is not a shortcut. It keeps download counters, expiration, password checks, and public tokens attached to one public resource.

Compared with “copying share state onto every child under a folder,” this design is much easier to keep consistent:

- state is centralized
- lifecycle is singular
- public token stays stable
- counting semantics stay clear

### Why share creation locks the resource

`create_share_in_scope()` locks the target file / folder inside the transaction before checking whether an active share already exists.

This is there to prevent duplicate creation under concurrency.

The intended semantics are:

- within the same space, a resource keeps at most one active share
- an expired share can be removed and recreated

Without the lock, two concurrent requests can both pass the check before either sees the other's new row, and the result would be two active shares.

### Why public access does not blindly trust the target

The public-access path loads the share first and then re-checks the target file / folder to validate all of the following:

- whether the share has expired
- whether the download limit has already been reached
- whether the target resource still exists
- whether the target resource still belongs to the shared space
- for folder shares, whether child files / folders still remain inside the shared subtree

That check cannot be skipped, because the share token only says “someone once allowed public access to this resource,” not “the resource is still valid now.”

### Folder-share boundary control

The dangerous part of folder sharing is usually not the root folder itself. It is how child resources are constrained to the shared subtree.

The current design uses two levels of constraints:

1. First verify that the target file / folder and the share belong to the same workspace
2. Then use `verify_folder_in_scope()` to verify that it is inside the descendant tree of the share root

This is necessary because matching only `team_id` or `user_id` is not enough to prove it belongs to the actual share range.

### Password and counter design

Share passwords are not stored in plaintext; they are hashed the same way ordinary login passwords are hashed.

The public-access counters are also deliberately split:

- `view_count` means view count only
- `download_count` means actual download count only
- `max_downloads = 0` means unlimited, not “downloads forbidden”

Incrementing and rolling back the download counter uses dedicated atomic repository operations, because high-concurrency public access needs a small over-limit window.

### Why the sharing service does not own the original resource lifecycle

The sharing service intentionally does not create immutable snapshots or private read-only copies for shared files.

That means:

- a share always reflects the current resource state
- if the source resource is moved to trash or leaves the shared range, public access stops working
- a share is not an archive snapshot; it is just a constrained public entry point

The advantage is simplicity, low storage cost, and immediate visibility of source changes. The cost is that the share naturally inherits the source resource lifecycle.

If we ever introduce “immutable sharing” or “version-pinned public snapshots,” that will be a different product semantic and should not be forced into the current sharing pipeline.

## 3. Background task system

Main code paths:

- `src/services/task/mod.rs`
- `src/services/task/spec/`
- `src/services/task/registry.rs`
- `src/services/task/dispatch/`
- `src/services/task/runtime.rs`
- `src/services/task/offline_download/`
- `src/services/task/storage_policy_cleanup.rs`
- `src/services/task/storage_migration.rs`
- `src/db/repository/background_task_repo/`
- `src/db/repository/storage_migration_checkpoint_repo.rs`

### Design goal

The background-task system currently handles two classes of work:

- user-visible and potentially slow business tasks, such as archive compression, extraction, and thumbnail generation
- execution records for system periodic tasks, such as cleanup, dispatch, and health checks

It is not an independent task service or an external queue. It is a persistent product task subsystem. Generic execution primitives come from `aster_forge_tasks`, including the typed spec contract, execution context, lane configuration, leases, and scheduled-task coordination.

The core goals are:

- provide recoverable asynchronous execution inside one process
- let the API and admin UI query task state directly
- avoid letting worker restarts or concurrent execution write stale results back into fresh state

### Task type contract

Each business task first implements `BackgroundTaskSpec` under `src/services/task/spec/` and is then registered in `src/services/task/registry.rs`. The spec is the single type contract for:

- `BackgroundTaskKind`
- strongly typed payload / result codecs
- initial steps
- lane assignment
- max attempts and retry class
- process entry point

Task creation must use the typed create helpers instead of constructing `background_task::ActiveModel` or hand-writing `payload_json` / `result_json`. Otherwise the dispatcher, presentation layer, retry policy, and creation path can drift into separate `kind` match tables.

### Why `background_tasks` is both a queue and a history table

The repository does not keep a separate queue table and history table. Instead, `background_tasks` carries:

- queued work
- leased processing state
- completed and failed records
- progress, steps, error, and result summaries for the UI

The benefits are:

- one source of truth
- no cross-table stitching for API / admin pages
- retry, cleanup, and retention can operate on the same row

The trade-off is a heavier table, but that is acceptable for the current monolithic scale.

### Claim model: lease plus fencing token

The most important design point is not “how to run the task” but “how to stop an old worker from overwriting a new result.”

Current flow:

1. the dispatcher picks claimable tasks from the database
2. claiming atomically increments `processing_token`
3. all heartbeats, progress writes, completion writes, and failure writes include that token
4. only matching-token writes are allowed to succeed

This is a standard fencing-token pattern. Once an old worker token expires, its later database writes must fail and cannot overwrite state with stale success or failure.

### Why `TaskLeaseGuard` still exists

Fencing tokens alone stop stale database writes, but they do not stop stale local side effects.

For long tasks such as compression or extraction, work may continue inside `spawn_blocking` even after the database lease has been lost. That still wastes resources and can cause collisions.

So there is a second layer:

- successful heartbeat or status write refreshes the local lease
- if the lease is lost or not renewed for too long, the execution flow should terminate itself

So the two protections are:

- `processing_token` prevents stale database writes
- `TaskLeaseGuard` makes stale workers stop local execution quickly

### Execution context and shutdown semantics

Business task entry points receive `TaskExecutionContext`, not a bare `TaskLeaseGuard`. The context ties together:

- the lease guard for the current processing token
- the cancellation token for process graceful shutdown

Task implementations and long-running helpers should call `context.ensure_active()`, `context.sleep_or_shutdown()`, or `context.shutdown_requested()`. This keeps regular async flow, download polling, and `spawn_blocking` archive compression / extraction loops under the same cooperative shutdown contract.

Tokio cannot forcibly interrupt a `spawn_blocking` closure that has already started running. The runtime shutdown grace period only waits for workers to exit cooperatively; if blocking code does not check `TaskExecutionContext` periodically, aborting the outer async handle after the grace period does not stop already-running blocking work. Compression, extraction, bulk copy, and similar blocking loops must therefore place `context.ensure_active()` checkpoints inside the loop body instead of relying only on outer future cancellation.

`TaskLeaseGuard` still exists, but it is a lower-level fencing and heartbeat implementation detail. Helpers that write progress, runtime metadata, or final state still need the guard because those writes include the processing token. New business task code and helpers that wait on I/O, sleep, or run long loops should not treat a bare guard as their execution context.

Graceful shutdown is not a business failure. When a worker exits because `TaskExecutionContext` observes shutdown, the dispatcher releases the row from `Processing` back to `Retry` with the current processing token, clears the lease fields, and wakes the dispatcher. This does not increment `attempt_count` and does not write `last_error`. If the token no longer matches, the release is blocked by the normal fencing condition so an old worker cannot overwrite a newer worker state.

### Heartbeats and stale reclaim

The dispatcher renews heartbeats periodically. The database tracks:

- `last_heartbeat_at`
- `lease_expires_at`

If the process crashes, the task stalls, or the node changes, another dispatcher can reclaim tasks that exceed the stale threshold.

The implementation is intentionally conservative:

- heartbeat write failures do not immediately kill the task
- the current worker only self-terminates once the lease truly expires

That avoids briefly flaky storage or database issues from creating duplicate workers.

### Retry model

Failed tasks do not always restart immediately. The decision depends on:

- current attempt count
- task kind
- whether the failure is retryable

Depending on those inputs, the task may move to:

- `Failed`
- `Retry`
- or continue under a new lease

So “failed” here is not a single meaning. It is a result that includes retry budget and lease state.

### Lane-based scheduling

The dispatcher does not use one global pool for everything. It schedules by lane:

- `Archive`: `archive_compress`, `archive_extract`, `archive_preview_generate`, limited by `background_task_archive_max_concurrency`
- `Thumbnail`: `thumbnail_generate`, `image_preview_generate`, `media_metadata_extract`, limited by `background_task_thumbnail_max_concurrency`
- `OfflineDownload`: `offline_download`, limited by `offline_download_max_concurrency`
- `StorageMigration`: `storage_policy_migration`, limited by `background_task_storage_migration_max_concurrency`
- `Fallback`: `storage_policy_temp_cleanup`, `trash_purge_all`, `blob_maintenance`, and system runtime records, limited by `background_task_max_concurrency`

Archive preview is read-only, but it still touches object storage and ZIP parsing, so it shares the archive lane. Image preview generation and media metadata parsing also read raw objects and spend CPU time, so they share the thumbnail lane.

Archive and thumbnail lanes keep pulling the next batch within one dispatch round so large clusters of the same kind do not have to wait for the next periodic tick. OfflineDownload is isolated so slow external transfers do not consume archive, thumbnail, or maintenance capacity. StorageMigration is isolated to avoid occupying the other business lanes. Fallback is intentionally conservative so maintenance work does not starve other lanes.

`storage_policy_temp_cleanup` is the backstop cleanup task after force-deleting a storage policy. If temporary objects or multipart uploads still need to wait for presigned URLs to expire, the server schedules this task.

`trash_purge_all` is created when a user or team empties trash. It stays in the fallback lane because it mainly does batch database traversal, physical deletion, and one final sync event, and it should not consume archive or thumbnail budget.

`blob_maintenance` is the admin task for blob integrity checks, reference-count repair, and orphan cleanup. It stays in the fallback lane for the same reason.

`storage_policy_migration` is isolated because it can read the source driver, write the target driver, update blob references, and needs its own recovery checkpoint.

### Why system periodic tasks use the same table

`runtime.rs` records noteworthy system periodic task results in `background_tasks`, but those rows use `SystemRuntime` and are never dispatched again. Quiet empty polls do not write rows; successful health checks refresh the most recent success row instead of creating noise.

This keeps one shared observability surface for both user tasks and system tasks without pretending that system tasks are the same as ordinary user tasks.

### Why steps and results are stored as JSON

Task steps, input payloads, and execution results are serialized into JSON fields rather than separate tables for every task type.

That works because:

- task kinds are still growing
- each kind has a different step structure
- the UI mainly needs a limited generic view, not deeply relational queries

So the system behaves more like a persisted state machine snapshot than a full orchestration platform.

## 4. Storage-policy migration tasks

Main code paths:

- `src/api/routes/admin/storage_migrations.rs`
- `src/services/task/storage_migration.rs`
- `src/db/repository/storage_migration_checkpoint_repo.rs`
- `src/entities/storage_migration_checkpoint.rs`

This is not “change policy A to policy B.” It actually migrates the blob content owned by `file_blobs.policy_id = source_policy_id` to the target policy, then updates the database to point each blob at the new policy and storage path.

### Why dry-run exists

`POST /admin/storage-migrations/dry-run` does not create a task. It only preflights and estimates:

- how many blobs live under the source policy and how many bytes they represent
- how many blobs can be merged by content SHA-256 versus opaque hash
- how many hashes already exist in the target policy
- whether the target driver supports stream upload
- whether the target driver can complete a write / delete probe

That answers “what would this task do, and is the target obviously writable?” It does not provide a final consistency guarantee. The execution phase rechecks policy freshness and driver capability.

### Why checkpoints exist

Migration jobs can run for a long time, so the system stores a checkpoint in `storage_migration_checkpoints`:

- `task_id` binds the checkpoint to the task
- `source_policy_id` / `target_policy_id` lock the direction
- `plan_hash` captures the policy version and parameters used at creation time
- `stage` tracks prepare / migrate / finish
- `last_processed_blob_id` and counters let the resumer continue scanning from the right place

If a task fails or the process exits, an admin can resume it through `/admin/storage-migrations/{task_id}/resume`.

### Merge, skip, and failure

During migration, each blob is reloaded from the latest database state:

- if the blob is no longer under the source policy, it is counted as skipped
- if the target already has a matching content hash, the references are merged
- otherwise, the object is read from the source driver and streamed to the target driver, then the database reference is updated
- one blob failure increases the failure count and sends the task through retry handling

The first version does not support `delete_source_after_success = true`. The field is still present in the API, but a `true` value is rejected so the client does not assume old-policy objects will be auto-cleaned.

## 5. Admin file / blob observability

Main code paths:

- `src/api/routes/admin/files.rs`
- `src/services/files/admin.rs`
- `src/db/repository/file_repo/`

This surface is for admin debugging and migration validation, not ordinary file business flows.

Two query tracks exist:

- file view: `/admin/files` and `/admin/files/{id}` show file records, current blobs, and version summaries
- blob view: `/admin/file-blobs` and `/admin/file-blobs/{id}` show blob records, hash kind, reference count, and the files / versions that reference them

These use reader connections only and do not cause business side effects. Typical uses:

- inspect which blobs remain under one policy
- see which files or versions reference a particular blob
- confirm blob policy, path, and ref-count changes before and after migration
- debug the behavior difference between content SHA-256 blobs and opaque blobs

`hash_kind` is a derived display value, not a stored database field: 64 hex characters means `content_sha256`; everything else is `opaque`.

## 6. `doctor` / consistency audit

Main code paths:

- `src/cli/doctor/mod.rs`
- `src/cli/doctor/execute.rs`
- `src/cli/doctor/storage_scan.rs`
- `src/services/ops/integrity.rs`
- `src/storage/traits/driver.rs`

### Design goal

`doctor` is not a normal business service. It is an operational diagnosis entry point.

It answers questions such as:

- is the deployment environment usable
- is migration complete
- can runtime config be loaded
- is there long-term drift between storage and the database

Those questions should not be buried inside online requests or hidden behind background tasks only. Operators need a place they can trigger directly and get a structured report from.

### Layering

`doctor` is split into two layers:

1. `src/cli/doctor/mod.rs`
   handles argument parsing, mode selection, report aggregation, human-readable output, and JSON output
2. `src/services/ops/integrity.rs`
   handles the real deep audit and some repair logic

The point is to separate “how the result is shown” from “how the actual system state is calculated.”

### Shallow checks vs. deep checks

Default checks focus on environment usability:

- database connection
- migration state
- SQLite search acceleration
- runtime config snapshot loading
- public-site URL / mail / preview-app config
- basic storage-policy availability

`--deep` enters consistency auditing, currently including:

- `storage_usage`
- `blob_ref_counts`
- `storage_objects`
- `folder_tree`

Deep checks are slower and may touch object storage, so they should not be the default cost of a quick health run.

### Why consistency audit scans in batches

Many drift problems cannot be found by a single business request, for example:

- `users.storage_used` differs from actual file usage
- `file_blobs.ref_count` differs from actual references
- orphan objects exist in storage but not in the database
- the folder tree has cross-workspace parent-child links or cycles

These all require a global view, cross-table aggregation, and offline batch scanning. They are not good candidates for inline write-and-fix logic inside online requests.

### `--fix` boundary

Current `doctor --deep --fix` only repairs drift that is deterministic enough to write back safely:

- `storage_used`
- `file_blobs.ref_count`

It does not auto-repair:

- broken folder-tree structure
- extra or missing objects in storage
- cross-scope problems that need human judgment

That boundary is intentionally conservative. Automatic repair only happens when the action is deterministic enough not to damage data that still needs human inspection.

### Why object scanning uses the storage-driver abstraction

`audit_storage_objects()` does not assume the backend is local filesystem or a particular S3 SDK. It reuses the traversal capability exposed by `StorageDriver`.

That means:

- the audit logic does not need driver-specific details
- new drivers can reuse the same audit logic as long as they expose the same traversal capability

So object audit depends on “the driver can enumerate objects,” not on one concrete backend implementation.

### Current trade-off

`doctor` is not trying to be a real-time self-healing system.

It is more like:

- a structured operator-triggered inspection entry point
- an operational backend that combines environment checks and drift audit
- a limited auto-repair tool for deterministic problems only

That makes it useful for:

- first post-deploy inspection
- storage / migration / quota debugging
- factual validation before cleanup or manual repair

It is not meant to replace monitoring, logs, or alerts.

## 7. Cross-database migration CLI

Main code paths:

- `src/cli/database_migration/mod.rs`
- `src/cli/database_migration/apply/mod.rs`
- `src/cli/database_migration/apply/copy.rs`
- `src/cli/database_migration/apply/convert.rs`
- `src/cli/database_migration/checkpoint.rs`
- `src/cli/database_migration/schema.rs`
- `src/cli/database_migration/verify.rs`

### Design goal

`database-migrate` moves an already-running AsterDrive instance from one database backend to another, such as SQLite to PostgreSQL or MySQL to PostgreSQL.

It is not an online business request and it is not a replacement for SeaORM migrations. It:

- connects to source and target databases
- validates backend and migration state on both sides
- prepares the target schema
- copies business data in a fixed table order
- maintains resumable checkpoints
- verifies row counts, unique constraints, and foreign-key constraints after copying

### Why this is a CLI

Cross-database migration needs long-lived external connections, progress reporting, interruption recovery, and a real maintenance window. Putting that into HTTP admin APIs would cause avoidable problems:

- HTTP timeouts and reverse-proxy limits would interfere with long jobs
- recovery would need a separate remote control plane
- ordinary traffic should not keep writing during a migration window

So the repository implements it as an offline CLI: the command handles user interaction and reporting, while the migration logic handles deterministic copying and validation.

### Table copy order

Migration is not alphabetical dump order. `COPY_TABLE_ORDER` fixes the sequence so foundational tables are copied before the tables that depend on them, for example:

- `managed_followers`
- `storage_policies`
- `storage_connector_application_configs`
- `storage_policy_credentials`
- `storage_policy_groups`
- `storage_policy_group_items`
- `follower_enrollment_sessions`
- `users`
- `storage_policy_authorization_flows`
- `user_profiles`
- `user_invitations`
- `auth_sessions`
- `passkeys`
- `mfa_factors`
- `mfa_recovery_codes`
- `mfa_login_flows`
- `mfa_email_codes`
- `mfa_totp_setup_flows`
- `teams`
- `team_members`
- `tags`
- `folders`
- `webdav_accounts`
- `file_blobs`
- `blob_media_metadata`
- `files`
- `file_versions`
- `shares`
- `upload_sessions`
- `upload_session_parts`
- `contact_verification_tokens`
- `external_auth_providers`
- `external_auth_identities`
- `external_auth_login_flows`
- `external_auth_email_verification_flows`
- `master_bindings`
- `remote_storage_targets`
- `system_config`
- `audit_logs`
- `mail_outbox`
- `background_tasks`
- `runtime_leases`
- `scheduled_tasks`
- `storage_migration_checkpoints`
- `entity_properties`
- `resource_locks`
- `wopi_sessions`

This order must stay aligned with foreign-key dependencies. When adding a new table, evaluate whether it should be added to `COPY_TABLE_ORDER` and where it belongs.

### Resumable model

Migration checkpoints are stored in the target database's `aster_cli_database_migrations` table.

That table is for CLI state, not business data. It records:

- current migration key
- current phase
- current table being copied
- cursor / batch progress
- overall execution status

That lets the next run continue from the checkpoint instead of starting over.

### Mode selection

Current run modes:

- default `apply`: prepare target schema, copy data, and run validation
- `--dry-run`: only plan and preflight, no business writes
- `--verify-only`: only verify the target database, no copying

`ASTER_CLI_PROGRESS` controls progress output, `ASTER_CLI_COPY_BATCH_SIZE` adjusts batch size, and `ASTER_CLI_FAIL_AFTER_BATCHES` is kept for test-time interruption simulation.

### Validation boundary

Post-copy validation checks:

- source and target row counts
- target unique-constraint conflicts
- target foreign-key violations
- whether auto-increment sequences need reset

It does not decide whether specific historical records should be migrated. The goal is to faithfully copy the current database state, not to clean or redesign it.

## 8. Frontend download coordination and transfer activity area

Main code paths:

- `frontend-panel/src/services/downloadCoordinator.ts`
- `frontend-panel/src/components/files/DownloadCenter.tsx`
- `frontend-panel/src/components/files/UploadAreaHost.tsx`
- `frontend-panel/src/components/layout/BottomRightActivityShell.tsx`
- `frontend-panel/src/stores/frontendConfigStore.ts`

### Download ownership boundaries

Pages only collect the selection in the current workspace and pass file / folder IDs plus display metadata such as names and sizes to the download layer. Local page state may lag behind a selection after pagination, search refresh, or item expiry. If the number of locally resolved items differs from the number of requested IDs, the page must send the original IDs to the backend ZIP endpoint for authoritative resolution instead of silently dropping missing items.

`downloadCoordinator` owns browser-side download protocol details and lifecycle management, including:

- building authenticated download paths through the workspace-aware file service
- proxy streaming, progress, cancellation, failed-item retry, and memory fallback
- directory-write capability detection and recursive folder traversal
- file and folder pagination cursor advancement

Pagination loops must check both `next_*_cursor` and whether the current page returned any items. An empty page with a stale cursor terminates the loop immediately to avoid repeated requests or infinite pagination.

`DownloadCenter` only presents download choices and task state. It does not build file paths or decide authentication policy. Single-file “browser default” downloads use the coordinator's authenticated file entry point; multi-item “browser ZIP” downloads use the batch service's backend archive stream. Proxy file, proxy ZIP, and local-directory downloads create observable browser download tasks through the coordinator.

### Backend capability remains authoritative

The anonymous bootstrap endpoint `GET /api/v1/public/frontend-config` exposes signed-in and public-share ZIP download switches under `downloads`. The frontend config store caches and consumes them; when a switch is disabled or bootstrap config is unavailable, ZIP choices are hidden conservatively. Backend endpoints still enforce runtime configuration, so frontend capability checks are presentation logic rather than authorization.

### Unified activity area and layout reservation

The upload panel and download center both portal into one bottom-right shell through `BottomRightActivityPortal`. The shell owns width, borders, stacking, and section layout; `transferActivityStore` owns whether upload or download is expanded. The upload manager must not keep a duplicate expanded-state pass-through.

The shell is visually fixed at the bottom right, but it must not cover the last rows of file content. `BottomRightActivityShell` measures its real height with `ResizeObserver` and writes `--bottom-right-activity-shell-height`. File browsing, search, category, share, and trash surfaces reserve that height through the shared padding constant. Any new page that displays transfer activity must use the same reservation instead of introducing another independent overlay.

## When to extend this document

If a module satisfies both of these conditions, it is worth adding here:

- it is already part of the main runtime path or the main operational path
- its design constraints are hard to understand from signatures alone

Likely next candidates:

- WebDAV protocol layer and the database lock system
- WOPI session and target resolution
- team-space model and member-permission boundaries
- runtime config definitions, snapshots, and hot-update flow
