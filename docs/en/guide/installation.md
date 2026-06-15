# Choosing a Deployment Method

::: tip This page only helps you choose a path
The complete deployment documentation now lives under [Deployment Overview](/en/deployment/). This page remains as a short entry point so old links and deployment links from the user guide land on the same selection page.
:::

For local use, LAN use, or a temporary trial, go straight to [Getting Started](./getting-started).  
For long-term operation, a domain name, HTTPS, backup, and upgrades, start from [Deployment Overview](/en/deployment/).

## Choose a Runtime First

| Method | Best for | Next step |
| --- | --- | --- |
| Docker | NAS devices, home servers, small teams, existing container environments | [Docker Deployment](/en/deployment/docker) |
| Docker follower node | Connecting another AsterDrive instance as a remote storage backend | [Docker Follower Node](/en/deployment/docker-follower) |
| systemd | Cloud servers, physical machines, long-term stable operation | [systemd Deployment](/en/deployment/systemd) |
| Direct binary run | Local tests and temporary validation | [Getting Started](./getting-started) |

For a first deployment, prefer Docker. For long-running Linux servers, prefer systemd.

## What to Confirm Before Launch

A production deployment is more than starting a container. Confirm these items first:

- Data directory: `config.toml`, the database, and the local upload directory must survive upgrades and restarts
- Access path: the public entry point should provide HTTPS through a reverse proxy
- Public site URL: shares, mail, WOPI, and cross-origin access all depend on it
- WebDAV: if Finder, Windows, rclone, or sync tools will use it, the proxy layer must pass the required methods and upload sizes
- Storage location: each storage policy backend has different maintenance costs
- Backup and restore: verify the backup and restore flow before launch, not after a failure

These topics are covered in order in [Deployment Overview](/en/deployment/). This page only keeps the selection path.

## Common Next Steps

- Want to get it running first: read [Getting Started](./getting-started)
- Want to launch for real: read [Deployment Overview](/en/deployment/)
- Want HTTPS: read [Reverse Proxy](/en/deployment/reverse-proxy)
- Want to confirm what startup completed automatically: read [First-Start Checklist](/en/deployment/runtime-behavior)
- Want command-line checks, offline configuration, or cross-database migration: read [Operations CLI](/en/deployment/ops-cli)
- Want backup and restore: read [Backup and Restore](/en/deployment/backup)
- Want to upgrade: read [Upgrades and Version Migration](/en/deployment/upgrade)
- Want a remote storage backend: start with [Follower Nodes](./remote-nodes)
