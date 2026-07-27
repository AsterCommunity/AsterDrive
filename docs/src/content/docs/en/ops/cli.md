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

In operations scripts, if you do not want to repeat long arguments on every command, use the CLI-specific `ASTER_CLI_*` environment variables. They only affect the command-line tools and do not change service startup configuration.

| Environment variable | Corresponding argument | Applicable commands |
| --- | --- | --- |
| `ASTER_CLI_DATABASE_URL` | `--database-url` | `doctor`, `config`, `node enroll` |
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
`ASTER__DATABASE__URL` overrides the `config.toml` read by the service. `ASTER_CLI_DATABASE_URL` is only used as an argument for the operations CLI.

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
- `blob-ref-counts`: compares `file_blobs.ref_count` with real references from `files` / `file_versions`
- `storage-objects`: scans object paths under each storage policy to find missing blobs, untracked objects, and orphan thumbnails
- `folder-tree`: checks missing parent folders, cross-workspace parent folders, and folder cycles

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

For bulk import, the input file can be either of these JSON shapes:

```json
[
  { "key": "public_site_url", "value": ["https://drive.example.com", "https://panel.example.com"] },
  { "key": "auth_cookie_secure", "value": "true" }
]
```

```json
{
  "configs": [
    { "key": "public_site_url", "value": ["https://drive.example.com", "https://panel.example.com"] },
    { "key": "auth_cookie_secure", "value": "true" }
  ]
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

The export result is better suited for review, backup, or script processing.  
If you plan to import it again, first normalize it into the "key/value array" shape above or the `{"configs": [...]}` shape, then pass it to `import`.

If you only want to confirm whether a value is valid, prefer `validate`. Do not use `set` directly.

## Follower Node Enrollment: `node enroll`

This command is only for follower nodes. After the primary admin panel generates an enrollment token, run this on the follower machine:

```bash
./aster_drive node enroll \
  --master-url https://drive.example.com \
  --token enr_xxxxx
```

If you do not pass the database URL explicitly, the command reads `[database].url` from the current `data/config.toml`. You can also specify it directly:

```bash
./aster_drive node enroll \
  --master-url https://drive.example.com \
  --token enr_xxxxx \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc"
```

It performs these steps:

- confirms the current config is `[server].start_mode = "follower"`
- if the current directory has no config file, generates a default `data/config.toml` in follower mode first
- exchanges the token with the primary for local binding information, and writes it into the follower database
- outputs the current listen address, config file path, and next connectivity check hints

The command does not create the primary node's default remote storage target for you, and it does not start the HTTP service. After it succeeds, restart the follower process, then return to the primary admin panel to create or apply the default remote storage target.

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
