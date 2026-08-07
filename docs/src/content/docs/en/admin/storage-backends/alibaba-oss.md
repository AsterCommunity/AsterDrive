---
description: Alibaba Cloud OSS storage policy tutorial covering native OSS V4 signing, public and server-side endpoints, CNAME, transfer modes, CORS, and production validation.
title: "Alibaba Cloud OSS Storage Policy Tutorial"
---

:::tip[What this page covers]
This page explains how to store AsterDrive files in Alibaba Cloud Object Storage Service. AsterDrive uses native `OSS4-HMAC-SHA256` signing; it does not disguise a Cloudreve or native OSS policy as generic S3-compatible storage.
:::

## When to Choose Alibaba Cloud OSS

- You already have an OSS bucket and need native endpoint and region semantics
- AsterDrive backend traffic should use an OSS internal endpoint while browser direct transfers use the public endpoint
- The bucket is bound to a custom CNAME domain
- You need `relay_stream`, `presigned`, and multipart upload paths

An endpoint that merely resembles S3 does not provide native OSS behavior. OSS V4 uses a different algorithm, credential scope, canonical URI, and query-signing contract from AWS SigV4.

## Entry Points

| Task | Location |
| --- | --- |
| Create the bucket, AccessKey, and CORS rules | Alibaba Cloud OSS console |
| Create the policy | `Admin -> Storage Policies -> New Policy -> Alibaba Cloud OSS` |
| Assign users or teams | `Admin -> Policy Groups` |
| Compare `relay_stream` and `presigned` | [Storage Capability Matrix](/en/reference/storage-matrix/) |

## 1. Prepare the OSS Bucket

1. Create a dedicated bucket and record its region, for example `cn-hangzhou`
2. Record the public endpoint, for example `https://oss-cn-hangzhou.aliyuncs.com`
3. If AsterDrive and OSS share a cloud network, record a server-side internal endpoint such as `https://oss-cn-hangzhou-internal.aliyuncs.com`
4. Create an AccessKey scoped to this bucket with the object read, write, delete, list, and multipart permissions AsterDrive needs
5. If you plan to use `presigned`, configure OSS CORS for the AsterDrive site

:::caution[Alibaba Cloud international-site accounts]
The [Alibaba Cloud China documentation](https://help.aliyun.com/zh/oss/user-guide/regions-and-endpoints) continues to list `oss-cn-<region>.aliyuncs.com` as the public endpoint for Chinese mainland regions, so the default configuration above applies to China-site accounts. Alibaba Cloud [separately states](https://www.alibabacloud.com/en/notice/oss_update_notice_policy_change_in_calling_data_api_operations_via_the_default_public_domain_name_45a) that international-site users who activate OSS after March 20, 2025 at 00:00:00 (UTC+8) cannot call data API operations for buckets in Chinese mainland regions through default public domain names. International-site users who activated OSS before that time and internal domain names are not affected. If a request returns `PublicEndpointForbidden` (HTTP 400, EC `0048-00000401`), [bind a custom domain to the bucket](https://www.alibabacloud.com/help/en/oss/user-guide/access-buckets-via-custom-domain-names), enable **Use CNAME custom domain**, and use that domain as the public endpoint. Browser presigned uploads and downloads must also use the CNAME domain in this case.
:::

Do not paste the AccessKey into logs, screenshots, or issues. AsterDrive encrypts connector credentials at rest; backups and migrations must also preserve `[auth].storage_credential_secret_key`.

## 2. Understand the Three Endpoint Settings

| Field | Purpose |
| --- | --- |
| Public endpoint | Generates browser-visible presigned URLs; also handles backend I/O when no server-side endpoint is set |
| Server-side endpoint | Optional and used only by AsterDrive backend requests; never appears in browser presigned URLs |
| Use CNAME custom domain | Treats the public endpoint as a custom domain already bound to the current bucket |

In normal mode, endpoints must use an `aliyuncs.com` OSS hostname. In CNAME mode, the public endpoint must be a custom domain. The bucket remains part of the OSS V4 canonical URI but is not repeated in the transmitted URL path.

:::caution[CNAME does not replace the server-side endpoint]
When a server-side endpoint is configured, backend I/O uses it while browser presigned URLs continue to use the public endpoint. Validate DNS, HTTPS, and reachability for the backend and browser paths separately.
:::

## 3. Create the AsterDrive Storage Policy

Under `Admin -> Storage Policies`, create **Alibaba Cloud OSS** and fill in:

- Public endpoint
- Optional server-side endpoint
- OSS region
- Bucket
- Optional base path
- CNAME mode
- AccessKey ID / AccessKey Secret
- Upload and download modes

Start with `relay_stream` for connection testing and end-to-end validation. After server-side reads, writes, Range requests, deletes, copies, and multipart operations are stable, switch to `presigned` if needed.

## 4. Configure CORS for Presigned Transfers

Browser direct transfers normally need the AsterDrive site origin to use `GET`, `HEAD`, `PUT`, `POST`, and `DELETE`, and send the headers used by uploads. Single-object presigned PUT completion verifies object metadata and size server-side, so it does not require browser access to `ETag`; presigned multipart parts still require `ETag` to complete the multipart object. Expose `ETag` when using multipart, and expose `Content-Length` / `Content-Range` when the selected workflow reads them. Use the current OSS console CORS form as the authority for exact field names.

If the connection test succeeds but browser direct transfer fails, check:

1. The browser URL uses the public endpoint or CNAME, not the internal endpoint
2. CORS `AllowedOrigin` exactly matches the origin in the browser address bar
3. Multipart part `ETag` responses are visible to browser JavaScript
4. The custom-domain TLS certificate covers the selected hostname

## 5. Configure a Policy Group and Validate

Create a test policy group, bind one test user or team, then verify:

- Small-file upload, download, delete, and restore
- Large multipart upload, resume, and cancellation
- Range download, PDF / video seeking, and image preview
- File copy, move, and overwrite conflicts
- Public-share download
- Both `relay_stream` and `presigned` upload/download modes

Until validation is complete, do not edit the bucket, endpoint, region, CNAME, or base path of a policy that already owns files. Together, those fields determine the real object location and signing behavior.

## Troubleshooting

### `SignatureDoesNotMatch`

Confirm that the region matches the bucket endpoint, credentials contain no extra whitespace, system time is correct, and the native OSS policy was not entered through the generic S3 connector.

### Backend Works but Browser Presigned URLs Fail

The server-side endpoint only proves backend reachability. Check the public endpoint / CNAME, HTTPS, DNS, and CORS separately.

### The Bucket Appears Again in a CNAME URL

Enable **Use CNAME custom domain** and make sure the public endpoint is the custom domain bound to the bucket, not an `aliyuncs.com` provider endpoint.
