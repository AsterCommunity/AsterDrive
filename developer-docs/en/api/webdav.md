# WebDAV API and Protocol Capabilities

WebDAV-related content has three parts: accounts, mount entry, and protocol capabilities.

The protocol layer is currently split under `src/webdav/**`: `mod.rs` handles Actix mounting, runtime switches, audit context, and method dispatch; `auth.rs` handles WebDAV-specific Basic Auth; `protocol.rs` parses `Depth`, `Destination`, `If`, ETag, and related protocol headers; `responses.rs` centralizes HTTP / XML response builders; `props/` handles `PROPFIND` / `PROPPATCH`; `transfer/` handles `GET` / `HEAD` / `PUT`; `resources/` handles `MKCOL` / `DELETE` / `COPY` / `MOVE`; `locks/` handles `LOCK` / `UNLOCK`; `fs/`, `file/`, and `dir_entry.rs` adapt the filesystem; `path_resolver.rs` resolves paths; `db_lock_system.rs` implements locks; and `deltav.rs` contains the minimal DeltaV subset.

## Account API

The following paths are relative to `/api/v1` and require authentication.

### Personal accounts

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/webdav-accounts` | List the current user's WebDAV accounts |
| `POST` | `/webdav-accounts` | Create a WebDAV account |
| `DELETE` | `/webdav-accounts/{id}` | Delete a WebDAV account |
| `POST` | `/webdav-accounts/{id}/toggle` | Enable or disable an account |
| `GET` | `/webdav-accounts/settings` | Read the active mount prefix and client endpoint |
| `POST` | `/webdav-accounts/test` | Test a set of WebDAV credentials |

### Team accounts

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/teams/{team_id}/webdav-accounts` | List team WebDAV accounts |
| `POST` | `/teams/{team_id}/webdav-accounts` | Create a team WebDAV account |
| `DELETE` | `/teams/{team_id}/webdav-accounts/{account_id}` | Delete a team WebDAV account |
| `POST` | `/teams/{team_id}/webdav-accounts/{account_id}/toggle` | Enable or disable a team WebDAV account |

Common details:

- if `password` is empty when creating an account, the server generates a random password
- plaintext password is returned only once at creation time
- for personal accounts, `root_folder_id = null` means the account can access the whole user space; for team accounts, it means the account can access the whole team space
- when `root_folder_id` is provided, the server verifies that the folder belongs to the personal or team workspace for that account
- `/toggle` has no request body; each call switches enabled / disabled state
- `/settings` returns:
  - `prefix`: the active server mount prefix
  - `endpoint`: a client-usable URL; if `public_site_url` is configured, this is absolute, otherwise it is relative. With multiple public origins, the server matches the current request origin exactly and falls back to the first configured origin when no match is found.
- `/test` validates credentials without requiring a real client mount
- `GET /webdav-accounts` is paginated with `limit` and `offset`
- `GET /teams/{team_id}/webdav-accounts` is also paginated with `limit` and `offset`
- team members can create team WebDAV accounts; ordinary members can list, delete, and toggle only accounts they created, while team `owner` / `admin` users can list and manage all WebDAV accounts in the team
- team WebDAV accounts must be managed through `/teams/{team_id}/webdav-accounts/*`; the personal `/webdav-accounts/{id}` endpoints reject team accounts

Create request:

```json
{
  "username": "dav-demo",
  "password": null,
  "root_folder_id": 12
}
```

## Mount URL

The default WebDAV path is:

```text
/webdav
```

Example full URL:

```text
http://localhost:3000/webdav
```

Changing `[webdav].prefix` changes the mount URL as well.

## Protocol capabilities

Common WebDAV methods are supported:

- `PROPFIND`
- `PROPPATCH`
- `MKCOL`
- `PUT`
- `GET`
- `HEAD`
- `DELETE`
- `COPY`
- `MOVE`
- `LOCK`
- `UNLOCK`
- `OPTIONS`

A minimal DeltaV subset is also implemented:

- `REPORT` with `DAV:version-tree`
- `VERSION-CONTROL`
- `OPTIONS` with `DAV: version-control`

This reuses `file_versions`, so clients can read version trees.

Limits:

- `REPORT version-tree` supports files only
- this is not a full DeltaV server, only the minimal useful subset
- the `/webdav/` mount root is a virtual entry point, not a persisted folder entity. `PROPFIND /webdav/` may list contents and read live DAV properties, but `PROPPATCH /webdav/` explicitly returns `403 Forbidden`; custom dead properties are supported only on concrete files or folders.
- missing `Depth` on `PROPFIND` is parsed as `infinity`; when the target is a collection, the server returns `403` with `DAV:propfind-finite-depth` instead of doing unbounded recursion.
- `COPY` accepts `Depth: 0` or missing / `infinity`, and rejects `Depth: 1`; `COPY Depth: 0` copies only the collection itself and its dead properties, not children.
- `GET` supports `Range: bytes=...` and returns `206 Partial Content` for partial content; `GET` / `HEAD` support `If-None-Match` and return `304` when it matches.
- `PUT`, `DELETE`, `COPY`, and `MOVE` evaluate standard ETag preconditions and WebDAV `If` lock-token conditions.
- `MKCOL` requires an empty request body; non-empty bodies return `415 Unsupported Media Type`.
- `Destination` must stay on the current WebDAV server and under the active WebDAV prefix.
- `LOCK` supports exclusive and shared write locks; multiple shared locks may coexist, while an exclusive lock blocks other shared / exclusive locks.
- creating a `LOCK` on a missing non-collection path creates a zero-byte file and returns `201 Created`.

## Authentication and runtime switches

- Basic Auth: uses a dedicated WebDAV account and may be restricted to `root_folder_id`
- the current WebDAV mount does not accept normal `Authorization: Bearer <jwt>`; Bearer is rejected as an unsupported auth scheme
- if `webdav_enabled = false`, WebDAV requests return `503`
- if `webdav_block_system_files_enabled = true`, WebDAV writes / moves / copies are blocked according to `webdav_block_system_file_patterns`, which by default includes common client junk names such as `.DS_Store`, `._*`, `Thumbs.db`, `desktop.ini`, and `$RECYCLE.BIN`. REST folder listing does not apply this filter

When deployed behind a reverse proxy, make sure the proxy allows WebDAV methods and related headers. See [reverse proxy deployment](https://drive.astercosm.com/en/deployment/reverse-proxy/).

## Conformance and client compatibility testing

For local protocol checks, the pinned Litmus baseline, known-difference management, the real-client matrix, and CI artifacts, see [WebDAV Conformance and Compatibility Testing](../testing/webdav-compliance-testing.md).
