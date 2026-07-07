---
description: AsterDrive system and operations feature map covering startup config, runtime system settings, background tasks, mail, monitoring, audit, CLI, backup, upgrades, and troubleshooting.
---

# System and Operations

System and operations keep the service stable, configurable, observable, and recoverable. This area spans startup configuration, database-backed runtime settings, background tasks, health checks, logs, mail, audit, and CLI operations.

## Capability Boundaries

| Capability | Notes | Related docs |
| --- | --- | --- |
| Startup configuration | Listen address, database, logging, WebDAV prefix, node mode, rate limiting, cache | [Configuration Overview](/en/config/), [Server](/en/config/server), [Database](/en/config/database) |
| Runtime system settings | Admin-console hot settings for site, registration, mail, WOPI, trash, task retention | [System Settings](/en/config/runtime) |
| Background tasks | Thumbnails, archive jobs, migration, offline download, cleanup, periodic jobs, retries | [System Settings](/en/config/runtime), [Operations CLI](/en/deployment/ops-cli) |
| Mail | SMTP, templates, outbox, test mail | [Mail](/en/config/mail) |
| Monitoring | Health, readiness, Prometheus metrics, Grafana dashboard | [Monitoring and Grafana](/en/deployment/monitoring) |
| Audit | Admin operations, team audit, filterable audit logs | [Admin Console](/en/guide/admin-console) |
| CLI | doctor, offline config, node enroll, cross-database migration | [Operations CLI](/en/deployment/ops-cli) |
| Backup and upgrades | Database, config, local upload directories, version upgrades, rollback | [Backup and Restore](/en/deployment/backup), [Upgrade and Version Migration](/en/deployment/upgrade) |

## Backend Modules

| Module | Owns |
| --- | --- |
| `config::loader`, `config_service` | Static configuration loading and runtime system settings |
| `runtime::startup`, `runtime::tasks` | Primary/follower startup and periodic background tasks |
| `task_service` | User-visible background tasks, scheduling, retries, cleanup |
| `mail::sender`, `mail::outbox`, `mail::template` | Mail delivery and templates |
| `ops::health`, `readiness_service`, `metrics` | Health checks, readiness, metrics |
| `audit_service` | Audit log recording, querying, presentation |
| `cli::*` | Offline operations commands |

## Configuration Boundaries

- Anything required before startup belongs in `config.toml` or `ASTER__...` environment variables.
- Administrator-adjustable runtime behavior belongs in the database-backed `system_config` table.
- System default definitions are centralized in `src/config/definitions.rs`.
- New user-visible background tasks should use `task_service::create_task_record()` and wake the dispatcher.

## Troubleshooting Direction

- Service does not start: check config path, database connection, occupied ports, directory permissions.
- Readiness fails: check migrations, default policies, node mode, follower binding state.
- Background tasks pile up: check dispatcher state, retention settings, failure reasons, retry count.
- Admin console is unavailable but config must change: use the `config` subcommand in [Operations CLI](/en/deployment/ops-cli).
