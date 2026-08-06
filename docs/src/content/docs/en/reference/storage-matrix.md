---
description: "Storage capability matrix for the eight backends: browser direct upload / download, capacity observation, storage-native processing, credentials at rest, and the authoritative relay_stream vs presigned comparison."
title: "Storage Capability Matrix"
---

:::tip[This page is a quick reference, not a tutorial]
Per-backend onboarding steps live in the [Storage Backends](/en/admin/storage-backends/) tutorials; the storage policy and policy group concepts live in [Storage Policies and Policy Groups](/en/admin/storage-policies/). This page only answers "what can each backend do" and "which path do uploads / downloads take".
:::

## Capability Quick Reference

| Backend | Browser direct upload / download | Capacity observation | Storage-native processing | Credentials at rest |
| --- | --- | --- | --- | --- |
| `local` | Not supported; AsterDrive reads/writes the local disk | Supported (filesystem) | Not supported | No credentials |
| `s3` | `presigned` upload + download | Not supported | Not supported | Plaintext |
| `alibaba_oss` | `presigned` upload + download; browsers always use the public endpoint | Not supported | Not supported | AES-256-GCM encrypted |
| `azure_blob` | `presigned` (SAS URL) upload + download | Not supported | Not supported | Plaintext |
| `tencent_cos` | `presigned` upload + download | Not supported | COS CI (per-policy switch) | Plaintext |
| `one_drive` | `frontend_direct` upload, Graph direct download | Supported (Graph quota) | Not supported | AES-256-GCM encrypted |
| `sftp` | Not supported; server-side streaming | Not supported | Not supported | Plaintext |
| `remote` | `presigned` requires direct mode plus a browser-reachable follower `base_url` | Follows the remote storage target | Not supported | Plaintext |

The encryption master key for OneDrive credentials is `[auth].storage_credential_secret_key` in `config.toml`; keep it safe across migrations and restores. See [Login and Sessions](/en/reference/config/auth/#storage_credential_secret_key).

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
