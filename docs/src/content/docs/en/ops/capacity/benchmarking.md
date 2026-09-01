---
description: "AsterDrive performance benchmarking and load testing: k6 script coverage, environment preparation, local runs, background task mixed load, object storage and remote node testing, and smoke baselines."
title: "Performance Benchmarking and Load Testing"
---

:::tip[What this page covers]
AsterDrive's built-in k6 load testing scripts: login, listing, search, upload and download, WebDAV, mixed traffic, and long-running stability tests.
This page **does not provide a universal capacity number**. The goal is to produce reproducible results on your own machine, database, and storage, then use them for regression comparison before and after version changes.
At the end of this page, there is an Apple M2 Pro / SQLite smoke baseline as a comparison starting point, but it **does not represent a production capacity limit**.
:::

AsterDrive's performance benchmark scripts are under `tests/performance/` in the repository.

The goal of these benchmarks is not to provide a "universal capacity number". Instead, they help you produce reproducible results on your own machine, database, and storage policy, then use those results as regression comparisons before and after upgrades.

## Benchmark Scope

Current benchmarks cover the core scenarios listed in issue `#120`:

- login and refresh concurrency
- file listing queries (`100` / `1000` / `10000` file directories)
- search queries
- concurrent file downloads
- repeated Range file downloads
- concurrent negotiated stream uploads
- concurrent chunked uploads
- batch move concurrency
- WebDAV read/write concurrency
- concurrent WebDAV GET
- repeated WebDAV Range GET
- WebDAV large-directory `PROPFIND Depth: 1`
- staged mixed workload ramp (watch latency/failure rate as concurrency increases)
- background archive compression + foreground REST download mixed load
- background thumbnail tasks + foreground WebDAV read mixed load
- background storage migration + foreground session upload mixed load
- REST download / upload + WebDAV read mixed load while background tasks are being dispatched
- long-running mixed workload soak test

## Tooling

- Main benchmark: `k6`
- Data seeding: `bun tests/performance/seed.mjs`
- Long-running observation: combine with `scripts/monitor.sh`, system process metrics, or `/health/metrics`

## Prepare the Environment

1. Start the service in a production-like way. `cargo run --profile release-performance` is recommended.
2. Point it at an independent database and independent local storage directory.
3. A new database allows local HTTP first login by default; when reusing an older benchmark database, confirm runtime `auth_cookie_secure` is disabled.
4. Install `k6`.
5. Run seed once first.

Example:

```bash
export ASTER_BENCH_BASE_URL="http://127.0.0.1:3000"
export ASTER_BENCH_USERNAME="bench_user"
export ASTER_BENCH_PASSWORD="bench-pass-1234"
export ASTER_BENCH_EMAIL="bench_user@example.com"
export ASTER_BENCH_SEARCH_TERM="needle"
export ASTER_BENCH_WEBDAV_USERNAME="bench_webdav"
export ASTER_BENCH_WEBDAV_PASSWORD="bench_webdav_pass123"
export ASTER_BENCH_WEBDAV_LIST_SIZE=1000
export ASTER_BENCH_WEBDAV_RANGE_FILE_BYTES=5242880
export ASTER_BENCH_THUMBNAIL_IMAGE_COUNT=128

bun tests/performance/seed.mjs
```

## Running Locally

Performance script details are in [`tests/performance/README.md`](https://github.com/AsterCommunity/AsterDrive/blob/master/tests/performance/README.md).

Common commands:

```bash
k6 run tests/performance/k6/auth-login.js
k6 run tests/performance/k6/auth-refresh.js

ASTER_BENCH_LIST_SIZE=100 k6 run tests/performance/k6/folder-list.js
ASTER_BENCH_LIST_SIZE=1000 k6 run tests/performance/k6/folder-list.js
ASTER_BENCH_LIST_SIZE=10000 k6 run tests/performance/k6/folder-list.js

k6 run tests/performance/k6/search.js
k6 run tests/performance/k6/download.js
ASTER_BENCH_RANGE_BYTES=262144 k6 run tests/performance/k6/download-range.js
k6 run tests/performance/k6/upload-stream.js
k6 run tests/performance/k6/upload-chunked.js
k6 run tests/performance/k6/batch-move.js
k6 run tests/performance/k6/webdav-rw.js
k6 run tests/performance/k6/webdav-concurrent-read.js
ASTER_BENCH_RANGE_BYTES=262144 k6 run tests/performance/k6/webdav-range-read.js
ASTER_BENCH_WEBDAV_LIST_SIZE=10000 k6 run tests/performance/k6/webdav-propfind-large.js
ASTER_BENCH_MIXED_RAMP_STAGES=1:20s,8:30s,32:30s,64:45s,0:15s \
k6 run tests/performance/k6/mixed-ramp.js

k6 run tests/performance/k6/mixed-background-archive-download.js
k6 run tests/performance/k6/mixed-background-thumbnail-webdav.js
k6 run tests/performance/k6/mixed-background-rest-webdav.js

ASTER_BENCH_STORAGE_MIGRATION_SOURCE_POLICY_ID=1 \
ASTER_BENCH_STORAGE_MIGRATION_TARGET_POLICY_ID=2 \
k6 run tests/performance/k6/mixed-background-storage-migration-upload.js
```

The `ASTER_BENCH_MIXED_RAMP_STAGES` format is `target_vus:duration`, for example `32:30s`.

To write results to disk:

```bash
mkdir -p tests/performance/results/local
ASTER_BENCH_SUMMARY_DIR=tests/performance/results/local \
k6 run tests/performance/k6/download.js
```

Download, Range download, upload, WebDAV, and `mixed-ramp.js` summaries now include byte counters. You can use `count` / `rate` directly to inspect effective throughput, instead of only looking at single-request latency such as `http_req_duration`, which does not fully represent effective throughput.

## Background Task Mixed Load

Background tasks are dispatched per lane, but lane concurrency does not mean foreground requests are unaffected. When load testing, run at least these four mixed scenarios:

- `mixed-background-archive-download.js`: foreground REST downloads while archive compression tasks are continuously created in the background.
- `mixed-background-thumbnail-webdav.js`: foreground WebDAV GET while image thumbnails are continuously requested, triggering thumbnail tasks.
- `mixed-background-storage-migration-upload.js`: foreground session uploads while a storage policy migration runs in the background. This script requires explicitly setting `ASTER_BENCH_STORAGE_MIGRATION_SOURCE_POLICY_ID` and `ASTER_BENCH_STORAGE_MIGRATION_TARGET_POLICY_ID`.
- `mixed-background-rest-webdav.js`: foreground REST download, REST upload, and WebDAV GET together, while archive and thumbnail tasks are dispatched in the background.

These scripts sample the `pending` / `processing` / `retry` totals from `/api/v1/admin/tasks`, so the benchmark user must have administrator privileges. Save summaries to a separate directory:

```bash
mkdir -p tests/performance/results/background-local
ASTER_BENCH_SUMMARY_DIR=tests/performance/results/background-local \
k6 run tests/performance/k6/mixed-background-rest-webdav.js
```

When judging whether background tasks interfere with the foreground path, do not only look at how fast tasks complete. Record at least:

- Foreground path p95 / p99: REST download, REST upload, WebDAV GET.
- Background task creation latency: `aster_mixed_*_task_create_duration` or thumbnail request duration.
- Queue backlog: `aster_mixed_*_task_backlog`, broken down by `pending` / `processing` / `retry` labels.
- Server metrics: `background_tasks_pending`, `background_task_retries_total`, HTTP route p95/p99, DB query p95/p99, storage driver p95/p99.
- Process RSS, CPU, network throughput, and growth of `data/.tmp` and `data/.uploads`.

If foreground p95/p99 rises noticeably as task lane concurrency goes up, or `retry` keeps growing, lower the corresponding runtime concurrency first, then locate the bottleneck among CPU, temporary directories, the database connection pool, or storage backend throughput. Do not raise background lane concurrency without mixed-load regression results.

For performance PRs, put both before/after summaries under `tests/performance/results/<run-name>/`, and paste the key scripts' p95/p99, error rate, and byte rate into the PR description. For example:

```bash
mkdir -p tests/performance/results/webdav-before
ASTER_BENCH_SUMMARY_DIR=tests/performance/results/webdav-before \
k6 run tests/performance/k6/webdav-range-read.js
```

## Object Storage and Remote Nodes

These k6 scripts do not hardcode local, S3, Azure, OneDrive, or remote follower at the script layer. To test a given backend, start the service with the corresponding storage policy, make it the default policy used when the benchmark user uploads fixtures, then rerun seed and the same set of scripts.

Suggested minimum comparison:

- local baseline: `download.js`, `download-range.js`, `webdav-concurrent-read.js`, `webdav-range-read.js`
- S3-compatible / Azure / OneDrive: the same set of scripts, focusing on range p95/p99, throughput, and error rate
- remote follower: the same set of scripts, combined with tunnel logs and network metrics on both the follower and primary sides
- WebDAV large directories: `webdav-propfind-large.js`, used to isolate `Depth: 1` metadata enumeration latency

Common bottlenecks with object storage and remote nodes are often not in the WebDAV handler itself, but in the SDK HTTP client, connection pools, object storage throttling, keepalive, remote tunnel framing, primary/follower network topology, and database metadata queries. When load testing, enable the `metrics` feature and scrape:

- HTTP route p95/p99 and 5xx ratio
- storage driver operation p95/p99, failure counts, and the difference between not_found and hard failures
- DB query p95/p99, error counts, and connection pool exhaustion logs
- process RSS, CPU, and network throughput
- in remote follower scenarios, latency and error logs on both the primary and follower sides

## SQLite Search Validation

If your deployment backend is SQLite, confirm two things before running search benchmarks:

1. `SQLite search acceleration` is `ok` in `doctor`.
2. `EXPLAIN QUERY PLAN` shows the `VIRTUAL TABLE INDEX` for `files_name_fts` / `folders_name_fts`.

Example:

```bash
./aster_drive doctor \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --output-format human

sqlite3 /var/lib/asterdrive/data/asterdrive.db "
EXPLAIN QUERY PLAN
SELECT files.id, files.name, file_blobs.size
FROM files_name_fts
JOIN files ON files_name_fts.rowid = files.id
JOIN file_blobs ON file_blobs.id = files.blob_id
WHERE files_name_fts MATCH '\"needle\"'
  AND files.deleted_at IS NULL
  AND files.user_id = 1
  AND files.team_id IS NULL
ORDER BY files.name ASC
LIMIT 50 OFFSET 0;
"
```

The important thing is not the absolute number, but planner output like:

- `SCAN files_name_fts VIRTUAL TABLE INDEX ...`
- `SEARCH files USING INTEGER PRIMARY KEY ...`

If you see ordinary full-table `SCAN` on `files` / `folders`, those results are not suitable as a search benchmark baseline. First investigate the SQLite runtime and migration state.

## Soak Test

`soak-mixed.js` only generates sustained mixed traffic. What you actually need to watch is service process RSS, CPU, heap usage, latency drift, and connection pool behavior.

Recommended combination:

```bash
ASTER_BENCH_SOAK_DURATION=24h \
ASTER_BENCH_SUMMARY_DIR=tests/performance/results/soak \
k6 run tests/performance/k6/soak-mixed.js
```

Open another terminal for observation:

```bash
./scripts/monitor.sh 30 /tmp/asterdrive-soak.csv
```

If you deploy in a container, run the script with the same name inside the container.

For long-running stability tests, focus on:

- whether p95 keeps rising between 6 and 24 hours
- whether RSS / heap only grows and does not drop
- whether logs show database connection pool exhaustion, retries, or cleanup backlog
- whether upload and download throughput degrades noticeably over time

## Manual CI Smoke

The repository includes a manual workflow that does not block normal PR / Push:

- File: `.github/workflows/performance.yml`
- Trigger: `workflow_dispatch` in GitHub Actions

It:

1. builds frontend and backend
2. starts a local release-performance service
3. runs lightweight seed
4. executes a short smoke benchmark set
5. uploads summary artifacts

This workflow only checks that "scripts still run and major paths did not regress". It is not formal capacity validation.

## Local Smoke Baseline Example

The data below is a smoke baseline from `2026-04-15` on a local development machine. It mainly provides a comparison sample for script acceptance and future version regression checks. It does not represent a production environment capacity limit and should not be directly extrapolated into deployment guidance.

Runtime environment:

- Date: `2026-04-15`
- Host: Apple M2 Pro / 32 GB / macOS 15.7.4 / `arm64`
- Binary: `target/release-performance/aster_drive`
- Database: SQLite
- Storage: local filesystem

Core results:

| Scenario | Measurement | Avg | p95 | Rate |
| --- | --- | --- | --- | --- |
| Login | `auth-login.js` | `97.27 ms` | `111.71 ms` | `61.57 req/s` |
| Folder list 100 | `folder-list.js` | `4.68 ms` | `6.28 ms` | `1216.62 req/s` |
| Folder list 1000 | `folder-list.js` | `4.96 ms` | `5.62 ms` | `1154.71 req/s` |
| Folder list 10000 | `folder-list.js` | `11.93 ms` | `13.12 ms` | `490.28 req/s` |
| Search | `search.js` | `13.24 ms` | `14.09 ms` | `445.35 req/s` |
| Download 5 MiB | `download.js` | `5.37 ms` | `6.61 ms` | `733.75 req/s` |
| Stream upload 1 MiB | `upload-stream.js` | `3.80 ms` | `9.30 ms` | `715.24 req/s` |
| Chunked upload 10 MiB | flow metric | `61.91 ms` | `74.00 ms` | single flow sample |
| Batch move 10 files | flow metric | `13.12 ms` | `21.91 ms` | single flow sample |
| WebDAV PUT 64 KiB | `webdav-rw.js` | `52.81 ms` | `65.15 ms` | single flow sample |
| WebDAV GET 64 KiB | `webdav-rw.js` | `50.60 ms` | `54.45 ms` | single flow sample |
