# Cache

::: tip This page covers `[cache]`
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

## What Happens If Redis Is Unreachable

If `backend` is set to `redis` but Redis cannot be reached, AsterDrive will **automatically fall back to memory cache and continue running**.

The service usually will not fail to start just because Redis is temporarily unavailable, but cache will no longer be shared across instances.

## Environment Variables

```bash
ASTER__CACHE__BACKEND=memory
ASTER__CACHE__ENDPOINT=redis://127.0.0.1:6379/0
ASTER__CACHE__DEFAULT_TTL=3600
```
