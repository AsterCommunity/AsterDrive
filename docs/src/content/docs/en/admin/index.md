---
description: "AsterDrive admin console menu map: what Overview, Users, Teams, Storage Policies, Remote Nodes, External Auth, Policy Groups, Shares, Files, Tasks, Locks, System Settings, and Audit Logs each do, with links to the matching admin scenario pages."
title: "Admin Console"
---

:::tip[What this page covers]
This page is the **menu map** of the admin console: what each menu does, when to use it, and which scenario page goes deeper. Jump to the entry matching your task — no need to read end to end.
:::

The first successfully created account automatically becomes the administrator; see [First Start and the First Admin](/en/start/first-admin/).
After signing in, open `Admin` from the user menu in the top-right corner.

## Quick Entry Table

| You want to | Open first | Go deeper |
| --- | --- | --- |
| See site status, recent activity, task events | `Admin -> Overview` | [Overview](#overview) on this page |
| Create users, change roles or quotas, reset passwords or MFA | `Admin -> Users` | [Users and Teams](/en/admin/users-teams/) |
| Create, archive, or restore team spaces | `Admin -> Teams` | [Users and Teams](/en/admin/users-teams/) |
| Decide where files actually land, migrate storage data | `Admin -> Storage Policies` | [Storage Policies and Policy Groups](/en/admin/storage-policies/) |
| Connect a new storage backend | `Admin -> Storage Policies` | [Storage Backends](/en/admin/storage-backends/) |
| Attach another AsterDrive as a storage backend | `Admin -> Remote Nodes` | [Remote Nodes](/en/admin/follower-nodes/) |
| Route different users or teams to different storage | `Admin -> Policy Groups` | [Storage Policies and Policy Groups](/en/admin/storage-policies/#storage-policies-vs-policy-groups) |
| Add OIDC / OAuth2 / GitHub / QQ / Google / Microsoft sign-in | `Admin -> External Auth` | [Registration, Login and SSO](/en/admin/auth-sso/) |
| Set up activation, password-reset and other mail | `Admin -> System Settings -> Mail Delivery` | [Mail Delivery](/en/admin/mail/) |
| Configure link-import (offline download) engines | `Admin -> System Settings -> File Processing` | [Offline Download](/en/admin/offline-download/) |
| Integrate OnlyOffice / Collabora online preview and editing | `Admin -> System Settings -> Site -> Preview Apps` | [Preview and File Processing](/en/admin/preview-processing/) |
| Review share links, disable abusive shares | `Admin -> Shares` | [Shares](#shares) on this page |
| Inspect file records, Blob locations, version references | `Admin -> Files` / `Admin -> File Blobs` | [Files and File Blobs](#files-and-file-blobs) on this page |
| See why a background task failed | `Admin -> Tasks` | [Tasks](#tasks) on this page |
| Clean up stuck WebDAV / WOPI locks | `Admin -> Locks` | [Locks](#locks) on this page |
| Change registration, public site URL, recycle bin, and other rules | `Admin -> System Settings` | [System Settings](/en/reference/config/runtime/) |
| Replace or heavily customize the frontend | deployment-side `frontend-override/` | [Custom Frontend](/en/admin/custom-frontend/) |
| See who did what | `Admin -> Audit Logs` | [Audit Logs](#audit-logs) on this page |

## Our Deliberate Restraint

AsterDrive's admin console is **intentionally not "full-featured"**.

Our judgment:

- **Common things take a few clicks** — disabling a user, changing a quota, reading audit logs, toggling registration should not require reading docs first
- **Uncommon things go to the CLI** — database migrations, bulk config changes, disaster recovery live in the [operations CLI](/en/ops/cli/), not crammed into the web console
- **Dangerous things state their consequences** — force-deleting a user or emptying the recycle bin says exactly what will be deleted on the button, not hidden in a sub-menu dressed up as "advanced"

If a common action feels roundabout in the console, [tell us](https://github.com/AsterCommunity/AsterDrive/issues) — the iteration direction is **"the admin's daily actions get shorter"**, not "more features get stacked on".

## Overview

Come here first when checking overall site status.

You will see:

- Total users, enabled users, disabled users
- File count, total file size, total underlying file objects
- Share count
- Last 7 days trend
- Recent activity
- Recent background task events
- Daily summaries

Absolute times in the overview follow your configured display timezone; the 7-day trend and daily summaries aggregate in that timezone too.
If audit logging is turned off, trends and recent activity shrink noticeably or show as empty; background task events keep showing.

## Shares

The `Shares` page lists every public link on the site.

Common uses:

- A public link should stop being accessible
- A share is no longer needed
- Auditing what material is currently exposed publicly

Administrators can delete any share directly here. For the user-facing sharing workflow, see [Sharing and Collaboration](/en/using/sharing/).

## Files and File Blobs

`Files` and `File Blobs` are observability pages for troubleshooting storage problems, not a general file manager.

The `Files` page shows current state per file record. You can filter and view:

- File name, size, MIME type, and deletion state
- Owning user, owning team, current policy, and current Blob
- Current Blob's hash, storage path, and policy location
- This file's historical version references

The `File Blobs` page shows underlying objects. You can filter and view:

- Blob hash, size, policy ID, and storage path
- Reference count
- Which current files reference this Blob
- Which historical versions reference this Blob

These pages answer questions like:

- Which policy does a file actually live on right now
- After a storage migration, do file records already point at the target policy
- Does a Blob still have current-file or historical-version references
- When `doctor --deep` or logs report storage inconsistency, locate the exact file, Blob, and policy first

They do not replace backup, migration, and repair tools. When you see something abnormal, confirm the symptom and backup state before deciding whether to continue with the CLI or a background migration task.

## Tasks

The `Tasks` page lists recorded background tasks: system periodic tasks, personal-workspace tasks, and team tasks.

You will see:

- Task name, type, source, and status
- Current progress, last activity time, and error summary in the summary row
- Expanded step-by-step execution details, durations, checkpoints, and retry info
- Online archive, online extraction, batch download, and system runtime task records
- Storage policy data migration task records
- Thumbnail generation, media metadata parsing, and archive preview generation task records

This page is best for two things: confirming whether a background task is actually running, and seeing whether a batch of tasks has been failing lately.
You can also filter history by task type or status, and conditionally clean up finished task records. Cleanup only touches completed, failed, or canceled records; queued, running, and retrying tasks are never deleted.

`System Settings -> Runtime -> Task Retention` controls how long background-task temporary artifacts are kept, such as batch-download results, online-archive output, extraction staging, or link-import temp files. It does not delete task history from the list; history is cleaned up by the administrator on the Tasks page with conditions.

Storage migration tasks usually have longer step details. When troubleshooting, read the summary row first to decide whether it failed, then expand to see whether it stalled at pre-check, copy, verify, or commit. For what "Renamed Opaque Keys" means in migration results, see [Storage Policies and Policy Groups](/en/admin/storage-policies/#blob-matching-rules-during-migration).

## Locks

The `Locks` page handles stuck locks.

Most common scenarios:

- A file keeps showing as locked
- A WebDAV client exited abnormally without releasing its lock
- A WOPI editor closed abnormally and the file is still treated as being edited
- The administrator wants to clear a batch of expired locks first

You can view current lock paths, owners, and states, clean expired locks, or force-unlock a specific lock.

AsterDrive allows shared locks in the protocol sense, so one resource can legitimately hold several valid locks. Do not treat multiple locks as an error by itself; what needs handling is expired locks, locks left behind by clients that exited abnormally, or abnormal locks clearly blocking later operations. Folder moves, copies, and deletes also check subtree locks recursively, so a locked child cannot be bypassed by changing the parent directory.

## Audit Logs

The `Audit Logs` page shows key operation records.

Common uses:

- Find who deleted a file
- Find when a user signed in or modified content
- Troubleshoot share, lock, and team-management issues
- Check whether admin operations happened as expected

Primary node service starts and stops are also written as audit events, so you can trace when an instance came online or shut down.
Whether audit logs are recorded, which operations are covered, and how long they are kept are all controlled in system settings.

## About

The `About` page shows the deployed version, license, repository, and docs entries.
When answering "which version is actually running", look here first.

## Daily Admin Checklist

Confirm these items regularly:

1. `Public Site URL` still points at the real HTTP(S) origin; add each public entry individually
2. The default storage policy and default policy group are still usable
3. Policy groups bound to users and teams still match current usage
4. If followers are attached, the latest remote-node test state is healthy
5. Recycle bin, version history, task artifacts, and team archive retention still fit current capacity
6. Test mail still sends successfully
7. No share links are public that should not be
8. No background tasks have been failing or stuck for a long time
9. No locks have stayed unreleased for a long time
10. Audit logging is enabled with sufficient retention
