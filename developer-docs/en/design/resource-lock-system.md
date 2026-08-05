# Resource Lock System Refactor Contract

This document defines AsterDrive's current resource-lock architecture. The breaking `v0.x` migration is implemented: active `resource_locks` rows are authoritative, and file and folder rows no longer persist an `is_locked` projection.

The lock system is shared by REST mutations, WebDAV Class 2 locking, WOPI locking, administrative cleanup, and read-only lock presentation. After the refactor, active `resource_locks` rows are the only authority. Cache entries and API projections are never authoritative state.

## Decisions

- Drop the `files.is_locked` and `folders.is_locked` columns and all synchronization code around them.
- Replace public `is_locked: bool` fields with a typed `ResourceLockState` projection.
- Add one explicit database lock namespace row per personal or team workspace.
- Use the namespace row as the common transaction serialization point and its `generation` as the cache version.
- Keep mutation decisions on the writer database transaction. Cache is read-only acceleration.
- Define folder and workspace depth locks by resource hierarchy. A path is protocol presentation data, not the sole lock identity.
- Share one Drive-owned lifecycle across REST, WebDAV, WOPI, and administrative operations.
- Keep Forge product-neutral: Forge owns RFC parsing, planning, and response grammar; Drive owns workspace resolution, persistence, permissions, storage, and transactions.

## RFC Basis

Protocol behavior follows the RFC Editor text for [RFC 4918](https://www.rfc-editor.org/rfc/rfc4918):

- Section 6.1 defines the lock model and conflicting locks.
- Section 7.4 defines collection write-lock coverage.
- Section 7.5 requires lock-token submission for every locked resource changed by a method.
- Section 9.10.3 requires all-or-nothing Depth infinity LOCK and UNLOCK behavior.
- Section 9.10.4 defines successful LOCK behavior for unmapped non-collection URLs.
- Section 9.10.5 defines shared and exclusive compatibility.

## Domain Model

```rust
pub enum LockWorkspace {
    Personal { user_id: i64 },
    Team { team_id: i64 },
}

pub enum LockRoot {
    WorkspaceRoot,
    Folder { folder_id: i64 },
    File { file_id: i64 },
}

pub enum LockDepth {
    Resource,
    Infinity,
}

pub enum LockMode {
    Exclusive,
    Shared,
}

pub enum LockOrigin {
    Product,
    WebDav,
    Wopi,
}

pub struct LockTarget {
    pub workspace: LockWorkspace,
    pub root: LockRoot,
    pub depth: LockDepth,
}
```

File and folder roots must belong to the declared workspace. A workspace root derives its identity from the workspace and no longer masquerades as a user or team entity type. File infinity depth may be preserved for WebDAV presentation but has the same conflict coverage as resource depth.

## Persistence

### `resource_lock_namespaces`

```text
id                BIGINT primary key
workspace_type    VARCHAR(16) not null
workspace_id      BIGINT not null
generation        BIGINT not null default 0
created_at        datetime not null
updated_at        datetime not null

unique(workspace_type, workspace_id)
```

The namespace row is both the `SELECT ... FOR UPDATE` serialization point and the version source for read-only cache projections.

### `resource_locks`

```text
id                BIGINT primary key
token             VARCHAR unique not null
namespace_id      BIGINT not null references resource_lock_namespaces(id)
root_kind         VARCHAR(16) not null
root_folder_id    BIGINT null references folders(id)
root_file_id      BIGINT null references files(id)
depth             VARCHAR(16) not null
mode              VARCHAR(16) not null
origin            VARCHAR(16) not null
holder_user_id    BIGINT null
owner_info        TEXT null
lockroot_path     VARCHAR null
timeout_at        datetime null
created_at        datetime not null
```

Valid root combinations are:

| `root_kind` | `root_folder_id` | `root_file_id` |
| --- | --- | --- |
| `workspace_root` | null | null |
| `folder` | non-null | null |
| `file` | null | non-null |

`lockroot_path` is a canonical WebDAV presentation snapshot. Hierarchy conflicts are resolved from workspace and resource identity, not only from string prefixes.

## Authoritative Transaction Order

Every lock mutation and every mutation of a potentially locked resource follows one order:

```text
begin writer transaction
-> resolve workspace and namespace
-> SELECT namespace FOR UPDATE
-> lock storage usage when the mutation changes stored resources
-> lock resource rows in deterministic order
-> SELECT relevant resource_locks FOR UPDATE
-> validate timeout, hierarchy, mode, owner, and submitted credentials
-> mutate resources and/or locks
-> increment namespace generation when the projection changed
-> commit
-> run non-authoritative audit, observation, and cache cleanup
```

No path may hold a `resource_locks` row while waiting for a namespace or target-resource row. Multi-path operations sort workspace and resource identities before acquiring locks. Ancestors are locked from the workspace root toward the target.

## Lifecycle Contracts

Acquire, unlock, force unlock, refresh, expiration cleanup, MOVE rebind, and rooted-lock deletion use the shared Drive lifecycle.

Token/id release may perform an unlocked snapshot read to locate the namespace. Inside the transaction it must lock the namespace and snapshot target first, then re-read the lock row `FOR UPDATE` and verify that token/id, namespace, and root identity are unchanged. A changed identity aborts the transaction and restarts from a fresh snapshot.

Expiration cleanup groups candidates by namespace, rechecks expiration inside a short writer transaction, increments generation only when rows were actually removed, and reports the actual affected-row count.

Resource mutations use a transaction-aware evaluator:

```rust
pub async fn enforce_mutation_locks_on(
    txn: &DatabaseTransaction,
    target: &LockTarget,
    submitted: &SubmittedLockCredentials,
) -> Result<()>;
```

The evaluator checks direct locks, ancestor folder infinity locks, and workspace-root infinity locks. Ordinary REST mutations do not carry WebDAV or WOPI credentials and therefore conflict with any active protocol lock covering the target.

The mutation credential contract is explicit:

- Forge parses `If` and forwards only positive tokens applicable to an actual conflicting lock-root URI. `Not <token>` participates in condition evaluation but is not an authorization credential.
- Drive carries owned `LockMutationCredentials` into the final writer transaction and converts them to the evaluator's borrowed view. There is no `validated=true` or `skip_lock_check` bypass.
- A matching holder may satisfy a Product lock. WebDAV and WOPI locks require their internal lock token and cannot be bypassed only because the actor user matches.
- A WOPI opaque lock value is used only for WOPI header comparison. A successful comparison forwards the corresponding `resource_locks.token`, not the opaque value.

## Atomic WebDAV Lock-Null Creation

Forge owns RFC 4918 LOCK planning, applicable-token selection, and HTTP status mapping. Drive owns path and workspace resolution, storage staging, and the final writer transaction.

For an existing target, Drive acquires the lock in the namespace transaction without staging an object. For an unmapped non-collection target, Drive stages the empty storage object outside the database transaction, then locks the namespace and storage usage, validates the parent collection membership credentials, creates the blob and file metadata, creates the lock row, increments the namespace generation, and commits all database state once.

If the initial resolution found an existing target but the transaction observes that it was concurrently deleted, the transaction rolls back, stages one empty object, and retries. If another request wins the create race before the retry, Drive locks the existing resource and cleans up only the unused object owned by the current request.

`PreparedEmptyFile` distinguishes a shared deduplicated empty object from an owned non-deduplicated object. A known database failure cleans up only the owned non-deduplicated object. A shared deduplicated object is never deleted by the failed request. When the database commit outcome is uncertain, the staged object is retained because committed metadata may already reference it. Cleanup follows staging ownership and never deletes by WebDAV path.

A missing parent collection maps to HTTP 409, an unmapped collection target maps to 404, and an actual lock conflict remains 423. The Forge acquire result reports whether the target already existed; the generic handler does not infer Drive transaction or cleanup behavior.

## Public Projection

```rust
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ResourceLockState {
    Unlocked,
    Direct {
        mode: LockMode,
        expires_at: Option<DateTime<Utc>>,
    },
    Inherited {
        root: LockRootSummary,
        mode: LockMode,
        expires_at: Option<DateTime<Utc>>,
    },
}
```

This state is computed for responses and is never written to file or folder rows. Ordinary projections exclude tokens, WebDAV owner XML, and WOPI lock values. The protected administrative lock API continues to read full lock records.

## Cache Contract

The Forge Redis cache can temporarily fall back to process-local memory when Redis is unavailable. It is therefore a cache, not a distributed mutex. The lock system must not use Redis `SET NX`, `set_bytes_if_absent`, pub/sub delivery, or cached unlocked results as a correctness primitive.

Projection keys include the database generation:

```text
resource_lock_projection:v1:personal:<user_id>:g:<generation>
resource_lock_projection:v1:team:<team_id>:g:<generation>
```

Values contain only generation, root identity, depth, mode, and timeout. Cache fill reads generation, loads active roots on miss, reads generation again, and writes only when both reads match. A generation change retries once; another change returns the current database result without caching.

The TTL is the smaller of the configured maximum projection TTL and the time until the earliest lock timeout. Cached entries are still filtered against the current time after loading. Cache failure always falls back to database reads and changes neither protocol status nor mutation behavior.

Cross-instance events may reclaim old generation keys early, but event delivery is not required for correctness.

## Target Module Boundaries

```text
src/services/files/lock/
  domain.rs
  resolve.rs
  lifecycle.rs
  enforcement.rs
  projection.rs
  cache.rs
  cleanup.rs
  models.rs

src/db/repository/
  lock_namespace_repo.rs
  lock_repo.rs
```

WebDAV remains a protocol adapter and WOPI remains a header/state adapter. Neither owns a duplicate lock transaction. Thin rename-only wrappers are prohibited; extracted functions own a real transaction invariant, domain conversion, batch query, or protocol mapping responsibility.

## Breaking Migration

1. Create namespaces and the new root/mode/depth/origin columns.
2. Resolve every existing file/folder/root lock to its workspace and namespace.
3. Convert old target and boolean fields to the new enum columns.
4. Fail the migration on unresolved workspace/root identity, invalid values, or duplicate tokens. Do not silently drop locks.
5. Switch code to the new model.
6. Drop old target, shared, and deep columns.
7. Drop `files.is_locked` and `folders.is_locked`.

The down migration must fail explicitly when new data cannot be represented by the old model.

## Acceptance Matrix

The implementation must cover:

- File, folder, personal-root, and team-root lifecycle operations.
- Shared/exclusive compatibility and timeout boundaries.
- Folder and workspace infinity coverage, including resources created later.
- Cross-mount identity conflicts.
- Barrier/failpoint concurrency tests for parent/child acquire, acquire/unlock, refresh/cleanup, and MOVE/rebind.
- Generation increments and stale cache-fill isolation.
- Memory/Redis cache parity and database fallback.
- Cache payloads that exclude tokens and owner payloads.
- REST, WebDAV, WOPI, and admin cross-protocol behavior.
- Atomic lock-null success and failure cleanup.
- OpenAPI and generated TypeScript migration from `is_locked` to `lock_state`.
- SQLite migration tests and PostgreSQL/MySQL locking paths.
- WebDAV, WOPI, strict Clippy, and Litmus compatibility validation.

The refactor is complete only when no file/folder entity field or synchronization helper named `is_locked` remains and every mutation uses the writer-transaction lock evaluator.
