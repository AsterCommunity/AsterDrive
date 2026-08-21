---
description: Huawei Cloud OBS storage policy tutorial covering native OBS signing, regional endpoints, custom domains, credentials, CORS, presigned URLs, and multipart validation.
title: "Huawei Cloud OBS Storage Policy Tutorial"
---

:::tip[What this page covers]
This page explains how to write AsterDrive files to Huawei Cloud OBS: prepare a bucket and OBS credentials, create the `asterdrive.storage.huawei_obs` connector policy, choose regional or custom-domain addressing, configure policy groups, and validate relay or presigned transfers.

Huawei Cloud OBS uses native OBS signing in AsterDrive. It is not a generic S3 policy with an OBS endpoint pasted into the form. For AWS SigV4 or generic S3-compatible services, use the [S3 / MinIO / R2 storage policy tutorial](/en/admin/storage-backends/s3/).
:::

## When to use it

Huawei Cloud OBS is a good fit when you:

- already use OBS and want AsterDrive to write to a specific bucket;
- need the native OBS signing contract instead of generic S3 AWS SigV4;
- need OBS multipart, Range reads, presigned URLs, or custom-domain access; or
- want the admin console to show OBS explicitly, with endpoint, region, and addressing mode visible for review.

## Choose an addressing mode

| Mode | Endpoint example | `obs_region` | Request URL | Use it when |
| --- | --- | --- | --- | --- |
| `virtual_hosted` | `https://obs.cn-north-4.myhuaweicloud.com` | Required, for example `cn-north-4` | `https://BUCKET.obs.REGION.myhuaweicloud.com/OBJECT` | Using an official regional endpoint; recommended default |
| `custom_domain` | `https://files.example.com` | May be empty | `https://files.example.com/OBJECT` | A custom hostname is bound to the OBS bucket |

You may enter either the official regional root endpoint or a bucket-prefixed form such as:

```text
https://archive-bucket.obs.cn-north-4.myhuaweicloud.com/
```

AsterDrive normalizes that value to the regional root and generates virtual-hosted requests using the bucket. Generic S3 endpoints, path-prefixed endpoints, and endpoints containing a query or fragment are rejected before saving.

Custom-domain mode sends requests directly to the OBS-bound hostname. AsterDrive does not prepend the bucket to that hostname, and its canonical signed resource follows the OBS SDK's CNAME behavior. Do not mark an official OBS endpoint as a custom domain.

## 1. Prepare the OBS bucket and credentials

Create or select a dedicated OBS bucket, for example:

```text
archive-bucket
```

Plan a separate prefix for each AsterDrive instance when appropriate:

```text
prod/
```

Do not let multiple instances share an unplanned prefix. AsterDrive's delete, migration, and cleanup tasks depend on object paths recorded in the database.

Create a least-privilege OBS credential for AsterDrive. It must cover the operations enabled for the policy, typically:

- listing the target bucket or prefix;
- reading objects and object metadata;
- writing objects;
- deleting objects; and
- multipart initiation, part upload, part listing, completion, and abort.

Use Huawei Cloud's current OBS documentation for the exact IAM action names. Do not place an account-wide administrative credential in AsterDrive.

## 2. Choose upload and download paths

For the first rollout, use server relay:

| Direction | Recommended initial value | Reason |
| --- | --- | --- |
| Upload | `relay_stream` | The browser does not contact OBS; validate signing, permissions, and object paths first |
| Download | `relay_stream` | Keep the response through AsterDrive while troubleshooting |

After basic reads, writes, shares, and Range requests are stable, consider `presigned`:

```text
Browser -> OBS
AsterDrive only issues a short-lived OBS URL
```

Presigned mode requires the browser to reach the OBS endpoint or custom domain, and requires correct OBS CORS, HTTPS certificates, and exposed response headers.

## 3. Configure OBS CORS

When using only `relay_stream`, the browser does not call OBS directly, so CORS can be configured later. Before enabling presigned uploads or downloads, verify:

See the [S3 / MinIO / R2 tutorial's CORS section](/en/admin/storage-backends/s3/#12-configure-cors-for-presigned) for the general `presigned` rules; the OBS-specific console field mapping is listed below.

- `AllowedOrigin` includes the AsterDrive public site origin, such as `https://drive.example.com`;
- uploads allow `PUT` and the request headers sent by AsterDrive;
- downloads allow `GET`, `HEAD`, and the headers needed for Range requests;
- `ExposeHeader` includes `ETag`; multipart direct uploads need part ETags; and
- the presigned hostname, certificate, and browser network path are reachable.

In the Huawei Cloud OBS console, start with the following rule for presigned single-object and multipart uploads:

| OBS field | Recommended value |
| --- | --- |
| Allowed origins | The actual AsterDrive page origin; use `*` temporarily while diagnosing |
| Allowed methods | `GET`, `HEAD`, `PUT` |
| Allowed headers | `Content-Type`; use `*` temporarily while diagnosing |
| Exposed headers | `ETag`; add `Content-Length`, `Content-Range`, and `Accept-Ranges` for Range downloads |
| Cache time | `3600` |

`ETag` is an upload response header and belongs in the exposed-header field, not the request-header allowlist used by preflight. Do not copy AWS S3 `x-amz-*` headers into an OBS rule; the browser preflight currently needs at least `Content-Type`. When the console reports `OPTIONS` 403, check that origin, `PUT`, and `Content-Type` all match the same rule.

AsterDrive's connection test validates the endpoint, credentials, and basic object requests from the server. It does not replace browser-side CORS and network validation.

## 4. Create a Huawei Cloud OBS storage policy

Go to:

```text
Admin -> Storage Policies -> New Policy
```

Choose:

```text
Huawei Cloud OBS
```

If an existing generic `s3` policy already uses an official OBS endpoint and an explicit `s3_region`, the admin console can offer the `promote_from_s3` connector upgrade. It switches the connector and encrypted credentials in place without copying objects; the bucket, base path, endpoint, region, and object namespace remain unchanged. Generic S3 endpoints, `s3_region = auto`, and mismatched endpoints are not eligible for the upgrade recommendation.

Typical values:

| Field | `virtual_hosted` example | `custom_domain` example |
| --- | --- | --- |
| Endpoint | `https://obs.cn-north-4.myhuaweicloud.com` | `https://files.example.com` |
| Bucket | `archive-bucket` | `archive-bucket` |
| OBS region | `cn-north-4` | May be empty |
| OBS addressing mode | `virtual_hosted` | `custom_domain` |
| Base path | `prod/` | `prod/` |
| Access Key ID | Huawei Cloud AK | Huawei Cloud AK |
| Secret Access Key | Huawei Cloud SK | Huawei Cloud SK |

Signing is fixed to the native OBS protocol by the connector driver and is not an administrator-facing policy field. Do not put an OBS endpoint into a generic S3 policy or switch it to AWS SigV4.

## 5. Test the connection and configure a policy group

Before or after saving, run `Test Connection` and verify:

1. the AsterDrive server reaches the endpoint;
2. the bucket and region match;
3. the AK/SK can read, write, delete, and use multipart under the target prefix;
4. the custom domain is actually bound to the target bucket; and
5. the AsterDrive server clock is accurate.

When editing a saved policy, leaving credential fields blank lets draft tests reuse the saved static credential. A new policy still needs complete credentials.

Create a test policy group and bind one test user or team to it. Do not change the default policy group or a policy serving production traffic as the first experiment.

## 6. Validate the complete workflow

With the test account, run:

- small-file upload and download;
- large-file multipart upload;
- presigned upload, if enabled;
- presigned download, if enabled;
- image or video Range reads;
- object metadata reads;
- delete and recycle-bin restore;
- shared-link download; and
- multipart retry and cleanup after a failed upload.

When inspecting OBS or AsterDrive logs, never record AK, SK, temporary tokens, or complete presigned URLs. Confirm that objects land under the expected bucket and prefix before moving real users or teams to the policy group.

## Troubleshooting

### OBS endpoint entered under `s3`

The ordinary `s3` connector uses AWS SigV4 and does not represent a native OBS policy. Choose **Huawei Cloud OBS**; the connector driver fixes the native OBS signing protocol internally.

### Endpoint validation fails

Check that:

- `virtual_hosted` uses `obs.<region>.myhuaweicloud.com` or an official regional suffix;
- `obs_region` matches the region in the hostname;
- the endpoint has no path prefix, query, fragment, username, or password; and
- `custom_domain` contains the bound hostname, not `bucket.obs.<region>...`.

### Server test passes but browser presigned requests fail

These are different network paths. Check:

- DNS and browser reachability for the presigned hostname;
- OBS CORS origin, methods, request headers, and exposed headers;
- HTTPS certificate coverage for the actual hostname; and
- whether `GET`, `HEAD`, `PUT`, and Range requests are allowed.

## Official references

- [Huawei Cloud OBS authorization header](https://support.huaweicloud.com/intl/en-us/api-obs/obs_04_0010.html)
- [Huawei Cloud OBS presigned URL](https://support.huaweicloud.com/intl/en-us/api-obs/obs_04_0011.html)
- [Huawei Cloud OBS object listing](https://support.huaweicloud.com/intl/en-us/api-obs/obs_04_0022.html)
- [Huawei Cloud OBS Go SDK](https://github.com/huaweicloud/huaweicloud-sdk-go-obs)
