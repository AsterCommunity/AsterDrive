# System Settings

::: tip This page covers every group under `Admin -> System Settings`
System settings are site-wide rules maintained directly by administrators in the admin console. Jump to the group you care about; you do not need to read from the beginning.
**Most changes do not require a service restart**. After saving, they affect later new requests, new uploads, and newly sent emails.
:::

Entry point:

```text
Admin -> System Settings
```

## Start from Your Goal

| Goal | Check This Group First | If It Is Still Wrong |
| --- | --- | --- |
| Site links, share links, or mail link domains are wrong | Site Configuration | Then check [reverse proxy](/en/deployment/reverse-proxy) |
| Login cookie, token, activation link, or email-code MFA timing is unsuitable | Authentication and Cookies | Then check [login and sessions](/en/config/auth) |
| Registration, avatars, or Gravatar behavior is unexpected | User Management | Then check [admin console](/en/guide/admin-console) |
| Passkey, MFA, external login, or external identity binding is unexpected | Site Configuration / Admin -> External Authentication / Authentication and Cookies | Then check [login and sessions](/en/config/auth) |
| Mail cannot be received, or links are wrong | Mail Delivery | Then check [mail](/en/config/mail) |
| Browser blocks cross-origin API calls | Network Access | First confirm it is not a `Public Site URL` issue |
| Background tasks, thumbnails, archive preview, or trash retention behaves abnormally | Runtime and Scheduling / Storage and Retention | Then check [operations CLI](/en/deployment/ops-cli) |
| Audio/video playback links on share pages expire too quickly or too slowly | Runtime and Scheduling | Then check [sharing and public access](/en/guide/sharing) |
| WebDAV global switch, system-file blocking, or connection behavior is abnormal | WebDAV | Then check [WebDAV](/en/config/webdav) |
| You need to see who changed what | Audit Logs | Then check [admin console](/en/guide/admin-console#audit-logs) |

## Places Administrators Change Most Often

| What you want to do | Where to change it |
| --- | --- |
| Make share links, mail links, WebDAV addresses, and online previews point to the correct domain | `Site Configuration -> Public Site URL` |
| Change the title, logo, or favicon shown on login and share pages | `Site Configuration` |
| Add external preview or WOPI opening methods for Office files | `Site Configuration -> Preview Apps` |
| Enable or limit read-only archive preview | `Storage and Retention -> Archive Preview` |
| Connect OIDC / Generic OAuth2 / SSO login providers | `Admin -> External Authentication` |
| Disable public registration | `User Management -> Allow Public User Registration` |
| Change the default quota for new users, then recheck actual team quotas after creating teams | `Storage and Retention -> New User Default Storage Quota` |
| Tune cookie security requirements and Access / Refresh Token TTLs | `Authentication and Cookies` |
| Tune activation, email-change, and password reset link TTLs | `Authentication and Cookies` |
| Enable email-code MFA, or allow TOTP users to use email codes as fallback | `Authentication and Cookies` |
| Tune the external login email verification mail template | `Mail Delivery -> External Login Email Verification` |
| Tune the login email code mail template | `Mail Delivery -> Login Email Code` |
| Configure SMTP, send test mail, or edit transactional mail templates | `Mail Delivery` |
| Tune retention for trash, version history, team archives, and task artifacts | `Storage and Retention` |
| Tune the online extraction staging size limit | `Storage and Retention -> Online Extraction Staging Size Limit` |
| Tune thumbnail size limits and vips / ffmpeg / ffprobe processors | `Storage and Retention -> Media Processing` |
| Disable WebDAV, or adjust blocking for system files such as `.DS_Store` and `Thumbs.db` | `WebDAV` |
| Tune mail dispatch, background task dispatch, concurrency, retry, and periodic cleanup frequency | `Runtime and Scheduling` |
| Tune the temporary audio/video streaming session TTL on share pages | `Runtime and Scheduling -> Share Streaming Playback Session TTL` |
| Enable or disable audit logs | `Audit Logs` |

## Current Groups

- **Site Configuration** - Public site URL, title, logo, favicon, preview apps
- **User Management** - Public registration, registration activation, avatars, Gravatar
- **Authentication and Cookies** - Cookie security rules, token TTLs, activation/email-change/reset link TTLs, email-code MFA
- **Mail Delivery** - SMTP, sender, test mail, registration activation/email-change/password reset/external login email verification/login email code mail templates
- **Network Access** - Browser cross-site access rules (CORS)
- **Runtime and Scheduling** - Mail queue, background tasks, task-lane concurrency, share streaming playback sessions, periodic cleanup, low-level consistency checks, follower node health checks, list limits
- **Storage and Retention** - Trash, version history, default quotas, task artifacts, online extraction staging, archive preview, media processing
- **WebDAV** - Global switch and common system-file blocking
- **Audit Logs** - Switch and retention time
- **Custom Configuration**, **Other** - Advanced scenarios only

## Site Configuration

If the site needs to be accessed externally, configure this group first.

- **`Public Site URL`**
  Enter the HTTP(S) origins users actually use to access the site. Fill one origin per input in the list, for example:

  ```text
  https://drive.example.com
  https://panel.example.com
  ```

  Each item should contain only the origin: protocol, domain, and optional port. Do not include paths, do not include `/api`, and do not use wildcards. The system uses these origins to generate share pages, mail links, WebDAV addresses, Office / WOPI preview URLs, and absolute URLs needed by later callbacks. When left empty, most browser pages can work from the current access address, but external entry points are more likely to generate wrong links. **Production deployments should explicitly configure the public site URL.**

  If the same instance is accessed through multiple domains, add all of them to this list. AsterDrive finds an exact matching origin in the list based on the current request Host. If matched, it uses that origin to generate links. If not matched, it uses the first item as the fallback origin.

  ::: warning This is not a CORS allowlist
  `Public Site URL` means "which public entry points belong to this AsterDrive instance", and it also participates in same-site CSRF trust decisions for cookie writes. It does not automatically allow browsers to call APIs cross-origin. Cross-origin access is configured separately under `Network Access -> Allowed CORS Origins`.
  :::
- **`Site Title`, `Site Description`**
  Affect the title and description on login pages, share pages, and app pages.
- **`favicon`, light logo, dark logo**
  Affect branding shown in browser tabs, login pages, and the site header.
- **`Preview Apps`**
  Provide additional "open with" options for Office, PDF, spreadsheet, or other files. Built-in previewers, external URL templates, and WOPI opening methods are managed here together.
- **`WOPI-related TTLs`**
  Adjust these only when integrating online Office preview/editing services such as OnlyOffice. Normal deployments should keep the defaults.

::: tip Recommended order for enabling WOPI

1. Configure `Public Site URL` correctly first
2. Enable an existing app under `Preview Apps`, or import a new app through `WOPI Discovery`
3. Confirm the external Office / WOPI service can call back to `/api/v1/wopi/...` generated from `Public Site URL`
4. If browser cross-origin calls to the AsterDrive API are blocked, allow the corresponding origin under `Network Access`
5. Open real `docx` / `xlsx` / `pptx` files once and confirm they can be saved back to AsterDrive

:::

`WOPI access token TTL`, `WOPI lock TTL`, and `WOPI discovery cache duration` are all in this group. Adjust them manually only after you have connected a WOPI service and actually run into problems such as session expiry or discovery updates not taking effect in time.

## User Management

This group controls account entry points and avatar-related behavior.

- **`Allow Public User Registration`** - After disabling it, the login page only supports existing-account login and administrator initialization. New accounts can only be created by administrators.
- **`Require Email Activation After Registration`** - After enabling it, normal users created through public registration must click the activation email before logging in.
- **`Avatar Directory`** - User-uploaded avatars are written to this local directory. Relative paths resolve under server-side `./data`.
- **`Avatar Upload Size Limit`** - Avatar files exceeding this limit are rejected directly.
- **`Gravatar Base URL`** - If official Gravatar access is unstable, change it to a proxy or mirror.

## Authentication and Cookies

This group decides browser login behavior and session safety.

- **`Authentication Cookie Sent Only Over HTTPS`** - Keep enabled in production. Disable temporarily only for local or intranet plain-HTTP trial runs.
- **`Access Token TTL`, `Refresh Token TTL`** - Control how long login state is maintained.
- **`Registration Activation Link TTL`**
- **`Email Address Change Link TTL`**
- **`Password Reset Link TTL`**
- **`Verification Email Resend Cooldown`**
- **`Password Reset Request Cooldown`**
- **`Require Email Code MFA`** - Requires working mail delivery. After enabling it, verified-email users without TOTP can complete second-factor verification with an 8-digit email code after password or external identity login.
- **`Allow TOTP Email Fallback`** - Allows users who already have an authenticator to choose email code on the MFA login page. Security-sensitive sites can keep it disabled.
- **`Email Login Code TTL`** - Default is `10` minutes; actual validity never exceeds the remaining lifetime of the current MFA login flow.
- **`Email Login Code Resend Cooldown`** - Default is `60` seconds.

For normal deployments, you usually only need to confirm cookie security requirements and link TTLs match your site policy.

::: warning Email codes depend on mail security
Email-code MFA is useful only when SMTP delivery is reliable and user email addresses are verified. Before enabling it, send a test mail under `Mail Delivery` and confirm the `Login Email Code` template matches your site's wording.
:::

## Mail Delivery

This group decides whether registration activation, password reset, and email address change emails can be sent. The most commonly changed items are:

- SMTP host, port, username, password
- Sender address and sender name
- Whether to enable SMTP encryption
- Test mail
- Registration activation, password reset, email address change, external login email verification, and login email code mail templates

::: warning Check before enabling registration
If the site will allow registration, password recovery, or email address changes, check mail configuration and `Public Site URL` **together**. Do not configure only one of them.

If external authentication allows users to continue binding or account creation through email verification, it also depends on this mail configuration group.
:::

See [mail](/en/config/mail) for detailed guidance.

## Network Access

This group mainly handles browser cross-site access rules (CORS).

Change it only in these scenarios:

- The browser page and AsterDrive are not under the same domain
- You want another site to call AsterDrive directly from the browser

::: tip Same-site deployments usually do not need changes
Most deployments where "frontend pages and APIs are on the same site" do not need to touch this group.

When connecting an external WOPI service, the most common issue is not here. It is usually that the Office service cannot call back to the WOPI URL generated from `Public Site URL`. Add an origin here only when the browser console clearly reports a CORS error for the AsterDrive API.
:::

## Runtime and Scheduling

Administrators decide the pace of background work here. Default behavior:

| Task | Default Frequency |
| --- | --- |
| Mail queue scan | Every 5 seconds |
| Background task queue scan | Every 5 seconds |
| Background task idle backoff maximum | Every 60 seconds |
| Periodic cleanup | Every 1 hour |
| Low-level file consistency check | Every 6 hours |
| System health checks (database / cache / follower nodes) | Every 5 minutes |

You can also tune:

- Background task idle backoff maximum
- Reserved fallback background task concurrency limit. It is currently used only by future unclassified task kinds, not by existing task lanes
- Concurrency limit for archive tasks: online compression, online extraction, and archive preview
- Thumbnail generation task concurrency limit
- Storage policy migration task concurrency limit
- Maximum background task attempts
- Share download rollback queue capacity
- Share streaming playback session TTL
- System health check interval
- Team member list page size limit
- Task list page size limit

If there are no obvious performance issues, queue backlogs, or follower node detection delays, keep the defaults.  
If you increase concurrency for the archive, thumbnail, or storage-migration lanes, matching tasks can run together more easily, and CPU, memory, network, and I/O pressure will increase with them. The reserved fallback concurrency cap is not a "global total concurrency" setting, so do not rely on it to limit every background task.

Audio and video on share pages create a short-lived streaming playback session first to support Range playback. The default TTL is `3` hours, configurable from `5` minutes to `24` hours. Longer TTLs work better for long background music playback; shorter TTLs reduce the access window after a link leak.

## Storage and Retention

This group decides "how long data is kept" and "how much space new objects get by default". Default rules:

| Item | Default |
| --- | --- |
| Historical versions per file | `10` |
| Trash retention | `7` days |
| Team archive retention | `7` days |
| Task retention | `24` hours |
| New user default storage quota | `0` (unlimited) |
| Online extraction source archive size limit | `512 MiB` |
| Online extraction staging size limit | `2 GiB` |
| Online extraction uncompressed size limit | `1 GiB` |
| Online extraction entry count limit | `10000` |
| Online extraction duration limit | `300` seconds |
| Archive build entry count limit | `10000` |
| Archive build total source size limit | `2 GiB` |
| Archive build output size limit | `2 GiB` |
| Archive preview global switch | Disabled |
| Archive preview user-side switch | Disabled |
| Archive preview share-side switch | Disabled |
| Archive preview source file size limit | `64 MiB` |
| Archive preview entry count limit | `2000` |
| Archive preview manifest size limit | `64 KiB` |
| Archive preview scan duration limit | `30` seconds |
| Thumbnail source file size limit | `64 MiB` |
| Media metadata extraction | Enabled |
| Media metadata source file size limit | `256 MiB` |

::: warning Default quotas affect only new objects

- The UI label for this item is `New User Default Storage Quota`
- After creating a team, recheck the actual team quota under `Admin -> Teams`
- This setting **only affects objects created later**. Existing accounts or teams are not automatically rewritten.

:::

### Archive Preview

Archive preview is read-only. It scans metadata from supported archive formats and generates a manifest; it does not extract the archive into the user's folder. It is not the same thing as "online extraction".

This group has three layers of switches:

- **Enable Archive Preview**: global switch
- **Enable Archive Preview for Users**: whether logged-in users can preview archives in personal and team spaces
- **Enable Archive Preview for Shares**: whether public share pages can preview archives after passing password and share-scope checks

All three are disabled by default. Enable them only when users really need to inspect archive contents, especially the share-side switch. It lets visitors see metadata such as internal file names, directory structure, sizes, and modified times.

Limits control source archive size, entry count, returned manifest size, and single-scan duration. When an archive is opened for the first time and the manifest has not been cached, the system creates an `archive_preview_generate` background task. After generation completes, reopening reuses the cached manifest.

When users switch `filename encoding` in the preview toolbar, AsterDrive rereads or regenerates the manifest with the selected encoding. This is for old ZIP files or ZIP file names created across language environments that display as garbled text. It does not modify the original archive.

### Online Extraction and Archive Building

Online extraction, online compression, and folder archive download all use the archive background-task lane, and all of them use the server temporary directory. The defaults are sized for common personal and small-team files. Do not raise every limit immediately.

If users often process large archives or large folders, check these settings separately:

- **Online extraction source archive size limit**: rejects source archives that are too large
- **Online extraction staging size limit**: counts the source archive downloaded locally plus files extracted into staging
- **Online extraction uncompressed size, entry count, path depth, compression ratio, and duration limits**: protect against archive bombs and abnormal metadata-heavy archives
- **Archive build entry count, total source size, and output size limits**: affect batch online compression and folder archive downloads

Before raising these limits, confirm that the disk backing `server.temp_dir`, CPU, and archive-task concurrency can handle the extra load. Otherwise the usual result is not "larger files work", but "tasks queue longer or the temp disk fills faster".

### Media Processing

Media processing is responsible for thumbnail generation, not online preview itself.  
It now has a structured editor under `Storage and Retention -> Media Processing`, so you do not need to edit JSON manually.

You can do these things there:

- Enable or disable a processor
- Bind file extensions to a processor
- Configure commands used by `vips_cli`, `ffmpeg_cli`, or `ffprobe_cli`
- Test whether the command can be executed by the server
- Keep AsterDrive's built-in image processor as a fallback

The default built-in path covers common image formats.  
If you want to extend support for HEIC, AVIF, PDF covers, video thumbnails, or video metadata, you can connect `vips`, `ffmpeg`, or `ffprobe`, but only if those commands are actually installed in the server environment.

::: tip Keep the built-in processor first
Unless you have confirmed the command paths, permissions, and extension bindings for vips / ffmpeg / ffprobe, keeping the built-in processor as a fallback is simpler.
:::

::: details Media processing ENV on first startup
When the service initializes system settings for the first time, it reads three bootstrap environment variables to decide whether CLI processors are enabled in the default media processing configuration:

```bash
ASTER_BOOTSTRAP_ENABLE_VIPS_CLI=true
ASTER_BOOTSTRAP_ENABLE_FFMPEG_CLI=true
ASTER_BOOTSTRAP_ENABLE_FFPROBE_CLI=true
```

The official Docker image already installs `vips`, `ffmpeg`, and `ffprobe`, and enables these three bootstrap ENV values by default. New databases usually get the corresponding processors directly.

These three variables only affect the initial default value when `media_processing_registry_json` does not yet exist. This rule table is the unified media processing configuration entry point. It manages enabled state, capability purposes, extension bindings, and command paths for built-in `images`, built-in `lofty`, VIPS CLI, FFmpeg CLI, and FFprobe CLI. Thumbnails and media metadata both use this path.
:::

### Media Metadata

Media metadata and thumbnails share `media_processing_registry_json`:

- `media_metadata_enabled` is the global switch
- `media_metadata_max_source_bytes` limits the source file size accepted by media metadata background tasks
- When the `images` processor is enabled and has the `metadata:image` purpose, it handles image metadata
- When the `lofty` processor is enabled and has the `metadata:audio` purpose, it handles audio metadata; when it has the `thumbnail:audio` purpose, it generates WebP thumbnails from embedded audio covers
- When the `ffprobe_cli` processor is enabled and has the `metadata:video` purpose, it handles video metadata; its `config.command` can be a command name or an absolute path

If server-side `ffprobe` has been renamed, is not in PATH, or needs a custom installation path, change `ffprobe_cli.config.command` in `media_processing_registry_json` to the corresponding command or absolute path, then run `test_ffprobe_cli` in the media processing registry to probe it.

## WebDAV

This group controls site-wide WebDAV runtime behavior:

- **`Enable WebDAV`**
- **`Block WebDAV System Files`**
- **`Blocked WebDAV System-File Patterns`**

After disabling it, desktop clients can no longer access files through WebDAV immediately.

By default, AsterDrive blocks WebDAV clients from creating common operating-system metadata files and folders, including `.DS_Store`, `._*`, `.Spotlight-V100`, `.Trashes`, `.fseventsd`, `Thumbs.db`, `desktop.ini`, `$RECYCLE.BIN`, and `System Volume Information`. These are usually written automatically by Finder, Windows Explorer, or sync tools, and most deployments do not want them scattered through the drive.

The patterns match basenames, ignore case, and support simple `*` wildcards. Disable this behavior or remove a pattern only when you explicitly need to sync those system files.

::: tip Change the path prefix in the site configuration page
If you only want to change the WebDAV path prefix or the hard WebDAV upload size limit, that is not on this page. Change `[webdav]` in [`config.toml`](/en/config/webdav) instead, then restart.
:::

## Audit Logs

This group decides whether admin and key operations leave records.

- **`Enable Audit Logs`**
- **`Audit Log Retention`**

::: warning Do not disable casually
If you want to later investigate "who deleted files, who created shares, who changed team members", keep it enabled.
:::

## When Changes Take Effect

| Change | Effective Timing |
| --- | --- |
| Site address, title, logo, favicon | Shown with the new values after refreshing the page |
| Preview apps / online Office related settings | Applied to previews opened later |
| WOPI access token / lock / discovery cache | Applied to new WOPI sessions opened later |
| Public registration, registration activation, mail templates | Applied to later login flows and newly sent emails |
| External login providers | Applied to the login page and later external login flows after saving |
| External login email verification mail template, login email code mail template | Applied to newly sent matching emails |
| Email-code MFA switch, fallback policy, TTL, and resend cooldown | Applied to later MFA login flows and newly sent email codes |
| Cookie security, token TTLs | Applied to later login, refresh, and share password verification |
| Avatar directory, avatar size limit | Applied to avatar uploads after the change |
| Default quota | Only affects objects created later |
| Trash, team archive, task artifact, audit log retention | Background cleanup tasks work with the new rules |
| Version history limit | Applied when new versions are produced later |
| Online extraction staging limit | Applied to online extraction tasks created later |
| Online extraction source, uncompressed size, entry count, path depth, compression ratio, and duration limits | Applied to online extraction tasks created later |
| Archive build entry, total source size, and output size limits | Applied to online compression and archive download tasks created later |
| Archive preview switches and limits | Applied to later requests and new `archive_preview_generate` tasks |
| Thumbnail source file size limit | Applied to files entering thumbnail tasks later |
| Media processor switches, commands, extension bindings | Applied to files entering thumbnail tasks later |
| Media metadata switch, size limit, processor binding | Applied to files entering media metadata tasks later; existing caches are not automatically rescanned because configuration changed |
| Mail dispatch, background tasks, periodic cleanup, follower node health check frequency | Applied to later background polling |
| Background task lane concurrency and maximum attempts | Applied to background tasks scheduled or retried later |
| Share streaming playback session TTL | Applied to audio/video playback sessions created later on share pages |
| WebDAV switch, system-file blocking rules, CORS | New requests respond with the new rules immediately |

## About "Custom Configuration"

The `Custom Configuration` group is **mainly for custom frontend developers**. It is a global-variable persistence layer reserved for **custom frontend developers**.

If you replace the frontend with your own version by using the `./frontend-override/` directory, and you need to persist some site-level configuration such as theme, brand color, custom entry points, or third-party integration credentials, you can write them into the database through `Custom Configuration`, then expose them to the frontend through backend APIs.

::: tip Naming convention
Custom configuration keys use the `{namespace}.{name}` form, for example:

- `my-frontend.theme`
- `my-frontend.brand.primary_color`
- `my-frontend.feature.enable_xxx`

Use an identifier for your custom frontend as `namespace` to avoid conflicts with others. Built-in system configuration is all `source="system"`; custom configuration is `source="custom"`. The admin console separates them by this field.
:::

::: warning Keep it empty when not using a custom frontend
For normal deployments using the official frontend, leave the whole `Custom Configuration` group **empty**. Its content does not affect any official frontend feature.

If you just want to find things like "theme color", "site title", or "Logo", adjust them in the `Site Configuration` group.
:::
