---
description: AsterDrive deployment path selection, dispatching by trial, single instance, multi-instance, remote storage node, and Kubernetes to the matching scenario page, with pre-launch prerequisites.
title: "Choose a Deployment"
---

:::tip[This page only picks the path]
Full deployment steps live in the [Deployment](/en/deploy/) section. This page helps you pick one main path by scale and environment, then sends you there.
:::

For local, intranet, or temporary trials, skip the choosing and go straight to [Quick Start](/en/start/quick-trial/).

## Choose by Scale

| Your scale | Recommended path | Scenario page |
| --- | --- | --- |
| Local trial, temporary evaluation | Run the official image directly; plain HTTP is fine | [Quick Start](/en/start/quick-trial/) |
| Personal, family, or small-team single instance | Docker Compose or systemd | [Single-Instance Docker](/en/deploy/docker/), [Single-Instance systemd](/en/deploy/systemd/) |
| Many users, need multiple Primaries for traffic | cluster profile + load balancing | [Multi-Instance and Load Balancing](/en/deploy/multi-instance/) |
| Already have a Kubernetes platform | Reuse the repository's built-in manifests | [Kubernetes Deployment](/en/deploy/kubernetes/) |
| Want to attach another AsterDrive as remote storage | Keep the primary; run a follower on the new machine | [Follower Storage Node](/en/deploy/follower-node/) |

For a first deployment, prefer Docker. For long-term Linux servers, prefer systemd. Multiple Primaries are not "two copies of a single instance" — read the contract in [Multi-Instance and Load Balancing](/en/deploy/multi-instance/) first.

## Choose by Environment

| Environment | Also read |
| --- | --- |
| Any production public entry | [Reverse Proxy](/en/deploy/reverse-proxy/): HTTPS, upload size, WebDAV method passthrough |
| Follower on another network | [Follower Node Network Topologies](/en/deploy/follower-node/network/): public internet, VPN, Docker network, or reverse tunnel |
| Object storage (S3 / MinIO / R2 / COS / Azure / OneDrive / SFTP) | The matching tutorial in [Storage Backends](/en/admin/storage-backends/), connected after deployment |

## Confirm Before Launch

A production deployment is more than starting a container. Confirm these ahead of time:

- Data directory: `config.toml`, the database, and local upload directories must survive upgrades and restarts
- Access method: the public entry should provide HTTPS through a reverse proxy
- Public site URL: sharing, mail, WOPI, and cross-origin access all depend on it
- WebDAV: if Finder, Windows, rclone, or sync tools will connect, the proxy layer must allow the corresponding methods and upload sizes
- Storage location: different storage policy backends have different maintenance costs
- Backup and restore: confirm the backup and restore flow before launch instead of improvising after a failure

## After Deployment

- What startup completed automatically and what to validate immediately -> [First-Start Checklist](/en/ops/first-check/)
- The full pre-launch list -> [Production Launch Checklist](/en/ops/launch-checklist/)
- Backup and restore -> [Backup and Restore](/en/ops/backup/)
- Version upgrades -> [Upgrade and Version Migration](/en/ops/upgrade/)
