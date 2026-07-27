---
description: "The WebDAV group of System Settings: the global switch and operating-system file blocking rules; the path prefix and hard upload limit live in config.toml."
title: "WebDAV (System Settings)"
---

Entry point: `Admin -> System Settings -> WebDAV`. For other groups and when changes take effect, see [System Settings](/en/reference/config/runtime/).

This group controls site-wide WebDAV runtime behavior:

- **`Enable WebDAV`**
- **`Block WebDAV System Files`**
- **`Blocked WebDAV System-File Patterns`**

After disabling it, desktop clients can no longer access files through WebDAV immediately.

By default, AsterDrive blocks WebDAV clients from creating common operating-system metadata files and folders, including `.DS_Store`, `._*`, `.Spotlight-V100`, `.Trashes`, `.fseventsd`, `Thumbs.db`, `desktop.ini`, `$RECYCLE.BIN`, and `System Volume Information`. These are usually written automatically by Finder, Windows Explorer, or sync tools, and most deployments do not want them scattered through the drive.

The patterns match basenames, ignore case, and support simple `*` wildcards. Disable this behavior or remove a pattern only when you explicitly need to sync those system files.

:::tip[The path prefix and hard upload limit are not here]
If you only want to change the WebDAV path prefix or the hard WebDAV upload size limit, that is not in System Settings. Change `[webdav]` in [`config.toml`](/en/reference/config/webdav/) instead, then restart.
:::
