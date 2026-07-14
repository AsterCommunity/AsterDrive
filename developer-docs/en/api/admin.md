# Admin API

The following paths are relative to `/api/v1`. Every endpoint on this page requires administrator privileges except the storage OAuth provider callback `/admin/policies/storage-authorization/callback`.

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

## Overview

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/admin/overview` | Read aggregated admin dashboard data |

The overview response includes user, file, blob, share, audit, and task summaries plus recent activity. Query parameters include `days`, `timezone`, and `event_limit`.

## Storage policies

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/admin/policies` | List storage policies |
| `POST` | `/admin/policies` | Create storage policy |
| `GET` | `/admin/policies/{id}` | Read policy details |
| `GET` | `/admin/policies/{id}/capacity` | Read policy capacity observation |
| `PATCH` | `/admin/policies/{id}` | Update policy |
| `DELETE` | `/admin/policies/{id}` | Delete policy |
| `GET` | `/admin/policies/storage-drivers` | List storage connector descriptors |
| `GET` | `/admin/policies/storage-credential-providers` | List storage OAuth credential providers |
| `POST` | `/admin/policies/{id}/test` | Test saved policy |
| `POST` | `/admin/policies/{id}/action` | Execute a storage action for a saved policy |
| `POST` | `/admin/policies/{id}/promote-s3-driver` | Promote a generic S3-compatible policy to a supported specialized driver |
| `POST` | `/admin/policies/{id}/storage-authorization/start` | Start storage OAuth authorization for a policy |
| `GET` | `/admin/policies/{id}/storage-credentials` | List stored OAuth credentials for a policy |
| `POST` | `/admin/policies/{id}/storage-credentials/{provider}/validate` | Validate a stored OAuth credential |
| `GET` | `/admin/policies/storage-authorization/callback` | Storage OAuth provider callback entry; does not require an admin JWT and redirects back to the admin UI |
| `POST` | `/admin/policies/test` | Test connection with draft parameters |
| `POST` | `/admin/policies/action` | Execute a storage action with draft policy parameters |

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

- `driver_type` currently supports `local`, `s3`, `sftp`, `azure_blob`, `tencent_cos`, `remote`, and `one_drive`
- `GET /admin/policies/storage-drivers` returns `StorageConnectorDescriptor` entries. The frontend should use descriptor `capabilities`, `fields`, `upload_workflows`, `actions`, and `credential_mode` to decide forms, connection tests, upload/download strategies, and action affordances instead of maintaining a hard-coded driver capability matrix.
- create and update both honor request `chunk_size`
- `options` carries policy-level behavior:
  - S3-compatible / Azure Blob / Tencent COS object-storage connectors use `object_storage_upload_strategy` / `object_storage_download_strategy` for transfer strategy. Legacy `s3_upload_strategy` / `s3_download_strategy` JSON remains accepted as a compatibility alias.
  - Remote upload and download strategies through `remote_upload_strategy` / `remote_download_strategy`
  - local `content_dedup`
  - generic S3 path-style addressing through `s3_path_style` (defaults to `true`)
  - S3 connect / read / operation timeouts
  - storage-native thumbnails / image previews with `storage_native_processing_enabled`, `thumbnail_processor`, and `thumbnail_extensions`
  - storage-native media metadata with `storage_native_media_metadata_enabled` and `media_metadata_extensions`
  - OneDrive location options: `onedrive_account_mode`, `onedrive_tenant`, `onedrive_site_id`, `onedrive_drive_id`, `onedrive_group_id`, and `onedrive_root_item_id`
  - SFTP host key pinning: `sftp_host_key_fingerprint`
- `application_config.microsoft_graph` stores OneDrive / Microsoft Graph app settings. Client secrets are stored encrypted; API responses expose only `client_secret_configured`.
- `driver_type = "azure_blob"` uses Azure Block Blob capabilities. Presigned browser upload uses SAS URLs and requires the client to send `x-ms-blob-type: BlockBlob`.
- `driver_type = "one_drive"` uses Microsoft Graph OAuth credentials. Save the policy and `application_config.microsoft_graph` before starting authorization.
- `driver_type = "sftp"` uses SSH username / password credentials to connect to an SFTP server. Endpoint supports `sftp://host:port`, bare `host`, and `host:port`; the remote root belongs in `base_path`. Unknown or mismatched SSH host keys are rejected as `StorageErrorKind::Precondition` with diagnostics that include actual / expected fingerprints; the confirmed fingerprint is stored in `options.sftp_host_key_fingerprint`.
- `driver_type = "tencent_cos"` uses the S3-compatible object path for normal reads and writes, validates Tencent COS endpoint shape, and can expose COS CI storage-native thumbnail / image-preview / media-metadata capabilities when the policy opts in
- built-in Local, S3-compatible, SFTP, Azure Blob, OneDrive, and Remote drivers do not expose storage-native thumbnail, image-preview, or media-metadata capabilities
- legacy `{"presigned_upload":true}` remains compatible with object-storage presigned upload
- `allowed_types` can be managed through REST
- `driver_type = "remote"` requires `remote_node_id`
- `PATCH` cannot change `driver_type`
- `POST /admin/policies/{id}/promote-s3-driver` currently supports promoting a generic `s3` policy to `tencent_cos`. The body must include the target driver and current endpoint / bucket, for example `{ "target_driver_type": "tencent_cos", "endpoint": "https://bucket-1250000000.cos.ap-guangzhou.myqcloud.com", "bucket": "bucket-1250000000" }`. Promotion is rejected unless the bucket stays unchanged, there are no active upload sessions for the policy, and the target driver validates the endpoint / bucket combination.
- `GET /admin/policies` supports `limit`, `offset`, `sort_by`, `sort_order`
- `GET /admin/policies/{id}/capacity` returns `StoragePolicyCapacityInfo`; local returns filesystem capacity, S3-compatible and Azure Blob are explicitly unsupported, OneDrive reads Microsoft Graph drive quota, and remote forwards follower capacity status
- `DELETE /admin/policies/{id}?force=true` only cleans upload sessions that still reference the policy. Existing blobs or policy-group references still block deletion. If temp objects or multipart uploads need delayed cleanup, a `storage_policy_temp_cleanup` task is created.

### Storage connection tests

`POST /admin/policies/{id}/test` and `POST /admin/policies/test` return an ordinary empty success response on success:

```json
{
  "code": "success",
  "msg": "",
  "data": {}
}
```

Failed connection tests no longer return a `StoragePolicyProbeResult` success payload. They use the standard error response and expose redacted diagnostics through `error.diagnostic`:

```json
{
  "code": "storage.auth_failed",
  "msg": "storage authentication failed",
  "error": {
    "retryable": false,
    "diagnostic": {
      "kind": "auth",
      "message": "credentials were rejected by the storage provider"
    }
  }
}
```

Draft test requests support optional `policy_id`. While editing a saved policy, blank sensitive fields such as `access_key` or `secret_key` can be filled from the saved policy by S3-compatible, SFTP, Azure Blob, and Tencent COS connectors. Unsaved new policies must still provide complete credentials.

### Storage OAuth Credentials

These endpoints currently mainly serve the OneDrive / Microsoft Graph connector:

- `GET /admin/policies/storage-credential-providers` lists providers. `microsoft_graph` is currently supported; `google_drive` is reserved with `supported = false`.
- Creating or updating a OneDrive policy first saves Microsoft Graph app settings through `application_config.microsoft_graph`; client secrets are encrypted at rest.
- `POST /admin/policies/{id}/storage-authorization/start` only needs the provider; the backend reuses saved application config to start authorization.
- After successful authorization, the callback writes `storage_policy_credentials` and redirects to `/admin/policies?storage_authorization=success&policy_id=...`. The callback does not require an admin JWT because Microsoft Graph and similar providers return to it through the browser.
- `GET /admin/policies/{id}/storage-credentials` returns credential status, tenant, account label, scopes, expiry, and refresh timestamps, but never access or refresh tokens.
- `POST /admin/policies/{id}/storage-credentials/{provider}/validate` validates the stored credential and updates status to `authorized`, `reauth_required`, `permission_denied`, or `invalid`.

Start authorization example:

```http
POST /api/v1/admin/policies/12/storage-authorization/start
```

```json
{
  "provider": "microsoft_graph"
}
```

The returned `authorization_url` is meant for browser navigation:

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "authorization_url": "https://login.microsoftonline.com/common/oauth2/v2.0/authorize?...",
    "expires_in": 300,
    "provider": "microsoft_graph",
    "microsoft_graph": {
      "cloud": "global",
      "tenant": "common",
      "client_id": "00000000-0000-0000-0000-000000000000",
      "client_secret_configured": true,
      "scopes": ["offline_access", "Files.ReadWrite.All", "Sites.ReadWrite.All"]
    }
  }
}
```

### Storage policy actions

Storage policy actions are the unified entry point for optional management capabilities exposed by storage drivers. Default built-in capabilities should extend the `StoragePolicyActionType` enum instead of adding provider-specific HTTP routes. Future plugin capabilities may expose their own schema route for parameter discovery, but execution should still stay action-oriented where possible.

Current actions:

| action | Supported driver | Mutates remote state | Description |
| --- | --- | --- | --- |
| `configure_tencent_cos_cors` | `tencent_cos` | yes | Configure Tencent COS bucket CORS from `public_site_url` |

Saved policy request:

```http
POST /api/v1/admin/policies/12/action
```

```json
{
  "action": "configure_tencent_cos_cors"
}
```

Draft policy request:

```http
POST /api/v1/admin/policies/action
```

```json
{
  "action": "configure_tencent_cos_cors",
  "policy_id": 12,
  "driver_type": "tencent_cos",
  "endpoint": "https://bucket-1250000000.cos.ap-guangzhou.myqcloud.com",
  "bucket": "bucket-1250000000",
  "access_key": "AKID...",
  "secret_key": "...",
  "base_path": "prod/"
}
```

`configure_tencent_cos_cors` behavior:

- request bodies do not accept `allowed_origin` or `allowed_origins`
- draft request connection fields are flat fields, not nested under a `policy` object
- `policy_id` is optional and is only used for draft actions while editing a saved policy; if `access_key` or `secret_key` is blank, the backend fills that blank credential field from the saved policy
- without `policy_id`, draft actions must carry complete credentials themselves; this covers unsaved new policies and purely transient parameter tests
- the backend reads all origins from runtime config `public_site_url` and writes them as multiple `AllowedOrigin` entries in one COS CORS rule
- if `public_site_url` is empty, the action returns `policy.action_parameter_required`
- if the policy is not `tencent_cos`, the action returns `policy.action_unsupported`
- AsterDrive uses the stable rule id `asterdrive-presigned-access`
- Tencent COS does not provide an atomic append-CORS-rule API; AsterDrive reads current rules with `GET Bucket cors`, preserves unrelated rules, replaces the rule with the same ID, and writes the full document back with `PUT Bucket cors`
- `PUT Bucket cors` requires `Content-MD5`; the server calculates the MD5 of the XML body and includes it in the COS signature
- successful execution writes admin audit action `admin_trigger_storage_action`; details include `action`, `driver_type`, `used_draft_values`, and `mutates_remote_state`

Success response example:

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "action": "configure_tencent_cos_cors",
    "tencent_cos_cors": {
      "rule_id": "asterdrive-presigned-access",
      "allowed_origins": [
        "https://drive.example.com",
        "https://panel.example.com"
      ],
      "request_id": "NmEy...",
      "preserved_rule_count": 1,
      "replaced_existing_rule": true,
      "response_vary": true
    }
  }
}
```

Common error codes:

| Code | Meaning |
| --- | --- |
| `policy.action_unsupported` | The action does not support this policy or driver type |
| `policy.action_parameter_required` | Required backend configuration is missing, such as an empty `public_site_url` |
| `policy.action_parameter_invalid` | Action parameters or backend-derived parameters are invalid |
| `storage.auth_failed` | COS credentials are wrong or signing failed |
| `storage.permission_denied` / `storage.permission` | COS CAM permissions are insufficient, for example missing `name/cos:PutBucketCORS` |
| `storage.misconfigured` | COS reports a configuration error, such as bad bucket, endpoint, required headers, or XML |
| `storage.transient_failure` / `storage.transient` | COS or network failure that may be retried later |

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
| `GET` | `/admin/remote-nodes/{id}/storage-target-drivers` | List follower remote storage target driver descriptors |
| `GET` | `/admin/remote-nodes/{id}/storage-targets` | List follower remote storage targets |
| `POST` | `/admin/remote-nodes/{id}/storage-targets` | Create follower remote storage target |
| `PATCH` | `/admin/remote-nodes/{id}/storage-targets/{target_key}` | Update follower remote storage target |
| `DELETE` | `/admin/remote-nodes/{id}/storage-targets/{target_key}` | Delete follower remote storage target |

Version `0.4.0` removed the legacy `/ingress-profile-drivers` and `/ingress-profiles` compatibility paths. Clients must use `/storage-target-drivers` and `/storage-targets`; DTO field names use `target_key`.

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
- remote storage target request bodies match the follower internal storage protocol; see [Internal storage protocol](./internal-storage.md)

## External authentication providers

External auth providers are configured by admins. Anonymous login reads only enabled public summaries. Supported provider kinds are `oidc`, `generic_oauth2`, `github`, `qq`, `google`, and `microsoft`.

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

Microsoft example:

```json
{
  "provider_kind": "microsoft",
  "display_name": "Microsoft",
  "icon_url": "/static/external-auth/microsoft-logo.svg",
  "options": {
    "microsoft": {
      "tenant": "organizations"
    }
  },
  "client_id": "00000000-0000-0000-0000-000000000000",
  "client_secret": "secret-value",
  "scopes": "openid profile email",
  "enabled": true,
  "auto_provision_enabled": false,
  "auto_link_verified_email_enabled": false,
  "require_email_verified": false
}
```

Implementation notes:

- provider `key` is generated by the server and used in `/auth/external-auth/{kind}/{provider}/start`
- URLs must be HTTPS except localhost; fragments are not allowed
- `oidc` supports discovery; `generic_oauth2` uses manual authorization / token / userinfo endpoints
- `github`, `qq`, `google`, and `microsoft` are dedicated provider kinds; endpoints and default claim semantics are fixed by backend drivers, so unsupported manual endpoint fields should not be sent
- `microsoft` tenant configuration uses `options.microsoft.tenant`, which accepts `common`, `organizations`, `consumers`, or a concrete tenant UUID; provider details return normalized `options`
- `options` is currently used for provider-specific configuration; `options.microsoft` is rejected for non-Microsoft providers
- provider capabilities and field requirements come from `GET /admin/external-auth/provider-kinds`
- `client_secret` is redacted as `***REDACTED***` when reading details, and `client_secret_configured` reports whether a saved secret exists
- auto-provisioning can create local users, with optional email-domain restrictions
- verified-email auto-linking can bind external identities to existing local users
- when `require_email_verified` is enabled, unverified external emails go through `/auth/external-auth/email-verification/*`
- dedicated provider behavior is documented in [External Authentication Module](../external-auth.md)
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
| `PATCH` | `/admin/users/{id}` | Update user profile, role, status, quota, policy group, or forced password-change flag |
| `PUT` | `/admin/users/{id}/password` | Reset a user's password |
| `DELETE` | `/admin/users/{id}/mfa` | Clear a user's MFA setup and revoke sessions |
| `POST` | `/admin/users/{id}/sessions/revoke` | Revoke all existing sessions for the user |
| `DELETE` | `/admin/users/{id}` | Delete user |
| `GET` | `/admin/users/{id}/avatar/{size}` | Read uploaded user avatar |

User lists support keyword / role / status style filtering and offset pagination. Avatar responses are raw binary and are not wrapped JSON.

`POST /admin/users` accepts an optional `password` and optional `must_change_password`. When `password` is omitted or blank, the server generates a 24-character temporary password, sets `must_change_password = true`, and returns the generated value once in `generated_password`. When a password is provided, `must_change_password` defaults to `false` unless the request explicitly sets it.

`PATCH /admin/users/{id}` accepts `must_change_password: true | false`. Setting it to `true` requires the user to change their password after the next successful login; setting it to `false` clears the requirement before the user completes that flow. Any change to this flag increments `session_version`, deletes existing refresh sessions, invalidates the auth snapshot cache, and records `admin_update_user` audit details including the new `must_change_password` value.

While the flag is set, successful password, MFA, passkey, and external-auth login completions return:

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "status": "password_change_required",
    "expires_in": 900
  }
}
```

The issued access token is scoped to password change only. It can call `GET /auth/me`, `PUT /auth/password`, and `POST /auth/logout`; other authenticated routes return `403` with `auth.password_change_required`. `POST /auth/refresh` is also rejected while the flag or password-change token scope is present. `PUT /auth/password` still requires the current password and clears `must_change_password` after a successful update. The current password is temporary only when an administrator has reset the user's password; if an administrator only sets `must_change_password`, the user's existing password remains the current password.

## Teams

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/admin/teams` | Paginated teams |
| `POST` | `/admin/teams` | Create team and assign initial admin |
| `GET` | `/admin/teams/{id}` | Read team details |
| `PATCH` | `/admin/teams/{id}` | Update team |
| `DELETE` | `/admin/teams/{id}` | Archive a team |
| `POST` | `/admin/teams/{id}/restore` | Restore an archived team |
| `GET` | `/admin/teams/{id}/audit-logs` | Read team audit logs |
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
| `POST` | `/admin/tasks/cleanup` | Delete finished task records matching filters |
| `GET` | `/admin/config` | List runtime config entries |
| `GET` | `/admin/config/schema` | Read system config schema |
| `GET` | `/admin/config/template-variables` | Read template variable catalog |
| `GET` | `/admin/config/{key}` | Read one runtime config entry |
| `PUT` | `/admin/config/{key}` | Set runtime config entry |
| `DELETE` | `/admin/config/{key}` | Delete custom runtime config entry |
| `POST` | `/admin/config/{key}/action` | Execute a config action for supported keys |
| `GET` | `/admin/locks` | Paginated lock list |
| `DELETE` | `/admin/locks/{id}` | Release a lock |
| `DELETE` | `/admin/locks/expired` | Delete expired locks |
| `GET` | `/admin/audit-logs` | Paginated audit logs |

Runtime config entries defined by the system cannot be deleted; custom entries can. The single source of truth for system config definitions is `src/config/definitions.rs`.

Media-derivative limits are regular runtime config entries. `thumbnail_max_source_bytes` bounds which original files are accepted for thumbnail generation, while `thumbnail_max_dimension` and `image_preview_max_dimension` bound the rendered longest edge for list thumbnails and preview-panel images. Changing a dimension creates a dimension-specific derivative cache namespace instead of rewriting another configured size.

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

File size, per-task speed, concurrency, and request timeout apply to all engines. When the `aria2` registry entry is enabled, the aria2 RPC URL, RPC secret, RPC timeout, split, per-server connection count, and low-speed limit keys configure the administrator-managed aria2 JSON-RPC daemon. AsterDrive does not pass through arbitrary aria2 options, and the per-task speed limit maps to aria2 `max-download-limit`, not a daemon-wide limit. Admins can execute `test_aria2_rpc` against `offline_download_engine_registry_json`; the server probes aria2 with `aria2.getVersion`. Config actions may include unsaved form drafts in `value` and `draft_values`, so the aria2 probe can test the current registry, RPC URL, secret, and timeout before saving. Wrong RPC secrets return `code = "offline_download.aria2_rpc_auth_failed"`; other probe failures return `code = "offline_download.aria2_rpc_probe_failed"` instead of storage-driver error codes. Operational setup, temporary-directory semantics, and troubleshooting live in [Offline Download](../../../docs/en/config/offline-download.md).

When aria2 is enabled, AsterDrive still validates the HTTP/HTTPS source URL before dispatching the task, but the aria2 daemon performs its own DNS resolution and outbound connection. Operators should isolate the daemon at the network layer and restrict the JSON-RPC endpoint to AsterDrive.

## Operational notes

- Admin endpoints are intentionally grouped around operational ownership: storage, users, teams, shares, tasks, config, locks, audit, and observability.
- Many admin actions write audit logs.
- For storage behavior involving remote followers, read this page together with [Internal storage protocol](./internal-storage.md).
- For user-facing file semantics, read [Files](./files.md), [Folders](./folders.md), and [Teams](./teams.md) instead of treating admin observability endpoints as business APIs.
