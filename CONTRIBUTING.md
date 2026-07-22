# Contributing to AsterDrive

## Community Expectations

Please read and follow the [Code of Conduct](CODE_OF_CONDUCT.md) before participating in issues, pull requests, discussions, and review threads.

## Getting Started

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
cargo test --test test_auth
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Frontend checks
cd frontend-panel
bun run check
bun run build
```

Replace `test_auth` with the integration test that covers your change. Prefer a targeted
`cargo test --lib <filter>` or `cargo test --test <name> <filter>` while iterating; run a
broader suite when the change crosses service, database, storage, or protocol boundaries.
If an OpenAPI schema changes, also run:

```bash
cargo test --features openapi --test generate_openapi
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
