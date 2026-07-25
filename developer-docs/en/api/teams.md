# Teams and Team Spaces

The following paths are relative to `/api/v1` and require authentication.

Team capabilities have two layers:

- team management: team profile, members, and audit logs
- team workspace: files, folders, uploads, search, tags, shares, WebDAV accounts, trash, and tasks

## Team management

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/teams` | List teams visible to the current user |
| `POST` | `/teams` | Create a team |
| `GET` | `/teams/{id}` | Read team details |
| `PATCH` | `/teams/{id}` | Update team name or description |
| `DELETE` | `/teams/{id}` | Archive a team |
| `POST` | `/teams/{id}/restore` | Restore an archived team |
| `GET` | `/teams/{id}/audit-logs` | Read team audit logs |
| `GET` | `/teams/{id}/members` | Paginated team members |
| `POST` | `/teams/{id}/members` | Add a team member |
| `PATCH` | `/teams/{id}/members/{member_user_id}` | Change member role |
| `DELETE` | `/teams/{id}/members/{member_user_id}` | Remove a member |

Current behavior:

- `GET /teams` supports `archived=true`
- `POST /teams` is still system-admin only and sets the caller as team `owner`
- admins can create a team for someone else through `/admin/teams`; that entry adds the target user with `admin` role
- `DELETE /teams/{id}` archives rather than physically deletes; cleanup happens after `team_archive_retention_days`
- `GET /teams/{id}/audit-logs` requires team `owner` or `admin` and supports filters such as `user_id`, `action`, `after`, `before`, `limit`, `offset`
- `GET /teams/{id}/members` supports `keyword`, `role`, `status`, `limit`, `offset`, `sort_by`, `sort_order`
- `POST /teams/{id}/members` accepts either `user_id` or `identifier`, exactly one of them; omitted `role` defaults to `member`
- member pagination returns `owner_count` and `manager_count` in addition to `items` / `total` / `limit` / `offset`

## Team workspace

Team workspace APIs are rooted at:

```text
/api/v1/teams/{team_id}
```

They are not a separate "team filesystem" implementation. They reuse the personal-space file / folder / upload / search / tag / share / trash / task / WebDAV-account semantics under a team scope.

## Folder and file endpoints

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/teams/{team_id}/folders` | List team root |
| `POST` | `/teams/{team_id}/folders` | Create a team folder |
| `GET` | `/teams/{team_id}/folders/{id}` | List a team subfolder |
| `GET` | `/teams/{team_id}/folders/{id}/info` | Read full team folder info |
| `GET` | `/teams/{team_id}/folders/{id}/ancestors` | Read team breadcrumb ancestors |
| `PATCH` | `/teams/{team_id}/folders/{id}` | Rename, move, or set folder policy |
| `DELETE` | `/teams/{team_id}/folders/{id}` | Soft-delete a team folder |
| `POST` | `/teams/{team_id}/folders/{id}/lock` | Lock / unlock a team folder |
| `POST` | `/teams/{team_id}/folders/{id}/copy` | Recursively copy a team folder |
| `POST` | `/teams/{team_id}/files/upload` | Team multipart direct upload |
| `POST` | `/teams/{team_id}/files/new` | Create an empty team file |
| `POST` | `/teams/{team_id}/files/upload/init` | Negotiate team upload mode |
| `GET` | `/teams/{team_id}/files/upload/sessions` | List recoverable team upload sessions |
| `PUT` | `/teams/{team_id}/files/upload/{upload_id}/{chunk_number}` | Upload a team chunk |
| `POST` | `/teams/{team_id}/files/upload/{upload_id}/presign-parts` | Presign object-storage / remote multipart part URLs |
| `POST` | `/teams/{team_id}/files/upload/{upload_id}/complete` | Complete team upload |
| `GET` | `/teams/{team_id}/files/upload/{upload_id}` | Read team upload progress |
| `DELETE` | `/teams/{team_id}/files/upload/{upload_id}` | Cancel team upload |
| `GET` | `/teams/{team_id}/files/{id}` | Read team file metadata |
| `GET` | `/teams/{team_id}/files/{id}/archive-preview` | Read team archive preview manifest |
| `GET` | `/teams/{team_id}/files/{id}/direct-link` | Create team direct-download token |
| `POST` | `/teams/{team_id}/files/{id}/preview-link` | Create team preview link |
| `POST` | `/teams/{team_id}/files/{id}/resource-handle` | Resolve the team file resource contract |
| `POST` | `/teams/{team_id}/files/{id}/wopi/open` | Create WOPI launch session for team file |
| `GET` | `/teams/{team_id}/files/{id}/download` | Download team file |
| `GET` | `/teams/{team_id}/files/{id}/thumbnail` | Get team file thumbnail |
| `GET` | `/teams/{team_id}/files/{id}/image-preview` | Get team file WebP preview |
| `GET` | `/teams/{team_id}/files/{id}/media-metadata` | Get team file media metadata |
| `PUT` | `/teams/{team_id}/files/{id}/content` | Overwrite team file content |
| `POST` | `/teams/{team_id}/files/{id}/extract` | Create archive extraction task |
| `PATCH` | `/teams/{team_id}/files/{id}` | Rename or move a team file |
| `DELETE` | `/teams/{team_id}/files/{id}` | Soft-delete a team file |
| `POST` | `/teams/{team_id}/files/{id}/lock` | Lock / unlock a team file |
| `POST` | `/teams/{team_id}/files/{id}/copy` | Copy a team file |
| `GET` | `/teams/{team_id}/files/{id}/versions` | List team file versions |
| `POST` | `/teams/{team_id}/files/{id}/versions/{version_id}/restore` | Restore a team file version |
| `DELETE` | `/teams/{team_id}/files/{id}/versions/{version_id}` | Delete a team file version |

Request bodies, paging parameters, upload modes, locks, and versions match personal-space APIs. See [Files](./files.md) and [Folders](./folders.md).

## Batch, Search, Tags, Sharing, Trash, Tasks, and WebDAV

| Method | Path | Description |
| --- | --- | --- |
| `POST` | `/teams/{team_id}/batch/delete` | Batch-delete team resources |
| `POST` | `/teams/{team_id}/batch/move` | Batch-move team resources |
| `POST` | `/teams/{team_id}/batch/copy` | Batch-copy team resources |
| `POST` | `/teams/{team_id}/batch/archive-compress` | Create team archive-compress task |
| `POST` | `/teams/{team_id}/batch/archive-download` | Create team ZIP download ticket |
| `GET` | `/teams/{team_id}/batch/archive-download/{token}` | Download team ZIP |
| `GET` | `/teams/{team_id}/search` | Search team workspace |
| `GET` | `/teams/{team_id}/tags` | List team tags |
| `POST` | `/teams/{team_id}/tags` | Create a team tag |
| `PATCH` | `/teams/{team_id}/tags/{tag_id}` | Rename or recolor a team tag |
| `DELETE` | `/teams/{team_id}/tags/{tag_id}` | Delete a team tag |
| `GET` | `/teams/{team_id}/tags/{entity_type}/{entity_id}` | List tags for a team file or folder |
| `PUT` | `/teams/{team_id}/tags/{entity_type}/{entity_id}` | Replace all tags for a team file or folder |
| `PUT` | `/teams/{team_id}/tags/{tag_id}/{entity_type}/{entity_id}` | Attach a tag to a team file or folder |
| `DELETE` | `/teams/{team_id}/tags/{tag_id}/{entity_type}/{entity_id}` | Detach a tag from a team file or folder |
| `PUT` | `/teams/{team_id}/tags/{tag_id}/batch` | Attach one tag to multiple team files/folders |
| `DELETE` | `/teams/{team_id}/tags/{tag_id}/batch` | Detach one tag from multiple team files/folders |
| `POST` | `/teams/{team_id}/shares` | Create team file/folder share |
| `GET` | `/teams/{team_id}/shares` | List current user's shares in this team |
| `PATCH` | `/teams/{team_id}/shares/{id}` | Edit team share |
| `DELETE` | `/teams/{team_id}/shares/{id}` | Delete team share |
| `POST` | `/teams/{team_id}/shares/batch-delete` | Batch-delete team shares |
| `GET` | `/teams/{team_id}/trash` | List team trash |
| `POST` | `/teams/{team_id}/trash/{entity_type}/{id}/restore` | Restore team trash item |
| `DELETE` | `/teams/{team_id}/trash/{entity_type}/{id}` | Permanently delete team trash item |
| `DELETE` | `/teams/{team_id}/trash` | Empty team trash |
| `GET` | `/teams/{team_id}/tasks` | List team tasks |
| `POST` | `/teams/{team_id}/tasks/offline-download` | Create a team offline-download import task |
| `GET` | `/teams/{team_id}/tasks/{id}` | Read one team task |
| `POST` | `/teams/{team_id}/tasks/{id}/retry` | Retry failed team task |
| `GET` | `/teams/{team_id}/webdav-accounts` | List team WebDAV accounts |
| `POST` | `/teams/{team_id}/webdav-accounts` | Create a team WebDAV account |
| `DELETE` | `/teams/{team_id}/webdav-accounts/{account_id}` | Delete a team WebDAV account |
| `POST` | `/teams/{team_id}/webdav-accounts/{account_id}/toggle` | Enable or disable a team WebDAV account |

These reuse personal-space contracts. See [Batch](./batch.md), [Search](./search.md), [Tags](./tags.md), [Sharing](./shares.md), [Trash](./trash.md), [Tasks](./tasks.md), [WebDAV](./webdav.md), and [WOPI](./wopi.md).

Team-specific notes:

- public REST access for team shares still uses `/api/v1/s/{token}`, and the frontend page is still `/s/:token`
- file writes prefer folder-level `policy_id`; without folder override, the effective storage policy is resolved through `teams.policy_group_id`
- team WOPI startup uses `/teams/{team_id}/files/{id}/wopi/open`, but callbacks still go to `/api/v1/wopi/files/{id}` because team scope lives in the access token
- team archive-download tickets can only be consumed under the matching team route
- recoverable team upload sessions return the same shape as personal sessions but are scoped to the team
- `frontend_client_id` works for team upload init and recoverable-session filtering
- `POST /teams/{team_id}/tasks/offline-download` creates an `offline_download` task that imports an HTTP/HTTPS URL into the team space; its request body matches personal `/tasks/offline-download`
- team WebDAV accounts authenticate through the same WebDAV mount entry, but their storage scope is the team; ordinary members can manage only accounts they created, while team `owner` / `admin` users can manage all accounts in the team
- archive extraction and compression create background tasks rather than blocking synchronously
- `target_folder_id = null` defaults to source-folder or common-parent behavior just like personal space

`POST /teams/{team_id}/files/{id}/resource-handle` reuses the personal file purpose, delivery-mode, representation, and response contract. Its `identity.scope` is `team`, and its `request.url` uses the team-scoped path. See [Files resource handle](./files.md#post-filesidresource-handle) for the complete contract.
