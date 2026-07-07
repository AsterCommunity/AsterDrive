---
description: AsterDrive uploads and storage feature map covering upload modes, blobs, quota, storage policies, policy groups, local storage, S3, Azure Blob, Tencent COS, OneDrive, SFTP, follower storage, and routing behavior.
---

# Uploads and Storage

Uploads and storage turn a file sent by a browser or client into a database file record plus an object in a storage driver. This area is highly affected by reverse proxies, CORS, object storage, and follower-node topology.

## Capability Boundaries

| Capability | Notes | Related docs |
| --- | --- | --- |
| Direct small-file upload | Browser posts to primary; server writes to the target storage | [Uploads and Large Files](/en/guide/upload-modes) |
| Chunked upload | Local chunk sessions, progress, resume, 24h session TTL | [Uploads and Large Files](/en/guide/upload-modes) |
| Object-storage presigned upload | Browser PUTs directly to S3-compatible storage, Azure Blob SAS URLs, or Tencent COS; server verifies and finalizes | [Uploads and Large Files](/en/guide/upload-modes), [Storage Policies](/en/config/storage) |
| Object-storage multipart | Browser uploads parts in batches; server completes and validates content | [Uploads and Large Files](/en/guide/upload-modes), [S3 / MinIO / R2](/en/storage/s3-minio-r2), [Azure Blob Storage](/en/storage/azure-blob), [Tencent COS](/en/storage/tencent-cos) |
| Microsoft Graph storage | Writes files to OneDrive, SharePoint site drives, or Microsoft 365 group drives after administrator authorization | [OneDrive](/en/storage/onedrive), [Storage Policies](/en/config/storage) |
| SFTP storage | Server-side streaming reads and writes to an SSH/SFTP file server | [SFTP](/en/storage/sftp), [Storage Policies](/en/config/storage) |
| Storage policies | Decide whether files land on local, s3, sftp, azure_blob, tencent_cos, one_drive, or remote | [Storage Policies](/en/config/storage) |
| Policy groups | Route by user, team, and file size to storage policies | [Storage Policies](/en/config/storage) |
| Follower storage | Primary writes objects to a follower; the follower stores them locally or in S3 | [Follower Node Enrollment](/en/guide/remote-nodes), [Follower Node Storage Policy](/en/storage/remote-follower) |

## Backend Modules

| Module | Owns |
| --- | --- |
| `upload_service` | Upload sessions, chunks, progress, status transitions |
| `workspace::storage_core` | Blob dedupe, file records, quota, policy choice, finalization |
| `storage_policy::policy` | Storage policies, policy groups, rules |
| `storage::traits`, `storage::drivers`, `storage::connectors` | `StorageDriver` and `StorageConnector` abstractions, local, S3-compatible, SFTP, Azure Blob, Tencent COS, OneDrive, and remote drivers |
| `storage::remote_protocol` | Primary/follower internal remote storage protocol |
| `remote::remote_node`, `managed_ingress_profile_service` | Follower nodes and ingress targets |
| `task_service::storage_migration` | Storage migration tasks |

## Key Boundaries

- Quota checks have two layers: fast-fail outside the transaction and authoritative checking inside the transaction.
- Filename uniqueness, blob ref counts, and upload-session state transitions must use repository-level atomic helpers.
- Whether `presigned` works depends on browser reachability to object storage or follower `base_url`, not only primary reachability.
- A successful admin connection test only proves that the AsterDrive server can reach the backend. `presigned` upload / download still needs browser network and CORS checks for object storage, Azure Blob, or follower endpoints.
- SFTP is a server-side streaming backend. Its operational checks are Endpoint, SSH credentials, base path, and host key fingerprint; browser CORS and presigned URLs are not involved.
- Follower `reverse_tunnel` is suitable for `relay_stream`, not browser-direct `presigned`.

## Troubleshooting Direction

- Small files upload but large files fail: check reverse proxy body size, timeout, temporary directories, and chunk size.
- `relay_stream` works but `presigned` fails: check CORS, browser network, and endpoint reachability.
- Connection test fails: read the backend diagnostic first. Storage diagnostics are returned in the standard error response as `error.diagnostic.message`.
- Follower storage fails: check node enabled state, transport mode, default ingress target, and protocol capabilities.
- Quota or blob references drift: run a deep check with [Operations CLI](/en/deployment/ops-cli).
