---
title: "Deployment Profile"
description: AsterDrive single and cluster deployment profiles, shared dependencies, and topology checks.
---

`[deployment]` declares whether the instance belongs to a single-primary or multi-primary deployment. The default keeps the single-instance setup lightweight:

```toml
[deployment]
profile = "single"
```

Supported values:

| Value | Purpose |
| --- | --- |
| `single` | Default profile for SQLite, memory cache, local storage, and single-primary reverse tunnels |
| `cluster` | Multi-primary deployment with shared-dependency and topology compatibility checks |

## Cluster Prerequisites

```toml
[deployment]
profile = "cluster"
internal_endpoint = "http://primary-a:3000"
internal_proxy_secret = "replace-with-at-least-32-random-characters"

[database]
url = "postgres://aster:password@postgres/asterdrive"

[cache]
backend = "redis"
endpoint = "redis://redis:6379/0"

[config_sync]
backend = "redis"
endpoint = "redis://redis:6379/0"
topic = "aster_drive.config_reload"
```

`internal_endpoint` must be an absolute `http`/`https` URL directly reachable from every other Primary and uniquely identify this instance, without a query or fragment. Do not use the shared load-balancer address. `internal_proxy_secret` must contain at least 32 characters and be identical on every Primary. Leaving both empty creates a direct-only cluster; setting only one fails static configuration validation.

The `cluster` profile checks the shared database, Redis, and storage topology during startup, `/health/ready`, and `aster_drive doctor`. A fresh database creates no storage policy in either single or cluster; both enter `needs_storage` after administrator creation, but cluster requires an administrator to create storage reachable by every Primary and make it the default.

See [Load Balancing and Multi-Instance Deployments](/en/deploy/multi-instance/) for the authoritative shared-dependency, static-secret, upload and SFTP limit, reverse-tunnel owner routing, health-check, migration-lock, task-lease, and Ingress contract. See [Uploads and Large Files](/en/using/upload-download/#choosing-uploads-for-a-cluster-deployment) for upload-mode selection.

Environment variable form:

```bash
ASTER__DEPLOYMENT__PROFILE=cluster
```

To reproduce the two-primary reverse-tunnel routing and failover acceptance tests locally, enable the dedicated test feature explicitly:

```bash
cargo test --features multi-primary-e2e --test multi_primary cluster::reverse_tunnel_ -- --ignored
```
