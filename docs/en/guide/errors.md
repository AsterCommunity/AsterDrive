# Error Code Handling

This page helps you understand AsterDrive errors: which field to read first, what a user can retry, and where an administrator should investigate.

Regular users do not need to memorize every code. In most cases, send the visible error, the time, and what you were doing to an administrator.
If you are writing scripts, integrating with the API, or troubleshooting WebDAV / WOPI / remote nodes, read the top-level `code` first.

::: tip 0.3.0 Error Code Contract
Issue 211 has converged the public error contract to one source: the top-level `code: ApiErrorCode`.

The old numeric error code, `error.code`, `error.subcode`, and `error.internal_code` are no longer exposed as public API response fields. Client copy and business logic should use the top-level `code` directly.
:::

## Which Field To Read

A failed AsterDrive response usually looks like this:

```json
{
  "code": "auth.credentials_failed",
  "msg": "Invalid Credentials",
  "error": {
    "retryable": false
  }
}
```

Field meanings:

| Field | How to use it |
| --- | --- |
| `code` | **The stable string error code**. Frontends, SDKs, scripts, and third-party clients should use this for business logic. Successful responses use `success`. |
| `error.retryable` | Whether automatic retry is suggested. `true` does not guarantee success; it only means the failure is likely temporary. |
| `msg` | A human-readable diagnostic message. Do not branch on it and do not use it as an i18n key. |

When reporting an issue or asking an administrator for help, include:

1. `code`
2. `msg`
3. Time and operation that produced the error

Only pasting an English message makes the issue harder to locate.

## String Codes Use Domains

`code` uses stable snake/dot values. The part before the dot is usually the domain, and the part after the dot is the reason:

| Domain | Common codes |
| --- | --- |
| Common | `bad_request`, `not_found`, `internal_server_error`, `conflict` |
| Login, sessions, permissions, MFA | `auth.credentials_failed`, `auth.token_expired`, `forbidden`, `auth.mfa_failed` |
| Files, uploads, thumbnails, locks | `file.not_found`, `upload.session_expired`, `thumbnail.failed`, `resource.locked` |
| Storage policies, drivers, quotas, remote storage | `storage.quota_exceeded`, `storage.auth_failed`, `storage.transient_failure` |
| Folders | `folder.not_found` |
| Shares | `share.expired`, `share.password_required` |

When you see a new error, use the code domain for direction, then use the complete string for exact handling.

## Common Paths

### Login, Sessions, And Accounts

If login fails, start here:

- `auth.credentials_failed` / `auth.failed`: username, email, password, or credential is wrong. Re-enter it; if it is correct, ask an administrator to check account status.
- `auth.pending_activation`: the account is not activated. Verify the email or ask an administrator to activate it manually.
- `auth.contact_verification_invalid` / `auth.contact_verification_expired`: the verification link is invalid or expired. Send a new verification email.
- `auth.token_missing` / `auth.token_invalid` / `auth.token_expired`: login state is missing, invalid, or expired. Refresh the page; if it repeats, clear cookies and log in again.
- `auth.refresh_token_stale`: refresh token is too old, often caused by multiple devices, an old page, or an old session. Log in again.
- `auth.refresh_token_reuse_detected`: refresh token replay was detected. The session is rejected; revoke other sessions and log in again.
- `auth.password_change_required`: an administrator requires this account to change its password first. After login, finish the forced password-change flow; normal APIs are denied until it succeeds.
- `auth.account_disabled`: the account is disabled. Regular users must contact an administrator.
- `auth.registration_disabled`: public registration is disabled. Ask an administrator to create an account or enable registration.

When password login, passkeys, external auth, and MFA overlap, still trust `code` first instead of a generic "login failed" message.

### MFA And Passkeys

MFA errors usually happen during second-factor verification, authenticator setup, email-code verification, or recovery-code flows:

- `auth.mfa_failed`: generic MFA failure. Check whether the same response contains a more specific code.
- `auth.mfa_flow_invalid` / `auth.mfa_flow_expired`: the verification flow is invalid or expired. Return to login and start again.
- `auth.mfa_code_invalid`: authenticator code or recovery code is wrong. Check authenticator time, or use an unused recovery code.
- `auth.mfa_attempts_exceeded`: too many attempts. Restart the login flow.
- `auth.mfa_factor_required`: the account requires MFA, but the factor state is incomplete. Ask an administrator to reset MFA.
- `auth.mfa_factor_already_exists`: the account already has this factor type.
- `auth.mfa_recovery_code_used`: the recovery code has already been used. Regenerate recovery codes after login.
- `auth.mfa_email_code_required` / `auth.mfa_email_code_expired`: send an email code first, or send a new one because it expired.

Passkey errors:

- `passkey.name_invalid` / `passkey.name_too_long`: passkey name is invalid or too long. Rename and retry.
- `passkey.not_discoverable`: the browser or security key did not create a discoverable credential. Use a supported device / browser, or add the passkey again.

Use HTTPS in production. Many WebAuthn / passkey behaviors do not work as expected on insecure origins.

### Permissions, Teams, And Workspaces

`code = "forbidden"` only means "not allowed". When an operation needs a more specific reason, the response returns a more specific string code:

- `auth.admin_required`: administrator permission required.
- `team.not_member`: current account is not a member of the team.
- `team.owner_required`: team owner permission required.
- `team.admin_or_owner_required`: team administrator or owner permission required.
- `workspace.scope_denied`: the resource is outside the current workspace.
- `share.scope_denied`: the share scope does not allow access to this resource.
- `lock.not_owner`: current user is not the lock holder or resource owner.
- `external_auth.provider_disabled` / `external_auth.policy_denied`: external-auth provider is disabled, or policy denies this operation.

Regular users should check whether they are in the correct team and workspace.
Administrators should check team membership, roles, share scope, and external-auth policies.

### CSRF, Request Source, And Reverse Proxies

When cookie-authenticated admin, WOPI, or WebDAV-related requests fail, common codes are:

- `auth.request_source_missing`: request is missing source information such as `Origin` / `Referer`.
- `auth.request_source_untrusted`: request source is untrusted.
- `auth.request_origin_untrusted` / `auth.request_referer_untrusted`: `Origin` or `Referer` is not trusted.
- `auth.csrf_cookie_missing` / `auth.csrf_header_missing` / `auth.csrf_token_invalid`: CSRF cookie, request header, or token is wrong.
- `validation.request_origin_invalid` / `validation.request_referer_invalid`: source header format is invalid.
- `validation.request_host_invalid` / `validation.request_scheme_invalid`: Host or scheme from the reverse proxy is wrong.
- `validation.request_header_value_invalid`: source-related header is too long or cannot be parsed.

Check in this order:

1. Refresh the page and log in again.
2. Confirm the public site URL in system settings matches the actual access URL.
3. Confirm the reverse proxy forwards `Host`, `X-Forwarded-Host`, and `X-Forwarded-Proto` correctly.
4. For cross-origin follower uploads, WOPI, or custom scripts, confirm origin and CORS settings match.

### Files, Folders, And Conflicts

File / folder codes:

- `file.not_found` / `folder.not_found`: the file or folder does not exist, was deleted, moved to trash, or permanently cleaned.
- `file.name_conflict` / `folder.name_conflict`: another item with the same name exists in the directory. Rename or refresh and retry.
- `file.etag_mismatch` / `file.modified_during_write` / `precondition_failed`: another client changed the file before your submission. Refresh and edit again.
- `resource.locked`: the resource is held by a WebDAV LOCK. Wait for the lock to expire, or have an administrator unlock it.
- `file.too_large`: file exceeds policy limit. Ask an administrator to adjust policy or policy group.
- `file.type_not_allowed`: policy forbids this file type.

If the database still references an object but the storage backend cannot find the actual content, you may see `storage.object_not_found` or `storage.not_found`. This is not a normal user-fixable 404; administrators should check underlying storage and backup-restore history.

### Uploads And Large Files

For upload failures, first identify the stage:

- `upload.session_not_found` / `upload.session_expired`: upload session does not exist or expired. Start the upload again.
- `upload.assembling`: file is still being assembled on the server. This is not a failure; wait a few seconds and check again.
- `upload.chunk_failed`: one chunk failed, usually related to network, disk space, or temporary directories.
- `upload.assembly_failed`: server-side assembly failed. Administrators should inspect tasks and server logs.
- `upload.status_conflict` / `upload.previous_failure`: upload session state has changed; usually do not reuse the old session.
- `upload.incomplete_chunks` / `upload.incomplete_parts` / `upload.missing_part`: some chunks or S3 parts are missing.
- `upload.chunk_number_out_of_range` / `upload.part_number_out_of_range` / `upload.part_numbers_too_many`: submitted chunk / part numbers are invalid.
- `upload.chunk_size_mismatch` / `upload.request_size_mismatch` / `upload.final_object_size_mismatch`: declared size and actual size differ.
- `upload.temp_dir_create_failed`, `upload.temp_file_write_failed`, `upload.local_staging_write_failed`, `upload.assembly_io_failed`: server temporary directory, staging area, or disk write failed.

Users can retry once. If it repeats, administrators should check:

- Whether `data/.uploads`, `data/.tmp`, or custom temp directories are on a full partition
- Whether user / team / policy quota is exhausted
- Whether the object-storage multipart or remote presigned browser-direct URL is reachable
- Whether the remote follower is healthy and has an applied default ingress target

### Storage Policies, S3, And Remote Nodes

Storage errors are usually not solved by clicking retry repeatedly. Administrators should handle them by `code`:

- `storage.policy_not_found`: user, team, or policy group references a missing policy.
- `storage.quota_exceeded`: user, team, or system quota is exhausted.
- `storage.unsupported_driver` / `storage.operation_unsupported` / `storage.unsupported`: driver or remote node does not support this operation.
- `storage.auth_failed` / `storage.auth`: S3 / remote credential or binding authentication failed.
- `storage.permission_denied` / `storage.permission`: credentials are valid but cannot read or write the object / prefix.
- `storage.misconfigured`: policy is incomplete or inconsistent, or the remote follower is not ready.
- `storage.rate_limited`: object storage, gateway, or follower rate-limited the request.
- `storage.transient_failure` / `storage.transient`: temporary network, object storage, or follower failure. Retry later.
- `storage.precondition_failed` / `storage.precondition`: conditional write, remote ingress state, or concurrent operation conflict.
- `storage.driver_error` / `storage.unknown`: driver returned an unclassified error. Check server logs.

Remote-node codes:

- `remote_node.disabled`: remote node is disabled.
- `remote_node.enrollment_required`: follower has not completed enrollment.
- `remote_node.unique_conflict`: remote-node binding or unique field conflict.
- `managed_ingress.required`, `managed_ingress.default_missing`, `managed_ingress.default_not_applied`: follower has no usable default ingress target.
- `managed_ingress.local_path_invalid`: follower local ingress path is invalid, commonly because it escapes the allowed root.
- `managed_ingress.driver_unsupported`: current ingress target driver is unsupported.
- `managed_ingress.single_primary_required`: this follower must be bound to only one primary.
- `master_binding.disabled`: master / follower binding is disabled.

If a remote policy uses browser-direct upload, also confirm browsers can reach the follower `base_url` and the follower CORS policy allows required upload headers.

### Shares

Share errors:

- `share.not_found`: share does not exist, token is wrong, share was deleted, or source file was deleted.
- `share.expired`: share expired. Ask the creator to generate a new one.
- `share.password_required`: share password must be verified first, or the verification cookie was lost / expired.
- `share.download_limit_reached`: download limit has been reached.
- `share.scope_denied`: share scope does not allow access to the target file or folder.

Note: share password verification is cached. After changing a share password, already verified visitors may keep access until the cache expires.

### Thumbnails, Avatars, And Archive Preview

These errors usually do not mean the original file is gone. They mean a processing pipeline failed.

Thumbnails:

- `thumbnail.failed`: generic thumbnail failure.
- `thumbnail.source_too_large`: source file exceeds thumbnail processing limit.
- `thumbnail.processor_unavailable`: media processor is unavailable; check `vips` / `ffmpeg` or the relevant config.
- `thumbnail.format_guess_failed` / `thumbnail.decode_failed` / `thumbnail.encode_failed`: format detection, decoding, or encoding failed.
- `thumbnail.source_open_failed` / `thumbnail.source_stream_failed`: reading the source failed, possibly involving underlying storage.
- `thumbnail.task_panicked`: thumbnail task panicked; administrators should check tasks and logs.

Avatars:

- `avatar.file_required`: no avatar file was submitted.
- `avatar.upload_read_failed`: reading uploaded avatar failed.
- `avatar.processor_unavailable`: avatar processor is unavailable.
- `avatar.empty_image`: image is empty or has no valid dimensions.
- `avatar.render_failed` / `avatar.output_invalid`: rendering or output failed.

Archive preview:

- `archive_preview.disabled`: global archive preview switch is off.
- `archive_preview.user_disabled` / `archive_preview.share_disabled`: user-space or share-page archive preview is disabled.
- `archive_preview.unsupported_type`: unsupported archive type.
- `archive_preview.source_too_large`: source archive is too large.
- `archive_preview.invalid_archive`: archive is damaged or invalid.
- `archive_preview.manifest_too_large`: generated listing exceeds the limit.
- `archive_preview.source_size_mismatch`: source size differs from the record during scanning.
- `archive_preview.rejected`: background task rejected execution, usually due to permission, file state, or runtime-condition changes.

If first open only shows "generating", wait for the archive preview task in Task Center to finish, then open again.

### Background Tasks

Background tasks can return stable error codes too:

- `task.lease_lost`: task lease was lost, usually because another worker took over or the task was reclaimed.
- `task.lease_renewal_timed_out`: task lease renewal timed out, possibly due to database, worker, or system load.
- `task.worker_shutdown_requested`: worker received shutdown request, commonly during service restart or task dispatcher shutdown.

Administrators should inspect `Admin -> Tasks` for status, failure reason, retry count, and same-time logs.

### Mail, External Auth, And Offline Download

Mail:

- `mail.not_configured`: SMTP is not configured. Administrators configure it under `Admin -> System Settings -> Mail Delivery`.
- `mail.delivery_failed`: SMTP delivery failed. Send a test mail first, then inspect SMTP response and mail outbox logs.

External auth:

- `external_auth.provider_disabled`: provider is disabled.
- `external_auth.policy_denied`: policy does not allow the external login, link, or account creation.

Offline download:

- `offline_download.aria2_rpc_auth_failed`: aria2 RPC secret is wrong.
- `offline_download.aria2_rpc_probe_failed`: aria2 probe failed. Check RPC URL, network, timeout, and aria2 process status.

### WOPI And WebDAV

WOPI:

- `wopi.public_site_url_required`: public site URL is required for WOPI.
- `wopi.app_disabled`: target WOPI app is disabled.
- `wopi.request_origin_untrusted` / `wopi.request_referer_untrusted`: WOPI request source is untrusted.
- `wopi.max_expected_size_exceeded`: file exceeds the WOPI app or AsterDrive edit-size limit.

WebDAV:

- `webdav.username_exists`: WebDAV username already exists.
- `resource.locked`: WebDAV LOCK conflict, corresponding to HTTP `423`.
- `precondition_failed`: conditional request failed, corresponding to HTTP `412`.

Many WebDAV clients show only HTTP status, not JSON errors:

| HTTP Status | Common meaning |
| --- | --- |
| `401` | Authentication failed. Use a dedicated WebDAV account, not the normal login password. |
| `403` | Account is valid but lacks permission; system-file blocking may also reject `.DS_Store`, `Thumbs.db`, or `desktop.ini`. |
| `404` | Path does not exist. |
| `412` | Precondition failed, usually `precondition_failed`. |
| `423` | Resource is locked, usually `resource.locked`. |
| `503` | WebDAV global switch is off. |

## Stable String Code Reference

The following table groups current public `ApiErrorCode` values by handling path. `*` is only used here to group related codes for readability; clients should still match complete strings and should not infer behavior from prefixes alone.

### Common And Runtime

| Code | Meaning |
| --- | --- |
| `success` | Success response, not an error. |
| `bad_request` | Request parameters or body are invalid. |
| `not_found` | Resource does not exist. |
| `endpoint.not_found` | API route does not exist. |
| `internal_server_error` | Internal server error. |
| `database.error` | Database connection or operation failed. |
| `config.error` | Static or runtime configuration error. |
| `rate_limited` | Request was rate-limited. |
| `conflict` | Resource conflict; often accompanied by a more specific conflict code. |
| `mail.not_configured` / `mail.delivery_failed` | Mail is not configured or delivery failed. |

### Auth, Security, And Accounts

| Code | Meaning |
| --- | --- |
| `auth.failed` / `auth.credentials_failed` | Authentication failed or credentials are wrong. |
| `auth.token_missing` / `auth.token_invalid` / `auth.token_expired` | Token is missing, invalid, or expired. |
| `auth.refresh_token_stale` / `auth.refresh_token_reuse_detected` | Refresh token is stale or replay was detected. |
| `auth.password_change_required` | The account must change its password before continuing. |
| `auth.pending_activation` | Account is pending activation. |
| `auth.contact_verification_invalid` / `auth.contact_verification_expired` | Contact verification is invalid or expired. |
| `forbidden` | Permission denied, usually with a more specific permission code. |
| `auth.admin_required` / `auth.account_disabled` | Admin permission required, or account is disabled. |
| `auth.username_exists` / `auth.email_exists` / `auth.identifier_exists` | Login identifier conflict. |
| `auth.registration_disabled` | Public registration is disabled. |
| `auth.session_user_mismatch` | Session and account do not match. |
| `auth.request_source_missing` / `auth.request_source_untrusted` | Request source is missing or untrusted. |
| `auth.request_origin_untrusted` / `auth.request_referer_untrusted` | Origin or Referer is untrusted. |
| `auth.csrf_cookie_missing` / `auth.csrf_header_missing` / `auth.csrf_token_invalid` | CSRF validation failed. |
| `auth.mfa_failed`, `auth.mfa_*` | MFA flow, code, recovery code, or factor state error. |
| `passkey.name_invalid` / `passkey.name_too_long` / `passkey.not_discoverable` | Passkey name or credential capability issue. |
| `external_auth.provider_disabled` / `external_auth.policy_denied` | External-auth provider or policy denied the operation. |

### Files, Uploads, Folders, And Locks

| Code | Meaning |
| --- | --- |
| `file.not_found` / `folder.not_found` | File or folder does not exist. |
| `file.too_large` / `file.type_not_allowed` | File exceeds size limit or type is forbidden. |
| `file.upload_failed` | Generic file upload failure. |
| `file.name_conflict` / `folder.name_conflict` | File or folder name conflict. |
| `file.etag_mismatch` / `file.modified_during_write` | File changed before write; precondition failed. |
| `resource.locked` / `lock.not_owner` | Resource is locked, or current user is not the lock owner. |
| `precondition_failed` | Conditional request failed. |
| `upload.session_not_found` / `upload.session_expired` | Upload session does not exist or expired. |
| `upload.chunk_failed` / `upload.assembly_failed` | Chunk upload or assembly failed. |
| `upload.assembling` | File is still being assembled. |
| `upload.temp_*` / `upload.local_staging_*` / `upload.assembly_io_failed` | Server temp directory, staging, or assembly I/O failed. |
| `upload.request_*` / `upload.body_size_overflow` / `upload.declared_size_invalid` | Request body, size, or declared value is invalid. |
| `upload.chunk_*` / `upload.part_*` | Chunk / part number, size, count, or transport mode is invalid. |
| `upload.incomplete_*` / `upload.missing_part` | Chunks or parts are incomplete. |
| `upload.temp_object_*` / `upload.final_object_size_mismatch` | Temporary object or final object size mismatch. |
| `upload.status_conflict` / `upload.previous_failure` / `upload.session_corrupted` | Upload session state conflict, previous failure, or corruption. |

### Storage, Remote Nodes, And Ingress

| Code | Meaning |
| --- | --- |
| `storage.policy_not_found` | Storage policy does not exist. |
| `storage.driver_error` / `storage.unknown` | Storage driver returned an unknown error. |
| `storage.quota_exceeded` | Quota exceeded. |
| `storage.unsupported_driver` / `storage.unsupported` | Driver or capability is unsupported. |
| `storage.auth_failed` / `storage.auth` | Storage authentication failed. |
| `storage.permission_denied` / `storage.permission` | Storage permission denied. |
| `storage.misconfigured` | Storage configuration is wrong. |
| `storage.object_not_found` / `storage.not_found` | Storage object does not exist. |
| `storage.rate_limited` | Storage backend rate-limited the request. |
| `storage.transient_failure` / `storage.transient` | Temporary storage failure. |
| `storage.precondition_failed` / `storage.precondition` | Storage precondition failed. |
| `storage.operation_unsupported` | Storage operation is unsupported. |
| `remote_node.disabled` / `remote_node.enrollment_required` / `remote_node.unique_conflict` | Remote node is disabled, not enrolled, or conflicts with a unique field. |
| `managed_ingress.*` | Follower ingress target is missing, not applied, invalid, unsupported, or inconsistent. |
| `master_binding.disabled` | Master / follower binding is disabled. |

### Shares, Teams, And Workspaces

| Code | Meaning |
| --- | --- |
| `share.not_found` / `share.expired` | Share does not exist or expired. |
| `share.password_required` | Share password verification is required. |
| `share.download_limit_reached` | Share download limit reached. |
| `share.scope_denied` | Share scope denies access to the target resource. |
| `team.not_member` | Current account is not a team member. |
| `team.owner_required` / `team.admin_or_owner_required` | Team owner or administrator permission required. |
| `team.member_exists` | Team member already exists. |
| `workspace.scope_denied` | Current workspace cannot access target resource. |
| `policy.*` | Storage policy precondition, configuration, remote-node binding, or driver-promotion validation failed. |

### Preview, Media Processing, WOPI, WebDAV, And Others

| Code | Meaning |
| --- | --- |
| `thumbnail.failed` / `thumbnail.*` | Thumbnail generation, read, decode, encode, temp-file, or processor failure. |
| `avatar.*` | Avatar upload, read, processing, rendering, or output failure. |
| `archive_preview.*` | Archive preview switch, format, size, manifest, or task rejection issue. |
| `wopi.public_site_url_required` | WOPI requires a public site URL. |
| `wopi.app_disabled` | WOPI app is disabled. |
| `wopi.request_origin_untrusted` / `wopi.request_referer_untrusted` | WOPI request source is untrusted. |
| `wopi.max_expected_size_exceeded` | File exceeds WOPI size limit. |
| `webdav.username_exists` | WebDAV username already exists. |
| `offline_download.aria2_rpc_auth_failed` / `offline_download.aria2_rpc_probe_failed` | aria2 RPC authentication or probe failed. |
| `task.lease_lost` / `task.lease_renewal_timed_out` / `task.worker_shutdown_requested` | Background task lease, renewal, or worker shutdown issue. |
| `validation.*` | Request source, Host, scheme, header, or initialization-state validation failed. |

## Still Not Solved

Regular users can report the problem like this:

```text
At 2026-06-03 21:10, uploading test.zip failed.
code: upload.assembly_failed
msg: ...
```

Administrators can continue with:

1. Check server logs for the same time range.
2. If the problem involves uploads, storage, thumbnails, archive preview, or offline downloads, inspect `Admin -> Tasks`.
3. If storage metadata may not match underlying objects, run `doctor` from [Operations CLI](/en/deployment/ops-cli).
4. If it is likely an AsterDrive bug, open a [GitHub Issue](https://github.com/AptS-1547/AsterDrive/issues) with reproduction steps and the error response.

Error codes are the fastest entry point for diagnosis. Paste `code` first, not only the UI message.
