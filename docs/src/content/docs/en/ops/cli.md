---
description: "AsterDrive operations CLI reference: doctor deployment checks, offline system settings, remote node enroll, and cross-database migration command usage."
title: "Operations CLI"
---

:::tip[What this page covers]
Besides starting the service, the `aster_drive` executable includes a set of command-line subcommands: `doctor` for deployment checks, `config` for offline system settings, `node` for follower node enrollment, and `database-migrate` for cross-database migration.
**Use the admin panel first for day-to-day settings changes**. This page is for cases such as "the admin panel is inaccessible", "I want to script this", or "I need to switch database backends".
:::

AsterDrive now includes a set of command-line tools for these scenarios:

- The service is already deployed, but you want to check the database and key settings offline first.
- The admin panel is temporarily inaccessible, and you need to view or modify system settings directly.
- You need to run `node enroll` on a follower node shell.
- You are preparing to migrate SQLite to PostgreSQL or MySQL, or migrate back the other way.
- You want check results to be handled by scripts, CI, or an operations platform.

These commands are still part of the same `aster_drive` executable.  
Running `./aster_drive` directly starts the service. Running it with a subcommand performs an operations task.

## Service roles

`./aster_drive [serve [all|api|worker]]` defaults to `all`. Follower remains config-selected. See [separate roles](/en/deploy/roles/) for ownership, cancellation, limits and migration.

Server startup (`serve all/api/worker` or no subcommand) and `--config-file` / `--data-dir` remain available without the `operations-cli` Cargo feature. That feature controls the operational subcommands below. Build a service-only binary with `cargo build --no-default-features --features server`, or pass `--build-arg CARGO_FEATURES=server` to Docker.

## Command Quick Reference

| What you want to do | Command | When to use it |
| --- | --- | --- |
| Check database, migrations, public site URL, mail, and default policies | `doctor` | New deployment, before launch, after upgrade |
| Deep-check capacity, blob references, object inventory, and folder tree | `doctor --deep` | When you suspect data and storage are inconsistent |
| Automatically fix some counter drift | `doctor --deep --fix` | After confirming the CLI is allowed to modify counters |
| View system settings offline | `config list` / `config get` | When the admin panel is inaccessible, or for scripts |
| Modify system settings offline | `config set` / `config import` | Maintenance windows, batch configuration, disaster recovery |
| Complete enrollment on a follower | `node enroll` | After the primary admin panel generates an enrollment command |
| Migrate between SQLite / PostgreSQL / MySQL | `database-migrate` | Switching database backends or rehearsing migration |

:::caution[Back up before letting the CLI write data]
`doctor --fix`, `config set`, `config import`, and `database-migrate` may all change the database. Back up production first. Avoid experimental writes without a backup, because that may turn into a restore incident later.
:::

When running outside the instance directory, use `--config-file /srv/asterdrive/data/config.toml` or `--data-dir /srv/asterdrive/data`. Database target precedence is explicit `--database-url`, `ASTER_CLI_DATABASE_URL`, then `[database].url` in that config file. `doctor` and `config` only access an existing instance; they do not create a default config file or SQLite database when the target is missing. `database-migrate` always requires explicit source and target URLs.

Prometheus and OTLP exporters absent from the binary, including their exporter-specific settings, are skipped without blocking offline `config` operations or server startup. `doctor` checks the effective instance configuration. Compiled and enabled exporters still require valid settings and credentials.

## Prepare the Database URL First

The most common forms are:

```text
sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc
postgres://user:password@127.0.0.1:5432/asterdrive
mysql://user:password@127.0.0.1:3306/asterdrive
```

If you use the official Docker container, the simplest approach is usually to enter the container first, then run these commands:

```bash
docker exec -it asterdrive sh
```

That avoids confusion between SQLite paths, mounted volumes, and actual file locations inside the container.

## Use Environment Variables Instead of Common Arguments

Operations scripts can use `ASTER_CLI_*` to supply defaults for the corresponding arguments. Database targets and output formats affect their operations commands; the instance-location options also apply to `serve` and startup without a subcommand.

| Environment variable | Corresponding argument | Applicable commands |
| --- | --- | --- |
| `ASTER_CLI_DATABASE_URL` | `--database-url` | `doctor`, `config`, `node enroll` |
| `ASTER_CLI_CONFIG_FILE` | `--config-file` | server startup and CLI commands |
| `ASTER_CLI_DATA_DIR` | `--data-dir` | server startup and CLI commands |
| `ASTER_CLI_OUTPUT_FORMAT` | `--output-format` | `doctor`, `config`, `node`, `database-migrate` |
| `ASTER_CLI_MASTER_URL` | `node enroll --master-url` | `node enroll` |
| `ASTER_CLI_ENROLLMENT_TOKEN` | `node enroll --token` | `node enroll` |
| `ASTER_CLI_SOURCE_DATABASE_URL` | `database-migrate --source-database-url` | `database-migrate` |
| `ASTER_CLI_TARGET_DATABASE_URL` | `database-migrate --target-database-url` | `database-migrate` |

`doctor` also has these script-friendly switches:

```bash
ASTER_CLI_DOCTOR_STRICT=true
ASTER_CLI_DOCTOR_DEEP=true
ASTER_CLI_DOCTOR_FIX=true
ASTER_CLI_DOCTOR_SCOPE=blob-ref-counts,storage-objects
ASTER_CLI_DOCTOR_POLICY_ID=3
```

During database migration, if you need progress output or a smaller batch size, you can also use:

```bash
ASTER_CLI_PROGRESS=1
ASTER_CLI_COPY_BATCH_SIZE=100
```

:::tip[Service configuration and CLI parameters use separate ENV sets]
`ASTER__DATABASE__URL` overrides the database address read from `config.toml`; `doctor` / `config` reuse that result when no explicit target is supplied. `ASTER_CLI_DATABASE_URL` directly supplies the operations database target, taking precedence over the instance config without changing the server startup target.

If you start the service and run commands from the same shell, keep these two classes of variables clearly separated. Troubleshooting becomes much easier.
:::

## Deployment Check: `doctor`

After first deployment, or before production launch, run this first:

```bash
./aster_drive doctor \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc"
```

Default mode checks the places most likely to go wrong:

- whether the database can be connected
- whether pending migrations remain
- if the backend is SQLite, whether the `FTS5 + trigram tokenizer` search acceleration capability is available, and whether related FTS tables / triggers are complete
- whether runtime system settings can be read normally
- whether `Public Site URL` is empty or malformed
- whether `Public Site URL` is still `http://`, which means production lacks HTTPS
- whether mail delivery configuration is complete
- whether preview application configuration parses correctly
- whether the default storage policy and default policy group are ready

If you want `warn` to count as failure, add:

```bash
./aster_drive doctor \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --strict
```

For script processing, add an output format:

```bash
./aster_drive doctor \
  --output-format json \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc"
```

Best uses:

- first acceptance check after a new deployment
- health check after upgrade
- after changing `Public Site URL`, mail, or preview applications, confirm the config was not broken
- confirm a default SQLite deployment really has search acceleration instead of silently falling back to full table scans

If you suspect existing "data and storage inconsistency" in the database, run a deep check:

```bash
./aster_drive doctor \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --deep
```

`--deep` adds these checks:

- `storage-usage`: compares `users.storage_used` / `teams.storage_used` with actual usage from files and historical versions
- `blob-ref-counts`: compares `file_blobs.ref_count` with real references from `files` / non-current `file_revisions`
- `storage-objects`: scans object paths under each storage policy to find missing blobs, untracked objects, and orphan thumbnails
- `folder-tree`: checks missing parent folders, cross-workspace parent folders, and folder cycles
- `revision-ledger`: checks history/head/predecessor pointers and the `files` current projection

If you only want part of the deep checks, narrow the scope:

```bash
./aster_drive doctor \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --scope blob-ref-counts,storage-objects
```

If you only want to check a specific storage policy, add:

```bash
./aster_drive doctor \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --scope storage-objects \
  --policy-id 3
```

When counter drift is found, the CLI can fix it directly:

```bash
./aster_drive doctor \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --deep \
  --fix
```

Pay attention to four things:

- `--scope` only affects deep checks. It does not disable base checks such as database connection, migrations, and runtime configuration.
- `--policy-id` only applies to `blob-ref-counts` and `storage-objects`; `storage-usage` and `folder-tree` still check the whole database.
- `--fix` currently only repairs `storage_used` and `file_blobs.ref_count` counters. It does not automatically delete objects or modify folder structure.
- Deep scans run in database batches and object storage pages, but they only validate path-level existence. They do not read object contents or calculate checksums.

## Offline System Settings: `config`

For normal settings changes, prefer `Admin -> System Settings`.  
`config` is better for these cases:

- the admin panel is temporarily inaccessible
- you do not want to use a web page during a maintenance window
- you want to export, validate, or import system settings in bulk

List current items:

```bash
./aster_drive config \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  list
```

View one item:

```bash
./aster_drive config \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  get \
  --key public_site_url
```

Validate first, then write to the database:

```bash
./aster_drive config \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  validate \
  --key public_site_url \
  --value '["https://drive.example.com"]'

./aster_drive config \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  set \
  --key public_site_url \
  --value '["https://drive.example.com"]'
```

`public_site_url` supports multiple public origins. When writing from the command line, pass a JSON string array:

```bash
./aster_drive config \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  set \
  --key public_site_url \
  --value '["https://drive.example.com","https://panel.example.com"]'
```

Sensitive settings prompt without terminal echo by default. Automation can use stdin or a file with mode `0600`:

```bash
printf '%s' 'smtp-secret' | ./aster_drive config --database-url "$DB_URL" set --key mail_smtp_password --value-stdin
./aster_drive config --database-url "$DB_URL" set --key mail_smtp_password --value-file /run/secrets/smtp-password
```

Explicit `--value` and `ASTER_CLI_CONFIG_VALUE` remain available, but expose the value in command arguments, shell history, or process diagnostics.

`config export` emits the single versioned JSON backup format accepted directly by `import` and `validate --input-file`. Sensitive values are omitted by default and listed under `omitted_sensitive_keys`. A complete restore requires `--include-sensitive`; its output contains plaintext credentials and must be protected as a secret file. Import files require mode `0600`.

```json
{
  "format": "asterdrive.runtime-config",
  "version": 1,
  "configs": [{"key":"public_site_url","value":["https://drive.example.com"],"source":"system","visibility":"private","is_sensitive":false}],
  "omitted_sensitive_keys": ["mail_smtp_password"]
}
```

Import example:

```bash
./aster_drive config \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  import \
  --input-file ./runtime-config.json
```

Export existing config like this:

```bash
./aster_drive config \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --output-format pretty-json \
  export
```

Import validates every entry before writing the complete file in one transaction. Failure leaves no partial changes. Settings absent from the backup keep their target values. Only the current versioned backup format is accepted; display output and older key/value arrays are not restorable inputs.

System entries must exist in the current registry; their `source`, `visibility`, and `is_sensitive` metadata are read-only. Custom entries must explicitly use `custom`. Duplicate keys, unknown system keys, redacted placeholders, and display-only fields are rejected. Sensitive entries must remain `private`, and imports cannot remove existing sensitivity. Reload notifications are sent only after commit; if delivery fails, the error states that settings were saved and asks you to check `config_sync` or restart the affected instances.

Set a restrictive umask such as `umask 077` before redirecting an export that includes secrets.

If you only want to confirm whether a value is valid, prefer `validate`. Do not use `set` directly.

## Follower Node Enrollment: `node enroll`

This command is only for follower nodes. After the primary admin panel generates an enrollment token, run this on the follower machine:

```bash
./aster_drive node enroll \
  --master-url https://drive.example.com \
  --token-prompt
```

Interactive token input hides terminal echo. Automation can use `--token-stdin`, `--token-file`, or `ASTER_CLI_ENROLLMENT_TOKEN`. Explicit `--token` exposes it in process arguments.

If you do not pass the database URL explicitly, the command reads `[database].url` from the current `data/config.toml`. You can also specify it directly:

```bash
./aster_drive node enroll \
  --master-url https://drive.example.com \
  --token-file /run/secrets/enrollment-token \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc"
```

It performs these steps:

- confirms the current config is `[server].start_mode = "follower"`
- if the current directory has no config file, generates a default `data/config.toml` in follower mode first
- exchanges the token with the primary for local binding information, and writes it into the follower database
- outputs the current listen address, config file path, and next connectivity check hints

The command does not create a primary-managed remote storage target or start the HTTP service. After it succeeds, restart the follower process, create a target from the primary admin panel, and select it explicitly in the remote storage policy.

For Docker followers, using startup environment variables to auto-enroll is recommended instead of entering the container manually to run this command. See [Docker Follower Node Deployment](/en/deploy/follower-node/).

## Cross-Database Migration: `database-migrate`

This command is for "switching database backends".  
It is not the automatic schema migration that runs during normal startup. It copies existing business data from one database to another.

Common scenarios:

- SQLite to PostgreSQL
- SQLite to MySQL
- switching between PostgreSQL and MySQL

Recommended sequence:

1. Run `--dry-run` first.
2. Prepare a downtime window so the source database is not written during migration.
3. Run the real migration.
4. Only cut production over to the new database after `ready_to_cutover = true`.

Trial run:

```bash
./aster_drive database-migrate \
  --source-database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --target-database-url "postgres://user:password@127.0.0.1:5432/asterdrive_new" \
  --dry-run
```

Real run:

```bash
./aster_drive database-migrate \
  --source-database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --target-database-url "postgres://user:password@127.0.0.1:5432/asterdrive_new"
```

Verify target database only:

```bash
./aster_drive database-migrate \
  --source-database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --target-database-url "postgres://user:password@127.0.0.1:5432/asterdrive_new" \
  --verify-only
```

This command currently handles:

- checking migration state for source and target databases
- automatically bringing the target schema up to the current version
- copying business tables in a fixed order
- validating row counts, unique constraints, and foreign keys
- writing checkpoints into the target database so the same command can resume after interruption

Remember three things when using it:

- The source database must already be at the "current schema"; pending migrations cause immediate refusal.
- Do not continue writing new data into the source database during migration.
- Only `ready_to_cutover = true` in the report means the target database is ready for switching.

## When to Read This Page First

- Deployment is complete, but you are not ready to launch confidently.
- The admin panel cannot open, and you urgently need to inspect configuration.
- A follower node has an enrollment token and needs to enroll from the shell.
- You are preparing to move from SQLite to PostgreSQL / MySQL.
- You want to turn checks into scripts.
