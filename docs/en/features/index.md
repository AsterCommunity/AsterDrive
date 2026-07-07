---
description: AsterDrive feature map organized by identity and access, files and workspaces, uploads and storage, preview and processing, and system operations.
---

# Feature Map

This section is organized by backend capability, not by user task.

If you are a regular user, start with the [User Manual](/en/guide/user-guide) and [Common Workflows](/en/guide/core-workflows). If you manage an instance, troubleshoot behavior, or build on top of AsterDrive, the feature map helps locate which capability owns which boundary and which documentation page to read next.

## Feature Areas

| Area | Owns | Main entry points |
| --- | --- | --- |
| [Identity and Access](./auth-access) | Login, sessions, MFA, Passkey, external authentication, WebDAV accounts, public access boundaries | [Login and Sessions](/en/config/auth), [External Authentication](/en/config/external-auth), [WebDAV](/en/config/webdav) |
| [Files and Workspaces](./files-workspaces) | Personal spaces, team spaces, folders, file records, trash, versions, shares | [User Manual](/en/guide/user-guide), [Teams and Permissions](/en/guide/teams-and-permissions), [Sharing and Public Access](/en/guide/sharing) |
| [Uploads and Storage](./upload-storage) | Upload modes, blobs, quota, storage policies, policy groups, local / S3 / Azure Blob / COS / OneDrive / follower storage | [Uploads and Large Files](/en/guide/upload-modes), [Storage Policies](/en/config/storage), [Storage Backends](/en/storage/) |
| [Preview and Processing](./preview-processing) | Thumbnails, media metadata, archive preview, WOPI, editing, share streaming | [Online Preview and WOPI](/en/guide/preview-and-wopi), [File Editing](/en/guide/editing), [System Settings](/en/config/runtime) |
| [System and Operations](./runtime-operations) | Startup config, runtime config, background tasks, mail, monitoring, audit, CLI, backup and upgrades | [Configuration Overview](/en/config/), [Deployment Overview](/en/deployment/), [Operations CLI](/en/deployment/ops-cli) |

## Backend Module Quick Reference

| Module | Area | Notes |
| --- | --- | --- |
| `auth_service`, `mfa_service`, `passkey_service`, `external_auth_service` | Identity and Access | Local login, security verification, external identity binding |
| `file`, `folder`, `team_service`, `share`, `files::trash`, `content::version` | Files and Workspaces | File chain, team spaces, shares, trash, versions |
| `upload_service`, `workspace::storage`, `storage_policy::policy`, `storage::*` | Uploads and Storage | Upload sessions, storage policy selection, blob writes, driver abstraction |
| `thumbnail`, `media::processing`, `media::metadata`, `files::archive::preview`, `preview::wopi` | Preview and Processing | Derived file results, online opening, preview capabilities |
| `config_service`, `task_service`, `mail::sender`, `audit_service`, `ops::health`, `readiness_service` | System and Operations | Hot config, background tasks, mail, audit, health checks |

## How to Use This Section

- To learn how users operate the product, go back to [User Guides](/en/guide/).
- To find where administrators configure behavior, read [Admin Console](/en/guide/admin-console) and [Configuration Overview](/en/config/).
- To locate backend ownership for a capability, start from the matching feature area here.
- For deployment and incidents, use [Deployment and Operations](/en/deployment/) and [Troubleshooting](/en/deployment/troubleshooting).
