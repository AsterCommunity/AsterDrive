# Performance Benchmarks

Issue `#120` uses `k6` as the primary benchmark runner.

## What This Covers

- `auth-login.js`: login endpoint throughput
- `auth-refresh.js`: refresh endpoint concurrency
- `folder-list.js`: folder listing latency for `100` / `1000` / `10000` file directories
- `search.js`: search latency against the seeded corpus
- `download.js`: authenticated file download throughput
- `download-range.js`: authenticated repeated ranged download throughput
- `upload-direct.js`: direct multipart upload throughput
- `upload-chunked.js`: chunked upload throughput
- `batch-move.js`: concurrent batch move operations
- `webdav-rw.js`: WebDAV concurrent read/write flow
- `webdav-concurrent-read.js`: concurrent full-file WebDAV GET throughput
- `webdav-range-read.js`: repeated WebDAV ranged GET throughput
- `webdav-propfind-large.js`: WebDAV `PROPFIND Depth: 1` over a seeded directory
- `mixed-ramp.js`: staged mixed workload ramp for latency / error curve observation
- `mixed-background-archive-download.js`: foreground REST downloads while archive compression tasks are dispatched
- `mixed-background-thumbnail-webdav.js`: foreground WebDAV reads while thumbnail tasks are dispatched
- `mixed-background-storage-migration-upload.js`: foreground direct uploads while a storage policy migration runs
- `mixed-background-rest-webdav.js`: mixed REST download/upload and WebDAV reads while archive and thumbnail tasks are dispatched
- `soak-mixed.js`: long-running mixed workload for memory / pool observation

## Prerequisites

1. Start AsterDrive in a local or staging environment.
2. Make sure the API is reachable.
3. Install `k6`.
4. Seed benchmark data once.

## Environment Variables

These defaults are shared by `seed.mjs` and the k6 scripts:

```bash
export ASTER_BENCH_BASE_URL="http://127.0.0.1:3000"
export ASTER_BENCH_USERNAME="bench_user"
export ASTER_BENCH_PASSWORD="bench-pass-1234"
export ASTER_BENCH_EMAIL="bench_user@example.com"
export ASTER_BENCH_SEARCH_TERM="needle"
export ASTER_BENCH_WEBDAV_USERNAME="bench_webdav"
export ASTER_BENCH_WEBDAV_PASSWORD="bench_webdav_pass123"
export ASTER_BENCH_WEBDAV_LIST_FOLDER="bench-webdav-list"
export ASTER_BENCH_WEBDAV_RANGE_FILE="webdav-range-5mb.bin"
export ASTER_BENCH_ARCHIVE_SOURCE_FOLDER="bench-list-10000"
export ASTER_BENCH_ARCHIVE_TARGET_FOLDER="bench-archive-output"
export ASTER_BENCH_THUMBNAIL_FOLDER="bench-thumbnail"
```

## Seed Data

Seed root folders and fixtures:

```bash
bun tests/performance/seed.mjs
```

Useful seed knobs:

```bash
ASTER_BENCH_LIST_SIZES=100,1000,10000
ASTER_BENCH_SEED_UPLOAD_CONCURRENCY=16
ASTER_BENCH_DOWNLOAD_BYTES=5242880
ASTER_BENCH_WEBDAV_LIST_SIZE=1000
ASTER_BENCH_WEBDAV_RANGE_FILE_BYTES=5242880
ASTER_BENCH_THUMBNAIL_IMAGE_COUNT=128
```

The seed step creates:

- `bench-list-100`
- `bench-list-1000`
- `bench-list-10000`
- `bench-download`
- `bench-batch-target`
- `bench-webdav`
- `bench-webdav/bench-webdav-list`
- `bench-webdav/webdav-range-5mb.bin`
- a reusable WebDAV account
- `bench-thumbnail` with distinct BMP fixtures for thumbnail task dispatch

## Local Benchmark Commands

Login:

```bash
k6 run tests/performance/k6/auth-login.js
```

Refresh:

```bash
k6 run tests/performance/k6/auth-refresh.js
```

Folder list:

```bash
ASTER_BENCH_LIST_SIZE=100 k6 run tests/performance/k6/folder-list.js
ASTER_BENCH_LIST_SIZE=1000 k6 run tests/performance/k6/folder-list.js
ASTER_BENCH_LIST_SIZE=10000 k6 run tests/performance/k6/folder-list.js
```

Search:

```bash
k6 run tests/performance/k6/search.js
```

Download:

```bash
k6 run tests/performance/k6/download.js
ASTER_BENCH_RANGE_BYTES=262144 \
k6 run tests/performance/k6/download-range.js
```

Direct upload:

```bash
k6 run tests/performance/k6/upload-direct.js
```

Chunked upload:

```bash
k6 run tests/performance/k6/upload-chunked.js
```

Batch move:

```bash
k6 run tests/performance/k6/batch-move.js
```

WebDAV read/write:

```bash
k6 run tests/performance/k6/webdav-rw.js
k6 run tests/performance/k6/webdav-concurrent-read.js
ASTER_BENCH_RANGE_BYTES=262144 \
k6 run tests/performance/k6/webdav-range-read.js
ASTER_BENCH_WEBDAV_LIST_SIZE=10000 \
k6 run tests/performance/k6/webdav-propfind-large.js
```

Mixed ramp:

```bash
ASTER_BENCH_MIXED_RAMP_STAGES=1:20s,8:30s,32:30s,64:45s,0:15s \
k6 run tests/performance/k6/mixed-ramp.js
```

Stage format is `target_vus:duration`, for example `32:30s`.

Mixed foreground/background:

```bash
k6 run tests/performance/k6/mixed-background-archive-download.js
k6 run tests/performance/k6/mixed-background-thumbnail-webdav.js
k6 run tests/performance/k6/mixed-background-rest-webdav.js
```

Storage migration mixed load needs explicit source and target policy IDs:

```bash
ASTER_BENCH_STORAGE_MIGRATION_SOURCE_POLICY_ID=1 \
ASTER_BENCH_STORAGE_MIGRATION_TARGET_POLICY_ID=2 \
k6 run tests/performance/k6/mixed-background-storage-migration-upload.js
```

The benchmark user must be an admin for mixed background scripts because they
sample `/api/v1/admin/tasks` backlog totals.

Long soak:

```bash
ASTER_BENCH_SOAK_DURATION=24h \
ASTER_BENCH_SUMMARY_DIR=tests/performance/results \
k6 run tests/performance/k6/soak-mixed.js
```

All k6 scripts include `summaryTrendStats` for `p(99)` and `p(99.9)`, and
the compact JSON summary exposes them as `p99` and `p999`.

## Rust Microbenchmarks

Rust benchmarks cover isolated internal hotspots. They are not a replacement
for k6 service-level latency tests, but they are useful for catching regressions
in path and naming helpers used by file, upload, and WebDAV flows.

```bash
cargo bench --bench path_hotspots
```

## Collecting Summaries

If `ASTER_BENCH_SUMMARY_DIR` is set, each script writes a compact JSON summary:

```bash
mkdir -p tests/performance/results/local
ASTER_BENCH_SUMMARY_DIR=tests/performance/results/local \
k6 run tests/performance/k6/download.js
```

Data-plane scripts now emit byte counters in the compact summary, so you can derive effective throughput instead of staring at request latency alone:

- `download.js` → `aster_download_bytes`
- `download-range.js` → `aster_download_range_bytes`
- `upload-direct.js` → `aster_upload_direct_bytes`
- `upload-chunked.js` → `aster_upload_chunked_bytes`
- `webdav-rw.js` → `aster_webdav_put_bytes`, `aster_webdav_get_bytes`
- `webdav-concurrent-read.js` → `aster_webdav_read_bytes`
- `webdav-range-read.js` → `aster_webdav_range_bytes`
- `webdav-propfind-large.js` → `aster_webdav_propfind_response_bytes`
- `mixed-ramp.js` → `aster_mixed_ramp_bytes`
- `mixed-background-archive-download.js` → `aster_mixed_archive_download_bytes`, `aster_mixed_archive_task_backlog`
- `mixed-background-thumbnail-webdav.js` → `aster_mixed_thumbnail_webdav_read_bytes`, `aster_mixed_thumbnail_task_backlog`
- `mixed-background-storage-migration-upload.js` → `aster_mixed_storage_migration_upload_bytes`, `aster_mixed_storage_migration_task_backlog`
- `mixed-background-rest-webdav.js` → `aster_mixed_bg_foreground_bytes`, `aster_mixed_bg_task_backlog`

## Object Storage and Remote Follower Runs

The k6 scripts are storage-backend agnostic. To compare local, S3-compatible,
Azure, OneDrive, or remote follower reads, start AsterDrive with the target
storage policy as the default upload policy, run `bun tests/performance/seed.mjs`,
then run the same scripts against that environment.

For object-storage and remote-node regressions, capture at least:

- `download.js` and `download-range.js` for REST full and ranged reads.
- `webdav-concurrent-read.js` and `webdav-range-read.js` for WebDAV read paths.
- `webdav-propfind-large.js` when directory metadata latency is part of the risk.
- `http_req_duration`, script-specific p95/p99 metrics, byte counters, and error rate.
- `/health/metrics` storage-driver and DB metrics when the server is built with
  the `metrics` feature.

Store comparable before/after summaries under `tests/performance/results/<run-name>`:

```bash
mkdir -p tests/performance/results/s3-before
ASTER_BENCH_SUMMARY_DIR=tests/performance/results/s3-before \
k6 run tests/performance/k6/download-range.js
```

## Soak-Test Observation

`soak-mixed.js` only drives workload. Pair it with runtime monitoring:

- local process: `scripts/test.sh` or system tools such as `ps`, `vm_stat`, `top`
- container runtime: `scripts/monitor.sh`
- optional metrics endpoint: run the server with the `metrics` feature and scrape `/health/metrics`

Recommended soak checklist:

1. Run `soak-mixed.js` for `24h`.
2. Sample RSS / heap / CPU every `30s` to `60s`.
3. Watch p95 latency drift in the k6 summary.
4. Watch DB pool exhaustion, request retries, and cleanup backlog in logs.
