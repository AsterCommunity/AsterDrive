# CLI 架构

AsterDrive 在同一个 `aster_drive` 可执行文件中提供服务启动与离线运维。CLI 是主 crate 内的产品入口，复用配置、数据库、存储和业务 service；不建立独立 CLI crate 或第二套业务实现。

## 进程与模块边界

`src/main.rs` 完成进程初始化后调用 `cli::run()`。CLI 入口构建静态 Clap 命令树、解析参数，再交给 `dispatch`；结果为 `CommandOutcome::Serve` 或 `CommandOutcome::Exit`。服务启动与进程退出归 main 所有。

- `app.rs`：根参数、全局实例路径、命令树与参数契约测试。
- `dispatch.rs`：命令分发、最终 stdout/stderr 和退出状态；不执行 SQL 或存储协议解析。
- `common/`：实例解析、数据库访问策略、输入、终端、输出和错误，各自有明确模块。
- `commands/`：每个子命令的参数、执行、报告和渲染。

CLI 只向进程公开 `run()` 和结果类型；参数、报告、执行器和辅助模块都不作为库公共入口。产品的 service、repository、storage 和 migration 不反向依赖 CLI。调用方直接引用所有者模块，不保留旧路径 re-export 或转发 facade。

## 命令所有权

| 命令 | 代码所有者 | 职责 |
| --- | --- | --- |
| `config` | `commands/config/` | `args` 定义参数，`values` 解析并复用产品归一化，`execute` 编排事务和刷新，`model` 表达结果，`render` 只展示报告；`interaction` 与 `transfer` 分别负责终端和备份格式 |
| `doctor` | `commands/doctor/` | `args`、`report`、`execute`、`render` 分离；`checks/config`、`checks/database`、`checks/integrity` 分组检查，`storage_scan` 适配驱动审计；数据核对和修复仍归 `services::ops::integrity` |
| `node enroll` | `commands/node/` | 参数、输入/确认与执行、报告及渲染分离；enrollment 协议和持久化复用 `services::remote::node_enrollment` |
| `database-migrate` | `commands/database_migration/` | `plan` 选择表、顺序和复制计划，`schema` 读取后端元数据；`apply`、`checkpoint`、`verify` 负责复制、恢复与校验；`model`、`render`、`progress` 分离结果与终端输出 |
| `completions` | `commands/completions.rs` | 从同一份 Clap 元数据生成补全、补齐 Fish 位置参数枚举，按 shell/XDG 规则安装 |
| `version` | `commands/build_info.rs` | 读取构建信息并生成报告，不加载实例配置 |
| verifier | `commands/hash_password.rs` | 读取并验证有界输入，返回 verifier，由 dispatch 输出 |

命令执行遵循 `Args -> execute -> Report -> render`。报告模型没有 I/O，渲染器不连接数据库、不写配置、不调用网络。交互提示/确认通过 `interaction`、`common/input`、`common/terminal` 写 stderr；迁移的阶段和进度由 `progress` 显式管理生命周期，最终报告由 dispatch 输出。

## 共享能力与副作用

- `common/context.rs` 解析已有实例的配置和数据库目标，不 bootstrap 实例。
- `common/database.rs` 使用 `DatabaseAccess::ReadOnly` / `ReadWrite` 明确连接意图；SQLite 只读连接保留原文件与 journal 模式，不复用会启用 WAL 的 writer 初始化。跨数据库的真正写入与事务仍遵守产品层边界。
- `common/input.rs` 拥有有界 stdin、私有文件及隐藏终端输入；secret 的 Debug 和错误不暴露输入值。
- `common/terminal.rs` 拥有颜色、终端检测和默认拒绝的写入确认。
- `common/output.rs` 统一 `OutputFormat`、JSON 成功/错误 envelope 和序列化失败回退。各命令保留自己的 human 展示语义。
- `common/error.rs` 区分取消和业务失败。取消为 130，其余状态保持既有命令契约。

帮助、version 和补全不加载实例配置或连接数据库。`serve`、无子命令启动、实例路径、version、帮助及补全均不依赖 `operations-cli`；该 feature 只控制运维命令及交互依赖。

## 验证入口

`tests/operations/cli.rs` 保留公共进程/数据库 helper；`cli_cases/` 按 help、fast paths、config、doctor、node、迁移、补全、输入和脱敏拆分。迁移产品 fixture 在 `cli_cases/fixtures.rs`，PTY 驱动仍在 `cli_interaction.py`。nextest 中三个跨库迁移用例继续使用单并发测试组，模块改名时必须同步过滤器。

```bash
cargo nextest run --profile ci --lib cli::
cargo nextest run --profile ci --test operations cli::
cargo test --no-default-features --features server --test server_commands
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

feature 构建共享 `target/debug/aster_drive`，不同 feature 的子进程测试必须串行，或者使用不同 target 目录。输出协议、参数/ENV/配置文件优先级、只读零写入、备份往返、事务回滚、刷新通知、取消和凭据脱敏都由行为测试证明；目录移动或编译通过不代替这些验证。
