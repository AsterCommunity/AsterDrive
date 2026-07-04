# Server Configuration

::: tip This page covers `[server]`
Listen address, port, worker count, temporary directories, and node startup mode. These decide "where the service is exposed, where temporary files land, and whether it runs as primary or follower".
Most primary deployments only need to check two things: whether `host` is `0.0.0.0`, and whether temporary directories are on a disk with enough capacity.
:::

```toml
[server]
host = "127.0.0.1"
port = 3000
workers = 0
temp_dir = ".tmp"
upload_temp_dir = ".uploads"
start_mode = "primary"

[server.follower]
remote_storage_target_local_root = "remote-storage-targets"
```

If `data/config.toml` was generated automatically, relative paths are resolved to `data/` at runtime, such as `data/.tmp`, `data/.uploads`, and `data/remote-storage-targets`.

## When to Change It

- **Container/Docker deployment** - Change `host` to `0.0.0.0`; otherwise the service cannot be reached from outside the container.
- **Port is occupied** - Change `port`.
- **The disk containing temporary directories is small** - Move `temp_dir` and `upload_temp_dir` to a larger disk.
- **Unsure about worker count** - Keep `workers = 0` and let AsterDrive choose based on CPU.
- **This instance should run as a follower node** - Change `start_mode` to `follower`, and make sure `remote_storage_target_local_root` is on a disk with suitable capacity.

## Options

| Option | Default | Purpose |
| --- | --- | --- |
| `host` | `"127.0.0.1"` | Listen address. Use `0.0.0.0` for container deployments. |
| `port` | `3000` | HTTP listen port |
| `workers` | `0` | Worker count. `0` = choose automatically based on CPU. |
| `temp_dir` | `".tmp"` | General server-side temporary file directory |
| `upload_temp_dir` | `".uploads"` | Temporary directory for chunked uploads and upload recovery |
| `start_mode` | `"primary"` | Node startup role. `primary` is the normal controller; `follower` is a remote storage follower node. |
| `follower.remote_storage_target_local_root` | `"remote-storage-targets"` | Root directory for local ingress targets managed by the primary on the follower |

## Where Temporary Directories Are Used

`temp_dir` and `upload_temp_dir` directly affect local disk usage. They are mainly consumed by:

- Large-file chunked uploads
- Upload recovery/resume
- Temporary assembly for local storage
- A few upload paths that require temporary server-side processing

::: tip Move them if you upload large files often
By default, they land in `data/.tmp` and `data/.uploads`. If you expect many large uploads, bind these two directories to a local disk with more capacity.
:::

## How to Choose `start_mode`

The default is `primary`. Normal deployments, login entry points, the admin console, sharing, WebDAV, and the user file browser are all primary responsibilities.

Change it only when you explicitly want this machine to join as a remote storage follower node:

```toml
[server]
start_mode = "follower"
```

`start_mode` is a static startup role. Restart the process after changing it.  
A follower is not a second login site. It only provides health checks and internal remote storage APIs. See [follower nodes](/en/guide/remote-nodes) for the complete enrollment flow.

## Follower Ingress Root

`[server.follower].remote_storage_target_local_root` only matters in follower mode.

When the primary creates a `local` ingress target in the follower node details, it can only enter a relative path. The follower joins that relative path under `remote_storage_target_local_root`, so the primary cannot write arbitrary host directories directly.

For example:

```toml
[server.follower]
remote_storage_target_local_root = "/data/remote-storage-targets"
```

When the primary creates the ingress target, enter:

```text
base_path = "default"
```

The final write location is:

```text
/data/remote-storage-targets/default
```

Plan this directory together with real file capacity. It is not a temporary directory; it stores real objects received by the follower.

::: tip The configuration key is under `[server.follower]`
The ingress root is now `server.follower.remote_storage_target_local_root`.

If an old configuration still uses `managed_ingress_local_root` under `[server.follower]`, it remains accepted as a compatibility alias. New configurations should use `remote_storage_target_local_root`.
:::

## Common Examples

### Local Testing

```toml
[server]
host = "127.0.0.1"
port = 3000
workers = 0
temp_dir = "data/.tmp"
upload_temp_dir = "data/.uploads"
start_mode = "primary"
```

### Docker / Container

```toml
[server]
host = "0.0.0.0"
port = 3000
workers = 0
temp_dir = "/data/.tmp"
upload_temp_dir = "/data/.uploads"
start_mode = "primary"
```

### Docker Follower

```toml
[server]
host = "0.0.0.0"
port = 3000
workers = 0
temp_dir = "/data/.tmp"
upload_temp_dir = "/data/.uploads"
start_mode = "follower"

[server.follower]
remote_storage_target_local_root = "/data/remote-storage-targets"
```

## Practical Notes

- Most deployments do not need manual `workers` tuning.
- For long-running deployments, use absolute paths for temporary directories.
- If a reverse proxy is already in front, keep the application listening on an internal port. Do not expose it directly to the public internet.

## Environment Variables

```bash
ASTER__SERVER__HOST=0.0.0.0
ASTER__SERVER__PORT=3000
ASTER__SERVER__WORKERS=0
ASTER__SERVER__TEMP_DIR=/data/.tmp
ASTER__SERVER__UPLOAD_TEMP_DIR=/data/.uploads
ASTER__SERVER__START_MODE=follower
ASTER__SERVER__FOLLOWER__REMOTE_STORAGE_TARGET_LOCAL_ROOT=/data/remote-storage-targets
```
