---
title: "Cache"
---

:::tip[This page covers `[cache]`]
A single-node deployment can keep the default memory cache. Only consider Redis for multi-instance deployments that need shared cache state.
Not sure whether to introduce Redis? Most single-node deployments do not need it.
:::

```toml
[cache]
backend = "memory"
endpoint = ""
default_ttl = 3600
```

## Keep the Default for Most Deployments

For single-node, NAS, personal, or small-team deployments, the memory cache is enough. **Redis is worth adding only in these two cases**:

- Multi-instance deployments
- Multiple application instances need to share cache data

## Options

| Option | Default | Purpose |
| --- | --- | --- |
| `backend` | `"memory"` | `memory` or `redis` |
| `endpoint` | `""` | Redis connection URL, used only when `backend = "redis"` |
| `default_ttl` | `3600` | Default TTL in seconds |

## Redis Authentication

Existing complete URL strings remain compatible:

```toml
endpoint = "redis://encoded-user:encoded-password@cache.internal:6379/0"
```

When the username or password contains reserved characters, prefer raw structured credentials:

```toml
endpoint = { base_url = "redis://cache.internal:6379/0", username = "RAW_USERNAME", password = "RAW_PASSWORD" }
```

For Redis deployments with a password but no ACL username, use `username = ""`. Do not pre-encode raw credentials, and do not include userinfo in `base_url`; AsterDrive and Forge omit username/password from Debug and configuration serialization. Restrict production configuration permissions, and on Kubernetes prefer mounting the complete configuration from a Secret.

Equivalent structured environment variables:

```bash
ASTER__CACHE__ENDPOINT__BASE_URL=redis://cache.internal:6379/0
ASTER__CACHE__ENDPOINT__USERNAME=
ASTER__CACHE__ENDPOINT__PASSWORD=RAW_PASSWORD
```

## What Happens If Redis Is Unreachable

If `backend` is set to `redis` but Redis is unreachable during startup, AsterDrive asks the Forge cache constructor to use the `ReturnError` policy, returns the connection error, and exits. This keeps both single and cluster from silently using process-local cache when operators configured Redis, and lets the orchestrator retry after Redis recovers.

After a Redis backend has been established successfully, a temporary runtime outage enters a controlled fallback/circuit state: `/health` still reports process liveness while `/health/ready` fails. The backend returns to ready after the connection recovers.

## Environment Variables

```bash
ASTER__CACHE__BACKEND=memory
ASTER__CACHE__ENDPOINT=redis://127.0.0.1:6379/0
ASTER__CACHE__DEFAULT_TTL=3600
```
