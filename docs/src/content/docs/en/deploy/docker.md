---
description: Single-instance Docker deployment scenario for AsterDrive, covering image user and directory permissions, first run, long-term config.toml management, Compose examples, aria2 link import, launch acceptance, and upgrade entry points.
title: "Single-Instance Docker Deployment"
---

:::tip[Who this scenario is for]
NAS, single-machine, small-team, or existing container-orchestrated single-instance deployments. You can get it running in 10 minutes.
For **production launch**, put a reverse proxy in front to handle HTTPS. Do not expose port `3000` directly to the public internet.
:::

:::note[When not to use this scenario]

- Need multiple Primaries on a shared data plane -> [Load Balancing and Multi-Instance Deployments](/en/deploy/multi-instance/)
- Orchestrating with Kubernetes -> [Kubernetes Deployment](/en/deploy/kubernetes/)
- Want this machine to run as a remote storage node -> [Docker Follower Node Deployment](/en/deploy/follower-node/)

:::

## Prerequisites

- Docker or a compatible container runtime
- A data directory you can keep long term (bind mount recommended; backups and migration are clearer)
- For production launch: a resolved domain name and a reverse proxy (see [Reverse Proxy](/en/deploy/reverse-proxy/))

The official image runs as a **non-root user** by default (UID/GID fixed to `10001:10001`, username `aster`) and includes a `HEALTHCHECK` based on `/health/ready`.

### Choose the full or slim image

The default tags continue to publish the full image with `vips`, `ffmpeg`, and `ffprobe`. Use it for HEIC/AVIF/PDF covers, video thumbnails, or video metadata. Slim images omit those external media commands to reduce downloads, storage, and the software bill of materials. Built-in processing for common images and audio, storage-provider-native processing, and all other product capabilities such as databases, storage backends, WebDAV, WOPI, and remote nodes remain available.

| Release channel | Default features | Metrics enabled |
| --- | --- | --- |
| Stable release | `vX.Y.Z` / `latest` / `stable` | `vX.Y.Z-metrics` / `latest-metrics` / `stable-metrics` |
| Slim | `vX.Y.Z-slim` / `latest-slim` / `stable-slim` | `vX.Y.Z-metrics-slim` / `latest-metrics-slim` / `stable-metrics-slim` |
| Prerelease | `edge` / `edge-slim` | `edge-metrics` / `edge-metrics-slim` |

Fresh slim instances disable the `vips_cli`, `ffmpeg_cli`, and `ffprobe_cli` processors by default. Before switching an existing instance from the full image, review these processors under `Admin -> System Settings -> File Processing -> Media Processing`. The existing database keeps its configuration, but a slim container reports the missing commands as unavailable and does not advertise their formats through public capability endpoints. Keep using the full image when the instance needs any of these processors.

If you bind mount a host directory directly to `/data`, **create the directory first and change its owner to `10001:10001`**. Otherwise, container startup will fail with permission errors when generating `config.toml`, creating the SQLite file, or creating temporary directories:

```bash
mkdir -p ./data
sudo chown -R 10001:10001 ./data
```

If you use a named volume (`docker volume create` or a `volumes:` section in Compose), Docker automatically sets the volume owner to the user running inside the container. You do not need to run `chown` manually.

:::tip[If this container should run as a follower node]
Follower nodes now support reading bootstrap ENV during startup and completing enrollment directly.  
If you want to attach another AsterDrive instance as a follower node with Docker, the old flow of manually running `docker exec ... node enroll` is no longer recommended. See [Docker Follower Node Deployment](/en/deploy/follower-node/) instead.
:::

## 1. Try It First

If you are still in a plain HTTP test environment, you can run the following command. The `-p 3000:3000` mapping listens on every host interface and is only appropriate on an isolated, trusted network. For host-local access, use `-p 127.0.0.1:3000:3000` instead, and never expose this HTTP entry directly to the public Internet.

```bash
mkdir -p ./data
sudo chown -R 10001:10001 ./data

docker run -d \
  --name asterdrive \
  -p 3000:3000 \
  -e ASTER__SERVER__HOST=0.0.0.0 \
  -e ASTER__DATABASE__URL="sqlite:///data/asterdrive.db?mode=rwc" \
  -v "$(pwd)/data:/data" \
  ghcr.io/astercommunity/asterdrive:latest
```

Fresh installations disable the browser Cookie HTTPS requirement only for first initialization by default, so a plain-HTTP trial needs no extra environment variable. If the administrator is created directly from an HTTPS origin, setup enables Secure cookies before automatic login. If the instance moves from HTTP to HTTPS later, enable the corresponding system setting in the admin panel.

After startup, use `docker ps` to check container status. Normally it becomes `healthy` after a short time.

## 2. Long-Term Deployment: Edit `config.toml` on the Host

`config.toml` is now generated uniformly at `/data/config.toml`, in the same volume as the database and upload directories. It **no longer needs** to be mounted separately as read-only as older documentation described.

After binding `./data` to `/data` with the command above, AsterDrive automatically generates `./data/config.toml` on first startup. You can then edit that file directly on the host to override defaults, for example:

```toml
[auth]
jwt_secret = "replace-with-your-own-random-secret"

[server]
temp_dir = "/data/.tmp"
upload_temp_dir = "/data/.uploads"
```

Restart the container after editing for changes to take effect. `bootstrap_insecure_cookies` is the exception: it only matters before the database is initialized for the first time. For an existing instance, change the cookie security requirement in the admin system settings.

## 3. Compose Example

```yaml
services:
  asterdrive:
    image: ghcr.io/astercommunity/asterdrive:latest
    ports:
      - "3000:3000"
    environment:
      ASTER__SERVER__HOST: 0.0.0.0
      ASTER__DATABASE__URL: sqlite:///data/asterdrive.db?mode=rwc
    volumes:
      - ./data:/data
      - /etc/localtime:/etc/localtime:ro
    restart: unless-stopped
```

Before running `docker compose up -d` for the first time, prepare the host directory with `mkdir -p ./data && sudo chown -R 10001:10001 ./data` as described at the top. Otherwise, the in-container `aster` user (UID/GID `10001`) cannot write to it, and startup will fail.

## 4. Enable aria2 Link Import with Compose

The repository root `docker-compose.yml` includes an optional `aria2` profile. Plain `docker compose up -d` does not start it; aria2 is started only when the profile is enabled explicitly.

Prepare both the AsterDrive data directory and the aria2 configuration directory first. AsterDrive and aria2 must mount the same host `./data` directory at the same in-container `/data` path, because AsterDrive passes task temporary file paths such as `/data/.tmp/...` to aria2 as absolute paths:

```bash
mkdir -p ./data ./aria2-config
sudo chown -R 10001:10001 ./data ./aria2-config
```

Set an RPC secret and start both services. `ASTERDRIVE_ARIA2_RPC_SECRET` is required; do not start the `aria2` profile with this variable unset, because the Compose service passes it directly to `RPC_SECRET` for `p3terx/aria2-pro`:

```bash
export ASTERDRIVE_ARIA2_RPC_SECRET="$(openssl rand -hex 24)"
docker compose --profile aria2 up -d
```

Then open `Admin -> System Settings -> File Processing -> Link Import` and enable `aria2` in the link-import engine registry. If you want the built-in downloader as fallback, keep `builtin` enabled after `aria2`; if you want aria2 only, disable `builtin`. Then set these runtime config values:

| Config key | Value |
| --- | --- |
| `offline_download_temp_dir` | `/data/.tmp/offline-download` |
| `offline_download_aria2_rpc_url` | `http://aria2:6800/jsonrpc` |
| `offline_download_aria2_rpc_secret` | the value of `ASTERDRIVE_ARIA2_RPC_SECRET` above |

If you start only aria2 with Compose while running AsterDrive on the host with `cargo run`, use `http://127.0.0.1:6800/jsonrpc` instead. This mixed development mode still requires `offline_download_temp_dir` to be the same absolute path visible to both sides. For example, mount host `./data/offline-download-temp` into the aria2 container at `/srv/asterdrive/offline-download-temp`, then put that host absolute path in AsterDrive.

When aria2 runs as a different OS user, AsterDrive must let that external writer create the downloaded temp file under the per-task `token_dir`. The compatibility path in `allow_external_aria2_writer_chain` makes the per-task directories world-writable, while leaving the shared parent tasks directory traversable only. This is acceptable for isolated single-tenant Compose deployments where the temp volume is not shared with untrusted local users. Safer production alternatives are to run both processes under the same UID, assign a shared group and use `0o770`, or apply POSIX ACLs for the aria2 user on `token_dir`.

After saving, use **Test aria2** in the link-import engine registry. The server calls `aria2.getVersion` with the current RPC URL and secret to confirm AsterDrive can reach the aria2 JSON-RPC endpoint.

You can also write the SQLite runtime config from the CLI during a maintenance window:

```bash
docker compose exec asterdrive /usr/local/bin/aster_drive \
  config --database-url "sqlite:///data/asterdrive.db?mode=rwc" \
  set --key offline_download_engine_registry_json \
  --value '{"version":1,"engines":[{"kind":"aria2","enabled":true},{"kind":"builtin","enabled":true}]}'

docker compose exec asterdrive /usr/local/bin/aster_drive \
  config --database-url "sqlite:///data/asterdrive.db?mode=rwc" \
  set --key offline_download_aria2_rpc_url --value http://aria2:6800/jsonrpc

docker compose exec asterdrive /usr/local/bin/aster_drive \
  config --database-url "sqlite:///data/asterdrive.db?mode=rwc" \
  set --key offline_download_temp_dir --value /data/.tmp/offline-download

docker compose exec asterdrive /usr/local/bin/aster_drive \
  config --database-url "sqlite:///data/asterdrive.db?mode=rwc" \
  set --key offline_download_aria2_rpc_secret --value "$ASTERDRIVE_ARIA2_RPC_SECRET"
```

Do not publish aria2 port `6800` to the public internet in production; if host-side AsterDrive does not need to reach it, do not publish it to the host either. aria2 still performs its own DNS resolution and outbound connection for downloads, so production deployments should also restrict its reachable network using Docker networking, host firewall rules, or upstream network policy.

For full configuration, security boundaries, and troubleshooting, see [Offline Download](/en/admin/offline-download/).

## 5. Data Persistence Boundaries

If you bind mount `./data` to the container's `/data` as shown above, you will usually see:

- `config.toml`
- `asterdrive.db`
- `uploads/`
- `avatar/` (after users upload avatars)
- `.tmp/`
- `.uploads/`

Among these:

- `config.toml`, `asterdrive.db`, `uploads/`, and `avatar/` if avatar upload is enabled, must be kept long term.
- `.tmp/` and `.uploads/` generally do not need backup, but they affect local disk usage.

See [Backup and Restore](/en/ops/backup/) for more complete backup / restore guidance.

## 6. Launch Acceptance

First deployment checks worth doing:

- Whether `auth.jwt_secret` has been fixed.
- Whether plain-HTTP first login works with the default `bootstrap_insecure_cookies = true`, without an extra environment variable.
- Whether HTTPS initialization enabled the cookie security switch automatically; if HTTPS was added later, whether it was enabled manually in the admin panel.
- Whether the home page response headers include the browser page baseline `Content-Security-Policy` returned by AsterDrive, and whether the proxy has removed it or replaced it with an incompatible policy.
- If the site is publicly accessible, whether `Public Site URL` is set to a real `https://` origin. Add multiple public domains one by one, with the default origin first.
- If public registration, password recovery, or email rebinding will be enabled, whether a test email has been sent successfully.
- Whether the database, upload directory, and temporary directories all live in the bind-mounted `./data` directory, with nothing accidentally written inside the container layer.
- Whether the default policy group has been created.
- If external Office / WOPI openers are enabled, whether at least one real Office file can be opened and saved.
- If aria2 link import is enabled, whether `offline_download_aria2_rpc_url` points to the Docker-internal address `http://aria2:6800/jsonrpc` for full Docker deployments, whether `offline_download_temp_dir` is the same absolute path visible to both sides, or whether RPC points to `http://127.0.0.1:6800/jsonrpc` for host-side `cargo run` + Compose aria2 development; and whether the aria2 RPC port is not exposed publicly.
- If you plan to use S3 / MinIO later, whether browser upload CORS rules and secret management for object storage have been planned.
- If this instance should actually run as a `follower`, whether long-term `start_mode`, single-use bootstrap ENV, and the primary-side default remote storage target have been configured according to [Docker Follower Node Deployment](/en/deploy/follower-node/).

The full pre-launch list is in the [Production Launch Checklist](/en/ops/launch-checklist/).

## Day-to-Day Maintenance Entry Points

View runtime status:

```bash
docker logs -f asterdrive
```

- Upgrades: see the "Upgrade" section below; full version migration notes are in [Upgrade and Version Migration](/en/ops/upgrade/)
- Backups: see [Backup and Restore](/en/ops/backup/)
- Monitoring: see [Monitoring and Grafana](/en/ops/monitoring/)
- Troubleshooting: locate container startup or upload failures in [Troubleshooting](/en/ops/troubleshooting/)

## Upgrade

If you use the Compose example above:

```bash
docker compose pull
docker compose up -d
```

If you run directly with `docker run`, the steps are the same: pull the new image, stop the old container, and start it again with the same command. The bind-mounted `./data` is not affected:

```bash
docker pull ghcr.io/astercommunity/asterdrive:latest
docker rm -f asterdrive
# Run the docker run command from "Try It First" again
```

After upgrading, reopen the browser page and recheck login, upload, sharing, policy groups, WebDAV, and any external openers currently in use.
