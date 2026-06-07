# WOPI

WOPI capabilities have two layers:

- launch layer: an authenticated user creates a WOPI launch session for a file
- protocol layer: the Office / WOPI host calls back to `/api/v1/wopi/files/{id}` and `/contents`

## Launch endpoints

The following paths are relative to `/api/v1` and require authentication.

| Method | Path | Description |
| --- | --- | --- |
| `POST` | `/files/{id}/wopi/open` | Create a WOPI launch session for a personal file |
| `POST` | `/teams/{team_id}/files/{id}/wopi/open` | Create a WOPI launch session for a team file |

Request body:

```json
{
  "app_key": "custom.onlyoffice"
}
```

Response is `WopiLaunchSession` inside the unified wrapper:

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "access_token": "...",
    "access_token_ttl": 1775995200000,
    "action_url": "https://office.example.com/hosting/wopi/word/edit?WOPISrc=https%3A%2F%2Fdrive.example.com%2Fapi%2Fv1%2Fwopi%2Ffiles%2F1",
    "form_fields": {},
    "mode": "iframe"
  }
}
```

Semantics:

- `app_key` must match an enabled `/public/preview-apps` entry with `provider = "wopi"`
- `public_site_url` must be configured because the server must generate an absolute `WOPISrc`
- with multiple public origins, the current request host is matched exactly first, then the first configured origin is used as fallback
- if `config.action_url` is configured, the server expands or appends `WOPISrc` directly
- otherwise, if `config.discovery_url` is configured, discovery XML is fetched and an action URL is selected by extension, MIME type, then wildcard
- `access_token_ttl` is a Unix millisecond expiry timestamp, not a TTL in seconds
- team launch routes still callback to the shared `/api/v1/wopi/files/{id}`; the team scope is encoded in the access token

## Protocol callback endpoints

These paths are also relative to `/api/v1`, but they are WOPI host callbacks, not ordinary frontend JSON APIs.

Successful responses are raw WOPI JSON or file streams. Errors reuse the unified `ApiResponse` JSON error shape.

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/wopi/files/{id}?access_token=...` | `CheckFileInfo` |
| `POST` | `/wopi/files/{id}?access_token=...` | `LOCK` / `UNLOCK` / `REFRESH_LOCK` / `GET_LOCK` / `PUT_RELATIVE` / `RENAME_FILE` / `PUT_USER_INFO` |
| `GET` | `/wopi/files/{id}/contents?access_token=...` | Read file content |
| `POST` | `/wopi/files/{id}/contents?access_token=...` | Overwrite content with `X-WOPI-Override: PUT` |

## `GET /wopi/files/{id}`

Returns raw WOPI `CheckFileInfo` JSON, not `ApiResponse`.

Current fields include:

- `BaseFileName`
- `FileNameMaxLength`
- `OwnerId`
- `Size`
- `UserId`
- `UserCanNotWriteRelative`
- `UserCanRename`
- `UserInfo`
- `UserCanWrite`
- `ReadOnly`
- `SupportsGetLock`
- `SupportsLocks`
- `SupportsExtendedLockLength`
- `SupportsRename`
- `SupportsUserInfo`
- `SupportsUpdate`
- `Version`

Current implementation sets write/update/lock/rename capabilities according to the existing file permissions and persisted launch session. If the user profile contains `wopi_user_info`, it is passed through to `UserInfo`.

## `POST /wopi/files/{id}`

Dispatches by `X-WOPI-Override`:

- `LOCK`
- `UNLOCK`
- `REFRESH_LOCK`
- `GET_LOCK`
- `PUT_RELATIVE`
- `RENAME_FILE`
- `PUT_USER_INFO`

`LOCK` + `X-WOPI-OldLock` performs WOPI `UnlockAndRelock`. Unsupported overrides return `501 Not Implemented`.

### Lock operations

`LOCK` / `UNLOCK` / `REFRESH_LOCK` / `GET_LOCK` use:

- `X-WOPI-Lock`

`UnlockAndRelock` additionally uses:

- `X-WOPI-OldLock`

Conflicts return `409` with `X-WOPI-LockFailureReason` and, when known, `X-WOPI-Lock`.

Current implementation notes:

- loading WOPI lock state prunes expired lock rows for the file before evaluating the request
- repeating `LOCK` with the same opaque lock value refreshes the lock timeout
- `LOCK` with `X-WOPI-OldLock` performs an atomic unlock-and-relock when the old value matches
- non-WOPI locks and multiple simultaneous active lock rows are treated as conflicts outside WOPI, usually with an empty `X-WOPI-Lock`
- `GET_LOCK` returns `200` with the current `X-WOPI-Lock`; when no active lock exists, the value is empty
- `UNLOCK` / `REFRESH_LOCK` without an active lock return `409`

### `PUT_RELATIVE`

Creates or overwrites a sibling file. Required headers:

- exactly one of `X-WOPI-SuggestedTarget` or `X-WOPI-RelativeTarget`
- optional `X-WOPI-OverwriteRelativeTarget`
- optional `X-WOPI-Size`

Success returns raw WOPI JSON with `Name` and `Url`. Conflicts may include `X-WOPI-ValidRelativeTarget`.

### `RENAME_FILE`

Driven by:

- `X-WOPI-RequestedName`
- optional `X-WOPI-Lock`

The server validates and normalizes the requested name. Name conflicts are resolved by returning an available name rather than blindly failing. Invalid names return `400` with `X-WOPI-InvalidFileNameError`.

### `PUT_USER_INFO`

Persists a WOPI host user-state string to the user profile. Constraints:

- body must be valid UTF-8
- body must be ASCII
- maximum length is `1024` bytes

## `GET /wopi/files/{id}/contents`

`GET /wopi/files/{id}/contents` returns the raw file stream. It behaves like normal download: inline by default, supports `If-None-Match`, and supports `X-WOPI-MaxExpectedSize`.

## `POST /wopi/files/{id}/contents`

`POST /wopi/files/{id}/contents` currently supports only `X-WOPI-Override: PUT`. It overwrites through the normal file-content update path, writes version history, updates ETag/version information, and returns `X-WOPI-ItemVersion`. If an active WOPI lock exists, the caller must provide the matching `X-WOPI-Lock`.

## Security boundary

The protocol layer validates:

- access token exists and is not expired
- file ID, user session version, and team scope in the token match
- disabled users, revoked sessions, disabled WOPI apps, or removed WOPI apps invalidate persisted sessions
- when trusted origins can be derived from app config, `Origin` / `Referer` is checked

Missing `Origin` and `Referer` are allowed. Malformed headers return `400`. Untrusted origins return an unauthorized protocol response.

## Related docs

- [Files](./files.md)
- [Teams and team spaces](./teams.md)
- [Public API](./public.md)
