# Object Naming and OneDrive Direct Downloads

This document records the current object naming capability, the Microsoft Graph object layout used by OneDrive, and the filename rules for direct downloads. It is intended for backend, storage-driver, and test maintainers rather than deployment or end-user documentation.

## Design Goal

Microsoft Graph controls the response filename of a native content download URL. With the legacy flat object path `files/{upload_uuid}`, Graph can expose the UUID as the downloaded filename. OneDrive therefore keeps the original filename in the provider item name.

Object naming is a connector capability. It is not inferred from the upload transport. Relay upload, streaming-direct upload, chunk completion, and provider resumable upload describe different data paths; the policy connector descriptor owns the object naming decision.

## Connector Capability

The capability is defined in:

- `src/storage/connector_descriptor.rs`
- `src/storage/connectors/`

```rust
pub enum StorageConnectorObjectNamingMode {
    OpaqueUuid,
    OriginalFilename,
}
```

`StorageConnectorCapabilities.object_naming` is required and every built-in connector declares it explicitly:

| Connector | `object_naming` | Object layout |
| --- | --- | --- |
| OneDrive | `original_filename` | `files/{upload_uuid}/{filename}` |
| Local | `opaque_uuid` | `files/{upload_uuid}` |
| S3-compatible | `opaque_uuid` | `files/{upload_uuid}` |
| Azure Blob | `opaque_uuid` | `files/{upload_uuid}` |
| Tencent COS | `opaque_uuid` | `files/{upload_uuid}` |
| Remote | `opaque_uuid` | `files/{upload_uuid}` |
| SFTP | `opaque_uuid` | `files/{upload_uuid}` |

The OpenAPI wire values are stable:

```json
"opaque_uuid"
"original_filename"
```

Backend code resolves the capability with `resolve_policy_object_naming(policy)`. Production path generation must not infer naming from `DriverType::OneDrive`, `ProviderResumable`, or another upload-mode enum.

## Path Generation

The shared entry points are:

```text
src/services/workspace/storage/blob_upload.rs
  prepare_non_dedup_blob_upload()
  nondedup_storage_path_for_policy()
```

The flow is:

1. Resolve the connector upload transport for the policy.
2. Generate a new UUID for the upload.
3. Resolve `object_naming` from the connector descriptor.
4. Generate `files/{uuid}` for `opaque_uuid`.
5. Normalize and validate the filename, then generate `files/{uuid}/{filename}` for `original_filename`.

All of these entry points reuse the same path function:

- regular non-deduplicated upload
- provider direct resumable initialization
- streaming-direct upload
- chunk completion
- empty-file upload
- WebDAV upload
- temporary pre-upload objects

### Same-Name Files

The filename is not the uniqueness key. Each upload gets an exclusive UUID parent directory:

```text
files/550e8400-e29b-41d4-a716-446655440000/same-name.mp4
files/6ba7b810-9dad-11d1-80b4-00c04fd430c8/same-name.mp4
```

The OneDrive driver creates the UUID namespace with conflict failure semantics. Reusing an existing UUID namespace returns a precondition error instead of overwriting an older object.

## OneDrive Namespace Lifecycle

The named-object layout is:

```text
files/
└── {upload_uuid}/
    └── {filename}
```

Before writing a named object, the driver:

1. Creates or verifies the shared `files` folder.
2. Creates the exclusive UUID folder for the upload.
3. Verifies that both objects are folders.
4. Starts a small upload or a Graph upload session.

The UUID folder is cleaned up when any of these operations fail:

- Graph upload-session creation
- large streaming upload
- small content upload

Deleting a named object deletes its `files/{upload_uuid}` parent directory. Legacy flat objects are deleted by their original path. NotFound deletion remains idempotent.

Provider direct resumable initialization owns two provider-side resources:

```text
abort the Graph upload session
delete files/{upload_uuid}
```

Upload-id collision, session encryption failure, database persistence failure, and an empty upload URL all enter this cleanup flow. Abort and delete are attempted independently, and both failure reasons are retained when both operations fail.

## Direct Download Rules

`PresignedDownloadOptions.download_name` carries the current logical filename. The download service and file resource handle pass `file.name` to the presigned driver.

OneDrive policies also expose `provider_download_filename_mode`:

| Mode | Default | Behavior |
| --- | --- | --- |
| `provider_native` | Yes | Prefer the filename stored in OneDrive and keep Graph direct downloads available |
| `strict_current` | No | Require the provider filename to match AsterDrive's current filename; use relay streaming when it does not |

In `provider_native` mode, the OneDrive driver does not compare the current filename with the provider name. Whenever Graph returns a valid HTTP(S) URL, the object is downloaded directly; a renamed file may still use the older OneDrive filename, and a legacy `files/{uuid}` object may expose the UUID filename.

In `strict_current` mode, the OneDrive driver returns a Graph direct-download URL only when:

1. The storage path matches `files/{uuid}/{filename}`.
2. The provider item filename matches `download_name`, or the caller did not provide a name.
3. Graph returns a valid HTTP(S) download URL.

The driver returns `None`, and AsterDrive uses relay streaming, for:

- legacy `files/{uuid}` objects in strict mode, because the path cannot prove the provider filename
- files renamed in AsterDrive in strict mode
- shared blobs whose logical filename differs from the provider item name in strict mode
- drivers without a presigned capability
- conditional requests or inline sandbox cases that require same-origin handling

In strict mode, Graph's temporary URL may expose the old filename because Graph owns its response headers, so AsterDrive uses relay streaming. Filename policy is explicit; `provider_native` mode does not silently switch delivery modes because of a rename.

## Legacy Compatibility

Existing OneDrive objects may still use:

```text
files/{upload_uuid}
```

The path parser classifies this as the legacy layout. Read, metadata, stream, range, and delete operations remain supported. `provider_native` mode can still request a Graph direct download, while `strict_current` mode uses relay streaming because the path cannot prove the provider filename.

New code must not reinterpret a legacy path as `files/{uuid}/{filename}` or bulk-rewrite historical blob paths based only on the provider type. Historical migration requires a separate storage migration task with an explicit database/object consistency plan.

## Test Acceptance Matrix

When changing naming or OneDrive direct-download behavior, run at least:

```bash
cargo test --lib storage::connectors::tests
cargo test --lib services::workspace::storage
cargo test --lib storage::drivers::onedrive
cargo test --lib services::files::upload
cargo test --test test_upload
cargo test --test webdav
cargo test --features openapi --test generate_openapi
```

Required behavior coverage:

- Every built-in connector explicitly declares an object naming mode.
- `opaque_uuid` and `original_filename` produce different layouts.
- Same-name files are isolated by different UUID parents.
- Unicode filenames are normalized before entering the object path.
- Empty names, separators, backslashes, parent traversal, and invalid UUIDs are rejected.
- Named OneDrive uploads create the shared folder and exclusive UUID folder.
- An existing shared folder is verified and reused.
- A UUID namespace collision does not overwrite the existing folder.
- Session creation, small upload, and large upload failures clean up the UUID folder.
- Legacy flat paths remain readable. They keep Graph direct download in
  `provider_native` mode and use relay streaming in `strict_current` mode.
- Matching named objects return a Graph direct-download URL.
- Renamed files remain on Graph direct download in `provider_native` mode.
- Renamed or mismatched logical names use relay streaming in `strict_current` mode.
- All four abort/delete cleanup combinations retain the relevant result.
- OpenAPI and generated frontend types contain `object_naming`.

## Requirements for a New Connector

A new storage connector must:

1. Declare `object_naming` in its descriptor.
2. Document its object layout and same-name isolation strategy.
3. Reuse `nondedup_storage_path_for_policy()` from every upload entry point.
4. Apply `PresignedDownloadOptions.download_name` when provider URLs fix the response filename.
5. Add legacy-path, invalid-input, failure-cleanup, and direct-download fallback tests.
6. Regenerate OpenAPI and the frontend SDK.
