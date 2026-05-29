# Error Code Handling

This page covers error codes that regular users and administrators may see in the frontend, API, and WebDAV clients: what they mean, what you can do yourself, and when to contact an administrator.  
Jump to the error code or UI message you saw; you do not need to read it all.

For 5xx-level server errors such as `internal_server_error`, `database_error`, or `config_error`, follow [Troubleshooting](/en/deployment/troubleshooting).

## Error Code Ranges

Backend error codes are grouped by thousands:

| Range | Purpose |
| --- | --- |
| `0` | Success |
| `1xxx` | Common / server |
| `2xxx` | Authentication and sessions |
| `3xxx` | Files / uploads / downloads |
| `4xxx` | Storage policies |
| `5xxx` | Folders |
| `6xxx` | Shares |

Once you know the range, a new error code roughly tells you which category of problem it belongs to.

Many errors also include a more specific `error.code`. This is the new stable string error code. The frontend, scripts, and third-party clients should check it first instead of parsing the English `msg`. `msg` is only suitable as a fallback explanation or troubleshooting clue.

Before 0.3.0, some responses also include `error.subcode`. It is only a transition field for older clients. New scripts, integrations, and frontend logic should use `error.code`. When troubleshooting or opening an issue, record `error.code` first, then the numeric top-level `code`.

---

## Common (1xxx)

### `bad_request` (1000)

Request parameters are invalid.

For regular users, this usually means a form field is wrong, such as an invalid character in a name or an invalid date format. Check the form message and correct it.

If you are sure the parameters are correct, the frontend and backend versions may be inconsistent. Force-refresh the page.

Request-source validation formatting problems include more specific suberrors:

- `validation.request_origin_invalid`: invalid `Origin` request header format
- `validation.request_referer_invalid`: invalid `Referer` request header format
- `validation.request_host_invalid`: invalid request Host, commonly caused by wrong reverse proxy forwarded-header configuration
- `validation.request_scheme_invalid`: invalid request scheme, commonly caused by an HTTPS reverse proxy not passing `X-Forwarded-Proto` correctly
- `validation.request_header_value_invalid`: request-source-related header is too long or cannot be processed

### `not_found` (1001) / `endpoint_not_found` (1005)

The requested resource or endpoint does not exist.

- `not_found`: the concrete object you accessed, such as a user or configuration item, does not exist
- `endpoint_not_found`: the URL route itself does not exist; usually you manually changed the URL or the frontend version does not match

### `internal_server_error` (1002) / `database_error` (1003) / `config_error` (1004)

Server-side exception.

Regular users: retry later once. If it keeps failing, send the error code and approximate time to an administrator.  
Administrators: read [Troubleshooting](/en/deployment/troubleshooting).

### `rate_limited` (1006)

Requests are too frequent and were blocked by rate limiting.

- Regular users: wait a few seconds before trying again
- Administrators: if legitimate users hit this often, adjust `[rate_limit]` or upstream reverse proxy rate limits in [Rate Limiting](/en/config/rate-limit)

### `mail_not_configured` (1007)

The mail system is not configured, so activation emails, password reset emails, and similar messages cannot be sent.

If you are a regular user, ask an administrator to configure SMTP. If you are an administrator, fill SMTP information in `Admin -> System Settings -> Mail Delivery` and send a test email to verify.

### `mail_delivery_failed` (1008)

Mail delivery failed.

First send a test email from `Admin -> System Settings -> Mail Delivery`, then check server logs for SMTP / mail outbox errors. Common causes:

- Wrong SMTP configuration, such as port, TLS mode, or authentication
- Recipient domain rejects mail
- The server has been blocked by the recipient mail provider

### `conflict` (1009)

Resource conflict. Most commonly, the thing you are creating or changing already exists:

- Duplicate username, email, or login identifier
- File / folder with the same name already exists in the directory
- Team member already exists
- WebDAV username is already occupied
- Remote node binding hit a uniqueness conflict

Regular users should follow the page prompt, choose another name, or refresh and retry.
Administrators encountering this during batch import, script calls, or remote-node enrollment should first read `error.code`; it is more specific than `conflict` itself. Older transition responses may also include `error.subcode`, but that field is only a compatibility reference.

---

## Authentication (2xxx)

### `auth_failed` (2000)

Username or password is wrong.

If you are sure the password is correct, the account may be locked by an administrator. In that case the error code is usually `forbidden`.

### `token_expired` (2001) / `token_invalid` (2002)

Login has expired or is invalid.

Normally the frontend refreshes automatically and continues without interruption. If it repeats, common causes are:

- Browser disabled cookies or third-party cookies
- You or an administrator manually revoked the session ("revoke all sessions" under `Admin -> Users`)
- Server clock is badly skewed

Clear browser cookies and log in again.

### `forbidden` (2003)

You do not have permission to perform this operation.

- Regular user operating admin features: switch to an account with permission
- Administrator disabled: contact another administrator
- Operating another user's resource: check share permissions or team membership permissions

The same `forbidden` error uses `error.code` to explain the concrete reason. Before 0.3.0, the same value may also appear in the compatibility field `error.subcode`:

- `auth.admin_required`: administrator permission required
- `auth.account_disabled`: account has been disabled
- `auth.request_source_untrusted` / `auth.request_origin_untrusted` / `auth.request_referer_untrusted`: Cookie-authenticated request source is untrusted, usually related to cross-site requests, reverse proxy site-address configuration, or browser origin
- `auth.request_source_missing`: request that requires source validation is missing source information such as `Origin` / `Referer`
- `auth.csrf_cookie_missing` / `auth.csrf_header_missing` / `auth.csrf_token_invalid`: missing CSRF Cookie, missing `X-CSRF-Token` request header, or token validation failed; refresh the page and retry
- `auth.registration_disabled`: public registration is disabled
- `auth.session_user_mismatch`: current session and current account do not match; log in again
- `team.not_member`: current account is not a member of the team
- `team.owner_required`: team owner permission required
- `team.admin_or_owner_required`: team administrator or owner permission required
- `workspace.scope_denied`: resource does not belong to the current workspace
- `share.scope_denied`: share scope does not allow access to this resource
- `lock.not_owner`: current user is not the lock holder or resource owner
- `external_auth.provider_disabled` / `external_auth.policy_denied`: external authentication provider is disabled, or policy does not allow the current operation
- `wopi.app_disabled` / `wopi.request_origin_untrusted` / `wopi.request_referer_untrusted`: WOPI app is disabled, or WOPI request source is untrusted

### `pending_activation` (2004)

The account is not activated yet and needs email verification first.

Check the activation email received during registration. If it is missing:

- Check spam
- Use the `Resend verification email` entry on the login page or under `Settings -> Security`
- If system mail is not configured, ask an administrator to activate manually

### `contact_verification_invalid` (2005) / `contact_verification_expired` (2006)

The email verification link is invalid or expired.

Links are time-limited, 24 hours by default. Request a new one. If `invalid` repeats, check whether the mail client truncated the link, which is common with enterprise mail systems.

### Passkey-Related Suberrors

Passkey problems usually still belong to main error codes such as `bad_request`, `auth_failed`, or `token_invalid`. Check `error.code` for accuracy:

- `passkey.name_invalid`: passkey name contains control characters; choose a normal name
- `passkey.name_too_long`: passkey name is too long; shorten it and retry
- `passkey.not_discoverable`: browser or security key did not create a discoverable credential; use a device/browser that supports passkeys, or add the passkey again

If the login page says the current browser does not support passkeys, the browser, system, or current access origin usually does not meet WebAuthn requirements. HTTPS is recommended for production deployments.

### MFA-Related Suberrors

MFA problems usually happen during login second-factor verification, sending email codes, authenticator setup, MFA disable, or recovery-code regeneration:

- `auth.mfa_flow_invalid`: MFA login or setup flow is invalid; return to the login page or restart setup
- `auth.mfa_flow_expired`: flow expired, about 5 minutes by default; start again
- `auth.mfa_code_invalid`: authenticator code or recovery code is incorrect; check whether authenticator time is synced, or use an unused recovery code
- `auth.mfa_attempts_exceeded`: too many attempts; restart the login flow
- `auth.mfa_factor_required`: account requires an enabled MFA factor but current state is incomplete; contact an administrator to reset MFA
- `auth.mfa_factor_already_exists`: this account already has TOTP enabled; cannot add the same factor again
- `auth.mfa_recovery_code_used`: recovery code has already been used; regenerate recovery codes after login
- `auth.mfa_email_code_required`: email-code verification was selected, but no code has been sent yet; send one first
- `auth.mfa_email_code_expired`: the email code has expired; send a new one

Email-code MFA also depends on mail delivery and a verified email address. If the login page does not show an email-code option, the usual causes are that the administrator has not enabled it, the account email is not verified, or the site does not allow TOTP users to fall back to email.

If both authenticator and recovery codes are lost and there is no usable email-code path, regular users cannot bypass MFA themselves. Contact an administrator to reset MFA from `Admin -> Users -> User Details -> Security Actions`.

### External Authentication Problems

External authentication failures usually show "external login failed" on the login page. Backend main error codes may be `auth_failed`, `forbidden`, `bad_request`, or `mail_delivery_failed`.

Administrators should check in this order:

1. Whether `Admin -> System Settings -> Site Configuration -> Public site URL` is correct
2. Whether the provider is enabled in `Admin -> External Auth`
3. Whether the redirect URI has been registered with the identity provider
4. Whether Issuer URL, Client ID, Client Secret, scope, and claim mapping are correct
5. If email verification is involved, whether `Admin -> System Settings -> Mail Delivery` can send external-login email verification messages

---

## Files / Uploads (3xxx)

### `file_not_found` (3000)

The file does not exist or has been deleted.

- You or someone else just deleted it
- The file was moved to trash -> find it in left-side `Trash`
- The file was permanently cleaned -> it is gone; see [Backup and Restore](../deployment/backup)

### `file_too_large` (3001)

The file exceeds the maximum size allowed by the policy.

Policy-level limits are set by administrators in `Admin -> Storage Policies`. If you are a legitimate user, ask an administrator to adjust the limit or switch policy groups.

### `file_type_not_allowed` (3002)

The file type is forbidden by the policy.

Same as above, this is controlled by policy. Common restricted extensions include executables, usually for security.

### `file_upload_failed` (3003)

Generic upload failure.

Check in this order:

1. Whether the network was interrupted
2. Whether the browser console has a more specific error
3. Whether server logs around that time contain a more precise `error_code`

### `upload_session_not_found` (3004) / `upload_session_expired` (3005)

The chunked upload session does not exist or has expired.

Most resumable upload sessions are valid for 24 hours. S3 / follower-node single presigned direct-upload sessions are usually valid for 1 hour. Old sessions remain valid after service restart because they are persisted in the database; the server-returned `expires_at` is authoritative. If this appears:

- Your upload exceeded session lifetime -> start again
- You reloaded the page -> the frontend restores the session from localStorage; if the session was cleaned on the server, this error appears
- You are trying to resume from another device -> unsupported; continuation must happen in the same browser and same localStorage

Start the upload again.

### `chunk_upload_failed` (3006)

A single chunk in a chunked upload failed.

Most common causes:

- Disk full (administrator checks the partition containing `data/.uploads`)
- User quota is full
- Default policy or bound policy group is disabled

### `upload_assembly_failed` (3007)

Server-side chunk assembly failed.

This usually means uploaded chunk data is incomplete or hash verification failed. Re-upload. If it repeats, an intermediate network component may be corrupting transfer.

### `thumbnail_failed` (3008)

Thumbnail generation failed.

This does not affect the file itself. Common causes:

- File is damaged
- File type is not in the thumbnail support list
- Thumbnail worker failed (administrators check `Admin -> Tasks`)
- Media processor is not enabled, or `vips` / `ffmpeg` command is unavailable

### `resource_locked` (3009)

The resource is held by a WebDAV LOCK.

- Wait for the lock to expire; locks have a timeout by default
- Manually unlock in `Admin -> Locks` with administrator permission
- Ask the occupying client to exit normally; ideally the client sends UNLOCK

### `precondition_failed` (3010)

A precondition is not satisfied, often from multiple clients editing the same file at the same time.

Refresh the page to get the latest version, then submit again.

If the error message contains `managed_ingress.*`, the follower node's ingress target is usually not ready. Administrators should check this follower in `Admin -> Follower Nodes`:

- Whether it has a default ingress target
- Whether the default ingress target has been applied
- Whether the local ingress path escapes `server.follower.managed_ingress_local_root`
- Whether this follower is bound to only one primary

### `upload_assembling` (3011)

The file is still being assembled from chunks on the server. **This is not an error**.

Wait a few seconds and retry complete. Large files take longer to assemble because the server merges chunks and calculates SHA256. Do not retry repeatedly right away.

### Archive Preview Suberrors

archive preview errors usually hang under `bad_request` or `forbidden`. Check `error.code`:

- `archive_preview.disabled`: global archive preview switch is not enabled
- `archive_preview.user_disabled`: archive preview for logged-in users is not enabled
- `archive_preview.share_disabled`: archive preview for share pages is not enabled
- `archive_preview.unsupported_type`: current file is not a supported archive format
- `archive_preview.source_too_large`: source archive exceeds preview size limit
- `archive_preview.invalid_archive`: archive is damaged or invalid
- `archive_preview.manifest_too_large`: generated listing exceeds manifest size limit
- `archive_preview.source_size_mismatch`: source file size differs from the record during scanning; usually re-upload or check underlying storage
- `archive_preview.rejected`: background task refused to run, probably because the file changed, permission changed, or runtime limits are no longer satisfied

If the first open only shows "generating", that is not an error. Wait for `archive preview generation` in `Admin -> Tasks` / `Task Center` to finish, then open again.  
If the UI says the current filename encoding cannot parse this ZIP, switch `Filename encoding` in the archive preview toolbar and retry. This kind of prompt may not have a separate backend error code.

---

## Storage Policies (4xxx)

### `storage_policy_not_found` (4000)

The policy does not exist.

Usually a policy bound to a user was deleted, or a policy group rule references a deleted policy. Administrators should check `Admin -> Storage Policies` / `Policy Groups`.

### `storage_driver_error` (4001)

The storage backend returned an error.

Check by driver type:

- `local`: check directory permissions and disk space
- `s3`: check endpoint, credentials, and whether the bucket exists; if the S3 side is slow or down, AsterDrive's configured timeout may trigger
- `remote`: check whether the bound remote node is enabled, whether `base_url` is reachable, and whether the follower has completed enrollment and is healthy; also confirm the follower has an applied default ingress target
- Other: read the concrete error

If the remote policy uses `presigned`, also check whether the remote node capability summary supports internal protocol `v2` and `browser_presigned_cors`. When the browser connects directly to the follower, CORS must allow `content-type` / `range` and expose response headers such as `ETag`, `Accept-Ranges`, `Content-Range`, and `Content-Length`.

### `storage_quota_exceeded` (4002)

Storage space is insufficient.

- User quota full -> clean trash, delete large files, or ask an administrator to increase quota
- Team space full -> same, handled by a team administrator
- System total quota full -> administrator increases policy capacity

### `unsupported_driver` (4003)

The driver type configured by the policy is not supported in the current version.

This usually happens after downgrading from a higher version, or after manually editing the DB incorrectly. Re-select a supported driver in `Admin -> Storage Policies`.

### `storage_auth_failed` (4004)

Storage backend authentication failed.

- S3 / MinIO: check Access Key, Secret Key, session token, signature version, and endpoint
- remote: check whether the remote node binding is still valid, and whether binding information on the primary and follower was deleted
- Local driver usually does not return this unless the upper layer configured the storage type incorrectly

This kind of error is not solved by user retry. Administrators must fix credentials first.

### `storage_permission_denied` (4005)

Credentials are valid, but they do not have permission for the current operation.

Common causes:

- S3 credentials are read-only and cannot write
- Bucket policy does not allow access to the current prefix
- Local directory permissions do not allow the current process to write
- Remote follower ingress path or internal API permission is wrong

Fix storage backend permissions first, then retry upload.

### `storage_misconfigured` (4006)

The storage policy configuration itself is incomplete or inconsistent.

Check:

- endpoint, bucket, region, base path
- Whether the local storage root exists and is in the expected place
- Whether the remote follower completed enroll and has an applied default ingress target
- Whether the remote follower internal protocol version and capability summary are compatible with the current primary; currently `v2` is required
- Whether the remote ingress path escapes the follower's allowed root directory

This is usually a deployment or policy configuration problem, not a browser problem.

### `storage_object_not_found` (4007)

The database still references an object, but the storage backend cannot find the actual content.

This is more serious than a regular 404: it means metadata and real storage are inconsistent. Administrators should first run:

```bash
./aster_drive doctor \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --deep \
  --scope storage-objects
```

If you recently performed manual migration, backup restore, bucket cleanup, or local directory moves, start from those operations.

### `storage_rate_limited` (4008)

The storage backend rate-limited AsterDrive.

This commonly happens when object storage receives too many requests, an S3-compatible service is under heavy load, or a gateway behind the follower is rate-limiting. Waiting may recover it. If it repeats, administrators should check storage provider or follower logs.

### `storage_transient_failure` (4009)

Temporary storage backend failure.

Typical causes are network jitter, temporary object storage outage, or follower connection interruption. Users can retry later. Administrators should check network, S3 / MinIO, or follower logs for the same time period.

### `storage_precondition_failed` (4010)

A storage backend precondition was not satisfied.

Common cases include object-storage conditional-write conflicts, remote ingress state changes, and concurrent operations on the same object. Refresh and retry first. If it keeps appearing, administrators should check the corresponding storage policy and remote node status.

### `storage_operation_unsupported` (4011)

The current storage driver does not support this operation.

For example, some backends cannot generate presigned URLs, cannot use a certain streaming read/write path, or the remote follower version does not match the capability expected by the primary. Administrators need to change policy configuration, upgrade nodes, or disable the upload / download mode that depends on this capability.

---

## Folders (5xxx)

### `folder_not_found` (5000)

The folder does not exist or has been deleted.

Same as `file_not_found`: deleted, moved to trash, or permanently cleaned.

---

## Shares (6xxx)

### `share_not_found` (6000)

The share link does not exist or has become invalid.

Most likely causes:

1. Token was mistyped or truncated by chat software
2. The creator deleted the share
3. The source file for the share was deleted

### `share_expired` (6001)

The share has passed `expires_at`.

Ask the share creator to generate a new one. The new link has a new token.

### `share_password_required` (6002)

The share requires a password, but the current request has no valid share password verification cookie.

This usually means verification has not happened, the cookie is lost, or verification has expired. When actually submitting a password, if the password is wrong, the server handles it as authentication failure; the common error code is `auth_failed`.

Note: the server has a 1-hour password verification cache. **After changing the password, the other side may still access with the old password for up to 1 hour**. This is intentional design, not a bug.

### `share_download_limit_reached` (6003)

The share has reached its download limit.

- The creator can increase the download count limit in left-side `My Shares`
- Or recreate the share

---

## Messages Outside the Ranges Above

### Frontend `unexpected_error`

Frontend fallback message, not a backend error code. It means the frontend received a response it does not recognize.

Common causes:

- Backend returned a new error code not mapped by the frontend, usually from version mismatch after upgrade
- Network layer failed, and the frontend did not even receive `error_code`

Force-refresh the page once. If it remains, check the concrete browser console error.

### WebDAV Client Shows Strange HTTP Status Codes

WebDAV clients usually do not show `error_code`; they only show HTTP status codes:

- `401`: authentication failed; use a dedicated WebDAV account, not the normal login account
- `403`: account is valid but has no permission to access the path; it may also mean the administrator enabled WebDAV system-file blocking and the client is trying to create metadata files such as `.DS_Store`, `Thumbs.db`, or `desktop.ini`
- `404`: path does not exist
- `423`: resource is locked; corresponds to `resource_locked`
- `412`: precondition failed; corresponds to `precondition_failed`
- `503`: WebDAV global switch is off; administrator enables it in `Admin -> System Settings -> WebDAV`

---

## Still Not Solved

Submit the issue in this order:

1. Record the error code, both number and string name
2. Record the steps that produced the error
3. Run `aster_drive doctor` once as administrator
4. Open an issue in [GitHub Issues](https://github.com/AptS-1547/AsterDrive/issues)

`error.code` and the numeric `code` are the fastest clues for locating the problem. When reporting an issue, paste the error code first. It is more useful than pasting an English error message.
