# CLI architecture

AsterDrive provides server startup and offline operations through one `aster_drive` executable. The CLI belongs to the main product crate and reuses product configuration, persistence, storage, and services. It has no separate workspace crate or parallel business implementation.

## Process and module boundaries

After process initialization, `src/main.rs` calls `cli::run()`. The CLI entry builds the static Clap tree, parses arguments, and dispatches the command. `CommandOutcome::Serve` and `CommandOutcome::Exit` leave server startup and process exit with main.

- `app.rs` owns root arguments, global instance selection, the command tree, and parsing tests.
- `dispatch.rs` owns routing, final stdout/stderr, and exit status; it contains no SQL or storage protocol parsing.
- `common/` separates instance context, database access, input, terminal behavior, output, and errors.
- `commands/` gives each command its own arguments, execution, reports, and rendering.

Only `run()` and its result type are public process entry points. Command arguments, reports, executors, and helpers stay internal. Product services, repositories, storage, and migrations never depend on the CLI. Callers import the owning module directly; old paths have no compatibility re-exports or forwarding facades.

## Command ownership

| Command | Owner | Responsibilities |
| --- | --- | --- |
| `config` | `commands/config/` | `args` declares input, `values` resolves and normalizes through product rules, `execute` owns transactions and reload delivery, `model` represents results, and `render` presents them; `interaction` and `transfer` own prompts and backups |
| `doctor` | `commands/doctor/` | Arguments, reports, execution, and rendering are separate; `checks/config`, `checks/database`, and `checks/integrity` group findings, while `storage_scan` adapts driver audits; data auditing and repairs remain in `services::ops::integrity` |
| `node enroll` | `commands/node/` | Separates arguments, input/confirmation and execution, reports, and rendering; reuses `services::remote::node_enrollment` for protocol and persistence |
| `database-migrate` | `commands/database_migration/` | `plan` selects tables, order, and copy plans; `schema` reads backend metadata; `apply`, `checkpoint`, and `verify` handle copying, recovery, and validation; `model`, `render`, and `progress` separate results from terminal output |
| `completions` | `commands/completions.rs` | Uses the same Clap metadata, supplements Fish positional enum candidates, and installs according to shell/XDG rules |
| `version` | `commands/build_info.rs` | Renders build information without loading instance configuration |
| verifier | `commands/hash_password.rs` | Reads and validates bounded input, returning the verifier for dispatch to print |

Commands follow `Args -> execute -> Report -> render`. Report models have no I/O, and rendering does not connect to databases, change configuration, or call networks. Interaction prompts use stderr through `interaction`, `common/input`, and `common/terminal`. Migration `progress` explicitly manages stage output and terminal lifecycle; dispatch prints the final report.

## Shared capabilities and side effects

- `common/context.rs` resolves an existing instance and database target without bootstrapping it.
- `common/database.rs` expresses connection intent through `DatabaseAccess::ReadOnly` / `ReadWrite`. Read-only SQLite access preserves files and journal mode rather than using writer initialization that enables WAL. Product transaction and write boundaries remain authoritative.
- `common/input.rs` owns bounded stdin, private files, and hidden input. Secret Debug and error paths never expose input values.
- `common/terminal.rs` owns colors, terminal detection, and write confirmation that defaults to declining.
- `common/output.rs` shares `OutputFormat`, success/error JSON envelopes, and serialization failure handling. Each command retains its human presentation contract.
- `common/error.rs` distinguishes cancellation from business errors. Cancellation exits with 130; other statuses retain existing command semantics.

Help, version, and completion do not load instance configuration or connect to a database. Server startup, instance selection, version, help, and completion are available without `operations-cli`; that feature gates operational commands and interactive dependencies only.

## Validation

`tests/operations/cli.rs` holds shared process/database helpers. `cli_cases/` separates help, fast paths, config, doctor, node, migration, completion, input, and redaction tests. Product migration fixtures live in `cli_cases/fixtures.rs`, and the PTY driver remains `cli_interaction.py`. The three cross-database migration tests retain a single-concurrency nextest group; module renames must update its filter.

```bash
cargo nextest run --profile ci --lib cli::
cargo nextest run --profile ci --test operations cli::
cargo test --no-default-features --features server --test server_commands
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Feature builds share `target/debug/aster_drive`. Serialize subprocess tests with different features or isolate target directories. Behavioral tests prove output, argument/environment/configuration precedence, read-only side effects, backup round trips, rollback, reload delivery, cancellation, and redaction. File moves and compilation alone do not establish those contracts.
