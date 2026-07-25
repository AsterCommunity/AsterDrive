# Internal Storage Protocol (Follower)

These endpoints are the internal object storage protocol between the primary node and follower nodes. They are not public browser or third-party client APIs.

This page describes the follower-side `/api/v1/internal/storage/*` endpoints that actually perform object reads and writes. The primary also exposes `/api/v1/internal/remote-tunnel/*` for followers that cannot be reached directly by the primary.

All paths below are relative to:

```text
/api/v1/internal/storage
```

and are registered only on `follower` nodes.

## Direct vs. Reverse Tunnel

Remote-node object protocol has two layers:

- `/api/v1/internal/storage/*` exists only on the follower and performs object access, binding sync, and remote storage target management
- `/api/v1/internal/remote-tunnel/*` exists only on the primary and is the reverse-tunnel control and transport entry

In `direct` mode, the primary directly calls the follower's `/api/v1/internal/storage/*`. In `reverse_tunnel` mode, the primary registers the same internal storage request in the tunnel registry; the follower polls or opens a WebSocket to the primary, runs the internal storage logic locally, and returns the response.

Primary-side reverse-tunnel entries:

| Method | Path | Description |
| --- | --- | --- |
| `POST` | `/api/v1/internal/remote-tunnel/poll` | Follower long-polls pending requests |
| `POST` | `/api/v1/internal/remote-tunnel/complete` | Follower returns a polled request result |
| `GET` | `/api/v1/internal/remote-tunnel/connect` | Follower opens a streaming WebSocket tunnel |

These reverse-tunnel endpoints also use remote-node signature auth.

## Authentication

Two access forms are supported:

- primary-signed request headers:
  - `x-aster-access-key`
  - `x-aster-timestamp`
  - `x-aster-nonce`
  - `x-aster-signature`
- presigned query:
  - `aster_access_key`
  - `aster_expires`
  - `aster_signature`

Control-plane endpoints require signed headers. Object GET / PUT can support presigned URLs depending on the scenario.

## Endpoints

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/capabilities` | Read follower protocol capabilities |
| `GET` | `/capacity` | Read capacity status for the current follower receiving target |
| `PUT` | `/binding` | Sync remote-node binding information maintained by the primary |
| `GET` | `/targets` | List remote storage targets available to the current binding |
| `POST` | `/targets` | Create a remote storage target |
| `PATCH` | `/targets/{target_key}` | Update a remote storage target |
| `DELETE` | `/targets/{target_key}` | Delete a remote storage target |
| `POST` | `/compose` | Compose part objects into a target object |
| `GET` | `/objects` | List object keys by prefix |
| `GET` | `/objects/{tail}/metadata` | Read object metadata |
| `PUT` | `/objects/{tail}` | Upload object content |
| `GET` | `/objects/{tail}` | Read object content |
| `HEAD` | `/objects/{tail}` | Probe object existence and headers |
| `DELETE` | `/objects/{tail}` | Delete object |

Version `0.4.0` removed the legacy `/ingress-profiles` and `/ingress-profiles/{target_key}` compatibility paths. Primaries and followers must use `/targets`.

## `GET /capabilities`

Typical fields include:

- `protocol_version`
- `min_supported_protocol_version`
- `server_version`
- `features`
- `browser_cors`
- `limits`
- `supports_list`
- `supports_range_read`
- `supports_stream_upload`
- `supports_capacity`

The current protocol version is `v5`, with `v4` as the compatibility floor, so the current local supported range is `v4-v5`. `v4` / `v5` are not wire-compatible with `v2` or `v3`: internal storage JSON envelopes now use the stable string `ApiErrorCode` in the top-level `code` field instead of the old numeric code. Upgrade both primary and follower before binding remote policies across this boundary.

Protocol `v5` adds remote-storage-target driver capabilities. The Rust model field is named `remote_storage_target`, but the `v4` / `v5` wire JSON still serializes it as `managed_ingress` and also accepts `remote_storage_target` as a deserialization alias. The legacy wire key is a compatibility detail, not a separate product model.

The compatibility boundary is explicit:

- When a `v4` follower omits this capability, the primary applies the legacy assumption that Local and S3 drivers are available.
- A `v5` follower must declare remote-storage-target capability explicitly. Missing or disabled capability does not receive the implicit `v4` Local / S3 fallback.
- The primary exposes only drivers both declared by the follower and backed by a descriptor registered in the current build. Unknown future driver IDs remain protocol data but do not automatically become configurable choices.

During policy loading and binding refresh, the primary validates:

- `protocol_version` / `min_supported_protocol_version` must overlap with the local supported range, currently `v4-v5`
- Base remote policies require `object_get`, `object_head`, `object_put`, `object_delete`, `metadata`, `range_get`, `accept_ranges_header`, `list`, and `compose`
- Browser presigned download requires `browser_cors` to allow the `range` request header and expose `Accept-Ranges`, `Content-Range`, and `Content-Length`
- Browser presigned upload requires `browser_cors` to allow the `content-type` request header and expose `ETag`

## `GET /capacity`

Returns `StorageCapacityInfo` for the follower's current remote storage target driver:

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "capacity": {
      "status": "supported",
      "total_bytes": 1099511627776,
      "available_bytes": 549755813888,
      "used_bytes": 549755813888,
      "source": "local_filesystem",
      "observed_at": "2026-05-28T12:00:00Z"
    }
  }
}
```

Local targets usually return real filesystem capacity. S3 targets explicitly return unsupported, which the primary converts into a user-visible `unsupported` capacity state. This endpoint is used for admin capacity observation and migration preflight, not the hot upload / download path.

## `PUT /binding`

The primary uses this endpoint to sync follower binding metadata. Request fields include:

- `name`
- `is_enabled`

This updates binding metadata only; it does not move object data.

## Remote storage target management

These endpoints let the primary manage follower-side remote storage targets, deciding whether future object writes land in follower-local storage or follower-managed S3.

Local target request:

```json
{
  "driver_type": "local",
  "name": "local-default",
  "base_path": "data/storage",
  "is_default": true
}
```

S3 target request:

```json
{
  "driver_type": "s3",
  "name": "edge-s3",
  "endpoint": "https://s3.example.com",
  "bucket": "aster-edge",
  "access_key": "AKIA...",
  "secret_key": "...",
  "base_path": "objects/",
  "is_default": false
}
```

Updates use a flat request shape and can change `name`, `driver_type`, connection fields, `base_path`, and `is_default`. The current target drivers are `local` and `s3`; the actual choices are also constrained by the follower's `remote_storage_target` capability declaration. These control-plane endpoints accept only signed primary headers, not presigned query access.

## Object operations

### `POST /compose`

Composes uploaded parts into a final object. Request fields include:

- `target_key`
- `part_keys`
- `expected_size`

Successful composition returns `bytes_written` and cleans consumed part objects.

### `PUT /objects/{tail}`

Writes one object. The request must include `Content-Length`, and the follower checks size limits according to the ingress policy.

### `GET /objects/{tail}`

Returns raw object bytes, not JSON.

Optional query parameters:

- `offset`
- `length`
- `response-cache-control`
- `response-content-disposition`
- `response-content-type`

Standard `Range: bytes=...` also works; partial responses use `206 Partial Content`.

### `HEAD /objects/{tail}`

Returns object existence and basic response headers.

### `GET /objects/{tail}/metadata`

Returns wrapped JSON with fields such as:

- `size`
- `content_type`

### `DELETE /objects/{tail}`

Deletes the object and returns an empty success response.

## Listing

`GET /objects` supports:

- `prefix`: return only object keys with the requested prefix
- `cursor`: continue from a relative position, normally using the previous page's `next_cursor`
- `limit`: requested page size; it must be greater than `0` and is clamped to the server's internal page-size ceiling

New clients should always send `limit`. A paged response looks like:

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "items": ["files/part-001", "files/part-002"],
    "next_cursor": 2
  }
}
```

`next_cursor` is present only when another page exists. Omitting `limit` preserves the legacy unpaged response used by older clients and returns every matching item at once. Returned `items` are relative keys inside the follower binding namespace, not raw provider prefixes.

## When to read this page

Look here instead of the ordinary `files` / `upload` / `shares` routes when:

- primary-to-remote-node writes fail
- managed follower part composition fails
- remote-node health is green but object listing / reading / deletion is wrong
- enrollment succeeded but later object synchronization behaves incorrectly
