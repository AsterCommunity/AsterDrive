# Sharing

Sharing has two surfaces: managing your own shares, and public access to shared content.

All paths below are relative to `/api/v1`.

## Owned shares

| Method | Path | Description |
| --- | --- | --- |
| `POST` | `/shares` | Create a share |
| `GET` | `/shares` | List shares created by the current user |
| `PATCH` | `/shares/{id}` | Edit an existing share |
| `DELETE` | `/shares/{id}` | Delete a share |
| `POST` | `/shares/batch-delete` | Delete shares in batch |

Create request:

```json
{
  "target": {
    "type": "file",
    "id": 1
  },
  "password": "123456",
  "expires_at": "2026-03-31T12:00:00Z",
  "max_downloads": 10
}
```

Notes:

- `target.type` can only be `file` or `folder`
- the old top-level `file_id` / `folder_id` request shape is no longer accepted
- only one active share is allowed for the same resource at the same time
- `max_downloads = 0` means unlimited
- an empty password is treated as no password
- `GET /shares` is paginated with `limit` and `offset`

Edit request:

```json
{
  "password": "new-secret",
  "expires_at": "2026-04-02T12:00:00Z",
  "max_downloads": 5
}
```

Edit semantics:

- omitted `password`: keep the current password
- `password = ""`: remove the password
- `password = "xxx"`: replace the password
- `expires_at = null`: never expires
- `max_downloads = 0`: unlimited downloads

Batch-delete request:

```json
{
  "share_ids": [1, 2, 3]
}
```

Each share is processed independently, with a maximum of 1000 items per request. The response uses the same `BatchResult` shape as other batch APIs.

## Public access

The following `/s/...` paths are still relative to `/api/v1`, so the full REST path is `/api/v1/s/{token}/*`. The frontend public page route is `/s/:token`.

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/s/{token}` | Read public share information |
| `POST` | `/s/{token}/verify` | Verify the share password |
| `POST` | `/s/{token}/preview-link` | Create a short-lived preview link for a shared file |
| `GET` | `/s/{token}/archive-preview` | Read archive preview manifest for a shared archive file |
| `POST` | `/s/{token}/stream-session` | Create a short-lived stream session for a shared file |
| `GET` | `/s/{token}/download` | Download a shared file |
| `POST` | `/s/{token}/archive-download` | Create a ZIP download ticket for files / folders in share scope |
| `GET` | `/s/{token}/archive-download/{ticket}` | Consume the ticket and stream the ZIP |
| `GET` | `/s/{token}/stream/{session_token}/{filename}` | Stream a shared file through a stream session |
| `GET` | `/s/{token}/content` | List the root contents of a folder share |
| `GET` | `/s/{token}/folders/{folder_id}/content` | Browse a subfolder inside the shared tree |
| `GET` | `/s/{token}/folders/{folder_id}/ancestors` | Read ancestors relative to the shared root |
| `GET` | `/s/{token}/files/{file_id}/download` | Download a child file inside a folder share |
| `POST` | `/s/{token}/files/{file_id}/preview-link` | Create a preview link for a child file |
| `GET` | `/s/{token}/files/{file_id}/archive-preview` | Read archive preview for a child archive |
| `POST` | `/s/{token}/files/{file_id}/stream-session` | Create a stream session for a child file |
| `GET` | `/s/{token}/thumbnail` | Get thumbnail for a shared file |
| `GET` | `/s/{token}/image-preview` | Get WebP image preview for a shared file |
| `GET` | `/s/{token}/media-metadata` | Get media metadata for a shared file |
| `GET` | `/s/{token}/files/{file_id}/thumbnail` | Get thumbnail for a child file |
| `GET` | `/s/{token}/files/{file_id}/image-preview` | Get WebP image preview for a child file |
| `GET` | `/s/{token}/files/{file_id}/media-metadata` | Get media metadata for a child file |
| `GET` | `/s/{token}/avatar/{size}` | Get the share owner's uploaded avatar |

Important details:

- `/verify` writes a one-hour `aster_share_<token>` cookie on success
- password-protected preview, archive-preview, stream, metadata, thumbnail, and download calls require that cookie
- file-only endpoints reject folder shares, and folder-tree endpoints validate that child resources are still inside the shared subtree
- archive download accepts mixed `file_ids` / `folder_ids` plus optional `archive_name`, and returns a short-lived `StreamTicketInfo`
- consuming `/archive-download/{ticket}` returns a raw ZIP stream instead of wrapped JSON and does not create a normal `/tasks` background task
- ancestors contain only the path below the shared root; out-of-scope folders return `403`
- archive preview supports the same `filename_encoding` query as authenticated file APIs
- metadata / thumbnail / archive-preview endpoints may return `202` while background generation is queued
- image-preview responses are raw WebP with `ETag`, not wrapped JSON
- `/avatar/{size}` currently supports uploaded avatars at sizes `512` and `1024`

Stream session response:

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "path": "/api/v1/s/share_token/stream/session_token/audio.mp3",
    "expires_at": "2026-04-12T15:00:00Z"
  }
}
```

Stream sessions default to a 3-hour lifetime, controlled by `share_stream_session_ttl_secs`. They support `Range` and return raw file streams. A stream session consumes the share download counter only once; response-build failures attempt to roll that counter back.

Shared archive download request:

```json
{
  "file_ids": [11, 12],
  "folder_ids": [21],
  "archive_name": "shared-selection.zip"
}
```

Both ticket creation and consumption enforce the password cookie, share subtree, download counter, and `archive_download_share_enabled`; disabling the feature returns `archive_download.share_disabled`. A ticket is bound to the share token that created it and cannot be consumed under another share. Password-protected shares must complete `/verify` before using archive-download or ancestors endpoints.

Folder-share content listing supports the same directory-list parameters:

- `folder_limit` / `folder_offset`
- `file_limit`
- `sort_by` / `sort_order`
- `file_after_value` / `file_after_id`

Public share archive preview uses the same manifest structure as authenticated archive preview, but requires `archive_preview_share_enabled = true` and uses public cache-control semantics.
