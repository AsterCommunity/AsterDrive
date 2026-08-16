# Testing and Database Backends

This document describes the test-backend switching mechanism that is already implemented in the repository, not a future plan.

## Bottom line

- Integration tests still use in-memory SQLite by default
- `ASTER_TEST_DATABASE_BACKEND` can switch the shared `common::setup()` in `tests/common/mod.rs` to PostgreSQL or MySQL
- PostgreSQL / MySQL do not require you to hand-write a database URL; tests start containers through `testcontainers`
- To support parallel tests, each test instance gets its own database under PostgreSQL / MySQL instead of sharing a schema

## Environment variable

Supported values:

- `sqlite`
- `postgres`
- `mysql`

If unset, it behaves as:

```bash
ASTER_TEST_DATABASE_BACKEND=sqlite
```

## How to run

Integration tests are grouped into Cargo test targets by product domain:

- `auth`: authentication, MFA, external auth, and user accounts
- `files`: files, folders, uploads, metadata, search, and trash
- `sharing`: shares, teams, and public access
- `storage`: storage drivers, policies, remote nodes, and migration
- `operations`: administration, audit, CLI, maintenance, mail, and tasks
- `platform`: databases, cache, middleware, configuration, and structural contracts
- `webdav` / `wopi`: protocol integration tests
- `multi_primary`: multi-Primary E2E tests that require the `multi-primary-e2e` feature

Use `cargo nextest run --test <target> <module>::` to narrow the default SQLite test suite, for example `cargo nextest run --test files search::`.

Default SQLite:

```bash
cargo nextest run
```

PostgreSQL and MySQL use the dedicated `database` profile. It preserves nextest's process-per-test isolation while allowing a longer diagnostic window for database bootstrap and abnormal-exit cleanup.

Switch to PostgreSQL:

```bash
ASTER_TEST_DATABASE_BACKEND=postgres cargo nextest run --profile database
```

Switch to MySQL:

```bash
ASTER_TEST_DATABASE_BACKEND=mysql cargo nextest run --profile database
```

If you only want one test group, filter by name as usual:

```bash
ASTER_TEST_DATABASE_BACKEND=postgres cargo nextest run --profile database --test files search::test_search_by_name
ASTER_TEST_DATABASE_BACKEND=mysql cargo nextest run --profile database --test operations admin::test_admin_team_crud
```

To reuse an already running external MySQL instance, point `ASTER_TEST_MYSQL_DATABASE_URL` at a dedicated test database. When that database is not the schema-template container managed by testcontainers, also set `ASTER_TEST_DISABLE_MYSQL_SCHEMA_TEMPLATE=1`; the test will migrate and exercise that database directly. This switch is only for disposable external test databases, never an instance containing product data.

## Current behavior

`common::setup()` in `tests/common/mod.rs` works like this:

1. Read `ASTER_TEST_DATABASE_BACKEND`
2. If it is `sqlite`, return the in-memory SQLite `AppState`
3. If it is `postgres` or `mysql`, start a shared container through `testcontainers`
4. Validate the migration fingerprint under a suite-scoped cross-process lock, rebuilding one migrated PostgreSQL template database or MySQL schema template only when it is stale
5. Clone a PostgreSQL database from the template, or build an isolated MySQL schema from the in-memory DDL and migration-history snapshot
6. Initialize default policies and runtime config in the isolated database, and then return `AppState`

This means:

- The shared PostgreSQL / MySQL container is reused across multiple local test runs whenever possible
- The actual databases are not reused, so parallel integration tests do not pollute one another
- Resources from processes that exit during one nextest run are retained under `NEXTEST_RUN_ID` to avoid interleaving large MySQL table creates and drops; the next run reclaims them deterministically

## PostgreSQL / MySQL differences

### PostgreSQL

- Uses the container's `postgres` admin account to create the base database
- The suite migrates one template database and each test instance is created with `CREATE DATABASE ... TEMPLATE ...`
- The business test connection uses the dedicated test database directly

### MySQL

- Business tests still use the container's `aster` user by default
- The isolated database is still created through `root`
- Permissions for the normal test user are granted once at container startup instead of running a separate `GRANT` for each test database
- Migrations run once on the suite template. Each process snapshots DDL and migration history, releases the fixture lock, and then builds its own schema in parallel
- The reusable-container endpoint identity includes the configured `table_definition_cache` and `max_connections`, so a changed capacity contract is reprobed and applied

## When to switch backends

Do not rely on SQLite alone when:

- You just changed repo-layer queries with backend-specific branches
- You just changed full-text search, indexes, pagination, sorting, or case-insensitive matching
- You suspect a SQL / SeaORM builder behaves differently on PostgreSQL or MySQL
- You are fixing a bug that only appears in production databases while SQLite stays green

Practical guidance:

- Use SQLite for fast iteration
- After changing database-related logic, rerun at least once with `postgres`
- If the code path still has MySQL-specific branches, rerun with `mysql` as well

## Relation to existing smoke tests

The repository still has [`tests/platform/database_backends.rs`](../../../tests/platform/database_backends.rs), and its purpose has not changed:

- It mainly covers production-database smoke behavior
- It explicitly validates PostgreSQL / MySQL search indexes, search flows, and cross-database migration paths
- It is a dedicated backend smoke suite, not the only place that can run multiple backends; any integration test that uses `common::setup()` can also switch backends through `ASTER_TEST_DATABASE_BACKEND`

The new `ASTER_TEST_DATABASE_BACKEND` mechanism solves a different problem:

- It lets most integration tests that already use `common::setup()` rerun against other backends without changing the test body

## Limits and notes

- PostgreSQL / MySQL depend on a locally available Docker or container runtime
- The first run is slower because the image must be pulled
- Repeated `postgres` / `mysql` runs in the same workspace usually reuse the shared container, so the cold-start cost is much lower
- If a test does not go through `common::setup()` and instead initializes the database manually, it will not automatically pick up this switch
- `common::setup_with_database_url(...)` is still available for cases that need an explicit database URL; it does not interpret `ASTER_TEST_DATABASE_BACKEND` for you

## Troubleshooting tips

If you suspect the test did not switch backends as expected, check these three things first:

1. Does the test case actually use `common::setup()`?
2. Is `ASTER_TEST_DATABASE_BACKEND` exported in the shell?
3. Is Docker available locally, and can the corresponding image start successfully?

## SFTP Integration Tests

The SFTP driver has a dedicated integration test:

```bash
cargo nextest run --test storage sftp::
```

This test starts an `lscr.io/linuxserver/openssh-server` container through `testcontainers` by default and runs a real upload, download, range read, delete, and host-key fingerprint confirmation flow. It requires a local Docker / container runtime.

If the current environment cannot run Docker, disable it explicitly:

```bash
ASTER_SFTP_TEST_DOCKER=0 cargo nextest run --test storage sftp::
```

With that variable set, the container round trip is skipped. Do not make this the default CI behavior; SFTP is a real storage driver, so PRs touching the driver, connector, descriptor, or upload/download path should keep the default Docker test enabled.

`src/storage/drivers/sftp.rs` also contains a manual real-server test that requires `ASTER_SFTP_TEST_*` and `ASTER_SFTP_TEST_HOST_KEY_FINGERPRINT`. It does not replace the default Docker coverage in `tests/storage/sftp.rs`; it is mainly for debugging compatibility with a specific SFTP server.
