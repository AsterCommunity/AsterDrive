---
title: Google Drive Storage Policy
description: Configure administrator-managed Google Drive blob pools, OAuth, shared drives, and server-side resumable uploads.
---

An administrator authorizes a Google account and uses a folder, shared drive, or appDataFolder as an AsterDrive storage pool. Users keep using AsterDrive files, permissions, versions, quotas, and shares without authorizing their own Google accounts. This backend does not provide user Drive import, browsing, or synchronization.

## Register the OAuth application

1. Enable the Google Drive API in a Google Cloud project.
2. Configure the consent screen and create a **Web application** OAuth client.
3. Create a Google Drive storage policy in AsterDrive and save the client ID and secret.
4. Copy the exact callback URL shown by the policy authorization panel into the Google client's **Authorized redirect URIs**. Configure a reachable HTTPS public site URL matching the callback.
5. During testing, add the administrator as a test user. Complete Google's publication and verification requirements for long-term use: external applications in Testing typically receive Drive refresh tokens lasting seven days.

Regular folders and shared drives request `https://www.googleapis.com/auth/drive`; appDataFolder requests `https://www.googleapis.com/auth/drive.appdata`. The regular-folder scope grants broad Drive access. AsterDrive still limits object lookup and deletion to the configured pool and application ownership marker.

## Choose the pool

- **Root folder ID**: the folder ID from its Drive URL, rather than the full URL. Empty uses the account root. A dedicated folder is recommended.
- **Shared drive ID**: optional Google Workspace shared drive. Empty root folder uses its root; a specified folder must belong to this drive. The administrator needs permission to add and delete files.
- **Use appDataFolder**: hidden application data storage; cannot be combined with either ID. These objects cannot be browsed or shared as regular Drive files.

After storing data, avoid changing the pool, OAuth application, or account. Existing keys resolve under the original pool and application; changing them can make old data inaccessible. Use AsterDrive storage migration when moving data.

## Authorize and verify

Save the policy and authorize using the administrator account. The flow uses one-time state and PKCE. Offline refresh tokens and the client secret are stored in the backend's encrypted credential store. Check authorization status, validate credentials, and run the connection test, which uses AsterDrive's temporary object write/delete workflow.

Add the policy to a [policy group and bind users or teams](/en/admin/storage-policies/). Verify upload, preview, download, and deletion. Large uploads use server relay and Google resumable upload with at most one 8 MiB fragment buffered per upload. Administrator tokens and session URLs stay on the server. Downloads and Range previews stream through AsterDrive; Google browser-direct downloads are not exposed.

Logical object keys are stored in `appProperties.asterdrive_storage_path`. Lookup and deletion require that application marker and the configured parent folder. Duplicate keys fail closed. Do not manually move these files or alter their application metadata.

## Troubleshooting

- **Reauthorization required**: token expiry, revocation, missing offline refresh token, or an OAuth application still in Testing. Reauthorize and check publishing status.
- **Permission denied**: check folder/shared drive permissions and consented scopes.
- **Invalid configuration**: use IDs, not full URLs, and verify the folder belongs to the configured shared drive.
- **Rate limited or temporarily unavailable**: retry later and inspect Google quota and server outbound networking.
- **Object missing**: check for manual moves or deletion. Restore the original folder/application configuration; do not edit blob keys in the AsterDrive database.

Google capacity, API quota, and organization policies continue to apply. AsterDrive owns its quotas and blob reference counts; remote deletion occurs through garbage collection after references are released.
