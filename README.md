<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="frontend-panel/public/static/asterdrive/asterdrive-light.svg" />
    <img src="frontend-panel/public/static/asterdrive/asterdrive-dark.svg" alt="AsterDrive" width="320" />
  </picture>
</p>

<p align="center">
  Self-hosted file infrastructure in Rust for small teams that need storage control, reliable large-file uploads, WebDAV/WOPI, and operations visibility without adopting a full private-cloud suite.
  <br />
  Route files across local, S3-compatible, and remote-node backends with one MIT-licensed Rust + React service built for deployment, auditability, and modification.
</p>

<p align="center">
  <a href="https://drive.astercosm.com/"><img alt="Documentation Site" src="https://img.shields.io/badge/docs-VitePress-7C3AED?style=for-the-badge&logo=vitepress&logoColor=white"></a>
  <a href="README.zh.md"><img alt="中文 README" src="https://img.shields.io/badge/README-中文-E11D48?style=for-the-badge"></a>
  <a href="docs/guide/getting-started.md"><img alt="Quick Start" src="https://img.shields.io/badge/quick%20start-guide-2563EB?style=for-the-badge"></a>
  <a href="docs/deployment/ops-cli.md"><img alt="Ops CLI" src="https://img.shields.io/badge/ops-CLI-0EA5E9?style=for-the-badge"></a>
  <a href="developer-docs/en/architecture.md"><img alt="Architecture" src="https://img.shields.io/badge/architecture-overview-0F172A?style=for-the-badge"></a>
  <a href="developer-docs/en/api/index.md"><img alt="API Docs" src="https://img.shields.io/badge/API-reference-059669?style=for-the-badge"></a>
  <a href="docs/deployment/docker.md"><img alt="Docker" src="https://img.shields.io/badge/docker-deployment-2496ED?style=for-the-badge&logo=docker&logoColor=white"></a>
</p>

<p align="center">
  <img src="assets/Readme/Screenshot-English.webp" alt="AsterDrive English screenshot" width="1280" />
</p>

## What is AsterDrive?

AsterDrive is an MIT-licensed self-hosted file service for people who want control over where files live and how they move through the system. It is built around the core drive workflow: upload reliably, organize folders, recover mistakes, share access, connect WebDAV clients, open Office files through WOPI-compatible services, and route objects to the right storage backend.

It is not trying to become a full private-cloud suite. AsterDrive focuses on file infrastructure: storage policies, large-file upload paths, team and personal workspaces, sharing, version history, WebDAV, WOPI, auditability, and deployment/operations tooling.

The current `v0.3.x` line is an active development line focused on organization and extensibility. In the `0.x` series, minor versions carry major compatibility or product-scope changes, while patch versions carry smaller feature and maintenance updates.

## Where it fits

AsterDrive is a good fit when you want:

- a single self-hosted service with embedded frontend assets
- SQLite out of the box, with optional PostgreSQL / MySQL later
- local filesystem, S3-compatible object storage, or remote AsterDrive follower-node storage
- upload strategies for both small files and large objects: direct, resumable chunked, object-storage presigned, and object-storage multipart
- personal and team workspaces with quotas, shares, trash, tasks, audit logs, and storage policy groups
- WebDAV access with independent accounts and scoped root folders
- Office preview/editing through external WOPI services such as OnlyOffice or Collabora
- a codebase that is meant to be read, modified, and deployed without a plugin marketplace or enterprise stack

AsterDrive is probably not the right first choice when you need:

- a complete collaboration suite with calendars, contacts, chat, mail, and an app ecosystem
- mature native desktop and mobile sync clients today
- an ultra-minimal web UI over a single server directory
- multi-primary clustering, automatic failover, or enterprise compliance guarantees
- a vendor-managed SaaS where someone else owns the deployment and data responsibility

## Design focus

- **File safety first** - trash, version history, locks, quota checks, and cleanup tasks are part of the core workflow, not decorative extras.
- **Storage control** - policies can route uploads to local storage, S3-compatible storage, or remote follower nodes by user, team, and file size.
- **Large-file paths** - the backend negotiates direct uploads, chunked uploads, object-storage presigned uploads, and object-storage multipart uploads based on policy and object size.
- **Interoperability without sprawl** - WebDAV and WOPI cover practical client and Office workflows without turning the project into an all-in-one cloud suite.
- **Operations built in** - health checks, runtime configuration, audit logs, background tasks, storage tests, `doctor`, and migration commands are first-class features.
- **Hackable core** - Rust backend, React frontend, SeaORM migrations, explicit error codes, API docs, and clear service/repository boundaries.

## Quick start

### Run with Docker

For a local HTTP trial, prepare a writable data directory and start the official image:

```bash
mkdir -p ./data
sudo chown -R 10001:10001 ./data

docker run -d \
  --name asterdrive \
  -p 3000:3000 \
  -e ASTER__SERVER__HOST=0.0.0.0 \
  -e ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES=true \
  -e "ASTER__DATABASE__URL=sqlite:///data/asterdrive.db?mode=rwc" \
  -v "$(pwd)/data:/data" \
  ghcr.io/apts-1547/asterdrive:latest
```

Open:

```text
http://127.0.0.1:3000
```

The first registered user becomes `admin`.

`ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES=true` is only for local or internal HTTP testing. For production, put AsterDrive behind HTTPS and keep secure cookies enabled.

You can also use the included Compose file:

```bash
mkdir -p ./data
sudo chown -R 10001:10001 ./data
docker compose up -d
```

See [`docs/deployment/docker.md`](docs/deployment/docker.md) for the full Docker guide.

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
- create the default local storage policy and default policy group
- initialize built-in runtime configuration items in `system_config`

## Production notes

- Do not expose `:3000` directly to the public Internet. Put AsterDrive behind a reverse proxy that handles HTTPS, upload limits, WebDAV/WOPI passthrough, and security headers.
- Configure public site URLs before relying on share links, WebDAV URLs, mail links, or WOPI callbacks.
- Run `./aster_drive doctor` after deployment and upgrades. The default SQLite search acceleration expects `FTS5 + trigram tokenizer` support.
- Plan backups for the database, uploaded blobs, config, and any external object-storage credentials. Start with [`docs/deployment/backup.md`](docs/deployment/backup.md).
- If you enable WOPI, test real `docx`, `xlsx`, and `pptx` files through the final public URL and confirm that edits save back into AsterDrive.

## Core capabilities

### File management

- folders, breadcrumbs, list/grid views, search, multi-select, and batch operations
- file upload, folder upload, download, rename, move, copy, delete, restore, and purge
- archive download, online archive compression/extraction, and background task progress
- thumbnails, browser-native previews, read-only ZIP/7z archive manifest previews, and configurable external preview apps
- Monaco-based text editing, lock awareness, version history, restore, and version deletion

### Workspaces and sharing

- personal workspace plus team workspaces
- independent files, shares, trash, tasks, quotas, audit records, and policy groups per workspace
- public file and folder shares at `/s/:token`
- optional share password, expiration time, download limits, open/download counters, and direct links
- shared-folder browsing with child-file download, preview, and thumbnail access

### Access and editing

- HttpOnly cookie auth plus Bearer JWT for API clients
- first-user setup, registration controls, activation, password reset, and email-change confirmation
- WebDAV accounts with independent passwords, scoped root folders, database-backed locks, custom properties, and a small DeltaV subset
- WOPI launch sessions and file endpoints for Office preview/editing through external WOPI hosts
- optional Passkey / WebAuthn registration and login endpoints

### Storage and delivery

- local storage, S3-compatible storage, and remote follower-node storage policies
- policy groups that route uploads by user, team, and file size
- optional local-only blob deduplication using SHA-256 and reference counting
- object-storage upload/download strategies: `relay_stream`, `presigned`, and multipart uploads
- remote-node upload/download strategies: `relay_stream` and `presigned`
- streaming upload/download paths where the selected strategy allows it

### Administration and operations

- admin overview, users, teams, storage policies, policy groups, remote nodes, shares, tasks, locks, runtime settings, and audit logs
- schema-driven runtime configuration stored in `system_config`
- health endpoints: `/health`, `/health/ready`, optional `/health/memory`, and optional `/health/metrics`
- storage policy and remote-node connection tests
- background task records for archive jobs, thumbnail generation, mail dispatch, cleanup, and runtime tasks
- periodic cleanup for uploads, trash, locks, audit logs, teams, WOPI sessions, and orphaned blobs
- Swagger UI in debug builds with the `openapi` feature, plus static OpenAPI export

## Roadmap

### v0.3.x: organization and extensibility

The `v0.3.x` line focuses on richer workspace organization and the foundation for controlled integrations.

- tags for files and folders
- tag-based filtering in file lists and search
- WASM/Extism plugin design and spike
- capability-based plugin permissions
- event subscriptions and webhook-style automation
- file actions and plugin-provided admin settings

## Documentation

- [Getting started](docs/guide/getting-started.md)
- [User guide](docs/guide/user-guide.md)
- [Teams and permissions](docs/guide/teams-and-permissions.md)
- [Sharing and public access](docs/guide/sharing.md)
- [Preview and WOPI](docs/guide/preview-and-wopi.md)
- [Storage backends](docs/storage/index.md)
- [Remote follower storage](docs/storage/remote-follower.md)
- [Docker deployment](docs/deployment/docker.md)
- [Production checklist](docs/deployment/production-checklist.md)
- [Backup and restore](docs/deployment/backup.md)
- [Operations CLI](docs/deployment/ops-cli.md)
- [Developer docs](developer-docs/README.md)
- [Architecture](developer-docs/en/architecture.md)
- [API overview](developer-docs/en/api/index.md)

## Development

### Requirements

- Rust `1.94.0+`
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

## Project structure

```text
src/                    Rust backend
migration/              SeaORM migrations
frontend-panel/         React admin/file panel
docs/                   Deployment and end-user documentation
developer-docs/         API, architecture, testing, and internal positioning docs
tests/                  Integration tests
```

## License

[MIT](LICENSE) - Copyright (c) 2026 AptS-1547
