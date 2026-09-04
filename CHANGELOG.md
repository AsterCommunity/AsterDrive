# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Storage placement policy engine** — Policy groups upgraded to placement profiles with versioned upload admission, ordered matchers, `first_available` / `weighted_random` target selection, draining/unavailable fallback, and upload execution preferences; the legacy item migration preserves original size routing, and user/team assignment, folder override, existing blob policy, and upload session decision boundaries remain unchanged. New blob ingress uniformly reuses the immutable `PolicySnapshot` resolver; the admin side supports rule/target editing and backend dry-run simulation, showing normalized categories, admission results, rule traces, excluded targets, and stable reason codes.

- **WebP EXIF metadata** — The built-in image metadata handler supports extracting camera, capture parameters, time, orientation, and GPS information from the WebP `EXIF` chunk; WebP without EXIF can still return image dimensions and format.

### Changed

- **Unified storage connection and remote target RPC** — Regular storage policies and follower remote storage targets share the same `StorageConnectionInput`, connector config/credential normalization, descriptor, credential retention semantics, and direct driver factory; the remote target is only additionally responsible for binding, target key, default, revision, reconciliation, and signed RPC, no longer maintaining a driver enum, flat provider fields, legacy request adapters, or driver construction paths that fabricate policies. Local, S3, SFTP, Tencent Cloud COS, Alibaba Cloud OSS, Qiniu Kodo, Azure Blob, and Huawei OBS are all wired in through a single runtime connector registry, and the admin side reuses the ordinary storage descriptor-driven field forms. Internal protocol V6 negotiates capabilities solely via `remote_storage_target.connector_ids`; a one-time pre-startup 0.5.0 conversion writes old Local/S3 flat configs into the connector envelope and encrypts old S3 credentials, with the old physical columns and conversion code kept behind a `TODO(remote-storage-target-0.7.0)` cleanup marker.

- **Unified upload session protocol** — Non-empty files uniformly init first: init pins filename, MIME, size, storage policy, placement profile/rule/revision, and the `stream` / `chunked` / `presigned` / `presigned_multipart` / `provider_resumable` transport. Single-request stream now uses the raw `application/octet-stream` body of `PUT /files/upload/{upload_id}/body`, creating the file/blob, updating quota, and completing the session within the same database transaction; other transports continue to be published idempotently by `/complete`. Duplicate or expired stream bodies are rejected via an atomic session claim before writing. When the target storage can report capacity, init performs an advisory capacity pre-check; actual writes are still governed by driver results, with no concurrent capacity reservation. Zero-byte files uniformly use `/files/new`.
- **Upload session schema and ID** — A new migration adds the `mime_type` resolved and pinned at init to `upload_sessions`; continuation, resume, and completion no longer re-infer the upload plan from subsequent bodies or default filenames. Newly created upload sessions use standard UUIDv7 with an embedded millisecond timestamp, remaining compatible with existing 36-character UUIDs, URLs, the database, and legacy UUIDv4 sessions.

### Removed

- **Single multipart upload entry point** — Removed the personal and team `POST /files/upload` compatibility entry points and the server-side staged multipart fallback; clients must first initialize an upload session, then submit content via the negotiated data plane.

## [v0.5.1] - 2026-08-24

### Added

- **Huawei Cloud OBS connector** — Added `asterdrive.storage.huawei_obs`, reusing the AWS S3 SDK's object, streaming, and multipart serialization while integrating Huawei's native OBS signing; supports regional virtual-hosted endpoints, custom domains, Range, marker-based ListObjects, presigned single/multipart requests, and descriptor-driven admin configuration.
- **Team and system audit CSV export** — Added three server-side streaming export endpoints for user teams, admin teams, and admin system; exports reuse server-side filter conditions, read in batches with keyset cursor pagination, use a fixed 16-column UTF-8 / RFC 4180 CSV contract — system audit preserves sorting parameters, team audit is fixed to `created_at DESC, id DESC` output — with a 100000-row per-export limit.
- **Built-in login method control** — Added a hot-reloadable password login toggle that still combines independently with the Passkey toggle; disabling password login also disables public registration, activation resend, password invitation acceptance, password reset, and external identity password binding; unfinished password-first-factor MFA flows re-check policy at completion, and external authentication and Passkey logins are no longer blocked by leftover forced-password-change flags. The backend only allows disabling both password and Passkey when an enabled external authentication provider exists, and prevents disabling or deleting the last external provider, avoiding losing all login entry points after saving.
- **Remote node connection lifecycle audit** — Reverse tunnel connections, normal shutdowns, abnormal disconnects, and heartbeat timeouts now write system audits aggregated by remote node / binding; simultaneous changes across the four streaming lanes produce only one node-level state transition, recording connection count, interruption count, lane count, transport, and a stable reason code, without access keys, secrets, signatures, URL credentials, or tokens.

### Changed

- **Streaming write attempt lifecycle** — Reverse follower and streaming drivers uniformly use `StreamUploadAttempt` with a unique owner, declared size validation, `stage_attempt` / `commit_attempt`, and structured abort outcomes; failed cleanup no longer infers ownership from target-object `exists` snapshots; S3-compatible, Azure, and Remote write directly and atomically to a pre-allocated opaque final key without adding provider-side full-object copy/compose; Local/SFTP use a separate staging area followed by atomic rename; OneDrive hands streams larger than 1 MiB to a provider upload session; plus concurrency, cancellation, retry, 15-minute stage timeout, Deferred cleanup metrics, and resource budget contracts.
- **SFTP atomic overwrite capability note** — Overwriting an existing object requires the server to declare `posix-rename@openssh.com`; when the extension is missing, the old object is preserved and an explicit capability error is returned, without a plain double-rename fallback that leaves a brief missing window.

- **Cross-layer authentication flow state machine** — Login, MFA, Passkey, OIDC/OAuth, external authentication email recovery, registration activation, password reset, email change, invitation acceptance, and session refresh uniformly use typed flow lifecycle, transition guards, expiry, cancellation, single consumption, attempt budget, replay, and revision conflict contracts; existing strongly typed authentication entities, public API envelopes, cookies, redirects, and session behavior are preserved, with no catch-all auth JSON table introduced.
- **Login page state and policy coordination** — The React login page converges into a single top-level `AuthUiFlow` state, with a command coordinator uniformly handling flow events, request cancellation, and stale responses; auth check, frontend config, and external provider lists are merged via a policy coordinator with generations, and the URL only restores a typed flow reference with a TTL.
- **Authentication side-effect boundaries** — Primary login, MFA, external callback, recovery, invitation, and refresh rotation execute cookie, redirect, mail, audit, and cache side effects only after the writer transaction / conditional update completes; runtime authentication policy is re-read on the next advance of an unfinished flow.

- **Site title home navigation** — The site title in the file browser and share page top bars returns to `/` via client-side routing, the admin console top bar returns to `/admin/overview`, preserving existing brand assets, theme, responsive display, and accessibility semantics.
- **Storage policy and policy group lifecycle** — The storage policy and default policy group created during first-time setup are no longer permanent system objects pinned to fixed IDs; after references such as blobs, upload sessions, policy group items, and user/team bindings are cleared, the first or last default policy can be deleted; deleting the last default policy group returns the system to `needs_storage`, and it recovers to `ready` after reconfiguring the default storage topology, without silently clearing business bindings. Default switching, deletion, and re-setup coordinate multiple Primaries using stable database locks; existing data protection remains unchanged.
- **Storage policy credential compatibility layer fully closed out** — Removed the 0.5.x startup-phase legacy credential importer, connector legacy import hooks, the old OneDrive OAuth conversion, deprecated credential entities / repositories, and the old credential copy and import paths of `database-migrate`. The current runtime only consumes `connector_id`, typed `storage_config`, and `storage_policy_connector_credentials`.
- **Storage policy final schema migration** — Added `m20260820_000001_remove_storage_policy_legacy`. The migration checks the old credential tables and old static credential columns before any DDL; if an unfinished 0.5.x conversion is found, it hard-fails and preserves the original schema / data; once the check passes, it drops the two old credential tables, old `storage_policies` columns, indexes, and the remote node foreign key.
- **Cross-database migration boundaries** — `database-migrate` only copies the current policy envelope and connector credentials; a source database with unmigrated legacy credentials is rejected before copying, and empty historical legacy stores no longer enter the target database.

### Fixed

- **Directory upload concurrent initialization transaction** — When concurrently initializing multiple files under the same new directory, parent directory creation now uses the database's native conflict-ignore with winner read-back, avoiding reuse of an aborted transaction after a PostgreSQL unique-key race that caused the second file to return 500; directory creation for unrelated workspaces is not serialized, and concurrent initialization plus multi-level path regression coverage is added.
- **Authentication flow expiry and replay boundaries** — Contact verification token consumption now requires `consumed_at IS NULL AND expires_at > now`, avoiding consuming an already-expired token in a concurrency window; external callback validates an active flow before atomic consumption, Passkey challenges use a single-consumption envelope with identity, revision, and expiry, session refresh rejects expired/revoked sessions, and illegal transitions no longer produce partial authentication side effects.
- **Azure Blob loopback copy fallback latency** — After a server-side URL copy fails on loopback endpoints such as Azurite, the local streaming copy fallback kicks in immediately instead of first exhausting the Azure SDK's full retry window; non-loopback Azure endpoints keep the default retry policy.

- **Docker edge release channel follows stable** — On official releases, GHCR and Docker Hub `edge`, `edge-metrics`, `edge-slim`, and `edge-metrics-slim` sync with the corresponding `stable` manifests; alpha, beta, and rc releases only move edge, while `latest` / `stable` keep the previous official version, preventing edge from lingering on an old candidate.
- **PDF preview rotation position retention** — Multi-page PDFs retain the current virtual page and in-page scroll offset when rotating on a middle or later page, no longer jumping to the bottom of the document due to page-size remeasurement.
- **Migration idempotency and rollback boundaries** — Covers old column / index / foreign-key cleanup paths across SQLite, PostgreSQL, and MySQL, preserves SQLite foreign-key state, and verifies that existing data referencing `storage_policies` is not lost.
- **Schema drift and historical test boundaries** — Distinguishes historical migrations, 0.5.x compatibility schema, and the final schema; adds hard-failure-on-unmigrated-credentials, empty-old-table cleanup, final column set, and repeated-execution tests.
- **Slim image media processing capability and derived cache** — Switching between full and slim images preserves existing media processing configuration, with the admin side separately showing configured, runtime-available, and effectively-enabled states; the public thumbnail capability only declares formats that can currently be generated, independently of media metadata capability. Existing thumbnails and image preview caches remain readable; missing `vips`, `ffmpeg`, or `ffprobe` only blocks new related derivations and returns a structured processor-unavailable error; the Docker release flow also guarantees all slim variants are pushed before full variants.

### Security

- **Audit export sensitive data and spreadsheet injection protection** — Audit lists and CSV exports uniformly remove password, token, secret, credential, session, MFA, external authentication, WOPI, and storage credential fields recursively and never output share tokens; user-controllable CSV text fields neutralize formula prefixes to avoid being interpreted as formulas in desktop spreadsheet software.

### Statistics

- 334 files changed, 18,044 insertions(+), 5,023 deletions(-)
- 18 commits
- 2 database migrations
- Rust Edition 2024, MSRV 1.95.0

## [v0.5.0] - 2026-08-20

### Changed

- **Documentation site migration** — User and developer documentation uniformly migrated to `https://drive.docs.astercosm.com/`, with in-repo site links, the sitemap, `security.txt`, the documentation home entry, and docs build configuration updated in sync.
- **D9 frameless frontend visual system** — The file browser, sidebar, sharing, tasks, WebDAV, trash, settings, and team management pages uniformly adopt a frameless design partitioned by color scales and spacing; the dark theme moves to warm charcoal layers, folders use filled glyphs, file-type fallbacks use semantic colors, selection adds sidebar/directory-tree indicator bars, and the file area supports entrance animations and grid column-split transitions that respect `prefers-reduced-motion`. Input fields, table hairlines, outline buttons, and overlays retain necessary boundaries.
- **File browser interaction and layout** — Grid column count is now computed from container width and maximum card width, preventing long filenames from breaking the track; file/folder grid and list share selection, drag-and-drop, and action resolution; added Cmd/Ctrl multi-select, Shift range selection, arrow-key navigation, Shift+arrow selection extension, and Enter to open single selections, plus keyboard boundaries for input fields, menus, dialogs, and IME scenarios.
- **Unified trash browser** — Trash grid and table reuse the common `FileGrid` / `FileTable`, showing original location and expiry time, and provide restore and permanent deletion through a unified action registry; trash selection state is incorporated into the shared file store, and normal file pages and public share pages are unaware of trash-specific actions.
- **Team management page routing** — Team management converged from in-dialog views into a `/settings/teams/:id` page, retaining member, overview, WebDAV, audit, and dangerous-operation capabilities, and unifying tabs, scroll restoration, data loading, and route-back behavior; dialogs remain only for scenarios that truly need overlay boundaries.

### Fixed

- **Reverse tunnel idle connection keep-alive and close handshake** — Single and cluster Primaries uniformly use WebSocket Ping/Pong to determine streaming lane liveness, no longer misjudging 60 seconds without business frames as a disconnection and triggering four-lane reconnection storms; Primary shutdown stops accepting new requests, drains in-flight requests on the lanes within a bounded time, then sends Close; a normal Close is logged as info without alerting, while abnormal drops such as EOF/ConnectionReset keep WARN; heartbeat loss still logs one actionable alert, and lane / poll retries while the Primary is offline only alert at fault onset and log on recovery; runtime logs use the binding ID, avoiding printing full access keys.

## [v0.5.0-rc.1] - 2026-08-18

### Release Highlights

Since `v0.4.0`, the AsterDrive mainline has completed a major round of evolution targeting production multi-Primary deployments, WebDAV protocol boundaries, the upload data plane, the storage connector platform, and internal crate ownership. Added an explicit `single` / `cluster` deployment profile, Redis cross-instance event synchronization, reverse tunnel owner lease and forwarding, and Kubernetes Kustomize / Helm deployment paths; the initialization flow unified into the three states `needs_admin` / `needs_storage` / `ready`, and requires the administrator to explicitly create the first storage policy.

WebDAV migrated to the AsterForge WebDAV 0.2 protocol engine, adding multi-Range downloads, RFC 4331 quota properties, virtual mount root locks, directory pagination, resource budgets, and request-level observability; and implemented RFC 3253 core DeltaV versioning on top of the canonical revision ledger, providing `VERSION-CONTROL`, version-tree / expand-property `REPORT`, and read-only immutable version resources. The resource lock system moved to a database-authoritative namespace / generation model, with conditional PUT / COPY and atomic recursive DELETE completed. File and directory APIs upgraded from the boolean `is_locked` to a structured `lock_state`. On the upload side, a OneDrive server relay resumable mode was added, and 0.5.0 session boundaries were tightened, completely removing the old payload-per-chunk compatibility path.

Storage policies were refactored from built-in `DriverType` branches and flat frontend/backend fields into a connector-owned contract: a stable reverse-domain `ConnectorId`, versioned `storage_config`, a separate credential schema, descriptor-driven fields and actions, connector-provided localization resources, and drivers dynamically constructed by the registry. Local, S3, SFTP, OneDrive, Azure Blob, Tencent COS, and Remote connectors all joined the same contract; old credentials were converted under a migration lock during the 0.5.x startup phase into encrypted connector-owned payloads, while the old database structure was retained until 0.6.0 for unified removal.

- **Multi-Primary cluster deployment** — shared PostgreSQL / MySQL, Redis cache / config sync, cross-instance storage events, task and scheduler fencing, reverse tunnel owner routing
- **Kubernetes production deployment** — Kustomize base / overlays, Helm chart, StatefulSet, PDB, NetworkPolicy, RWX avatar storage, and multi-Primary E2E
- **WebDAV, DeltaV, and resource lock refactor** — AsterForge WebDAV 0.2, multi-Range, quota properties, RFC 3253 core versioning, conditional writes, atomic mutations, virtual root locks, structured `lock_state`, Litmus stress suites
- **Storage connector platformization** — plugin-ready registry, versioned typed config, descriptor / action / capability contract, separate credential schema, connector-owned localization
- **Storage policy and credential migration** — the current policy schema converged to `connector_id` + `storage_config`, with static and OAuth credentials migrated into encrypted `storage_policy_connector_credentials`
- **Upload and object storage enhancements** — OneDrive server relay resumable, concurrent chunk claims, S3 SigV4 signing region configuration, Tencent COS native Q-Sign
- **Alibaba Cloud OSS native support** — OSS V4 signing, public / server-side endpoints, CNAME, presigned PUT, multipart, and full connector integration
- **Qiniu Cloud Kodo native support** — official HTTPS S3 endpoint, AWS SigV4, Qiniu S3 bucket names, presigned GET/PUT, multipart, and full connector integration
- **Initialization and configuration convergence** — three-state setup, explicit creation of the first storage policy, structured database / Redis credentials, Redis failures no longer silently degrade
- **Internal ownership convergence** — Drive retains the four domain crates of model, migration, storage, and metrics; the common HTTP body limit and ciphertext envelope are reused from AsterForge

### Breaking

- **File version database model** — `m20260813_000001_canonical_file_revision_ledger` migrates the mutable `file_versions` table into `file_revision_histories`, `file_revisions`, and `file_revision_properties`, dropping the original table after backfill. External scripts that query the old table directly, depend on old version-number primary keys, or write version history bypassing REST/services need migration; the downgrade migration rebuilds a representable legacy history, but the new ledger's actor, comment, reason, property snapshot, and stable public ID have no corresponding fields in the old table.
- **Storage policy API / schema** — Storage policies no longer expose provider-specific flat fields such as `driver_type`, `endpoint`, `bucket`, `base_path`, `access_key`, `secret_key`, and `options`; responses now use the stable `connector_id`, `connector_config`, and `behavior`, creation requests uniformly submit `connection = { connector_config, behavior, credential }`, and update requests submit connector config, behavior, and tagged credential separately. Clients depending on old DTOs, the `DriverType` enum, or field names need to construct requests per the connector descriptor.
- **Storage credential input** — Static keys and authorization application configs became mutually exclusive tagged credential channels (`none` / `static` / `authorization_application`), with field names defined by the connector schema — for example, S3 and Tencent COS use their own namespaces, no longer sharing the ambiguous `access_key` / `secret_key` form fields.
- **Storage connector action / promotion** — Removed the old dedicated `promote-s3-driver` API and provider-specific action enums; generic actions continue to read action IDs, endpoints, input fields, and side-effect declarations from the connector catalog; in-place promotion now uses `promote-connector`, with the target connector's `promotions` descriptor declaring allowed source connectors, config matching requirements, config / credential field mappings, and object namespace fields that must remain unchanged.
- **Storage-native processing behavior** — `thumbnail_processor`, `thumbnail_extensions`, and `media_metadata_extensions` converge into four fields: `storage_native_thumbnail_enabled` / `storage_native_thumbnail_extensions` and `storage_native_media_metadata_enabled` / `storage_native_media_metadata_extensions`. Disabling native processing only removes the provider-native candidate; it does not turn off the global thumbnail or media metadata processing chain.
- **Presigned upload response** — upload init no longer returns `presigned_url` and `presigned_headers` separately, and multipart part presign no longer returns a plain URL; both now uniformly return `PresignedUploadRequest { url, headers? }`. Browsers or third-party clients must forward the headers in the descriptor as-is and must not add provider-specific headers on their own.
- **Upload session 0.5.0 boundary** — `upload_sessions.session_kind` is tightened to `NOT NULL`, and the 0.4.x payload-per-chunk `chunk_N`, `assembled` assembly/relay, kind inference, and assembly limiter are removed. The upgrade migration stops and preserves the original row when it encounters a null or invalid kind; deployers need to clean up expired legacy upload sessions first.
- **Resource lock API schema** — `is_locked: boolean` in file, directory, search, recycle bin, and admin DTOs is replaced with the structured `lock_state`, distinguishing `unlocked`, `direct`, and `inherited`. API clients relying on the old field need to update.
- **Initialization flow adjustment** — new instances no longer automatically create the `Local Default` storage policy. After the first administrator is created, the system enters `needs_storage`; the administrator must explicitly create the first default storage policy before the system enters `ready`.
- **Redis startup semantics** — when a Redis cache is configured, connection or configuration errors terminate startup instead of silently falling back to the in-process memory cache; `/health/ready` returns `503` when the cache is unavailable.

### Added

- **GitHub PR and CI lifecycle automation** — using a pinned-revision organization-shared Action, repository-owned configuration, and the `AsterCommunity Automation` GitHub App identity, it idempotently maintains language, documentation, product scope, and high-risk labels based on changed files, and links PR status to closing issues; it aggregates path-filtered workflows on the current PR HEAD into a stable `PR Gate` and an always-published, in-place-updated diagnostic comment, terminates all historical unfinished Gates when the HEAD is superseded or the PR is closed or merged, and maintains failure Issues with fingerprints and consecutive-recovery determination for default-branch and scheduled-job failures.

- **Multi-primary deployment profile**
  - Added `[deployment].profile = "single" | "cluster"`; cluster mode requires shared PostgreSQL / MySQL, Redis cache, Redis config sync, and shared object storage
  - storage topology, user policy groups, and storage change events support cross-instance publishing, reconnect reconciliation, and `sync.required` notifications
  - scheduler lease, background task claim, mail dispatch, and migration use database coordination and fencing, covering failover and database-partition recovery tests
  - reverse tunnels add an owner directory, lease / fencing tokens, and an inter-primary HMAC streaming proxy; a request hitting a non-owner primary can be forwarded to the current owner
  - `aster_drive doctor` adds static configuration and deployment topology checks; `/health/ready` validates shared dependencies and runtime topology
- **Kubernetes / Helm deployment**
  - Added multi-primary StatefulSet, headless / ClusterIP Service, PodDisruptionBudget, RWX avatar PVC, Ingress examples, and NetworkPolicy
  - Provided an OrbStack smoke-test overlay, production-example overlay, and Helm chart; the chart validates sensitive configuration, selector labels, resource name length, and the fixed shared-storage boundary
- **Slim Docker images** — GHCR and Docker Hub add default / metrics `-slim` multi-architecture tags that do not bundle the optional FFmpeg, ffprobe, and libvips toolchains; existing tags continue to publish fully equipped media-processing images, and full and slim artifacts of the same feature / architecture share one Rust binary build.
- **Three-state system initialization** — Added `SystemSetupState` (`needs_admin` / `needs_storage` / `ready`), propagated to `/auth/check`, `/health/ready`, route guards, and the frontend first-storage setup flow.
- **Structured connection credentials** — database, cache, and config-sync endpoints support the `{ base_url, username, password }` inline table and nested environment variables; raw reserved characters are safely encoded by the configuration layer.
- **Plugin-ready storage connector registry**
  - Added the object-safe `StorageConnector` contract and `StorageConnectorRegistry`; connectors use dynamic dispatch to handle descriptor, config decoding, credential handling, authorization, connection tests, actions, and driver construction; the business orchestration layer no longer matches built-in providers
  - connectors only receive the controlled `StorageConnectorContext` instead of the full application state; the registry registers and validates Local, S3, SFTP, OneDrive, Azure Blob, Tencent COS, and Remote connectors once at startup
  - Added stable reverse-domain `ConnectorId`s (e.g. `asterdrive.storage.s3`) with length, character, and namespace segment validation; persisted envelopes for unknown connectors retain the original data and report unavailability instead of being silently rewritten or dropped
  - descriptors declare deployment scope, initial-setup support, credential mode, object and upload capabilities, config schema version, credential schema version, UI badge RGB, fields, and actions; multi-primary topology and policy capability validation uniformly consume the registry contract
- **Versioned storage policy configuration**
  - Added `StoragePolicyConfigEnvelope`, placing connector-owned typed config and core-owned behavior into a single `storage_config`; connector config carries its own format version, connector ID, schema version, and typed values
  - Local, S3, SFTP, OneDrive, Azure Blob, Tencent COS, and Remote all use Rust typed schemas to generate descriptors, defaults, and serialization/deserialization rules, removing hand-assembled JSON and the provider field matrix maintained in duplicate on frontend and backend
  - policy create, update, draft connection test, and action input uniformly use the normalization / validation contract defined by the descriptor
- **Connector-owned credential lifecycle**
  - Added `storage_policy_connector_credentials`, storing static credentials, OAuth application configuration, and delegated credentials with connector ID, an independent credential schema version, revision, and encrypted payload
  - credential schema and config schema evolve independently; the encryption AAD binds policy ID, connector ID, and credential schema version
- **Connector descriptor–driven admin console**
  - storage policy creation, editing, connection testing, and action UI switch to a unified field renderer that renders text, secret, boolean, number, select, automatic/preset/custom string options, conditional visibility/required/options, linked default values, deactivated-value cleanup, collapsed advanced fields, placeholders, descriptions, badges, confirmations, and saved-policy gates per descriptor
  - action select supports `remote_nodes` and node-dependent `remote_storage_targets` dynamic data sources
- **Connector-owned admin console localization**
  - Added the `StorageConnectorLocalization` contract; built-in connectors keep field, action, authorization status, and credential management copy alongside the connector implementation, with message ID, locale, and key coverage validated at registry registration
  - admin console languages migrate from the fixed `en | zh` enumeration to a normalized BCP 47 `LocaleTag`; connector bundles resolve by request locale and fall back to their own default locale
  - Added the admin-only `/api/v1/admin/policies/storage-drivers/localizations`, returning namespaced resources by catalog context and locale, with a stable ETag / `304 Not Modified`
- **OneDrive server-relay resumable upload**
  - Added the `ProviderRelayResumable` session kind; the server streams Microsoft Graph fragments in order without buffering an entire fragment in memory
  - chunk claim, heartbeat, stale claim recovery, and progress reconciliation use atomic database coordination; the frontend adjusts concurrency and ordering based on the backend-returned `upload_scheduling`
  - chunks in concurrent uploads return the retryable `202 upload.chunk_pending`; an independent 90-second fragment timeout prevents claims from being held indefinitely
- **S3 SigV4 signing region** — S3-compatible policies add `s3_region`, defaulting to `auto`; connection tests and runtime use the same signing region, validated across the model, descriptor, admin UI, and API layers.
- **Tencent COS native AWS SDK auth replacement** — plugs COS Q-Sign into the AWS SDK auth scheme slot, unifying plain GET / PUT / COPY, presigned GET / PUT / UploadPart, and the multipart lifecycle; before signing, `x-amz-*` headers, checksums, and queries are normalized, and the canonical path for non-ASCII / reserved-character object keys is encoded only once.
- **Alibaba Cloud OSS connector**
  - Added the `asterdrive.storage.alibaba_oss` connector with independent config/credential schemas, descriptor, localization resources, and a connection-test entry, along with synced Chinese/English admin/developer documentation and E2E verification
  - backend I/O supports an optional server-side endpoint while browser presigned URLs always use the public endpoint; standard OSS endpoints and CNAME addressing are supported
  - reuses the AWS S3 operation/runtime while implementing plain requests, presigned GET/PUT, UploadPart, and the multipart lifecycle via the OSS-native `OSS4-HMAC-SHA256` auth scheme
- **Qiniu Kodo connector**
  - Added the `asterdrive.storage.qiniu` V1 connector with an independent static credential schema, descriptor, localization resources, and catalog projection, plus Chinese/English admin and developer documentation
  - uses only the Kodo S3-compatible API with AWS SigV4; accepts and normalizes official HTTPS service endpoints and S3 bucket-level endpoints, validates the matching signing region and Qiniu S3 bucket name, and lets the connector choose the addressing mode automatically, without keeping the QBox, UploadToken, or native form-upload data plane
  - reuses the shared S3 data plane, supporting plain object I/O, Range, presigned GET/PUT, multipart, ETag, connection testing, and stable error classification; provides a real-Kodo smoke harness protected by environment credentials
- **WebDAV provider Range performance baseline** — Added `get_range` / `get_stream` benchmarks covering Local, S3, OneDrive, SFTP, Remote, and fallback paths, versioned baselines, p95 TTFB / p50 throughput regression policy, provider artifact verification, and scheduled CI; without external credentials it produces a structured skipped artifact, redacted by actual secret values before artifact upload.
- **Storage connector documentation projection** — Added `make storage-docs` / `make storage-docs-check`, which generate a reviewable manifest, backend matrix, policy capability table, and sidebar from runtime descriptors and the localization catalog, and block code-capability-versus-documentation drift in CI.
- **WebDAV 0.2 capabilities**
  - Supports single-Range / multi-Range GET, RFC 4331 `quota-used-bytes` / `quota-available-bytes`, directory keyset pagination, and capability snapshot method gating
  - COPY / MOVE / DELETE enforce resource budgets on file count, directory count, depth, and frontier; exceeding limits returns `507` with the stable error code `operation.resource_limit_exceeded`
  - Supports personal / team virtual mount root locks, expressing file, directory, and workspace lock scope with a structured root
  - LOCK can create a lock-null empty file for a missing non-collection resource when the parent directory exists, followed by normal UNLOCK and write flows
  - The admin console lock list shows badges for file, directory, workspace root, and unknown root types to help distinguish lock scope
  - Added WebDAV operation observation recording transferred bytes, Range count, accessed resource count, backend calls, protocol failures, and stream completion / cancellation
  - Litmus CI is split into baseline and scheduled stress suites, with extended compatibility coverage for curl, rclone, largefile, lockbomb, and more
- **RFC 3253 core DeltaV versioning**
  - Supports `VERSION-CONTROL` on plain WebDAV files, recording subsequent content and dead property mutations as canonical revisions in checked-in / checkout-checkin auto-version mode
  - Supports version-tree and expand-property `REPORT`, plus the `checked-in`, `auto-version`, and `supported-report-set` live properties
  - Exposes authorizable immutable version resources via `/.asterdrive-deltav/versions/{public_id}`; supports `GET` / `HEAD` / `PROPFIND` and rejects `PUT` / `DELETE` / `PROPPATCH`
  - Added raw HTTP DeltaV workflow, version-tree / expand-property REPORT, and cadaver 0.26 compatibility E2E coverage
- **Runtime diagnostics** — Added build revision / profile / target / variant identity, written to the startup log.

### Changed

- **Project license** — AsterDrive moves from a single MIT license to the dual `MIT OR Apache-2.0` license; users may choose either, and workspace, frontend, and documentation package metadata plus future contribution terms adopt the same declaration.
- **Empty-file metadata-only creation with transactional idempotency** — `/files/new` and the frontend zero-byte file flow now reuse the canonical `virtual_empty` Blob with no corresponding connector object instead of uploading a zero-byte object; added the `Idempotency-Key` writer-transaction claim / replay contract, and tightened download, WebDAV audit, presigning, thumbnail/preview, archive, deletion, and integrity audit boundaries accordingly.
- **Avatar upload resource bounds and synchronous result contract** — the built-in frontend normalizes crop results to a maximum 1024×1024 WebP, and the server bounds avatar peaks with streaming staging, a 10 MiB default / 16 MiB hard-cap source file limit, 1024×1024 dimensions, a 32 MiB decode allocation, and 2 concurrent renders per process; the images processor reuses the same decoder for dimension validation and pixel decoding, avoiding fully reading JPEG-compressed sources twice, and distinguishes normal queueing from actual rendering via wait-duration, waiting, and active metrics; before publishing it prepares the user directory outside the database transaction, preserves the scene on a final-version directory conflict, performs exactly one atomic rename inside the transaction, and exposes slow-storage latency via a publish duration metric; upload synchronously returns `profile + applied` — a concurrent avatar mutation that overwrites the candidate or fails processing keeps the current avatar without creating a background task.
- **Storage policy persistence model** — the current `storage_policy` entity converges to `id`, `name`, `connector_id`, `storage_config`, file size/type/default-policy/chunk behavior, and timestamps; the runtime reads endpoint, bucket, base path, remote bindings, and provider behavior via connector projection instead of directly accessing the old flattened columns.
- **Storage policy API orchestration** — create, update, draft connection test, saved connection test, authorization, custom action, and connector promotion uniformly look up and dispatch through the connector registry; promotion is declared by the target connector via source / requirement / mapping contracts, with built-in support for in-place promotion of matching generic S3 policies to Tencent COS, Alibaba Cloud OSS, or Qiniu Kodo; the server rejects active upload sessions before and after the transaction, samples existing objects through the candidate target driver, atomically replaces the policy config and re-encrypts credentials, records source / target / promotion / sample audit details, and refreshes driver, snapshot, media capability, and cross-instance topology; malformed / unknown connectors in requests return input validation errors, while unknown connector IDs in the database are treated as persisted configuration corruption.
- **Credential migration ownership** — 0.5.x startup, after runtime config and the encryption key become available but before listening for service, runs the idempotent legacy credential import under the database migration lock; `database-migrate apply` reuses the same importer after target-data replication verification completes.
- **Storage admin UI data ownership** — the frontend no longer maintains hardcoded form, action, account mode, and provider copy matrices for Local / S3 / SFTP / OneDrive / Azure Blob / Tencent COS / Remote; the connector catalog and localization resources become the source of truth for the configuration admin UI, while preserving the existing policy list, edit dialog, field descriptions, and badge visual hierarchy.
- **Storage-native processing semantics** — thumbnails and media metadata each use an explicit enabled flag and an extension list; when the switch is off, the extensions remain as dormant configuration, and an empty extension list matches no files even when enabled.
- **Presigned request ownership** — the URL and the headers required for signing are generated as a complete request descriptor by the same storage driver; S3, Azure Blob, Alibaba OSS, Tencent COS, Remote, and multipart refresh all use this contract, and the frontend no longer hardcodes `Content-Type: application/octet-stream`.
- **Async Future size** — the 64 KiB in-stack buffers in the archive preview and decompression copy loops become one bounded allocation per operation, shrinking the relevant Futures below 16 KiB; upload and WebDAV hot paths gain no extra boxing or per-request dynamic allocation.
- **Azure Blob chunked upload memory bound** — `put_reader` streams commits per Azure block boundary using a fixed 64 KiB request buffer per block, no longer allocating a whole heap chunk per effective block size; the 50,000-block planning, commit ordering, and declared-size validation remain unchanged.
- **Reverse tunnel upload memory bound** — Reader uploads stream directly when the stream lane is available, and post-consume failures are retried by the upper layer reopening the data source; polling carries only small bodies that fit the follower's 1 MiB control-plane envelope budget, removing extra copies of the request body and the full base64 string.
- **Local default storage path** — the Local connector, driver, first-time setup, and tests uniformly use `./data/uploads`; a missing or empty base path is filled in from the connector default, no longer scattered across multiple fallbacks.
- **S3-compatible driver reuse** — Extracted AWS request vendor normalization and converged duplicate S3-compatible / Tencent COS implementations via storage / multipart delegation macros; the S3 driver constructor now takes explicit options + an SDK config hook for provider auth and signing customization.
- **Authoritative resource lock model** — Removed the persisted `files.is_locked` / `folders.is_locked` booleans; added workspace-scoped `resource_lock_namespaces`, a generation counter, and structured lock roots, with write paths re-validated via namespace locks and `SELECT FOR UPDATE` in the same transaction.
- **REST directory tree mutation memory bounds** — Folder delete and trash restore now use explicit resource, frontier, and depth budgets for synchronous requests; personal / team operations exceeding the budget return `202` with `folder_tree_mutation` task details structured for both English and Chinese display. Background tasks scan by bounded ID pages and a DFS depth stack, keeping delete / restore, locks, audit, and storage changes consistent through persisted staging membership, an Infinity resource lock, and same-transaction final-state commits; existing WebDAV budgets and protocol behavior are unchanged.
- **WebDAV mutation atomicity** — DELETE / MOVE / COPY now fold resource changes, lock cleanup, and lock path rebinding into the same writer transaction; a recursive DELETE that hits a lock conflict or backend failure rolls back entirely and returns a request-level `423` / `500` instead of committing partial results; UNLOCK / force unlock likewise guarantees consistent rollback of lock rows and related state.
- **WebDAV protocol ownership** — Protocol parsing, XML, HTTP conditional / Range, lock grammar, and canonical responses moved to `aster_forge_webdav` / `aster_forge_xml`; Drive retains the workspace, permission, persistence, storage, quota, audit, and integration layers.
- **Upload session contract** — Chunk PUT, Progress, Complete, and Cancel / Cleanup only accept a persisted explicit `UploadSessionKind`, and continue validating the combined invariants of multipart, temp keys, and provider session metadata; OffsetStaging, StreamStaging, relay, presigned, and resumable main paths keep connector-owned transport negotiation.
- **Presigned PUT completion contract** — For single-object presigned PUT on OSS and Tencent COS, the server now validates object metadata and size instead of requiring the browser to read the ETag; multipart parts must still retain the ETag. `presigned_put_requires_etag` renamed to `presigned_single_put_requires_etag` to express the boundary.
- **Workspace crate split**
  - `aster_drive_model`: shared types and SeaORM entities
  - `aster_drive_migration`: database migrations
  - `aster_drive_storage`: driver traits, connector descriptors, typed config / localization contract, object keys, and structured storage errors
  - `aster_drive_metrics`: Drive metrics contract, Noop recorder, and AsterForge adapter
- **Forge HTTP / crypto integration** — Removed the local `aster_drive_http`; WOPI discovery and Tencent COS now use `aster_forge_http`; MFA secrets and storage credential tokens now use the AsterForge versioned secret envelope while preserving the existing HKDF context, AAD, master-key policy, and `v1:nonce:ciphertext` compatibility.
- **Dependency baseline upgrades** — SeaORM / migrations upgraded to `2.0.1`, Arrow to `58.4.0`, with Russh, Lettre, AsterForge, and frontend build / test dependencies updated in sync.
- **Integration test container infrastructure** — `testcontainers` upgraded to `0.28`; shared PostgreSQL / MySQL containers, checkout isolation, PID resource registration, orphan database reclamation, and the MySQL parallel schema parameter are now provided by `aster_forge_test`, with Drive keeping only product migrations, test accounts, and template schema orchestration.
- **XML and remote response boundaries** — WOPI discovery, Tencent COS CORS / media metadata, and other XML paths now uniformly use `aster_forge_xml`; HTTP responses switched to streaming bounded reads covering size, depth, element-count, and DTD / entity injection limits.
- **Release diagnosability** — Release profile changed to `strip = "debuginfo"`, keeping function symbols for panic backtraces; Docker / release workflows inject the Git revision.
- **Integration test structure** — The previously flat `tests/test_*.rs` files were consolidated into domain targets such as auth, files, sharing, storage, operations, platform, multi_primary, and wopi.

### Fixed

- **Remote node probe and reverse tunnel telemetry isolation** — Clarified that `managed_followers.last_error` / `last_checked_at` only reflect the most recent explicit capability probe or connection test; `tunnel_last_error` / `tunnel_last_seen_at` only reflect runtime state that can be cleared by the next successful poll or stream handshake. Fixed integration tests that mistook transient tunnel errors for historical errors and raced the worker's next poll round, and added field comments plus management API semantics documentation.
- **Remote node effective transport mode** — The primary resolves `direct` / `reverse_tunnel` / `auto` into a strongly typed `resolved_transport` desired state; the follower continuously pulls over a signed binding control plane independent of the object data path, persisting, applying, and implicitly ACKing revisions on the next round. Direct, disabled, and auto nodes with a usable `base_url` do not start polling / WebSocket workers, and the primary simultaneously rejects their tunnel poll, complete, and connect; bidirectional transport switching converges eventually even when both the old and new paths are unavailable. Legacy followers remain compatible via capability-driven `PUT /binding` push; when the WebSocket lane stops, a normal close handshake completes, and request-body or local-response blocking responds to cancellation while reaping all child tasks. Tunnel frame version stays at `1`; the internal storage protocol stays at `v5` with a minimum of `v4`.
- **First-login cookie bootstrap** — New installs allow HTTP first login by default; HTTPS setup enables the Secure Cookie before subsequent automatic logins, and existing runtime settings are not overridden by upgrades or restarts.
- **Upload failure and retry orchestration** — Non-retryable upload-stage failures now terminate and clean up the session; correctable, authentication, database, and retryable errors keep the session for recovery. The frontend distinguishes single / batch retry and terminal task cleanup by retryability and serializes cleanup/retry to avoid concurrent cleanup and retry of the same task.
- **WebDAV lock and mutation consistency** — Fixed depth-inherited collection locks, parent/member lock root confusion, destination lock rebinding after MOVE, partial unlock commits, and swallowed backend errors.
- **WebDAV conditional writes and COPY** — PUT re-validates the target file snapshot, ETag, Last-Modified, and lock state inside the writer transaction after receiving the request body, rejecting concurrent overwrites and creation races; both COPY and MOVE enforce DAV `If` state-token / ETag conditions.
- **WebDAV lock expiry boundary** — A lock whose timeout equals the current moment is immediately treated as expired, no longer holding owner quota, appearing in discovery, or blocking resource operations.
- **WebDAV lock discovery** — `Depth: 0` locks now only match actually overlapping paths and no longer erroneously appear in child resource discovery.
- **WebDAV paths and status codes** — PUT / COPY / MOVE targeting a missing parent directory return `409 Conflict`; a locked destination root returns a request-level `423 Locked`; unsupported methods return `405` based on capability.
- **WebDAV streaming** — Improved reader lifecycle and error propagation; the default Range fallback keeps bounded cost, and the local driver uses native seek to read only the requested range.

### Security

- **WebDAV auth cache key hardening** — Added `auth.webdav_auth_cache_secret`; Redis cache keys are now derived via HMAC, reducing the risk of offline password enumeration after a cache key listing leak.
- **WebDAV / archive resource exhaustion protection** — XML control requests, directory traversal, archive scanning, Range counts, and response bodies all have explicit limits; exceeding them ends with stable protocol / API errors.
- **Argon2 concurrency limits** — Added `auth.password_hash_max_concurrency`, bounding the number of concurrently executing password hashing tasks per process and their working memory footprint.
- **Dependency and parser upgrades** — Removed `xmltree`; upgraded `base64`, `jsonwebtoken`, `validator`, and frontend dependencies; the frontend audit override updates `js-yaml` to `4.3.1` and records that the React Router RSC advisory does not apply to the Vite SPA.
- **Credential exposure cleanup after storage policy deletion** — Storage policy cleanup task payloads no longer embed static `access_key` / `secret_key`; connectors requiring credentials only copy the existing ciphertext bound to the policy ID, connector ID, and credential schema version, and the cleanup driver is constructed at execution time from an encrypted snapshot.

### Database Migrations

- `m20260817_000001_add_remote_binding_control_state`
  - Added binding desired / applied revisions to the primary's `managed_followers` and `resolved_transport`, desired revision, and applied revision to the follower's `master_bindings`; default transport is `reverse_tunnel` with revision `1`, so legacy payloads are read with the original tunnel behavior, supporting independent control pull, runtime reconciliation, and persisted ACK
- `m20260813_000001_canonical_file_revision_ledger`
  - Backfill `file_versions` and each file's current contents in recoverable 500-file transactional batches, establishing the immutable revision predecessor chain, stable public IDs, current head / next sequence, and user attribute snapshots, then deleting legacy `file_versions` upon completion; a mid-batch failure on rerun skips already-committed history and resumes after the legacy/ledger maximum revision ID
  - MySQL uses `utf8mb4_bin` virtual generated columns to maintain XML property `(namespace, name)` case-sensitive uniqueness, without setting a global gate on the server version string and without relying on 8.0.23's invisible-column syntax
- `m20260723_000001_require_upload_session_kind`
  - Check legacy / invalid upload sessions before upgrade and tighten `upload_sessions.session_kind` to `NOT NULL`
- `m20260725_000001_remote_tunnel_owners`
  - Added `remote_tunnel_owners`, persisting reverse tunnel owner runtime, internal endpoint, fencing token, and lease expiry
- `m20260728_000001_provider_relay_resumable_upload`
  - Added an index for provider relay resumable chunk claims, supporting atomic claiming and resumption across primaries
- `m20260803_000001_refactor_resource_locks`
  - Validate workspace / root identity of legacy locks; legacy `owner_info = NULL` locks migrate fail-closed as WebDAV-originated; upgrade stops and original data is preserved if any unresolvable or invalid legacy lock exists
  - Added `resource_lock_namespaces`, restructured the `resource_locks` schema, and removed the files / folders `is_locked` columns
- `m20260803_000002_storage_policy_connector_configs`
  - Added `storage_config` and `connector_id` to `storage_policies`, converting the legacy flat fields for Local, S3, SFTP, OneDrive, Azure Blob, Tencent COS, and Remote into versioned connector / behavior envelopes per the frozen 0.5.0 mapping
  - Fully resolve all legacy rows before the write transaction begins; stop the entire backfill without committing partial migration results if any policy fails to convert
- `m20260803_000003_add_storage_policy_connector_credentials`
  - Added `storage_policy_connector_credentials`, unique per policy, storing connector ID, credential schema version, revision, ciphertext, and UTC timestamps
  - Legacy static credentials, authorized application configs, and delegated credentials are encrypted and imported by the 0.5.x startup importer once the runtime encryption key is available; the historical migrations themselves never read runtime config or keys
- `m20260805_000001_allow_connector_policy_writes_with_legacy_schema`
  - Added an empty-string compatibility default for the legacy `driver_type` retained until 0.6.0, and only temporarily allow the deprecated `TEXT options` to be NULL on MySQL so the current slim entity can write against the 0.5.x schema
  - SQLite rebuild preserves current + legacy columns, data, and indexes; the production migration coordinator manages the connection-local foreign-key pragma outside the Forge transaction, validates referential integrity inside the transaction, and restores the original state after commit / rollback; PostgreSQL / MySQL forward and rollback explicitly restore the original constraints
- `m20260810_000001_folder_tree_operation_members`
  - Added folder-tree staging membership deduplicated by task, resource type, and resource ID; background scanning is recoverable, and final-state mutation, staging cleanup, operation lock release, and task success status commit in the same transaction

### Notes

- Before upgrading from `v0.4.0`, expired or abnormal legacy upload sessions should be cleaned up first; if the `session_kind` pre-check fails, the migration will not proceed.
- The 0.5.x storage upgrade has two stages: the SeaORM migration first backfills `connector_id` / `storage_config` and creates the new credential store; the service imports and re-encrypts legacy credentials once config, the master encryption key, and the migration lock are available. A policy's legacy `access_key` / `secret_key` is only cleared after its new ciphertext is successfully written, and converted rows in the deprecated credential stores are then deleted.
- 0.5.x deliberately retains nine legacy columns on `storage_policies`, the related indexes / foreign keys, and the two deprecated tables `storage_policy_credentials` and `storage_connector_application_configs`; they serve upgrade compatibility only, with physical deletion performed by #463 in 0.6.0, which will hard-fail and roll back if unmigrated credentials are found.
- `database-migrate apply` first completes copy verification against the target database, then runs the same credential importer under the target database's migration lock; MySQL legacy nullable `options` are normalized to `{}` when copied to SQLite / PostgreSQL.
- Clients using the storage policy management API must first read the connector catalog and construct requests from the descriptor's config / credential / action schemas, no longer relying on built-in driver enumerations or fixed provider fields. The connector localization endpoint is available only to authenticated admins and supports ETag caching.
- The Local connector's default data directory is now `./data/uploads`; migrations of existing policies keep their explicit base path, and the connector default is applied only when it is missing or empty.
- Third-party clients using the file / directory REST DTOs need to migrate from `is_locked` to `lock_state.state` and handle inherited locks.
- WebDAV DeltaV currently provides an RFC 3253 core subset: `VERSION-CONTROL`, version-tree / expand-property `REPORT`, and immutable version resources. The product management interface for version history remains the AsterDrive REST API; clients should not assume the full RFC 3253 workspace / activity / merge capabilities are implemented.
- `single` remains the default deployment profile; enabling `cluster` requires a shared database, Redis, shared object storage, and RWX avatar storage, with topology checked via `aster_drive doctor`.
- New installs stay in `needs_storage` after creating the admin, only entering `ready` once the first default storage policy is created; upgraded instances with an existing default policy need no repeated initialization.
- The Forge HTTP / secret envelope integration changes no database schema, public API, or existing ciphertext format, and requires no additional migration.
### Statistics

- 1394 files changed, 135113 insertions(+), 66601 deletions(-)
- 72 commits
- 9 database migrations

## [v0.4.0] - 2026-07-23

### Release Highlights

**AsterDrive `0.4.0` official release.** Building on the `0.4.0` beta / RC line (AsterForge shared crates infrastructure migration, the all-new download center and unified transfer panel, cross-workspace batch moves, OneDrive browser direct connections, external-auth browser binding, WebDAV auth rate limiting and forced setup, public share routed navigation, plus rc.2's AsterForge bug fixes and security hardening), this release introduces a WebDAV RFC 4918 compliance test baseline (Litmus Phase 0) and lands a batch of WebDAV protocol compliance fixes, advancing the `0.4.0` series from RC to stable release.

- **WebDAV compliance test baseline** — Introduced the Litmus Phase 0 compliance test framework, upgraded Litmus to 0.18, and added a dedicated WebDAV compatibility CI (multi-client matrix)
- **WebDAV protocol compliance fixes** — MKCOL returns 405 for existing resources, collection depth lock tokens are accepted for descendant operations, PROPFIND enforces strict `DAV:` namespace validation
- **WebDAV documentation filled in** — Added a user-facing WebDAV access guide

### Changed

- **WebDAV PROPFIND parsing compliance**
  - Element matching now strictly validates the `DAV:` namespace (`is_dav_element`) instead of matching by local name only
  - Unknown elements are treated as absent and skipped per RFC 4918 §17 instead of erroring; duplicate `include` is treated as an illegal request
  - Missing `prop` / `allprop` / `propname` no longer implicitly falls back to `AllProp`; the illegal request body is rejected instead
- **WebDAV test structure modularization** — The former `tests/test_webdav*.rs` integration tests moved to the `tests/webdav/` module directory, split into resource / security-policy cases per Litmus suite

### Fixed

- **WebDAV MKCOL semantics** — `MKCOL` returns `405 Method Not Allowed` for existing resources (previously it attempted creation), per RFC 4918
- **WebDAV collection lock descendant operations** — Unlock validation now matches the submitted token against each conflicting lock using that lock's own href: when a parent directory holds a depth lock (collection lock), descendant resource operations submitting that same lock token are no longer wrongly judged unauthorized

### Notes

- This is the official release of the `0.4.0` series
- Upgrading from `v0.4.0-rc.2` to `v0.4.0` adds no new database migrations
- No new required production config schema entries
- Docker users are advised to use the `v0.4.0`, `stable`, or `latest` image tags; `edge` remains reserved for subsequent pre-release versions
- Statistics: 37 files changed, 3461 insertions(+), 65 deletions(-)
- 8 commits in this scope

## [v0.4.0-rc.2] - 2026-07-21

### Release Highlights

**AsterDrive `v0.4.0-rc.2` is the second release candidate of the `0.4.0` series, focused on a large round of bug fixes and security hardening for the AsterForge shared infrastructure libraries.** The cache system fixes reservation leaks, expiry policy in the in-memory backend, and data resurrection after Redis failover recovery; config sync fixes hot-reload failures in single-process deployments and loss of restart-only config, with the Redis Pub/Sub transport extracted into a standalone events crate supporting automatic reconcile after reconnection; the database layer fixes LIKE escaping order and SQLite `BUSY` / `LOCKED` retry classification, unifying transaction retry configuration into a `RetryConfig` profile; the runtime fixes resource cleanup on aborted startup, a task lease overflow panic, and hot-spinning periodic tasks with zero interval. On the security side, CSRF token validation switched to constant-time comparison, external-auth return paths reject control characters, Microsoft multi-tenant tokens no longer trust the `email_verified` claim, and template rendering now uses single-pass placeholder expansion.

- **Cache system fixes** — reservation leak, per-entry expiration policy, Redis shadow data revival, glob prefix escaping
- **Config sync fixes** — single-process hot reload, restart-only config retention, events crate extraction and reconnect reconcile
- **Database layer fixes** — LIKE escaping, SQLite lock error retry classification, unified transaction retry configuration
- **Runtime and task fixes** — startup-abort resource cleanup, lease overflow saturation, periodic task interval clamping, scheduled task claim renewal
- **Security hardening** — CSRF constant-time comparison, return path control character validation, Microsoft `email_verified` enforcement, single-pass template expansion

### Changed

- **Dependency updates** — AsterForge `55dbb87e` → `19e82ace` (7 upstream commits); transaction retry API migrated from `TransactionRetryConfig` to the unified `RetryConfig::deadlock()` profile, with retry backoff parameters tuned for deadlock scenarios
- **Internal: event transport layer extraction** — Redis Pub/Sub connection / reconnection / backoff / shutdown logic extracted into a standalone `aster_forge_events` crate; config sync delegates subscription lifecycle management to it (purely internal refactor, no behavior change)

### Fixed

- **Cache system**
  - In-memory backend now expires each entry independently, fixing premature eviction and immediate invalidation on write when `default_ttl=0`
  - Cache reservations now use an RAII guard, fixing reservation leaks under concurrent races; removed a faulty `remove` that could delete a concurrently written new value
  - Redis backend clears the local shadow after a successful remote read, fixing revival of never-persisted data after failure recovery
  - `invalidate_prefix` escapes glob metacharacters so the prefix matches literally
- **Config sync**
  - Fixed config hot reload becoming unavailable when `publish_reload` fails in single-process deployments (no subscription worker)
  - `reload()` / `replace()` retain restart-only config records; in-process values persist until restart
  - Numeric config validation rejects NaN / infinity, preventing non-finite values from entering arithmetic operations
  - Config sync supervisor supports bounded exponential backoff reconnection and automatically reconciles the full configuration after reconnecting
- **Database layer**
  - Fixed LIKE escaping order (escape backslashes before wildcards); SQLite adds an explicit `ESCAPE` clause
  - SQLite `BUSY` / `LOCKED` error family (including extended error codes) classified as retryable
  - On connection close, both the reader / writer connection pools attempt to close; audit logs retain driver-level error classification
- **Runtime and background tasks**
  - When a required component aborts startup, already-started components still run their shutdown phase, ensuring resource cleanup
  - Task lease expiry calculation saturates to the maximum duration on overflow instead of panicking
  - Periodic task zero intervals are clamped to a 1s minimum, preventing hot loops from hammering the database
  - Added scheduled task claim renewal to protect long-running tasks
  - When replica numbers are exhausted, a new replica tier opens instead of wrapping around or panicking
- **Other**
  - File classification only accepts ASCII alphanumeric extensions; `text/csv` correctly classified as Spreadsheet
  - SMTP auth username trim semantics kept consistent between readiness validation and credential mounting
  - Metrics batch registration rolls back partial registrations on failure; `RUST_LOG` distinguishes unset from invalid values; panic hook no longer double-panics when stderr is closed

### Security

- **CSRF token timing side channel** — double-submit token validation switched to constant-time comparison, preventing timing analysis from probing token values
- **External auth return path injection** — return path rejects CR / LF / TAB / NUL control characters, preventing redirect and log injection
- **Microsoft multi-tenant token claim trust** — multi-tenant tokens force `email_verified: false`, no longer trusting attacker-controllable claims
- **Template recursive substitution** — placeholder expansion runs in a single pass, so user-controlled values no longer trigger second-pass substitution; `&amp;` is decoded last to avoid double-decoding into `<`
- **CORS preflight matching** — configured header names normalized to lowercase, fixing preflight matching failures when browsers send lowercase headers
- **Rate limit Retry-After** — sub-second retry delays round up to 1s, no longer returning `Retry-After: 0` inviting immediate retries

### Statistics

- 5 files changed, 98 insertions(+), 63 deletions(-)
- 2 commits
- AsterForge `55dbb87e` → `19e82ace` (7 upstream commits)

### Notes

- This version is a pure infrastructure fix release with no database migrations and no API changes
- Config hot reload for single-process deployments is fixed in this version; instances previously affected by broken hot reload need no additional action after upgrading
- Instances running `v0.4.0-rc.1` are advised to continue validating after upgrading

## [v0.4.0-rc.1] - 2026-07-20

### Release Highlights

**AsterDrive `v0.4.0-rc.1` is the first release candidate of the `0.4.0` series, focused on the all-new download center, cross-workspace batch move, and OneDrive browser direct connection.** File downloads are fully redesigned: download actions open a method-selection dialog offering four options — proxied file download, proxied archive download (ZIP), File System Access API folder download, and browser default download; a unified transfer activity panel in the bottom-right corner shows upload and download tasks together with progress, speed, and estimated completion time, supporting cancellation and failure retry. Batch operations support moving files and folders between personal and team workspaces (copy-then-delete semantics, locked resources rejected as a whole and fully reported); single-item moves / copies are routed automatically to file / folder-specific endpoints, no longer occupying the batch channel.
OneDrive storage policies add browser direct-connection capability: uploads can use Microsoft Graph upload sessions for chunked direct transfer without server relay, with upload URLs encrypted at rest; downloads can return the Graph native address directly, and `original_filename` object naming lets the browser get the correct file name. On the deployment side, the Docker base image upgrades to Alpine 3.24 and pins libtiff 4.7.2 to fix CVE-2026-4775; SeaORM upgrades to the 2.0.0 stable release; the docs site migrates from VitePress to Astro Starlight.

- **Download center** — four download methods, unified transfer activity panel, progress / speed / cancel / retry; admins can toggle archive download separately per user and per share
- **Cross-workspace batch move** — move between personal / team workspaces, copy-then-delete semantics, full reporting for locked resources; single-item operations go directly to dedicated endpoints
- **OneDrive browser direct upload** — Microsoft Graph upload session chunked direct transfer, upload URLs encrypted at rest, cancellation and resumable-state query supported
- **OneDrive direct download** — returns the Graph native download address directly; `original_filename` object naming guarantees correct download file names
- **Docker image security update** — Alpine 3.23 → 3.24, pins libtiff 4.7.2-r0 to fix CVE-2026-4775, requires ffmpeg ≥ 8.1.2
- **Docs site migration** — VitePress → Astro Starlight, search / multi-version / component system fully rebuilt

### Added

- **Download center and unified transfer panel**
  - Download actions open a method-selection dialog: proxied file download (frontend streaming, progress bar), proxied archive download (backend-built ZIP), File System Access API folder download (preserves directory structure), browser default download
  - The bottom-right transfer activity panel shows upload and download tasks together, displaying progress, speed, estimated completion time, and queued / preparing / downloading / completed / failed / canceled states
  - Supports canceling in-progress downloads and retrying failed downloads; download task counts show in the app title bar
  - Content-Disposition file name parsing supports RFC 5987 and quoted formats

- **Admin toggles for archive download**
  - Added runtime configs `archive_download_user_enabled` / `archive_download_share_enabled` (both on by default), controlling archive download for authenticated users and public share visitors respectively
  - Frontend public config version bumped to 2, carrying archive download capability flags; old-version caches invalidate automatically
  - When a toggle is off, the UI hides the corresponding download option; saving related settings in the admin panel auto-refreshes the frontend config

- **Cross-workspace batch move**
  - Added `POST /workspace-transfer/move` endpoint, supporting moving files and folders between personal and team workspaces
  - Cross-workspace move uses copy-then-delete semantics: a copy is created at the target, then the source is deleted; if source deletion fails, the target copy is rolled back and cleaned up
  - Locked resources cause the whole operation to be rejected with all blocked resources reported; audit logs record source and target details
  - The destination folder dialog adds source / target workspace selectors
  - Single-item moves / copies are routed automatically to `PATCH /files/{id}` / `PATCH /folders/{id}`; only multiple items (2+) go through `POST /batch/move` / `POST /batch/copy`

- **OneDrive browser direct upload**
  - OneDrive storage policies add `provider_resumable_upload_strategy`: `server_relay` (default, via server relay) or `frontend_direct` (browser direct upload)
  - Direct-upload mode creates a Microsoft Graph upload session; the browser uploads chunks directly with Content-Range, with automatic retry on 416 range conflicts and transient failures
  - Upload URLs are encrypted at rest as `provider_session_ciphertext`, supporting cancellation (abort session) and resumable-state query (next_expected_ranges)
  - Upload sessions add the `provider_resumable` data-plane type with a corresponding completion plan (validates object size)

- **OneDrive direct download and object naming**
  - OneDrive storage policies add `provider_download_strategy`: when `frontend_direct`, the Microsoft Graph native download address is returned directly
  - Added connector-level `object_naming` capability declaration (`opaque_uuid` / `original_filename`); OneDrive object layout changed to `files/{upload_uuid}/{filename}`, and the Graph native download address returns the correct file name

- **Security disclosure channel**
  - Added RFC 9116 `/.well-known/security.txt`; README and SECURITY.md add security advisory links

### Changed

- **Internal: storage driver extension interface refactor** — removed standalone downcast methods like `as_presigned()` / `as_list()` / `as_stream_upload()`, unified into `extensions()` returning `StorageDriverExtensions`; decorators forward the extension bundle wholesale, so new driver capabilities no longer require changing every wrapper (purely internal, no behavior change)
- **Internal: frontend toolchain** — TypeScript 6 + openapi-typescript (bunx), refreshed dependency lockfile (toolchain only, no behavior change)
- **Dependency updates** — SeaORM `2.0.0-rc.43` → `2.0.0` stable
- **Docker base image** — Alpine 3.23 → 3.24
- **Docs site** — migrated from VitePress to Astro Starlight; navigation / search / multi-version build system rebuilt

### Fixed

- **Directory download pagination loop** — no longer loops forever when an empty page returns a stale cursor
- **Stale selection fallback** — when selections on category / search pages expire, fall back to backend archive download
- **Batch move reliability** — target copies are rolled back and cleaned up on move failure; when locked resources block a move, all blocked resources in the selection are reported
- **Upload worker idle spinning** — workers are no longer created when the queue is empty
- **OneDrive error message redaction** — sensitive upload URLs in client error messages are uniformly redacted, preventing token leakage into logs

### Security

- **Docker image libtiff CVE fix** — the Alpine 3.24 repository still ships libtiff 4.7.1 affected by CVE-2026-4775, so the image build pins `tiff=4.7.2-r0` from the Alpine edge repository and requires `ffmpeg>=8.1.2-r0`
- **OneDrive upload URL leak prevention** — upload URLs encrypted at rest (`provider_session_ciphertext`), client error messages uniformly redacted

### Database Migrations

- `m20260719_000001_add_upload_provider_session`
  - Added `provider_session_ciphertext` column (nullable) to `upload_sessions`, storing encrypted provider session metadata (e.g. OneDrive direct-upload URLs)
  - Non-null only under the provider resumable data plane; existing sessions are unaffected

### Statistics

- 422 files changed, 17927 insertions(+), 6362 deletions(-)
- 13 commits
- 1 database migration

### Notes

- This version is the first release candidate of the `0.4.0` series; focused validation of each download method in the download center, cross-workspace move, and OneDrive browser direct connection is recommended
- 1 database migration runs automatically at startup
- Both archive download toggles are on by default (matching legacy behavior); disable them in the admin panel if not needed
- OneDrive storage policies still default to `server_relay`; after switching to `frontend_direct`, the browser must be able to reach the corresponding Microsoft Graph endpoints directly (international / 21Vianet)
- The Docker base image upgrades to Alpine 3.24; deployments built on custom image layers should re-validate

## [v0.4.0-beta.3] - 2026-07-18

### Release Highlights

**AsterDrive `v0.4.0-beta.3` is the third beta of the `0.4.0` series, focused on architectural improvements to the upload data plane, WebDAV security hardening, and a fix for a storage quota concurrency deadlock.** Chunked uploads introduce the `.offset-staging-v1` staging file format: the Init phase preallocates space, Chunk PUTs write into the staging file at offsets, and standalone files are no longer created per chunk; upload sessions add a persisted `session_kind` that explicitly distinguishes data-plane types, with legacy sessions inferred via a compatibility branch (the compatibility branch is planned for removal in `0.5.0`); chunk assembly is brought under the global concurrency limit, curbing temporary-file resource usage under highly concurrent uploads.
On the security side, this version contains a WebDAV security fix (GHSA-7797-6gjx-hwgh, CVE pending); all instances with WebDAV enabled are advised to upgrade as soon as possible; remote protocol control-plane responses are uniformly size-limited to defend against memory exhaustion from malicious remote responses; the release profile panic strategy changes from `abort` to `unwind`, so a single-request panic no longer terminates the whole process. Additionally, storage quotas lock quota rows in order before writes, eliminating the deadlock when InnoDB shared locks upgrade to exclusive locks; remote storage listing supports cursor pagination (max 1000 items per page, response body limited to 8MB).

- **Upload offset-staging file** — Init preallocates, Chunk PUTs write at offsets, retries only validate the receipt without rewriting data
- **Upload session kind persistence** — `session_kind` explicitly distinguishes data planes; chunk assembly under a global concurrency limit
- **WebDAV security fix** — GHSA-7797-6gjx-hwgh (CVE pending); instances with WebDAV enabled are advised to upgrade as soon as possible
- **Quota deadlock fix** — quota rows locked in order before writes, eliminating the InnoDB lock-upgrade deadlock
- **Remote storage list pagination** — cursor pagination with a max of 1000 items per page, response body limited to 8MB
- **Improved process fault tolerance** — panic strategy `abort` → `unwind`; a single-request panic no longer takes down the whole process

### Added

- **Upload offset-staging file**
  - Added `.offset-staging-v1` staging file format; the Init phase preallocates space, and each Chunk PUT writes at its offset
  - Chunk PUT runs `sync_data` first, then records the receipt in a short transaction; retries only validate the existing receipt and do not rewrite data
  - The `staging.rs` module centrally manages the offset-staging file lifecycle

- **Upload session kind persistence and concurrency limit**
  - `upload_sessions` adds a `session_kind` field that explicitly distinguishes data planes such as offset_staging / stream_staging / provider_relay_multipart / provider_presigned_single
  - `resolve_upload_session_kind()` uniformly handles kind resolution for new and old sessions; legacy sessions (NULL) are inferred via a compatibility branch
  - Added `UploadRuntime`, applying a global concurrency limit to uploads concurrently assembling chunks into local temporary files

- **Remote storage list cursor pagination**
  - List operations support cursor-based pagination with a max of 1000 items per page and response bodies limited to 8MB
  - `RemoteStorageListResponse` adds a `next_cursor` field; clients track the cursor automatically
  - Without a `limit` parameter, the legacy unpaginated response is preserved for compatibility with existing clients

- **Development workflow and docs publishing**
  - Added a Makefile covering the full development workflow: setup / dev / test / build / docs, etc.
  - The docs site supports multi-version publishing; the vitepress workflow automatically creates release/x.y branches and builds docs for each version, with a version banner shown on old docs
  - Added a docs-check workflow validating error-code doc synchronization and docs builds

### Changed

- **Upload type system refactor** — added the `UploadSessionKind` enum; each data plane is explicitly modeled instead of inferred from provider fields
- **WebDAV recursive operations made iterative** — recursive COPY / MOVE now processed via an iterative work queue; added `extend_unique_failures()` to deduplicate lock-conflict reports
- **Remote protocol shared function extraction** — extracted shared response-reading functions such as `read_reqwest_response_body_limited()`; `append_query_pairs()` generalized to `AsRef<str>` to reduce allocations
- **Remote protocol error type adjustment** — response-body-over-limit errors changed from `Transient` to `Misconfigured`
- **Dependency updates** — `aws-sdk-s3` 1.138.0 → 1.138.1; `aster_forge_utils` upgraded to a version containing XML security utilities

### Fixed

- **Storage quota deadlock** — quota rows are locked in order before file completion and version restore / deletion, eliminating the deadlock when InnoDB shared locks upgrade to exclusive locks
- **WebDAV recursive COPY / MOVE edge behavior** — fixed lock-conflict handling and destination-overwrite logic in recursive operations
- **Remote list client cursor tracking** — fixed automatic cursor-advance logic and verified that cursor advancement prevents infinite loops

### Security

- **WebDAV security fix (GHSA-7797-6gjx-hwgh, CVE pending)** — this version fixes a process-level denial-of-service issue triggerable by an authenticated WebDAV user; all instances with WebDAV enabled are advised to upgrade as soon as possible; technical details will be disclosed once the security advisory is published
- **Remote protocol response limits** — control-plane response bodies limited to 1MB, paginated list responses limited to 8MB, defending against memory exhaustion from malicious remote responses
- **Improved process fault tolerance** — release / profiling profile panic strategy changes from `abort` to `unwind`; single-request / task panics no longer terminate the whole process, and Actix workers do not affect each other

### Database Migrations

- `m20260717_000001_add_upload_session_kind`
  - Added `session_kind` column (string(32), nullable) to `upload_sessions`, persisting each upload session's data-plane type
  - For existing sessions this column is NULL, with the kind inferred via a compatibility branch (the compatibility branch is planned for removal in `0.5.0`)

### Statistics

- 144 files changed, 9438 insertions(+), 3223 deletions(-)
- 6 commits
- 1 database migration

### Notes

- This version contains a WebDAV security fix (GHSA-7797-6gjx-hwgh); all instances with WebDAV enabled are advised to upgrade as soon as possible; WebDAV can be temporarily disabled in the admin panel before upgrading
- 1 database migration runs automatically at startup
- Existing upload sessions infer their data plane via the `session_kind` compatibility branch; the compatibility branch is planned for removal in `0.5.0`
- Remote storage listing preserves the legacy unpaginated response when no `limit` parameter is provided; custom remote storage clients are advised to migrate to paginated mode gradually

## [v0.4.0-beta.2] - 2026-07-16

### Release Highlights

**AsterDrive `v0.4.0-beta.2` is the second beta of the `0.4.0` series, focused on security hardening of the authentication and access chain, plus polish of the public share navigation experience.** External auth (OAuth2 / OIDC) login flows bind to the browser session (HttpOnly cookie + SHA-256 binding hash), blocking CSRF and session fixation at the protocol level; remote node registration tokens are tightened to one-time atomic redemption, so replays no longer leak bootstrap credentials; WebDAV Basic auth gains layered rate limiting (per-IP + username backoff) and a unified error boundary, no longer exposing failure details such as credentials, ownership, disablement, or team access; on first startup, regular registration is blocked until system initialization completes, with a database initialization lock guaranteeing a single initial admin. Meanwhile, public shares gain routed subfolder navigation, a sidebar directory tree, and logged-in session detection; workspace routes are merged into a single component that preserves the current view when switching workspaces.

- **External auth browser binding** — OAuth2 / OIDC login flows bind an HttpOnly cookie, verified with SHA-256; unbound legacy flows rejected
- **One-time registration token redemption** — remote node registration tokens claimed atomically; replays rejected, bootstrap credentials not leaked
- **WebDAV auth rate limiting and credential boundary** — Basic auth layered rate limiting (per-IP + username backoff) + unified 401, hiding failure details
- **Setup required before registration** — regular registration blocked until system initialization; a database lock guarantees a unique initial admin
- **Public share navigation** — routed subfolder navigation, sidebar directory tree, logged-in session detection on public pages
- **Unified workspace routing** — merged personal / team routes; switching workspaces preserves the current view

### Added

- **External auth browser binding (security)**
  - `external_auth_login_flows` adds a `browser_binding_hash` column (SHA-256 of a 32-byte random secret)
  - OAuth / OIDC flows generate a secret at initiation, store its hash, and deliver the plaintext via an HttpOnly cookie
  - Before issuing a session token, verify that the binding cookie matches the flow; the cookie is cleared on both success and failure
  - Reject legacy unbound flows where `browser_binding_hash` is NULL
  - Cover binding validation, tampering detection, and parallel-flow tests

- **WebDAV auth rate limiting and credential boundary**
  - Basic auth applies the auth-tier rate limit; added per-IP rate limiting (client IP resolution trusting the proxy) and username failure backoff across rotating IPs
  - When rate limited, return a WebDAV-compatible 401 with `Retry-After`
  - WebDAV account connection tests are guarded by actor ownership checks and reuse the auth rate limit tier
  - Credential / ownership / disabled / team access failures uniformly converge to an auth error without exposing differences

- **Mandatory first-launch setup**
  - Normal registration is forbidden until system initialization completes; added the `validation.system_not_initialized` error code and frontend copy
  - A database initialization lock ensures concurrent setup creates only one initial admin, and setup retries roll back cleanly

- **One-time redemption of remote node registration tokens**
  - Redeemable tokens are atomically claimed by token hash; replays are rejected without exposing bootstrap credentials
  - Retained replaced / completed / expired / redeemed terminal-state errors; a redeemed token is treated as already configured during node bootstrap

- **External auth provider creation defaults moved to the backend**
  - The provider kind schema adds `create_defaults` (display_name / options / scopes / enabled / auto_provision / auto_link_verified_email / require_email_verified) and Microsoft tenant defaults
  - Added the `issuer_url_supported` boolean flag, replacing frontend logic that inferred issuer field visibility from multiple flags
  - When switching providers, the frontend rebuilds the form from backend defaults, removing frontend provider-specific branches like isGitHub / Google / Microsoft / Qq

- **Routed navigation for public shares**
  - Added canonical public share subfolder routes with route validation; subfolder contents, breadcrumbs, sorting, and refresh load by route; added a scoped ancestors query endpoint
  - Public folder shares add a sidebar directory tree (lazy loading, breadcrumb hydration, request dedupe, canceling stale responses on token change, branch-level retry)
  - Public pages perform an optional session probe (`/auth/me`) for logged-in users without forcing a login redirect
  - The `ShareAccess` extractor (const generic flags) replaces manual cookie checks, converging download-count and archive-download feature gating

- **Unified workspace routing**
  - `PersonalWorkspaceRoute` and `TeamWorkspaceRoute` are merged into a single `WorkspaceRoute` sharing the same route element, so workspace switches no longer unmount the page
  - Added `workspaceSwitchPath`, which preserves query and hash of workspace-agnostic views (search / shares / tasks / trash / categories) across workspace switches; team id validation tightened to a strict integer regex

- **PDF virtual scroll accuracy**
  - All PDF page sizes are preloaded concurrently (up to 8 workers) before exposing the virtual scrollbar, eliminating layout jitter caused by equal-height estimation
  - Each page's initial size uses the real aspect ratio; measured heights are preserved across page navigation and refreshed only on rotation

### Changed

- **Admin daily report counts SystemSetup** — the `SystemSetup` audit action is included in the admin daily report new_users metric
- **Test account creation helpers extracted** — `setup_test_account_via_api` / `create_test_account_via_api` / `create_test_account_at_api_endpoint` are reused, replacing duplicated registration boilerplate across test files
- **Internal: frontend toolchain migration to TypeScript 7** — tsgo / native-preview scripts switched to incremental TypeScript 7 tsc checks, added tsconfig incremental build info, refreshed frontend dependencies and Biome schema (toolchain only, no behavior change)
- **Internal: CI / build maintenance** — SeaORM migration and locked dependency upgrades, Rust toolchain components, CI split into format / clippy jobs, added OpenAPI and generated SDK drift checks, tightened cargo audit triggers and treat warnings as failures, support explicit `ASTER_BUILD_TIME` and isolated frontend fallback embed (maintenance only)

### Fixed

- **Music playback position lost on source refresh** — replaying the same prepared source no longer reloads the audio; the loaded source key is recorded to avoid redundant loads; current playback time is restored from metadata after refresh
- **Video preview cannot fill fullscreen** — fullscreen expansion state is passed into the video preview, added a reusable video frame, removed the aspect-ratio constraint, and kept native fallback sizing
- **Absolute HTTP resource URLs misclassified** — absolute HTTP(S) URLs are treated as browser-addressable resources, and video preview initialization preserves absolute stream session URLs
- **Public share navigation state jitter** — preserve the breadcrumb source index of the compact dropdown drag target; directory tree expansion state moved to the toggle button; cancel stale load-more on refresh / sort; ignore stale navigation and file share responses; the public auth probe stays logged out when offline

### Security

- **External auth CSRF / session fixation protection** — OAuth2 / OIDC login flows bind to the browser session (HttpOnly cookie + SHA-256 hash), rejecting unbound legacy flows
- **Remote node registration token replay** — atomic claim enforces one-time redemption; replays do not leak bootstrap credentials
- **Converged WebDAV auth probing surface** — layered rate limiting + unified auth error boundary hides credential / ownership / disabled / team access failure differences
- **Unauthorized first registration** — normal registration is forbidden before system initialization completes; a database lock guarantees a unique initial admin

### Database Migrations

- `m20260716_000001_bind_external_auth_login_flows`
  - Added the `browser_binding_hash` column to `external_auth_login_flows`, backing external auth browser binding
  - Existing unbound flows (NULL hash) are rejected at login and must be re-initiated

### Statistics

- 179 files changed, 10296 insertions(+), 2947 deletions(-)
- 14 commits
- 1 database migration

### Notes

- This version is the second beta of the `0.4.0` series; it is recommended to first verify external auth login (OAuth2 / OIDC), WebDAV Basic auth rate limiting, remote node registration, and the first-launch setup flow in a test environment
- 1 database migration runs automatically at startup; in-progress legacy unbound flows will be rejected and must re-initiate login
- After upgrading, clients (browsers) must support cookies, as external auth login relies on the browser binding cookie to complete validation
- WebDAV clients receive a 401 with `Retry-After` after repeated failures and should honor the backoff instead of retrying immediately
- In multi-instance deployments, one-time registration token redemption relies on the same authoritative database

## [v0.4.0-beta.1] - 2026-07-14

### Release Highlights

**AsterDrive `v0.4.0-beta.1` is the first beta of the `0.4.0` series. The main theme is migrating reusable infrastructure into AsterForge shared crates and completing the configuration sync, runtime leases, and task scheduling infrastructure required for multi-instance operation.** Common capabilities such as API, database, cache, audit, mail, external auth, validation, general utilities, Actix middleware, metrics, and the task runtime are no longer maintained as parallel implementations inside AsterDrive; the product layer keeps AsterDrive domain orchestration for files, workspaces, sharing, uploads, storage policies, remote nodes, WebDAV, and WOPI. In addition, this version adds Redis pub/sub-based runtime configuration sync across instances, lands runtime leases, scheduled tasks, and a background task dedupe contract, and completes the removal of legacy ingress profile API compatibility routes planned for `0.4.0`. The CORS allowlist becomes a string array and supports Chrome / Edge, Firefox, and Safari Web Extension origins.

- **AsterForge shared capability migration** — API contract, database, cache, config, audit, mail, external auth, validation, utilities, crypto, metrics, runtime, tasks, and Actix middleware uniformly use `aster_forge_*` crates, deleting AsterDrive's internal duplicate implementations
- **Cross-instance runtime config sync** — added `[config_sync]`, which can notify other instances via Redis pub/sub to reload runtime config from the authoritative database; disabled by default for single instances
- **Multi-instance task runtime foundation** — added runtime leases, scheduled tasks, and background task dedupe keys; runtime components start up and shut down gracefully through an explicit dependency graph
- **Extended CORS origin support** — `cors_allowed_origins` becomes a string array supporting HTTP(S) and browser extension origins; legacy comma-separated config is migrated at startup
- **Legacy remote storage compatibility routes removed** — the admin API uniformly uses `/storage-targets` / `/storage-target-drivers`, and the follower internal protocol uniformly uses `/targets`

### Added

- **Cross-instance runtime config sync**
  - Added the static config group `[config_sync]` with `disabled` and `redis` backends
  - The admin API and `aster_drive config set` / `delete` / `import` publish a reload notification after the database write succeeds
  - Other instances reload the full runtime config from the writer database upon notification; Redis only delivers notifications and stores no config values
  - Added config reload / mutation metrics and subscription worker logs
  - primary, follower, and test runtime states uniformly adopt `ConfigSyncRuntime`
  - Added multilingual (Chinese/English) multi-instance config sync docs and production checklist items

- **Multi-instance runtime and task scheduling foundation**
  - Added the `runtime_leases` table for lease coordination of multi-instance runtime capabilities
  - Added the `scheduled_tasks` table with name / next-run indexes
  - `background_tasks` gains an optional `dedupe_key` and unique index, supporting task dedupe queries
  - Task execution context gains an explicit lease renewal timeout
  - The runtime component graph makes shutdown dependencies explicit for HTTP, background task, mail outbox, audit, database, and other components

- **Browser extension CORS origins**
  - Supports `chrome-extension://`, `moz-extension://`, and `safari-web-extension://`
  - The admin console edits allowed origins one-by-one as an array
  - Added validation for full origin, wildcard, and credential combinations
  - Added tests for legacy comma-separated value migration, browser extension origins, and the frontend form

- **Build fallback assets**
  - Automatically detects and refreshes legacy fallback directories containing the `Frontend Not Built` marker
  - Fallback output now includes CSS, the service worker, and the web manifest
  - Fallback page structure matches the normal frontend build output

### Changed

- **Core infrastructure migrated to AsterForge**
  - The local `api-docs-macros` workspace crate migrated to `aster_forge_api_docs_macros`
  - allocator, cache, logging, panic, metrics, Actix observability, and middleware use Forge implementations
  - `NullablePatch`, pagination structures, and the public API error bridge use `aster_forge_api`
  - `DbHandles`, transaction, retry, pagination, sort, search query, and index migration helpers use `aster_forge_db`
  - audit log, system config, and mail outbox use the Forge shared database contract and runtime component
  - the external OAuth2 / OIDC driver registry uses `aster_forge_external_auth`
  - filename, email, display text, URL, path, number, ID, HTTP validators, hash, and crypto helpers use Forge shared implementations
  - task execution, lease, step, retry, lane, and registry contracts use `aster_forge_tasks`
  - Removed the corresponding thin product-side wrappers, duplicate types, and forwarding facades

- **Componentized runtime lifecycle**
  - primary / follower startup assembles runtime capabilities using the `AsterRuntime` component graph
  - audit switched to the Forge buffered batch writer
  - Shutdown ordering of mail outbox, background task, audit, and database is expressed through dependencies
  - The config loader returns a structured `ConfigLoadReport` emitted uniformly by the startup entry point, avoiding unstructured stderr mixing into JSON mode

- **CORS config becomes an array**
  - The runtime config type of `cors_allowed_origins` changes from `string` to `string_array`
  - The admin API and frontend read and write it as a JSON string array
  - Legacy comma-separated origin lists are normalized to a JSON array at startup
  - `["*"]` allows any origin but still cannot be combined with cross-origin credentials

- **Unified cache config fields**
  - `[cache].redis_url` renamed to `[cache].endpoint`
  - Config examples, user docs, and tests uniformly use `endpoint`

- **Database migration infrastructure**
  - The migration crate uses the cross-database index helpers provided by `aster_forge_db`
  - Runtime lease and scheduled task tables are added to the database replication order
  - Fixed a MySQL-incompatible string default in the system config migration rollback
  - Schema drift tests cover the runtime lease and scheduled task tables

- **System health and task error representation**
  - The admin health component adds structured `details`
  - Remote node health counters changed from `usize` to `u64`
  - Forge task core errors map to stable product API error codes instead of uniformly degrading to internal errors

- **Dependency upgrades**
  - `aes-gcm` 0.10 → 0.11
  - Upgraded AWS SDK, `azure_core`, `rand`, `russh`, `sea-orm`, `criterion`, `tokio-tungstenite`, and other dependencies
  - Test RSA key generation migrated to `rsa 0.10.0-rc` and `rand 0.10`

- **Docs and terminology sync**
  - "ingress target" unified as "remote storage target"
  - External auth development docs updated to the AsterForge shared driver / registry boundary
  - Added SFTP to the storage capability list
  - Internal protocol docs clarify the current version v5 and minimum compatible version v4
  - `/public/frontend-config` documented as the current endpoint for public branding and login entry config

### Removed

- Removed the deprecated remote storage ingress profile compatibility routes from the admin API:
  - `GET /api/v1/admin/remote-nodes/{id}/ingress-profile-drivers`
  - `GET /api/v1/admin/remote-nodes/{id}/ingress-profiles`
  - `POST /api/v1/admin/remote-nodes/{id}/ingress-profiles`
  - `PATCH /api/v1/admin/remote-nodes/{id}/ingress-profiles/{target_key}`
  - `DELETE /api/v1/admin/remote-nodes/{id}/ingress-profiles/{target_key}`
- Removed the `/ingress-profiles` route alias from the follower internal storage protocol, unifying on `/targets`
- Removed the corresponding OpenAPI operations and generated frontend API client methods
- Removed AsterDrive's internally duplicated cache, middleware, external-auth driver, mail sender, config contract, database helper, general utils, allocator, and API docs macro implementations

### Fixed

- Legacy fallback frontend assets can be detected and regenerated by the build script instead of continuing to use an incomplete fallback directory
- The CORS wildcard-plus-credential combination is validated on both the admin frontend and backend config normalization sides
- First-time config file generation no longer mixes unstructured config reports into JSON log mode
- Migration index rename / drop helpers stay idempotent on MySQL
- The database migration replication flow includes `runtime_leases` and `scheduled_tasks`
- The system config migration rollback no longer produces MySQL-incompatible `DEFAULT ""`

### Database Migrations

- `m20260712_000001_align_forge_audit_contract`
  - Align existing `audit_logs` with the Forge contract, preserving existing audit data
  - Extend the IP address field and fill in system user default semantics
- `m20260712_000002_add_forge_audit_query_indexes`
  - Add Forge audit query indexes
- `m20260712_000003_align_forge_system_config_contract`
  - Align `system_config` with the Forge contract while preserving existing config
- `m20260712_000004_align_forge_mail_outbox_contract`
  - Align `mail_outbox` with the Forge contract while preserving existing mail records
- `m20260713_000001_runtime_leases`
  - Add `runtime_leases`
- `m20260713_000002_background_task_dedupe_key`
  - Add `dedupe_key` and a unique index to `background_tasks`
- `m20260713_000003_scheduled_tasks`
  - Add `scheduled_tasks` with scheduling indexes

### Configuration Changes

- Added `[config_sync]`:
  - `backend = "disabled"`: the default, suitable for single-instance deployments
  - `backend = "redis"`: syncs runtime config reload notifications via Redis pub/sub
  - `endpoint = ""`：Redis URL
  - `topic = "aster_drive.config_reload"`: must be kept consistent within the same instance group
- `[cache].redis_url` renamed to `[cache].endpoint`
- `cors_allowed_origins` changed from a comma-separated string to a JSON string array; existing legacy values are migrated automatically at startup
- The `server.follower.managed_ingress_local_root` config alias remains for compatibility; new config should still use `server.follower.remote_storage_target_local_root`

### Statistics

- 582 files changed, 12840 insertions(+), 23524 deletions(-)
- 22 commits
- 7 database migrations
- Static config groups added: 1 (`config_sync`)
- Removed legacy remote storage compatibility APIs: 5 admin routes and 4 follower internal protocol routes

### Notes

- This version is the first beta of the `0.4.0` series; it is recommended to first verify database migrations, external auth, mail sending, background tasks, and remote node paths in a test environment
- 7 database migrations run automatically at startup; the audit, system config, and mail outbox migrations preserve existing data
- Single-instance deployments need no Redis; just keep `[config_sync].backend = "disabled"`
- Multi-instance deployments enabling config sync must have all instances connected to the same authoritative database, the same Redis service, and use the same topic
- Redis pub/sub does not replay messages missed while offline; instances load the full config from the database at startup
- Clients using the legacy `/ingress-profiles` admin API or internal storage protocol alias must migrate to `/storage-targets`, `/storage-target-drivers`, and `/targets`
- The `managed_ingress.*` error codes keep their historical prefix for compatibility with existing clients and logs
- The current remote internal protocol is v5, and the minimum compatible version remains v4

## [v0.3.2] - 2026-07-08

### Release Highlights

**AsterDrive `v0.3.2` is a quick hotfix release after `0.3.1`, focused on initializing the frontend public authentication routes and converging token refresh behavior.** On public pages such as invite registration, password reset, email verification, and OIDC callback, the app no longer issues pointless bootstrap auth checks at startup, and 401 errors no longer trigger token refresh attempts, reducing invalid API calls and avoiding incorrectly redirecting public flows to the login page; it also completes i18n namespace loading for multiple authentication-related pages, the trash page, and error pages, adds the tasks namespace to the share view page, and introduces corresponding test coverage.

- **Public auth routes skip initialization checks** — `/invite/:token` and `/reset-password` no longer trigger the bootstrap auth check
- **Public endpoints skip token refresh** — password reset, email verification, OIDC callback, and invite-related endpoints no longer attempt token refresh on 401
- **URL matching compares pathname only** — fixes an issue where query/fragment prevented public endpoints from being recognized
- **i18n namespace completion** — force password change, reset password, invite registration, team details, trash, and error pages load the correct namespaces; share view adds tasks

### Fixed

- Frontend public auth pages (invite registration, password reset) no longer run the bootstrap auth check at startup
- Public auth endpoints no longer attempt token refresh on 401 errors
- token refresh skip matching now uses pathname, avoiding query/fragment interference
- Completed i18n namespace loading for ForcePasswordChangePage / ResetPasswordPage / InviteRegisterPage / AdminTeamDetailPage / TrashPage / ErrorPage
- ShareViewPage adds the tasks namespace

### Statistics

- 6 files changed, 133 insertions(+), 13 deletions(-)
- 1 commit

## [v0.3.1] - 2026-07-08

### Release Highlights

**AsterDrive `v0.3.1` is an architecture-cleanup and performance-polish release in the `0.3.0` series, focused on reorganizing the service layer into domain modules, improving WebDAV performance and HTTP conditional-request compliance, and contract-hardening the upload finalize path.** The service layer was reorganized from flat `*_service` files into a domain-nested module tree (`auth` / `files` / `share` / `user` / `mail` / `remote` / `workspace` / `preview` / `webdav` / `media`, etc.), a pure path migration with no API or schema changes; WebDAV implements RFC 9110 conditional requests (`If-Match` / `If-None-Match` / `If-Modified-Since` / `If-Unmodified-Since`) and `Last-Modified` responses, range reads and large PROPFINDs are significantly faster, and read/write DB separation fixes read-after-write consistency; the upload finalize path introduces immutable value objects such as `VerifiedUploadedBlob`, moving size / policy / path validation and cleanup plans into the type layer. Alongside this, "managed ingress profile" was renamed to the clearer "remote storage target", and remote storage policies can be bound to a specific target; this release also merges a contributor-submitted SFTP storage backend.

- **Service layer reorganized into domain modules** — flat `*_service` files nested into a domain module tree, pure path migration, no API / schema changes
- **WebDAV performance and compliance** — RFC 9110 conditional requests and `Last-Modified` responses, significantly faster range reads and large PROPFINDs, read/write DB separation fixes read-after-write consistency, download audit window coalescing
- **Contract-hardened upload finalize path** — immutable value objects such as `VerifiedUploadedBlob` / `VerifiedTempStoreBlob` converge validation and cleanup plans, locking in the invariants of idempotent complete retries and no quota charge on quota overflow
- **Remote storage target-ization** — `managed ingress profile` renamed to `remote storage target`, remote policies bind to a specific target, old API paths and config keys kept for compatibility (deprecated)
- **SFTP storage backend** — based on `russh`, with connection pooling, enforced host key pinning (TOFU), and efficient range reads; server-relay-only uploads, no presigned / multipart support

### Added

- **Remote node driver descriptor discovery**
  - Added `ManagedIngressDriverDescriptor` and field descriptors (Text / Secret / Boolean / Number + required / secret / label_key / placeholder / help_key), with built-in descriptors for the local + S3 drivers
  - New API pulls supported driver descriptors from the remote node; the admin console dynamically renders fields per current driver, shows driver-specific help / placeholder, and reports a validation error when selecting a driver the remote does not support
  - Remote internal protocol version v4 → v5; `RemoteStorageCapabilities` adds a `managed_ingress` capability field; the v4 compatibility layer is retained (`MIN_SUPPORTED_PROTOCOL_VERSION` remains v4)

- **Remote storage policies bound to a target**
  - The `storage_policies` table adds a `remote_storage_target_key` column and index; policy creation / update validates that the target is applied and error-free, and non-remote policies reject a target key
  - Internal / presigned requests route to the policy-specified target instead of always going through the binding default
  - The admin console policy dialog adds a secondary target selector (loading / empty / error / hint), supporting quick target creation directly within the policy form

- **Download disposition parameter + resource-handle endpoints**
  - `DownloadQuery` adds an optional `disposition` (`inline` / `attachment`), with a validation error for invalid values; threaded through the personal / team / share download chains (empty value = legacy attachment behavior, backward compatible)
  - New endpoint `POST /api/v1/files/{id}/resource-handle` and the team counterpart, returning `FileResourceHandle`; in `Auto` mode, non-renderable images (HEIC / RAW / TIFF) get a derived WebP representation selected, and sandboxed MIME types like HTML are forced same-origin

- **WebDAV HTTP conditional-request compliance**
  - Added `src/utils/http_validators.rs` implementing RFC 9110 evaluation of `If-Match` (strong ETag) / `If-None-Match` (weak ETag) / `If-Modified-Since` / `If-Unmodified-Since` with header precedence; GET / HEAD responses carry `Last-Modified`

- **WebDAV download audit coalescing**
  - New config option `webdav_download_audit_coalesce_window_secs` (default 30s); repeated downloads within the window for the same account + file + request type (full / ranged) + client fingerprint (SHA256 of IP + User-Agent) are coalesced into a single audit record; setting it to 0 records every download

- **SFTP storage backend**
  - Added `DriverType::Sftp` (persisted string `"sftp"`, stored as a string by sea-orm, no migration needed for existing databases), based on `russh 0.62.1` + `russh-sftp 2.3.0`
  - Connection pool (default 4, idle TTL 60s, acquire timeout 30s, RAII lease return), enforced host key pinning (TOFU; without a configured fingerprint the connection is refused and the server's actual `SHA256:` fingerprint is returned), efficient range reads (SFTP seek + take)
  - Uploads stream via server relay, **no** presigned / multipart / browser direct upload support — capability boundaries differ from S3

### Changed

- **Service layer reorganized into domain modules** (purely internal, no API / schema changes)
  - Flat `*_service` files under `src/services/` → domain-nested module tree; the vast majority is `use` path migration
  - Added `developer-docs/{en,zh-CN}/backend-service-ownership.md`, a service-layer responsibility boundary document

- **Terminology unification: managed ingress profile → remote storage target**
  - API endpoint `/ingress-profiles` → `/storage-targets`; the old path remains as a deprecated alias (planned for removal in `0.4.0`)
  - Config key `server.follower.managed_ingress_local_root` → `server.follower.remote_storage_target_local_root`; the loader accepts both old and new keys; default value `managed-ingress` → `remote-storage-targets`
  - Retained compatibility items: API error codes remain `managed_ingress.*`; audit log entity type / action remains `RemoteIngressProfile`; wire protocol capabilities still use the `managed_ingress` field name

- **`max_file_size` moved up from the target layer to the policy layer**
  - Removed the `remote_storage_targets.max_file_size` column; instead read from the `max_file_size` signed query parameter of the request (value sourced from the storage policy), with negative values clamped to 0
  - The target creation form removes max_file_size, and target cards no longer show revision

- **WebDAV read/write DB separation and finer-grained caching**
  - `resolve_path_cached_in_scope` split into read / write variants: write operations (PUT / MKCOL / COPY / MOVE / DELETE / LOCK) use `writer_db()`, read operations use `reader_db()`, avoiding stale cache hits after writes
  - Introduced `CacheInvalidationTargets` supporting prefix + targeted key deletion; folder changes now delete targeted keys instead of flushing the whole prefix; PROPFIND skips dead-property and lock discovery loading when only standard live properties are requested

- **WebDAV range-read and PROPFIND path optimizations**
  - The download path resolves file + blob + meta in one pass, eliminating duplicate metadata queries; streaming chunk size 16 KiB → 64 KiB
  - `read_dir` N+1 eliminated: directory entries constructed directly from `file` records; ETag now uses the structured `file_etag`, no longer depending on blob hash
  - WebDAV auth cache invalidation triggered by team member changes is now fire-and-forget (with retry + logging), no longer blocking member operations

- **Contract-hardened upload finalize path** (purely internal, no API / behavior changes)
  - Added the immutable value objects `VerifiedUploadedBlob` / `VerifiedTempStoreBlob` / `VerifiedPreuploadedNondedupStoreBlob`, moving size / policy / path validation and cleanup plans into the type layer
  - Added regression tests locking in the invariants: complete retries don't double-charge quota; on quota overflow the session is set to Failed without charging quota (error code `E032`)

- **Frontend admin / file actions made plugin-extensible**
  - Added four registries for admin settings actions / editors / invalidation / save-transactions; file actions via `fileActionRegistry` (declarative action descriptors, supporting builtin + plugin actions)
  - `AdminPoliciesPage` reduced from 1595 lines to 235 (extracting 5 controller hooks + 3 utility modules); 12 controllers migrated to the generic `useManagedListQueryState` / `useManagedAdminList`, removing about 800 lines of boilerplate

- **S3 remote target editing enforces access_key input**
  - Added `src/storage/field_contract.rs` to unify required / secret / boolean field semantics; editing an S3 remote target always requires entering `access_key`, while `secret_key` keeps its original value when unchanged
  - The frontend moves from hardcoding `driver_type === "s3"` to checking the descriptor's `secretKeyField?.secret === true`

### Fixed

- **WebDAV read-after-write consistency** — write operations resolve authoritatively via `writer_db()`; added tests covering immediate GET / PROPFIND after PUT / COPY / MOVE / LOCK / DELETE
- **Music playback interrupted by metadata refresh** — track metadata is keyed by resource path rather than the whole object; cross-origin redirected resources skip the range probe
- **Stale WebDAV auth cache after team archive / restore** — invalidates the WebDAV auth cache when archiving / restoring a team
- **k6 benchmark token reuse** — `refreshSession` kept reusing the old refresh token; `mixed-ramp.js` / `soak-mixed.js` changed to have each VU do its own `login()`

### Security

- **SFTP enforced host key pinning** — tightened from blindly trusting any host key to TOFU + pin, preventing MITM; without a configured fingerprint the connection is refused and the server's actual `SHA256:` fingerprint is returned to guide admin confirmation
- **S3 remote target editing enforces access_key** — public credentials must be entered every time; only `secret_key` keeps its original value when unchanged
- **Negative `max_file_size` rejected** — `normalize_storage_policy_max_file_size` rejects negative values
- **Remote target relative path escape detection** — `normalize_relative_local_target_path` detects path-escape segments

### Database Migrations

- `m20260704_000001_rename_managed_ingress_profiles_to_remote_storage_targets` (table / column rename, reversible)
- `m20260704_000002_add_remote_storage_target_key_to_storage_policies` (add column + index)
- `m20260705_000001_drop_remote_storage_target_max_file_size` (drop the `max_file_size` column, switched to a policy query parameter)

### Configuration Changes

- `server.follower.managed_ingress_local_root` → `server.follower.remote_storage_target_local_root` (loader accepts both old and new keys; the old key is deprecated, planned for removal in `0.4.0`)
- Default value `managed-ingress` → `remote-storage-targets`
- Added `webdav_download_audit_coalesce_window_secs` (default 30s; set to 0 to record an audit entry for every download)

### Statistics

- 916 files changed, 45826 insertions(+), 17459 deletions(-)
- 30 commits (including several `[skip ci]` repository migrations / test image pinning)
- 3 database migrations (run automatically, reversible)
- New backend drivers: 1 (SFTP)

### Notes

- Upgrading from `v0.3.0`: the 3 DB migrations run automatically at startup; the old API path `/ingress-profiles` and the old config key are kept for compatibility (deprecated) — migrating to `/storage-targets` / `remote_storage_target_local_root` as soon as possible is recommended
- First-time SFTP policy setup requires the host key TOFU confirmation flow: without `sftp_host_key_fingerprint` configured, the connection test returns the server's actual `SHA256:` fingerprint; confirm it and write it in
- The SFTP backend is pure server-side relay, with no presigned / multipart / browser direct upload support — capability boundaries differ from S3
- Remote internal protocol v4 → v5: `MIN_SUPPORTED_PROTOCOL_VERSION` remains v4, old followers keep working via the compatibility layer; upgrading primary + follower mixed deployments together is recommended
- Docker users are advised to use the `v0.3.1` / `stable` / `latest` image tags

## [v0.3.0] - 2026-06-23

### Release Highlights

**AsterDrive `0.3.0` is officially released.** Building on the `v0.3.0` beta / RC line (Azure Blob and OneDrive drivers, the unified `StorageConnector` abstraction, object-storage terminology unification and credential masking, inline previews via presigned direct links, indefinite audit log retention, etc.), this release closes out public share preview cache stability and the CI build pipeline, advancing the `0.3.0` series from RC to stable release.

- **Stable preview resource cache identity** — public share preview responses return a canonical ETag (blob hash); the frontend reuses blob / text caches keyed by stable identity, sharing the same resource across expired preview tokens
- **Conditional requests no longer trigger CORS preflight** — `If-None-Match` conditional requests stay same-origin, no longer 302-redirecting to presigned object storage URLs
- **CI frontend build made independent** — GitHub Actions splits the frontend build into a standalone `build-frontend` job; downstream build / integration-backends download the artifact, avoiding duplicate builds and Rust cache pollution
- **Dependency upgrades** — `react-image-crop` 11.0.10 → 11.1.2, `@typescript/native-preview` upgraded to `7.0.0-dev.20260622.1`

### Added

- **Stable identity type for preview resources**
  - The frontend adds a `ResourceRequest` type distinguishing `cacheKey` / `etag` / `requestPath`, plus `resourceCacheKey` / `resourceRequestPath` / `resourceCanonicalEtag` helpers for centralized handling
  - `useBlobUrl` / `useTextContent` cache by stable identity and skip the `If-None-Match` conditional request when a canonical ETag is available
  - Preview components (`BlobImagePreview` / `PdfPreview` / `CsvTablePreview` / `JsonPreview` / `MarkdownPreview` / `TextCodePreview` / `XmlPreview` / `FilePreviewBody` / `ImagePreviewPanel`) accept `ResourcePath` (string | `ResourceRequest`)
  - `PreviewLinkInfo` adds a canonical etag field (blob hash); public share preview responses carry the stable identity
  - Added unit tests for `resourceRequest` / `useBlobUrl` / `useTextContent` covering cache key fallback and conditional revalidation logic

### Changed

- **Conditional requests converged to same-origin**
  - `file_service::download::build` rejects conditional requests (`If-None-Match`) before a presigned redirect, preventing browsers from carrying conditions cross-origin and triggering CORS preflight
  - `preview_link_service` carries a canonical ETag in the response for the frontend to reuse caches

- **CI frontend build as a standalone job**
  - `rust.yml` adds a `build-frontend` job that builds the frontend separately and uploads an artifact; `build` / `integration-backends` now use `needs: build-frontend` and pull `frontend-panel/dist` via `download-artifact`
  - Removed the previously scattered `Setup bun` and `Build frontend` steps from `build` / `integration-backends`, preventing the Rust cache from being polluted by frontend artifacts

- **Unified path type for preview components**
  - Frontend preview components move from inline type definitions to importing the shared `ResourcePath` type; mock helpers in tests converge to the `resourceCacheKey` / `resourceCanonicalEtag` utilities

- **Dependency upgrades**
  - `react-image-crop` 11.0.10 → 11.1.2
  - `@typescript/native-preview` upgraded to `7.0.0-dev.20260622.1`

### Fixed

- `OverviewRecentEventsSection` table columns now use fixed widths + `max-w-0` truncation, fixing column jitter when content is too long

### Notes

- This is the official stable release of the `0.3.0` series
- Upgrading from `v0.3.0-rc.2` to `v0.3.0` adds no new database migrations
- No new required items were added to the production config schema
- Docker users are advised to use the `v0.3.0`, `stable`, or `latest` image tags; `edge` remains reserved for future pre-release versions
- Statistics: 31 files changed, 923 insertions(+), 174 deletions(-)
- This scope covers 2 commits

## [v0.3.0-rc.2] - 2026-06-22

### Release Highlights

**AsterDrive `0.3.0-rc.2` is a preview-experience and audit-retention reinforcement release on the 0.3.0 release-candidate line, with the main themes being inline previews served via object-storage presigned direct links and indefinite audit log retention.** Inline previews (images / videos / audio / PDF / Markdown / code / tables / JSON / XML, etc.) now issue a direct 302 to the object-storage presigned URL when the storage policy allows it and the MIME type does not require a same-origin CSP sandbox, instead of the server adding CSP and cache headers and relaying the stream, reducing server bandwidth and forwarding latency; the public share preview (preview link) download path switches to the unified download endpoint to benefit from presigned URLs as well. The frontend `apiUrl` excludes URLs on the backend API origin from "external resources" and adds `shouldSendResourceCredentials` — presigned URLs on the object-storage origin no longer carry session credentials, avoiding CORS preflight failures and credential leaks; each preview component's `path` is now nullable, showing a loading placeholder while loading. Setting the audit log retention period `audit_log_retention_days` to `0` skips automatic cleanup, achieving permanent retention.

- **Inline previews via presigned direct links** — When the storage policy allows it and the MIME type does not require a same-origin sandbox, inline previews issue a direct 302 to the object-storage presigned URL; preview links switch to the unified download endpoint in sync
- **CORS-safe credential handling** — Backend API origin URLs are no longer misclassified as external resources by the frontend; the new `shouldSendResourceCredentials` centralizes the decision; presigned URLs on the object-storage origin carry no session credentials
- **Indefinite audit log retention** — `audit_log_retention_days = 0` skips automatic cleanup; i18n copy adds "set to 0 for permanent retention"
- **Storage operation audit copy completion** — New "admin-triggered storage operation" i18n copy (en / zh)

### Added

- **Indefinite audit log retention**
  - `audit_service::cleanup_expired` skips cleanup and returns 0 when `retention_days <= 0`; admins can set `audit_log_retention_days` to `0` for permanent retention
  - `settings-operations` i18n copy adds "set to 0 for permanent retention, no automatic cleanup"
  - New `audit_action_admin_trigger_storage_action` i18n copy (en / zh) covering storage operation audits
  - New `test_audit_cleanup_retention_zero_keeps_logs` integration test asserting that records from 365 days ago are still retained with retention=0

### Changed

- **Inline previews via presigned direct links**
  - In `file_service::download::build`, `should_presign` changes from "Attachment only" to "presigned redirect for everything except inline MIME types requiring a same-origin CSP sandbox"; `build_presigned_redirect_outcome` accepts a `disposition` parameter, so content-disposition is no longer hardcoded to Attachment
  - `preview_link_service::download_file` switches from calling `build_stream_outcome_with_disposition_and_range` directly to `build_download_outcome_with_disposition_and_range`, so public share previews also benefit from presigned URLs
  - Frontend adds the `useContentPreviewResourcePath` hook: resolves the preview link path first when `previewLinkFactory` exists, otherwise falls back to downloadPath
  - Frontend preview components (`PdfPreview` / `BlobImagePreview` / `MusicPreview` / `VideoPreview` / `MarkdownPreview` / `CsvTablePreview` / `XmlPreview` / `JsonPreview` / `TextCodePreview`) switch from `downloadPath` to `contentPreviewPath`; `path` is now nullable and shows a loading placeholder while loading
  - The direct-link inline presigned test in `tests/test_upload.rs` now asserts a 302 redirect + `Cache-Control: no-store` + `Location` carrying `response-content-disposition`

- **Frontend resource credential checks tightened**
  - `apiUrl.ts` adds `isConfiguredApiUrl` to determine whether a URL belongs to the backend API origin; `isExternalResourceUrl` no longer treats backend API URLs as external resources
  - New `shouldSendResourceCredentials(path)`: carries session credentials only when the target is "not an external resource and not a public resource"; `authenticatedResource` now uses this check
  - Presigned URLs on the object-storage origin (different from the API origin) carry no credentials, avoiding CORS preflight failures and credential leaks

- **S3 / MinIO / R2 documentation completed**
  - `docs/storage/s3-minio-r2.md` and `docs/en/storage/s3-minio-r2.md` completed with the CORS and credential configuration needed for inline previews via presigned direct links

### Statistics

- 35 files changed, 1641 insertions(+), 116 deletions(-)
- 3 commits

## [v0.3.0-rc.1] - 2026-06-22

### Release Highlights

**AsterDrive `0.3.0-rc.1` is the release-candidate version of the 0.3.0 series, with the main themes being storage policy terminology unification and credential security hardening.** Policy fields named for S3 are unified under generic object-storage terminology (`s3_upload_strategy` → `object_storage_upload_strategy`, etc., with serde aliases keeping old names backward compatible); Microsoft Graph client secrets / tokens are wrapped in `secrecy::SecretString` with manual `Debug` implementations on all credential-holding types to ensure log redaction; the reverse tunnel stream lane falls back to poll mode on offline / closed / timeout instead of failing outright.

- **Storage policy terminology unification (Object Storage, backward compatible)** — S3-specific field names changed to generic object-storage naming; old names remain usable via serde aliases and a frontend legacy fallback
- **Credential security hardening** — Microsoft Graph client secrets / tokens wrapped in `SecretString`; related entities and providers implement `Debug` manually to guarantee log redaction
- **Reverse tunnel reliability** — Stream lane falls back to poll requests on offline / closed / timeout
- **OneDrive / Azure Blob docs completed** — Admin API and storage backend docs filled in with the new drivers

### Changed

- **Storage policy terminology unification (Breaking, backward compatible)**
  - `StoragePolicyOptions` JSON fields `s3_upload_strategy` / `s3_download_strategy` → `object_storage_upload_strategy` / `object_storage_download_strategy`; old names remain usable via `#[serde(alias = "...")]` and a frontend legacy fallback
  - Enum types `S3UploadStrategy` / `S3DownloadStrategy` → `ObjectStorageUploadStrategy` / `ObjectStorageDownloadStrategy` (Rust + OpenAPI + frontend types); enum values `relay_stream` / `presigned` unchanged
  - connector capability `s3_transfer_strategy` → `object_storage_transfer_strategy`
  - Frontend admin panel copy changed from "S3 upload / download method" to "object storage upload / download method"

- **OneDrive authorization request body tightened**
  - `POST /admin/policies/{id}/storage-authorization/start` only accepts `{ "provider": "microsoft_graph" }`; Client ID / Secret / tenant / scopes must be saved to `application_config.microsoft_graph` first

### Security

- New `secrecy = "0.10"` dependency; Microsoft Graph `client_secret` / `refresh_token` / `access_token` wrapped in `SecretString`, extracted via `expose_secret()` only when calling Microsoft Graph
- `storage_policy` / `managed_follower` / `master_binding` / `managed_ingress_profile` entities and all Microsoft Graph token provider / request types implement `Debug` manually; `access_key` / `secret_key` appear as `***REDACTED***` in logs, each type with a unit test asserting no plaintext leakage

### Fixed

- The reverse tunnel stream lane automatically falls back to poll requests on `reverse tunnel is offline` / `lane closed` / `response channel closed` / streaming wait timeout, where it previously failed outright
- RenameDialog disables its button during rename submission and uses a ref guard, preventing rapid repeated clicks from triggering multiple rename requests

### Statistics

- 109 files changed, 1808 insertions(+), 691 deletions(-)
- 3 commits

## [v0.3.0-beta.2] - 2026-06-21

### Release Highlights

**AsterDrive `0.3.0-beta.2` is a stability and error-handling polish release on the 0.3.0 beta line, with the main themes being robustness closeout of the connector abstraction and OneDrive authorization flow fixes.** Panic paths (`expect`/`unwrap`) are comprehensively replaced with `Result` error propagation; storage connection test diagnostics are unified to be returned via error metadata (migrated from the success response payload); OneDrive authorization switches to "save credentials first, then authorize", and connection tests for draft policies support reusing saved credentials; per-service cache logic is moved into dedicated cache submodules.

- **Error-handling robustness closeout** — `expect`/`unwrap` comprehensively replaced with `Result`; multipart read failures mapped to BAD_REQUEST; missing connector registration becomes an explicit error instead of silently falling back to local
- **Storage connection test API contract unified** — Diagnostics migrate from the success response payload to `ApiErrorInfo.diagnostic`, read by the frontend from `ApiError.diagnostic`
- **OneDrive authorization flow fixed** — Authorization requests send only the provider type; credential changes must be saved before authorization can start
- **Draft policy credential reuse** — Connection tests accept an optional `policy_id`; blank credential fields reuse saved credentials (S3 / Azure Blob / Tencent COS)
- **Cache module extraction** — Per-service cache logic moved into dedicated cache submodules

### Added

- **Draft policy credential reuse**
  - The connection test endpoint adds an optional `policy_id`; S3 / Azure Blob / Tencent COS reuse saved credentials when credential fields are blank, so admins don't need to re-enter sensitive information when changing configuration

### Changed

- **Error-handling refactor**
  - Panic paths in CORS, encryption, shutdown handler, CLI serialization, time/timezone arithmetic, and in-memory mail sender mutex poisoning are replaced with `Result` propagation and log degradation
  - Signal handler installation failures no longer panic; archive preview / offline download / WOPI discovery client construction failures change from silent fallback to error propagation
  - `connector_or_local` silent fallback to local becomes `connector_or_registered` with an explicit error; intentionally unreachable branches are annotated with `#[allow(clippy::expect_used)]` to mark invariants

- **Storage connection test API contract unified**
  - Removed `StoragePolicyProbeResult` and the `probe_connection*` endpoints; the standard error response carries `ApiErrorInfo.diagnostic` instead
  - New `ApiErrorDiagnostic` schema exposing message / kind publicly, with api_code / retryable kept internal
  - Frontend connection tests read the failure reason from `ApiError.diagnostic`

- **OneDrive authorization flow**
  - Authorization requests no longer carry draft Microsoft Graph credentials, sending only the provider type; the backend reuses the saved application_config
  - After credential changes, you must save before initiating authorization

- **Connector descriptor configuration**
  - Driver-type conditional branches replaced with explicit input structs (`ObjectStorageConnectorDescriptorInput` / `ObjectStorageFieldDescriptorInput`)
  - Descriptor UI logic moved from a centralized function down to each connector; the multipart ETag requirement becomes an explicit input field

- **Cache module extraction**
  - Cache operations for admin / auth / folder / passkey / preview link / share stream / stream ticket / workspace scope / WebDAV auth / WebDAV path resolver moved into dedicated cache submodules, with scope and invalidation tests completed
  - `share_stream` marker encoding moved into the cache module; `reserve_count_marker` / `store_count_marker` now propagate `Result`

- **Dependency upgrades**
  - actix-web 4.13.0 → 4.14.0、actix-multipart 0.7.2 → 0.8.0、actix-http 3.12.1 → 3.13.0、actix-multipart-derive 0.7.0 → 0.8.0
  - derive_more unified to 2.1.1, foldhash 0.2.0, impl-more 0.3.1; parse-size replaced with bytesize 2.4.0
  - Frontend `@typescript/native-preview` upgraded to `7.0.0-dev.20260621.1`

### Fixed

- Multipart upload read failures (`UploadFieldReadFailed` / `AvatarUploadReadFailed`) mapped to BAD_REQUEST, previously 500
- The offline download loop no longer silently aborts on shutdown signals and instead returns a transient error
- WOPI discovery client construction failures no longer fall back to `reqwest::Client::new` and instead propagate the error

### Statistics

- 114 files changed, 3665 insertions(+), 1347 deletions(-)
- 4 commits

## [v0.3.0-beta.1] - 2026-06-21

### Release Highlights

**AsterDrive `0.3.0-beta.1` is the first beta of the 0.3.0 series, with the main themes being storage backend expansion and connector abstraction unification.** This release adds Azure Blob Storage and OneDrive (including SharePoint sites / Group drives) cloud-drive drivers, and introduces a unified `StorageConnector` abstraction so six drivers expose capabilities and form fields through a consistent descriptor; OneDrive credentials go through Microsoft Graph OAuth + PKCE, with tokens persisted using an HKDF-derived key + AES-256-GCM encryption. The cache backends are streamlined to memory / Redis with the obsolete `cache.enabled` switch removed, and Redis gains a local memory second-level fallback; storage policy connection probing moves to structured diagnostic endpoints, returning 200 + redacted reasons on failure. Also included: structured audit log details, one-click Tencent COS CORS configuration, a PWA precache refactor, and a toast visual redesign.

- **Azure Blob Storage driver** — Block Blob CRUD, SAS URL presigned upload/download, multipart upload
- **OneDrive driver + OAuth credential management** — personal / work_or_school / SharePoint site / group drive, PKCE flow, chunked resume for >250MiB
- **StorageConnector unified abstraction** — Six drivers with unified descriptors, fields, and upload workflows; adds `/admin/policies/storage-drivers`
- **Cache backend streamlining and Redis second-level fallback (Breaking)** — Removes `cache.enabled`; falls back to memory on Redis failure
- **Storage policy diagnostic endpoint** — Structured `StoragePolicyDiagnostic`, with SAS / account key redaction
- **Structured audit logs** — File / user / policy / session operations completed with context
- **Tencent COS CORS auto-configuration** — Derives multiple origins from `public_site_url`, applied in one click
- **PWA precache and toast redesign** — Glob-based precache manifest + budget warnings, themed toasts

### Added

- **Azure Blob Storage driver**
  - New `azure_blob` driver type with full CRUD, SAS URL presigned upload / download, and Block Blob multipart upload
  - `InitUploadResponse` adds a `presigned_require_etag` field; the `presigned_put_requires_etag` trait method differentiates drivers (Azure Blob = false, S3 / remote = true)
  - `AzureBlobConfigError`: in production the SAS `spr` protocol is restricted to https; loopback / Azurite allows https + http
  - Frontend form fields changed to Storage Account Name / Key; presigned requests carry the `x-ms-blob-type: BlockBlob` header

- **OneDrive driver and OAuth credential management**
  - New `onedrive` driver type + `OneDriveAccountMode` (personal / work_or_school / sharepoint_site / group_drive)
  - 7 `onedrive_*` policy fields; Microsoft Graph OAuth + PKCE authorization flow
  - Tokens and client secrets encrypted with AES-256-GCM using a key derived via HKDF from `auth.storage_credential_secret_key` before persisting
  - >250MiB goes through the Graph native chunked upload session
  - Driver root resolved dynamically (personal / work / site / group)
  - Frontend credential panel with authorization status badge and authorize / callback entry points

- **StorageConnector unified abstraction and driver descriptors**
  - Introduces the `StorageConnector` trait + `StorageConnectorDescriptor`; six drivers uniformly expose capabilities / fields / upload workflows / actions
  - New `GET /admin/policies/storage-drivers` endpoint; the frontend replaces driver-type string checks with TTL-cached descriptors
  - New `ProviderResumableUploadDriver` trait (OneDrive / Graph native resumable upload)
  - New DB table `storage_connector_application_configs`; Microsoft Graph app configuration migrates from policy key fields to connector application config

- **Storage policy diagnostic endpoint**
  - Connection probing returns a structured `StoragePolicyDiagnostic` (api_code / kind / message / retryable)
  - Probe failures return 200 + `ok:false` instead of 4xx
  - SAS tokens and account keys redacted in responses

- **Tencent COS CORS auto-configuration**
  - New `POST /admin/policies/action` and `POST /admin/policies/{id}/action`, with `StoragePolicyActionType` as an extensible action enum
  - `configure_tencent_cos_cors` derives multiple origins from `public_site_url`, replacing only rules with the ID `asterdrive-presigned-access`
  - Draft actions support reusing saved policy credentials
  - Emits audit log entries

- **Structured audit log details**
  - User management, storage policies, teams, session revocation, and other operations completed with audit snapshots and display copy
  - File / folder operations completed with location and transfer path details
  - Batch tag operations, MFA challenges, passkey logins, WebDAV, and WOPI converted to structured details
  - New structs such as `ShareDeleteAuditDetails`, `UserMfaManageAuditDetails`, `ExternalAuthUnlinkAuditDetails`

### Changed

- **Cache backend streamlining (Breaking)**
  - Removed the `cache.enabled` config option and the noop cache backend, keeping only memory / Redis
  - CacheBackend trait adds `take_bytes` / `delete_many`
  - Redis gains a second-level fallback: on Redis failure or circuit breaking, it falls to a local MemoryCache, cleaning up shadow entries after recovery
  - Extracted a `RedisClient` trait + `FakeRedisClient` test double

- **Storage policy frontend checks refactored**
  - Removed `isS3CompatibleDriver` / `isOneDriveDriver` / `isObjectStorageDriver` in favor of field-presence detection
  - `POLICY_OPTION_SERIALIZERS` replaced by `buildPolicyOptionsFallback`, writing only non-default values

- **Upload session field unification (DB migration handles it automatically)**
  - Renamed `s3_temp_key` → `object_temp_key`, `s3_multipart_id` → `object_multipart_id`
  - `S3_MULTIPART_MIN_PART_SIZE` → `OBJECT_MULTIPART_MIN_PART_SIZE`
  - OneDrive credential storage migrated from `storage_policies.access_key` / `secret_key` to `storage_connector_application_configs`; legacy fields are cleared automatically (code falls back to reading legacy fields)

- **PWA precache and toast rework**
  - vite-plugin-pwa switched to glob patterns + critical asset verification; manual precache manifest and forbidden list removed
  - Precache budget soft warnings: entries ≤450, raw size ≤5MB; large modules such as admin / file browser / music player / PDF / office excluded
  - `pwaWarmup` tracks user / admin routes separately
  - Toaster theming (oklch, backdrop-blur, 4.2s duration, i18n.dir direction)

- **Exhaustive audit logging**
  - `detail_message` wildcard branch replaced with an exhaustive match, preventing new actions from being silently missed
  - `mfa_factor_repo::list_for_user` generalized to `ConnectionTrait`, supporting in-transaction calls

- **Dependency upgrades**
  - Rust：sea-orm 2.0.0-rc.41、aws-sdk-s3 1.137、h2 0.4.15
  - Frontend: @base-ui/react 1.6, axios 1.18, react-router-dom 7.18, tailwindcss 4.3.1, biome 2.5, @types/node 26, @typescript/native-preview 7.0.0-dev.20260620.1

### Fixed

- **PWA precache case-sensitive exclusion misses**
  - Fixed missed exclusions on case-sensitive file systems: added lowercase / camelCase variants `assets/**/*admin*`, `assets/**/*musicPlayer*`

### Security

- OneDrive OAuth tokens and client secrets are encrypted at rest with an HKDF-derived key + AES-256-GCM; the API and audit logs expose only the `client_secret_configured` boolean state
- Storage policy connectivity diagnostics now redact SAS tokens and account keys in responses
- OneDrive clears the legacy `storage_policies.access_key` / `secret_key` fields on policy create / update, preventing plaintext secrets from lingering

### Database Migrations

- Added `m20260612_000001`: `storage_policy_credential`, `storage_policy_authorization_flow` tables (encrypted OneDrive OAuth token storage)
- Added `m20260620_000001`: backfills 3 JSON columns with `{}` and enforces NOT NULL (across MySQL / SQLite / PostgreSQL)

### Configuration Changes

- **Removed `cache.enabled` (Breaking)** — the cache toggle no longer has any effect; only the memory / Redis backends remain; the `cache.enabled` field in existing configs is ignored
- Added `auth.storage_credential_secret_key` — master encryption key for OneDrive / future OAuth driver credentials (auto-generated at startup if missing)
- Added OneDrive policy fields: `onedrive_account_mode`, `onedrive_tenant`, `onedrive_site_id`, `onedrive_drive_id`, `onedrive_group_id`, `onedrive_scopes`, etc.

### Statistics

- 316 files changed, 36108 insertions(+), 3381 deletions(-)
- 9 commits

## [v0.3.0-alpha.5] - 2026-06-13

### Release Highlights

**AsterDrive `0.3.0-alpha.5` is the fifth pre-release of the 0.3.0 series, focusing on security hardening, folder-level storage policy binding, anti-enumeration of the activation flow, and read-only browsing / share archive download experience.** This release splits the public share password cookie, public direct links, preview links, and share streaming playback sessions from `auth.jwt_secret` into dedicated HMAC keys, and hardens boundary checks in WebDAV paths, XML parsing, upload size validation, and share download concurrency; it adds folder-level storage policy binding (admin-only) with policy inheritance resolution; activation email resends now return a uniform anti-account-enumeration response; the file browser introduces read-only mode with support for share archive downloads; upload retries, lazy-loaded thumbnails, and i18n bundle splitting further improve frontend performance.

- **Auth and public link key isolation (Breaking)** — added `share_cookie_secret`, `direct_link_secret`, splitting direct-link / preview / streaming token verification
- **Folder-level storage policy binding** — admins can bind/clear folder policies; uploads use the nearest ancestor policy with full inheritance resolution
- **Anti-enumeration activation resend flow** — uniform responses for login and activation resend, enforced response delays, per-outcome granular metrics
- **Share archive download and read-only browsing** — `FileBrowserProvider` read-only mode, `/s/{token}/archive-download` endpoint, download quotas with rollback on interruption
- **Security hardening across modules** — XXE protection, WebDAV path normalization, lock count limits, share cookies bound to the client, actual upload size validation
- **schema drift detection** — cross SQLite/Postgres/MySQL consistency checks of column definitions between SeaORM entities and migration output
- **Frontend performance optimizations** — on-demand i18n bundle splitting, post-login PWA warmup path, in-viewport thumbnail fetching, debounced upload retries

### Added

- **Folder-level storage policy binding**
  - Added `PUT /api/v1/admin/folders/{id}/policy` endpoint (admin-only) to set or clear folder policy bindings
  - Policy inheritance resolution: folders inherit the nearest ancestor's explicit policy, falling back to user/team policy groups
  - Upload service resolves the effective policy along the folder hierarchy
  - Audit log gains a `folder_policy_change` action (records previous and new policy IDs)
  - Access control: only admins can set/clear folder policies; regular PATCH requests reject the `policy_id` field
  - Validation: rejects unavailable policies, locked/deleted folders, and broken hierarchy chains
  - Frontend `FolderPolicyDialog` component (policy selection, inheritance visualization, effective policy hints)
  - Context-menu entry visible to admins only, with dialog preloading

- **Anti-enumeration activation resend flow**
  - Added `ActivationResendRequestPanel` component (entry inside the login form; does not reveal whether the account exists)
  - Email field auto-populated from the login identifier
  - Backend introduces the `RegisterActivationResendOutcome` enum (Sent / EmailNotFound / AlreadyActive / AccountDisabled / Cooldown / EmailPolicyRejected)
  - Structured metrics emitted per outcome via `record_auth_event`; externally only a generic 200 response is visible
  - Email policy rejections return a generic 200 instead of 400, avoiding leaking account status
  - `apply_auth_mail_response_floor` enforces a minimum response delay on `setup`/`register`/`login`, mitigating timing enumeration attacks
  - Login failures uniformly converge to `auth.credentials_failed` (wrong credentials, pending activation, and disabled account share the same response)
  - Introduced `LoginFailureReason` enum preserving internal context for metrics without writing it into API responses

- **Share archive download (shared ZIP)**
  - Added `POST /s/{token}/archive-download` and `GET /s/{token}/archive-download/{ticket}` endpoints
  - `stream_ticket_service` adds the `SharedArchiveDownload` ticket type
  - `task_service::archive::selection` adds `prepare_shared_archive_download`, `stream_shared_archive_download`
  - Added `archive_download_user_enabled`, `archive_download_share_enabled` runtime config toggles
  - Added API error codes `ArchiveDownloadUserDisabled`, `ArchiveDownloadShareDisabled`
  - Share download quota is shared with archive downloads: download counts are reserved and rolled back when creating/consuming tickets, and automatically rolled back when the client interrupts the stream

- **File browser read-only mode**
  - `FileBrowserContextValue` adds `readOnly`, `selectionEnabled` flags
  - `FileBrowserContextValue` adds a `getThumbnailPath` callback (injects thumbnails in read-only scenarios)
  - Read-only mode suppresses selection, dragging, sorting, and the context menu; can be unlocked independently via `selectionEnabled`
  - Delete/tag/move buttons auto-hide when no corresponding handler is present
  - `useFileBrowserBatchActions` adds `allowDelete`, `allowTagManagement` options
  - Removed `ReadOnlyFileCollection`/`ReadOnlyFileGrid`/`ReadOnlyFileTable`, replaced by `FileBrowserProvider` + `readOnly` mode
  - `ShareFolderView` now uses `FileBrowserProvider` + read-only mode; archive download goes through `shareService.streamArchiveDownload`

- **schema drift detection**
  - Added `tests/test_schema_drift.rs::test_entity_columns_match_migrated_database_schema`
  - Schema introspection across SQLite (PRAGMA), PostgreSQL/MySQL (information_schema.columns)
  - Collects column definitions of 42 entities via a macro-based registry and compares them against migration results
  - Uses `BTreeSet` to guarantee deterministic comparison ordering

- **Admin system info endpoint**
  - Added `GET /api/v1/admin/system-info` (build version and build time exposed only after authentication)
  - Frontend admin "About" page shows backend build time, falling back to a localized "unknown" when missing/invalid
  - Added `formatBuildTime` to validate timestamps and reuse `formatDateTime`

- **WebDAV per-user lock count limit**
  - Added `webdav.max_active_locks_per_user` config (default 1024)
  - Added `count_active_by_owner` repository function (counts unexpired and no-timeout locks)
  - Introduced `DavLockPreflightError`, `DavLockSystem::prepare_lock` hook; re-checks inside the transaction to avoid TOCTOU
  - Over-limit returns HTTP 507 Insufficient Storage (`webdav.lock_limit_exceeded`)
  - Added user row-level locking (`lock_by_id`) to ensure accurate quota checks under concurrency

### Changed

- **Auth and public link key isolation (Breaking)**
  - Added `auth.share_cookie_secret` and `auth.direct_link_secret`, splitting the public share password verification cookie, public direct links, preview links, and share streaming playback sessions from `auth.jwt_secret` into dedicated HMAC keys
  - Existing `data/config.toml` files get missing `auth.jwt_secret`, `auth.share_cookie_secret`, `auth.direct_link_secret`, and `auth.mfa_secret_key` auto-filled at startup without overwriting existing values
  - Since direct link / preview / stream tokens no longer accept `auth.jwt_secret` verification, public direct links, preview links, and share streaming playback session tokens generated before the upgrade become invalid and must be regenerated
  - Share password verification cookies become invalid due to the `auth.share_cookie_secret` switch; users need to re-enter share passwords

- **Download filename encoding (Content-Disposition)**
  - `DownloadDisposition` extracted into a standalone `download_headers` module
  - Switched to RFC 5987 `filename*=UTF-8''<percent-encoded>` format, using actix-web's `ContentDisposition` constructor
  - Sanitizes control characters such as `\r`, `\n`, `\0` to prevent header injection
  - Covers all download endpoints (direct links, preview links, archive streaming)

- **Share password cookie client binding**
  - Introduced `ShareCookieBinding`: binds the Cookie MAC to the user agent SHA256 hash + IP subnet (IPv4 /24, IPv6 /64)
  - HMAC structure: `share_verified:{token}:ua:{hash}:ip:{subnet}`
  - Merged duplicate cookie checks into `check_share_cookie` / `check_share_cookie_ignoring_download_limit`
  - Cookies remain valid within the same subnet and same user agent; cross-client/cross-network replay is blocked

- **Public health check information convergence**
  - `/health` and `/ready` no longer return raw `version`, `build_time`, preventing unauthenticated clients from obtaining build metadata
  - Storage readiness failure responses drop the `error` field to prevent driver detail leakage
  - Health check endpoints return `HealthResponse` directly, no longer wrapped in `ApiResponse`
  - Build metadata moved to the authenticated `/admin/system-info` endpoint added above

- **Multipart / direct upload actual size validation**
  - `UploadedMultipartPart` carries part number + size metadata
  - `list_uploaded_parts` renamed to `list_uploaded_part_details`, returning size information
  - S3 completion flow: fetches each part's actual size, validates consecutive part numbers, compares against the declared total size, and calls `AbortMultipartUpload` on mismatch
  - After direct streaming uploads complete, re-reads blob metadata, compares against the declared size and policy limits, and re-verifies quota
  - Cleans up the preuploaded blob on validation failure, preventing clients from bypassing quota

- **WebDAV path normalization and XXE protection**
  - Introduced `PathEscape` error; percent-decoding first, then `.` / `..` segment folding
  - Rejects paths with leading `..` segments (including encoded variants like `%2e%2e`) escaping the mount root
  - Collection resources preserve trailing slashes
  - Calls `reject_xml_dtd_or_entity` before PROPFIND / PROPPATCH / LOCK / REPORT parsing, detecting `<!DOCTYPE`, `<!ENTITY`
  - Returns 403 + `<no-external-entities/>` error body when triggered (with the `xmlns:D` namespace)

- **WebDAV lock error model**
  - Introduced `DavLockError` (`Conflict`/`LimitExceeded`/`Backend`) replacing `Result<DavLock, DavLock>`, so lock conflicts and quota exhaustion return distinct responses
  - `Backend` is used for database failures, mapped to HTTP 500
  - Added structured tracing: includes path, entity type/ID, and error details
  - Lock quota validation fixed from a `prepare_lock` no-op to actually taking effect; scope is validated before refreshing timeout
  - Missing Timeout header now defaults to `MAX_LOCK_DURATION_SECS` (7 days) instead of "infinite" semantics, and rejects oversized values that could overflow chrono

- **WebDAV cached authentication re-validation**
  - `CachedWebdavAuth` adds `account_id`; on cache hits it re-validates directly via DB
  - `validate_cached_scope` replaced by `validate_cached_account`: re-checks the account's username, user/team/folder IDs, and enabled status
  - Immediately invalidates the cache and returns `AuthForbidden` when an account is disabled or its fields change
  - Validates the cached password hash, preventing stale credentials from continuing to hit

- **WOPI security enhancements**
  - Added `X-WOPI-Token` header as the preferred authentication route, with query parameter fallback for older clients
  - All WOPI responses include `Cache-Control: no-store`, preventing token leakage via browser caches
  - The `access_token` query parameter changed from required to optional
  - Default WOPI access token TTL reduced from 60 minutes to 15 minutes

- **Share download count atomicity**
  - After `increment_download_count` succeeds, the share record is immediately re-read, avoiding stale in-memory values for limit checks
  - Replaces the `saturating_add(1)` comparison with a direct `>=` against the re-read counter
  - On reload failure, logs a warning and falls back to cache invalidation
  - Debug logs now include `share_id`, `download_count`, `max_downloads`

- **Admin dashboard quantity-unit input controls**
  - Added `AdminNumberUnitInput`, a generic numeric value + unit dropdown control
  - Quota inputs replaced hardcoded MB with `AdminStorageQuotaInput` (bytes → TB)
  - Scaling numeric fields in system settings now use the shared unit component
  - System settings hint text moved behind a help trigger tooltip, reducing visual noise
  - Scaling numeric controls gained live validation: rejects converted values exceeding `Number.MAX_SAFE_INTEGER`, preserving invalid drafts and displayed units
  - `AdminTeamDetailDialog` quota state merged into a single `quotaDraftOveride`, fixing stale draft behavior on save/delete/restore
  - Quota validation accepts 0 and positive integers; non-positive multipliers and negative conversions are rejected by the schema

- **Frontend thumbnail and auth cache isolation**
  - Persisted user cache now keeps only profile, preferences, and token expiry, stripping `id`/`email`/`role`/`storage`
  - Migrates and clears sensitive fields when reading existing caches
  - Thumbnail cache namespace changed from user ID-derived to a session UUID in sessionStorage
  - Namespace cleaned up on session expiry

- **Frontend i18n and PWA warmup reordering**
  - Split i18n loading into an authenticated shell and the full bundle; the shell contains only core/files/tasks/share/search/errors/offline
  - The successful login path directly warms up shell i18n and the file browser route
  - User route warmup is skipped when triggered on the post-login path, avoiding duplication
  - Preview engine warmed up on demand when entering the file browser
  - `App` renders authenticated routes only after the auth check and shell i18n are both ready, avoiding untranslated flashes
  - Service Worker static asset caching strategy changed from StaleWhileRevalidate to CacheFirst for more reliable offline behavior

- **Frontend file browser / upload / thumbnail experience**
  - `useEnteredViewport` adds `trackVisibility`/`isInViewport`; `FileThumbnail` fetches only when in the viewport
  - Persisted thumbnails revalidate in the background by ETag; blob URLs are cleaned up when leaving the viewport to free memory
  - `useUploadAreaUploads` adds a `retryingTaskIdsRef` set, debouncing concurrent retry triggers
  - `summarizeUploadTasks` progress denominator now uses `progressCount`, weighted by size
  - `UploadPanel` introduces `taskRowKey` and passes `getItemKey` to the virtualizer so old rows are not reused on state changes
  - Several UI states in `FolderPolicyDialog` and `FileBrowserPage` migrated to `useReducer`, with a `targetKey` sentinel discarding stale async results
  - Upgraded to `@testing-library/user-event` to simulate real interactions

- **Routing and lazy loading**
  - Introduced `localizedLazyPage` helper that preloads required i18n namespaces before rendering lazy pages
  - `AdminRoute` preloads `admin` and `core` namespaces before rendering, degrading with a warning on failure
  - Reordered the `/external-auth/links` route to avoid being swallowed by the `/{kind}/{provider}` wildcard

- **Token generation scheme**
  - `new_share_token()` changed from an 8-character base62 string to a 32-character UUID v4 hex
  - Remove custom charset and unused `rand::RngExt` reference

- **Window navigation security attributes**
  - All `window.open` calls in share view download and folder download now pass the three-argument `noopener,noreferrer`

- **Dependency updates**
  - `nom-exif` 3.6.0 → 3.6.1，`aws-smithy-types` 1.4.9 → 1.5.0，`aws-smithy-eventstream` 0.60.20 → 0.60.21
  - Minor updates to `block-buffer`, `cc`, `memchr`, `regex`, `regex-syntax`, `rust_decimal`, `smallvec`, `time`/`time-core`/`time-macros`, `uuid`, `zerocopy`/`zerocopy-derive`, etc.
  - Remove unused `powerfmt` dependency
  - Add `AptS-1543` as an author in `Cargo.toml`

### Fixed

- **Share archive download concurrency and rollback**
  - Reserve and roll back the share download count when creating/consuming an archive ticket
  - Roll the download count back to 0 on client abort or downstream stream failure; error logs now include `share_id`
  - Cache validated folder IDs to avoid repeated authorization
  - `FileCard` checkbox `onChange` fallback prevents errors when the handler is undefined

- **Admin quota draft**
  - `quotaValueToBytes` compares actual byte values instead of display strings, avoiding false change detection
  - `AdminNumberUnitInput` applies destructive borders to both the number input and unit selector when in `invalidState`
  - Unit change handler now matches against the units array, avoiding unsafe comparisons with empty values
  - Keep displaying invalid drafts instead of silently discarding them
  - Split `UserDetailDialogBody` into Content / Footer / Profile / Security subcomponents for readability

- **Route guards and concurrent refresh**
  - Seed a CSRF token for the winning token in `test_concurrent_refresh_same_token_has_single_winner`
  - Add `ensureI18nNamespaces` and logger mocks to `routeGuards.test.tsx`, plus a new test case for admin locale loading failure

- **Miscellaneous fixes**
  - Remote driver `list_uploaded_part_details` filters out invalid part numbers (≤0)
  - S3 `list_parts` pagination handles missing `next_part_number_marker` correctly
  - MFA test assertions updated to `401 UNAUTHORIZED` + `auth.credentials_failed`, consistent with current behavior for unverified-email login
  - Share download concurrency tests isolated into a dedicated SQLite file, using a pool-size-1 connection to avoid shared-state interference

### Security

- Public share password cookie / public direct link / preview link / share streaming playback sessions switched to dedicated HMAC keys
- Share password cookie bound to user agent hash and IP subnet, blocking cross-client replay
- Multipart and direct uploads validate actual uploaded size against the policy limit and count quota by real size
- WebDAV path normalization blocks directory traversal (including percent-encoded `..` variants)
- WebDAV XML endpoints reject DTD/ENTITY, mitigating XXE
- WebDAV limits the number of active locks per user, preventing resource abuse
- Login and activation resend unified error codes + minimum response latency to mitigate account enumeration
- Health checks no longer leak version and build time; storage readiness no longer leaks driver error details
- WOPI prefers header authentication, responses use `Cache-Control: no-store`, default TTL shortened to 15 minutes
- Persisted user cache stripped of sensitive fields such as ID/email/role/storage
- Thumbnail cache namespace switched from user ID to session UUID
- Shared RFC 5987 encoding sanitizes `\r`/`\n`/`\0`, preventing Content-Disposition header injection
- `window.open` calls uniformly append `noopener,noreferrer`, preventing tabnabbing and referrer leaks
- WebDAV cached-auth hits re-verify account status and password hash

### Testing

- Add schema drift integration tests covering SQLite/PostgreSQL/MySQL
- Add integration tests for activation resend with active accounts, disabled accounts, and email policy blacklist scenarios, verifying no email is sent for skipped cases
- Add tests for WebDAV lock quota, path traversal (multiple `..` encodings), Timeout boundaries, XXE, and cached auth re-verification
- Add share download count atomicity test: 32 concurrent requests against `max_downloads=1` yield exactly one reservation
- Add integration test for share archive download abort rollback: download count returns to 0 and subsequent downloads proceed normally
- Add tests for direct upload policy boundaries and metadata size overflowing `i64::MAX`
- Add regression test ensuring password generation includes all four character classes
- Add coverage for `FolderPolicyDialog` close, stale result discarding, keeping the dialog on save failure, and empty policy lists
- Add route tests for i18n namespace preloading and admin namespace loading failure
- Add test for `AdminSettingsConfigRows` retaining configuration on partial updates
- Fix `AdminTeamDetailDialog` test by moving value assertions into `waitFor` to avoid races

### Database Migrations

No new migrations.

### Configuration Changes

- Add `auth.share_cookie_secret` and `auth.direct_link_secret` (auto-populated at startup when missing)
- Add `archive_download_user_enabled` and `archive_download_share_enabled` runtime toggles
- Add `webdav.max_active_locks_per_user` (default 1024)
- WOPI default access token TTL reduced from 60 minutes to 15 minutes

### Statistics

- 249 files changed, 11957 insertions(+), 2361 deletions(-)
- 10 commits

---

## [v0.3.0-alpha.4] - 2026-06-11

### Release Highlights

**AsterDrive `0.3.0-alpha.4` focuses on PWA startup performance auditing, fine-grained API error code classification, Service Worker cache optimization, and unified database type constraints.** This version adds a startup performance monitoring toolchain that automatically generates performance reports and Web Vitals metrics; API error codes are refined in layers by module (search, policy, storage, tasks), improving debugging and documentation accuracy; the Service Worker caching strategy is restructured to support fine-grained version control; runtime database type constraints are unified as `DatabaseConnection`, removing redundant `ConnectionTrait` generics. The security policy documentation was also updated to support the branch identifier `master`, and documentation examples were fixed.

- **PWA startup performance audit** — automated performance test scripts, Web Vitals metrics collection, HTML report generation
- **Fine-grained API error code classification** — independent error code sets for the search / policy / storage / tasks modules
- **Service Worker cache optimization** — version-level cache isolation and update strategies
- **Unified database type constraints** — `ConnectionTrait` generics removed, `DatabaseConnection` trait object used
- **Documentation and configuration updates** — security policy branch reference update, Cargo.toml optimizations

### Added

- **PWA startup performance audit tooling**
  - Add `frontend-panel/scripts/audit-startup.mjs` automated audit script
  - Web Vitals metrics collection (LCP, FID, CLS, FCP, TTFB)
  - HTML report generation with score grading and detailed metrics
  - Performance baselines and threshold detection

- **Fine-grained API error code classification (Breaking)**
  - `search` module: `search.invalid_query`, `search.query_timeout`, etc.
  - `policy` module: `policy.not_found`, `policy.driver_type_mismatch`, etc.
  - `storage` module: `storage.quota_exceeded`, `storage.access_denied`, etc.
  - `tasks` module: `tasks.not_found`, `tasks.invalid_state`, etc.
  - Documentation updated in sync; all error codes mapped to fine-grained categories

- **Service Worker cache version isolation**
  - Version-level cache key support (cache name includes the application version)
  - Enhanced PWA route warm-up phase
  - Optimized cache update and cleanup strategies

### Changed

- **Unified database access types (Breaking on internal API)**
  - `ConnectionTrait` generics fully removed
  - All service layers and API routes use the `DatabaseConnection` trait object
  - Zero-cost abstraction with identical behavior
  - Affects 40+ files

- **Documentation and configuration updates**
  - `SECURITY.md` supported branch updated from `main` to `master`
  - `CONTRIBUTING.md` example repository URL updated
  - `Cargo.toml` release configuration: `strip = false` → `strip = true`

### Fixed

- **Startup performance test coverage**
  - Add `frontend-panel/src/lib/pwaWarmupLoaders.test.ts` unit tests
  - Improved verification of initialization route loading logic

### Database Migrations

No new migrations.

### Statistics

- 150 files changed, 3676 insertions(+), 1022 deletions(-)
- 6 commits

---

## [v0.3.0-alpha.3] - 2026-06-10

### Release Highlights

**AsterDrive `0.3.0-alpha.3` is a release pipeline correction of `0.3.0-alpha.2`. The two are equivalent in application code, database migrations, runtime configuration, and user-visible features; `alpha.3` exists solely to re-publish the complete GitHub Release assets.** The initial release of `0.3.0-alpha.2` triggered GitHub's immutable release restriction, leaving some archive assets un-uploaded; this version fixes the release process by creating a draft release first, uploading all assets, then publishing. Since this correction only affects the GitHub Release publishing process, Docker images or in-image version metadata may still identify as `0.3.0-alpha.2`, which is equivalent in application-layer content to `0.3.0-alpha.3`.

### Changed

- **Release process fix**
  - GitHub Releases are now created as drafts with all archive assets uploaded before being published as an official release / prerelease
  - Avoids failures from uploading assets after a release is already published in immutable release repositories
  - `v0.3.0-alpha.3` is equivalent in application-layer changes to `v0.3.0-alpha.2`; see `v0.3.0-alpha.2` for the full feature changes
  - Docker images or in-image version metadata may still show `0.3.0-alpha.2`; this is a release identifier difference only, not a functional or code difference

## [v0.3.0-alpha.2] - 2026-06-10

### Release Highlights

**AsterDrive `0.3.0-alpha.2` is the second prerelease of the 0.3.0 series, focusing on storage policy management, user security controls, and file browsing experience.** This version introduces an automatic S3-compatible storage driver promotion mechanism and path-style configuration, supporting more flexible object storage integration; adds a mandatory password change flow where admins can require users to update their password on first login or in specific scenarios; the tag system gains creation capabilities and real-time event notifications; and the file browser gets a full UI revamp with upgraded preview dialogs, music player, and filter toolbar. It also fixes user login statistics missing passkey and external authentication, and improves the CI release process.

- **S3 storage policy enhancements** — driver promotion, path-style configuration, storage wizard revamp
- **Mandatory password change** — forced password change on first login, manual admin trigger, full audit flow
- **Tag management upgrades** — users can create tags directly, storage event notifications, UI polish
- **File browser revamp** — unified filter toolbar, redesigned preview dialog, enhanced music player
- **Configurable image dimensions** — thumbnail and preview image sizes dynamically adjustable via system configuration

### Added

- **Mandatory password change flow**
  - Add `user.must_change_password` field (migration `m20260610_000001_add_user_must_change_password`)
  - Restricted token mechanism: when a password change is required at login, a restricted token with `password_change: true` is issued
  - Restricted tokens can only access `/api/v1/auth/password/change` and `/api/v1/auth/logout`
  - Password change enhancements: reject identical old/new passwords, automatically clear the `must_change_password` flag on success
  - Admin user creation enhancements: password is optional (blank generates a 24-character temporary password), returns `generated_password`
  - Admins can manually trigger/clear the mandatory password change requirement for users
  - Frontend adds `ForcePasswordChangePage`, `GeneratedPasswordDialog`, `UserSecurityActionsSection`
  - Route guards: `LoginGuard` and `ProtectedRoute` detect restricted tokens and redirect to the mandatory change page
  - Internationalization support (Chinese and English)
  - Full test coverage (restricted tokens, temporary passwords, audit redaction)

- **Configurable thumbnail and preview dimensions**
  - New configuration keys: `thumbnail_max_dimension` (default 400px) and `image_preview_max_dimension` (default 1600px)
  - Non-default dimensions use dimension-specific cache paths (e.g., `1-d320`, `1-d2048`)
  - All derived rendering paths (vips_cli, ffmpeg_cli, lofty, storage_native) pass configured dimensions
  - Configuration validation: range 1–16384; default values use default cache paths

- **Tag management enhancements**
  - Inline creation in the tag library manager: a "Create tag" button appears when a search query has no matches, with Enter shortcut support
  - Inline tag color editing: a color picker added to the editor, exporting `TAG_COLOR_PALETTE` for reuse
  - Storage change events: new `tag.created`, `tag.updated`, `tag.deleted`, `tag.assignment_changed` events
  - Frontend real-time subscriptions: `SearchBrowserPage` and `CategoryBrowserPage` subscribe to tag events and reload when displayed files are affected
  - UI improvements: dialog scroll layout fixes, draft state retained during close animation
  - Add `affected_parent_ids_for_entities()` helper (chunked queries, 500 per batch)

- **Lock state change notifications and share refresh**
  - New `lock.created` and `lock.deleted` storage events
  - `ShareDialog` gains an `onShareCreated` callback (file browser list refreshes after a page share is created)
  - Fix `onShareCreated` synchronous-throw regression: wrap with `.then()`, exceptions caught by `.catch()`

- **Online archive compression toggle**
  - New `archive_compress_enabled` configuration key (default true)
  - Returns `archive_compress.disabled` (HTTP 403) when the flag is off
  - Internationalization support

- **S3-compatible driver promotion and path-style control**
  - New `POST /api/v1/admin/policies/{id}/promote-s3-driver` endpoint
  - Driver promotion guards: explicit allowlist (S3 → TencentCos), active upload session check, storage bucket immutability verification
  - S3 path-style control: `StoragePolicyOptions` gains an `s3_path_style` field (default true)
  - Remove Cloudflare R2-specific logic: no longer rewrites R2 URLs or rejects `.r2.dev`
  - Frontend UI: the creation wizard shows a driver suggestion banner when a Tencent COS endpoint is detected; the edit form shows a promotion panel
  - New `S3PathStyleField` toggle (visible only for the generic `s3` driver)
  - Form stability improvements: replace `useEffect`+`useState` with `useRef` to avoid stale state
  - Add deep-equality check `policyFormValueEquals` to detect unsaved changes

- **UI/UX revamp and enhancements**
  - New `AdminFilterToolbar` collapsible component (with toggle button and active filter badges)
  - New `useRetainedDialogValue` Hook (retains dialog content during close animation)
  - Global search filters: filters collapsed behind a toggleable inline button, tag options hidden in a secondary selector
  - Search moved from dialog to a full page, with new `/search` and `/teams/:id/search` routes
  - Admin sidebar navigation reordered
  - Profile settings view refactor: uses `SettingsRow` layout, `usePendingAction` Hook
  - Security page improvements: each panel gains a `descriptionKey`, two-column layout on large screens
  - MFA actions simplified: custom action components removed, replaced with standard `animate-in`/`fade-in` classes
  - About page redesign: two-column grid layout, color bar decorations, build details grid, four feature cards
  - Settings UI density tightened: reduced spacing (space-y-10 → space-y-6), narrower navigation
  - Save bar animation improvements: CSS transition-based, `latestVisibleStateRef` freezes content on exit
  - File browser redesign: left-aligned file cards, meta text row showing size, amber folder icon container
  - File preview enhancements: new `FilePreviewFileSummary` component, preview surface component system (`PreviewSurface` family)

### Changed

- **CI/CD optimizations**
  - GitHub Release publishing process improvements: binaries packaged into archives (tar.gz/zip) before upload
  - `.tar.gz` for Linux/macOS targets, `.zip` for Windows targets
  - Release notes updated with download links and checksum instructions

- **Test coverage enhancements**
  - E2E tests: adapted to UI changes, added file browser filter interaction tests
  - New unit tests: tag creation, mandatory password change, auth resources, preview components

- **Dependency updates**
  - `wasm-bindgen` upgraded to 0.2.123
  - Add `audit.toml` advisory suppression configuration

### Fixed

- **User login statistics fix**
  - Fix: login count statistics in the admin service now correctly include passkey and external authentication logins
  - Previously only password logins were counted, making statistics inaccurate for WebAuthn or OIDC users

### Database Migrations

- `m20260610_000001_add_user_must_change_password` — adds a `must_change_password` field to the `user` table (default false)

### Configuration Changes

- New configuration keys:
  - `thumbnail_max_dimension` — maximum thumbnail dimension (default 400px, range 1–16384)
  - `image_preview_max_dimension` — maximum preview image dimension (default 1600px, range 1–16384)
  - `archive_compress_enabled` — online archive compression toggle (default true)
- Storage policies support the `s3_path_style` option (S3-compatible storage)

---

**Statistics**:
- 339 files changed, 15,023 insertions(+), 3,282 deletions(-)
- 8 commits

## [v0.3.0-alpha.1] - 2026-06-09

### Release Highlights

**AsterDrive `0.3.0-alpha.1` is a prerelease of the 0.3.0 series, focusing on unified API error code protocol, the user invitation flow, the file tag system, and runtime architecture decoupling.** This version merges the backend's dual-track `ErrorCode` (numeric) and `ApiSubcode` (string) into a single string `ApiErrorCode`, serving as the sole stable error code source for the frontend and documentation; it adds a user invitation system where admins can send one-time registration links via email; it launches a file/folder tag system with workspace scoping, batch operations, and search integration; and it splits runtime state into a composable trait system, paving the way for multi-runtime scenarios. On the UI side, the action menu, z-index system, category browsing page, and full-page search were also reworked.

- **Unified API error code protocol** — merges `ErrorCode`/`ApiSubcode` into string `ApiErrorCode`; internal storage protocol bumped to v4, backward incompatible
- **User invitation system** — admin email invitations, one-time registration links, status tracking, revocation, customizable email templates
- **File/folder tag system** — workspace scoping, batch binding, search filtering, visual color management
- **Runtime trait architecture** — `PrimaryAppState`/`FollowerAppState` become composable traits, improving testability and multi-runtime extensibility
- **Explicit API state access** — all API routes uniformly use `state.get_ref()`, removing implicit Deref and improving type safety
- **Frontend experience engineering** — action menu, semantic z-index tokens, category browsing page, full-page search, inline confirmation UI

### Added

- **User invitation system**
  - New `user_invitations` table, supporting pending / accepted / expired / revoked state transitions
  - Invitation repository, service layer, token generation and validation logic
  - Admin API: create, list, and revoke invitations
  - Public API: validate and accept invitations
  - Invitation-specific error codes (invalid, expired, revoked, accepted)
  - Customizable invitation email templates (HTML + subject, Chinese and English supported)
  - Automatic expiration and revocation mechanisms + audit log coverage
  - Frontend `InviteUserDialog`, `UserInvitationsTable`, `InviteRegisterPage` components
  - `LoginPage` integrated invitation flow with internationalized error codes
  - Correct handling of logged-in user state when accepting an invitation

- **File/folder tag system**
  - New `tags` table with personal/team workspace scoping and normalized name index
  - Tag CRUD: create, rename, recolor, delete
  - Tag binding endpoints: attach/remove tags on files and folders
  - Batch tag operations (across multiple files/folders)
  - Tag filtering integrated into existing file/folder search
  - Audit logs for tag lifecycle and binding operations
  - Frontend `TagChips` component (color coding + overflow handling)
  - Frontend `TagManagerDialog` (single/batch management) + `TagLibraryManagerDialog` (workspace-level management)
  - Tags wired into the file browser (cards/table rows/context menus/bulk actions)
  - Tag display and management wired into file/folder info dialogs
  - Global search adds tag filtering with any/all matching modes
  - Tag library management entry added to file browser toolbar and context menu
  - Chinese and English translations
  - Rename action semantics: `copy` → `copy_to`, `move` → `move_to`
  - API endpoints:
    - `GET /api/v1/tags`、`POST /api/v1/tags`、`PATCH /api/v1/tags/:id`、`DELETE /api/v1/tags/:id`
    - `GET/PUT /api/v1/tags/:entity_type/:entity_id` query and replace entity tags
    - `PUT/DELETE /api/v1/tags/:tag_id/:entity_type/:entity_id` single attach/remove
    - `PUT/DELETE /api/v1/tags/:tag_id/batch` batch attach/remove
    - Mirrored endpoints under team workspaces: `/api/v1/teams/:team_id/tags`

- **Mail audit log**
  - New audit actions `mail_send` and `mail_delivery_failed`
  - New audit entity type `mail`
  - Record mail delivery attempts (template, recipients, error details)
  - Covers the outbox dispatcher and direct-send scenarios (MFA, config test)
  - Enhanced audit fields: optional IP, User-Agent
  - Sensitive fields (recipient name, subject, error) UTF-8 safely truncated to 1024 characters
  - Frontend i18n support for mail audit entries (Chinese and English)

- **Frontend UI experience**
  - New `ManagerDialogShell` generic dialog skeleton (fixed header/scrollable middle/fixed footer)
  - `AdminTableList` adds toolbar, pagination, and filter empty-state support
  - File browser adds per-entry action menu (`FileBrowserItemActionMenu`)
  - Global search header adds an active filter chip row with individual removal
  - New `usePendingAction` hook to prevent duplicate async submissions
  - Category browsing page: video/audio thumbnail generation, file location jump ("Go to file location")
  - Sidebar category links changed from triggering search to direct navigation
  - Category view supports infinite scrolling (100 per page)
  - Search API adds `sort_by`/`sort_order` parameters
  - Category browsing page adds a file info panel synced with list state

### Changed

- **Unified API error code protocol (Breaking)**
  - Removed backend `error_code.rs` (numeric `ErrorCode`) and `subcode.rs` (`ApiSubcode`)
  - All `*_with_subcode` helper functions renamed to `*_with_code` variants
  - `AsterError` uses `api_error_code_override()` instead of `api_error_subcode()`
  - `ApiResponse.code` field changed from numeric `ErrorCode` to string `ApiErrorCode`
  - OpenAPI schema removes `ApiSubcode` and `ErrorCode`, keeping only `ApiErrorCode`
  - `ApiErrorInfo` response contract removes `subcode` and `internal_code` fields
  - Internal storage protocol version bumped from v3 to v4, minimum supported version raised to v4 accordingly (backward incompatible)
  - `StoragePolicyCleanupRemoteNodeSnapshot` adds `last_capabilities` field (serde default)
  - Frontend fully migrated from `ErrorCode`/`ApiSubcode` to `ApiErrorCode` strings
  - `ApiError` constructor simplified to `(code, message)`, removing the old subcode wrapping
  - `useApiError` removes subcode classification logic, unified on `error.code`
  - Integration tests switched to string code assertions (`"success"`, `"auth.token_missing"`)
  - Public API examples uniformly use `code: "success"` and string error codes

- **Runtime architecture refactor (Breaking on internal API)**
  - `PrimaryAppState`/`FollowerAppState` introduce a trait system:
    - `SharedRuntimeState` for unified access to config / db / cache / storage / policy / metrics / mail
    - Specialized traits: `TaskRuntimeState`, `MailRuntimeState`, `StorageChangeRuntimeState`, `RemoteProtocolRuntimeState`
  - Service layer parameters changed from concrete types to trait bounds (`impl SharedRuntimeState`, etc.)
  - Field access changed to method calls (`state.config` → `state.config()`)
  - `PrimaryRuntimeState` split into 4 specialized traits + `TaskRuntimeState`
  - 40+ service functions accept `SharedRuntimeState` or specific sub-traits
  - `web::Data<T>` provides a blanket impl preserving API compatibility
  - `TaskRuntimeState` adds `wake_background_task_dispatcher`
  - Health checks now use `RemoteProtocolRuntimeState` for remote node tests
  - Affects 208 files, zero-cost abstraction, behavior fully identical

- **Explicit API state access**
  - All API routes use `state.get_ref()` instead of implicit Deref
  - Middleware explicitly accesses runtime config
  - Primary/follower health checks with explicit state access
  - WebDAV and remote tunnel clients updated in sync
  - Removed implicit `Deref` implementation on `PrimaryAppState`
  - Affects 44 files

- **Frontend architecture improvements**
  - `AppLayout` and `TopBar` drop the `actions` prop; the search button moved to `HeaderControls` as `mobileSearchAction`
  - `EditShareDialog`, `TagLibraryManagerDialog`, `TagManagerDialog` migrated to `ManagerDialogShell`
  - Admin pages (Tasks / Teams / Users / Invitations / External Auth) switched to `AdminTableList` + split headers/rows
  - Theme switching: custom layered animation replaces the View Transition API for cross-browser consistency
  - Theme switching adds a gloss overlay layer, cleaned up on unmount to prevent memory leaks
  - Search changed from a dialog to a full-page results browser with Enter-to-submit
  - Search header adds `onSubmitSearch` callback and `searchReady` state
  - File store removes `searchQuery`, `searchFiles`, `searchFolders`, `search()`, `clearSearch()`
  - Routes add `/search` and `/teams/:id/search`
  - Destructive actions switched to inline confirmation UI (team member removal, WebDAV account deletion, storage policy connection test, remote node ingress profile deletion, unsaved file preview changes, team archiving)
  - File card action menu hidden on desktop, making room for status indicators
  - Account menu dropdown sizing and spacing optimized for mobile viewports
  - Upload panel expand/collapse state linked to bottom padding system
  - Removed dialog-based `GlobalSearchResultRow` / `GlobalSearchResultsPanel`

- **Theme/UI system**
  - Introduced semantic z-index token system (`--z-fixed`, `--z-dialog`, `--z-dropdown`, `--z-popover`, `--z-tooltip`, `--z-alert-dialog`, `--z-toast`)
  - Fixed chrome elements (bulk action bar, sidebar, upload panel, music player) unified to `--z-fixed`
  - Full layering order: fixed (40) < dialog (50) < dropdown/popover (60) < tooltip (65) < alert-dialog (70) < toast (80)
  - File browser selection toolbar changed to an absolute-positioned overlay with `bg-card` background
  - Upload drag overlay and settings save bar unified via CSS variable tokens

- **Other engineering**
  - jemalloc configuration split per platform, with tuned Linux settings
  - Tunnel online detection now relies entirely on heartbeat timestamps
  - Mock auth server configured with a single worker to prevent races
  - Consolidated reqwest patterns in external auth tests
  - Removed duplicate database backend assertions
  - `.cargo/audit.toml` removes fixed RUSTSEC-2026-0097
  - GitHub Actions upgraded codecov-action to v7
  - MSRV raised from 1.91.1 to 1.94.0

- **Batch move performance optimization**
  - New repository methods such as `find_by_names_in_parent`, `find_by_names_in_team_parent`
  - Batch moves use `load_target_file_name_map`/`load_target_folder_name_map` for batch name conflict checks
  - Conflict detection database queries reduced from O(n) to O(1)
  - Batch moves add Unicode normalization (NFC/NFD) support to prevent false conflicts
  - Normalized query variant generation with NFD fallback lookup
  - k6 performance tests add multipart upload timing metrics (init / chunk / complete / client gap)
  - Fixed k6 API success code check to support both string `"success"` and numeric 0

### Fixed

- Frontend duplicate submission prevention (file/folder creation dialogs + `usePendingAction`)
- Frontend auth error handling: 502/503/504 gateway errors preserve cached auth state; only 401/403/token errors force logout
- `ApiError` class adds a `status` field threaded through the error chain
- `readHttpStatus` extracts status directly from the error object
- Refresh token failure sets `isAuthStale` to trigger a retry
- Fixed orphaned storage objects when cleanup fails
- Improved atomicity and consistency of delete operations
- Tunnel heartbeat reliability across polling intervals
- k6 client code formatting and success code parsing
- Theme switching resource cleanup on component unmount

### Security

- External auth URL configuration validation and specialized checks
- Local email policy prevents unintended registrations
- Invitation links use secure token hashing
- State validation before accepting an invitation
- Invitation endpoints forbid token refresh attempts
- One-time invitations update status automatically
- Mail audit sensitive fields UTF-8 safe truncation

### Testing

- New component tests: invitation dialog, invitation table, ManagerDialogShell, FileBrowserItemContextMenu, FileInfoDialog, FileThumbnail
- New z-index layering and token usage validation suite
- New overlay tests for bottom overlay offset and z-index assignment
- New sorted search result tests (files by size desc, folders by name desc)
- New submission protection tests for repeated-click scenarios
- New edge tests for 502/503/504 gateway errors in auth checks and token refresh
- New ApiError status preservation tests
- New tests for `isSessionAuthFailure` across various status codes
- New unit tests for mail audit field UTF-8 truncation
- New tunnel heartbeat tests across polling intervals
- e2e tests unified via `E2eApiResponse<T>` + `expectApiSuccess` helper + `E2E_API_SUCCESS_CODE` constant
- e2e search flow updated to "submit first, then navigate to the results page"
- Batch moves add Unicode normalization conflict detection tests + index verification tests
- Batch empty-request error code exact-match tests (BadRequest)
- OpenAPI tests verify all `ApiResponse` schemas reference `ApiErrorCode`
- Migration tests add `seed_user_invitation_fixture` and `seed_tag_fixture` assertions
- OIDC test assertions updated to `bad_request` instead of the old `wopi.public_site_url_required`

### Documentation

- **API error code v4 migration**
  - Public API examples remove old numeric error codes, `error.code`/`error.subcode`/`error.internal_code`
  - Response examples unified to `code: "success"` + string error codes
  - Removed error code range table and numeric-to-string mapping table
  - Error handling docs focus on the top-level `code` field
  - Internal storage protocol v4 documented (backward incompatible)
  - Deployment/troubleshooting docs consistently reference the `code` field
  - GitHub Actions workflow triggers changed to releases
  - Logging docs reference the API response `code` field
  - Error code contract notes merged into a single authoritative source

### Notes

- This version is the first pre-release of `0.3.0` (`alpha.1`)
- **Breaking Change**: API error code protocol
  - Public error codes changed to string `ApiErrorCode`; the former `ErrorCode` (numeric) and `ApiSubcode` have been removed
  - `ApiErrorInfo` removes `subcode` and `internal_code` fields
  - Internal storage protocol v3 → v4, **minimum supported version raised to v4**; v3 nodes cannot interoperate with v4 primary/follower nodes
- **Breaking Change**: internal API
  - Runtime trait system replaces concrete type parameters (affects internal service-layer calls, not the HTTP API)
  - API routes use explicit `state.get_ref()` instead of implicit Deref (compile-time errors, no runtime impact)
- **Breaking Change**: removed deprecated endpoints
  - `/api/v1/public/branding` removed; use `/public/frontend-config` instead
- **Breaking Change**: thumbnail capability response
  - The flat `PublicThumbnailSupport.extensions` field removed, replaced by `image_thumbnail.extensions` / `audio_thumbnail.extensions` capability fields
- New database migrations:
  - `m20260607_000001_add_user_invitations` — user invitation table
  - `m20260608_000001_add_tags` — tag system tables
- Pre-release versions are recommended for test environments; production deployment is not advised
- Client integrations need to update in sync:
  - Parse the `code` field as a string rather than a number
  - Deprecate `error.subcode` / `error.internal_code` parsing logic
  - Use `image_thumbnail.extensions` / `audio_thumbnail.extensions` for thumbnail capability queries
  - Use the `/api/v1/invitations` endpoint family for the invitation flow
  - Use the `/api/v1/tags` endpoint family for tag operations

---

**Statistics**:
- 675 files changed, 29,067 insertions(+), 11,921 deletions(-)
- 20 commits
- Rust Edition 2024, MSRV 1.94.0

## [v0.2.7] - 2026-06-06

### Release Highlights

**AsterDrive `0.2.7` focuses on enterprise-grade login, image preview, WebDAV protocol compliance, and storage driver diversification.** This version adds support for four major OAuth2/OIDC providers — GitHub, Google, Microsoft, and QQ — enabling quick single sign-on access; fully implements fullscreen image preview, zoom, rotation, and native AVIF support; substantially improves WebDAV RFC 4918 compliance with multi-active locking, recursive conflict detection, and shared locks; adds a Tencent Cloud COS storage driver with native media processing and thumbnail generation; and continues strengthening enterprise features with email policies, Passkey login control, and team-level policy group migration.

- **OAuth2/OIDC external authentication** — Adds four major providers: GitHub, Google, Microsoft, and QQ, supporting single sign-on and Microsoft tenant management
- **Complete image preview system** — Fullscreen viewing, zoom, rotation, navigation, native AVIF support, and browser capability detection
- **WebDAV RFC 4918 compliance** — Multi-active locking, recursive conflict detection, shared lock support, and improved If header handling
- **Tencent Cloud COS driver** — Native media metadata and thumbnail generation support, S3 addressing style configuration
- **Enterprise authentication policies** — Local email allowlist/blocklist, Passkey login control, policy group team extension
- **Performance and stability** — jemalloc memory management, thumbnail cache optimization, concurrency limits, heartbeat isolation

### Added

- **OAuth2/OIDC external authentication**
  - New GitHub OAuth provider with automatic verified primary email extraction
  - New Google OIDC provider with standard configuration support
  - New Microsoft Entra provider with tenant management and custom configuration
  - New QQ Connect OAuth2 provider
  - Provider-specific option support, Microsoft tenant value normalization
  - Prevents URL configuration overrides for specialized providers
  - Audit logging of external authentication operations
  - Frontend provider configuration forms and settings preflight

- **Image preview system**
  - Fullscreen image viewer with zoom, pan, and rotation
  - Image preview navigation with previous/next switching
  - Native AVIF format support
  - Browser rendering capability detection, graceful HEIF/HEIC fallback
  - Image preview policy configuration
  - Per-preview thumbnail capability detection
  - Lazy generation optimization, no processing before a cache hit
  - Simplified preview state management and zoom logic

- **WebDAV protocol enhancements**
  - Full RFC 4918 compliant implementation
  - Multi-active locking support with validity trimming
  - Lock conflict detection for recursive operations
  - Shared WebDAV lock support
  - Case-insensitive handling of the If header Not keyword
  - Centralized HTTP response builders
  - Extracted request-origin helper functions
  - Consolidated context structures

- **Storage driver expansion**
  - Tencent Cloud COS driver implementation
    - Native media metadata support
    - Native thumbnail generation
    - Private URL addressing
  - S3 addressing style configuration (virtual-hosted, path-style), with Tencent COS compatibility
  - Remote storage driver modularization (extracted submodule structure)
  - Blob migration multipart upload support
  - Storage usage tracking for files and folders

- **Enterprise authentication policies**
  - Local email whitelist/blacklist support
  - Passkey login policy control
  - Policy group migration extended to team assignments

- **Performance and operations**
  - jemalloc memory allocator support (optional feature)
  - Maximum concurrency limit and thumbnail cache size validation
  - Removed thumbnail metadata pre-check, optimized range reads
  - Heartbeat moved to a standalone task to prevent SQLite deadlocks
  - Cancellable context for storage operations
  - File copy logic extracted and optimized

### Changed

- Root crate version upgraded from `0.2.6` to `0.2.7`
- **WebDAV architecture refactor**
  - Centralized HTTP response builders (`webdav/responses.rs`)
  - Protocol handling moved to a standalone module (`webdav/protocol.rs`)
  - Extracted request origin and context structures
- **Thumbnail processing**
  - Removed metadata pre-check, read cache directly
  - Range read optimization and Bytes type improvements
- **Storage drivers**
  - Remote driver split into submodules (`remote/protocol.rs`, `remote/client.rs`, etc.)
  - Simplified Blob migration function signatures
  - Cache warmup disabled for maintenance tasks
- **Task scheduling**
  - Heartbeat logic moved to a standalone background task
  - Prevented SQLite connection deadlocks
- **Type safety**
  - Improved stream handling and type safety
  - Refined external auth provider types

### Fixed

- Fixed WebDAV DAV namespace prefix declaration (RFC 4918 PROPFIND)
- Fixed image preview logic and Microsoft OIDC legacy issuer handling
- Prevented specialized OAuth providers from unexpected URL configuration overrides
- Fixed orphaned storage objects on cleanup failure
- Improved atomicity and consistency of delete operations

### Security

- External auth URL configuration validation and specialization checks
- Local email policies prevent unintended registrations

### Testing

- Added comprehensive WebDAV protocol tests (3929+ lines)
- Added OAuth2/OIDC integration tests (covering all providers)
- Added storage migration tests (911+ lines)
- Added task management tests (431+ lines)
- Added dedicated tests for the WebDAV lock system
- Frontend added tests for admin settings, preview, and external auth configuration

### Documentation

- **New documentation**
  - Capacity planning and deployment guidance (`deployment/capacity-planning.md`)
  - Feature guide modularization (auth, files, preview, upload, operations)
  - Detailed local storage guide
  - Tencent Cloud COS configuration and usage
  - Complete architecture design documentation
  - jemalloc profiling guide
- **Updated documentation**
  - API docs synced with all new providers and storage drivers
  - External auth documentation fully rewritten (provider setup, configuration, troubleshooting)
  - WebDAV and WOPI docs reflect RFC compliance improvements
  - Configuration docs add auth and storage driver options

### Notes

- This release is the `0.2.7` feature and ecosystem expansion release
- New database migrations:
  - `m20260604_000001_allow_shared_webdav_locks` — shared WebDAV lock support
  - `m20260606_000001_add_external_auth_provider_options` — external auth provider options
- **Breaking Change**: API endpoint renames
  - Policy group migration endpoint: `POST /admin/policy-groups/{id}/migrate-users` → `POST /admin/policy-groups/{id}/migrate-assignments`
  - Reason: the endpoint was extended from migrating only users to migrating both user and team assignments
  - Request type: `MigratePolicyGroupUsersReq` → `MigratePolicyGroupAssignmentsReq`
  - Response type: `PolicyGroupUserMigrationResult` → `PolicyGroupAssignmentMigrationResult`
  - Response adds the `affected_teams` field
- **Breaking Change**: external auth API adjustments
  - Added 4 OAuth2/OIDC provider types (GitHub, Google, Microsoft, QQ)
  - Provider configuration adds the `options` field
  - Microsoft provider supports tenant configuration (`tenant_id` normalization)
- **Breaking Change**: WebDAV protocol improvements
  - Multi-active locking and shared lock database schema changes
  - Resources allow multiple shared locks, requiring explicit release via `lock_token`
  - Improved WOPI preview lifecycle management
- **Breaking Change**: email policy validation
  - Local email whitelist/blacklist accepts ASCII domains only
  - Rejects Unicode domains (including punycode)
  - Email validation requires exactly one `@` separator
  - Improved fault tolerance (automatic whitespace trim, skip invalid entries)
- **Breaking Change**: Passkey policy
  - New `passkey_login_policy` configuration option
  - Defaults to enabled when the field is missing in older databases
- Strongly-typed API clients should be regenerated to sync external auth, storage driver, WebDAV, and policy group migration interfaces
- Docker users can use the jemalloc profiling variant for memory profiling
- Custom client implementations need to update references to the `migrate-users` endpoint and handle the new `affected_teams` field

---

**Statistics**:
- 529 files changed, 49,170 insertions(+), 8,301 deletions(-)
- 46 commits
- Rust Edition 2024, MSRV 1.94.0

## [v0.2.6] - 2026-06-02

### Release Highlights

**AsterDrive `0.2.6` focuses on the aria2 offline download engine, background task graceful shutdown, custom configuration visibility, and follower node audit observability.** This release adds aria2 external download engine support with resume capability, multi-connection concurrency, and built-in engine fallback; the background task system introduces a graceful shutdown mechanism, granting a 30-second grace period for tasks to exit safely on service restart; custom configuration supports private / authenticated / public three-level visibility control so the frontend can expose configuration as needed; follower node storage operations now have complete audit log and tracing coverage.

- **aria2 offline download engine** — added aria2 external engine support with RPC calls, resume capability, multi-connection, speed limits, probing, and fallback to the built-in engine
- **Background task graceful shutdown** — a cancellation signal is sent on service shutdown; tasks support synchronous checkpoint detection and asynchronously interruptible sleep, with forced termination after a 30-second grace period
- **Custom configuration visibility control** — system configuration adds a visibility field supporting private / authenticated / public three-level visibility, with caching and Vary headers on the public API
- **Follower node audit logs** — added 8 follower-specific audit actions covering binding sync, object read/write/delete, and Ingress Profile management
- **WOPI RSA security refactor** — `rsa` replaced with `ring` in production, public keys gain constraint validation, test keys are generated at runtime
- **English developer documentation** — added complete English REST API, architecture, module design, and testing documentation
- **Admin console version badge easter egg** — ↖(^ω^)↗

### Added

- **aria2 offline download engine**
  - Added the `Aria2` download engine, calling an external aria2 process via RPC (`aria2.addUri`, `aria2.tellStatus`)
  - Resume support: persists `gid` and `processing_token` to `runtime_json`, restored after restart
  - Multi-connection download support: `split` shard count, `max_connection_per_server` maximum connections per server
  - Minimum speed limit support: `lowest_speed_limit_bytes_per_sec`, automatically retries below the threshold
  - Added RPC probing: `probe_aria2_rpc` tests connectivity and returns the aria2 version
  - Added engine registry architecture: `offline_download_engine_registry_json`, supporting multi-engine priority ordering and chained fallback
  - Automatically falls back to the built-in engine on aria2 failure, and cleans up aria2 runtime state
  - Docker Compose adds an optional `aria2` service (`p3terx/aria2-pro`), started with `--profile aria2`
  - Added configuration options such as `offline_download_engine_registry_json`, `offline_download_aria2_rpc_url`, `offline_download_aria2_rpc_secret`
  - Frontend adds the `OfflineDownloadEngineRegistryEditor` component, supporting visual engine management, enable/disable, priority drag-and-drop, and RPC connectivity testing
  - Added `offline_download` documentation (Chinese and English), covering engine configuration and Docker deployment
- **Background task graceful shutdown**
  - Added `TaskExecutionContext`, uniformly wrapping `TaskLeaseGuard` and `shutdown_token`
  - Provides `ensure_active()` synchronous checkpoint, `sleep_or_shutdown()` asynchronously interruptible sleep, and `shutdown_requested()` async waiting
  - Compression tasks (`archive/compress.rs`) call `context.ensure_active()` before and after `spawn_blocking`
  - The task dispatcher checks `shutdown_token.is_cancelled()` every loop iteration
  - The task executor's outer `select!` monitors both the business flow and heartbeat/lease
  - All workers of system periodic tasks (`tasks.rs`) listen to `shutdown_token.cancelled()`
  - On shutdown, `release_task_for_shutdown()` releases the lease of running tasks back to the `Retry` state, avoiding marking them as failed
  - Added the `TaskWorkerShutdownRequested` error code, distinguishing normal shutdown, lease loss, and renewal timeout
- **Custom configuration visibility control**
  - The `system_config` table adds a `visibility` field (`private` / `authenticated` / `public`), defaulting to `private`
  - Added the `idx_system_config_visibility` index
  - Built-in configuration cannot have its visibility modified; only `custom.*` custom configuration can
  - Sensitive configuration values are redacted to `***REDACTED***` in API responses
  - Added `GET /api/v1/public/custom-config`: anonymous returns `public`, authenticated returns `public` + `authenticated`
  - Anonymous responses use `Cache-Control: public, max-age=60`, authenticated responses use `Cache-Control: private, max-age=60`
  - Added `Vary` header handling for public configuration responses
  - Added 5 integration tests and E2E test coverage
- **Follower node audit logs**
  - Added 8 follower-specific audit actions: `FollowerBindingSync`, `FollowerObjectRead`, `FollowerObjectWrite`, `FollowerObjectDelete`, `FollowerObjectCompose`, `FollowerIngressProfileCreate`, `FollowerIngressProfileUpdate`, `FollowerIngressProfileDelete`
  - The follower node initializes `global_audit_log_manager` at startup
- **English developer documentation**
  - Added `developer-docs/en/` complete English documentation covering the REST API (admin, auth, batch, files, folders, health, public, shares, tasks, teams, trash, webdav, wopi), architecture, module design, and testing guides
  - Original Chinese documentation moved to `developer-docs/zh-CN/`
- **Admin console version badge easter egg**
  - ↖(^ω^)↗
- **Test coverage**
  - Added tests for task dispatch, archive validation, and offline download paths
  - Added tests for offline download path length and permission fixes
  - Added integration tests and E2E tests for public custom configuration visibility
  - Added follower node network topology deployment documentation

### Changed

- Root crate version upgraded from `0.2.5` to `0.2.6`
- **WOPI RSA security refactor**
  - Production dependency `rsa` removed, `ring` added
  - WOPI proof verification uses `ring::signature::RSA_PKCS1_2048_8192_SHA256`
  - Added RSA public key constraint validation: modulus 2048-8192 bits and odd, exponent an odd number of 3 or greater
  - Tests keep `rsa 0.9` only for runtime test key generation (dev-dependencies)
- **Dependency upgrades**
  - `jsonwebtoken` switched from `rust_crypto` to `aws_lc_rs`
  - `sea-orm` upgraded from `2.0.0-rc.38` to `2.0.0-rc.40`
- **Sensitive configuration redaction**
  - `SystemConfig` serialization automatically replaces sensitive values with `***REDACTED***`
  - Sensitive values in audit logs are also redacted
  - Audit logs record `visibility` and `prior_visibility` changes
- **Compression task error handling**
  - Improved error handling in the archive compression workflow, unified via `TaskExecutionContext`

### Fixed

- Fixed aria2 output directory permission issues
- Fixed migration timestamp correction from `000001` to `000002`
- Fixed static RSA test keys, switched to runtime generation (reducing test file size and key leakage risk)

### Notes

- This release is the `0.2.6` feature enhancement release
- New database migrations:
  - `m20260601_000001_add_system_config_visibility` — adds the `visibility` field to the `system_config` table
  - `m20260601_000002_add_background_task_runtime_json` — adds the `runtime_json` field to the `background_tasks` table
- **Breaking Change**: API changes
  - Added `GET /api/v1/public/custom-config` public custom configuration endpoint
  - Sensitive values in `SystemConfig` responses may appear as `***REDACTED***`
- **Breaking Change**: dependency changes
  - Production builds no longer depend on the `rsa` crate, replaced by `ring`
- New runtime configuration options:
  - `offline_download_engine_registry_json` — engine registry
  - `offline_download_aria2_rpc_url` / `offline_download_aria2_rpc_secret` / `offline_download_aria2_request_timeout_secs` / `offline_download_aria2_split` / `offline_download_aria2_max_connection_per_server` / `offline_download_aria2_lowest_speed_limit_bytes_per_sec` — aria2-specific configuration
- Docker users who need aria2 offline downloads should use `docker compose --profile aria2 up -d`
- Strongly-typed API clients should be regenerated to sync public custom configuration and offline download engine interfaces

---

**Statistics**:
- 209 files changed, 15,033 insertions(+), 2,223 deletions(-)
- 41 commits
- Rust Edition 2024

## [v0.2.5] - 2026-06-01

### Release Highlights

**AsterDrive `0.2.5` focuses on offline downloads, structured audit log presentation, and admin console settings UX improvements.** This release adds a background task for HTTP/HTTPS link offline downloads with rate limiting, concurrency control, and security validation; audit logs introduce a structured presentation layer with configurable action scope filtering and grouped display; the admin settings page refactors category metadata, with runtime configuration collapsed by default to improve browsing efficiency.

- **Offline download** — added an HTTP/HTTPS link import background task supporting personal and team workspaces, with built-in rate limiting, concurrency control, URL security validation, and file size limits
- **Structured audit log presentation** — added the `AuditPresentation` structured presentation type, supporting grouping by action and configurable action scope filtering; audit responses add a `presentation` field
- **Admin console settings page refactor** — category metadata extracted into a standalone module and lookup table, settings page navigation and loading logic split, runtime configuration sections collapsed by default (except background tasks)
- **Auth error code enhancements** — added a dedicated structured error code for disabled registration
- **Documentation and project conventions updates** — README adds product screenshots, project commit language standardized to English

### Added

- **Offline download (HTTP/HTTPS link import)**
  - Added `POST /api/v1/tasks/offline-download` and `POST /api/v1/teams/{team_id}/tasks/offline-download` endpoints
  - Added the `OfflineDownload` background task type, supporting streaming downloads, resume capability, and progress tracking
  - Added URL security validation: forced HTTPS (except local development), domain blacklist, port restrictions, protocol whitelist
  - Added rate limiting: per-user/team limits on concurrent download count and request frequency
  - Added speed limit and concurrency control configuration: supports global and per-task bandwidth and concurrency limits
  - Added the `offline_download` audit action type, recording the download initiator and target URL
  - Added `task_service/offline_download.rs` (1052 lines) and `spec/offline_download.rs` (65 lines)
  - Frontend task presentation adds dedicated summaries and icon mappings for offline downloads
- **Structured audit log presentation layer**
  - Added the `AuditPresentation` type, supporting grouping by action, counts, and nested detail display
  - Audit log responses add a `presentation` field (optional structured presentation data)
  - Added the `audit_log_recorded_actions` runtime configuration, supporting a customizable action scope for audit recording
  - Added `audit_service/presentation.rs` (298 lines), implementing audit presentation formatting logic
  - Added the `server_start` and `server_shutdown` audit action types
  - Frontend audit formatting library extended to parse presentation fields (`lib/audit.ts`, 131 lines changed)
  - Added complete documentation for the audit presentation layer and configuration fields (Chinese and English)
  - Added audit presentation edge-case handling: missing enum groups, array parameter compatibility fixes
- **Admin console settings UX improvements**
  - Runtime configuration sections collapsed by default, with only background tasks kept expanded, reducing visual noise on the page
  - Added the `AdminSettingsLoadedContent` component, separating loaded content display for configuration
  - Added `adminSettingsCategoryMetadata.ts` (228 lines) and tests (211 lines), centrally maintaining category metadata

### Changed

- Root crate version upgraded from `0.2.4` to `0.2.5`
- **Admin console settings architecture refactor**
  - Category metadata consolidated from scattered definitions into a unified lookup table (`adminSettingsCategoryMetadata.ts`)
  - Settings page data loading split into `useAdminSettingsData` and standalone content components
  - Configuration item schema adds an `options` field, supporting dropdown option types
- **Configuration module cleanup**
  - Renamed settings categories, split file-handling configuration into a standalone module
  - Cleaned up `config/admin` and `config/settings` related structures
- **Audit log query enhancements**
  - Audit queries support serialization and deserialization of the `presentation` field
  - Extended the audit action enum with `server_start`, `server_shutdown`, `offline_download`
- **Documentation Updates**
  - Added product screenshots to README and README.zh
  - Switched project commit language to English
- **Task Scheduling**
  - Updated task scheduling lane logic to support per-channel scheduling for the `offline_download` task type
  - Extended the task registry and type system with the offline download spec

### Fixed

- Fixed formatting failures in audit display caused by missing enum groups
- Fixed improper handling of array parameters in audit display
- Fixed async data assertion stability in admin settings page tests

### Notes

- This is the `0.2.5` feature enhancement release
- No new database migrations
- **Breaking Change**: API changes
  - Audit log responses add a `presentation` field (optional)
  - `AuditAction` enum adds `server_start`, `server_shutdown`, `offline_download`
  - New `POST /api/v1/tasks/offline-download` and team-scoped endpoints
  - Config schema adds an `options` field
- Regenerating strongly typed API clients is recommended to sync offline download endpoints, audit presentation fields, and new audit actions
- New runtime configuration options:
  - `audit_log_recorded_actions` — controls the scope of actions recorded in the audit log
  - Rate limit and concurrency control configuration for offline downloads

---

**Statistics**:
- 177 files changed, 7,627 insertions(+), 1,444 deletions(-)
- 17 commits
- Rust Edition 2024

## [v0.2.4] - 2026-05-31

### Release Highlights

**AsterDrive `0.2.4` focuses on generic OAuth2 external authentication, team WebDAV accounts, the background task spec system, and frontend architecture refactoring.** This release adds a generic OAuth2 external authentication provider, supporting standard OIDC providers such as Logto and Keycloak; WebDAV gains team workspace account support and Range requests; the background task system introduces a type-safe spec layer that unifies task creation, encoding/decoding, and presentation logic; the frontend routing system and several admin pages were refactored into decomposed components.

- **Generic OAuth2 External Authentication** — New Generic OAuth2 provider supporting PKCE, public clients, and multiple client authentication methods, with default scopes including openid for compatibility with providers like Logto
- **Team WebDAV Accounts** — Team workspaces support independent WebDAV account management, including create/delete/audit logging
- **WebDAV Enhancements** — Support for HTTP Range requests, fixed false lock detection on Finder's lock-holding PUT, and module splitting for better maintainability
- **Background Task Spec System** — Introduced the `BackgroundTaskSpec` trait and `TypedTaskCreate` builder, unifying task type declaration, payload encoding/decoding, and presentation logic
- **Frontend Architecture Refactoring** — Decomposed the routing system into components; team management/shares/WebDAV/external authentication pages extracted controller hooks
- **Dependency Upgrades** — rsa 0.10, xmltree replacing quick-xml

### Added

- **Generic OAuth2 External Authentication Provider**
  - New `GenericOAuth2` provider driver (711 lines), supporting manually configured authorization, token, and userinfo endpoints
  - Supports the PKCE flow, public client authentication (no client_secret), and the ClientSecretPost authentication method
  - Default scopes include `openid email profile`, compatible with OIDC providers such as Logto
  - New URL validation module `url.rs`, unifying HTTPS enforcement and localhost exemption logic
  - Frontend adds OAuth2 icon assets and a configuration form
  - New generic OAuth2 provider configuration documentation (English and Chinese)
  - New OAuth2 integration tests (490+ lines)
- **Team WebDAV Accounts**
  - New `GET/POST/DELETE /api/v1/teams/{team_id}/webdav-accounts` endpoints
  - New `WebdavAccountTable`, `WebdavAccountRow`, `WebdavCreateAccountDialog` frontend components
  - Team WebDAV account audit logging recorded independently
  - New WebDAV account integration tests (491 lines)
- **Background Task Spec System**
  - New `BackgroundTaskSpec` trait, unifying declaration of task type, payload/result encoding/decoding, steps, lane, and max attempts
  - New `TypedTaskCreate` builder, a type-safe task creation interface
  - New `TaskPresentation` type, supporting structured task status presentation messages
  - New `src/services/task_service/spec/` module and `registry.rs` (257 lines)
  - New `presentation.rs` (538 lines), with the backend emitting presentation text directly
- **WebDAV Enhancements**
  - Support for HTTP Range requests (partial content download)
  - WebDAV module split into locks/props/resources/transfer/file/fs submodules
  - New WebDAV integration tests (785 lines)
- **Frontend Components**
  - New `WorkspaceOutlet`, `AdminRoute`, `LoginGuard`, `ProtectedRoute` routing components
  - New `MyShareCard`, `MyShareStatusBadge`, `MySharesSelectionBar` sharing components
  - New `useAdminExternalAuthPageController` (743 lines) external authentication page controller
  - Workspace switcher restores dropdown expanded state after route changes

### Changed

- Root crate version upgraded from `0.2.3` to `0.2.4`
- **Frontend Routing System Refactoring** — Split from a single routing file into multiple dedicated routing components
- **Team Management Page Refactoring** — `TeamManageDialog.tsx` split from 658 lines into view/shell/actions/state modules
- **My Shares Page Refactoring** — `MySharesPage.tsx` split from 381 lines into multiple presentation components
- **WebDAV Accounts Page Refactoring** — Simplified from 462 lines to 267 lines, extracting shared components
- **External Authentication Page Refactoring** — Extracted controller hook, simplifying the view layer
- **Module File Structure Refactoring** — 20+ single-file modules converted to directory modules (cli, types, runtime/startup, etc.)
- **Task Presentation Logic** — Replaced frontend parsing with backend structured presentation messages, improving fault tolerance
- Renamed `MfaFactorMethod` to `MfaPersistentFactorMethod` for clearer semantics
- Unified archive format detection logic, removing lenient MIME-based matching
- Dependency upgrades: rsa 0.9→0.10, xmltree replacing quick-xml, aws-sdk-s3 1.134, nom-exif 3.6

### Fixed

- Fixed an issue where Finder's lock-holding PUT was misjudged as locked by someone else
- Fixed current-user indicator display on the WebDAV account management page
- Fixed boundary checks for WebDAV team account features
- Enhanced diagnostic information in OAuth2 error responses
- Fixed workspace search keyboard event handling
- Fixed team management pagination and navigation issues

### Notes

- This is the `0.2.4` feature enhancement release
- New database migrations:
  - `m20260530_000001_add_webdav_account_team_scope`
- **Breaking Change**: API changes
  - Task info adds a `presentation` field (structured presentation message)
  - External authentication providers add the `GenericOAuth2` type
  - Teams add WebDAV account management endpoints
- **Breaking Change**: Dependency changes
  - `rsa` upgraded to 0.10 (breaking API)
  - `quick-xml` replaced with `xmltree`
- Regenerating strongly typed API clients is recommended to sync external authentication and team WebDAV endpoints

---

**Statistics**:
- 270 files changed, 16,436 insertions(+), 9,368 deletions(-)
- 40 commits
- Rust Edition 2024

## [v0.2.3] - 2026-05-29

### Release Highlights

**AsterDrive `0.2.3` focuses on reverse tunneling for remote storage, Blob maintenance tasks, and consolidation of archive capabilities.** This release adds a reverse tunnel transport mode, allowing remote nodes without a public IP to connect via outbound connections; a new Blob maintenance background task supports orphan cleanup, reference count reconciliation, and health checks; the archive service continues to focus on ZIP preview and extraction, preserving abstraction boundaries for adding more formats later; task presentation logic is fully rebuilt with a new 688-line task presentation module supporting runtime name mapping for 20+ system tasks and internationalization of 70+ statuses; the remote storage protocol transport layer is refactored, unifying request/response encoding and streaming frame handling; database queries are optimized, replacing hand-written SQL concatenation with the SeaORM query builder.

- **Reverse Tunnel Transport Mode** — Remote nodes support Direct/ReverseTunnel/Auto transport modes; nodes without a public IP can actively connect to the primary via reverse tunnel
- **Archive Capability Consolidation** — Continued support for ZIP preview and in-browser extraction; 7z support was evaluated during development but not included in this release, avoiding `crc64fast` i686 build failures and FFI/GPL route risks
- **Blob Maintenance Task** — New `BlobMaintenance` background task type supporting scanning, checking, reference reconciliation, and orphan cleanup
- **Task Presentation Refactoring** — New `taskPresentation.ts` module (688 lines), supporting runtime task name mapping and status internationalization
- **Remote Storage Protocol Refactoring** — Transport layer refactor with new `transport.rs` and `runtime.rs`, unifying request/response encoding and streaming frame handling
- **Database Query Optimization** — Replaced hand-written SQL concatenation with the SeaORM query builder, improving security and maintainability
- **Admin File Info Enhancements** — Added creator info, Blob reference counts, health status, and uploader info
- **Frontend Page Refactoring** — Remote node page logic extracted into a controller hook (637 lines); tasks page, admin files page, and admin tasks page fully refactored

### Added

- **Reverse Tunnel Transport Mode**
  - New `/internal/remote-tunnel` API endpoints (poll/complete/connect)
  - New `RemoteNodeTransportMode` enum (Direct/ReverseTunnel/Auto)
  - New tunnel client implementation (1456 lines), supporting multi-channel streaming, automatic reconnection, and backpressure handling
  - New tunnel server implementation (1160+ lines of tests), including authentication, frame encoding, registry management, and persistent polling
  - Supports both WebSocket and HTTP long-polling transports
  - Database adds `managed_followers.transport_mode/tunnel_last_error/tunnel_last_seen_at` fields
  - Frontend adds the `TransportModeSelector.tsx` component with accessibility support
  - New `useAdminRemoteNodesPageController.ts` hook (637 lines), extracting remote node page logic
- **Archive Format Capability Declaration**
  - New `ArchiveFormat` abstraction, unifying ZIP preview and extraction format management, preserving boundaries for future formats
  - Frontend adds `archivePreviewFormatCapabilities.ts`, centrally maintaining archive preview format capabilities
  - Filters unsupported formats (e.g., RAR, 7z) from preview options, avoiding exposing unavailable entry points in the frontend
- **Blob Maintenance Task**
  - New `BackgroundTaskKind::BlobMaintenance` task type
  - New `blob_maintenance.rs` service (767 lines), supporting scanning, checking, reference reconciliation, and orphan cleanup
  - Batch processing (1000 items per batch), progress tracking, transaction support
  - New `POST /admin/files/blobs/maintenance` API endpoint
  - New `AdminFileBlobHealth` enum (Healthy/Orphan/RefCountMismatch/CleanupClaimed)
  - Database adds the `storage_migration_checkpoints.renamed_opaque_blobs` field
- **Task Presentation Enhancements**
  - New `taskPresentation.ts` module (688 lines), runtime task name mapping and status internationalization
  - Supports display name mapping for 20+ system tasks
  - Supports internationalized text for 70+ statuses
  - New `tasks/common.json` and `tasks/status-kind.json` internationalization files (English and Chinese)
  - New `steps.rs` module (44 lines), a unified step status management interface
- **Storage Policy Enhancements**
  - New `StoragePolicySummaryFields.tsx` component (165 lines) for storage policy summary display
  - New `S3DownloadStrategyField.tsx` and `S3UploadStrategyField.tsx`, separating S3 policy fields
- **Admin File Info Enhancements**
  - `AdminFileInfo` adds a `created_by` field (creator user summary)
  - `AdminFileBlobInfo` adds `file_ref_count/version_ref_count/actual_ref_count` fields (reference counts)
  - `AdminFileBlobInfo` adds a `health` field (Blob health status)
  - `AdminFileBlobInfo` adds `uploader_count/uploaders` fields (uploader info)
  - `AdminFileBlobReferenceFile` adds `created_by_*` fields
- **User Identity Components**
  - New `UserIdentityGroup.tsx` component (49 lines) for user identity display

### Changed

- Root crate version upgraded from `0.2.2` to `0.2.3`
- **Remote Storage Protocol Refactoring**
  - New `transport.rs` (770 lines), unified request/response encoding and streaming frame handling
  - New `runtime.rs` (179 lines), async task management and connection lifecycle management
  - Refactored `client.rs` (536 lines changed), supporting multiple transport modes and improved error handling
  - Enhanced `errors.rs` (72 lines changed) with new tunnel-related error types
- **Archive Service Refactoring**
  - Extracted `format.rs` (format management), `io.rs` (I/O operations), and `scan.rs` (scanning logic)
  - Refactored the `zip_scan/` module, improving Zip scanning performance
  - Improved `archive_preview_service/`, preserving ZIP original manifest cache rebuild and legacy cache compatibility
- **Database Query Optimization**
  - Replaced hand-written SQL concatenation with the SeaORM query builder (`apply/copy.rs`)
  - Optimized Blob query performance (`blob/lookup.rs`, 177 lines changed)
- **Task Service Refactoring**
  - Storage migration tasks support opaque key rename counting (`storage_migration.rs`, 210 lines changed)
  - Task dispatch supports the new blob maintenance task type (`dispatch/execute.rs`, 41 lines changed)
  - Extracted extraction staging logic (`archive/extract/staging.rs`); archive extraction continues to reuse the ZIP safety validation and staged import paths
- **Frontend Page Refactoring**
  - `AdminRemoteNodesPage.tsx` simplified from 588 lines to 72 lines, with logic extracted into a controller hook
  - `TasksPage.tsx` improved task presentation logic and internationalization support (137 lines changed)
  - `AdminFilesPage.tsx` adds Blob health status display (846 lines changed)
  - `AdminTasksPage.tsx` supports new task types and improved filtering and sorting (577 lines changed)
  - `AdminOverviewPage.tsx` adds a background tasks section and a system health status banner (146 lines changed)
- **Configuration and Documentation Updates**
  - Runtime configuration documentation adds tunnel-related configuration details (`runtime.md`, 40+ lines changed)
  - Storage driver configuration updated (`storage.md`, 43 lines changed)
  - API documentation adds Blob maintenance and remote tunnel API docs

### Fixed

- Fixed archive preview legacy cache compatibility issues, handling the `zip_utf8` field alias and missing fields
- Fixed reverse tunnel streaming error handling and stream abort logic
- Fixed storage migration blob summary building, replacing hand-written SQL with the SeaORM query builder

### Notes

- This is the `0.2.3` feature enhancement release
- 7z in-browser preview and extraction was evaluated during the `0.2.3` development cycle but ultimately not included in this release:
  - Few pure Rust options exist, and current candidates indirectly trigger `crc64fast` i686 build failures
  - The FFI/xz binding route carries GPL licensing risks
  - `.7z` files still display as a regular archive file type, but no archive preview or in-browser extraction entry points are exposed
  - Issue #206 has been marked `not planned`; it will only be re-evaluated when dependency licensing, cross-platform builds, and maintenance costs are all controllable
- New database migrations:
  - `m20260528_000001_add_storage_migration_opaque_rename_count`
  - `m20260529_000001_add_remote_node_transport`
- **Breaking Change**: Database schema changes
  - `managed_followers` table adds `transport_mode/tunnel_last_error/tunnel_last_seen_at` fields
  - `storage_migration_checkpoints` table adds a `renamed_opaque_blobs` field
  - Database migrations must be run before startup
- **Breaking Change**: API changes
  - New `blob_maintenance` task type; clients need to update the task type enum
  - `RemoteNodeInfo` adds a `transport_mode` field (defaults to "direct")
  - `AdminFileInfo` adds a `created_by` field (optional)
  - `AdminFileBlobInfo` adds multiple reference count and health status fields

---

**Statistics**:
- 271 files changed, 23,475 insertions(+), 3,416 deletions(-)
- 33 commits
- Rust Edition 2024, MSRV 1.91.1

## [v0.2.2] - 2026-05-28

### Release Highlights

**AsterDrive `0.2.2` focuses on storage policy migration, admin observability, error code system refactoring, and frontend performance optimization.** This release adds a complete storage policy data migration workflow with resume-from-checkpoint and failure recovery; the admin console adds files and Blob observability pages with multi-dimensional filtering, sorting, and storage usage inspection; `ApiErrorCode` replaces `ApiSubcode` as the stable error identifier, improving client error handling; frontend startup performance is optimized by deferring non-critical configuration and SSE connections; task cards are refactored into a two-section summary + expandable detail layout, improving usability with large numbers of tasks.

- **Storage Policy Data Migration** — New complete migration workflow (select source/target policy → pre-check → create task → resume → complete), supporting resume-from-checkpoint and failure recovery for large-scale data migrations
- **Admin Observability Pages** — New files and Blob observability pages supporting multi-dimensional filtering, sorting, and pagination; migration dialog adds a "Check Plan" button showing pre-check results
- **Error Code System Refactoring** — Introduced `ApiErrorCode` to replace `ApiSubcode`; responses add a `code` field, the frontend reads `error.code` first while remaining backward compatible with `error.subcode`
- **Frontend Performance Optimization** — Non-critical configuration deferred to idle time; SSE connections gain an initial delay; upload session recovery deferred; folder tree switching prioritizes cache reuse
- **Task Card Refactoring** — Redesigned into a two-section summary + expandable detail layout, with key information at a glance and details on demand
- **Metrics Image Builds** — Docker build matrix adds a `metrics` variant; image tags uniformly add the `-metrics` suffix
- **Refresh Token Error Handling Improvements** — Added expired-token reuse detection, making multi-tab session management more stable
- **Documentation Domain Migration** — All documentation links migrated from `asterdrive.docs.esap.cc` to `drive.astercosm.com`

### Added

- **Storage Policy Data Migration**
  - New complete storage migration workflow: select source/target policy → pre-check → create task → resume → complete
  - Backend adds the `StoragePolicyMigration` task type with a dedicated concurrency channel (StorageMigration lane)
  - Database adds the `storage_migration_checkpoints` table, supporting resume-from-checkpoint and failure recovery
  - Migration results include detailed statistics: counts and byte sizes of migrated/skipped/failed objects
  - New `POST /admin/storage-migrations`, `POST /admin/storage-migrations/dry-run`, `POST /admin/storage-migrations/resume` endpoints
  - New RustFS S3 end-to-end migration, resume-from-checkpoint, and cross-batch merge integration tests
- **Admin Observability Pages**
  - Added `/admin/files` and `/admin/file-blobs` pages with multi-dimensional filtering, sorting, and pagination
  - Added `admin_file_service` module on the backend, providing reverse-reference queries for files and blobs
  - Storage migration dialog adds a "Check Plan" button that shows pre-check results (source data statistics, target capacity, deduplication estimates)
  - Task detail dialog supports resuming a failed migration task from its checkpoint
  - Added `GET /admin/files` and `GET /admin/file-blobs` endpoints
- **Error code system**
  - Added `ApiErrorCode` enum (654 lines) covering all existing `ApiSubcode` values
  - `ApiErrorInfo` responses add a `code` field; the frontend reads `error.code` first while remaining backward compatible with `error.subcode`
  - Added `RefreshTokenStale` and `RefreshTokenReuseDetected` error codes
  - Login failures return a uniform generic error message to avoid leaking user existence
- **Task cards**
  - Task cards now use a two-section layout: summary + expandable details
  - Added `summaryParts` function to generate structured summaries (text + icon chips)
  - Added `TaskSummaryChip` component to display key info such as file names and policies
  - Added `taskIcon` function mapping an icon to each task type
  - Progress, step details, and timestamps moved into a collapsible expansion panel
- **Metrics image**
  - Docker build matrix adds a `metrics` variant enabling the `server,cli,metrics` features
  - Each variant adds a `suffix` field; metrics image tags uniformly get the `-metrics` suffix
  - Build cache scope and registry ref now include the variant dimension to avoid cache conflicts
  - `publish-manifest` job switched to a matrix strategy, publishing multi-arch manifests for default and metrics separately

### Changed

- Root crate version bumped from `0.2.1` to `0.2.2`
- Marked `ApiSubcode` as deprecated in 0.3.0, keeping transitional compatibility
- Frontend error handling checks `error.code` first instead of `error.subcode`
- Refresh token expiry or reuse now returns dedicated error codes; the frontend automatically syncs session state
- Cross-tab refresh coordination adds heartbeat detection and stale-takeover logic
- Check whether the target path is already referenced before storage migration, to avoid deleting existing blob objects by mistake
- Refactored `copy_blob_streaming` and `cleanup_unmoved_target_object` to uniformly guard cleanup operations via `target_object_is_referenced`
- Non-critical public configuration (preview apps, thumbnails, media data) is deferred to idle-time loading
- SSE connections add a 1500ms initial delay to avoid competing for network resources during page load
- Upload session resumption is delayed by 600ms to reduce initial rendering pressure
- Cache `lastFolderContents` in fileStore; reuse existing data first when switching in the folder tree
- MFA status requests add caching with force refresh support and automatic invalidation after changes
- Cross-tab refresh lock is compatible with legacy lock records lacking an updatedAt field
- Route guards show a loading state only when unauthenticated, avoiding a flash for logged-in users being re-checked
- Folder tree controller removes setTimeout and directly reuses the store's cached snapshot
- Migration dialog disables the submit button while dry-run is loading
- AdminTaskTable rows add aria-expanded/aria-controls accessibility attributes
- File/blob rows add keyboard Enter/Space interaction support
- All documentation links migrated from `asterdrive.docs.esap.cc` to `drive.astercosm.com`

### Fixed

- Fixed an issue where storage migration did not check whether the target path was already referenced, which could delete existing blob objects by mistake
- Fixed incomplete error handling when a refresh token is expired or reused
- Fixed unstable session management in multi-tab scenarios
- Fixed instability in E2E tests using heading/cell role queries
- Fixed rename dialog input logic to select-all then type character by character
- Fixed archive task creation to pass the file name stem instead of the full file name
- Corrected the refresh token error code from E012 to E019

### Security

- Login failures return a uniform generic error message to avoid leaking user existence
- Check target path references before storage migration to avoid deleting referenced objects by mistake

### Notes

- This version is a `0.2.2` feature and stability maintenance release
- Added database migrations:
  - `m20260528_000001_add_storage_migration_checkpoints`
- **Breaking Change**: `ApiErrorCode` replaces `ApiSubcode`
  - `ApiErrorInfo` responses add a `code` field
  - The frontend should check `error.code` first instead of `error.subcode`
  - `ApiSubcode` is marked deprecated in 0.3.0, keeping transitional compatibility
  - Old clients keep working but will receive deprecation warnings
- New API endpoints:
  - `POST /admin/storage-migrations` - create a storage migration task
  - `POST /admin/storage-migrations/dry-run` - pre-check a migration plan
  - `POST /admin/storage-migrations/resume` - resume a failed migration task
  - `GET /admin/files` - query the file list
  - `GET /admin/file-blobs` - query the blob list
- Docker images add a `-metrics` variant that users can pull to enable metrics features
- Strongly-typed API clients should be regenerated to sync error codes, storage migrations, and admin observability endpoints
- Statistics: 145 files changed, 11,903 insertions(+), 890 deletions(-)
- This scope contains 14 commits

## [v0.2.1] - 2026-05-26

### Release Highlights

**AsterDrive `0.2.1` focuses on account security, team capacity management, upload resumption isolation, and documentation improvements.** This version adds email verification code MFA login, lets admins configure storage quotas directly in the team creation and editing flows, isolates upload session resumption per frontend instance to prevent multiple tabs or browser instances from competing for resumable tasks; it also completes the full English documentation, a modernized VitePress docs site theme, and synchronized updates to API, configuration, and user docs.

- **Email verification code MFA login** — Adds email verification code as a second-factor method alongside TOTP and recovery codes, with send cooldown, validity period, TOTP fallback policy, and a dedicated email template
- **Team storage quota management** — Admins can set storage quotas directly when creating or editing a team; the team detail page shows quota and usage progress
- **Upload session instance isolation** — Upload sessions record the frontend instance ID; the resumable upload list is filtered by browser instance, reducing multi-tab resumption conflicts
- **English documentation system and docs site redesign** — Adds a complete English documentation directory; VitePress supports Chinese/English sites, dark mode, modern visual variables, and theme-switch animations
- **Configuration and email delivery protection** — SMTP configuration is validated before enabling email verification code MFA; related MFA capabilities are automatically disabled when email configuration becomes invalid, reducing the risk of users being locked out
- **Developer docs sync** — API, runtime configuration, WebDAV, auth, upload, team, error code, and architecture docs updated with the new capabilities

### Added

- **Email verification code MFA**
  - Added `email_code` MFA challenge method, sending an 8-digit one-time verification code to the user's verified email
  - Added `POST /api/v1/auth/mfa/challenge/email-code/send` endpoint for sending email verification codes
  - Added runtime configuration for email verification code validity period and resend cooldown
  - Added runtime configuration for whether TOTP-enabled users may use email verification code fallback
  - Added email verification code login email subject and HTML template
  - Added `mfa_email_codes` table, entity, repository, and cleanup/consumption logic
  - Audit log adds an email verification code send action
- **Team storage quota**
  - The admin console team creation dialog supports setting a team storage quota
  - The admin console team detail editing flow supports modifying the team storage quota
  - The team detail Overview section adds quota value and usage progress display
  - Backend admin team create/update APIs support an optional `storage_quota` field
- **Upload session resumption isolation**
  - Upload sessions add a `frontend_client_id` field
  - Upload initialization requests and upload session queries accept a frontend instance ID
  - The frontend generates and persists an upload client ID per browser instance
  - The upload panel adds a status display for canceled upload tasks
- **Docs and docs site**
  - Added a complete English documentation system covering configuration, deployment, operations, user guide, storage, and troubleshooting
  - VitePress configuration adds Chinese/English locales, navigation, descriptions, and Open Graph information
  - The docs site adds brand colors, shadows, grid background, dark mode variables, and homepage visual improvements
  - The docs site adds navbar, dropdown menus, search box, sidebar, and theme-switch animations
  - Supports a circular-reveal theme-switch animation based on the View Transition API, compatible with `prefers-reduced-motion`

### Changed

- Root crate version bumped from `0.2.0-hotfix.1` to `0.2.1`
- Upload initialization internal parameters are unified into `InitUploadParams`; personal and team workspace upload initialization share the same parameter model
- Upload session resumption queries filter by the current frontend instance by default; legacy clients that omit `frontend_client_id` keep the original compatible behavior
- System configuration writes are now transactional, keeping linked configuration and audit log records atomic
- Configuration audit logs record the actually stored normalized values instead of raw input values
- Enabling email verification code MFA validates that the SMTP email delivery configuration is complete
- When the email delivery configuration is changed to an unavailable state, email verification code MFA login is automatically disabled
- Email template rendering context adds a `{{lang}}` variable so login verification code emails output the correct HTML language tag
- The admin console settings page now loads the full configuration list with pagination, preventing some configuration items from being invisible beyond the single-page limit
- Team quota input and display uniformly use the storage quota parsing utility, supporting more precise conversion from bytes to MB
- The login page MFA challenge panel supports switching between TOTP, recovery code, and email verification code methods
- TOTP code frontend validation tightened to exactly 6 digits
- Thumbnail and blob URL hooks add current-user namespace isolation and race protection
- README and About page update product positioning, emphasizing self-hosted file infrastructure and Docker-first quick start
- Developer API docs updated with email verification code MFA, upload instance isolation, team quotas, WebDAV system file interception, and error code documentation
- Configuration docs updated with email verification code MFA, email templates, runtime configuration, and WebDAV system file protection configuration

### Fixed

- Fixed team quota of `0` being misdetected as unsaved changes in the edit dialog
- Fixed inconsistent validation of team quota input for decimals, negative values, non-numeric input, and overflow
- Fixed the transaction boundary between email verification code generation and email sending, preventing unusable codes from being sent after hashing or persistence failure
- Fixed expired email verification codes still being consumable
- Fixed admin settings page hiding configuration items such as email templates due to configuration pagination limits
- Fixed duplicated email delivery configuration readiness checks in the frontend
- Fixed a race where rapidly switching resources could leave orphaned blob object URLs
- Fixed thumbnail cache not being isolated per current user, which could cause state crosstalk
- Corrected test error code assertion notes in README / About page

### Security

- Email verification code MFA is disabled by default and must be explicitly enabled by an administrator
- Email verification codes are stored hashed only; sending a new code invalidates old unconsumed codes
- Only one unconsumed email verification code record per user at a time, enforced by a database unique index
- Email verification code validity does not exceed the remaining time of the current MFA login flow
- Enabling email verification code MFA forcibly checks email delivery configuration, preventing users from entering a login path they cannot complete
- Email verification code MFA is automatically disabled when email delivery configuration becomes unavailable, reducing the risk of configuration changes locking accounts
- WebDAV docs add system file interception configuration notes, clarifying that writes of `.DS_Store`, `Thumbs.db`, and other system files can be blocked

### Notes

- This version is a `0.2.1` feature and documentation maintenance release
- Added database migrations:
  - `m20260526_000001_add_upload_session_frontend_client`
  - `m20260526_000002_add_mfa_email_codes`
- Added runtime configuration items:
  - `auth_email_code_login_enabled`
  - `auth_email_code_login_allow_totp_fallback`
  - `auth_email_code_login_ttl_secs`
  - `auth_email_code_login_resend_cooldown_secs`
- Added email template configuration items:
  - `mail_template_login_email_code_subject`
  - `mail_template_login_email_code_html`
- API enum extensions:
  - `MfaChallengeMethodType` adds `email_code`
  - `MfaChallengeRequestMethod` adds `email_code`
  - `ApiSubcode` adds email verification code MFA related subcodes
- Upload clients that want instance isolation should pass a stable `frontend_client_id` when initializing uploads and querying upload sessions
- Strongly-typed API clients should be regenerated to sync MFA methods, error subcodes, team quotas, and upload session fields
- Statistics: 168 files changed, 15,701 insertions(+), 737 deletions(-)
- This scope contains 10 commits

## [v0.2.0-hotfix.1] - 2026-05-25

### Release Highlights

**First hotfix of the `0.2.0` series.** This version refines authentication error code semantics, splitting the previously generic `AuthFailed` (2000) into three distinct codes — `TokenMissing` (2007), `CredentialsFailed` (2008), and `MfaFailed` (2009) — so the frontend can handle authentication failures more precisely.

- **Authentication error code refinement** — Missing token, wrong credentials, and MFA verification failure return distinct error codes; the frontend can trigger refresh or redirect by semantics
- **Session refresh before SSE reconnection** — After the storage change event stream disconnects, refresh the access token before reconnecting, reducing consecutive reconnection failures caused by token expiry
- **Automatic refresh on chunked upload auth failure** — When chunked upload fails token authentication, refresh the session and retry immediately without waiting for backoff delay
- **Sidebar drag handle accessibility semantics fix** — Changed the resize handle from `<input type="range">` to `<hr>` + ARIA separator, matching the actual interaction semantics

### Changed

- The backend auth middleware returns `TokenMissing` (2007) when the token is missing, no longer mixing it with credential errors
- MFA-related errors (wrong code, expired flow, too many attempts, factor not enabled, recovery code already used) are uniformly mapped to `MfaFailed` (2009)
- Credential errors (wrong password, wrong share password, etc.) return `CredentialsFailed` (2008)
- The frontend `isTokenAuthError` matches `TokenMissing`; the HTTP interceptor triggers a token refresh retry on `TokenMissing`
- The storage change event stream does not connect during auth initialization (`isChecking`), avoiding invalid SSE requests during bootstrap
- After the storage change event stream disconnects, refresh the access token first; if the refresh clears the session, do not reconnect
- When chunked upload fails token authentication, refresh the session and retry immediately, skipping the exponential backoff delay
- The sidebar width resize handle changed from `role=slider` to `role=separator`, fixing the accessibility semantics

### Fixed

- Fixed the SSE event stream giving up after consecutive reconnection failures caused by access token expiry
- Fixed chunked uploads still waiting for backoff delay after failing due to token expiry
- Fixed MFA verification failure error codes being mapped to generic `AuthFailed` instead of distinct `MfaFailed`
- Fixed requests missing a token being misclassified as credential errors

### Notes

- This version is the first hotfix release of the `0.2.0` series
- API error codes add 2007 (TokenMissing), 2008 (CredentialsFailed), 2009 (MfaFailed); `AuthFailed` (2000) is retained but no longer produced by current code paths
- Custom clients that check `code == 2000` for authentication failure should switch to matching the 2000-2009 range or handling specific subcodes
- No new database migrations
- Statistics: 28 files changed, 430 insertions(+), 72 deletions(-)

## [v0.2.0] - 2026-05-25

### Release Highlights

**AsterDrive `0.2.0` official release.** Building on `v0.2.0-rc.1`'s account security, MFA, monitoring metrics, SQLite read/write splitting, media metadata, and archive preview, this version continues polishing frontend experience, mobile layouts, document preview iframe permissions, and test coverage, moving the `0.2.0` series from RC to stable release.

- **Stable release hardening** — Root crate version bumped to `0.2.0`; frontend package version and product name synced to `asterdrive-panel` / `0.2.0`
- **Document preview permission enhancements** — The trusted document viewer iframe supports clipboard, fullscreen, picture-in-picture, autoplay, and safe popup escape, making Office / Google and other online preview interactions more complete
- **Folder tree interaction polish** — The sidebar folder tree adds smooth expand/collapse animations, the root supports independent collapsing, and keyboard and ARIA semantics are completed
- **Mobile layout fixes** — Full adaptation to dynamic viewport height and bottom safe area, fixing the sidebar folder tree being squeezed or unscrollable on short viewports
- **MFA settings experience enhancements** — The MFA binding flow in security settings adds step transitions, Presence, and height-measurement animations, reducing panel jumping
- **Test coverage completion** — Added short-viewport sidebar E2E, folder tree animation lifecycle, and MFA animation component tests

### Added

- Folder tree adds expand/collapse transition animations; subtree height and icon state transition in sync
- The root row adds an independent expand/collapse control, no longer requiring navigation actions to affect its collapsed state
- MFA settings flow adds step transition animations, Presence animations, and a dynamic height measurement component
- Added short-viewport sidebar E2E tests covering scroll usability under mobile browser address bars and small-screen layouts
- Added unit tests for folder tree animations, root collapsing, and MFA animations

### Changed

- Document viewer iframe permissions are differentiated by trusted scenario; Office / Google and other document previews get more complete interaction capabilities
- Frontend layout gradually adjusted from fixed `vh` to `dvh` and safe area adaptation, improving usable-space calculation when the mobile address bar changes
- The sidebar is split into navigation, quick filters, capacity display, content area, and drag-resize subcomponents, reducing main component complexity
- Folder tree control logic migrated to a reducer and controller hook; the main component keeps only render orchestration
- WOPI preview session management switched to a resource subscription model, making preview lifecycle and cleanup paths more controllable
- Share creation/edit dialog state management migrated to a reducer, reducing scattered form state and duplicate update logic

### Fixed

- Fixed the sidebar folder tree scroll area being squeezed by content on short viewports and mobile scenarios
- Fixed an accessibility issue where collapsed folder trees could still retain interactive child content
- Fixed password inputs missing `autocomplete="new-password"`, which caused browser autofill semantics issues
- Fixed async state boundaries in some preview, share, and MFA interaction tests

### Security

- Refined iframe sandbox policy: external web app previews keep tighter sandbox permissions; only the trusted document viewer gets same-origin, top-level navigation, and popup escape capabilities
- Document preview iframe `allow` policy explicitly limits clipboard, fullscreen, picture-in-picture, and autoplay capabilities, reducing unintentional privilege expansion

### Notes

- This version is the official release of the `0.2.0` series
- Upgrading from `v0.2.0-rc.1` to `v0.2.0` adds no new database migrations
- The production configuration schema adds no required items; `src/config/loader.rs` only fills in auth examples in test configurations
- Docker users are advised to use the `v0.2.0`, `stable`, or `latest` image tags; `edge` remains reserved for future pre-release versions
- Statistics: 55 files changed, 2,720 insertions(+), 1,060 deletions(-)
- This scope contains 5 commits

## [v0.2.0-rc.1] - 2026-05-24

### Release Highlights

**The `0.2.0` series enters RC.** This version continues consolidating account security, multi-factor authentication, monitoring metrics, database connection models, and media/archive previews; the frontend adds MFA login and security settings, plus concentrated polish on archive encoding compatibility, media metadata display, and share page loading experience.

- **MFA multi-factor authentication** — Adds TOTP, recovery codes, login second-factor verification, and admin reset capabilities
- **Prometheus metrics system** — Introduces `MetricsRecorder`, covering key paths such as API, database, storage, upload, runtime, and WOPI
- **SQLite read/write splitting** — Introduces `DbHandles` and a reader pool, fixing consistency issues caused by read-only connection permission checks
- **Archive preview encoding compatibility** — ZIP manifest cache upgraded to v2, supporting automatic/manual selection of GB18030, UTF-8, CP437, and other file name encodings
- **Media metadata enhancements** — Extended RAW / TIFF / GPS / audio-video metadata extraction and public media capability endpoints
- **Frontend quality hardening** — File preview, share pages, info panel, MFA, upload, and team/remote node get extensive Vitest / E2E coverage

### Added

- **Multi-factor authentication (MFA)**
  - Added TOTP factor binding, verification, disabling, and deletion flows
  - Added recovery code generation, display, copy, download, and regeneration flows
  - Login flow supports the `mfa_required` challenge; both password login and external authentication can enter second-factor verification
  - Admin console user details support resetting a user's MFA
  - Added `mfa_factors`, `mfa_recovery_codes`, `mfa_login_flows`, `mfa_totp_setup_flows` tables
- **Monitoring metrics**
  - Added `MetricsRecorder` trait and Prometheus recorder
  - Covered metrics for the HTTP API, database queries, storage drivers, uploads, background jobs, and WOPI
  - Added monitoring deployment docs, a Grafana dashboard, and production checklist items
- **Media and preview capabilities**
  - Added public media metadata capability endpoint with frontend caching
  - RAW image metadata supports extracting basic EXIF and GPS information
  - TIFF raw format gained an EXIF fallback parser
  - ZIP archive preview added filename encoding selection
- **Frontend experience**
  - Login page added an MFA challenge panel
  - Security settings added an MFA management section
  - Share page split into password panel, controller, and infinite-scroll loading logic
  - File info panel extended with media metadata display

### Changed

- **Database connection model**
  - Removed the redundant `db` field from `AppState`; database access is unified through `writer_db()` / reader handles
  - SQLite introduced separate read/write connection pools, reducing read requests' occupancy of write connections
- **Archive preview architecture**
  - Split ZIP raw-scanning and display-layer restricted-signature logic
  - Archive manifest cache upgraded to v2, recording encoding, compatibility hints, and finer error classification
  - Frontend archive preview split into a state model, content components, and interaction controls
- **Media metadata and preview**
  - Media metadata extraction supports range reads, reducing read costs for remote storage scenarios
  - File preview, music player, share playback queue, and info panel now uniformly read backend media capabilities
- **Frontend structure and quality**
  - Shell, share view, file info panel, preview dialog, and other modules continue splitting into controller / hook / view
  - Added `aria-label`, `aria-expanded`, and screen reader helper text in many places
  - Removed unused frontend dependencies and upgraded Vite, Vitest, Base UI, Hono, shadcn, and other dependencies

### Fixed

- Fixed multiple edge-case issues in MFA login, recovery codes, TOTP setup, and error-state handling
- Fixed download metrics recording and storage driver cache invalidation issues
- Fixed a potential consistency issue when permission checks used read-only database connections
- Fixed compatibility issues with some media files in metadata extraction, preview, and detail display
- Fixed archive preview compatibility issues with non-UTF-8 filenames, encoding detection, and error display

### Security

- Added MFA secret encryption configuration and TOTP key protection
- Web app embedded preview iframe gained `sandbox` restrictions
- `SECURITY.md` expanded with security policy, reporting process, and supported versions
- Metrics docs clarified that `/health/metrics` requires intranet or allowlist protection

### Notes

- This version is the first RC in the `0.2.0` series
- New database migration: `m20260523_000001_add_mfa`
- New config option: `[auth].mfa_secret_key`; replacing this key makes authenticator secrets of enabled MFA undecryptable, so back up your config and database before upgrading
- Login API responses changed to a tagged enum with `status`; custom clients need to handle the `mfa_required` branch
- Prometheus metrics require recompiling with the `metrics` feature enabled, and `/health/metrics` should be exposed with care
- Docker now ships the `ffprobe` CLI by default, used for media metadata capability detection
- Statistics: 707 files changed, 40,624 insertions(+), 10,506 deletions(-)
- This scope covers 39 commits

## [v0.2.0-beta.3] - 2026-05-21

### Release Highlights

**The `0.2.0` series continues filling out media metadata and error semantics.** This version wires blob-level media metadata extraction into background jobs and database caching; the frontend file details and share pages can display more complete EXIF / audio-video information. It also extends API subcodes and consolidates health checks and i18n structure.

- **Blob-level media metadata cache** — Added the `blob_media_metadata` table, repository, and extraction service, caching image / audio / video metadata by blob hash
- **File details metadata display** — File info panel and share page now display more complete media information, covering image EXIF, audio tags, and basic video info
- **API subcode consolidation** — Extended stable machine-readable error subcodes, with OpenAPI, frontend error mapping, and type definitions updated in sync
- **Lightweight health checks** — Readiness probe changed from a write test to a lightweight `readiness_check`, reducing health-probe side effects
- **Frontend structure cleanup** — i18n resources split by module; media-related frontend components and config copy consolidated in sync

### Added

- Added blob-level media metadata migration, repository, and extraction service
- Added a media metadata background job for async extraction and caching
- Admin console added a media metadata toggle and related config options
- Frontend added media metadata rendering, thumbnail helpers, and file details display logic
- `ApiSubcode` enum and OpenAPI definitions extended in sync

### Changed

- Media display logic in file info, preview, music player, and share pages further split and reorganized
- The ready check in `health_service` switched to a lighter implementation
- Frontend i18n split from a single resource file into a modular directory structure
- Media processing config copy and page structure adjusted in sync

### Fixed

- Fixed multiple edge-case issues in media metadata caching, parsing, and thumbnail handling
- Fixed compatibility issues with some media files in preview and detail pages

### Notes

- New database migration in this version: `m20260520_000001_add_blob_media_metadata`
- Backing up the database and data directory before upgrading is recommended
- Statistics: 409 files changed, 19,731 insertions(+), 7,744 deletions(-)
- This scope covers 20 commits

## [v0.2.0-beta.2] - 2026-05-19

### Release Highlights

**The `0.2.0` series continues filling out media experience and authentication security details.** This version upgrades audio preview to a global music player, adds an image preview derivative endpoint, and improves share stream playback sessions, upload progress, multi-tab refresh coordination, plus OIDC / Passkey / storage policy edge cases.

- **Global music player** — Audio preview upgraded to a playback queue that survives page navigation, with previous / next, loop / single / shuffle, volume, progress, media sessions, and metadata parsing
- **Image preview WebP derivatives** — New image preview endpoints for personal, team, and share pages, supporting 1600px WebP derivative caching, ETag / 304, and HEIF backend preview fallback
- **Configurable share stream playback sessions** — Share audio / video Range stream session TTL is now runtime-configurable, defaulting to 3 hours with validation for a 5-minute to 24-hour range
- **Enhanced upload experience** — Upload tasks show smoothed speeds; direct / presigned / chunked / multipart requests are uniformly tracked and cancellable, reducing leftover requests after cancellation
- **More reliable auth refresh** — Multi-tab access token refresh gained localStorage coordination; refresh token reuse detection adds a same-client short-window check, reducing false kills from concurrent refreshes
- **Public site and Passkey config consolidation** — Setup can initialize `public_site_url` from the request Origin; the admin side supports one-click fill-in of the current address, and Passkey returns a clear config error when the site URL is missing

### Added

- **Global music player**
  - Added `MusicPlayerHost`, a music player store, and queue-building utilities
  - Supports queue playback of audio file lists, share-page music playback, a background panel entry, and playback details
  - Supports `music-metadata` parsing of title, artist, album, and cover art, integrated with the browser Media Session
  - When playing shared files, stream playback sessions nearing expiry are automatically refreshed
- **Image preview endpoints**
  - Added `/api/v1/files/{id}/image-preview`
  - Added `/api/v1/teams/{team_id}/files/{id}/image-preview`
  - Added `/api/v1/s/{token}/image-preview` and an image preview endpoint for files inside shared folders
  - Image previews uniformly output WebP; cache paths are isolated by processor and version
- **Share stream playback config**
  - Added `share_stream_session_ttl_secs` runtime config
  - Admin console added the corresponding config copy and validation
- **Upload speed display**
  - Upload task items now show speed
  - direct, presigned, chunked, and presigned multipart uploads all record uploaded bytes and speed
- **Multi-tab refresh coordination**
  - Frontend added a cross-tab refresh lock, preventing multiple tabs from refreshing the access token simultaneously
  - After syncing a peer's refresh result, the local session expiry time is backfilled
- **Documentation**
  - Added docs for Passkey, external authentication, ZIP preview, stream playback sessions, internal storage protocol, sharing, remote nodes, and error handling
  - Docs site added a CNAME and robots.txt

### Changed

- **Audio / video preview architecture**
  - The video stream factory in preview components was generalized into a media stream factory
  - The old blob media preview was split into image preview, music preview, and video preview
  - Audio preview no longer just embeds a single `<audio>`; files are loaded into the global player instead
- **Media processing**
  - Built-in image pipeline, vips_cli, ffmpeg_cli, and storage_native processors support image preview derivatives
  - vips / ffmpeg logs no longer output local input/output paths, reducing path leakage risk
  - Thumbnail and image preview ETags carry the processor namespace and version
- **Upload cancellation**
  - Frontend uniformly registers upload XHRs; canceling a request aborts all in-flight upload requests for the same task
  - Presigned multipart cancellation logic adjusted: non-assembling states immediately delete the session and abort the remote multipart upload
- **Public site config**
  - The admin side reads the latest config before detecting `public_site_url`; multi-source config no longer pops a single-value fix dialog but redirects to the settings page
  - The settings page's string-array config supports adding the current access address directly to `public_site_url`
- **Storage policy validation**
  - S3-compatible policy creation, updates, and connection tests require non-empty `access_key` / `secret_key`
  - Local and remote policies keep their existing connection field behavior
- **Dependencies**
  - Frontend added `music-metadata`
  - Updated Hono, ip-address, react-arborist, tsgo preview, and other dependencies

### Fixed

- **Refresh token concurrent false kills**
  - Repeatedly submitting a just-rotated refresh token by the same client within a short grace window returns a stale token instead of revoking all sessions outright
  - Reuse by a different client, with missing client evidence, or beyond the grace window is still treated as suspected leakage and revokes sessions
- **Audit log IP**
  - Login, refresh, logout, session revocation, and password-change audits parse `X-Forwarded-For` according to trusted proxy config
  - Forged forwarding headers from untrusted proxy sources are ignored
  - Supports parsing forwarded addresses with port-bearing IPv4 and bracketed IPv6
- **Passkey / OIDC config**
  - Passkey login returns a clear config error when `public_site_url` is missing
  - Added test coverage for OIDC provider slug, issuer normalization, and callback edge cases
- **Upload cleanup**
  - After presigned multipart cancellation, it waits until the remote multipart abort is visible, reducing leftover temporary uploads on RustFS / S3
- **Image preview compatibility**
  - HEIF / HEIC images prefer backend-derived previews, so browsers without native format support no longer fail to display outright

### Security

- **Finer-grained refresh token reuse detection**
  - Retains the security policy of revoking all sessions on refresh token reuse
  - Same-client short-window concurrent refreshes are recognized as stale refresh, avoiding erroneous revocation from normal multi-tab concurrency
- **Trusted proxy audit IP**
  - Audit logs only use `X-Forwarded-For` when the peer matches a trusted proxy CIDR / IP
  - Untrusted clients cannot pollute login and session audit IPs with forged headers
- **S3 policy credential validation**
  - Prevents creating or testing S3-compatible storage policies that lack an access key / secret key

### Notes

- This version is the second beta in the `0.2.0` series, focusing on media experience, share stream playback, and authentication stability
- No new database migrations
- The default validity of share audio / video playback links changed from 30 minutes to 3 hours, configurable via `share_stream_session_ttl_secs`
- Custom clients wanting image previews should prefer the new `image-preview` endpoint and handle `304 Not Modified` based on ETag
- For Docker / production upgrades, backing up the database and data directory first is still recommended

---

**Statistics**:
- 167 files changed, 11,410 insertions(+), 590 deletions(-)
- 9 commits

---

## [v0.2.0-beta.1] - 2026-05-18

### Release Highlights

**AsterDrive enters the `0.2.0` series!** Building on the `v0.1.0` stable release, this version focuses on enterprise-grade login, archive preview, remote storage protocol, and search/audit observability capabilities, plus consolidation across security and performance.

- **OIDC single sign-on** — Full support for OpenID Connect external authentication, including an admin config panel, provider management, email verification, and account linking flows
- **WebAuthn Passkey** — Added the full Passkey registration / login / management flow, with conditional UI auto-detection and caching
- **ZIP archive read-only preview** — Archive previews are now generated asynchronously as manifests by background jobs; supports Range-based direct scanning of ZIP directories without downloading the full file
- **Remote storage protocol v2** — Introduced a capability negotiation mechanism that rejects incompatible nodes at the probing stage; refined CORS / Range contracts
- **Search and file categorization enhancements** — Files gained category and extension fields; global search supports type filtering, with sidebar quick-category entries
- **Signing chain upgraded to HMAC-SHA256** — Direct-link and preview tokens now use HMAC-SHA256 bound to a purpose string, eliminating length-extension attack risk

### Added

- **OIDC external authentication (SSO)**
  - Full support for the OpenID Connect single sign-on flow
  - Admin console added an external login provider config panel and state management
  - Email templates gained external authentication email verification, linking, and error notifications
  - External authentication service and frontend components split into standalone modules
- **WebAuthn Passkey**
  - User security settings added the full Passkey registration / login / deletion flow
  - Login page supports conditional UI auto-detection of available credentials
  - Backend introduced `webauthn-rs` and a new `passkeys` table for credential metadata
- **Archive preview**
  - Added read-only ZIP archive preview; the frontend browses archive contents as a directory tree
  - Archive manifests are now generated by async background jobs, avoiding blocking preview requests
  - Supports Range reads for direct ZIP directory scanning, skipping full-file downloads
  - Added the `archive_preview` config group and a corresponding rate limit policy
- **Remote storage protocol v2**
  - Introduced the `RemoteStorageCapabilities` capability negotiation mechanism
  - Node probing validates constraints such as `features` / `browser_cors` / `limits`
  - Incompatible older remote nodes are rejected at the join stage
- **Search and file categorization**
  - File entities gained `extension` / `compound_extension` / `file_category` fields
  - Global search API supports filtering by file type category and extension
  - Frontend sidebar added quick-category entries for images / videos / documents / audio
- **PDF preview virtual scrolling**
  - Direct URL streaming loads replaced the previous Blob preloading
  - Introduced `@tanstack/react-virtual` virtual scrolling; large documents render only pages within the viewport
- **WebDAV system file interception**
  - Added `webdav_block_system_files_enabled` runtime config
  - Supports pattern-matched interception of `.DS_Store`, `Thumbs.db`, and other junk system files
- **API error subcodes**
  - Introduced a type-safe `ApiSubcode` enum system
  - The `subcode` field in the OpenAPI schema changed from a dynamic string to a known enum set
- **Administration and operations**
  - Team list backend supports keyword search, pagination, and debounced queries
  - Background job scheduler performance optimizations; audit logs switched to batched writes

### Changed

- **Storage change events**
  - Soft-delete events changed to `file.trashed` / `folder.trashed`; `*.deleted` is reserved for hard deletes only
  - Added fine-grained events such as `file.purged` / `folder.purged` / `file.version_restored` / `file.version_deleted`
  - Events carry `affects_quota` and `storage_delta`; the frontend refreshes user quotas based on these fields
- **Chunked upload**
  - The endpoint changed from `web::Bytes` to `web::Payload` for streaming intake
  - Chunk size is validated in real time, returning 413 immediately when over the limit and avoiding memory pre-allocation
  - Optimized resource usage on the upload path; thumbnails are now generated by background jobs
- **Audit logs**
  - The `entity_type` field tightened from a dynamic string to the `AuditEntityType` strongly-typed enum
  - The database column was extended in length and made NOT NULL, with historical null values backfilled in batches
- **User info endpoint**
  - `/auth/me` supports `?fields=quota,profile,preferences,session` for on-demand queries
- **Security settings page**
  - Refactored into a tabbed layout with new animated collapse components
  - Passkey list gained a local caching strategy, reducing API round trips
- **Branding config**
  - Stricter control-character handling and validation, preventing abnormal characters from polluting branding fields
- **Archive extraction events**
  - Storage change events for extraction jobs merged from multiple publishes into a single publish
- **Remote storage CORS contract**
  - Presigned downloads / uploads must satisfy `Range` / `Content-Range` and other header requirements
  - Older nodes that don't comply are identified at the capability negotiation stage

### Fixed

- **Archive preview**
  - Fixed archive preview cache edge cases, WebDAV property isolation, and file type detection issues
  - Restricted signatures no longer affect archive manifest cache validity
- **WebDAV / storage / job scheduling**
  - Fixed multiple cache-validation races and background job scheduling logic defects
  - Fixed edge-case issues in job error handling and storage event aggregation
- **Workspace**
  - Fixed multiple defects in search, authentication, and data query logic
- **Frontend input experience**
  - Fixed the cursor jumping to the end while editing input fields, and improved focus-state behavior

### Security

- **Token signing upgraded to HMAC-SHA256**
  - Direct-link tokens gained a v2 format `v2.<base62-id>.<HMAC-SHA256>`
  - Preview link and share stream signatures changed from bare SHA256 to HMAC-SHA256
  - Signatures are bound to a purpose string, eliminating length-extension attacks and cross-purpose reuse risk
  - Preview link rate limiting logic refactored in sync
- **Hardened external authentication end to end**
  - Improved security validation across OIDC callbacks, email verification, and account linking
  - Added failure paths and error notifications

### Breaking Changes

- **Remote storage protocol minimum version raised to v2** — Older remote nodes running the v1 protocol will no longer pass compatibility validation at the probing stage and must be upgraded in lockstep
- **Storage change event semantics adjusted** — soft-delete events renamed from `file.deleted` / `folder.deleted` to `file.trashed` / `folder.trashed`; third-party clients listening over SSE / WebSocket must update their event handling logic
- **API sub-error-code schema tightened** — the `subcode` field type in the OpenAPI spec changed from `string` to an enum set; the wire format is still a string, but generated SDK type definitions need to be updated accordingly

### Notes

- This version is the first pre-release (beta.1) of the `0.2.0` series and is still in the feature-expansion phase; production environments are advised to continue using the `v0.1.0` stable release
- Back up the database and data directory before upgrading; this version includes 4 new migrations (passkeys, external auth, file type fields, audit log entity_type)
- Remote follower nodes must be upgraded to a version supporting protocol v2 before they can continue working
- Custom clients listening to storage-change SSE need to switch soft-delete listening from `*.deleted` to `*.trashed`
- Docker users can use the `v0.2.0-beta.1` or `edge` image tags

---

**Statistics**:
- 491 files changed, 49,482 insertions(+), 5,069 deletions(-)
- 46 commits

---

## [v0.1.0] - 2026-05-15

### Release Highlights

**AsterDrive's first stable release!** From `v0.0.1-alpha.1` to `v0.1.0`, AsterDrive completed the first round of productization for self-hosted cloud storage core capabilities, remote storage, team collaboration, WebDAV, sharing, online preview, background tasks, and production deployment documentation.

- **Stable-release hardening** — on top of rc.2, added graceful service shutdown, crash diagnostics, SSE close semantics, and local temp-file cleanup logging, lowering production troubleshooting costs
- **Production deployment docs improved** — docs site navigation and homepage restructured; added production launch checklist, S3 / MinIO / R2, remote follower nodes, team permissions, online preview, glossary, and FAQ
- **Multi-arch image publishing optimized** — Docker images are now built per-architecture on native amd64 / arm64 runners, then published as a multi-arch manifest, with SBOM and cosign signatures still generated
- **File browser UX tweaks** — with a single file selected, the download button downloads the original file directly; archive download is only used for multi-selection or when folders are included; the workspace switcher moved to the top of the sidebar
- **Server observability enhanced** — added tracing logs to key paths for teams, policies, locks, WebDAV, deletion, cleanup, and version recycling, making production issues easier to locate
- **E2E and integration test stability fixes** — improved task cards, batch operations, team spaces, WebDAV password fields, and time-precision assertions, reducing test selector and database precision noise

### Added

- **Production deployment and usage docs**
  - Added a production launch checklist covering reverse proxy, persistent directories, backup, email, tasks, preview, and pre-upgrade verification
  - Added S3 / MinIO / Cloudflare R2 storage configuration docs
  - Added remote follower node storage docs, covering primary-follower deployment, enrollment, reverse proxy, and troubleshooting workflows
  - Added team and permissions, online preview and WOPI, glossary, FAQ, and documentation contribution guides
- **Runtime shutdown and crash diagnostics**
  - HTTP services now uniformly use a custom shutdown signal handler with an 8-second graceful shutdown timeout
  - Storage-change SSE actively terminates existing connections on service shutdown and rejects new connections after shutdown
  - Panic diagnostics are written to `data/crash.log`; if the write fails, the full diagnostic report is output to stderr
- **Release image capabilities**
  - Docker CI adds per-architecture build jobs for amd64 / arm64
  - The release stage generates multi-arch manifests for GHCR and Docker Hub
  - Stable releases automatically publish `latest` / `stable` tags; pre-releases continue to publish `edge`
- **Observability logs**
  - Added tracing events for deletion, permanent cleanup, file version recycling, lock lifecycle, team archive / restore / force delete, policy deletion, WebDAV account deletion, and other paths
  - Local storage temp-file cleanup failures are no longer silently ignored; a warn log is recorded

### Changed

- **Version and release positioning**
  - Root crate version bumped from `0.1.0-rc.2` to `0.1.0`
  - The `0.1.0` series switched from release candidate to the first stable release
- **Docs site structure**
  - VitePress navigation reorganized from flat entries into "Getting Started / Usage / Administration / Configuration / Storage / Deployment / Development"
  - Homepage converted to an entry page oriented toward deployment, usage, operations, and secondary development
  - Configuration, deployment, and usage docs gained more cross-links and notes on current-version behavior
- **File browser operations**
  - Batch selection toolbar and context menu now uniformly use `downloadAction`
  - When only a single file is selected, "Download" downloads the original file directly; multi-selection or folders-included selection continues to use archive download tasks
  - Workspace switcher moved from the TopBar to the top of the sidebar, making the team space entry more stable
- **Docker runtime environment**
  - Runtime image adds `vips-poppler`, strengthening PDF / document preview processing dependencies
  - `docker-compose.yml` adds `stop_grace_period: 45s` to cooperate with server-side graceful shutdown
- **Frontend real-time events**
  - EventSource no longer enters backoff reconnection when permanently closed by the server, avoiding pointless reconnections during shutdown

### Fixed

- **SSE connection handling on service shutdown**
  - Fixed an issue where the storage-change event stream could remain hanging during server shutdown
  - Fixed an issue where new SSE connections created after shutdown still entered the streaming response; they now return `204 No Content`
- **Crash log reliability**
  - Fixed diagnostic logs failing to write when the crash log directory does not exist
  - Fixed an issue where, on write-lock contention or permission failure, users only saw a brief failure notice and could not get the full report
- **Local storage temp-file cleanup**
  - Fixed temp-file cleanup failures being silently swallowed in local upload, dedup promotion, and copy fallback paths
  - Cleanup failures now log the specific path and error, making disk leftovers traceable
- **Test stability**
  - Fixed flaky async assertions on password fields in WebDAV account tests
  - Fixed intermittent failures in background task health-check tests caused by database time-precision differences
  - Improved Playwright test locators to avoid mismatches in task list, team space, and batch operation scenarios
- **Production build pipeline**
  - Avoided Docker multi-arch images being too slow or unstable under single-job QEMU builds; switched to native runner builds followed by manifest merging

### Notes

- This version is AsterDrive's first stable release and the official release of the `0.1.0` series
- Upgrading from `v0.1.0-rc.2` to `v0.1.0` adds no new database migrations
- Backing up the database and data directory before upgrading is still recommended; production deployments should follow the new production checklist item by item
- Docker users are advised to use the `v0.1.0`, `stable`, or `latest` image tags; `edge` remains reserved for alpha / beta / rc pre-releases
- Custom clients relying on the `/api/v1/auth/events/storage` SSE connection should note that connections may end normally during service shutdown, and new connections after shutdown will return `204`

---

**Statistics**:
- 101 files changed, 3,621 insertions(+), 751 deletions(-)
- 12 commits

---

## [v0.1.0-rc.2] - 2026-05-13

### Release Highlights

- **File browser batch operations reworked** — batch move, copy, delete, archive download, and compress entries are unified into the selection toolbar, with more consistent operation feedback and refresh flow
- **Real-time event deduplication** — file operations triggered locally are no longer redundantly refreshed by SSE echoes, reducing list jitter and duplicate state updates
- **Top workspace switcher** — personal and team spaces can be quickly switched at the top, with team search and management entry shortcuts
- **Enhanced admin console async operation feedback** — delete, unlock, cleanup, and other operations gain pending states, preventing repeated clicks and misoperation
- **Storage policy force delete and fallback cleanup** — admins can force-delete policies occupied by upload sessions, with related upload sessions and temp objects cleaned up automatically
- **Public configuration caching optimized** — public branding, preview app, and thumbnail support configuration gained caching and invalidation, reducing redundant computation and API overhead

### Added

- **Workspace switcher**
  - Top bar adds a personal space / team space switch entry
  - Supports team search, current space indicator, and team management shortcuts
  - Team loading logic moved up to the layout layer, reducing duplicate handling inside pages
- **Storage policy deletion fallback tasks**
  - Force-deleting a storage policy cleans up associated upload sessions
  - Added a fallback cleanup task for temp objects after storage policy deletion
  - Added delayed cleanup tests for force-deleting presigned upload policies
- **Public configuration caching**
  - Public configuration API adds caching and cache invalidation
  - Branding, preview app, and thumbnail support reads reduce redundant queries
- **Test coverage**
  - Substantially expanded local / S3 / remote storage, task scheduling, email, caching, policy, and upload integration tests
  - Added unit tests for file browser batch operations, context menus, workspace switcher, and admin console pending states

### Changed

- **File browser batch operations**
  - Batch operation logic migrated to a dedicated hook, reducing state entanglement in page components
  - File / folder context menus and the selection toolbar now behave more consistently for batch actions
  - Recycle bin batch operations, tables, and grid view selection feedback adjusted in sync
- **Real-time storage event handling**
  - Added frontend storage event echo tracking and deduplication logic
  - SSE echoes triggered by local operations such as delete and restore are recognized and duplicate processing is skipped
- **Admin console list experience**
  - Users, policies, policy groups, shares, locks, and remote node lists add pending states for async operations
  - Buttons are disabled with in-progress feedback during delete, unlock, cleanup, and other operations
- **Module splitting and maintainability**
  - Backend repository, audit, lock, task scheduling, upload completion, thumbnail, and local / S3 / remote storage drivers split into submodules
  - Type definitions split into `types/*` modules organized by domain
  - Frontend admin console query parameter types migrated to generated API types

### Fixed

- **SSE duplicate refreshes**
  - Fixed an issue where receiving the same event again after a local operation caused duplicate list refreshes and state jitter
- **Background task record noise**
  - Recent records are reused when system health checks succeed consecutively, reducing background task list noise and database growth
- **Admin operation duplicate submission**
  - Repeated clicks are blocked while async operations like delete and unlock are running, reducing duplicate requests and misoperation risk

### Notes

- This version is the second release candidate of the `0.1.0` series
- This version contains extensive internal module splitting, mainly affecting maintainability and test coverage, without changing the main usage of public APIs
- Custom clients relying on storage policy deletion, upload session, or file list real-time refresh behavior should focus on verifying the related flows
- Backing up the database before upgrading is still recommended; confirm per the rc.1 migration baseline requirements that old deployments have completed the pre-rc.1 migration chain

---

**Statistics**:
- 244 files changed, 24,194 insertions(+), 15,147 deletions(-)
- 18 commits

---

## [v0.1.0-rc.1] - 2026-05-12

### Release Highlights

- **First RC release and migration baseline finalization** — version bumped to `0.1.0-rc.1`; database migrations recompressed into `m20260512_000001_baseline_schema`; existing deployments must complete the pre-rc.1 old migration chain before upgrading
- **Admin console full-list sorting** — users, teams, members, policies, policy groups, remote nodes, shares, locks, background tasks, and audit log lists support whitelist-field sorting, with sort state preserved via URL parameters
- **User identity display unified as UserSummary** — admin console, team, share, lock, task, and audit-related responses upgraded from bare user IDs / usernames to nested user summaries; the frontend uniformly displays avatar, display name, and username
- **Theme accent color switched to hex values** — the preference `color_preset` switched from fixed enum names to `#rrggbb`; the frontend supports custom color input and remains compatible with reading old preset names
- **Unified admin console table experience** — extracted a shared AdminTable component, unifying list spacing, borders, sortable headers, accessibility states, and interaction feedback

### Added

- **Admin console sorting parameters**
  - Admin user list supports sorting by ID, username, email, role, status, usage, quota, created time, and updated time
  - Admin team list supports sorting by ID, name, usage, quota, created time, updated time, and archived time
  - Team member list supports sorting by username, email, role, status, created time, and updated time
  - Admin policies, policy groups, remote nodes, shares, locks, background tasks, and audit log lists all add `sort_by` / `sort_order` query parameters
  - The backend maps sort fields using whitelisted enums, and all non-ID sorts append ID as a stable tie-breaker
- **User summary response model**
  - Added `UserSummary`, containing user ID, username, and profile information
  - Admin overview recent tasks, audit log, team list / detail / members, share list, WebDAV lock list, and team audit records return user summaries
  - Frontend adds a `UserIdentity` shared component, uniformly displaying avatar, display name, and `@username`
- **Custom theme colors**
  - `ColorPreset` supports parsing and returning normalized `#rrggbb` hex colors
  - Frontend color picker adds a native color input, allowing accent colors outside the presets
  - Old `blue` / `green` / `purple` / `orange` preference values are read compatibly and normalized
- **Test coverage**
  - Added integration tests for explicit sorting of each admin list, rejection of invalid sort parameters, and background task ID tie-breakers
  - Added migration rebase tests covering full pre-rc.1 history rewrite, incomplete history rejection, and SQLite schema baseline alignment
  - Added theme color tests for hex acceptance, invalid color rejection, and old preset name normalization
  - Added AdminTable unit tests covering table structure, styles, and sortable header interaction

### Changed

- **Database migration baseline**
  - The current migration set is compressed into `m20260512_000001_baseline_schema`
  - The old `m20260502_000001_baseline_schema`, file / folder owner provenance split migrations, and the background task `failure_can_retry` migration are folded into the new rc.1 baseline
  - Databases that fully applied the pre-rc.1 migration chain verify key schema sentinels, then only rewrite `seaql_migrations` metadata to the new baseline
  - Upgrade docs updated with the pre-rc.1 rebase strategy and how to handle incomplete old databases
- **Admin console API responses**
  - Audit log changed from `user_id` to `user: UserSummary | null`
  - Share list changed from bare user ID to `user: UserSummary | null`
  - WebDAV lock list changed from `owner_id` to `owner: UserSummary | null`
  - Background task events changed from `creator_user_id` to `creator: UserSummary | null`
  - Team creator, member users, and team audit actor / member uniformly return user summaries
- **Frontend admin tables**
  - Admin console tables migrated to unified `AdminTable` / `AdminSortableTableHead` components
  - Sort state is written to `sortBy` / `sortOrder` URL queries, preserving the current sort after refresh or link copy
  - Headers add `aria-sort`; the currently sorted column shows a direction icon
- **Dependencies and generated types**
  - Rust crate version bumped to `0.1.0-rc.1`
  - `aws-sdk-s3` upgraded to `1.132.0`; `utoipa` upgraded to `5.5.0`
  - Frontend upgraded i18next, react-arborist, tailwind-merge, Biome, Playwright, Vite, Vitest, MSW, and other dependencies in sync
  - OpenAPI generated types updated in sync for sorting parameters, `UserSummary`, and the hex `ColorPreset` schema

### Fixed

- **User update request compatibility**
  - Admin-side user updates now strip out `policy_group_id: null`, preventing the backend from misreading "do not change policy group" as an illegal clearing operation
- **Migration rebase safety validation**
  - Rebase validation adds pre-rc.1 schema sentinels such as `owner_user_id`, `created_by_user_id`, `created_by_username`, and `background_tasks.failure_can_retry`
  - Mixed old/new baselines, empty migration records with existing business tables, or an incomplete pre-rc.1 migration chain are explicitly rejected at startup with an upgrade hint
- **Theme preference compatibility**
  - Users with saved old color preset names will not lose their theme color after upgrading; values are automatically mapped to the corresponding hex color on read

### Notes

- This version is the first release candidate of the `0.1.0` series
- Back up the database before upgrading; existing deployments must first run the last pre-rc.1 build and complete `m20260502_000001_baseline_schema`, `m20260508_000001_split_file_folder_owner_provenance`, `m20260511_000001_add_background_task_failure_can_retry`, then upgrade to this version
- New deployments directly execute `m20260512_000001_baseline_schema`; deployments with a complete pre-rc.1 history only rewrite migration metadata, without clearing business tables
- Custom clients consuming admin console / team / share / lock / audit APIs need to migrate bare user fields to nested `UserSummary` objects
- The `color_preset` in user preferences is now returned as `#rrggbb`; old preset names can still be read but are normalized to hex output
- Admin console lists add `sort_by` / `sort_order`; unknown sort fields are rejected by request parameter validation

---

**Statistics**:
- 133 files changed, 6,236 insertions(+), 2,709 deletions(-)
- 6 commits

---

## [v0.1.0-beta.5] - 2026-05-12

### Release Highlights

- **HTTP Range and video streaming preview** — file download, direct link, preview link, and public share download support single-part Range requests; video preview switched to direct link / temporary stream session streaming playback, reducing memory usage for large file previews
- **Share video streaming playback session** — public shares add short-term stream sessions; multiple Range fetches within the same playback session count as one download, compatible with password-protected shares and files inside shared folders
- **Archive extraction and build safety limits** — ZIP extraction and archive building add multi-dimensional limits on size, entry count, directory depth, path length, compression ratio, and duration, strengthening zip bomb and abnormal archive protection
- **Background task lane-based scheduling** — archive, thumbnail, and fallback tasks are independently rate-limited per lane; failed tasks record retryable status; task claiming reduces duplicate claims and concurrency overrun
- **Upload and storage performance optimization** — upload initialization, directory upload, filename conflict resolution, local dedup, audit log writes, and temp-file write paths all reduce redundant queries, memory usage, and syscalls
- **Cross-route upload persistence and expanded E2E coverage** — the upload area is lifted to the workspace route level, so uploads survive page switches; new multi-module Playwright coverage added

### Added

- **HTTP Range support**
  - File download, direct link download, preview link, and public share download support `Range` requests
  - Responses return `206 Partial Content`, `Accept-Ranges`, and `Content-Range`
  - Video / audio seeking supported; currently only single-part Range is supported
- **Share video streaming playback session**
  - Added a public share stream session API
  - Both single-file shares and files inside shared folders can generate short-term playback sessions
  - Multiple Range requests within the same playback session count as one download
  - Password-protected shares validate playback permission via access cookie
- **Archive safety limit configuration**
  - Added limits for ZIP extraction source file size, total expanded size, entry count, file count, and directory count
  - Added limits for path depth, path length, compression ratio, and per-task duration caps
  - Added limits for archive build entry count, total source file size, and temp output estimation
  - Admin console runtime settings add archive limit and background task concurrency configuration notes
- **Background task retryable failure status**
  - The `background_tasks` table adds a `failure_can_retry` field
  - The task API's `can_retry` is returned based on failure type; security / validation failures no longer allow manual retry
  - Historical failed tasks keep compatible semantics
- **E2E and integration test coverage**
  - Added Playwright coverage for admin audit, teams, search, settings, archive tasks, and WebDAV
  - Added backend integration tests for Range download, share streaming sessions, upload initialization collisions, directory upload, task scheduling, and archive safety limits

### Changed

- **Video preview**
  - Frontend video preview switched from fetching the whole Blob to directly using HTTP / public share / stream session links
  - Artplayer only preloads metadata and falls back to the native `<video>` on initialization failure
  - Share page video preview automatically creates a temporary streaming playback session in controlled-access scenarios
- **Background task scheduling**
  - Per-lane concurrency control for archive, thumbnail, and fallback tasks
  - Task claiming re-checks lane capacity within the transaction, reducing over-scheduling under concurrency
  - Task claiming logic merged into a single batched transaction
- **Upload initiation and completion flow**
  - Upload initiation inserts the session first, then prepares external resources / directories; auto-retries on `upload_id` conflict
  - Attempts to abort the remote upload when S3 multipart initialization fails
  - Upload completion path reduces redundant quota checks, policy resolution, folder validation, and actor lookups
  - Directory uploads batch-prefetch parent folder policies and candidate file names
- **Audit log writes**
  - Audit logs switched to global asynchronous batched writes
  - Query, statistics, and shutdown flows proactively flush pending audit records
  - High-frequency upload and file operation paths reduce synchronous database write pressure
- **Local storage and thumbnail generation**
  - Local content-deduplicated uploads use no-clobber hard links / temporary copies to improve atomicity
  - Added `BufWriter` buffering for upload temp file writes
  - Thumbnail generation reads the local path first; remote objects are streamed to a temp file before processing
  - Temp file and directory cleanup extracted into an RAII guard
- **Recycle bin and storage events**
  - Recycle bin folder purge switched to batched forest purge, with a single-folder fallback preserved when batch processing fails
  - Recycle bin list counts use the server-side total; folders and files can continue loading pages independently
  - SSE storage change events now refresh personal usage, team list, and team usage
  - Storage change cache invalidation merges over a short window, reducing needless folder path cache purges
- **Dependencies and build**
  - Rust profiling build configuration renamed and adjusted
  - Frontend scripts now invoke local `biome` directly
  - Upgraded React / React DOM, Vite, Tailwind CSS, i18next, MSW and other frontend dependencies

### Fixed

- **Upload session conflict detection**
  - Fixed unique-conflict detection logic: only treat it as an `upload_id` collision when the ID is confirmed to already exist
  - Avoid misjudging other unique-constraint violations or database errors as retryable conflicts
- **Upload and quota correctness**
  - Fixed quota check ordering in pre-upload / completion; non-deduplicated pre-uploads now fast-fail before writing objects
  - Fixed the risk of pre-upload objects not being cleaned up on database or quota failure
  - Fixed extra queries caused by duplicate quota pre-checks at upload completion
  - Fixed missing overflow check before blob `ref_count` increment
- **Archive extraction safety**
  - Rejects encrypted entries, symlinks, special files, duplicate paths, file / directory conflicts, and abnormal compression methods
  - Validates declared sizes, compression ratios, and zip bomb risks
  - Fixed the issue where a failed extraction import could leave partially created directories / files; new root directories are cleaned up on failure
- **Share download counting**
  - Fixed share download counts potentially inflating on client abort or response build failure
  - Range / stream session paths now use unified download-count recording semantics
- **Frontend upload and recycle bin**
  - Fixed active upload tasks potentially being lost when the file browser page unmounts after navigating away
  - Fixed the recycle bin count appearing too small when only the first page is loaded
  - Fixed pagination breaking on "more folders but no more files"
- **Other correctness fixes**
  - Fixed Unicode NFC / NFD normalization edge cases in filename conflict resolution
  - Fixed tracing log field formatting issues
  - Fixed the misleading email placeholder in the first field of the registration / first-initialization form
  - Fixed personal or team usage information potentially lagging after storage change events arrive

### Notes

- This version is the fifth pre-release in the `0.1.0-beta` series
- The upgrade requires running a database migration: the `background_tasks` table adds a nullable boolean column `failure_can_retry`
- Download endpoints now support single-part `Range`; multi-part Range is not yet supported and returns a validation error
- The `can_retry` semantics of the task API are tightened; new failures explicitly distinguish retryable / non-retryable
- Added a public share stream session API with OpenAPI schema, allowing custom clients to implement controlled video streaming via this endpoint
- System configuration adds per-lane background task concurrency and archive limit settings; `background_task_max_concurrency` serves as the fallback lane cap
- README removed the warning block about "still under active development, not production-ready"

---

**Statistics**:
- 132 files changed, 10,303 insertions(+), 1,197 deletions(-)
- 29 commits

---

## [v0.1.0-beta.4] - 2026-05-08

### Release Highlights

- **Multi-layer cache optimization system** — Introduced an application-layer cache abstraction supporting both in-memory and Redis backends, with automatic fallback to local cache when Redis fails; share service query performance greatly improved
- **File/folder ownership model refactor** — Split `user_id` into `owner_user_id`, `created_by_user_id`, `created_by_username`, supporting resource ownership tracing in team workspaces
- **Frontend component architecture refactor** — Split large admin console components into maintainable sub-component directory structures, extracting state management logic into standalone hooks
- **Multi-architecture native support expansion** — Release CI added Linux ARM64/ARMv7, macOS ARM64/x86_64, and Windows ARM64 build targets; Docker images support the linux/arm64 platform
- **Public config auto-revalidation** — Frontend refreshes every 60 seconds plus on window focus/visibility changes, keeping branding and preview app config up to date

### Added

- **Multi-layer cache system**
  - Added `src/cache/` module providing a unified cache abstraction interface
  - Supports moka in-memory cache and Redis dual backends, with automatic failure detection and circuit-breaker fallback
  - Cache reservation mechanism prevents concurrent write conflicts (e.g. thumbnail generation)
  - Share service integration: share token lookup cache (60s TTL), active share target cache
- **File/folder ownership fields**
  - `files` and `folders` tables add `owner_user_id`, `created_by_user_id`, `created_by_username` fields
  - Supports distinguishing the resource owner (`NULL` for team workspaces) from the actual creator
- **Public config auto-revalidation mechanism**
  - `App.tsx` automatically revalidates public config every 60 seconds
  - Window `focus` and `visibilitychange` events trigger immediate refresh
  - Covers three public config stores: branding, previewApp, thumbnailSupport
- **Docker multi-architecture image support**
  - Added `linux/arm64` platform support
  - Two-tier build cache strategy (gha + registry)
  - Image cosign signing
- **Release CI multi-architecture compilation greatly expanded**
  - Added Linux ARM64, ARMv7, macOS ARM64/x86_64, and Windows ARM64 targets
  - checksums.txt cosign Sigstore signing

### Changed

- **Share service performance optimization** — Introduced multi-layer caching to reduce database queries, with batch cache invalidation by scope prefix
- **CI build strategy optimization** — Docker build cache strategy improvements, Release workflow architecture matrix expansion

### Refactored

- **Database migration** — `user_id` field split migration (compatible with three backends: SQLite table rebuild / SQL column alteration)
- **Frontend component architecture** — 98 components split and reorganized, e.g. `AdminTeamDetailDialog` → `admin-team-detail/` directory structure
- **Service layer code structure** — Parameter passing improvements, function splitting, reduced code duplication

### Notes

- ⚠️ **Breaking database schema change**: `files` and `folders` tables remove the `user_id` field; upgrades must run the migration
- ⚠️ **API response format change**: `FileInfo` / `FolderInfo` no longer return `user_id`; replaced with `owner_user_id` / `created_by_user_id` / `created_by_username`
- This migration is marked irreversible, because the creator's user may already have been deleted, making a safe rollback impossible
- Custom clients depending on the `user_id` field must update the field names accordingly

---

**Statistics**:
- 225 files changed, 19,206 insertions(+), 11,701 deletions(-)
- 8 commits

---

## [v0.1.0-beta.3] - 2026-05-06

### Release Highlights

- **System health monitoring** — Added comprehensive health checks for database, cache, and remote nodes; the admin console homepage shows healthy / degraded / unhealthy status with problem components
- **Redis cache fallback and circuit breaker** — Redis operations gain timeout protection, short-lived circuit breaking, and local reservation fallback, preventing cache failures from slowing the main path
- **Extended audit coverage** — Greatly expanded audit logs for users, files, folders, shares, batch operations, WebDAV, WOPI, background tasks, and remote node management
- **Remote node enrollment protection** — Connection tests, health checks, and network sync are blocked until remote node enrollment completes; the creation entry adds main site URL pre-validation
- **Admin console overview upgrade** — Homepage adds a system health banner, recent background tasks, recent audit events, and trend charts, with access to system runtime task history
- **Semantic recycle bin expiry time** — Recycle bin list API / frontend display changed from `deleted_at` to `expires_at`, showing cleanup time directly
- **Share page visual redesign** — Public share page layout, loading skeleton, owner info, password page, and file list visual hierarchy redone; more stable on mobile

### Added

- **System health checks**
  - Added `health_service` that periodically checks database ping, cache backend health, and remote node probe results
  - Added the `system-health-check` system runtime task, recording health check results every 5 minutes
  - Background task results support carrying `system_health` metadata with overall status and component details
  - `/health/ready` verifies database availability first on both primary and follower nodes, then storage / follower readiness
  - Remote node health probing is concurrency-limited to 4 and skips nodes that are disabled, have no configured URL, or have incomplete enrollment
- **Admin console system health panel**
  - `GET /api/v1/admin/overview` response adds `system_health` and `recent_background_tasks`
  - Admin homepage shows a system health banner, listing degraded / unhealthy components with a link to system runtime task history when abnormal
  - Admin homepage adds a recent background tasks list showing status, duration, error messages, and completion time
  - Trend charts use `recharts`, showing new users, uploads, and share creation trends
- **Redis cache circuit breaker**
  - Redis backend adds a 250ms operation timeout, 500ms connection timeout, and bounded reconnect strategy
  - A 5-second fallback circuit opens after Redis operation failures or timeouts, skipping Redis requests during that window
  - `health_check()` reports the Redis fallback state, so the system health panel can directly expose cache degradation
  - `set_bytes_if_absent` uses a local reservation fallback, still preventing duplicate generation tasks when Redis is unavailable
- **Audit log coverage**
  - Added many `AuditAction` enum values covering admin user / policy / config / lock / task / remote node operations
  - Audit writes for files, folders, versions, attributes, batch copy/move/delete, archive downloads, recycle bin restore/permanent delete paths
  - Audit writes for WebDAV file writes, moves, deletes, lock/unlock, and WOPI open/edit/rename/UserInfo updates
  - Audit writes for user login, logout, registration, password reset, email changes, preference settings, avatars, and session revocation
  - Frontend adds `lib/audit.ts`; the admin audit page can format action and entity type locally
- **Remote node enrollment status**
  - Remote node list and detail responses add `enrollment_status`
  - Enrollment status distinguishes `not_started`, `pending`, `redeemed`, `completed`, `expired`
  - Connection tests, health checks, and binding sync only run after `completed`
  - Returns the `remote_node.enrollment_required` sub-error code when enrollment is incomplete
- **Upload panel empty state**
  - The upload panel keeps showing when there is upload activity but the task list is empty
  - Added empty-task copy to avoid an abrupt panel state after restoring / clearing completed tasks

### Changed

- **Version number**
  - Rust crate version upgraded to `0.1.0-beta.3`
- **Runtime health configuration**
  - The former remote node health check configuration is now described as system health checks, covering database, cache, and remote nodes
  - `system_health_check_interval_secs` semantics expanded from a single remote node probe to the comprehensive system health check interval
- **Background task run records**
  - Periodic tasks uniformly record non-quiet SystemRuntime events, including cleanup tasks, email dispatch, blob reconcile, and system health checks
  - SystemRuntime tasks carry duration, summary, errors, and optional health check details, so the admin console can show run history directly
- **Recycle bin API**
  - Recycle bin file / folder list item fields changed from `deleted_at` to `expires_at`
  - The file cursor query parameter changed to `file_after_expires_at`; the backend converts back to the internal deleted cursor using the retention period
  - Frontend recycle bin table, grid, pagination cursor, and copy uniformly show "expiry / cleanup time"
- **Share page UI**
  - Share page split into owner info, meta info row, centered status panel, loading skeleton, and folder content sections
  - Folder shares support a clearer breadcrumb, view switching, download action, and empty state
  - Password input, error page, expiry page, and top bar visual hierarchy reorganized
  - Public share page max width, card borders, shadows, and dark mode appearance unified
- **Remote node management**
  - Requires the main site URL to be configured before creating a remote node; otherwise the frontend prompts directly and blocks the creation flow
  - Remote node updates only sync follower binding config when enrollment is completed
  - Remote node health checks sync binding config and persist capability / last_error / last_checked_at
- **WOPI service interface cleanup**
  - Consolidated WOPI write, save-as, rename and other service entry parameters into request structs, reducing long parameter lists and passing clippy checks
- **Dependency upgrades**
  - `utoipa` upgraded to `5.5.0`
  - `react-router-dom` upgraded to `7.15.0`
  - `vite-plugin-pwa` upgraded to `1.3.0`
  - Several Rust transitive dependencies updated in sync

### Fixed

- **Redis failures slowing requests**
  - Fixed the issue where Redis backend operations without short timeouts / circuit breaking could keep blocking cache calls
  - Fixed the issue where cache health status was not clearly reflected in the admin console when Redis was unavailable
- **Remote nodes without completed enrollment touching the network**
  - Fixed the issue where connection tests, health checks, or binding sync could still run when remote node enrollment was incomplete
  - Fixed the creation flow proceeding into enrollment when the main site URL was missing
- **Audit gaps**
  - Fixed the lack of audit trails for many critical write operations, especially WebDAV, WOPI, batch operations, and admin maintenance operations
  - Audit log writes uniformly truncate IP / User-Agent, preventing malformed request headers from polluting audit records
- **Recycle bin display semantics**
  - Fixed the recycle bin UI showing deletion time as cleanup time
  - Fixed the recycle bin cursor exposing deletion time to the frontend, causing unclear semantics

### Notes

- This version is the third pre-release in the `0.1.0-beta` series, with no database schema migration
- The recycle bin list response field `deleted_at` has changed to `expires_at`; custom frontends or clients relying on this endpoint must update the field name
- The recycle bin file cursor query parameter changed from `file_after_deleted_at` to `file_after_expires_at`
- `system_health_check_interval_secs` no longer means only the remote node health check interval; it is the system health check interval
- Health checks access the default storage policy and remote nodes with completed enrollment; remote storage failures are recorded as unhealthy in system runtime tasks
- The Redis fallback circuit window is 5 seconds; during it, cache requests go through local fallback logic and report the anomaly in health checks

---

**Statistics**:
- 118 files changed, 5,597 insertions(+), 752 deletions(-)
- 11 commits

## [v0.1.0-beta.2] - 2026-05-05

### Release Highlights

- **Empty file upload** — Supports uploading zero-byte files, automatically using direct mode and skipping actual storage operations
- **Byte-level upload progress** — Upload progress is now weighted by file size; chunked and presigned multipart uploads support real-time per-chunk callbacks
- **IME compatibility** — Fully fixed shortcuts misfiring during Chinese/Japanese and other IME composition; unified IME detection utility module
- **Permanent delete cascades to shares** — Permanently deleting files/folders automatically cleans up associated share records, eliminating orphaned shares
- **Panic crash report improvements** — Users see a short friendly message, diagnostic details are written to crash.log, and source code info no longer leaks to stderr
- **Admin share pagination** — Admin console share list switched to offset pagination, driven by URL parameters
- **Clipboard compatibility** — Unified clipboard copy utility with automatic fallback to the legacy API, improving browser compatibility

### Added

- **Empty file upload**
  - Relaxed `total_size` validation to `min = 0`; empty files automatically use direct mode
  - Added negative size validation; `total_size < 0` returns a 400 error
  - Frontend multipart upload distinguishes "missing file field" from "empty file"
  - Added integration tests: full empty-file upload flows for personal and team workspaces
- **Byte-level upload progress**
  - Added `totalBytes` field and `calculateByteProgress` weighted progress calculation
  - Chunked and presigned multipart uploads support real-time per-chunk progress callbacks
  - Task resume correctly computes cumulative bytes of completed chunks
  - S3 presigned upload progress cap unified from 90% to 95%
- **IME compatibility**
  - Added `lib/keyboard.ts` utility module: IME composition state detection, Safari 32ms grace period
  - All keyboard shortcuts and components with input fields add IME detection
  - Covers: global shortcuts, select-all, search, code editor, PDF page number input, new folder, admin console Ctrl+S
  - Added unit tests covering IME signal detection and browser compatibility edge cases
- **Clipboard copy utility**
  - Added `lib/clipboard.ts`: prefers `navigator.clipboard.writeText`, automatically falls back to `execCommand("copy")`
  - Copy actions for share links, my shares, WebDAV credentials, remote nodes, etc. migrated uniformly
  - Added unit tests covering four scenarios
- **Admin share pagination**
  - `AdminSharesPage` switched to URL-parameter-driven offset pagination (offset + pageSize)
  - Page size options 10/20/50; auto-falls back to the previous page when deleting the last item
  - Added tests covering pagination loading, delete page fallback, and URL parameter sync
- **Container resource monitoring**
  - Added `scripts/monitor.sh`: in-container resource monitoring supporting cgroup v1/v2, with console table and CSV output
  - Added `scripts/test.sh`: runtime memory monitoring helper script

### Changed

- **Version number**
  - Rust crate version upgraded to `0.1.0-beta.2`
- **Panic crash report**
  - User stderr shows only a short friendly message, no longer including source location or stack traces
  - Full diagnostic info (version, platform, backtrace) written to crash.log
  - Degrades gracefully instead of panicking again when opening crash.log fails
- **i18n copy**
  - Share module: "Share link created" → "Link created", "Create share link" → "Create link"
  - Task module: "Download as archive" → "Download as ZIP"
  - Unified WebDAV and WOPI terminology
  - Removed the `registration_closed_desc` key
- **Admin force-delete user**
  - Share deletion moved ahead of file/folder deletion to avoid orphaned share records
- **CI Rust toolchain**
  - Added `rust-toolchain.toml` to pin the stable channel
  - Clippy expanded to `--workspace --all-targets --all-features`

### Fixed

- **Share cascade cleanup**
  - Fixed associated share records not being cleaned up when files/folders were permanently deleted
  - Added `share_repo::delete_by_file_ids` / `delete_by_folder_ids` batch deletion methods
  - Covered three paths: trash purge, WebDAV recursive deletion, and admin force-delete user
- **IME shortcut misfires**
  - Fixed shortcuts being triggered simultaneously when pressing the confirm key during IME composition (e.g., Chinese/Japanese input methods)
- **Clipboard copy failure**
  - Fixed `navigator.clipboard.writeText` silently failing on non-HTTPS pages or when the page is unfocused

### Notes

- This is the second prerelease of the `0.1.0-beta` series, with no API-level breaking changes
- No database schema changes involved
- i18n added the `share_direct_link_action` key and removed the `registration_closed_desc` key; update any custom translation overrides accordingly
- The crash.log path is based on `current_dir`; mind the working directory when deploying
- The S3 presigned upload progress cap was raised from 90% to 95%; this only affects frontend progress display

---

**Statistics**:
- 65 files changed, 1,812 insertions(+), 177 deletions(-)
- 7 commits

## [v0.1.0-beta.1] - 2026-05-04

### Release Highlights

- **First Beta prerelease** — AsterDrive moved from the alpha stage to the beta stage, with the core version bumped to `0.1.0-beta.1`
- **Server-side upload resumption capability** — Added a resumable upload session list API covering personal and team workspaces, providing backend support for resuming uploads after refresh
- **Upload panel experience upgrade** — The frontend now supports configuring upload concurrency and auto-clearing completed tasks, with reworked upload task display and resumption flow
- **Concurrency safety and data consistency enhancements** — Added atomic transitions and re-checks to upload completion, file overwrite, folder move/delete, lock cleanup, and background task takeover
- **Database batching optimizations** — Blob reference counting, version cleanup, and file/folder batch operations reduce serial queries, improving efficiency for large batch operations
- **Presigned upload finalization** — After a single-file direct upload completes, the object is uniformly migrated to its final key and the temporary object cleaned up, reducing the risk of temporary object leaks
- **Visual system polish** — Global color tokens, dark mode, and the visual hierarchy of cards/buttons/modals/upload panels are further unified

### Added

- **Resumable upload**
  - Added a resumable upload session list API for personal workspaces
  - Added a resumable upload session list API for team workspaces
  - Responses include upload mode, target folder, progress, completed parts, expiration time, and the metadata needed for resumption
  - OpenAPI and frontend generated types updated with the resumable upload APIs and DTOs
- **Upload settings**
  - New frontend upload concurrency setting supporting 1–8 concurrent tasks
  - Added an auto-remove completed tasks setting, persisted to localStorage
  - The upload resumption flow can load unfinished sessions from the server and reconcile them with local pending file state
- **Test coverage**
  - Added test scenarios for upload resumption, upload settings, PDF preview, file store, remote storage, and task takeover

### Changed

- **Version stage**
  - Rust crate version bumped to `0.1.0-beta.1`
  - This is the first beta prerelease, not a stable release
- **Upload completion flow**
  - Expired sessions no longer enter the assembly stage
  - After a presigned single-file upload completes, the object is copied from the temporary object to the final `files/{uuid}` key, and the temporary object cleanup is attempted
  - Upload progress and resumption responses now include richer part and session state information
- **Concurrency consistency**
  - File overwrite, folder move/delete, lock cleanup, and background task claiming flows now include locking, state re-checks, and atomic conditional updates
  - Background tasks only take over processing tasks whose explicit lease has expired, avoiding hijacking tasks still running
- **Batch operation performance**
  - Blob reference counting supports batch CASE updates and batch queries
  - File deletion, version cleanup, moves, and folder tree processing reduce repeated queries and serial updates
- **Frontend experience**
  - Reworked the upload panel, upload task items, and resumption interaction to reduce state confusion
  - Global design system updates improve light/dark mode contrast, control hierarchy, and overall visual consistency
  - PDF preview horizontal scrolling and the layout of various preview containers are more stable
- **Documentation**
  - Fully synchronized user docs, deployment docs, API docs, and module design docs, adding error explanations, reverse proxy configuration, and the resumable upload API description

### Fixed

- **Upload reliability**
  - Fixed an issue where expired upload sessions could still proceed through the completion flow
  - Fixed an issue where the cleanup task could mistakenly process upload sessions in assembling state
  - Fixed an issue where direct upload of large files was affected by the default request timeout
- **File and lock consistency**
  - Fixed an edge case where blob/file records could be polluted by stale state during concurrent overwrites
  - Fixed an edge case where lock cleanup racing with concurrent relocking caused inconsistent state
  - Fixed insufficient tree structure re-checks during concurrent folder move/delete
- **Frontend state**
  - Fixed residual loading / error states in file lists after move and cut-paste operations
  - Fixed insufficient horizontal scroll range in zoomed PDF view
  - Fixed several edge issues in upload resumption and task item state display
- **Security and compatibility**
  - Tightened avatar path and task claiming related compatibility logic
  - Fixed several consistency edge cases covered by S3, remote storage, team, and task tests

### Notes

- This is the first beta prerelease of AsterDrive, marking the core features moving from the alpha exploration stage into a more stable validation stage
- This version does not yet promise stable-level long-term compatibility of APIs, configuration, and data migrations; backing up the database and storage directories is still recommended before upgrading production environments
- This release focuses on upload resumption, concurrency consistency, batch performance, and UI polish, paving the way for the upcoming stable release
- No explicit configuration or API breaking changes found
- Deployments using presigned upload must confirm that backend storage credentials have object copy/delete permissions
- Old processing background tasks without `lease_expires_at` will no longer be automatically taken over based solely on heartbeat or started_at

---

**Statistics**:
- 164 files changed, 3,568 insertions(+), 1,017 deletions(-)
- 10 commits

## [v0.0.1-alpha.26] - 2026-05-03

### Release Highlights

- **Migration architecture hard cutover landed** — Merged 23 historical migrations into a baseline, simplifying the upgrade path and maintenance cost for new deployments
- **Chunked upload persistence refactor** — Rewrote the session persistence logic for local chunked uploads, strengthening idempotency and reliability, and fixed WebDAV empty-file write failures
- **Avatar path security hardening** — Fixed an avatar storage path validation vulnerability, blocking path traversal attacks
- **Preference settings robustness improvements** — Strengthened frontend preference settings defensiveness, rejecting and cleaning up invalid values
- **CLI and cache module extraction** — Extracted the db_shared module to eliminate duplication, and unified cache reservation logic under ReservationSet

### Changed

- **Database migrations**
  - Merged the 23 historical migration files into a single baseline; new deployments no longer need to run historical migrations step by step
  - Introduced a hard cutover upgrade strategy, supporting a direct switch from the old architecture to the new migration system
  - Cleaned up migration module dependencies and unified the base64 version
- **Chunked upload**
  - Refactored local chunked upload persistence logic; session state and chunk metadata are more reliable
  - Strengthened idempotency handling; re-uploading the same chunk no longer causes state confusion
- **CLI refactor**
  - Extracted the `db_shared` module, eliminating duplicate implementations of database helper functions
- **Cache optimization**
  - Extracted the `ReservationSet` struct to unify cache reservation logic

### Fixed

- **Security fixes**
  - Fixed an avatar storage path validation vulnerability, preventing path traversal attacks via crafted filenames
- **WebDAV writes**
  - Fixed the failure when writing empty files
- **Preference settings**
  - Strengthened the robustness of frontend preference setting storage, defending against invalid value writes

### Notes

- This upgrade uses a hard cutover strategy for the migration system; new environments will create table structures directly from the baseline
- When upgrading existing production environments, ensure the current database version has reached the latest historical state (v0.0.1-alpha.25)

---

**Statistics**:
- 148 files changed, 6,461 insertions(+), 9,427 deletions(-)
- 9 commits

## [v0.0.1-alpha.25] - 2026-04-30

### Release Highlights

- **Managed ingress architecture landed** — Remote follower write entry points are now ingress profiles managed by the primary, supporting local / S3 targets and default profile management
- **Multi-primary ingress migration preparation complete** — Master binding introduces `storage_namespace` isolation, avoiding object key conflicts when multiple primaries bind the same follower
- **Public site URL supports multiple origins** — `public_site_url` upgraded from a single origin to an origin list; share, preview, WebDAV, and WOPI links are generated by matching the current request origin
- **Remote storage download enhancements** — The remote storage driver now supports presigned downloads, Range reads, and download response header pass-through
- **Remote node management experience upgrade** — The admin console adds onboarding status display, duplicate enrollment interception, and a managed ingress profile management section
- **Upload audit logging completed** — Audit events are recorded when file uploads complete, and completion retries avoid duplicate log entries
- **Object key and origin validation hardening** — Unified object key normalization, and fixed security boundary issues including path escape, prefix boundaries, CSRF same-site, and share password validation

### Added

- **Managed ingress**
  - Added the `managed_ingress_profiles` table, used on the follower side to maintain write target configurations managed by the primary
  - Added managed ingress profile services, repositories, entities, and Admin APIs supporting create, update, delete, query, and setting the default profile
  - Supports local and S3 managed ingress profiles; the remote driver is explicitly rejected as a managed ingress target
  - Local managed ingress is strictly restricted under `server.follower.managed_ingress_local_root`, preventing path escapes pushed down by the primary
  - Before internal writes, the follower validates that a default profile exists, has been applied, and has no errors, returning explicit precondition errors
- **Multi-primary ingress isolation**
  - `master_bindings` introduces `storage_namespace`, used to isolate remote object paths of different primaries
  - The unique constraint for managed ingress profiles changed from global `profile_key` to `master_binding_id + profile_key`
  - Added a multi-primary ingress migration that handles master binding, managed ingress profile, and namespace compatibility data
- **Remote node management**
  - The remote node list now shows onboarding status, covering `not_started`, `pending`, `redeemed`, `completed`, `expired`
  - Remote nodes that have completed onboarding can no longer generate an enrollment command
  - Remote node details add a managed ingress profile management section, showing ready / pending / error status, revision, and error information
  - The admin console supports creating, editing, and deleting local / S3 ingress profiles, and switching the default profile
- **Public site URL multi-origin**
  - The `public_site_url` configuration type upgraded to `string_array`, supporting multiple trusted HTTP(S) origins
  - The public branding API adds `site_urls`, so the frontend can read all public origins at startup
  - Added request origin matching logic; share, preview, WebDAV, and WOPI URLs can select the public origin based on the current request origin
- **Remote download and internal object APIs**
  - The remote storage driver implements presigned download capability
  - Internal object APIs support `Range: bytes=...` and `offset` / `length` query parameters
  - Remote presigned GET supports pass-through of `response-cache-control`, `response-content-disposition`, `response-content-type`
- **Upload and audit**
  - After a file upload completes, a `FileUpload` audit log is recorded, covering personal and team workspaces
  - If an upload completion retry finds the session already `Completed`, the audit log is not duplicated
- **Frontend experience**
  - The user sidebar supports drag and keyboard width adjustment, persisted to localStorage
  - File type icon logic optimized: image extensions uniformly show the image icon, avoiding non-code files mistakenly using a language icon

### Changed

- **Remote node enrollment**
  - `node enroll` no longer requires or accepts an ingress policy; follower onboarding is only responsible for establishing the master binding
  - Removed namespace, ingress policy id, and ingress policy name from the enrollment bootstrap response
  - The actual remote write target is now managed by the primary-side managed ingress profile
- **Remote write target resolution**
  - Follower internal storage requests no longer use `ingress_policy_id` on the master binding
  - Remote PUT / compose / list / get / delete uniformly compute the provider path via `storage_namespace + object_key`
  - The follower ready check confirms that enabled master bindings have a usable default managed ingress profile
- **Configuration system**
  - The system configuration API and CLI upgraded from plain string values to `SystemConfigValue`
  - CLI `config set` / `import` / `validate` support JSON array parsing and validation for the `string_array` type
  - Sensitive configuration remains masked in API responses and audit logs
- **Public URL generation**
  - Share, preview, WebDAV, and WOPI related URLs no longer hard-code a single public origin
  - If the current request origin matches the `public_site_url` list, an absolute URL for that origin is generated; otherwise it falls back to the first configured origin
- **Internal storage CORS**
  - CORS for the remote presigned internal object API expanded from PUT-only to GET / PUT / OPTIONS
  - Preflight now allows `content-type` and `range`
  - GET responses expose `Cache-Control`, `Content-Disposition`, `Content-Length`, `Content-Range`, `Content-Type`, `ETag`
- **Dependencies and versions**
  - Rust crate version bumped to `0.0.1-alpha.25`
  - Frontend dependency updates include `i18next`, `react-i18next`, `shadcn`, `@typescript/native-preview`, `jsdom`, `msw`

### Fixed

- **Object key and path security**
  - Added a unified object key helper that normalizes duplicate slashes, `.`, and backslashes, and rejects `..` path escapes
  - Remote object operations are forbidden from pointing directly at the storage namespace root
  - Prefix strip now only matches at full path segment boundaries, preventing `base` from incorrectly matching `baseball/...`
  - The local storage driver strengthens relative path sanitization and rejects parent directory escapes
- **CSRF origin validation**
  - CSRF origin validation supports multiple `public_site_url` origins
  - `Origin` / `Referer` must exactly match the request origin or any configured public origin
  - `Sec-Fetch-Site: same-site` is no longer unconditionally allowed; cookie-authenticated actions are rejected when no trusted `Origin` / `Referer` is present
- **Share access restrictions**
  - Share password cookie validation now loads the valid share record, ensuring expiration time and download count limits also apply at the password validation stage
- **Range and download**
  - Follower internal object GET adds strict Range parsing, rejecting multi-part ranges, invalid units, invalid boundaries, empty ranges, and out-of-bounds offsets
  - Range requests are not allowed for empty objects
  - S3 presigned download passes through response header overrides, fixing inconsistent filenames, content types, and cache control in remote / S3 download scenarios
- **Remote node enrollment**
  - Remote nodes that have completed onboarding cannot generate an enrollment command again
  - Added completed enrollment queries and integration test coverage
- **Task and thumbnail concurrency**
  - Task drain no longer exits early when there are no new claims but processing tasks remain
  - When thumbnail reads encounter a concurrent worker causing the cached object to transiently change, they are treated as a cache miss instead of surfacing a transient 500

### Breaking Changes

- **Database migrations (must run)**
  - `m20260425_000001_create_managed_ingress_profiles`: adds the managed ingress profile table
  - `m20260427_000001_drop_master_binding_ingress_policy_id`: removes the ingress policy binding on master bindings
  - `m20260429_000001_prepare_multi_primary_ingress`: migrates master binding namespaces and adjusts the scoping constraints of managed ingress profiles
- **Remote node upgrade risks**
  - This release restructures the follower write entry point and the multi-primary namespace binding model; after upgrading, historical write paths of old remote nodes may not automatically map to the new `storage_namespace + managed ingress profile`
  - If an old remote node used the legacy ingress policy / namespace binding, its files may appear invisible or seemingly lost after the upgrade
  - Back up the database and remote node storage directories before upgrading; after upgrading, check each remote node's managed ingress profile, default profile, and file access status
  - If a remote node cannot be restored to the correct write path, it may be necessary to delete and re-add the remote node, then reconfigure the managed ingress profile
- **`public_site_url` configuration format**
  - `public_site_url` changed from a string to a JSON string array
  - Old format: `https://drive.example.com`
  - New format: `["https://drive.example.com"]`
  - Wildcard origins are not supported; origins must be plain HTTP(S) origins — path, query, fragment, username, or password are not allowed
  - The public branding response field changed from `site_url` to `site_urls`
- **Follower onboarding model**
  - `node enroll` removed `--ingress-policy-id`
  - Removed `ASTER_BOOTSTRAP_REMOTE_INGRESS_POLICY_ID`
  - The `master_bindings.ingress_policy_id` database column was removed
  - After migration, managed ingress profiles must be configured on the primary side for remote nodes before the follower can accept remote writes
- **Namespace field migration**
  - `master_bindings.namespace` migrated to `master_bindings.storage_namespace`
  - `managed_followers.namespace` removed
  - The storage isolation namespace is no longer explicitly passed in by the primary when creating a remote node; it is now allocated on the follower master binding
- **Managed ingress local root directory configuration**
  - New configuration option `server.follower.managed_ingress_local_root`
  - Default value is `managed-ingress`
  - The old configuration key `server.managed_ingress_local_root` will be rejected; migrate it to `server.follower.managed_ingress_local_root`

### Notes

- Remote node users should upgrade with caution: this version may make old remote node files invisible or require re-adding remote nodes. Back up the database and follower storage directories before upgrading
- During the multi-primary ingress migration, if `managed_ingress_profiles` data already exists and there is more than one `master_bindings` entry, the migration cannot automatically determine which master binding an old profile should bind to and will abort, requiring manual handling
- The default profile of a managed ingress profile cannot be directly unmarked as default, nor can it be deleted while other profiles still exist; switch the default profile first
- An empty string for `public_site_url` is no longer a valid normalize input; configure it as an empty array or at least one HTTP(S) origin
- Reverse proxies that need to support remote presigned download must allow the `Range` request header and correctly forward download response headers

---

**Statistics**:
- 167 files changed, 8,806 insertions(+), 1,625 deletions(-)
- 22 commits

## [v0.0.1-alpha.24] - 2026-04-24

### Release Highlights

- **Unified media processing service landed** — Added a configurable media processing pipeline supporting built-in image processing, `vips_cli`, `ffmpeg_cli`, and storage-native thumbnail capabilities
- **Greatly enhanced thumbnail capabilities** — Thumbnails upgraded to v2, with processor metadata, old cache compatibility, public capability queries, and smart frontend fallback
- **Docker deployment with out-of-the-box media processing** — The Docker image bundles `vips-tools`, `ffmpeg`, `libheif`, and enables CLI media processors by default
- **Docker follower auto-enroll** — Followers support automatic first-start enrollment to the primary via environment variables, reducing manual steps for remote node deployment
- **Background task management enhancements** — The admin console supports filtering tasks by type/status, and adds the ability to clean up historical terminal-state tasks by criteria
- **Storage error classification system landed** — Storage driver errors are subdivided into authentication, permission, configuration, rate-limiting, transient failure, and other kinds, mapped to clearer API subcodes and frontend messages
- **More reliable upload completion flow** — Upload completion is handled uniformly; retryable transient storage errors no longer immediately mark a session as failed
- **Pre-beta data normalization** — Added migrations to clean up old thumbnail, preview app, remote upload strategy, and lock owner data formats

### Added

- **Media processing and thumbnails**
  - Added the `media_processing` configuration module, centrally managing the processor registry, defaults, extension matching, command normalization, and public thumbnail capability export
  - Added `media_processing_service`, unifying avatar processing, thumbnail generation, CLI input preparation, processor resolution, and shared processing logic
  - Added `vips_cli` and `ffmpeg_cli` media processors, supporting more image, video, and HEIC input formats via libvips / ffmpeg
  - Added public endpoint `/api/v1/public/thumbnail-support`, letting the frontend fetch supported extension capabilities before requesting thumbnails
  - `file_blobs` adds the `thumbnail_processor` metadata field, used together with `thumbnail_version` to distinguish caches produced by different processing pipelines
  - Storage policies add `thumbnail_processor = "storage_native"` and `thumbnail_extensions`, supporting per-extension binding of storage-native thumbnail capabilities
- **Admin console**
  - Added a media processing configuration editor supporting editing processor enablement, extension lists, and CLI commands, plus `vips` / `ffmpeg` availability probing
  - The system settings page adds a media processing configuration entry and related English/Chinese copy
  - The background tasks page adds a filter toolbar, task cleanup dialog, and a standalone task table component
  - Background task APIs add `kind` / `status` query filters, plus a `POST /admin/tasks/cleanup` cleanup endpoint
  - The frontend adds `thumbnailSupportService` and `thumbnailSupportStore`, centrally loading and caching public thumbnail capabilities
- **Docker and remote nodes**
  - Added a follower environment-variable auto-enroll service that writes the seed config on first start, redeems the enrollment token, and binds to the primary automatically
  - Added `docs/deployment/docker-follower.md`, documenting the Docker follower auto-enroll deployment process
  - The Docker image adds `vips-tools`, `ffmpeg`, `libheif`, and enables CLI media processing bootstrap configuration by default
- **Error system**
  - Added the `StorageErrorKind` classification system, covering authentication failure, permission denied, configuration error, object not found, rate limiting, transient failure, precondition failure, unsupported operation, and more
  - API error responses add structured `error` information containing `internal_code` and `subcode`
  - The frontend `ApiError` supports parsing `subcode`, and adds fine-grained error messages for uploads, thumbnails, avatars, storage, remote nodes, and more

### Changed

- **Media processing behavior**
  - Thumbnail generation upgrades from primarily built-in `image` processing to a unified pipeline resolved by processor priority
  - Thumbnail cache paths and ETags now incorporate `thumbnail_processor` and `thumbnail_version`, preventing wrong cache reuse across processors or versions
  - Avatar upload processing migrates to the unified media processing service, supporting built-in image processing and the `vips_cli` processing path
  - The frontend thumbnail component now reads the public support list first and requests thumbnails only for supported extensions, reducing pointless requests and error toasts
  - Thumbnail task payloads, display names, and completion results now include processor information, easing background task deduplication and troubleshooting
- **Upload and storage**
  - Upload completion is extracted into `run_upload_completion_stage`, uniformly handling assembling, completion, error recovery, and failure marking
  - Upload sessions recover to their original state on retryable storage errors, allowing clients to complete again; unrecoverable errors still mark the session as failed
  - The S3 driver upgrades error classification, recognizing provider errors such as `NoSuchKey`, `NoSuchUpload`, `SlowDown`, `Throttling`, and `ServiceUnavailable`
  - The remote storage protocol maps remote API error codes and HTTP statuses to local `StorageErrorKind`, making cross-node errors more consistent
  - AWS SDK S3 upgraded to `1.131.0`; `reqwest` upgraded to `0.13`
- **Background tasks and runtime**
  - Background task dispatch result handling is extracted into a dedicated function; successful tasks reduce log noise, and failures record runtime results
  - The admin task list switches to server-side filtering; the frontend persists task type and status filters via URL search params
  - Task cleanup adds a constraint to only delete terminal-state tasks, and supports combined filtering by completion time, task type, and terminal status
  - Follower mode continues to skip primary-only background tasks, keeping only follower-safe base tasks
- **Configuration and preview apps**
  - `config_service` is split into `actions`, `public`, `schema`, and `system` submodules
  - Built-in preview app keys uniformly add the `builtin.` namespace, e.g. `builtin.image`, `builtin.video`, `builtin.pdf`
  - Preview app configuration removes the legacy `label_i18n_key` field in favor of `labels` localized labels
  - The admin console removes a redundant hint block from the local storage policy page
  - System configuration default initialization supports enabling media processors via bootstrap environment variables
- **Internal refactoring**
  - `file_service/deletion.rs` is split into `soft_delete`, `purge`, and `blob_cleanup` submodules, with added concurrency and retry protection for blob cleanup
  - `user_service.rs` is split into `admin`, `models`, `preferences`, and `queries` submodules
  - The media processing module is split into a configuration layer and a service layer, clarifying CLI input preparation, processor resolution, and avatar/thumbnail processing responsibilities

### Fixed

- **Upload reliability**
  - Fixed sessions failing immediately on transient storage errors during upload completion; rate-limiting/transient failures are now retryable
  - Improved subcodes and frontend hints for direct relay, chunk, assembly, missing temporary object, and size mismatch upload errors
  - Fixed a risk where a quoted S3 multipart ETag could cause complete to fail
- **Thumbnails and media processing**
  - Fixed potential reuse of old caches across different thumbnail processors or versions
  - Fixed legacy thumbnail caches without version/processor metadata failing to migrate smoothly; added historical path reading and metadata backfill
  - Thumbnail output now validates format, dimensions, and size limits, preventing abnormal CLI output from being treated as a valid image
  - CLI input source preparation supports multiple strategies including local paths, presigned URLs, and streaming temp files, improving processing reliability on remote storage
  - The frontend falls back to a file icon when thumbnail loading fails, reducing repeated requests and error noise from unsupported formats
- **Storage and remote nodes**
  - Storage driver error display strips internal classification prefixes, preventing users from seeing unfriendly coded messages
  - The remote storage protocol classifies remote status codes, remote business errors, and network errors, helping clients distinguish authentication, permission, configuration, or transient failures
  - Docker follower bootstrap idempotently skips scenarios where the token is completed, expired, or replaced and a local binding already exists, avoiding repeated startup failures
- **Data cleanup and consistency**
  - Permanent file deletion logic enhanced: blob cleanup claims first, and failed deletions release the claim, preventing concurrent cleanup from wrongly deleting or leaving unrecoverable states
  - After a blob deletion failure, it checks whether the object no longer exists; if the object is gone, DB row deletion proceeds, improving cleanup idempotency
  - Expired resource lock cleanup checks for a replacement lock before clearing the `is_locked` cache, avoiding wrongly clearing lock state during concurrent relocking
  - Resource lock `owner_info` migrates from legacy XML / plain-text compatible forms to structured JSON, improving deserialization stability
- **Frontend error experience**
  - `useApiError` supports subcode-first mapping, making upload, thumbnail, avatar, storage, and remote node errors more specific
  - The HTTP client parses `error.subcode` from responses instead of relying only on top-level error codes
  - Added extensive English/Chinese error messages covering storage authentication, permissions, configuration, rate limiting, transient failures, unavailable thumbnail processors, avatar processing failures, and more

### Breaking Changes

- **Database migrations (must run)**
  - `m20260424_000001_normalize_thumbnail_metadata`: adds the `thumbnail_processor` field to `file_blobs`
  - `m20260424_000002_normalize_beta_compat_data`: cleans up pre-beta compatibility data; a one-way normalization migration
- **Preview app configuration**
  - Built-in preview app keys uniformly change to `builtin.*`; external configurations depending on old keys should verify the migration result
  - The preview app configuration schema no longer uses the legacy `label_i18n_key` field; use `labels` instead
- **Media processing configuration**
  - New system configuration option `media_processing_registry_json`
  - If `vips_cli` / `ffmpeg_cli` is enabled, the corresponding command must exist in the runtime environment
  - CLI media processing is enabled by default in the Docker image; non-Docker deployments wanting the same capability must install and configure `vips` / `ffmpeg` themselves
- **Storage policy configuration**
  - The old `remote_upload_strategy = "chunked"` will be migrated to `"presigned"`
  - `thumbnail_extensions` is only valid when `thumbnail_processor = "storage_native"`; otherwise configuration validation fails
- **API error structure**
  - API error responses add an `error` field; old clients ignoring it are unaffected, while new clients can use `subcode` for fine-grained hints
  - Storage error codes evolve from the generic `StorageDriverError` into more specific storage error types

### Notes

- Docker deployments now have more complete media processing capability by default; systemd / bare-metal deployments wanting the same capability must install `vips`, `ffmpeg`, and related codec dependencies themselves
- The down migration of `m20260424_000002_normalize_beta_compat_data` is empty; backing up the database before upgrading is recommended
- The frontend relies on `/api/v1/public/thumbnail-support` to decide whether to request thumbnails; reverse proxies must allow this public endpoint

---

**Statistics**:
- 206 files changed, 16,525 insertions(+), 4,013 deletions(-)
- 28 commits

## [v0.0.1-alpha.23] - 2026-04-22

### Release Highlights

- **Remote node storage architecture landed** — Added primary-follower mode, remote node management, and the enrollment onboarding flow, extending storage capability to independent nodes
- **Remote storage upload/download paths completed** — Remote storage supports `relay_stream` and `presigned` download strategies, with presigned direct upload and browser CORS support completed
- **Remote relay streaming chunked upload** — Added a remote node relay streaming upload path, reducing dependence on temporary disk writes on the primary for large file uploads
- **Auth session system upgrade** — Introduced the `auth_sessions` table with refresh token rotation, per-device session management, and revocation
- **Time zone preference and unified time display** — The frontend adds a time zone preference setting, unifies absolute time display formats, and adds UTC offset information in key scenarios
- **Remote node CLI and ops enhancements** — Added commands like `aster_drive node enroll` to simplify follower onboarding and operational troubleshooting
- **Documentation continues to grow** — Added documentation on remote nodes, custom frontends, direct-link download routing, login/sessions, and architecture

### Added

- **Remote nodes and remote storage**
  - Added remote node management APIs, the enrollment token / ack flow, and primary-follower binding capability
  - Added the `remote` storage driver and internal storage protocol, supporting remote health checks, file transfers, and policy integration
  - Added remote storage `presigned` direct upload, presigned download redirect, and relay streaming upload modes
  - Added the remote node admin console page, node dialogs, and onboarding flow UI
- **Authentication and session management**
  - Added the `auth_sessions` table and related migrations, supporting refresh token rotation and persistent session management
  - The security settings page adds a logged-in devices list and the ability to revoke the current or other sessions
- **User preferences and frontend experience**
  - Added user-defined preference key-value pairs
  - Added the `display_time_zone` preference field to control the time zone for absolute time display
  - Added session platform icon detection and display
- **CLI and documentation**
  - Added the `aster_drive node enroll` CLI command
  - Added documentation on remote nodes, archive tasks, custom frontends, and installation/deployment

### Changed

- **Upload and download strategies**
  - Unified upload strategy resolution logic; S3 and remote storage automatically select direct / chunked / presigned modes at initialization based on policy
  - Updated direct-link download documentation and storage policy descriptions, clarifying the behavior of `?download=1` when the presigned download strategy is hit
- **Authentication system**
  - Refactored the refresh token flow; authentication state now revolves around session records and the rotation mechanism
- **Frontend time display**
  - Absolute time display uniformly goes through the formatting utility and the user's time zone preference
  - Trash, sharing, settings, and other pages add clearer time zone information
- **Naming and architectural semantics**
  - `remote_node` renamed to `managed_follower`
  - `AppState` renamed to `PrimaryAppState`
  - Related runtime, service-layer, and route naming adjusted in sync to emphasize primary-follower semantics
- **Documentation and dependencies**
  - Expanded architecture and API documentation, improving storage, authentication, remote node, and deployment guides
  - Upgraded some frontend and backend dependencies, improving dialog animations and routing experience

### Fixed

- **Remote storage upload compatibility**
  - Improved browser CORS support in the remote presigned direct upload mode
- **Remote node reliability**
  - Added inbound file size limit validation
  - Improved concurrency logic for remote node health checks
- **Authentication security**
  - Refresh token reuse detection can now revoke the entire group of related sessions, shrinking the window of validity after token replay
- **Time display consistency**
  - Unified frontend absolute time display, reducing misreading across time zones

### Breaking Changes

- **Database migrations (must run)**
  - `m20260420_000001_create_auth_sessions`: adds the `auth_sessions` table for refresh token rotation and session management
  - `m20260420_000002_create_remote_nodes`: adds remote node, binding, and enrollment related tables

### Notes

- `remote_node` → `managed_follower` and `AppState` → `PrimaryAppState` are mainly internal naming refactors with no impact on external HTTP paths
- After the auth session mechanism upgrade, old login states may require re-login after upgrading
- Time display is now affected by the user's time zone preference; UI display may differ from older versions

---

**Statistics**:
- 427 files changed, 22,410 insertions(+), 3,511 deletions(-)
- 33 commits

## [v0.0.1-alpha.22] - 2026-04-19

### Release Highlights

- **In-house WebDAV protocol layer** — Removed the `dav-server` dependency with an in-house protocol dispatch layer, streaming reads/writes eliminating temp file overhead, and unified Basic Auth simplifying client compatibility
- **Background task system upgrade** — Introduced concurrency control and lease (heartbeat) mechanisms; thumbnail generation migrated to unified task-system scheduling, supporting safe multi-instance cooperation
- **WOPI Microsoft 365 proof-key verification** — Fully implemented RSA proof-key dual-key verification, rejecting future timestamps and replay attacks
- **Storage driver architecture refactor** — Separated driver capabilities via trait extensions (`ListStorageDriver` / `PresignedStorageDriver` / `StreamUploadDriver`)
- **Runtime temp directory isolation** — Short-lived temp files are isolated under `temp_dir/_runtime`; startup cleanup applies only to that subdirectory
- **Trusted proxies and rate-limit hardening** — The rate-limit middleware adds `trusted_proxies` CIDR configuration; `/auth` split into anonymous/authenticated rate-limit buckets
- **Major test infrastructure expansion** — Added test files such as `test_security_fixes` / `test_tasks` / `test_wopi` / `test_local_driver_security` / `test_health`

### Added

- **In-house WebDAV protocol layer and streaming I/O**
  - Removed the `dav-server` crate dependency and added an in-house protocol dispatch layer in `webdav/dav.rs` / `webdav/mod.rs` (full implementation of PROPFIND/PROPPATCH/MKCOL/COPY/MOVE/LOCK/UNLOCK)
  - Upload/download switched to fully streaming, eliminating temp file writes before upload
  - LOCK requests on nonexistent paths now return 404 instead of 423, per RFC 4918
  - Removed Bearer JWT authentication, unifying on Basic Auth (compatible with more clients such as Windows / macOS Finder / Cyberduck)
- **Background task concurrency control and lease mechanism**
  - Added the `background_task_heartbeat` field and lease takeover mechanism (migration `m20260417_000001`), supporting a multi-instance task system
  - Added `task_service/runtime.rs`, introducing concurrency limits and worker-pool scheduling
  - Thumbnail generation migrated from a channel queue to the `task_service/thumbnail.rs` background task system for unified management
  - Thumbnail metadata persisted to the `file_blob` table (migration `m20260417_000002`), avoiding redundant generation
- **WOPI proof-key verification**
  - Added `wopi_service/proof.rs`, implementing RSA proof-key + old-proof-key dual-key verification
  - `wopi_service/discovery` split into seven submodules: actions/apps/cache/parser/security/types/url
  - Rejects future timestamps and adds replay window validation
- **Online extraction security limits**
  - Added `archive_extract_max_staging_bytes` system configuration (default 2 GiB), limiting temporary disk usage per extraction
  - Pre-validates the combined size of the source archive and total extracted size before extraction
  - Validates per-entry file size permissions against the storage policy
  - Verifies actual written bytes against declared sizes, preventing ZIP entry size tampering
  - Automatically cleans up the staging temporary directory on failure
- **Security and filename normalization**
  - Added `security_headers` middleware for secure response headers, injecting CSP / `X-Frame-Options` / `Referrer-Policy`
  - Unicode NFC normalization for filenames, rejecting Windows reserved names (CON/PRN/AUX/NUL/COM*/LPT*)
  - Introduced the `validator` crate, adding field-level validation to all DTOs such as admin/teams/users/policies/batch/shares/properties/webdav/wopi, with `validate_request()` called uniformly at route entry
  - Share cookie signing changed from hand-rolled SHA256 concatenation to HMAC-SHA256, eliminating potential side channels
  - S3 presigned URL TTL clamped to an upper limit (max 1 hour), preventing long-lived credential leakage
- **Trusted proxies and rate limiting hardening**
  - Rate limiting middleware gained a `trusted_proxies` CIDR list, extracting the real IP from `X-Forwarded-For` per the allowlist
  - `/auth` routes split into separate `auth` and `api` rate limit buckets, preventing anonymous brute-force requests from exhausting authenticated users' quota
  - Rate limit configuration now validates against zero values
- **Download and email reliability**
  - Added `AbortAwareStream` + `on_abort` hook, rolling back `download_count` on client disconnect, eliminating inflated counts and premature `max_downloads` triggering
  - `share_repo` gained a `decrement_download_count_by` batch rollback method (guarding against count underflow)
  - Added `ShareDownloadRollbackQueue` async rollback queue and the `share_download_rollback_queue_capacity` system configuration
  - Email `mark_sent` now retries with backoff after SMTP success (up to 5 attempts, total budget ~7.6s), shrinking the "DB jitter → duplicate email" window
- **Streaming upload support**
  - Added a streaming upload path, bypassing actix-web's default 10MB payload limit
- **MIT License declaration** — `Cargo.toml` explicitly declares `license = "MIT"`
- **Documentation**
  - Added `docs/deployment/troubleshooting.md` troubleshooting guide (startup, upload/download, shares, WebDAV, Office/WOPI, background tasks, upgrade failures)
  - Added `docs/deployment/upgrade.md` upgrade and version migration guide (Docker / systemd procedures, MySQL large-table caveats, rollback steps)
  - Added `docs/guide/errors.md` error code handling manual
  - Added `docs/guide/about.md` project positioning and design principles
  - Added `developer-docs/module-designs.md` core module design documentation
- **Tests**
  - Added `tests/test_security_fixes.rs` (287 lines) covering CSRF, HMAC, proxy IP, proof-key, and other fixes
  - Added `tests/test_tasks.rs` (979 lines) covering task scheduling, leases, concurrency control, and archive compress/extract
  - Added `tests/test_wopi.rs` (345 lines) covering proof-key signature verification, locking, and session lifecycle
  - Added `tests/test_local_driver_security.rs`, `tests/test_health.rs`, `tests/test_directory_upload.rs`, `tests/test_edit.rs`, `tests/test_batch.rs`, `tests/test_files.rs`, and more
  - CI integration tests now support Postgres / MySQL backends

### Changed

- **Storage driver architecture**
  - Introduced a trait extension mechanism: `StorageDriver` split into a base trait plus three capability traits: `ListStorageDriver` / `PresignedStorageDriver` / `StreamUploadDriver`
  - Restructured directory layout: `storage/local.rs` → `storage/drivers/local.rs`, `storage/s3.rs` → `storage/drivers/s3.rs`, added `storage/extensions.rs`
- **API routes and DTO reorganization**
  - Added `api/dto` module to centrally manage all request/response structures (admin/auth/batch/files/folders/properties/shares/teams/trash/validation/webdav/wopi)
  - Merged personal / team workspace routes: removed `team_batch.rs` / `team_search.rs` / `team_shares.rs` / `team_space.rs` / `team_tasks.rs` / `team_trash.rs`, migrating logic into unified `batch` / `search` / `shares` / `folders` / `tasks` / `trash` modules
  - `auth.rs` split into `auth/cookies` / `auth/profile` / `auth/public` / `auth/session`, with each endpoint independently binding rate limiting middleware and `JwtAuth`
- **Security middleware refactor**
  - CSRF middleware split into constants / source / token / tests submodules
  - CORS middleware split into constants / mod / tests; added `RuntimeCors` supporting dynamic policies and WebDAV/WOPI protocol headers
  - Extracted `request_auth` module to unify token extraction logic (cookie / bearer)
- **Runtime temporary directory isolation**
  - Added `runtime_temp_dir` / `runtime_temp_file_path` functions
  - Only the `_runtime` directory is cleaned at startup, preserving background task artifacts such as `tasks`
  - Avoids mistakenly deleting other contents in shared temporary directories (e.g. `/tmp`)
  - WebDAV, file upload, WOPI, and other modules uniformly switched to the new temporary paths
- **Large module splitting**
  - `download` service split into `build` / `response` / `streaming` / `tests` / `types`
  - `upload_service/init` split into `context` / `s3` submodules; `complete` split out a `chunked` submodule
  - `workspace_storage_core` split into `blob` / `file_record` / `finalize` / `path` / `policy` / `quota`
  - `workspace_storage_service/store` split out a `from_temp` submodule
  - `cli/doctor` split into `execute` / `storage_scan` submodules
  - Frontend `useUploadAreaManager` split from a 1210-line single hook into `uploadAreaManagerShared/View`, `UploadRunners` (simple/resumable), `UploadTaskActions`, `useUploadAreaRestore`, `useUploadAreaUploads`, and other standalone modules
  - `TeamManageDialog` (1168 lines) split into `TeamManageShell` / `TeamManageSections` / `types`
  - `FileBrowserPage` split out `FileBrowserDialogs` / `useFileBrowserArchiveActions` / `useFileBrowserContextValue` / `useFileBrowserDragAndDrop` / `useFileBrowserPageState`
- **Code quality and defensive enhancements**
  - Enabled `clippy::cast_possible_truncation` / `cast_sign_loss` / `unwrap_used` lints, covering the main crate / migration / api-docs-macros
  - Globally replaced `as` numeric casts with `utils::numbers` safe conversion functions
  - Multi-parameter functions across services now take parameter structs (`StoreFromTempParams` / `StoreFromTempHints` / `CreateFileWithBlobInput` / `FolderListParams` / `CopyNameTemplate`, etc.), eliminating `clippy::too_many_arguments`
  - `get_ancestors_in_scope` now uses a single recursive SQL query instead of level-by-level loops
  - Background periodic tasks attach a `bg_task` span per iteration, correctly propagating trace context across awaits
- **Database**
  - Unified pagination query ordering to creation time descending
  - SQLite switched from `Database::connect` to `SqlxSqliteConnector`, fixing failures to connect with Windows backslash paths
  - Improved SQLite URL detection logic (`starts_with` instead of `contains`)
  - Added `db/transaction.rs` unifying the `begin/commit` transaction interface
- **i18n namespace unification**
  - Common keys such as `username` / `email` / `password` / `refresh` migrated to the `core` namespace, removing duplicate definitions in `admin` / `auth`
  - `share_expired` / `share_not_found` error messages migrated from the `share` to the `errors` namespace
  - `formatDate` supports an optional i18n parameter, providing an English relative-time default fallback (just now / Xm ago / Xh ago / Xd ago)
- **Frontend**
  - Multiple `ConfirmDialog` usages refactored into a `useConfirmDialog` hook, eliminating redundant open state
  - `useStorageChangeEvents` gained exponential backoff reconnection (30s cap, circuit-breaker threshold of 8) and `onopen` counter reset
  - `uploadPersistence` degrades gracefully on write failure: halving first when quota is exceeded, then clearing the key if still failing to prevent crashes
  - Added `FilePreviewBody` / `FilePreviewPanel` / `FilePreviewMethodChooser` / `AnimatedCollapsible` (supports `prefers-reduced-motion`)

### Fixed

- **WebDAV LOCK 404** — return 404 instead of 423 for non-existent paths, per RFC 4918
- **SQLite Windows paths** — failure to connect with backslash paths (switched to `SqlxSqliteConnector`), added a Windows-style path integration test
- **WOPI timestamp validation** — reject future timestamps to prevent replay attacks
- **Storage policy invalidation order** — `policy delete` / `update` now `invalidate driver` first, then `reload snapshot`, eliminating the silent misrouting window
- **Inflated download counts** — roll back `download_count` via `AbortAwareStream` when the client disconnects mid-download, avoiding premature `max_downloads` triggering
- **Duplicate email sends** — `mark_sent` failure retries with backoff, shrinking the duplicate-email window caused by DB jitter
- **Background task shutdown delay** — `shutdown` now uses `join_all + timeout` instead of 50ms polling
- **Rate limit configuration zero values** — fixed degenerate behavior when rate limit configuration is `0`
- **PDF preview cross-origin** — pass a Blob object instead of a blob URL to react-pdf, avoiding caching issues
- **CORS configuration conflict** — frontend validation now forbids enabling wildcard origins together with credentials
- **Silent path traversal** — log a warning when path resolution escapes `base_dir`, preventing misconfiguration from taking effect silently
- **`RUST_LOG` silent override** — append a warning when the environment variable is detected, noting that the `config.toml` level has been overridden
- **Multiple `unwrap` and unsafe `as` casts** — `build.rs`, database migrations, progress bars, retries, task scheduling, WebDAV `DavPath::root()` / `StatusCode::MULTI_STATUS`, etc.
- **Page layout** — missing `flex-col` in the flex layouts of `SettingsPage` / `ShareViewPage` / `TasksPage` and other pages

### Breaking Changes

- **WebDAV authentication** — removed Bearer JWT authentication; WebDAV clients must use Basic Auth (a dedicated WebDAV account is recommended)
- **Database migrations (must run)**
  - `m20260417_000001_add_background_task_heartbeat`: added heartbeat field to the background task table, supporting multi-instance leases
  - `m20260417_000002_add_file_blob_thumbnail_metadata`: added thumbnail metadata columns to the file_blob table
- **Storage driver trait split** — third-party storage driver implementations must additionally implement `ListStorageDriver` / `PresignedStorageDriver` / `StreamUploadDriver` traits as capabilities require
- **Temporary directory layout** — short-lived temporary files now live under `temp_dir/_runtime` after service startup; custom cleanup scripts assuming `temp_dir` is emptied directly need adjusting
- **Route module consolidation** — standalone route modules `team_batch` / `team_search` / `team_shares` / `team_space` / `team_tasks` / `team_trash` have been removed and merged into unified modules (external HTTP paths unchanged; only affects downstream development)

---

**Statistics**:
- 608 files changed, 41,139 insertions(+), 16,484 deletions(-)
- 33 commits

## [v0.0.1-alpha.21] - 2026-04-17

### Release Highlights

- **Full-text search acceleration (cross-database)** — unified indexing across SQLite FTS5 + trigram, PostgreSQL pg_trgm GIN, and MySQL ngram FULLTEXT backends; queries degrade automatically, with short queries falling back to LIKE
- **Global search dialog** — the top bar search was rebuilt as a global dialog summoned by `/` / `Ctrl+K`, with debounced search, keyboard navigation, infinite scrolling, and direct preview navigation from search results
- **Online archive and extraction tasks** — new multi-step background task framework supporting batch compression (ZIP) and single-file extraction, available in both personal and team workspaces
- **S3 presigned direct-link downloads** — storage policies gained S3 download strategy configuration; in `presigned` mode, after authentication a 302 redirects to a short-lived S3 URL, reducing server-side traffic
- **Large-scale service module splitting** — 12 large service files such as `auth_service`/`file_service`/`folder_service`/`team_service` split into submodules, with the route layer split in parallel
- **Test infrastructure optimization** — PostgreSQL template database + MySQL schema copying speed up test concurrency; degraded Argon2 test parameters for faster runs

### Added

- **Full-text search acceleration (FTS)**
  - SQLite FTS5 virtual tables + trigram indexes + sync triggers, speeding up file/folder/user/team searches
  - PostgreSQL `pg_trgm` GIN indexes, MySQL `ngram` FULLTEXT indexes
  - Extracted `search_acceleration.rs` shared utilities to uniformly generate table/trigger/rollback SQL
  - Abstracted `search_query.rs` builder functions: `sqlite_fts_match_condition`, `mysql_boolean_mode_query`, etc.
  - Refactored `search_repo`/`team_repo`/`user_repo`: automatically select the optimal query path
  - `doctor` command gained a `sqlite_search_acceleration` check
  - Dockerfile base image upgraded to Alpine 3.23
- **Global search dialog**
  - `GlobalSearchDialog` component: debounced search, keyboard navigation (↑↓/Enter/Esc), infinite scroll loading more
  - Search results grouped by file/folder, with thumbnail previews
  - TopBar search entry rebuilt, summoned by click or by pressing `/` / `Ctrl+K`
  - `AppLayout` registers global shortcuts; search results navigate directly to the target folder and open a preview
- **Online archive and extraction tasks**
  - Added `steps_json` field (background task step progress)
  - `createArchiveCompressTask`: batch-compress personal/team files into ZIP
  - `createArchiveExtractTask`: extract a single file (.zip) into a target folder
  - Task step state machine: `Pending`/`Active`/`Succeeded`/`Failed`/`Canceled`
  - Task detail panel collapsed by default, showing the step flow and timeline when expanded
- **S3 presigned downloads**
  - `S3DownloadStrategy` enum: `relay_stream` (default, streaming) / `presigned` (redirect)
  - Downloads routed by strategy: presigned returns a 302 to a signed S3 URL, carrying override headers such as `Content-Disposition`
  - `StorageDriver::presigned_url` gained a `PresignedDownloadOptions` parameter
  - Added an "S3 download method" selector to the storage policy edit page in the frontend admin panel
- **Audit logs pushed down to the service layer**
  - Added `*_with_audit` wrapper functions to batch operation/file/folder/share/upload services
  - Audit log calls moved from the route layer into the service layer, eliminating route-layer boilerplate

### Changed

- **Large-scale service module splitting**
  - 12 large services split into submodules: `auth_service`→password/registration/session/tokens, `file_service`→common/content/deletion/download/lock/thumbnail/transfer, etc.
  - `auth.rs` → `auth/mod.rs` + `auth/cookies.rs`，`files.rs` → `access/mutations/upload/versions`
  - Team workspace file routes migrated into unified `files/mod.rs` management
  - `repo` layer split in parallel: `file_repo`/`folder_repo` split by common/blob/mutation/query/trash
- **Strongly typed configuration sources and value types**
  - `SystemConfigSource`/`SystemConfigValueType` enums replace strings
  - `AuditAction`/`ThemeMode`/`ColorPreset`/`PrefViewMode`/`Language` moved into `types.rs`
  - Storage policy options/allowed_types changed from JSON strings to a `StoragePolicyOptions` struct
  - Task Payload/Result changed to tagged enums, distinguishing compress/extract types via `kind`
- **Non-deduplicated blob upload transaction decoupling**
  - Upload I/O moved outside the database transaction, with orphaned temporary files cleaned up automatically on failure
  - Added `PreparedNonDedupBlobUpload` enum and functions such as `prepare_non_dedup_blob_upload`
- **Graceful background task shutdown**
  - Introduced `CancellationToken` instead of blunt `abort`, with a grace period of up to 30s on shutdown
  - Periodic tasks add random jitter (up to 30s), avoiding multiple instances triggering cleanup races simultaneously
  - Extracted `run_periodic_iteration` for uniform panic capture
- **Folder tree requests honor sort preferences**
  - Folder tree requests now carry `sortBy`/`sortOrder`; the tree cache resets automatically when sorting changes
- **E2E test modularization**
  - Removed a 1391-line single file, splitting into independent specs by functional domain: `00-auth`/`admin`/`file-browser`/`shares`/`navigation`/`webdav`, etc.
  - Extracted `support/` shared utilities: `auth`/`files`/`network`/`shares`/`test`
- **Release build optimization level adjustment**
  - Cargo.toml `opt-level` changed from `"s"` (optimize for size) to `2` (optimize for performance)
- **Dockerfile base image upgrade**
  - Alpine 3.21 → 3.23
- **CI workflow naming**
  - `rust.yml` renamed to `Rust CI`, `frontend.yml` renamed to `Frontend CI`

### Fixed

- **MySQL timestamp 2038 overflow** — all `timestamp_with_time_zone` replaced with `utc_date_time_column`, using `DATETIME(6)` on MySQL; historical migration files updated accordingly
- **Upload cancellation race** — cancellation now introduces a grace period waiting for in-flight chunks to drain before cleanup; `mark_upload_session_completed` detects races where cancellation happens during assembly
- **MySQL full-text search minimum character count** — raised from 2 to 3, fixing empty results under `ngram` indexes
- **Test container orphaned database leak** — container lifecycle databases are tracked by PID; leftover test databases from exited processes are cleaned up automatically on next startup

### Breaking Changes

- **MySQL database migration (must run)** — `m20260415_000004_fix_mysql_utc_datetime_columns` changes all `TIMESTAMP` columns to `DATETIME(6)`; MySQL instances already in use must run the migration
- **Test infrastructure changes** — when `ASTER_TEST_DATABASE_BACKEND=postgres/mysql`, test container management has changed; see `developer-docs/testing.md` for details

---

**Statistics**:
- 347 files changed, 36,054 insertions(+), 21,310 deletions(-)
- 21 commits

## [v0.0.1-alpha.20] - 2026-04-15

### Release Highlights

- **End-to-end CSRF protection** — implemented Double Submit Cookie pattern CSRF protection; all Cookie-authenticated write operations must carry the `X-CSRF-Token` header, automatically injected by the frontend axios interceptor, with the backend additionally validating Origin/Referer/Sec-Fetch-Site origin trustworthiness
- **`doctor --deep` deep consistency check** — new `integrity_service` supports storage count drift detection, blob reference count verification, storage object inventory comparison (finding unowned/missing/orphaned objects), and directory tree structure validation (circular references/missing parents), with `--fix` for automatic repair
- **File info sidebar and fullscreen preview** — desktop file info panel redesigned from a dialog into a persistent sidebar with slide-in/slide-out animation and new quick-action area plus overview/status sections; file preview dialog adds fullscreen/restore window toggle
- **Comprehensive security hardening** — SVG/HTML inline sandbox CSP policy, Docker non-root runtime, Sigstore cosign signing, dependency security audit CI, minimum password length raised to 8, and fixed stack overflow in high-concurrency downloads
- **Large-scale refactoring** — file browser state management split into 7 slices, admin settings page componentized, WOPI service modularized, database migration tool modularized, team details split into components, `parking_lot` replacing std library locks


### Added

- **CSRF double-submit token protection**
  - New `csrf.rs` middleware on the backend: generates a 32-byte random token on login/refresh and writes it to the `aster_csrf` Cookie; non-safe requests validate the `X-CSRF-Token` header
  - Additionally validates origin trustworthiness via the `Origin`/`Referer`/`Sec-Fetch-Site` headers
  - Frontend axios interceptor automatically reads and injects the CSRF token from the Cookie; chunked uploads (XHR) attach it synchronously
- **`doctor --deep` deep consistency audit**
  - New `integrity_service`: storage count drift, blob reference counts, storage object inventory comparison, directory tree structure validation
  - Storage drivers add a `scan_paths` visitor interface (local traverses by directory, S3 consumes paginated streams)
  - CLI supports `--deep`, `--scope`, `--policy-id`, `--fix` parameters; keyset batching (1000 per batch) avoids full table loads
- **SVG inline sandbox and dual-mode preview**
  - HTML/SVG/XHTML files switched to inline responses + `Content-Security-Policy: sandbox` + `X-Content-Type-Options: nosniff`, allowing preview while blocking script execution
  - Frontend SVG files add image/code dual-mode preview toggle
- **File info sidebar**
  - Desktop `FileInfoDialog` redesigned as a persistent sidebar (220ms slide-in/slide-out animation); mobile keeps the dialog
  - New quick-action area: preview, download, share, rename, version history, lock (optimistic updates)
  - Info panel split into overview/status sections, introducing `DetailList`, `Section`, `ActionGrid` subcomponents
- **File preview fullscreen toggle**
  - Preview dialog adds a fullscreen/restore window toggle button
- **Automatic version number resequencing**
  - After deleting a historical version, subsequent version numbers are automatically decremented by 1 to keep displayed numbering contiguous
- **Dialog preloading**
  - New `lazyWithPreload` utility wrapping `requestIdleCallback` to preload dialog modules when idle
  - New `adminPolicyGroupLookup` module with global caching and request deduplication for policy group data
- **Mobile responsiveness improvements**
  - Breadcrumb navigation: on small screens with more than two levels, middle items collapse into an ellipsis dropdown; root directory uses a House icon
  - Toolbar, sort menu, and view toggle buttons adapted to small screen sizes
  - Hamburger menu List/X icon toggle animation, sidebar overlay opacity transition
- **Security infrastructure**
  - Docker container now runs as a non-root user with UID/GID 10001
  - CI adds Sigstore cosign signing (Docker images + Release checksums.txt)
  - CI adds weekly dependency security audit (`cargo audit` + `bun pm audit`)
  - Minimum password length raised from 6 to 8; new `existingPasswordSchema` ensures users with existing short passwords can still log in
- **E2E test suite**
  - Playwright E2E coverage: admin user CRUD, storage policy CRUD, file batch operations, chunked upload resumption, WebDAV PROPFIND/MKCOL/PUT/GET/DELETE, mobile layouts
- **k6 performance benchmarks**
  - 10+ benchmark scripts covering: login, token refresh, folder listing, search, download, direct/chunked upload, batch move, WebDAV read/write, long-run mixed load, staged concurrency ramp-up (mixed-ramp)
  - Download/upload/WebDAV scripts add byte counters, enabling throughput calculation directly from the summary
- **Documentation**
  - Reverse proxy documentation rewritten: complete config examples for Caddy/Nginx/Traefik; HTTPS changed from "recommended" to "required"
  - New backup and restore documentation covering SQLite/PostgreSQL/MySQL + local/S3 scenarios
  - New performance benchmark docs and Community Code of Conduct (`CODE_OF_CONDUCT.md`)


### Changed

- **File browser state management refactor**
  - `fileStore` split into 7 slices: `navigationSlice`, `searchSlice`, `selectionSlice`, `clipboardSlice`, `crudSlice`, `preferencesSlice`, `requestSlice`
  - Introduced `FileBrowserContext`/`FileBrowserProvider` to eliminate props drilling in `FileGrid`/`FileTable`
  - HTTP request layer adds `AbortSignal` support to prevent races in navigation/search/sort operations
- **File browser and team details component split**
  - `FileBrowserPage` split into `FileBrowserToolbar`, `FileBrowserWorkspace`, and other standalone components
  - `AdminTeamDetailDialog` split into `AdminTeamDetailShell`, `AdminTeamDetailSections`, and other subcomponents, supporting both page and dialog layouts
  - Extracted `useUploadAreaManager` hook to decouple upload area logic from the `UploadArea` component
  - New `useMediaQuery` hook encapsulating media query responsive logic
- **Admin settings page split**
  - `AdminSettingsPage` split from a 3220+ line single file into `CategoryContent`, `SaveBar`, `Dialogs`, and other subcomponents plus 3 custom hooks
  - `AdminPolicyGroupsPage` split into `PolicyGroupsTable`, `PolicyGroupDialog`, `PolicyGroupMigrationDialog`
- **WOPI service modularization and `parking_lot` adoption**
  - `wopi_service.rs` split into `locks`/`operations`/`session`/`targets`/`types`/`discovery`/`tests` submodules
  - Globally introduced `parking_lot` replacing std `Mutex`/`RwLock`, eliminating lock-poison boilerplate
- **Database migration tool modularization**
  - `database_migration.rs` split into `apply`/`checkpoint`/`helpers`/`schema`/`verify` submodules
- **WebDAV interface simplification**
  - `AppState` implements `Clone`; `AsterDavFs`/`AsterDavFile` now hold `AppState` instead of expanded multiple fields, eliminating lots of redundant parameter passing
- **SQLite row lock simplification**
  - Removed pseudo row-lock UPDATEs for SQLite in file_repo/folder_repo/team_repo, relying on the single-connection pool to serialize concurrency
- **Preview app configuration persistent cache**
  - `previewAppStore` adds localStorage caching with one-time per-session revalidation, instant hydration across refreshes
  - `FilePreviewDialog` merges dual Dialogs into a single Dialog
- **Unified global error mapping**
  - New `map_aster_err_with` method and extracted `display_error` utility function
  - Globally unified to the `map_aster_err_with(|| ...)` and `map_aster_err_ctx("ctx", f)` patterns
- **Legacy root layout compatibility code removal**
  - Deleted `reject_legacy_root_layout` and `LEGACY_*` constants — temporary compatibility paths introduced in alpha.17
- **Backend route refactor**
  - `team_scope` helper moved up to `routes/mod.rs`, removing duplicate definitions across team route modules
- **Dialog mount strategy**
  - All dialogs add `keepMounted` to avoid losing form input values when switching tabs
- **Redis cache error handling**
  - `set_ex`/`del`/prefix scans log a `warn` on failure instead of silently dropping
- **CI separation**
  - Frontend CI split from `rust.yml` into `frontend.yml`, triggered only on `frontend-panel/**` changes
  - Rust CI adds `cargo fmt --check` format checking
  - Added code coverage reporting to Codecov


### Fixed

- **Stack overflow in high-concurrency downloads** — the `RequestId` middleware changes `span.enter()` across `.await` to `.instrument(span)`, avoiding stack overflow from incorrectly nested request spans on actix workers ([`3ce13e2`](https://github.com/AsterCommunity/AsterDrive/commit/3ce13e2) Co-authored-by: AptS-1738)
- **Dangerous MIME type inline vulnerability** — HTML/SVG/XHTML files could be inlined and executed same-origin via direct and preview links; switched to a CSP sandbox policy
- **Misuse of password reset token** — when a password reset token was used on the contact verification endpoint it incorrectly hit `unreachable!`; now returns an `Invalid` redirect
- **Integer overflow in exponential backoff** — delay computation in `db/retry.rs` uses `checked_shl` and `saturating_mul` to prevent overflow
- **Mobile sidebar not filling full height** — `inset-y-16` split into `top-16 bottom-0`
- **Sidebar expand/collapse without animation** — switched to `translate-x` transition animation instead of display toggling
- **Dialog loses input values when switching tabs** — `<Wrapper>` JSX changed to a function call to prevent React remounting
- **RenameDialog not syncing external name changes** — added `useEffect` to sync the `currentName` prop
- **Long breadcrumb filenames breaking layout** — fixed overflow truncation styles
- **SVG image preview sizing out of control** — `BlobMediaPreview` handles SVG layout width separately
- **`public_site_url` using http without warning** — `doctor` returns a warn status for `http://` during checks


### Breaking Changes

- **CSRF token enforcement**: all Cookie-authenticated write operations must carry the `X-CSRF-Token` header; custom API clients need to read the token from the `aster_csrf` Cookie and inject it
- **Minimum password length changed from 6 to 8**: new registrations and password changes must meet 8 characters; users with existing 6-7 character passwords can still log in
- **Docker container runs as non-root**: mounted volumes must be readable/writable by UID/GID 10001; adjust `chown` or override with the `user:` directive
- **Legacy root layout compatibility code removed**: layouts from before alpha.17 with `config.toml`/`asterdrive.db` in the root directory no longer get migration hints


---

**Statistics**:
- 327 files changed, 32,763 insertions(+), 15,727 deletions(-)
- 29 commits


## [v0.0.1-alpha.19] - 2026-04-14

### Release Highlights

- **Cross-database backend migration tool** — new `aster-drive database-migrate` subcommand supporting offline full data migration between SQLite, PostgreSQL, and MySQL. Dependency-aware table copy ordering, resumable transfers, data integrity verification, and progress bar display
- **Offline health check** — new `aster-drive doctor` subcommand, similar to `brew doctor`, one-command check of database connection, migration status, runtime configuration, mail configuration, and storage policy integrity, with `--strict` mode
- **WOPI protocol completion** — five new WOPI operations: GET_LOCK, RENAME_FILE, PUT_USER_INFO, UnlockAndRelock, PutRelativeFile, greatly improving Office online editing compatibility
- **Unique index for same-name files/folders** — conditional unique indexes added at the database level, completely resolving same-name race conditions and data integrity issues in soft-delete scenarios
- **CLI module refactor and human output** — CLI split into a module directory structure, new human-readable terminal output format with color support and automatic format detection


### Added

- **Cross-database migration tool (`database-migrate`)**
  - Three run modes: `apply` (execute), `dry-run` (plan), `verify-only` (verify)
  - 22 tables copied in foreign key dependency order; resumable transfers support interrupt recovery
  - Automatic verification after migration: row count matching, unique constraints, foreign key constraints
  - Cross-backend type mapping (Bool/Int32/Int64/Float64/String/Bytes/TimestampWithTimeZone)
  - PostgreSQL/MySQL sequence auto-reset
  - Configurable batch size (`ASTER_CLI_COPY_BATCH_SIZE`, default 200)
- **Offline health check (`doctor`)**
  - Checks: database connection and backend type, migration status, runtime configuration snapshot, Public Site URL format, SMTP configuration completeness, preview app registry, storage policies and policy groups
  - `--strict` mode treats warnings as failures
- **WOPI protocol extensions**
  - GET_LOCK: query the current file lock value
  - RENAME_FILE: WOPI rename (automatically preserves extension, cleans illegal characters, truncates overlong names, auto-assigns on conflict)
  - PUT_USER_INFO: save/read WOPI user preferences (stored in `user_profiles.wopi_user_info`)
  - UnlockAndRelock: atomic lock swap operation
  - PutRelativeFile: create/overwrite adjacent files (Suggested mode auto-deduplicates names + Relative mode for exact specification)
  - CheckFileInfo adds `SupportsGetLock`/`SupportsRename`/`UserCanRename`/`SupportsUserInfo`/`FileNameMaxLength` fields
- **Database unique indexes**
  - `idx_files_unique_live_name`: unique constraint on file names in active state (distinguishing personal/team workspaces)
  - `idx_folders_unique_live_name`: unique constraint on folder names in active state
  - `idx_contact_verification_tokens_single_active`: only one unconsumed verification token per user/channel/purpose
  - `user_profiles.wopi_user_info` column (VARCHAR(1024))
- **CLI human output format**
  - Terminal auto-detection: human format for terminals, JSON for piped output
  - Color output: supports `CLICOLOR_FORCE` / `NO_COLOR` environment variables
  - Sensitive value masking, multiline value summaries, source badges (`[system]`/`[custom]`)
  - Progress bar display (database-migrate)
- **Ops CLI documentation** — new `docs/deployment/ops-cli.md` with a complete usage guide for doctor/config/database-migrate; cross-referenced in the README and across the docs site


### Changed

- **CLI module structure refactor**
  - Split from the single `cli.rs` file into a module directory: `cli/config.rs`, `cli/doctor.rs`, `cli/database_migration.rs`, `cli/shared.rs`
  - Extracted common utilities to `cli/shared.rs`: OutputFormat, CliTerminalPalette, Success/ErrorEnvelope
- **`/auth/check` endpoint simplification**
  - Removed the `CheckReq` request body (previously containing an `identifier` field); the endpoint now only returns instance auth state
  - `operation_id` changed from `check_identifier` to `check_auth_state`
  - Frontend `authService.check()` and `LoginPage` updated accordingly
- **Background task management**
  - New `BackgroundTasks` struct collects all JoinHandles
  - Panic capture moved from child task spawn to `AssertUnwindSafe + catch_unwind`
  - Shutdown order changed to: abort background tasks first → then close database connections
- **config_repo upsert optimization**
  - `upsert_with_actor` changed to INSERT ON CONFLICT DO NOTHING + TryInsertResult check
  - Eliminates the SELECT-then-INSERT race condition
- **File copy retry logic**
  - File/folder copy changed from check-then-create to try-create-and-retry (up to 32 attempts)
  - Completely eliminates TOCTOU race conditions in copy operations
- **WOPI error responses**
  - 403 is no longer mapped to 401; uses standard actix_web error responses instead
- **Storage quota calculation**
  - On file overwrite, quota increment changed to the full size of new content (rather than the delta)


### Fixed

- **Same-name file/folder conflicts** — issues such as being unable to create same-name files after soft delete, restore conflicts from the trash, and name release after batch operations are thoroughly resolved via database unique indexes
- **Duplicate verification token sends** — repeated requests for verification emails for the same user/channel/purpose no longer send new emails; the unique index guarantees only one active token
- **Unique constraints on user registration/email change** — distinguishes username vs email conflicts, returning more precise error messages
- **SQLite URL missing write mode** — SQLite URLs without query parameters automatically get `?mode=rwc` appended


### Breaking Changes

- **`/auth/check` endpoint change**: request body removed; `operation_id` changed from `check_identifier` to `check_auth_state`; clients depending on this endpoint must remove the `identifier` parameter
- **CLI output format default behavior**: the `config` subcommand now outputs human format by default in terminals instead of JSON; scripts relying on JSON output must explicitly specify `--output-format json`
- **WOPI CheckFileInfo response change**: `UserCanNotWriteRelative` changed from `true` to `false`; several capability declaration fields added
- **Storage quota calculation change**: quota increment on file overwrite is now the full size of new content; users near the quota limit may be affected
- **Database schema**: 4 new migrations (unique indexes + wopi_user_info column); database migrations must be run. The unique index migrations automatically clean up existing duplicate data


---

**Statistics**:
- 71 files changed, 10,354 insertions(+), 1,030 deletions(-)
- 9 commits


## [v0.0.1-alpha.18] - 2026-04-13

> **⚠️ Required reading before upgrading**: this version moves configuration and database files to the `data/` directory. Manual migration is required before upgrading:
> ```bash
> mkdir -p data
> mv config.toml data/
> mv asterdrive.db data/        # SQLite users
> ```
> Un-migrated old instances will refuse to start and prompt with the migration steps.

### Release Highlights

- **Ops CLI** — New `aster-drive cli` subcommand system for offline viewing, modifying, and importing/exporting runtime configuration, enabling ops work without the web admin console
- **Config files migrated to data/ directory** — `config.toml` and the SQLite database file are now consolidated under the `data/` directory, normalizing the data layout. Old layouts are auto-detected with a migration prompt
- **Preview app configuration v2** — Preview app configuration refactored from rule-matching mode to direct extension-binding mode, simplifying configuration logic. New WOPI Discovery auto-import can generate preview app configurations from Collabora/OnlyOffice in one click
- **Service layer DTO refactor** — All API responses switched from exposing database entity models directly to returning dedicated DTOs, strengthening API contract stability and security
- **Multiple security and performance improvements** — Unified permission checks for batch operations, cursor-based batching for trash cleanup, database-side pagination of team members, Redis log credential redaction


### Added

- **Ops CLI**
  - New `cli config` subcommands: `list`/`get`/`set`/`delete`/`validate`/`export`/`import`
  - Environment variable parameter passing: `ASTER_CLI_DATABASE_URL`, `ASTER_CLI_CONFIG_KEY`, etc.
  - Output formats: JSON / Pretty JSON with the standard envelope structure
  - User-identity-free writes: config writes support CLI scenarios (`upsert_with_actor`)
- **WOPI Discovery auto-import**
  - `execute_config_action` adds the `build_wopi_discovery_preview_config` action
  - Parses WOPI Discovery XML to auto-generate WOPI preview app configurations
  - Smart deduplication: identifies already-imported apps by discovery_url, preserving user-disabled states
  - New Discovery URL input dialog on the frontend
- **Admin console trend chart enhancements**
  - Overview trend chart expanded from one line to 4 lines (total events, uploads, share creations, new users), with custom tooltips
- **End-to-end debug instrumentation**
  - Added `tracing::debug` logs to core paths such as authentication, file/folder operations, search, and upload
- **API documentation**
  - Added docs for the WOPI API, batch archive download, and background task API
  - Configuration docs rewritten (five-layer config structure), user guide and deployment docs updated


### Changed

- **Preview app configuration v2**
  - Config version bumped to v2: `rules` field removed, extension lists declared directly on the app
  - Merged `builtin.formatted_json` and `builtin.formatted_xml` into `builtin.formatted`
  - Frontend editor switched to dialog mode, with a new "Add app" selection dialog (Embed/URL template/WOPI Discovery)
- **Config file path migration**
  - `config.toml` moved to `data/config.toml`, SQLite default path changed to `data/asterdrive.db`
  - Old layouts auto-detected; the service refuses to start and shows the migration steps
- **Service layer DTO refactor**
  - New `workspace_models` (FileInfo/FolderInfo/FileVersion) and per-service DTOs
  - New `workspace_scope_service` to centralize scope validation
  - All service-layer public function return types replaced with DTOs instead of entity models
- **Batch operation permission checks**
  - `load_normalized_selection_in_scope` uniformly handles delete/move/copy permission checks
  - New `find_by_ids_in_scope` repo method family to prevent cross-scope privilege escalation
- **Trash cleanup**
  - `purge_all` now processes in cursor-based batches of 100, reducing memory pressure for large datasets
- **Team member list**
  - Switched from full in-memory loading to database-side filtering/sorting/pagination
- **Upload path resolution**
  - Split into `parse_relative_upload_path` (validation) + `ensure_upload_parent_path` (creation), decoupling validation from creation logic
- **Legacy storage policy cleanup**
  - Removed the `user_storage_policies` table and the `user_profiles.avatar_policy_id` field
  - Cleaned up deprecated user-policy CRUD methods in `policy_repo`
- **Background task type slimming**
  - Removed `BackgroundTaskKind::ArchiveDownload` (archive download now streams directly via stream ticket)


### Fixed

- **Share password state misjudged** — Updating a share without the password field incorrectly cleared the existing password; the original password state is now preserved
- **Team archive deletion atomicity** — Introduced transaction locks for concurrency safety; cleanup tolerates missing targets on failure
- **Redis log credential leak** — Connection logs automatically strip username/password from URLs


### Breaking Changes

- **Config file paths**: `config.toml` and the SQLite database file must be manually migrated to the `data/` directory; starting with the old layout will fail with a migration-steps prompt
- **Preview app configuration v2**: config format upgraded from v1 to v2 (`rules` removed, extensions declared directly on the app); custom preview app configurations must be reconfigured
- **Database schema**: removed the `user_storage_policies` table and the `avatar_policy_id` field; database migration required
- **ArchiveDownload task type removed**: `BackgroundTaskKind::ArchiveDownload` has been deleted; archive download now streams directly via stream ticket


---

**Statistics**:
- 143 files changed, 7,850 insertions(+), 5,115 deletions(-)
- 7 commits


## [v0.0.1-alpha.17] - 2026-04-12

### Release Highlights

- **WOPI protocol support** — Full implementation of the WOPI (Web Application Open Platform Interface) protocol, enabling integration with Collabora Online, OnlyOffice, and other WOPI-compatible office suites for online document editing. Includes CheckFileInfo, GetFile/PutFile, full locking mechanism, Discovery caching, and Access Token management
- **Preview app system refactor** — Refactored hardcoded file preview logic into a rule-engine-based configurable "preview app" system. Supports three providers (Builtin/UrlTemplate/Wopi), with a visual configuration editor in the admin console and 12 built-in preview apps
- **Background task system and archive download** — New generic background task framework (state machine, auto-retry, exponential backoff, expiry cleanup), plus multi-file/folder ZIP streaming download based on stream tickets
- **Thumbnail system optimization** — Introduced thumbnail versioning (v2), source file size limits, viewport lazy loading, and concurrent worker tuning to lower peak memory and improve loading experience
- **Operations and scheduling configuration** — New operations configuration category; mail dispatch intervals, task scheduling intervals, and maintenance cleanup cycles are all hot-editable in the admin console. Settings page adds time/size unit pickers


### Added

- **WOPI protocol**
  - New `wopi_service`: CheckFileInfo, GetFile/PutFile, full locking mechanism (lock/unlock/refresh), Discovery XML caching
  - WOPI endpoint routes: `/api/v1/wopi/files/{id}` and the `/contents` sub-routes
  - `wopi_sessions` table: Access Token storage (SHA-256 hashed), expiry cleanup
  - Runtime configuration: `wopi_access_token_ttl_secs`, `wopi_lock_ttl_secs`, `wopi_discovery_cache_ttl_secs`
  - Frontend `WopiPreview` component: submits the token to the WOPI action_url via a hidden form POST, supporting iframe/new_tab modes
  - CORS middleware adds WOPI-related request/response headers
  - Full integration test coverage (1400+ lines)
- **Preview app system**
  - New `preview_app_service`: three provider types, rule engine matching files to preview apps by extensions/mime_types/categories
  - `PublicPreviewAppsConfig` stored in the `system_config` table, with 12 built-in apps (image, video, audio, pdf, markdown, table, formatted_json, formatted_xml, code, try_text, office_google, office_microsoft)
  - `UrlTemplatePreview` / `EmbeddedWebAppPreview` generic preview components
  - Admin console `PreviewAppsConfigEditor` visual editor (2700+ lines), supporting app add/edit/delete, rule editing, and validation
  - 14 SVG preview app icons
  - `/api/v1/public/preview-apps` public endpoint
- **Background task framework**
  - New `task_service`: task dispatch (batch claiming), state machine (pending→processing→succeeded/failed/retry), auto-retry (exponential backoff), expiry cleanup
  - `background_tasks` table: fields including kind, status, progress, payload_json, attempt_count
  - Task API: `GET /api/v1/tasks` (paginated list), `GET /api/v1/tasks/{id}` (detail), `POST /api/v1/tasks/{id}/retry` (manual retry)
  - Team workspace task API (same structure)
- **Archive download**
  - `stream_ticket_service`: one-time download tokens (valid for 5 minutes) with moka caching
  - `POST /api/v1/batch/archive-download` + `GET /api/v1/batch/archive-download/{token}` endpoints
  - Team workspace archive download routes
  - New "Archive download" option in the file context menu/batch action bar
- **Operations and scheduling configuration**
  - `operations` configuration category: `mail_outbox_dispatch_interval_secs`, `background_task_dispatch_interval_secs`, `maintenance_cleanup_interval_secs`, `blob_reconcile_interval_secs`, `team_member_list_max_limit`, `task_list_max_limit`, `avatar_max_upload_size_bytes`, `thumbnail_max_source_bytes`
  - Settings page adds a time unit picker (seconds/minutes/hours/days/weeks) and a size unit picker (bytes/KB/MB/GB/TB), auto-detecting the most suitable unit
  - New `auth_register_activation_enabled` config item (whether email activation is required after registration)
  - Refined settings categories: `user` split into `user.registration_and_login` + `user.avatar`, new `general.preview` subcategory


### Changed

- **Thumbnail system**
  - Storage paths now versioned: `_thumb/v2/{hash...}.webp`, old-path thumbnails auto-cleaned
  - ETag format changed to `thumb-v2-{blob_hash}`, share page cache policy changed to `must-revalidate`
  - Max concurrent workers reduced from `min(cpu, 4)` to `min(cpu, 2)`
  - Workers receive the `runtime_config` parameter to read dynamic configuration
  - Frontend thumbnails support viewport lazy loading (`IntersectionObserver`) and loading-state indicators
- **Background periodic task scheduling**
  - `spawn_periodic()` interval changed from a fixed Duration to a closure reading dynamically from runtime configuration
  - All periodic tasks (upload/trash/lock/audit cleanup, etc.) uniformly use the `maintenance_cleanup_interval` configuration
- **File preview architecture**
  - `OpenWithMode` changed from a restricted enum to an open string type, allowing the server to define arbitrary open-with modes
  - `formatted` preview mode split into `formatted_json` and `formatted_xml`
  - Removed old components such as `OfficeOnlinePreview`, `OpenWithChooser`, and `PreviewModeSwitch`
- **CORS middleware**
  - Allowed-headers list changed from hardcoded strings to dynamic concatenation of the `ALLOWED_HEADERS` constant array


### Fixed

- **Admin settings page** — Desktop navigation bar changed to sticky positioning, fixing the nav not following on long-page scrolling
- **Brand asset preview** — favicon and dark wordmark preview boxes now have a white background for consistent appearance across themes


### Breaking Changes

- **Database schema**: added `background_tasks` and `wopi_sessions` tables; database migration required
- **Thumbnail paths**: storage path changed from `_thumb/{hash...}` to `_thumb/v2/{hash...}`; after upgrading, old thumbnails are auto-cleaned and regenerated on access
- **Thumbnail ETag**: format gains the `thumb-v2-` prefix; old ETags cached by clients will become invalid
- **Preview app configuration**: the `frontend_preview_apps_json` format has been fully reworked (new version, provider, config, etc. fields); custom configurations must be reconfigured
- **Settings category keys**: the `user` category split into subcategories and `general` gains `general.preview`; automation scripts relying on category names may be affected


---

**Statistics**:
- 191 files changed, 19,997 insertions(+), 2,048 deletions(-)
- 7 commits


## [v0.0.1-alpha.16] - 2026-04-09

### Release Highlights

- **Mail system** — Introduced lettre/SMTP mail service, a new outbox async delivery queue, and 5 customizable HTML mail templates (registration activation, email change, password reset, etc.), with online template editing in the admin console
- **Complete authentication flows** — Added email verification activation, email change confirmation, and password reset flows; all sensitive operations trigger mail notifications. New registration toggle configuration supports disabling public registration
- **Office online preview** — Supports Microsoft Office Online and Google Docs providers for online preview of Word/Excel/PowerPoint/ODF documents. New preview link service generates time- and count-limited preview tokens
- **Real-time file change push (SSE)** — The backend broadcasts file/folder change events via Server-Sent Events; the frontend auto-refreshes the current directory, and users can toggle real-time sync in settings
- **Site branding configuration** — Custom site title, description, favicon, and light/dark logos (wordmark); custom branding shows on pre-login pages


### Added

- **Mail infrastructure**
  - New `mail_service.rs`: lettre-based SMTP mail sending with TLS/STARTTLS support
  - New `mail_outbox` table: async mail delivery queue with failure retry
  - Background tasks periodically handle mail retries (`spawn_background_tasks` adds a mail processing task)
  - New `MemoryMailSender` for test environments
- **Mail template system**
  - 5 built-in HTML templates: registration activation, email change confirmation/notification, password reset/notification
  - Template variable substitution: `{{username}}`, `{{verification_url}}`, `{{reset_url}}`, etc.
  - New mail template editing page in the admin console with expand/collapse group editing
- **Email verification flow**
  - Activation email sent after registration; logging in with an unactivated account returns the `PendingActivation` error code
  - Frontend login page adds a pending-activation notice panel + activation email resend
  - Email changes require confirmation: a change confirmation email to the new address, plus a notification email to the old address
- **Password reset**
  - `POST /auth/request_password_reset` + `POST /auth/confirm_password_reset`
  - Reuses the `contact_verification_token` infrastructure with a new `PasswordReset` verification purpose
  - `session_version` rotates automatically after a successful reset, forcing all existing sessions to become invalid
  - Sends a reset-link email and a reset-success notification email, and records audit logs
- **Registration toggle**
  - New `auth_allow_user_registration` runtime config item (default `true`)
  - When disabled, `/auth/register` returns 403; the `/auth/setup` initialization flow is unaffected
  - Frontend login page hides the registration entry based on configuration
- **Office online preview**
  - New `OfficeOnlinePreview` component supporting Microsoft Office Online / Google Docs
  - Timeout detection, localhost/HTTP link error prompts, and retry
  - Enhanced file type detection: doc/docx/xls/xlsx/ppt/pptx/odt/ods/odp files classified into document/spreadsheet/presentation categories
- **Preview link service** (`preview_link_service`)
  - Generates usage-count-limited preview tokens for personal/team files and shared files
  - `GET /pv/{token}/{filename}` route provides inline download
  - Tokens valid for 5 minutes with a maximum of 5 uses
- **Real-time file change push (SSE)**
  - `storage_change_service`: broadcasts file/folder change events via a broadcast channel
  - `GET /auth/events/storage` SSE endpoint, with heartbeat keepalive (30s) and message-backlog degradation
  - Frontend `useStorageChangeEvents` hook: subscribes to real-time changes and auto-refreshes the current directory
  - User preference `storage_event_stream_enabled` field, toggleable in settings
- **Site branding configuration**
  - New `branding_title`, `branding_description`, `branding_favicon_url` config items
  - New `branding_wordmark_dark_url`, `branding_wordmark_light_url` logo configuration
  - Frontend fetches branding configuration at startup via `/api/v1/public/branding`
  - Backend injects brand placeholders when rendering `index.html`, showing custom branding even before login
- **Frontend enhancements**
  - `usePageTitle` hook: dynamic titles on all pages, formatted as `Page · App`
  - `AdminSiteUrlMismatchPrompt` standalone component: site URL mismatch detection and update
  - CORS gains a separate `cors_enabled` toggle configuration


### Changed

- **Authentication flow refactor**
  - `/auth/check` no longer accepts the `identifier` parameter and instead returns public authentication status (registration toggle, setup state, etc.)
  - Frontend login page fetches authentication status once at page initialization, removing debounced input checks
  - Unified minimum response time to prevent user enumeration attacks
- **Avatar storage migration**
  - Migrated from object storage policies to the local filesystem, with a new `avatar_dir` config item
  - Recursive cleanup of empty directories on deletion
  - Compatible with old `avatar_policy_id` records for a smooth migration
- **Admin console settings page**
  - Default route changed from `/admin/settings/auth` to `/admin/settings/general`
  - New mail template editing section
- **CI improvements**
  - Replaced `actions/cache` with `Swatinem/rust-cache@v2`, simplifying configuration


### Fixed

- **Code editor**
  - Word wrap disabled by default (`wordWrap: off`)


### Breaking Changes

- **Authentication API**: `/auth/check` removes the `identifier` parameter and now returns global authentication status. The frontend must adapt to the new login initialization logic
- **Registration activation**: email verification becomes a required registration step (SMTP must be configured); unactivated accounts cannot log in
- **Password reset**: `session_version` rotates automatically after a successful reset, forcing all existing sessions to become invalid
- **Avatar storage**: newly uploaded avatars are stored on the local filesystem (`avatar_dir`), no longer using object storage policies
- **Admin console**: settings page default route changed from `/admin/settings/auth` to `/admin/settings/general`
- **CORS**: new independent `cors_enabled` toggle that must be explicitly enabled


---

**Statistics**:
- 243 files changed, 19,542 insertions(+), 1,920 deletions(-)
- 15 commits


## [v0.0.1-alpha.15] - 2026-04-07

### Release Highlights

- **Direct File Link Sharing** — Added Direct Link sharing mode, generating direct download links that bypass the share page. Supports a force-download parameter and independent rate limiting. The frontend share dialog can switch between share-page and direct-link modes with one click
- **Runtime Authentication Policy** — Migrated authentication settings such as Cookie security policy and Token TTL from static config.toml to database runtime configuration, allowing admins to adjust them in the backend in real time without restarting the service
- **Admin Settings Page Rework** — System configuration organized into categorized tab navigation (Auth/Network/Storage/WebDAV/Audit/General/Custom), with batch save, sensitive value masking, default value display and one-click restore, and i18n labels
- **Avatar Cropping** — Added a circular cropper with zoom and position adjustment, outputting 1024×1024 WebP format
- **Mobile Responsive Improvements** — Dialogs and settings pages fully adapted to mobile layouts; tabs gained transition direction detection


### Added

- **Direct File Link Service**
  - Added `direct_link_service.rs`: generates signed direct-link download tokens
  - API endpoints: `GET /api/v1/files/{id}/direct-link`, `GET /api/v1/team-space/files/{id}/direct-link`
  - Public download endpoint: `GET /d/{token}/{filename}`, supports `?download=1` to force download
  - Independent rate limiting configuration
- **Runtime Authentication Configuration**
  - Added `auth_runtime.rs`: reads `auth_cookie_secure`, `auth_access_token_ttl_secs`, `auth_refresh_token_ttl_secs` from the database
  - New static config option `bootstrap_insecure_cookies` (effective only on first initialization)
  - Cookie path isolation: Access Token → `/`, Refresh Token → `/api/v1/auth/refresh`
- **Avatar Cropping**
  - Added `AvatarCropDialog` component + `avatarCrop.ts` utilities
  - Based on `react-image-crop`, circular crop box + live preview
- **Frontend Share Enhancements**
  - Share dialog now offers dual-mode switching: Share page / Direct link
  - Direct-link mode does not support password or expiry time; supports generating force-download links
  - File context menu supports choosing the share mode directly
- **System Configuration i18n**
  - Configuration definitions gained `label_i18n_key` / `description_i18n_key` fields
  - Configuration items support categories: auth / network / storage / webdav / audit / general
  - Sensitive value flag (`is_sensitive`) and restart-required flag (`requires_restart`)
  - Chinese and English translations cover all system configuration items
- **UI Component Enhancements**
  - Select gained a `width` variant (compact / page-size / fit / full)
  - Tabs `line` variant supports full-width style + transition direction detection
  - Audit log page supports URL parameter sync, per-page item count selection, and active filter indicators


### Changed

- **Authentication Service Rework**
  - `issue_tokens_for_user` now obtains Token TTL and Cookie policy from runtime configuration
  - Share verification Cookie gained security flags and path isolation (`/api/v1/s/{token}`)
- **Admin Settings Page**
  - Reworked into categorized tab navigation (sidebar on desktop, dropdown on mobile)
  - New batch save mechanism (draft value management)
  - Sensitive values displayed masked (`********`), with default value display and one-click restore
- **Dialog Responsive Layout**
  - `AdminTeamDetailDialog` / `TeamManageDialog` / `UserDetailDialog` fully adapted to mobile
  - Two-column layouts rebuilt with flex + overflow-hidden, adapting to a single column on mobile
  - Added scroll position memory and tab transition direction detection
- **Select Component**
  - Removed hardcoded height, replaced with a variant system
  - Admin pages uniformly use the `width` prop


### Fixed

- **Cookie Security Policy**
  - Fixed inability to log in on first deployment in plain HTTP environments (`bootstrap_insecure_cookies` bootstrap config)
- **Audit Log Page**
  - Fixed filter and pagination state not persisting or being shareable via URL
- **Mobile Layout**
  - Fixed chaotic scrolling behavior of admin dialogs on mobile
  - Fixed bottom buttons being cut off in the user detail dialog


### Breaking Changes

- **Configuration File**: `[auth]` section removed `access_token_ttl_secs`, `refresh_token_ttl_secs`, `cookie_secure`, now runtime configuration. Added `bootstrap_insecure_cookies` (effective only on first initialization)
- **Cookie Behavior**: Refresh Token Cookie path restricted from `/` to `/api/v1/auth/refresh`; share verification Cookie path restricted to `/api/v1/s/{token}`
- **Frontend Routing**: Admin settings page gained sub-route `/admin/settings/:section`


---

**Statistics**:
- 99 files changed, 6,749 insertions(+), 1,629 deletions(-)
- 7 commits


## [v0.0.1-alpha.14] - 2026-04-05

### Release Highlights

- **Team Workspaces** — Added full team lifecycle management: create teams, invite members, assign roles (Owner/Member), and multi-workspace file isolation. Share links gained team-scope support, making team collaboration smoother
- **Upload Performance Optimization** — Removed the proxy_tempfile intermediate strategy and added a relay_stream no-staging streaming fast path; local storage uploads skip the global temp directory, reducing small-file upload latency
- **Custom CORS Middleware** — Replaced actix-cors with a runtime-configurable custom implementation supporting dynamic cross-origin policy adjustments that take effect immediately in the admin console
- **Admin Route Restructuring** — Split the bloated admin.rs into 8 independent submodules (users/policies/teams/shares/config/locks/audit_logs/overview), improving code maintainability
- **Fine-Grained Thumbnail Errors** — Distinguished 202 (generating), 400 (unsupported type), 500 (generation failed) status codes, enabling more precise user feedback in the frontend


### Added

- **Team Features**
  - Added `teams` / `team_members` / `team_spaces` database tables with soft-delete support
  - Complete Team API: create, update, delete, member management, workspace listing
  - Team workspace file management: team file storage independent of user workspaces
  - Shares support team scope (`team_id` field); team members can access team shares
  - Full frontend `TeamManagePage` / `TeamsSettingsView` / `TeamManageDialog` interface
  - Supports team-level batch operations, search, trash, and share management
  - Audit logs cover team-related operations
- **Team File Storage Service** (`workspace_storage_service`)
  - Independent workspace quota calculation and permission validation
  - Full lifecycle management of folders/files within teams
  - Team file version history support
- **Upload Optimization**
  - `relay_stream` no-staging streaming mode (replacing the original relay mode)
  - Local storage fast path: small files written directly to the target path, skipping the global temp directory
- **Custom CORS Middleware**
  - `CorsConfig` runtime configuration support
  - Manual CORS header handling based on the `http` crate
  - Admin console configuration changes take effect immediately
- **Refined Thumbnail API**
  - `ThumbnailStatus` enum: Generating/Unsupported/Error
  - HTTP 202 + `Retry-After` header to indicate generation in progress
  - HTTP 400 explicitly identifies unsupported MIME types


### Changed

- **Admin Route Restructuring**
  - Split `admin.rs` into 8 submodules: users/policies/teams/shares/config/locks/audit_logs/overview
  - Shared utility functions extracted to `admin/common.rs`
- **Upload Policy**
  - Removed the `S3UploadStrategy::ProxyTempfile` variant
  - `relay_stream` becomes the new relay mode implementation
- **File Repository**
  - `find_or_create_blob` retry strategy changed to exponential backoff (reducing high-concurrency conflicts)
- **Share Service**
  - Reworked share permission validation with team-scope validation support
  - Share list query optimization with team filtering support
- **Thumbnail Error Handling**
  - Generation failures return 500 (previously 404)
  - Unsupported types return 400 (with an explicit error message)


### Fixed

- **Security**
  - Polished API error messages to avoid leaking sensitive internal details (e.g., database schema, internal paths)
- **S3 Driver**
  - Fixed edge case in handling negative content_length
- **Application Shutdown**
  - Reworked graceful shutdown logic to ensure thumbnail workers and background tasks shut down correctly


### Breaking Changes

- **API**: `POST /api/v1/uploads` removed the `proxy_tempfile` strategy option (automatically migrated to `relay_stream`)
- **API**: Thumbnail endpoint status code semantic changes:
  - 202: thumbnail is being generated (previously returned 404)
  - 400: unsupported file type (new)
  - 500: generation failed (previously returned 404)
- **Internal**: `S3UploadStrategy` enum removed the `ProxyTempfile` variant


---

**Statistics**:
- 180 files changed, 33,028 insertions(+), 6,842 deletions(-)
- 12 commits


## [v0.0.1-alpha.13] - 2026-04-02

### Release Highlights

- **Storage Policy Groups** — Added a policy group subsystem replacing the original one-to-one user-policy assignment. Policy groups support multiple policy rules (matched by priority + file size range); after a user binds a policy group, uploads are automatically routed to the most suitable storage policy
- **Access Token Auto-Renewal** — The frontend added an `expires_at`-based auto-renewal mechanism that triggers refresh 2 minutes early; login/change-password responses return `expires_in`, making the session lifecycle fully trackable
- **Lightweight Code Preview** — Removed the Monaco Editor dependency (~350 lines), replaced with a Prism-based lightweight code editor that lazily loads 40+ languages, greatly reducing build output size
- **Optional OpenAPI Compilation** — All utoipa dependencies converted to an optional feature; release builds exclude OpenAPI support by default, producing a smaller binary
- **Admin Policy Group Page** — Complete policy group CRUD page with rule editing, user migration confirmation, and automatic seeding of the system default policy group
- **Frontend Infrastructure Enhancements** — Added pagination/query parameter utilities, extracted shared share-dialog logic, and useApiList race protection


### Added

- **Storage Policy Groups**
  - `storage_policy_groups` + `storage_policy_group_items` database tables (migration)
  - `users` table gained a `policy_group_id` column (FK + SET NULL cascade)
  - 6 Admin API routes: CRUD + user migration (`/admin/policy-groups/*`)
  - `PolicySnapshot` extensions: caches policy groups/items/user bindings, adding `resolve_policy_in_group`, `resolve_user_policy_for_size`, and other methods
  - `ensure_policy_groups_seeded` at startup: system default policy automatically wrapped as the default policy group, legacy `user_storage_policies` records migrated automatically
  - On upload, matches the most suitable policy in the policy group by file size
  - Audit log gained 4 new actions: `AdminCreatePolicyGroup`, `AdminUpdatePolicyGroup`, `AdminDeletePolicyGroup`, `AdminMigratePolicyGroupUsers`
  - Full frontend `AdminPolicyGroupsPage` policy group management page (1439 lines)
  - `UserDetailDialog` reworked: storage policy assignment changed to a single policy group selection
  - ~40 policy group translations added for each of Chinese and English i18n
- **Access Token Auto-Renewal**
  - Backend auth response body returns `expires_in` and `access_token_expires_at`
  - `authStore` gained `expiresAt` state, sessionStorage persistence, and `refreshToken()` deduplication/reuse
  - `startAutoRefresh()` / `stopAutoRefresh()`: setTimeout-based auto-renewal 2 minutes early
  - HTTP interceptor refresh queue changed from an array to `refreshPromise` reuse
- **Prism Code Editor**
  - Added `CodePreviewEditor` replacing MonacoCodeEditor, based on prism-react-renderer
  - Lazily loads Prism components for 40+ languages on demand
  - Added `prismClassNames` module to resolve Scoped CSS className conflicts
  - Added `toml` and `groovy` language mappings
- **Frontend Infrastructure**
  - `lib/pagination.ts`: generic offset pagination parameter parsing and building
  - `lib/queryParams.ts`: generic query string building utilities
  - `components/files/shareDialogShared.ts`: shared share-dialog logic (expiry calculation, download-count normalization)
  - `api-docs-macros` workspace crate: custom proc-macro expanding to `#[utoipa::path]` under the debug+openapi feature
- **Test Coverage**
  - Added `AdminPolicyGroupsPage.test.tsx` (873 lines)
  - Added `policyGroupDialogShared.test.ts`, `storagePolicyDialogShared.test.ts`, `shareDialogShared.test.ts`
  - Added `prismClassNames.test.ts`, `file-capabilities.test.ts`
  - Added `useApiList.test.tsx`, `pagination.test.ts`, `queryParams.test.ts`
  - Added `authStore.edge.test.ts`


### Changed

- **Optional OpenAPI Compilation**
  - `utoipa` / `utoipa-swagger-ui` changed to `optional = true`, with a new `openapi` feature
  - All `#[derive(ToSchema)]` / `#[derive(IntoParams)]` project-wide changed to `#[cfg_attr]` conditional compilation
  - `#[utoipa::path]` replaced with `#[api_docs_macros::path]`
  - `openapi` module conditionally compiled as a whole
- **Admin Page Rework**
  - `AdminUsersPage` heavily reworked, using the `useApiList` hook + URL search params management
  - `AdminPoliciesPage` uses the new pagination utilities
  - `AdminAuditPage` changed from manual `useCallback + useEffect` to the `useApiList` hook
  - `adminService.ts` uses `withQuery()` throughout for query strings, with parameters using generated request types
- **Upload Policy Resolution Reworked to File-Size-Based Routing**
  - `upload_service` calls the new `resolve_policy_for_size` instead of the original `resolve_policy`
- **Simplified User Creation Flow**
  - `create_user_with_role` no longer creates `user_storage_policies` rows; instead it sets `policy_group_id`
- **`useApiList` Hook Enhancements**
  - Added `requestIdRef` race protection, discarding stale responses when filter/offset changes quickly
  - Added `setTotal` return value
- **Removed Relay Upload Mode**
  - Deleted `relay_field_to_s3`, `create_relay_cleanup_handle`, and other functions (~170 lines)


### Fixed

- Fixed `StoragePolicyDialog` policy summary card sticky positioning failing on large screens (added `self-start`)


### Breaking Changes

- **API**: Removed 4 legacy user-storage-policy routes (`/admin/users/{user_id}/policies/*`); replacement is `/admin/policy-groups/*` + `policy_group_id` on `PATCH /admin/users/{id}`
- **API**: `POST /auth/login`, `POST /auth/refresh`, `PUT /auth/password` response bodies changed from `{ data: null }` to `{ data: { expires_in } }`
- **API**: `GET /auth/me` response gained `access_token_expires_at` and `policy_group_id` fields
- **API**: All user info response bodies gained a `policy_group_id` field
- **Behavior**: `user_storage_policies` is deprecated; new code should use the policy group system
- **Frontend**: Removed the `monaco-editor` dependency, replaced with `prismjs` + `prism-react-renderer`


---

**Statistics**:
- 137 files changed, 10,275 insertions(+), 3,305 deletions(-)
- 4 commits


## [v0.0.1-alpha.12] - 2026-03-31

### Release Highlights

- **Session Revocation Mechanism** — Added `session_version` field to the users table, embedded in JWT as a version number; admins can revoke all of a user's sessions with one click, and password changes automatically invalidate old tokens
- **In-Memory Runtime Configuration and Policy Snapshots** — System configuration and storage policies cached in `RwLock<HashMap>`, zero DB queries on hot paths, synced immediately on write
- **Batch SQL Operations** — Delete/move/copy reworked into batch SQL with single-transaction validation + execution, per-item error reporting; DB round trips for N operations reduced from ~6N to ~10
- **Admin Permission Middleware** — Extracted `RequireAdmin` as a standalone middleware; admin routes nest `JwtAuth → RequireAdmin`, removing inline role checks from handlers
- **Optional Local Storage Content Deduplication** — Added `content_dedup` policy option; when disabled, skips SHA256 computation and uses independent blob short-token keys
- **Database Index Optimization** — Added composite indexes for directory listing and trash pagination, eliminating full table scans


### Added

- **Session Revocation**
  - `users` table gained a `session_version` column (migration)
  - `AuthSnapshot` struct carries `status`, `role`, `session_version`
  - Added `POST /api/v1/admin/users/{id}/sessions/revoke` — admin revokes all of a user's sessions
  - Password change/admin password reset automatically increments `session_version`; the current session returns a new token to stay logged in
  - JWT Claims embed `session_version`; the authentication middleware validates consistency
  - WebDAV Bearer authentication upgraded to `authenticate_access_token`, rejecting refresh tokens
  - New audit actions: `AdminRevokeUserSessions`, `UserLogout`
  - Frontend user detail dialog gained a "Revoke all sessions" button
- **In-Memory Runtime Configuration**
  - `RuntimeConfig` struct: `reload`, `apply`, `remove` + typed getters (`get_bool`, `get_i64`, `get_u64`, etc.)
  - `PolicySnapshot` struct: `reload`, `get_policy`, `resolve_default_policy_id`, `set_user_default_policy`
  - Preloads all configuration and policies into memory at startup
  - All services (audit, auth, config, file, thumbnail, upload, trash, version, webdav) now read from the snapshot
- **Local Storage Content Deduplication Option**
  - `StoragePolicyOptions` adds the `content_dedup` field
  - When disabled: skips SHA256 and generates an independent blob key using `new_short_token()`
  - When enabled: computes SHA256 after writing the temporary file and reuses blobs with identical content
  - `local_content_dedup_enabled()` / `create_nondedup_blob()` public functions
- **Admin about page**
  - New `AdminAboutPage`: shows version, release channel (alpha/beta/rc/stable), license (MIT), external links
  - `AsterDriveWordmark` theme-aware SVG component (automatic dark/light switching)
  - `index.html` injects the `asterdrive-version` meta tag, with the version written at build time
  - Full Chinese and English i18n support
- **Database indexes**
  - `idx_folders_user_deleted_parent_name` / `idx_files_user_deleted_folder_name` — folder listing queries
  - `idx_folders_user_deleted_at_id` / `idx_files_user_deleted_at_id` — trash pagination queries
- **Test coverage**
  - `test_batch.rs` — batch operation tests (472 lines)
  - `test_db_indexes.rs` — index effectiveness validation (`EXPLAIN QUERY PLAN`)
  - `test_webdav_path_resolver.rs` — WebDAV path resolution tests (518 lines)
  - `test_services.rs` — tree visibility, empty leaves, trash paths, etc. (332 lines)


### Changed

- **Upload completion logic refactor**
  - Extracted the `create_new_file_from_blob`, `finalize_upload_session_blob`, `finalize_upload_session_file` public primitives
  - Extracted `complete_s3_multipart_upload_session` to unify multipart completion logic
  - Extracted the `ensure_uploaded_s3_object_size`, `transition_upload_session_to_assembling` helper functions
  - Removed the old `finalize_upload_session` and `clear_relay_cleanup_handle` implementations
- **Batch operations refactored to bulk SQL**
  - New `find_by_folders`, `find_all_in_folders`, `find_children_in_parents`, `find_all_children_in_parents` batch query methods
  - `batch_delete`: single-transaction validation + recursive subtree collection + bulk soft delete
  - `batch_move`: bulk conflict/cycle detection + bulk update, with per-item error reporting
  - `batch_copy`: pre-allocates unique file names, supports renaming duplicate IDs
- **Folder tree traversal made iterative**
  - BFS iteration replaces recursive per-item async queries
  - `build_trash_path_cache` bulk preloads trash parent folder paths
  - WebDAV path resolution now uses recursive CTE queries
- **Admin routes moved to middleware**
  - admin routes moved to a nested scope: `JwtAuth` → `RequireAdmin`
  - Removed the `claims: web::ReqData<Claims>` parameter and the `require_admin()` helper from handlers
- **Search multi-database compatibility**
  - `name_search_condition` selects the query strategy based on the database backend
  - PostgreSQL uses `ilike`, MySQL uses `MATCH AGAINST BOOLEAN MODE`
  - New `escape_like_query` prevents wildcard injection
- **Admin console UI refactor**
  - Storage policy dialog split into four sections (overview / connection / storage details / upload rules); edit mode adds a policy summary card on the right
  - Policy table rows are now fully clickable, removing the separate edit button
  - User table rows are now fully clickable
  - Creation wizard adds step transition animations
  - Driver type badges distinguished by color (S3=blue, local=green)
  - Built-in system policies cannot be deleted, with a tooltip hint
- **Authentication service adjustments**
  - `refresh_token` changed to an async function
  - `logout` extracts the token from the Authorization header and writes an audit log
  - Password change returns new access/refresh tokens (preserving session continuity)


### Fixed

- Fixed MySQL migration where the `allowed_types` and `options` columns were incompatible with `DEFAULT` value syntax
- Fixed raw SQL `Expr::cust_with_values` replaced with type-safe SeaORM expressions (ref_count, storage_used, view_count)
- Fixed the issue where a max file size of 0 displayed "0 bytes" instead of "unlimited"
- Fixed browser autofill on password inputs (added `autoComplete="new-password"`)
- Fixed browser autofill on access key inputs (added `autoComplete="off"`)


### Breaking Changes

- **API**: `PUT /api/v1/auth/password` now returns new access/refresh tokens (Cookie), preserving the current session
- **JWT**: new tokens include a `session_version` field; old tokens (without it) remain compatible via `#[serde(default)]`
- **Behavior**: S3 uploads uniformly use the `files/{upload_id}` path format
- **Behavior**: local storage defaults to `content_dedup: false`, creating an independent blob per upload (different from the previous implicit dedup behavior)
- **Internal**: all services must read configuration/policies from snapshots; direct calls to `policy_repo`/`config_repo` are forbidden


---

**Statistics**:
- 113 files changed, 7,785 insertions(+), 1,815 deletions(-)
- 13 commits


## [v0.0.1-alpha.11] - 2026-03-30

### Release Highlights

- **Admin overview panel** — new system overview dashboard showing user statistics, file storage, daily activity trend charts, and recent audit events
- **Streaming relay upload strategy** — new S3 streaming direct relay mode that forwards directly to S3 Multipart without local temporary files
- **Password management enhancements** — users can change their own passwords, and admins can directly reset user passwords
- **Share management upgrade** — supports editing existing share settings (password/expiration/download count) and adds bulk share deletion
- **Storage policy wizard refactor** — improved step-by-step creation wizard experience, with new S3/R2 endpoint normalization and validation
- **Search API officially enabled** — full file/folder search capability with multi-dimensional filtering and pagination
- **API response type safety** — fully replaces inline JSON with strongly typed response structures


### Added

- **Admin overview panel**
  - New `GET /api/v1/admin/overview` endpoint, supporting `days`/`timezone`/`event_limit` parameters
  - User statistics: total, active, and disabled counts
  - File statistics: total file count, storage bytes, blob count
  - Daily activity reports: login, upload, share, and delete trends
  - Frontend `AdminOverviewPage` integrates Recharts charts for display
- **Streaming relay upload strategy**
  - New `S3UploadStrategy` enum: `ProxyTempfile` / `RelayStream` / `Presigned`
  - New `upload_session_parts` table persisting parts and ETags
  - `RelayStream` mode streams directly to S3 with no local buffering
  - Upload progress queries support relay multipart mode
- **Password management**
  - New `PUT /api/v1/auth/password` — self-service password change (requires verifying the current password)
  - New `PUT /api/v1/admin/users/{id}/password` — admin password reset
  - Frontend `SecuritySettingsView` security settings page
  - Audit actions: `UserChangePassword`, `AdminResetUserPassword`
- **Share management enhancements**
  - New `PATCH /api/v1/shares/{id}` — edit share settings
  - New `POST /api/v1/shares/batch-delete` — bulk delete shares (up to 1000)
  - Share password semantics: `null` = keep, `""` = remove, `"value"` = replace
  - Frontend `EditShareDialog` edit dialog
- **S3/R2 endpoint normalization**
  - Automatically extracts the bucket name from the R2 endpoint path
  - Rejects insecure `.r2.dev` public URLs
  - Validates consistency between endpoint and bucket fields
  - Enforces the `http://` or `https://` protocol prefix
- **Search API**
  - `GET /api/v1/search` officially enabled, supporting fuzzy file name search
  - Filter conditions: type, MIME, size, date, directory scope
  - Pagination returns `FileSearchItem` / `FolderSearchItem`
- **Share page enhancements**
  - Share pages show the owner's avatar and display name
  - Single-file shares add thumbnail display
  - File icon and color refinements
- **Database maintenance indexes**
  - `upload_sessions_status_expires_at` — cleanup query optimization
  - `files_blob_id` / `file_versions_blob_id` — reference counting optimization
  - `file_blobs_storage_path` — orphan blob detection
- **Background maintenance service**
  - `maintenance_service` scheduled tasks: expired upload cleanup (hourly), blob reconciliation (every 6 hours)
  - Atomic `claim_blob_cleanup` mechanism prevents concurrent races
- **Database query metrics**
  - `db_queries_total` counter (by backend/type/status)
  - `db_query_duration_seconds` latency histogram


### Changed

- **Storage policy dialog refactor**
  - Step-by-step creation wizard: choose type → configure connection → confirm rules
  - Edit mode retains the single-page layout
  - Built-in system policies cannot be deleted
  - S3 parameter change detection with forced save confirmation
- **API response strong typing**
  - Replaced inline `serde_json::json!()` with structured response types
  - Structured audit details: `AdminCreateUserDetails`, `BatchDeleteDetails`, etc.
  - Frontend types reorganized by module group
- **PATCH semantics fix**
  - Introduced the `NullablePatch<T>` tri-state type: `Absent` / `Null` / `Value`
  - `PATCH /files/{id}` supports `folder_id: null` to move to the root directory
  - `PATCH /folders/{id}` supports `parent_id: null` to move to the root directory
- **Share expiration status code**
  - `ShareExpired` error HTTP status code changed from 410 to 404
  - Error responses add `Cache-Control: no-store` to prevent CDN caching
- **Numeric conversion utilities**
  - New `utils::numbers` module: `bytes_to_usize`, `i32_to_usize`, `calc_total_chunks`
  - Eliminated bare `as` casts across layers, unified checked conversion


### Fixed

- Fixed relay multipart progress queries not reading the database parts table
- Fixed a blob cleanup concurrency race condition
- Fixed missing cache control header on share download links


### Breaking Changes

- **API**: `ShareExpired` error HTTP status code changed from 410 to 404
- **API**: the `presigned_upload` boolean setting has been migrated to the `s3_upload_strategy` enum (automatically compatible)
- **API**: `PATCH` endpoints now correctly handle `null` semantics (explicit clear vs. ignore field)
- **Frontend**: storage policy configuration structure changed; custom frontends need to adapt to the new policy wizard


---

**Statistics**:

- 179 files changed, 13,838 insertions(+), 1,756 deletions(-)
- 14 commits


## [v0.0.1-alpha.10] - 2026-03-29

### Release Highlights

- New **user profile system**: supports custom display name, avatar upload, Gravatar with source switching, and custom Gravatar mirror URLs
- File lists introduce **virtual scrolling**; both grid and table views use `@tanstack/react-virtual`, significantly improving rendering performance with large datasets
- New **video preview enhancements**: integrates the Artplayer player, supporting dynamic aspect ratio calculation and a custom video browser
- Code editor migrated from `@monaco-editor/react` to native `monaco-editor`, with on-demand lazy loading of language support, greatly reducing build output size
- Settings page split into **Profile** and **Interface Preferences** as two separate route sections for clearer navigation
- Error page refactor: distinguishes production/development environments, hiding debug information in production
- Icon library migrated from `@devicon/react` to `react-devicons`, uniformly using the original variant
- New route transition animations (View Transitions API) for smoother page switching
- Built-in system storage policies cannot be deleted; added S3 parameter change detection with forced save confirmation

### Added

- **User profile system**
  - New `user_profiles` database table with two migrations
  - Full `profile_service` implementation: display name editing (max 64 characters), avatar upload (auto-cropped to square + WebP encoding, 512px/1024px sizes), Gravatar with source switching
  - New API endpoints: `PATCH /auth/profile`, `POST /auth/profile/avatar/upload`, `PUT /auth/profile/avatar/source`, `GET /auth/profile/avatar/{size}`
  - Frontend `UserAvatarImage` component supporting sm/md/lg/xl sizes
  - New `ProfileSettingsView` profile settings page: display name editing, avatar management, read-only username/email display
  - New `gravatar_base_url` runtime configuration supporting custom Gravatar mirrors (e.g. Cravatar)
- **File list virtual scrolling**
  - `FileGrid` and `FileTable` introduce `@tanstack/react-virtual` virtual scrolling
  - Responsive grid view columns (2-6), overscan tuned for smooth scrolling
- **Video preview enhancements**
  - New `VideoPreview` component based on the Artplayer player, supporting dynamic aspect ratio calculation
  - New `CustomVideoBrowserPreview`, a custom browser for external video sources
  - Video browser configuration module `video-browser-config.ts`
- **Interface settings page**
  - New `InterfaceSettingsView`: unified management of theme mode, color palette, language, and view mode
- **Route transition animations**
  - Navigation links integrate the View Transitions API for smoother page switching
- **Runtime configuration module**
  - New `frontend-panel/src/config/runtime.ts`, uniformly managing environment variables and dev mode flags
- **Policy protection and change detection**
  - Built-in system storage policy (ID=1) cannot be deleted
  - Admin policy editing adds S3 parameter change detection with a forced save confirmation dialog

### Changed

- **Monaco editor migration**
  - Migrated from `@monaco-editor/react` to native `monaco-editor`
  - New `monaco-environment.ts` for on-demand lazy loading of language support
  - `MonacoCodeEditor` replaces the old editor component
- **Settings page route refactor**
  - Settings page split into `/settings/profile` and `/settings/interface` route sections
  - The former `ThemeSwitcher` / `LanguageSwitcher` standalone components moved into the settings page
- **Error page refactor**
  - Fully rewritten `ErrorPage` with card layout + status code badge + recovery suggestions
  - Stack traces and other debug information hidden in production
- **Animation performance optimization**
  - File card/table transition animations shortened from 300ms to 150ms, scale transforms removed
  - Tooltip animation duration adjusted to 100ms
- **Icon library migration**
  - Migrated from `@devicon/react` to `react-devicons`
  - Language icons uniformly use the original variant
- **Vite build chunking optimization**
  - Enhanced `manualChunks` strategy: vendor-react / vendor-router / vendor-i18n / vendor-react-icons / vendor-devicons, etc.
  - Base UI split into vendor-ui-forms / vendor-ui-overlays / vendor-ui-controls
  - Preview-only chunks: preview-data / preview-xml
  - PWA workbox excludes unused Monaco worker files
- **Share page experience improvements**
  - New owner information display (name/email) and drag-and-drop preview support
  - File share cards add a preview button
- **Unified file preview loading state**
  - New `PreviewLoadingState` component, unifying loading state display across previewers
  - File preview dialog improved height adaptation and video size calculation
- **HeaderControls enhancements**
  - Top bar controls integrate the user avatar and display name

### Fixed

- Fixed storage policy zero-value field handling and user list avatar display issues
- Fixed policy connection test logic
- Fixed the issue where identity verification requests could not be retried after a network error
- Fixed Vue icon display and quota cell styling issues

### Breaking Changes

- **API**: `GET /api/v1/auth/me` response body adds a `profile` field containing `display_name`, `avatar` (source / url_512 / url_1024 / version)
- **API**: Admin user endpoints add user profile information to response bodies
- **Frontend**: settings page routes split from `/settings` into `/settings/profile` and `/settings/interface`
- **Frontend**: `ThemeSwitcher` / `LanguageSwitcher` standalone components removed, functionality consolidated into `InterfaceSettingsView`

---

**Statistics**:
- 147 files changed, 7,340 insertions(+), 1,484 deletions(-)
- 21 commits

## [v0.0.1-alpha.9] - 2026-03-28

### Release Highlights

- New **server-side user preference persistence** (theme, color palette, view mode, sorting, language), with automatic multi-device sync
- New **"My Shares" page**, supporting share status tracking (active / expired / exhausted / deleted) and paginated management
- File and folder lists add **share and lock status indicators** to distinguish resource status at a glance
- Integrated **devicon language icons**, upgrading code preview and file type icons across the board
- **Drag-and-drop interaction enhancements**: folder tree supports cross-component dragging and prevents dropping a folder into itself or its descendants
- **i18n namespace split**: common → core / errors / validation / offline + on-demand loading of share / settings / webdav
- **Large-scale frontend and backend test coverage additions**, with 4000+ lines of new unit tests + integration tests

### Added

- **Server-side user preference persistence**
  - New `PATCH /api/v1/auth/preferences` endpoint
  - Supports preferences such as theme mode, color palette, view mode, sorting, and language
  - Frontend debounce sync, automatic sync across multiple logged-in devices
  - Database migration: users.config JSON field
- **"My Shares" page**
  - Added `/my-shares` route with share list browsing and management
  - Backend `ShareStatus` enum (active / expired / exhausted / deleted)
  - `MyShareInfo` DTO includes resource name, status, remaining downloads, etc.
- **File/folder status indicators**
  - Share status and lock status icons added to list and grid views
  - `FileItemStatusIndicators` component
- **devicon language icon integration**
  - New `language-icon.tsx` component based on the devicon icon library
  - Upgraded icons for code preview file types
  - Added CMap extraction script for PDF Chinese display support
- **Drag-and-drop enhancements**
  - Folder tree supports dragging into the file browser
  - Prevents dropping a folder into itself or its descendant directories
  - Drag-and-drop logic extracted into the `lib/dragDrop.ts` shared module
- **Code preview minimap**
  - Enabled minimap in TextCodePreview
- **Share lookup indexes**
  - Migration adds lookup indexes on the share table, improving token and resource query performance

### Changed

- **Audit action type safety**
  - Audit logs refactored from string literals to an `AuditAction` enum
- **Route layer logic pushed down**
  - Business logic in auth, share_public, files, folders, batch and other route layers moved down to the service layer
- **i18n namespace split**
  - `common` split into `core`, `errors`, `validation`, `offline`
  - Added `settings`, `share`, `webdav` as separate namespaces
  - Layered optimization of initial vs. lazy loading
- **Error log leveling**
  - 5xx → `tracing::error`，4xx → `tracing::warn`
  - Silently ignored errors uniformly replaced with warn logs
- **Frontend shared module extraction**
  - `ToolbarBar` generic toolbar component
  - `AdminTableList` generic admin list component
  - Deduplicated multiple hooks / utils
- **Admin user update optimization**
  - Merged into a single batched update (role + status + quota)
  - Added audit logging
- **Share page layout refactor**
  - Extracted `ShareTopBar` and `ToolbarBar` shared components

### Fixed

- Fixed share download links failing due to relative paths
- Fixed null destination path in copy operations not being resolved to the root directory
- Fixed silently ignored errors in fire-and-forget operations (switched to warn logs)
- Fixed potential runtime errors caused by frontend non-null assertions
- Fixed layout scroll area style issues
- Resolved multiple accessibility issues

### Breaking Changes

- **API**: `GET /api/v1/shares` response body changed from `share::Model` to a `MyShareInfo` paginated object, including new fields such as `status` enum, `resource_name`, and `remaining_downloads`
- **API**: `GET /api/v1/auth/me` response body changed from `UserInfo` to `MeResponse`, adding a `preferences` field
- **API**: Added `PATCH /api/v1/auth/preferences` endpoint
- **Frontend**: i18n namespace `common` has been split into `core` / `errors` / `validation` / `offline`; custom frontends need to update translation references accordingly

---

**Statistics**:
- 291 files changed, 28,047 insertions(+), 2,216 deletions(-)
- 24 commits

## [v0.0.1-alpha.8] - 2026-03-27

### Release Highlights

- Admin panel adds **admin-created users**, suited for centralized account management in self-hosted scenarios
- Multiple admin endpoints and user-side lists unified to an **offset pagination structure**, more stable with large datasets and more consistent frontend/backend types
- File drag experience upgraded: added a **custom drag preview**, and the folder tree supports **drag-hover auto-expand**
- PWA startup experience improved: added an **offline startup fallback page**, plus warming up frequently used route resources after login
- Share access boundaries and WebDAV account management reinforced — public access, path display, and permission checks are more reliable

### Added

- **Admin user creation**
  - Backend adds `POST /api/v1/admin/users`
  - Admin panel supports creating users directly, without relying on self-registration
- **Admin user detail panel**
  - Upgraded user detail viewing and editing experience
  - Role, status, quota, etc. now use a unified save interaction
- **Drag experience enhancements**
  - Custom drag preview added to file cards and list rows
  - Folder tree supports drag-hover auto-expand, making moves to deep directories smoother
- **PWA startup enhancements**
  - Added an offline startup fallback page
  - Warms up frequently used route resources after login, improving installed and weak-network experiences
- **Unified pagination foundation**
  - Added generic `LimitOffsetQuery` / `OffsetPage<T>` pagination structures
  - Admin endpoints and some user endpoints unified on offset pagination

### Changed

- **Admin list pagination unification**
  - Users, policies, shares, configs, locks, audit logs, and user policy lists switched to offset pagination responses
- **User-side list pagination unification**
  - `/api/v1/shares` and `/api/v1/webdav-accounts` now return paginated objects
- **Admin panel layout refactor**
  - Top bar, page containers, description text, and control sizes given a round of unified cleanup
- **WebDAV account path building optimization**
  - Batch path building reduces repeated queries and makes path display more stable
- **Dependency and build configuration updates**
  - Upgraded some frontend and backend dependencies
  - Added a performance build profile and adapted to the new `sha2` Digest API

### Fixed

- Fixed multiple edge cases in public share access, including expired shares, out-of-bounds access, and access to deleted sub-files / subdirectories
- Fixed duplicate active share creation not being properly blocked
- Fixed WebDAV account root folder validation and disabled-account test edge cases
- Fixed PWA offline startup flow when no cached user is present
- Reinforced test coverage and permission boundary verification for audit logs, shares, and WebDAV

### Breaking Changes

- **API**: response structures of multiple list endpoints changed from arrays to paginated objects:
  - `/api/v1/shares`
  - `/api/v1/webdav-accounts`
  - Multiple `/api/v1/admin/*` list endpoints
- Custom frontends, scripts, or third-party clients relying on the old array response format need to adapt

---

**Statistics**:
- 87 files changed, 6,021 insertions(+), 1,783 deletions(-)
- 15 commits

## [v0.0.1-alpha.7] - 2026-03-26

### Release Highlights

- File lists add multi-field sorting and upgrade to cursor-based pagination, making deep-directory and large-folder browsing smoother
- Frontend integrates PWA with update prompts and offline session persistence, more stable on weak or disconnected networks
- Folder tree state management refactored with on-demand loading and ancestor path restoration, notably improving directory navigation performance
- New file/folder details dialog for quickly viewing size, type, timestamps, lock status, and child counts
- Recycle bin batch restore and batch purge pipelines refactored, reducing transactions and DB round trips — delete and empty operations are more efficient
- Upload panel introduces virtual scrolling with unified error states and retry entry points, keeping the frontend more stable with many tasks and in exceptional scenarios

### Added

- **File list sorting and pagination enhancements**
  - File lists support sorting by `name` / `size` / `created_at` / `updated_at` / `type`
  - Frontend adds a sort menu with ascending / descending toggle
  - File list pagination upgraded to cursor mode, supporting `file_after_value` + `file_after_id`
- **PWA support**
  - Frontend integrates `vite-plugin-pwa`
  - Supports manifest, service worker registration, and new-version update prompts
- **Offline session persistence**
  - `authStore` caches user info, preserving the existing session on network errors
- **File/folder details dialog**
  - Files support viewing size, MIME, created/modified times, lock status, blob id
  - Folders support viewing created/modified times, lock status, policy id, and child counts
- **Folder ancestor path endpoint**
  - Added `/folders/{id}/ancestors` for restoring deep-directory navigation paths

### Changed

- **Folder tree state management refactor**
  - Frontend folder tree switched to on-demand loading, reducing the pressure of loading the whole tree at once
  - Entering deep directories correctly restores ancestor paths and tree expansion state
- **Recycle bin batch pipeline refactor**
  - Batch restore, batch purge, and recursive purge logic unified through batch processing paths
  - Reduced transaction count and database round trips
- **Upload panel performance optimization**
  - Introduced virtual scrolling to improve rendering performance with many upload tasks
- **Frontend asset loading optimization**
  - i18n switched to on-demand loading
  - Vite build splitting optimization, paired with PWA caching strategy to improve loading experience

### Fixed

- File list state not syncing after switching sort order; the list now correctly resets and reloads when sorting changes
- Inconsistent error states in file preview; unified error display and retry entry
- Share content list lacking parity with the main file list; sorting and cursor pagination pipelines added
- Thumbnail generation duplicate enqueuing and instability under high load; added deduplication and retry optimizations
- Edge cases during recycle bin batch restore / purge, avoiding duplicate and missed processing

### Breaking Changes

- **API**: file list queries no longer use `file_offset`; replaced with cursor pagination parameters `file_after_value` + `file_after_id`
- **API**: file list endpoints add `sort_by` and `sort_order` query parameters; existing callers need to adapt

---

**Statistics**:
- 91 files changed, 4,209 insertions(+), 1,477 deletions(-)
- 18 commits

## [v0.0.1-alpha.6] - 2026-03-25

### Release Highlights

- File lists, recycle bin, and share pages fully support pagination + frontend infinite scrolling — no more loading everything at once
- Thumbnails switched to background async generation with 202 responses and frontend polling retries, resolving memory spikes after bulk file uploads
- Recycle bin permanent-delete batch optimization: N files reduced from ~12N DB queries to ~10
- Added clipboard operations (Ctrl+C/X/V) and an F2 rename shortcut
- Added four-tier rate-limit middleware (auth/public/api/write), an empty-file creation endpoint, and user status caching

### Added

- **Pagination system**
  - Backend adds `FolderListQuery` pagination parameters (`folder_limit/offset`, `file_limit/offset`), defaults folder_limit=200, file_limit=100
  - Folder list, recycle bin list, and share content list endpoints all support pagination
  - Response bodies add `folders_total` / `files_total` fields
  - Frontend `fileStore` adds `loadMoreFiles` + IntersectionObserver infinite scrolling
  - TrashPage and ShareViewPage wired up with pagination and infinite scrolling
  - Folder tree and destination folder picker dialogs pass `file_limit: 0` to load folders only
- **Async background thumbnail generation**
  - `thumbnail_service::get_or_enqueue()` — enqueues background generation when a thumbnail is missing, returning 202 + `Retry-After: 2`
  - `AppState.thumbnail_tx` with a dedicated tokio worker consuming the queue sequentially; HashSet dedup prevents processing the same blob twice
  - WebDAV fs/file/handler pass the thumbnail channel through the full chain
  - Frontend `useBlobUrl` automatically retries on 202 at the `Retry-After` interval (up to 5 times)
- **Rate-limit middleware**
  - `RateLimitConfig` four-tier rate limiting (auth/public/api/write), off by default, enabled as needed
  - `AsterIpKeyExtractor` — 429 responses return a unified JSON format with a `Retry-After` header
  - Routes attach the Governor rate-limit middleware per tier via `Condition`
- **Empty-file creation endpoint**
  - `POST /api/v1/files/new` creates a 0-byte empty file, with blob deduplication and automatic renaming on filename conflicts
  - Frontend `CreateFileDialog` component, supporting creating empty files directly in the file browser
- **Clipboard operations and rename shortcut**
  - `fileStore` adds `clipboardCopy` / `clipboardCut` / `clipboardPaste` / `clearClipboard`
  - `useKeyboardShortcuts` adds Ctrl+C/X/V clipboard shortcuts and an F2 rename shortcut
  - FileGrid / FileTable add an `onRename` callback
- **Recycle bin batch operation repo functions**
  - `file_repo::delete_many` / `delete_blobs` / `decrement_blob_ref_counts`
  - `folder_repo::delete_many` / `find_all_children` / `find_all_files_in_folder`
  - `property_repo::delete_all_for_entities`、`version_repo::delete_all_by_file_ids`

### Changed

- **Recycle bin batch purge refactor**
  - `file_service::batch_purge` — handles all DB operations in a single transaction, with parallel physical cleanup afterward
  - `webdav_service::recursive_purge_folder` changed to recursive collection first, then batch purge
  - `trash_service::purge_all` prioritizes batching top-level folders, then batch purges top-level loose files
- **User status caching**
  - Auth middleware introduces user status caching (TTL=30s), reducing per-request DB lookups
  - Cache is proactively invalidated when an admin disables a user
- **Frontend components**
  - `ScrollArea` changed to `forwardRef`, with ref pointing to the Viewport element to support IntersectionObserver
  - Frontend empty-file creation now calls the new endpoint, removing multipart FormData logic
- **Code formatting**
  - Unified rustfmt formatting across the project, splitting overly long chained calls and function parameters

### Fixed

- Removed the `is_locked` check in `purge` — files in the recycle bin should not be restricted by locks
- Recycle bin list switched to SQL-level filtering and pagination of top-level deleted items, removing the in-memory HashSet filtering logic
- `recursive_purge_folder` now uses `find_all_children` (which does not filter deleted_at), fixing missed soft-deleted subdirectories

---

**Statistics**:
- 72 files changed, 2,844 insertions(+), 318 deletions(-)
- 6 commits

## [v0.0.1-alpha.5] - 2026-03-25

### Release Highlights

- Greatly simplified S3 upload flow: dropped SHA256 read-back and copy_object, using `files/{uuid}` directly as the final storage path, reducing latency and traffic
- Idempotent upload retry: upload_session records file_id; repeated completes return the existing file directly; added an Assembling intermediate state (HTTP 202) to prevent frontend polling from hanging
- Log rotation: supports daily automatic rotation + configurable retention of historical files (`enable_rotation` / `max_backups`)
- Frontend settings page and WebDAV accounts page refactored with the SettingsScaffold component, unifying the card-style layout
- Frontend types uniformly exported from the generated API schema, eliminating hand-written duplicate definitions
- File streaming response performance optimization, reducing memory usage

### Added

- **Idempotent upload retry**
  - upload_sessions table adds a `file_id` column (migration); the associated file ID is recorded on completion
  - Repeated complete requests: session already completed → returns the existing file directly; in progress → returns HTTP 202 (ErrorCode 3011)
  - Assembly failure automatically marks the session as Failed, preventing infinite frontend retries
  - `generate_upload_id()` collision detection with up to 5 retries
- **Log rotation**
  - `LoggingConfig` adds `enable_rotation` (default true) and `max_backups` (default 5)
  - Daily rotation via tracing_appender rolling, automatically cleaning up historical logs beyond the retained count
  - On rotation failure, automatically falls back to stdout with a warning
- **Frontend SettingsScaffold component**
  - `SettingsPageIntro` / `SettingsSection` / `SettingsRow` / `SettingsIcon` reusable components
  - Unified card-style layout with action slot and custom content areas

### Changed

- **S3 upload flow simplification**
  - presigned / multipart uploads no longer read back the S3 object for SHA256, using an `s3-{upload_id}` placeholder hash instead
  - No longer copy_object to a content-addressed path; uses `files/{upload_id}` directly as the final key
  - Removed the S3 temporary object deletion step (no more temp → final two-step operation)
- **Frontend page refactor**
  - SettingsPage rewritten with SettingsScaffold, greatly reducing code volume
  - WebdavAccountsPage refactored and slimmed down, with a unified layout style
  - Frontend types uniformly exported from `api.generated.ts`; `types/api.ts` is only a re-export
  - searchService / fileService / uploadService switched to generated type definitions
- **macOS temp directory cleanup**
  - `cleanup_temp_dir` adds a retry mechanism (up to 3 times + 50ms interval) to handle ENOTEMPTY caused by Spotlight
- **File streaming response**
  - `file_service` optimizes streaming response performance, reducing memory usage

### Fixed

- Fixed indentation formatting in the PDF preview header info area
- Fixed edge-case handling in the directory upload utility functions

---

**Statistics**:
- 24 files changed, 1,045 insertions(+), 950 deletions(-)
- 5 commits

## [v0.0.1-alpha.4] - 2026-03-25

### Release Highlights

- Added S3 multipart direct upload (presigned_multipart) with resumable support, improving large-file upload performance and stability
- Refactored the recycle bin page and features, adding batch operations and drag-to-delete
- File preview adds embedded PDF preview with paging, zoom, rotation, and download
- Refactored the WebDAV account management page, upgrading the UI and completing internationalized copy
- Optimized folder tree caching and interactions, improving initial load and operation responsiveness
- Settings page switched to a responsive card layout with enhanced internationalization support
- Major refactor of the user documentation site organization, migrating API and architecture docs to developer-docs
- Multiple security hardening measures, including the Cookie Secure flag, upload permission checks, and concurrent update protection
- Performance optimizations and bug fixes, including the upload flow, file tree interactions, and frontend state management

### Added

- presigned_multipart upload mode with batched presigning, uploads, and state persistence
- Drag-and-drop, keyboard shortcuts, and batch selection to recycle bin
- react-pdf integration, with a built-in PDF preview window and toolbar
- Directory upload support: frontend drag/select directory parsing and backend relative-path recursive creation
- Audit log cleanup and panic-safe wrapping for multiple background tasks
- Upload panel progress bars and grouped display

### Changed

- Documentation site refactor, user-perspective focused, with improved navigation and structure
- File browser view initial load performance optimization
- Rewrote upload-related hooks, removing redundant code and unused endpoints
- Tightened iframe sandbox restrictions for security, limiting script execution

### Fixed

- Fixed frontend clearing the login state after token refresh failure
- Fixed file size inconsistencies in multiple places and a version regression bug
- Fixed automatic suffixing for duplicate filenames
- Fixed upload states overwriting each other and possible concurrency conflicts
- Fixed recycle bin path filtering and recycle bin detail/sync issues

### Breaking Changes

- API /api/v1/auth/login request field changed from username to identifier


## [v0.0.1-alpha.3] - 2026-03-24

### Release Highlights

**A comprehensive upgrade to preview, upload, and authentication!** From file preview and login flow to the upload task panel, this release pushes the frontend and backend experience forward together.

- **Auth flow refactor** — supports unified username / email login, plus first-time admin initialization setup
- **Unified file preview system** — supports Markdown, JSON, XML, CSV/TSV, media, and code preview
- **Enhanced sharing** — public files can be previewed directly, and folder shares support downloading files within
- **Upload experience upgrade** — added an upload task panel, concurrent uploads, chunk retries, and status tracking
- **Version restore refactor** — rolling back now trims subsequent historical versions, with improved blob cleanup and regression tests
- **Frontend UX polish** — overall refinements to the login page, file browser, TopBar, toast notifications, and internationalization

### Added

- **Authentication and initialization flow**
  - Added `/api/v1/auth/check`, which automatically determines the login / register / first-time initialization path based on input
  - Added `/api/v1/auth/setup` to create an admin account on first system startup
  - Login accepts email or username as a unified identifier
- **New file preview system**
  - Unified `FilePreviewDialog` as the preview entry point
  - Added previewers for Markdown, JSON, XML, CSV/TSV, text code, and more
  - Supports Open With mode switching, capability detection, and unsaved-changes leave confirmation
- **Sharing enhancements**
  - Public share file page supports direct preview
  - Folder sharing adds public download capability for child files
  - Share metadata now includes `mime_type` and `size`
- **Upload task panel**
  - Added `UploadPanel` / `UploadTaskItem`
  - direct / chunked / presigned upload modes unified into the task list
  - Supports concurrent uploads, chunk retries, status tracking, and task retention after completion
- **File size redundant field**
  - Added `size` field to the `files` table
  - Migration backfills historical data, providing stable size information for list display and API responses
- **Skeleton screens and brand assets optimization**
  - Added skeleton components for file grid / table / tree, etc.
  - Restructured logo SVG and improved brand presentation on the login page and TopBar

### Changed

- **Login page**
  - Redesigned as a two-column brand layout + multi-step authentication flow
  - Supports automatic account status checking and dynamic switching between login / register / initialization modes
  - Improved form validation, enter animations, and exit animations
- **File browser**
  - Batch move / copy now uses a target directory selection dialog
  - Batch operation results now show friendlier detailed notifications
  - Version history dialog converted to controlled mode, with complete restore / delete confirmation interactions
- **Notifications and internationalization**
  - Toasts now appear in the bottom-right corner with swipe-right-to-dismiss support
  - Batch operations, error messages, version history, and other copy unified into Chinese-English translations
- **Version restore semantics**
  - Restoring to a version deletes that version and all historical versions after it
  - Restore logic is now transactional, with blob reference cleanup after commit
- **Background periodic tasks**
  - Upload cleanup, recycle bin cleanup, lock cleanup, and audit log cleanup unified into `runtime/tasks.rs`
  - Periodic tasks get panic-safe wrappers, preventing a single task failure from killing the entire loop
- **Error handling**
  - Introduced `MapAsterErr` to unify error context mapping and reduce boilerplate duplication

### Fixed

- Fixed the public share page being incorrectly blocked by the login-state check and redirecting to `/login`
- Fixed frontend session state cleanup logic after token refresh failure
- Fixed inconsistency between the history list and blob cleanup after version restore
- Fixed inconsistency of file size information across multiple code paths
- Fixed UX issues in the upload task list: states overwriting each other, no scrolling, and tasks disappearing immediately on completion
- Fixed missing operation feedback when dragging files in the tree to the root directory

### Breaking Changes

- **API**: `/api/v1/auth/login` request field changed from `username` to `identifier`

---

**Statistics**:
- 139 files changed, 7,915 insertions(+), 1,786 deletions(-)
- 11 commits

## [v0.0.1-alpha.2] - 2026-03-23

### Release Highlights

**Complete frontend rewrite!** Upgraded from PoC level to a modern UI architecture, adding internationalization, theming system, and responsive layout.

- **i18n internationalization** — react-i18next, Chinese-English bilingual, 5 namespaces, instant switching
- **Theme system** — Light / Dark / System modes + 4 color palettes (Blue / Green / Purple / Orange), CSS variables in oklch
- **Responsive layout** — collapsible sidebar, global top bar, mobile overlay
- **Grid / list views** — dual-view switching with remembered preference, thumbnail cards + sortable table
- **Multi-select + batch operations** — checkbox selection, floating bottom action bar, batch delete / move / copy
- **Recursive folder tree** — lazy-load expansion, replacing the original flat list

### Added

- **i18n system**
  - react-i18next + i18next-browser-languagedetector
  - 5 namespaces: common / files / auth / admin / search
  - Complete Chinese-English translations (125+ key-value pairs)
  - Auto-detects browser language, persisted to localStorage
- **Theme system**
  - `themeStore` — Light / Dark / System modes, matchMedia listens to system preference
  - 4 color palette presets (blue / green / purple / orange), each with light + dark variants
  - CSS variables in oklch color space, switched via `[data-theme]` attribute
  - All preferences stored in localStorage
- **Common component library** `components/common/`
  - ThemeSwitcher — Sun / Moon / Monitor dropdown switcher
  - ColorPresetPicker — palette dot selector
  - LanguageSwitcher — Chinese-English language dropdown
  - EmptyState — icon + title + description + action button
  - LoadingSpinner — centered spinning loader
  - ConfirmDialog — AlertDialog wrapper with destructive variant
  - ViewToggle — grid / list icon toggle
  - BatchActionBar — floating bottom bar (selection count + delete / move / copy)
- **New layout components**
  - Sidebar — collapsible on desktop (240px / 56px), overlay + backdrop on mobile
  - TopBar — global top bar: hamburger menu + breadcrumbs + theme / language / user dropdowns
- **File browser components**
  - FileGrid — responsive grid (2-6 columns), thumbnail cards
  - FileTable — list table with sortable column headers and select-all checkbox
  - FileCard — grid card with checkbox shown on hover
  - FileThumbnail — extracted for reuse, sm / lg sizes
  - FileContextMenu — context menu (download / share / copy / rename / lock / versions / delete)
  - CreateFolderDialog — extracted from FileBrowserPage
  - RenameDialog — file / folder rename, auto-selects the filename (excluding extension)
- **Settings page** `/settings`
  - Theme mode + palette selection
  - Language switching
  - File browser default view mode
- **Keyboard shortcuts**
  - Ctrl/Cmd + A — select all
  - Escape — cancel selection
  - / or Ctrl/Cmd + K — focus search
- **Utility functions** `lib/format.ts`
  - `formatBytes` / `formatDate` / `formatDateAbsolute`
  - Replaces 5 duplicated implementations

### Changed

- **AppLayout** — rewritten as a three-part TopBar + collapsible Sidebar + main content layout
- **FolderTree** — rewritten from a flat list to a recursive lazy-loading tree (expand / collapse / child folder loading)
- **fileStore** — fully rewritten, adding viewMode / sortBy / sortOrder / selectedFileIds / selectedFolderIds
- **FileBrowserPage** — rewritten from a 267-line monolith to a ~80-line orchestrator
- **PageHeader** — simplified to a thin component, breadcrumbs moved to TopBar
- **AdminLayout** — added i18n translations + ThemeSwitcher / LanguageSwitcher
- **All 11 pages** — fully covered with i18n translations, hardcoded English strings reduced to zero
- **All destructive operations** — uniformly confirmed via ConfirmDialog
- **All native `<select>` elements** — uniformly replaced with the shadcn Select component
- **Dark mode compatibility** — Badge / status colors all given `dark:` variants

### Removed

- `FileList.tsx` — replaced by FileGrid + FileTable
- The batch PoC panel in FileBrowserPage (manual ID input) — replaced by BatchActionBar
- 5 duplicated inline `formatBytes` / `formatDate` functions

### Dependencies

- Added `react-i18next` 16.6
- Added `i18next` 25.10
- Added `i18next-browser-languagedetector` 8.2

---

**Statistics**:
- 79 files changed, 3,632 insertions(+), 1,506 deletions(-)
- 1 commit

## [v0.0.1-alpha.1] - 2026-03-23

### Release Highlights

**First public release of AsterDrive!** Self-hosted cloud storage system, distributed as a single Rust binary, MIT licensed.

- **Complete file management** — upload (direct/chunked/S3 presigned), download, copy, move, online editing, version history, thumbnails
- **WebDAV protocol** — RFC 4918 Class 1 + LOCK, independent account system, database-persisted locks, DeltaV version queries
- **Storage policy system** — Local + S3 dual drivers, user-level/folder-level policy overrides, sha256 deduplication + ref_count
- **Share links** — password protection, expiration, download count limits, thumbnail support
- **Search + batch operations + audit log** — complete backend API, Admin audit traceability

### Added

- **File management**
  - multipart streaming upload (64KB chunks with sha256, blob deduplication + ref_count)
  - Chunked upload (init → chunk → complete, idempotency guaranteed)
  - S3 presigned direct upload (policy-level toggle, temp path → copy_object → delete temp)
  - Streaming download (Content-Length, no full buffering)
  - File copy (blob reference counting, no actual data copied)
  - Move / rename (same-name conflict detection)
  - Online editing (PUT /content, ETag optimistic locking + pessimistic lock checking)
  - File version history (old versions auto-saved, rollback supported)
  - Image thumbnails (WebP, generated on demand, long-term caching)
- **Folder management**
  - Create / delete / copy / move / rename
  - Recursive operations (soft delete, hard delete, and copy all support deep nesting)
  - Cycle detection (prevents A → B → A when moving)
- **Storage system**
  - Storage policy hierarchy (system default + user-level + folder-level overrides)
  - Local driver + S3 driver (aws-sdk-s3)
  - Storage quota management (user-level, admin-adjustable)
  - Driver Registry hot reloading (cache automatically cleared after policy updates)
- **Authentication and authorization**
  - JWT dual tokens (Access + Refresh), stored in HttpOnly Cookies
  - argon2 password hashing
  - Automatic 401 → refresh token retry
  - Role system (admin / user), first registered user automatically becomes admin
- **WebDAV**
  - RFC 4918 Class 1 + LOCK fully implemented
  - Basic Auth (separate webdav_accounts table) + Bearer JWT
  - DbLockSystem database-persisted locks (locks survive restarts, background hourly cleanup of expired locks)
  - root_folder_id access restriction
  - Streaming handling of large-file temp files
  - macOS compatibility (filters out `._*` / `.DS_Store`)
  - RFC 3253 DeltaV version history queries
- **Share links**
  - Unique token + password protection (argon2) + expiration + download count limits
  - Public routes `/s/{token}` (view / verify password / download / folder browsing / thumbnails)
  - Cookie signature verification (SHA256, valid for 1 hour)
- **Recycle bin**
  - Soft delete (deleted_at column, automatically filtered from all list queries)
  - Restore (automatically restored to root directory if the original folder was deleted)
  - Permanent deletion (blob cleanup + thumbnails + properties + quota)
  - Background auto-cleanup (configurable retention days, default 7 days)
- **Search API**
  - GET `/api/v1/search` — filename LIKE fuzzy search + metadata filtering (MIME / size / date)
  - Cross-database compatibility (LOWER() + LIKE)
  - Supports file / folder / all type filtering, folder_id scope restriction, pagination
- **Batch operations**
  - POST `/api/v1/batch/{delete,move,copy}` — mixed file_ids + folder_ids
  - Each item executed independently, returning succeeded / failed / errors summary
  - 100-item limit
- **Audit log**
  - audit_logs table (action + entity + details + IP / UA)
  - Fire-and-forget writes (does not block business operations)
  - Runtime config toggles (audit_log_enabled / audit_log_retention_days)
  - Admin query API (filtering + pagination)
  - Background auto-cleanup of expired logs
  - Coverage: files / folders / login & registration / shares / batch operations / config changes
- **Custom properties**
  - entity_properties table (entity_type + entity_id + namespace + name + value)
  - WebDAV PROPPATCH compatible
  - REST API: GET / PUT / DELETE
- **Configuration system**
  - Static config: `config.toml` (ASTER__ environment variable overrides), auto-generated on first startup
  - Runtime config: system_config table (hot-modified via Admin API)
  - Single source of truth for config definitions (definitions.rs), ensure_defaults at startup
  - Schema API + type validation + frontend grouped rendering
- **Caching**
  - CacheBackend trait（NoopCache / MemoryCache / RedisCache）
  - CacheExt generic extension (automatic serde serialization)
  - Policy + Share query caching
- **Monitoring**
  - Prometheus metrics (`metrics` feature-gated) + sysinfo system metrics
  - Health / Ready endpoints
- **Admin console**
  - User management (roles, status, quota, force delete)
  - Storage policy management (CRUD, connection testing, user-level assignment)
  - Share management (global list, admin deletion)
  - WebDAV lock management (list, force release, expired cleanup)
  - System config management (categories, schema, type validation)
  - Audit log queries
- **Frontend PoC**
  - React 19 + Vite 8 + Tailwind CSS 4 + shadcn/ui + zustand
  - File browser (list view + breadcrumb navigation + thumbnails + preview + drag-and-drop upload)
  - Admin console (users / policies / shares / locks / config / audit log)
  - Search page, batch operations panel
  - rust-embed compiled into the single binary
- **Testing**
  - 30+ integration tests covering all core features
  - OpenAPI spec auto-generation (utoipa + swagger-ui)
- **API documentation**
  - utoipa annotations on all endpoints
  - Swagger UI (debug builds)
  - OpenAPI JSON auto-export

### Dependencies

- **Web**: actix-web 4.13, actix-governor 0.10
- **ORM**: sea-orm 2.0.0-rc.37（SQLite / MySQL / PostgreSQL）
- **Auth**: jsonwebtoken 10, argon2 0.5
- **Storage**: aws-sdk-s3 1.127
- **Cache**: moka 0.12, redis 1.1
- **WebDAV**: dav-server 0.11
- **API Docs**: utoipa 5.4, utoipa-swagger-ui 9.0
- **Image**: image crate（jpeg/png/gif/webp/bmp/tiff）
- **Frontend**: React 19, Vite 8, Tailwind CSS 4, shadcn/ui, zustand 5, uppy 5

---

**Statistics**:
- 287 files changed, 48,597 insertions(+)
- 66 commits
- Rust Edition 2024, MSRV 1.91.1

[Unreleased]: https://github.com/AsterCommunity/AsterDrive/compare/v0.5.1...HEAD
[v0.5.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.5.0...v0.5.1
[v0.5.0]: https://github.com/AsterCommunity/AsterDrive/compare/v0.5.0-rc.1...v0.5.0
[v0.5.0-rc.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.4.0...v0.5.0-rc.1
[v0.4.0]: https://github.com/AsterCommunity/AsterDrive/compare/v0.4.0-rc.2...v0.4.0
[v0.4.0-rc.2]: https://github.com/AsterCommunity/AsterDrive/compare/v0.4.0-rc.1...v0.4.0-rc.2
[v0.4.0-rc.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.4.0-beta.3...v0.4.0-rc.1
[v0.4.0-beta.3]: https://github.com/AsterCommunity/AsterDrive/compare/v0.4.0-beta.2...v0.4.0-beta.3
[v0.4.0-beta.2]: https://github.com/AsterCommunity/AsterDrive/compare/v0.4.0-beta.1...v0.4.0-beta.2
[v0.4.0-beta.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.2...v0.4.0-beta.1
[v0.3.2]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.1...v0.3.2
[v0.3.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.0...v0.3.1
[v0.3.0]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.0-rc.2...v0.3.0
[v0.3.0-rc.2]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.0-rc.1...v0.3.0-rc.2
[v0.3.0-rc.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.0-beta.2...v0.3.0-rc.1
[v0.3.0-beta.2]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.0-beta.1...v0.3.0-beta.2
[v0.3.0-beta.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.0-alpha.5...v0.3.0-beta.1
[v0.3.0-alpha.5]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.0-alpha.4...v0.3.0-alpha.5
[v0.3.0-alpha.4]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.0-alpha.3...v0.3.0-alpha.4
[v0.3.0-alpha.3]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.0-alpha.2...v0.3.0-alpha.3
[v0.3.0-alpha.2]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.0-alpha.1...v0.3.0-alpha.2
[v0.3.0-alpha.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.7...v0.3.0-alpha.1
[v0.2.7]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.6...v0.2.7
[v0.2.6]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.5...v0.2.6
[v0.2.5]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.4...v0.2.5
[v0.2.4]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.3...v0.2.4
[v0.2.3]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.2...v0.2.3
[v0.2.2]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.1...v0.2.2
[v0.2.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.0-hotfix.1...v0.2.1
[v0.2.0-hotfix.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.0...v0.2.0-hotfix.1
[v0.2.0]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.0-rc.1...v0.2.0
[v0.2.0-rc.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.0-beta.3...v0.2.0-rc.1
[v0.2.0-beta.3]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.0-beta.2...v0.2.0-beta.3
[v0.2.0-beta.2]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.0-beta.1...v0.2.0-beta.2
[v0.2.0-beta.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.1.0...v0.2.0-beta.1
[v0.1.0]: https://github.com/AsterCommunity/AsterDrive/compare/v0.1.0-rc.2...v0.1.0
[v0.1.0-rc.2]: https://github.com/AsterCommunity/AsterDrive/compare/v0.1.0-rc.1...v0.1.0-rc.2
[v0.1.0-rc.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.1.0-beta.5...v0.1.0-rc.1
[v0.1.0-beta.5]: https://github.com/AsterCommunity/AsterDrive/compare/v0.1.0-beta.4...v0.1.0-beta.5
[v0.1.0-beta.4]: https://github.com/AsterCommunity/AsterDrive/compare/v0.1.0-beta.3...v0.1.0-beta.4
[v0.1.0-beta.3]: https://github.com/AsterCommunity/AsterDrive/compare/v0.1.0-beta.2...v0.1.0-beta.3
[v0.1.0-beta.2]: https://github.com/AsterCommunity/AsterDrive/compare/v0.1.0-beta.1...v0.1.0-beta.2
[v0.1.0-beta.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.26...v0.1.0-beta.1
[v0.0.1-alpha.26]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.25...v0.0.1-alpha.26
[v0.0.1-alpha.25]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.24...v0.0.1-alpha.25
[v0.0.1-alpha.24]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.23...v0.0.1-alpha.24
[v0.0.1-alpha.23]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.22...v0.0.1-alpha.23
[v0.0.1-alpha.22]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.21...v0.0.1-alpha.22
[v0.0.1-alpha.21]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.20...v0.0.1-alpha.21
[v0.0.1-alpha.20]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.19...v0.0.1-alpha.20
[v0.0.1-alpha.19]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.18...v0.0.1-alpha.19
[v0.0.1-alpha.18]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.17...v0.0.1-alpha.18
[v0.0.1-alpha.17]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.16...v0.0.1-alpha.17
[v0.0.1-alpha.16]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.15...v0.0.1-alpha.16
[v0.0.1-alpha.15]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.14...v0.0.1-alpha.15
[v0.0.1-alpha.14]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.13...v0.0.1-alpha.14
[v0.0.1-alpha.13]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.12...v0.0.1-alpha.13
[v0.0.1-alpha.12]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.11...v0.0.1-alpha.12
[v0.0.1-alpha.11]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.10...v0.0.1-alpha.11
[v0.0.1-alpha.10]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.9...v0.0.1-alpha.10
[v0.0.1-alpha.9]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.8...v0.0.1-alpha.9
[v0.0.1-alpha.8]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.7...v0.0.1-alpha.8
[v0.0.1-alpha.7]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.6...v0.0.1-alpha.7
[v0.0.1-alpha.6]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.5...v0.0.1-alpha.6
[v0.0.1-alpha.5]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.4...v0.0.1-alpha.5
[v0.0.1-alpha.4]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.3...v0.0.1-alpha.4
[v0.0.1-alpha.3]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.2...v0.0.1-alpha.3
[v0.0.1-alpha.2]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.1...v0.0.1-alpha.2
[v0.0.1-alpha.1]: https://github.com/AsterCommunity/AsterDrive/releases/tag/v0.0.1-alpha.1
