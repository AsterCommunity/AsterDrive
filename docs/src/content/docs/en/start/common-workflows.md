---
description: AsterDrive common workflows quick reference, organized by daily scenario with authority-page entries for first checks, Office openers, storage routing, teams, sharing, resume, WebDAV, recycle bin, and lock handling.
title: "Common Workflows"
---

:::tip[How to use this page]
Find your scenario, read the summary, then jump to the authority page and follow it. Steps are not duplicated here — each procedure is maintained in exactly one place.
:::

## Deployment and Launch

| Scenario | In one sentence | Authority page |
| --- | --- | --- |
| First admin check after deployment | Walk the admin entries in order: Overview, Users, Teams, Storage Policies, Policy Groups, Tasks, System Settings, WebDAV | [First-Start Checklist](/en/ops/first-check/) |
| Acceptance before going live | Confirm data directories, HTTPS, public site URL, monitoring, backup, and rollback plans item by item | [Production Launch Checklist](/en/ops/launch-checklist/) |
| Offline checks from the command line | Run `aster_drive doctor` for a deployment health pass | [Operations CLI](/en/ops/cli/) |

## Storage and Uploads

| Scenario | In one sentence | Authority page |
| --- | --- | --- |
| Assign storage routes to users or teams | Create storage policies first, configure size-based routing rules in policy groups, then bind to users or teams | [Storage Policies and Groups](/en/admin/storage-policies/) |
| Prepare storage for a follower node | After enroll, you still need a default remote storage target, or uploads are rejected | [Follower Storage Node](/en/deploy/follower-node/) |
| Resume an interrupted large upload | Go back to the original folder and pick the same file again; an unexpired session resumes | [Upload and Download](/en/using/upload-download/) |
| Upload failure triage | Check workspace and policy group first, then size limits, proxy, CORS, and quota | [Upload and Download](/en/using/upload-download/) |

## Collaboration and Sharing

| Scenario | In one sentence | Authority page |
| --- | --- | --- |
| Create a team space | A system administrator creates it under `Admin -> Teams` with an initial admin and policy group | [Users and Teams](/en/admin/users-teams/) |
| Send files to someone | Switch to the right workspace, create a share, set password, expiry, and download limits as needed | [Sharing and Public Access](/en/using/sharing/) |
| Add online openers for Office files | Set the public site URL correctly, then import WOPI Discovery in preview apps | [Preview and File Processing](/en/admin/preview-processing/) |
| Create a WebDAV account for a device | One account per device; disable individually when lost | [Using WebDAV](/en/using/webdav/) |

## When Something Goes Wrong

| Scenario | In one sentence | Authority page |
| --- | --- | --- |
| Handle accidental deletion | Restore from the recycle bin first; emptying it is a background task | [Recycle Bin and Versions](/en/using/recycle-bin/) |
| A file stays locked | Let the editor save and exit normally; admins clean stale locks under `Admin -> Locks` | [Preview and Editing](/en/using/preview-editing/) |
| Thumbnails not as expected | Check failed tasks, processor toggles, extension bindings, test commands, and source size limits in order | [Preview and File Processing](/en/admin/preview-processing/) |
| Other symptoms | Locate by symptom index | [Troubleshooting](/en/ops/troubleshooting/) |
