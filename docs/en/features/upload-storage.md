---
description: AsterDrive uploads and storage feature map covering upload modes, blobs, quota, storage policies, policy groups, storage backends, and routing behavior.
---

# Uploads and Storage

Uploads and storage turn a file sent by a browser or client into a database file record plus an object in a storage driver. This area is highly affected by reverse proxies, CORS, object storage, and follower-node topology.

## Capability Boundaries

| Capability | Notes | Related docs |
| --- | --- | --- |
| Direct small-file upload | Browser posts to primary; server writes to the target storage | [Uploads and Large Files](/en/guide/upload-modes) |
| Chunked upload | Local chunk sessions, progress, resume, 24h session TTL | [Uploads and Large Files](/en/guide/upload-modes) |
| Object-storage presigned upload | Browser PUTs directly to S3-compatible storage or Tencent COS; server verifies and finalizes | [Uploads and Large Files](/en/guide/upload-modes), [Storage Policies](/en/config/storage) |
| Object-storage multipart | Browser uploads parts in batches; server completes and validates content | [Uploads and Large Files](/en/guide/upload-modes), [S3 / MinIO / R2](/en/storage/s3-minio-r2), [Azure Blob Storage](/en/storage/azure-blob), [Tencent COS](/en/storage/tencent-cos) |
| Storage policies | Decide whether files land on local, s3, tencent_cos, or remote | [Storage Policies](/en/config/storage) |
| Policy groups | Route by user, team, and file size to storage policies | [Storage Policies](/en/config/storage) |
| Follower storage | Primary writes objects to a follower; the follower stores them locally or in S3 | [Follower Node Enrollment](/en/guide/remote-nodes), [Follower Node Storage Policy](/en/storage/remote-follower) |

## Backend Modules

| Module | Owns |
| --- | --- |
| `upload_service` | Upload sessions, chunks, progress, status transitions |
| `workspace_storage_core` | Blob dedupe, file records, quota, policy choice, finalization |
| `policy_service` | Storage policies, policy groups, rules |
| `storage::traits`, `storage::drivers` | `StorageDriver` abstraction, local and S3-compatible drivers |
| `storage::remote_protocol` | Primary/follower internal remote storage protocol |
| `managed_follower_service`, `managed_ingress_profile_service` | Follower nodes and ingress targets |
| `task_service::storage_migration` | Storage migration tasks |

## Key Boundaries

- Quota checks have two layers: fast-fail outside the transaction and authoritative checking inside the transaction.
- Filename uniqueness, blob ref counts, and upload-session state transitions must use repository-level atomic helpers.
- Whether `presigned` works depends on browser reachability to object storage or follower `base_url`, not only primary reachability.
- Follower `reverse_tunnel` is suitable for `relay_stream`, not browser-direct `presigned`.

## Troubleshooting Direction

- Small files upload but large files fail: check reverse proxy body size, timeout, temporary directories, and chunk size.
- `relay_stream` works but `presigned` fails: check CORS, browser network, and endpoint reachability.
- Follower storage fails: check node enabled state, transport mode, default ingress target, and protocol capabilities.
- Quota or blob references drift: run a deep check with [Operations CLI](/en/deployment/ops-cli).
