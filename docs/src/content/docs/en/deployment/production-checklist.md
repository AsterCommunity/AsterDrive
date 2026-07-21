---
title: "Production Launch Checklist"
---

:::tip[When to read this]
Before making AsterDrive available to real users, use this page for the final check. It does not repeat every detail from each configuration page; it collects the items most likely to be missed before launch into one checklist.
:::

## Summary First

| Check item | Passing standard | Continue reading |
| --- | --- | --- |
| Data directory | `config.toml`, database, local storage directories, and avatar directory are all persisted | [Backup and Restore](/en/deployment/backup/) |
| Access entry | Users access through HTTPS, and the reverse proxy preserves required request and response headers | [Reverse Proxy](/en/deployment/reverse-proxy/) |
| Public Site URL | `Admin -> System Settings -> Site Configuration -> Public Site URL` is a real HTTPS origin | [System Settings](/en/config/runtime/#site-configuration) |
| Login security | `jwt_secret` is fixed, and HTTPS-only Cookies are enabled | [Login and Sessions](/en/config/auth/) |
| Storage route | Default storage policy, default policy group, and user/team bindings match expectations | [Storage Policies](/en/config/storage/) |
| Mail | Registration activation, password reset, and email rebinding messages can be delivered | [Mail](/en/config/mail/) |
| Multi-instance configuration sync | Instances share one database, Redis endpoint, and topic, and cross-instance updates have been verified | [Configuration Synchronization](/en/config/config-sync/) |
| Monitoring | If Prometheus is needed, the `metrics` feature is compiled as needed, and `/health/metrics` access sources are restricted | [Monitoring and Grafana](/en/deployment/monitoring/) |
| Backup | At least one backup has been completed, and the restore sequence is known | [Backup and Restore](/en/deployment/backup/) |
| Upgrade | The current version source is known, and release notes have been reviewed before upgrade | [Upgrade and Version Migration](/en/deployment/upgrade/) |

## 1. Data Must Survive

Confirm these contents will not be lost when containers are recreated, the system restarts, or upgrades happen:

- `data/config.toml`
- database file, or external database instance
- default local storage directory
- additional local `local` storage policy directories
- avatar directory, usually `data/avatar` by default
- the follower node's `remote_storage_target_local_root`, if you use local remote storage targets

If you use Docker bind mounts, also confirm the host directory owner and permissions match the container runtime user. The official image uses UID/GID `10001:10001` by default.

:::caution[Do not back up only the database]
The database only stores metadata. File objects, avatars, local remote storage targets, and object storage state must also be included in the backup boundary. If you only back up the database, restored file records may still exist while the objects no longer do.
:::

## 2. HTTPS and Public Entry

Production should always run behind a reverse proxy, with the proxy handling TLS, HSTS, upload limits, WebDAV method passthrough, and long connections.

At minimum, confirm the reverse proxy:

- preserves the real `Host`
- passes the public protocol, such as `X-Forwarded-Proto`
- has upload size limits large enough for direct small-file uploads and WebDAV writes
- does not block WebDAV methods
- does not overwrite the site with an incompatible global CSP
- passes through WOPI, sharing page, preview, and thumbnail paths

Before launch, access the site once with the real domain, then confirm:

```text
Admin -> System Settings -> Site Configuration -> Public Site URL
```

Every entry is a real HTTPS origin users will access, for example:

```text
https://drive.example.com
https://panel.example.com
```

If you have multiple public domains, log in through each domain once and confirm sharing links, WebDAV addresses, and WOPI addresses use the current domain or your expected default origin.

## 3. Login and Cookies

Before launch, confirm:

- `auth.jwt_secret` has been fixed and is no longer a temporary random value
- `Admin -> System Settings -> Authentication and Cookies -> Send Authentication Cookies over HTTPS Only` is enabled
- Access Token / Refresh Token lifetimes match your security policy
- the first administrator account password has been changed to a production password
- there is more than one administrator account, or at least the recovery flow can recreate an administrator

If you initially bootstrapped over plain HTTP:

```toml
[auth]
bootstrap_insecure_cookies = true
```

After switching to HTTPS, change the Cookie security requirement back to enabled in the admin system settings. This bootstrap configuration only affects first initialization of system settings; later behavior follows the admin system setting.

## 4. Storage Policies and Policy Groups

At minimum, confirm:

- the default storage policy exists and passes testing
- the default policy group exists and is enabled
- the new-user default policy group matches expectations
- existing user and team policy group bindings do not point to deprecated policies
- single-file size limits, chunk size, user quota, and team quota match real usage scenarios
- S3 / MinIO / R2 CORS, endpoint, bucket, and secrets have been tested
- follower nodes are enrolled, enabled, and have applied default remote storage targets

If you plan to move production traffic to a new storage backend, do not directly change an existing policy's `base_path`, `bucket`, `endpoint`, or bound follower node. A safer path is to create a new policy, migrate data, then switch policy groups.

## 5. Mail and Account Flows

If public registration, email rebinding, or password reset will be enabled, mail must work first.

Before launch, test at least:

- SMTP connection test passes
- test email can be received
- registration activation links use the correct domain
- password reset links use the correct domain
- email rebinding links use the correct domain
- link expiration and resend cooldown match expectations

If you do not plan to allow public registration, disable:

```text
Admin -> System Settings -> User Management -> Allow Public User Registration
```

## 6. WebDAV and Online Editing

If desktop clients will use WebDAV:

- the global WebDAV switch in system settings is enabled
- WebDAV prefix matches the reverse proxy path
- real clients can connect, list directories, upload, overwrite-save, and delete
- clients use dedicated WebDAV accounts, not normal web login passwords
- if team spaces need WebDAV access, clients use WebDAV accounts created in the team space and member permissions match expectations

If online preview or WOPI is enabled:

- `Public Site URL` is reachable by the external Office / WOPI service
- `Preview Applications` has the corresponding opener enabled
- WOPI Discovery URL is reachable
- real `docx` / `xlsx` / `pptx` files can open
- saving writes back to AsterDrive and creates historical versions
- the reverse proxy does not block WOPI callback paths

The full integration flow is in [Online Preview and WOPI](/en/guide/preview-and-wopi/).

## 7. Backup and Restore Rehearsal

Before launch, take at least one backup and confirm you know the restore sequence.

At minimum, cover:

- `data/config.toml`
- database
- local storage directories
- avatar directory
- additional local policy directories
- bucket versioning, replication, or external backup strategy for object storage

A small restore rehearsal is recommended: restore the backup into a test environment, run `aster_drive doctor` after startup, then test upload, download, sharing, and restoring a trash item with a real account.

## 8. Logs, Audit, and Rate Limiting

In production, confirm:

- runtime logs are collected by Docker logs, journald, or a file collection system
- `RUST_LOG` is not mistakenly set to an overly verbose level
- audit logs are enabled according to your needs
- audit log retention matches capacity and compliance requirements
- public entry rate limits match site scale
- reverse-proxy-side and AsterDrive-side rate limit boundaries do not conflict

If the site is exposed to the public internet, at least enable rate limiting for authentication, public sharing, and write paths.

## 9. Prometheus Metrics

Prometheus metrics are compiled on demand and are not enabled in the default build. To collect them in production, use:

```bash
cargo build --release --features metrics
```

Or use the `full` feature that includes all optional capabilities.

Before launch, confirm:

- `/health/metrics` is accessible from the scraper
- `/health/metrics` is not exposed directly to the public internet
- reverse proxy, firewall, or security group only allows monitoring systems to access it
- scrape interval, retention period, and scrape targets are defined
- HTTP, DB, upload, download, background task, storage driver, and process RSS / CPU / uptime metrics are visible

AsterDrive currently does not apply application-layer authentication to `/health/metrics`. This keeps Prometheus scraping simple and stable, with access control handled by the reverse proxy or network boundary.

Grafana dashboard and local Prometheus + Grafana examples are in [Monitoring and Grafana](/en/deployment/monitoring/).

## 10. Multi-Instance Configuration Synchronization

If only one AsterDrive process runs, keep `[config_sync].backend = "disabled"`.

For multiple instances, confirm:

- every primary instance explicitly sets `[deployment].profile = "cluster"`
- every instance connects to the same authoritative database
- every instance can reach the configured Redis endpoint
- every instance uses exactly the same `aster_drive.config_reload` topic
- every storage policy uses shared storage rather than independent `local` roots
- if reverse tunnels are enabled, every primary sets its own `deployment.internal_endpoint` and the same `deployment.internal_proxy_secret`
- the reverse-tunnel owner directory exposes a live lease/fencing token, and a failover drill confirms stale tokens are rejected before the standby takes over
- changing a system setting through instance A becomes visible through instance B promptly
- the Redis outage and recovery procedure has been rehearsed

See [Deployment Profile](/en/config/deployment/) for the deployment contract and [Configuration Synchronization](/en/config/config-sync/) for notification failure behavior.

## 11. Final Acceptance Pass

Before launch, use the real domain, real accounts, and real clients to run through:

1. `/health` returns 200.
2. `/health/ready` returns 200.
3. If metrics are enabled, restricted-source access to `/health/metrics` returns Prometheus text.
4. Login, page refresh, and logout all work.
5. Upload one small file and one large file.
6. Download a file and preview an image or PDF.
7. Create a sharing link and open it in a browser without login.
8. Delete a file, then restore it from trash.
9. If WebDAV is enabled, read and write once with a real client.
10. If WOPI is enabled, open and save a real Office file once.
11. Check `Admin -> Tasks` for recently and repeatedly failing tasks.
12. Check `Admin -> Audit Logs` to confirm key operations are recorded as expected.
13. Run `aster_drive doctor`.

## What to Watch on the First Day After Launch

- Whether the error logs repeatedly show the same class of 4xx / 5xx.
- Whether upload failures concentrate in a specific policy group or storage backend.
- Whether the mail queue has persistent failures.
- Whether thumbnail and online extraction tasks accumulate.
- Whether public sharing access uses the correct domain.
- Whether WebDAV clients frequently leave locks behind.
- Whether data and temporary directory disk usage matches expectations.
