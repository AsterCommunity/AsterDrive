<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="frontend-panel/public/static/asterdrive/asterdrive-light.svg" />
    <img src="frontend-panel/public/static/asterdrive/asterdrive-dark.svg" alt="AsterDrive" width="320" />
  </picture>
</p>

<p align="center">
  A lightweight self-hosted cloud drive built with Rust and React.
  <br />
  Personal and team workspaces, local / S3 / remote-node storage policies, sharing, WebDAV, previews, WOPI, version history, trash, thumbnails, and large-file uploads.
</p>

<p align="center">
  <a href="https://asterdrive.docs.esap.cc/"><img alt="Documentation Site" src="https://img.shields.io/badge/docs-VitePress-7C3AED?style=for-the-badge&logo=vitepress&logoColor=white"></a>
  <a href="README.zh.md"><img alt="中文 README" src="https://img.shields.io/badge/README-中文-E11D48?style=for-the-badge"></a>
  <a href="docs/guide/getting-started.md"><img alt="Quick Start" src="https://img.shields.io/badge/quick%20start-guide-2563EB?style=for-the-badge"></a>
  <a href="docs/deployment/ops-cli.md"><img alt="Ops CLI" src="https://img.shields.io/badge/ops-CLI-0EA5E9?style=for-the-badge"></a>
  <a href="developer-docs/architecture.md"><img alt="Architecture" src="https://img.shields.io/badge/architecture-overview-0F172A?style=for-the-badge"></a>
  <a href="developer-docs/api/index.md"><img alt="API Docs" src="https://img.shields.io/badge/API-reference-059669?style=for-the-badge"></a>
  <a href="docs/deployment/docker.md"><img alt="Docker" src="https://img.shields.io/badge/docker-deployment-2496ED?style=for-the-badge&logo=docker&logoColor=white"></a>
</p>

## What is AsterDrive?

AsterDrive is an MIT-licensed self-hosted cloud drive for people who want to own their files without running a heavyweight collaboration suite. It focuses on the core drive experience: upload files, organize folders, recover mistakes, share links, mount WebDAV clients, and decide where objects are stored.

It is built as a Rust backend plus a React frontend, shipped as one server binary or an Alpine-based container image. The current `v0.1.x` line is an early stable release: usable for personal and small-team deployments, but still evolving quickly.

## Highlights

- **Self-hosted by default** - single service, embedded frontend assets, SQLite out of the box, optional PostgreSQL / MySQL
- **Personal and team workspaces** - separate files, shares, trash, tasks, quotas, audit trail, and storage policy groups per workspace
- **Flexible storage routing** - local filesystem, S3-compatible object storage, or another AsterDrive follower node; route uploads by user, team, and file size
- **Large-file friendly uploads** - direct uploads, resumable chunked uploads, S3 presigned uploads, and S3 multipart uploads, negotiated by policy and file size
- **Sharing and direct links** - file and folder shares with optional password, expiration time, download limits, public pages, nested folder browsing, and single-file direct links
- **WebDAV support** - dedicated WebDAV accounts, independent passwords, scoped root folders, database-backed locks, custom properties, and a small DeltaV subset
- **Preview and editing** - built-in preview for common browser-readable files, Monaco-based text editing, version history, thumbnails, and WOPI integration for external Office editors
- **Operations built in** - admin console, runtime settings, storage policy testing, health checks, audit logs, background tasks, mail queue, cleanup jobs, and `doctor` / migration CLI commands

## Quick start

### Run from source

```bash
git clone https://github.com/AptS-1547/AsterDrive.git
cd AsterDrive

cd frontend-panel
bun install
bun run build
cd ..

cargo run
```

On first startup, AsterDrive will automatically:

- generate `data/config.toml` under the current working directory if it does not exist
- create the default SQLite database when using the default database URL
- run all database migrations
- create the default local storage policy
- initialize built-in runtime configuration items in `system_config`

Default address:

```text
http://127.0.0.1:3000
```

The first registered user becomes `admin`.

Do not expose `:3000` directly to the public Internet in production. Put AsterDrive behind a reverse proxy and let the proxy handle HTTPS, page-level `Content-Security-Policy` and related security headers, upload limits, and WebDAV / WOPI passthrough. Do not replace the whole site's CSP with `sandbox`; script-capable inline file responses are sandboxed separately by the app.

### Run with Docker

```bash
# Build image
docker build -t asterdrive .

# Run container
docker run -d \
  --name asterdrive \
  -p 3000:3000 \
  -e ASTER__SERVER__HOST=0.0.0.0 \
  -e "ASTER__DATABASE__URL=sqlite:///data/asterdrive.db?mode=rwc" \
  -v asterdrive-data:/data \
  asterdrive

# Or use Compose
docker compose up -d
```

The current container image is an **Alpine runtime image** that runs as a non-root user and includes a `/health/ready` health check. The recommended persistent volume is `/data`.

Default SQLite search acceleration now depends on `FTS5 + trigram tokenizer` support. After deployment, run `./aster_drive doctor` at least once and make sure the `SQLite search acceleration` check reports `ok`.

See [`docker-compose.yml`](docker-compose.yml) and [`docs/deployment/docker.md`](docs/deployment/docker.md) for a complete deployment example.

If you need offline deployment checks, runtime-config changes from the command line, or cross-database migration from SQLite to PostgreSQL / MySQL, start with [`docs/deployment/ops-cli.md`](docs/deployment/ops-cli.md).

## Core capabilities

### File management

- hierarchical folders, directory tree navigation, list / grid views, and breadcrumb navigation
- file upload, folder upload, download, rename, move, copy, delete, restore, and permanent deletion
- search within the current workspace, multi-select, batch move / copy / delete, and archive download
- online archive compression / extraction and background task progress tracking
- thumbnails, browser-native previews, archive previews, and configurable external preview apps
- version history, version restore / deletion, and Monaco-based text editing with lock awareness
- browser-side storage-change events for refreshing the current view when files change

### Workspaces, sharing, and access

- personal workspace plus team workspaces with independent files, shares, trash, tasks, quotas, and audit records
- team membership with owner / admin / member roles, team archive / restore, and team policy-group assignment
- public share pages at `/s/:token` for files and folders
- password-protected shares, expiration time, download limits, share open / download counters, and share management pages
- shared-folder browsing with child-file download, preview, and thumbnail access inside the shared tree
- single-file direct links with inline and forced-download variants
- WebDAV accounts with independent passwords, root-folder restriction, database-backed locks, custom properties, and DeltaV subset support

### Storage and delivery

- local storage, S3-compatible storage, and remote follower-node storage policies
- policy groups that route uploads by user, team, and file size
- optional local-only blob deduplication with SHA-256 + reference counting
- S3 upload / download strategies: `relay_stream` and `presigned`, including multipart uploads for large files
- remote-node upload / download strategies: `relay_stream` and `presigned`, with follower ingress profiles backed by local or S3 storage
- streaming upload / download paths to avoid full-buffer transfers where the selected strategy allows it

### Authentication and user settings

- HttpOnly cookie auth plus Bearer JWT support for API clients
- first-user setup, public registration switch, registration activation, password reset, and email-change confirmation flows
- user profiles, uploaded avatars, Gravatar avatars, theme / language / timezone / view preferences, and session management
- optional Passkey / WebAuthn registration and login endpoints

### Operations and administration

- admin overview, user management, team management, storage policies, policy groups, remote nodes, shares, tasks, locks, runtime settings, and audit logs
- runtime config stored in `system_config`, with schema-driven admin UI and CLI access for offline operations
- health endpoints: `/health`, `/health/ready`, optional `/health/memory` (`debug_assertions + openapi`), `/health/metrics` (`metrics` feature)
- storage policy and remote-node connection testing
- background task records for archive jobs, thumbnail generation, mail dispatch, cleanup, and system runtime tasks
- periodic cleanup for uploads, trash, locks, audit logs, teams, WOPI sessions, and orphaned blobs
- Swagger UI in debug builds with the `openapi` feature, plus static OpenAPI export via `cargo test --features openapi --test generate_openapi`

## Documentation map

- [Getting started](docs/guide/getting-started.md)
- [User guide](docs/guide/user-guide.md)
- [Teams and permissions](docs/guide/teams-and-permissions.md)
- [Sharing and public access](docs/guide/sharing.md)
- [Preview and WOPI](docs/guide/preview-and-wopi.md)
- [Storage backends](docs/storage/index.md)
- [Remote follower storage](docs/storage/remote-follower.md)
- [Docker deployment](docs/deployment/docker.md)
- [Operations CLI](docs/deployment/ops-cli.md)
- [Developer docs](developer-docs/README.md)
- [Architecture](developer-docs/architecture.md)
- [API overview](developer-docs/api/index.md)

## Development

### Requirements

- Rust `1.91.1+`
- Bun
- Node.js `24+` for the current Docker frontend build stage

### Common commands

```bash
# Backend
cargo run
cargo check
cargo test
cargo test --features openapi --test generate_openapi

# Frontend
cd frontend-panel
bun install
bun run dev
bun run build
bun run check
```

### Notes

- Type checking uses `tsgo`, not `tsc`
- Linting uses `biome`, not ESLint
- TypeScript `enum` is not allowed; use `as const` objects
- Type-only imports must use `import type`

## Configuration

Static configuration is loaded with this priority:

```text
Environment variables > data/config.toml > built-in defaults
```

Examples:

```bash
ASTER__SERVER__HOST=0.0.0.0
ASTER__SERVER__PORT=3000
ASTER__DATABASE__URL="postgres://aster:secret@127.0.0.1:5432/asterdrive"
ASTER__WEBDAV__PREFIX="/webdav"
```

Runtime configuration is stored in the database and can be updated from the admin API / admin panel.

## Project structure

```text
src/                    Rust backend
migration/              Sea-ORM migrations
frontend-panel/         React admin/file panel
docs/                   Deployment and end-user documentation
developer-docs/         API and architecture docs for contributors
tests/                  Integration tests
```

## License

[MIT](LICENSE) - Copyright (c) 2026 AptS-1547
