---
description: AsterDrive offline download configuration, covering link import, the built-in downloader, aria2, Docker and local development setups, troubleshooting, and security boundaries.
title: "Offline Download (Link Import)"
---

Offline download is the "Import from link" feature in the file page. A user submits an HTTP/HTTPS download URL, and AsterDrive creates a background task that downloads the file on the server, verifies it, and imports it into a personal or team workspace.

:::tip[Naming]
The UI usually says "link import", while configuration keys and task types use `offline_download`. This page treats them as the same feature.
:::

## User-Facing Behavior

When creating a link-import task, users can provide:

- Source URL: must be `http://` or `https://`
- Filename: optional; if omitted, AsterDrive prefers the response header or URL path
- Target folder: defaults to the current folder
- Expected SHA-256: optional; if set, the final file hash is verified and the task fails on mismatch

Creating the task does not block the page. Users track queued, downloading, verifying, and importing progress in the current workspace's `Task Center`; tasks created in a team space only appear in that team's task center.

## Security Boundary

AsterDrive applies baseline protections before dispatching a task:

- Only HTTP/HTTPS URLs are accepted
- HTTP redirects are not followed; use the final direct download URL if the source returns a redirect
- Hosts resolving to loopback, private, link-local, multicast, documentation, or cloud metadata ranges are rejected
- The built-in downloader streams into a temporary file and does not buffer the whole file in memory
- SHA-256 verification and workspace import happen only after the download completes

When aria2 is enabled, AsterDrive still performs these URL checks first, but the aria2 daemon performs its own DNS resolution and outbound connection. Production deployments should isolate aria2 at the network layer and restrict the JSON-RPC endpoint to AsterDrive.

## Engine Registry

The offline-download engines are controlled by `offline_download_engine_registry_json`. It is an ordered registry that currently supports:

- `builtin`: AsterDrive's built-in downloader
- `aria2`: an administrator-managed aria2 JSON-RPC downloader

The default registry enables `builtin` and disables `aria2`. When multiple engines are enabled, tasks try them in registry order; if one engine fails, the next enabled engine is tried. If all engines are disabled, new link-import tasks are rejected, which is useful as an explicit maintenance switch.

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

Task details show the downloader that actually completed the task. The aria2 engine also stores its GID in internal `runtime_json` while running, for diagnostics and recovery boundaries.

## Runtime Settings

The following settings live under `Admin -> System Settings -> File Processing -> Link Import`:

| Setting | Default | Notes |
| --- | --- | --- |
| Link import engine registry | `builtin` enabled, `aria2` disabled | Controls enabled downloaders and fallback order |
| Link import file size limit | `1 GiB` | Maximum source file size the server may download |
| Link import download speed limit | `5` MB/s | Per-task average speed limit; `0` means unlimited |
| Link import concurrency limit | `1` | How many link-import tasks may run at once |
| Link import request timeout | `600` seconds | Overall download duration limit |
| Offline download temp directory | Empty, use the default server temp directory | Optional absolute path; AsterDrive and external downloaders must be able to access the same path |
| aria2 RPC URL | Empty | Used only when the aria2 engine is enabled |
| aria2 RPC secret | Empty | Sensitive setting; reads are redacted |
| aria2 RPC request timeout | `10` seconds | Timeout for one JSON-RPC call, not the full download timeout |
| aria2 split | `5` | Per-task aria2 `split` option |
| aria2 per-server connections | `5` | Per-task aria2 `max-connection-per-server` option |
| aria2 low-speed limit | `0` | aria2 `lowest-speed-limit`; `0` disables it |

AsterDrive does not pass through arbitrary aria2 options. It exposes only the administrator-controlled safe subset above. The link-import speed limit maps to aria2's per-task `max-download-limit`, not a daemon-wide limit.

## Temporary Directory Semantics

`offline_download_temp_dir` is the staging root for offline downloads. When it is blank, AsterDrive uses the default server temp directory. When it is set, it must be an absolute path.

AsterDrive creates `tasks/{task_id}/{processing_token}/source` under this directory. The built-in downloader writes that file directly. The aria2 engine sends the same directory and filename to aria2 through JSON-RPC. This is not a host/container path mapping table; it is the same path string that both AsterDrive and aria2 must see.

Deployment requirements:

- The AsterDrive process must have read, write, and execute access to the directory.
- External downloaders such as aria2 must also be able to access and write the same absolute path.
- The directory contains only task staging files and can be excluded from backups.

For full Docker deployments, use `/data/.tmp/offline-download` and mount the same host `./data` directory to `/data` in both AsterDrive and aria2. For mixed host `cargo run` + Compose aria2 development, use a host absolute path such as `/srv/asterdrive/offline-download-temp`, and mount the aria2 container to the same absolute path:

```yaml
volumes:
  - ./data/offline-download-temp:/srv/asterdrive/offline-download-temp
```

## Enable aria2

Docker deployments can start aria2 with the repository-root `aria2` profile:

```bash
mkdir -p ./data ./aria2-config
sudo chown -R 10001:10001 ./data ./aria2-config

export ASTERDRIVE_ARIA2_RPC_SECRET="$(openssl rand -hex 24)"
docker compose --profile aria2 up -d
```

In full Docker deployments, AsterDrive and aria2 must mount the same host `./data` directory at the same in-container `/data` path, because AsterDrive passes task temporary paths to aria2. Set `offline_download_temp_dir` to `/data/.tmp/offline-download`.

Then configure:

| Scenario | `offline_download_aria2_rpc_url` | `offline_download_temp_dir` |
| --- | --- | --- |
| AsterDrive and aria2 both run in the Compose network | `http://aria2:6800/jsonrpc` | `/data/.tmp/offline-download` |
| AsterDrive runs on the host with `cargo run`, while aria2 runs in Compose | `http://127.0.0.1:6800/jsonrpc` | The same host absolute path visible to both processes |

Set `offline_download_aria2_rpc_secret` to the value of `ASTERDRIVE_ARIA2_RPC_SECRET`. Before saving, you can use **Test aria2** in the link-import engine registry; the server calls `aria2.getVersion` using the current drafts to verify the RPC URL, secret, and reachability.

:::caution[Do not expose aria2 RPC]
Do not publish aria2 port `6800` to the public internet in production. If host-side AsterDrive does not need to reach it, do not publish it to the host either.
:::

## Troubleshooting

| Symptom | Common cause | Fix |
| --- | --- | --- |
| **Test aria2** succeeds, but a real task fails with `Permission denied` or `Failed to make the directory ...` | RPC works, but aria2 cannot write into the task temporary directory passed by AsterDrive | Set `offline_download_temp_dir` to the same absolute path visible to both sides. For full Docker deployments, use `/data/.tmp/offline-download`; for host `cargo run` + Compose aria2, mount the host absolute path into the container at that same path |
| **Test aria2** returns authentication failure | `offline_download_aria2_rpc_secret` does not match aria2 `RPC_SECRET` | Set `ASTERDRIVE_ARIA2_RPC_SECRET`, restart aria2, and save the same secret in system settings |
| The sensitive input is empty | Sensitive settings are redacted on read, and the frontend does not fill `***REDACTED***` into the input | Leaving it blank keeps the existing value; type a new secret to change it |
| Cannot reach `http://aria2:6800/jsonrpc` | The service name `aria2` resolves only when AsterDrive also runs in the Compose network | Use `http://aria2:6800/jsonrpc` for full Docker deployments. Use `http://127.0.0.1:6800/jsonrpc` when AsterDrive runs on the host, and make sure Compose publishes `6800:6800` |
| The task succeeds even though aria2 failed | `builtin` is enabled after aria2 as fallback | This is expected. Check task details, result, and logs for the engine that actually completed the task. Temporarily disable `builtin` if you want aria2 failures to surface directly |
| Logs mention `Active Download not found for GID...` | Cleanup found that aria2 no longer has that GID | This is usually not the root cause. Inspect the first aria2 failure logged before cleanup |
| Creating tasks is rejected after disabling all engines | Disabling all engines explicitly disables link import | Re-enable at least one engine |

## Related Pages

- [User Manual: Import Files From Links](/en/guide/user-guide/#import-files-from-links)
- [Runtime System Settings](/en/config/runtime/)
- [Docker Deployment](/en/deployment/docker/)
- [Operations CLI](/en/deployment/ops-cli/)
