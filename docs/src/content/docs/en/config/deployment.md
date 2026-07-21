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

Every primary instance must also:

- Connect to the same authoritative PostgreSQL or MySQL database
- Use the same Redis cache endpoint
- Use the same config-sync endpoint and topic
- Use storage reachable by every instance, such as S3, Azure Blob, OneDrive, or SFTP; a `local` policy is not shared storage
- If `reverse_tunnel` or an `auto` node with an empty `base_url` is used, configure a per-primary `internal_endpoint` and the same `internal_proxy_secret` on every primary

Reverse-tunnel connections, lanes, and pending requests remain process-local. In the cluster profile, a shared database owner directory stores the owner lease and fencing token, and an authenticated streaming proxy forwards requests from a non-owner primary to the owner. File bodies stay on the owner primary HTTP data plane; Redis and the database carry control state only.

`internal_endpoint` must be an absolute `http`/`https` URL reachable from every other primary, without a query or fragment. `internal_proxy_secret` must contain at least 32 characters and be identical on all primaries. Leaving both empty keeps a direct-only cluster topology; setting only one fails static configuration validation.

The `cluster` profile checks these combinations during startup, `/health/ready`, and `aster_drive doctor`. It declares deployment intent and blocks known incompatible topologies; load balancing and cross-instance storage SSE remain separate capabilities. Reverse-tunnel owner routing is enabled by the owner directory and proxy settings above.

A fresh cluster database does not seed a local default policy. Create a shared storage policy and make it the default through the admin console; `/health/ready` remains not ready until that step is complete.

Environment variable form:

```bash
ASTER__DEPLOYMENT__PROFILE=cluster
```

To reproduce the two-primary reverse-tunnel routing and failover acceptance tests locally, enable the dedicated test feature explicitly:

```bash
cargo test --features multi-primary-e2e --test test_multi_primary_e2e reverse_tunnel_ -- --ignored
```
