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
- Use `direct` transport for remote nodes

The reverse tunnel registry currently belongs to one primary process. Therefore, `reverse_tunnel` nodes and `auto` nodes with an empty `base_url` belong to the `single` profile until cross-primary tunnel owner routing is implemented.

The `cluster` profile checks these combinations during startup, `/health/ready`, and `aster_drive doctor`. It declares deployment intent and blocks known incompatible topologies; load balancing, cross-instance storage SSE, and reverse tunnel owner routing remain separate capabilities.

A fresh cluster database does not seed a local default policy. Create a shared storage policy and make it the default through the admin console; `/health/ready` remains not ready until that step is complete.

Environment variable form:

```bash
ASTER__DEPLOYMENT__PROFILE=cluster
```
