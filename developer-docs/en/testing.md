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

Default SQLite:

```bash
cargo test
```

Switch to PostgreSQL:

```bash
ASTER_TEST_DATABASE_BACKEND=postgres cargo test
```

Switch to MySQL:

```bash
ASTER_TEST_DATABASE_BACKEND=mysql cargo test
```

If you only want one test group, filter by name as usual:

```bash
ASTER_TEST_DATABASE_BACKEND=postgres cargo test --test test_search test_search_by_name
ASTER_TEST_DATABASE_BACKEND=mysql cargo test --test test_admin test_admin_team_crud
```

## Current behavior

`common::setup()` in `tests/common/mod.rs` works like this:

1. Read `ASTER_TEST_DATABASE_BACKEND`
2. If it is `sqlite`, return the in-memory SQLite `AppState`
3. If it is `postgres` or `mysql`, start a shared container through `testcontainers`
4. Create a unique database name for the current test instance on top of the base database in the container
5. Run migrations, initialize default policies and runtime config, and then return `AppState`

This means:

- The shared PostgreSQL / MySQL container is reused across multiple local test runs whenever possible
- The actual databases are not reused, so parallel integration tests do not pollute one another
- Leftover databases from exited test processes are cleaned up automatically the next time that backend's container starts

## PostgreSQL / MySQL differences

### PostgreSQL

- Uses the container's `postgres` admin account to create the base database
- The test-instance database is created through that admin connection
- The business test connection uses the dedicated test database directly

### MySQL

- Business tests still use the container's `aster` user by default
- The isolated database is still created through `root`
- But permissions for the normal test user are granted once at container startup instead of running a separate `GRANT` for each test database

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

The repository still has [`tests/test_database_backends.rs`](../../tests/test_database_backends.rs), and its purpose has not changed:

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
cargo test --test test_sftp
```

This test starts an `lscr.io/linuxserver/openssh-server` container through `testcontainers` by default and runs a real upload, download, range read, delete, and host-key fingerprint confirmation flow. It requires a local Docker / container runtime.

If the current environment cannot run Docker, disable it explicitly:

```bash
ASTER_SFTP_TEST_DOCKER=0 cargo test --test test_sftp
```

With that variable set, the container round trip is skipped. Do not make this the default CI behavior; SFTP is a real storage driver, so PRs touching the driver, connector, descriptor, or upload/download path should keep the default Docker test enabled.

`src/storage/drivers/sftp.rs` also contains a manual real-server test that requires `ASTER_SFTP_TEST_*` and `ASTER_SFTP_TEST_HOST_KEY_FINGERPRINT`. It does not replace the default Docker coverage in `tests/test_sftp.rs`; it is mainly for debugging compatibility with a specific SFTP server.
