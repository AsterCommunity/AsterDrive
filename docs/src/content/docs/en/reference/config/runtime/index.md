---
description: "System Settings overview: find the right group by goal, the settings administrators change most, links into each group, when changes take effect, and what Custom Configuration is for."
title: "System Settings"
---

:::tip[This section covers every group under `Admin -> System Settings`]
System settings are site-wide rules maintained directly by administrators in the admin console. Each group has its own page; jump to the group you care about.
**Most changes do not require a service restart**. After saving, they affect later new requests, new uploads, and newly sent emails.
:::

Entry point:

```text
Admin -> System Settings
```

## Start from Your Goal

| Goal | Check This Group First | If It Is Still Wrong |
| --- | --- | --- |
| Site links, share links, or mail link domains are wrong | [Site Configuration](/en/reference/config/runtime/site/) | Then check [reverse proxy](/en/deploy/reverse-proxy/) |
| Login cookie, token, activation link, or email-code MFA timing is unsuitable | [Authentication and Cookies](/en/reference/config/runtime/auth/) | Then check [login and sessions](/en/reference/config/auth/) |
| Registration, password / Passkey sign-in, local email allow/block lists, avatars, or Gravatar behavior is unexpected | [User Management](/en/reference/config/runtime/users/) | Then check [login and sessions](/en/reference/config/auth/) |
| Passkey, MFA, external login, or external identity binding is unexpected | [Site Configuration](/en/reference/config/runtime/site/) / Admin -> External Authentication / [Authentication and Cookies](/en/reference/config/runtime/auth/) | Then check [login and sessions](/en/reference/config/auth/) |
| Mail cannot be received, or links are wrong | [Mail Delivery](/en/reference/config/runtime/mail/) | Then check [mail](/en/admin/mail/) |
| Browser blocks cross-origin API calls | [Network Access](/en/reference/config/runtime/network/) | First confirm it is not a `Public Site URL` issue |
| Background tasks, thumbnails, image preview, archive preview, or trash retention behaves abnormally | [Runtime](/en/reference/config/runtime/jobs/) / [File Processing](/en/reference/config/runtime/file-processing/) / [Storage and Retention](/en/reference/config/runtime/retention/) | Then check [operations CLI](/en/ops/cli/) |
| Link import file size, speed, concurrency, or timeout is unsuitable | [Runtime](/en/reference/config/runtime/jobs/) / [File Processing](/en/reference/config/runtime/file-processing/) | Then check [operations CLI](/en/ops/cli/) |
| Audio/video playback links on share pages expire too quickly or too slowly | [Runtime](/en/reference/config/runtime/jobs/) | Then check [sharing and public access](/en/using/sharing/) |
| WebDAV global switch, system-file blocking, or connection behavior is abnormal | [WebDAV](/en/reference/config/runtime/webdav/) | Then check [WebDAV](/en/reference/config/webdav/) |
| You need to see who changed what, or want to narrow the audit scope | [Audit Logs](/en/reference/config/runtime/audit/) | Then check [admin console](/en/admin/#audit-logs) |

## Places Administrators Change Most Often

| What you want to do | Where to change it |
| --- | --- |
| Make share links, mail links, WebDAV addresses, and online previews point to the correct domain | `Site Configuration -> Public Site URL` |
| Change the title, logo, or favicon shown on login and share pages | `Site Configuration` |
| Add external preview or WOPI opening methods for Office files | `Site Configuration -> Preview Apps` |
| Enable or limit read-only archive preview | `File Processing -> Archive Preview` |
| Connect OIDC / Generic OAuth2 / GitHub / QQ / Google / Microsoft login providers | `Admin -> External Authentication` |
| Disable public registration | `User Management -> Allow Public User Registration` |
| Temporarily disable Passkey sign-in | `User Management -> Registration & Login -> Allow Passkey Sign-In` |
| Temporarily disable password sign-in | `User Management -> Registration & Login -> Allow password sign-in` |
| Restrict email addresses usable for local registration and local email changes | `User Management -> Registration & Login -> Local Account Email Allowlist / Blocklist` |
| Change the default quota for new users; teams created without an explicit quota also use it, so recheck actual team quotas after creation | `Storage and Retention -> New User Default Storage Quota` |
| Tune cookie security requirements and Access / Refresh Token TTLs | `Authentication and Cookies` |
| Tune activation, email-change, and password reset link TTLs | `Authentication and Cookies` |
| Enable email-code MFA, or allow TOTP users to use email codes as fallback | `Authentication and Cookies` |
| Tune the external login email verification mail template | `Mail Delivery -> External Login Email Verification` |
| Tune the login email code mail template | `Mail Delivery -> Login Email Code` |
| Configure SMTP, send test mail, or edit transactional mail templates | `Mail Delivery` |
| Tune retention for trash, version history, and team archives | `Storage and Retention` |
| Tune temporary background task artifact retention | `Runtime -> Background Tasks` |
| Tune the online extraction staging size limit | `File Processing -> Online Extraction Staging Size Limit` |
| Tune thumbnail size limits, image preview strategy, and vips / ffmpeg / ffprobe processors | `File Processing -> Media Processing` |
| Tune HTTP/HTTPS link import file size, speed, concurrency, and timeout | `File Processing -> Link Import` |
| Disable WebDAV, or adjust blocking for system files such as `.DS_Store` and `Thumbs.db` | `WebDAV` |
| Tune mail dispatch, background task dispatch, concurrency, retry, and periodic cleanup frequency | `Runtime` |
| Tune the temporary audio/video streaming session TTL on share pages | `Runtime -> Share Streaming Playback Session TTL` |
| Enable or disable audit logs, or adjust the recorded scope | `Audit Logs` |

## Current Groups

- **[Site Configuration](/en/reference/config/runtime/site/)** - Public site URL, title, logo, favicon, preview apps
- **[User Management](/en/reference/config/runtime/users/)** - Public registration, registration activation, password / Passkey sign-in, local email allow/block lists, avatars, Gravatar
- **[Authentication and Cookies](/en/reference/config/runtime/auth/)** - Cookie security rules, token TTLs, activation/email-change/reset link TTLs, email-code MFA
- **[Mail Delivery](/en/reference/config/runtime/mail/)** - SMTP, sender, test mail, registration activation/email-change/password reset/external login email verification/login email code mail templates
- **[Network Access](/en/reference/config/runtime/network/)** - Browser cross-site access rules (CORS)
- **[Runtime](/en/reference/config/runtime/jobs/)** - Mail queue, background tasks, temporary task artifact retention, task-lane concurrency, share streaming playback sessions, periodic cleanup, low-level consistency checks, follower node health checks, list limits
- **[Storage and Retention](/en/reference/config/runtime/retention/)** - Trash, version history, default quotas
- **[File Processing](/en/reference/config/runtime/file-processing/)** - Online extraction, archive building, archive preview, link import, thumbnails, media metadata, and media processors
- **[WebDAV](/en/reference/config/runtime/webdav/)** - Global switch and common system-file blocking
- **[Audit Logs](/en/reference/config/runtime/audit/)** - Switch, recorded scope, and retention time
- **Custom Configuration**, **Other** - Advanced scenarios only

## When Changes Take Effect

| Change | Effective Timing |
| --- | --- |
| Site address, title, logo, favicon | Shown with the new values after refreshing the page |
| Preview apps / online Office related settings | Applied to previews opened later |
| WOPI access token / lock / discovery cache | Applied to new WOPI sessions opened later |
| Public registration, registration activation, mail templates | Applied to later login flows and newly sent emails |
| Local email allowlist / blocklist | Applied to later local registration and local email changes; third-party SSO is not affected |
| Passkey sign-in switch | Applied to later Passkey sign-in requests; existing Passkeys are not deleted |
| Password sign-in switch | Applied to later local password sign-in requests; existing passwords, accounts, and sessions are not deleted |
| External login providers | Applied to the login page and later external login flows after saving |
| External login email verification mail template, login email code mail template | Applied to newly sent matching emails |
| Email-code MFA switch, fallback policy, TTL, and resend cooldown | Applied to later MFA login flows and newly sent email codes |
| Cookie security, token TTLs | Applied to later login, refresh, and share password verification |
| Avatar directory, avatar size limit | Applied to avatar uploads after the change |
| Default quota | Only affects accounts created later, and teams created later without an explicit quota |
| Audit log switch and recorded scope | Later audit writes follow the new scope |
| Audit log retention window | Background cleanup tasks work with the new rules |
| Version history limit | Applied when new versions are produced later |
| Online extraction staging limit | Applied to online extraction tasks created later |
| Online extraction source, uncompressed size, entry count, path depth, compression ratio, and duration limits | Applied to online extraction tasks created later |
| Online archive compression global switch | Applied to online-compression tasks created later; does not affect online extraction, folder archive downloads, or archive preview |
| User and share archive-download switches | After saving, the official frontend refreshes public capabilities and hides or shows the matching ZIP methods; new requests are also enforced by the backend |
| Archive build entry, total source size, and output size limits | Applied to online compression and archive download tasks created later |
| Link import engine registry, temp directory, file size, speed, concurrency, request timeout, and aria2 parameters | Applied to link-import tasks created later; manual retries clean old artifacts from both the default temp directory and the current offline-download temp directory |
| Archive preview switches and limits | Applied to later requests and new `archive_preview_generate` tasks |
| Thumbnail source file size limit | Applied to files entering thumbnail and image-preview tasks later |
| Thumbnail and image-preview max dimensions | Applied to later thumbnail and image-preview generation; non-default dimensions use dimension-specific cache paths and ETags |
| Image preview strategy | Applied when the frontend later opens image previews and chooses the default source |
| Media processor switches, commands, extension bindings | Applied to files entering thumbnail and image-preview tasks later |
| Media metadata switch, size limit, processor binding | Applied to files entering media metadata tasks later; existing caches are not automatically rescanned because configuration changed |
| Mail dispatch, background tasks, periodic cleanup, follower node health check frequency | Applied to later background polling |
| Background task lane concurrency and maximum attempts | Applied to background tasks scheduled or retried later |
| Share streaming playback session TTL | Applied to audio/video playback sessions created later on share pages |
| WebDAV switch, system-file blocking rules, CORS | New requests respond with the new rules immediately |

## About "Custom Configuration"

The `Custom Configuration` group is **mainly for custom frontend developers**. It is a global-variable persistence layer reserved for **custom frontend developers**.

If you replace the frontend with your own version by using the `./frontend-override/` directory, and you need to persist some site-level configuration such as theme, brand color, custom entry points, or third-party integration credentials, you can write them into the database through `Custom Configuration`, then expose them to the frontend through backend APIs.

:::tip[Naming convention]
Custom configuration keys use the `{namespace}.{name}` form, for example:

- `my-frontend.theme`
- `my-frontend.brand.primary_color`
- `my-frontend.feature.enable_xxx`

Use an identifier for your custom frontend as `namespace` to avoid conflicts with others. Built-in system configuration is all `source="system"`; custom configuration is `source="custom"`. The admin console separates them by this field.
:::

:::caution[Keep it empty when not using a custom frontend]
For normal deployments using the official frontend, leave the whole `Custom Configuration` group **empty**. Its content does not affect any official frontend feature.

If you just want to find things like "theme color", "site title", or "Logo", adjust them in the `Site Configuration` group.
:::
