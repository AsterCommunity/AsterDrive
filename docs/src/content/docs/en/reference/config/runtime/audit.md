---
description: "The Audit Logs group of System Settings: the global switch, the recorded action scope, and retention; service start and shutdown are also recorded."
title: "Audit Logs"
---

Entry point: `Admin -> System Settings -> Audit Logs`. For other groups and when changes take effect, see [System Settings](/en/reference/config/runtime/).

This group decides whether admin and key operations leave records, and also lets you narrow the recorded action scope.

- **`Enable Audit Logs`**
- **`Recorded Audit Actions`**
- **`Audit Log Retention`**

:::caution[Do not disable casually]
If you want to later investigate "who created shares, who changed team members", keep it enabled.

The primary node's service startup and shutdown are also recorded as audit events, as `server_start` and `server_shutdown`.
:::

## File Activity Retention

`Workspace activity retention days` (`workspace_activity_retention_days`) controls workspace file/folder business history. Its default, `0`, retains all activity; positive values prune activity older than that number of days.

Required file activities commit atomically with business mutations and are independent of the audit switch and action selection above, which control system, administrative and security audit records. Audit log retention does not prune file activity. Pruning activity preserves current creator/modifier attribution, and old audit records are not converted into invented business history.
