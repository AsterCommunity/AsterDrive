---
title: "Admin Console"
---

This page covers the daily actions administrators can perform in the admin console: user and team management, storage policies and policy groups, follower-node enrollment, file and blob observability, background tasks, manual intervention for shares and locks, system settings, audit logs, and version information.  
Jump to the section for what you need to do; you do not have to read it all.

The first successfully created account automatically becomes an administrator.  
After logging in, administrators can enter `Admin` from the user menu in the top-right corner.

## Admin Entry Quick Reference

| What you want to do | Open first | Continue reading |
| --- | --- | --- |
| View site-wide status, recent activity, task events | `Admin -> Overview` | This page: [Overview](#overview) |
| Create users, change roles, change quotas, disable accounts | `Admin -> Users` | This page: [Users](#users) |
| Create teams, archive or restore team spaces | `Admin -> Teams` | This page: [Teams](#teams) |
| Decide where files are actually stored | `Admin -> Storage Policies` | [Storage Policies](/en/config/storage/) |
| Connect another AsterDrive instance as a storage backend | `Admin -> Follower Nodes` | [Follower Nodes](./remote-nodes/) |
| Connect OIDC / Generic OAuth2 / GitHub / QQ / Google / Microsoft login | `Admin -> External Auth` | This page: [External Auth](#external-auth) |
| A user lost authenticator and recovery codes | `Admin -> Users -> User Details` | This page: [Users](#users) |
| Route different users or teams to different storage paths | `Admin -> Policy Groups` | [Storage Policies](/en/config/storage/#storage-policies-vs-policy-groups) |
| Inspect share links or stop abnormal shares | `Admin -> Shares` | This page: [Shares](#shares) |
| Inspect file records, blob locations, and version references | `Admin -> Files` / `Admin -> File Blob` | This page: [Files and File Blobs](#files-and-file-blobs) |
| See why background tasks failed | `Admin -> Tasks` | This page: [Tasks](#tasks) |
| Clean abnormal WebDAV / WOPI locks | `Admin -> Locks` | This page: [Locks](#locks) |
| Change registration, mail, public site URL, WOPI, trash | `Admin -> System Settings` | [System Settings](/en/config/runtime/) |
| Check who did what | `Admin -> Audit Logs` | This page: [Audit Logs](#audit-logs) |

## Our Restraint in the Admin Console

AsterDrive's admin console is intentionally not "fully featured".

Our judgment is:

- **Common things should take only a few clicks**: disabling a user, changing quota, checking audit, or closing registration should not require reading documentation first
- **Uncommon things belong in CLI**: database migration, batch configuration changes, and disaster recovery should go to [Operations CLI](/en/deployment/ops-cli/), not be crammed into the web admin console
- **Dangerous things must state consequences**: for force-delete-user or empty-trash actions, the button should say what will be deleted instead of hiding it behind an "advanced" menu

If a common action in the admin console feels unnecessarily roundabout, [tell us](https://github.com/AsterCommunity/AsterDrive/issues). The direction of admin-console iteration is **"shorter daily admin actions"**, not "more and more stacked features".

## What Is in the Admin Console

The current left-side admin menu includes:

- Overview
- Users
- Teams
- Storage Policies
- Follower Nodes
- External Auth
- Policy Groups
- Shares
- Files
- File Blob
- Tasks
- Locks
- System Settings
- Audit Logs
- About

## Overview

Enter here when you want to see current site-wide status first.

You will see:

- Total users, enabled users, disabled users
- File count, total file size, underlying blob count
- Share count
- Last 7 days trend
- Recent activity
- Recent background task events
- Daily summaries

Absolute times in the overview follow your currently configured display time zone. The last 7 days trend and daily summaries are also aggregated by this time zone.  
If audit logs are disabled, trends and recent activity become much smaller or even empty; background task events continue to show.

## Users

The `Users` page handles daily account-level management.

You can:

- Create users
- Search and filter users
- Adjust roles and enabled status
- Modify total quota
- Bind a policy group to users
- Open user details for more operations

In user details, you can also:

- Reset the user's login password
- Require the user to change their password after the next login
- Reset the user's MFA
- Force all current devices for this user to log in again
- View current space usage and quota

The system protects the initial administrator account to avoid accidentally disabling, demoting, or deleting the only administrator.

When an administrator enables `Force password change`, AsterDrive invalidates the user's existing login sessions. After the next successful password, MFA, passkey, or external-auth login, the user can only enter the forced password-change screen. They must enter the current temporary password and set a new password before accessing files, teams, the admin console, or other normal app areas. After the password is changed, the requirement is cleared automatically and audit logs record the event.

Resetting MFA applies when a user loses their authenticator and recovery codes. This clears the user's authenticator, recovery codes, and unfinished MFA login flows, and invalidates the user's current sessions. The user must bind an authenticator again after the next login.

## Teams

The `Teams` page handles creation, archiving, restoration, and global maintenance of team workspaces.

You can:

- Create teams
- Choose the initial team administrator
- Bind a usable policy group to a team
- View member count, space usage, and archive status
- Open team details to inspect members and team audit

After a team is created:

- System administrators continue global maintenance here
- Administrators and owners inside the team continue team-internal management from `Settings -> Teams`

## Storage Policies

Storage policies decide two things:

- Where files actually land
- Which method writes files during upload

The current admin console supports these policy types:

- `local`: local directory
- `s3`: S3 or compatible object storage
- `azure_blob`: Azure Blob Storage containers through the Azure Blob SDK and SAS URLs
- `tencent_cos`: Tencent COS; base object operations reuse S3-compatible behavior, with additional Tencent-native capabilities such as COS CI
- `one_drive`: Microsoft Graph-accessible OneDrive, SharePoint, or Microsoft 365 group drives
- `sftp`: server-side streaming reads and writes to an SSH/SFTP file server
- `remote`: bound to a follower node, where another AsterDrive follower handles real object reads and writes

Here you can:

- Create and edit policies
- Test connections
- Set the system default policy
- Control the single-file size limit
- Control chunk size
- Choose upload and download modes for S3 / Azure Blob / COS, such as `relay_stream` or `presigned`
- Save Microsoft Graph app credentials for OneDrive, start authorization or reauthorization, and choose server relay or Microsoft Graph browser-direct upload/download independently
- Fill the endpoint, SSH username, password, and remote root for SFTP, then confirm the server host-key fingerprint after the first connection test
- Control path-style access for generic S3 policies, matching endpoint behavior across MinIO, RustFS, R2, AWS S3, and other providers
- When conditions are safe, promote a Tencent COS policy that was originally created as generic `s3` to `tencent_cos`
- Create a storage policy data migration task that copies existing objects from a source policy to a target policy

When editing an existing policy, the left side shows the current capacity observation. `local` policies read total, available, and used bytes from the underlying filesystem; `one_drive` policies read Microsoft Graph drive quota; `remote` policies ask the follower for the capacity of the remote storage target bound to the policy; `s3` / `tencent_cos` / `azure_blob` / `sftp` do not expose a standardized, reliable free-capacity API, so they are shown as unsupported instead of using guessed values.

`Path-style access` on a generic `s3` policy controls the request URL shape. Compatible services such as MinIO and RustFS usually need it enabled; services that support virtual-hosted style, such as AWS S3, can usually leave it disabled. Test the connection before saving instead of guessing only from the provider name.

If the admin console detects that a generic `s3` policy points to Tencent COS, the edit page can suggest driver promotion. Promotion only changes the driver AsterDrive uses for that policy, enabling COS endpoint normalization, signing, and later COS CI support. It does not move objects in the bucket. AsterDrive checks the allowed promotion direction, active upload sessions, and bucket immutability before switching.

The OneDrive authorization button uses only the saved Microsoft Graph application configuration. After changing Client ID, Client Secret, tenant, target drive type, or location fields, save the policy before clicking `Authorize` or `Reauthorize`. This keeps the backend authorization flow, audit logs, token refresh, and later background tasks on the same configuration.

When a connection test fails, the admin console prefers the backend diagnostic. That diagnostic is returned in the standard error response as `error.diagnostic.message`, with secrets and tokens redacted. It is useful for administrators, but should not be treated as a stable script branch.

Before creating a migration task, `Migrate Data` runs a preflight check. The plan shows source object count, source bytes, estimated objects to copy, target objects already present, capacity check result, and opaque key conflict count. Capacity only blocks task creation when the target is confirmed to be insufficient. If the target driver does not support capacity observation or the check is temporarily unavailable, the UI shows a warning but still allows the migration.

For policies already used by files, do not directly modify options that decide the real storage location, such as `base_path`, `bucket`, `endpoint`, Azure container, OneDrive drive / root item / site / group location fields, or the bound follower node. To move locations, create the target policy first, use `Migrate Data` in the page to run preflight checks and create a background migration task, then switch policy groups after completion is confirmed.

## Follower Nodes

The `Follower Nodes` page registers follower nodes, generates one-time enroll commands, and later tests connectivity from the primary to the follower.

You can:

- Create remote node records
- Fill name, transport mode, and optional `base_url`
- Generate the enroll command the follower needs to run after saving
- View last test time, capability summary, and errors
- Enable, disable, edit, or delete nodes
- Create and maintain follower remote storage targets in node details

Notes:

- Direct mode needs `base_url`; reverse tunnel can leave it empty; in auto mode, empty means reverse tunnel
- Reverse tunnel is still under test and is suitable for `relay_stream`; remote `presigned` still needs direct transport and a follower `base_url` reachable by browsers
- Remote storage targets are pushed from the primary node to the follower; currently `local` and `s3` are supported
- A `local` remote storage target only accepts a relative path, and it ultimately lands under the follower's `server.follower.remote_storage_target_local_root`
- Without an applied default remote storage target, remote writes are rejected
- Before deleting a node, rebind any remote storage policies that reference it
- For the detailed flow, see [Follower Nodes](./remote-nodes/)

## External Auth

The `External Auth` page manages external identity providers shown on the login page. AsterDrive supports OpenID Connect, Generic OAuth2, and dedicated providers for GitHub, QQ, Google, and Microsoft; see [External Authentication](/en/config/external-auth/) for the full setup guide.

You can:

- Create external authentication providers
- Choose OpenID Connect, Generic OAuth2, GitHub, QQ, Google, or Microsoft
- Fill display name, icon, Issuer URL, Client ID, and optional Client Secret
- Manually fill Authorization, Token, and UserInfo endpoints for Generic OAuth2; dedicated providers use fixed endpoints or fixed discovery
- Copy the redirect URI generated by AsterDrive and register it with the identity provider
- Validate provider configuration; OIDC loads discovery/JWKS, while Generic OAuth2 only checks format and required fields without probing authorization/token/userinfo endpoints
- Restrict allowed email domains
- Adjust claim mapping, such as username, display name, email, and email verification status
- Control whether the identity provider must return a verified email
- Control whether verified emails may auto-bind to local accounts
- Control whether local regular users may be created automatically
- Enable, disable, edit, or delete providers

The default policy is conservative: external identities are identified first by provider identity namespace and subject; email auto-binding must be enabled explicitly. Deleting a provider deletes the corresponding external identity bindings, but does not delete existing local users.

:::caution[Configure public site URL first]
External authentication redirect URIs depend on `Admin -> System Settings -> Site Configuration -> Public site URL`. If this is not set correctly, identity-provider callbacks land on the wrong address.
:::

If an external identity cannot directly match a local account, the user may go through login-and-bind or email verification. Email verification depends on the external-login email verification template under `Admin -> System Settings -> Mail Delivery`.

## Policy Groups

Policy groups decide "which storage policy a user or team should hit when uploading".

The most common patterns:

- The default policy group has one rule, and all files use the default local policy
- When using local and S3 together, split into multiple rules by file size
- Different users or teams bind to different policy groups
- Set one policy group as the default for new users

If you need to change upload routes, the usual order is:

1. Prepare storage policies first
2. Configure policy group rules
3. Finally bind users or teams to the corresponding policy group

Policy groups can be disabled first. After disabling, they can no longer be assigned to new users or teams.
If you need to delete a policy group that is still bound to users or teams, first use the page's "migrate assignments" action to batch move user and team bindings to another group, then delete it.

## Shares

The `Shares` page lists all public links across the site.

Common uses:

- A public link should no longer be accessible
- A share is no longer needed
- You want to check which materials are still public

Administrators can delete any share directly here.

## Files and File Blobs

`Files` and `File Blob` are observability pages for administrators who need to troubleshoot storage. They are not a regular file manager.

The `Files` page shows file records. You can filter and inspect:

- file name, size, MIME type, and deletion state
- owner user, owning team, current policy, and current blob
- the current blob hash, storage path, and policy location
- version references for that file

The `File Blob` page shows underlying objects. You can filter and inspect:

- blob hash, size, policy ID, and storage path
- reference count
- which current files reference this blob
- which historical versions reference this blob

These pages are useful when you need to answer questions such as:

- which policy a file currently lands on
- whether file records point to the target policy after storage migration
- whether a blob is still referenced by current files or historical versions
- which file, blob, and policy to inspect after `doctor --deep` or logs report storage inconsistency

They do not replace backups, migration, or repair tools. If you see an anomaly, first confirm the symptom and backup state, then decide whether to continue with the CLI or a background migration task.

## Tasks

The `Tasks` page lists recorded background tasks in the system, including system periodic tasks, personal workspace tasks, and team tasks.

You will see:

- Task name, type, source, and status
- current progress, recent activity time, and error summary in the summary row
- expanded step details, elapsed time, checkpoints, and retry information
- Online compression, online extraction, package download, and system runtime task records
- storage policy data migration task records
- Thumbnail generation task records
- Media metadata extraction task records
- Archive preview generation task records

This page is best for:

- Confirming whether a background task is actually running
- Checking whether a group of tasks has been continuously failing recently
- Filtering historical records by task type or status
- Conditionally cleaning finished historical task records

Cleaning historical tasks only handles completed, failed, or canceled records. Queued, processing, and retrying tasks are not deleted.

`System Settings -> Runtime -> Task Retention` controls how long temporary task artifacts are kept, such as package-download results, online-compression output, online-extraction staging files, or link-import temporary files. It does not automatically remove historical records from the task list; administrators clean those records conditionally from the Tasks page.

For storage policy data migration tasks, the expanded details are usually more important. Start with the summary row to see whether the task failed, then expand it to check whether it stopped during preflight, copy, verification, or commit.

After a migration succeeds, task details show migrated objects, skipped objects, failed objects, migrated bytes, and renamed opaque keys. Renamed opaque keys mean that the source policy had opaque blob keys that already existed in the target policy. AsterDrive did not merge them across policies; it copied the source object to a new key under the target policy to avoid overwriting or incorrectly reusing the existing target object.

## Locks

The `Locks` page is for stuck locks.

The most common scenarios:

- A file keeps showing as locked
- A WebDAV client exited abnormally without releasing the lock
- A WOPI editor closed abnormally and the file is still considered being edited
- An administrator wants to clear a batch of expired locks

You can:

- View current lock path, holder, and status
- Clean expired locks
- Force-unlock one lock

AsterDrive allows shared locks where the protocol semantics allow them, so the same resource may have multiple valid locks at once. Do not treat multiple locks as an error by itself. The records that usually need intervention are expired locks, locks left behind by crashed clients, or abnormal locks that clearly block later operations. Folder move, copy, and delete operations also check locks recursively in the subtree so clients cannot bypass child locks by modifying a parent directory.

## System Settings

`System Settings` maintains site-wide runtime rules.  
The current page shows these groups:

- Site Configuration
- User Management
- Authentication and Cookies
- Mail Delivery
- Network Access
- Runtime
- Storage and Retention
- File Processing
- WebDAV
- Audit Logs
- Custom Configuration
- Other

Commonly changed items include:

- Public site URL
- Titles, logo, and favicon for login page, share page, and main UI
- Whether public registration is allowed
- Whether email activation is required after registration
- Whether browser cookies must be sent through HTTPS
- Access / Refresh Token lifetimes
- Expiration for registration activation, email-change, and password-reset links
- Email-code MFA switch, TOTP fallback policy, code TTL, and resend cooldown
- MFA encryption key is not changed here; it belongs to `[auth].mfa_secret_key` in `config.toml`
- WebDAV switch, system-file blocking, and blocking rules
- Trash retention, version count, and team archive retention
- Default quota for new users
- Temporary task artifact retention
- Thumbnail source file size limit
- Online extraction source, staging, uncompressed size, entry count, path, and compression-ratio limits
- Global switch for online archive compression
- Independent ZIP archive-download switches for signed-in users and public shares
- Entry count, total source size, and output size limits for online compression and archive downloads
- Archive preview switches and limits
- Media processors, vips / ffmpeg / ffprobe commands, and extension bindings
- Mail queue, background task, and periodic cleanup frequency
- Background task lane concurrency limits, maximum attempts, and system health check interval
- Storage policy migration task concurrency limit; inspect concrete migration plans, checkpoints, and results under `Admin -> Storage Policies` and `Admin -> Tasks`
- Share streaming session TTL
- Whether to record audit logs
- Preview apps, and TTLs related to online Office / WOPI open methods
- Gravatar avatar URL
- CORS origin settings

For actual quota after creating a team space, verify it again on the `Teams` page.

If you plan to enable public registration, password recovery, email change, or email-code MFA, configure the `Mail Delivery` group first.  
If you plan to enable external authentication, configure `Public site URL` correctly first and copy the redirect URI from `Admin -> External Auth` to the identity provider.
If you plan to connect online preview or online editing such as OnlyOffice, focus on:

- `Site Configuration -> Public site URL`
- `Site Configuration -> Preview Apps`
- Callback network from the external Office / WOPI service to AsterDrive

In `Preview Apps`, you can enable, disable, and sort open methods directly, or import a group of WOPI apps through `WOPI Discovery`.  
If the browser console clearly reports an AsterDrive API CORS error, add the corresponding origin to allowed CORS origins under `Network Access`.

## Audit Logs

The `Audit Logs` page shows records of important operations.

Common uses:

- Find who deleted a file
- Find when a user logged in or changed content
- Diagnose share, lock, and team-management issues
- Check whether administrator operations happened as expected

The primary node's service startup and shutdown are also written as audit events, which makes it easier to trace when the instance came up or exited.

Whether audit logs are recorded, which actions are recorded, and how long they are retained are controlled by system settings.

## About

The `About` page shows the current deployment version, license, repository, and documentation entry points.  
When diagnosing "which version is actually running now", start here.

## Administrator Routine Checklist

Confirm these items regularly:

1. Whether `Public site URL` still points to real HTTP(S) origins; add each public entry point separately
2. Whether the default storage policy and default policy group are still usable
3. Whether policy groups bound to users and teams match current usage
4. If follower nodes are connected, whether recent remote-node test status is normal
5. Whether trash, version history, task artifacts, and team archive retention match current capacity
6. Whether test mail can still be sent normally
7. Whether there are share links that should no longer be public
8. Whether there are long-failing or stuck background tasks
9. Whether there are long-unreleased locks
10. Whether audit logs are enabled and retained long enough
