---
description: Huawei Cloud OBS storage policy tutorial covering native signing, CNAME, Type A / SHA-256 CDN delivery, private origins, CORS, and multipart validation.
title: "Huawei Cloud OBS Storage Policy Tutorial"
---

See the [provider evidence index](https://drive.docs.astercosm.com/developer/en/reference/storage-provider-evidence/#huawei_obs) for protocol sources, versions, review status and cloud acceptance boundaries. Validate origin APIs, upload endpoints, CDN and custom domains separately.

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

## Custom domains (CNAME)

`obs_use_cname` has the same meaning as OSS `oss_use_cname` and COS `cos_use_cname`: disabled lets the driver construct the official bucket address; enabled directly uses a domain already bound to that bucket. Object I/O, multipart, and presigned URLs use the same switch, without duplicating the bucket in the final host or path. Each driver handles its provider's signing protocol.

| `obs_use_cname` | Endpoint example | `obs_region` | Request URL | Use it when |
| --- | --- | --- | --- | --- |
| `false` | `https://obs.cn-north-4.myhuaweicloud.com` | Required, for example `cn-north-4` | `https://BUCKET.obs.REGION.myhuaweicloud.com/OBJECT` | Using an official regional endpoint; recommended default |
| `true` | `https://files.example.com` | May be empty | `https://files.example.com/OBJECT` | A custom hostname is bound to the OBS bucket |

You may enter either the official regional root endpoint or a bucket-prefixed form such as:

```text
https://archive-bucket.obs.cn-north-4.myhuaweicloud.com/
```

AsterDrive normalizes that value to the regional root and generates virtual-hosted requests using the bucket. Generic S3 endpoints, path-prefixed endpoints, and endpoints containing a query or fragment are rejected before saving.

Custom-domain mode sends requests directly to the OBS-bound hostname. AsterDrive does not prepend the bucket to that hostname, and its canonical signed resource follows the OBS SDK's CNAME behavior. Do not mark an official OBS endpoint as a custom domain.

The **Use a custom domain (CNAME)** switch maps to connector field `obs_use_cname` and defaults to `false`. It follows the official Go SDK's [`WithCustomDomainName(bool)`](https://github.com/huaweicloud/huaweicloud-sdk-go-obs/blob/master/obs/conf.go) and Python SDK's `is_cname` option. The upgrade migration converts legacy `obs_addressing_mode=virtual_hosted` / `custom_domain` to `false` / `true` in policies, remote targets, and deleted-policy cleanup snapshots without changing existing object paths or signing semantics. Upgrade all nodes together; API clients must use the boolean field. Run the corresponding down migration before reverting to an older binary. Finish active OBS storage-migration tasks before upgrading and regenerate unexecuted migration preflight plans afterward.

OBS supports CDN origins, but CNAME mode does not enable CDN download authentication. Use the separate CDN delivery settings below for accelerated downloads; CNAME can still address the OBS API origin.

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

## 2.1 Optional: Huawei CDN download delivery

AsterDrive supports Huawei CDN **Type A URL authentication with SHA-256**. The default is **OBS origin (native OBS signature)**. Choosing **Huawei CDN accelerated download (Type A + SHA-256)** reveals the required CDN domain, validity, authentication parameter, and key.

| Field | Setting |
| --- | --- |
| Download strategy | `presigned`; `relay_stream` continues to use AsterDrive relay while preserving CDN settings |
| `download_delivery` | Default `obs_origin`; select `huawei_cdn_type_a_sha256` for CDN |
| `download_base_url` | HTTPS acceleration domain, for example `https://cdn.example.com`, without credentials, non-default port, path, query, or fragment |
| `obs_cdn_auth_validity_seconds` | Default 300 seconds, range 1–31536000; must match the Huawei CDN console |
| `obs_cdn_auth_param` | Default `auth_key`, matching the console; 1–100 ASCII letters, digits or underscores, starting with a letter |
| `obs_cdn_auth_key` | Active Type A primary or backup key, 6–32 ASCII letters or digits; encrypted separately from OBS AK/SK |

Object paths continue to use the policy's **Base path**. With `prod`, the download path is `/prod/OBJECT_KEY`; do not add another prefix to the CDN domain. Uploads, multipart, listing, deletion, cleanup, and server relay continue to use the OBS API endpoint.

### Cloud preparation and AsterDrive configuration

1. Add the acceleration domain in Huawei CDN, enable HTTPS, and select the existing OBS bucket as origin.
2. For a private bucket, complete CDN authorization to access the private OBS origin. Check the bucket, origin Host and permissions; do not make the bucket public to enable CDN delivery.
3. Under the domain's **Access Control → URL Authentication**, enable **Method A**, **SHA256**, and authentication for **all files**. AsterDrive object keys may have no extension, so extension-only rules are insufficient.
4. Enter the same active primary or backup key, authentication parameter and validity in AsterDrive. AsterDrive requires positive validity and does not use the console's zero-second setting.
5. Configure CORS on the CDN domain for the AsterDrive site. Check GET, HEAD, Range and required exposed response headers such as `Content-Range`, `Content-Length`, `Content-Type`, `Content-Disposition`, and `ETag`. OBS bucket CORS does not replace CDN CORS.
6. Preserve object path case and encoding without rewrites that change object identity. Authenticate the final encoded request path; validate Chinese characters, spaces, `+`, `%`, `#`, and `?` separately.
7. Configure **Ignore URL Parameters** and other cache rules according to Huawei's documentation so random authentication values do not unnecessarily split the cache. Ignoring parameters in the cache key, stripping them before origin fetches, and authentication on cache hits are separate behaviors to verify. Restrict unexpected business queries, especially OBS `response-*` overrides.

**The ordinary connection test validates the OBS API endpoint and local CDN configuration completeness.** It does not read or modify the CDN console and cannot prove edge authentication, CORS, caching, or private-origin authorization.

:::caution[Client authentication and private-origin authorization are separate]
Giving CDN permission to read a private OBS bucket does not require clients to authenticate. Enable Type A / SHA-256 for all paths as well. AsterDrive signs links only after existing file or share permission checks; this cannot replace cloud-side access control.
:::

### Link validity and caching

Type A emits `auth_key=timestamp-rand-0-sha256`, using the configured parameter name and a UUID without hyphens. The digest follows Huawei's `Filename-Timestamp-rand-uid-PrivateKey` formula. Filename is the final encoded path without query. AsterDrive adds neither OBS signatures nor unauthenticated response-header overrides to the CDN URL.

**The validity boundary is `timestamp + console validity`, not timestamp alone.** AsterDrive limits the remaining lifetime to the smaller of the caller's request and configured validity. Shorter requests backdate the signing time so the resource handle's expiration matches. With 300 seconds configured and 60 requested, the signing time is backdated 240 seconds; it is not set to the current time plus 60 seconds.

**Authentication validity is separate from cache retention.** An object may remain cached while expired or invalid credentials must still be rejected. Check browser caching response headers separately from edge retention; configure an appropriate CDN response policy such as `Cache-Control: private, no-store` when browsers must not retain private downloads. Share revocation and permission changes do not immediately revoke issued links, which remain reusable during their validity. Urgent revocation requires cloud-side key rotation, accounting for concurrently valid primary and backup keys.

### Requests eligible for CDN

- Authorized browser Blob downloads and safe previews can use CDN. The frontend saves the current file name and reconstructs MIME from the resource handle.
- Exact attachment names, strict response headers, and requests requiring application-origin isolation retain relay. Inline webpage previews requiring isolation are not delivered directly through CDN.
- OBS-origin mode never activates CDN merely because a CDN domain remains saved. Switching modes preserves recoverable settings.
- This release does not support B/C1/C2, MD5, upload acceleration, or application integration for M3U8 subresource authentication inheritance.

### Layered acceptance

Validate ordinary OBS/CNAME requests, presigned PUT/GET, multipart and cleanup before CDN:

1. Check a freshly issued GET, HEAD and one-byte Range, including returned content and `Content-Range`.
2. On a cold object and again after filling the cache with a valid link, check missing signatures, wrong keys, modified path/time/digest and expired links. All must be rejected by CDN. Verify Huawei's inclusive expiration boundary: current time is less than or equal to timestamp plus validity.
3. Prove the first origin fetch can read the private bucket, then check caching and authentication on repeated requests. Confirm the intended CDN domain, origin bucket and Host.
4. Check browser Blob downloads, image previews, current file names, Range, CORS and exposed headers; inspect browser and edge cache retention separately.
5. Check special object keys, extensionless objects, query caching/forwarding and key rotation. Unexpected queries must not change a shared cached response.

Local vectors, mocks and browser form tests do not establish real Huawei CDN acceptance. Record origin, edge cache, CORS, expiration and rotation results separately without storing complete signed URLs or credentials.

See Huawei's [Method A authentication](https://support.huaweicloud.com/usermanual-cdn/cdn_01_0040.html) and [full domain configuration API](https://support.huaweicloud.com/api-cdn/UpdateDomainFullConfig.html).

## 3. Configure OBS CORS

Administrators can explicitly select **Configure CORS** for a new draft, edited draft, or saved policy and accept the existing remote-state confirmation. Configure a valid **Public site URL** first; the backend strictly derives origins without defaulting to `*`. Blank AK/SK fields in an edit reuse the saved credential while the action targets the current draft bucket. Ordinary saves and connection tests never configure CORS.

In addition to object permissions, this action needs only `obs:bucket:GetBucketCORS` and `obs:bucket:PutBucketCORS` on the target bucket. The action does not call the deletion API, but Huawei’s `PutBucketCORS` permission itself also permits deleting bucket CORS. It does not require public access, bucket policy or CDN administration. The action reads and merges existing rules, preserving IDs and order, and skips writes when current requests are already satisfied. An earlier matching rule hiding required headers such as ETag, unknown XML fields, or the merger's local safety bounds of 100 rules / 64 KiB blocks the write instead of clearing the configuration. Avoid simultaneous console changes: replacing a complete document is not an atomic cross-console merge.

Results show the bucket scope, normalized origins, change status, preserved rule count and available OBS request ID. Official mode uses the bucket's regional OBS endpoint; CNAME mode uses the endpoint directly bound to OBS, never a CDN acceleration domain. The independent CDN download domain is not used for management. See the [OBS CORS protocol evidence](https://drive.docs.astercosm.com/developer/design/huawei-obs-cors-contract/) for pinned official SDK sources, signing vectors and validation boundaries. Origin CORS changes do not configure CDN response headers, caches or authentication.

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
| Allowed origins | The actual AsterDrive page origin, such as `https://drive.example.com` |
| Allowed methods | `GET`, `HEAD`, `PUT` |
| Allowed headers | `Content-Type`, `Range` |
| Exposed headers | `ETag`, `Content-Length`, `Content-Range`, `Accept-Ranges`, `Content-Disposition` |
| Cache time | `600` |

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

If an existing generic `s3` policy already uses an official OBS endpoint and an explicit `s3_region`, the admin console can offer the `promote_from_s3` connector upgrade. It switches the connector and encrypted credentials in place without copying objects; the bucket, base path, region, and object namespace remain unchanged. When the source policy uses a bucket-prefixed OBS endpoint, promotion normalizes it to the corresponding regional root endpoint; an endpoint that is already regional remains unchanged. Generic S3 endpoints, `s3_region = auto`, and mismatched endpoints are not eligible for the upgrade recommendation.

Typical values:

| Field | Official endpoint example | Custom domain example |
| --- | --- | --- |
| Endpoint | `https://obs.cn-north-4.myhuaweicloud.com` | `https://files.example.com` |
| Bucket | `archive-bucket` | `archive-bucket` |
| OBS region | `cn-north-4` | May be empty |
| Use a custom domain (CNAME) | `false` | `true` |
| Base path | `prod/` | `prod/` |
| Access Key ID | Huawei Cloud AK | Huawei Cloud AK |
| Secret Access Key | Huawei Cloud SK | Huawei Cloud SK |

Signing is fixed to the native OBS protocol by the connector driver and is not an administrator-facing policy field. Do not put an OBS endpoint into a generic S3 policy or switch it to AWS SigV4.

## 5. Test the connection and configure a policy group

Before or after saving, run `Test Connection` and verify:

1. the AsterDrive server reaches the endpoint;
2. when `obs_use_cname=false`, the bucket, regional endpoint, and `obs_region` match;
3. the AK/SK can read, write, delete, and use multipart under the target prefix;
4. when `obs_use_cname=true`, the custom domain is bound to the target bucket; `obs_region` may be empty; and
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

- `false` uses `obs.<region>.myhuaweicloud.com` or an official regional suffix;
- `obs_region` matches the region in the hostname;
- the endpoint has no path prefix, query, fragment, username, or password; and
- `true` contains the bound hostname, not `bucket.obs.<region>...`.

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
