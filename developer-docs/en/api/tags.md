# Tags

The following paths are relative to `/api/v1` and require authentication.

Tags are scoped to a workspace:

- personal tags live under `/tags`
- team tags live under `/teams/{team_id}/tags`

Personal and team tag libraries are isolated. A tag created in a personal workspace cannot be attached to a team item, and a team tag cannot be attached to a personal item.

## Endpoints

### Personal Workspace

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/tags` | List the current user's personal tags |
| `POST` | `/tags` | Create a personal tag |
| `PATCH` | `/tags/{tag_id}` | Rename or recolor a personal tag |
| `DELETE` | `/tags/{tag_id}` | Delete a personal tag and detach it from bound items |
| `GET` | `/tags/{entity_type}/{entity_id}` | List tags attached to one file or folder |
| `PUT` | `/tags/{entity_type}/{entity_id}` | Replace all tags attached to one file or folder |
| `PUT` | `/tags/{tag_id}/{entity_type}/{entity_id}` | Attach one tag to one file or folder |
| `DELETE` | `/tags/{tag_id}/{entity_type}/{entity_id}` | Detach one tag from one file or folder |
| `PUT` | `/tags/{tag_id}/batch` | Attach one tag to multiple files and folders |
| `DELETE` | `/tags/{tag_id}/batch` | Detach one tag from multiple files and folders |

### Team Workspace

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/teams/{team_id}/tags` | List team tags |
| `POST` | `/teams/{team_id}/tags` | Create a team tag |
| `PATCH` | `/teams/{team_id}/tags/{tag_id}` | Rename or recolor a team tag |
| `DELETE` | `/teams/{team_id}/tags/{tag_id}` | Delete a team tag and detach it from bound items |
| `GET` | `/teams/{team_id}/tags/{entity_type}/{entity_id}` | List tags attached to one team file or folder |
| `PUT` | `/teams/{team_id}/tags/{entity_type}/{entity_id}` | Replace all tags attached to one team file or folder |
| `PUT` | `/teams/{team_id}/tags/{tag_id}/{entity_type}/{entity_id}` | Attach one tag to one team file or folder |
| `DELETE` | `/teams/{team_id}/tags/{tag_id}/{entity_type}/{entity_id}` | Detach one tag from one team file or folder |
| `PUT` | `/teams/{team_id}/tags/{tag_id}/batch` | Attach one tag to multiple team files and folders |
| `DELETE` | `/teams/{team_id}/tags/{tag_id}/batch` | Detach one tag from multiple team files and folders |

`entity_type` currently supports `file` and `folder`.

## Tag Library

`GET /tags` and `GET /teams/{team_id}/tags` use offset pagination:

- `limit`: default `50`, capped by the shared API page maximum
- `offset`: default `0`
- `q`: optional case-insensitive name search, max 64 characters

The response is an `OffsetPage<TagInfo>`:

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "items": [
      {
        "id": 7,
        "scope_type": "personal",
        "owner_user_id": 1,
        "team_id": null,
        "name": "Invoice",
        "normalized_name": "invoice",
        "color": "#3b82f6",
        "sort_order": 0,
        "usage_count": 12,
        "created_at": "2026-06-10T12:00:00Z",
        "updated_at": "2026-06-10T12:00:00Z"
      }
    ],
    "total": 1,
    "limit": 50,
    "offset": 0
  }
}
```

Create request:

```json
{
  "name": "Invoice",
  "color": "#3b82f6"
}
```

Patch request:

```json
{
  "name": "Receipts",
  "color": "#22c55e"
}
```

Rules:

- tag names are trimmed, cannot be empty, and are capped at 64 Unicode scalar characters
- names are normalized with lower-case comparison and must be unique inside the same workspace
- `color` must be a 7-character hex color such as `#3b82f6`
- deleting a tag removes its `system.tags` entity-property bindings but does not delete files or folders

## Entity Bindings

Entity tag responses use this shape:

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "tags": [
      {
        "id": 7,
        "name": "Invoice",
        "color": "#3b82f6"
      }
    ]
  }
}
```

Replace request:

```json
{
  "tag_ids": [7, 8]
}
```

Batch attach / detach request:

```json
{
  "file_ids": [10, 11],
  "folder_ids": [20]
}
```

Binding rules:

- `tag_ids` cannot contain more than 64 IDs and must all belong to the current workspace
- batch requests accept up to 1024 file IDs and 1024 folder IDs
- file and folder IDs are verified in the current personal or team workspace before mutation
- attach, detach, and replace operations are idempotent from the client's perspective
- single-entity attach, detach, and replace responses return the current tag list for that entity
- batch attach and detach return an empty success response

## Search and Events

The search API can filter by tags:

- `tag_ids=1,2,3`
- `tag_match=any` or `tag_match=all`, default `any`

See [Search](./search.md) for the full query contract.

Tag writes also publish storage-change events to authenticated subscribers:

- `tag.created`
- `tag.updated`
- `tag.deleted`
- `tag.assignment_changed`

These events are delivered through `GET /auth/events/storage` when the affected workspace is visible to the current user. See [Authentication](./auth.md) for the SSE endpoint.
