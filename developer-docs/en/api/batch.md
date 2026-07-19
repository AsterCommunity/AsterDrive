# Batch Operations

The following paths are relative to `/api/v1` and require authentication.

## Endpoints

| Method | Path | Description |
| --- | --- | --- |
| `POST` | `/batch/delete` | Batch-delete files and folders |
| `POST` | `/batch/move` | Batch-move files and folders |
| `POST` | `/batch/copy` | Batch-copy files and folders |
| `POST` | `/workspace-transfer/copy` | Copy files and folders between workspaces |
| `POST` | `/workspace-transfer/move` | Move files and folders between workspaces |
| `POST` | `/batch/archive-compress` | Create an archive-compress background task |
| `POST` | `/batch/archive-download` | Create a batch ZIP download ticket |
| `GET` | `/batch/archive-download/{token}` | Stream a ZIP by ticket |

## Request body

Selection-style request bodies use mixed resource selection:

```json
{
  "file_ids": [1, 2],
  "folder_ids": [10, 11]
}
```

- `file_ids` and `folder_ids` can both be present
- the total item count per request is capped at 1000
- each item is processed independently; one failure does not roll back the rest

## Response shape

- `POST /batch/delete`
- `POST /batch/move`
- `POST /batch/copy`

return a `BatchResult` structure with:

- `succeeded`
- `failed`
- `errors`

The frontend uses this to drive batch progress and grouped toast messages.

- `POST /batch/archive-compress` returns `TaskInfo`
- `POST /batch/archive-download` returns `StreamTicketInfo`

## `POST /batch/delete`

Behavior:

- files and folders use the same soft-delete logic as the single-item endpoints
- results are counted per item
- one failure does not stop the rest of the batch

## `POST /batch/move`

Request body also includes the target folder:

```json
{
  "file_ids": [1, 2],
  "folder_ids": [10],
  "target_folder_id": 99
}
```

Behavior:

- files and folders can be moved together to the same destination
- `/batch/move` requires at least two selected resources; one file uses `PATCH /files/{id}` and one folder uses `PATCH /folders/{id}`
- `target_folder_id = null` means move to root
- drag-move and batch-move reuse the same capability

## `POST /batch/copy`

Request body:

```json
{
  "file_ids": [1],
  "folder_ids": [10],
  "target_folder_id": 99
}
```

Behavior:

- `/batch/copy` requires at least two selected resources; use the single-item copy endpoint for one file or folder
- file copy does not physically duplicate the blob; it only increments the reference count
- folder copy recursively copies the subtree
- name conflicts at the destination are resolved automatically, just like single-item copy

## Cross-workspace copy and move

Personal and team spaces are separate resource scopes. Cross-scope operations use explicit
transfer endpoints instead of overloading `/batch/copy` or `/batch/move`:

- `POST /workspace-transfer/copy`
- `POST /workspace-transfer/move`

### Request body

Both endpoints use the same request shape:

```json
{
  "source_workspace": { "kind": "personal" },
  "file_ids": [1, 2],
  "folder_ids": [10],
  "destination_workspace": { "kind": "team", "team_id": 42 },
  "target_folder_id": null
}
```

`kind = "personal"` refers to the current user's personal space. `kind = "team"` must include
a positive `team_id`. `target_folder_id = null` means the root of the destination space.

### Scope and access checks

- The actor must have access to both source and destination spaces.
- Every source file and folder must belong to `source_workspace`.
- The target folder must belong to `destination_workspace`.
- Personal-space resources are never accepted as another user's personal resources.
- Team membership is checked again on the server for every transfer request.
- Empty selections, non-positive IDs, and batches larger than 1000 items are rejected as validation errors.

### Copy behavior

`/workspace-transfer/copy` leaves the source untouched and creates records under the destination scope:

- Folder copies include descendant folders and files.
- Name conflicts receive an automatically generated copy name.
- Ownership, creator fields, and quota accounting are recalculated for the destination.
- Existing blob content may be reused by reference instead of being written again.
- Storage-change events are published for the destination scope.

### Move behavior

For cross-workspace moves, `/workspace-transfer/move` uses a copy-then-trash sequence:

1. Create destination resources using copy semantics.
2. Move source resources to trash only after every selected item copied successfully.
3. If any copy item fails, keep the source resources and return per-item errors.

Callers must inspect `succeeded`, `failed`, and `errors`; a partial result is not a completed move.
Locked selected files and folders are rejected before a cross-workspace move starts. Once source
resources enter trash, the existing restore, purge, and quota-cleanup rules apply.

Single-item moves inside one workspace use the file/folder PATCH endpoint; multi-item moves use `/batch/move`. Neither path uses the cross-workspace copy flow.

### Result and events

Both endpoints return the regular `BatchResult`. A successful cross-workspace move normally emits:

- file/folder creation events for the destination scope;
- trash events for the source scope;
- an audit entry containing both source and destination workspace details.

When copy is only partially successful, no source deletion events are emitted, although successfully
copied destination items may already exist. Clients should reload both scopes from the result instead
of updating a single visible row in place.

## Archive download and archive compress

### `POST /batch/archive-compress`

Creates an `archive_compress` background task that packages the selected files / folders into a ZIP and writes the result back into the current workspace.

Request body:

```json
{
  "file_ids": [1, 2],
  "folder_ids": [10],
  "archive_name": "workspace-export",
  "target_folder_id": 99
}
```

Semantics:

- if `target_folder_id = null`, the server first checks whether the selected items all come from the same parent directory; if they do, the ZIP is written back there, otherwise it goes to the root
- the endpoint returns `TaskInfo`, not a file stream
- the result later appears in the background-task API
- the team-space variant is mounted under `/teams/{team_id}/batch/archive-compress`

### `POST /batch/archive-download`

Request body:

```json
{
  "file_ids": [1, 2],
  "folder_ids": [10],
  "archive_name": "workspace-export"
}
```

Success returns a short-lived stream ticket:

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "token": "st_xxxxx",
    "download_path": "/api/v1/batch/archive-download/st_xxxxx",
    "expires_at": "2026-04-12T12:00:00Z"
  }
}
```

Key points:

- `archive_name` is optional; the final file always ends in `.zip`
- the ticket expires after 5 minutes by default
- `download_path` may be relative or absolute depending on `public_site_url`
- the ticket is bound to the current user and workspace, so it cannot be reused elsewhere
- this path is a short-lived ticket plus direct streaming ZIP download, not a background-task record

### `GET /batch/archive-download/{token}`

The `download_path` from the previous call returns the raw `application/zip` stream.

Implementation notes:

- empty folders are preserved
- multiple selected folders are packed according to the current tree shape
- duplicate top-level names inside the ZIP are automatically disambiguated
- only active items that are visible to the current workspace are included

## Related docs

- [Files](./files.md)
- [Folders](./folders.md)
- [Core workflows](https://drive.astercosm.com/en/guide/core-workflows/)
