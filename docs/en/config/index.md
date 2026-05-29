---
description: AsterDrive configuration overview, explaining config.toml, admin system settings, storage policies, policy groups, follower nodes, WebDAV, rate limiting, cache, and logging.
---

# Configuration Overview

::: tip This page first helps you decide "where to change it"
AsterDrive separates configuration cleanly. Once you separate these layers, it becomes much easier to tell which issues are deployment issues, which are admin rules, and which should be written into `config.toml`.
First identify which layer you need to change, then open the corresponding page. You do not need to read this page from top to bottom.
:::

## What Layers Are There?

- **`config.toml`** - Decides how the service starts: listen address, node mode, database, logging, WebDAV prefix, network trust, and rate limiting
- **`Admin -> System Settings`** - Site-wide rules: public site URL, branding, registration/login, mail, CORS, task scheduling, share streaming playback, media processing, archive preview, trash, version history, WOPI, WebDAV switch, and audit logs
- **`Admin -> External Authentication`** - External identity providers: OIDC / SSO login entries, redirect URIs, account binding, and auto-create policies
- **`Admin -> Storage Policies`** - Where files are actually stored, and which upload method is used
- **`Admin -> Policy Groups`** - Which storage route different users, teams, or file sizes use
- **`Admin -> Follower Nodes`** - How the primary connects to followers, and where the follower receives objects by default
- **Reverse proxy / object storage configuration** - HTTPS, large uploads, WebDAV passthrough, and direct S3 uploads

The earlier layers are managed by AsterDrive itself. The last layer belongs to the reverse proxy, object storage, and external network environment.

```text
Start service
  |
  +-- config.toml / ASTER__ environment variables
  |     Responsible for: listen address, database, logging, WebDAV prefix, node mode
  |
  +-- System settings in the database
  |     Responsible for: site, registration, cookies, mail, share streaming playback, archive preview, trash, WOPI, audit
  |
  +-- External authentication providers
  |     Responsible for: OIDC / SSO login entries, binding rules between external identities and local accounts
  |
  +-- Storage policies + policy groups
  |     Responsible for: file placement, upload method, user/team routing
  |
  +-- External environment
        Responsible for: HTTPS, reverse proxy, S3 CORS, WebDAV method passthrough
```

::: tip Rule of thumb
Anything the service must know before startup usually belongs in `config.toml`. Site-wide rules adjusted after the service is already available usually belong in admin system settings. File placement belongs in storage policies and policy groups. Public ingress, certificates, and upload body limits belong in the reverse proxy.
:::

## I Want to Do This. Where Should I Change It?

| What you want to do | Where to change it |
| --- | --- |
| Change listen address, port, worker count, temporary directories, or primary / follower mode | [Server](/en/config/server) |
| Change database address, connection pool, or startup retries | [Database](/en/config/database) |
| Pin the login signing secret, MFA encryption key, or first plain-HTTP bootstrap | [Login and Sessions](/en/config/auth) |
| Public site URL, branding, registration, cookies, tokens, scheduling, share streaming playback, archive preview, trash, versions, quotas, WOPI, WebDAV, audit | [Runtime System Settings](/en/config/runtime) |
| Connect OIDC / SSO external login and manage external identity providers | [Login and Sessions](/en/config/auth) / [Admin Console](/en/guide/admin-console#external-authentication) |
| Configure SMTP, send test mail, or edit mail templates | [Mail](/en/config/mail) |
| Decide where files are stored and how uploads/downloads work | [Storage Policies](/en/config/storage) |
| Follow a tutorial for S3 / MinIO / R2 backends | [Storage Policy Backends](/en/storage/) |
| Decide which storage route different users/teams use | [Storage Policies](/en/config/storage) |
| Connect a follower node and configure the default ingress target | [Follower Nodes](/en/guide/remote-nodes) |
| Change the WebDAV path or hard WebDAV upload limit | [WebDAV](/en/config/webdav) |
| Add rate limiting to the public entry point | [Rate Limiting](/en/config/rate-limit) |
| Change cache or log output behavior | [Cache](/en/config/cache) / [Logging](/en/config/logging) |

## Where Is `config.toml`, and How Should I Write It?

On first startup, if the current working directory does not contain `data/config.toml`, AsterDrive automatically generates a default configuration file, including random `jwt_secret` and `mfa_secret_key` values.

::: tip Write only the items you want to override
You do not need to copy the full default configuration. Put only the items you want to change in `config.toml`; the rest keep their defaults.
:::

Configuration precedence:

```text
ASTER__ environment variables  >  config.toml  >  built-in defaults
```

Environment variables use double underscores `__` to represent nesting:

```bash
ASTER__SERVER__PORT=8080
ASTER__DATABASE__URL="postgres://user:pass@localhost/asterdrive"
ASTER__WEBDAV__PREFIX=/dav
```

If you start the bare binary, the process first reads `.env` in the current working directory, then starts with the same environment variable rules. For long-running deployments, put `.env` in the actual working directory of the service and tighten its permissions.

## Common Environment Variable Categories

| Type | Example | When to use |
| --- | --- | --- |
| Startup configuration overrides | `ASTER__SERVER__HOST`, `ASTER__DATABASE__URL`, `ASTER__SERVER__START_MODE` | Configuration that must be decided before the service starts; higher priority than `config.toml` |
| First-bootstrap switches | `ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES` | Only affects the default value written the first time system settings are initialized; after initialization, change it in admin system settings |
| Follower node auto-enrollment | `ASTER_BOOTSTRAP_REMOTE_MASTER_URL`, `ASTER_BOOTSTRAP_REMOTE_ENROLLMENT_TOKEN` | Automatically enroll a Docker follower on first startup; remove after success |
| Media processing defaults | `ASTER_BOOTSTRAP_ENABLE_VIPS_CLI`, `ASTER_BOOTSTRAP_ENABLE_FFMPEG_CLI`, `ASTER_BOOTSTRAP_ENABLE_FFPROBE_CLI` | Used only when media-processing system settings do not yet exist, to decide initial default processors |
| Operations CLI arguments | `ASTER_CLI_DATABASE_URL`, `ASTER_CLI_OUTPUT_FORMAT` | Use in scripts to avoid writing long arguments every time. See [operations CLI](/en/deployment/ops-cli) |

::: tip Should an ENV stay long term?
Variables such as `ASTER__SERVER__START_MODE` and `ASTER__DATABASE__URL` are long-running configuration and can stay.

One-time bootstrap inputs such as enrollment tokens can be removed after success, so later troubleshooting does not mistakenly assume they still take effect.
:::

## Sections in `config.toml`

| Section | Purpose |
| --- | --- |
| [server](/en/config/server) | Listen address, port, worker count, temporary directories, node mode, follower ingress root |
| [database](/en/config/database) | Database connection, connection pool, startup retries |
| [auth](/en/config/auth) | Login signing secret, MFA encryption key, first plain-HTTP bootstrap |
| [cache](/en/config/cache) | Memory cache / Redis / disable cache |
| [logging](/en/config/logging) | Log level, format, output file, rotation |
| [webdav](/en/config/webdav) | WebDAV path prefix and hard upload size limit |
| `[network_trust]` | Trusted reverse proxy addresses, affecting real client IP detection |
| [rate_limit](/en/config/rate-limit) | Rate limiting rules for login, public shares, and general access |

## Current Admin System Settings Groups

`Admin -> System Settings` currently displays these groups:

- Site Configuration
- User Management
- Authentication and Cookies
- Mail Delivery
- Network Access
- Runtime and Scheduling
- Storage and Retention
- WebDAV
- Audit Logs
- Custom Configuration
- Other

::: tip Easy-to-miss items before going online
- Before exposing the site publicly, set `Public Site URL`; add multiple public domains one by one and put the default origin first
- Before enabling registration, password recovery, or email address changes, verify that mail can be sent
- Before enabling external authentication, set `Public Site URL` correctly, then copy the redirect URI from `Admin -> External Authentication`
- Disable the HTTPS requirement for cookies only in plain-HTTP test environments
- When capacity is tight, shorten retention for trash, historical versions, and task artifacts
- If thumbnails do not behave as expected, check `Storage and Retention -> Media Processing`
- If you need archive manifest preview, check `Storage and Retention -> Archive Preview`
- If you need online preview such as OnlyOffice, adjust `Site Configuration -> Preview Apps`
- When connecting follower nodes, after enrollment succeeds, create the default ingress target in the follower node details
:::

See [runtime system settings](/en/config/runtime) and [mail](/en/config/mail) for details.

If the admin console is temporarily unavailable, or you want to inspect, validate, or batch-write system settings offline during a maintenance window, use the [operations CLI](/en/deployment/ops-cli).

## Storage Policies and Policy Groups Are Not in `config.toml`

They are maintained in admin pages and decide:

- **Storage policy** - Where files are actually stored, single-file size limit, chunk size, and upload method
- **Policy group** - Which storage policy a user or team hits when uploading

See [storage policies](/en/config/storage) for details.

## Know What Relative Paths Are Relative To

If you write relative paths, remember these three semantics differ:

- Location of `data/config.toml` - **relative to the current working directory**
- Relative paths in `[database]` and `[server]` - **relative to the directory containing `data/config.toml`**, meaning `./data/`
- Default local storage policy `data/uploads` - **relative to the current working directory**, not inside `data/`

Default locations by deployment method:

- Run locally: `data/` under the project directory
- systemd: `WorkingDirectory/data/`
- Official Docker image: `/data` inside the container

::: warning Use absolute paths for long-running deployments
Database paths, local storage paths, and temporary directories should preferably be absolute paths to avoid later surprises from working directory changes.
:::
