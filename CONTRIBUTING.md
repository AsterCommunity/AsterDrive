# Contributing to AsterDrive

## Community Expectations

Please read and follow the [Code of Conduct](CODE_OF_CONDUCT.md) before participating in issues, pull requests, discussions, and review threads.

## Getting Started

Backend tests use [cargo-nextest](https://nexte.st/). Install it before running the test commands below.

1. Fork the repository
2. Clone your fork:
   ```bash
   git clone https://github.com/<your-github-username>/AsterDrive.git
   cd AsterDrive
   git remote add upstream https://github.com/AsterCommunity/AsterDrive.git
   ```
3. Build and run:
   ```bash
   # Frontend
   cd frontend-panel && bun install --frozen-lockfile && bun run build && cd ..

   # Backend
   cargo run
   ```

## Development Workflow

### AI-Assisted Contributions

AsterCommunity welcomes responsible use of AI-assisted development tools. You may use them to read code, learn the architecture, explore an unfamiliar subsystem, draft an implementation, write tests, or review a change. We consider using these tools to understand the project better a productive part of contributing.

The contributor remains responsible for every submitted change, regardless of which tools helped produce it. Before opening a pull request, you must:

- understand the code you submit and be able to explain its behavior and architectural fit;
- review generated suggestions against the current code, project contracts, and authoritative external specifications instead of accepting them blindly;
- add and run tests that cover the changed behavior, including relevant failure, boundary, rollback, concurrency, permission, protocol, and compatibility cases;
- verify that the change does not expose credentials, personal data, proprietary material, or content with incompatible licensing;
- accurately report what was tested, what was not tested, and any remaining limitations;
- address review feedback and maintain the contribution just as you would for code written without AI assistance.

AI assistance does not lower the bar for correctness, security, maintainability, or test coverage. A generated patch without demonstrated understanding and appropriate verification is not ready to merge. The person submitting the pull request—not the tool—is accountable for the result.

### Branch Naming

- `feat/<description>` - New features
- `fix/<description>` - Bug fixes
- `refactor/<description>` - Refactoring
- `docs/<description>` - Documentation

### Commit Messages

Use conventional commits:

```
feat(storage): add S3 driver support
fix(auth): handle expired refresh token correctly
refactor(api): simplify error response format
docs: update API endpoint documentation
```

### Before Submitting a PR

```bash
# Backend checks
cargo fmt --all -- --check
cargo check
cargo nextest run --profile ci --test auth auth::
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Frontend checks
cd frontend-panel
bun run check
bun run build
```

Replace `auth auth::` with the integration-test target and module that cover your change. Prefer a targeted
`cargo nextest run --lib <filter>` or `cargo nextest run --test <name> <filter>` while iterating; run a
broader suite when the change crosses service, database, storage, or protocol boundaries.
If an OpenAPI schema changes, also run:

```bash
cargo nextest run --features openapi --test generate_openapi
cd frontend-panel
bun run generate-api
```

## Project Conventions

### Error System (Two Layers)

- **Internal**: `AsterError` variants expose `E001`-style internal codes for logs and debugging
- **API**: `ApiErrorCode` exposes stable string wire codes such as `success`, `auth.credentials_failed`, and `storage.driver_error`

### Type Safety

- All DB enum fields use `DeriveActiveEnum` (UserRole, UserStatus, DriverType)
- No magic strings for enum values
- `TokenType` is a plain Rust enum (not stored in DB)

### Route Registration

- Each module exports `pub fn routes()` returning `Scope` or `impl HttpServiceFactory`
- Use `impl HttpServiceFactory` when `.wrap()` is needed
- Frontend routes registered last (SPA fallback)

### API Response Format

```json
{ "code": "success", "msg": "", "data": { ... } }
{ "code": "auth.credentials_failed", "msg": "Invalid Credentials" }
```

### Frontend Conventions

- Type checking: TypeScript 7 native `tsc` with incremental project caches
- Linting: `biome`, not ESLint
- No TS enums (`erasableSyntaxOnly`), use `as const` objects
- Type imports must use `import type` (`verbatimModuleSyntax`)
- shadcn/ui components use `render` prop (not `asChild`)

## Architecture

See the [developer documentation](developer-docs/README.md) and
[architecture overview](developer-docs/en/architecture/index.md) for the current module and runtime boundaries.

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
