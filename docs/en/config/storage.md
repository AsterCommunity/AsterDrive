# Storage Policies

::: tip This page covers the model and boundaries only
- **`Admin -> Storage Policies`**: where files are actually written
- **`Admin -> Policy Groups`**: which storage policy a user or team upload hits
- **Backend tutorials**: for specific storage policy backends, see [Storage Policy Backends](/en/storage/)

Users and teams are not bound to storage policies directly. They are bound to **policy groups**; policy groups then route uploads to specific policies by rules.
:::

## What Exists After the First Startup?

After a newly deployed instance starts for the first time, the system automatically prepares:

- Default local storage policy `Local Default`
- Default policy group `Default Policy Group`

If you change nothing, new users are automatically bound to the default policy group, and that policy group routes uploads to the default local storage policy. When a system administrator creates a new team without manually choosing a policy group, the default policy group is used.

## Currently Supported Storage Types

| Type | Description | Tutorial |
| --- | --- | --- |
| `local` | Files are stored in a local directory | [Local disk](/en/storage/local) |
| `s3` | Files are stored in S3 or S3-compatible object storage, such as MinIO / R2 / B2 / OSS | [S3 / MinIO / R2](/en/storage/s3-minio-r2) |
| `azure_blob` | Files are stored in an Azure Blob Storage container using the Azure Blob SDK and SAS URLs | [Azure Blob Storage](/en/storage/azure-blob) |
| `tencent_cos` | Files are stored in Tencent COS; base object operations reuse S3-compatible behavior, with additional Tencent-native capabilities such as COS CI (Cloud Infinite / 数据万象). See the Tencent COS tutorial for what COS CI provides and when it may be billed. | [Tencent COS](/en/storage/tencent-cos) |
| `one_drive` | Files are written to Microsoft Graph-accessible OneDrive, SharePoint, or Microsoft 365 group drives | [OneDrive](/en/storage/onedrive) |
| `sftp` | Files are streamed by the AsterDrive server to an SSH/SFTP file server | [SFTP](/en/storage/sftp) |
| `remote` | Files are written to another AsterDrive follower node through the internal remote storage protocol | [Follower Node Storage Policy](/en/storage/remote-follower) |

## Storage Policies vs Policy Groups

- If you only want to change "which storage backend files ultimately land in", create or edit storage policies
- If you want different users, teams, or file sizes to use different routes, configure policy groups

Typical admin-console workflow:

1. Create or test the storage policy
2. Create policy group rules
3. Bind users or teams to the target policy group

If you are migrating existing data, do not directly change the old policy path, bucket, endpoint, or follower node to the new location. Create the target policy first, use `Admin -> Storage Policies -> Migrate Data` to create a migration task, and only then adjust policy groups.

## Common Storage Policy Fields

| Item | Purpose |
| --- | --- |
| Name | Display name in the admin console |
| Driver type | `local`, `s3`, `azure_blob`, `tencent_cos`, `one_drive`, `sftp`, or `remote` |
| Connection information | Local directory / S3 endpoint, bucket, secrets / Azure Blob endpoint, container, account keys / COS endpoint, bucket, secrets / OneDrive Microsoft Graph target and authorization settings / SFTP endpoint, SSH credentials, host key fingerprint / bound follower node |
| Base path | Directory, prefix, or remote-target relative path used when writing through this policy |
| Single-file size limit | Maximum upload size. `0` = unlimited. |
| Chunk size | Size of each chunk for large-file uploads |
| Default policy | Preferred by newly created default groups or default routing rules |
| Extra options | Local content deduplication, S3 / Azure Blob / COS upload and download modes, S3 path-style access, OneDrive target-drive location, SFTP host key fingerprint, remote upload and download modes, storage-native processing, and so on |

The storage policy form is not driven only by frontend hardcoded provider fields. AsterDrive reads each driver's `StorageConnector` descriptor from the backend, including fields, capabilities, upload workflows, and management actions. When a storage backend grows or changes, the admin UI can follow the backend descriptor instead of re-creating a parallel capability table.

## Reading Connection Tests

Storage policies have two connection-test paths:

- **Test saved policy**: probe the policy already saved in the database.
- **Test draft settings**: probe the current form values before saving. For static-credential backends such as S3, Azure Blob, and Tencent COS, blank credential fields can reuse the saved credentials for the same policy.

A successful connection test means the AsterDrive server can reach the backend and the basic read/write path for credentials, bucket / container / drive / follower ingress is usable. It does not prove that browsers can directly reach object storage or a follower node. If you use `presigned`, still check browser networking, HTTPS certificates, CORS, and exposed response headers.

When a connection test fails, the admin console prefers the standard error response's `error.diagnostic.message`. The diagnostic is derived from backend storage errors, keeps useful troubleshooting context where possible, and redacts sensitive values such as SAS tokens, account keys, and secret keys. Scripts and third-party clients can read the same shape:

```json
{
  "code": "storage.permission_denied",
  "msg": "Storage permission denied",
  "error": {
    "retryable": false,
    "diagnostic": {
      "kind": "permission",
      "message": "provider denied access to the target prefix"
    }
  }
}
```

The top-level `code` remains the stable error code. `diagnostic.message` is administrator-facing text and should not be used for program branches.

::: warning Storage-native processing can incur provider charges
`Storage-native processing` is a master switch on each storage policy. AsterDrive only calls native data-processing features exposed by the resolved storage driver after this switch is enabled. For Tencent COS policies, this maps to COS CI.

AsterDrive caches generated thumbnails, media information, and similar derivatives so they are not processed on every view, but initial generation and subsequent provider-side processing requests may incur charges from your cloud provider. For Tencent COS setup, suffix rules, and free-quota notes, see the [Tencent COS storage policy tutorial](/en/storage/tencent-cos).
:::

## How to Choose Between Storage Types

### `local`

Suitable for single-node deployments, NAS, and files that should land directly on local disk. For directory planning, permissions, content deduplication, and test policy groups, see the [local disk storage policy tutorial](/en/storage/local).

### `s3`

Suitable when files are stored in MinIO, AWS S3, or other compatible object storage.

`s3` means a generic S3-compatible backend. It only relies on common object-storage APIs and does not assume provider-specific data-processing features. If you want Tencent COS CI capabilities, choose `tencent_cos` instead of configuring COS as a generic `s3` policy.

Generic `s3` policies can control path-style access. When enabled, requests look closer to `endpoint/bucket/key`, which is common for compatible services such as MinIO and RustFS. When disabled, AsterDrive uses virtual-hosted style, which is common for services such as AWS S3. Provider and gateway behavior differs, so test the connection after creating or editing the policy.

If an older policy configured Tencent COS as generic `s3`, the admin console may suggest promoting the driver to `tencent_cos`. This does not migrate objects or change the bucket. It only makes that policy use the Tencent COS driver. AsterDrive only allows explicit allowlisted promotion directions and rejects promotion when active upload sessions exist or the bucket no longer matches.

For buckets, credentials, CORS, upload/download modes, and policy-group routing, see the [S3 / MinIO / R2 storage policy tutorial](/en/storage/s3-minio-r2).

### `azure_blob`

Suitable when files are stored in Azure Blob Storage containers. `azure_blob` uses the Azure Blob SDK and Azure SAS URLs. It does not use the S3-compatible API.

When configuring it, keep the field names straight: Endpoint is the Blob service endpoint, Bucket means Azure container, Access Key means storage account name, and Secret Key means storage account key. If you use `presigned` direct upload, configure Blob service CORS and allow the `x-ms-blob-type` request header. See the [Azure Blob Storage policy tutorial](/en/storage/azure-blob) for the full flow.

### `one_drive`

Suitable when files should be written to Microsoft Graph-accessible OneDrive, SharePoint document libraries, or Microsoft 365 group drives.

OneDrive policies require a Microsoft app registration and administrator delegated OAuth authorization. Save the policy and Microsoft Graph application credentials before starting authorization; the authorization request does not carry unsaved Client ID / Secret drafts. The target drive can be resolved automatically after authorization, or specified with a Drive ID, SharePoint site ID, or group ID.

The upload mode can be `server_relay` or `frontend_direct`. `server_relay` is the default retained for existing-policy compatibility and sends files through AsterDrive. `frontend_direct` lets the browser upload directly to Microsoft Graph, reducing bandwidth on the AsterDrive node. Microsoft provides the cross-origin support required for direct upload, so no additional AsterDrive setting is needed. See the [OneDrive storage policy tutorial](/en/storage/onedrive) for the full flow.

### `sftp`

Suitable when files should be written to an SSH/SFTP file server, NAS, or traditional server directory.

SFTP policies use server-side streaming for both uploads and downloads; browsers never connect directly to the SFTP server. Endpoint can be `sftp://host:port`, `host`, or `host:port`; the default port is `22`, and the remote root belongs in Base path. SSH username / password still use the API fields `access_key` / `secret_key`, but the admin form labels them as SSH credentials.

SFTP rejects unknown host keys by default. The first connection test reports the server's actual `SHA256:...` fingerprint; after the administrator confirms it, save it as `storage_policy.options.sftp_host_key_fingerprint`. Later connections must match that fingerprint. See the [SFTP storage policy tutorial](/en/storage/sftp) for the full flow.

### `tencent_cos`

Suitable when files are stored in Tencent COS and you want to enable Tencent-native capabilities per policy.

`tencent_cos` reuses S3-compatible logic for base object reads/writes, multipart upload, and download routing. COS-specific code handles Tencent endpoint normalization, COS signing, and COS CI features. See the [Tencent COS storage policy tutorial](/en/storage/tencent-cos) for the full setup flow.

### `remote`

Suitable when the control plane should stay on the primary node while real object placement is split to another AsterDrive follower node.

A remote policy binds a follower node and one of that node's remote storage targets; it no longer has a separate endpoint or access key. When no target is selected explicitly, the **default remote storage target** in the follower node details is used. See the [follower node storage policy tutorial](/en/storage/remote-follower) for the full setup flow.

## Capacity Observation and Migration Preflight

The storage policy edit dialog shows current capacity observation:

| Policy type | Capacity behavior |
| --- | --- |
| `local` | Reads total, available, and used bytes from the filesystem that contains the policy base directory |
| `s3` / `tencent_cos` | Shows unsupported; the standard S3-compatible API does not expose a unified, reliable bucket free-capacity interface |
| `azure_blob` | Shows unsupported; the Blob data API does not expose unified storage account capacity observation |
| `one_drive` | Reads Microsoft Graph drive quota; if Graph does not return quota data, the result is shown as unavailable |
| `sftp` | Shows unsupported; SFTP has no unified reliable remote filesystem capacity interface |
| `remote` | Asks the remote storage target bound to the policy through the internal protocol. If the target is local, filesystem capacity is usually available. If the target is S3, it is shown as unsupported. |

During data migration, preflight compares the target policy's available capacity with the estimated bytes that still need to be copied. It does not simply use the source policy's total size. Content SHA-256 blobs that already exist in the target policy are treated as reusable and are excluded from the estimated copy size.

Capacity check statuses:

| Status | Meaning | Blocks migration task creation |
| --- | --- | --- |
| Sufficient | Target available capacity is greater than or equal to estimated copy bytes | No |
| Insufficient | Target is confirmed to have too little capacity | Yes |
| Unsupported | The driver has no reliable capacity interface, such as S3/COS/Azure Blob | No, but the UI warns you to confirm capacity |
| Unavailable | This capacity check failed or returned incomplete information | No, but the UI warns you to confirm capacity |

## Blob Matching Rules During Storage Migration

Migration processes blobs, not individual file records. To avoid incorrect merges, AsterDrive separates two kinds of blob keys:

| Type | Detection | Migration matching rule |
| --- | --- | --- |
| Content SHA-256 | 64 hexadecimal characters | If the target policy already has the same hash and size, AsterDrive verifies the target object and then merges references |
| Opaque key | Any other blob key | Never participates in cross-policy matching, and is not merged even if key and size are the same |

If a content SHA-256 hash matches but the size differs, the migration fails and leaves the source blob unchanged. This usually indicates inconsistent database or object-storage state and should be investigated by an administrator.

If an opaque key already exists in the target policy, the migration does not overwrite the target object and does not merge the source blob into the target blob. AsterDrive generates a new `migration-...` key for the source blob, copies the object to a new path under the target policy, and records the count as renamed opaque keys in the task result.

## Changes You Should Not Make Directly

::: warning Do not change these on a policy that already has files written

- Local directory
- Bucket
- Endpoint
- SFTP base path
- Bound follower node

Old files are read from their original locations. Changing the location directly means existing files cannot be found.

A safer approach:

1. Create a new policy
2. Select the source and target policies under `Admin -> Storage Policies -> Migrate Data`
3. Click `Check Plan` first, and confirm target probing, stream-upload capability, and capacity checks do not have blocking issues
4. Create the migration task and confirm completion under `Admin -> Tasks`
5. Switch users or teams to the policy group containing the new policy

:::

## Migrating Existing Policy Data

`Migrate Data` creates a background task that copies existing blobs from the source policy to the target policy, and updates file records and version references during migration.

Before the task is created, the page runs `Check Plan`:

- count source-policy objects and total size
- probe whether the target policy can be written
- check whether the target supports stream upload required for migration
- estimate how many objects already exist on the target and can be reused, then calculate the bytes that still need to be copied
- try to verify whether the target has enough free capacity for those remaining bytes
- count opaque key conflicts

Only confirmed insufficient capacity blocks migration task creation. If the capacity check is unsupported or unavailable, it does not always mean migration is impossible. It means the current driver cannot reliably report free space. Before creating the real task, confirm target capacity yourself.

After the task is created, check progress under `Admin -> Tasks`. For large migrations, reserve a maintenance window and avoid writing many new files to the source policy while migration is running.

::: warning Migration is not backup
Migration tasks move file objects and references known to AsterDrive. They do not replace database, configuration, or object-storage backups. For production migrations, read [Backup and Restore](/en/deployment/backup) first.
:::

## Daily Maintenance

- Keep at least one usable default storage policy
- Keep at least one enabled default policy group
- Test the connection once before saving
- When assigning different storage routes to different users/teams, bind policy groups under `Admin -> Users` or `Admin -> Teams`
- When connecting external backends, prefer the specific tutorials under [Storage Policy Backends](/en/storage/)
