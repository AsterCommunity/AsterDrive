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
If you want to later investigate "who deleted files, who created shares, who changed team members", keep it enabled.

The primary node's service startup and shutdown are also recorded as audit events, as `server_start` and `server_shutdown`.
:::
