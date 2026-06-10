---
description: AsterDrive capacity planning reference for estimating production resources by file count, database size, object storage, memory, temporary disk, and background task concurrency.
---

# Capacity Planning

::: tip Do not look for one universal number
This page gives planning methods and conservative estimates, not a capacity limit that applies to every machine.

AsterDrive's real capacity depends on database backend, storage policy, filename length, version count, search indexes, background tasks, upload mode, and concurrent access. Before production launch, still run [Performance Benchmarking and Load Testing](/en/deployment/performance-benchmarking) in your own environment.
:::

## Separate Three Kinds of Capacity

| Capacity type | Main growth sources | Stored in | Planning focus |
| --- | --- | --- | --- |
| Metadata capacity | Users, teams, folders, file records, Blobs, versions, shares, tasks, audit logs, search indexes | Database | File count, version count, indexes, and history retention |
| Object data capacity | File content, historical version content, thumbnails, preview cache, avatars | Local disk, S3 / MinIO, remote follower | Real file bytes, version retention, trash retention, storage backend redundancy |
| Runtime capacity | Upload buffers, temporary chunks, archive staging, background tasks, database connections, cache | Process memory and temporary directories | Concurrent uploads, background task concurrency, single-file size, temporary directory free space |

The database is not the file-content store. A 10 MiB file and a 10 GiB file usually have similar metadata size in the database. Database size is mostly driven by how many records exist and how long those records are retained.

## How File Count Amplifies Metadata

A normal file usually creates:

- 1 `files` record
- 1 new or reused `file_blobs` record
- 0 to many `file_versions` records
- possible share, lock, property, thumbnail, media metadata, background task, and audit records
- under SQLite, possible filename / folder-name search acceleration index entries

A folder usually creates:

- 1 `folders` record
- possible share, lock, property, and audit records
- under SQLite, possible search acceleration index entries

For production planning, start with this conservative budget:

| Item | Low-change, few versions | Search, sharing, tasks, and a few versions | Heavy versions / audit / properties |
| --- | --- | --- | --- |
| Database budget per file or folder | `2-4 KiB` | `4-8 KiB` | `8-20 KiB+` |
| 100k files/folders | `200-400 MiB` | `400-800 MiB` | `800 MiB-2 GiB+` |
| 1M files/folders | `2-4 GiB` | `4-8 GiB` | `8-20 GiB+` |

This table is intentionally conservative. Actual size can be smaller with short names, no versions, and short audit retention. It can also be larger if you keep versions, shares, audit logs, and background task history for a long time.

## Choosing a Database Backend

| Scale / usage pattern | Recommendation |
| --- | --- |
| Personal, NAS, or small team; tens of thousands to low hundreds of thousands of files; low write concurrency | SQLite is a good starting point and easiest to back up |
| File count reaches hundreds of thousands; listing, search, and background tasks become frequent | Benchmark before staying on SQLite; prepare a PostgreSQL / MySQL migration window |
| Expected file count near or above one million; sustained multi-user uploads, search, WebDAV, and visible background task concurrency | Prefer PostgreSQL or MySQL |
| Need managed database backup, stronger operations visibility, long audit retention, or larger deployment workflows | Prefer PostgreSQL or MySQL |

SQLite is not "unusable"; it is best for single-node and light-concurrency deployments. If you already know you will have many files, many concurrent users, or long version and audit retention, do not wait until the database is already huge before planning the migration.

## Object Storage Capacity

Do not estimate object data capacity only from the currently visible file list. Estimate by lifecycle:

```text
Object data budget =
  currently visible file content
  + file content retained in trash
  + historical version content
  + thumbnail / preview / media processing cache
  + unfinished uploads and temporary objects
  + storage backend redundancy, versioning, or replication overhead
```

Common amplification points:

- Longer trash retention delays object release after deletion.
- More file versions amplify object data in editing-heavy workflows.
- If S3 / MinIO bucket versioning is enabled, the bucket may keep old versions even after AsterDrive deletes objects.
- Remote follower capacity depends on the follower's real storage target, not the primary disk.
- During storage migration, source and target policies may hold objects at the same time. Reserve near-duplicate capacity for that window.

For local storage, reserve at least:

- `30` days of normal growth
- temporary space for the largest storage migration or bulk import you expect
- upload temporary space for `data/.tmp` and `data/.uploads`
- filesystem reserve, especially for ext4 / XFS / ZFS / Btrfs snapshot setups

## Memory Capacity

AsterDrive streams most normal request paths, so memory does not grow linearly with total file size. The real memory and temporary-space pressure usually comes from:

- concurrent direct / chunked uploads
- large downloads and WebDAV client retries
- thumbnail, image preview, and media metadata tasks
- archive preview, compression, and extraction
- offline downloads
- large directory listings, search, and admin statistics
- database connection pools and object-storage HTTP connections

Conservative starting points:

| Deployment type | Suggested memory | Notes |
| --- | --- | --- |
| Personal test or small NAS | `512 MiB-1 GiB` | Enough to run features, not ideal for many concurrent background tasks |
| Family / small team | `1-2 GiB` | Better for WebDAV, thumbnails, and light concurrent uploads |
| Multi-user, S3, follower nodes, or frequent processing tasks | `4 GiB+` | Leaves room for database, cache, archive tasks, and object-storage connections |
| Large file count, soak testing, or production instance | Scale by monitoring | Watch RSS, p95 latency, task backlog, and database slow queries, not just idle memory |

Default background task concurrency is intentionally modest:

| Config | Default | Capacity impact |
| --- | --- | --- |
| `background_task_max_concurrency` | `1` | Generic fallback lane concurrency |
| `background_task_archive_max_concurrency` | `2` | Compression, extraction, and archive preview use more CPU, memory, and temporary disk |
| `background_task_thumbnail_max_concurrency` | `1` | Image / media processing concurrency |
| `background_task_storage_migration_max_concurrency` | `1` | Storage migration concurrency, mainly consuming object-storage throughput |
| `offline_download_max_concurrency` | `1` | Offline download concurrency, consuming network and temporary space |

Do not raise these values only because CPU is idle. Archive and media tasks consume temporary directories, storage migration consumes source and target throughput, and offline downloads consume network and disk.

## Temporary Disk Capacity

Temporary directories usually include:

- `data/.uploads`: chunked upload pieces and pre-assembly state
- `data/.tmp`: runtime temporary files for archives, previews, conversions, downloads, and similar tasks
- temporary directories used by offline download tooling
- follower managed ingress profile directories

Remember these default limits:

| Item | Default | Planning meaning |
| --- | --- | --- |
| `thumbnail_max_source_bytes` | `64 MiB` | Source files above this size do not generate thumbnails by default |
| `thumbnail_max_dimension` | `400 px` | Generated thumbnails keep their largest edge at or below this size by default |
| `image_preview_max_dimension` | `1600 px` | Generated image previews keep their largest edge at or below this size by default |
| `media_metadata_max_source_bytes` | `256 MiB` | Media above this size does not extract metadata by default |
| `archive_extract_max_source_bytes` | `512 MiB` | Default max source file size for one extraction task |
| `archive_extract_max_staging_bytes` | `2 GiB` | Default staging limit for one extraction task |
| `archive_extract_max_uncompressed_bytes` | `1 GiB` | Default uncompressed total limit for one extraction task |
| `archive_build_max_temp_bytes` | `2 GiB` | Default temp budget for one archive build task |
| `offline_download_max_file_size_bytes` | `1 GiB` | Default max size for one offline download file |

Estimate temporary disk with:

```text
Temporary space budget =
  total size of concurrent chunked uploads
  + staging limits of concurrent archive tasks
  + limits of concurrent offline downloads
  + preview / thumbnail / media processing temporary files
  + 20%-30% safety margin
```

If users often upload large files, prefer object-storage presigned / multipart upload for large objects to reduce pressure on the primary's local temporary directories.

## Versions, Trash, and Audit Retention

The easiest capacity to underestimate is data that is no longer visible but not yet cleaned:

- Files in trash still consume object storage.
- Historical versions may continue referencing old Blobs.
- Audit logs, background task history, share download counters, and system runtime records keep writing into the database.
- Unfinished uploads may consume temporary space or object-storage multipart state before they expire.

After launch, review these runtime settings:

- `trash_retention_days`
- `max_versions_per_file`
- `task_retention_hours`
- `audit_retention_days`
- `maintenance_cleanup_interval_secs`
- `blob_reconcile_interval_secs`

If you need long audit retention, budget audit log growth separately instead of hiding it inside file-count estimates.

## Observation and Estimation Commands

### Count Metadata Rows

Use this SQL as a first capacity inventory:

```sql
SELECT 'users' AS table_name, COUNT(*) AS rows FROM users
UNION ALL SELECT 'teams', COUNT(*) FROM teams
UNION ALL SELECT 'folders', COUNT(*) FROM folders
UNION ALL SELECT 'files', COUNT(*) FROM files
UNION ALL SELECT 'file_blobs', COUNT(*) FROM file_blobs
UNION ALL SELECT 'file_versions', COUNT(*) FROM file_versions
UNION ALL SELECT 'shares', COUNT(*) FROM shares
UNION ALL SELECT 'upload_sessions', COUNT(*) FROM upload_sessions
UNION ALL SELECT 'background_tasks', COUNT(*) FROM background_tasks
UNION ALL SELECT 'audit_logs', COUNT(*) FROM audit_logs;
```

If a table does not exist in your version, remove that row and rerun. Do not repeatedly run expensive statistics against large production tables during peak hours.

### Check Database Size

SQLite:

```bash
du -h data/asterdrive.db data/asterdrive.db-wal 2>/dev/null
sqlite3 data/asterdrive.db "PRAGMA page_count; PRAGMA page_size;"
```

PostgreSQL:

```sql
SELECT pg_size_pretty(pg_database_size(current_database())) AS database_size;

SELECT
  relname,
  pg_size_pretty(pg_total_relation_size(relid)) AS total_size
FROM pg_catalog.pg_statio_user_tables
ORDER BY pg_total_relation_size(relid) DESC
LIMIT 20;
```

MySQL:

```sql
SELECT
  table_name,
  ROUND((data_length + index_length) / 1024 / 1024, 2) AS size_mb
FROM information_schema.tables
WHERE table_schema = DATABASE()
ORDER BY data_length + index_length DESC
LIMIT 20;
```

### Check Object and Quota Consistency

Run the normal check first:

```bash
./aster_drive doctor \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc"
```

When you need object directory, Blob reference, and real storage usage checks, run deep mode:

```bash
./aster_drive doctor \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --deep
```

`--deep` is slower, especially with many objects. Do not run it casually during peak traffic.

## Expansion Triggers

When you see these signals, expand capacity or adjust policy:

| Signal | Action |
| --- | --- |
| Database file keeps growing and file count enters the hundreds of thousands | Evaluate PostgreSQL / MySQL and shorten task / audit retention |
| Directory listing or search p95 keeps rising | Run benchmarks, check indexes, SQLite FTS, and database slow queries |
| `background_tasks_pending` keeps piling up | Increase the relevant lane concurrency or reduce task generation; confirm memory and temporary space first |
| `process_memory_rss_bytes` grows one-way and does not drop when idle | Run a soak test, keep logs and monitoring samples, investigate leak or excessive concurrency |
| `data/.uploads` or `data/.tmp` often approaches disk limit | Reduce concurrency, clean failed uploads, use object-storage multipart, or enlarge temporary directories |
| Local storage free space drops below 20%-30% | Expand disk, migrate policies, shorten trash / version retention, or add S3 / follower storage |
| Storage migration dry-run reports insufficient capacity | Do not force it; expand target capacity or migrate in batches |

## A Practical Starting Point

If you need an initial decision:

1. **Single user / family**: SQLite + local storage, start with `1 GiB` memory, and back up the whole `data/` directory regularly.
2. **Small team**: SQLite can still start, but benchmark before file count approaches hundreds of thousands; `2 GiB` memory is steadier.
3. **Sustained team use or expected million-scale files**: start with PostgreSQL / MySQL, use local large disk, S3, or follower storage, and start with at least `4 GiB` memory.
4. **Frequent archive processing, previews, offline downloads, or storage migration**: plan background task and temporary directory capacity first, then decide concurrency.

Capacity planning is not a one-time document. After launch, review database size, object storage growth, RSS, background task backlog, and backup restore drills at least monthly. The dangerous part is not that a file system grows; it is not knowing why it grows.
