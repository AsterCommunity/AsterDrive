# 日志

::: tip 这一篇覆盖 `[logging]`
先决定日志写到哪里（stdout / journald / 文件），其他选项都是围绕这个来配的。
排障时优先看 API 响应里的 `code`；运行日志里如果有结构化 `error_code` 字段，也一起保留。错误码对照见 [错误码处理](/guide/errors)。
:::

```toml
[logging]
level = "info"
format = "text"
file = ""
enable_rotation = true
max_backups = 5
```

## 先决定日志写到哪里

| 部署方式 | 推荐做法 |
| --- | --- |
| Docker | 不写文件，直接输出到 stdout，让容器日志系统接 |
| systemd | 不写文件，交给 journald |
| 裸机单进程 | 写入单独文件 + 开启轮转 |

## 选项一览

| 选项 | 默认值 | 作用 |
| --- | --- | --- |
| `level` | `"info"` | `trace` / `debug` / `info` / `warn` / `error` |
| `format` | `"text"` | `text` 或 `json` |
| `file` | `""` | 日志文件路径；留空 = 输出到 stdout |
| `enable_rotation` | `true` | 是否按天轮转，仅 `file` 非空时生效 |
| `max_backups` | `5` | 保留的历史日志文件数 |

## 格式怎么选

- **本机排障** —— `text`，肉眼好读
- **对接集中式日志系统**（Loki / ELK / 自建采集） —— `json`，字段直接结构化

## `RUST_LOG` 和配置文件谁优先

日志初始化时**优先读 `RUST_LOG`**，没有再回退到 `logging.level`。

临时调日志级别用 `RUST_LOG` 最方便：

```bash
RUST_LOG=debug
```

也能用 `ASTER__` 环境变量覆盖：

```bash
ASTER__LOGGING__LEVEL=debug
```

## 生产环境示例

```toml
[logging]
level = "info"
format = "json"
file = "/var/log/asterdrive.log"
enable_rotation = true
max_backups = 7
```

::: tip 运行日志 ≠ 审计日志

- **运行日志**（这一页讲的）—— 用于排障，记录请求、错误、内部事件
- **审计日志** —— 用于追责，记录"谁在什么时候做了什么"，在 `管理 -> 系统设置 -> 审计日志` 里开关，详见 [系统设置](/config/runtime#审计日志)

:::
