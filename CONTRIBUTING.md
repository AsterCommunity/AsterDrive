# Contributing to AsterDrive

## Community Expectations

Please read and follow the [Code of Conduct](CODE_OF_CONDUCT.md) before participating in issues, pull requests, discussions, and review threads.

## Getting Started

1. Fork the repository
2. Clone your fork:
   ```bash
   git clone https://github.com/AsterCommunity/AsterDrive.git
   cd AsterDrive
   ```
3. Build and run:
   ```bash
   # Frontend
   cd frontend-panel && bun install && bun run build && cd ..

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
cargo check
cargo test --test api_integration
cargo clippy -- -D warnings

# Frontend checks
cd frontend-panel
bun run check
bun run build
```

## Project Conventions

### Error System (Two Layers)

- **Internal**: `AsterError` with string codes (E001-E040) for logging/debugging
- **API**: `ErrorCode` with numeric codes grouped by domain (0=success, 1000=general, 2000=auth, 3000=file, 4000=policy, 5000=folder)

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
{ "code": 0, "msg": "", "data": { ... } }
{ "code": 2000, "msg": "Invalid Credentials" }
```

### Frontend Conventions

- Type checking: TypeScript 7 native `tsc` with incremental project caches
- Linting: `biome`, not ESLint
- No TS enums (`erasableSyntaxOnly`), use `as const` objects
- Type imports must use `import type` (`verbatimModuleSyntax`)
- shadcn/ui components use `render` prop (not `asChild`)

## Architecture

See [docs/architecture.md](docs/architecture.md) for detailed architecture documentation.

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
