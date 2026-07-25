# Jemalloc Heap Profiling

This document records the maintainer workflow for diagnosing Rust heap growth in AsterDrive with jemalloc profiling. It is intended for development and incident investigation, not as a default production setting.

## When to use it

Use this workflow when:

- `Private_Dirty` / `Anonymous` keeps growing during a soak test
- allocator metrics exposed through `/health` grow with traffic and do not settle
- RSS stays high after load stops and you need to separate Rust heap, allocator retention, mmap, thread stacks, and other sources
- you need the call stacks that hold heap memory instead of only aggregate `/proc` numbers

If `Private_Dirty` / `Anonymous` stays mostly flat over 1 to 2 hours, do not enable heap profiling first. jemalloc profiling adds overhead and continuously writes `.heap` files.

## Existing wiring

The repository already has the required wiring:

- `Cargo.toml` defines `jemalloc`, `jemalloc-stats`, and `jemalloc-profiling`
- `jemalloc-profiling` enables `tikv-jemallocator/profiling` and includes `jemalloc-stats`
- `src/main.rs` installs `tikv_jemallocator::Jemalloc` as the global allocator under the `jemalloc` feature
- `src/main.rs` also embeds low-memory defaults through `_rjem_malloc_conf`: `narenas:1`, `dirty_decay_ms:1000`, `muzzy_decay_ms:1000`, `background_thread:true`
- `aster_forge_alloc::stats()` reads `stats::allocated` and `stats::resident` under `jemalloc-stats`

This means the first investigation usually does not need code changes. Build a profiling binary and enable profile dumps at runtime.

## Build the profiling binary

Use the repository's existing `profiling` profile. It is close to release mode, but keeps symbols and disables LTO so `jeprof` can unwind stacks more reliably.

```bash
RUSTFLAGS="-C force-frame-pointers=yes" \
cargo build --profile profiling --features jemalloc-profiling
```

The binary is written to:

```bash
target/profiling/aster_drive
```

For production investigation, give the binary a separate name instead of replacing the stable binary:

```bash
cp target/profiling/aster_drive ./aster_drive-jprof
```

If you copy heap profiles back to a development machine, keep the matching binary too. `jeprof` needs the same binary that generated the profile for symbolization.

## Enable profiling at startup

Create a dump directory first:

```bash
sudo mkdir -p /var/log/asterdrive/jemalloc
sudo chown -R asterdrive:asterdrive /var/log/asterdrive/jemalloc
```

AsterDrive currently uses the default prefixed `tikv-jemallocator` build, so the runtime configuration environment variable is normally `_RJEM_MALLOC_CONF`, not plain `MALLOC_CONF`. If you are not sure whether a deployment package was built unprefixed, set both variables to the same value.

Recommended low-overhead starting point:

```bash
export ASTER_JEMALLOC_PROF='prof:true,prof_active:true,prof_prefix:/var/log/asterdrive/jemalloc/asterdrive,lg_prof_sample:19,lg_prof_interval:26,prof_final:true'

_RJEM_MALLOC_CONF="$ASTER_JEMALLOC_PROF" \
MALLOC_CONF="$ASTER_JEMALLOC_PROF" \
./aster_drive-jprof serve
```

Option meanings:

- `prof:true`: enables heap profiling. The binary must be built with `--features jemalloc-profiling`.
- `prof_active:true`: starts sampling immediately after process startup.
- `prof_prefix:...`: sets the heap dump file prefix.
- `lg_prof_sample:19`: samples roughly every `2^19 = 512 KiB` of allocation. Start here for a service using tens of MiB.
- `lg_prof_interval:26`: dumps a profile after roughly `2^26 = 64 MiB` of cumulative allocation. This produces samples sooner on low-traffic services than `28`.
- `prof_final:true`: writes a final profile when the process exits.

If samples are too sparse, use:

```text
lg_prof_sample:18
```

That changes the sampling interval to roughly `256 KiB`, which is more detailed and more expensive.

If too many dump files are produced, increase the interval:

```text
lg_prof_interval:28
```

That changes the dump interval to roughly `256 MiB` of cumulative allocation.

## systemd setup

For a systemd-managed service, create an override:

```bash
sudo systemctl edit asterdrive
```

Add:

```ini
[Service]
Environment="ASTER_JEMALLOC_PROF=prof:true,prof_active:true,prof_prefix:/var/log/asterdrive/jemalloc/asterdrive,lg_prof_sample:19,lg_prof_interval:26,prof_final:true"
Environment="_RJEM_MALLOC_CONF=prof:true,prof_active:true,prof_prefix:/var/log/asterdrive/jemalloc/asterdrive,lg_prof_sample:19,lg_prof_interval:26,prof_final:true"
Environment="MALLOC_CONF=prof:true,prof_active:true,prof_prefix:/var/log/asterdrive/jemalloc/asterdrive,lg_prof_sample:19,lg_prof_interval:26,prof_final:true"
```

Then reload and restart:

```bash
sudo mkdir -p /var/log/asterdrive/jemalloc
sudo chown -R asterdrive:asterdrive /var/log/asterdrive/jemalloc
sudo systemctl daemon-reload
sudo systemctl restart asterdrive
```

Verify startup:

```bash
sudo journalctl -u asterdrive -n 200 --no-pager
sudo ls -lh /var/log/asterdrive/jemalloc
```

If the journal contains `<jemalloc>:` unknown option or malformed conf messages, fix the configuration string first. Do not include spaces in the option string.

## Recommended sampling workflow

For suspected leaks, use this sequence:

1. Start the service with the profiling binary.
2. Wait 3 to 5 minutes after startup while the service is idle.
3. Keep the earliest `.heap` file as the baseline.
4. Run real traffic, a k6 soak, or let the service run for 12 to 24 hours.
5. Stop the service so `prof_final:true` writes the final profile.
6. Use `jeprof --base baseline final` to inspect net growth call stacks.

The diff is the important part. A single profile tells you who currently holds heap memory; a diff tells you who grew during the observation window.

## Analyze with jeprof

Check that `jeprof` is available:

```bash
which jeprof
```

If it is missing, install the jemalloc tools package or copy `jeprof` from the matching jemalloc build. Distribution package names differ; on Debian / Ubuntu it may be `jemalloc-bin`.

Inspect one profile:

```bash
jeprof --text ./aster_drive-jprof \
  /var/log/asterdrive/jemalloc/asterdrive.<pid>.<seq>.heap \
  | head -80
```

Compare a baseline profile and a final profile:

```bash
jeprof --text \
  --base /var/log/asterdrive/jemalloc/asterdrive.<pid>.0001.i0001.heap \
  ./aster_drive-jprof \
  /var/log/asterdrive/jemalloc/asterdrive.<pid>.0010.f.heap \
  | head -80
```

Export an SVG if needed:

```bash
jeprof --svg ./aster_drive-jprof \
  /var/log/asterdrive/jemalloc/asterdrive.<pid>.<seq>.heap \
  > /tmp/asterdrive-heap.svg
```

## Interpreting results

Common cases:

- `/proc` is stable and `jeprof --base` shows no meaningful net growth: this does not look like a leak.
- `/proc` keeps growing and `jeprof --base` points to specific Rust stacks: inspect the related business cache, task queue, buffer, connection pool, or retained object path.
- `/proc` grows but `jeprof` does not show meaningful growth: check non-Rust-heap sources such as mmap, file mappings, thread stacks, C library allocations, or kernel page-cache accounting.
- `allocated` drops but `resident` does not drop immediately: this can be allocator retention or decay behavior, not necessarily a leak.
- `Pss_File` changes while `Anonymous` does not grow: this is usually not the main Rust heap leak signal.

## Troubleshooting checklist

If no `.heap` files are produced, check:

1. Is the running binary really built with `cargo build --profile profiling --features jemalloc-profiling`?
2. Did systemd or the shell actually pass `_RJEM_MALLOC_CONF` to the process?
3. Does the `prof_prefix` directory exist, and can the service user write to it?
4. Is `lg_prof_interval` too large for the current low-traffic workload?
5. Does journal / stderr contain `<jemalloc>:` configuration errors?
6. Did the option string include spaces, for example `prof:true, prof_active:true`? Do not do that.

If stacks only show addresses or poor symbols, check:

1. Are you analyzing the `.heap` file with the same `aster_drive-jprof` binary?
2. Did the binary come from `profile.profiling` instead of a stripped release build?
3. Was it built with `RUSTFLAGS="-C force-frame-pointers=yes"`?
4. Are you analyzing on another machine where paths, debug info, or build IDs no longer match?

## Future improvement

If precise dump timing becomes necessary, add an internal diagnostics entry point gated behind `jemalloc-profiling`, for example:

```text
POST /api/v1/admin/diagnostics/jemalloc/dump
```

or a CLI command:

```bash
aster_drive diagnostics jemalloc-dump
```

The implementation should call `mallctl("prof.dump")`. This capability must be feature-gated and restricted to administrators or local operations. Do not expose it as a default public route, because it is a disk-write and performance-pressure switch.
