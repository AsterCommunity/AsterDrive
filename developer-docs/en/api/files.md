# Files

The following paths are relative to `/api/v1` and require authentication.

## Endpoints

| Method | Path | Description |
| --- | --- | --- |
| `POST` | `/files/upload` | Ordinary multipart direct upload |
| `POST` | `/files/new` | Create an empty file |
| `POST` | `/files/upload/init` | Negotiate upload mode |
| `GET` | `/files/upload/sessions` | List recoverable upload sessions |
| `PUT` | `/files/upload/{upload_id}/{chunk_number}` | Upload one chunk |
| `POST` | `/files/upload/{upload_id}/presign-parts` | Request object-storage / remote multipart part URLs |
| `POST` | `/files/upload/{upload_id}/complete` | Assemble chunks or confirm presigned upload |
| `GET` | `/files/upload/{upload_id}` | Read upload progress |
| `DELETE` | `/files/upload/{upload_id}` | Cancel upload |
| `GET` | `/files/{id}` | Read file metadata |
| `GET` | `/files/{id}/archive-preview` | Read read-only archive preview manifest |
| `GET` | `/files/{id}/direct-link` | Create direct-download token |
| `POST` | `/files/{id}/preview-link` | Create short-lived preview link |
| `POST` | `/files/{id}/wopi/open` | Create WOPI launch session |
| `GET` | `/files/{id}/download` | Download file content |
| `GET` | `/files/{id}/thumbnail` | Get thumbnail |
| `GET` | `/files/{id}/image-preview` | Get WebP image preview |
| `GET` | `/files/{id}/media-metadata` | Get image / audio / video metadata |
| `PUT` | `/files/{id}/content` | Overwrite content and write version history |
| `POST` | `/files/{id}/extract` | Create archive extraction task |
| `PATCH` | `/files/{id}` | Rename or move file |
| `DELETE` | `/files/{id}` | Soft-delete to trash |
| `POST` | `/files/{id}/lock` | Lock / unlock file |
| `POST` | `/files/{id}/copy` | Copy file |
| `GET` | `/files/{id}/versions` | List versions |
| `POST` | `/files/{id}/versions/{version_id}/restore` | Restore a version |
| `DELETE` | `/files/{id}/versions/{version_id}` | Delete a version |

## Uploads

Primary upload entries:

- `POST /files/upload/init`: negotiate mode first
- `POST /files/upload`: ordinary multipart upload
- `GET /files/upload/sessions`: recover unfinished sessions after refresh

Directory-upload semantics are supported through:

- `folder_id`
- `relative_path`
- `declared_size`
- `frontend_client_id`

`folder_id = null` means root. Missing directories in `relative_path` are created automatically. Empty path segments such as `docs//bad.txt` are rejected.

Negotiation returns one of four modes:

- `direct`: small-file direct upload
- `chunked`: resumable chunked upload
- `presigned`: single object-storage or remote presigned `PUT`
- `presigned_multipart`: object-storage or remote multipart direct upload; the client must request part URLs separately

The frontend never sees an additional `relay_stream` mode. Actual transfer strategy is decided by storage connectors and policy options:

- `options.object_storage_upload_strategy`: transfer strategy for S3-compatible, Azure Blob, and Tencent COS object-storage connectors
- `options.remote_upload_strategy`
- OneDrive uses Microsoft Graph native upload capabilities and follows the upload workflow exposed by the connector
- `relay_stream`: `init` still returns `direct` / `chunked`, but the server relays bytes straight to object storage / follower instead of writing a local temp file
- `presigned`: `init` returns `presigned` / `presigned_multipart`

Object-storage and remote uploads fall back to `relay_stream` by default. Legacy `{"presigned_upload":true}` and `{"s3_upload_strategy":"presigned"}` are accepted as compatibility inputs for object-storage presigned upload; new clients should send `{"object_storage_upload_strategy":"presigned"}`.

Presigned browser uploads require usable CORS on the object storage or follower internal storage endpoint. Azure Blob presigned upload uses SAS URLs and requires `x-ms-blob-type: BlockBlob`; S3-compatible, Tencent COS, and Remote multipart parts usually require returned ETags. Remote presigned upload only works for directly reachable remote nodes; reverse-tunnel remote nodes reject `remote_upload_strategy = "presigned"`.

## Direct, chunked, and completion stages

- `POST /files/upload`: ordinary multipart upload; empty files are rejected, and same-folder same-name files are not overwritten. With object-storage / Remote `relay_stream`, the body is relayed directly to the target driver.
- `POST /files/new`: creates a 0-byte file for “new text file” style actions
- `GET /files/upload/sessions`: lists unexpired, recoverable sessions in `uploading` / `assembling` / `presigned` status; `frontend_client_id` can filter sessions created by the same frontend instance
- `PUT /files/upload/{upload_id}/{chunk_number}`: uploads one chunk, with `chunk_number` starting at `0`
- `POST /files/upload/{upload_id}/presign-parts`: used only for `presigned_multipart`
- `GET /files/upload/{upload_id}`: returns upload progress used by resumable upload
- `POST /files/upload/{upload_id}/complete`: completes `chunked`, `presigned`, or `presigned_multipart`

Recoverable session fields include:

- `upload_id`
- `mode`
- `status`
- `filename`
- `total_size`
- `chunk_size`
- `total_chunks`
- `received_count`
- `folder_id`
- `chunks_on_disk`
- `completed_parts`
- `expires_at`
- `updated_at`

Completion behavior:

- local path: validates size and quota; if local `content_dedup` is enabled, computes SHA-256 and deduplicates blobs
- object-storage / OneDrive / Remote paths: validate size and quota but do not deduplicate; each upload creates an independent blob using an upload-session-derived opaque hash and `files/{upload_id}`-style object path

`POST /files/new` follows the same rule: local content dedup can reuse the 0-byte blob, while non-local connectors always create an independent blob.

`presigned_multipart` completion must include object-storage returned `parts`; other modes may omit the body.

## File operations

- `GET /files/{id}`: read metadata; trashed files behave as not found
- `GET /files/{id}/archive-preview`: read archive manifest; returns `202` and queues `archive_preview_generate` if not ready
- `GET /files/{id}/direct-link`: returns a short token; real download is `/d/{token}/{filename}`
- `POST /files/{id}/preview-link`: returns a short preview link; real content is `/pv/{token}/{filename}`
- `POST /files/{id}/wopi/open`: creates a WOPI launch session for a configured WOPI previewer
- `GET /files/{id}/download`: streams file content or redirects to a presigned GET URL when policy says so; supports `If-None-Match`
- `GET /files/{id}/thumbnail`: returns thumbnail, or `202` with `Retry-After` while generating
- `GET /files/{id}/image-preview`: returns raw WebP with `ETag`, or `202` with `Retry-After` while generating
- `GET /files/{id}/media-metadata`: returns blob-cached metadata, or `202` while queued
- `PUT /files/{id}/content`: overwrite existing content, check locks, create version history, and return a new `ETag`
- `POST /files/{id}/extract`: creates an archive extraction task
- `PATCH /files/{id}`: rename or move
- `DELETE /files/{id}`: soft-delete to trash

File info and list items include persisted classification fields:

- `extension`: lowercase final extension without dot
- `compound_extension`: lowercase compound extension such as `tar.gz`
- `file_category`: `image`, `video`, `audio`, `document`, `spreadsheet`, `presentation`, `archive`, `code`, or `other`

These fields are recalculated on create, upload, overwrite, and rename.

Detail responses from `GET /files/{id}` and `GET /teams/{team_id}/files/{id}` also include `storage_used`. This is the quota-accounting size for the file detail view: current `size` plus all historical version sizes. Directory list items omit this field.

## `PATCH /files/{id}`

Request:

```json
{
  "name": "renamed.pdf",
  "folder_id": 5
}
```

Supports rename, move, and `folder_id = null` to move to root. Name conflicts at the destination are rejected, and locked files cannot be modified.

## Thumbnails

Thumbnail support comes from the media processing registry and is exposed anonymously through `/public/thumbnail-support`. The built-in `images` processor covers common image formats. The built-in `lofty` processor can expose audio suffixes for embedded cover thumbnails. Optional `vips_cli` / `ffmpeg_cli` processors contribute additional extensions only when enabled and available.

Storage policies can also contribute storage-native thumbnail and image-preview support with `storage_native_processing_enabled = true`, `thumbnail_processor = "storage_native"`, and `thumbnail_extensions`. Built-in `tencent_cos` policies can expose this through COS CI; built-in Local, S3-compatible, Azure Blob, OneDrive, and Remote policies do not expose native thumbnail or image-preview capabilities.

Thumbnails return WebP and reuse cache by blob, processor, processor version, and effective max dimension. The max source byte limit is controlled by `thumbnail_max_source_bytes`; the rendered longest edge is controlled by runtime config `thumbnail_max_dimension`.

The default thumbnail dimension keeps the legacy cache version namespace. Non-default dimensions add a `-d{dimension}` suffix to the derivative version, so changing the configured size does not overwrite or accidentally reuse a different-size cache entry.

## Image previews

Image preview endpoints return larger WebP images for preview panels and are separate from thumbnails:

- thumbnails are list/card-oriented and may return `202`
- image previews are previewer-oriented; cache hits return raw WebP, while cache misses enqueue `image_preview_generate` and return `202` with `Retry-After`
- image previews use runtime config `image_preview_max_dimension` for the rendered longest edge
- unsupported types return file/thumbnail-domain errors instead of falling back to original bytes
- the frontend can choose its default strategy from `/public/frontend-config` field `media.image_preview_preference`

The supported image-preview extensions are the same public capability union advertised by `/public/thumbnail-support` under image thumbnail/preview support. They can come from backend media processors or a storage-native provider such as Tencent COS when the policy opts in. Image-preview caches follow the same dimension-aware derivative-version rule as thumbnails.

## Media metadata

Media metadata is cached by blob. Image metadata is read by the built-in `images` processor, audio by `lofty`, and video by `ffprobe_cli`. `media_metadata_enabled` is the master switch, while per-kind settings live in `media_processing_registry_json`.

Storage-native media metadata can be enabled per policy with `storage_native_processing_enabled = true`, `storage_native_media_metadata_enabled = true`, and `media_metadata_extensions`. Built-in `tencent_cos` policies can expose native audio/video metadata through COS CI; built-in Local, S3-compatible, Azure Blob, OneDrive, and Remote policies do not expose native media metadata.

Audio embedded cover art is exposed through the existing thumbnail path when the `lofty` processor has `thumbnail:audio`.

## Archive preview

`GET /files/{id}/archive-preview` returns a read-only manifest for supported archive files without extracting them into the workspace.

Optional `filename_encoding` controls ZIP entry-name decoding:

- `auto`
- `utf8`
- `gb18030`
- `cp437`
- `cp850`
- `shift_jis`
- `big5`
- `euc_kr`
- `windows_1252`

Explicit values override auto detection.

Response shape:

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "schema_version": 2,
    "format": "zip",
    "source_blob_id": 42,
    "source_hash": "abc...",
    "generated_at": "2026-05-18T12:00:00Z",
    "entry_count": 2,
    "file_count": 1,
    "directory_count": 1,
    "total_uncompressed_size": 128,
    "truncated": false,
    "entries": [
      {
        "path": "docs/readme.txt",
        "name": "readme.txt",
        "parent": "docs",
        "kind": "file",
        "size": 128,
        "compressed_size": 64,
        "modified_at": "2026-05-18T12:00:00Z"
      }
    ]
  }
}
```

Current implementation:

- supports `.zip` and corresponding MIME types
- disabled by default; requires both `archive_preview_enabled` and `archive_preview_user_enabled`
- first uncached request queues or reuses `archive_preview_generate` and returns `202`
- raw manifest is cached under `entity_properties` as `system.archive_preview / zip_raw_manifest.v2`
- success responses include `ETag` and support `If-None-Match`
- limits are controlled by archive-preview and archive-extraction runtime settings
- range-capable storage drivers are used for metadata scanning when possible

## Direct and preview links

`GET /files/{id}/direct-link` returns only a token. The actual URL is:

```text
/d/{token}/{filename}
```

`POST /files/{id}/preview-link` returns `PreviewLinkInfo`; actual content is served from:

```text
/pv/{token}/{filename}
```

These root-level endpoints return raw file data or redirects instead of wrapped JSON.
