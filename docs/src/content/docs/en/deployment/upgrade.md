---
title: "Upgrade and Version Migration"
---

This page covers the AsterDrive version upgrade flow: what to back up before upgrading, how to upgrade binary and Docker deployments, how to verify after upgrade, and how to roll back if something goes wrong.
Find the section matching your deployment method.

:::caution[Back up before upgrading]
Stable releases aim to keep upgrade paths clear, but upgrades may still include database migrations, configuration item migrations, or image dependency changes. Before upgrading, fully back up `config.toml`, the database, and local storage directories. See [Backup and Restore](./backup/).
:::

## Do These Before Upgrading

No matter which deployment method you use, do these first:

1. **Read the corresponding version section in the [changelog](https://github.com/AsterCommunity/AsterDrive/blob/master/CHANGELOG.md)**, especially `Changed` / `Removed` / `Deprecated`.
2. **Take a full backup once**. At minimum, include `data/config.toml`, the database, and all local storage directories. Preserve both the login signing key and MFA encryption key in `config.toml`. See [Backup and Restore](./backup/).
3. **Confirm the database account has DDL permissions**. Startup automatically runs migrations. If the account lacks `CREATE` / `ALTER`, startup fails.
4. **Estimate the downtime window**. Small deployments take tens of seconds. If the database already has a lot of data, read the MySQL section below.

If you manage production, run the full upgrade flow in a test environment before upgrading production.

## Docker Upgrade

The most common and simplest scenario.

```bash
# Pull the latest image
docker pull ghcr.io/astercommunity/asterdrive:latest

# Restart the container
docker compose down
docker compose up -d

# Watch startup logs and confirm migration finishes
docker compose logs -f asterdrive
```

Startup logs show migration phase output. Seeing a message such as `application started` is enough.

If you use `docker run` instead of compose, keep mounted volumes unchanged (`asterdrive-data` volume + `config.toml` mount point).

After upgrading, validate according to [Check These Items Immediately After Startup](./runtime-behavior/#check-these-items-immediately-after-startup).

## systemd / Binary Upgrade

```bash
# 1. Stop service
sudo systemctl stop asterdrive

# 2. Back up current binary in case rollback is needed
sudo cp /usr/local/bin/aster_drive /usr/local/bin/aster_drive.bak

# 3. Replace binary
sudo install -m 755 ./aster_drive /usr/local/bin/aster_drive

# 4. Start
sudo systemctl start asterdrive

# 5. Watch logs
sudo journalctl -u asterdrive -f
```

Migrations run automatically at startup. Seeing the service listen on the port normally is enough.

If you want to run migration separately before startup, for example to separate migration errors from service startup errors, use:

```bash
sudo -u asterdrive ./aster_drive database-migrate
```

See [Operations CLI](./ops-cli/).

## MySQL Large Table ALTER Notes

:::caution[Large deployments need a maintenance window]
Some version migrations execute `ALTER TABLE ... MODIFY COLUMN` on multiple tables. If your `files` / `file_blobs` tables already have millions of rows, MySQL 5.7 / 8.0 default `INPLACE` may still trigger full table rebuilds and hold table locks for a long time.
:::

For large MySQL deployments:

1. **Reserve a maintenance window**. Stop the service, run migration, confirm completion, then start the service.
2. **Or use online schema change tools** such as `gh-ost` or `pt-online-schema-change` to run ALTER first, then start the new version service.

PostgreSQL and SQLite are not affected by this limitation.

If future versions include similar migrations, they will be clearly marked in the [changelog](https://github.com/AsterCommunity/AsterDrive/blob/master/CHANGELOG.md).

## Post-Upgrade Validation

After upgrading, run through this checklist:

1. `/health` returns 200.
2. `/health/ready` returns 200 with `data.status` set to `ready` (this checks the DB, default policy, and lightweight driver prerequisites; it does not prove remote object-storage read/write availability).
3. The admin panel opens normally.
4. Use a real account to log in, upload a file, download, share, and restore one trash item.
5. Run `aster_drive doctor` once.

If WebDAV / WOPI is enabled, also validate:

- WebDAV client can mount, read, and write.
- WOPI client can open Office files.

These validations are not ceremony; they are **your own confidence check** that the upgrade did not break any edge feature.

## What If Upgrade Fails

Separate by failure phase:

### Migration Phase Fails

Read the specific error in logs. Common causes:

- Database account lacks DDL permissions -> grant permissions and start again.
- A previous upgrade was interrupted and the migration table state is inconsistent -> back up the current state before contacting developers. This is important.

If you urgently need to restore service, you can roll back to the old version only if **no DDL from the migration actually succeeded**. If migration partially executed, the old version may fail to start because the schema no longer matches. You must restore from backup.

### Some Features "Disappeared" After Startup

They usually did not disappear; their location or name changed. First read the corresponding version section in the [changelog](https://github.com/AsterCommunity/AsterDrive/blob/master/CHANGELOG.md). If the changelog does not mention it, open an issue.

### Behavior Is Abnormal After Startup but There Is No Error

Handle it according to [Troubleshooting](./troubleshooting/).

## Rollback

:::danger[Cross-major-version rollback has data risk]
If the new version has already successfully run migrations and changed the schema, rolling back to an old version usually fails to start because the old binary does not recognize the new schema. Worse, it may start but silently truncate data.

**The safe rollback method is restoring from backup**, not simply replacing the binary with the old one.
:::

Rollback steps:

1. Stop the service.
2. Restore `config.toml`, database, and local storage directories from backup to the point before upgrade.
3. Replace the binary with the old version.
4. Start.
5. Run `aster_drive doctor` to confirm state.

See [Backup and Restore](./backup/#restore-sequence).

## Upgrading from Old Versions

The formal upgrade path for the current version is based on migration history from `v0.1.0` and later. That means the database `seaql_migrations` table should contain the current baseline migration record:

```text
m20260512_000001_baseline_schema
```

If you upgrade from `v0.1.0` or later, follow the Docker / systemd steps above. The service automatically applies subsequent migrations during startup.

Early alpha / beta / rc prerelease builds used migration history that has since been rearranged. The current version no longer includes those rebase compatibility branches, and it will not automatically rewrite old migration records into the current baseline. Instances still on early prerelease history need to first upgrade to an intermediate version that can complete the rebase at that time and confirm migration completion, then continue upgrading; or restore from a backup that already completed the current baseline.

Do not manually modify `seaql_migrations`, and do not empty business tables to bypass migration errors. If migration metadata and real business table structure do not match, directly starting the new version may create harder-to-recover data problems.

:::tip[Unsure which migration range your database is in?]
Back up the current state first, then use the current version's `aster_drive doctor --database-url ...` to inspect the `Database migrations` check item. It lists unknown migration records or pending current migrations.
:::
