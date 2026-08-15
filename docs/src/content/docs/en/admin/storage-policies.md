---
description: "AsterDrive storage policies and policy groups, the concept authority: the two-layer model, first-start default state, policy fields, connection tests, capacity observation, migration pre-checks and Blob matching rules, and the fields you must not edit in place."
title: "Storage Policies and Policy Groups"
---

:::tip[The two-layer model]
- **`Admin -> Storage Policies`**: where files actually get written
- **`Admin -> Policy Groups`**: which storage policy a user's or team's upload hits

Users and teams do not bind storage policies directly — they bind **policy groups**; the group then routes uploads to concrete policies by rule.
For how to connect a specific backend, see the tutorials under [Storage Backends](/en/admin/storage-backends/).
:::

## What Exists After First Start

Both deployment profiles use the same initialization state machine:

| Profile | First-start behavior |
| --- | --- |
| `single` | After the first administrator is created, the system enters `needs_storage`; the administrator can set `local` or another supported policy as default |
| `cluster` | Also enters `needs_storage` after the first administrator; the default policy must be reachable by every Primary — `local` is not allowed |

When the administrator sets the first policy as default, the system atomically creates or coordinates the default policy group and backfills administrators who have no group yet, then enters `ready`. New users created afterwards automatically bind the current default policy group, which decides their upload target. Single and cluster call the same creation, backfill, and state-migration code; they differ only in which storage capabilities may be selected.

When a system administrator creates a new team without specifying a policy group, the team uses the current default policy group.

## Currently Supported Storage Types

<!-- storage-connectors:policy-catalog:start -->
| Connector ID | Backend | Credential mode | Full tutorial |
| --- | --- | --- | --- |
| `asterdrive.storage.local` | Local | None | [Local](/en/admin/storage-backends/local/) |
| `asterdrive.storage.s3` | S3 | Static secret | [S3](/en/admin/storage-backends/s3/) |
| `asterdrive.storage.alibaba_oss` | Alibaba Cloud OSS | Static secret | [Alibaba Cloud OSS](/en/admin/storage-backends/alibaba-oss/) |
| `asterdrive.storage.sftp` | SFTP | Static secret | [SFTP](/en/admin/storage-backends/sftp/) |
| `asterdrive.storage.azure_blob` | Azure Blob | Static secret | [Azure Blob](/en/admin/storage-backends/azure-blob/) |
| `asterdrive.storage.tencent_cos` | Tencent COS | Static secret | [Tencent COS](/en/admin/storage-backends/tencent-cos/) |
| `asterdrive.storage.remote` | Remote | None | [Remote](/en/admin/storage-backends/remote-follower/) |
| `asterdrive.storage.onedrive` | OneDrive | Delegated OAuth | [OneDrive](/en/admin/storage-backends/onedrive/) |
| `asterdrive.storage.qiniu` | Qiniu Kodo | Static secret | [Qiniu Kodo](/en/admin/storage-backends/qiniu-kodo/) |
<!-- storage-connectors:policy-catalog:end -->

## Storage Policies vs Policy Groups

- To change "which backend files finally land on" — create or edit a storage policy
- To route different users, teams, or file sizes differently — configure policy groups

Typical console order:

1. Create and test the storage policy
2. Create policy group rules
3. Bind users or teams to the target policy group

Most common patterns:

- The default policy group has a single rule sending everything to the current default policy; a single instance can use a local policy, while multi-Primary deployments should use a shared policy
- With both local and S3 in use, split rules by file size
- Bind different users or teams to different policy groups
- Mark one policy group as the default for new users

A policy group can be disabled first; a disabled group cannot be assigned to new users or teams. To delete a group still bound by users or teams, first use the page's "migrate bindings" action to batch-move those bindings to another group, then delete.

When migrating existing data, do not edit the old policy's path, bucket, endpoint, or bound remote node into the new location. Create the target policy first, use `Admin -> Storage Policies -> Migrate Data` to create a migration task, and only then adjust policy groups.

## Common Storage Policy Fields

| Item | Purpose |
| --- | --- |
| Name | Display name in the console |
| Driver type | `local`, `s3`, `alibaba_oss`, `azure_blob`, `tencent_cos`, `one_drive`, `sftp`, or `remote` |
| Connection info | Local directory / S3 endpoint, bucket, keys / OSS public endpoint, optional server-side endpoint, region, bucket, CNAME, keys / Azure Blob endpoint, container, account keys / COS endpoint, bucket, keys / OneDrive Microsoft Graph target and authorization / SFTP endpoint, SSH credentials, host key fingerprint / bound remote node |
| Base path | Directory, prefix, or remote relative path used when writing through this policy |
| Max single-file size | Largest allowed upload; `0` = unlimited |
| Chunk size | Size of each part for large-file uploads |
| Default policy | Preferred by new default groups or default routing rules |
| Additional options | Local content dedup, S3 / OSS / Azure Blob / COS upload and download modes, S3 path-style access, OSS CNAME, OneDrive drive targeting, SFTP host key fingerprint, remote upload/download modes, storage-native processing switch, etc. |

The console's storage policy form is not hardcoded per vendor on the frontend. AsterDrive reads the fields, capabilities, upload workflows, and management actions supported by the current driver from the backend `StorageConnector` descriptor, so when storage backends are added or adjusted, the admin UI follows backend capabilities as much as possible.

## Reading Connection Test Results

Storage policies have two kinds of connection tests:

- **Test a saved policy**: read-write probe against a policy already saved in the database.
- **Test a draft config**: probe with current form values before saving; for static-credential backends like S3, Alibaba Cloud OSS, Azure Blob, and Tencent COS, leaving the secret field empty reuses the saved credential.

A successful connection test only means the AsterDrive server can reach the backend and the basic read-write paths — credentials, bucket / container / drive / follower remote storage target — work. It does not mean a browser can reach the object storage or follower directly. Whenever `presigned` is used, you still need to check browser networking, HTTPS certificates, CORS, and exposed response headers.

When a connection test fails, the console preferentially shows `error.diagnostic.message` from the standard error response. This diagnostic comes from the backend's classification of storage errors, keeps as much actionable information as possible, and redacts sensitive material like SAS tokens, account keys, and secret keys. Scripts and third-party clients should read it too:

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

The `code` is still the stable error code; `diagnostic.message` is explanatory text for administrators — do not branch on it programmatically.

:::caution[Storage-native processing may incur cloud provider charges]
`Storage-native processing` is a per-policy master switch. Only after it is enabled does AsterDrive call the native data-processing capabilities exposed by the current storage driver; on a Tencent COS policy this corresponds to COS CI.

AsterDrive caches derived results such as thumbnails and media info to avoid re-processing on every file view, but first-time generation or provider-side processing requests may still incur charges. See the [Tencent COS storage policy tutorial](/en/admin/storage-backends/tencent-cos/) for COS-specific configuration, suffix policy, and free-quota notes.
:::

## Capacity Observation and Migration Pre-checks

The storage policy edit dialog shows current capacity observation:

| Policy type | Capacity observation behavior |
| --- | --- |
| `local` | Reads total, available, and used space of the filesystem holding the policy's base directory |
| `s3` / `alibaba_oss` / `tencent_cos` | Returns "unsupported"; these object-storage APIs have no uniform, reliable bucket free-space interface |
| `azure_blob` | Returns "unsupported"; the Blob data API offers no uniform storage-account capacity observation |
| `one_drive` | Reads Microsoft Graph drive quota; if Graph returns no quota, shows "unavailable" |
| `sftp` | Returns "unsupported"; the SFTP protocol has no uniform, reliable remote filesystem capacity interface |
| `remote` | Asks the remote storage target bound to the policy via the internal remote storage protocol; a local target usually reports filesystem capacity, an S3 target again shows "unsupported" |

During data migration, the pre-check compares the target policy's available capacity against the "estimated blob bytes to copy", not simply the source policy's total size. Content SHA-256 blobs already present on the target count as reusable and are excluded from the estimate.

Capacity check states:

| State | Meaning | Blocks task creation |
| --- | --- | --- |
| Sufficient | Target available capacity is greater than or equal to estimated copy bytes | No |
| Insufficient | Target clearly lacks capacity | Yes |
| Unsupported | Driver has no reliable capacity interface, e.g. S3/OSS/COS/Azure Blob | No, but prompts you to confirm capacity |
| Unavailable | This capacity query failed or returned incomplete data | No, but prompts you to confirm capacity |

## Blob Matching Rules During Migration

Migration works per blob and does not re-copy objects for every file record. To avoid incorrect merging, AsterDrive distinguishes two kinds of blob keys:

| Type | How it is recognized | Migration matching rule |
| --- | --- | --- |
| Content SHA-256 | 64-character hex string | If the target policy already has a blob with the same hash and same size, the target object is verified and references are merged |
| Opaque key | Any other blob key | Does not participate in cross-policy matching, and never merges just because key and size match |

If the content SHA-256 hash matches but the size differs, migration fails and the source blob is left unchanged. This usually indicates abnormal database or object-storage state and needs administrator attention.

If an opaque key already exists on the target policy, migration neither overwrites the target object nor merges the source blob into it. The system generates a new `migration-...` key for the source blob, copies the object to the new path on the target policy, and records a "Renamed Opaque Keys" count in the task result.

## Edits You Must Not Make Directly

:::caution[On a policy that already has files written, do not change these]

- Local directory
- Bucket
- Endpoint
- Azure container
- OneDrive drive / root item / site / group targeting fields
- SFTP base path
- Bound remote node

Old files are read from their original location; changing the location in place = every existing file becomes unfindable.

The safer path:

1. Create a new policy
2. Under `Admin -> Storage Policies -> Migrate Data`, pick source and target policies
3. Click `Check Plan` first and confirm target probing, streaming upload capability, and capacity checks have no blockers
4. Create the migration task and confirm completion under `Admin -> Tasks`
5. Switch users or teams to the policy group containing the new policy

:::

## Migrating Existing Policy Data

`Migrate Data` creates a background task that copies existing Blobs from the source policy to the target policy, updating file records and version references along the way.

Before creating the task, the page runs a `Check Plan` round:

- Counts objects and total size under the source policy
- Probes whether the target policy is writable
- Checks whether the target supports the streaming uploads migration needs
- Estimates how many reusable objects already exist on the target, and computes the actual bytes left to copy
- Tries to confirm the target has enough remaining capacity for that data
- Counts opaque key conflicts

The pre-check blocks task creation only when the target clearly lacks capacity. "Unsupported" or "unavailable" capacity checks do not mean migration is impossible; the driver just cannot reliably read free space. Confirm target capacity yourself before actually creating the task.

After the task is created, watch progress under `Admin -> Tasks`. Schedule a maintenance window for large migrations, and avoid heavy new writes to the source policy during the run.

:::caution[Migration is not backup]
Migration moves file objects and reference relationships known to AsterDrive; it does not replace database, configuration, and object-storage backups. Before a production migration, still read [Backup and Restore](/en/ops/backup/) first.
:::

## Routine Maintenance

- Keep at least one usable default storage policy
- Keep at least one enabled default policy group
- Run a connection test before saving
- To assign different storage routes to different users/teams, bind policy groups under `Admin -> Users` or `Admin -> Teams`
- When connecting an external backend, start with the matching tutorial under [Storage Backends](/en/admin/storage-backends/)
