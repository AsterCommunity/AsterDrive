# Logging

::: tip This page covers `[logging]`
First decide where logs should go: stdout, journald, or a file. The other options are built around that choice.
When troubleshooting, look at the API response `code` first. If runtime logs include a structured `error_code` field, keep it as well. See [error handling](/en/reference/errors) for the error code reference.
:::

```toml
[logging]
level = "info"
format = "text"
file = ""
enable_rotation = true
max_backups = 5
```

## Decide Where Logs Should Go First

| Deployment Method | Recommended Approach |
| --- | --- |
| Docker | Do not write files. Output directly to stdout and let the container logging system collect it. |
| systemd | Do not write files. Let journald handle logs. |
| Bare-metal single process | Write to a dedicated file and enable rotation. |

## Options

| Option | Default | Purpose |
| --- | --- | --- |
| `level` | `"info"` | `trace` / `debug` / `info` / `warn` / `error` |
| `format` | `"text"` | `text` or `json` |
| `file` | `""` | Log file path. Empty means output to stdout. |
| `enable_rotation` | `true` | Whether to rotate daily. Applies only when `file` is not empty. |
| `max_backups` | `5` | Number of historical log files to keep |

## How to Choose a Format

- **Local troubleshooting** - `text`, easier to read by eye
- **Centralized logging systems** such as Loki, ELK, or a custom collector - `json`, so fields are structured directly

## Which Takes Priority: `RUST_LOG` or the Config File?

During logging initialization, AsterDrive **reads `RUST_LOG` first** and falls back to `logging.level` only when `RUST_LOG` is not set.

For temporary log-level changes, `RUST_LOG` is the easiest option:

```bash
RUST_LOG=debug
```

You can also override it with an `ASTER__` environment variable:

```bash
ASTER__LOGGING__LEVEL=debug
```

## Production Example

```toml
[logging]
level = "info"
format = "json"
file = "/var/log/asterdrive.log"
enable_rotation = true
max_backups = 7
```

::: tip Runtime logs are not audit logs

- **Runtime logs** (this page) - Used for troubleshooting. They record requests, errors, and internal events.
- **Audit logs** - Used for accountability. They record "who did what and when". Enable them in `Admin -> System Settings -> Audit Logs`. See [runtime system settings](/en/config/runtime#audit-logs) for details.

:::
