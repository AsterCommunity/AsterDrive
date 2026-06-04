# Admin API

The following paths are relative to `/api/v1` and require administrator privileges.

This page keeps the most important admin endpoint groups. For usage-oriented admin-console guidance, see [Admin Console](../../../docs/guide/admin-console.md).

Most admin list endpoints already use offset pagination:

- `/admin/policies`
- `/admin/policy-groups`
- `/admin/remote-nodes`
- `/admin/users`
- `/admin/teams`
- `/admin/teams/{id}/members`
- `/admin/shares`
- `/admin/tasks`
- `/admin/files`
- `/admin/file-blobs`
- `/admin/config`
- `/admin/locks`
- `/admin/audit-logs`

Default ordering varies by DTO. Common defaults:

- users, teams, policies, policy groups, remote nodes, shares, audit logs: `created_at desc`
- background tasks: `updated_at desc`
- locks: `id asc`
- team members: `role asc`

## Storage policies

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/admin/policies` | List storage policies |
| `POST` | `/admin/policies` | Create storage policy |
| `GET` | `/admin/policies/{id}` | Read policy details |
| `GET` | `/admin/policies/{id}/capacity` | Read policy capacity observation |
| `PATCH` | `/admin/policies/{id}` | Update policy |
| `DELETE` | `/admin/policies/{id}` | Delete policy |
| `POST` | `/admin/policies/{id}/test` | Test saved policy |
| `POST` | `/admin/policies/test` | Test connection with draft parameters |

Create example:

```json
{
  "name": "archive-s3",
  "driver_type": "s3",
  "endpoint": "https://s3.example.com",
  "bucket": "archive",
  "access_key": "AKIA...",
  "secret_key": "...",
  "base_path": "asterdrive/",
  "max_file_size": 10737418240,
  "chunk_size": 10485760,
  "is_default": false
}
```

Current notes:

- create and update both honor request `chunk_size`
- `options` carries policy-level behavior:
  - S3 / Remote upload and download strategies
  - local `content_dedup`
  - S3 connect / read / operation timeouts
  - storage-native thumbnails, only when a driver explicitly exposes that capability
- legacy `{"presigned_upload":true}` remains compatible with S3 presigned upload
- `allowed_types` can be managed through REST
- `driver_type = "remote"` requires `remote_node_id`
- `PATCH` cannot change `driver_type`
- `GET /admin/policies` supports `limit`, `offset`, `sort_by`, `sort_order`
- `GET /admin/policies/{id}/capacity` returns `StoragePolicyCapacityInfo`; local can return real filesystem capacity, S3 is explicitly unsupported, and remote forwards follower capacity status
- `DELETE /admin/policies/{id}?force=true` only cleans upload sessions that still reference the policy. Existing blobs or policy-group references still block deletion. If temp objects or multipart uploads need delayed cleanup, a `storage_policy_temp_cleanup` task is created.

## Storage migrations

Admins can create and resume cross-policy blob migration tasks.

| Method | Path | Description |
| --- | --- | --- |
| `POST` | `/admin/storage-migrations` | Create storage-policy migration task |
| `POST` | `/admin/storage-migrations/dry-run` | Preflight migration plan without creating a task |
| `POST` | `/admin/storage-migrations/{task_id}/resume` | Resume existing migration task |

Create request:

```json
{
  "source_policy_id": 1,
  "target_policy_id": 2,
  "delete_source_after_success": false
}
```

Rules:

- source and target policy IDs must be positive and different
- `delete_source_after_success = true` is currently rejected
- `dry-run` checks target stream-upload support and performs a write/delete probe
- capacity checks use the bytes still expected to be copied, not the whole source policy size
- `insufficient` capacity blocks task creation; `unsupported` and `unavailable` become warnings
- opaque key conflicts are counted and resolved by writing migrated blobs under new `migration-...` keys
- task kind is `BackgroundTaskKind::StoragePolicyMigration`
- migration tasks have independent checkpoints and resume support
- content SHA-256 blobs can be merged across policies only when hash is 64-hex and size also matches
- opaque blobs are never merged across policies

## Remote nodes

Remote nodes are follower storage nodes managed by the primary, mainly for `driver_type = "remote"` policies.

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/admin/remote-nodes` | Paginated managed follower nodes |
| `POST` | `/admin/remote-nodes` | Create remote-node record |
| `GET` | `/admin/remote-nodes/{id}` | Read remote-node details |
| `PATCH` | `/admin/remote-nodes/{id}` | Update name, base URL, transport mode, or enabled state |
| `DELETE` | `/admin/remote-nodes/{id}` | Delete remote node; rejected while referenced by policies |
| `POST` | `/admin/remote-nodes/{id}/test` | Test saved remote-node connection |
| `POST` | `/admin/remote-nodes/test` | Test draft remote-node connection |
| `POST` | `/admin/remote-nodes/{id}/enrollment-token` | Generate follower enrollment command |
| `GET` | `/admin/remote-nodes/{id}/ingress-profiles` | List follower managed ingress profiles |
| `POST` | `/admin/remote-nodes/{id}/ingress-profiles` | Create follower ingress profile |
| `PATCH` | `/admin/remote-nodes/{id}/ingress-profiles/{profile_key}` | Update follower ingress profile |
| `DELETE` | `/admin/remote-nodes/{id}/ingress-profiles/{profile_key}` | Delete follower ingress profile |

Create example:

```json
{
  "name": "edge-sh-01",
  "base_url": "",
  "transport_mode": "auto",
  "is_enabled": true
}
```

Notes:

- `transport_mode` supports `direct`, `reverse_tunnel`, and `auto`
- `direct` requires a primary-reachable `base_url`
- `reverse_tunnel` requires the follower to actively connect back to `/api/v1/internal/remote-tunnel/*`
- `auto` uses direct when `base_url` is non-empty and reverse tunnel otherwise
- empty `base_url` usually means the enrollment flow will complete binding later
- remote-node details include `transport_mode`, `enrollment_status`, `last_error`, `capabilities`, `last_checked_at`, and `tunnel`
- reverse tunnel cannot be combined with remote browser presigned upload / download strategies
- ingress profile request bodies match the follower internal storage protocol; see [Internal storage protocol](./internal-storage.md)

## External authentication providers

External auth providers are configured by admins. Anonymous login reads only enabled public summaries. Supported provider kinds are `oidc` and `generic_oauth2`.

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/admin/external-auth/provider-kinds` | List supported provider kinds |
| `GET` | `/admin/external-auth/providers` | Paginated providers |
| `POST` | `/admin/external-auth/providers` | Create provider |
| `POST` | `/admin/external-auth/providers/test` | Test draft provider config |
| `GET` | `/admin/external-auth/providers/{id}` | Read provider details |
| `PATCH` | `/admin/external-auth/providers/{id}` | Update provider |
| `DELETE` | `/admin/external-auth/providers/{id}` | Delete provider |
| `POST` | `/admin/external-auth/providers/{id}/test` | Test saved provider |

OIDC example:

```json
{
  "provider_kind": "oidc",
  "display_name": "Corp SSO",
  "icon_url": "/static/external-auth/corp.svg",
  "issuer_url": "https://idp.example.com",
  "client_id": "asterdrive",
  "client_secret": "secret",
  "scopes": "openid email profile",
  "enabled": true,
  "auto_provision_enabled": true,
  "auto_link_verified_email_enabled": true,
  "require_email_verified": true,
  "allowed_domains": ["example.com"]
}
```

Generic OAuth2 example:

```json
{
  "provider_kind": "generic_oauth2",
  "display_name": "Logto",
  "icon_url": "/static/external-auth/oauth-logo.svg",
  "issuer_url": "https://id.example.com",
  "authorization_url": "https://id.example.com/oidc/auth",
  "token_url": "https://id.example.com/oidc/token",
  "userinfo_url": "https://id.example.com/oidc/me",
  "client_id": "asterdrive",
  "client_secret": "secret",
  "scopes": "openid email profile",
  "enabled": true,
  "auto_provision_enabled": false,
  "auto_link_verified_email_enabled": false,
  "require_email_verified": true
}
```

Implementation notes:

- provider `key` is generated by the server and used in `/auth/external-auth/{kind}/{provider}/start`
- URLs must be HTTPS except localhost; fragments are not allowed
- `oidc` supports discovery; `generic_oauth2` uses manual authorization / token / userinfo endpoints
- provider capabilities and field requirements come from `GET /admin/external-auth/provider-kinds`
- `client_secret` is redacted as `***REDACTED***` when reading details
- auto-provisioning can create local users, with optional email-domain restrictions
- verified-email auto-linking can bind external identities to existing local users
- when `require_email_verified` is enabled, unverified external emails go through `/auth/external-auth/email-verification/*`
- create, update, delete, and test write admin audit logs

## File and blob management

These endpoints are admin-side observability and maintenance surfaces, not business file APIs.

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/admin/files` | Inspect file records, owning blobs, and version summaries |
| `GET` | `/admin/files/{id}` | Inspect one file and its versions |
| `GET` | `/admin/file-blobs` | Inspect blob records, hash kinds, and reference counts |
| `GET` | `/admin/file-blobs/{id}` | Inspect one blob's file and version references |
| `POST` | `/admin/file-blobs/maintenance` | Create maintenance task for selected blobs |

Filters:

- `/admin/files`: `name`, `blob_id`, `policy_id`, `owner_user_id`, `team_id`, `deleted`, `limit`, `offset`, `sort_by`, `sort_order`
- `/admin/file-blobs`: `hash`, `policy_id`, `storage_path`, `ref_count_min`, `ref_count_max`, `size_min`, `size_max`, `limit`, `offset`, `sort_by`, `sort_order`

`hash_kind` is derived for observability: 64-hex SHA-256 is `content_sha256`, everything else is `opaque`.

Maintenance request:

```json
{
  "action": "ref_count_reconcile",
  "blob_ids": [1, 2, 3]
}
```

Supported actions:

- `integrity_check`: check object existence and size only
- `ref_count_reconcile`: recompute and fix `ref_count`
- `orphan_cleanup`: recompute references, then clean blobs whose actual references and `ref_count` are both zero

## Policy groups

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/admin/policy-groups` | List storage policy groups |
| `POST` | `/admin/policy-groups` | Create policy group |
| `GET` | `/admin/policy-groups/{id}` | Read policy group details |
| `PATCH` | `/admin/policy-groups/{id}` | Update policy group |
| `DELETE` | `/admin/policy-groups/{id}` | Delete policy group |
| `POST` | `/admin/policy-groups/{id}/migrate-assignments` | Migrate user and team policy group bindings by updating `policy_group_id` |

Policy groups define storage policy selection for users and teams. They are rejected from deletion while still referenced. Migration responses report `affected_users`, `affected_teams`, and `migrated_assignments`.

## Users

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/admin/users` | Paginated users |
| `POST` | `/admin/users` | Create user |
| `GET` | `/admin/users/{id}` | Read user details |
| `PATCH` | `/admin/users/{id}` | Update user profile, role, status, quota, or policy group |
| `DELETE` | `/admin/users/{id}` | Delete user |
| `GET` | `/admin/users/{id}/avatar/{size}` | Read uploaded user avatar |

User lists support keyword / role / status style filtering and offset pagination. Avatar responses are raw binary and are not wrapped JSON.

## Teams

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/admin/teams` | Paginated teams |
| `POST` | `/admin/teams` | Create team and assign initial admin |
| `GET` | `/admin/teams/{id}` | Read team details |
| `PATCH` | `/admin/teams/{id}` | Update team |
| `DELETE` | `/admin/teams/{id}` | Archive / delete according to current semantics |
| `GET` | `/admin/teams/{id}/members` | Paginated team members |
| `POST` | `/admin/teams/{id}/members` | Add team member |
| `PATCH` | `/admin/teams/{id}/members/{member_user_id}` | Update member role |
| `DELETE` | `/admin/teams/{id}/members/{member_user_id}` | Remove team member |

Admin team creation can create a team for another user and give that user the initial team-admin role. User-side `POST /teams` is more restrictive.

## Shares, tasks, config, locks, and audit

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/admin/shares` | Paginated share audit / management list |
| `DELETE` | `/admin/shares/{id}` | Delete share |
| `GET` | `/admin/tasks` | Paginated all background tasks |
| `GET` | `/admin/tasks/{id}` | Read any background task |
| `POST` | `/admin/tasks/{id}/retry` | Retry failed task when allowed |
| `GET` | `/admin/config` | List runtime config entries |
| `GET` | `/admin/config/{key}` | Read one runtime config entry |
| `PATCH` | `/admin/config/{key}` | Update runtime config entry |
| `DELETE` | `/admin/config/{key}` | Delete custom runtime config entry |
| `GET` | `/admin/locks` | Paginated lock list |
| `DELETE` | `/admin/locks/{id}` | Release a lock |
| `GET` | `/admin/audit-logs` | Paginated audit logs |

Runtime config entries defined by the system cannot be deleted; custom entries can. The single source of truth for system config definitions is `src/config/definitions.rs`.

Custom runtime config entries also have a `visibility` field:

| Visibility | Behavior |
| --- | --- |
| `private` | Admin-only; never returned by `/api/v1/public/custom-config` |
| `public` | Readable without login |
| `authenticated` | Returned only when the request carries a valid access token |

The field only applies to `source = "custom"` entries. Built-in system configuration cannot be made public through it. When omitted, new custom entries default to `private`.

`GET /admin/config` now includes `visibility` in addition to `id`, `key`, `value`, `source`, `namespace`, `updated_at`, and `updated_by`. Sensitive values are still redacted as `***REDACTED***`.

The frontend custom-configuration read path is documented in [Public API](./public.md) under `GET /public/custom-config`. That endpoint only returns the key/value map visible to the current request identity and does not expose admin-only fields.

Admin task APIs can see system tasks and blob-level cache tasks that ordinary users normally cannot see.

Link import is controlled by the `offline_download_*` runtime config keys. `offline_download_engine_registry_json` is the current structured engine registry: it contains ordered `builtin` and `aria2` engine entries with `enabled` flags. Enabled engines are tried in registry order; if all engines are disabled, link import is disabled. The older `offline_download_engine` single-value key remains as a compatibility fallback when the registry is absent or invalid.
`offline_download_temp_dir` is the staging root for link-import tasks. When blank, AsterDrive uses the default server temp directory. When set, it must be the same absolute path visible to both AsterDrive and any external downloader such as aria2.

File size, per-task speed, concurrency, and request timeout apply to all engines. When the `aria2` registry entry is enabled, the aria2 RPC URL, RPC secret, RPC timeout, split, per-server connection count, and low-speed limit keys configure the administrator-managed aria2 JSON-RPC daemon. AsterDrive does not pass through arbitrary aria2 options, and the per-task speed limit maps to aria2 `max-download-limit`, not a daemon-wide limit. Admins can execute `test_aria2_rpc` against `offline_download_engine_registry_json`; the server probes aria2 with `aria2.getVersion`. Config actions may include unsaved form drafts in `value` and `draft_values`, so the aria2 probe can test the current registry, RPC URL, secret, and timeout before saving. Wrong RPC secrets return `error.code = "offline_download.aria2_rpc_auth_failed"`; other probe failures return `error.code = "offline_download.aria2_rpc_probe_failed"` instead of storage-driver error codes. Operational setup, temporary-directory semantics, and troubleshooting live in [Offline Download](../../../docs/en/config/offline-download.md).

When aria2 is enabled, AsterDrive still validates the HTTP/HTTPS source URL before dispatching the task, but the aria2 daemon performs its own DNS resolution and outbound connection. Operators should isolate the daemon at the network layer and restrict the JSON-RPC endpoint to AsterDrive.

## Operational notes

- Admin endpoints are intentionally grouped around operational ownership: storage, users, teams, shares, tasks, config, locks, audit, and observability.
- Many admin actions write audit logs.
- For storage behavior involving remote followers, read this page together with [Internal storage protocol](./internal-storage.md).
- For user-facing file semantics, read [Files](./files.md), [Folders](./folders.md), and [Teams](./teams.md) instead of treating admin observability endpoints as business APIs.
