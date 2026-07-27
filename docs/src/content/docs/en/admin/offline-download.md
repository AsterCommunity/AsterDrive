---
description: "AsterDrive offline download (link import) administration: engine registry, runtime settings, temp directory semantics, aria2 integration steps, security boundaries, and common failure handling."
title: "Offline Download (Link Import)"
---

:::tip[What this page covers]
Link import configuration from the administrator's perspective: download engines, limits, temp directories, and aria2 integration.
For how users submit link-import tasks, see [Upload and Download](/en/using/upload-download/).
:::

Offline download is the "import from link" capability on the files page: a user submits an HTTP/HTTPS download URL, AsterDrive creates a server-side background task, downloads the file to a temp directory, verifies it, and imports it into a personal or team workspace.

:::tip[Do not confuse the names]
The UI usually says "link import", while config keys and task types say `offline_download`. This page treats them as the same feature.
:::

## Security Boundaries

AsterDrive applies basic protections before dispatching a task:

- Only HTTP/HTTPS URLs are accepted
- HTTP redirects are not followed; if the source responds with a redirect, use the final real download URL instead
- Hosts resolving to loopback, private networks, link-local, multicast, documentation-reserved ranges, or cloud metadata addresses are rejected
- The built-in downloader streams to a temp file and never reads the whole file into memory first
- SHA-256 verification and workspace import happen only after the download completes

With aria2 enabled, AsterDrive still performs these URL checks itself, but actual DNS resolution and outbound connections are made by the aria2 daemon. In production, isolate aria2 at the network layer and restrict the JSON-RPC endpoint to AsterDrive only.

## Engine Registry

Offline download engines are controlled by `offline_download_engine_registry_json`. It is an ordered registry currently supporting:

- `builtin`: the AsterDrive built-in downloader
- `aria2`: an administrator-maintained aria2 JSON-RPC downloader

The default registry enables `builtin` and disables `aria2`. With multiple engines enabled, tasks are tried in registry order; if an earlier engine fails, the next enabled one is attempted. With everything disabled, new link-import tasks are rejected — usable as an explicit off switch during maintenance windows.

Typical configuration:

```json
{
  "version": 1,
  "engines": [
    {
      "kind": "aria2",
      "enabled": true
    },
    {
      "kind": "builtin",
      "enabled": true
    }
  ]
}
```

Task details show which downloader actually completed the task; the aria2 engine also writes the GID into the internal `runtime_json` for diagnostics and recovery boundaries.

## Runtime Settings

Adjustable under `Admin -> System Settings -> File Processing -> Link Import`:

| Setting | Default | Description |
| --- | --- | --- |
| Link import engine registry | `builtin` enabled, `aria2` disabled | Which downloaders are enabled and their fallback order |
| Link import file size limit | `1 GiB` | Maximum source file size the server will download |
| Link import speed limit | `5` MB/s | Maximum average speed per task; `0` means unlimited |
| Link import concurrency limit | `1` | How many link-import tasks may run at once |
| Link import request timeout | `600` seconds | How long a complete download may take |
| Offline download temp directory | Empty, uses the service default temp dir | Optional absolute path; AsterDrive and the external downloader must both access the same path |
| aria2 RPC address | Empty | Used only when the aria2 engine is enabled |
| aria2 RPC secret | Empty | Sensitive config; redacted on read |
| aria2 RPC request timeout | `10` seconds | Per JSON-RPC call timeout; does not replace the full download timeout |
| aria2 split | `5` | `split` passed to aria2 per task |
| aria2 connections per server | `5` | `max-connection-per-server` passed to aria2 per task |
| aria2 lowest speed threshold | `0` | `lowest-speed-limit` passed to aria2; `0` disables it |

AsterDrive does not pass arbitrary aria2 options through — only the administrator-controlled safe subset above. The link import speed limit maps to aria2's per-task `max-download-limit`, not a daemon-wide throttle.

## Temp Directory Semantics

`offline_download_temp_dir` is the staging root for offline download. Empty means the service default temp directory; when set, it must be an absolute path.

AsterDrive creates `tasks/{task_id}/{processing_token}/source` under this directory. The built-in downloader writes this file itself; the aria2 engine sends the same directory and filename to aria2 over JSON-RPC. So this path is not a "host path mapping table" — it is one path string both AsterDrive and aria2 must see identically.

When deploying, ensure:

- The AsterDrive process has read, write, and execute permission on the directory
- The external downloader (e.g. aria2) can also access and write the same absolute path
- The directory holds only task temp artifacts and can be excluded from backups

For all-Docker deployments, set it to `/data/.tmp/offline-download` and mount the same host `./data` into both AsterDrive's and aria2's `/data`. For the mixed mode of `cargo run` on the host plus Compose aria2, set it to a host absolute path like `/srv/asterdrive/offline-download-temp` and mount the aria2 container at the same in-container absolute path:

```yaml
volumes:
  - ./data/offline-download-temp:/srv/asterdrive/offline-download-temp
```

## Enabling aria2

In a Docker deployment, start aria2 with the `aria2` profile from the repository root:

```bash
mkdir -p ./data ./aria2-config
sudo chown -R 10001:10001 ./data ./aria2-config

export ASTERDRIVE_ARIA2_RPC_SECRET="$(openssl rand -hex 24)"
docker compose --profile aria2 up -d
```

For all-Docker deployments, AsterDrive and aria2 must mount the same host `./data` at the same in-container `/data` path, because AsterDrive passes task temp file paths to aria2. Setting `offline_download_temp_dir` to `/data/.tmp/offline-download` is recommended.

Then configure in system settings:

| Scenario | `offline_download_aria2_rpc_url` | `offline_download_temp_dir` |
| --- | --- | --- |
| AsterDrive and aria2 both in the Compose network | `http://aria2:6800/jsonrpc` | `/data/.tmp/offline-download` |
| AsterDrive via `cargo run` on the host, aria2 via Compose in a container | `http://127.0.0.1:6800/jsonrpc` | The same host absolute path visible to both sides |

Set `offline_download_aria2_rpc_secret` to the value of `ASTERDRIVE_ARIA2_RPC_SECRET`. Before saving, click "Test aria2" in the link import engine registry; the server will call `aria2.getVersion` with the current draft to verify the RPC address, secret, and connectivity.

:::caution[Do not expose aria2 RPC publicly]
In production, never expose aria2's port `6800` to the public internet. If the host-running AsterDrive does not need it, do not publish it on the host either.
:::

## Common Problems

| Symptom | Common cause | Fix |
| --- | --- | --- |
| "Test aria2" passes, but real tasks fail with `Permission denied` or `Failed to make the directory ...` in logs | RPC works, but aria2 cannot write the task temp directory AsterDrive passed | Set `offline_download_temp_dir` to the same absolute path both sides can access. All-Docker: `/data/.tmp/offline-download`; host `cargo run` + Compose aria2: mount the host absolute path into the container at the same path |
| "Test aria2" authentication fails | `offline_download_aria2_rpc_secret` and aria2's `RPC_SECRET` differ | Regenerate `ASTERDRIVE_ARIA2_RPC_SECRET`, restart aria2, and save the same secret in system settings |
| Sensitive input shows empty | Sensitive config is redacted on read; the frontend does not refill `***REDACTED***` | Leave empty to keep unchanged; type the new secret to change it |
| Cannot connect to `http://aria2:6800/jsonrpc` | The service name `aria2` only resolves when AsterDrive is also in the Compose network | All-Docker: `http://aria2:6800/jsonrpc`; AsterDrive on the host: `http://127.0.0.1:6800/jsonrpc`, and confirm Compose publishes `6800:6800` |
| Task still succeeds after aria2 fails | The registry has `builtin` as fallback after aria2 | Expected behavior. Check the task detail name, result, and logs for the engine that actually ran; temporarily disable `builtin` if you only want to surface aria2 problems |
| Logs show `Active Download not found for GID...` | Cleanup found aria2 no longer has this GID | Usually not the root cause; focus on the first aria2 failure log above it |
| Cannot create tasks after disabling all engines | All-disabled means link import is explicitly off | Re-enable at least one engine |

## Related Pages

- [Upload and Download: Import from Link](/en/using/upload-download/)
- [System Settings](/en/reference/config/runtime/)
- [Docker Deployment](/en/deploy/docker/)
- [Operations CLI](/en/ops/cli/)
