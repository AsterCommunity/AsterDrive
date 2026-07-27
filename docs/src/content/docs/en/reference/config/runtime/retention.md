---
description: "The Storage and Retention group of System Settings: trash retention, version history limit, team archive retention, and the default storage quota for new users."
title: "Storage and Retention"
---

Entry point: `Admin -> System Settings -> Storage and Retention`. For other groups and when changes take effect, see [System Settings](/en/reference/config/runtime/).

This group decides "how long data is kept" and "how much space new users / new teams get by default". Default rules:

| Item | Default |
| --- | --- |
| Historical versions per file | `10` |
| Trash retention | `7` days |
| Team archive retention | `7` days |
| New user default storage quota | `0` (unlimited) |

:::caution[Default quotas affect only accounts and teams created later]

- The UI label for this item is `New User Default Storage Quota`
- When an administrator creates a team without entering an explicit quota, the team also uses this default value
- After creating a team, recheck the actual team quota under `Admin -> Teams`
- This setting **only affects accounts and teams created later**. Existing accounts or teams are not automatically rewritten.

:::
