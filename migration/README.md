# Running Migrator CLI

## Migration policy

The supported migration chain starts at `m20260512_000001_baseline_schema`.

- Fresh installs run the current migration chain directly.
- Existing supported deployments must already have migration metadata from the current chain.
- Historical pre-`v0.1.0` rebase rows are no longer rewritten in place. Databases that still contain those rows must first be upgraded through a supported intermediate release or restored from a backup made after the current baseline was applied.
- Do not edit `seaql_migrations` by hand. If migration metadata and application tables disagree, fix the database from a backup or an explicitly supported upgrade path.

- Generate a new migration file
    ```sh
    cargo run -p migration --features cli -- generate MIGRATION_NAME
    ```
- Apply all pending migrations
    ```sh
    cargo run -p migration --features cli
    ```
    ```sh
    cargo run -p migration --features cli -- up
    ```
- Apply first 10 pending migrations
    ```sh
    cargo run -p migration --features cli -- up -n 10
    ```
- Rollback last applied migrations
    ```sh
    cargo run -p migration --features cli -- down
    ```
- Rollback last 10 applied migrations
    ```sh
    cargo run -p migration --features cli -- down -n 10
    ```
- Drop all tables from the database, then reapply all migrations
    ```sh
    cargo run -p migration --features cli -- fresh
    ```
- Rollback all applied migrations, then reapply all migrations
    ```sh
    cargo run -p migration --features cli -- refresh
    ```
- Rollback all applied migrations
    ```sh
    cargo run -p migration --features cli -- reset
    ```
- Check the status of all migrations
    ```sh
    cargo run -p migration --features cli -- status
    ```
