---
description: "AsterDrive recycle bin and version history: restore and permanent deletion after deleting, the empty-trash background task, and viewing, restoring, and truncation semantics of file versions."
title: "Recycle Bin and Versions"
---

:::tip[What this page covers]
Two kinds of "undo": accidentally deleted files live in the recycle bin; overwritten content lives in version history.
:::

## Recycle Bin

A normal delete does not immediately erase a file or folder — it goes to the recycle bin first.

In the recycle bin you can:

- Restore items
- Permanently delete single items
- Empty the entire bin
- Restore in batch
- Permanently delete in batch

If the original parent directory no longer exists, restored items return to the root directory.

Emptying the entire recycle bin creates a background task instead of blocking the page. After confirming, watch the `Recycle bin cleanup` task progress in the current workspace's `Task Center`; team-space cleanup tasks likewise appear only in that team space's task center.

The recycle bin follows the current workspace. Items deleted from your personal space are in the personal bin; items deleted from a team space are in that team's bin.

:::tip[How long items stay]
The recycle bin retention period is controlled by the administrator in system settings. Items past the retention period are cleaned up automatically and cannot be recovered.
:::

## Version History

Every overwrite write generates a historical version. Common sources:

- In-browser text editing
- WOPI online saves
- WebDAV overwrite saves
- Any other write that directly overwrites the original file content

In version history you can:

- View old versions
- Restore a specific version
- Delete versions you no longer need

:::caution[Restoring truncates later versions]
After restoring an old version, all newer versions after it are truncated together. Confirm before restoring.
:::

If a conflict prompt appears when saving, someone else changed the file after you opened it. Refresh the content first, then decide whether to continue saving.

The number of retained versions is controlled by the administrator. Versions count toward a file's "occupied space", visible in the file details; see [Files and Organization](/en/using/files/#what-the-details-panel-shows).
