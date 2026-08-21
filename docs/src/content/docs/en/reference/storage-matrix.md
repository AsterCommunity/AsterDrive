---
description: "Capability matrix for built-in storage backends: deployment scope, browser direct upload / download, capacity, storage-native processing, credential mode, and relay_stream vs presigned."
title: "Storage Capability Matrix"
---

:::tip[This page is a quick reference, not a tutorial]
Per-backend onboarding steps live in the [Storage Backends](/en/admin/storage-backends/) tutorials; the storage policy and policy group concepts live in [Storage Policies and Policy Groups](/en/admin/storage-policies/). This page only answers "what can each backend do" and "which path do uploads / downloads take".
:::

## Capability Quick Reference

<!-- storage-connectors:matrix:start -->
| Backend | Deployment scope | Browser direct upload | Direct download | Capacity | Storage-native processing | Credential mode |
| --- | --- | --- | --- | --- | --- | --- |
| [Local](/en/admin/storage-backends/local/) | Instance-local | No | No | Yes | No | None |
| [S3](/en/admin/storage-backends/s3/) | Shared across Primary instances | Presigned | Yes | No | No | Static secret |
| [Alibaba Cloud OSS](/en/admin/storage-backends/alibaba-oss/) | Shared across Primary instances | Presigned | Yes | No | No | Static secret |
| [SFTP](/en/admin/storage-backends/sftp/) | Shared across Primary instances | No | No | No | No | Static secret |
| [Azure Blob](/en/admin/storage-backends/azure-blob/) | Shared across Primary instances | Presigned | Yes | No | No | Static secret |
| [Huawei Cloud OBS](/en/admin/storage-backends/huawei-obs/) | Shared across Primary instances | Presigned | Yes | No | No | Static secret |
| [Tencent COS](/en/admin/storage-backends/tencent-cos/) | Shared across Primary instances | Presigned | Yes | No | Thumbnail + media metadata | Static secret |
| [Remote](/en/admin/storage-backends/remote-follower/) | Shared across Primary instances | Presigned | Yes | Yes | No | None |
| [OneDrive](/en/admin/storage-backends/onedrive/) | Shared across Primary instances | Provider-direct | Yes | Yes | No | Delegated OAuth |
| [Qiniu Kodo](/en/admin/storage-backends/qiniu-kodo/) | Shared across Primary instances | Presigned | Yes | No | No | Static secret |
<!-- storage-connectors:matrix:end -->

The table shows each connector's static capability ceiling; policy settings and deployment topology can narrow the usable paths. For example, remote-node `presigned` transfer requires direct mode and a browser-reachable follower `base_url`, while its capacity result depends on the remote storage target.

`Static secret` and `Delegated OAuth` describe how credentials are acquired, not a plaintext database format. Every connector-managed static secret, authorization-application secret, and OAuth token is encrypted at rest with AES-256-GCM using `[auth].storage_credential_secret_key`. Preserve that key across backups and migrations. See [Authentication and Sessions](/en/reference/config/auth/#storage_credential_secret_key).

## `relay_stream` vs `presigned`

This section is the **single authoritative explanation** of upload / download modes; each backend tutorial only covers its own enablement conditions and differences.

| Mode | Data path | Upside | Cost |
| --- | --- | --- | --- |
| `relay_stream` | Browser ↔ AsterDrive ↔ storage backend | Browser never touches the backend directly, no CORS pitfalls; works with intranet backends; easier to troubleshoot | Traffic flows through AsterDrive, consuming node bandwidth and connections |
| `presigned` / direct | Browser ↔ storage backend (AsterDrive only signs URLs) | Offloads AsterDrive bandwidth; steadier for large files and high concurrency | Browser must reach the backend; requires CORS, HTTPS certificates, and exposed response headers |

Suggested order: bring up any new backend with `relay_stream` first — uploads, downloads, previews, shares — and switch to `presigned` per the tutorial only after it proves stable.

Before switching to `presigned`, confirm:

- Browsers can directly reach the object storage endpoint or the follower `base_url` (usually a real HTTPS hostname)
- Backend CORS allows the AsterDrive site's origin and exposes the response headers downloads and Range requests need
- Public shares, image previews, and PDF / video Range requests were all re-validated under the new mode

OneDrive is the exception: cross-origin support for its `frontend_direct` upload is provided by Microsoft, so no extra CORS setup is needed on the AsterDrive or object-storage side.
