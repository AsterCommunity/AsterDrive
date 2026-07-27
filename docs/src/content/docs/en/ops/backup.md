---
description: "AsterDrive backup and restore: which directories and secrets to back up, backup strategy, restore order, and post-restore validation steps."
title: "Backup and Restore"
---

AsterDrive currently **does not provide a unified `backup` / `restore` CLI**.  
The safer approach is to use the backup capabilities of your database and storage backend directly, then use `aster_drive doctor` for unified post-restore validation.

The reason is simple:

- SQLite, PostgreSQL, and MySQL already have mature backup tools and restore workflows.
- Local disk and S3 / MinIO do not have the same data consistency boundaries, so they are not a good fit for a single unified command.
- Exporting only the database is not a complete backup. Local object directories, avatar directories, and object storage state must also be considered.

## Define the Backup Boundary First

At minimum, preserve these together:

- `data/config.toml`
- the current database
- all local persistent directories

`config.toml` contains the login signing key and MFA/TOTP encryption key. During restore, you must use the same configuration file. Do not create a fresh default config and attach it to an old database.

:::caution[Do not lose `auth.mfa_secret_key`]
If any users have MFA enabled, losing or replacing `auth.mfa_secret_key` makes existing authenticator secrets impossible to decrypt. Users will no longer be able to complete two-factor verification with their existing authenticator, and an administrator will need to reset MFA for each affected user before they can bind a new one.
:::

Here, "local persistent directories" usually include:

- local storage policy directories created in the admin panel, such as `/data/uploads` or `data/uploads`
- the local directory for `Admin -> System Settings -> User Management -> Avatar Directory`; the default is `data/avatar`
- any other `local` storage policy root directories you configured manually

If you use local storage, underlying blobs and thumbnails follow their respective local storage roots. **Do not back up only the database**.

These directories are usually not long-term data and are not recommended as formal backup contents:

- `data/.tmp`
- `data/.uploads`

Whether to keep log files depends on your audit, troubleshooting, and compliance requirements. They are not required to restore AsterDrive's runtime state.

## Consistency Principles

Before taking backups, keep these rules in mind:

- The safest approach is to schedule a maintenance window, stop writes, and then back up the database and local persistent directories at the same time.
- If you must back up online, prefer the database backend's own online backup semantics instead of relying on manual judgment about backup timing.
- Do not restore a "new database snapshot" together with an "old object directory". When points in time do not match, the most common failure is database references that point to missing objects.
- `database-migrate` is a cross-database migration tool, not a daily backup tool.
- `config export` only exports runtime system settings. It does not replace a full restore.

## Recommended Strategies

### SQLite + Local Storage

This is the most common setup for single-machine, NAS, and most Docker / systemd deployments.

Recommended sequence:

1. Stop the AsterDrive service or container.
2. Archive the persistent contents under `data/`.
3. If you have local storage directories outside `data/`, or an avatar directory configured with an absolute path, archive them too.
4. Start the service.

For systemd / directly running the binary, a common approach looks like this:

```bash
sudo systemctl stop asterdrive
sudo tar -C /var/lib/asterdrive \
  --exclude='data/.tmp' \
  --exclude='data/.uploads' \
  -czf /srv/backups/asterdrive-$(date +%F-%H%M%S).tar.gz \
  data
sudo systemctl start asterdrive
```

If you use the default SQLite database, a local policy under `data/uploads`, and the default avatar directory, this archive usually covers:

- `data/config.toml`
- `data/asterdrive.db`
- `data/uploads/` (only when you manually configured a local policy there)
- `data/avatar/`

Docker deployment is essentially the same thing:  
stop the container first, then back up the mounted volume or the real directory behind the bind mount.

### PostgreSQL / MySQL + Local Storage

For this setup, treat "database backup" and "local directory backup" as separate operations:

- PostgreSQL: prefer `pg_dump`, physical backup, or your existing managed backup system.
- MySQL: prefer `mysqldump`, physical backup, or your existing managed backup system.
- Local storage directories and avatar directories: continue using `tar`, `rsync`, filesystem snapshots, or your host backup solution.

Example:

```bash
pg_dump \
  --format=custom \
  --file /srv/backups/asterdrive-$(date +%F-%H%M%S).dump \
  "postgres://user:password@127.0.0.1:5432/asterdrive"
```

```bash
mysqldump \
  --single-transaction \
  --routines \
  --events \
  --databases asterdrive \
  > /srv/backups/asterdrive-$(date +%F-%H%M%S).sql
```

If the database and local object directories are not captured at the same consistent point in time, restored references may still drift. It is still best to handle them together in a maintenance window.

### External Database + S3 / MinIO Object Storage

This deployment must still back up at least:

- `data/config.toml`
- the database
- the local avatar directory, if user avatar upload is enabled

Object data itself is better protected by object-storage-side capabilities:

- bucket versioning
- lifecycle rules
- cross-region replication
- managed snapshots / backups

If S3 / MinIO does not have versioning or replication enabled, backing up only the database is not a complete plan.

## Restore Sequence

Restore in this order:

1. Stop AsterDrive.
2. Restore `config.toml`.
3. Restore the database to the same backup point in time.
4. Restore all local persistent directories, or confirm that object storage has been switched back to the corresponding version.
5. Start AsterDrive.
6. Run `doctor` validation.

Run at least once:

```bash
./aster_drive doctor \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc"
```

If you also restored local object directories or object storage, continue with a deep check:

```bash
./aster_drive doctor \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --deep
```

`--deep` focuses on finding these classes of issues:

- `storage-usage`: database usage counters do not match actual file usage.
- `blob-ref-counts`: blob reference counts have drifted.
- `storage-objects`: missing objects, untracked objects, and orphan thumbnails.
- `folder-tree`: abnormal folder structure.

## One Last Thing

Do not only verify that backups can be created. Regularly rehearse that they can be restored.

A more practical routine is:

- define a fixed backup frequency
- keep at least one offline copy
- regularly restore backups into a test environment
- run `doctor` after restore
- then use a real account to log in, upload, download, share, and restore an item from trash as spot checks
