# WebDAV Conformance and Compatibility Testing

This document explains how to validate AsterDrive's WebDAV protocol behavior. The checks are split into three layers:

1. Rust regression tests for protocol boundaries that are explicitly defined by the project;
2. the Litmus conformance baseline for a stable and comparable set of WebDAV protocol cases;
3. real-client tests with rclone, curl, and cadaver for practical client workflows.

Each layer answers a different question. Passing unit or integration tests does not prove that every external client interoperates correctly. Passing Litmus does not cover all AsterDrive storage, quota, versioning, audit, and team-workspace semantics either.

## Current test entry points

| Layer | Entry point | Run by default | Purpose |
| --- | --- | --- | --- |
| WebDAV Rust regression | Modules such as `tests/webdav/protocol.rs` | Yes | Precisely verifies methods, status codes, headers, locks, properties, ranges, paths, and related boundaries |
| Litmus conformance baseline | `tests/webdav/litmus_compliance.rs` | No; external cases are marked `ignored` | Runs the pinned `basic`, `copymove`, `props`, `locks`, and `http` suites |
| Real-client compatibility | `tests/webdav/client_e2e.rs` | No; marked `ignored` | Exercises real workflows through pinned rclone, curl, and cadaver binaries |
| CI workflow | `.github/workflows/webdav-compatibility.yml` | Triggered by paths, schedule, or manually | Pins tool versions, runs checks, and preserves artifacts |

All WebDAV integration tests, external-client tests, Litmus tests, and their fixtures live under `tests/webdav/`. Cargo automatically discovers the single `webdav` test target through `tests/webdav/main.rs`, so `Cargo.toml` does not need a separate `[[test]]` declaration for every module.

The main implementation entry points for protocol debugging are:

- `src/webdav/protocol.rs`: `Depth`, `Destination`, `If`, ETag, and other protocol headers;
- `src/webdav/responses.rs`: HTTP status codes and XML responses;
- `src/webdav/props/`: `PROPFIND` and `PROPPATCH`;
- `src/webdav/transfer/`: `GET`, `HEAD`, and `PUT`;
- `src/webdav/resources/`: `MKCOL`, `DELETE`, `COPY`, and `MOVE`;
- `src/webdav/locks/` and `src/webdav/db_lock_system.rs`: `LOCK`, `UNLOCK`, and lock persistence;
- `src/webdav/fs/`, `src/webdav/file/`, and `src/webdav/path_resolver.rs`: filesystem adaptation and path resolution.

## Run the in-project WebDAV regression tests first

After changing WebDAV code, begin with the smallest relevant target instead of compiling an unnecessarily broad test matrix:

```bash
cargo test --test webdav protocol::<test_name> -- --nocapture
```

For example, after changing `MKCOL`:

```bash
cargo test --test webdav protocol::test_webdav_mkcol -- --nocapture
```

After a change spans multiple WebDAV modules, run the complete target:

```bash
cargo test --test webdav -- --nocapture
```

This layer is suitable for exact AsterDrive behavior and for regression coverage around failures exposed by Litmus. When fixing a conformance issue, do not merely remove a Litmus baseline entry. Add a Rust regression test that does not depend on an external binary, then update the conformance baseline.

## Pinned Litmus 0.18 baseline

The current automated check pins **Litmus 0.18**. Versions, source commits, and SHA-256 checksums for Litmus, bundled neon, and the real-client tools are recorded together in:

```text
scripts/ci/webdav-compat/versions.env
```

`tests/webdav/litmus_compliance.rs` also pins the version, groups, and expected case counts:

| Group | Expected cases |
| --- | ---: |
| `basic` | 16 |
| `copymove` | 13 |
| `props` | 33 |
| `locks` | 40 |
| `http` | 4 |

These five groups are the Litmus 0.18 default suite and the conformance gate for ordinary pull requests. Pinning the version, source commits, and checksums keeps case names, counts, output format, and known differences stable. Substituting another binary through `LITMUS_BIN` while interpreting its output as the current baseline is invalid. A baseline upgrade must update the version, case counts, parser verification, CI installation method, and known-difference file together.

The Litmus 0.18 installation also contains four suites that stay outside the ordinary pull-request gate:

- `largefile`: large-file transfer, including a resource of about 2 GiB;
- `lockbomb`: multithreaded high-volume LOCK/UNLOCK stress;
- `lockbomb-single`: single-threaded high-volume LOCK/UNLOCK stress;
- `protected`: behavior for protected metadata paths such as `.DAV`.

Run `largefile` and the lock-stress suites through scheduled or manual jobs with separate timeouts and resource limits. Define AsterDrive's product semantics for protected paths before establishing a `protected` result baseline.

## Install the pinned Litmus 0.18

The Litmus installer, client installer, and version manifest live together under `scripts/ci/webdav-compat/`. Do not place downloaded source trees or build products in the repository. The installer downloads Litmus and neon into a temporary directory, verifies SHA-256 checksums, builds the pinned commits, and installs into `WEBDAV_COMPAT_TOOLS_DIR`.

Install the macOS build dependencies first:

```bash
brew install autoconf automake pkg-config openssl@3
```

Then run this from the AsterDrive repository root:

```bash
WEBDAV_COMPAT_TOOLS_DIR="$HOME/.local/webdav-compat" \
  scripts/ci/webdav-compat/install-litmus.sh

"$HOME/.local/webdav-compat/bin/litmus" --version
```

On Linux, install `autoconf`, `automake`, a C build toolchain, `curl`, the `libexpat` development files, the OpenSSL development files, and `pkg-config`. Linux, macOS, and CI use the same installer entry point.

Ubuntu 24.04 provides Litmus 0.13 through `apt`. That package remains useful for ad hoc probing but does not satisfy the current pinned 0.18 baseline. CI therefore builds the verified source commits through the installer above instead of installing the `litmus` apt package.

Expected output:

```text
litmus 0.18
```

Use the `bin/litmus` wrapper under the installation prefix. It records the installed locations of the suite programs, `htdocs`, and bundled neon, so it still works after the test process changes its working directory.

## Run the Litmus conformance check locally

Return to the AsterDrive repository root. Set both an absolute Litmus path and an artifact directory explicitly:

```bash
mkdir -p "$PWD/artifacts/webdav-local"

LITMUS_BIN="$HOME/.local/webdav-compat/bin/litmus" \
ASTER_WEBDAV_COMPAT_ARTIFACT_DIR="$PWD/artifacts/webdav-local" \
cargo test --test webdav litmus_compliance::test_litmus_ -- \
  --ignored --skip extended_litmus:: --nocapture --test-threads=1
```

`--test-threads=1` is required. Each group starts an independent local HTTP server, creates temporary WebDAV credentials, and uses its own working directory. Serial execution keeps output and artifact ownership deterministic.

To reproduce one group, place the Rust test name before `--`:

```bash
LITMUS_BIN="$HOME/.local/webdav-compat/bin/litmus" \
ASTER_WEBDAV_COMPAT_ARTIFACT_DIR="$PWD/artifacts/webdav-local" \
cargo test --test webdav litmus_compliance::test_litmus_basic -- \
  --ignored --nocapture --test-threads=1
```

Available test names:

```text
litmus_compliance::test_litmus_basic
litmus_compliance::test_litmus_copymove
litmus_compliance::test_litmus_props
litmus_compliance::test_litmus_locks
litmus_compliance::test_litmus_http
```

The resource-intensive suites live separately in `tests/webdav/litmus/extended.rs`:

```text
litmus_compliance::extended_litmus::test_litmus_largefile
litmus_compliance::extended_litmus::test_litmus_lockbomb
litmus_compliance::extended_litmus::test_litmus_lockbomb_single
```

Run only `largefile`:

```bash
LITMUS_BIN="$HOME/.local/webdav-compat/bin/litmus" \
ASTER_WEBDAV_COMPAT_ARTIFACT_DIR="$PWD/artifacts/webdav-local" \
cargo test --test webdav \
  litmus_compliance::extended_litmus::test_litmus_largefile -- \
  --ignored --nocapture --test-threads=1
```

Use `litmus_compliance::extended_litmus::` as the filter to run all three resource-intensive suites. `largefile` transfers about 2 GiB and has a 30-minute timeout. `lockbomb` runs 20 threads with 20,000 LOCK/UNLOCK iterations per thread and has a two-hour timeout. `lockbomb-single` runs 20,000 iterations in one thread and has a one-hour timeout. Reserve sufficient temporary-storage, database, and artifact capacity before running them, and keep `--test-threads=1`.

When `LITMUS_BIN` is unset, the harness searches `PATH` for `litmus`. An explicit absolute path is still preferred because it prevents accidental use of another release.

## What the Litmus harness actually does

`tests/webdav/litmus_compliance.rs` does not require a manually started AsterDrive process. It:

1. creates an isolated test state and database through `common::setup()`;
2. creates a random user and dedicated WebDAV Basic Auth account;
3. starts a real Actix HTTP server on a random `127.0.0.1` port;
4. uses that server's `/webdav/` mount URL;
5. runs one Litmus group at a time through `TESTS=<group>`;
6. enforces a 120-second timeout per group and terminates the whole Litmus process group on timeout;
7. stops the HTTP server, parses the Litmus output, and compares it with the committed baseline;
8. writes a structured report and redacts generated usernames, passwords, and Basic Auth values from persisted logs.

This path exercises real HTTP and WebDAV Basic Auth rather than invoking an in-memory handler directly.

## Artifacts and debugging order

With `ASTER_WEBDAV_COMPAT_ARTIFACT_DIR` set, every group gets a separate directory:

```text
artifacts/webdav-local/litmus/basic/
artifacts/webdav-local/litmus/copymove/
artifacts/webdav-local/litmus/props/
artifacts/webdav-local/litmus/locks/
artifacts/webdav-local/litmus/http/
```

Important files:

| File | Contents |
| --- | --- |
| `result.json` | Structured cases, accepted differences, and evaluation errors |
| `stdout.log` | Litmus standard output |
| `stderr.log` | Litmus standard error |
| `debug.log` | neon HTTP request and response trace |
| `child.log` | Litmus child-process log when generated |

Use this order when investigating a failure:

1. inspect `result.json`, especially `errors`, `accepted_differences`, and failed case names;
2. inspect `stdout.log` for the expected and actual status reported by Litmus;
3. compare the method, URI, WebDAV headers, response status, and XML body in `debug.log`;
4. inspect `stderr.log` and `child.log` for timeouts or early process exits.

The harness redacts generated credentials, but artifacts may still contain filenames, paths, response bodies, and deployment details. Keep CI artifacts within the repository's access controls and retention policy.

## Interpret Litmus statuses

| Status | Meaning | Baseline treatment |
| --- | --- | --- |
| `pass` | The case passed | Do not add it to known differences |
| `FAIL` | Protocol behavior differs from the case expectation | Fix it first; a temporary entry requires an independent tracking issue |
| `SKIPPED` | A prerequisite is missing or an earlier step prevented execution | Track it as a difference rather than silently ignoring it |
| `WARNING` | The case completed with a compatibility warning | Track it as a difference |
| `XFAIL` | Litmus declares the case an expected failure | Do not add it to AsterDrive known differences under the current policy |

Evaluation is strict in both directions:

- a `FAIL`, `SKIPPED`, or `WARNING` missing from the baseline fails the test;
- a committed baseline entry that no longer occurs also fails the test and requests removal of the stale waiver;
- a case count that differs from the pinned version fails the test;
- disagreement between process exit status and parsed failures fails the test.

The baseline is therefore not a blanket failure waiver. It is a compatibility-debt list that must keep shrinking.

## Update the known-difference baseline

The baseline lives at:

```text
tests/webdav/fixtures/litmus-baseline.txt
```

Each record has five fields:

```text
group | FAIL|SKIPPED|WARNING | test name | independent tracking issue URL | rationale
```

Example shape:

```text
locks | FAIL | TEST_NAME | https://github.com/AsterCommunity/AsterDrive/issues/ISSUE | concise reason
```

Rules:

- every entry must reference an independent AsterDrive tracking issue; the WebDAV umbrella issue `#421` is not a substitute for a concrete defect;
- group, status, and test name must be unique as a tuple;
- the rationale must describe the currently reproducible protocol difference rather than saying only "known issue";
- remove the entry after the behavior is fixed; stale entries fail the check;
- determine whether the source is AsterDrive, the reverse proxy, or the Litmus version before adding a baseline entry.

The `props::propextended` difference exposed by the 0.18 migration was fixed in [#426](https://github.com/AsterCommunity/AsterDrive/issues/426). `PROPFIND` now ignores unknown XML elements, attributes, and complete unknown subtrees as required by RFC 4918 Section 17, while recognized controls remain namespace-aware and method-specific grammar is still enforced. XML body-size, depth, malformed-document, DTD, and entity validation runs before the semantic layer ignores an extension subtree. The in-project regression matrix covers ordering, namespace collisions, nested recognized names, unknown attributes, invalid selector combinations, and safety violations inside ignored subtrees; the pinned Litmus 0.18 `props` group now passes all 33 cases, so the stale baseline entry has been removed. The cross-handler WebDAV XML extensibility and grammar audit is tracked separately in [#427](https://github.com/AsterCommunity/AsterDrive/issues/427).

After editing the baseline, run at least:

```bash
cargo test --test webdav litmus_compliance::committed_litmus_baseline_is_well_formed
cargo test --test webdav litmus_compliance::litmus_baseline_requires_independent_tracking_issues
```

Then rerun the affected external group.

## Check a deployed WebDAV endpoint directly

Keep "validate the current checkout" separate from "validate the deployment path." The repository harness validates the current code. Direct Litmus execution additionally covers reverse-proxy, TLS, and deployment configuration:

```bash
litmus "https://HOST/webdav/" "WEBDAV_USERNAME" "WEBDAV_PASSWORD"
```

Run one group:

```bash
TESTS=locks litmus \
  "https://HOST/webdav/" \
  "WEBDAV_USERNAME" \
  "WEBDAV_PASSWORD"
```

Apply these boundaries before running it:

- use a disposable WebDAV account scoped to an isolated empty root folder;
- make sure the target parent does not already contain a business directory named `litmus`;
- Litmus creates, modifies, copies, moves, locks, and deletes test resources;
- test a direct AsterDrive endpoint first, then the public reverse-proxy endpoint, so application and proxy behavior remain distinguishable;
- keep the trailing `/` and quote the URL, username, and password;
- verify that test resources and lock records are gone after completion.

## Run real-client compatibility tests

The real-client tests cover practical workflows outside Litmus:

- rclone: listing, upload, download, sync, recursive copy and move, special filenames, and range reads;
- curl: WebDAV methods, ranges, COPY/MOVE, LOCK/UNLOCK, and response headers;
- cadaver: interactive-client directory creation, upload, download, move, and cleanup.

CI installs pinned releases through `scripts/ci/webdav-compat/install-clients.sh`. That installer targets Linux CI. Versions and SHA-256 checksums are recorded in `scripts/ci/webdav-compat/versions.env`.

Once the required clients are available locally, run:

```bash
cargo test --test webdav client_e2e:: -- \
  --ignored --nocapture --test-threads=1
```

Run one client family:

```bash
cargo test --test webdav client_e2e::webdav_rclone -- \
  --ignored --nocapture --test-threads=1

cargo test --test webdav client_e2e::webdav_curl -- \
  --ignored --nocapture --test-threads=1

cargo test --test webdav client_e2e::webdav_cadaver -- \
  --ignored --nocapture --test-threads=1
```

When local versions differ from the CI-pinned versions, use local results for fast diagnosis and the pinned CI job for the final compatibility result.

## CI behavior

`.github/workflows/webdav-compatibility.yml` contains two jobs.

### Litmus baseline

- runs for WebDAV-related paths on pull requests, `master` pushes, scheduled runs, and manual dispatch;
- builds Litmus 0.18 from pinned Litmus/neon commits and SHA-256 checksums;
- runs the five default ignored Litmus groups serially;
- preserves tool versions, the combined test log, per-group `result.json`, and request logs;
- retains artifacts for 30 days by default.

### External clients

- runs only on the schedule and through `workflow_dispatch`;
- installs pinned rclone, curl, and cadaver releases;
- runs the ignored tests in `tests/webdav/client_e2e.rs`;
- preserves tool versions and the complete client-test log.

The Litmus job provides the faster protocol baseline on pull requests. The slower real-client matrix remains scheduled and manually runnable.

## Recommended matrix by change type

| Change | Minimum check | Add before merge |
| --- | --- | --- |
| `Depth`, ETag, `If`, or `Destination` | Corresponding `test_webdav` case | Affected Litmus group |
| `PROPFIND`, `PROPPATCH`, or XML | `protocol` property regressions | `litmus_compliance::test_litmus_props`, plus rclone/cadaver when relevant |
| `MKCOL` or `DELETE` | Resource regression tests | `litmus_compliance::test_litmus_basic` |
| `COPY` or `MOVE` | Resource and conditional-request regressions | `litmus_compliance::test_litmus_copymove` plus rclone |
| `LOCK` or `UNLOCK` | Lock and `If` header regressions | `litmus_compliance::test_litmus_locks` plus curl/cadaver |
| `GET`, `HEAD`, `PUT`, or Range | Transfer regressions | `litmus_compliance::test_litmus_http` plus rclone/curl |
| Basic Auth, account scope, or cache invalidation | `protocol` and `accounts` tests | All Litmus groups plus real clients |
| Path encoding or special filenames | Path regressions | Relevant Litmus and rclone/curl/cadaver cases |
| Reverse proxy, CORS, or TLS | Application regression tests | Repeat client checks against the deployed endpoint |

## Upgrade the Litmus baseline

Treat an upgrade to a later release as an independent change:

1. read the target release's `NEWS` and identify added, removed, or renamed cases;
2. run the target Litmus directly in an isolated environment and preserve raw group output;
3. update `scripts/ci/webdav-compat/versions.env`;
4. update the version, expected case counts, and ignore messages in `tests/webdav/litmus_compliance.rs`;
5. verify that the parser still recognizes `pass`, `FAIL`, `SKIPPED`, `XFAIL`, and `WARNING`;
6. re-evaluate every known difference instead of copying the old baseline unchanged;
7. update the installation source and tool-version recording in `.github/workflows/webdav-compatibility.yml`;
8. run the complete workflow manually before merging the version bump.

`largefile`, `lockbomb`, and `lockbomb-single` now exist as ignored resource tests in `tests/webdav/litmus/extended.rs`, but remain outside the ordinary pull-request gate. `protected` still requires a defined product policy. Connecting any extra suite to CI requires separate trigger, timeout, resource-use, and baseline design. In particular, keep large-file and lock-stress tests out of the ordinary fast pull-request check.

## Failure-location quick reference

| Litmus group or symptom | Inspect first |
| --- | --- |
| `basic` MKCOL/DELETE/PUT | `resources/`, `fs/`, `path_resolver.rs` |
| `copymove` Depth/Overwrite/Destination | `resources/`, `protocol.rs` |
| `props` 207/XML/namespace | `props/`, `responses.rs` |
| `locks` token/owner/depth | `locks/`, `db_lock_system.rs`, `protocol.rs` |
| `http` Expect/Range/connection behavior | `transfer/`, `responses.rs` |
| Direct endpoint passes, proxy endpoint fails | Proxy method allowlist, header forwarding, body limits, and TLS |
| Litmus passes, real client fails | Client-specific probing order, path encoding, retries, and sync semantics |
| Local run passes, CI fails | Pinned tool versions, architecture, environment variables, and artifacts |

Do not stop at a green process exit. A useful conformance result includes the pinned tool versions, explicit suite groups, structured results, request traces, and an independent tracking issue for every retained difference.
