# Troubleshooting

This page covers the most common issues after deployment: service startup failure, upload/download failure, sharing access problems, WebDAV connection failure, Office / WOPI blank screens, background tasks not running, and post-upgrade issues.  
Find the section that matches the symptom you see. You do not need to read the whole page.

If your symptom is outside this page's scope, such as a functional bug or data problem, run `aster_drive doctor` first, then see [Still Not Solved](#still-not-solved).

Before troubleshooting, do two things:

1. Check the responses from `/health` and `/health/ready`; they can directly tell you whether the DB is unreachable or the storage backend is not ready.
2. Check recent logs. The top-level `code` in API responses is the public error code; comparing it against [Error Code Handling](/en/reference/errors) is faster than staring at English error text.

## Service Does Not Start

### Startup Exits Immediately with No Logs

This usually means an error happened during configuration loading, before logging was initialized.

Check in this order:

1. Is the current working directory the one you think it is? `data/config.toml` is resolved from the working directory.
2. Does `data/config.toml` have write permission? First startup needs to write it.
3. Are there typoed keys under the `ASTER__` environment variable prefix? Misspelled keys are not ignored; they make loading fail.
4. Can the database URL be parsed? A wrong SQLite path also falls into this category.

### `/health` 200 but `/health/ready` 503

`/health` only checks whether the process is alive. `/health/ready` actually pings the database and default storage policy.

Distinguish by the `code` in the 503 response:

- `database.error`: DB cannot connect. Check connection string, network, username, and password first.
- `storage.*` related error code: default storage policy cannot start. For S3, check endpoint, credentials, and bucket existence. For local storage, check path permissions.

### Configuration Changes Do Not Take Effect

There are two classes:

- Changes to `data/config.toml` require a process restart.
- Most runtime settings under `Admin -> System Settings` do not require restart. If the page marks an item as "requires restart", restart according to the page hint.
- Rate limiting is not in system settings. It is `[rate_limit]` in `config.toml`, and changes require restart.

If you changed runtime settings but behavior does not move, first check whether the description for that setting says "takes effect after restart".

## Upload Failure

### Direct Small-File Upload Fails, but Chunked Large-File Upload Works

Direct upload uses `web::Bytes` and receives the full body in one shot. Its body limit is affected by both the actix default limit and the reverse proxy.

Check in this order:

1. Reverse proxy (`nginx` / `caddy` / `traefik`) `client_max_body_size` or equivalent configuration.
2. Whether WAF / CDN blocked it.
3. Whether intermediate network devices have their own body limits.

Large files use chunked upload, and each chunk is 5 MiB by default, so chunk body limits are less likely to be hit.

### Chunked Upload Gets Stuck Halfway

First identify the stuck stage:

- Stuck on a specific chunk: usually a network interruption. The frontend retries by itself; check that request's status in browser developer tools.
- All chunks uploaded but complete does not return: the server is merging and calculating SHA256. Large files take time. Do not immediately retry complete; it will hit the dedup path but waste another merge attempt.

### `upload.session_expired`

Most resumable upload sessions last 24 hours. Single presigned direct-upload sessions for S3 / follower nodes are usually 1 hour. Use the frontend recovery list or the server-returned `expires_at` as the source of truth.

If you plan to upload very large files that may cross days, check network speed before starting. Sessions are not automatically renewed after expiration; you must start again.

### `upload.chunk_failed`

The most common cause is a full disk.

Check in this order:

1. Available space on the partition containing `data/.uploads`.
2. Whether the user's quota is already exhausted.
3. Whether the default policy / user-bound policy group is disabled.

### `upload.assembling`

The backend is merging chunks. This is not an error. Wait a few seconds and retry complete. If it repeats and lasts more than 1 minute, check whether the `data/.uploads` temporary directory has abnormalities.

## Download and Preview

### Download Stream Breaks Halfway

Download uses streaming + Range support. If the stream breaks, reconnecting can resume.

If it repeatedly breaks at the same position:

- Check the reverse proxy `proxy_read_timeout` or equivalent configuration. Large downloads need a long read timeout.
- Check whether the link between client and service has an idle timeout, such as CDN or Cloudflare Tunnel.

### Preview Cannot Open / Office File Cannot Open

Separate by preview type:

- Images / videos / code: browser-native preview. Check the browser console for CSP / MIME errors.
- Office files: depends on the preview method you use.
  - If using an external previewer, first check whether binding is correct under `Admin -> System Settings -> Site Configuration -> Preview Applications`.
  - If using WOPI (Collabora / OnlyOffice), confirm the WOPI service can reverse-access AsterDrive's public site origin.
- Archive previews: first check the global switch and user/share-side switches under `Admin -> System Settings -> File Processing -> Archive Preview`, then check whether `Archive Preview Generation` failed under `Admin -> Tasks`.

WOPI integration details are in [File Editing](/en/guide/editing).

## Sharing Access

### Sharing Link 404

Most likely causes, ordered by probability:

1. Token typo or truncation, especially when copied from WeChat / enterprise IM.
2. Share expired, beyond `expires_at`.
3. Share deleted.
4. Share reached the `max_downloads` limit.

### Share Requires Password, but Correct Password Still Fails

First distinguish "not verified yet" from "verification failed":

- `share.password_required`: the current request has no valid share password verification cookie. Usually not verified yet, cookie lost, or verification expired.
- `auth.failed`: submitted password is wrong.
- `share.expired` / `share.not_found`: same as the previous section, not a password issue.

The server caches password verification for 1 hour. After changing the password, **the other party may still access with the old password within 1 hour**. This is intentional behavior, not a bug.

### Sharing Page Audio / Video Plays for a While, then 404 or Expires

Sharing page audio and video use short-lived stream playback sessions, valid for `3` hours by default. This is not the expiration time of the sharing link itself.

If playback fails after a user plays music or video for a long time:

- Refreshing the sharing page creates a new playback session.
- Administrators can adjust it under `Admin -> System Settings -> Runtime -> Share Stream Playback Session Lifetime`.
- Configurable range is `5` minutes to `24` hours.

If it fails immediately after opening, first confirm the sharing link itself has not expired, has not reached the download limit, and the password verification Cookie is still valid.

## WebDAV

### Client Cannot Connect

Check in this order:

1. Whether the global WebDAV switch in system settings is off. If off, it returns 503 directly.
2. Whether the reverse proxy allows WebDAV methods (`PROPFIND` / `MKCOL` / `MOVE` / `COPY` / `LOCK` / `UNLOCK`). Nginx defaults to only allowing GET/POST/HEAD.
3. Whether the reverse proxy allows headers such as `Depth` / `Destination` / `Overwrite` / `If`.
4. Whether the URL includes the correct `/webdav` prefix.

Reverse proxy examples are in [Reverse Proxy Deployment](./reverse-proxy).

### Can Connect, but All Operations Return 401

WebDAV uses its own account system. It is **not the normal login account**. Create a dedicated account in the workspace you want to connect: use the personal-space `WebDAV` page for personal files, and the team workspace or `Settings -> Teams -> Team Details -> WebDAV` for team files.

Normal login JWT can also use Bearer authentication, but most WebDAV clients do not support custom headers. Dedicated accounts are the easiest path.

### Can List Directories, but Writes Fail

Check the error code:

- `resource.locked`: a file or folder is held by a WebDAV LOCK. Usually another client did not release it, or the previous client crashed without UNLOCK. Wait for lock expiration or unlock manually in the admin panel.
- `precondition_failed`: the client sent `If-Match` / `If-None-Match`, but the condition was not satisfied. Common when multiple clients edit the same file at once.
- Quota-related: user quota is full.

If moving, copying, or deleting a folder fails, a child file or subfolder in that tree may have a conflicting lock. AsterDrive checks these locks recursively so clients cannot bypass a lock by modifying a parent folder. Close other WebDAV clients or online editors first, then wait for expiration. If the lock is clearly stale, clean it under `Admin -> Locks`.

## Office / WOPI

### Collabora / OnlyOffice Loads a Blank Screen

The most common cause is **the WOPI service cannot connect back to AsterDrive**.

The WOPI protocol requires the preview service to fetch file content from AsterDrive, so the preview service side must be able to access the configured public site origin. When generating a WOPI URL, AsterDrive first matches the current request Origin (scheme + host[:port]) exactly. If it matches, that origin is used; if not, the first entry is used as fallback. Make sure the final selected origin is reachable by the WOPI service:

- Preview service and AsterDrive both run in Docker: use Docker network, and set the public site origin to a domain the other side can resolve.
- Preview service is on the public internet: public site origin must be publicly reachable HTTPS.
- Preview service is on the intranet: use an intranet domain + intranet certificate for public site origin.

### Token Failure / 401

WOPI `access_token` is short-lived. If the client has been open for a long time before an operation, the token may expire. Refresh the page to request a new one.

If it repeats:

- Check whether the AsterDrive clock is correct. WOPI proof-key validation has a +/-20 minute time window.
- Check the WOPI service's own clock.

### Save or Move Reports the File Is Locked

WOPI editing keeps locks on files, and AsterDrive also allows multiple shared locks where the protocol semantics allow them. While valid locks exist, conflicting overwrite, move, or delete operations are rejected. First confirm whether the file is still open in an Office editor, WebDAV client, or another browser tab. If a client already exited abnormally, administrators can clean expired locks or force-unlock stale records under `Admin -> Locks`.

## Background Tasks

### Mail Cannot Be Sent

First go to `Admin -> System Settings -> Mail Delivery` and send a test email, then inspect server logs for mail outbox / SMTP errors.

Separate by symptom:

- Everything fails: SMTP configuration is wrong. Use "Send Test Email" under `Admin -> System Settings -> Mail Delivery` to verify connectivity first.
- Partial failures: recipient addresses may be blocked by the recipient domain. Check specific delivery logs.
- Status appears stuck: first confirm the service process is still running. To check whether system scheduling continues to work, go to `Admin -> Tasks` and see whether recent system task records have repeated `Mail outbox dispatch` failures.

### Thumbnails Never Appear

Thumbnails are generated asynchronously by workers. They usually appear within seconds to tens of seconds after upload.

If they never appear:

1. Is the file type supported? Most images are supported; some videos are supported.
2. Does the file size exceed the thumbnail generation limit?
3. Are there recent repeated thumbnail or system task failures under `Admin -> Tasks`?

### Trash Items Do Not Clear After Expiration

Cleanup tasks run hourly, **not immediately**. If the setting is 30 days, an item that just reached 30 days will not vanish instantly.

If it is still there after a day:

- Confirm the trash retention days under `Admin -> System Settings -> Storage and Retention` were really changed.
- Check recent `Trash cleanup` system task failures under `Admin -> Tasks`, or inspect server logs directly.

## Post-Upgrade Issues

### Upgrade Fails at Startup with Database Errors

This usually means database migration did not finish.

Both binary and Docker image run migrations automatically at startup, but migration fails if the database account lacks DDL permissions.

Check in this order:

1. Whether the database account has `CREATE` / `ALTER` permissions.
2. The specific error during the migration phase in startup logs.
3. If upgrading from a very old version, run `aster_drive database-migrate` separately to see a more detailed error.

### Some Features "Disappeared" After Upgrade

They usually did not disappear; their location changed. First read the corresponding version section in the [changelog](https://github.com/AsterCommunity/AsterDrive/blob/master/CHANGELOG.md).

If upgrading from an early prerelease build, read the early-build notes in [Upgrade and Version Migration](./upgrade) first. A full backup is still recommended before upgrade; see [Backup and Restore](./backup).

## Still Not Solved

Submit an issue in this order:

1. Run `aster_drive doctor` once and paste the output.
2. Paste the full JSON from `/health/ready`.
3. Paste logs around the symptom, at least 50 lines before and after.
4. Open an issue in [GitHub Issues](https://github.com/AsterCommunity/AsterDrive/issues).

Do not delete the `code` field from the `/health/ready` JSON. If logs also include a structured `error_code` field, keep that too. These fields are the fastest clues for locating the problem.
