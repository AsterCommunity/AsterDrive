# Object Storage Custom Authentication and AWS SDK Reuse Boundaries

This document records the boundaries AsterDrive must preserve when reusing the
`aws-sdk-s3` operation/runtime for object storage providers such as Tencent
Cloud COS, Huawei Cloud OBS, and Alibaba Cloud OSS while replacing the SDK's authentication with
the provider-native signing protocol.

This is a driver implementation contract. It does not change the product
contracts exposed by `StorageDriver`, upload strategies, or connector
descriptors.

## Background

`S3CompatibleDriver` currently delegates object reads and writes, streaming
uploads, listing, multipart operations, and presigned operations through
`S3Driver` to `aws-sdk-s3`. Replacing only the endpoint does not replace the
signing protocol: the default client still emits AWS SigV4 headers and
`X-Amz-*` presigned query parameters.

Some providers expose HTTP operations and XML responses that resemble S3, but
use different authentication protocols:

- Tencent Cloud COS uses COS Q-Sign.
- Huawei Cloud OBS uses `SignatureObs`, with `Authorization: OBS AccessKeyID:Signature` and `Base64(HMAC-SHA1(SK, UTF-8(StringToSign)))`.
- Alibaba Cloud OSS V4 uses `OSS4-HMAC-SHA256`, `x-oss-*` fields, the
  `date/region/oss/aliyun_v4_request` scope, and the `aliyun_v4` signing-key
  derivation prefix.

The valid reuse boundary is therefore "reuse the operation/runtime and replace
the auth scheme." Support for a custom endpoint must not be presented as
native protocol compatibility.

## Verified AWS SDK Extension Points

The current repository locks `aws-sdk-s3 1.140.0`, `aws-runtime 1.9.1`, and
`aws-smithy-runtime-api 1.14.0`. An isolated mock spike verified the following
behavior for these versions:

1. `Config::builder().interceptor(...)` can register a
   `modify_before_signing` hook.
2. Per-operation `.customize().mutate_request(...)` also runs during the
   `modify_before_signing` stage.
3. The Smithy orchestrator runs endpoint resolution, `modify_before_signing`,
   `Sign::sign_http_request`, and request transmission in that order.
4. `push_auth_scheme(...)` can replace the identity resolver and signer for an
   already registered scheme.
5. Generated `.presigned()` operations still invoke the replacement signer.

This allows a provider signer to inspect or modify the method, URI, query, and
headers after the final endpoint has been resolved, then generate either a
header signature or query signature. The AWS SDK continues to own input
serialization, HTTP bodies, timeouts, transport, retry orchestration, and
response parsing.

## Auth Scheme ID Constraint

The S3 endpoint resolver only declares auth schemes that it recognizes.
Registering a signer under an arbitrary new ID such as `oss4` triggers
`MissingEndpointConfig` before signing begins.

A provider-specific client must register its `AuthScheme` under the existing
SigV4 scheme ID:

```rust
const SCHEME_ID: AuthSchemeId = aws_runtime::auth::sigv4::SCHEME_ID;
```

The public SDK contract guarantees that `push_auth_scheme` replaces a scheme
with the same ID. This replacement must be scoped to a COS- or OSS-specific
client. Ordinary S3 clients must retain the AWS signer and must not share a
client whose authentication components have been replaced.

## Presigned Operations

Generated S3 `.presigned()` operations install
`SigV4PresigningRuntimePlugin`. The plugin:

- sets `SigV4OperationSigningConfig.signing_options.signature_type` to query;
- records `expires_in`;
- configures an unsigned payload for the operation;
- stops the orchestrator before transmission and returns the signed request.

It does not select a different `AuthScheme` or replace the active one. A
provider signer can read `SigV4OperationSigningConfig`, use `signature_type` to
distinguish ordinary header signing from presigning, and read the effective
expiration. When these types are used in production code, the currently
transitive `aws-runtime`, `aws-smithy-runtime-api`, and `aws-smithy-types`
dependencies must be declared directly instead of relying on Cargo's
transitive dependency implementation details.

## COS Migration Boundary

The basic COS object operations continue to reuse this chain:

```text
TencentCosDriver -> S3CompatibleDriver -> S3Driver -> aws_sdk_s3::Client
```

The difference is that the `aws_sdk_s3::Client` registers the COS
`AuthScheme` during construction. Ordinary object requests, multipart
operations, and presigned URLs are authenticated by the COS Q-Sign signer and
no longer contain AWS SigV4 signatures.

COS CI image processing, media metadata, and bucket CORS currently use
`reqwest` with native COS signing. Those capabilities do not block migration
of the underlying client. Keep their existing request paths during the
migration so provider-native APIs and basic object APIs are not changed at the
same time.

The COS signer must also handle the narrow differences between the AWS
operation serializer and native COS fields, including copy-source headers,
SDK-default checksum headers, and query parameters added by the S3 endpoint.
These transformations belong in the COS driver/signing module. They do not
belong in services, connector common code, or the shared
`aster_drive_storage` trait.

## Huawei OBS Reuse Boundary

Huawei OBS reuses the AWS SDK operation/runtime chain with an independent OBS signer:

```text
HuaweiObsDriver -> S3CompatibleDriver -> S3Driver -> aws_sdk_s3::Client
```

The driver registers a `SignatureObs` hook under the existing SigV4 scheme ID. The AWS SDK continues to own request serialization, bodies, timeouts, retries, and XML response parsing; the signing hook converts AWS header/query residue to OBS fields and generates either `Authorization: OBS ...` or OBS presigned query parameters.

OBS addressing and listing must not inherit generic S3 blindly:

- virtual-hosted mode requires an official regional OBS endpoint and matching region;
- custom-domain mode removes the bucket host prefix added by the AWS SDK and uses the official OBS SDK CNAME canonical resource;
- the official OBS SDK and API use marker-based `ListObjects`, so the driver does not send S3 `list-type=2` or continuation tokens;
- `x-amz-meta-*`, copy-source, storage-class, ACL, grant, and security-token fields are converted to their `x-obs-*` equivalents; and
- ordinary S3, COS, and OBS clients keep separate signer configuration.

The implementation is pinned against Huawei's official Go OBS SDK `v3.26.6`, commit `fd2b44881f0cd9bd41ffff2fabeb94c783ccc321`, especially `obs/auth.go`, `obs/authV2.go`, `obs/conf.go`, `obs/trait_object.go`, `obs/trait_part.go`, `obs/convert.go`, and `obs/client_object.go`.

## OSS Implementation Boundary

OSS follows the auth-scheme structure verified for COS, but its signer,
endpoint/addressing behavior, and field transformations must remain
independent:

- Backend I/O uses the server-side endpoint, falling back to the public
  endpoint when no server-side endpoint is configured.
- Browser presigned URLs use the public endpoint.
- Region is a required OSS V4 signing input.
- CNAME mode independently determines the Host and bucket addressing behavior;
  it must not introduce provider-specific branches in the frontend or service
  layer.
- Header signing and query presigning both use OSS V4 rather than AWS SigV4.

`AlibabaOssDriver` keeps a public client for browser presigning and constructs
a separate backend client when a server-side endpoint is configured. Without
a server-side endpoint, both paths share one client. This avoids changing an
endpoint on a single request and breaking consistency between the Host and
canonical URI.

The OSS signer lives in `src/storage/drivers/alibaba_oss/signing.rs`. Current
coverage includes an official Go SDK vector, captured normal requests,
generated presigning, the CNAME wire path, CopyObject header conversion, and
public/server endpoint separation. Real-provider integration remains an
external release-validation boundary.

## Boundary with Provider Option Plugins

Issue #458 is now the storage refactor contract, not a future compatibility
layer. Built-in connectors and dynamically loaded plugins use the same
namespaced `ConnectorConfigEnvelope`:

- `connector_id`, format version, schema version, and connector-owned
  values are persisted as one envelope.
- Descriptor fields declare defaults, scalar validation, secret handling, and
  UI metadata; the connector owns normalization and runtime decoding.
- Core services do not match provider field names or maintain a
  `DriverType`-to-options matrix.
- `StoragePolicyOptions` and its provider-specific enums are
  transitional legacy code and must be deleted after all built-in connectors
  are migrated.
- Cross-connector product behavior, such as media processing limits, belongs
  to a separate core policy behavior contract rather than a connector
  namespace.
- Unknown connector envelopes remain inspectable as unavailable policy data;
  they are not silently converted to another connector or discarded.

### Legacy option ownership map

The following legacy fields are the deletion checklist for
StoragePolicyOptions:

| Legacy fields | New owner |
| --- | --- |
| object_storage_upload_strategy, object_storage_download_strategy, s3_path_style, s3_region, s3_*_timeout_secs | S3 connector config |
| object_storage_upload_strategy, object_storage_download_strategy, storage_native_processing_enabled, storage_native_media_metadata_enabled | Object-storage connector config declared by each descriptor |
| remote_download_strategy, remote_upload_strategy | Remote connector config |
| provider_resumable_upload_strategy, provider_download_strategy, provider_download_filename_mode, onedrive_* | OneDrive connector config |
| sftp_host_key_fingerprint | SFTP connector config |
| content_dedup | Local connector config |
| thumbnail_processor, thumbnail_extensions, media_metadata_extensions | Core storage policy behavior |

The provider enum types move with their connector or become private parser
types. They must not remain in the shared model facade after the migration.

## Verification Requirements

Each provider signer must cover at least:

- official fixed-time signing vectors;
- captured ordinary GET, HEAD, PUT, DELETE, and COPY requests;
- canonicalization of Range, Content-Type, and provider headers;
- presigned GET, PUT, and UploadPart requests;
- multipart initiate, upload, list, complete, and abort operations;
- removal or transformation of headers and query parameters added by the SDK;
- endpoints, virtual-hosted addressing, CNAME, and non-default ports;
- parsing of provider XML success and error responses;
- optional integration tests against the real provider.

Mock tests establish the request contract and SDK orchestration only. Complete
compatibility must not be claimed without real-provider testing.
