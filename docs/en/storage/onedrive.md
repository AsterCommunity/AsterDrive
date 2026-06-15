---
description: OneDrive storage policy tutorial covering Microsoft app registration, Global / China cloud endpoints, delegated permissions, OAuth authorization, target drive resolution, and the app configuration storage design.
---

# OneDrive Storage Policy Tutorial

::: tip What this page covers
This page walks through the complete flow for writing AsterDrive files to Microsoft OneDrive or SharePoint / Microsoft 365 group drives: prepare a Microsoft app, create a OneDrive storage policy, authorize Microsoft Graph, configure policy group rules, bind users or teams, and understand how Client ID / Secret are stored.
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

Creating only a OneDrive storage policy is not enough. After saving the policy, authorize Microsoft from the AsterDrive admin console so AsterDrive can obtain delegated tokens for the target drive.

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

::: warning Do not mix Global and China
The Microsoft app registration, sign-in endpoint, and Graph endpoint must belong to the same cloud. Personal Microsoft accounts do not support the China endpoint. Use Global for personal OneDrive.
:::

## 2. Prepare the Microsoft App Registration

Prepare an app in Microsoft Entra ID app registrations.

At minimum, check:

- Application (client) ID
- Client Secret, optional for public-client scenarios but recommended for typical server deployments
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

::: tip Why offline_access is required
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
| Client Secret | Microsoft app secret; leave empty only for public-client setups |
| Drive type | Usually keep the default during creation and let authorization resolve the default drive |

After saving the policy, open the policy edit page and start authorization.

## 4. Complete Microsoft Authorization

Open the OneDrive policy edit page:

```text
Admin -> Storage Policies -> target OneDrive policy
```

In the `Microsoft Graph credential` panel, click `Authorize`.

After authorization succeeds, the browser returns to the AsterDrive admin console and shows the result. AsterDrive stores:

- access token ciphertext
- refresh token ciphertext
- token expiration time
- authorization time
- target drive / root item metadata
- Microsoft cloud / tenant / app metadata

When background tasks need OneDrive access, AsterDrive refreshes the access token automatically. Successful refresh writes the new token state back to the database. If Microsoft rejects the refresh token, the policy enters reauthorization-required state.

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

::: tip Root item
Leave Root item ID empty or set it to `root` to write under the drive root.
:::

## 6. Create a Test Policy Group

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

## 7. Bind a Test User or Test Team

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

## 8. Run a Real Acceptance Check

With a test account, run at least:

1. Upload a small file
2. Upload a larger file
3. Download a file
4. Preview an image or trigger thumbnail generation
5. Delete and restore a file
6. Confirm on the Microsoft side that objects are written into the target drive
7. Click `Validate` in the AsterDrive admin console

If background tasks report Microsoft Graph `401` or token-related errors, check the credential status on the policy edit page. If the status requires reauthorization, click `Reauthorize`.

## 9. Current App Configuration Storage Design

AsterDrive currently does not create a separate shared application configuration table for OneDrive. Microsoft app settings reuse storage policy connection fields:

| Storage field | OneDrive meaning |
| --- | --- |
| `storage_policies.access_key` | Microsoft Application (client) ID |
| `storage_policies.secret_key` | Microsoft Client Secret |

This is an intentional design trade-off. The current AsterDrive storage policy model is "each policy owns its connection configuration", which is also how S3, COS, Azure Blob, and other backends are configured. OneDrive currently does not require multiple policies to share one Microsoft App, so AsterDrive does not introduce an `oauth_app_configs` or `storage_provider_app_configs` table ahead of a real product requirement.

Benefits of the current design:

- It matches the existing storage policy connection model
- The create and authorize flow stays direct
- There is no extra lifecycle, permission, or reference management for shared app configs
- The schema does not grow for a reuse requirement that is not yet needed

If a future requirement appears, such as multiple OneDrive policies sharing one Microsoft App, Google Drive using the same OAuth storage-app model, or administrators needing centralized app secret rotation, AsterDrive can migrate to a model like:

```text
storage_provider_app_configs
storage_policies.app_config_id -> storage_provider_app_configs.id
```

Until then, treating Client ID / Secret as the OneDrive storage policy's connection configuration is simpler and better aligned with the current architecture.

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
