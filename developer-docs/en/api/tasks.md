# Background Tasks

The following paths are relative to `/api/v1` and require authentication.

These endpoints list existing tasks, read details, and retry failed tasks. Task creation is distributed across other modules; admins can also create storage-migration tasks.

## Personal space

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/tasks` | Paginated list of current user's personal tasks |
| `GET` | `/tasks/{id}` | Read one personal task |
| `POST` | `/tasks/{id}/retry` | Retry a failed personal task |
| `POST` | `/tasks/offline-download` | Create a personal offline-download import task |

## Team space

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/teams/{team_id}/tasks` | Paginated list of team tasks |
| `GET` | `/teams/{team_id}/tasks/{id}` | Read one team task |
| `POST` | `/teams/{team_id}/tasks/{id}/retry` | Retry a failed team task |
| `POST` | `/teams/{team_id}/tasks/offline-download` | Create a team offline-download import task |

## Common task creators

Current common creation paths include:

- `POST /batch/archive-compress`
- `POST /teams/{team_id}/batch/archive-compress`
- `POST /files/{id}/extract`
- `POST /teams/{team_id}/files/{id}/extract`
- `GET /files/{id}/archive-preview`
- `GET /teams/{team_id}/files/{id}/archive-preview`
- public-share archive preview, thumbnail, and media metadata endpoints
- authenticated thumbnail and media metadata endpoints
- `POST /tasks/offline-download`
- `POST /teams/{team_id}/tasks/offline-download`
- `DELETE /trash`
- `DELETE /teams/{team_id}/trash`
- `POST /admin/storage-migrations`
- `POST /admin/file-blobs/maintenance`

System-created or system-recorded kinds include:

- `thumbnail_generate`
- `media_metadata_extract`
- `storage_policy_migration`
- `storage_policy_temp_cleanup`
- `blob_maintenance`
- `offline_download`
- `system_runtime`

Thumbnail and media metadata tasks are blob-level cache tasks. They often have no creator, so the API returns `creator = null`, and ordinary `/tasks` lists usually do not show them. Admins can see all tasks through `/api/v1/admin/tasks`.

## Storage migration task result

`storage_policy_migration` tasks are created through `/api/v1/admin/storage-migrations`. They have independent checkpoints and can be resumed through `/api/v1/admin/storage-migrations/{task_id}/resume`.

The `result` is `StoragePolicyMigrationTaskResult` and currently includes:

- `source_policy_id`
- `target_policy_id`
- `scanned_blobs`
- `migrated_blobs`
- `merged_blobs`
- `skipped_blobs`
- `failed_blobs`
- `migrated_bytes`
- `renamed_opaque_blobs`

Opaque keys are not content hashes and are never merged across policies. If the target policy already has the same opaque key, the source blob is copied to a new `migration-...` key and counted in `renamed_opaque_blobs`.

## Offline download

`offline_download` imports a file from a URL. Request body:

```json
{
  "url": "https://example.com/file.zip",
  "filename": "file.zip",
  "target_folder_id": 12,
  "expected_sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
}
```

`filename`, `target_folder_id`, and `expected_sha256` are optional. The server stores a redacted source URL in `payload.source_display_url`. On success, `result` includes the imported `file_id`, `file_name`, `folder_id`, `file_path`, `source_display_url`, `content_length`, actual `sha256`, and `download_engine`.

The link-import engines are selected by the administrator runtime registry, not by the task request body. The registry can enable the built-in downloader, aria2, or both in fallback order; if all engines are disabled, create requests are rejected before a background task is inserted. Switching engines does not change the task kind, create request, or `payload` shape. The selected engine is persisted in internal runtime metadata while the task is running and in `result.download_engine` after success, so task presentation can show the actual downloader used. When aria2 is used, the execution GID is also persisted in `background_tasks.runtime_json` for diagnostics and recovery boundaries; runtime metadata is not exposed as a public API field.

## Pagination

List endpoints use offset pagination:

- `limit`
- `offset`

Current defaults:

- default `limit = 20`
- the max is controlled by runtime config `task_list_max_limit`, default `100`
- personal endpoints return tasks created by the current user with `team_id = null`
- team endpoints return tasks for the specified `team_id`

## `TaskInfo`

Lists and details return `TaskInfo`, with fields such as:

- `id`
- `kind`
- `status`
- `display_name`
- `creator`
- `team_id`
- `share_id`
- `progress_current`
- `progress_total`
- `progress_percent`
- `status_text`
- `attempt_count`
- `max_attempts`
- `last_error`
- `payload`
- `result`
- `steps`
- `can_retry`
- `lease_expires_at`
- `started_at`
- `finished_at`
- `expires_at`
- `created_at`
- `updated_at`

`payload` and `result` are structured objects, not the old `payload_json` / `result_json` fields. Internal execution state is not exposed through `TaskInfo`; for example, the aria2 engine persists its GID in `background_tasks.runtime_json`, not in `payload` or `result`. The public offline-download result may include `download_engine`, but this is the selected engine name, not aria2 internal state. `steps` gives per-stage state and progress. `can_retry = true` appears only for failed tasks that allow manual retry.

## Current task kinds and statuses

`BackgroundTaskKind` currently includes:

- `archive_extract`
- `archive_compress`
- `archive_preview_generate`
- `thumbnail_generate`
- `media_metadata_extract`
- `trash_purge_all`
- `storage_policy_temp_cleanup`
- `storage_policy_migration`
- `blob_maintenance`
- `offline_download`
- `system_runtime`

`BackgroundTaskStatus` currently includes:

- `pending`
- `processing`
- `retry`
- `succeeded`
- `failed`
- `canceled`

## `POST /tasks/{id}/retry`

Retry only accepts failed tasks:

- only `status = failed` can be retried
- successful retry resets the task to a pending state
- the implementation clears the task's old temp directory before reset

If the task is not failed, the API returns `400`.

## Notes

- `/batch/archive-download` uses a short-lived stream ticket and direct ZIP streaming; it does not create a `background_tasks` row
- `/batch/archive-compress` and `/files/{id}/extract` do create visible background tasks
- archive preview endpoints return `202` while generation is queued; clients should retry the original endpoint
- `DELETE /trash` creates `trash_purge_all` instead of synchronously emptying trash
- `/tasks/offline-download` returns `TaskInfo` immediately; progress should be shown in the task center; clients should not pass an engine choice in the request body
- `/admin/storage-migrations/dry-run` does not create a task; `POST /admin/storage-migrations` does
- `POST /admin/file-blobs/maintenance` creates `blob_maintenance`
