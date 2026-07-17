# Glossary

This page explains terms that appear repeatedly in the AsterDrive documentation. When a concept is unfamiliar, check its meaning here before continuing with the related configuration or workflow.

## Nodes and Runtime Modes

| Term | Meaning | Related docs |
| --- | --- | --- |
| Primary node / primary | The default runtime mode. It handles login, frontend, admin console, shares, WebDAV, policies, and metadata. | [Server Configuration](/en/config/server) |
| Follower node / follower | A remote storage backend. It only accepts internal object requests signed by the primary node and does not let regular users log in. | [Follower Nodes](/en/guide/remote-nodes) |
| Remote node | A follower record registered in the primary admin console, including node address, status, secrets, and remote storage targets. | [Follower Nodes](/en/guide/remote-nodes) |
| Remote storage target | The real place where a follower writes objects, either a local directory or S3 / MinIO; a remote policy can bind a specific target. | [Follower Nodes](/en/guide/remote-nodes) |
| enroll | The action that binds a follower to the primary. Usually, the admin console generates a command and you run it on the follower. | [Operations CLI](/en/deployment/ops-cli) |

## Storage and Uploads

| Term | Meaning | Related docs |
| --- | --- | --- |
| Storage policy | Defines which storage policy backend receives files and how uploads are performed. | [Storage Policies](/en/config/storage) |
| Policy group | Decides which storage policy a user or team hits during upload. It can route by file size. | [Storage Policies](/en/config/storage) |
| Blob | The underlying file object. Multiple file records can reference one blob for content deduplication and version references. | [About AsterDrive](/en/reference/about) |
| Chunked upload | Splits a large file into multiple chunks and tries to resume after failure. | [Uploads and Large Files](/en/guide/upload-modes) |
| Direct-to-object-storage upload | The browser uploads directly to an object storage backend; the server only signs requests and confirms completion. | [Uploads and Large Files](/en/guide/upload-modes) |
| Server relay | The browser uploads to AsterDrive first, then the server writes to the storage policy backend. | [Uploads and Large Files](/en/guide/upload-modes) |

## Configuration

| Term | Meaning | Related docs |
| --- | --- | --- |
| `config.toml` | Startup configuration. It controls listen address, database, logging, WebDAV prefix, node mode, and similar startup-level behavior. | [Configuration Overview](/en/config/) |
| System settings | Site-wide rules changed live from the admin console, including public site URLs, registration, cookies, mail, trash, WOPI, and audit logs. | [System Settings](/en/config/runtime) |
| Public site URL | The externally reachable HTTP(S) origin of AsterDrive, used for shares, mail, WebDAV, WOPI callbacks, and more. | [System Settings](/en/config/runtime) |
| CORS | Browser cross-origin access rules. Only allow origins when you explicitly need cross-origin API calls. | [System Settings](/en/config/runtime) |
| Reverse proxy | The public entry point such as Caddy, Nginx, or Traefik, responsible for HTTPS, domains, upload sizes, and passing WebDAV methods. | [Reverse Proxy](/en/deployment/reverse-proxy) |

## Access Protocols and External Services

| Term | Meaning | Related docs |
| --- | --- | --- |
| WebDAV | Lets Finder, Windows, rclone, or sync tools access AsterDrive through a file protocol. | [WebDAV](/en/config/webdav) |
| WOPI | The Office online preview/editing protocol. AsterDrive provides file APIs while OnlyOffice / Collabora and similar services open the files. | [File Editing](/en/guide/editing) |
| Preview app | A file open method configured in the admin console. It can be a built-in preview, an external URL template, or a WOPI app. | [System Settings](/en/config/runtime) |
| Audit log | Records important user and admin actions for troubleshooting, tracing, and routine checks. | [Admin Console](/en/guide/admin-console) |

## Operations

| Term | Meaning | Related docs |
| --- | --- | --- |
| `doctor` | Operations CLI subcommand for deployment checks, configuration checks, and deep consistency checks. | [Operations CLI](/en/deployment/ops-cli) |
| Database migration | Moves business data from one database backend to another, such as SQLite to PostgreSQL. | [Operations CLI](/en/deployment/ops-cli) |
| Frontend asset cache | A common cause of broken pages after upgrades, when a browser or proxy caches old page assets. | [Frontend Asset Cache](/en/deployment/frontend-assets) |
| Rollback | Returning to an old version after a failed upgrade, usually with the old binary/image and pre-upgrade backups. | [Upgrades and Version Migration](/en/deployment/upgrade) |
