---
description: OneDrive storage policy tutorial covering Microsoft app registration, OAuth authorization, target drives, server relay, and Microsoft Graph browser-direct upload.
title: "OneDrive Storage Policy Tutorial"
---

:::tip[What this page covers]
This page walks through the complete flow for writing AsterDrive files to Microsoft OneDrive or SharePoint / Microsoft 365 group drives: prepare a Microsoft app, authorize Microsoft Graph, choose server relay or browser-direct upload, configure policy group rules, bind users or teams, and understand how credentials stay protected.
:::

## When to Use It

OneDrive storage policies are suitable when:

- You already use Microsoft 365, OneDrive, or SharePoint document libraries
- You want team files to be stored in a Microsoft Graph-accessible drive
- You want an administrator to authorize one OneDrive / SharePoint drive as an AsterDrive backend
- You want to use Microsoft Graph delegated permissions through browser-based administrator authorization

If you only need a generic object-storage backend, S3 / MinIO / R2 or Tencent COS is usually more direct. OneDrive integrates with the Microsoft ecosystem, but it requires correct Microsoft app registration, OAuth redirect URI, and delegated permissions.

## First, Separate the Layers

```mermaid
flowchart TD
  MicrosoftApp["Microsoft app registration"] --> Policy["AsterDrive OneDrive storage policy"]
  Policy --> OAuth["Microsoft Graph OAuth authorization"]
  OAuth --> Drive["Target drive / root item"]
  Drive --> Rule["Policy group rule"]
  Rule --> Binding["User or team bound to the policy group"]
```

Creating only a OneDrive storage policy is not enough. After saving the policy and Microsoft Graph application credentials, authorize Microsoft from the AsterDrive admin console so AsterDrive can obtain delegated tokens for the target drive.

## Entries Used in This Page

| What you want to do | Entry |
| --- | --- |
| Create a OneDrive policy | `Admin -> Storage Policies -> New Policy` |
| Copy the Microsoft redirect URI | `Admin -> Storage Policies -> OneDrive policy -> Microsoft Graph credential` |
| Authorize or reauthorize | `Admin -> Storage Policies -> OneDrive policy -> Authorize` |
| Validate the saved credential | `Admin -> Storage Policies -> OneDrive policy -> Validate` |
| Create routing rules | `Admin -> Policy Groups` |
| Bind a policy group to a user | `Admin -> Users -> User Details` |
| Bind a policy group to a team | `Admin -> Teams -> Team Details` |

## 1. Choose the Microsoft Cloud Endpoint

Choose the Microsoft cloud endpoint when creating the OneDrive policy:

| Cloud | Sign-in endpoint | Graph endpoint | Accounts |
| --- | --- | --- | --- |
| Global | `login.microsoftonline.com` | `graph.microsoft.com` | Personal Microsoft accounts and Entra ID work or school accounts |
| China (21Vianet) | `login.chinacloudapi.cn` | `microsoftgraph.chinacloudapi.cn` | China cloud organization accounts |

:::caution[Do not mix Global and China]
The Microsoft app registration, sign-in endpoint, and Graph endpoint must belong to the same cloud. Personal Microsoft accounts do not support the China endpoint. Use Global for personal OneDrive.
:::

## 2. Prepare the Microsoft App Registration

Prepare an app in Microsoft Entra ID app registrations.

At minimum, check:

- Application (client) ID
- Client Secret, required by the current AsterDrive server-side storage authorization flow
- Redirect URI
- Microsoft Graph delegated permissions

### The Redirect URI Must Match Exactly

AsterDrive shows the redirect URI on the OneDrive policy edit page. Copy that full URI into the Microsoft app registration.

A common form is:

```text
https://drive.example.com/api/v1/admin/policies/storage-authorization/callback
```

Microsoft matches redirect URIs exactly. If the scheme, host, port, or path differs by even one character, the authorization callback fails.

### Use Delegated Permissions

OneDrive storage policies use Microsoft Graph delegated authorization completed by an administrator in the browser. They do not use application permissions.

AsterDrive chooses default authorization scopes by target type:

| Target type | Default scopes |
| --- | --- |
| Personal OneDrive / default work or school OneDrive | `offline_access Files.ReadWrite` |
| Personal or work/school account with explicit Drive ID | `offline_access Files.ReadWrite.All` |
| SharePoint site drive / Microsoft 365 group drive | `offline_access Files.ReadWrite.All Sites.ReadWrite.All` |

Do not manually enter scopes in the AsterDrive frontend. Make sure the Microsoft app registration allows the required delegated permissions, then grant consent on the Microsoft authorization page.

:::tip[Why offline_access is required]
`offline_access` is used to obtain a refresh token. Without a refresh token, background thumbnails, capacity checks, and read/write tasks will require reauthorization after the access token expires.
:::

## 3. Create a OneDrive Storage Policy

Open:

```text
Admin -> Storage Policies -> New Policy
```

Choose the driver type:

```text
OneDrive
```

Fill in:

| Field | Recommendation |
| --- | --- |
| Microsoft cloud | Choose Global or China based on the account's cloud |
| Client ID | Application (client) ID from the Microsoft app registration |
| Client Secret | Microsoft app secret; currently required. Public-client / no-secret flows are not supported by this storage backend. |
| Drive type | Usually keep the default during creation and let authorization resolve the default drive |
| OneDrive upload mode | Choose `Server relay` or `Microsoft Graph direct upload` based on bandwidth flow; Graph direct upload needs no additional cross-origin configuration |
| OneDrive download mode | Choose `Server relay` or `Microsoft Graph direct download` based on bandwidth flow |

After saving the policy, open the policy edit page and start authorization.

:::caution[Save before authorizing]
The OneDrive authorization request only uses Microsoft Graph application settings already saved on the backend. If you just changed Client ID, Client Secret, tenant, cloud, drive type, or location fields in the form, save the policy first, then click `Authorize` or `Reauthorize`.

This avoids sending unsaved secret drafts in the browser authorization request, and it keeps audit logs, the authorization flow, token refresh, and later background tasks on the same configuration.
:::

## 4. Complete Microsoft Authorization

Open the OneDrive policy edit page:

```text
Admin -> Storage Policies -> target OneDrive policy
```

In the `Microsoft Graph credential` panel, click `Authorize`.

Authorization uses the saved Microsoft application settings and does not read an unsaved Client ID or Client Secret from the page. After authorization succeeds, the browser returns to the AsterDrive admin console and shows the result.

AsterDrive securely stores the information needed for later OneDrive access and renews authorization when needed. If Microsoft revokes access or the credential expires, the policy page prompts the administrator to authorize again.

:::tip[Temporary cleanup after policy deletion]
After a policy with temporary upload data is deleted, AsterDrive continues the cleanup in the background. If cleanup fails, the reason is available on the admin task page.
:::

## 5. How the Target Drive Is Resolved

Usually you do not need to enter a Drive ID. AsterDrive resolves it after authorization:

| Drive type | Resolution |
| --- | --- |
| Personal OneDrive | The signed-in account's default drive |
| Work or school OneDrive | The signed-in account's default drive |
| SharePoint site drive | The site's default drive by Site ID, unless Drive ID is provided |
| Microsoft 365 group drive | The group drive by Group ID, unless Drive ID is provided |

Use advanced fields only when you need a non-default document library or a fixed root item:

| Field | When to use it |
| --- | --- |
| Drive ID | Target a non-default drive or bypass automatic resolution |
| Root item ID | Restrict AsterDrive writes to a specific folder |
| Site ID | Required for SharePoint site drive mode unless Drive ID is provided |
| Group ID | Required for Microsoft 365 group drive mode unless Drive ID is provided |

:::tip[Root item]
Leave Root item ID empty or set it to `root` to write under the drive root.
:::

## 6. Choose the OneDrive Upload Mode

OneDrive policies support two upload modes:

| Upload mode | Data path | Best fit |
| --- | --- | --- |
| Server relay (`server_relay`) | Browser -> AsterDrive -> Microsoft Graph | The default retained for compatibility with existing policies; browser traffic always passes through AsterDrive |
| Microsoft Graph direct upload (`frontend_direct`) | Browser uploads directly to Microsoft Graph | An out-of-the-box bandwidth-saving path for large files or servers with limited bandwidth |

Server relay is the default to preserve existing policy behavior. Administrators can switch the upload mode on the OneDrive policy edit page.

### Server Relay

The browser uploads the file to AsterDrive first, then the server writes it to Microsoft Graph.

This path consumes upload bandwidth on the AsterDrive node, but browsers only need connectivity to AsterDrive. Prefer it when user devices cannot connect reliably to Microsoft.

### Microsoft Graph Direct Upload

AsterDrive confirms the upload, then the browser sends the file directly to Microsoft Graph. The file does not pass through the AsterDrive node, which can substantially reduce server bandwidth use.

```mermaid
flowchart TD
  Browser["Browser selects a file"] --> Mode{"OneDrive upload mode"}
  Mode -->|Server relay| Relay["File passes through AsterDrive"]
  Relay --> Graph["Microsoft Graph"]
  Mode -->|Microsoft Graph direct upload| Direct["File bypasses AsterDrive"]
  Direct --> Graph
  Graph --> Done["AsterDrive shows upload complete"]
```

Microsoft access and refresh tokens always stay on the AsterDrive server and are never sent to the browser. Interrupted direct uploads can continue, while canceled or expired uploads are cleaned up automatically.

:::tip[Graph direct upload needs no extra cross-origin rules]
Graph direct upload is designed to work out of the box. Microsoft provides the required cross-origin support. There is no corresponding option in AsterDrive, the Microsoft app registration, or the storage policy.

If direct upload fails in a particular network, check browser extensions, the company network, and whether the correct Microsoft cloud is selected. You can also switch back to server relay.
:::

### Microsoft Graph Direct Download

The download mode is configured separately:

| Download mode | Best fit |
| --- | --- |
| Server relay | Compatibility first; the file passes through AsterDrive before reaching the browser |
| Microsoft Graph direct download | Reduces AsterDrive download bandwidth; the browser fetches the file directly from Microsoft |

When Microsoft Graph direct download is enabled, you can also choose the download filename:

| Download filename | Behavior |
| --- | --- |
| Prefer the OneDrive filename | Renamed files still prefer direct download; the downloaded name may remain the name stored in OneDrive |
| Always use the AsterDrive filename | Use relay streaming when the names differ, so the download uses AsterDrive's current filename |

With direct download, AsterDrive still checks the signed-in user, permissions, sharing rules, and download rules first. It then sends the browser to a short-lived Microsoft download URL. Microsoft transfers the file, so the AsterDrive node does not relay the complete file body.

New files uploaded to OneDrive keep their original filenames. Files with the same name are stored in separate locations and do not overwrite one another. The policy also offers a download filename choice: by default it prefers the filename stored in OneDrive so renamed files can keep using direct download; strict AsterDrive filenames use relay streaming when the names differ.

Direct download needs no additional Microsoft or AsterDrive cross-origin setting. If the browser or company network cannot reach Microsoft, the download falls back to server relay; administrators can also select server relay directly in the policy editor.

## 7. Create a Test Policy Group

Do not move real users to a new OneDrive policy immediately. Create a test policy group first.

Open:

```text
Admin -> Policy Groups
```

Create a policy group, for example:

```text
OneDrive Test Group
```

Add one rule:

| Field | Recommendation |
| --- | --- |
| Storage policy | The OneDrive policy you just created and authorized |
| Priority | Keep the default or make it match first |
| File size range | Cover all sizes first, which makes testing easier |

## 8. Bind a Test User or Test Team

### Bind a User

Open:

```text
Admin -> Users -> User Details
```

Change the test user's policy group to `OneDrive Test Group`.

### Bind a Team

Open:

```text
Admin -> Teams -> Team Details
```

Change the test team's policy group to `OneDrive Test Group`.

Team space uploads follow the team policy group, not the individual user's policy group.

## 9. Run a Real Acceptance Check

With a test account, run at least:

1. Upload a small and a larger file with server relay
2. Switch to Graph direct upload and repeat both uploads
3. Download a file once with server relay and once with Microsoft Graph direct download
4. Upload two files with the same name and confirm that both open and download correctly
5. Rename one of them, then download it. In `provider_native` mode, the download may retain the older filename stored by OneDrive; switch to `strict_current` before asserting that the download uses AsterDrive's current filename.
6. Preview an image or trigger thumbnail generation
7. Delete and restore a file
8. Confirm on the Microsoft side that objects are written into the target drive
9. Click `Validate` in the AsterDrive admin console

If the admin console reports that Microsoft Graph authorization has expired, check the credential status on the policy edit page. If the status requires reauthorization, click `Reauthorize`.

## 10. How Credentials Are Stored

AsterDrive encrypts the Microsoft Client Secret and authorization information. Plaintext credentials are not returned to the browser, API responses, or audit logs.

When editing an existing policy, leave Client Secret empty to keep the saved secret. Enter and save a new value only when replacing it.

Credential encryption depends on `auth.storage_credential_secret_key`. Preserve this setting when backing up or migrating AsterDrive. See [Authentication & Session - `storage_credential_secret_key`](/en/config/auth/#storage_credential_secret_key).

## FAQ

### Authorization Returns an Error

Check in this order:

1. Whether the redirect URI matches exactly
2. Whether Client ID / Secret come from the same Microsoft app
3. Whether the Microsoft cloud endpoint is correct
4. Whether a personal Microsoft account was accidentally used with the China endpoint
5. Whether the Microsoft app allows the required delegated permissions

### Authorization Succeeds but Drive Resolution Fails

Check the Drive type and target fields:

- Default personal / work-school OneDrive usually does not need Drive ID
- SharePoint site drive needs Site ID unless Drive ID is provided
- Microsoft 365 group drive needs Group ID unless Drive ID is provided
- Leaving Root item ID empty or `root` is the safest initial setup

### Reauthorization Is Required Frequently

The refresh token is usually unavailable or rejected by Microsoft. Check:

- Whether authorization included `offline_access`
- Whether Microsoft organization policy restricts refresh tokens
- Whether an administrator revoked the grant on the Microsoft side
- Whether Client Secret was rotated but the AsterDrive policy was not updated

### Server Relay Works but Microsoft Graph Direct Upload Fails

First confirm that regular upload, download, and the admin `Validate` action work. Then check browser extensions, the company network, and whether the correct Microsoft cloud is selected. AsterDrive has no additional Graph cross-origin setting. This type of problem usually affects only browser-direct upload; switch back to server relay when needed.

### Microsoft Graph Direct Download Is Not Used

First confirm that the OneDrive policy selects `Microsoft Graph direct download`. With `Always use the AsterDrive filename`, renamed files use relay streaming; older objects or browsers that cannot reach Microsoft may also use relay streaming.
