---
description: AsterDrive multi-instance runtime configuration synchronization, including Redis notifications, the authoritative database, config.toml, environment variables, CLI behavior, and failure boundaries.
title: "Configuration Synchronization"
---

:::tip[This page covers `[config_sync]`]
Keep it disabled for a single instance. Enable it only when multiple AsterDrive processes share one database and changes from system settings or `aster_drive config` must propagate promptly to the other instances.
:::

```toml
[config_sync]
backend = "disabled"
endpoint = ""
topic = "aster_drive.config_reload"
```

## What It Solves

When an administrator changes a system setting, the instance handling the write updates its own runtime configuration immediately. Other instances do not automatically know that the database changed.

With configuration synchronization enabled, the writer publishes a reload notification through Redis pub/sub. Other instances receive the notification and reload the full runtime configuration from the authoritative database.

```text
Admin API / config CLI
        │
        ├─ write shared database
        ├─ update local process snapshot
        └─ publish Redis reload notification
                    │
              other AsterDrive instances
                    │
                    └─ reload fully from shared database
```

Redis **does not store configuration values** and does not replace the database. Keys in a notification are used for observability and derived-cache invalidation; receivers still load authoritative values from the database.

## Keep It Disabled for One Instance

Single-host, NAS, and one-process deployments do not need Redis:

```toml
[config_sync]
backend = "disabled"
endpoint = ""
topic = "aster_drive.config_reload"
```

System settings and CLI writes continue to work when disabled; they simply do not emit cross-process notifications.

## Multi-Instance Configuration

Every instance must:

- connect to the same PostgreSQL, MySQL, or other shared authoritative database
- connect to the same Redis service
- use the same `topic`
- enable `[config_sync]`

```toml
[config_sync]
backend = "redis"
endpoint = "redis://127.0.0.1:6379/"
topic = "aster_drive.config_reload"
```

If Redis requires authentication or TLS, use the standard Redis URL form in `endpoint` and restrict access to the configuration file.

:::caution[SQLite is not suitable for multi-host instances]
Configuration synchronization does not replicate local SQLite files. Multiple instances must genuinely access one authoritative database. Do not place a separate SQLite file on every host and expect Redis to synchronize configuration values.
:::

## Options

| Option | Default | Purpose |
| --- | --- | --- |
| `backend` | `"disabled"` | `disabled` or `redis` |
| `endpoint` | `""` | Redis URL, used only with `backend = "redis"` |
| `topic` | `"aster_drive.config_reload"` | Product reload topic; all instances in a group must match |

Environment variables:

```bash
ASTER__CONFIG_SYNC__BACKEND=redis
ASTER__CONFIG_SYNC__ENDPOINT=redis://127.0.0.1:6379/
ASTER__CONFIG_SYNC__TOPIC=aster_drive.config_reload
```

When Redis configuration synchronization is enabled, AsterDrive derives a separate topic for cross-instance Storage SSE from the same instance configuration: `aster_drive.config_reload` maps to `aster_drive.storage_events`. Configuration reloads and file/folder change events do not share a Redis channel. Every instance must use the same `topic` to receive each other's events.

Restart the process after changing `[config_sync]`. It is static startup configuration and is not changed dynamically through system settings.

## Writes That Publish Notifications

These paths publish a reload notification after the database write succeeds:

- system-setting updates and deletes through the admin console or API
- `aster_drive config set`
- `aster_drive config delete`
- `aster_drive config import`

If one operation changes multiple dependent settings, one notification contains all changed keys. Startup migrations, default seeding, and startup configuration repairs do not publish notifications; each instance loads a full snapshot during startup.

## What Happens When Redis Fails

`[config_sync]` and `[cache]` have different failure behavior:

- when `[cache].backend = "redis"` cannot connect, cache can fall back to in-process memory
- when the backend, endpoint URL, or another `[config_sync].backend = "redis"` value is invalid and the notification backend cannot be constructed, the instance fails to start
- when the Redis URL is valid but the service is temporarily unreachable, the instance may finish startup; the subscription worker records the disconnect and reconnects automatically with backoff, so a process restart is not required
- if an admin API or CLI database write succeeds but notification publishing then fails, the command returns an error; the local value is already stored, while other instances may remain stale until restart or a later successful notification

During a runtime Redis outage, the subscription worker reports `disconnected` and `reconnecting`. Drive emits one `sync.required` event to each local SSE stream so the frontend can refresh from authoritative APIs. Once Redis is available again, the worker reports `recovered` and continues receiving new cross-instance events. Redis pub/sub does not replay the concrete events missed during the outage; the `sync.required` refresh covers that gap.

:::caution[Redis pub/sub does not replay history]
Redis pub/sub is not a durable message queue. Notifications missed while an instance is offline are not replayed. Every instance performs a full database load at startup, so restarting returns it to authoritative state.
:::

## Relationship to Cache Redis

`[cache]` and `[config_sync]` may use the same Redis service or different services/URLs, but solve different problems:

- `[cache]`: shared cache contents and TTLs
- `[config_sync]`: tells other processes to reload database-backed configuration

If a multi-instance deployment configures Redis cache without configuration synchronization, cache state may be shared while system-setting changes still take effect immediately only on the instance that handled the write.

## Deployment Verification

Start at least two instances connected to the same database and Redis, then:

1. Change a non-restart system setting on instance A, such as the site title.
2. Refresh the relevant page through instance B and confirm the change appears promptly.
3. Confirm logs do not repeatedly report `runtime config reload subscription stopped`.
4. Change a test custom configuration entry through the CLI and read it from another instance.
5. Pause Redis and verify the reconnect warning plus a `sync.required` SSE event; restore Redis and confirm the subscription recovers automatically and the next change synchronizes again.

Add Redis availability and topic consistency across instances to the [production launch checklist](/en/deployment/production-checklist/).

## Common Problems

### A change applies to only one instance

Check:

- every instance enables `backend = "redis"`
- `endpoint` is reachable from inside each host or container
- `topic` matches exactly
- every instance connects to the same database
- logs do not show persistent Redis errors without a later `recovered` observation

### Can I enable it only on primary?

Not recommended. Every instance serving reads, running background work, or reading runtime settings as a follower should use the same synchronization configuration.

### Does it synchronize static `config.toml`?

No. It only synchronizes database-backed runtime system settings. Listen addresses, database URLs, node mode, logging, WebDAV prefix, cache, and config sync itself must still be deployed to each instance and require restart after changes.
