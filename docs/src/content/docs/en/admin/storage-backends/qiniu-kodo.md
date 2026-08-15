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

Only HTTPS is accepted. Use the service-level `https://s3.<region>.qiniucs.com` endpoint, such as `https://s3.cn-east-1.qiniucs.com`, and make `<region>` match the SigV4 region field. This connector rejects plaintext HTTP, bucket-prefixed endpoints, custom CNAMEs, non-standard ports, and URLs containing a path, query, or fragment. Use the generic S3 connector for other S3-compatible services.

Start with `relay_stream` for both upload and download. Enable `presigned` only after server-side reads and writes are proven. Browser-direct use requires a Kodo endpoint reachable by users and CORS allowing the AsterDrive site origin, `GET`, `HEAD`, `PUT`, and the headers needed for Range requests.

## Create a policy

Open `Admin -> Storage Policies -> New Policy`, choose **Qiniu Kodo**, then provide:

| Field | Meaning |
| --- | --- |
| Kodo S3 endpoint | Official service-level endpoint, such as `https://s3.cn-east-1.qiniucs.com`, without the S3 space name. |
| Qiniu S3 space name | Globally unique name returned by the console or `Get Service`; it may differ from the ordinary Kodo space name. |
| Base path | Optional object prefix; empty uses the S3 space root. |
| Kodo SigV4 signing region | Region ID embedded in the endpoint host, such as `cn-east-1`; the two values must match. |
| Path-style addressing | Enabled by default for `/S3-space-name/key`; disabling it uses `S3-space-name.s3.<region>.qiniucs.com/key`. Qiniu documents support for both styles. |
| AccessKey / SecretKey | Static credentials dedicated to this policy; SecretKey is never returned with the policy. |

Run the draft connection test before saving and the saved-policy test afterwards. Tests write and delete a temporary object, so credentials need write and delete permissions for the target prefix.

## Acceptance and troubleshooting

Bind a test user or team through a policy group and verify small upload, multipart upload, download, Range preview, deletion, and object cleanup. After enabling presigned mode, verify browser PUT, GET/HEAD, Range, and CORS behavior from a real browser.

For failures, check endpoint reachability, the S3 space name, endpoint/region agreement, path-style, credentials, permissions, and server time in that order. Never paste SecretKey values or complete signed URLs into logs, error reports, or tickets.

Create a target policy and migrate existing blobs only when the S3 space name, base path, or actual storage target changes. For the same space and base path, switching path-style addressing or correcting the matching endpoint or region does not change object keys and needs no migration; fix mismatched configuration in place.
