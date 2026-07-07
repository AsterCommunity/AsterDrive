---
description: AsterDrive files and workspaces feature map covering personal spaces, team spaces, folders, file records, trash, versions, and sharing.
---

# Files and Workspaces

Files and workspaces are the main business path in AsterDrive. This area connects the visible file tree with database records, blobs, permissions, quota, and share state.

## Capability Boundaries

| Capability | Notes | Related docs |
| --- | --- | --- |
| Personal space | User-owned files, folders, trash, tasks, quota | [User Manual](/en/guide/user-guide) |
| Team space | Team-owned files, member roles, team archive, team audit | [Teams and Permissions](/en/guide/teams-and-permissions) |
| Folder tree | Create, rename, move, copy, recursive delete, path building | [Common Workflows](/en/guide/core-workflows) |
| File records | Filename conflicts, versions, blob references, lock state, properties | [File Editing](/en/guide/editing), [Architecture Overview](/en/guide/architecture) |
| Trash | Delete, restore, purge, periodic cleanup | [User Manual](/en/guide/user-guide), [System Settings](/en/config/runtime) |
| Shares | File/folder shares, passwords, expiration, download limits, scope checks | [Sharing and Public Access](/en/guide/sharing) |
| Batch operations | Batch move, copy, delete, workspace boundary checks | [User Manual](/en/guide/user-guide) |

## Backend Modules

| Module | Owns |
| --- | --- |
| `workspace::scope`, `workspace::models` | Personal/team workspace scope |
| `file`, `folder` | Files, folders, paths, listings, access checks |
| `workspace::storage_core`, `workspace::storage` | File records, blobs, quota, storage-policy finalization |
| `team_service` | Teams, members, roles, archive |
| `share_service`, `share_public` routes | Share creation, public access, share scope |
| `trash_service`, `version_service`, `lock_service` | Trash, versions, file locks |
| `property_service` | File/folder extended properties |

## Data Boundaries

- File content is not stored directly in the database. The database stores files, folders, blobs, versions, shares, and permission relationships.
- Personal and team spaces share the same file path, but use different workspace scopes.
- Team-space files belong to the team and do not charge the operator's personal quota.
- Share scope must be checked against folder tree and file ownership, not only against the share ID.

## Troubleshooting Direction

- File list looks wrong: confirm the current workspace, trash state, and team archive state.
- Quota looks wrong: use [Operations CLI](/en/deployment/ops-cli) with `doctor --deep`.
- Share link access fails: check expiration, password, deletion state, and whether the file moved outside the share scope.
- WebDAV and web UI behavior differ: check WebDAV account scope, locks, and system-file blocking rules.
