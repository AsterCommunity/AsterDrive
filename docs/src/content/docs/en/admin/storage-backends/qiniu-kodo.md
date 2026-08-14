---
title: "Qiniu Kodo"
description: "Configure Qiniu Cloud Kodo through its S3-compatible API."
---

The Qiniu Kodo connector uses the **Kodo S3-compatible API** and AWS SigV4. AsterDrive continues to own files, versions, quotas, trash, and object cleanup; Kodo stores object content only.

It does not use QBox, UpToken, native form uploads, or Qiniu-native multipart REST. Do not enter a native upload domain or token in this form.

## Before you start

Create least-privilege AccessKey / SecretKey credentials for a dedicated Kodo bucket. Record the S3-compatible endpoint and the SigV4 region required by that endpoint. Enter endpoint and bucket separately; do not include the bucket twice in the endpoint.

Start with `relay_stream` for both upload and download. Enable `presigned` only after server-side reads and writes are proven. Browser-direct use requires a Kodo endpoint reachable by users and CORS allowing the AsterDrive site origin, `GET`, `HEAD`, `PUT`, and the headers needed for Range requests.

## Create a policy

Open `Admin -> Storage Policies -> New Policy`, choose **Qiniu Kodo**, then provide:

| Field | Meaning |
| --- | --- |
| Kodo S3 endpoint | The HTTP(S) S3-compatible service endpoint, without the bucket. |
| Bucket | Target Kodo bucket. |
| Base path | Optional object prefix; empty uses the bucket root. |
| Kodo SigV4 signing region | Region required by the Kodo endpoint, as documented by Qiniu. |
| Path-style addressing | Enabled by default for `/bucket/key`; disable only after verifying virtual-hosted-style support. |
| AccessKey / SecretKey | Static credentials dedicated to this policy; SecretKey is never returned with the policy. |

Run the draft connection test before saving and the saved-policy test afterwards. Tests write and delete a temporary object, so credentials need write and delete permissions for the target prefix.

## Acceptance and troubleshooting

Bind a test user or team through a policy group and verify small upload, multipart upload, download, Range preview, deletion, and object cleanup. After enabling presigned mode, verify browser PUT, GET/HEAD, Range, and CORS behavior from a real browser.

For failures, check endpoint reachability, bucket, region, path-style, credentials, permissions, and server time in that order. Never paste SecretKey values or complete signed URLs into logs, error reports, or tickets.

If endpoint, bucket, base path, region, or addressing style changes, create a target policy and use a storage migration task for existing blobs. Editing a live policy directly can make old object paths unreadable.
