# systemd Deployment

::: tip Who this page is for
Cloud hosts, physical machines, and long-running stable Linux servers.
systemd only starts the process. HTTPS / domain / WebDAV passthrough all belong in the reverse proxy in front. Do not expose `aster_drive` directly to the public internet.
:::

systemd is suitable for long-running stable Linux servers.
For this type of deployment, the most important thing is to decide the working directory first, then decide where the config file, database, upload directories, and temporary directories should live.

For production launch, do not expose `aster_drive` directly to the public internet.
systemd only starts the process; HTTPS, domains, HSTS, upload limits, and WebDAV / WOPI passthrough should all be handled by the reverse proxy in front. The browser page baseline `Content-Security-Policy` is returned by AsterDrive, and the proxy layer should preserve that response header instead of overwriting it with an incompatible policy.

## 1. Prepare the Runtime Directory

```bash
sudo useradd -r -s /usr/sbin/nologin asterdrive
sudo mkdir -p /var/lib/asterdrive
sudo chown -R asterdrive:asterdrive /var/lib/asterdrive
```

## 2. Place the Executable

Put the `aster_drive` executable at a fixed path, for example:

```bash
sudo install -m 0755 ./aster_drive /usr/local/bin/aster_drive
```

## 3. Prepare the Config File

Put `config.toml` into the `data/` directory:

```bash
sudo mkdir -p /var/lib/asterdrive/data
sudo cp config.toml /var/lib/asterdrive/data/config.toml
sudo chown -R asterdrive:asterdrive /var/lib/asterdrive/data
```

If you continue using default relative paths, the working directory will usually contain:

- `data/config.toml`
- `data/asterdrive.db`
- `data/uploads`
- `data/.tmp`
- `data/.uploads`

For long-term deployment, database paths, local storage paths, and temporary directories should preferably use absolute paths.

If you plan to run long term, do not only think about "how to start"; plan backup and restore at the same time.
SQLite, local storage directories, avatar directories, and any custom local `local` storage roots should all be included in the same backup strategy. See [Backup and Restore](/en/deployment/backup).

## 4. Write the Service File

Create `/etc/systemd/system/asterdrive.service`:

```ini
[Unit]
Description=AsterDrive
After=network.target

[Service]
Type=simple
User=asterdrive
Group=asterdrive
WorkingDirectory=/var/lib/asterdrive
ExecStart=/usr/local/bin/aster_drive
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
```

If you are still in an intranet HTTP test environment, you can temporarily set `auth.bootstrap_insecure_cookies` to `true` in `config.toml`.
It only affects the Cookie HTTPS requirement during first initialization. After switching to HTTPS in production, change the corresponding admin setting back to enabled, then remove this static bootstrap option.

## 5. Start the Service

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now asterdrive
sudo systemctl status asterdrive
```

## 6. View Logs

```bash
journalctl -u asterdrive -f
```

## 7. First Acceptance Check

- `/health` returns 200.
- `/health/ready` returns 200.
- The home page response headers include the browser page baseline `Content-Security-Policy` returned by AsterDrive.
- First startup logs show database updates and default policy initialization have completed.
- The default policy group is visible in the admin panel.
- Browser login works normally.
- If WebDAV is enabled, the actual mount path matches `[webdav].prefix`.
- If external Office / WOPI openers are enabled, at least one real Office file can be opened and saved.
- If the database, upload directory, or temporary directories are placed on another disk, confirm paths and permissions are correct.

## 8. Common Environment Variable Patterns

### Put the Database Somewhere Else

```ini
Environment=ASTER__DATABASE__URL=sqlite:///srv/asterdrive/asterdrive.db?mode=rwc
```

### Listen on All Interfaces

```ini
Environment=ASTER__SERVER__HOST=0.0.0.0
```

### Fix the Login Signing Key

```ini
Environment=ASTER__AUTH__JWT_SECRET=replace-with-your-own-secret
```

### Run This Service as a Follower Node

```ini
Environment=ASTER__SERVER__START_MODE=follower
Environment=ASTER__SERVER__FOLLOWER__REMOTE_STORAGE_TARGET_LOCAL_ROOT=/srv/asterdrive/managed-ingress
```

If you also want the follower node to complete one-time enrollment during startup, temporarily add:

```ini
Environment=ASTER_BOOTSTRAP_REMOTE_MASTER_URL=https://drive.example.com
Environment=ASTER_BOOTSTRAP_REMOTE_ENROLLMENT_TOKEN=enr_replace_me
```

After confirming enrollment succeeded, remove these two `ASTER_BOOTSTRAP_REMOTE_*` values and keep only the long-term `ASTER__...` overrides required for runtime. See the full flow in [Follower Nodes](/en/guide/remote-nodes).

## 9. HTTPS and Domain

systemd only starts the service.
If you need HTTPS, domains, WebDAV client access, or external Office / WOPI openers, add a reverse proxy in front. The proxy handles TLS, HSTS, upload limits, and long-connection passthrough while preserving the page CSP returned by AsterDrive. See [Reverse Proxy Deployment](/en/deployment/reverse-proxy).
