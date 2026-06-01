# Docker Deployment

::: tip Who this page is for
NAS, single-machine, small-team, or existing container-orchestrated deployments. You can get it running in 10 minutes.
For **production launch**, put a reverse proxy in front to handle HTTPS. Do not expose port `3000` directly to the public internet.
:::

The official image runs as a **non-root user** by default (UID/GID fixed to `10001:10001`, username `aster`) and includes a `HEALTHCHECK` based on `/health/ready`.

If you bind mount a host directory directly to `/data` (recommended, because backups and migration are clearer), **create the directory first and change its owner to `10001:10001`**. Otherwise, container startup will fail with permission errors when generating `config.toml`, creating the SQLite file, or creating temporary directories:

```bash
mkdir -p ./data
sudo chown -R 10001:10001 ./data
```

If you use a named volume (`docker volume create` or a `volumes:` section in Compose), Docker automatically sets the volume owner to the user running inside the container. You do not need to run `chown` manually.

Running the service in a container does not mean you should expose port `3000` to the public internet long term.  
For production launch, you should still put a reverse proxy in front to handle HTTPS, HSTS, upload limits, WebDAV, and WOPI, and preserve the **browser page baseline** `Content-Security-Policy` returned by AsterDrive. Do not rewrite the whole site's CSP to a site-wide `sandbox`.

::: tip If this container should run as a follower node
Follower nodes now support reading bootstrap ENV during startup and completing enrollment directly.  
If you want to attach another AsterDrive instance as a follower node with Docker, the old flow of manually running `docker exec ... node enroll` is no longer recommended. See [Docker Follower Node Deployment](/en/deployment/docker-follower) instead.
:::

## What `/data` Usually Contains

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

See [Backup and Restore](/en/deployment/backup) for more complete backup / restore guidance.

## Try It First

If you are still in a plain HTTP test environment, you can run:

```bash
mkdir -p ./data
sudo chown -R 10001:10001 ./data

docker run -d \
  --name asterdrive \
  -p 3000:3000 \
  -e ASTER__SERVER__HOST=0.0.0.0 \
  -e ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES=true \
  -e ASTER__DATABASE__URL="sqlite:///data/asterdrive.db?mode=rwc" \
  -v "$(pwd)/data:/data" \
  ghcr.io/apts-1547/asterdrive:latest
```

This only disables the browser Cookie HTTPS requirement during first initialization.  
After switching to HTTPS for production, change the corresponding system setting back to enabled in the admin panel, then remove this environment variable.

After startup, use `docker ps` to check container status. Normally it becomes `healthy` after a short time.

## Long-Term Deployment: Edit `config.toml` on the Host

`config.toml` is now generated uniformly at `/data/config.toml`, in the same volume as the database and upload directories. It **no longer needs** to be mounted separately as read-only as older documentation described.

After binding `./data` to `/data` with the command above, AsterDrive automatically generates `./data/config.toml` on first startup. You can then edit that file directly on the host to override defaults, for example:

```toml
[auth]
jwt_secret = "replace-with-your-own-random-secret"
bootstrap_insecure_cookies = false

[server]
temp_dir = "/data/.tmp"
upload_temp_dir = "/data/.uploads"
```

Restart the container after editing for changes to take effect.

## Compose Example

```yaml
services:
  asterdrive:
    image: ghcr.io/apts-1547/asterdrive:latest
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

## Enable aria2 Link Import with Compose

The repository root `docker-compose.yml` includes an optional `aria2` profile. Plain `docker compose up -d` does not start it; aria2 is started only when the profile is enabled explicitly.

Prepare both the AsterDrive data directory and the aria2 configuration directory first. AsterDrive and aria2 must mount the same host `./data` directory at the same in-container `/data` path, because AsterDrive passes task temporary file paths such as `/data/.tmp/...` to aria2 as absolute paths:

```bash
mkdir -p ./data ./aria2-config
sudo chown -R 10001:10001 ./data ./aria2-config
```

Set an RPC secret and start both services:

```bash
export ASTERDRIVE_ARIA2_RPC_SECRET="$(openssl rand -hex 24)"
docker compose --profile aria2 up -d
```

Then open `Admin -> System Settings -> File Processing -> Link Import` and enable `aria2` in the link-import engine registry. If you want the built-in downloader as fallback, keep `builtin` enabled after `aria2`; if you want aria2 only, disable `builtin`. You can also disable both engines, which disables new link-import tasks. Then set these runtime config values:

| Config key | Value |
| --- | --- |
| `offline_download_aria2_rpc_url` | `http://aria2:6800/jsonrpc` |
| `offline_download_aria2_rpc_secret` | the value of `ASTERDRIVE_ARIA2_RPC_SECRET` above |

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
  set --key offline_download_aria2_rpc_secret --value "$ASTERDRIVE_ARIA2_RPC_SECRET"
```

Do not publish aria2 port `6800` to the host or the public internet. This Compose example uses `expose` so only services on the same Docker network can reach the RPC endpoint. aria2 still performs its own DNS resolution and outbound connection for downloads, so production deployments should also restrict its reachable network using Docker networking, host firewall rules, or upstream network policy.

## First Deployment Checks Worth Doing

- Whether `auth.jwt_secret` has been fixed.
- If this is temporarily a plain HTTP test, whether `bootstrap_insecure_cookies = true` was set only for first bootstrap.
- After switching to HTTPS, whether the Cookie security switch in system settings has been changed back to enabled.
- Whether the home page response headers include the browser page baseline `Content-Security-Policy` returned by AsterDrive, and whether the proxy has removed it or replaced it with an incompatible policy.
- If the site is publicly accessible, whether `Public Site URL` is set to a real `https://` origin. Add multiple public domains one by one, with the default origin first.
- If public registration, password recovery, or email rebinding will be enabled, whether a test email has been sent successfully.
- Whether the database, upload directory, and temporary directories all live in the bind-mounted `./data` directory, with nothing accidentally written inside the container layer.
- Whether the default policy group has been created.
- If external Office / WOPI openers are enabled, whether at least one real Office file can be opened and saved.
- If aria2 link import is enabled, whether `offline_download_aria2_rpc_url` points to the Docker-internal address `http://aria2:6800/jsonrpc`, and whether the aria2 RPC port is not exposed publicly.
- If you plan to use S3 / MinIO later, whether browser upload CORS rules and secret management for object storage have been planned.
- If this instance should actually run as a `follower`, whether long-term `start_mode`, single-use bootstrap ENV, and the primary-side default receiving target have been configured according to [Docker Follower Node Deployment](/en/deployment/docker-follower).

## View Runtime Status

```bash
docker logs -f asterdrive
```

## Upgrade

If you use the Compose example above:

```bash
docker compose pull
docker compose up -d
```

If you run directly with `docker run`, the steps are the same: pull the new image, stop the old container, and start it again with the same command. The bind-mounted `./data` is not affected:

```bash
docker pull ghcr.io/apts-1547/asterdrive:latest
docker rm -f asterdrive
# Run the docker run command from "Try It First" again
```

After upgrading, reopen the browser page and recheck login, upload, sharing, policy groups, WebDAV, and any external openers currently in use.
