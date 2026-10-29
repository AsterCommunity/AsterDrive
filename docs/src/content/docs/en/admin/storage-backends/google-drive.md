---
title: Google Drive Storage Policy Tutorial
description: Configure a Google Cloud OAuth application, administrator authorization, folders and shared drives, browser-direct uploads, quota observation, downloads, and troubleshooting.
---

:::tip[What this page covers]
An administrator authorizes a Google account and selects a dedicated folder, shared drive, or appDataFolder as an AsterDrive blob pool. Ordinary users retain AsterDrive files, permissions, versions, quotas, and shares without authorizing their own Google accounts. This backend does not provide user Drive browsing, import, or synchronization.
:::

## Separate the configuration layers

The complete path is: Google Cloud OAuth application → AsterDrive storage policy → administrator authorization and connection test → policy group rules → user or team assignment.

| Task | Admin entry |
| --- | --- |
| Create or edit the policy | `Admin → Storage Policies` |
| Copy callback URL, authorize, reauthorize, validate | The policy's Google Drive authorization panel |
| Configure routing and target priority | `Admin → Policy Groups` |
| Assign a policy group | User or team details |
| Inspect cleanup or migration failures | `Admin → Background Tasks` |

Google Drive requires a saved application followed by browser authorization. It cannot be the first storage policy during system setup. Complete Local or another immediately verifiable storage setup first, then add Google Drive.

## 1. Prepare a Google Cloud project

1. Create or select a dedicated project in Google Cloud Console.
2. Enable the **Google Drive API** in the API library.
3. Configure Google Auth Platform / the OAuth consent screen, application details, and a suitable user type.
4. Create an OAuth client of type **Web application** and retain its client ID and secret.
5. For an external application in **Testing**, add the Google account that will authorize AsterDrive as a test user.

This is administrator-consented delegated OAuth. A service account JSON file, an API key, or another application's secret does not replace the web client's secret.

### Testing applications and token expiry

Drive refresh tokens issued to external applications in Testing typically expire after seven days. For long-running deployments, complete Google's publication and any required verification. Organization administrators can also restrict third-party applications and Drive scopes.

Application secrets, account authorization, and publication status are separate concerns. Changing the secret does not repair a revoked grant; reauthorization does not repair an incorrect client secret.

### Requested scopes

| Pool | Scope requested by AsterDrive |
| --- | --- |
| Regular folder, account root, shared drive | `https://www.googleapis.com/auth/drive` |
| appDataFolder | `https://www.googleapis.com/auth/drive.appdata` |

Regular pools use the broad Drive scope. AsterDrive additionally restricts object lookup and deletion to the configured pool and application ownership marker. Use a dedicated application and folder, and review Google's and your organization's consent requirements.

## 2. Create the policy and register its callback

Open `Admin → Storage Policies → New Policy` and select **Google Drive**.

| Field | What to enter |
| --- | --- |
| OAuth client ID | The Google Web application client's ID |
| OAuth client secret | The same client's secret; required for initial creation |
| Root folder ID | The folder ID from its URL; empty uses the account or selected shared-drive root |
| Shared drive ID | A Google Workspace shared drive, when applicable |
| Use appDataFolder | Hidden application data storage; cannot be combined with either ID |
| Upload transfer | Server relay or browser direct; server relay is the default |
| Capacity probe timeout | Defaults to 10 seconds; accepts 2–30 seconds and only controls quota observation |

Save the policy, copy the exact OAuth callback URL from the authorization panel, and add it to the Google client's **Authorized redirect URIs**. For example:

```text
https://drive.example.com/api/v1/admin/policies/storage-authorization/callback
```

:::caution[The callback must match exactly]
The scheme, hostname, port, and path must match. Production deployments should use the correct external HTTPS site URL. Behind a reverse proxy, verify that the panel shows the external address. Register the actual address shown by your deployment, rather than the example hostname.
:::

When editing, omitting or leaving the secret empty preserves the saved secret for the **same client ID**. Changing the client ID requires a new secret and authorization.

## 3. Choose the storage pool

### Regular folder

For `https://drive.google.com/drive/folders/FOLDER_ID`, enter `FOLDER_ID`, not the full URL.

A dedicated empty folder is recommended. Leaving the field empty uses the account root, but makes it harder to distinguish AsterDrive-managed objects from other administrator files.

### Google Workspace shared drive

The shared drive ID defines the search scope. A root folder ID can further restrict the pool to a folder inside that drive. An empty root folder ID uses the shared-drive root.

The administrator needs access, permission to add files, and permission to delete them. Browse-only access is insufficient. Credential validation checks the root and its shared-drive membership; the saved-policy connection test also creates and deletes a test object.

### appDataFolder

This hidden space is isolated by application and uses the `drive.appdata` scope. It is not available in the normal Drive file UI and cannot be shared, moved across spaces, or trashed as ordinary Drive content.

Enable it with both IDs empty. AsterDrive manages file deletion and cleanup; there is no need to manipulate a Google web trash folder.

:::caution[Do not switch an existing pool or account casually]
Existing objects depend on the original application, Google account, and location. Changing these can make old files inaccessible. Move data through AsterDrive storage migration; do not edit blob keys in the database.
:::

## 4. Authorize and validate

1. Save the policy and OAuth application configuration.
2. Click Authorize and consent using the intended administrator Google account.
3. After returning to AsterDrive, inspect authorization status.
4. Validate the credential to check access to the configured pool.
5. Run the saved-policy connection test to verify temporary object creation and deletion.

The flow uses one-time state and PKCE. The backend encrypts the client secret, access token, and refresh token. Ordinary users do not receive administrator OAuth tokens.

Authorization uses the saved application. If the application, credential, or storage configuration changes during the flow, the stale callback is rejected; start authorization again. An old browser page cannot overwrite the newly saved configuration.

Normal access-token expiry is handled by automatic refresh. Administrator reauthorization is needed when the grant expires or is revoked. Rate limits, proxy errors, and temporary networking failures preserve saved authorization.

## 5. Choose the upload transfer

Both strategies use Google resumable upload. The difference is where the file bytes travel.

| Strategy | Data path | Suitable use |
| --- | --- | --- |
| Server relay (`server_relay`, default) | Browser → AsterDrive → Google Drive | Clients can reach AsterDrive, or a centralized network path is preferred |
| Browser direct (`frontend_direct`) | Browser → Google Drive | Clients can reliably reach Google and server bandwidth should be reduced |

### Server relay

Large files transfer sequentially with at most one 8 MiB fragment buffered at a time. AsterDrive does not buffer the full object in memory. Provider sessions stay encrypted on the backend; browsers call AsterDrive upload endpoints and receive no Google session URL.

The browser only needs AsterDrive connectivity. Primary instances need access to Google OAuth and Drive APIs. Capacity probe deadlines do not change file transfer timeouts.

### Browser direct

AsterDrive checks permissions, quotas, and placement, then creates the Google upload session on the backend. The browser receives the temporary capability URL for this upload and PUTs the fragments directly. Administrator OAuth tokens remain on the server.

The session URL authorizes this specific upload. Do not publish it in comments, logs, or screenshots. It does not make the entire storage folder publicly shared.

Google acknowledges ordinary intermediate fragments with `308 Resume Incomplete` and a `Range` header. AsterDrive resumes from confirmed bytes. When the response header is not visible to the browser or a response is lost, the browser asks AsterDrive to reconcile progress with the provider.

### Browser connectivity and CORS

- Both the browser and backend need the appropriate Google connectivity. A server-side proxy does not automatically give user devices direct Google access.
- The browser must allow cross-origin PUT with `Content-Type` and `Content-Range`.
- Google controls CORS on its own endpoints. AsterDrive's reverse proxy cannot add headers to Google's responses.
- A hidden `Range` header triggers backend progress reconciliation. If the preflight itself is blocked, repair browser connectivity or use server relay.

AsterDrive does not attach its login cookies or CSRF headers to provider upload requests. Browser success still requires backend object and size validation before file publication and quota accounting.

### Resume, cancel, and switch strategies

After reloading the page, recover the task from AsterDrive's recoverable uploads and reselect the original file when prompted. Recovery uses the provider-confirmed position. An expired session requires a new upload.

Cancel through AsterDrive to clean up the backend session and temporary object. Aborting a browser request and cancelling the entire upload are separate actions. Inspect background tasks and retry failed cleanup.

Changing the strategy affects newly initialized uploads. An existing upload retains its original session type; editing a policy does not convert an in-progress session.

## 6. Understand capacity observation

Regular account folders and appDataFolder observe Google `about.storageQuota`.

| Condition | Interpretation |
| --- | --- |
| A finite limit and usage are returned | Available bytes are `max(limit − usage, 0)` |
| Google Workspace pooled storage | Limit and usage can describe the organization pool, not dedicated folder space |
| No limit for an unlimited account | No finite capacity bound is invented; this is not zero free space |
| Temporary or incomplete quota response | Capacity is inconclusive and existing admission rules apply |
| Shared drive | The administrator's personal quota is not attributed to the drive; its independent capacity bound is not observable here |

The `usage` field can include other services such as Gmail and Google Photos. Subtracting only Drive file usage would overstate available capacity.

Observations are cached and are not reservations. Other writers can consume space after a successful observation. Google write results and organization limits still apply.

## 7. Downloads and filenames

Google Drive downloads currently stream through AsterDrive, including Range previews. AsterDrive checks access first and returns the **current AsterDrive filename**. Renaming a file does not require renaming the underlying Google object.

This connector therefore has no OneDrive-style direct-download or provider-native / strict-current filename selector. OneDrive uses preauthenticated download URLs; ordinary Google media requests require authorization. `webContentLink` and asynchronous `files.download` results must not be assumed equivalent for distributing private administrator-owned objects. This implementation does not make folders public or expose administrator tokens to create a direct link.

`files.download` also does not accept appDataFolder's dedicated scope. Its 24-hour operation lifetime does not establish anonymous download access for 24 hours. Server relay currently provides consistent authorization and filenames across regular folders, shared drives, and appDataFolder.

Google also exposes optional thumbnail links and image/video metadata, but this connector does not currently integrate storage-native processing. AsterDrive continues using its existing thumbnail and media processing chain. Provider API fields alone do not establish product support.

## 8. Assign a policy group and verify

Add the policy to a [policy group and bind users or teams](/en/admin/storage-policies/). In an isolated pool, verify:

1. Upload an empty file, a small file, and a file larger than 8 MiB, checking the selected policy.
2. Download and compare content, then exercise Range previews.
3. For browser direct, also verify CORS requests, reload/resume, and cancellation.
4. Rename a file and confirm the downloaded name follows AsterDrive.
5. Delete files, empty trash, and wait for physical cleanup after blob references are released.
6. Inspect capacity, background tasks, and errors. A single connection test does not verify the whole file lifecycle.

Local protocol tests do not replace acceptance against a real Google account, shared-drive roles, and organization policies. Verify each chosen pool and transfer strategy in the deployment's actual network.

## Object ownership and maintenance

AsterDrive stores the logical key in Google application metadata and scopes lookup/deletion to the configured pool. Multiple objects for one logical key fail closed instead of deleting an arbitrary match.

Do not manually move, delete, or alter application metadata on these objects. Versions, shares, and other references can retain a blob after file deletion. Physical removal follows AsterDrive reference and cleanup rules. Deleting through the Google web UI bypasses those rules.

## Troubleshooting

| Symptom | Check first |
| --- | --- |
| `redirect_uri_mismatch` | Exact registered callback URL and the proxy-generated external scheme/hostname |
| Consent screen denies access | Test-user membership, publication state, Workspace application restrictions |
| Reauthorization required | Refresh-token expiry/revocation, Testing's seven-day lifetime, offline grant |
| Invalid OAuth application | Client ID and secret belong to the same Web application; save repairs and reauthorize |
| Folder/shared-drive permission denied | IDs, drive membership, administrator role; validate and test after repairs |
| 308 during upload | Normal incomplete acknowledgement; investigate only when progress cannot be reconciled or requests fail |
| Browser-direct upload fails | Client-to-Google networking, CORS preflight, browser extensions, enterprise proxies; use relay if needed |
| Rate limit or temporary failure | Google quotas, proxies, backend outbound networking; retry without first deleting credentials |
| Inconclusive capacity | Unlimited account, shared drive, or missing quota response; this does not mean zero bytes remain |
| Missing or duplicate object | Manual moves/deletion/copies or altered application metadata; restore the original application and pool first |

## Official references

- [Google OAuth Web Server](https://developers.google.com/identity/protocols/oauth2/web-server)
- [Drive resumable upload](https://developers.google.com/workspace/drive/api/guides/manage-uploads)
- [appDataFolder](https://developers.google.com/workspace/drive/api/guides/appdata)
- [Account/organization quota](https://developers.google.com/workspace/drive/api/reference/rest/v3/about)
- [Download and export](https://developers.google.com/workspace/drive/api/guides/manage-downloads)
