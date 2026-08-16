---
description: "AsterDrive first-start checklist: health checks, default policy, login, upload and download, WebDAV, WOPI, and per-role items to validate immediately after startup."
title: "First-Start Checklist"
---

:::tip[What this page covers]
What to check immediately after deployment, what AsterDrive automatically does on first startup, and how to complete administrator and default-storage setup.
Run through the checklist in [Check These Items Immediately After Startup](#check-these-items-immediately-after-startup) to confirm the service is really ready.
:::

After AsterDrive starts successfully for the first time, it automatically completes a set of basic preparation tasks.  
If you just deployed it, the most practical approach is to confirm the service is really ready using the checklist below.

## What Happens Automatically After First Successful Startup

- If the current working directory does not have `data/config.toml`, generate a default config automatically.
- Connect to the database and update the database structure automatically.
- Neither single nor cluster creates a storage policy automatically; both use the same `needs_admin -> needs_storage -> ready` state machine.
- After the administrator makes the first permitted policy the default, atomically create or reconcile the default policy group and assign administrators that still have no group.
- Initialize built-in default entries for admin system settings.
- Start mail dispatch, background task dispatch, periodic cleanup, and low-level file consistency check tasks.

In single, the administrator may create a `local` policy, for example with the absolute path `/data/uploads`. Cluster must instead use a data plane reachable by every Primary, such as object storage, SFTP, or a remote Follower. Both profiles use the same setup API, policy creation, default-group backfill, and readiness transition code; only deployment capability validation differs.

The built-in system settings written on first startup cover these categories:

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

`File Processing` includes switches and limits for read-only archive preview, disabled by default. `Runtime` includes background task lane concurrency, idle backoff, and share-page audio/video stream playback session lifetime.

Default background task frequencies:

- Mail queue scans every `5` seconds.
- Background task queue scans every `5` seconds.
- Periodic cleanup runs every `1` hour.
- Low-level file consistency check runs every `6` hours.
- Enabled, enrolled follower nodes whose current transport can be used are probed every `5` minutes.

Periodic cleanup covers by default:

- expired upload sessions
- completed upload sessions
- trash items
- archived teams
- locks
- audit logs
- task artifacts
- WOPI sessions
- external authentication temporary login flows and email verification flows
- MFA login and setup temporary flows

## Default State of a New Deployment

A newly deployed instance usually also has these defaults:

- default listen address is `127.0.0.1:3000`
- default WebDAV prefix is `/webdav`
- the first created user automatically becomes an administrator
- public registration is enabled by default
- public registration users need email activation by default
- new user quota is unlimited by default
- after default-storage setup is complete, new users are automatically bound to the current default policy group
- newly created teams also use the current default policy group unless a policy group is specified separately

`auth.bootstrap_insecure_cookies` defaults to `true`, so plain-HTTP first login works by default. When the administrator is created from an HTTPS origin, setup enables Secure cookies before automatic login. This static value only controls the first database write; restart does not overwrite an existing runtime setting.

## What Usually Appears in the Default Directory

If you use default relative paths, after first startup you will usually see:

- `data/config.toml`
- `data/asterdrive.db`
- `data/.tmp`
- `data/.uploads`
- `data/remote-storage-targets` (when a follower uses a local remote storage target)

If the administrator manually creates a local policy pointing to `data/uploads`, that directory appears on the first write; startup does not create it as a default. `data/.tmp` and `data/.uploads` are runtime temporary directories, not long-term data directories.
`data/remote-storage-targets` is the local receiving root managed by the primary for a follower. It only matters if this instance is used as a follower node.

`auth.jwt_secret` and `auth.mfa_secret_key` in `data/config.toml` are written as random values when first generated. Keep them during future backup, migration, and restore. If MFA is enabled, replacing `mfa_secret_key` prevents existing authenticators from continuing to verify.

## Check These Items Immediately After Startup

1. Whether `/health` returns 200.
2. After administrator and storage setup is complete, whether `/health/ready` returns 200 with `data.status` set to `ready`.
3. Whether `data/config.toml` is generated in the expected directory.
4. Whether the database is created in the expected location and updated.
5. Whether the administrator has created and tested the default storage policy; in a cluster, it must be shared storage reachable by every Primary.
6. Whether the default policy group exists and the administrator is assigned to it.
7. Whether the admin panel opens normally.
8. Whether default values for all groups are visible under `Admin -> System Settings`.
9. If WebDAV will be used, whether the mount path matches the configuration.
10. If WOPI will be enabled, whether `Public Site URL` and `Preview Applications` can be saved correctly.
11. If external authentication will be enabled, whether `Public Site URL` is correct, and whether redirect URIs generated under `Admin -> External Authentication` have been registered with the identity provider.
12. If follower nodes will be used, whether the follower has completed enrollment, internal protocol capabilities are compatible, and an applied default remote storage target exists.

## If Checks Fail, Look Here First

- Is the current working directory the one you think it is?
- Is `data/config.toml` actually being read by the service?
- Do the database, upload directory, and temporary directories have write permission?
- Is the container volume, systemd `WorkingDirectory`, or host path mounted incorrectly?
