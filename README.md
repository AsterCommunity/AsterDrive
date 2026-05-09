<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="frontend-panel/public/static/asterdrive/asterdrive-light.svg" />
    <img src="frontend-panel/public/static/asterdrive/asterdrive-dark.svg" alt="AsterDrive" width="320" />
  </picture>
</p>

<p align="center">
  Self-hosted cloud storage built with Rust and React.
  <br />
  Single-binary delivery, Alpine container support, storage policies, WebDAV, sharing, version history, trash, thumbnails, and four upload modes.
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

## Highlights

- **Single binary delivery** - frontend assets are embedded into the Rust server with `rust-embed`
- **Multi-database** - SQLite by default, with MySQL and PostgreSQL support through SeaORM
- **Pluggable storage policies** - local filesystem and S3-compatible object storage, with user-level and folder-level overrides
- **Four upload modes** - `direct`, `chunked`, `presigned`, and `presigned_multipart`, negotiated by policy and file size
- **Sharing** - file and folder sharing with password, expiration time, download limits, public share page, nested shared-folder browsing, child-file download, and shared thumbnails
- **WebDAV** - dedicated WebDAV accounts, scoped root folder access, database-backed locks, custom properties, and minimal DeltaV version-tree support
- **Lifecycle management** - trash, version history, thumbnails, locks, periodic cleanup jobs, blob reconciliation, and runtime config management
- **Admin console** - overview dashboard plus users, storage policies, runtime settings, shares, locks, WebDAV accounts, and audit logs from the frontend panel

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

- hierarchical folders
- file upload, download, rename, move, copy, delete
- directory upload with `relative_path` auto-folder creation
- inline search and batch operations
- thumbnails and file previews
- version history, restore, and Monaco-based text editing with lock awareness

### Storage and delivery

- optional local-only blob deduplication with SHA-256 + reference counting
- local storage and S3-compatible storage policies
- user default policy + folder override
- S3 transport strategies: `relay_stream` and `presigned`
- streaming upload/download paths to avoid full-buffer transfers

### Collaboration and access

- HttpOnly cookie auth and Bearer JWT support
- public share pages at `/s/:token`
- password-protected and expiring shares
- shared folder browsing with child-file download and thumbnail access inside the shared tree
- profile, avatar upload / Gravatar, and user preference APIs
- WebDAV accounts with independent passwords, root-folder restriction, and DeltaV subset support

### Operations

- health endpoints: `/health`, `/health/ready`, optional `/health/memory` (`debug_assertions + openapi`), `/health/metrics` (`metrics` feature)
- runtime config stored in `system_config`
- admin overview, config schema, and policy connection testing endpoints
- audit logs for key actions
- Swagger UI in debug builds with the `openapi` feature, plus static OpenAPI export via `cargo test --features openapi --test generate_openapi`
- 5-second mail/background-task dispatch, hourly maintenance cleanup, and 6-hour blob reconciliation

## Documentation map

- [Getting started](docs/guide/getting-started.md)
- [Installation and deployment](docs/guide/installation.md)
- [Operations CLI](docs/deployment/ops-cli.md)
- [Performance benchmarking](docs/deployment/performance-benchmarking.md)
- [Code of Conduct](CODE_OF_CONDUCT.md)
- [Developer docs](developer-docs/README.md)
- [Architecture](developer-docs/architecture.md)
- [Docker deployment](docs/deployment/docker.md)
- [API overview](developer-docs/api/index.md)
- [User guide](docs/guide/user-guide.md)

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
