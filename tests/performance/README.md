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

### WebDAV Hotspot Baseline

Issue `#382` recorded a local SQLite/local-filesystem baseline on 2026-07-05
where REST 256 KiB range GET was much faster than the equivalent WebDAV range
GET:

- `download-range.js`: p95 `1.85 ms`, p99 `2.29 ms`, `1.57 GB/s`
- `webdav-range-read.js`: p95 `106.82 ms`, p99 `114.51 ms`, `19.86 MB/s`
- `webdav-concurrent-read.js`: p95 `120.93 ms`, p99 `132.73 ms`, `350.76 MB/s`
- `webdav-propfind-large.js` over 10000 files: p95 `415.26 ms`, p99 `513.90 ms`

When comparing before/after runs, keep the same seed data and set
`ASTER_BENCH_SUMMARY_DIR` so each script writes a compact JSON summary. WebDAV
GET/HEAD now emits debug fields for path resolution and storage-open time, and
PROPFIND emits debug fields for metadata, listing collection, preload, and XML
rendering time. Enable debug logs for `aster_drive::webdav` while running the
k6 scripts to separate protocol overhead from storage and directory listing
work.

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

## Folder Tree Mutation Memory Boundary

Issue `#497` has a separate ignored Rust integration benchmark because its
acceptance fixtures contain 100,000, 500,000, and 1,000,000 resources. The
runner uses file-backed SQLite, creates one folder plus enough files to reach
each exact resource count, and excludes fixture construction from the measured
interval.

For both REST delete and trash restore it records:

- HTTP response and end-to-end task completion time;
- live heap baseline, peak growth, and end state;
- cumulative allocation bytes and allocation count;
- SQLite database and WAL peak growth.

The live heap peak is a concurrent sampling approximation: allocator updates can
change the live counter between its read and the peak update. Cumulative
allocation also includes any allocation performed by the 2 ms SQLite/WAL
observer, so it contains duration-dependent sampling noise. The acceptance gate
uses the cross-size live-heap ratio; cumulative allocation and database growth
remain diagnostic occupancy evidence.

The summary requires every operation to return `202 Accepted` and requires the
maximum heap peak across the three fixture sizes to stay within 1.25x of the
minimum. This checks the configured memory boundary without putting machine
specific latency or absolute byte thresholds into ordinary CI.

Run the complete acceptance matrix:

```bash
tests/performance/run-issue-497-folder-tree-memory.sh
```

Useful overrides:

```bash
ASTER_ISSUE497_RESOURCE_COUNTS=100000 \
ASTER_ISSUE497_KEEP_DATABASES=1 \
ASTER_ISSUE497_RESULT_DIR=/tmp/issue-497-smoke \
ASTER_ISSUE497_MAX_PEAK_RATIO=1.5 \
tests/performance/run-issue-497-folder-tree-memory.sh
```

The single-size override is only a smoke test for the runner and task path. Its
max/min ratio is necessarily `1.0`, so it does not validate bounded growth
across resource counts. Keep the default three sizes for issue acceptance.

The benchmark target is gated by the existing `benchmarks` feature, so ordinary
test runs do not execute or compile the large-fixture runner. Workspace
all-feature/all-target Clippy still compiles it to keep the measurement tooling
in sync with application APIs.

## WebDAV Provider Range Baselines

Issue `#449` uses a separate Rust runner for provider efficiency. It calls the
same `StorageDriver::get_stream()` / `get_range()` operations selected by the
WebDAV download adapter, without putting wall-clock assertions into ordinary
unit tests.

The fixed scenario set is:

- full GET;
- an early single range;
- a late single range;
- two disjoint ranges, with one backend open per final segment;
- the default `get_stream + prefix skip` fallback, measured against an
  instrumented in-memory fixture.

Each artifact contains raw samples plus p50/p95/p99 summaries for open time,
TTFB, read time, total time, and payload throughput. It also records selected
bytes, logical backend call count, bytes actually pulled through the returned
reader, and fallback prefix bytes. `backend_call_count` follows the existing
WebDAV observation contract: it counts `get_stream` / `get_range` opens. SDK
retries and a provider's internal redirect hop remain transport details and
must be called out in the provider fixture summary when they change.

Machine metadata distinguishes debug from optimized binaries. Baseline lookup
also matches warmup count, sample count, and read-buffer size, so results from a
quick debug smoke run cannot be compared with the optimized `cargo bench`
workflow by accident.

Provider summaries record behavior-relevant configuration such as path style,
protocol versions, host-key pinning, and whether explicit target selection is
enabled. Credentials and fixture identifiers such as endpoints, bucket names,
drive/item IDs, remote target keys, and SFTP fingerprints are deliberately
redacted from artifacts.

The fixture is explicitly bounded to at most 1 GiB. Download samples validate
the deterministic byte pattern incrementally with a 64 KiB buffer rather than
buffering a complete response or writing a temporary download.

Local run:

```bash
ASTER_BENCH_RANGE_PROVIDER=local \
ASTER_BENCH_RANGE_BASELINE_PROFILE="$(uname -s)-$(uname -m)-local-v1" \
ASTER_BENCH_RANGE_BASELINE=tests/performance/baselines/webdav-provider-range-v1.json \
cargo bench --bench webdav_provider_range --features benchmarks
```

The default artifact path is:

```text
tests/performance/results/webdav-provider-range/artifact.json
```

External fixtures are opt-in. Missing variables produce a structured `skipped`
artifact with the exact prerequisite list; set
`ASTER_BENCH_RANGE_PROVIDER_REQUIRED=true` when a selected fixture must exist.

### S3-compatible

Required:

```text
ASTER_BENCH_S3_ENDPOINT
ASTER_BENCH_S3_BUCKET
ASTER_BENCH_S3_ACCESS_KEY
ASTER_BENCH_S3_SECRET_KEY
```

Optional: `ASTER_BENCH_S3_REGION`, `ASTER_BENCH_S3_BASE_PATH`, and
`ASTER_BENCH_S3_PATH_STYLE`.

### OneDrive

Required:

```text
ASTER_BENCH_ONEDRIVE_ACCESS_TOKEN
ASTER_BENCH_ONEDRIVE_DRIVE_ID
ASTER_BENCH_ONEDRIVE_ROOT_ITEM_ID
```

Optional: `ASTER_BENCH_ONEDRIVE_GRAPH_BASE_URL` and
`ASTER_BENCH_ONEDRIVE_BASE_PATH`. Use a benchmark-only folder because the
runner overwrites and normally deletes its fixture object. The configured base
folder must already exist; the benchmark does not mutate OneDrive folder
topology just to time object reads.

### SFTP

Required:

```text
ASTER_BENCH_SFTP_ENDPOINT
ASTER_BENCH_SFTP_USERNAME
ASTER_BENCH_SFTP_PASSWORD
ASTER_BENCH_SFTP_HOST_KEY_FINGERPRINT
```

Optional: `ASTER_BENCH_SFTP_BASE_PATH`. The host key pin is deliberately
mandatory; a timing run is not an excuse to weaken the connector contract.

### Remote driver

Required:

```text
ASTER_BENCH_REMOTE_BASE_URL
ASTER_BENCH_REMOTE_ACCESS_KEY
ASTER_BENCH_REMOTE_SECRET_KEY
```

Optional: `ASTER_BENCH_REMOTE_BASE_PATH` and
`ASTER_BENCH_REMOTE_STORAGE_TARGET_KEY`. Set
`ASTER_BENCH_REMOTE_CAPABILITIES_JSON` to the follower's stored discovery
document when benchmarking an older compatible protocol revision; otherwise
the runner uses the current protocol capability model.

### Versioned baselines

Baseline profiles live in
`tests/performance/baselines/webdav-provider-range-v1.json`. A profile is
matched by profile name, provider, payload size, and range size. The default
policy reports a regression when p95 TTFB exceeds `1.5x` or p50 throughput
drops below `0.7x`; scheduled runs report the comparison in the artifact and do
not turn network variance into a protocol-test failure.

Update a profile only from a reviewed artifact captured from a clean Git
worktree on the same machine and provider fixture. The updater rejects dirty,
incomplete, empty, or non-finite artifacts before touching the baseline file:

```bash
bun tests/performance/update-webdav-provider-range-baseline.mjs \
  tests/performance/results/webdav-provider-range/artifact.json \
  tests/performance/baselines/webdav-provider-range-v1.json \
  PROFILE_NAME
```

The dedicated `WebDAV Provider Range Baselines` workflow runs local storage on
a weekly schedule and exposes manual provider selection. It always uploads the
artifact, including skip artifacts, so fixture drift is visible instead of
silently disappearing.

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
