# Backend Service Ownership Boundaries

This guide records the backend ownership boundaries AsterDrive should use while continuing the 0.3 service-layer cleanup.

It describes the current repository shape, not a generic architecture theory. When deciding where a change belongs, start from AsterDrive product semantics: files, workspaces, uploads, storage policies, remote nodes, remote storage targets, WebDAV, and WOPI.

## Quick Rules

The primary backend flow is still:

```text
src/api/routes/*
  -> src/services/*
  -> src/services/<domain>/*
  -> src/db/repository/* / src/storage/* / src/webdav/*
```

If one function parses protocol details, validates business rules, writes the database, builds a storage driver, calls the remote protocol, shapes a UI descriptor, and reloads audit or registries, the boundary is already mixed. Split out the specific responsibility before adding more logic.

## Layer Ownership

### Route Layer: Protocol Adapters

Directories:

- `src/api/routes/*`
- `src/api/primary.rs`
- `src/api/follower.rs`

Route code owns the entry shape:

- HTTP path, query, body, and header extraction
- JWT, admin, team, internal storage, and WOPI token guards
- Lightweight DTO to service input conversion
- Calling services
- REST envelope, file stream, SSE, Prometheus, WebDAV, and WOPI response mapping

Route code must not own:

- Storage policy selection rules
- Upload finalization, quota, version, and blob reference-count consistency
- Storage driver capability decisions
- Remote node or remote storage target selection
- UI form field matrices
- Database transaction orchestration

Protocol compatibility can stay visible at the route boundary. Examples are WOPI header mapping, internal storage HTTP status behavior, and file download Range / conditional request headers. Those protocol details must not decide product behavior.

### Service Layer: Use-Case Orchestration

Directory:

- `src/services/*`

Public service entries should orchestrate one complete product use case, such as:

- Initialize, chunk, complete, cancel, or inspect an upload
- Create, move, download, or prepare preview access for a file
- Create a storage policy, test a connection, or execute a policy action
- Create a remote node, test remote node health, or synchronize a binding
- Create a WOPI launch session or handle WOPI file writeback

Service code may:

- Load the context required by the use case
- Call domain helpers for normalization, validation, and resolution
- Call repositories for reads and writes
- Call storage drivers or remote protocol clients for required side effects
- Trigger audit, storage change, cache invalidation, policy snapshot reload, driver registry reload, and task creation at explicit points
- Present domain results through stable route-facing response types

Service code should not become:

- A replacement driver registry
- A remote protocol wire parser
- A place to pile up repository SQL
- A hidden source of frontend descriptor rules
- A cross-domain bag of internal helpers

### Domain Helpers: Testable Business Rules

Typical shapes:

- `src/services/<domain>/normalization.rs`
- `src/services/<domain>/driver.rs`
- `src/services/<domain>/paths.rs`
- `src/services/<domain>/complete/plan.rs`
- `src/services/<domain>/scope.rs`
- `src/services/<domain>/targets.rs`

Domain helpers own reusable, testable rules with as little `AppState` dependency as possible:

- Input normalization
- Path parsing and traversal prevention
- Upload completion planning
- Capability resolution
- Descriptor building
- Target selection
- Finalization contracts
- Protocol-independent conflict, lock, and rename rules

If a rule can be a pure function, keep it pure. If it must read the database, make that boundary explicit by passing repository-loaded models or an explicit `ConnectionTrait`.

### Repository Layer: Data Access And Atomic SQL

Directory:

- `src/db/repository/*`

Repositories own database facts:

- Query by id, scope, token, binding, or key
- Pagination, sorting, and aggregate counts
- Transactional create, update, and delete operations
- Cross-database-compatible SQL
- Atomic counters, row locking, and unique-constraint conflict mapping

Repositories should not know:

- HTTP, WebDAV, WOPI, or internal storage protocols
- How storage drivers are built
- Which remote storage target fields should be displayed
- How upload modes are negotiated
- How diagnostics should be shown in the UI
- Which page creates a policy target

If a repository function needs `DriverType::S3` to decide a workflow, stop. That decision usually belongs in a storage connector, policy service, upload service, or domain helper.

### Storage Connectors And Drivers: Object Content Capabilities

Directories:

- `src/storage/connectors/*`
- `src/storage/drivers/*`
- `src/storage/traits/*`

The storage layer owns:

- Driver and connector descriptors
- Connection test actions
- Credential and application config handling
- Upload transport capabilities
- Presigned, multipart, streaming, range read, delete, compose, metadata, and capacity capabilities

The storage layer does not own:

- Users, teams, folder permissions, or shares
- Workspace quota ownership
- Storage policy group priority
- Remote node page or policy page organization
- Audit wording
- REST envelopes

### Remote Protocol Layer: Wire Contract

Directories:

- `src/storage/remote_protocol/*`
- `src/storage/remote_protocol/tunnel/*`

Remote protocol code only owns the primary-to-follower wire contract:

- Internal auth signatures and presigned query/header constants
- HTTP and reverse tunnel transport
- Remote storage request and response models
- Capability wire models
- Path encoding
- Response parsing
- Protocol version fallback

Remote protocol code must not decide:

- Whether a remote node should be the default storage target
- Which follower-side target a policy should select
- How remote storage target descriptors are presented in the UI
- Whether a product-level error should block a policy change

Those decisions belong to `managed_follower_service`, `remote_storage_target_service`, `policy_service`, or a capability / target resolver.

### WebDAV And WOPI Protocol Entry Points

Directories:

- `src/webdav/*`
- `src/api/routes/wopi.rs`
- `src/services/webdav_service.rs`
- `src/services/wopi_service/*`

WebDAV and WOPI are protocol entry points, not ordinary variants of the REST file API.

They should:

- Preserve protocol-required status codes, headers, locks, ETags, Range behavior, `PUT_RELATIVE`, rename behavior, proof validation, and token behavior
- Reuse AsterDrive file, folder, workspace scope, storage, quota, audit, and storage change semantics
- Keep protocol-specific compatibility mapping at the protocol boundary, without polluting the global REST envelope

They should not:

- Bypass `workspace_storage_service` / `workspace_storage_core` with a separate file finalization path
- Bypass `file_service` / `folder_service` for blob references, versions, and trash semantics
- Push WOPI/WebDAV-specific error formats into the generic service error model

## When To Split A Module

Split a domain helper or submodule before adding more code when:

- One service function mentions `AppState`, repository writes, remote protocol clients, driver construction, descriptor shaping, audit, and registry reloads together
- `match driver_type` repeatedly appears in business services instead of connector, driver registry, capability resolver, or remote-storage-target registration code
- One function normalizes input, interprets remote capabilities, and performs database side effects
- A use-case function builds a frontend form field matrix
- Upload completion behavior is spread across several entries so quota, blob, file version, cleanup, and audit cannot be reviewed in one place
- A function is about 80-120 lines long without a clear load-context -> validate -> write -> side-effects -> present structure

Preferred shape:

```rust
pub async fn create_xxx(state, input) -> Result<Output> {
    let input = normalize(input)?;
    let context = load_context(state, &input).await?;
    validate(&context, &input)?;
    let result = repo::create(...).await?;
    run_required_side_effects(state, &result).await?;
    Ok(present(result))
}
```

## Responsibility Inventory

Use this inventory during review to decide whether logic belongs in the current service. It is not a request to rewrite everything at once.

### `upload_service`

Current responsibilities:

- Upload facade for direct multipart body upload, init, chunk, complete, cancel, progress, recoverable sessions, and presigned parts
- Maps personal and team requests into `WorkspaceStorageScope`
- Negotiates upload modes from storage policy and driver capabilities: direct, chunked, presigned single, presigned multipart, relay multipart, and remote presigned
- Chooses a completion plan under `complete/*` and turns temporary upload state into a final file
- Records upload metrics and route-level audit wrappers

Should stay:

- Upload lifecycle orchestration
- Upload session state transitions
- Completion plan selection
- Public upload-mode response models
- Unified cancel, cleanup, and recoverable session entry points

Should move down or continue to converge:

- The finalization contract for trusted size, actual size, hash, blob, file version, quota charge, and cleanup must stay reviewable in one shape
- Different completion paths should converge toward the same `workspace_storage_service` finalization shape
- Remote and object multipart details should remain in focused init/complete submodules, not flow back into the facade

Side effects that must be explicit:

- Upload session state updates
- Temporary object, multipart upload, and chunk cleanup
- Quota accounting
- Metrics
- Audit
- Storage change and cache invalidation when the path creates or updates a file

### `workspace_storage_service`

Current responsibilities:

- Unified workspace file facade
- Re-exports scope helpers, storage core, multipart/store/blob upload capabilities
- Handles REST direct upload, WebDAV flush, preuploaded blob, multipart/staged, and streaming direct entries
- Provides stable entries for file creation, content writes, and temporary file persistence

Should stay:

- `WorkspaceStorageScope` entry point and scope-aware file writes
- Orchestration from multiple upload entries into the storage core
- Storage operation cancellation and cleanup boundaries
- Stable facade exposed to routes, WebDAV, and file services

Should move down or continue to converge:

- `mod.rs` has a wide re-export surface; narrow it only after auditing callers
- Connector upload transport decisions should come from `src/storage/connectors/*`; the service should consume the result
- WebDAV-specific write paths should pass parsed write intent into this service instead of adding protocol decisions here

Side effects that must be explicit:

- Object writes and failure cleanup
- Compensation for transaction-external side effects
- Storage change emission
- Actor attribution supplied by audit callers

### `workspace_storage_core`

Current responsibilities:

- Stable core actions for workspace file writes
- Policy resolution, parent path creation, blob/file record creation, upload session finalization, and quota reads/writes
- Upload-method-independent and HTTP-independent file consistency foundation

Should stay:

- `resolve_policy_for_size*`
- `check_quota` / `update_storage_used*`
- `create_*_file_from_blob*`
- `finalize_upload_session_*`
- `ensure_upload_parent_path`

Should not own:

- HTTP multipart parsing
- WOPI/WebDAV protocol status codes
- Remote protocol transport
- Frontend descriptors
- Route-level audit

Side effects that must be explicit:

- File, blob, version, and quota consistency inside database transactions
- Binding between completed upload sessions and final files

### `policy_service`

Current responsibilities:

- Storage policy and policy group management use cases
- Policy connection normalization, preparation, and validation
- Policy group defaults and assignment migration
- Capacity info, connection tests, draft/saved actions, and S3-compatible driver promotion
- Policy snapshot reload and public thumbnail/media capability cache invalidation after policy changes
- Admin audit wrappers

Should stay:

- Storage policy and policy group product semantics
- Default policy and default group consistency
- Validation that a policy can be deleted, including blob/group/upload-session references
- Orchestration of storage connector descriptors and actions

Should move down or continue to converge:

- Connector field normalization and application config persistence belong in `src/storage/connectors/*`
- Remote target product selection should be consumed from a capability / target resolver instead of being owned by remote node service
- Policy action diagnostics may be service models, but driver-specific detail must come from connector/action code

Side effects that must be explicit:

- Policy snapshot reload
- Driver registry or public capability cache invalidation
- Upload session cleanup task creation
- Audit

### `managed_follower_service`

Current responsibilities:

- Primary-side remote node CRUD, pagination, and enrollment status presentation
- Base URL and transport mode normalization
- Connection tests, health tests, and capability probes
- Remote binding sync
- Remote protocol client access
- Driver registry / policy snapshot reload or invalidation after remote node changes

Should stay:

- Remote node connection-object lifecycle
- Enrollment, health, transport, and capability probing
- Direct / reverse tunnel entry selection
- Detection of completed enrollment

Should not own:

- Follower-side storage target product ownership
- How the storage policy UI creates targets
- Final remote storage target driver descriptor field rules
- Remote object path and blob/file finalization behavior

Should move down or continue to converge:

- Capability parsing may stay here, but capability-to-product interpretation should be in a resolver shared by policy, remote storage target, and UI descriptor flows
- Remote node presentation should not grow into policy target presentation

Side effects that must be explicit:

- Warn logging for remote binding sync failures
- Registry reload / invalidation
- Policy snapshot reload
- Health test writes for last capabilities and last error

### `remote_storage_target_service`

This is the current implementation home for what older issue text called "managed ingress profile service". Legacy wire fields and error codes may still use `managed_ingress.*` for compatibility, but new product ownership should use remote storage target terminology.

Current responsibilities:

- Follower-side remote storage target CRUD
- Primary-side forwarding for remote target CRUD
- Remote storage target driver descriptors and field descriptors
- Local/S3 field normalization, path normalization, driver build, and validation
- Default target selection, revision apply status, and effective target resolution

Should stay:

- Remote storage target lifecycle as follower-side ingress targets
- Remote-storage-target-specific driver registration
- Target normalization, driver validation, and effective target resolution
- Primary-to-follower remote forwarding facade

Should not own:

- The complete primary storage policy workflow
- Remote node connection lifecycle
- A replacement for the general storage connector registry
- UI-inferred driver capability tables

Should move down or continue to converge:

- `driver.rs` should remain remote-storage-target-specific registration, not be blindly merged with the general connector registry
- `remote.rs` currently combines capability filtering and remote forwarding; after the resolver matures it should consume resolver results
- Naming migration should continue toward remote storage target, keeping old route, wire field, and config aliases only as compatibility layers

Side effects that must be explicit:

- Desired/applied revision updates
- Default target replacement constraints
- Driver registry reload / target validation
- Protocol error mapping for remote forwarding failures

### `master_binding_service`

Current responsibilities:

- Follower-side primary binding upsert and sync
- Internal storage request authorization using header signature, nonce, timestamp, and content length
- Presigned PUT/GET query authorization
- Parsing master binding authorization into an ingress target result
- Provider storage namespace and object key prefix
- Follower readiness checks

Should stay:

- Follower trust relationship with the primary
- Product-level internal storage and presigned authorization
- Path isolation through binding storage namespaces
- Calling `remote_storage_target_service::resolve_effective_target` for the authorized ingress driver

Should not own:

- Concrete HTTP responses for object PUT/GET/compose/list
- Remote storage target CRUD
- Storage policy selection
- Remote node management page semantics

Should move down or continue to converge:

- Wire-level signature constants and algorithms should stay exposed by `remote_protocol`; the service should own only the authorization use case
- Provider path rules must continue to reuse object key normalization instead of route-level string concatenation

Side effects that must be explicit:

- Nonce cache writes
- Driver registry reload
- Precondition errors for disabled bindings and missing ingress targets

### `file_service`

Current responsibilities:

- Facade for file metadata, content updates, delete/permanent delete, download, thumbnails, resource handles, locks, and copy
- Personal and team file-level use cases
- Audit wrappers
- Download outcome construction: stream, range, conditional request, presigned redirect, and inline sandboxing
- Shared file writes and scope validation through `workspace_storage_service`

Should stay:

- Product semantics for file resources: read, write, move, lock, delete, copy, download, and preview
- Stable bridge between `DownloadOutcome` / file access outcomes and route responses
- File-level audit details
- File access rules for Range, ETag, sandbox, and disposition

Should not own:

- Upload session lifecycle
- Storage policy management
- WebDAV/WOPI protocol state machines
- Remote internal storage wire protocol

Should move down or continue to converge:

- The download submodules should keep streaming, range, and response behavior separated; the main path must not regress to whole-file buffering
- Resource handles should resolve accessible file-resource shapes, not frontend page flows
- Audit wrappers may stay in the facade while core file mutations stay reusable by WebDAV/WOPI

Side effects that must be explicit:

- Blob reference counts and cleanup
- Storage change emission
- Share/cache invalidation
- Audit
- Download metrics and share download result counters triggered by the calling path

### WebDAV Integration

Current responsibilities:

- `src/webdav/*` handles WebDAV / DeltaV protocol behavior, Basic Auth, path resolution, locks, properties, and transfer
- `src/services/webdav_service.rs` exposes product actions needed by protocol code, such as folder tree soft delete, purge, and copy
- WebDAV file writes use the unified `workspace_storage_service` path

Should stay:

- WebDAV-specific auth, path resolution, locks, properties, Depth, Range, and DeltaV behavior
- Adaptation from protocol semantics to AsterDrive workspace/file/folder semantics
- Reuse of `file_service`, `folder_service`, and `workspace_storage_service`

Should not own:

- A separate file model
- Separate upload finalization, quota, or blob cleanup rules
- REST envelopes or ordinary API DTOs

Side effects that must be explicit:

- WebDAV mutations triggering storage change, audit, and share/cache invalidation
- Folder tree purge cleanup for properties, shares, and folder path cache

### WOPI Integration

Current responsibilities:

- `src/api/routes/wopi.rs` handles WOPI HTTP method/header/status compatibility
- `src/services/wopi_service/*` handles discovery, sessions, proof validation, locks, targets, and operations
- WOPI file reads and writes reuse `file_service` / `workspace_storage_service`

Should stay:

- WOPI token/session, proof validation, and discovery cache behavior
- Protocol semantics for CheckFileInfo, GetFile, PutFile, PutRelative, Rename, Lock, Unlock, and RefreshLock
- WOPI conflict and invalid-name result models

Should not own:

- A replacement for the common AsterDrive file permission model
- Direct storage-driver branching
- REST file API response formats

Side effects that must be explicit:

- File version, quota, storage change, and audit after WOPI writeback
- Consistency between WOPI locks and file lock state
- Discovery cache refresh and expired session cleanup

## Review Checklist

When reviewing backend service changes, ask:

- Does the route only adapt protocol details and map responses?
- Is the service orchestrating a use case, or did it absorb driver/protocol/descriptor/repository details?
- Are testable rules in a domain helper?
- Does the repository only access data, without product workflow decisions?
- Do storage connector / driver capabilities come from the storage layer instead of a business-layer hard-coded matrix?
- Does remote protocol code only handle the wire contract?
- Do upload, file write, WebDAV writeback, and WOPI writeback reuse the same quota/blob/version/finalization semantics?
- Are policy, remote node, and remote storage target ownership boundaries clear: remote node manages connection, policy manages storage policy, remote storage target manages follower-side targets?
- Are all transaction-external side effects explicit in function names, parameters, or result types?

## Non-Goals

This document does not require a one-shot service rewrite. Follow-up PRs should:

- Move one coherent responsibility at a time
- Preserve public APIs and database schema unless the issue explicitly asks otherwise
- Add focused tests around moved rules
- Avoid introducing a broad framework-style architecture layer for purity
- Avoid renaming clear AsterDrive domain names into vague manager/helper/object names
