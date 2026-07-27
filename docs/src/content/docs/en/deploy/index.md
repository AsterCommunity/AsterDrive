---
description: Choose an AsterDrive deployment scenario across single-instance Docker, single-instance systemd, multi-instance, Kubernetes, follower storage nodes, and reverse proxy paths, plus the data, access, WebDAV, and storage decisions to make before deploying.
title: "Deployment Overview"
---

:::tip[What this page does]
Pick one deployment scenario path by scale and environment, then follow its scenario page from zero to launch acceptance:

| Scenario | Best for | Path |
| --- | --- | --- |
| Single-instance Docker | NAS, single machine, small teams, existing container environments | [Single-Instance Docker Deployment](/en/deploy/docker/) |
| Single-instance systemd / binary | Cloud hosts, physical machines, long-running stable services | [Single-Instance systemd Deployment](/en/deploy/systemd/) |
| Multi-instance and load balancing | Multiple Primaries on a shared data plane | [Load Balancing and Multi-Instance Deployments](/en/deploy/multi-instance/) |
| Kubernetes | Container orchestration, Ingress, StatefulSet | [Kubernetes Deployment](/en/deploy/kubernetes/) |
| Follower storage node | Attaching another AsterDrive as a remote storage node | [Docker Follower Node Deployment](/en/deploy/follower-node/) |
| Reverse proxy and public entry | Any production launch | [Reverse Proxy](/en/deploy/reverse-proxy/) |

Not sure which to pick? Start with [Choose a Deployment Method](/en/start/choose-deployment/).
:::

AsterDrive is delivered as a single service:

- browser page
- public sharing page
- admin panel
- WebDAV
- file preview and WOPI entry

All are served by the same process.  
The three most important deployment concerns are:

- keep the service running reliably
- preserve data correctly
- make uploads, WebDAV, and external openers work in your network environment

## Confirm These Four Things Before Deployment

They apply no matter which scenario path you take.

### Data Directory

These contents must survive restarts and upgrades:

- `data/config.toml`
- database
- local upload directory

If avatar upload is enabled, or if you configured additional local `local` storage policies, keep these too:

- the local directory corresponding to `avatar_dir` (usually `data/avatar` by default)
- any custom local storage root directories

The service also uses temporary directories at runtime:

- `data/.tmp`
- `data/.uploads`

These two directories usually do not need backup, but the local disk must have available space.

### Access Method

For production launch, you **must** serve HTTPS through a reverse proxy, and keep:

```toml
[auth]
bootstrap_insecure_cookies = false
```

If this is only local or intranet HTTP first bootstrap, you may temporarily set it to `true` so the system initializes the browser Cookie HTTPS requirement as disabled.  
After switching to HTTPS in production, change it back to enabled in the admin system settings.

If the site will be externally accessible, also confirm:

- The home page response headers include the browser page baseline `Content-Security-Policy` returned by AsterDrive, and the proxy has not removed it or overwritten it with an incompatible policy.
- `Admin -> System Settings -> Site Configuration -> Public Site URL` has been set to the real `https://` origin. Add multiple public domains one by one.
- If public registration, password recovery, or email rebinding will be enabled, `Admin -> System Settings -> Mail Delivery` has sent a test email successfully.

### WebDAV

If Finder, Windows, or sync tools need access, consider these during deployment:

- WebDAV path
- reverse proxy
- upload size limits

### Online Preview / WOPI

If you plan to open Office files through an external service, confirm these too:

- `Public Site URL` is set to the real `https://` origin.
- `Site Configuration -> Preview Applications` has the corresponding opener configured.
- The external Office / WOPI service can access the AsterDrive address represented by `Public Site URL`. If browser cross-origin calls to AsterDrive APIs are blocked, allow the corresponding origin under `Network Access`.

### Storage Location

- Local disk: simplest deployment.
- S3 / MinIO: suitable for object storage scenarios.

## What First Startup Does Automatically

After the service starts successfully, it automatically completes these preparations:

- generates the default `data/config.toml`
- connects to the database and updates the database structure automatically
- neither single nor cluster creates a storage policy automatically; both enter `needs_storage` after the first administrator is created
- when the administrator makes the first suitable policy the default, AsterDrive atomically creates or reconciles the default policy group and assigns unassigned administrators; single may use local storage, while cluster requires storage reachable by every Primary
- initializes default system setting entries
- starts mail dispatch, background task dispatch, periodic cleanup, and low-level file consistency check tasks

## Validate These After Launch

The full checklist is in [First-Start Checklist](/en/ops/first-check/#check-these-items-immediately-after-startup).

At minimum, verify these after deployment:

1. `/health` and `/health/ready` return normally.
2. The home page opens normally and login works.
3. You can create a folder and upload a file.
4. The admin panel opens.

Validate other role-specific areas (WebDAV, WOPI, mail, trash, and so on) according to the corresponding sections in the [First-Start Checklist](/en/ops/first-check/#check-these-items-immediately-after-startup).

## After Deployment: The Operations Lifecycle

Launch is only the beginning. These topics belong to day-to-day operations and are not expanded inside the deployment scenario pages:

- [Production Launch Checklist](/en/ops/launch-checklist/): full acceptance before going live
- [Backup and Restore](/en/ops/backup/): backup strategy, restore order, and post-restore validation
- [Upgrade and Version Migration](/en/ops/upgrade/): upgrade paths and version notes
- [Monitoring and Grafana](/en/ops/monitoring/): Prometheus metrics and dashboards
- [Capacity Planning](/en/ops/capacity/): estimating file count, database size, memory, and temporary disk
- [Troubleshooting](/en/ops/troubleshooting/): locate problems by symptom
- [Operations CLI](/en/ops/cli/): doctor, offline configuration, node enrollment, and database migration
