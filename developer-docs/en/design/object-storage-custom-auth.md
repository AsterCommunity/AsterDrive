# Object Storage Custom Authentication and AWS SDK Reuse Boundaries

This document records the boundaries AsterDrive must preserve when reusing the
`aws-sdk-s3` operation/runtime for object storage providers such as Tencent
Cloud COS and Alibaba Cloud OSS while replacing the SDK's authentication with
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

## Boundary for a Future OSS Implementation

OSS can follow the structure verified for COS, but its signer,
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

Prefer two clients that share a signer, one for backend operations and one for
browser presigning. Temporarily changing the endpoint for a single request can
break consistency between the Host and canonical URI.

## Boundary with Provider Option Plugins

Issue #458 tracks the future plugin-safe provider option contract. The built-in
OSS connector may temporarily use typed `oss_*` options, subject to these
rules:

- Each connector owns normalization and validation of its provider options.
- Do not continue expanding the provider-key matrix in
  `src/storage/connectors/common.rs`.
- Services and upload flows only consume stable common options, descriptors,
  and capabilities.
- Do not combine the built-in OSS implementation with a refactor of namespaced
  plugin option persistence.

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
