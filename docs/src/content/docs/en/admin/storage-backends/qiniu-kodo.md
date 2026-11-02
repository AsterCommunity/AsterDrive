---
title: "Qiniu Kodo"
description: "Configure Qiniu Cloud Kodo through its S3-compatible API."
---

The Qiniu Kodo connector uses the **Kodo S3-compatible API** and AWS SigV4. AsterDrive continues to own files, versions, quotas, trash, and object cleanup; Kodo stores object content only.

It does not use QBox, UpToken, native form uploads, or Qiniu-native multipart REST. Do not enter a native upload domain or token in this form.

## Before you start

Create AccessKey / SecretKey credentials for a dedicated Kodo space and grant only `s3:GetObject` (which also covers `GetObject` and `HeadObject`), `s3:PutObject`, `s3:DeleteObject`, `s3:ListBucket`, `s3:AbortMultipartUpload`, and `s3:ListMultipartUploadParts`. Scope permissions to the target space and allowed `base_path`; runtime requests do not require `s3:GetBucketLocation`. Record the space's **Qiniu S3 space name**, official S3 endpoint, and Region ID. If the ordinary Kodo space name is globally unique, it is also the S3 space name. Otherwise, Qiniu generates a separate globally unique S3 space name. Obtain this value from the Kodo console's space overview or [`Get Service`](https://developer.qiniu.com/kodo/manual/4087/compatible-s3-api#service-operation).

`s3:ListBucket` is a bucket-level operation: bind it to the bucket ARN and constrain visible keys with `s3:prefix`. Bind object and multipart operations to the object ARN. This [`Bucket Policy`](https://developer.qiniu.com/kodo/6317/BucketPolicy) example uses the S3 space name `example-space` and `tenant-a` as `base_path`:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": ["s3:ListBucket"],
      "Resource": ["arn:aws:s3:::example-space"],
      "Condition": {
        "StringLike": {
          "s3:prefix": ["tenant-a/*"]
        }
      }
    },
    {
      "Effect": "Allow",
      "Action": [
        "s3:GetObject",
        "s3:PutObject",
        "s3:DeleteObject",
        "s3:AbortMultipartUpload",
        "s3:ListMultipartUploadParts"
      ],
      "Resource": ["arn:aws:s3:::example-space/tenant-a/*"]
    }
  ]
}
```

Replace the example space name and prefix with the policy's actual values. For an empty `base_path`, use `*` in the list condition and `arn:aws:s3:::example-space/*` for object resources. Do not attach `s3:ListBucket` to an object ARN or object actions to the bucket ARN; Kodo rejects policies whose Action and Resource levels do not match.

The backend S3 endpoint accepts HTTPS only, in either official service-level form (`https://s3.<region>.qiniucs.com`) or S3-space-level form (`https://<S3-space-name>.s3.<region>.qiniucs.com`). Make `<region>` match the SigV4 region field and the space name in a space-level host match the Qiniu S3 space-name field. Both forms are normalized to the service endpoint used by the AWS SDK, while the connector selects addressing automatically and prevents the space name from being added twice. Use the generic S3 connector for other S3-compatible services.

The delivery domain is separate from the backend S3 endpoint. Optionally set `download_base_url` to an HTTPS source or CDN acceleration domain already bound to this Kodo space, with an optional path prefix. It is used only for `presigned` downloads; uploads, listing, deletion, multipart operations, cleanup, and connection tests continue to use the S3 endpoint. Qiniu source domains and CDN acceleration domains are distinct bindings in the Qiniu console. Configure DNS/CNAME and an HTTPS certificate first, verify that the domain is bound to this space, and ensure CDN caching cannot bypass private-download authentication. Anyone holding an issued URL can use it until it expires; changing AsterDrive permissions does not revoke issued URLs or cached content.

Start with `relay_stream` for both upload and download. Enable `presigned` only after server-side reads and writes are proven. Browser-direct use requires a Kodo endpoint reachable by users and CORS allowing the AsterDrive site origin, `GET`, `HEAD`, `PUT`, and the headers needed for Range requests.

## Create a policy

Open `Admin -> Storage Policies -> New Policy`, choose **Qiniu Kodo**, then provide:

| Field | Meaning |
| --- | --- |
| Kodo S3 endpoint | Official service-level endpoint such as `https://s3.cn-east-1.qiniucs.com`, or S3-space-level endpoint such as `https://example-space.s3.cn-east-1.qiniucs.com`; both forms are normalized automatically. |
| Qiniu S3 space name | Globally unique name returned by the console or `Get Service`; it may differ from the ordinary Kodo space name. |
| Base path | Optional object prefix; empty uses the S3 space root. |
| Kodo SigV4 signing region | Region ID embedded in the endpoint host, such as `cn-east-1`; the two values must match. |
| AccessKey / SecretKey | Static credentials dedicated to this policy; SecretKey is never returned with the policy. |
| Download delivery URL | Optional HTTPS source or CDN domain, with an optional path prefix; used only for `presigned` downloads. The S3 space name is not added to the object URL. |
| Download authentication | `private` uses Qiniu Kodo's native `e` / `token` private-download credential; `timestamp` uses Qiniu CDN `sign` / `t` timestamp anti-leech; `public` issues no credential and is only for explicitly public objects. Defaults to `private`. |
| CDN timestamp key | Required only for `timestamp`; must match the Qiniu CDN anti-leech key and is stored as an encrypted static credential. |

`private` signs the complete download URL and places `token` last. `timestamp` is for a Qiniu CDN acceleration domain and is distinct from the Kodo private-download token. `public` adds neither credentials nor response overrides. Qiniu limits response-header overrides on anonymous downloads, so cache control, content type, and download filename overrides are attached only in `private` mode. Configure the needed headers through source/CDN settings or object metadata for `timestamp` and `public`.

Run the draft connection test before saving and the saved-policy test afterwards. Tests write and delete a temporary object, so credentials need write and delete permissions for the target prefix.

## Acceptance and troubleshooting

Bind a test user or team through a policy group and verify small upload, multipart upload, download, Range preview, deletion, and object cleanup. After enabling presigned mode, verify browser PUT, GET/HEAD, Range, and CORS behavior from a real browser.

For failures, check endpoint reachability, the S3 space name, endpoint/region agreement, credentials, permissions, and server time in that order. The connector handles addressing automatically. Never paste SecretKey values or complete signed URLs into logs, error reports, or tickets.

Local tests validate URL construction and signing formats. They do not validate a real Qiniu space, domain binding, HTTPS certificate, CDN cache hit, or private-origin behavior. Complete those checks with a dedicated test space before production use.

Create a target policy and migrate existing blobs only when the S3 space name, base path, or actual storage target changes. For the same space and base path, correcting the matching endpoint or region does not change object keys and needs no migration; fix mismatched configuration in place.
