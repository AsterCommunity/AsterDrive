---
description: AsterDrive deployment overview for Docker, systemd, reverse proxy, first-start checklist, backup and restore, upgrades, and troubleshooting.
title: "Deployment Overview"
---

:::tip[Where to start]
Jump directly by deployment method:
- **Docker / NAS / small team** -> [Docker Deployment](/en/deployment/docker/)
- **Long-running Linux server** -> [systemd Deployment](/en/deployment/systemd/)
- **Kubernetes / multiple Primaries** -> [Load Balancing and Multi-Instance Deployments](/en/deployment/load-balancing/)
- **Want command-line checks, offline config changes, or cross-database migration** -> [Operations CLI](/en/deployment/ops-cli/)

This page walks through the "four things to decide before deployment" for first-time deployers.
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

## Recommended Methods

| Method | Best for |
| --- | --- |
| [Docker](/en/deployment/docker/) | NAS, single-machine, small-team, and existing container environments |
| [Docker follower node](/en/deployment/docker-follower/) | Attaching another AsterDrive instance directly as a Docker follower |
| [Follower node network topologies](/en/deployment/follower-network-topologies/) | Choosing between public HTTPS, Tailscale / VPN, Docker networks, and reverse tunnel |
| [Load Balancing and Multi-Instance Deployments](/en/deployment/load-balancing/) | Kubernetes, Ingress, multiple Primaries, and a shared data plane |
| [systemd](/en/deployment/systemd/) | Cloud hosts, physical machines, long-running stable services |
| Run the binary directly | Local testing and temporary validation |

## Confirm These Four Things Before Deployment

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
- in the single profile, creates the default local storage policy `Local Default` and seeds `Default Policy Group`
- in the cluster profile, skips both the local policy and policy group; when an administrator makes the first shared policy the default, AsterDrive atomically creates the default group and assigns unassigned administrators
- initializes default system setting entries
- starts mail dispatch, background task dispatch, periodic cleanup, and low-level file consistency check tasks

## Validate These After Launch

The full checklist is in [First-Start Checklist](/en/deployment/runtime-behavior/#check-these-items-immediately-after-startup).

At minimum, verify these after deployment:

1. `/health` and `/health/ready` return normally.
2. The home page opens normally and login works.
3. You can create a folder and upload a file.
4. The admin panel opens.

Validate other role-specific areas (WebDAV, WOPI, mail, trash, and so on) according to the corresponding sections in the [First-Start Checklist](/en/deployment/runtime-behavior/#check-these-items-immediately-after-startup).

## Where to Go Next

- Using Docker: see [Docker Deployment](/en/deployment/docker/)
- Running a remote follower node with Docker: see [Docker Follower Node Deployment](/en/deployment/docker-follower/)
- Unsure whether the follower should be public, inside Tailscale / VPN, or behind reverse tunnel: see [Follower Node Network Topologies](/en/deployment/follower-network-topologies/)
- Using systemd: see [systemd Deployment](/en/deployment/systemd/)
- Preparing backups, restore, and post-restore validation: see [Backup and Restore](/en/deployment/backup/)
- Want command-line deployment checks, offline configuration, or cross-database migration: see [Operations CLI](/en/deployment/ops-cli/)
- Preparing HTTPS: see [Reverse Proxy](/en/deployment/reverse-proxy/)
- Preparing Kubernetes, Ingress, or multiple Primaries: see [Load Balancing and Multi-Instance Deployments](/en/deployment/load-balancing/)
- Preparing Prometheus / Grafana: see [Monitoring and Grafana](/en/deployment/monitoring/)
- Estimating file count, database size, memory, and temporary disk: see [Capacity Planning](/en/deployment/capacity-planning/)
- Want to confirm exactly what first startup does automatically: see [First-Start Checklist](/en/deployment/runtime-behavior/)
- Preparing to upgrade: see [Upgrade and Version Migration](/en/deployment/upgrade/)
- Browser still shows the old UI after an upgrade: see [Frontend Asset Cache](/en/deployment/frontend-assets/)
- Want to establish or rerun performance baselines: see [Performance Benchmarking and Load Testing](/en/deployment/performance-benchmarking/)
